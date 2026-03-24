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
    },
    Menu {
        title: String,
        items: Vec<MenuItem>,
        selected: usize,
    },
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
