use anyhow::Result;
use crossterm::event::KeyEvent;
use tui_textarea::{CursorMove, TextArea};
use unicode_width::UnicodeWidthChar;

use super::Gui;

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Synchronize a free-entry row and keep it selected. Used where the typed
/// value is valid on its own and suggestions are optional completions.
pub fn sync_list_picker_prefer_free_entry(core: &mut ListPickerCore, free_entry_category: &str) {
    sync_list_picker_free_entry(core, free_entry_category);
    if !core.search_textarea.lines().join("").trim().is_empty() {
        core.selected = 0;
    }
}

/// Normalize an externally-sourced commit body for the soft-wrapped editor.
///
/// Soft-wrap is display-only (`BodySoftWrap` / `WrapLayout`), so logical
/// newlines from AI output, clipboard pastes, and history must be preserved.
/// Joining consecutive lines used to collapse bullet lists into one line
/// (`- a\n- b` → `- a - b`); we only normalize `\r\n` / trailing `\r`.
pub fn unwrap_commit_body(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Source-of-truth for the commit body when soft-wrap is in effect. The body
/// textarea becomes a *display* of this raw text — wrap-induced newlines never
/// touch the actual commit message, but user-pressed newlines (Enter on Body,
/// or Shift+Enter from Summary) do.
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
        let wrap_width = wrap_width.max(1);
        for (p_idx, para) in paragraphs.iter().enumerate() {
            let chars: Vec<char> = para.chars().collect();
            if chars.is_empty() {
                lines.push(WrapLine {
                    text: String::new(),
                    raw_start: para_start,
                    char_len: 0,
                });
            } else {
                let mut start = 0usize;
                while start < chars.len() {
                    // Grow by display width (crabcode-style), not char count,
                    // so wide glyphs don't force horizontal scroll.
                    let mut end = start;
                    let mut width = 0usize;
                    let mut last_space: Option<usize> = None;
                    while end < chars.len() {
                        let ch = chars[end];
                        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                        if end > start && width + ch_width > wrap_width {
                            break;
                        }
                        if ch == ' ' {
                            last_space = Some(end);
                        }
                        width += ch_width;
                        end += 1;
                        if width >= wrap_width {
                            break;
                        }
                    }
                    if end == start {
                        // Pathological zero-width / over-wide glyph: advance one char.
                        end = (start + 1).min(chars.len());
                    }

                    let at_end = end >= chars.len();
                    let (line_end, consumed) = if at_end {
                        (end, 0)
                    } else {
                        match last_space {
                            Some(i) if i > start => (i, 1),
                            _ => (end, 0),
                        }
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
            // Advance past this paragraph's chars + the \n separator (except after the last).
            para_start += chars.len();
            if p_idx + 1 < total_paragraphs {
                para_start += 1;
            }
        }
        if lines.is_empty() {
            lines.push(WrapLine {
                text: String::new(),
                raw_start: 0,
                char_len: 0,
            });
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
        (
            last,
            cursor.saturating_sub(line.raw_start).min(line.char_len),
        )
    }

    fn visual_to_cursor(&self, row: usize, col: usize) -> usize {
        let line = self
            .lines
            .get(row)
            .or_else(|| self.lines.last())
            .expect("wrap layout always has at least one line");
        line.raw_start + col.min(line.char_len)
    }

    pub fn as_textarea_text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

fn parse_command_key(key: &str) -> Option<KeyEvent> {
    let normalized = match key {
        "Enter" => "<enter>",
        "Tab" => "<tab>",
        "esc" | "Esc" => "<esc>",
        "Alt+↑" => "<a-k>",
        "Alt+↓" => "<a-j>",
        _ => key,
    };
    crate::config::keybindings::parse_key(normalized)
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
        let prev = self
            .raw
            .char_indices()
            .nth(self.cursor - 1)
            .map(|(b, _)| b)
            .unwrap();
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

    /// Cmd+Left equivalent: jump to the start of the current visual row,
    /// respecting soft-wrap boundaries (not just paragraph boundaries).
    pub fn move_visual_line_start(&mut self, wrap_width: usize) {
        let layout = WrapLayout::build(&self.raw, wrap_width.max(1));
        let (row, _) = layout.cursor_to_visual(self.cursor);
        self.cursor = layout.lines[row].raw_start;
    }

    /// Cmd+Right equivalent: jump to the end of the current visual row.
    pub fn move_visual_line_end(&mut self, wrap_width: usize) {
        let layout = WrapLayout::build(&self.raw, wrap_width.max(1));
        let (row, _) = layout.cursor_to_visual(self.cursor);
        let line = &layout.lines[row];
        self.cursor = line.raw_start + line.char_len;
    }

    /// Cmd+Backspace equivalent: delete from cursor back to the start of the
    /// current visual row. Stops at the row boundary so a single chord doesn't
    /// nuke the whole paragraph.
    pub fn delete_to_visual_line_start(&mut self, wrap_width: usize) {
        let layout = WrapLayout::build(&self.raw, wrap_width.max(1));
        let (row, _) = layout.cursor_to_visual(self.cursor);
        let start = layout.lines[row].raw_start;
        let end = self.cursor;
        if start >= end {
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
        self.cursor = start;
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

    pub fn set_cursor_from_visual(&mut self, row: usize, col: usize, wrap_width: usize) {
        let layout = WrapLayout::build(&self.raw, wrap_width.max(1));
        self.cursor = layout.visual_to_cursor(row, col);
    }

    /// Place the visual cursor on an already-projected `textarea` without
    /// rebuilding it. Keeps the existing viewport so Up/Down only scroll when
    /// the cursor would leave the visible area (browser-textarea behavior).
    pub fn apply_cursor_into(&self, textarea: &mut TextArea<'static>, wrap_width: usize) {
        let layout = WrapLayout::build(&self.raw, wrap_width.max(1));
        let (row, col) = layout.cursor_to_visual(self.cursor);
        textarea.move_cursor(CursorMove::Jump(row as u16, col as u16));
    }

    /// Re-render `textarea` to display the current raw text soft-wrapped at
    /// `wrap_width`, and place the visual cursor where it logically belongs.
    ///
    /// We rebuild the textarea from scratch (rather than mutating in place)
    /// because tui_textarea's internal viewport/scroll state can get stuck
    /// past the end of content after a terminal resize. A fresh TextArea
    /// always starts with a clean viewport. Prefer [`Self::apply_cursor_into`]
    /// for pure cursor moves so scroll is preserved.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitInputKind {
    Commit,
    Reword,
}

impl CommitInputKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Commit => "Commit message",
            Self::Reword => "Reword commit",
        }
    }
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
        kind: CommitInputKind,
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
    #[allow(dead_code)]
    Loading {
        title: String,
        message: String,
    },
    /// Multi-select checklist with search filter.
    Checklist {
        title: String,
        items: Vec<ChecklistItem>,
        selected: usize,
        search_textarea: TextArea<'static>,
        /// When present, non-empty search text is also a checkable custom item.
        free_entry_category: Option<String>,
        on_confirm: ChecklistAction,
    },
    /// Searchable command palette with keybinding hints.
    CommandPalette {
        sections: Vec<CommandSection>,
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
    /// Generic searchable list picker with free-text entry (path/author filters, etc.).
    /// Modeled after [`PopupState::RefPicker`] but with a configurable free-entry category.
    ListPicker {
        title: String,
        core: ListPickerCore,
        /// Category label for the synthetic free-entry row (e.g. `"[path]"`, `"[author]"`).
        free_entry_category: String,
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
    pub is_free_entry: bool,
}

/// Keep a free-entry checklist row in sync with the current search text.
///
/// When `free_entry_category` is set and the search box is non-empty, a
/// synthetic item whose label is the typed text is inserted at the top so
/// users can multi-select arbitrary values (authors) just like known ones.
pub fn sync_checklist_free_entry(
    items: &mut Vec<ChecklistItem>,
    free_entry_category: Option<&str>,
    search: &str,
) {
    let previously_checked = items
        .iter()
        .find(|item| item.is_free_entry)
        .map(|item| (item.label.clone(), item.checked));
    items.retain(|item| !item.is_free_entry);
    if free_entry_category.is_none() {
        return;
    }
    let search = search.trim();
    if search.is_empty() {
        return;
    }
    if items.iter().any(|item| item.label == search) {
        return;
    }
    let checked = previously_checked
        .as_ref()
        .is_some_and(|(label, checked)| label == search && *checked);
    items.insert(
        0,
        ChecklistItem {
            label: search.to_string(),
            checked,
            is_free_entry: true,
        },
    );
}

impl PartialEq for PopupState {
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other), (PopupState::None, PopupState::None))
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

/// Replace the summary textarea contents with `text`, cursor at end.
///
/// Always rebuilds a fresh `TextArea` so horizontal `scroll_top` is reset.
/// Reusing select_all/cut/insert_str can leave a stale scroll past the new
/// end (tui-textarea's `next_scroll_top` then clamps to `cursor`, showing
/// empty space to the right of the text).
pub fn set_commit_summary_text(textarea: &mut TextArea<'static>, text: &str) {
    let mut fresh = make_commit_summary_textarea();
    // Preserve focus/cursor styling from the existing widget.
    fresh.set_cursor_style(textarea.cursor_style());
    fresh.set_cursor_line_style(textarea.cursor_line_style());
    fresh.set_style(textarea.style());
    if !text.is_empty() {
        fresh.insert_str(text);
    }
    *textarea = fresh;
}

pub fn make_commit_body_textarea() -> TextArea<'static> {
    let mut ta = make_textarea("Optional");
    // Body starts unfocused — hide cursor
    ta.set_cursor_style(ratatui::style::Style::default());
    ta
}

pub fn make_command_palette_search_textarea() -> TextArea<'static> {
    use ratatui::style::{Color, Style};

    let mut ta = make_textarea("Search commands or keybindings...");
    ta.set_style(Style::default().fg(Color::Yellow));
    ta.set_cursor_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(ratatui::style::Modifier::REVERSED),
    );
    ta
}

pub fn make_checklist_search_textarea() -> TextArea<'static> {
    use ratatui::style::{Color, Style};

    let mut ta = make_textarea("Filter...");
    ta.set_style(Style::default().fg(Color::Yellow));
    ta.set_cursor_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(ratatui::style::Modifier::REVERSED),
    );
    ta
}

pub struct MenuItem {
    pub label: String,
    pub description: String,
    pub key: Option<String>,
    pub action: Option<MenuAction>,
}

pub struct CommandSection {
    pub title: String,
    pub entries: Vec<CommandEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    Dispatch(KeyEvent),
    OpenThemePicker,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub key: String,
    pub description: String,
    pub action: CommandAction,
}

impl CommandEntry {
    pub fn keybinding(key: String, description: String) -> Self {
        let action = parse_command_key(&key)
            .map(CommandAction::Dispatch)
            .unwrap_or(CommandAction::Unavailable);
        Self {
            key,
            description: description.into(),
            action,
        }
    }

    pub fn action(key: String, description: String, action: CommandAction) -> Self {
        Self {
            key,
            description,
            action,
        }
    }

    pub fn is_executable(&self) -> bool {
        self.action != CommandAction::Unavailable
    }
}

#[cfg(test)]
mod commit_body_wrap_tests {
    use super::{BodySoftWrap, WrapLayout, unwrap_commit_body};

    #[test]
    fn unwrap_preserves_newlines_and_bullets() {
        let raw = "- something\n- something 2\n\nparagraph\n";
        assert_eq!(
            unwrap_commit_body(raw),
            "- something\n- something 2\n\nparagraph\n"
        );
    }

    #[test]
    fn unwrap_normalizes_crlf() {
        assert_eq!(unwrap_commit_body("a\r\nb\rc"), "a\nb\nc");
    }

    #[test]
    fn wrap_layout_keeps_logical_newlines_as_rows() {
        let layout = WrapLayout::build("- something\n- something 2", 80);
        assert_eq!(layout.line_count(), 2);
        assert_eq!(layout.as_textarea_text(), "- something\n- something 2");
    }

    #[test]
    fn wrap_layout_breaks_on_display_width_not_char_count() {
        // Two wide chars (width 2 each) should wrap before a third at width 4.
        let layout = WrapLayout::build("あああ", 4);
        assert_eq!(layout.line_count(), 2);
        assert_eq!(layout.as_textarea_text(), "ああ\nあ");
    }

    #[test]
    fn soft_wrap_preserves_raw_newlines_while_rendering() {
        let mut state = BodySoftWrap::new();
        state.set_text("- something\n- something 2");
        let mut ta = super::make_commit_body_textarea();
        state.render_into(&mut ta, 80);
        assert_eq!(state.raw(), "- something\n- something 2");
        assert_eq!(ta.lines().join("\n"), "- something\n- something 2");
    }
}

#[cfg(test)]
mod command_entry_tests {
    use super::{CommandAction, CommandEntry};
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn single_key_binding_is_executable() {
        let entry = CommandEntry::keybinding("x".into(), "Delete".into());

        assert!(entry.is_executable());
        assert!(matches!(
            entry.action,
            CommandAction::Dispatch(key)
                if key.code == KeyCode::Char('x') && key.modifiers == KeyModifiers::NONE
        ));
    }

    #[test]
    fn compound_key_hint_is_not_executable() {
        let entry = CommandEntry::keybinding("j/k".into(), "Navigate".into());

        assert!(!entry.is_executable());
        assert_eq!(entry.action, CommandAction::Unavailable);
    }

    #[test]
    fn explicit_action_is_executable_without_a_keybinding() {
        let entry = CommandEntry::action(
            String::new(),
            "Color theme...".into(),
            CommandAction::OpenThemePicker,
        );

        assert!(entry.is_executable());
    }

    #[test]
    fn display_key_label_is_executable() {
        let entry = CommandEntry::keybinding("Tab".into(), "Next panel".into());

        assert!(matches!(
            entry.action,
            CommandAction::Dispatch(key) if key.code == KeyCode::Tab
        ));
    }
}

pub type ListPickerAction = Box<dyn FnOnce(&mut Gui, &str) -> Result<()>>;

/// Category used by [`PopupState::RefPicker`] for the synthetic free-entry row.
pub const REF_FREE_ENTRY_CATEGORY: &str = "[ref]";

#[derive(Debug, Clone)]
pub struct ListPickerItem {
    /// The value to pass to the callback (ref name, hash, theme id, etc.).
    pub value: String,
    /// Display label shown in the list.
    pub label: String,
    /// Section/category header (e.g. "Branches", "Tags"). Empty for flat lists.
    pub category: String,
    /// Optional right-aligned dimmed label (e.g. "light" / "dark" for themes).
    pub description: Option<String>,
}

/// Shared state for searchable list picker popups (RefPicker, ThemePicker, etc.).
pub struct ListPickerCore {
    pub items: Vec<ListPickerItem>,
    pub selected: usize,
    pub search_textarea: TextArea<'static>,
    pub scroll_offset: usize,
}

/// True when index 0 is the synthetic free-entry row for `free_entry_category`.
pub fn is_free_entry_item(items: &[ListPickerItem], free_entry_category: &str) -> bool {
    !items.is_empty() && items[0].category == free_entry_category
}

/// Remove the synthetic free-entry row at index 0 if present.
pub fn remove_free_entry_item(items: &mut Vec<ListPickerItem>, free_entry_category: &str) {
    if is_free_entry_item(items, free_entry_category) {
        items.remove(0);
    }
}

/// After the search textarea changes, sync the free-entry synthetic item and
/// update selection to the first matching real item (or the free-entry row).
///
/// Scroll offset is left to the caller when matches exist (key vs paste differ);
/// when search is cleared, `scroll_offset` is reset to 0.
pub fn sync_list_picker_free_entry(core: &mut ListPickerCore, free_entry_category: &str) {
    let new_search = core.search_textarea.lines().join("");
    remove_free_entry_item(&mut core.items, free_entry_category);

    let new_lower = new_search.to_lowercase();
    if !new_lower.is_empty() {
        let trimmed = new_search.trim().to_string();
        core.items.insert(
            0,
            ListPickerItem {
                value: trimmed.clone(),
                label: trimmed,
                category: free_entry_category.to_string(),
                description: None,
            },
        );

        if let Some(idx) = core.items.iter().skip(1).position(|i| {
            i.label.to_lowercase().contains(&new_lower)
                || i.value.to_lowercase().contains(&new_lower)
        }) {
            core.selected = idx + 1;
        } else {
            core.selected = 0;
        }
    } else {
        core.selected = 0;
        core.scroll_offset = 0;
    }
}

/// Whether a list-picker row matches the current search (empty search = all).
pub fn list_picker_item_matches(item: &ListPickerItem, search_lower: &str) -> bool {
    if search_lower.is_empty() {
        return true;
    }
    item.label.to_lowercase().contains(search_lower)
        || item.value.to_lowercase().contains(search_lower)
}

/// Indices into `items` that match `search` (trimmed, case-insensitive).
pub fn list_picker_matching_indices(items: &[ListPickerItem], search: &str) -> Vec<usize> {
    let search_lower = search.trim().to_lowercase();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| list_picker_item_matches(item, &search_lower))
        .map(|(i, _)| i)
        .collect()
}

/// Display-row index for `sel` among only the matching items (plus category headers).
pub fn list_picker_filtered_display_idx(
    items: &[ListPickerItem],
    matching: &[usize],
    sel: usize,
) -> usize {
    let mut di = 0usize;
    let mut last_cat = String::new();
    for &ei in matching {
        let Some(item) = items.get(ei) else {
            continue;
        };
        if !item.category.is_empty() && item.category != last_cat {
            di += 1;
            last_cat = item.category.clone();
        }
        if ei == sel {
            return di;
        }
        di += 1;
    }
    di
}

/// Next matching item index after `selected` (wraps within `matching`).
pub fn list_picker_next_match(matching: &[usize], selected: usize) -> Option<usize> {
    if matching.is_empty() {
        return None;
    }
    match matching.iter().position(|&i| i == selected) {
        Some(pos) => Some(matching[(pos + 1).min(matching.len() - 1)]),
        None => matching
            .iter()
            .copied()
            .find(|&i| i > selected)
            .or_else(|| matching.first().copied()),
    }
}

/// Previous matching item index before `selected` (wraps within `matching`).
pub fn list_picker_prev_match(matching: &[usize], selected: usize) -> Option<usize> {
    if matching.is_empty() {
        return None;
    }
    match matching.iter().position(|&i| i == selected) {
        Some(0) => Some(matching[0]),
        Some(pos) => Some(matching[pos - 1]),
        None => matching
            .iter()
            .rev()
            .copied()
            .find(|&i| i < selected)
            .or_else(|| matching.last().copied()),
    }
}

/// Keep `selected` on a matching row after the search string changes.
pub fn list_picker_clamp_selection_to_matches(
    matching: &[usize],
    selected: usize,
) -> Option<usize> {
    if matching.is_empty() {
        return None;
    }
    if matching.contains(&selected) {
        Some(selected)
    } else {
        matching.first().copied()
    }
}

/// Resolve the confirm value for a free-entry list picker: the selected item,
/// or the trimmed search text when nothing is selected but search is non-empty.
pub fn list_picker_confirm_value(core: &ListPickerCore) -> Option<String> {
    let search = core.search_textarea.lines().join("");
    if let Some(item) = core.items.get(core.selected) {
        Some(item.value.clone())
    } else if !search.trim().is_empty() {
        Some(search.trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod free_entry_tests {
    use super::*;

    fn core_with(items: Vec<ListPickerItem>, search: &str) -> ListPickerCore {
        let mut ta = make_command_palette_search_textarea();
        if !search.is_empty() {
            ta.insert_str(search);
        }
        ListPickerCore {
            items,
            selected: 0,
            search_textarea: ta,
            scroll_offset: 3,
        }
    }

    fn item(value: &str, category: &str) -> ListPickerItem {
        ListPickerItem {
            value: value.to_string(),
            label: value.to_string(),
            category: category.to_string(),
            description: None,
        }
    }

    #[test]
    fn sync_inserts_free_entry_and_selects_match() {
        let mut core = core_with(
            vec![
                item("main", "Branches"),
                item("feature/path-filter", "Branches"),
                item("v1.0", "Tags"),
            ],
            "path",
        );

        sync_list_picker_free_entry(&mut core, "[path]");

        assert_eq!(core.items.len(), 4);
        assert!(is_free_entry_item(&core.items, "[path]"));
        assert_eq!(core.items[0].value, "path");
        assert_eq!(core.items[0].category, "[path]");
        // First real match after the free-entry row
        assert_eq!(core.selected, 2);
        assert_eq!(core.items[core.selected].value, "feature/path-filter");
    }

    #[test]
    fn sync_selects_free_entry_when_no_match() {
        let mut core = core_with(vec![item("main", "Branches")], "orphan");

        sync_list_picker_free_entry(&mut core, "[author]");

        assert_eq!(core.items.len(), 2);
        assert_eq!(core.selected, 0);
        assert_eq!(core.items[0].value, "orphan");
        assert_eq!(core.items[0].category, "[author]");
    }

    #[test]
    fn preferred_free_entry_stays_selected_when_a_suggestion_matches() {
        let mut core = core_with(vec![item("src/config", "")], "src");

        sync_list_picker_prefer_free_entry(&mut core, "[path]");

        assert_eq!(core.selected, 0);
        assert_eq!(core.items[0].value, "src");
        assert_eq!(core.items[1].value, "src/config");
    }

    #[test]
    fn sync_replaces_previous_free_entry() {
        let mut core = core_with(vec![item("old", "[path]"), item("main", "Branches")], "new");

        sync_list_picker_free_entry(&mut core, "[path]");

        assert_eq!(core.items.len(), 2);
        assert_eq!(core.items[0].value, "new");
        assert!(core.items.iter().filter(|i| i.category == "[path]").count() == 1);
    }

    #[test]
    fn sync_clears_free_entry_and_resets_scroll_when_search_empty() {
        let mut core = core_with(vec![item("typed", "[path]"), item("main", "Branches")], "");
        core.selected = 1;

        sync_list_picker_free_entry(&mut core, "[path]");

        assert_eq!(core.items.len(), 1);
        assert_eq!(core.items[0].value, "main");
        assert_eq!(core.selected, 0);
        assert_eq!(core.scroll_offset, 0);
    }

    #[test]
    fn confirm_value_prefers_selected_item() {
        let mut core = core_with(vec![item("main", "Branches")], "mai");
        sync_list_picker_free_entry(&mut core, "[ref]");
        // selected should be the real match
        let value = list_picker_confirm_value(&core).unwrap();
        assert_eq!(value, "main");
    }

    #[test]
    fn confirm_value_falls_back_to_search_when_empty_list() {
        let core = core_with(vec![], "typed-value");
        assert_eq!(
            list_picker_confirm_value(&core).as_deref(),
            Some("typed-value")
        );
    }

    #[test]
    fn confirm_value_none_when_empty() {
        let core = core_with(vec![], "");
        assert!(list_picker_confirm_value(&core).is_none());
    }

    #[test]
    fn matching_indices_filters_case_insensitively() {
        let items = vec![
            item("src/main.rs", ""),
            item("README.md", ""),
            item("src/gui/mod.rs", ""),
        ];
        assert_eq!(list_picker_matching_indices(&items, "gui"), vec![2]);
        assert_eq!(list_picker_matching_indices(&items, "SRC"), vec![0, 2]);
        assert_eq!(list_picker_matching_indices(&items, ""), vec![0, 1, 2]);
        assert!(list_picker_matching_indices(&items, "zzz").is_empty());
    }

    #[test]
    fn next_prev_match_stay_within_filtered_set() {
        let matching = vec![0usize, 3, 7];
        assert_eq!(list_picker_next_match(&matching, 0), Some(3));
        assert_eq!(list_picker_next_match(&matching, 7), Some(7));
        assert_eq!(list_picker_prev_match(&matching, 7), Some(3));
        assert_eq!(list_picker_prev_match(&matching, 0), Some(0));
        assert_eq!(
            list_picker_clamp_selection_to_matches(&matching, 5),
            Some(0)
        );
        assert_eq!(
            list_picker_clamp_selection_to_matches(&matching, 3),
            Some(3)
        );
    }
}

#[cfg(test)]
mod checklist_free_entry_tests {
    use super::{ChecklistItem, sync_checklist_free_entry};

    fn item(label: &str, checked: bool) -> ChecklistItem {
        ChecklistItem {
            label: label.to_string(),
            checked,
            is_free_entry: false,
        }
    }

    #[test]
    fn inserts_custom_author_at_top() {
        let mut items = vec![item("Alice <a@example.com>", false)];

        sync_checklist_free_entry(&mut items, Some("[author]"), "Bob <b@example.com>");

        assert_eq!(items.len(), 2);
        assert!(items[0].is_free_entry);
        assert_eq!(items[0].label, "Bob <b@example.com>");
        assert!(!items[0].checked);
    }

    #[test]
    fn preserves_checked_state_for_same_custom_value() {
        let mut items = vec![
            ChecklistItem {
                label: "typed".to_string(),
                checked: true,
                is_free_entry: true,
            },
            item("Alice <a@example.com>", false),
        ];

        sync_checklist_free_entry(&mut items, Some("[author]"), "typed");

        assert_eq!(items.len(), 2);
        assert!(items[0].is_free_entry);
        assert!(items[0].checked);
    }

    #[test]
    fn removes_free_entry_when_search_cleared() {
        let mut items = vec![
            ChecklistItem {
                label: "typed".to_string(),
                checked: true,
                is_free_entry: true,
            },
            item("Alice <a@example.com>", true),
        ];

        sync_checklist_free_entry(&mut items, Some("[author]"), "");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Alice <a@example.com>");
        assert!(items[0].checked);
    }
}
