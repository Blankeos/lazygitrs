use std::fmt;

#[derive(Debug, Clone)]
pub struct File {
    pub name: String,
    pub display_name: String,
    pub status: FileStatus,
    pub has_staged_changes: bool,
    pub has_unstaged_changes: bool,
    pub tracked: bool,
    pub added: bool,
    pub deleted: bool,
    pub has_merge_conflicts: bool,
    pub short_status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renamed_file() -> File {
        File {
            name: "src/views/openai_oauth_flow.rs -> src/views/provider_oauth_flow.rs".to_string(),
            display_name: "src/views/openai_oauth_flow.rs -> src/views/provider_oauth_flow.rs"
                .to_string(),
            status: FileStatus::Renamed,
            has_staged_changes: true,
            has_unstaged_changes: false,
            tracked: true,
            added: false,
            deleted: false,
            has_merge_conflicts: false,
            short_status: "R ".to_string(),
        }
    }

    #[test]
    fn rename_helpers_split_display_name_from_git_paths() {
        let file = renamed_file();

        assert_eq!(
            file.rename_paths(),
            Some((
                "src/views/openai_oauth_flow.rs",
                "src/views/provider_oauth_flow.rs"
            ))
        );
        assert_eq!(file.current_path(), "src/views/provider_oauth_flow.rs");
        assert_eq!(file.git_add_path(), "src/views/provider_oauth_flow.rs");
        assert_eq!(
            file.diff_paths(),
            vec![
                "src/views/openai_oauth_flow.rs",
                "src/views/provider_oauth_flow.rs"
            ]
        );
    }
}

impl File {
    pub fn is_tracked(&self) -> bool {
        self.tracked
    }

    pub fn has_any_changes(&self) -> bool {
        self.has_staged_changes || self.has_unstaged_changes
    }

    /// For renamed files, `name` is stored as "old -> new". This returns
    /// both halves so callers can pass them to git as separate pathspecs.
    pub fn rename_paths(&self) -> Option<(&str, &str)> {
        self.name.split_once(" -> ")
    }

    /// The path that exists in the working tree / index for this change.
    ///
    /// Renames are represented in `name` as `old -> new`, but most filesystem
    /// operations and tree grouping should use only the new path.
    pub fn current_path(&self) -> &str {
        match self.rename_paths() {
            Some((_, new)) => new,
            None => &self.name,
        }
    }

    /// Git pathspecs that identify this file's diff. For staged renames, both
    /// old and new paths are required; passing the synthetic `old -> new` label
    /// matches nothing, and passing only one side makes Git show an add/delete.
    pub fn diff_paths(&self) -> Vec<&str> {
        match self.rename_paths() {
            Some((old, new)) => vec![old, new],
            None => vec![&self.name],
        }
    }

    /// Pathspec to pass to `git add` for this file. For renames, this is
    /// the post-rename path (the only one that exists on disk).
    pub fn git_add_path(&self) -> &str {
        self.current_path()
    }

    /// Pathspecs to pass to `git reset HEAD --` to fully unstage this file.
    /// For renames, both old and new paths are needed — resetting only one
    /// leaves the other half (e.g. the deletion of the old path) staged.
    pub fn git_reset_paths(&self) -> Vec<&str> {
        match self.rename_paths() {
            Some((old, new)) => vec![old, new],
            None => vec![&self.name],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Untracked,
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Unmerged,
    Both,
}

impl fmt::Display for FileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Untracked => write!(f, "?"),
            Self::Added => write!(f, "A"),
            Self::Modified => write!(f, "M"),
            Self::Deleted => write!(f, "D"),
            Self::Renamed => write!(f, "R"),
            Self::Copied => write!(f, "C"),
            Self::Unmerged => write!(f, "U"),
            Self::Both => write!(f, "B"),
        }
    }
}
