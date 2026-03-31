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
    #[serde(rename = "showCommandLog", skip_serializing_if = "Option::is_none")]
    pub show_command_log: Option<bool>,
    #[serde(rename = "showFileTree", skip_serializing_if = "Option::is_none")]
    pub show_file_tree: Option<bool>,
    #[serde(rename = "diffLineWrap", skip_serializing_if = "Option::is_none")]
    pub diff_line_wrap: Option<bool>,
    #[serde(rename = "colorTheme", skip_serializing_if = "Option::is_none")]
    pub color_theme: Option<String>,
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
