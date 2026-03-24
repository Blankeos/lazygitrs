pub mod author;
pub mod branch;
pub mod commit;
pub mod file;
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
    pub files: Vec<File>,
    pub branches: Vec<Branch>,
    pub commits: Vec<Commit>,
    pub stash_entries: Vec<StashEntry>,
    pub remotes: Vec<Remote>,
    pub tags: Vec<Tag>,
    pub worktrees: Vec<Worktree>,
    pub reflog_commits: Vec<Commit>,
    pub sub_commits: Vec<Commit>,
    pub commit_files: Vec<CommitFile>,
    pub authors: HashMap<String, Author>,
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
