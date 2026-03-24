#[derive(Debug, Clone)]
pub struct StashEntry {
    pub index: usize,
    pub name: String,
    pub hash: String,
}

impl StashEntry {
    pub fn ref_name(&self) -> String {
        format!("stash@{{{}}}", self.index)
    }
}
