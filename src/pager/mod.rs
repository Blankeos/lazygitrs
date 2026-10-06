pub mod diff_algo;
pub mod highlight;
pub mod image_preview;
pub mod side_by_side;
pub mod word_diff;

/// Types shared across the pager module.

/// Represents a single line in a side-by-side diff.
#[derive(Debug, Clone)]
pub struct DiffLine {
    /// Left side: (line_number, text). None if this line only exists on the right.
    pub old_line: Option<(usize, String)>,
    /// Right side: (line_number, text). None if this line only exists on the left.
    pub new_line: Option<(usize, String)>,
    /// What kind of change this line represents.
    pub change_type: ChangeType,
    /// Word-level diff segments for the old (left) side.
    pub old_segments: Option<Vec<InlineSegment>>,
    /// Word-level diff segments for the new (right) side.
    pub new_segments: Option<Vec<InlineSegment>>,
    /// If set, this line is a file header separator (multi-file diffs).
    pub file_header: Option<String>,
    /// Non-source placeholder row. Space-only labels provide vertical padding without source content.
    pub preview_placeholder: Option<&'static str>,
    /// Index of the file section this line belongs to (for multi-file highlighting).
    pub section_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Equal,
    Delete,
    Insert,
    Modified,
}

#[derive(Debug, Clone)]
pub struct InlineSegment {
    pub text: String,
    pub emphasized: bool,
}

/// Expand tabs to spaces.
pub fn expand_tabs(s: &str, tab_width: usize) -> String {
    let mut result = String::with_capacity(s.len());
    let mut col = 0;
    for c in s.chars() {
        if c == '\t' {
            let spaces = tab_width - (col % tab_width);
            for _ in 0..spaces {
                result.push(' ');
            }
            col += spaces;
        } else {
            result.push(c);
            col += 1;
        }
    }
    result
}
