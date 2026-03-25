use anyhow::Result;

use super::GitCommands;

impl GitCommands {
    /// Get diff for a specific file (unstaged changes).
    pub fn diff_file(&self, path: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["diff", "--color=never", "--", path])
            .run_expecting_success()?;
        Ok(result.stdout)
    }

    /// Get diff for a specific file (staged changes).
    pub fn diff_file_staged(&self, path: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["diff", "--cached", "--color=never", "--", path])
            .run_expecting_success()?;
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
        let result = self
            .git()
            .args(&["diff", "--cached", "--color=never"])
            .run_expecting_success()?;
        Ok(result.stdout)
    }

    /// Get diff for a specific commit.
    pub fn diff_commit(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["show", "--color=never", "--format=", hash])
            .run_expecting_success()?;
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
    pub fn file_content(&self, path: &str) -> Result<String> {
        let full_path = self.repo_path().join(path);
        Ok(std::fs::read_to_string(full_path).unwrap_or_default())
    }

    /// Get total insertions/deletions across all working tree changes,
    /// including untracked files (counted as all additions).
    /// Uses `git diff HEAD` to get the combined staged+unstaged delta from HEAD.
    pub fn diff_shortstat(&self) -> Result<(usize, usize)> {
        let result = self
            .git()
            .args(&["diff", "HEAD", "--shortstat"])
            .run()?;

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

        let (added, deleted) = parse_stat(&result.stdout);

        // Count lines in untracked files (git diff ignores these)
        let untracked_lines = self.count_untracked_lines().unwrap_or(0);

        Ok((added + untracked_lines, deleted))
    }

    /// Count total lines across all untracked files.
    fn count_untracked_lines(&self) -> Result<usize> {
        let result = self
            .git()
            .args(&["ls-files", "--others", "--exclude-standard"])
            .run()?;

        let mut total = 0;
        for file in result.stdout.lines() {
            if file.is_empty() {
                continue;
            }
            let path = self.repo_path().join(file);
            if let Ok(content) = std::fs::read_to_string(&path) {
                total += content.lines().count();
            }
            // Skip binary files that fail read_to_string
        }

        Ok(total)
    }

    /// Get the list of files changed in a commit with their change status.
    pub fn commit_files(&self, hash: &str) -> Result<Vec<crate::model::CommitFile>> {
        let result = self
            .git()
            .args(&["diff-tree", "--no-commit-id", "--name-status", "-r", hash])
            .run_expecting_success()?;

        let mut files = Vec::new();
        for line in result.stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            // Format: "M\tpath/to/file" or "R100\told\tnew"
            let mut parts = line.splitn(2, '\t');
            let status_str = parts.next().unwrap_or("");
            let name = parts.next().unwrap_or("").to_string();
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

            files.push(crate::model::CommitFile { name, status });
        }
        Ok(files)
    }

    /// Get the diff of a single file within a commit.
    pub fn diff_commit_file(&self, hash: &str, path: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["show", "--color=never", "--format=", hash, "--", path])
            .run_expecting_success()?;
        Ok(result.stdout)
    }

    /// Get the staged content of a file.
    pub fn file_content_staged(&self, path: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["show", &format!(":{}", path)])
            .run()?;
        if result.success {
            Ok(result.stdout)
        } else {
            Ok(String::new())
        }
    }
}
