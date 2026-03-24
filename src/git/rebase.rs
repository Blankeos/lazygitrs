use anyhow::Result;

use super::GitCommands;

/// Actions that can be performed on commits during interactive rebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseAction {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl RebaseAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Reword => "reword",
            Self::Edit => "edit",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            Self::Drop => "drop",
        }
    }
}

impl GitCommands {
    /// Interactive rebase: apply a single action to a specific commit.
    /// Uses GIT_SEQUENCE_EDITOR to non-interactively modify the todo list.
    pub fn rebase_interactive_action(
        &self,
        commit_hash: &str,
        action: RebaseAction,
    ) -> Result<()> {
        // Find the parent of the target commit for the rebase base
        let parent = self.commit_parent(commit_hash)?;

        // Build a sed command to change "pick <hash>" to "<action> <hash>"
        let short_hash = &commit_hash[..7.min(commit_hash.len())];
        let sed_cmd = format!(
            "sed -i '' 's/^pick {} /{} {} /'",
            short_hash,
            action.as_str(),
            short_hash
        );

        self.git()
            .args(&["rebase", "-i", &parent])
            .env("GIT_SEQUENCE_EDITOR", &sed_cmd)
            .run_expecting_success()?;
        Ok(())
    }

    /// Move a commit up in the history (swap with the one above it).
    pub fn move_commit_up(&self, commit_hash: &str) -> Result<()> {
        let parent = self.commit_parent(commit_hash)?;
        let grandparent = self.commit_parent(&parent)?;

        let short_hash = &commit_hash[..7.min(commit_hash.len())];
        let short_parent = &parent[..7.min(parent.len())];

        // Swap the two lines in the todo list
        let sed_cmd = format!(
            "sed -i '' '/^pick {}/{{ N; s/^\\(pick {}.*\\)\\n\\(pick {}.*\\)/\\2\\n\\1/ }}'",
            short_parent, short_parent, short_hash
        );

        self.git()
            .args(&["rebase", "-i", &grandparent])
            .env("GIT_SEQUENCE_EDITOR", &sed_cmd)
            .run_expecting_success()?;
        Ok(())
    }

    /// Move a commit down in the history (swap with the one below it).
    pub fn move_commit_down(&self, commit_hash: &str) -> Result<()> {
        // Find the commit below (child direction = parent in rebase list)
        let parent = self.commit_parent(commit_hash)?;
        let grandparent = self.commit_parent(&parent)?;

        let short_hash = &commit_hash[..7.min(commit_hash.len())];
        let short_parent = &parent[..7.min(parent.len())];

        // Swap: move the target commit below the parent
        let sed_cmd = format!(
            "sed -i '' '/^pick {}/{{ N; s/^\\(pick {}.*\\)\\n\\(pick {}.*\\)/\\2\\n\\1/ }}'",
            short_hash, short_hash, short_parent
        );

        self.git()
            .args(&["rebase", "-i", &grandparent])
            .env("GIT_SEQUENCE_EDITOR", &sed_cmd)
            .run_expecting_success()?;
        Ok(())
    }

    /// Squash a commit into its parent.
    pub fn squash_commit(&self, commit_hash: &str) -> Result<()> {
        self.rebase_interactive_action(commit_hash, RebaseAction::Squash)
    }

    /// Fixup a commit into its parent (discard its message).
    pub fn fixup_commit(&self, commit_hash: &str) -> Result<()> {
        self.rebase_interactive_action(commit_hash, RebaseAction::Fixup)
    }

    /// Drop a commit from history.
    pub fn drop_commit(&self, commit_hash: &str) -> Result<()> {
        self.rebase_interactive_action(commit_hash, RebaseAction::Drop)
    }

    /// Reword a non-HEAD commit via interactive rebase.
    pub fn reword_commit_rebase(&self, commit_hash: &str, new_message: &str) -> Result<()> {
        let parent = self.commit_parent(commit_hash)?;
        let short_hash = &commit_hash[..7.min(commit_hash.len())];

        // First, set the action to "reword"
        let sed_cmd = format!(
            "sed -i '' 's/^pick {} /reword {} /'",
            short_hash, short_hash
        );

        // Use GIT_SEQUENCE_EDITOR for the todo list and EDITOR for the message
        let echo_cmd = format!("echo '{}' >", new_message.replace('\'', "'\\''"));

        self.git()
            .args(&["rebase", "-i", &parent])
            .env("GIT_SEQUENCE_EDITOR", &sed_cmd)
            .env("GIT_EDITOR", &echo_cmd)
            .run_expecting_success()?;
        Ok(())
    }

    /// Create a fixup commit for the given target commit.
    pub fn create_fixup_commit(&self, target_hash: &str) -> Result<()> {
        self.git()
            .args(&["commit", "--fixup", target_hash])
            .run_expecting_success()?;
        Ok(())
    }

    /// Autosquash: rebase with --autosquash to apply fixup/squash commits.
    pub fn rebase_autosquash(&self, base_hash: &str) -> Result<()> {
        self.git()
            .args(&["rebase", "-i", "--autosquash", base_hash])
            .env("GIT_SEQUENCE_EDITOR", "true")
            .run_expecting_success()?;
        Ok(())
    }

    /// Skip during a rebase (when there's a conflict).
    pub fn rebase_skip(&self) -> Result<()> {
        self.git()
            .args(&["rebase", "--skip"])
            .run_expecting_success()?;
        Ok(())
    }

    /// Get the parent hash of a commit.
    fn commit_parent(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["rev-parse", &format!("{}^", hash)])
            .run_expecting_success()?;
        Ok(result.stdout_trimmed().to_string())
    }
}
