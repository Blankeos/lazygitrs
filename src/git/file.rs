use std::collections::HashMap;

use anyhow::Result;

use super::GitCommands;
use crate::model::{File, FileStatus};

impl GitCommands {
    /// Full load: porcelain status + per-file numstat/hunk counts.
    /// Prefer [`load_files_status_only`] on the Space-toggle hot path.
    pub fn load_files(&self) -> Result<Vec<File>> {
        let mut files = self.load_files_status_only()?;
        self.populate_file_diff_stats(&mut files);
        Ok(files)
    }

    /// Fast status-only load (no numstat / hunk-count subprocesses).
    /// Rapid stage/unstage stays responsive; stats catch up on full refresh.
    pub fn load_files_status_only(&self) -> Result<Vec<File>> {
        let result = self
            .git()
            .args(&["status", "--porcelain", "-uall"])
            .run_expecting_success()?;

        let mut files = Vec::new();
        for line in result.stdout.lines() {
            if line.len() < 4 {
                continue;
            }

            let x = line.chars().nth(0).unwrap_or(' ');
            let y = line.chars().nth(1).unwrap_or(' ');
            let raw = &line[3..];

            let (has_staged, has_unstaged, tracked, status) = parse_status_codes(x, y);

            let name = if raw.contains(" -> ") {
                let parts: Vec<&str> = raw.splitn(2, " -> ").collect();
                format!(
                    "{} -> {}",
                    unquote_porcelain_path(parts[0]),
                    unquote_porcelain_path(parts.get(1).copied().unwrap_or(""))
                )
            } else {
                unquote_porcelain_path(raw)
            };

            let display_name = name.clone();

            files.push(File {
                short_status: format!("{}{}", x, y),
                name,
                display_name,
                status,
                has_staged_changes: has_staged,
                has_unstaged_changes: has_unstaged,
                tracked,
                added: x == 'A' || y == 'A' || !tracked,
                deleted: x == 'D' || y == 'D',
                has_merge_conflicts: x == 'U'
                    || y == 'U'
                    || (x == 'A' && y == 'A')
                    || (x == 'D' && y == 'D'),
                hunk_count: 0,
                additions: 0,
                deletions: 0,
            });
        }

        Ok(files)
    }

    /// Populate final working-tree stats relative to HEAD. Failures are
    /// intentionally non-fatal: status remains useful even when a diff cannot
    /// be produced (for example, during unusual index states).
    fn populate_file_diff_stats(&self, files: &mut [File]) {
        let diff_base = if self
            .git()
            .args(&["rev-parse", "--verify", "HEAD"])
            .run()
            .is_ok_and(|result| result.success)
        {
            vec!["diff", "HEAD"]
        } else {
            vec!["diff", "--cached"]
        };

        let mut numstat_args = diff_base.clone();
        numstat_args.extend(["--numstat", "-z", "--find-renames", "--no-color"]);
        let line_stats = self
            .git()
            .args(&numstat_args)
            .run()
            .ok()
            .filter(|result| result.success)
            .map(|result| parse_numstat_z(&result.stdout))
            .unwrap_or_default();

        let mut patch_args = diff_base;
        patch_args.extend(["--unified=0", "--find-renames", "--no-color", "--no-prefix"]);
        let hunk_counts = self
            .git()
            .args(&patch_args)
            .run()
            .ok()
            .filter(|result| result.success)
            .map(|result| parse_hunk_counts(&result.stdout))
            .unwrap_or_default();

        for file in files {
            let path = file.current_path().to_string();
            if file.tracked {
                if let Some(&(additions, deletions)) = line_stats.get(&path) {
                    file.additions = additions;
                    file.deletions = deletions;
                }
                file.hunk_count = hunk_counts.get(&path).copied().unwrap_or(0);
            } else if let Ok(content) = std::fs::read_to_string(self.repo_path().join(&path)) {
                file.additions = content.lines().count();
                file.hunk_count = usize::from(file.additions > 0);
            }
        }
    }

    pub fn stage_file(&self, path: &str) -> Result<()> {
        // --literal-pathspecs (global flag, before subcommand) prevents git
        // from interpreting [] chars in paths as glob/magic pathspecs.
        self.git()
            .args(&["--literal-pathspecs", "add", "--", path])
            .run_expecting_success()?;
        Ok(())
    }

    /// Stage multiple files in a single git command.
    pub fn stage_files(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["--literal-pathspecs", "add", "--"];
        let refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        args.extend(refs);
        self.git().args(&args).run_expecting_success()?;
        Ok(())
    }

    pub fn unstage_file(&self, path: &str) -> Result<()> {
        self.git()
            .args(&["--literal-pathspecs", "reset", "HEAD", "--", path])
            .run_expecting_success()?;
        Ok(())
    }

    /// Unstage multiple files in a single git command.
    pub fn unstage_files(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["--literal-pathspecs", "reset", "HEAD", "--"];
        let refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        args.extend(refs);
        self.git().args(&args).run_expecting_success()?;
        Ok(())
    }

    pub fn stage_all(&self) -> Result<()> {
        self.git().args(&["add", "-A"]).run_expecting_success()?;
        Ok(())
    }

    pub fn unstage_all(&self) -> Result<()> {
        // When there are no commits yet, HEAD doesn't exist so `git reset HEAD`
        // fails. Use `git rm --cached -r .` to unstage everything instead.
        let head_exists = self
            .git()
            .args(&["rev-parse", "--verify", "HEAD"])
            .run()?
            .success;
        if head_exists {
            self.git()
                .args(&["reset", "HEAD"])
                .run_expecting_success()?;
        } else {
            self.git()
                .args(&["rm", "--cached", "-r", "."])
                .run_expecting_success()?;
        }
        Ok(())
    }

    pub fn discard_file(&self, path: &str, added: bool) -> Result<()> {
        // Unstage first if needed (ignore errors — file may not be staged)
        let _ = self.git().args(&["reset", "HEAD", "--", path]).run();

        if added {
            // New/untracked file: just delete it
            let full_path = self.repo_path().join(path);
            if full_path.is_dir() {
                std::fs::remove_dir_all(&full_path)?;
            } else {
                std::fs::remove_file(&full_path)?;
            }
        } else {
            // Tracked file: discard working tree changes
            self.git()
                .args(&["checkout", "--", path])
                .run_expecting_success()?;
        }
        Ok(())
    }

    pub fn ignore_file(&self, path: &str) -> Result<()> {
        let gitignore = self.repo_path().join(".gitignore");
        let mut contents = std::fs::read_to_string(&gitignore).unwrap_or_default();
        if !contents.ends_with('\n') && !contents.is_empty() {
            contents.push('\n');
        }
        contents.push_str(path);
        contents.push('\n');
        std::fs::write(gitignore, contents)?;
        Ok(())
    }

    pub fn exclude_file(&self, path: &str) -> Result<()> {
        let exclude = self.repo_path().join(".git/info/exclude");
        if let Some(parent) = exclude.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut contents = std::fs::read_to_string(&exclude).unwrap_or_default();
        if !contents.ends_with('\n') && !contents.is_empty() {
            contents.push('\n');
        }
        contents.push_str(path);
        contents.push('\n');
        std::fs::write(exclude, contents)?;
        Ok(())
    }
}

pub(super) fn parse_numstat_z(output: &str) -> HashMap<String, (usize, usize)> {
    let mut stats = HashMap::new();
    let fields: Vec<&str> = output.split('\0').collect();
    let mut index = 0;

    while index < fields.len() {
        let Some((additions, rest)) = fields[index].split_once('\t') else {
            index += 1;
            continue;
        };
        let Some((deletions, path)) = rest.split_once('\t') else {
            index += 1;
            continue;
        };
        let (Ok(additions), Ok(deletions)) = (additions.parse(), deletions.parse()) else {
            // Binary files use "-" for both counts.
            index += 1;
            continue;
        };

        if path.is_empty() {
            // With -z, renames are encoded as an empty path followed by old
            // and new path fields. Attribute the stats to the current path.
            if let Some(new_path) = fields.get(index + 2).filter(|path| !path.is_empty()) {
                stats.insert((*new_path).to_string(), (additions, deletions));
            }
            index += 3;
        } else {
            stats.insert(path.to_string(), (additions, deletions));
            index += 1;
        }
    }

    stats
}

pub(super) fn parse_hunk_counts(output: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    let mut current_path: Option<String> = None;
    let mut old_path: Option<String> = None;

    for line in output.lines() {
        if let Some(path) = line.strip_prefix("--- ") {
            old_path = (path != "/dev/null").then(|| unquote_porcelain_path(path));
        } else if let Some(path) = line.strip_prefix("+++ ") {
            current_path = if path == "/dev/null" {
                old_path.take()
            } else {
                Some(unquote_porcelain_path(path))
            };
        } else if line.starts_with("@@") {
            if let Some(path) = &current_path {
                *counts.entry(path.clone()).or_insert(0) += 1;
            }
        }
    }

    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numstat_for_regular_renamed_and_binary_files() {
        let output = "3\t2\tsrc/lib.rs\0".to_string()
            + "1\t0\t\0src/old name.rs\0src/new name.rs\0"
            + "-\t-\timage.png\0";

        let stats = parse_numstat_z(&output);

        assert_eq!(stats.get("src/lib.rs"), Some(&(3, 2)));
        assert_eq!(stats.get("src/new name.rs"), Some(&(1, 0)));
        assert!(!stats.contains_key("image.png"));
    }

    #[test]
    fn counts_hunks_for_modified_renamed_and_deleted_files() {
        let output = concat!(
            "--- src/lib.rs\n+++ src/lib.rs\n@@ -1 +1 @@\n@@ -8 +8 @@\n",
            "--- old.rs\n+++ new.rs\n@@ -2 +2 @@\n",
            "--- removed.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n",
        );

        let counts = parse_hunk_counts(output);

        assert_eq!(counts.get("src/lib.rs"), Some(&2));
        assert_eq!(counts.get("new.rs"), Some(&1));
        assert_eq!(counts.get("removed.rs"), Some(&1));
    }
}

/// Decode a path as emitted by `git status --porcelain`.
///
/// Git wraps paths containing special characters in double quotes with
/// C-style escapes (e.g. `"\303\241.txt"`, `"with\"quote.txt"`). Passing the
/// literal quoted form to later git commands makes git treat the quotes as
/// part of the pathspec and fail. This reverses that encoding.
fn unquote_porcelain_path(raw: &str) -> String {
    let bytes = raw.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'"' || bytes[bytes.len() - 1] != b'"' {
        return raw.to_string();
    }
    let inner = &bytes[1..bytes.len() - 1];
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        if c == b'\\' && i + 1 < inner.len() {
            let n = inner[i + 1];
            match n {
                b'a' => {
                    out.push(0x07);
                    i += 2;
                }
                b'b' => {
                    out.push(0x08);
                    i += 2;
                }
                b't' => {
                    out.push(b'\t');
                    i += 2;
                }
                b'n' => {
                    out.push(b'\n');
                    i += 2;
                }
                b'v' => {
                    out.push(0x0b);
                    i += 2;
                }
                b'f' => {
                    out.push(0x0c);
                    i += 2;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 2;
                }
                b'"' => {
                    out.push(b'"');
                    i += 2;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                }
                b'0'..=b'7'
                    if i + 3 < inner.len()
                        && (b'0'..=b'7').contains(&inner[i + 2])
                        && (b'0'..=b'7').contains(&inner[i + 3]) =>
                {
                    let val = ((inner[i + 1] - b'0') << 6)
                        | ((inner[i + 2] - b'0') << 3)
                        | (inner[i + 3] - b'0');
                    out.push(val);
                    i += 4;
                }
                _ => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

fn parse_status_codes(x: char, y: char) -> (bool, bool, bool, FileStatus) {
    match (x, y) {
        ('?', '?') => (false, true, false, FileStatus::Untracked),
        ('A', ' ') => (true, false, true, FileStatus::Added),
        ('A', 'M') => (true, true, true, FileStatus::Added),
        ('M', ' ') => (true, false, true, FileStatus::Modified),
        (' ', 'M') => (false, true, true, FileStatus::Modified),
        ('M', 'M') => (true, true, true, FileStatus::Modified),
        ('D', ' ') => (true, false, true, FileStatus::Deleted),
        (' ', 'D') => (false, true, true, FileStatus::Deleted),
        ('R', ' ') => (true, false, true, FileStatus::Renamed),
        ('R', 'M') => (true, true, true, FileStatus::Renamed),
        ('C', ' ') => (true, false, true, FileStatus::Copied),
        ('C', 'M') => (true, true, true, FileStatus::Copied),
        ('U', 'U')
        | ('A', 'A')
        | ('D', 'D')
        | ('U', 'A')
        | ('A', 'U')
        | ('U', 'D')
        | ('D', 'U') => (false, true, true, FileStatus::Unmerged),
        _ => (x != ' ', y != ' ', true, FileStatus::Modified),
    }
}
