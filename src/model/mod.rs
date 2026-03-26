pub mod author;
pub mod branch;
pub mod commit;
pub mod file;
pub mod file_tree;
pub mod remote;
pub mod stash;
pub mod tag;
pub mod worktree;

pub use author::Author;
pub use branch::Branch;
pub use commit::{Commit, CommitStatus};
pub use file::{File, FileStatus};
pub use remote::{Remote, RemoteBranch};
pub use stash::StashEntry;
pub use tag::Tag;
pub use worktree::Worktree;

use std::collections::HashMap;

/// Holds all repository data loaded from git.
#[derive(Debug, Default)]
pub struct Model {
    pub repo_name: String,
    pub head_hash: String,
    /// Current branch name, fetched cheaply via `git branch --show-current`.
    /// Empty string when HEAD is detached.
    pub head_branch_name: String,
    pub files: Vec<File>,
    pub branches: Vec<Branch>,
    pub commits: Vec<Commit>,
    pub stash_entries: Vec<StashEntry>,
    pub remotes: Vec<Remote>,
    pub tags: Vec<Tag>,
    pub worktrees: Vec<Worktree>,
    pub reflog_commits: Vec<Commit>,
    pub sub_commits: Vec<Commit>,
    pub sub_remote_branches: Vec<RemoteBranch>,
    pub commit_files: Vec<CommitFile>,
    pub authors: HashMap<String, Author>,
    // Total line changes
    pub total_additions: usize,
    pub total_deletions: usize,
    // In-progress operation state
    pub is_rebasing: bool,
    pub is_merging: bool,
    pub is_cherry_picking: bool,
    pub is_bisecting: bool,
}

#[derive(Debug, Clone)]
pub struct CommitFile {
    pub name: String,
    pub status: FileChangeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Unmerged,
}
