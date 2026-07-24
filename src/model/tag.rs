#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub hash: String,
    pub message: String,
    /// True when the tag also exists on at least one configured remote.
    pub on_remote: bool,
}
