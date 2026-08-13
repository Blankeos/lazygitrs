use anyhow::Result;

use super::GitCommands;
use super::file::{parse_hunk_counts, parse_numstat_z};

impl GitCommands {
    /// Get diff for a specific file (unstaged changes).
    pub fn diff_file(&self, path: &str) -> Result<String> {
        let paths = diff_paths_for_label(path);
        self.diff_file_paths(&paths)
    }

    /// Get diff for one logical file using one or more pathspecs.
    /// Renames need both old and new pathspecs for Git to show the rename as a
    /// single file rather than as independent delete/add halves.
    pub fn diff_file_paths(&self, paths: &[&str]) -> Result<String> {
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--color=never",
            "--",
        ];
        args.extend(paths.iter().copied());
        let result = self.git().args(&args).run_expecting_success()?;
        Ok(result.stdout)
    }

    /// Get diff for a specific file (staged changes).
    pub fn diff_file_staged(&self, path: &str) -> Result<String> {
        let paths = diff_paths_for_label(path);
        self.diff_file_staged_paths(&paths)
    }

    /// Get staged diff for one logical file using one or more pathspecs.
    pub fn diff_file_staged_paths(&self, paths: &[&str]) -> Result<String> {
        let mut args = vec![
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--color=never",
            "--",
        ];
        args.extend(paths.iter().copied());
        let result = self.git().args(&args).run_expecting_success()?;
        Ok(result.stdout)
    }

    /// Get diff for all files (staged + unstaged combined).
    pub fn diff_all(&self) -> Result<String> {
        let result = self
            .git()
            .args(&["diff", "HEAD", "--color=never"])
            .run_expecting_success()?;
        Ok(result.stdout)
    }

    /// Get the full staged diff (for AI commit generation).
    pub fn diff_staged(&self) -> Result<String> {
        self.diff_staged_paths(&[])
    }

    /// Staged diff limited to pathspecs (empty = all staged).
    pub fn diff_staged_paths(&self, paths: &[&str]) -> Result<String> {
        let mut args = vec![
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--color=never",
        ];
        if !paths.is_empty() {
            args.push("--");
            args.extend(paths.iter().copied());
        }
        let result = self.git().args(&args).run_expecting_success()?;
        Ok(result.stdout)
    }

    /// Get diff for a specific commit.
    pub fn diff_commit(&self, hash: &str) -> Result<String> {
        self.diff_commit_paths(hash, &[])
    }

    /// Get a commit's diff, optionally limited to pathspecs (dirs/files).
    /// Empty `paths` = whole commit. One git process — used for directory
    /// hover in Commit Files instead of N× per-file spawns.
    pub fn diff_commit_paths(&self, hash: &str, paths: &[&str]) -> Result<String> {
        let parent = format!("{}^1", hash);
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--color=never",
            parent.as_str(),
            hash,
        ];
        if !paths.is_empty() {
            args.push("--");
            args.extend(paths.iter().copied());
        }
        let result = self.git().args(&args).run();
        match result {
            Ok(r) if r.success => Ok(r.stdout),
            _ => {
                // Root commits have no parent — fall back to `git show`.
                let mut args = vec![
                    "show",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--color=never",
                    "--format=",
                    hash,
                ];
                if !paths.is_empty() {
                    args.push("--");
                    args.extend(paths.iter().copied());
                }
                let result = self.git().args(&args).run_expecting_success()?;
                Ok(result.stdout)
            }
        }
    }

    /// Working-tree + index vs HEAD for pathspecs (directory hover in Files).
    pub fn diff_paths_vs_head(&self, paths: &[&str]) -> Result<String> {
        let mut args = vec![
            "diff",
            "HEAD",
            "--no-ext-diff",
            "--no-textconv",
            "--color=never",
        ];
        if !paths.is_empty() {
            args.push("--");
            args.extend(paths.iter().copied());
        }
        let result = self.git().args(&args).run_expecting_success()?;
        Ok(result.stdout)
    }

    /// Get the old and new content of a file for side-by-side diff.
    pub fn file_content_at_commit(&self, hash: &str, path: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["show", &format!("{}:{}", hash, path)])
            .run()?;
        if result.success {
            Ok(result.stdout)
        } else {
            Ok(String::new())
        }
    }

    /// Get the current working tree content of a file.
    /// Returns empty for missing/unreadable files. Binary files are not
    /// returned as text — use [`Self::is_binary_path`] and synthesize a
    /// binary placeholder instead.
    pub fn file_content(&self, path: &str) -> Result<String> {
        let full_path = self.repo_path().join(path);
        let bytes = match std::fs::read(&full_path) {
            Ok(b) => b,
            Err(_) => return Ok(String::new()),
        };
        if bytes_look_binary(&bytes) {
            return Ok(String::new());
        }
        Ok(String::from_utf8(bytes).unwrap_or_default())
    }

    /// True if the worktree path exists and looks binary (NUL / invalid UTF-8).
    pub fn is_binary_path(&self, path: &str) -> bool {
        let full_path = self.repo_path().join(path);
        match std::fs::read(full_path) {
            Ok(bytes) => bytes_look_binary(&bytes),
            Err(_) => false,
        }
    }

    /// Get total insertions/deletions across tracked working tree changes.
    /// Uses `git diff HEAD` for the combined staged+unstaged delta from HEAD.
    /// Untracked files are omitted (matching lazygit) — reading every untracked
    /// path is prohibitively slow on large trees like node_modules.
    pub fn diff_shortstat(&self) -> Result<(usize, usize)> {
        // Unborn HEAD (no commits yet): fall back to index vs empty tree / worktree.
        let result = if self
            .git()
            .args(&["rev-parse", "--verify", "HEAD"])
            .run()
            .is_ok_and(|r| r.success)
        {
            self.git().args(&["diff", "HEAD", "--shortstat"]).run()?
        } else {
            // No HEAD: only staged changes have a meaningful shortstat.
            self.git()
                .args(&["diff", "--cached", "--shortstat"])
                .run()?
        };

        fn parse_stat(s: &str) -> (usize, usize) {
            let mut added = 0usize;
            let mut deleted = 0usize;
            // Format: " 3 files changed, 10 insertions(+), 2 deletions(-)"
            for part in s.split(',') {
                let part = part.trim();
                if part.contains("insertion") {
                    if let Some(n) = part.split_whitespace().next().and_then(|w| w.parse().ok()) {
                        added = n;
                    }
                } else if part.contains("deletion") {
                    if let Some(n) = part.split_whitespace().next().and_then(|w| w.parse().ok()) {
                        deleted = n;
                    }
                }
            }
            (added, deleted)
        }

        Ok(parse_stat(&result.stdout))
    }

    /// Get the list of files changed in a commit with their change status.
    /// Uses `hash^1..hash` to correctly handle merge commits (including stashes).
    /// Falls back to single-arg diff-tree for root commits (no parent).
    pub fn commit_files(&self, hash: &str) -> Result<Vec<crate::model::CommitFile>> {
        self.commit_files_inner(hash, true)
    }

    /// Get the list of files changed in a commit without computing diff stats.
    /// Useful on latency-sensitive paths where the file diff is loaded separately.
    pub fn commit_files_without_stats(&self, hash: &str) -> Result<Vec<crate::model::CommitFile>> {
        self.commit_files_inner(hash, false)
    }

    fn commit_files_inner(
        &self,
        hash: &str,
        include_stats: bool,
    ) -> Result<Vec<crate::model::CommitFile>> {
        // Try diffing against first parent; fall back for root commits.
        // Root commits need `--root` so git compares against the empty tree;
        // plain `diff-tree <hash>` succeeds but prints nothing for roots.
        let result = self
            .git()
            .args(&[
                "diff-tree",
                "--no-commit-id",
                "--name-status",
                "-r",
                &format!("{}^1", hash),
                hash,
            ])
            .run();
        let (result, stats_base) = match result {
            Ok(r) if r.success => (
                r,
                vec!["diff".to_string(), format!("{}^1", hash), hash.to_string()],
            ),
            _ => (
                self.git()
                    .args(&[
                        "diff-tree",
                        "--no-commit-id",
                        "--name-status",
                        "-r",
                        "--root",
                        hash,
                    ])
                    .run_expecting_success()?,
                vec![
                    "show".to_string(),
                    "--format=".to_string(),
                    hash.to_string(),
                ],
            ),
        };

        let mut files = Vec::new();
        for line in result.stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Format: "M\tpath/to/file" or "R100\told\tnew"
            let mut parts = line.split('\t');
            let status_str = parts.next().unwrap_or("");
            let first_path = parts.next().unwrap_or("");
            let second_path = parts.next();
            let name = if matches!(status_str.chars().next(), Some('R' | 'C')) {
                second_path
                    .map(|new_path| format!("{} -> {}", first_path, new_path))
                    .unwrap_or_else(|| first_path.to_string())
            } else {
                first_path.to_string()
            };
            if name.is_empty() {
                continue;
            }

            let status = match status_str.chars().next() {
                Some('A') => crate::model::FileChangeStatus::Added,
                Some('D') => crate::model::FileChangeStatus::Deleted,
                Some('R') => crate::model::FileChangeStatus::Renamed,
                Some('C') => crate::model::FileChangeStatus::Copied,
                Some('U') => crate::model::FileChangeStatus::Unmerged,
                _ => crate::model::FileChangeStatus::Modified,
            };

            files.push(crate::model::CommitFile {
                name,
                status,
                hunk_count: 0,
                additions: 0,
                deletions: 0,
            });
        }
        if include_stats {
            self.populate_commit_file_stats(&mut files, &stats_base);
        }
        Ok(files)
    }

    /// Get the diff of a single file within a commit.
    /// Uses `hash^1..hash` to correctly handle merge commits (including stashes).
    /// Falls back to `git show` for root commits (no parent).
    pub fn diff_commit_file(&self, hash: &str, path: &str) -> Result<String> {
        let parent = format!("{}^1", hash);
        let paths = diff_paths_for_label(path);
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--color=never",
            parent.as_str(),
            hash,
            "--",
        ];
        args.extend(paths.iter().copied());
        let result = self.git().args(&args).run();
        match result {
            Ok(r) if r.success => Ok(r.stdout),
            _ => {
                let current_path = current_path_for_label(path);
                let r = self
                    .git()
                    .args(&[
                        "show",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--color=never",
                        "--format=",
                        hash,
                        "--",
                        current_path,
                    ])
                    .run_expecting_success()?;
                Ok(r.stdout)
            }
        }
    }

    /// Get the list of files changed between two refs (for diff/compare mode).
    pub fn diff_refs_files(
        &self,
        ref_a: &str,
        ref_b: &str,
    ) -> Result<Vec<crate::model::CommitFile>> {
        let result = self
            .git()
            .args(&["diff", "--name-status", ref_a, ref_b])
            .run_expecting_success()?;

        let mut files = Vec::new();
        for line in result.stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split('\t');
            let status_str = parts.next().unwrap_or("");
            let first_path = parts.next().unwrap_or("");
            let second_path = parts.next();
            let name = if matches!(status_str.chars().next(), Some('R' | 'C')) {
                second_path
                    .map(|new_path| format!("{} -> {}", first_path, new_path))
                    .unwrap_or_else(|| first_path.to_string())
            } else {
                first_path.to_string()
            };
            if name.is_empty() {
                continue;
            }
            let status = match status_str.chars().next() {
                Some('A') => crate::model::FileChangeStatus::Added,
                Some('D') => crate::model::FileChangeStatus::Deleted,
                Some('R') => crate::model::FileChangeStatus::Renamed,
                Some('C') => crate::model::FileChangeStatus::Copied,
                Some('U') => crate::model::FileChangeStatus::Unmerged,
                _ => crate::model::FileChangeStatus::Modified,
            };
            files.push(crate::model::CommitFile {
                name,
                status,
                hunk_count: 0,
                additions: 0,
                deletions: 0,
            });
        }
        self.populate_commit_file_stats(
            &mut files,
            &["diff".to_string(), ref_a.to_string(), ref_b.to_string()],
        );
        Ok(files)
    }

    /// Enrich a commit-like file list with line and hunk counts in two bulk
    /// Git calls. Stats are best-effort so the file list remains available if
    /// a particular diff cannot be produced.
    fn populate_commit_file_stats(
        &self,
        files: &mut [crate::model::CommitFile],
        diff_base: &[String],
    ) {
        let run = |extra: &[&str]| {
            let mut args = diff_base.to_vec();
            args.extend(extra.iter().map(|arg| (*arg).to_string()));
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            self.git()
                .args(&refs)
                .run()
                .ok()
                .filter(|result| result.success)
                .map(|result| result.stdout)
        };

        let line_stats = run(&["--numstat", "-z", "--find-renames", "--no-color"])
            .map(|output| parse_numstat_z(&output))
            .unwrap_or_default();
        let hunk_counts = run(&["--unified=0", "--find-renames", "--no-color", "--no-prefix"])
            .map(|output| parse_hunk_counts(&output))
            .unwrap_or_default();

        for file in files {
            let path = file.current_path().to_string();
            if let Some(&(additions, deletions)) = line_stats.get(&path) {
                file.additions = additions;
                file.deletions = deletions;
            }
            file.hunk_count = hunk_counts.get(&path).copied().unwrap_or(0);
        }
    }

    /// Diff between two refs, optionally limited to pathspecs.
    pub fn diff_refs_paths(&self, ref_a: &str, ref_b: &str, paths: &[&str]) -> Result<String> {
        let mut args = vec![
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--color=never",
            ref_a,
            ref_b,
        ];
        if !paths.is_empty() {
            args.push("--");
            args.extend(paths.iter().copied());
        }
        let result = self.git().args(&args).run_expecting_success()?;
        Ok(result.stdout)
    }

    /// Get the diff of a single file between two refs (for diff/compare mode).
    pub fn diff_refs_file(&self, ref_a: &str, ref_b: &str, path: &str) -> Result<String> {
        let paths = diff_paths_for_label(path);
        self.diff_refs_paths(ref_a, ref_b, &paths)
    }

    /// Get the staged content of a file.
    pub fn file_content_staged(&self, path: &str) -> Result<String> {
        let result = self.git().args(&["show", &format!(":{}", path)]).run()?;
        if result.success {
            Ok(result.stdout)
        } else {
            Ok(String::new())
        }
    }
}

fn diff_paths_for_label(path: &str) -> Vec<&str> {
    match path.split_once(" -> ") {
        Some((old, new)) => vec![old, new],
        None => vec![path],
    }
}

/// Heuristic: NUL byte, or invalid UTF-8 → treat as binary.
fn bytes_look_binary(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return true;
    }
    std::str::from_utf8(bytes).is_err()
}

fn current_path_for_label(path: &str) -> &str {
    path.split_once(" -> ").map_or(path, |(_, new)| new)
}
