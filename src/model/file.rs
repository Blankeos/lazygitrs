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

impl File {
    pub fn is_tracked(&self) -> bool {
        self.tracked
    }

    pub fn has_any_changes(&self) -> bool {
        self.has_staged_changes || self.has_unstaged_changes
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
