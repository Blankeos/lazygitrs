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
