use std::fmt;

#[derive(Debug, Clone)]
pub struct Commit {
    pub hash: String,
    pub name: String,
    pub status: CommitStatus,
    pub action: String,
    pub tags: Vec<String>,
    pub extra_info: String,
    pub author_name: String,
    pub author_email: String,
    pub unix_timestamp: i64,
    pub parents: Vec<String>,
    pub divergence: Divergence,
}

impl Commit {
    pub fn short_hash(&self) -> &str {
        if self.hash.len() >= 7 {
            &self.hash[..7]
        } else {
            &self.hash
        }
    }

    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommitStatus {
    #[default]
    Pushed,
    Unpushed,
    Merged,
    Rebasing,
    Selected,
    Conflicted,
    Reflog,
}

impl fmt::Display for CommitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pushed => write!(f, "pushed"),
            Self::Unpushed => write!(f, "unpushed"),
            Self::Merged => write!(f, "merged"),
            Self::Rebasing => write!(f, "rebasing"),
            Self::Selected => write!(f, "selected"),
            Self::Conflicted => write!(f, "conflicted"),
            Self::Reflog => write!(f, "reflog"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Divergence {
    #[default]
    None,
    Left,
    Right,
}
