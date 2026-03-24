use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::config::Theme;

use super::highlight::FileHighlighter;
use super::{ChangeType, DiffLine, InlineSegment};

/// State for the diff view panel.
pub struct DiffViewState {
    pub scroll_offset: usize,
    pub horizontal_scroll: usize,
    pub lines: Vec<DiffLine>,
    pub hunk_starts: Vec<usize>,
    pub filename: String,
    pub old_content: String,
    pub new_content: String,
    pub tab_width: usize,
    /// Cached syntax highlighters — rebuilt only when content changes.
    cached_old_highlighter: Option<FileHighlighter>,
    cached_new_highlighter: Option<FileHighlighter>,
}

impl Default for DiffViewState {
    fn default() -> Self {
        Self {
            scroll_offset: 0,
            horizontal_scroll: 0,
            lines: Vec::new(),
            hunk_starts: Vec::new(),
            filename: String::new(),
            old_content: String::new(),
            new_content: String::new(),
            tab_width: 4,
            cached_old_highlighter: None,
            cached_new_highlighter: None,
        }
    }
}

impl DiffViewState {
    pub fn new() -> Self {
        Self {
            tab_width: 4,
            ..Default::default()
        }
    }

    /// Load a diff from old/new content.
    pub fn load(&mut self, filename: &str, old: &str, new: &str) {
        self.filename = filename.to_string();
        self.old_content = old.to_string();
        self.new_content = new.to_string();
        self.lines = super::diff_algo::compute_side_by_side(old, new, self.tab_width);
        self.hunk_starts = super::diff_algo::find_hunk_starts(&self.lines);
        self.scroll_offset = 0;
        self.horizontal_scroll = 0;
        // Pre-compute syntax highlighters once on load, not every frame
        self.cached_old_highlighter = Some(FileHighlighter::new(&self.old_content, &self.filename));
        self.cached_new_highlighter = Some(FileHighlighter::new(&self.new_content, &self.filename));
    }

    /// Load from raw diff output (git diff).
    pub fn load_from_diff_output(&mut self, filename: &str, diff_output: &str) {
        let (old, new) = parse_unified_diff(diff_output);
        self.load(filename, &old, &new);
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        let max = self.lines.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + amount).min(max);
    }

    pub fn scroll_left(&mut self, amount: usize) {
        self.horizontal_scroll = self.horizontal_scroll.saturating_sub(amount);
    }

    pub fn scroll_right(&mut self, amount: usize) {
        self.horizontal_scroll += amount;
    }

    pub fn next_hunk(&mut self) {
        if let Some(next) = self
            .hunk_starts
            .iter()
            .find(|&&h| h > self.scroll_offset)
        {
            self.scroll_offset = *next;
        }
    }

    pub fn prev_hunk(&mut self) {
        if let Some(prev) = self
            .hunk_starts
            .iter()
            .rev()
            .find(|&&h| h < self.scroll_offset)
        {
            self.scroll_offset = *prev;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Render a side-by-side diff view into the given area.
/// Uses direct buffer writes instead of per-cell Paragraph widgets for performance.
pub fn render_diff(
    frame: &mut Frame,
    area: Rect,
    state: &DiffViewState,
    theme: &Theme,
    focused: bool,
) {
    let border_style = if focused {
        theme.active_border
    } else {
        theme.inactive_border
    };

    if state.is_empty() {
        let block = Block::default()
            .title(" Diff ")
            .borders(Borders::ALL)
            .border_style(border_style);
        let widget = Paragraph::new(" No changes to display");
        frame.render_widget(widget.block(block), area);
        return;
    }

    let block = Block::default()
        .title(format!(" {} ", state.filename))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 10 || inner.height < 2 {
        return;
    }

    // Use cached highlighters (built once on load, not every frame)
    let default_highlighter = FileHighlighter::default();
    let old_highlighter = state.cached_old_highlighter.as_ref().unwrap_or(&default_highlighter);
    let new_highlighter = state.cached_new_highlighter.as_ref().unwrap_or(&default_highlighter);

    let gutter_width = 5u16;
    let divider_width = 1u16;

    // Detect new file: old content is empty, so no left panel needed
    let is_new_file = state.old_content.is_empty();

    let visible_height = inner.height as usize;
    let buf = frame.buffer_mut();

    if is_new_file {
        // New file: single full-width panel (right side only)
        let content_width = inner.width.saturating_sub(gutter_width);

        for (row, diff_line) in state.lines[state.scroll_offset..]
            .iter()
            .take(visible_height)
            .enumerate()
        {
            let y = inner.y + row as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let bg = theme.diff_add_bg;

            // Gutter
            let line_num = diff_line
                .new_line
                .as_ref()
                .map(|(n, _)| format!("{:>4} ", n))
                .unwrap_or_else(|| "     ".to_string());
            let gutter_style = Style::default().fg(Color::DarkGray).bg(bg);
            buf_write_str(buf, inner.x, y, &line_num, gutter_style, gutter_width);

            // Content
            let spans = build_content_spans(
                diff_line.new_line.as_ref().map(|(n, t)| (*n, t.as_str())),
                &diff_line.new_segments,
                diff_line.change_type,
                false,
                new_highlighter,
                bg,
                theme,
                content_width as usize,
            );
            buf_write_spans(buf, inner.x + gutter_width, y, &spans, content_width, state.horizontal_scroll);
        }
    } else {
        // Normal side-by-side diff
        let total_chrome = gutter_width * 2 + divider_width;
        let content_width = if inner.width > total_chrome {
            inner.width - total_chrome
        } else {
            inner.width
        };
        let panel_width = content_width / 2;

        for (row, diff_line) in state.lines[state.scroll_offset..]
            .iter()
            .take(visible_height)
            .enumerate()
        {
            let y = inner.y + row as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let (left_bg, right_bg) = line_bg_colors(diff_line.change_type, theme);

            // Left gutter
            let left_num = diff_line
                .old_line
                .as_ref()
                .map(|(n, _)| format!("{:>4} ", n))
                .unwrap_or_else(|| "     ".to_string());
            let gutter_style = Style::default().fg(Color::DarkGray).bg(left_bg);
            buf_write_str(buf, inner.x, y, &left_num, gutter_style, gutter_width);

            // Left content
            let left_spans = if diff_line.change_type == ChangeType::Insert {
                // Addition-only line: fill left side with slash pattern
                let slash_fill: String = std::iter::repeat('/').take(panel_width as usize).collect();
                vec![Span::styled(
                    slash_fill,
                    Style::default().fg(Color::Rgb(60, 60, 60)).bg(left_bg),
                )]
            } else {
                build_content_spans(
                    diff_line.old_line.as_ref().map(|(n, t)| (*n, t.as_str())),
                    &diff_line.old_segments,
                    diff_line.change_type,
                    true,
                    old_highlighter,
                    left_bg,
                    theme,
                    panel_width as usize,
                )
            };
            buf_write_spans(buf, inner.x + gutter_width, y, &left_spans, panel_width, state.horizontal_scroll);

            // Divider
            let div_x = inner.x + gutter_width + panel_width;
            let divider_style = Style::default().fg(Color::DarkGray);
            buf_write_str(buf, div_x, y, "│", divider_style, divider_width);

            // Right gutter
            let right_num = diff_line
                .new_line
                .as_ref()
                .map(|(n, _)| format!("{:>4} ", n))
                .unwrap_or_else(|| "     ".to_string());
            let right_gutter_style = Style::default().fg(Color::DarkGray).bg(right_bg);
            let right_gutter_x = div_x + divider_width;
            buf_write_str(buf, right_gutter_x, y, &right_num, right_gutter_style, gutter_width);

            // Right content
            let right_spans = if diff_line.change_type == ChangeType::Delete {
                // Deletion-only line: fill right side with slash pattern
                let slash_fill: String = std::iter::repeat('/').take(panel_width as usize).collect();
                vec![Span::styled(
                    slash_fill,
                    Style::default().fg(Color::Rgb(60, 60, 60)).bg(right_bg),
                )]
            } else {
                build_content_spans(
                    diff_line.new_line.as_ref().map(|(n, t)| (*n, t.as_str())),
                    &diff_line.new_segments,
                    diff_line.change_type,
                    false,
                    new_highlighter,
                    right_bg,
                    theme,
                    panel_width as usize,
                )
            };
            let right_content_x = right_gutter_x + gutter_width;
            let right_content_width = inner
                .width
                .saturating_sub(gutter_width * 2 + panel_width + divider_width);
            buf_write_spans(buf, right_content_x, y, &right_spans, right_content_width, state.horizontal_scroll);
        }
    }
}

/// Write a string directly to the buffer at (x, y) with the given style, clamped to max_width.
#[inline]
fn buf_write_str(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style, max_width: u16) {
    let buf_area = buf.area();
    if y < buf_area.y || y >= buf_area.y + buf_area.height {
        return;
    }
    let mut col = x;
    let end_col = x.saturating_add(max_width).min(buf_area.x + buf_area.width);
    for ch in text.chars() {
        if col >= end_col {
            break;
        }
        let width = unicode_display_width(ch);
        if width == 0 {
            continue;
        }
        if let Some(cell) = buf.cell_mut((col, y)) {
            cell.set_char(ch);
            cell.set_style(style);
        }
        col += width as u16;
    }
}

/// Write styled spans directly to the buffer at (x, y), clamped to max_width.
/// `h_scroll` skips the first N display columns of content.
#[inline]
fn buf_write_spans(buf: &mut Buffer, x: u16, y: u16, spans: &[Span<'_>], max_width: u16, h_scroll: usize) {
    let buf_area = buf.area();
    if y < buf_area.y || y >= buf_area.y + buf_area.height {
        return;
    }
    let mut col = x;
    let end_col = x.saturating_add(max_width).min(buf_area.x + buf_area.width);
    let mut skipped: usize = 0;
    for span in spans {
        for ch in span.content.chars() {
            if col >= end_col {
                return;
            }
            let width = unicode_display_width(ch);
            if width == 0 {
                continue;
            }
            if skipped < h_scroll {
                skipped += width;
                continue;
            }
            if let Some(cell) = buf.cell_mut((col, y)) {
                cell.set_char(ch);
                cell.set_style(span.style);
            }
            col += width as u16;
        }
    }
}

/// Get the display width of a character (1 for most, 2 for CJK wide chars).
#[inline]
fn unicode_display_width(ch: char) -> usize {
    if ch == '\t' || ch == '\n' || ch == '\r' {
        return 0;
    }
    unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1)
}

/// Get background colors for a diff line based on change type.
fn line_bg_colors(change_type: ChangeType, theme: &Theme) -> (Color, Color) {
    match change_type {
        ChangeType::Equal => (Color::Reset, Color::Reset),
        ChangeType::Delete => (theme.diff_remove_bg, Color::Reset),
        ChangeType::Insert => (Color::Reset, theme.diff_add_bg),
        ChangeType::Modified => (theme.diff_remove_bg, theme.diff_add_bg),
    }
}

/// Build styled spans for one side of a diff line.
#[allow(clippy::too_many_arguments)]
fn build_content_spans<'a>(
    line_data: Option<(usize, &str)>,
    segments: &Option<Vec<InlineSegment>>,
    change_type: ChangeType,
    is_old_side: bool,
    highlighter: &FileHighlighter,
    bg: Color,
    theme: &Theme,
    max_width: usize,
) -> Vec<Span<'a>> {
    let Some((line_num, text)) = line_data else {
        // Empty side — fill with background
        let fill = " ".repeat(max_width);
        return vec![Span::styled(fill, Style::default().bg(bg))];
    };

    // If we have word-level segments, use those
    if let Some(segs) = segments {
        return build_word_diff_spans(segs, is_old_side, bg, theme, max_width);
    }

    // Otherwise, try syntax highlighting
    let highlighted = highlighter.get_line_spans(line_num, Some(bg));
    if !highlighted.is_empty() {
        return highlighted;
    }

    // Fallback: plain text with background
    let fg = match change_type {
        ChangeType::Delete => theme.diff_remove.fg.unwrap_or(Color::Red),
        ChangeType::Insert => theme.diff_add.fg.unwrap_or(Color::Green),
        _ => Color::White,
    };

    let display = if text.len() > max_width {
        // Find a safe byte boundary to avoid slicing mid-character
        let mut end = max_width;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    } else {
        text
    };

    vec![Span::styled(
        display.to_string(),
        Style::default().fg(fg).bg(bg),
    )]
}

/// Build spans with word-level diff emphasis.
fn build_word_diff_spans<'a>(
    segments: &[InlineSegment],
    is_old_side: bool,
    bg: Color,
    theme: &Theme,
    _max_width: usize,
) -> Vec<Span<'a>> {
    segments
        .iter()
        .map(|seg| {
            if seg.emphasized {
                let emphasis_bg = if is_old_side {
                    theme.diff_remove_word
                } else {
                    theme.diff_add_word
                };
                Span::styled(
                    seg.text.clone(),
                    Style::default()
                        .bg(emphasis_bg)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(seg.text.clone(), Style::default().bg(bg).fg(Color::White))
            }
        })
        .collect()
}

/// Parse a unified diff into old/new content for side-by-side display.
/// This handles `git diff` output format.
fn parse_unified_diff(diff: &str) -> (String, String) {
    let mut old_lines = Vec::new();
    let mut new_lines = Vec::new();
    let mut in_hunk = false;

    for line in diff.lines() {
        if line.starts_with("@@") {
            in_hunk = true;
            continue;
        }

        if !in_hunk {
            continue;
        }

        if line.starts_with('-') {
            old_lines.push(&line[1..]);
        } else if line.starts_with('+') {
            new_lines.push(&line[1..]);
        } else if let Some(ctx) = line.strip_prefix(' ') {
            old_lines.push(ctx);
            new_lines.push(ctx);
        } else {
            // Could be "\ No newline at end of file" or other metadata
            old_lines.push(line);
            new_lines.push(line);
        }
    }

    (old_lines.join("\n"), new_lines.join("\n"))
}
