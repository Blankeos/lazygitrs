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
    pub hunk_count: usize,
    pub additions: usize,
    pub deletions: usize,
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
            hunk_count: 0,
            additions: 0,
            deletions: 0,
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

    /// Optimistically apply a stage transition in-memory (lazygit-style).
    /// Returns false when the short status isn't a simple map case.
    pub fn optimistic_stage(&mut self) -> bool {
        let next = match self.short_status.as_str() {
            "??" => "A ",
            " M" => "M ",
            "MM" => "M ",
            " D" => "D ",
            " A" => "A ",
            "AM" => "A ",
            "MD" => "D ",
            _ => return false,
        };
        self.apply_short_status(next);
        true
    }

    /// Optimistically apply an unstage transition in-memory (lazygit-style).
    pub fn optimistic_unstage(&mut self) -> bool {
        let next = match self.short_status.as_str() {
            "A " => "??",
            "M " => " M",
            "D " => " D",
            _ => return false,
        };
        self.apply_short_status(next);
        true
    }

    fn apply_short_status(&mut self, short: &str) {
        let mut chars = short.chars();
        let x = chars.next().unwrap_or(' ');
        let y = chars.next().unwrap_or(' ');
        self.short_status = format!("{}{}", x, y);
        let tracked = !(x == '?' && y == '?');
        let has_staged = x != ' ' && x != '?';
        let has_unstaged = y != ' ' && y != '?';
        // Untracked is both unstaged and untracked.
        let has_unstaged = if !tracked { true } else { has_unstaged };
        self.tracked = tracked;
        self.has_staged_changes = has_staged;
        self.has_unstaged_changes = has_unstaged;
        self.added = x == 'A' || y == 'A' || !tracked;
        self.deleted = x == 'D' || y == 'D';
        self.status = if !tracked {
            FileStatus::Untracked
        } else if x == 'A' || y == 'A' {
            FileStatus::Added
        } else if x == 'D' || y == 'D' {
            FileStatus::Deleted
        } else if x == 'R' {
            FileStatus::Renamed
        } else if x == 'C' {
            FileStatus::Copied
        } else {
            FileStatus::Modified
        };
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
