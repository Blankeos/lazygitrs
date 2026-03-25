use anyhow::Result;
use tui_textarea::TextArea;

use super::Gui;

pub type ConfirmAction = Box<dyn FnOnce(&mut Gui) -> Result<()>>;
pub type InputAction = Box<dyn FnOnce(&mut Gui, &str) -> Result<()>>;
pub type MenuAction = Box<dyn Fn(&mut Gui) -> Result<()>>;

pub enum PopupState {
    None,
    Confirm {
        title: String,
        message: String,
        on_confirm: ConfirmAction,
    },
    Input {
        title: String,
        textarea: TextArea<'static>,
        on_confirm: InputAction,
        /// When true, this is a commit message editor — enables AI generation via <c-g>.
        #[allow(dead_code)]
        is_commit: bool,
    },
    Menu {
        title: String,
        items: Vec<MenuItem>,
        selected: usize,
    },
    /// Informational message — dismissed by any key press.
    Message {
        title: String,
        message: String,
    },
    /// Shown while a background operation (like AI commit generation) is running.
    Loading {
        title: String,
        message: String,
    },
    /// Multi-select checklist with search filter.
    Checklist {
        title: String,
        items: Vec<ChecklistItem>,
        selected: usize,
        search: String,
        on_confirm: ChecklistAction,
    },
    /// Keybinding help overlay with integrated search.
    Help {
        sections: Vec<HelpSection>,
        selected: usize,
        search: String,
        scroll_offset: usize,
    },
}

pub type ChecklistAction = Box<dyn FnOnce(&mut Gui, Vec<String>) -> Result<()>>;

pub struct ChecklistItem {
    pub label: String,
    pub checked: bool,
}

impl PartialEq for PopupState {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (PopupState::None, PopupState::None)
        )
    }
}

pub fn make_textarea(placeholder: &str) -> TextArea<'static> {
    use ratatui::style::{Color, Style};

    let mut ta = TextArea::default();
    ta.set_placeholder_text(placeholder);
    ta.set_cursor_line_style(Style::default());
    ta.set_placeholder_style(Style::default().fg(Color::DarkGray));
    ta
}

pub struct MenuItem {
    pub label: String,
    pub description: String,
    pub key: Option<String>,
    pub action: Option<MenuAction>,
}

pub struct HelpSection {
    pub title: String,
    pub entries: Vec<HelpEntry>,
}

pub struct HelpEntry {
    pub key: String,
    pub description: String,
}
