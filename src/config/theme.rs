use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;
use super::user_config::ThemeConfig;

/// A complete color theme for the entire application.
///
/// Every hardcoded color in the UI should reference a field here so themes
/// can be swapped at runtime.
#[derive(Debug, Clone)]
pub struct Theme {
    // ── Borders & chrome ─────────────────────────────────────────────
    pub active_border: Style,
    pub inactive_border: Style,
    pub selected_line: Style,
    pub options_text: Style,
    pub title: Style,

    // ── Diff ─────────────────────────────────────────────────────────
    pub diff_add: Style,
    pub diff_remove: Style,
    pub diff_context: Style,
    pub diff_add_bg: Color,
    pub diff_remove_bg: Color,
    pub diff_add_gutter_bg: Color,
    pub diff_remove_gutter_bg: Color,
    pub diff_add_gutter_fg: Color,
    pub diff_remove_gutter_fg: Color,
    pub diff_add_word: Color,
    pub diff_remove_word: Color,

    // ── Commits ──────────────────────────────────────────────────────
    pub commit_hash: Style,
    pub commit_author: Style,
    pub commit_date: Style,
    pub commit_hash_pushed: Color,
    pub commit_hash_merged: Color,

    // ── Branches ─────────────────────────────────────────────────────
    pub branch_local: Style,
    pub branch_remote: Style,
    pub branch_head: Style,

    // ── Files ────────────────────────────────────────────────────────
    pub file_staged: Style,
    pub file_unstaged: Style,
    pub file_untracked: Style,
    pub file_conflicted: Style,

    // ── Search ───────────────────────────────────────────────────────
    pub search_match: Style,

    // ── Status bar ───────────────────────────────────────────────────
    pub status_bar: Style,
    pub spinner: Style,

    // ── UI chrome colors (popups, dialogs, etc.) ─────────────────────
    /// Primary accent color used for borders, focused elements, section headers.
    pub accent: Color,
    /// Secondary accent color (keybinding highlights, search highlights).
    pub accent_secondary: Color,
    /// Color for dimmed / secondary text.
    pub text_dimmed: Color,
    /// Default text color.
    pub text: Color,
    /// Strong/bright text color.
    pub text_strong: Color,
    /// Color for separator lines.
    pub separator: Color,
    /// Background for selected / highlighted items.
    pub selected_bg: Color,
    /// Background for popup overlays.
    pub popup_border: Color,

    // ── Command log ──────────────────────────────────────────────────
    pub cmd_log_border: Color,
    pub cmd_log_title: Color,
    pub cmd_log_hint: Color,
    pub cmd_log_text: Color,
    pub cmd_log_timestamp: Color,
    pub cmd_log_success: Color,

    // ── Diff panel (side-by-side viewer) ─────────────────────────────
    pub diff_gutter: Color,
    pub diff_line_number: Color,
    pub diff_selection_fg: Color,
    pub diff_selection_bg: Color,
    pub diff_search_highlight_bg: Color,
    pub diff_search_highlight_fg: Color,
    pub diff_search_cursor_bg: Color,
    pub diff_search_cursor_fg: Color,
    pub diff_grid_bg: Color,
    pub diff_grid_fg: Color,

    // ── Syntax highlighting ──────────────────────────────────────────
    pub syntax_comment: Color,
    pub syntax_keyword: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_function: Color,
    pub syntax_function_macro: Color,
    pub syntax_type: Color,
    pub syntax_variable_builtin: Color,
    pub syntax_variable_member: Color,
    pub syntax_module: Color,
    pub syntax_operator: Color,
    pub syntax_tag: Color,
    pub syntax_attribute: Color,
    pub syntax_label: Color,
    pub syntax_punctuation: Color,
    pub syntax_default: Color,

    // ── Graph colors ─────────────────────────────────────────────────
    pub graph_colors: [Color; 8],

    // ── Rebase mode ──────────────────────────────────────────────────
    pub rebase_pick: Color,
    pub rebase_reword: Color,
    pub rebase_edit: Color,
    pub rebase_squash: Color,
    pub rebase_fixup: Color,
    pub rebase_drop: Color,
    pub rebase_paused_bg: Color,

    // ── File change status badges ────────────────────────────────────
    pub change_added: Color,
    pub change_deleted: Color,
    pub change_renamed: Color,
    pub change_copied: Color,
    pub change_unmerged: Color,

    // ── Ref label colors ─────────────────────────────────────────────
    pub ref_head: Color,
    pub ref_remote: Color,
    pub ref_local: Color,
    pub ref_tag: Color,

    // ── Tag list ─────────────────────────────────────────────────────
    pub tag_name: Color,
    pub tag_hash: Color,
    pub tag_message: Color,

    // ── Stash list ───────────────────────────────────────────────────
    pub stash_index: Color,
    pub stash_message: Color,

    // ── Reflog ───────────────────────────────────────────────────────
    pub reflog_hash: Color,
    pub reflog_message: Color,

    // ── Remotes ──────────────────────────────────────────────────────
    pub remote_name: Color,
    pub remote_url: Color,
    pub remote_branch_name: Color,
    pub remote_branch_detail: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    pub fn from_config(config: &ThemeConfig) -> Self {
        let mut theme = Self::dark();

        if let Some(color) = parse_color_list(&config.active_border_color) {
            theme.active_border = Style::default().fg(color).add_modifier(Modifier::BOLD);
        }
        if let Some(color) = parse_color_list(&config.inactive_border_color) {
            theme.inactive_border = Style::default().fg(color);
        }
        if let Some(color) = parse_color_list(&config.selected_line_bg_color) {
            theme.selected_line = Style::default().bg(color);
        }
        if let Some(color) = parse_color_list(&config.options_text_color) {
            theme.options_text = Style::default().fg(color);
        }

        theme
    }

    pub fn dark() -> Self {
        Self {
            active_border: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            inactive_border: Style::default().fg(Color::DarkGray),
            selected_line: Style::default().bg(Color::DarkGray),
            options_text: Style::default().fg(Color::Blue),
            title: Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            diff_add: Style::default().fg(Color::Green),
            diff_remove: Style::default().fg(Color::Red),
            diff_context: Style::default().fg(Color::Gray),
            diff_add_bg: Color::Rgb(0, 60, 0),
            diff_remove_bg: Color::Rgb(60, 0, 0),
            diff_add_gutter_bg: Color::Rgb(0, 30, 0),
            diff_remove_gutter_bg: Color::Rgb(30, 0, 0),
            diff_add_gutter_fg: Color::Rgb(120, 180, 120),
            diff_remove_gutter_fg: Color::Rgb(200, 120, 120),
            diff_add_word: Color::Rgb(0, 120, 0),
            diff_remove_word: Color::Rgb(120, 0, 0),
            commit_hash: Style::default().fg(Color::Yellow),
            commit_author: Style::default().fg(Color::Green),
            commit_date: Style::default().fg(Color::Blue),
            commit_hash_pushed: Color::Rgb(102, 102, 102),
            commit_hash_merged: Color::Rgb(80, 80, 80),
            branch_local: Style::default().fg(Color::Green),
            branch_remote: Style::default().fg(Color::Red),
            branch_head: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            file_staged: Style::default().fg(Color::Green),
            file_unstaged: Style::default().fg(Color::Red),
            file_untracked: Style::default().fg(Color::LightRed),
            file_conflicted: Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
            search_match: Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black),
            status_bar: Style::default().fg(Color::DarkGray),
            spinner: Style::default().fg(Color::Cyan),

            // UI chrome
            accent: Color::Cyan,
            accent_secondary: Color::Yellow,
            text_dimmed: Color::DarkGray,
            text: Color::Gray,
            text_strong: Color::White,
            separator: Color::DarkGray,
            selected_bg: Color::DarkGray,
            popup_border: Color::Cyan,

            // Command log
            cmd_log_border: Color::Rgb(80, 80, 80),
            cmd_log_title: Color::Rgb(140, 140, 140),
            cmd_log_hint: Color::Rgb(90, 90, 90),
            cmd_log_text: Color::Rgb(100, 100, 100),
            cmd_log_timestamp: Color::Rgb(160, 160, 160),
            cmd_log_success: Color::Rgb(80, 130, 80),

            // Diff panel
            diff_gutter: Color::DarkGray,
            diff_line_number: Color::Rgb(60, 60, 60),
            diff_selection_fg: Color::Rgb(158, 203, 255),
            diff_selection_bg: Color::Rgb(30, 40, 55),
            diff_search_highlight_bg: Color::Rgb(120, 100, 30),
            diff_search_highlight_fg: Color::White,
            diff_search_cursor_bg: Color::Rgb(200, 170, 40),
            diff_search_cursor_fg: Color::Black,
            diff_grid_bg: Color::Rgb(40, 40, 50),
            diff_grid_fg: Color::Yellow,

            // Syntax highlighting
            syntax_comment: Color::Rgb(106, 115, 125),
            syntax_keyword: Color::Rgb(255, 123, 114),
            syntax_string: Color::Rgb(158, 203, 255),
            syntax_number: Color::Rgb(121, 192, 255),
            syntax_function: Color::Rgb(210, 168, 255),
            syntax_function_macro: Color::Rgb(240, 160, 240),
            syntax_type: Color::Rgb(255, 203, 107),
            syntax_variable_builtin: Color::Rgb(255, 123, 114),
            syntax_variable_member: Color::Rgb(121, 192, 255),
            syntax_module: Color::Rgb(255, 203, 107),
            syntax_operator: Color::Rgb(255, 123, 114),
            syntax_tag: Color::Rgb(126, 231, 135),
            syntax_attribute: Color::Rgb(210, 168, 255),
            syntax_label: Color::Rgb(255, 203, 107),
            syntax_punctuation: Color::Rgb(150, 160, 170),
            syntax_default: Color::Rgb(201, 209, 217),

            // Graph
            graph_colors: [
                Color::Cyan,
                Color::Green,
                Color::Yellow,
                Color::Magenta,
                Color::Blue,
                Color::Red,
                Color::LightCyan,
                Color::LightGreen,
            ],

            // Rebase
            rebase_pick: Color::Green,
            rebase_reword: Color::LightBlue,
            rebase_edit: Color::Yellow,
            rebase_squash: Color::Rgb(255, 165, 0),
            rebase_fixup: Color::Rgb(180, 130, 255),
            rebase_drop: Color::Red,
            rebase_paused_bg: Color::Rgb(50, 40, 10),

            // File change status
            change_added: Color::Green,
            change_deleted: Color::Red,
            change_renamed: Color::Yellow,
            change_copied: Color::Cyan,
            change_unmerged: Color::Red,

            // Ref labels
            ref_head: Color::Cyan,
            ref_remote: Color::Red,
            ref_local: Color::Green,
            ref_tag: Color::Cyan,

            // Tags
            tag_name: Color::Green,
            tag_hash: Color::Yellow,
            tag_message: Color::White,

            // Stash
            stash_index: Color::Yellow,
            stash_message: Color::White,

            // Reflog
            reflog_hash: Color::Blue,
            reflog_message: Color::White,

            // Remotes
            remote_name: Color::Cyan,
            remote_url: Color::White,
            remote_branch_name: Color::Cyan,
            remote_branch_detail: Color::DarkGray,
        }
    }
}

// ── JSON theme format ────────────────────────────────────────────────────

/// JSON-serializable theme format. All fields except `id` and `name` are
/// optional — missing values are derived from semantic base colors.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeJson {
    pub id: String,
    pub name: String,

    // Semantic base colors
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub accent: Option<String>,
    pub accent_secondary: Option<String>,
    pub success: Option<String>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub info: Option<String>,

    // Text
    pub text: Option<String>,
    pub text_strong: Option<String>,
    pub text_dimmed: Option<String>,

    // Background / chrome
    pub background: Option<String>,
    pub background_panel: Option<String>,
    pub selected_bg: Option<String>,
    pub separator: Option<String>,
    pub popup_border: Option<String>,

    // Borders
    pub border: Option<String>,
    pub border_active: Option<String>,

    // Diff
    pub diff_add: Option<String>,
    pub diff_remove: Option<String>,
    pub diff_context: Option<String>,
    pub diff_add_bg: Option<String>,
    pub diff_remove_bg: Option<String>,
    pub diff_add_word: Option<String>,
    pub diff_remove_word: Option<String>,
    pub diff_line_number: Option<String>,

    // Syntax
    pub syntax_comment: Option<String>,
    pub syntax_keyword: Option<String>,
    pub syntax_string: Option<String>,
    pub syntax_number: Option<String>,
    pub syntax_function: Option<String>,
    pub syntax_type: Option<String>,
    pub syntax_operator: Option<String>,
    pub syntax_punctuation: Option<String>,
    pub syntax_variable: Option<String>,

    // Graph colors
    pub graph_colors: Option<Vec<String>>,
}

impl ThemeJson {
    /// Convert this JSON theme into a full `Theme`, deriving any missing
    /// values from semantic base colors and the default dark theme.
    pub fn to_theme(&self) -> Theme {
        let dark = Theme::dark();

        // Resolve semantic base colors first (these cascade into derivations)
        let primary = self.primary.as_deref().and_then(parse_hex).unwrap_or(dark.accent);
        let secondary = self.secondary.as_deref().and_then(parse_hex).unwrap_or(dark.accent_secondary);
        let success = self.success.as_deref().and_then(parse_hex).unwrap_or(Color::Green);
        let error = self.error.as_deref().and_then(parse_hex).unwrap_or(Color::Red);
        let warning = self.warning.as_deref().and_then(parse_hex).unwrap_or(Color::Yellow);
        let info = self.info.as_deref().and_then(parse_hex).unwrap_or(Color::Cyan);

        let text_strong = self.text_strong.as_deref().and_then(parse_hex).unwrap_or(dark.text_strong);
        let text = self.text.as_deref().and_then(parse_hex).unwrap_or(dark.text);
        let text_dimmed = self.text_dimmed.as_deref().and_then(parse_hex).unwrap_or(dark.text_dimmed);

        let background = self.background.as_deref().and_then(parse_hex).unwrap_or(Color::Rgb(30, 30, 30));
        let background_panel = self.background_panel.as_deref().and_then(parse_hex).unwrap_or(background);
        let selected_bg = self.selected_bg.as_deref().and_then(parse_hex).unwrap_or(dark.selected_bg);
        let separator = self.separator.as_deref().and_then(parse_hex).unwrap_or(text_dimmed);
        let border = self.border.as_deref().and_then(parse_hex).unwrap_or(separator);
        let border_active = self.border_active.as_deref().and_then(parse_hex).unwrap_or(primary);
        let popup_border = self.popup_border.as_deref().and_then(parse_hex).unwrap_or(primary);

        let accent = self.accent.as_deref().and_then(parse_hex).unwrap_or(primary);
        let accent_secondary = self.accent_secondary.as_deref().and_then(parse_hex).unwrap_or(warning);

        // Diff
        let diff_add = self.diff_add.as_deref().and_then(parse_hex).unwrap_or(success);
        let diff_remove = self.diff_remove.as_deref().and_then(parse_hex).unwrap_or(error);
        let diff_context = self.diff_context.as_deref().and_then(parse_hex).unwrap_or(text_dimmed);
        let diff_add_bg = self.diff_add_bg.as_deref().and_then(parse_hex).unwrap_or(dark.diff_add_bg);
        let diff_remove_bg = self.diff_remove_bg.as_deref().and_then(parse_hex).unwrap_or(dark.diff_remove_bg);
        let diff_add_gutter_bg = mix_colors(diff_add_bg, Color::Rgb(0, 0, 0), 110);
        let diff_remove_gutter_bg = mix_colors(diff_remove_bg, Color::Rgb(0, 0, 0), 110);
        let diff_add_word = self.diff_add_word.as_deref().and_then(parse_hex).unwrap_or(dark.diff_add_word);
        let diff_remove_word = self.diff_remove_word.as_deref().and_then(parse_hex).unwrap_or(dark.diff_remove_word);
        let diff_line_number = self.diff_line_number.as_deref().and_then(parse_hex).unwrap_or(dark.diff_line_number);

        // Syntax
        let syntax_comment = self.syntax_comment.as_deref().and_then(parse_hex).unwrap_or(text_dimmed);
        let syntax_keyword = self.syntax_keyword.as_deref().and_then(parse_hex).unwrap_or(secondary);
        let syntax_string = self.syntax_string.as_deref().and_then(parse_hex).unwrap_or(success);
        let syntax_number = self.syntax_number.as_deref().and_then(parse_hex).unwrap_or(warning);
        let syntax_function = self.syntax_function.as_deref().and_then(parse_hex).unwrap_or(primary);
        let syntax_type = self.syntax_type.as_deref().and_then(parse_hex).unwrap_or(warning);
        let syntax_operator = self.syntax_operator.as_deref().and_then(parse_hex).unwrap_or(secondary);
        let syntax_punctuation = self.syntax_punctuation.as_deref().and_then(parse_hex).unwrap_or(text_strong);
        let syntax_variable = self.syntax_variable.as_deref().and_then(parse_hex).unwrap_or(error);

        // Graph colors
        let graph_colors = if let Some(ref gc) = self.graph_colors {
            let mut arr = dark.graph_colors;
            for (i, hex) in gc.iter().enumerate().take(8) {
                if let Some(c) = parse_hex(hex) {
                    arr[i] = c;
                }
            }
            arr
        } else {
            [primary, success, warning, secondary, info, error, accent, accent_secondary]
        };

        Theme {
            active_border: Style::default().fg(border_active).add_modifier(Modifier::BOLD),
            inactive_border: Style::default().fg(border),
            selected_line: Style::default().bg(selected_bg),
            options_text: Style::default().fg(info),
            title: Style::default().fg(text_strong).add_modifier(Modifier::BOLD),

            diff_add: Style::default().fg(diff_add),
            diff_remove: Style::default().fg(diff_remove),
            diff_context: Style::default().fg(diff_context),
            diff_add_bg,
            diff_remove_bg,
            diff_add_gutter_bg,
            diff_remove_gutter_bg,
            diff_add_gutter_fg: mix_colors(diff_add, Color::Rgb(255, 255, 255), 30),
            diff_remove_gutter_fg: mix_colors(diff_remove, Color::Rgb(255, 255, 255), 30),
            diff_add_word,
            diff_remove_word,

            commit_hash: Style::default().fg(warning),
            commit_author: Style::default().fg(primary),
            commit_date: Style::default().fg(info),
            commit_hash_pushed: text_dimmed,
            commit_hash_merged: border,

            branch_local: Style::default().fg(success),
            branch_remote: Style::default().fg(error),
            branch_head: Style::default().fg(primary).add_modifier(Modifier::BOLD),

            file_staged: Style::default().fg(success),
            file_unstaged: Style::default().fg(warning),
            file_untracked: Style::default().fg(info),
            file_conflicted: Style::default().fg(error).add_modifier(Modifier::BOLD),

            search_match: Style::default().bg(warning).fg(background),
            status_bar: Style::default().fg(text_dimmed),
            spinner: Style::default().fg(primary),

            accent,
            accent_secondary,
            text_dimmed,
            text,
            text_strong,
            separator,
            selected_bg,
            popup_border,

            cmd_log_border: border,
            cmd_log_title: text,
            cmd_log_hint: text_dimmed,
            cmd_log_text: text_dimmed,
            cmd_log_timestamp: text,
            cmd_log_success: success,

            diff_gutter: text_dimmed,
            diff_line_number,
            diff_selection_fg: info,
            diff_selection_bg: mix_colors(background, primary, 40),
            diff_search_highlight_bg: mix_colors(warning, background, 100),
            diff_search_highlight_fg: text_strong,
            diff_search_cursor_bg: warning,
            diff_search_cursor_fg: background,
            diff_grid_bg: mix_colors(background_panel, text_dimmed, 30),
            diff_grid_fg: warning,

            syntax_comment,
            syntax_keyword,
            syntax_string,
            syntax_number,
            syntax_function,
            syntax_function_macro: syntax_function, // derive from function
            syntax_type,
            syntax_variable_builtin: syntax_variable,
            syntax_variable_member: text_strong,
            syntax_module: syntax_type,
            syntax_operator,
            syntax_tag: success,
            syntax_attribute: secondary,
            syntax_label: warning,
            syntax_punctuation,
            syntax_default: text_strong,

            graph_colors,

            rebase_pick: success,
            rebase_reword: primary,
            rebase_edit: warning,
            rebase_squash: mix_colors(warning, error, 128),
            rebase_fixup: secondary,
            rebase_drop: error,
            rebase_paused_bg: mix_colors(warning, background, 60),

            change_added: success,
            change_deleted: error,
            change_renamed: warning,
            change_copied: info,
            change_unmerged: error,

            ref_head: primary,
            ref_remote: error,
            ref_local: success,
            ref_tag: info,

            tag_name: success,
            tag_hash: warning,
            tag_message: text,

            stash_index: warning,
            stash_message: text,

            reflog_hash: primary,
            reflog_message: text,

            remote_name: info,
            remote_url: text,
            remote_branch_name: info,
            remote_branch_detail: text_dimmed,
        }
    }
}

// ── Built-in color themes ─────────────────────────────────────────────────

/// A named color theme preset.
#[derive(Debug, Clone)]
pub struct ColorTheme {
    pub name: String,
    pub id: String,
}

impl ColorTheme {
    /// Apply this theme preset to produce a full Theme.
    pub fn to_theme(&self) -> Theme {
        if self.id == "default" {
            return Theme::dark();
        }

        // Try embedded themes (generated + custom built-in)
        for dir in &[&GENERATED_THEMES_DIR, &CUSTOM_THEMES_DIR] {
            for file in dir.files() {
                if file.path().extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Some(contents) = file.contents_utf8() {
                    if let Ok(theme_json) = serde_json::from_str::<ThemeJson>(contents) {
                        if theme_json.id == self.id {
                            return theme_json.to_theme();
                        }
                    }
                }
            }
        }

        // Try user themes loaded at runtime
        if let Some(theme) = load_user_theme(&self.id) {
            return theme;
        }

        Theme::dark()
    }
}

/// All available color themes (cached on first access).
pub static COLOR_THEMES: once_cell::sync::Lazy<Vec<ColorTheme>> =
    once_cell::sync::Lazy::new(load_color_themes);

/// Load all available color themes (embedded + user directory).
pub fn load_color_themes() -> Vec<ColorTheme> {
    let mut themes = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    // 1. Default theme first
    themes.push(ColorTheme {
        name: "Default (Dark)".to_string(),
        id: "default".to_string(),
    });
    seen_ids.insert("default".to_string());

    // 2. Embedded themes (generated + custom built-in)
    for dir in &[&GENERATED_THEMES_DIR, &CUSTOM_THEMES_DIR] {
        for file in dir.files() {
            if file.path().extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(contents) = file.contents_utf8() {
                if let Ok(theme_json) = serde_json::from_str::<ThemeJson>(contents) {
                    if seen_ids.insert(theme_json.id.clone()) {
                        themes.push(ColorTheme {
                            name: theme_json.name.clone(),
                            id: theme_json.id.clone(),
                        });
                    }
                }
            }
        }
    }

    // 3. User themes from ~/.config/lazygit/themes/
    if let Some(user_themes) = discover_user_themes() {
        for (id, name) in user_themes {
            if seen_ids.insert(id.clone()) {
                themes.push(ColorTheme { name, id });
            }
        }
    }

    // Sort non-default themes alphabetically by name
    if themes.len() > 1 {
        themes[1..].sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    themes
}

// ── Embedded themes (generated at build time by scripts/gen-themes.ts) ──

use include_dir::{include_dir, Dir};

/// All JSON files under `src/generated_themes/` are embedded at compile time.
/// No hardcoded list needed — adding/removing a file is all it takes.
static GENERATED_THEMES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/generated_themes");
static CUSTOM_THEMES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/themes");

// ── User theme discovery & loading ──────────────────────────────────────

fn user_themes_dirs() -> Vec<std::path::PathBuf> {
    crate::config::config_dir_candidates()
        .into_iter()
        .map(|dir| dir.join("themes"))
        .collect()
}

fn discover_user_themes() -> Option<Vec<(String, String)>> {
    let mut result = Vec::new();
    for dir in user_themes_dirs() {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(contents) = std::fs::read_to_string(&path) {
                        if let Ok(theme_json) = serde_json::from_str::<ThemeJson>(&contents) {
                            result.push((theme_json.id.clone(), theme_json.name.clone()));
                        }
                    }
                }
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn load_user_theme(id: &str) -> Option<Theme> {
    for dir in user_themes_dirs() {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(contents) = std::fs::read_to_string(&path) {
                        if let Ok(theme_json) = serde_json::from_str::<ThemeJson>(&contents) {
                            if theme_json.id == id {
                                return Some(theme_json.to_theme());
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

// ── Color helpers ────────────────────────────────────────────────────────

/// Mix two RGB colors. `amount` is 0..255 where 0 = all `a`, 255 = all `b`.
fn mix_colors(a: Color, b: Color, amount: u8) -> Color {
    let (ar, ag, ab) = color_to_rgb(a);
    let (br, bg, bb) = color_to_rgb(b);
    let t = amount as f32 / 255.0;
    Color::Rgb(
        (ar as f32 + (br as f32 - ar as f32) * t) as u8,
        (ag as f32 + (bg as f32 - ag as f32) * t) as u8,
        (ab as f32 + (bb as f32 - ab as f32) * t) as u8,
    )
}

fn color_to_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::Red => (255, 0, 0),
        Color::Green => (0, 255, 0),
        Color::Yellow => (255, 255, 0),
        Color::Blue => (0, 0, 255),
        Color::Magenta => (255, 0, 255),
        Color::Cyan => (0, 255, 255),
        Color::Gray => (128, 128, 128),
        Color::DarkGray => (80, 80, 80),
        Color::LightRed => (255, 100, 100),
        Color::LightGreen => (100, 255, 100),
        Color::LightYellow => (255, 255, 100),
        Color::LightBlue => (100, 100, 255),
        Color::LightMagenta => (255, 100, 255),
        Color::LightCyan => (100, 255, 255),
        Color::White => (255, 255, 255),
        _ => (128, 128, 128),
    }
}

fn parse_hex(s: &str) -> Option<Color> {
    let s = s.trim();
    if s.starts_with('#') && s.len() == 7 {
        let r = u8::from_str_radix(&s[1..3], 16).ok()?;
        let g = u8::from_str_radix(&s[3..5], 16).ok()?;
        let b = u8::from_str_radix(&s[5..7], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        parse_color(s)
    }
}

fn parse_color_list(colors: &[String]) -> Option<Color> {
    colors.first().and_then(|s| parse_color(s))
}

fn parse_color(s: &str) -> Option<Color> {
    match s.to_lowercase().as_str() {
        "default" => None,
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        s if s.starts_with('#') && s.len() == 7 => {
            let r = u8::from_str_radix(&s[1..3], 16).ok()?;
            let g = u8::from_str_radix(&s[3..5], 16).ok()?;
            let b = u8::from_str_radix(&s[5..7], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}
