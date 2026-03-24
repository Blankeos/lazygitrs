use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::AppConfig;
use crate::git::GitCommands;
use crate::gui::Gui;

pub struct App {
    pub config: AppConfig,
    pub repo_path: PathBuf,
}

impl App {
    pub fn new(repo_path: PathBuf, debug: bool) -> Result<Self> {
        let config = AppConfig::load(debug)?;

        // Validate git repo
        if !GitCommands::is_valid_repo(&repo_path) {
            anyhow::bail!(
                "'{}' is not a git repository",
                repo_path.display()
            );
        }

        Ok(Self { config, repo_path })
    }

    pub fn run(mut self) -> Result<()> {
        // Update recent repos
        let repo_str = self.repo_path.to_string_lossy().to_string();
        self.config.app_state.add_recent_repo(&repo_str);
        let _ = self.config.save_state();

        let git = GitCommands::new(&self.repo_path)
            .context("Failed to initialize git commands")?;

        let mut gui = Gui::new(self.config, git)?;
        gui.run()?;

        Ok(())
    }
}
