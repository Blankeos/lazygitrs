pub mod branch;
pub mod commit;
pub mod diff;
pub mod file;
pub mod loader;
pub mod remote;
pub mod stash;
pub mod status;
pub mod tag;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::model::Model;
use crate::os::cmd::CmdBuilder;

/// Facade for all git operations. Mirrors lazygit's GitCommand.
pub struct GitCommands {
    repo_path: PathBuf,
}

impl GitCommands {
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo_path = repo_path.canonicalize()?;
        Ok(Self { repo_path })
    }

    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    fn git(&self) -> CmdBuilder {
        CmdBuilder::git().cwd_path(&self.repo_path)
    }

    /// Load all model data from the repository.
    pub fn load_model(&self) -> Result<Model> {
        let mut model = Model::default();

        model.files = self.load_files()?;
        model.branches = self.load_branches()?;
        model.commits = self.load_commits(50)?;
        model.stash_entries = self.load_stash()?;
        model.remotes = self.load_remotes()?;
        model.tags = self.load_tags()?;

        Ok(model)
    }

    /// Refresh just the working tree files.
    pub fn refresh_files(&self) -> Result<Vec<crate::model::File>> {
        self.load_files()
    }

    /// Refresh just branches.
    pub fn refresh_branches(&self) -> Result<Vec<crate::model::Branch>> {
        self.load_branches()
    }

    /// Get the current branch name.
    pub fn current_branch_name(&self) -> Result<String> {
        let result = self.git().args(&["branch", "--show-current"]).run()?;
        Ok(result.stdout_trimmed().to_string())
    }

    /// Get the repo name (last component of path).
    pub fn repo_name(&self) -> String {
        self.repo_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Check if the working directory is a valid git repo.
    pub fn is_valid_repo(path: &Path) -> bool {
        CmdBuilder::git()
            .cwd_path(path)
            .args(&["rev-parse", "--git-dir"])
            .run()
            .map(|r| r.success)
            .unwrap_or(false)
    }

    /// Get the HEAD commit hash.
    pub fn head_hash(&self) -> Result<String> {
        let result = self
            .git()
            .args(&["rev-parse", "HEAD"])
            .run_expecting_success()?;
        Ok(result.stdout_trimmed().to_string())
    }
}
