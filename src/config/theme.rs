use ratatui::style::{Color, Modifier, Style};
use super::user_config::ThemeConfig;

#[derive(Debug, Clone)]
pub struct Theme {
    pub active_border: Style,
    pub inactive_border: Style,
    pub selected_line: Style,
    pub options_text: Style,
    pub title: Style,
    pub diff_add: Style,
    pub diff_remove: Style,
    pub diff_context: Style,
    pub diff_add_bg: Color,
    pub diff_remove_bg: Color,
    pub diff_add_word: Color,
    pub diff_remove_word: Color,
    pub commit_hash: Style,
    pub commit_author: Style,
    pub commit_date: Style,
    pub branch_local: Style,
    pub branch_remote: Style,
    pub branch_head: Style,
    pub file_staged: Style,
    pub file_unstaged: Style,
    pub file_untracked: Style,
    pub file_conflicted: Style,
    pub search_match: Style,
    pub status_bar: Style,
    pub spinner: Style,
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
            status_bar: Style::default().fg(Color::White).bg(Color::DarkGray),
            spinner: Style::default().fg(Color::Cyan),
        }
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
