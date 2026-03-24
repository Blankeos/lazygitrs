#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: String,
    pub branch: String,
    pub hash: String,
    pub is_current: bool,
    pub is_main: bool,
}
