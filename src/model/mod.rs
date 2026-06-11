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

use crate::git::submodule::Submodule;

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
    pub submodules: Vec<Submodule>,
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
    /// Short hash of the commit being rebased onto (from .git/rebase-merge/onto).
    pub rebase_onto_hash: String,
    /// HTTPS URL of the origin remote (empty if no origin or unset).
    pub repo_url: String,
    /// Top contributors as (name, commit_count), descending. Capped traversal.
    pub contributors: Vec<(String, usize)>,
}

impl Model {
    /// Replace `self.files` with `new_files`, but preserve the previous
    /// display order of files that still exist. New files are appended in
    /// the order `git status` returned them. Without this, staging a file
    /// would let `git status --porcelain` reshuffle the list (e.g. a newly
    /// staged file jumps to the top).
    /// Wholesale-replace the model with a freshly-loaded one, but keep the
    /// previous file display order via `set_files`.
    pub fn replace_keeping_file_order(&mut self, mut new_model: Model) {
        let prev_files = std::mem::take(&mut self.files);
        let new_files = std::mem::take(&mut new_model.files);
        *self = new_model;
        self.files = prev_files;
        self.set_files(new_files);
    }

    pub fn set_files(&mut self, new_files: Vec<File>) {
        if self.files.is_empty() {
            self.files = new_files;
            return;
        }
        use std::collections::HashMap;
        let mut by_name: HashMap<String, File> =
            new_files.into_iter().map(|f| (f.name.clone(), f)).collect();

        let mut out = Vec::with_capacity(by_name.len() + self.files.len());
        for prev in &self.files {
            if let Some(f) = by_name.remove(&prev.name) {
                out.push(f);
            }
        }
        // Append leftovers (truly new files). HashMap iteration is unstable,
        // so sort by name for deterministic placement.
        let mut leftovers: Vec<File> = by_name.into_values().collect();
        leftovers.sort_by(|a, b| a.name.cmp(&b.name));
        out.extend(leftovers);

        self.files = out;
    }
}

#[derive(Debug, Clone)]
pub struct CommitFile {
    pub name: String,
    pub status: FileChangeStatus,
}

impl CommitFile {
    pub fn rename_paths(&self) -> Option<(&str, &str)> {
        self.name.split_once(" -> ")
    }

    pub fn current_path(&self) -> &str {
        match self.rename_paths() {
            Some((_, new)) => new,
            None => &self.name,
        }
    }
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
