use anyhow::Result;

use super::GitCommands;
use crate::model::{File, FileStatus};

impl GitCommands {
    pub fn load_files(&self) -> Result<Vec<File>> {
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
                format!("{} -> {}", unquote_porcelain_path(parts[0]), unquote_porcelain_path(parts.get(1).copied().unwrap_or("")))
            } else {
                unquote_porcelain_path(raw)
            };

            let display_name = if name.contains(" -> ") {
                // Renamed file: "old -> new"
                name.split(" -> ").last().unwrap_or(&name).to_string()
            } else {
                name.clone()
            };

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
                has_merge_conflicts: x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D'),
            });
        }

        Ok(files)
    }

    pub fn stage_file(&self, path: &str) -> Result<()> {
        self.git()
            .args(&["add", "--", path])
            .run_expecting_success()?;
        Ok(())
    }

    /// Stage multiple files in a single git command.
    pub fn stage_files(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["add", "--"];
        let refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        args.extend(refs);
        self.git().args(&args).run_expecting_success()?;
        Ok(())
    }

    pub fn unstage_file(&self, path: &str) -> Result<()> {
        self.git()
            .args(&["reset", "HEAD", "--", path])
            .run_expecting_success()?;
        Ok(())
    }

    /// Unstage multiple files in a single git command.
    pub fn unstage_files(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["reset", "HEAD", "--"];
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
        if self.git().args(&["rev-parse", "--verify", "HEAD"]).run().is_err() {
            self.git()
                .args(&["rm", "--cached", "-r", "."])
                .run_expecting_success()?;
        } else {
            self.git()
                .args(&["reset", "HEAD"])
                .run_expecting_success()?;
        }
        Ok(())
    }

    pub fn discard_file(&self, path: &str, added: bool) -> Result<()> {
        // Unstage first if needed (ignore errors — file may not be staged)
        let _ = self.git()
            .args(&["reset", "HEAD", "--", path])
            .run();

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
                b'a' => { out.push(0x07); i += 2; }
                b'b' => { out.push(0x08); i += 2; }
                b't' => { out.push(b'\t'); i += 2; }
                b'n' => { out.push(b'\n'); i += 2; }
                b'v' => { out.push(0x0b); i += 2; }
                b'f' => { out.push(0x0c); i += 2; }
                b'r' => { out.push(b'\r'); i += 2; }
                b'"' => { out.push(b'"'); i += 2; }
                b'\\' => { out.push(b'\\'); i += 2; }
                b'0'..=b'7' if i + 3 < inner.len()
                    && (b'0'..=b'7').contains(&inner[i + 2])
                    && (b'0'..=b'7').contains(&inner[i + 3]) =>
                {
                    let val = ((inner[i + 1] - b'0') << 6)
                        | ((inner[i + 2] - b'0') << 3)
                        | (inner[i + 3] - b'0');
                    out.push(val);
                    i += 4;
                }
                _ => { out.push(c); i += 1; }
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
        ('R', ' ') | ('R', 'M') => (true, false, true, FileStatus::Renamed),
        ('C', ' ') | ('C', 'M') => (true, false, true, FileStatus::Copied),
        ('U', 'U') | ('A', 'A') | ('D', 'D') | ('U', 'A') | ('A', 'U') | ('U', 'D') | ('D', 'U') => {
            (false, true, true, FileStatus::Unmerged)
        }
        _ => (x != ' ', y != ' ', true, FileStatus::Modified),
    }
}
