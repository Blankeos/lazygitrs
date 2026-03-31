use ratatui::style::{Color, Modifier, Style};
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

// ── Built-in color themes ─────────────────────────────────────────────────

/// A named color theme preset.
#[derive(Debug, Clone)]
pub struct ColorTheme {
    pub name: &'static str,
    pub id: &'static str,
}

impl ColorTheme {
    /// Apply this theme preset to produce a full Theme.
    pub fn to_theme(&self) -> Theme {
        match self.id {
            "default" => Theme::dark(),
            "catppuccin-mocha" => catppuccin_mocha(),
            "catppuccin-macchiato" => catppuccin_macchiato(),
            "dracula" => dracula(),
            "tokyonight" => tokyonight(),
            "gruvbox" => gruvbox(),
            "nord" => nord(),
            "solarized-dark" => solarized_dark(),
            "onedark" => onedark(),
            "rosepine" => rosepine(),
            "kanagawa" => kanagawa(),
            "everforest" => everforest(),
            "monokai" => monokai(),
            _ => Theme::dark(),
        }
    }
}

/// All available built-in color themes.
pub static COLOR_THEMES: &[ColorTheme] = &[
    ColorTheme { name: "Default (Dark)", id: "default" },
    ColorTheme { name: "Catppuccin Mocha", id: "catppuccin-mocha" },
    ColorTheme { name: "Catppuccin Macchiato", id: "catppuccin-macchiato" },
    ColorTheme { name: "Dracula", id: "dracula" },
    ColorTheme { name: "Tokyo Night", id: "tokyonight" },
    ColorTheme { name: "Gruvbox Dark", id: "gruvbox" },
    ColorTheme { name: "Nord", id: "nord" },
    ColorTheme { name: "Solarized Dark", id: "solarized-dark" },
    ColorTheme { name: "One Dark", id: "onedark" },
    ColorTheme { name: "Rosé Pine", id: "rosepine" },
    ColorTheme { name: "Kanagawa", id: "kanagawa" },
    ColorTheme { name: "Everforest", id: "everforest" },
    ColorTheme { name: "Monokai Pro", id: "monokai" },
];

/// Helper: build a theme from the default dark base with overrides applied by a closure.
fn theme_from(f: impl FnOnce(&mut Theme)) -> Theme {
    let mut t = Theme::dark();
    f(&mut t);
    t
}

fn catppuccin_mocha() -> Theme {
    // Catppuccin Mocha palette
    let rosewater = Color::Rgb(245, 224, 220);
    let flamingo = Color::Rgb(242, 205, 205);
    let pink = Color::Rgb(245, 194, 231);
    let mauve = Color::Rgb(203, 166, 247);
    let red = Color::Rgb(243, 139, 168);
    let maroon = Color::Rgb(235, 160, 172);
    let peach = Color::Rgb(250, 179, 135);
    let yellow = Color::Rgb(249, 226, 175);
    let green = Color::Rgb(166, 227, 161);
    let teal = Color::Rgb(148, 226, 213);
    let sky = Color::Rgb(137, 220, 235);
    let sapphire = Color::Rgb(116, 199, 236);
    let blue = Color::Rgb(137, 180, 250);
    let lavender = Color::Rgb(180, 190, 254);
    let text = Color::Rgb(205, 214, 244);
    let subtext1 = Color::Rgb(186, 194, 222);
    let subtext0 = Color::Rgb(166, 173, 200);
    let overlay2 = Color::Rgb(147, 153, 178);
    let overlay1 = Color::Rgb(127, 132, 156);
    let overlay0 = Color::Rgb(108, 112, 134);
    let surface2 = Color::Rgb(88, 91, 112);
    let surface1 = Color::Rgb(69, 71, 90);
    let surface0 = Color::Rgb(49, 50, 68);
    let _base = Color::Rgb(30, 30, 46);
    let _mantle = Color::Rgb(24, 24, 37);
    let _crust = Color::Rgb(17, 17, 27);

    theme_from(|t| {
        t.active_border = Style::default().fg(mauve).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(surface1);
        t.selected_line = Style::default().bg(surface0);
        t.options_text = Style::default().fg(blue);
        t.title = Style::default().fg(text).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(overlay0);
        t.diff_add_bg = Color::Rgb(30, 60, 30);
        t.diff_remove_bg = Color::Rgb(60, 30, 35);
        t.diff_add_word = Color::Rgb(50, 120, 50);
        t.diff_remove_word = Color::Rgb(120, 50, 60);

        t.commit_hash = Style::default().fg(yellow);
        t.commit_author = Style::default().fg(blue);
        t.commit_date = Style::default().fg(sapphire);
        t.commit_hash_pushed = overlay0;
        t.commit_hash_merged = surface2;

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(mauve).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(green);
        t.file_unstaged = Style::default().fg(peach);
        t.file_untracked = Style::default().fg(flamingo);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(Color::Rgb(30, 30, 46));
        t.status_bar = Style::default().fg(overlay0);
        t.spinner = Style::default().fg(mauve);

        t.accent = mauve;
        t.accent_secondary = yellow;
        t.text_dimmed = overlay0;
        t.text = subtext0;
        t.text_strong = text;
        t.separator = surface1;
        t.selected_bg = surface0;
        t.popup_border = mauve;

        t.cmd_log_border = surface1;
        t.cmd_log_title = subtext0;
        t.cmd_log_hint = overlay0;
        t.cmd_log_text = overlay1;
        t.cmd_log_timestamp = subtext1;
        t.cmd_log_success = green;

        t.diff_gutter = overlay0;
        t.diff_line_number = surface2;
        t.diff_selection_fg = sky;
        t.diff_selection_bg = Color::Rgb(30, 40, 55);
        t.diff_search_highlight_bg = Color::Rgb(120, 100, 30);
        t.diff_search_highlight_fg = text;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = Color::Rgb(30, 30, 46);
        t.diff_grid_bg = surface0;
        t.diff_grid_fg = yellow;

        t.syntax_comment = overlay0;
        t.syntax_keyword = mauve;
        t.syntax_string = green;
        t.syntax_number = peach;
        t.syntax_function = blue;
        t.syntax_function_macro = pink;
        t.syntax_type = yellow;
        t.syntax_variable_builtin = red;
        t.syntax_variable_member = lavender;
        t.syntax_module = peach;
        t.syntax_operator = sky;
        t.syntax_tag = teal;
        t.syntax_attribute = mauve;
        t.syntax_label = flamingo;
        t.syntax_punctuation = overlay2;
        t.syntax_default = text;

        t.graph_colors = [mauve, green, yellow, pink, blue, red, teal, peach];

        t.rebase_pick = green;
        t.rebase_reword = blue;
        t.rebase_edit = yellow;
        t.rebase_squash = peach;
        t.rebase_fixup = mauve;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(50, 45, 25);

        t.change_added = green;
        t.change_deleted = red;
        t.change_renamed = yellow;
        t.change_copied = teal;
        t.change_unmerged = maroon;

        t.ref_head = mauve;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = teal;

        t.tag_name = green;
        t.tag_hash = yellow;
        t.tag_message = subtext0;

        t.stash_index = yellow;
        t.stash_message = subtext0;

        t.reflog_hash = blue;
        t.reflog_message = subtext0;

        t.remote_name = teal;
        t.remote_url = subtext0;
        t.remote_branch_name = teal;
        t.remote_branch_detail = overlay0;
    })
}

fn catppuccin_macchiato() -> Theme {
    let rosewater = Color::Rgb(244, 219, 214);
    let flamingo = Color::Rgb(240, 198, 198);
    let pink = Color::Rgb(245, 189, 230);
    let mauve = Color::Rgb(198, 160, 246);
    let red = Color::Rgb(237, 135, 150);
    let maroon = Color::Rgb(238, 153, 160);
    let peach = Color::Rgb(245, 169, 127);
    let yellow = Color::Rgb(238, 212, 159);
    let green = Color::Rgb(166, 218, 149);
    let teal = Color::Rgb(139, 213, 202);
    let sky = Color::Rgb(145, 215, 227);
    let sapphire = Color::Rgb(125, 196, 228);
    let blue = Color::Rgb(138, 173, 244);
    let lavender = Color::Rgb(183, 189, 248);
    let text = Color::Rgb(202, 211, 245);
    let subtext1 = Color::Rgb(184, 192, 224);
    let subtext0 = Color::Rgb(165, 173, 203);
    let overlay2 = Color::Rgb(147, 154, 183);
    let overlay1 = Color::Rgb(128, 135, 162);
    let overlay0 = Color::Rgb(110, 115, 141);
    let surface2 = Color::Rgb(91, 96, 120);
    let surface1 = Color::Rgb(73, 77, 100);
    let surface0 = Color::Rgb(54, 58, 79);
    let _base = Color::Rgb(36, 39, 58);

    theme_from(|t| {
        t.active_border = Style::default().fg(mauve).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(surface1);
        t.selected_line = Style::default().bg(surface0);
        t.options_text = Style::default().fg(blue);
        t.title = Style::default().fg(text).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(overlay0);
        t.diff_add_bg = Color::Rgb(30, 55, 30);
        t.diff_remove_bg = Color::Rgb(55, 30, 35);
        t.diff_add_word = Color::Rgb(50, 110, 50);
        t.diff_remove_word = Color::Rgb(110, 50, 55);

        t.commit_hash = Style::default().fg(yellow);
        t.commit_author = Style::default().fg(blue);
        t.commit_date = Style::default().fg(sapphire);
        t.commit_hash_pushed = overlay0;
        t.commit_hash_merged = surface2;

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(mauve).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(green);
        t.file_unstaged = Style::default().fg(peach);
        t.file_untracked = Style::default().fg(flamingo);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(Color::Rgb(36, 39, 58));
        t.status_bar = Style::default().fg(overlay0);
        t.spinner = Style::default().fg(mauve);

        t.accent = mauve;
        t.accent_secondary = yellow;
        t.text_dimmed = overlay0;
        t.text = subtext0;
        t.text_strong = text;
        t.separator = surface1;
        t.selected_bg = surface0;
        t.popup_border = mauve;

        t.cmd_log_border = surface1;
        t.cmd_log_title = subtext0;
        t.cmd_log_hint = overlay0;
        t.cmd_log_text = overlay1;
        t.cmd_log_timestamp = subtext1;
        t.cmd_log_success = green;

        t.diff_gutter = overlay0;
        t.diff_line_number = surface2;
        t.diff_selection_fg = sky;
        t.diff_selection_bg = Color::Rgb(30, 38, 50);
        t.diff_search_highlight_bg = Color::Rgb(110, 95, 30);
        t.diff_search_highlight_fg = text;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = Color::Rgb(36, 39, 58);
        t.diff_grid_bg = surface0;
        t.diff_grid_fg = yellow;

        t.syntax_comment = overlay0;
        t.syntax_keyword = mauve;
        t.syntax_string = green;
        t.syntax_number = peach;
        t.syntax_function = blue;
        t.syntax_function_macro = pink;
        t.syntax_type = yellow;
        t.syntax_variable_builtin = red;
        t.syntax_variable_member = lavender;
        t.syntax_module = peach;
        t.syntax_operator = sky;
        t.syntax_tag = teal;
        t.syntax_attribute = mauve;
        t.syntax_label = flamingo;
        t.syntax_punctuation = overlay2;
        t.syntax_default = text;

        t.graph_colors = [mauve, green, yellow, pink, blue, red, teal, peach];

        t.rebase_pick = green;
        t.rebase_reword = blue;
        t.rebase_edit = yellow;
        t.rebase_squash = peach;
        t.rebase_fixup = mauve;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(50, 45, 25);

        t.change_added = green;
        t.change_deleted = red;
        t.change_renamed = yellow;
        t.change_copied = teal;
        t.change_unmerged = maroon;

        t.ref_head = mauve;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = teal;

        t.tag_name = green;
        t.tag_hash = yellow;
        t.tag_message = subtext0;

        t.stash_index = yellow;
        t.stash_message = subtext0;

        t.reflog_hash = blue;
        t.reflog_message = subtext0;

        t.remote_name = teal;
        t.remote_url = subtext0;
        t.remote_branch_name = teal;
        t.remote_branch_detail = overlay0;
    })
}

fn dracula() -> Theme {
    let bg = Color::Rgb(40, 42, 54);
    let fg = Color::Rgb(248, 248, 242);
    let selection = Color::Rgb(68, 71, 90);
    let comment = Color::Rgb(98, 114, 164);
    let cyan = Color::Rgb(139, 233, 253);
    let green = Color::Rgb(80, 250, 123);
    let orange = Color::Rgb(255, 184, 108);
    let pink = Color::Rgb(255, 121, 198);
    let purple = Color::Rgb(189, 147, 249);
    let red = Color::Rgb(255, 85, 85);
    let yellow = Color::Rgb(241, 250, 140);

    theme_from(|t| {
        t.active_border = Style::default().fg(purple).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(comment);
        t.selected_line = Style::default().bg(selection);
        t.options_text = Style::default().fg(cyan);
        t.title = Style::default().fg(fg).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(comment);
        t.diff_add_bg = Color::Rgb(25, 60, 25);
        t.diff_remove_bg = Color::Rgb(60, 25, 25);
        t.diff_add_word = Color::Rgb(40, 130, 50);
        t.diff_remove_word = Color::Rgb(130, 40, 40);

        t.commit_hash = Style::default().fg(yellow);
        t.commit_author = Style::default().fg(cyan);
        t.commit_date = Style::default().fg(purple);
        t.commit_hash_pushed = comment;
        t.commit_hash_merged = Color::Rgb(60, 62, 75);

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(pink).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(green);
        t.file_unstaged = Style::default().fg(orange);
        t.file_untracked = Style::default().fg(cyan);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(bg);
        t.status_bar = Style::default().fg(comment);
        t.spinner = Style::default().fg(purple);

        t.accent = purple;
        t.accent_secondary = yellow;
        t.text_dimmed = comment;
        t.text = Color::Rgb(180, 180, 200);
        t.text_strong = fg;
        t.separator = Color::Rgb(68, 71, 90);
        t.selected_bg = selection;
        t.popup_border = purple;

        t.cmd_log_border = Color::Rgb(68, 71, 90);
        t.cmd_log_title = Color::Rgb(150, 150, 170);
        t.cmd_log_hint = comment;
        t.cmd_log_text = Color::Rgb(130, 130, 150);
        t.cmd_log_timestamp = Color::Rgb(180, 180, 200);
        t.cmd_log_success = green;

        t.diff_gutter = comment;
        t.diff_line_number = Color::Rgb(68, 71, 90);
        t.diff_selection_fg = cyan;
        t.diff_selection_bg = Color::Rgb(35, 45, 60);
        t.diff_search_highlight_bg = Color::Rgb(120, 120, 30);
        t.diff_search_highlight_fg = fg;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = bg;
        t.diff_grid_bg = Color::Rgb(50, 52, 66);
        t.diff_grid_fg = yellow;

        t.syntax_comment = comment;
        t.syntax_keyword = pink;
        t.syntax_string = yellow;
        t.syntax_number = purple;
        t.syntax_function = green;
        t.syntax_function_macro = cyan;
        t.syntax_type = cyan;
        t.syntax_variable_builtin = purple;
        t.syntax_variable_member = fg;
        t.syntax_module = orange;
        t.syntax_operator = pink;
        t.syntax_tag = pink;
        t.syntax_attribute = green;
        t.syntax_label = cyan;
        t.syntax_punctuation = fg;
        t.syntax_default = fg;

        t.graph_colors = [purple, green, yellow, pink, cyan, red, orange, Color::Rgb(139, 233, 253)];

        t.rebase_pick = green;
        t.rebase_reword = cyan;
        t.rebase_edit = yellow;
        t.rebase_squash = orange;
        t.rebase_fixup = purple;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(55, 50, 25);

        t.change_added = green;
        t.change_deleted = red;
        t.change_renamed = orange;
        t.change_copied = cyan;
        t.change_unmerged = red;

        t.ref_head = pink;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = cyan;

        t.tag_name = green;
        t.tag_hash = yellow;
        t.tag_message = Color::Rgb(180, 180, 200);

        t.stash_index = yellow;
        t.stash_message = Color::Rgb(180, 180, 200);

        t.reflog_hash = purple;
        t.reflog_message = Color::Rgb(180, 180, 200);

        t.remote_name = cyan;
        t.remote_url = Color::Rgb(180, 180, 200);
        t.remote_branch_name = cyan;
        t.remote_branch_detail = comment;
    })
}

fn tokyonight() -> Theme {
    let fg = Color::Rgb(192, 202, 245);
    let bg = Color::Rgb(26, 27, 38);
    let blue = Color::Rgb(122, 162, 247);
    let cyan = Color::Rgb(125, 207, 255);
    let green = Color::Rgb(158, 206, 106);
    let magenta = Color::Rgb(187, 154, 247);
    let orange = Color::Rgb(255, 158, 100);
    let red = Color::Rgb(247, 118, 142);
    let yellow = Color::Rgb(224, 175, 104);
    let teal = Color::Rgb(26, 188, 156);
    let comment = Color::Rgb(86, 95, 137);
    let dark3 = Color::Rgb(68, 75, 106);
    let dark5 = Color::Rgb(55, 62, 89);
    let fg_dark = Color::Rgb(162, 173, 210);
    let fg_gutter = Color::Rgb(59, 66, 97);

    theme_from(|t| {
        t.active_border = Style::default().fg(blue).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(dark5);
        t.selected_line = Style::default().bg(Color::Rgb(41, 46, 66));
        t.options_text = Style::default().fg(cyan);
        t.title = Style::default().fg(fg).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(comment);
        t.diff_add_bg = Color::Rgb(20, 50, 25);
        t.diff_remove_bg = Color::Rgb(55, 25, 30);
        t.diff_add_word = Color::Rgb(40, 100, 45);
        t.diff_remove_word = Color::Rgb(110, 40, 50);

        t.commit_hash = Style::default().fg(orange);
        t.commit_author = Style::default().fg(blue);
        t.commit_date = Style::default().fg(teal);
        t.commit_hash_pushed = comment;
        t.commit_hash_merged = dark3;

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(blue).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(green);
        t.file_unstaged = Style::default().fg(orange);
        t.file_untracked = Style::default().fg(cyan);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(bg);
        t.status_bar = Style::default().fg(comment);
        t.spinner = Style::default().fg(blue);

        t.accent = blue;
        t.accent_secondary = yellow;
        t.text_dimmed = comment;
        t.text = fg_dark;
        t.text_strong = fg;
        t.separator = dark5;
        t.selected_bg = Color::Rgb(41, 46, 66);
        t.popup_border = blue;

        t.cmd_log_border = dark5;
        t.cmd_log_title = fg_dark;
        t.cmd_log_hint = comment;
        t.cmd_log_text = dark3;
        t.cmd_log_timestamp = fg_dark;
        t.cmd_log_success = green;

        t.diff_gutter = comment;
        t.diff_line_number = fg_gutter;
        t.diff_selection_fg = cyan;
        t.diff_selection_bg = Color::Rgb(30, 40, 55);
        t.diff_search_highlight_bg = Color::Rgb(100, 90, 25);
        t.diff_search_highlight_fg = fg;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = bg;
        t.diff_grid_bg = Color::Rgb(36, 40, 55);
        t.diff_grid_fg = yellow;

        t.syntax_comment = comment;
        t.syntax_keyword = magenta;
        t.syntax_string = green;
        t.syntax_number = orange;
        t.syntax_function = blue;
        t.syntax_function_macro = cyan;
        t.syntax_type = yellow;
        t.syntax_variable_builtin = red;
        t.syntax_variable_member = cyan;
        t.syntax_module = yellow;
        t.syntax_operator = magenta;
        t.syntax_tag = red;
        t.syntax_attribute = yellow;
        t.syntax_label = orange;
        t.syntax_punctuation = fg_dark;
        t.syntax_default = fg;

        t.graph_colors = [blue, green, yellow, magenta, cyan, red, teal, orange];

        t.rebase_pick = green;
        t.rebase_reword = blue;
        t.rebase_edit = yellow;
        t.rebase_squash = orange;
        t.rebase_fixup = magenta;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(50, 40, 20);

        t.change_added = green;
        t.change_deleted = red;
        t.change_renamed = yellow;
        t.change_copied = cyan;
        t.change_unmerged = red;

        t.ref_head = blue;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = cyan;

        t.tag_name = green;
        t.tag_hash = orange;
        t.tag_message = fg_dark;

        t.stash_index = orange;
        t.stash_message = fg_dark;

        t.reflog_hash = blue;
        t.reflog_message = fg_dark;

        t.remote_name = cyan;
        t.remote_url = fg_dark;
        t.remote_branch_name = cyan;
        t.remote_branch_detail = comment;
    })
}

fn gruvbox() -> Theme {
    let fg = Color::Rgb(235, 219, 178);
    let bg = Color::Rgb(40, 40, 40);
    let red = Color::Rgb(251, 73, 52);
    let green = Color::Rgb(184, 187, 38);
    let yellow = Color::Rgb(250, 189, 47);
    let blue = Color::Rgb(131, 165, 152);
    let purple = Color::Rgb(211, 134, 155);
    let aqua = Color::Rgb(142, 192, 124);
    let orange = Color::Rgb(254, 128, 25);
    let gray = Color::Rgb(146, 131, 116);
    let bg1 = Color::Rgb(60, 56, 54);
    let bg2 = Color::Rgb(80, 73, 69);
    let bg3 = Color::Rgb(102, 92, 84);
    let fg4 = Color::Rgb(168, 153, 132);

    theme_from(|t| {
        t.active_border = Style::default().fg(yellow).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(bg3);
        t.selected_line = Style::default().bg(bg1);
        t.options_text = Style::default().fg(blue);
        t.title = Style::default().fg(fg).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(gray);
        t.diff_add_bg = Color::Rgb(30, 50, 15);
        t.diff_remove_bg = Color::Rgb(55, 20, 15);
        t.diff_add_word = Color::Rgb(50, 90, 25);
        t.diff_remove_word = Color::Rgb(100, 30, 20);

        t.commit_hash = Style::default().fg(yellow);
        t.commit_author = Style::default().fg(aqua);
        t.commit_date = Style::default().fg(blue);
        t.commit_hash_pushed = gray;
        t.commit_hash_merged = bg3;

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(yellow).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(green);
        t.file_unstaged = Style::default().fg(orange);
        t.file_untracked = Style::default().fg(aqua);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(bg);
        t.status_bar = Style::default().fg(gray);
        t.spinner = Style::default().fg(yellow);

        t.accent = yellow;
        t.accent_secondary = orange;
        t.text_dimmed = gray;
        t.text = fg4;
        t.text_strong = fg;
        t.separator = bg2;
        t.selected_bg = bg1;
        t.popup_border = yellow;

        t.cmd_log_border = bg2;
        t.cmd_log_title = fg4;
        t.cmd_log_hint = gray;
        t.cmd_log_text = bg3;
        t.cmd_log_timestamp = fg4;
        t.cmd_log_success = green;

        t.diff_gutter = gray;
        t.diff_line_number = bg3;
        t.diff_selection_fg = aqua;
        t.diff_selection_bg = Color::Rgb(35, 40, 45);
        t.diff_search_highlight_bg = Color::Rgb(120, 90, 20);
        t.diff_search_highlight_fg = fg;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = bg;
        t.diff_grid_bg = Color::Rgb(50, 48, 45);
        t.diff_grid_fg = yellow;

        t.syntax_comment = gray;
        t.syntax_keyword = red;
        t.syntax_string = green;
        t.syntax_number = purple;
        t.syntax_function = aqua;
        t.syntax_function_macro = orange;
        t.syntax_type = yellow;
        t.syntax_variable_builtin = orange;
        t.syntax_variable_member = blue;
        t.syntax_module = aqua;
        t.syntax_operator = fg;
        t.syntax_tag = aqua;
        t.syntax_attribute = yellow;
        t.syntax_label = orange;
        t.syntax_punctuation = fg4;
        t.syntax_default = fg;

        t.graph_colors = [yellow, green, aqua, purple, blue, red, orange, Color::Rgb(184, 187, 38)];

        t.rebase_pick = green;
        t.rebase_reword = blue;
        t.rebase_edit = yellow;
        t.rebase_squash = orange;
        t.rebase_fixup = purple;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(55, 50, 20);

        t.change_added = green;
        t.change_deleted = red;
        t.change_renamed = yellow;
        t.change_copied = aqua;
        t.change_unmerged = red;

        t.ref_head = yellow;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = aqua;

        t.tag_name = green;
        t.tag_hash = yellow;
        t.tag_message = fg4;

        t.stash_index = yellow;
        t.stash_message = fg4;

        t.reflog_hash = blue;
        t.reflog_message = fg4;

        t.remote_name = aqua;
        t.remote_url = fg4;
        t.remote_branch_name = aqua;
        t.remote_branch_detail = gray;
    })
}

fn nord() -> Theme {
    // Nord palette
    let nord0 = Color::Rgb(46, 52, 64);
    let nord1 = Color::Rgb(59, 66, 82);
    let nord2 = Color::Rgb(67, 76, 94);
    let nord3 = Color::Rgb(76, 86, 106);
    let nord4 = Color::Rgb(216, 222, 233);
    let nord5 = Color::Rgb(229, 233, 240);
    let _nord6 = Color::Rgb(236, 239, 244);
    let nord7 = Color::Rgb(143, 188, 187);     // teal
    let nord8 = Color::Rgb(136, 192, 208);     // light blue
    let nord9 = Color::Rgb(129, 161, 193);     // blue
    let nord10 = Color::Rgb(94, 129, 172);     // dark blue
    let nord11 = Color::Rgb(191, 97, 106);     // red
    let nord12 = Color::Rgb(208, 135, 112);    // orange
    let nord13 = Color::Rgb(235, 203, 139);    // yellow
    let nord14 = Color::Rgb(163, 190, 140);    // green
    let nord15 = Color::Rgb(180, 142, 173);    // purple

    theme_from(|t| {
        t.active_border = Style::default().fg(nord8).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(nord3);
        t.selected_line = Style::default().bg(nord1);
        t.options_text = Style::default().fg(nord9);
        t.title = Style::default().fg(nord4).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(nord14);
        t.diff_remove = Style::default().fg(nord11);
        t.diff_context = Style::default().fg(nord3);
        t.diff_add_bg = Color::Rgb(30, 50, 30);
        t.diff_remove_bg = Color::Rgb(50, 30, 30);
        t.diff_add_word = Color::Rgb(50, 100, 50);
        t.diff_remove_word = Color::Rgb(100, 50, 50);

        t.commit_hash = Style::default().fg(nord13);
        t.commit_author = Style::default().fg(nord8);
        t.commit_date = Style::default().fg(nord9);
        t.commit_hash_pushed = nord3;
        t.commit_hash_merged = nord2;

        t.branch_local = Style::default().fg(nord14);
        t.branch_remote = Style::default().fg(nord11);
        t.branch_head = Style::default().fg(nord8).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(nord14);
        t.file_unstaged = Style::default().fg(nord12);
        t.file_untracked = Style::default().fg(nord7);
        t.file_conflicted = Style::default().fg(nord11).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(nord13).fg(nord0);
        t.status_bar = Style::default().fg(nord3);
        t.spinner = Style::default().fg(nord8);

        t.accent = nord8;
        t.accent_secondary = nord13;
        t.text_dimmed = nord3;
        t.text = nord4;
        t.text_strong = nord5;
        t.separator = nord2;
        t.selected_bg = nord1;
        t.popup_border = nord8;

        t.cmd_log_border = nord2;
        t.cmd_log_title = nord4;
        t.cmd_log_hint = nord3;
        t.cmd_log_text = nord3;
        t.cmd_log_timestamp = nord4;
        t.cmd_log_success = nord14;

        t.diff_gutter = nord3;
        t.diff_line_number = nord2;
        t.diff_selection_fg = nord8;
        t.diff_selection_bg = Color::Rgb(30, 40, 55);
        t.diff_search_highlight_bg = Color::Rgb(100, 85, 30);
        t.diff_search_highlight_fg = nord4;
        t.diff_search_cursor_bg = nord13;
        t.diff_search_cursor_fg = nord0;
        t.diff_grid_bg = Color::Rgb(50, 55, 65);
        t.diff_grid_fg = nord13;

        t.syntax_comment = nord3;
        t.syntax_keyword = nord9;
        t.syntax_string = nord14;
        t.syntax_number = nord15;
        t.syntax_function = nord8;
        t.syntax_function_macro = nord7;
        t.syntax_type = nord7;
        t.syntax_variable_builtin = nord9;
        t.syntax_variable_member = nord4;
        t.syntax_module = nord7;
        t.syntax_operator = nord9;
        t.syntax_tag = nord7;
        t.syntax_attribute = nord8;
        t.syntax_label = nord12;
        t.syntax_punctuation = nord4;
        t.syntax_default = nord4;

        t.graph_colors = [nord8, nord14, nord13, nord15, nord9, nord11, nord7, nord12];

        t.rebase_pick = nord14;
        t.rebase_reword = nord8;
        t.rebase_edit = nord13;
        t.rebase_squash = nord12;
        t.rebase_fixup = nord15;
        t.rebase_drop = nord11;
        t.rebase_paused_bg = Color::Rgb(50, 45, 25);

        t.change_added = nord14;
        t.change_deleted = nord11;
        t.change_renamed = nord13;
        t.change_copied = nord7;
        t.change_unmerged = nord11;

        t.ref_head = nord8;
        t.ref_remote = nord11;
        t.ref_local = nord14;
        t.ref_tag = nord7;

        t.tag_name = nord14;
        t.tag_hash = nord13;
        t.tag_message = nord4;

        t.stash_index = nord13;
        t.stash_message = nord4;

        t.reflog_hash = nord10;
        t.reflog_message = nord4;

        t.remote_name = nord7;
        t.remote_url = nord4;
        t.remote_branch_name = nord7;
        t.remote_branch_detail = nord3;
    })
}

fn solarized_dark() -> Theme {
    let base03 = Color::Rgb(0, 43, 54);
    let _base02 = Color::Rgb(7, 54, 66);
    let base01 = Color::Rgb(88, 110, 117);
    let base00 = Color::Rgb(101, 123, 131);
    let base0 = Color::Rgb(131, 148, 150);
    let base1 = Color::Rgb(147, 161, 161);
    let _base2 = Color::Rgb(238, 232, 213);
    let yellow = Color::Rgb(181, 137, 0);
    let orange = Color::Rgb(203, 75, 22);
    let red = Color::Rgb(220, 50, 47);
    let magenta = Color::Rgb(211, 54, 130);
    let violet = Color::Rgb(108, 113, 196);
    let blue = Color::Rgb(38, 139, 210);
    let cyan = Color::Rgb(42, 161, 152);
    let green = Color::Rgb(133, 153, 0);

    theme_from(|t| {
        t.active_border = Style::default().fg(blue).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(base01);
        t.selected_line = Style::default().bg(Color::Rgb(7, 54, 66));
        t.options_text = Style::default().fg(cyan);
        t.title = Style::default().fg(base1).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(base01);
        t.diff_add_bg = Color::Rgb(15, 50, 15);
        t.diff_remove_bg = Color::Rgb(50, 15, 15);
        t.diff_add_word = Color::Rgb(40, 90, 15);
        t.diff_remove_word = Color::Rgb(100, 25, 20);

        t.commit_hash = Style::default().fg(yellow);
        t.commit_author = Style::default().fg(blue);
        t.commit_date = Style::default().fg(cyan);
        t.commit_hash_pushed = base01;
        t.commit_hash_merged = Color::Rgb(60, 72, 77);

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(blue).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(green);
        t.file_unstaged = Style::default().fg(orange);
        t.file_untracked = Style::default().fg(cyan);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(base03);
        t.status_bar = Style::default().fg(base01);
        t.spinner = Style::default().fg(blue);

        t.accent = blue;
        t.accent_secondary = yellow;
        t.text_dimmed = base01;
        t.text = base0;
        t.text_strong = base1;
        t.separator = base01;
        t.selected_bg = Color::Rgb(7, 54, 66);
        t.popup_border = blue;

        t.cmd_log_border = base01;
        t.cmd_log_title = base0;
        t.cmd_log_hint = base01;
        t.cmd_log_text = base00;
        t.cmd_log_timestamp = base0;
        t.cmd_log_success = green;

        t.diff_gutter = base01;
        t.diff_line_number = Color::Rgb(7, 54, 66);
        t.diff_selection_fg = cyan;
        t.diff_selection_bg = Color::Rgb(10, 50, 55);
        t.diff_search_highlight_bg = Color::Rgb(90, 75, 15);
        t.diff_search_highlight_fg = base1;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = base03;
        t.diff_grid_bg = Color::Rgb(10, 50, 60);
        t.diff_grid_fg = yellow;

        t.syntax_comment = base01;
        t.syntax_keyword = green;
        t.syntax_string = cyan;
        t.syntax_number = magenta;
        t.syntax_function = blue;
        t.syntax_function_macro = orange;
        t.syntax_type = yellow;
        t.syntax_variable_builtin = orange;
        t.syntax_variable_member = base0;
        t.syntax_module = violet;
        t.syntax_operator = green;
        t.syntax_tag = blue;
        t.syntax_attribute = magenta;
        t.syntax_label = orange;
        t.syntax_punctuation = base0;
        t.syntax_default = base0;

        t.graph_colors = [blue, green, yellow, magenta, cyan, red, violet, orange];

        t.rebase_pick = green;
        t.rebase_reword = blue;
        t.rebase_edit = yellow;
        t.rebase_squash = orange;
        t.rebase_fixup = violet;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(40, 45, 15);

        t.change_added = green;
        t.change_deleted = red;
        t.change_renamed = yellow;
        t.change_copied = cyan;
        t.change_unmerged = red;

        t.ref_head = blue;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = cyan;

        t.tag_name = green;
        t.tag_hash = yellow;
        t.tag_message = base0;

        t.stash_index = yellow;
        t.stash_message = base0;

        t.reflog_hash = blue;
        t.reflog_message = base0;

        t.remote_name = cyan;
        t.remote_url = base0;
        t.remote_branch_name = cyan;
        t.remote_branch_detail = base01;
    })
}

fn onedark() -> Theme {
    let fg = Color::Rgb(171, 178, 191);
    let bg = Color::Rgb(40, 44, 52);
    let red = Color::Rgb(224, 108, 117);
    let green = Color::Rgb(152, 195, 121);
    let yellow = Color::Rgb(229, 192, 123);
    let blue = Color::Rgb(97, 175, 239);
    let magenta = Color::Rgb(198, 120, 221);
    let cyan = Color::Rgb(86, 182, 194);
    let gutter = Color::Rgb(76, 82, 99);
    let comment = Color::Rgb(92, 99, 112);
    let selection = Color::Rgb(62, 68, 81);

    theme_from(|t| {
        t.active_border = Style::default().fg(blue).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(gutter);
        t.selected_line = Style::default().bg(selection);
        t.options_text = Style::default().fg(cyan);
        t.title = Style::default().fg(fg).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(comment);
        t.diff_add_bg = Color::Rgb(25, 50, 25);
        t.diff_remove_bg = Color::Rgb(55, 25, 28);
        t.diff_add_word = Color::Rgb(45, 100, 45);
        t.diff_remove_word = Color::Rgb(110, 40, 45);

        t.commit_hash = Style::default().fg(yellow);
        t.commit_author = Style::default().fg(blue);
        t.commit_date = Style::default().fg(cyan);
        t.commit_hash_pushed = comment;
        t.commit_hash_merged = gutter;

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(blue).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(green);
        t.file_unstaged = Style::default().fg(yellow);
        t.file_untracked = Style::default().fg(cyan);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(bg);
        t.status_bar = Style::default().fg(comment);
        t.spinner = Style::default().fg(blue);

        t.accent = blue;
        t.accent_secondary = yellow;
        t.text_dimmed = comment;
        t.text = Color::Rgb(140, 148, 160);
        t.text_strong = fg;
        t.separator = gutter;
        t.selected_bg = selection;
        t.popup_border = blue;

        t.cmd_log_border = gutter;
        t.cmd_log_title = Color::Rgb(140, 148, 160);
        t.cmd_log_hint = comment;
        t.cmd_log_text = comment;
        t.cmd_log_timestamp = Color::Rgb(140, 148, 160);
        t.cmd_log_success = green;

        t.diff_gutter = comment;
        t.diff_line_number = gutter;
        t.diff_selection_fg = cyan;
        t.diff_selection_bg = Color::Rgb(30, 40, 50);
        t.diff_search_highlight_bg = Color::Rgb(110, 90, 25);
        t.diff_search_highlight_fg = fg;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = bg;
        t.diff_grid_bg = Color::Rgb(48, 52, 62);
        t.diff_grid_fg = yellow;

        t.syntax_comment = comment;
        t.syntax_keyword = magenta;
        t.syntax_string = green;
        t.syntax_number = yellow;
        t.syntax_function = blue;
        t.syntax_function_macro = cyan;
        t.syntax_type = yellow;
        t.syntax_variable_builtin = red;
        t.syntax_variable_member = red;
        t.syntax_module = yellow;
        t.syntax_operator = magenta;
        t.syntax_tag = red;
        t.syntax_attribute = yellow;
        t.syntax_label = cyan;
        t.syntax_punctuation = fg;
        t.syntax_default = fg;

        t.graph_colors = [blue, green, yellow, magenta, cyan, red, Color::Rgb(152, 195, 121), Color::Rgb(229, 192, 123)];

        t.rebase_pick = green;
        t.rebase_reword = blue;
        t.rebase_edit = yellow;
        t.rebase_squash = Color::Rgb(209, 154, 102);
        t.rebase_fixup = magenta;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(50, 45, 25);

        t.change_added = green;
        t.change_deleted = red;
        t.change_renamed = yellow;
        t.change_copied = cyan;
        t.change_unmerged = red;

        t.ref_head = blue;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = cyan;

        t.tag_name = green;
        t.tag_hash = yellow;
        t.tag_message = Color::Rgb(140, 148, 160);

        t.stash_index = yellow;
        t.stash_message = Color::Rgb(140, 148, 160);

        t.reflog_hash = blue;
        t.reflog_message = Color::Rgb(140, 148, 160);

        t.remote_name = cyan;
        t.remote_url = Color::Rgb(140, 148, 160);
        t.remote_branch_name = cyan;
        t.remote_branch_detail = comment;
    })
}

fn rosepine() -> Theme {
    let base = Color::Rgb(25, 23, 36);
    let surface = Color::Rgb(31, 29, 46);
    let overlay = Color::Rgb(38, 35, 58);
    let muted = Color::Rgb(110, 106, 134);
    let subtle = Color::Rgb(144, 140, 170);
    let text = Color::Rgb(224, 222, 244);
    let love = Color::Rgb(235, 111, 146);
    let gold = Color::Rgb(246, 193, 119);
    let rose = Color::Rgb(234, 154, 151);
    let pine = Color::Rgb(49, 116, 143);
    let foam = Color::Rgb(156, 207, 216);
    let iris = Color::Rgb(196, 167, 231);

    theme_from(|t| {
        t.active_border = Style::default().fg(iris).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(muted);
        t.selected_line = Style::default().bg(overlay);
        t.options_text = Style::default().fg(foam);
        t.title = Style::default().fg(text).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(foam);
        t.diff_remove = Style::default().fg(love);
        t.diff_context = Style::default().fg(muted);
        t.diff_add_bg = Color::Rgb(20, 45, 40);
        t.diff_remove_bg = Color::Rgb(50, 25, 30);
        t.diff_add_word = Color::Rgb(40, 90, 80);
        t.diff_remove_word = Color::Rgb(110, 40, 50);

        t.commit_hash = Style::default().fg(gold);
        t.commit_author = Style::default().fg(iris);
        t.commit_date = Style::default().fg(foam);
        t.commit_hash_pushed = muted;
        t.commit_hash_merged = overlay;

        t.branch_local = Style::default().fg(foam);
        t.branch_remote = Style::default().fg(love);
        t.branch_head = Style::default().fg(iris).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(foam);
        t.file_unstaged = Style::default().fg(gold);
        t.file_untracked = Style::default().fg(rose);
        t.file_conflicted = Style::default().fg(love).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(gold).fg(base);
        t.status_bar = Style::default().fg(muted);
        t.spinner = Style::default().fg(iris);

        t.accent = iris;
        t.accent_secondary = gold;
        t.text_dimmed = muted;
        t.text = subtle;
        t.text_strong = text;
        t.separator = overlay;
        t.selected_bg = overlay;
        t.popup_border = iris;

        t.cmd_log_border = overlay;
        t.cmd_log_title = subtle;
        t.cmd_log_hint = muted;
        t.cmd_log_text = muted;
        t.cmd_log_timestamp = subtle;
        t.cmd_log_success = foam;

        t.diff_gutter = muted;
        t.diff_line_number = overlay;
        t.diff_selection_fg = foam;
        t.diff_selection_bg = Color::Rgb(25, 35, 45);
        t.diff_search_highlight_bg = Color::Rgb(110, 85, 25);
        t.diff_search_highlight_fg = text;
        t.diff_search_cursor_bg = gold;
        t.diff_search_cursor_fg = base;
        t.diff_grid_bg = surface;
        t.diff_grid_fg = gold;

        t.syntax_comment = muted;
        t.syntax_keyword = pine;
        t.syntax_string = gold;
        t.syntax_number = iris;
        t.syntax_function = rose;
        t.syntax_function_macro = foam;
        t.syntax_type = foam;
        t.syntax_variable_builtin = love;
        t.syntax_variable_member = text;
        t.syntax_module = iris;
        t.syntax_operator = subtle;
        t.syntax_tag = foam;
        t.syntax_attribute = iris;
        t.syntax_label = gold;
        t.syntax_punctuation = subtle;
        t.syntax_default = text;

        t.graph_colors = [iris, foam, gold, love, pine, rose, Color::Rgb(156, 207, 216), Color::Rgb(196, 167, 231)];

        t.rebase_pick = foam;
        t.rebase_reword = iris;
        t.rebase_edit = gold;
        t.rebase_squash = rose;
        t.rebase_fixup = pine;
        t.rebase_drop = love;
        t.rebase_paused_bg = Color::Rgb(45, 40, 20);

        t.change_added = foam;
        t.change_deleted = love;
        t.change_renamed = gold;
        t.change_copied = iris;
        t.change_unmerged = love;

        t.ref_head = iris;
        t.ref_remote = love;
        t.ref_local = foam;
        t.ref_tag = pine;

        t.tag_name = foam;
        t.tag_hash = gold;
        t.tag_message = subtle;

        t.stash_index = gold;
        t.stash_message = subtle;

        t.reflog_hash = iris;
        t.reflog_message = subtle;

        t.remote_name = foam;
        t.remote_url = subtle;
        t.remote_branch_name = foam;
        t.remote_branch_detail = muted;
    })
}

fn kanagawa() -> Theme {
    let fg = Color::Rgb(220, 215, 186);
    let bg = Color::Rgb(31, 31, 40);
    let red = Color::Rgb(195, 64, 67);
    let green = Color::Rgb(118, 148, 106);
    let yellow = Color::Rgb(192, 163, 110);
    let blue = Color::Rgb(126, 156, 216);
    let magenta = Color::Rgb(149, 127, 184);
    let cyan = Color::Rgb(106, 149, 137);
    let orange = Color::Rgb(255, 160, 102);
    let comment = Color::Rgb(114, 113, 105);
    let selection = Color::Rgb(54, 54, 70);
    let line_nr = Color::Rgb(84, 84, 109);
    let spring_green = Color::Rgb(152, 187, 108);

    theme_from(|t| {
        t.active_border = Style::default().fg(blue).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(line_nr);
        t.selected_line = Style::default().bg(selection);
        t.options_text = Style::default().fg(cyan);
        t.title = Style::default().fg(fg).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(spring_green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(comment);
        t.diff_add_bg = Color::Rgb(25, 45, 25);
        t.diff_remove_bg = Color::Rgb(50, 22, 22);
        t.diff_add_word = Color::Rgb(50, 90, 45);
        t.diff_remove_word = Color::Rgb(100, 30, 30);

        t.commit_hash = Style::default().fg(yellow);
        t.commit_author = Style::default().fg(blue);
        t.commit_date = Style::default().fg(cyan);
        t.commit_hash_pushed = comment;
        t.commit_hash_merged = line_nr;

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(blue).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(spring_green);
        t.file_unstaged = Style::default().fg(orange);
        t.file_untracked = Style::default().fg(cyan);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(bg);
        t.status_bar = Style::default().fg(comment);
        t.spinner = Style::default().fg(blue);

        t.accent = blue;
        t.accent_secondary = yellow;
        t.text_dimmed = comment;
        t.text = Color::Rgb(168, 165, 150);
        t.text_strong = fg;
        t.separator = line_nr;
        t.selected_bg = selection;
        t.popup_border = blue;

        t.cmd_log_border = line_nr;
        t.cmd_log_title = Color::Rgb(168, 165, 150);
        t.cmd_log_hint = comment;
        t.cmd_log_text = comment;
        t.cmd_log_timestamp = Color::Rgb(168, 165, 150);
        t.cmd_log_success = spring_green;

        t.diff_gutter = comment;
        t.diff_line_number = line_nr;
        t.diff_selection_fg = cyan;
        t.diff_selection_bg = Color::Rgb(30, 38, 48);
        t.diff_search_highlight_bg = Color::Rgb(100, 85, 25);
        t.diff_search_highlight_fg = fg;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = bg;
        t.diff_grid_bg = Color::Rgb(40, 40, 52);
        t.diff_grid_fg = yellow;

        t.syntax_comment = comment;
        t.syntax_keyword = magenta;
        t.syntax_string = green;
        t.syntax_number = orange;
        t.syntax_function = blue;
        t.syntax_function_macro = cyan;
        t.syntax_type = yellow;
        t.syntax_variable_builtin = red;
        t.syntax_variable_member = fg;
        t.syntax_module = cyan;
        t.syntax_operator = red;
        t.syntax_tag = magenta;
        t.syntax_attribute = yellow;
        t.syntax_label = orange;
        t.syntax_punctuation = Color::Rgb(168, 165, 150);
        t.syntax_default = fg;

        t.graph_colors = [blue, spring_green, yellow, magenta, cyan, red, orange, green];

        t.rebase_pick = spring_green;
        t.rebase_reword = blue;
        t.rebase_edit = yellow;
        t.rebase_squash = orange;
        t.rebase_fixup = magenta;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(50, 42, 18);

        t.change_added = spring_green;
        t.change_deleted = red;
        t.change_renamed = yellow;
        t.change_copied = cyan;
        t.change_unmerged = red;

        t.ref_head = blue;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = cyan;

        t.tag_name = green;
        t.tag_hash = yellow;
        t.tag_message = Color::Rgb(168, 165, 150);

        t.stash_index = yellow;
        t.stash_message = Color::Rgb(168, 165, 150);

        t.reflog_hash = blue;
        t.reflog_message = Color::Rgb(168, 165, 150);

        t.remote_name = cyan;
        t.remote_url = Color::Rgb(168, 165, 150);
        t.remote_branch_name = cyan;
        t.remote_branch_detail = comment;
    })
}

fn everforest() -> Theme {
    let fg = Color::Rgb(211, 198, 170);
    let bg = Color::Rgb(47, 53, 57);
    let red = Color::Rgb(230, 126, 128);
    let green = Color::Rgb(167, 192, 128);
    let yellow = Color::Rgb(219, 188, 127);
    let blue = Color::Rgb(127, 187, 179);
    let purple = Color::Rgb(214, 153, 182);
    let aqua = Color::Rgb(131, 192, 146);
    let orange = Color::Rgb(230, 152, 117);
    let grey0 = Color::Rgb(122, 130, 120);
    let grey1 = Color::Rgb(134, 142, 132);
    let bg1 = Color::Rgb(55, 62, 67);
    let bg3 = Color::Rgb(68, 75, 80);

    theme_from(|t| {
        t.active_border = Style::default().fg(green).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(bg3);
        t.selected_line = Style::default().bg(bg1);
        t.options_text = Style::default().fg(blue);
        t.title = Style::default().fg(fg).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(grey0);
        t.diff_add_bg = Color::Rgb(30, 48, 28);
        t.diff_remove_bg = Color::Rgb(55, 28, 28);
        t.diff_add_word = Color::Rgb(50, 95, 45);
        t.diff_remove_word = Color::Rgb(110, 45, 45);

        t.commit_hash = Style::default().fg(yellow);
        t.commit_author = Style::default().fg(blue);
        t.commit_date = Style::default().fg(aqua);
        t.commit_hash_pushed = grey0;
        t.commit_hash_merged = bg3;

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(green).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(green);
        t.file_unstaged = Style::default().fg(orange);
        t.file_untracked = Style::default().fg(blue);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(bg);
        t.status_bar = Style::default().fg(grey0);
        t.spinner = Style::default().fg(green);

        t.accent = green;
        t.accent_secondary = yellow;
        t.text_dimmed = grey0;
        t.text = grey1;
        t.text_strong = fg;
        t.separator = bg3;
        t.selected_bg = bg1;
        t.popup_border = green;

        t.cmd_log_border = bg3;
        t.cmd_log_title = grey1;
        t.cmd_log_hint = grey0;
        t.cmd_log_text = grey0;
        t.cmd_log_timestamp = grey1;
        t.cmd_log_success = green;

        t.diff_gutter = grey0;
        t.diff_line_number = bg3;
        t.diff_selection_fg = blue;
        t.diff_selection_bg = Color::Rgb(30, 40, 45);
        t.diff_search_highlight_bg = Color::Rgb(105, 88, 25);
        t.diff_search_highlight_fg = fg;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = bg;
        t.diff_grid_bg = Color::Rgb(52, 58, 62);
        t.diff_grid_fg = yellow;

        t.syntax_comment = grey0;
        t.syntax_keyword = red;
        t.syntax_string = green;
        t.syntax_number = purple;
        t.syntax_function = aqua;
        t.syntax_function_macro = orange;
        t.syntax_type = yellow;
        t.syntax_variable_builtin = orange;
        t.syntax_variable_member = blue;
        t.syntax_module = aqua;
        t.syntax_operator = orange;
        t.syntax_tag = aqua;
        t.syntax_attribute = yellow;
        t.syntax_label = orange;
        t.syntax_punctuation = grey1;
        t.syntax_default = fg;

        t.graph_colors = [green, aqua, yellow, purple, blue, red, orange, Color::Rgb(131, 192, 146)];

        t.rebase_pick = green;
        t.rebase_reword = blue;
        t.rebase_edit = yellow;
        t.rebase_squash = orange;
        t.rebase_fixup = purple;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(50, 48, 22);

        t.change_added = green;
        t.change_deleted = red;
        t.change_renamed = yellow;
        t.change_copied = aqua;
        t.change_unmerged = red;

        t.ref_head = green;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = blue;

        t.tag_name = green;
        t.tag_hash = yellow;
        t.tag_message = grey1;

        t.stash_index = yellow;
        t.stash_message = grey1;

        t.reflog_hash = blue;
        t.reflog_message = grey1;

        t.remote_name = aqua;
        t.remote_url = grey1;
        t.remote_branch_name = aqua;
        t.remote_branch_detail = grey0;
    })
}

fn monokai() -> Theme {
    let fg = Color::Rgb(248, 248, 242);
    let bg = Color::Rgb(39, 40, 34);
    let red = Color::Rgb(249, 38, 114);
    let green = Color::Rgb(166, 226, 46);
    let yellow = Color::Rgb(230, 219, 116);
    let blue = Color::Rgb(102, 217, 239);
    let purple = Color::Rgb(174, 129, 255);
    let orange = Color::Rgb(253, 151, 31);
    let comment = Color::Rgb(117, 113, 94);
    let selection = Color::Rgb(73, 72, 62);
    let line_nr = Color::Rgb(90, 90, 80);

    theme_from(|t| {
        t.active_border = Style::default().fg(green).add_modifier(Modifier::BOLD);
        t.inactive_border = Style::default().fg(comment);
        t.selected_line = Style::default().bg(selection);
        t.options_text = Style::default().fg(blue);
        t.title = Style::default().fg(fg).add_modifier(Modifier::BOLD);

        t.diff_add = Style::default().fg(green);
        t.diff_remove = Style::default().fg(red);
        t.diff_context = Style::default().fg(comment);
        t.diff_add_bg = Color::Rgb(30, 55, 15);
        t.diff_remove_bg = Color::Rgb(55, 15, 30);
        t.diff_add_word = Color::Rgb(55, 110, 20);
        t.diff_remove_word = Color::Rgb(120, 15, 50);

        t.commit_hash = Style::default().fg(yellow);
        t.commit_author = Style::default().fg(blue);
        t.commit_date = Style::default().fg(purple);
        t.commit_hash_pushed = comment;
        t.commit_hash_merged = line_nr;

        t.branch_local = Style::default().fg(green);
        t.branch_remote = Style::default().fg(red);
        t.branch_head = Style::default().fg(green).add_modifier(Modifier::BOLD);

        t.file_staged = Style::default().fg(green);
        t.file_unstaged = Style::default().fg(orange);
        t.file_untracked = Style::default().fg(blue);
        t.file_conflicted = Style::default().fg(red).add_modifier(Modifier::BOLD);

        t.search_match = Style::default().bg(yellow).fg(bg);
        t.status_bar = Style::default().fg(comment);
        t.spinner = Style::default().fg(green);

        t.accent = green;
        t.accent_secondary = yellow;
        t.text_dimmed = comment;
        t.text = Color::Rgb(180, 180, 170);
        t.text_strong = fg;
        t.separator = Color::Rgb(73, 72, 62);
        t.selected_bg = selection;
        t.popup_border = green;

        t.cmd_log_border = Color::Rgb(73, 72, 62);
        t.cmd_log_title = Color::Rgb(180, 180, 170);
        t.cmd_log_hint = comment;
        t.cmd_log_text = comment;
        t.cmd_log_timestamp = Color::Rgb(180, 180, 170);
        t.cmd_log_success = green;

        t.diff_gutter = comment;
        t.diff_line_number = line_nr;
        t.diff_selection_fg = blue;
        t.diff_selection_bg = Color::Rgb(30, 40, 50);
        t.diff_search_highlight_bg = Color::Rgb(110, 100, 25);
        t.diff_search_highlight_fg = fg;
        t.diff_search_cursor_bg = yellow;
        t.diff_search_cursor_fg = bg;
        t.diff_grid_bg = Color::Rgb(48, 48, 40);
        t.diff_grid_fg = yellow;

        t.syntax_comment = comment;
        t.syntax_keyword = red;
        t.syntax_string = yellow;
        t.syntax_number = purple;
        t.syntax_function = green;
        t.syntax_function_macro = blue;
        t.syntax_type = blue;
        t.syntax_variable_builtin = orange;
        t.syntax_variable_member = fg;
        t.syntax_module = orange;
        t.syntax_operator = red;
        t.syntax_tag = red;
        t.syntax_attribute = green;
        t.syntax_label = orange;
        t.syntax_punctuation = fg;
        t.syntax_default = fg;

        t.graph_colors = [green, blue, yellow, purple, red, orange, Color::Rgb(102, 217, 239), Color::Rgb(166, 226, 46)];

        t.rebase_pick = green;
        t.rebase_reword = blue;
        t.rebase_edit = yellow;
        t.rebase_squash = orange;
        t.rebase_fixup = purple;
        t.rebase_drop = red;
        t.rebase_paused_bg = Color::Rgb(55, 52, 18);

        t.change_added = green;
        t.change_deleted = red;
        t.change_renamed = yellow;
        t.change_copied = blue;
        t.change_unmerged = red;

        t.ref_head = green;
        t.ref_remote = red;
        t.ref_local = green;
        t.ref_tag = blue;

        t.tag_name = green;
        t.tag_hash = yellow;
        t.tag_message = Color::Rgb(180, 180, 170);

        t.stash_index = yellow;
        t.stash_message = Color::Rgb(180, 180, 170);

        t.reflog_hash = purple;
        t.reflog_message = Color::Rgb(180, 180, 170);

        t.remote_name = blue;
        t.remote_url = Color::Rgb(180, 180, 170);
        t.remote_branch_name = blue;
        t.remote_branch_detail = comment;
    })
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
