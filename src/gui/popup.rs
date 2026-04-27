use anyhow::Result;
use tui_textarea::{CursorMove, TextArea};

use super::Gui;

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Reverse hard-wrapping in an externally-formatted commit body so it can be
/// loaded into a soft-wrapped editor without spurious mid-paragraph line breaks.
///
/// Convention: blank lines separate paragraphs; consecutive non-blank lines
/// inside a paragraph are joined back into one logical line. Used when loading
/// AI-generated messages, clipboard pastes via the menu, and history entries.
pub fn unwrap_commit_body(text: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
            // Multiple blank lines collapse into one paragraph break.
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }
    paragraphs.join("\n\n")
}

/// Source-of-truth for the commit body when soft-wrap is in effect. The body
/// textarea becomes a *display* of this raw text — wrap-induced newlines never
/// touch the actual commit message, but user-pressed newlines (Shift+Enter) do.
///
/// Cursor is a char index into `raw` (not bytes) so multi-byte input is safe.
#[derive(Debug, Default, Clone)]
pub struct BodySoftWrap {
    pub raw: String,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
struct WrapLine {
    text: String,
    /// Char index in raw where this visual line starts.
    raw_start: usize,
    /// Number of raw chars covered by this line (excluding any space/newline
    /// consumed by the wrap break that follows).
    char_len: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct WrapLayout {
    lines: Vec<WrapLine>,
}

impl WrapLayout {
    fn build(raw: &str, wrap_width: usize) -> Self {
        let mut lines = Vec::new();
        let mut para_start = 0usize;
        let paragraphs: Vec<&str> = raw.split('\n').collect();
        let total_paragraphs = paragraphs.len();
        for (p_idx, para) in paragraphs.iter().enumerate() {
            let chars: Vec<char> = para.chars().collect();
            if chars.is_empty() {
                lines.push(WrapLine { text: String::new(), raw_start: para_start, char_len: 0 });
            } else if wrap_width == 0 {
                lines.push(WrapLine {
                    text: para.to_string(),
                    raw_start: para_start,
                    char_len: chars.len(),
                });
            } else {
                let mut start = 0usize;
                while start < chars.len() {
                    let remaining = chars.len() - start;
                    if remaining <= wrap_width {
                        let text: String = chars[start..].iter().collect();
                        lines.push(WrapLine {
                            text,
                            raw_start: para_start + start,
                            char_len: remaining,
                        });
                        start = chars.len();
                    } else {
                        let window = &chars[start..start + wrap_width];
                        let break_at = window.iter().rposition(|c| *c == ' ');
                        let (line_end, consumed) = match break_at {
                            Some(0) | None => (start + wrap_width, 0),
                            Some(i) => (start + i, 1),
                        };
                        let text: String = chars[start..line_end].iter().collect();
                        let len = line_end - start;
                        lines.push(WrapLine {
                            text,
                            raw_start: para_start + start,
                            char_len: len,
                        });
                        start = line_end + consumed;
                    }
                }
            }
            // Advance past this paragraph's chars + the \n separator (except after the last).
            para_start += chars.len();
            if p_idx + 1 < total_paragraphs {
                para_start += 1;
            }
        }
        if lines.is_empty() {
            lines.push(WrapLine { text: String::new(), raw_start: 0, char_len: 0 });
        }
        WrapLayout { lines }
    }

    fn cursor_to_visual(&self, cursor: usize) -> (usize, usize) {
        for (i, line) in self.lines.iter().enumerate() {
            let line_end = line.raw_start + line.char_len;
            // Cursor falls inside this line (raw_start..=line_end). The
            // end-of-line position belongs to THIS line, not the next — that
            // way `move_visual_up` can land here and stay (otherwise it would
            // bounce forward to the next row, getting stuck).
            if cursor >= line.raw_start && cursor <= line_end {
                return (i, cursor - line.raw_start);
            }
            // Cursor is in the gap between this line's end and the next line's
            // start (a space or \n consumed by the wrap). Snap to start of next.
            if i + 1 < self.lines.len() && cursor < self.lines[i + 1].raw_start {
                return (i + 1, 0);
            }
        }
        let last = self.lines.len() - 1;
        let line = &self.lines[last];
        (last, cursor.saturating_sub(line.raw_start).min(line.char_len))
    }

    pub fn as_textarea_text(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

impl BodySoftWrap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_text(text: impl Into<String>) -> Self {
        let raw = text.into();
        let cursor = raw.chars().count();
        Self { raw, cursor }
    }

    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    fn cursor_byte(&self) -> usize {
        self.raw
            .char_indices()
            .nth(self.cursor)
            .map(|(b, _)| b)
            .unwrap_or(self.raw.len())
    }

    fn char_count(&self) -> usize {
        self.raw.chars().count()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        let raw = text.into();
        self.cursor = raw.chars().count();
        self.raw = raw;
    }

    pub fn clear(&mut self) {
        self.raw.clear();
        self.cursor = 0;
    }

    pub fn insert_char(&mut self, c: char) {
        let b = self.cursor_byte();
        self.raw.insert(b, c);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        let b = self.cursor_byte();
        self.raw.insert_str(b, s);
        self.cursor += s.chars().count();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.raw.char_indices().nth(self.cursor - 1).map(|(b, _)| b).unwrap();
        self.raw.remove(prev);
        self.cursor -= 1;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.char_count() {
            return;
        }
        let b = self.cursor_byte();
        self.raw.remove(b);
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_count());
    }

    /// Move cursor to the start of the previous word (emacs/readline-style:
    /// skip preceding non-word chars, then skip word chars).
    pub fn move_word_left(&mut self) {
        let chars: Vec<char> = self.raw.chars().collect();
        let mut i = self.cursor;
        while i > 0 && !is_word_char(chars[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word_char(chars[i - 1]) {
            i -= 1;
        }
        self.cursor = i;
    }

    /// Move cursor past the end of the next word.
    pub fn move_word_right(&mut self) {
        let chars: Vec<char> = self.raw.chars().collect();
        let n = chars.len();
        let mut i = self.cursor;
        while i < n && !is_word_char(chars[i]) {
            i += 1;
        }
        while i < n && is_word_char(chars[i]) {
            i += 1;
        }
        self.cursor = i;
    }

    /// Delete from cursor back to the start of the previous word.
    pub fn delete_word_left(&mut self) {
        let end = self.cursor;
        self.move_word_left();
        let start = self.cursor;
        if start == end {
            return;
        }
        let start_byte = self
            .raw
            .char_indices()
            .nth(start)
            .map(|(b, _)| b)
            .unwrap_or(self.raw.len());
        let end_byte = self
            .raw
            .char_indices()
            .nth(end)
            .map(|(b, _)| b)
            .unwrap_or(self.raw.len());
        self.raw.replace_range(start_byte..end_byte, "");
    }

    pub fn move_home(&mut self) {
        let chars: Vec<char> = self.raw.chars().collect();
        let mut i = self.cursor;
        while i > 0 && chars[i - 1] != '\n' {
            i -= 1;
        }
        self.cursor = i;
    }

    pub fn move_end(&mut self) {
        let chars: Vec<char> = self.raw.chars().collect();
        let mut i = self.cursor;
        while i < chars.len() && chars[i] != '\n' {
            i += 1;
        }
        self.cursor = i;
    }

    pub fn move_visual_up(&mut self, wrap_width: usize) {
        let layout = WrapLayout::build(&self.raw, wrap_width);
        let (row, col) = layout.cursor_to_visual(self.cursor);
        if row == 0 {
            self.cursor = 0;
            return;
        }
        let target = &layout.lines[row - 1];
        self.cursor = target.raw_start + col.min(target.char_len);
    }

    pub fn move_visual_down(&mut self, wrap_width: usize) {
        let layout = WrapLayout::build(&self.raw, wrap_width);
        let (row, col) = layout.cursor_to_visual(self.cursor);
        if row + 1 >= layout.line_count() {
            self.cursor = self.char_count();
            return;
        }
        let target = &layout.lines[row + 1];
        self.cursor = target.raw_start + col.min(target.char_len);
    }

    /// Re-render `textarea` to display the current raw text soft-wrapped at
    /// `wrap_width`, and place the visual cursor where it logically belongs.
    ///
    /// We rebuild the textarea from scratch (rather than mutating in place)
    /// because tui_textarea's internal viewport/scroll state can get stuck
    /// past the end of content after a terminal resize. A fresh TextArea
    /// always starts with a clean viewport.
    pub fn render_into(&self, textarea: &mut TextArea<'static>, wrap_width: usize) {
        let layout = WrapLayout::build(&self.raw, wrap_width.max(1));
        let lines: Vec<String> = layout.lines.iter().map(|l| l.text.clone()).collect();
        let (row, col) = layout.cursor_to_visual(self.cursor);

        // Preserve existing visual styling so focus/cursor cues survive the rebuild.
        let cursor_style = textarea.cursor_style();
        let cursor_line_style = textarea.cursor_line_style();
        let placeholder_text = textarea.placeholder_text().to_string();
        let placeholder_style = textarea.placeholder_style();
        let style = textarea.style();

        let mut new_ta = TextArea::new(lines);
        new_ta.set_cursor_style(cursor_style);
        new_ta.set_cursor_line_style(cursor_line_style);
        new_ta.set_placeholder_text(placeholder_text);
        if let Some(s) = placeholder_style {
            new_ta.set_placeholder_style(s);
        }
        new_ta.set_style(style);
        new_ta.move_cursor(CursorMove::Jump(row as u16, col as u16));

        *textarea = new_ta;
    }
}

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
        /// Source-of-truth for body content. `body_textarea` is a soft-wrapped
        /// view of this string. All body edits flow through here so wrap-induced
        /// line breaks never end up in the actual commit message.
        body_state: BodySoftWrap,
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
