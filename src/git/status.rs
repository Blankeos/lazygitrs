use anyhow::Result;

use super::GitCommands;
use crate::os::cmd::CmdResult;

#[derive(Debug)]
pub struct RepoStatus {
    pub is_rebasing: bool,
    pub is_merging: bool,
    pub is_cherry_picking: bool,
    pub is_bisecting: bool,
    /// Short hash of the commit being rebased onto.
    pub rebase_onto_hash: String,
}

impl GitCommands {
    pub fn repo_status(&self) -> Result<RepoStatus> {
        // Use the resolved git dir, not repo_path/.git — in a linked worktree
        // .git is a file pointing at the real git dir, so probing
        // repo_path/.git/MERGE_HEAD would silently miss in-progress ops.
        let git_dir = &self.git_dir;

        let is_rebasing = self.is_rebase_in_progress();

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

        // ahead/behind are intentionally absent here: the branches panel
        // already derives them from `%(upstream:track)` in load_branches, so
        // a separate `rev-list --left-right` subprocess per refresh is waste.
        Ok(RepoStatus {
            is_rebasing,
            is_merging: git_dir.join("MERGE_HEAD").exists(),
            is_cherry_picking: git_dir.join("CHERRY_PICK_HEAD").exists(),
            is_bisecting: git_dir.join("BISECT_LOG").exists(),
            rebase_onto_hash,
        })
    }

    pub fn continue_rebase(&self) -> Result<()> {
        let result = self
            .git()
            .args(&["rebase", "--continue"])
            .env("GIT_EDITOR", "true")
            .run()?;
        self.handle_rebase_step_result("--continue", result)
    }

    pub fn abort_rebase(&self) -> Result<()> {
        self.git()
            .args(&["rebase", "--abort"])
            .run_expecting_success()?;
        Ok(())
    }

    pub(crate) fn is_rebase_in_progress(&self) -> bool {
        self.git_dir.join("rebase-merge").exists() || self.git_dir.join("rebase-apply").exists()
    }

    pub(crate) fn handle_rebase_step_result(&self, action: &str, result: CmdResult) -> Result<()> {
        if result.success {
            return Ok(());
        }

        if self.is_rebase_in_progress() && rebase_step_paused_on_next_conflict(&result) {
            return Ok(());
        }

        anyhow::bail!(
            "Command failed (exit {}): git rebase {}\n\n{}",
            result.exit_code.unwrap_or(-1),
            action,
            combined_output(&result),
        );
    }

    pub fn abort_merge(&self) -> Result<()> {
        self.git()
            .args(&["merge", "--abort"])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn continue_cherry_pick(&self) -> Result<()> {
        self.git()
            .args(&["cherry-pick", "--continue"])
            .env("GIT_EDITOR", "true")
            .run_expecting_success()?;
        Ok(())
    }

    pub fn abort_cherry_pick(&self) -> Result<()> {
        self.git()
            .args(&["cherry-pick", "--abort"])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn skip_cherry_pick(&self) -> Result<()> {
        self.git()
            .args(&["cherry-pick", "--skip"])
            .run_expecting_success()?;
        Ok(())
    }
}

fn rebase_step_paused_on_next_conflict(result: &CmdResult) -> bool {
    let output = format!("{}\n{}", result.stdout, result.stderr);
    output.contains("CONFLICT")
        || output.contains("could not apply")
        || output.contains("Could not apply")
}

fn combined_output(result: &CmdResult) -> String {
    let stdout = result.stdout.trim();
    let stderr = result.stderr.trim();

    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "No output from git.".to_string(),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (false, false) => format!("{}\n{}", stdout, stderr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed_result(stdout: &str, stderr: &str) -> CmdResult {
        CmdResult {
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            success: false,
            exit_code: Some(1),
        }
    }

    #[test]
    fn detects_pause_on_next_conflicting_commit() {
        let result = failed_result(
            "[detached HEAD abc1234] resolved current\nAuto-merging f.txt\nCONFLICT (content): Merge conflict in f.txt",
            "error: could not apply def5678... next commit",
        );

        assert!(rebase_step_paused_on_next_conflict(&result));
    }

    #[test]
    fn keeps_unresolved_current_conflicts_as_an_error() {
        let result = failed_result(
            "f.txt: needs merge\nYou must edit all merge conflicts and then\nmark them as resolved using git add",
            "",
        );

        assert!(!rebase_step_paused_on_next_conflict(&result));
    }

    #[test]
    fn error_output_includes_stdout_when_git_writes_there() {
        let result = failed_result("f.txt: needs merge", "");

        assert_eq!(combined_output(&result), "f.txt: needs merge");
    }
}
