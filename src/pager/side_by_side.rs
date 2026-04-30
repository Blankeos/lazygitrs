use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::config::Theme;

use super::highlight::FileHighlighter;
use super::{ChangeType, DiffLine, InlineSegment};

/// A section of a multi-file diff with its own highlighters.
pub struct FileSection {
    pub old_highlighter: FileHighlighter,
    pub new_highlighter: FileHighlighter,
}

/// Pre-parsed diff data that can be sent across threads.
/// Contains all the expensive-to-compute results (diff algorithm, tree-sitter highlighting).
pub struct ParsedDiff {
    pub filename: String,
    pub old_content: String,
    pub new_content: String,
    pub lines: Vec<DiffLine>,
    pub hunk_starts: Vec<usize>,
    pub hunk_line_offsets: Vec<(usize, usize, usize)>,
    pub sections: Vec<FileSection>,
    pub file_exists_on_disk: bool,
}

/// Which panel of the side-by-side diff the selection is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffPanel {
    Old, // Left panel (deleted / original code)
    New, // Right panel (added / new code)
}

/// Mouse text selection state in the diff view.
#[derive(Clone, Debug)]
pub struct TextSelection {
    /// Which panel (left/right) the selection lives in.
    pub panel: DiffPanel,
    /// Terminal column where the selection started.
    pub start_col: u16,
    /// Terminal row where the selection started.
    pub start_row: u16,
    /// Terminal column where the selection currently ends.
    pub end_col: u16,
    /// Terminal row where the selection currently ends.
    pub end_row: u16,
    /// Whether the user is still dragging (selection in progress).
    pub dragging: bool,
    /// True when the selection is a single click with no drag (shows edit tooltip only).
    pub is_click: bool,
    /// The extracted selected text (populated after rendering).
    pub text: String,
    /// The file line number at the top of the selection/click (populated after rendering).
    pub edit_line_number: Option<usize>,
    /// The file column number at the click position (1-based, populated after rendering).
    pub edit_column_number: Option<usize>,
}

impl TextSelection {
    /// Returns (top_row, top_col, bottom_row, bottom_col) in normalized order.
    pub fn normalized(&self) -> (u16, u16, u16, u16) {
        if self.start_row < self.end_row
            || (self.start_row == self.end_row && self.start_col <= self.end_col)
        {
            (self.start_row, self.start_col, self.end_row, self.end_col)
        } else {
            (self.end_row, self.end_col, self.start_row, self.start_col)
        }
    }
}

/// Computed column layout for the diff panel's left/right content areas.
/// All coordinates are absolute terminal X positions.
#[derive(Clone, Copy, Debug)]
pub struct DiffPanelLayout {
    /// Whether this is a new-file diff (only right panel visible).
    pub is_new_file: bool,
    /// Left panel content start X (after gutter).
    pub old_content_x: u16,
    /// Left panel content end X (exclusive, up to divider).
    pub old_content_end_x: u16,
    /// Right panel content start X (after right gutter).
    pub new_content_x: u16,
    /// Right panel content end X (exclusive).
    pub new_content_end_x: u16,
    /// Inner area Y start (after top border).
    pub inner_y: u16,
    /// Inner area Y end (exclusive, before bottom border).
    pub inner_end_y: u16,
}

impl DiffPanelLayout {
    /// Compute the panel layout from a main panel Rect and diff state.
    pub fn compute(panel_rect: Rect, state: &DiffViewState) -> Self {
        let inner_x = panel_rect.x + 1; // border
        let inner_y = panel_rect.y + 1;
        let inner_w = panel_rect.width.saturating_sub(2);
        let inner_end_y = inner_y + panel_rect.height.saturating_sub(2);

        let gutter: u16 = 5;
        let divider: u16 = 2;

        let is_new_file = state.old_content.is_empty() && state.sections.len() <= 1;

        // Single-side view uses full-width single panel
        let single_side = match state.side_view {
            DiffSideView::OldOnly => Some(DiffPanel::Old),
            DiffSideView::NewOnly => Some(DiffPanel::New),
            DiffSideView::Both => None,
        };

        if single_side.is_some() || is_new_file {
            // Single panel — gutter(5) + content(rest)
            let content_x = inner_x + gutter;
            let content_end_x = inner_x + inner_w;
            let panel = single_side.unwrap_or(DiffPanel::New);
            let (old_x, old_end, new_x, new_end) = match panel {
                DiffPanel::Old => (content_x, content_end_x, 0, 0),
                DiffPanel::New => (0, 0, content_x, content_end_x),
            };
            Self {
                is_new_file: is_new_file && single_side.is_none(),
                old_content_x: old_x,
                old_content_end_x: old_end,
                new_content_x: new_x,
                new_content_end_x: new_end,
                inner_y,
                inner_end_y,
            }
        } else {
            let total_chrome = gutter * 2 + divider;
            let content_w = if inner_w > total_chrome {
                inner_w - total_chrome
            } else {
                0
            };
            let panel_w = content_w / 2;

            let old_content_x = inner_x + gutter;
            let old_content_end_x = old_content_x + panel_w;
            // divider is at old_content_end_x, right gutter starts at old_content_end_x + 1
            let new_content_x = old_content_end_x + divider + gutter;
            let new_content_end_x = inner_x + inner_w;

            Self {
                is_new_file: false,
                old_content_x,
                old_content_end_x,
                new_content_x,
                new_content_end_x,
                inner_y,
                inner_end_y,
            }
        }
    }

    /// Determine which panel an X coordinate falls in, if any.
    pub fn panel_at_x(&self, x: u16) -> Option<DiffPanel> {
        if self.is_new_file {
            if x >= self.new_content_x && x < self.new_content_end_x {
                return Some(DiffPanel::New);
            }
            // Also count gutter clicks as panel clicks
            if x >= self.new_content_x.saturating_sub(5) && x < self.new_content_end_x {
                return Some(DiffPanel::New);
            }
            return None;
        }
        // Include gutter in the clickable zone for each panel
        if x >= self.old_content_x.saturating_sub(5) && x < self.old_content_end_x {
            Some(DiffPanel::Old)
        } else if x >= self.new_content_x.saturating_sub(5) && x < self.new_content_end_x {
            Some(DiffPanel::New)
        } else {
            None
        }
    }

    /// Get the content column range for a given panel.
    pub fn content_range(&self, panel: DiffPanel) -> (u16, u16) {
        match panel {
            DiffPanel::Old => (self.old_content_x, self.old_content_end_x),
            DiffPanel::New => (self.new_content_x, self.new_content_end_x),
        }
    }

    /// Get the divider X column between old and new panels (both-side view only).
    pub fn divider_x(&self) -> Option<u16> {
        if self.is_new_file || self.old_content_end_x == 0 || self.new_content_x == 0 {
            None
        } else {
            Some(self.old_content_end_x)
        }
    }
}

/// Which side(s) of the diff to display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSideView {
    Both,
    OldOnly,
    NewOnly,
}

/// A search match within the diff content, used for n/N navigation.
#[derive(Clone, Debug)]
pub struct DiffSearchMatch {
    /// Index into `DiffViewState::lines`.
    pub line_idx: usize,
    /// Which panel the match is on.
    pub panel: DiffPanel,
    /// Character (byte) offset within the line text.
    pub col: usize,
}

/// One entry in the diff view's revert-hunk undo stack.
pub struct RevertUndoEntry {
    pub file_path: String,
    pub pre_revert_bytes: Vec<u8>,
}

/// Maximum entries kept in the revert-hunk undo stack.
pub const REVERT_UNDO_STACK_CAP: usize = 20;

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
    /// Per-file-section highlighters for multi-file diffs.
    sections: Vec<FileSection>,
    /// Active mouse text selection, if any.
    pub selection: Option<TextSelection>,
    /// Which side(s) of the diff to show (Both, OldOnly, NewOnly).
    pub side_view: DiffSideView,
    /// Whether long lines are wrapped to fit the panel width.
    pub wrap: bool,
    /// Whether the currently viewed file exists in the working tree on disk.
    pub file_exists_on_disk: bool,
    /// Hunk line number offsets for unified diffs. Each entry is
    /// `(first_diff_line_idx, old_offset, new_offset)`.
    /// The offset is added to the 1-based content line number to get the
    /// actual file line number. Empty for full-content diffs (no offset needed).
    pub hunk_line_offsets: Vec<(usize, usize, usize)>,
    /// Whether the search input is currently active (typing).
    pub search_active: bool,
    /// Current search query string.
    pub search_query: String,
    /// All matches found in the diff content.
    pub search_matches: Vec<DiffSearchMatch>,
    /// Index of the current match (for n/N navigation).
    pub search_match_idx: usize,
    /// Textarea widget for search input.
    pub search_textarea: Option<tui_textarea::TextArea<'static>>,
    /// Currently selected revert-button hunk index (for keyboard cycling).
    pub selected_revert_hunk: Option<usize>,
    /// Hunk index currently under the mouse cursor (for tooltip rendering).
    pub hovered_revert_hunk: Option<usize>,
    /// Pre-revert file snapshots, most-recent last. Bounded by
    /// `REVERT_UNDO_STACK_CAP`. Used to undo revert-hunk actions within a
    /// session.
    pub revert_undo_stack: Vec<RevertUndoEntry>,
    /// Peak `revert_undo_stack.len()` since it was last empty. Drives the
    /// `n/m` denominator in the bottom-right footnote; resets to 0 once the
    /// stack drains so a fresh streak starts at `1/1`.
    pub revert_undo_high_water: usize,
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
            sections: Vec::new(),
            selection: None,
            side_view: DiffSideView::Both,
            wrap: false,
            file_exists_on_disk: false,
            hunk_line_offsets: Vec::new(),
            search_active: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_match_idx: 0,
            search_textarea: None,
            selected_revert_hunk: None,
            hovered_revert_hunk: None,
            revert_undo_stack: Vec::new(),
            revert_undo_high_water: 0,
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

    /// Get the actual file line number for a DiffLine, applying hunk offsets.
    /// Returns the display/file line number (e.g. for gutter or editAtLine).
    pub fn file_line_number(&self, line_idx: usize, panel: DiffPanel) -> Option<usize> {
        let dl = self.lines.get(line_idx)?;
        let content_num = match panel {
            DiffPanel::Old => dl.old_line.as_ref()?.0,
            DiffPanel::New => dl.new_line.as_ref()?.0,
        };
        let offset = self
            .hunk_line_offsets
            .iter()
            .rev()
            .find(|(start_idx, _, _)| *start_idx <= line_idx)
            .map(|(_, old_off, new_off)| match panel {
                DiffPanel::Old => *old_off,
                DiffPanel::New => *new_off,
            })
            .unwrap_or(0);
        Some(content_num + offset)
    }

    /// Get the filename that a given line index belongs to.
    /// For single-file diffs, returns `self.filename`.
    /// For multi-file diffs, walks backwards to find the nearest file header.
    pub fn file_at_line(&self, line_idx: usize) -> &str {
        for i in (0..=line_idx).rev() {
            if let Some(ref header) = self.lines.get(i).and_then(|l| l.file_header.as_ref()) {
                return header;
            }
        }
        &self.filename
    }

    /// Activate search mode with an empty query.
    pub fn start_search(&mut self) {
        self.search_active = true;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_match_idx = 0;
        let mut ta = tui_textarea::TextArea::default();
        ta.set_cursor_line_style(Style::default());
        self.search_textarea = Some(ta);
    }

    /// Update search matches after the query changes.
    pub fn update_search(&mut self) {
        self.search_matches.clear();
        if self.search_query.is_empty() {
            self.search_match_idx = 0;
            return;
        }
        let query_lower = self.search_query.to_lowercase();
        for (line_idx, line) in self.lines.iter().enumerate() {
            if line.file_header.is_some() {
                continue;
            }
            // Search old side
            if let Some((_, ref text)) = line.old_line {
                let text_lower = text.to_lowercase();
                let mut start = 0;
                while let Some(pos) = text_lower[start..].find(&query_lower) {
                    self.search_matches.push(DiffSearchMatch {
                        line_idx,
                        panel: DiffPanel::Old,
                        col: start + pos,
                    });
                    start += pos + 1;
                }
            }
            // Search new side
            if let Some((_, ref text)) = line.new_line {
                let text_lower = text.to_lowercase();
                let mut start = 0;
                while let Some(pos) = text_lower[start..].find(&query_lower) {
                    self.search_matches.push(DiffSearchMatch {
                        line_idx,
                        panel: DiffPanel::New,
                        col: start + pos,
                    });
                    start += pos + 1;
                }
            }
        }
        // Clamp match index
        if self.search_matches.is_empty() {
            self.search_match_idx = 0;
        } else {
            self.search_match_idx = self.search_match_idx.min(self.search_matches.len() - 1);
        }
    }

    /// Dismiss search input but keep the query and highlights.
    pub fn dismiss_search(&mut self) {
        self.search_active = false;
        self.search_textarea = None;
    }

    /// Clear search entirely.
    pub fn clear_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_match_idx = 0;
        self.search_textarea = None;
    }

    /// Navigate to the next search match and scroll to it.
    pub fn next_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_idx = (self.search_match_idx + 1) % self.search_matches.len();
        self.scroll_to_current_match();
    }

    /// Navigate to the previous search match and scroll to it.
    pub fn prev_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        if self.search_match_idx == 0 {
            self.search_match_idx = self.search_matches.len() - 1;
        } else {
            self.search_match_idx -= 1;
        }
        self.scroll_to_current_match();
    }

    /// Scroll so the current search match is visible.
    pub fn scroll_to_current_match(&mut self) {
        if let Some(m) = self.search_matches.get(self.search_match_idx) {
            let line = m.line_idx;
            // Scroll so the match line is visible (roughly centered)
            if line < self.scroll_offset || line >= self.scroll_offset + 20 {
                self.scroll_offset = line.saturating_sub(5);
            }
        }
    }

    /// Parse old/new content into a ParsedDiff on any thread (no &self needed).
    pub fn parse_content(
        filename: &str,
        old: &str,
        new: &str,
        tab_width: usize,
        file_exists_on_disk: bool,
    ) -> ParsedDiff {
        let lines = super::diff_algo::compute_side_by_side(old, new, tab_width);
        let hunk_starts = super::diff_algo::find_hunk_starts(&lines);
        let sections = vec![FileSection {
            old_highlighter: FileHighlighter::new(old, filename),
            new_highlighter: FileHighlighter::new(new, filename),
        }];
        ParsedDiff {
            filename: filename.to_string(),
            old_content: old.to_string(),
            new_content: new.to_string(),
            lines,
            hunk_starts,
            hunk_line_offsets: Vec::new(),
            sections,
            file_exists_on_disk,
        }
    }

    /// Parse raw diff output into a ParsedDiff on any thread (no &self needed).
    pub fn parse_diff_output(
        filename: &str,
        diff_output: &str,
        tab_width: usize,
        file_exists_on_disk: bool,
    ) -> ParsedDiff {
        let file_diffs = parse_multi_file_diff(diff_output);

        if file_diffs.len() <= 1 {
            let (old, new) = parse_unified_diff(diff_output);
            let actual_name = file_diffs
                .first()
                .map(|(name, _)| name.as_str())
                .unwrap_or(filename);
            let lines =
                super::diff_algo::compute_side_by_side_from_unified_diff(diff_output, tab_width);
            let hunk_starts = super::diff_algo::find_hunk_starts(&lines);
            let hunks = parse_hunk_headers(diff_output);
            let hunk_line_offsets = build_hunk_line_offsets(&hunks, &lines, 0);
            let sections = vec![FileSection {
                old_highlighter: FileHighlighter::new(&old, actual_name),
                new_highlighter: FileHighlighter::new(&new, actual_name),
            }];
            ParsedDiff {
                filename: actual_name.to_string(),
                old_content: old,
                new_content: new,
                lines,
                hunk_starts,
                hunk_line_offsets,
                sections,
                file_exists_on_disk,
            }
        } else {
            let file_count = file_diffs.len();
            let new_filename = format!("{} ({} files)", filename, file_count);
            let mut lines = Vec::new();
            let mut sections = Vec::new();
            let mut hunk_line_offsets = Vec::new();

            for (section_idx, (file_name, file_diff)) in file_diffs.iter().enumerate() {
                let (old, new) = parse_unified_diff(file_diff);

                lines.push(DiffLine {
                    old_line: None,
                    new_line: None,
                    change_type: ChangeType::Equal,
                    old_segments: None,
                    new_segments: None,
                    file_header: Some(file_name.clone()),
                    section_index: section_idx,
                });

                let section_start = lines.len();
                let mut section_lines =
                    super::diff_algo::compute_side_by_side_from_unified_diff(file_diff, tab_width);
                for line in &mut section_lines {
                    line.section_index = section_idx;
                }
                lines.append(&mut section_lines);

                let hunks = parse_hunk_headers(file_diff);
                let section_offsets = build_hunk_line_offsets(&hunks, &lines[section_start..], 0);
                for (idx, old_off, new_off) in section_offsets {
                    hunk_line_offsets.push((section_start + idx, old_off, new_off));
                }

                sections.push(FileSection {
                    old_highlighter: FileHighlighter::new(&old, file_name),
                    new_highlighter: FileHighlighter::new(&new, file_name),
                });
            }

            let hunk_starts = super::diff_algo::find_hunk_starts(&lines);

            ParsedDiff {
                filename: new_filename,
                old_content: String::new(),
                new_content: String::new(),
                lines,
                hunk_starts,
                hunk_line_offsets,
                sections,
                file_exists_on_disk,
            }
        }
    }

    /// Apply a pre-parsed diff result, preserving scroll position for same-file reloads.
    pub fn apply_parsed(&mut self, parsed: ParsedDiff) {
        let same_file = self.filename == parsed.filename;
        let prev_selected_revert_hunk = self.selected_revert_hunk;
        let prev_hovered_revert_hunk = self.hovered_revert_hunk;
        self.filename = parsed.filename;
        self.old_content = parsed.old_content;
        self.new_content = parsed.new_content;
        self.lines = parsed.lines;
        self.hunk_starts = parsed.hunk_starts;
        self.hunk_line_offsets = parsed.hunk_line_offsets;
        self.sections = parsed.sections;
        self.file_exists_on_disk = parsed.file_exists_on_disk;
        self.selected_revert_hunk = if same_file {
            prev_selected_revert_hunk.filter(|&i| i < self.hunk_starts.len())
        } else {
            None
        };
        self.hovered_revert_hunk = if same_file {
            prev_hovered_revert_hunk.filter(|&i| i < self.hunk_starts.len())
        } else {
            None
        };
        if same_file {
            let max = self.lines.len().saturating_sub(1);
            self.scroll_offset = self.scroll_offset.min(max);
        } else {
            self.scroll_offset = 0;
            self.horizontal_scroll = 0;
            self.selection = None;
            self.clear_search();
        }
    }

    /// Load a diff from old/new content (single file).
    pub fn load(&mut self, filename: &str, old: &str, new: &str) {
        // Preserve scroll position when reloading the same file (e.g. periodic refresh)
        let same_file = self.filename == filename;
        let prev_selected_revert_hunk = self.selected_revert_hunk;
        let prev_hovered_revert_hunk = self.hovered_revert_hunk;
        self.filename = filename.to_string();
        self.old_content = old.to_string();
        self.new_content = new.to_string();
        self.lines = super::diff_algo::compute_side_by_side(old, new, self.tab_width);
        self.hunk_starts = super::diff_algo::find_hunk_starts(&self.lines);
        self.hunk_line_offsets = Vec::new(); // Full content — no offsets needed
        if same_file {
            // Clamp scroll in case the diff got shorter
            let max = self.lines.len().saturating_sub(1);
            self.scroll_offset = self.scroll_offset.min(max);
        } else {
            self.scroll_offset = 0;
            self.horizontal_scroll = 0;
            self.selection = None;
            self.clear_search();
        }
        self.selected_revert_hunk = if same_file {
            prev_selected_revert_hunk.filter(|&i| i < self.hunk_starts.len())
        } else {
            None
        };
        self.hovered_revert_hunk = if same_file {
            prev_hovered_revert_hunk.filter(|&i| i < self.hunk_starts.len())
        } else {
            None
        };
        // Preserve side_view across reloads so periodic refresh doesn't reset it
        // Single section with index 0
        self.sections = vec![FileSection {
            old_highlighter: FileHighlighter::new(old, filename),
            new_highlighter: FileHighlighter::new(new, filename),
        }];
    }

    /// Load from raw diff output (git diff).
    /// Automatically detects multi-file diffs and splits into per-file sections.
    pub fn load_from_diff_output(&mut self, filename: &str, diff_output: &str) {
        let file_diffs = parse_multi_file_diff(diff_output);

        if file_diffs.len() <= 1 {
            // Single file diff — use the simple path
            let (old, new) = parse_unified_diff(diff_output);
            // Use the actual filename from the diff header if available
            let actual_name = file_diffs
                .first()
                .map(|(name, _)| name.as_str())
                .unwrap_or(filename);
            // Build lines directly from the unified diff's per-line markers,
            // bypassing Myers re-diff so multi-hunk content can't alias.
            let same_file = self.filename == actual_name;
            self.filename = actual_name.to_string();
            self.old_content = old.clone();
            self.new_content = new.clone();
            self.lines = super::diff_algo::compute_side_by_side_from_unified_diff(
                diff_output,
                self.tab_width,
            );
            self.hunk_starts = super::diff_algo::find_hunk_starts(&self.lines);
            let hunks = parse_hunk_headers(diff_output);
            self.hunk_line_offsets = build_hunk_line_offsets(&hunks, &self.lines, 0);
            self.sections = vec![FileSection {
                old_highlighter: FileHighlighter::new(&old, actual_name),
                new_highlighter: FileHighlighter::new(&new, actual_name),
            }];
            if same_file {
                let max = self.lines.len().saturating_sub(1);
                self.scroll_offset = self.scroll_offset.min(max);
            } else {
                self.scroll_offset = 0;
                self.horizontal_scroll = 0;
                self.selection = None;
                self.clear_search();
            }
        } else {
            // Multi-file diff — build per-section lines with highlighters
            let file_count = file_diffs.len();
            let new_filename = format!("{} ({} files)", filename, file_count);
            let same_file = self.filename == new_filename;
            let prev_selected_revert_hunk = self.selected_revert_hunk;
            let prev_hovered_revert_hunk = self.hovered_revert_hunk;
            self.filename = new_filename;
            self.old_content = String::new();
            self.new_content = String::new();
            self.lines = Vec::new();
            self.sections = Vec::new();
            if !same_file {
                self.scroll_offset = 0;
                self.horizontal_scroll = 0;
                self.selection = None;
                self.clear_search();
            }
            self.selected_revert_hunk = if same_file {
                prev_selected_revert_hunk
            } else {
                None
            };
            self.hovered_revert_hunk = if same_file {
                prev_hovered_revert_hunk
            } else {
                None
            };

            self.hunk_line_offsets = Vec::new();

            for (section_idx, (file_name, file_diff)) in file_diffs.iter().enumerate() {
                let (old, new) = parse_unified_diff(file_diff);

                // Add file header separator line
                let _header_idx = self.lines.len();
                self.lines.push(DiffLine {
                    old_line: None,
                    new_line: None,
                    change_type: ChangeType::Equal,
                    old_segments: None,
                    new_segments: None,
                    file_header: Some(file_name.clone()),
                    section_index: section_idx,
                });

                // Compute diff lines for this file section
                let section_start = self.lines.len();
                let mut section_lines = super::diff_algo::compute_side_by_side_from_unified_diff(
                    file_diff,
                    self.tab_width,
                );
                for line in &mut section_lines {
                    line.section_index = section_idx;
                }
                self.lines.append(&mut section_lines);

                // Compute hunk line offsets for this section
                let hunks = parse_hunk_headers(file_diff);
                let section_offsets =
                    build_hunk_line_offsets(&hunks, &self.lines[section_start..], 0);
                // Adjust indices to be global (relative to self.lines)
                for (idx, old_off, new_off) in section_offsets {
                    self.hunk_line_offsets
                        .push((section_start + idx, old_off, new_off));
                }

                // Create highlighters for this section
                self.sections.push(FileSection {
                    old_highlighter: FileHighlighter::new(&old, file_name),
                    new_highlighter: FileHighlighter::new(&new, file_name),
                });
            }

            self.hunk_starts = super::diff_algo::find_hunk_starts(&self.lines);
            self.selected_revert_hunk = if same_file {
                self.selected_revert_hunk
                    .filter(|&i| i < self.hunk_starts.len())
            } else {
                None
            };
            self.hovered_revert_hunk = if same_file {
                self.hovered_revert_hunk
                    .filter(|&i| i < self.hunk_starts.len())
            } else {
                None
            };

            if same_file {
                // Clamp scroll in case the diff got shorter
                let max = self.lines.len().saturating_sub(1);
                self.scroll_offset = self.scroll_offset.min(max);
            }
        }
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
        if let Some(next) = self.hunk_starts.iter().find(|&&h| h > self.scroll_offset) {
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

    /// Map a terminal row within the diff panel inner area to the visible diff line index.
    /// This is exact when wrapping is disabled.
    pub fn line_index_at_row(&self, row: u16, layout: &DiffPanelLayout) -> Option<usize> {
        if row < layout.inner_y || row >= layout.inner_end_y {
            return None;
        }
        let idx = self.scroll_offset + (row - layout.inner_y) as usize;
        if idx < self.lines.len() {
            Some(idx)
        } else {
            None
        }
    }

    /// Return true when the given line index is the first line of a diff hunk.
    pub fn is_hunk_start_line(&self, line_idx: usize) -> bool {
        self.hunk_starts.binary_search(&line_idx).is_ok()
    }

    /// Get the zero-based hunk index for a hunk-start line.
    pub fn hunk_index_for_start_line(&self, line_idx: usize) -> Option<usize> {
        self.hunk_starts.binary_search(&line_idx).ok()
    }

    /// For the visual hunk at `block_idx`, return the (old, new) line-number
    /// ranges (inclusive) the block covers in the underlying file. Either
    /// side may be `None` when the block is a pure insertion (no `-` lines)
    /// or pure deletion (no `+` lines). Used to slice a sub-patch out of the
    /// raw unified diff so revert affects only this visual block, not the
    /// surrounding `@@` hunk that may contain other change blocks.
    pub fn visual_block_line_ranges(
        &self,
        block_idx: usize,
    ) -> Option<(Option<(usize, usize)>, Option<(usize, usize)>)> {
        let start = *self.hunk_starts.get(block_idx)?;
        let mut end = start;
        while end < self.lines.len() && !matches!(self.lines[end].change_type, ChangeType::Equal) {
            end += 1;
        }
        // DiffLine line numbers are content-relative (positions inside the
        // concatenated old/new strings produced by parse_unified_diff), but
        // the unified-diff walker we feed these ranges to tracks file-relative
        // line numbers. `hunk_line_offsets` provides the per-`@@` deltas we
        // need to bridge the two.
        let (old_offset, new_offset) = self.line_number_offsets_at(start);
        let mut old_range: Option<(usize, usize)> = None;
        let mut new_range: Option<(usize, usize)> = None;
        for line in &self.lines[start..end] {
            if matches!(line.change_type, ChangeType::Delete | ChangeType::Modified) {
                if let Some((n, _)) = &line.old_line {
                    let n = *n + old_offset;
                    old_range = Some(match old_range {
                        None => (n, n),
                        Some((lo, hi)) => (lo.min(n), hi.max(n)),
                    });
                }
            }
            if matches!(line.change_type, ChangeType::Insert | ChangeType::Modified) {
                if let Some((n, _)) = &line.new_line {
                    let n = *n + new_offset;
                    new_range = Some(match new_range {
                        None => (n, n),
                        Some((lo, hi)) => (lo.min(n), hi.max(n)),
                    });
                }
            }
        }
        Some((old_range, new_range))
    }

    /// Find the `@@` hunk that owns the DiffLine at `line_idx` and return
    /// its (old, new) content→file line-number offsets. Returns (0, 0) when
    /// no hunk metadata is available (e.g. the `load(...)` raw-content path,
    /// where content and file numbering already coincide).
    fn line_number_offsets_at(&self, line_idx: usize) -> (usize, usize) {
        let mut offsets = (0usize, 0usize);
        for &(start_idx, old_off, new_off) in &self.hunk_line_offsets {
            if start_idx <= line_idx {
                offsets = (old_off, new_off);
            } else {
                break;
            }
        }
        offsets
    }

    /// Cycle to the next revertable hunk marker.
    pub fn select_next_revert_hunk(&mut self) {
        if self.hunk_starts.is_empty() {
            self.selected_revert_hunk = None;
            return;
        }
        let next = match self.selected_revert_hunk {
            Some(i) => (i + 1) % self.hunk_starts.len(),
            None => 0,
        };
        self.selected_revert_hunk = Some(next);
    }

    /// Cycle to the previous revertable hunk marker.
    pub fn select_prev_revert_hunk(&mut self) {
        if self.hunk_starts.is_empty() {
            self.selected_revert_hunk = None;
            return;
        }
        let prev = match self.selected_revert_hunk {
            Some(0) | None => self.hunk_starts.len() - 1,
            Some(i) => i.saturating_sub(1),
        };
        self.selected_revert_hunk = Some(prev);
    }

    /// Get the highlighters for a given section index.
    fn highlighters_for_section(
        &self,
        section_index: usize,
    ) -> Option<(&FileHighlighter, &FileHighlighter)> {
        self.sections
            .get(section_index)
            .map(|s| (&s.old_highlighter, &s.new_highlighter))
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
    diff_loading: bool,
    show_revert_markers: bool,
) {
    let border_style = if focused {
        theme.active_border
    } else {
        theme.inactive_border
    };

    if state.is_empty() {
        let msg = if diff_loading {
            " Loading diff..."
        } else {
            " No changes to display"
        };
        let block = Block::default()
            .title(" Diff ")
            .borders(Borders::ALL)
            .border_style(border_style);
        let widget = Paragraph::new(msg);
        frame.render_widget(widget.block(block), area);
        return;
    }

    let side_label = match state.side_view {
        DiffSideView::OldOnly => " [old] ",
        DiffSideView::NewOnly => " [new] ",
        DiffSideView::Both => "",
    };
    let title = if side_label.is_empty() {
        format!(" {} ", state.filename)
    } else {
        format!(" {}{}", state.filename, side_label)
    };

    let mut block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    // Bottom-right footnote: revert-hunk undo indicator. Only shown when
    // there's something to undo; the denominator is the peak stack depth
    // since it last drained, so a streak reads `1/1`, `2/2`, ... and undos
    // walk it back down to `1/3` etc.
    let undo_n = state.revert_undo_stack.len();
    if undo_n > 0 {
        let undo_m = state.revert_undo_high_water.max(undo_n);
        block = block.title_bottom(
            Line::from(vec![
                Span::styled(" ", Style::default().fg(theme.text_dimmed)),
                Span::styled(
                    "u",
                    Style::default()
                        .fg(theme.accent_secondary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" undo revert ({}/{}) ", undo_n, undo_m),
                    Style::default().fg(theme.text_dimmed),
                ),
            ])
            .alignment(ratatui::layout::Alignment::Right),
        );
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 10 || inner.height < 2 {
        return;
    }

    let gutter_width = 5u16;
    let divider_width = 2u16;

    // Detect new file: old content is empty, so no left panel needed
    let is_new_file = state.old_content.is_empty() && state.sections.len() <= 1;

    // Single-side view mode ([ for old, ] for new)
    let single_side = match state.side_view {
        DiffSideView::OldOnly => Some(DiffPanel::Old),
        DiffSideView::NewOnly => Some(DiffPanel::New),
        DiffSideView::Both => None,
    };

    let visible_height = inner.height as usize;
    let buf = frame.buffer_mut();

    if single_side.is_some() || is_new_file {
        // Single-panel mode: new file, old-only, or new-only
        let show_panel = single_side.unwrap_or(DiffPanel::New); // new-file defaults to New
        let content_width = inner.width.saturating_sub(gutter_width);

        let mut row = 0usize;
        for (idx_offset, diff_line) in state.lines[state.scroll_offset..].iter().enumerate() {
            if row >= visible_height {
                break;
            }
            let line_idx = state.scroll_offset + idx_offset;

            // Handle file header separator lines
            if let Some(ref header) = diff_line.file_header {
                let y = inner.y + row as u16;
                render_file_header(buf, inner.x, y, inner.width, header, theme);
                row += 1;
                continue;
            }

            let default_hl = FileHighlighter::default();
            let (old_highlighter, new_highlighter) = state
                .highlighters_for_section(diff_line.section_index)
                .unwrap_or((&default_hl, &default_hl));

            // Pick the appropriate side's data
            let (line_data, segments, is_old_side, highlighter) = match show_panel {
                DiffPanel::Old => (
                    &diff_line.old_line,
                    &diff_line.old_segments,
                    true,
                    old_highlighter,
                ),
                DiffPanel::New => (
                    &diff_line.new_line,
                    &diff_line.new_segments,
                    false,
                    new_highlighter,
                ),
            };

            let bg = if is_new_file {
                theme.diff_add_bg
            } else {
                match (diff_line.change_type, show_panel) {
                    (ChangeType::Delete, DiffPanel::Old) => theme.diff_remove_bg,
                    (ChangeType::Insert, DiffPanel::New) => theme.diff_add_bg,
                    (ChangeType::Modified, DiffPanel::Old) => theme.diff_remove_bg,
                    (ChangeType::Modified, DiffPanel::New) => theme.diff_add_bg,
                    _ => Color::Reset,
                }
            };
            let gutter_bg = if is_new_file {
                theme.diff_add_gutter_bg
            } else {
                match (diff_line.change_type, show_panel) {
                    (ChangeType::Delete, DiffPanel::Old) => theme.diff_remove_gutter_bg,
                    (ChangeType::Insert, DiffPanel::New) => theme.diff_add_gutter_bg,
                    (ChangeType::Modified, DiffPanel::Old) => theme.diff_remove_gutter_bg,
                    (ChangeType::Modified, DiffPanel::New) => theme.diff_add_gutter_bg,
                    _ => Color::Reset,
                }
            };
            let gutter_fg = if is_new_file {
                theme.diff_add_gutter_fg
            } else {
                match (diff_line.change_type, show_panel) {
                    (ChangeType::Delete, DiffPanel::Old) => theme.diff_remove_gutter_fg,
                    (ChangeType::Insert, DiffPanel::New) => theme.diff_add_gutter_fg,
                    (ChangeType::Modified, DiffPanel::Old) => theme.diff_remove_gutter_fg,
                    (ChangeType::Modified, DiffPanel::New) => theme.diff_add_gutter_fg,
                    _ => theme.diff_gutter,
                }
            };

            let line_num = state
                .file_line_number(line_idx, show_panel)
                .map(|n| format!("{:>4} ", n))
                .unwrap_or_else(|| "     ".to_string());
            let gutter_style = Style::default().fg(gutter_fg).bg(gutter_bg);

            if state.wrap && line_data.is_some() {
                let spans = build_content_spans(
                    line_data.as_ref().map(|(n, t)| (*n, t.as_str())),
                    segments,
                    diff_line.change_type,
                    is_old_side,
                    highlighter,
                    bg,
                    theme,
                    usize::MAX / 2,
                );
                let wrapped = wrap_spans(&spans, content_width as usize);
                for (chunk_idx, chunk) in wrapped.iter().enumerate() {
                    if row >= visible_height {
                        break;
                    }
                    let y = inner.y + row as u16;
                    let gutter_text = if chunk_idx == 0 {
                        line_num.clone()
                    } else {
                        "   · ".to_string()
                    };
                    buf_write_str(buf, inner.x, y, &gutter_text, gutter_style, gutter_width);
                    buf_write_spans(buf, inner.x + gutter_width, y, chunk, content_width, 0);
                    row += 1;
                }
            } else {
                let y = inner.y + row as u16;
                buf_write_str(buf, inner.x, y, &line_num, gutter_style, gutter_width);
                if line_data.is_some() {
                    let spans = build_content_spans(
                        line_data.as_ref().map(|(n, t)| (*n, t.as_str())),
                        segments,
                        diff_line.change_type,
                        is_old_side,
                        highlighter,
                        bg,
                        theme,
                        content_width as usize,
                    );
                    buf_write_spans(
                        buf,
                        inner.x + gutter_width,
                        y,
                        &spans,
                        content_width,
                        state.horizontal_scroll,
                    );
                } else {
                    let fill: String = std::iter::repeat(' ')
                        .take(content_width as usize)
                        .collect();
                    buf_write_str(
                        buf,
                        inner.x + gutter_width,
                        y,
                        &fill,
                        Style::default().bg(bg),
                        content_width,
                    );
                }
                row += 1;
            }
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
        let div_x = inner.x + gutter_width + panel_width;
        let right_gutter_x = div_x + divider_width;
        let right_content_x = right_gutter_x + gutter_width;
        let right_content_width = inner
            .width
            .saturating_sub(gutter_width * 2 + panel_width + divider_width);

        let mut hover_tooltip_y: Option<u16> = None;

        let mut row = 0usize;
        for (idx_offset, diff_line) in state.lines[state.scroll_offset..].iter().enumerate() {
            if row >= visible_height {
                break;
            }
            let line_idx = state.scroll_offset + idx_offset;

            // Handle file header separator lines
            if let Some(ref header) = diff_line.file_header {
                let y = inner.y + row as u16;
                render_file_header(buf, inner.x, y, inner.width, header, theme);
                row += 1;
                continue;
            }

            let default_hl = FileHighlighter::default();
            let (old_highlighter, new_highlighter) = state
                .highlighters_for_section(diff_line.section_index)
                .unwrap_or((&default_hl, &default_hl));

            let (left_bg, right_bg) = line_bg_colors(diff_line.change_type, theme);
            let (left_gutter_bg, right_gutter_bg) = gutter_bg_colors(diff_line.change_type, theme);
            let (left_gutter_fg, right_gutter_fg) = gutter_fg_colors(diff_line.change_type, theme);
            let gutter_style = Style::default().fg(left_gutter_fg).bg(left_gutter_bg);
            let right_gutter_style = Style::default().fg(right_gutter_fg).bg(right_gutter_bg);
            let divider_style = Style::default().fg(theme.diff_gutter);

            let left_num = state
                .file_line_number(line_idx, DiffPanel::Old)
                .map(|n| format!("{:>4} ", n))
                .unwrap_or_else(|| "     ".to_string());
            let right_num = state
                .file_line_number(line_idx, DiffPanel::New)
                .map(|n| format!("{:>4} ", n))
                .unwrap_or_else(|| "     ".to_string());

            let is_insert = diff_line.change_type == ChangeType::Insert;
            let is_delete = diff_line.change_type == ChangeType::Delete;

            if state.wrap {
                // Build wrapped rows for each side
                let left_wrapped: Vec<Vec<Span<'_>>> = if is_insert {
                    vec![] // placeholder; slash fill rendered per row
                } else {
                    let spans = build_content_spans(
                        diff_line.old_line.as_ref().map(|(n, t)| (*n, t.as_str())),
                        &diff_line.old_segments,
                        diff_line.change_type,
                        true,
                        old_highlighter,
                        left_bg,
                        theme,
                        usize::MAX / 2,
                    );
                    wrap_spans(&spans, panel_width as usize)
                };
                let right_wrapped: Vec<Vec<Span<'_>>> = if is_delete {
                    vec![] // placeholder; slash fill rendered per row
                } else {
                    let spans = build_content_spans(
                        diff_line.new_line.as_ref().map(|(n, t)| (*n, t.as_str())),
                        &diff_line.new_segments,
                        diff_line.change_type,
                        false,
                        new_highlighter,
                        right_bg,
                        theme,
                        usize::MAX / 2,
                    );
                    wrap_spans(&spans, right_content_width as usize)
                };

                let num_rows = if is_insert {
                    right_wrapped.len().max(1)
                } else if is_delete {
                    left_wrapped.len().max(1)
                } else {
                    left_wrapped.len().max(right_wrapped.len()).max(1)
                };

                for chunk_idx in 0..num_rows {
                    if row >= visible_height {
                        break;
                    }
                    let y = inner.y + row as u16;

                    let left_gutter_text = if chunk_idx == 0 {
                        left_num.clone()
                    } else {
                        "   · ".to_string()
                    };
                    let right_gutter_text = if chunk_idx == 0 {
                        right_num.clone()
                    } else {
                        "   · ".to_string()
                    };

                    // Left gutter + content
                    buf_write_str(
                        buf,
                        inner.x,
                        y,
                        &left_gutter_text,
                        gutter_style,
                        gutter_width,
                    );
                    if is_insert {
                        let slash: String =
                            std::iter::repeat('/').take(panel_width as usize).collect();
                        buf_write_str(
                            buf,
                            inner.x + gutter_width,
                            y,
                            &slash,
                            Style::default().fg(theme.diff_line_number).bg(left_bg),
                            panel_width,
                        );
                    } else if let Some(chunk) = left_wrapped.get(chunk_idx) {
                        buf_write_spans(buf, inner.x + gutter_width, y, chunk, panel_width, 0);
                    } else {
                        let fill: String =
                            std::iter::repeat(' ').take(panel_width as usize).collect();
                        buf_write_str(
                            buf,
                            inner.x + gutter_width,
                            y,
                            &fill,
                            Style::default().bg(left_bg),
                            panel_width,
                        );
                    }

                    // Divider or revert marker (first visual row of a hunk only).
                    let show_marker = show_revert_markers
                        && !state.wrap
                        && chunk_idx == 0
                        && state.is_hunk_start_line(line_idx);
                    let marker_hunk_idx = if show_marker {
                        state.hunk_index_for_start_line(line_idx)
                    } else {
                        None
                    };
                    let marker_is_hovered = show_marker
                        && marker_hunk_idx.is_some()
                        && marker_hunk_idx == state.hovered_revert_hunk;
                    let (divider_char, marker_style) = if show_marker {
                        let is_selected = marker_hunk_idx == state.selected_revert_hunk;
                        // Hover wins over selection so the hover state is always
                        // visible — even on a hunk that's currently selected.
                        let fg = if marker_is_hovered {
                            theme.accent_secondary
                        } else if is_selected {
                            theme.accent
                        } else {
                            theme.separator
                        };
                        ("󰧛", Style::default().fg(fg).add_modifier(Modifier::BOLD))
                    } else {
                        ("│", divider_style)
                    };
                    buf_write_str(buf, div_x, y, "  ", divider_style, divider_width);
                    buf_write_str(buf, div_x, y, divider_char, marker_style, divider_width);
                    if show_marker
                        && marker_hunk_idx.is_some()
                        && marker_hunk_idx == state.hovered_revert_hunk
                    {
                        hover_tooltip_y = Some(y);
                    }

                    // Right gutter + content
                    buf_write_str(
                        buf,
                        right_gutter_x,
                        y,
                        &right_gutter_text,
                        right_gutter_style,
                        gutter_width,
                    );
                    if is_delete {
                        let slash: String = std::iter::repeat('/')
                            .take(right_content_width as usize)
                            .collect();
                        buf_write_str(
                            buf,
                            right_content_x,
                            y,
                            &slash,
                            Style::default().fg(theme.diff_line_number).bg(right_bg),
                            right_content_width,
                        );
                    } else if let Some(chunk) = right_wrapped.get(chunk_idx) {
                        buf_write_spans(buf, right_content_x, y, chunk, right_content_width, 0);
                    } else {
                        let fill: String = std::iter::repeat(' ')
                            .take(right_content_width as usize)
                            .collect();
                        buf_write_str(
                            buf,
                            right_content_x,
                            y,
                            &fill,
                            Style::default().bg(right_bg),
                            right_content_width,
                        );
                    }

                    row += 1;
                }
            } else {
                let y = inner.y + row as u16;

                // Left gutter
                buf_write_str(buf, inner.x, y, &left_num, gutter_style, gutter_width);

                // Left content
                let left_spans = if is_insert {
                    let slash_fill: String =
                        std::iter::repeat('/').take(panel_width as usize).collect();
                    vec![Span::styled(
                        slash_fill,
                        Style::default().fg(theme.diff_line_number).bg(left_bg),
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
                buf_write_spans(
                    buf,
                    inner.x + gutter_width,
                    y,
                    &left_spans,
                    panel_width,
                    state.horizontal_scroll,
                );

                // Divider or revert marker.
                let show_marker =
                    show_revert_markers && !state.wrap && state.is_hunk_start_line(line_idx);
                let marker_hunk_idx = if show_marker {
                    state.hunk_index_for_start_line(line_idx)
                } else {
                    None
                };
                let marker_is_hovered = show_marker
                    && marker_hunk_idx.is_some()
                    && marker_hunk_idx == state.hovered_revert_hunk;
                let (divider_char, marker_style) = if show_marker {
                    let is_selected = marker_hunk_idx == state.selected_revert_hunk;
                    let fg = if marker_is_hovered {
                        theme.accent_secondary
                    } else if is_selected {
                        theme.accent
                    } else {
                        theme.separator
                    };
                    ("󰧛", Style::default().fg(fg).add_modifier(Modifier::BOLD))
                } else {
                    ("│", divider_style)
                };
                buf_write_str(buf, div_x, y, "  ", divider_style, divider_width);
                buf_write_str(buf, div_x, y, divider_char, marker_style, divider_width);
                if show_marker
                    && marker_hunk_idx.is_some()
                    && marker_hunk_idx == state.hovered_revert_hunk
                {
                    hover_tooltip_y = Some(y);
                }

                // Right gutter
                buf_write_str(
                    buf,
                    right_gutter_x,
                    y,
                    &right_num,
                    right_gutter_style,
                    gutter_width,
                );

                // Right content
                let right_spans = if is_delete {
                    let slash_fill: String =
                        std::iter::repeat('/').take(panel_width as usize).collect();
                    vec![Span::styled(
                        slash_fill,
                        Style::default().fg(theme.diff_line_number).bg(right_bg),
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
                buf_write_spans(
                    buf,
                    right_content_x,
                    y,
                    &right_spans,
                    right_content_width,
                    state.horizontal_scroll,
                );

                row += 1;
            }
        }

        if let Some(y) = hover_tooltip_y {
            let show_key = state.hovered_revert_hunk.is_some()
                && state.hovered_revert_hunk == state.selected_revert_hunk;
            render_revert_tooltip(
                buf,
                div_x + divider_width,
                y,
                right_content_width + gutter_width,
                theme,
                show_key,
            );
        }
    }
}

fn render_revert_tooltip(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    max_width: u16,
    theme: &Theme,
    show_key: bool,
) {
    let tip_style = Style::default().bg(theme.selected_bg).fg(theme.text_strong);
    let key_style = Style::default()
        .bg(theme.selected_bg)
        .fg(theme.accent_secondary)
        .add_modifier(Modifier::BOLD);

    let parts: Vec<(&str, Style)> = if show_key {
        vec![
            (" ", tip_style),
            ("enter", key_style),
            (" Revert hunk ", tip_style),
        ]
    } else {
        vec![(" Revert hunk ", tip_style)]
    };

    let buf_area = buf.area();
    if y < buf_area.y || y >= buf_area.y + buf_area.height {
        return;
    }
    let max_x = (x + max_width).min(buf_area.x + buf_area.width);

    let mut col = x;
    for (text, style) in &parts {
        for ch in text.chars() {
            if col >= max_x {
                return;
            }
            if let Some(cell) = buf.cell_mut((col, y)) {
                cell.set_char(ch);
                cell.set_style(*style);
            }
            col += 1;
        }
    }
}

/// Render a file header separator line spanning the full width.
fn render_file_header(buf: &mut Buffer, x: u16, y: u16, width: u16, filename: &str, theme: &Theme) {
    let buf_area = buf.area();
    if y < buf_area.y || y >= buf_area.y + buf_area.height {
        return;
    }

    let header_style = Style::default()
        .fg(theme.diff_selection_fg)
        .bg(theme.diff_selection_bg)
        .add_modifier(Modifier::BOLD);

    // Build header text: "── filename ──────..."
    let prefix = "── ";
    let suffix_char = '─';
    let label = format!("{}{} ", prefix, filename);
    let remaining = (width as usize).saturating_sub(label.len());
    let full_line = format!("{}{}", label, suffix_char.to_string().repeat(remaining));

    buf_write_str(buf, x, y, &full_line, header_style, width);
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
fn buf_write_spans(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    spans: &[Span<'_>],
    max_width: u16,
    h_scroll: usize,
) {
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

/// Split a list of styled spans into visual rows of at most `width` display columns each.
/// Used by wrap mode to soft-wrap long diff lines.
fn wrap_spans<'a>(spans: &[Span<'a>], width: usize) -> Vec<Vec<Span<'a>>> {
    if width == 0 {
        return vec![vec![]];
    }

    // Collect (char, style) pairs, skipping zero-width control chars
    let pairs: Vec<(char, Style)> = spans
        .iter()
        .flat_map(|sp| {
            let style = sp.style;
            sp.content.chars().filter_map(move |ch| {
                if unicode_display_width(ch) > 0 {
                    Some((ch, style))
                } else {
                    None
                }
            })
        })
        .collect();

    if pairs.is_empty() {
        return vec![vec![]];
    }

    let mut rows: Vec<Vec<Span<'a>>> = Vec::new();
    let mut start = 0;

    while start < pairs.len() {
        let mut col_w = 0usize;
        let mut end = start;

        while end < pairs.len() {
            let w = unicode_display_width(pairs[end].0);
            if col_w + w > width {
                break;
            }
            col_w += w;
            end += 1;
        }
        // Avoid infinite loop when a single char is wider than `width`
        if end == start {
            end = start + 1;
        }

        // Group consecutive chars with the same style into spans
        let mut row_spans: Vec<Span<'a>> = Vec::new();
        let mut i = start;
        while i < end {
            let style = pairs[i].1;
            let mut text = String::new();
            while i < end && pairs[i].1 == style {
                text.push(pairs[i].0);
                i += 1;
            }
            row_spans.push(Span::styled(text, style));
        }

        rows.push(row_spans);
        start = end;
    }

    rows
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

/// Get gutter background colors for a diff line. Slightly darker than the
/// content background so the gutter visually separates from the code area
/// (lumen-style).
fn gutter_bg_colors(change_type: ChangeType, theme: &Theme) -> (Color, Color) {
    match change_type {
        ChangeType::Equal => (Color::Reset, Color::Reset),
        ChangeType::Delete => (theme.diff_remove_gutter_bg, Color::Reset),
        ChangeType::Insert => (Color::Reset, theme.diff_add_gutter_bg),
        ChangeType::Modified => (theme.diff_remove_gutter_bg, theme.diff_add_gutter_bg),
    }
}

/// Foreground color for the gutter line number on the (left, right) side.
fn gutter_fg_colors(change_type: ChangeType, theme: &Theme) -> (Color, Color) {
    match change_type {
        ChangeType::Equal => (theme.diff_gutter, theme.diff_gutter),
        ChangeType::Delete => (theme.diff_remove_gutter_fg, theme.diff_gutter),
        ChangeType::Insert => (theme.diff_gutter, theme.diff_add_gutter_fg),
        ChangeType::Modified => (theme.diff_remove_gutter_fg, theme.diff_add_gutter_fg),
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
    let highlighted = highlighter.get_line_spans(line_num, Some(bg), theme);
    if !highlighted.is_empty() {
        return highlighted;
    }

    // Fallback: plain text with background
    let fg = match change_type {
        ChangeType::Delete => theme.diff_remove.fg.unwrap_or(Color::Red),
        ChangeType::Insert => theme.diff_add.fg.unwrap_or(Color::Green),
        _ => theme.syntax_default,
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
                        .fg(theme.text_strong)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    seg.text.clone(),
                    Style::default().bg(bg).fg(theme.syntax_default),
                )
            }
        })
        .collect()
}

/// Render search highlights over the diff buffer by scanning visible content areas
/// for occurrences of the search query and applying a highlight style.
/// `current_match_line` is the line index of the currently selected match (for emphasis).
pub fn render_diff_search_highlights(
    frame: &mut Frame,
    area: Rect,
    state: &DiffViewState,
    theme: &Theme,
) {
    if state.search_query.is_empty() || state.search_matches.is_empty() {
        return;
    }

    let pl = DiffPanelLayout::compute(area, state);
    let query_lower = state.search_query.to_lowercase();
    let query_len = query_lower.len();
    let current_match_line = state
        .search_matches
        .get(state.search_match_idx)
        .map(|m| m.line_idx);

    let buf = frame.buffer_mut();
    let buf_area = *buf.area();

    // Scan each content area (old panel, new panel) for matches
    let panel_ranges: Vec<(u16, u16)> = {
        let mut ranges = Vec::new();
        if pl.old_content_x > 0 && pl.old_content_end_x > pl.old_content_x {
            ranges.push((pl.old_content_x, pl.old_content_end_x));
        }
        if pl.new_content_x > 0 && pl.new_content_end_x > pl.new_content_x {
            ranges.push((pl.new_content_x, pl.new_content_end_x));
        }
        ranges
    };

    let visible_height = area.height.saturating_sub(2) as usize; // -2 for borders
    let highlight_style = Style::default()
        .bg(theme.diff_search_highlight_bg)
        .fg(theme.diff_search_highlight_fg);
    let current_highlight_style = Style::default()
        .bg(theme.diff_search_cursor_bg)
        .fg(theme.diff_search_cursor_fg)
        .add_modifier(Modifier::BOLD);

    for row_offset in 0..visible_height {
        let line_idx = state.scroll_offset + row_offset;
        let y = pl.inner_y + row_offset as u16;
        if y >= pl.inner_end_y || y >= buf_area.y + buf_area.height {
            break;
        }

        let is_current_line = current_match_line == Some(line_idx);

        for &(range_start, range_end) in &panel_ranges {
            // Read the row text from buffer cells in this range
            let mut row_chars: Vec<(u16, char)> = Vec::new();
            for x in range_start..range_end.min(buf_area.x + buf_area.width) {
                if let Some(cell) = buf.cell((x, y)) {
                    let ch = cell.symbol().chars().next().unwrap_or(' ');
                    row_chars.push((x, ch));
                }
            }

            // Build string for searching
            let row_text: String = row_chars.iter().map(|(_, ch)| *ch).collect();
            let row_lower = row_text.to_lowercase();

            let mut start = 0;
            while let Some(pos) = row_lower[start..].find(&query_lower) {
                let match_start = start + pos;
                let match_end = match_start + query_len;
                let style = if is_current_line {
                    current_highlight_style
                } else {
                    highlight_style
                };
                for i in match_start..match_end {
                    if i < row_chars.len() {
                        let x = row_chars[i].0;
                        if let Some(cell) = buf.cell_mut((x, y)) {
                            cell.set_style(style);
                        }
                    }
                }
                start = match_start + 1;
            }
        }
    }
}

/// Render a search bar at the bottom of the diff panel area.
pub fn render_diff_search_bar(frame: &mut Frame, area: Rect, state: &DiffViewState, theme: &Theme) {
    // Only render if search is active (typing) or has a query (dismissed but results shown)
    if !state.search_active && state.search_query.is_empty() {
        return;
    }

    // Position at the bottom row of the panel (inside the border)
    let bar_y = area.y + area.height.saturating_sub(2);
    let bar_x = area.x + 1;
    let bar_width = area.width.saturating_sub(2);

    if bar_width < 10 {
        return;
    }

    let bar_rect = Rect::new(bar_x, bar_y, bar_width, 1);

    // Clear the bar area
    let buf = frame.buffer_mut();
    for x in bar_rect.x..bar_rect.x + bar_rect.width {
        if let Some(cell) = buf.cell_mut((x, bar_y)) {
            cell.set_char(' ');
            cell.set_style(Style::default().bg(theme.diff_grid_bg));
        }
    }

    let match_info = if !state.search_matches.is_empty() {
        format!(
            " {}/{}",
            state.search_match_idx + 1,
            state.search_matches.len()
        )
    } else if !state.search_query.is_empty() {
        " (no matches)".to_string()
    } else {
        String::new()
    };

    if state.search_active {
        // Render with textarea
        let prefix_width = 2u16; // " /"
        let suffix_width = match_info.len() as u16;
        let ta_width = bar_width.saturating_sub(prefix_width + suffix_width);

        let prefix_rect = Rect::new(bar_rect.x, bar_y, prefix_width, 1);
        let prefix = Paragraph::new(Span::styled(
            " /",
            Style::default()
                .fg(theme.diff_grid_fg)
                .bg(theme.diff_grid_bg),
        ));
        frame.render_widget(prefix, prefix_rect);

        if let Some(ref ta) = state.search_textarea {
            let ta_rect = Rect::new(bar_rect.x + prefix_width, bar_y, ta_width, 1);
            frame.render_widget(&*ta, ta_rect);
        }

        if !match_info.is_empty() {
            let suffix_rect =
                Rect::new(bar_rect.x + prefix_width + ta_width, bar_y, suffix_width, 1);
            let suffix = Paragraph::new(Span::styled(
                match_info,
                Style::default()
                    .fg(theme.diff_grid_fg)
                    .bg(theme.diff_grid_bg),
            ));
            frame.render_widget(suffix, suffix_rect);
        }
    } else {
        // Dismissed search — show query + match info
        let text = format!(" /{}{}", state.search_query, match_info);
        let style = Style::default()
            .fg(theme.diff_grid_fg)
            .bg(theme.diff_grid_bg);
        buf_write_str(
            frame.buffer_mut(),
            bar_rect.x,
            bar_y,
            &text,
            style,
            bar_width,
        );
    }
}

/// Parse a multi-file unified diff into per-file sections.
/// Returns Vec of (filename, raw_diff_for_that_file).
fn parse_multi_file_diff(diff: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    let mut current_filename = String::new();
    let mut current_lines: Vec<&str> = Vec::new();

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            // Save previous section
            if !current_filename.is_empty() {
                sections.push((current_filename, current_lines));
                current_lines = Vec::new();
            }
            // Extract filename from "diff --git a/path b/path"
            current_filename = extract_filename_from_diff_header(line);
        } else {
            current_lines.push(line);
        }
    }

    // Save last section
    if !current_filename.is_empty() {
        sections.push((current_filename, current_lines));
    }

    sections
        .into_iter()
        .map(|(name, lines)| (name, lines.join("\n")))
        .collect()
}

/// Extract the filename from a "diff --git a/path b/path" header line.
fn extract_filename_from_diff_header(line: &str) -> String {
    // Format: "diff --git a/some/path b/some/path"
    // We want "some/path" (the b/ side, which is the new name)
    if let Some(b_part) = line.split(" b/").last() {
        b_part.to_string()
    } else {
        // Fallback: strip "diff --git " prefix
        line.trim_start_matches("diff --git ").to_string()
    }
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

/// Parse hunk headers from a unified diff, returning
/// `(old_start, new_start, old_count, new_count)` for each hunk.
fn parse_hunk_headers(diff: &str) -> Vec<(usize, usize, usize, usize)> {
    let mut hunks = Vec::new();
    for line in diff.lines() {
        if !line.starts_with("@@") {
            continue;
        }
        // Format: @@ -OLD_START[,OLD_COUNT] +NEW_START[,NEW_COUNT] @@
        let inner = line
            .trim_start_matches('@')
            .trim_start()
            .split("@@")
            .next()
            .unwrap_or("");
        let mut old_start = 1usize;
        let mut old_count = 1usize;
        let mut new_start = 1usize;
        let mut new_count = 1usize;
        for token in inner.split_whitespace() {
            if let Some(rest) = token.strip_prefix('-') {
                let mut parts = rest.splitn(2, ',');
                old_start = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                old_count = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
            } else if let Some(rest) = token.strip_prefix('+') {
                let mut parts = rest.splitn(2, ',');
                new_start = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                new_count = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
            }
        }
        hunks.push((old_start, new_start, old_count, new_count));
    }
    hunks
}

/// Build the hunk line offset table from parsed hunk headers and
/// computed DiffLines. Each entry is `(first_diff_line_idx, old_offset, new_offset)`.
fn build_hunk_line_offsets(
    hunks: &[(usize, usize, usize, usize)],
    lines: &[DiffLine],
    file_header_count: usize,
) -> Vec<(usize, usize, usize)> {
    if hunks.is_empty() {
        return Vec::new();
    }

    let mut offsets = Vec::new();
    let mut cumulative_old = 0usize; // content-relative old line count before this hunk
    let mut cumulative_new = 0usize;

    for (hunk_idx, &(old_start, new_start, old_count, new_count)) in hunks.iter().enumerate() {
        // The content line numbers for this hunk start at cumulative + 1
        let content_old_start = cumulative_old + 1;
        let content_new_start = cumulative_new + 1;

        // Find the first DiffLine that belongs to this hunk by matching
        // content line numbers.
        let first_line_idx = if hunk_idx == 0 {
            // First hunk: starts at the first non-header DiffLine
            file_header_count
        } else {
            // Find the first DiffLine whose old_line or new_line number
            // matches the content start of this hunk.
            lines
                .iter()
                .enumerate()
                .skip(file_header_count)
                .find(|(_, dl)| {
                    dl.old_line
                        .as_ref()
                        .map(|(n, _)| *n >= content_old_start)
                        .unwrap_or(false)
                        || dl
                            .new_line
                            .as_ref()
                            .map(|(n, _)| *n >= content_new_start)
                            .unwrap_or(false)
                })
                .map(|(idx, _)| idx)
                .unwrap_or(0)
        };

        // Offset: actual file line - content line
        let old_offset = old_start.saturating_sub(content_old_start);
        let new_offset = new_start.saturating_sub(content_new_start);

        offsets.push((first_line_idx, old_offset, new_offset));

        cumulative_old += old_count;
        cumulative_new += new_count;
    }

    offsets
}
