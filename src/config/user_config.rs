use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::keybindings::KeybindingConfig;
use super::theme::Theme;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserConfig {
    pub gui: GuiConfig,
    pub git: GitConfig,
    pub keybinding: KeybindingConfig,
    pub os: OsConfig,
    #[serde(rename = "customCommands")]
    pub custom_commands: Vec<CustomCommand>,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            gui: GuiConfig::default(),
            git: GitConfig::default(),
            keybinding: KeybindingConfig::default(),
            os: OsConfig::default(),
            custom_commands: Vec::new(),
        }
    }
}

impl UserConfig {
    pub fn load(config_dir: &Path) -> Result<Self> {
        let config_path = config_dir.join("config.yml");
        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            let config: UserConfig = serde_yaml::from_str(&contents)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    pub fn theme(&self) -> Theme {
        Theme::from_config(&self.gui.theme)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GuiConfig {
    #[serde(rename = "scrollHeight")]
    pub scroll_height: usize,
    #[serde(rename = "scrollPastBottom")]
    pub scroll_past_bottom: bool,
    #[serde(rename = "mouseEvents")]
    pub mouse_events: bool,
    #[serde(rename = "skipDiscardChangeWarning")]
    pub skip_discard_change_warning: bool,
    #[serde(rename = "sidePanelWidth")]
    pub side_panel_width: f64,
    pub theme: ThemeConfig,
    #[serde(rename = "showFileTree")]
    pub show_file_tree: bool,
    #[serde(rename = "showCommandLog")]
    pub show_command_log: bool,
    #[serde(rename = "showBottomLine")]
    pub show_bottom_line: bool,
    #[serde(rename = "nerdFontsVersion")]
    pub nerd_fonts_version: String,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            scroll_height: 2,
            scroll_past_bottom: true,
            mouse_events: true,
            skip_discard_change_warning: false,
            side_panel_width: 0.3333,
            theme: ThemeConfig::default(),
            show_file_tree: true,
            show_command_log: true,
            show_bottom_line: true,
            nerd_fonts_version: "3".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    #[serde(rename = "activeBorderColor")]
    pub active_border_color: Vec<String>,
    #[serde(rename = "inactiveBorderColor")]
    pub inactive_border_color: Vec<String>,
    #[serde(rename = "selectedLineBgColor")]
    pub selected_line_bg_color: Vec<String>,
    #[serde(rename = "optionsTextColor")]
    pub options_text_color: Vec<String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            active_border_color: vec!["green".to_string()],
            inactive_border_color: vec!["default".to_string()],
            selected_line_bg_color: vec!["blue".to_string()],
            options_text_color: vec!["blue".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GitConfig {
    #[serde(rename = "autoFetch")]
    pub auto_fetch: bool,
    #[serde(rename = "autoRefresh")]
    pub auto_refresh: bool,
    #[serde(rename = "branchLogCmd")]
    pub branch_log_cmd: String,
    pub paging: PagingConfig,
    pub commit: CommitConfig,
    pub merging: MergingConfig,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            auto_fetch: true,
            auto_refresh: true,
            branch_log_cmd: "git log --graph --color=always --abbrev-commit --decorate --date=relative --pretty=medium {{branchName}} --".to_string(),
            paging: PagingConfig::default(),
            commit: CommitConfig::default(),
            merging: MergingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PagingConfig {
    #[serde(rename = "useConfig")]
    pub use_config: bool,
}

impl Default for PagingConfig {
    fn default() -> Self {
        Self { use_config: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommitConfig {
    #[serde(rename = "signOff")]
    pub sign_off: bool,
    #[serde(rename = "autoWrapCommitMessage")]
    pub auto_wrap_commit_message: bool,
    #[serde(rename = "autoWrapWidth")]
    pub auto_wrap_width: usize,
    #[serde(rename = "generateCommand")]
    pub generate_command: String,
}

impl Default for CommitConfig {
    fn default() -> Self {
        Self {
            sign_off: false,
            auto_wrap_commit_message: true,
            auto_wrap_width: 72,
            generate_command: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MergingConfig {
    #[serde(rename = "manualCommit")]
    pub manual_commit: bool,
    pub args: String,
}

impl Default for MergingConfig {
    fn default() -> Self {
        Self {
            manual_commit: false,
            args: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OsConfig {
    #[serde(rename = "editCommand")]
    pub edit_command: String,
    #[serde(rename = "editCommandTemplate")]
    pub edit_command_template: String,
    #[serde(rename = "openCommand")]
    pub open_command: String,
    #[serde(rename = "copyToClipboardCmd")]
    pub copy_to_clipboard_cmd: String,
}

impl Default for OsConfig {
    fn default() -> Self {
        let (open_cmd, copy_cmd) = if cfg!(target_os = "macos") {
            ("open {{filename}}", "pbcopy")
        } else if cfg!(target_os = "windows") {
            ("start \"\" {{filename}}", "clip")
        } else {
            ("xdg-open {{filename}}", "xclip -selection clipboard")
        };

        Self {
            edit_command: String::new(),
            edit_command_template: "{{editor}} {{filename}}".to_string(),
            open_command: open_cmd.to_string(),
            copy_to_clipboard_cmd: copy_cmd.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomCommand {
    pub key: String,
    pub context: String,
    pub command: String,
    pub description: String,
    #[serde(default)]
    pub stream: bool,
    #[serde(rename = "showOutput")]
    #[serde(default)]
    pub show_output: bool,
    #[serde(default)]
    pub prompts: Vec<CustomCommandPrompt>,
}

impl Default for CustomCommand {
    fn default() -> Self {
        Self {
            key: String::new(),
            context: String::new(),
            command: String::new(),
            description: String::new(),
            stream: false,
            show_output: false,
            prompts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCommandPrompt {
    #[serde(rename = "type")]
    pub prompt_type: Option<String>,
    pub title: Option<String>,
    pub key: Option<String>,
    pub command: Option<String>,
    pub filter: Option<String>,
}
