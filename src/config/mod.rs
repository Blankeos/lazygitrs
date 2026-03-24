pub mod app_state;
pub mod keybindings;
pub mod theme;
pub mod user_config;

use std::path::PathBuf;

use anyhow::Result;

pub use app_state::AppState;
pub use keybindings::KeybindingConfig;
pub use theme::Theme;
pub use user_config::UserConfig;

/// Top-level application configuration.
pub struct AppConfig {
    pub debug: bool,
    pub version: String,
    pub user_config: UserConfig,
    pub app_state: AppState,
    pub config_dir: PathBuf,
    pub state_path: PathBuf,
}

impl AppConfig {
    pub fn load(debug: bool) -> Result<Self> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lazygit");

        let state_path = config_dir.join("state.yml");

        let user_config = UserConfig::load(&config_dir)?;
        let app_state = AppState::load(&state_path)?;

        Ok(Self {
            debug,
            version: env!("CARGO_PKG_VERSION").to_string(),
            user_config,
            app_state,
            config_dir,
            state_path,
        })
    }

    pub fn save_state(&self) -> Result<()> {
        self.app_state.save(&self.state_path)
    }
}
