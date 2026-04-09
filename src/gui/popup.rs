use anyhow::Result;
use tui_textarea::TextArea;

use super::Gui;

pub type ConfirmAction = Box<dyn FnOnce(&mut Gui) -> Result<()>>;
pub type InputAction = Box<dyn FnOnce(&mut Gui, &str) -> Result<()>>;
pub type MenuAction = Box<dyn Fn(&mut Gui) -> Result<()>>;

/// Result sent back from a menu item's background operation.
pub enum MenuAsyncResult {
    /// Copy the string to the clipboard.
    CopyToClipboard(String),
    /// Open the string as a URL/file.
    OpenUrl(String),
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Error,
    Info,
}

/// Which field is focused in the commit input popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitInputFocus {
    Summary,
    Body,
}

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
        /// When true, focus is on the Confirm button instead of the textarea.
        confirm_focused: bool,
    },
    /// Two-field commit message editor (summary + body), like lazygit.
    CommitInput {
        summary_textarea: TextArea<'static>,
        body_textarea: TextArea<'static>,
        focus: CommitInputFocus,
        on_confirm: InputAction,
    },
    Menu {
        title: String,
        items: Vec<MenuItem>,
        selected: usize,
        /// When set, this menu item index is running an async operation (shows inline spinner).
        loading_index: Option<usize>,
    },
    /// Informational or error message — dismissed by any key press.
    Message {
        title: String,
        message: String,
        kind: MessageKind,
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
        search_textarea: TextArea<'static>,
        scroll_offset: usize,
    },
    /// Searchable ref picker (branches, tags, commits) with a callback.
    RefPicker {
        title: String,
        core: ListPickerCore,
        on_confirm: ListPickerAction,
    },
    /// Color theme picker with live preview and search.
    ThemePicker {
        core: ListPickerCore,
        /// The theme index before opening the picker (for cancel/revert).
        original_theme_index: usize,
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

pub fn make_commit_summary_textarea() -> TextArea<'static> {
    make_textarea("Required")
}

pub fn make_commit_body_textarea() -> TextArea<'static> {
    let mut ta = make_textarea("Optional");
    // Body starts unfocused — hide cursor
    ta.set_cursor_style(ratatui::style::Style::default());
    ta
}

pub fn make_help_search_textarea() -> TextArea<'static> {
    use ratatui::style::{Color, Style};

    let mut ta = make_textarea("Type to filter...");
    ta.set_style(Style::default().fg(Color::Yellow));
    ta.set_cursor_style(Style::default().fg(Color::Yellow).add_modifier(ratatui::style::Modifier::REVERSED));
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

pub type ListPickerAction = Box<dyn FnOnce(&mut Gui, &str) -> Result<()>>;

#[derive(Debug, Clone)]
pub struct ListPickerItem {
    /// The value to pass to the callback (ref name, hash, theme id, etc.).
    pub value: String,
    /// Display label shown in the list.
    pub label: String,
    /// Section/category header (e.g. "Branches", "Tags"). Empty for flat lists.
    pub category: String,
}

/// Shared state for searchable list picker popups (RefPicker, ThemePicker, etc.).
pub struct ListPickerCore {
    pub items: Vec<ListPickerItem>,
    pub selected: usize,
    pub search_textarea: TextArea<'static>,
    pub scroll_offset: usize,
}
