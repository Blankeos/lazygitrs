#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
    pub hash: String,
    pub recency: String,
    pub pushables: String,
    pub pullables: String,
    pub upstream: Option<String>,
    pub head: bool,
}

impl Branch {
    pub fn display_name(&self) -> &str {
        &self.name
    }

    pub fn is_tracking(&self) -> bool {
        self.upstream.is_some()
    }

    pub fn ahead_behind(&self) -> Option<(usize, usize)> {
        let ahead = self.pushables.parse::<usize>().unwrap_or(0);
        let behind = self.pullables.parse::<usize>().unwrap_or(0);
        if ahead > 0 || behind > 0 {
            Some((ahead, behind))
        } else {
            None
        }
    }
}
