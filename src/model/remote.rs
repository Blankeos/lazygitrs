#[derive(Debug, Clone)]
pub struct Remote {
    pub name: String,
    pub urls: Vec<String>,
    pub branches: Vec<RemoteBranch>,
}

#[derive(Debug, Clone)]
pub struct RemoteBranch {
    pub name: String,
    pub remote_name: String,
    pub hash: String,
}

impl RemoteBranch {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.remote_name, self.name)
    }
}
