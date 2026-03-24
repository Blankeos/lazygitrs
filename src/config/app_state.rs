use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppState {
    #[serde(rename = "recentRepos")]
    pub recent_repos: Vec<String>,
    #[serde(rename = "startupPopupVersion")]
    pub startup_popup_version: u32,
}

impl AppState {
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            let state: AppState = serde_yaml::from_str(&contents)?;
            Ok(state)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_yaml::to_string(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    pub fn add_recent_repo(&mut self, path: &str) {
        self.recent_repos.retain(|r| r != path);
        self.recent_repos.insert(0, path.to_string());
        self.recent_repos.truncate(20);
    }
}
