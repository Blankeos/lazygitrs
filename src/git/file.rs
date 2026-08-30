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

        // Match lazygit: only attach numstat/hunk counts for tracked paths.
        // Reading every untracked file (e.g. a full node_modules tree) makes
        // load_files hang for tens of seconds on large untracked worktrees.
        for file in files {
            if !file.tracked {
                continue;
            }
            let path = file.current_path().to_string();
            if let Some(&(additions, deletions)) = line_stats.get(&path) {
                file.additions = additions;
                file.deletions = deletions;
            }
            file.hunk_count = hunk_counts.get(&path).copied().unwrap_or(0);
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
        let _ = self
            .git()
            .args(&["--literal-pathspecs", "reset", "HEAD", "--", path])
            .run();

        if added {
            self.remove_worktree_path(path)?;
        } else {
            // Tracked file: discard working tree changes
            self.git()
                .args(&["--literal-pathspecs", "checkout", "--", path])
                .run_expecting_success()?;
        }
        Ok(())
    }

    /// Discard many files in a few git calls instead of one process per path.
    ///
    /// Mirrors lazygit `DiscardAllDirChanges`: bucket into reset / checkout /
    /// remove, then run each git command on path batches under ARG_MAX.
    pub fn discard_files(&self, files: &[File]) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        if files.len() == 1 {
            let file = &files[0];
            return self.discard_file(file.current_path(), file.added);
        }

        let mut special_files = Vec::new();
        let mut files_to_reset = Vec::new();
        let mut files_to_checkout = Vec::new();
        let mut files_to_remove = Vec::new();

        for file in files {
            let path = file.current_path().to_string();
            // Renames and certain merge-conflict statuses need per-file logic.
            if file.rename_paths().is_some()
                || file.short_status == "AA"
                || file.short_status == "DU"
            {
                special_files.push(file);
                continue;
            }

            if file.has_staged_changes || file.has_merge_conflicts {
                files_to_reset.push(path.clone());
                if file.short_status == "DD" || file.short_status == "AU" {
                    continue;
                }
                if file.added {
                    files_to_remove.push(path);
                } else {
                    files_to_checkout.push(path);
                }
                continue;
            }

            if file.short_status == "DD" || file.short_status == "AU" {
                continue;
            }

            if file.added {
                files_to_remove.push(path);
            } else {
                files_to_checkout.push(path);
            }
        }

        for file in special_files {
            self.discard_file(file.current_path(), file.added)?;
        }

        self.run_git_on_paths(&["reset", "HEAD"], &files_to_reset)?;
        for path in &files_to_remove {
            self.remove_worktree_path(path)?;
        }
        self.run_git_on_paths(&["checkout"], &files_to_checkout)
    }

    fn remove_worktree_path(&self, path: &str) -> Result<()> {
        let full_path = self.repo_path().join(path);
        if full_path.is_dir() {
            std::fs::remove_dir_all(&full_path)?;
        } else if full_path.exists() {
            std::fs::remove_file(&full_path)?;
        }
        Ok(())
    }

    /// Run `git <subcommand> -- <paths...>`, splitting to stay under ARG_MAX.
    /// Windows CreateProcess is ~32 KB; 30 KB matches lazygit's threshold.
    fn run_git_on_paths(&self, subcommand: &[&str], paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        for chunk in chunk_paths(paths, MAX_GIT_PATH_ARG_BYTES) {
            let mut args = Vec::with_capacity(1 + subcommand.len() + 1 + chunk.len());
            args.push("--literal-pathspecs");
            args.extend(subcommand.iter().copied());
            args.push("--");
            args.extend(chunk.iter().map(String::as_str));
            self.git().args(&args).run_expecting_success()?;
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
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "lazygitrs-{prefix}-{unique}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn git(repo: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(repo)
                .status()
                .expect("run git command")
                .success()
        );
    }

    fn init_repo(path: &Path) {
        git(path, &["init", "-q"]);
        git(path, &["config", "user.email", "test@example.com"]);
        git(path, &["config", "user.name", "Test"]);
    }

    fn seed_tracked_files(repo: &Path, n: usize) {
        std::fs::create_dir_all(repo.join("src")).expect("mkdir src");
        for i in 0..n {
            std::fs::write(repo.join("src").join(format!("f{i}.txt")), "original\n")
                .expect("write tracked file");
        }
        git(repo, &["add", "."]);
        git(repo, &["commit", "-qm", "init"]);
        for i in 0..n {
            std::fs::write(repo.join("src").join(format!("f{i}.txt")), "changed\n")
                .expect("modify tracked file");
        }
    }

    fn porcelain_paths(repo: &Path) -> Vec<String> {
        let output = Command::new("git")
            .args(["status", "--porcelain", "-uall"])
            .current_dir(repo)
            .output()
            .expect("git status");
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.len() >= 4)
            .map(|line| line[3..].to_string())
            .collect()
    }

    #[test]
    fn chunk_paths_splits_on_arg_max() {
        let long = |ch: char| ch.to_string().repeat(9_000);
        let p1 = long('a');
        let p2 = long('b');
        let p3 = long('c');
        let p4 = long('d');

        assert!(chunk_paths(&[], 30_000).is_empty());

        let fit_paths = [p1.clone(), p2.clone(), p3.clone()];
        let fit = chunk_paths(&fit_paths, 30_000);
        assert_eq!(fit.len(), 1);
        assert_eq!(fit[0].len(), 3);

        let split_paths = [p1, p2, p3, p4];
        let split = chunk_paths(&split_paths, 30_000);
        assert_eq!(split.len(), 2);
        assert_eq!(split[0].len(), 3);
        assert_eq!(split[1].len(), 1);
    }

    #[test]
    fn discard_files_handles_mixed_added_staged_and_modified() {
        let temp = TempDir::new("discard-mixed");
        let repo = &temp.path;
        init_repo(repo);

        std::fs::write(repo.join("tracked.txt"), "original\n").expect("write");
        std::fs::write(repo.join("staged.txt"), "original\n").expect("write");
        std::fs::write(repo.join("both.txt"), "original\n").expect("write");
        git(repo, &["add", "."]);
        git(repo, &["commit", "-qm", "init"]);

        std::fs::write(repo.join("tracked.txt"), "changed\n").expect("modify");
        std::fs::write(repo.join("staged.txt"), "changed\n").expect("modify");
        git(repo, &["add", "staged.txt"]);
        std::fs::write(repo.join("both.txt"), "staged\n").expect("modify");
        git(repo, &["add", "both.txt"]);
        std::fs::write(repo.join("both.txt"), "unstaged\n").expect("modify");
        std::fs::write(repo.join("untracked.txt"), "new\n").expect("write untracked");
        std::fs::write(repo.join("added.txt"), "added\n").expect("write added");
        git(repo, &["add", "added.txt"]);

        let git_cmds = GitCommands::new(repo).expect("git");
        let files = git_cmds.load_files_status_only().expect("status");
        assert_eq!(files.len(), 5);
        git_cmds.discard_files(&files).expect("discard");

        assert!(porcelain_paths(repo).is_empty());
        assert_eq!(
            std::fs::read_to_string(repo.join("tracked.txt")).unwrap(),
            "original\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("staged.txt")).unwrap(),
            "original\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("both.txt")).unwrap(),
            "original\n"
        );
        assert!(!repo.join("untracked.txt").exists());
        assert!(!repo.join("added.txt").exists());
    }

    #[test]
    fn discard_files_is_faster_than_per_file_on_large_changeset() {
        const N: usize = 120;
        let sequential_temp = TempDir::new("discard-seq");
        let batched_temp = TempDir::new("discard-batch");
        init_repo(&sequential_temp.path);
        init_repo(&batched_temp.path);
        seed_tracked_files(&sequential_temp.path, N);
        seed_tracked_files(&batched_temp.path, N);

        let sequential_git = GitCommands::new(&sequential_temp.path).expect("git");
        let sequential_files = sequential_git.load_files_status_only().expect("status");
        assert_eq!(sequential_files.len(), N);
        let sequential_start = std::time::Instant::now();
        for file in &sequential_files {
            sequential_git
                .discard_file(file.current_path(), file.added)
                .expect("discard_file");
        }
        let sequential = sequential_start.elapsed();

        let batched_git = GitCommands::new(&batched_temp.path).expect("git");
        let batched_files = batched_git.load_files_status_only().expect("status");
        assert_eq!(batched_files.len(), N);
        let batched_start = std::time::Instant::now();
        batched_git
            .discard_files(&batched_files)
            .expect("discard_files");
        let batched = batched_start.elapsed();

        eprintln!(
            "discard {N} files: sequential={sequential:?} batched={batched:?} ({:.1}x)",
            sequential.as_secs_f64() / batched.as_secs_f64().max(0.000_001)
        );

        assert!(porcelain_paths(&sequential_temp.path).is_empty());
        assert!(porcelain_paths(&batched_temp.path).is_empty());
        assert!(
            batched < sequential,
            "batched discard should beat per-file git spawn: sequential={sequential:?} batched={batched:?}"
        );
        assert!(
            batched.as_secs() < 3,
            "batched discard too slow on {N} files: {batched:?}"
        );
    }

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

/// Windows CreateProcess is ~32 KB; match lazygit's 30 KB path-batch limit.
const MAX_GIT_PATH_ARG_BYTES: usize = 30_000;

/// Split `paths` into batches whose joined length stays under `max_arg_bytes`.
fn chunk_paths(paths: &[String], max_arg_bytes: usize) -> Vec<&[String]> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < paths.len() {
        let mut end = start;
        let mut total = 0;
        while end < paths.len() {
            total += paths[end].len() + 1; // +1 for the separating space
            if total > max_arg_bytes && end > start {
                break;
            }
            end += 1;
        }
        chunks.push(&paths[start..end]);
        start = end;
    }
    chunks
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
