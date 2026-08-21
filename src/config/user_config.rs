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
    pub refresher: RefresherConfig,
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
            refresher: RefresherConfig::default(),
            keybinding: KeybindingConfig::default(),
            os: OsConfig::default(),
            custom_commands: Vec::new(),
        }
    }
}

/// Mirrors lazygit's `refresher` config block. `refreshInterval` is the
/// files/submodules auto-refresh cadence; `fetchInterval` is the periodic
/// background `git fetch` cadence. Both are in seconds; 0 disables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RefresherConfig {
    #[serde(rename = "refreshInterval")]
    pub refresh_interval: u64,
    #[serde(rename = "fetchInterval")]
    pub fetch_interval: u64,
}

impl Default for RefresherConfig {
    fn default() -> Self {
        Self {
            refresh_interval: 10,
            fetch_interval: 60,
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
    /// Named editor preset (`helix`, `nvim`, `vim`, `vscode`, `zed`, …).
    /// Fills empty edit templates; ignored when templates are set explicitly.
    #[serde(rename = "editPreset")]
    pub edit_preset: String,
    /// Command template to open a file in the user's editor.
    /// Uses `{{filename}}` as placeholder. e.g. `"hx {{filename}}"`
    pub edit: String,
    /// Command template to open a file at a specific line.
    /// Uses `{{filename}}`, `{{line}}`, and optionally `{{column}}`.
    #[serde(rename = "editAtLine")]
    pub edit_at_line: String,
    /// Command template to open a file at a specific line and wait for close.
    #[serde(rename = "editAtLineAndWait")]
    pub edit_at_line_and_wait: String,
    /// Command template to open a file/URL in the default program.
    /// Uses `{{filename}}` as placeholder.
    pub open: String,
    /// Command template to open a directory in the editor.
    #[serde(rename = "openDirInEditor")]
    pub open_dir_in_editor: String,
    /// Command to copy text to clipboard (text is piped via stdin).
    #[serde(rename = "copyToClipboardCmd")]
    pub copy_to_clipboard_cmd: String,
    /// When true, suspend the TUI and wait for the editor (terminal editors).
    /// When false, spawn and return immediately (GUI editors).
    /// When unset, inferred from `editPreset` / `$EDITOR`.
    #[serde(rename = "editInTerminal")]
    pub edit_in_terminal: Option<bool>,
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
            edit_preset: String::new(),
            edit: String::new(),
            edit_at_line: String::new(),
            edit_at_line_and_wait: String::new(),
            open: open_cmd.to_string(),
            open_dir_in_editor: String::new(),
            copy_to_clipboard_cmd: copy_cmd.to_string(),
            edit_in_terminal: None,
        }
    }
}

/// How an edit/open should be launched from the TUI.
#[derive(Debug, Clone)]
pub struct EditorLaunch {
    pub program: String,
    pub args: Vec<String>,
    /// Leave the alternate screen and wait for the process (hx/nvim/vim).
    pub suspend: bool,
}

impl EditorLaunch {
    pub fn display_cmd(&self) -> String {
        let mut parts = Vec::with_capacity(1 + self.args.len());
        parts.push(self.program.as_str());
        parts.extend(self.args.iter().map(String::as_str));
        parts.join(" ")
    }
}

struct EditPreset {
    edit: &'static str,
    edit_at_line: &'static str,
    edit_at_line_and_wait: &'static str,
    open_dir_in_editor: &'static str,
    suspend: bool,
}

fn preset_for(name: &str) -> Option<EditPreset> {
    // Match Go lazygit's editor_presets.go (subset we care about).
    match name {
        "vim" | "nvim" | "vi" => Some(EditPreset {
            edit: "{editor} -- {{filename}}",
            edit_at_line: "{editor} +{{line}} -- {{filename}}",
            edit_at_line_and_wait: "{editor} +{{line}} -- {{filename}}",
            open_dir_in_editor: "{editor} -- {{dir}}",
            suspend: true,
        }),
        // Homebrew/cargo install the binary as `hx`. Go lazygit also ships a
        // separate "helix (hx)" preset; we always use `hx` for both names.
        "helix" | "hx" => Some(EditPreset {
            edit: "hx -- {{filename}}",
            edit_at_line: "hx -- {{filename}}:{{line}}:{{column}}",
            edit_at_line_and_wait: "hx -- {{filename}}:{{line}}:{{column}}",
            open_dir_in_editor: "hx -- {{dir}}",
            suspend: true,
        }),
        "nano" => Some(EditPreset {
            edit: "{editor} -- {{filename}}",
            edit_at_line: "{editor} +{{line}} -- {{filename}}",
            edit_at_line_and_wait: "{editor} +{{line}} -- {{filename}}",
            open_dir_in_editor: "{editor} -- {{dir}}",
            suspend: true,
        }),
        "emacs" => Some(EditPreset {
            edit: "emacsclient --alternate-editor=emacs --no-wait -- {{filename}}",
            edit_at_line: "emacsclient --alternate-editor=emacs --no-wait +{{line}} -- {{filename}}",
            edit_at_line_and_wait: "{editor} +{{line}} -- {{filename}}",
            open_dir_in_editor: "emacsclient --alternate-editor=emacs --no-wait -- {{dir}}",
            suspend: false,
        }),
        "vscode" | "code" => Some(EditPreset {
            edit: "code --reuse-window -- {{filename}}",
            edit_at_line: "code --reuse-window --goto -- {{filename}}:{{line}}:{{column}}",
            edit_at_line_and_wait: "code --reuse-window --goto --wait -- {{filename}}:{{line}}:{{column}}",
            open_dir_in_editor: "code -- {{dir}}",
            suspend: false,
        }),
        "zed" | "zed.exe" => Some(EditPreset {
            edit: "zed -- {{filename}}",
            edit_at_line: "zed -- {{filename}}:{{line}}:{{column}}",
            edit_at_line_and_wait: "zed --wait -- {{filename}}:{{line}}:{{column}}",
            open_dir_in_editor: "zed -- {{dir}}",
            suspend: false,
        }),
        _ => None,
    }
}

fn guess_editor_name() -> String {
    for key in ["VISUAL", "EDITOR"] {
        if let Ok(val) = std::env::var(key) {
            let trimmed = val.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Take the first token (ignore flags like `nvim -p`).
            let first = trimmed.split_whitespace().next().unwrap_or(trimmed);
            let base = std::path::Path::new(first)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(first);
            return base.to_string();
        }
    }
    String::new()
}

fn resolve_preset(os: &OsConfig) -> (EditPreset, String) {
    let guessed = if os.edit_preset.is_empty() {
        guess_editor_name()
    } else {
        os.edit_preset.clone()
    };
    let editor_bin = if guessed.is_empty() {
        "vim".to_string()
    } else if guessed == "helix" {
        // `editPreset: helix` must not exec a non-existent `helix` binary.
        "hx".to_string()
    } else {
        guessed.clone()
    };
    let preset_name = if os.edit_preset.is_empty() {
        editor_bin.as_str()
    } else {
        os.edit_preset.as_str()
    };
    let preset = preset_for(preset_name).unwrap_or(EditPreset {
        edit: "{editor} -- {{filename}}",
        edit_at_line: "{editor} +{{line}} -- {{filename}}",
        edit_at_line_and_wait: "{editor} +{{line}} -- {{filename}}",
        open_dir_in_editor: "{editor} -- {{dir}}",
        suspend: true,
    });
    (preset, editor_bin)
}

fn apply_editor(template: &str, editor: &str) -> String {
    template.replace("{editor}", editor)
}

fn expand_placeholders(
    template: &str,
    filename: &str,
    line: Option<usize>,
    column: Option<usize>,
) -> String {
    let mut s = template
        .replace("{{filename}}", filename)
        .replace("{{dir}}", filename);
    if let Some(ln) = line {
        s = s.replace("{{line}}", &ln.to_string());
    }
    if let Some(col) = column {
        s = s.replace("{{column}}", &col.to_string());
    } else {
        s = s.replace("{{column}}", "1");
    }
    s
}

/// Split a command string into argv. Supports simple double-quoted args.
fn split_command(cmd: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in cmd.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    parts.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

impl OsConfig {
    fn suspend_for_edit(&self, preset_suspend: bool) -> bool {
        self.edit_in_terminal.unwrap_or(preset_suspend)
    }

    fn edit_template(&self) -> (String, bool) {
        let (preset, editor) = resolve_preset(self);
        if !self.edit.is_empty() {
            return (self.edit.clone(), self.suspend_for_edit(preset.suspend));
        }
        (
            apply_editor(preset.edit, &editor),
            self.suspend_for_edit(preset.suspend),
        )
    }

    fn edit_at_line_template(&self) -> (String, bool) {
        let (preset, editor) = resolve_preset(self);
        if !self.edit_at_line.is_empty() {
            return (
                self.edit_at_line.clone(),
                self.suspend_for_edit(preset.suspend),
            );
        }
        (
            apply_editor(preset.edit_at_line, &editor),
            self.suspend_for_edit(preset.suspend),
        )
    }

    fn open_dir_template(&self) -> (String, bool) {
        let (preset, editor) = resolve_preset(self);
        if !self.open_dir_in_editor.is_empty() {
            return (
                self.open_dir_in_editor.clone(),
                self.suspend_for_edit(preset.suspend),
            );
        }
        (
            apply_editor(preset.open_dir_in_editor, &editor),
            self.suspend_for_edit(preset.suspend),
        )
    }

    /// Build a launch plan for editing `filename` (optionally at line/column).
    pub fn plan_edit(
        &self,
        filename: &str,
        line: Option<usize>,
        column: Option<usize>,
    ) -> anyhow::Result<EditorLaunch> {
        let (template, suspend) = if line.is_some() {
            self.edit_at_line_template()
        } else {
            self.edit_template()
        };
        Self::plan_from_template(&template, filename, line, column, suspend)
    }

    /// Build a launch plan for opening a directory in the editor.
    pub fn plan_open_dir(&self, dir: &str) -> anyhow::Result<EditorLaunch> {
        let (template, suspend) = self.open_dir_template();
        Self::plan_from_template(&template, dir, None, None, suspend)
    }

    /// Build a launch plan for `os.open` (may be `hx`/`nvim` or a GUI opener).
    pub fn plan_open(&self, filename: &str) -> anyhow::Result<EditorLaunch> {
        if self.open.is_empty() {
            anyhow::bail!("No open command configured");
        }
        let template = self.open.clone();
        // Heuristic: if the open command looks like a terminal editor, suspend.
        let first = template.split_whitespace().next().unwrap_or("");
        let base = std::path::Path::new(first)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(first);
        let looks_terminal = matches!(
            base,
            "hx" | "helix" | "nvim" | "vim" | "vi" | "nano" | "emacs" | "kak" | "kakoune"
        );
        let suspend = self.edit_in_terminal.unwrap_or(looks_terminal);
        Self::plan_from_template(&template, filename, None, None, suspend)
    }

    fn plan_from_template(
        template: &str,
        filename: &str,
        line: Option<usize>,
        column: Option<usize>,
        suspend: bool,
    ) -> anyhow::Result<EditorLaunch> {
        if template.is_empty() {
            anyhow::bail!("No command configured");
        }
        let cmd_str = expand_placeholders(template, filename, line, column);
        let parts = split_command(&cmd_str);
        if parts.is_empty() {
            anyhow::bail!("Empty command after template expansion");
        }
        Ok(EditorLaunch {
            program: parts[0].clone(),
            args: parts[1..].to_vec(),
            suspend,
        })
    }

    /// Run a command template, replacing `{{filename}}` with the given path.
    /// Spawns without waiting (GUI editors / `open`). Prefer [`plan_edit`] for editors.
    pub fn run_template(template: &str, filename: &str) -> anyhow::Result<()> {
        if template.is_empty() {
            anyhow::bail!("No command configured");
        }
        let cmd_str = expand_placeholders(template, filename, None, None);
        let parts = split_command(&cmd_str);
        if parts.is_empty() {
            anyhow::bail!("Empty command after template expansion");
        }
        crate::os::cmd::log_command(&cmd_str);
        std::process::Command::new(&parts[0])
            .args(&parts[1..])
            .spawn()?;
        Ok(())
    }

    /// Run a command template replacing `{{filename}}`, `{{line}}`, and `{{column}}`.
    pub fn run_template_at_line(
        template: &str,
        filename: &str,
        line: usize,
        column: usize,
    ) -> anyhow::Result<()> {
        if template.is_empty() {
            anyhow::bail!("No command configured");
        }
        let cmd_str = expand_placeholders(template, filename, Some(line), Some(column));
        let parts = split_command(&cmd_str);
        if parts.is_empty() {
            anyhow::bail!("Empty command after template expansion");
        }
        crate::os::cmd::log_command(&cmd_str);
        std::process::Command::new(&parts[0])
            .args(&parts[1..])
            .spawn()?;
        Ok(())
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
