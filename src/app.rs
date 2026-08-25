use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::AppConfig;
use crate::git::GitCommands;
use crate::gui::Gui;

pub struct App {
    pub config: AppConfig,
    pub repo_path: PathBuf,
    pub filter_path: Option<PathBuf>,
}

impl App {
    pub fn new(repo_path: PathBuf, debug: bool, filter_path: Option<PathBuf>) -> Result<Self> {
        let config = AppConfig::load(debug)?;

        // Validate git repo
        if !GitCommands::is_valid_repo(&repo_path) {
            anyhow::bail!("'{}' is not a git repository", repo_path.display());
        }

        Ok(Self {
            config,
            repo_path,
            filter_path,
        })
    }

    pub fn run(mut self) -> Result<()> {
        let git = GitCommands::new(&self.repo_path).context("Failed to initialize git commands")?;

        // Update recent repos
        let repo_str = git.repo_path().to_string_lossy().to_string();
        self.config.app_state.add_recent_repo(&repo_str);
        let _ = self.config.save_state();

        let mut gui = Gui::new(self.config, git)?;
        if let Some(path) = self.filter_path {
            gui.apply_startup_path_filter(path);
        }
        gui.run()?;

        Ok(())
    }
}
