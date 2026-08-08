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
    /// Changes whenever `commits` is replaced or extended. Used to cache the
    /// expensive commit graph layout between terminal frames.
    pub commits_revision: u64,
    pub stash_entries: Vec<StashEntry>,
    pub remotes: Vec<Remote>,
    pub tags: Vec<Tag>,
    pub worktrees: Vec<Worktree>,
    pub submodules: Vec<Submodule>,
    pub reflog_commits: Vec<Commit>,
    pub sub_commits: Vec<Commit>,
    /// Changes whenever `sub_commits` is replaced or cleared.
    pub sub_commits_revision: u64,
    pub sub_remote_branches: Vec<RemoteBranch>,
    pub commit_files: Vec<CommitFile>,
    pub authors: HashMap<String, Author>,
    /// Authors encountered in loaded commit history. Unlike `authors`, this is
    /// retained across filtering so the author palette remains useful.
    pub commit_filter_authors: HashMap<String, Author>,
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
    fn remember_commit_authors(&mut self, commits: &[Commit]) {
        for commit in commits {
            self.commit_filter_authors
                .entry(commit.author_email.clone())
                .or_insert_with(|| Author {
                    name: commit.author_name.clone(),
                    email: commit.author_email.clone(),
                });
        }
    }

    /// Replace `self.files` with `new_files`, but preserve the previous
    /// display order of files that still exist. New files are appended in
    /// the order `git status` returned them. Without this, staging a file
    /// would let `git status --porcelain` reshuffle the list (e.g. a newly
    /// staged file jumps to the top).
    /// Wholesale-replace the model with a freshly-loaded one, but keep the
    /// previous file display order via `set_files`.
    pub fn replace_keeping_file_order(&mut self, mut new_model: Model) {
        let commits_revision = self.commits_revision.wrapping_add(1);
        let sub_commits_revision = self.sub_commits_revision.wrapping_add(1);
        let prev_files = std::mem::take(&mut self.files);
        let previous_commit_authors = std::mem::take(&mut self.commit_filter_authors);
        let new_files = std::mem::take(&mut new_model.files);
        new_model
            .commit_filter_authors
            .extend(previous_commit_authors);
        *self = new_model;
        self.commits_revision = commits_revision;
        self.sub_commits_revision = sub_commits_revision;
        self.files = prev_files;
        self.set_files(new_files);
    }

    pub fn set_commits(&mut self, commits: Vec<Commit>) {
        self.remember_commit_authors(&commits);
        self.commits = commits;
        self.commits_revision = self.commits_revision.wrapping_add(1);
    }

    pub fn extend_commits(&mut self, commits: impl IntoIterator<Item = Commit>) {
        let commits = commits.into_iter().collect::<Vec<_>>();
        self.remember_commit_authors(&commits);
        let previous_len = self.commits.len();
        self.commits.extend(commits);
        if self.commits.len() != previous_len {
            self.commits_revision = self.commits_revision.wrapping_add(1);
        }
    }

    pub fn set_sub_commits(&mut self, commits: Vec<Commit>) {
        self.remember_commit_authors(&commits);
        self.sub_commits = commits;
        self.sub_commits_revision = self.sub_commits_revision.wrapping_add(1);
    }

    pub fn clear_sub_commits(&mut self) {
        if !self.sub_commits.is_empty() {
            self.sub_commits.clear();
            self.sub_commits_revision = self.sub_commits_revision.wrapping_add(1);
        }
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
            if let Some(mut f) = by_name.remove(&prev.name) {
                // Light status-only refreshes leave stats at 0; keep prior
                // numstat/hunk counts so the tree doesn't flicker until a full
                // refresh repopulates them.
                if f.additions == 0 && f.deletions == 0 && f.hunk_count == 0 {
                    f.additions = prev.additions;
                    f.deletions = prev.deletions;
                    f.hunk_count = prev.hunk_count;
                }
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
    pub hunk_count: usize,
    pub additions: usize,
    pub deletions: usize,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::commit::{CommitStatus, Divergence};

    fn commit(hash: &str) -> Commit {
        Commit {
            hash: hash.to_string(),
            name: hash.to_string(),
            status: CommitStatus::Pushed,
            action: String::new(),
            tags: Vec::new(),
            refs: Vec::new(),
            extra_info: String::new(),
            author_name: String::new(),
            author_email: String::new(),
            unix_timestamp: 0,
            parents: Vec::new(),
            divergence: Divergence::None,
        }
    }

    #[test]
    fn commit_mutators_advance_revisions() {
        let mut model = Model::default();

        model.set_commits(vec![commit("one")]);
        assert_eq!(model.commits_revision, 1);
        model.extend_commits([commit("two")]);
        assert_eq!(model.commits_revision, 2);
        model.extend_commits(Vec::new());
        assert_eq!(model.commits_revision, 2);

        model.set_sub_commits(vec![commit("sub")]);
        assert_eq!(model.sub_commits_revision, 1);
        model.clear_sub_commits();
        assert_eq!(model.sub_commits_revision, 2);
        model.clear_sub_commits();
        assert_eq!(model.sub_commits_revision, 2);
    }
}
