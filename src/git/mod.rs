pub mod ai_commit;
pub mod bisect;
pub mod branch;
pub mod commit;
pub mod diff;
pub mod file;
pub mod loader;
pub mod rebase;
pub mod remote;
pub mod staging;
pub mod stash;
pub mod status;
pub mod submodule;
pub mod tag;
pub mod worktree;

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};

use anyhow::Result;

use crate::model::{self, Model};
use crate::os::cmd::CmdBuilder;

/// A single piece of model data loaded from git. Each variant arrives
/// independently so the UI can display whichever data is ready first.
pub enum ModelPart {
    Files(Vec<model::File>),
    Branches(Vec<model::Branch>),
    Commits(Vec<model::Commit>),
    Stash(Vec<model::StashEntry>),
    Remotes(Vec<model::Remote>),
    Tags(Vec<model::Tag>),
    Worktrees(Vec<model::Worktree>),
    Submodules(Vec<submodule::Submodule>),
    Reflog(Vec<model::Commit>),
    DiffStats { added: usize, deleted: usize },
    RepoStatus {
        is_rebasing: bool,
        is_merging: bool,
        is_cherry_picking: bool,
        is_bisecting: bool,
        rebase_onto_hash: String,
    },
}

/// Total number of `ModelPart` variants that `load_model_streaming` sends.
pub const MODEL_PART_COUNT: usize = 11;

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

    /// Public access to the git command builder.
    pub fn git_cmd(&self) -> CmdBuilder {
        CmdBuilder::git().cwd_path(&self.repo_path)
    }

    /// Load all model data from the repository.
    ///
    /// Git commands are run in parallel using scoped threads since they are
    /// all independent reads against the same repo.
    pub fn load_model(&self) -> Result<Model> {
        let mut model = Model::default();

        model.repo_name = self.repo_name();
        model.head_hash = self.head_hash().unwrap_or_default();
        model.head_branch_name = self.current_branch_name().unwrap_or_default();

        // Run all independent git loads in parallel.
        std::thread::scope(|s| {
            let h_files = s.spawn(|| self.load_files());
            let h_branches = s.spawn(|| self.load_branches());
            let h_commits = s.spawn(|| self.load_commits(0));
            let h_stash = s.spawn(|| self.load_stash());
            let h_remotes = s.spawn(|| self.load_remotes());
            let h_tags = s.spawn(|| self.load_tags());
            let h_worktrees = s.spawn(|| self.load_worktrees());
            let h_submodules = s.spawn(|| self.load_submodules());
            let h_reflog = s.spawn(|| self.load_reflog(100));
            let h_shortstat = s.spawn(|| self.diff_shortstat());
            let h_status = s.spawn(|| self.repo_status());

            model.files = h_files.join().unwrap()?;
            model.branches = h_branches.join().unwrap()?;
            model.commits = h_commits.join().unwrap()?;
            model.stash_entries = h_stash.join().unwrap()?;
            model.remotes = h_remotes.join().unwrap()?;
            model.tags = h_tags.join().unwrap()?;
            model.worktrees = h_worktrees.join().unwrap().unwrap_or_default();
            model.submodules = h_submodules.join().unwrap().unwrap_or_default();
            model.reflog_commits = h_reflog.join().unwrap().unwrap_or_default();

            if let Ok((added, deleted)) = h_shortstat.join().unwrap() {
                model.total_additions = added;
                model.total_deletions = deleted;
            }

            if let Ok(status) = h_status.join().unwrap() {
                model.is_rebasing = status.is_rebasing;
                model.is_merging = status.is_merging;
                model.is_cherry_picking = status.is_cherry_picking;
                model.is_bisecting = status.is_bisecting;
                model.rebase_onto_hash = status.rebase_onto_hash;
            }

            Ok(model)
        })
    }

    /// Load model data by spawning one thread per data type. Each thread
    /// sends its result through `tx` as soon as it finishes, so the UI can
    /// waterfall-display whichever data arrives first.
    ///
    /// The caller should also set `model.repo_name` and `model.head_hash`
    /// synchronously since those are cheap.
    pub fn load_model_streaming(self: &Arc<Self>, tx: &mpsc::Sender<ModelPart>) {
        macro_rules! spawn_part {
            ($tx:expr, $self:expr, $variant:ident, $expr:expr) => {{
                let tx = $tx.clone();
                let git = Arc::clone($self);
                std::thread::spawn(move || {
                    if let Ok(data) = $expr(&git) {
                        let _ = tx.send(ModelPart::$variant(data));
                    }
                });
            }};
        }

        spawn_part!(tx, self, Files, |g: &GitCommands| g.load_files());
        spawn_part!(tx, self, Branches, |g: &GitCommands| g.load_branches());
        spawn_part!(tx, self, Commits, |g: &GitCommands| g.load_commits(0));
        spawn_part!(tx, self, Stash, |g: &GitCommands| g.load_stash());
        spawn_part!(tx, self, Remotes, |g: &GitCommands| g.load_remotes());
        spawn_part!(tx, self, Tags, |g: &GitCommands| g.load_tags());
        spawn_part!(tx, self, Worktrees, |g: &GitCommands| g
            .load_worktrees()
            .or_else(|_| Ok::<_, anyhow::Error>(Vec::new())));
        spawn_part!(tx, self, Submodules, |g: &GitCommands| g
            .load_submodules()
            .or_else(|_| Ok::<_, anyhow::Error>(Vec::new())));
        spawn_part!(tx, self, Reflog, |g: &GitCommands| g
            .load_reflog(100)
            .or_else(|_| Ok::<_, anyhow::Error>(Vec::new())));

        // DiffStats and RepoStatus have different shapes, spawn them directly.
        {
            let tx = tx.clone();
            let git = Arc::clone(self);
            std::thread::spawn(move || {
                if let Ok((added, deleted)) = git.diff_shortstat() {
                    let _ = tx.send(ModelPart::DiffStats { added, deleted });
                }
            });
        }
        {
            let tx = tx.clone();
            let git = Arc::clone(self);
            std::thread::spawn(move || {
                if let Ok(status) = git.repo_status() {
                    let _ = tx.send(ModelPart::RepoStatus {
                        is_rebasing: status.is_rebasing,
                        is_merging: status.is_merging,
                        is_cherry_picking: status.is_cherry_picking,
                        is_bisecting: status.is_bisecting,
                        rebase_onto_hash: status.rebase_onto_hash,
                    });
                }
            });
        }
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

    /// Resolve a ref (branch name, tag, hash) to a full commit hash.
    pub fn resolve_ref(&self, refspec: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["rev-parse", refspec])
            .run_expecting_success()?;
        Ok(result.stdout_trimmed().to_string())
    }

    /// Get the subject line of a commit.
    pub fn commit_subject(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["log", "-1", "--format=%s", hash])
            .run_expecting_success()?;
        Ok(result.stdout_trimmed().to_string())
    }
}
