pub mod app_state;
pub mod keybindings;
pub mod theme;
pub mod user_config;

use std::path::PathBuf;

use anyhow::Result;

pub use app_state::AppState;
pub use keybindings::KeybindingConfig;
pub use theme::{Theme, ColorTheme, COLOR_THEMES};
pub use user_config::UserConfig;

pub fn config_dir_candidates() -> Vec<PathBuf> {
    let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir.join(".config"));
    vec![base.join("lazygitrs"), base.join("lazygit")]
}

/// Top-level application configuration.
pub struct AppConfig {
    pub debug: bool,
    pub version: String,
    pub user_config: UserConfig,
    pub app_state: AppState,
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub state_path: PathBuf,
}

impl AppConfig {
    pub fn load(debug: bool) -> Result<Self> {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let candidates = config_dir_candidates();
        let config_dir = candidates
            .into_iter()
            .find(|dir| dir.join("config.yml").exists())
            .unwrap_or_else(|| {
                std::env::var("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| home_dir.join(".config"))
                    .join("lazygitrs")
            });

        let state_dir = std::env::var("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home_dir.join(".local").join("state"))
            .join("lazygitrs");

        let state_path = state_dir.join("state.yml");

        let user_config = UserConfig::load(&config_dir)?;
        let app_state = AppState::load(&state_path)?;

        Ok(Self {
            debug,
            version: env!("CARGO_PKG_VERSION").to_string(),
            user_config,
            app_state,
            config_dir,
            state_dir,
            state_path,
        })
    }

    pub fn save_state(&self) -> Result<()> {
        self.app_state.save(&self.state_path)
    }
}
