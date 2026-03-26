use anyhow::Result;

use super::GitCommands;

#[derive(Debug)]
pub struct RepoStatus {
    pub branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub is_rebasing: bool,
    pub is_merging: bool,
    pub is_cherry_picking: bool,
    pub is_bisecting: bool,
    /// Short hash of the commit being rebased onto.
    pub rebase_onto_hash: String,
}

impl GitCommands {
    pub fn repo_status(&self) -> Result<RepoStatus> {
        let branch = self.current_branch_name().unwrap_or_else(|_| "HEAD".to_string());

        let (ahead, behind) = self.ahead_behind().unwrap_or((0, 0));

        let git_dir = self.repo_path().join(".git");

        let is_rebasing = git_dir.join("rebase-merge").exists()
            || git_dir.join("rebase-apply").exists();

        // Read the "onto" hash when rebasing
        let rebase_onto_hash = if is_rebasing {
            // Try rebase-merge/onto first, then rebase-apply/onto
            std::fs::read_to_string(git_dir.join("rebase-merge/onto"))
                .or_else(|_| std::fs::read_to_string(git_dir.join("rebase-apply/onto")))
                .map(|s| {
                    let full = s.trim().to_string();
                    // Return short hash (first 12 chars)
                    full[..12.min(full.len())].to_string()
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        Ok(RepoStatus {
            branch,
            ahead,
            behind,
            is_rebasing,
            is_merging: git_dir.join("MERGE_HEAD").exists(),
            is_cherry_picking: git_dir.join("CHERRY_PICK_HEAD").exists(),
            is_bisecting: git_dir.join("BISECT_LOG").exists(),
            rebase_onto_hash,
        })
    }

    fn ahead_behind(&self) -> Result<(usize, usize)> {
        let result = self
            .git()
            .args(&["rev-list", "--left-right", "--count", "HEAD...@{u}"])
            .run()?;

        if !result.success {
            return Ok((0, 0));
        }

        let parts: Vec<&str> = result.stdout_trimmed().split_whitespace().collect();
        if parts.len() == 2 {
            let ahead = parts[0].parse().unwrap_or(0);
            let behind = parts[1].parse().unwrap_or(0);
            Ok((ahead, behind))
        } else {
            Ok((0, 0))
        }
    }

    pub fn continue_rebase(&self) -> Result<()> {
        self.git()
            .args(&["rebase", "--continue"])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn abort_rebase(&self) -> Result<()> {
        self.git()
            .args(&["rebase", "--abort"])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn abort_merge(&self) -> Result<()> {
        self.git()
            .args(&["merge", "--abort"])
            .run_expecting_success()?;
        Ok(())
    }
}
