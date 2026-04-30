use similar::{ChangeTag, TextDiff};

use super::{expand_tabs, ChangeType, DiffLine};
use super::word_diff::compute_word_diff;

/// Computes a side-by-side diff using GitHub-style pairing.
///
/// Consecutive deletions are paired with consecutive insertions on the same row.
pub fn compute_side_by_side(old: &str, new: &str, tab_width: usize) -> Vec<DiffLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = Vec::new();
    let mut old_num = 1usize;
    let mut new_num = 1usize;

    let changes: Vec<_> = diff.iter_all_changes().collect();
    let mut i = 0;

    while i < changes.len() {
        let change = &changes[i];

        match change.tag() {
            ChangeTag::Equal => {
                let text = expand_tabs(change.value().trim_end(), tab_width);
                lines.push(DiffLine {
                    old_line: Some((old_num, text.clone())),
                    new_line: Some((new_num, text)),
                    change_type: ChangeType::Equal,
                    old_segments: None,
                    new_segments: None,
                    file_header: None,
                    section_index: 0,
                });
                old_num += 1;
                new_num += 1;
                i += 1;
            }
            ChangeTag::Delete => {
                // Collect consecutive deletions
                let mut deletions = Vec::new();
                while i < changes.len() && changes[i].tag() == ChangeTag::Delete {
                    deletions.push((
                        old_num,
                        expand_tabs(changes[i].value().trim_end(), tab_width),
                    ));
                    old_num += 1;
                    i += 1;
                }

                // Collect consecutive insertions that follow
                let mut insertions = Vec::new();
                while i < changes.len() && changes[i].tag() == ChangeTag::Insert {
                    insertions.push((
                        new_num,
                        expand_tabs(changes[i].value().trim_end(), tab_width),
                    ));
                    new_num += 1;
                    i += 1;
                }

                // Pair deletions with insertions
                let max_len = deletions.len().max(insertions.len());
                for j in 0..max_len {
                    let old_line = deletions.get(j).cloned();
                    let new_line = insertions.get(j).cloned();

                    let change_type = match (&old_line, &new_line) {
                        (Some(_), Some(_)) => ChangeType::Modified,
                        (Some(_), None) => ChangeType::Delete,
                        (None, Some(_)) => ChangeType::Insert,
                        (None, None) => unreachable!(),
                    };

                    let (old_segments, new_segments) =
                        if matches!(change_type, ChangeType::Modified) {
                            let old_text =
                                old_line.as_ref().map(|(_, t)| t.as_str()).unwrap_or("");
                            let new_text =
                                new_line.as_ref().map(|(_, t)| t.as_str()).unwrap_or("");
                            if let Some((old_segs, new_segs)) =
                                compute_word_diff(old_text, new_text)
                            {
                                (Some(old_segs), Some(new_segs))
                            } else {
                                (None, None)
                            }
                        } else {
                            (None, None)
                        };

                    lines.push(DiffLine {
                        old_line,
                        new_line,
                        change_type,
                        old_segments,
                        new_segments,
                        file_header: None,
                        section_index: 0,
                    });
                }
            }
            ChangeTag::Insert => {
                lines.push(DiffLine {
                    old_line: None,
                    new_line: Some((
                        new_num,
                        expand_tabs(change.value().trim_end(), tab_width),
                    )),
                    change_type: ChangeType::Insert,
                    old_segments: None,
                    new_segments: None,
                    file_header: None,
                    section_index: 0,
                });
                new_num += 1;
                i += 1;
            }
        }
    }
    lines
}

/// Build side-by-side `DiffLine`s directly from a unified diff's per-line
/// markers (`-`, `+`, ` `), without re-running a diff algorithm.
///
/// This trusts git's classification, which avoids the cross-hunk aliasing
/// problems that arise when concatenating multiple hunks' content together
/// and re-diffing with Myers (consecutive blank lines or repeated context
/// lines can otherwise be matched ambiguously).
///
/// Line numbers in the returned `DiffLine`s are 1-based positions inside the
/// concatenated old/new content (as produced by `parse_unified_diff`); use
/// `build_hunk_line_offsets` to translate them to actual file line numbers.
pub fn compute_side_by_side_from_unified_diff(diff: &str, tab_width: usize) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut old_num = 1usize;
    let mut new_num = 1usize;
    let mut in_hunk = false;
    let mut pending_dels: Vec<(usize, String)> = Vec::new();
    let mut pending_ins: Vec<(usize, String)> = Vec::new();

    fn flush(
        dels: &mut Vec<(usize, String)>,
        ins: &mut Vec<(usize, String)>,
        out: &mut Vec<DiffLine>,
    ) {
        let max_len = dels.len().max(ins.len());
        for j in 0..max_len {
            let old_line = dels.get(j).cloned();
            let new_line = ins.get(j).cloned();
            let change_type = match (&old_line, &new_line) {
                (Some(_), Some(_)) => ChangeType::Modified,
                (Some(_), None) => ChangeType::Delete,
                (None, Some(_)) => ChangeType::Insert,
                (None, None) => unreachable!(),
            };
            let (old_segments, new_segments) = if matches!(change_type, ChangeType::Modified) {
                let old_text = old_line.as_ref().map(|(_, t)| t.as_str()).unwrap_or("");
                let new_text = new_line.as_ref().map(|(_, t)| t.as_str()).unwrap_or("");
                if let Some((o, n)) = compute_word_diff(old_text, new_text) {
                    (Some(o), Some(n))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };
            out.push(DiffLine {
                old_line,
                new_line,
                change_type,
                old_segments,
                new_segments,
                file_header: None,
                section_index: 0,
            });
        }
        dels.clear();
        ins.clear();
    }

    for line in diff.lines() {
        if line.starts_with("@@") {
            flush(&mut pending_dels, &mut pending_ins, &mut lines);
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        if line.starts_with("\\") {
            // "\ No newline at end of file" marker — ignore
            continue;
        }
        if let Some(rest) = line.strip_prefix('-') {
            pending_dels.push((old_num, expand_tabs(rest.trim_end(), tab_width)));
            old_num += 1;
        } else if let Some(rest) = line.strip_prefix('+') {
            pending_ins.push((new_num, expand_tabs(rest.trim_end(), tab_width)));
            new_num += 1;
        } else {
            // Context line: " text" (or a bare empty line in some diffs)
            flush(&mut pending_dels, &mut pending_ins, &mut lines);
            let ctx = line.strip_prefix(' ').unwrap_or(line);
            let text = expand_tabs(ctx.trim_end(), tab_width);
            lines.push(DiffLine {
                old_line: Some((old_num, text.clone())),
                new_line: Some((new_num, text)),
                change_type: ChangeType::Equal,
                old_segments: None,
                new_segments: None,
                file_header: None,
                section_index: 0,
            });
            old_num += 1;
            new_num += 1;
        }
    }
    flush(&mut pending_dels, &mut pending_ins, &mut lines);
    lines
}

/// Find the start indices of change hunks in the diff output.
pub fn find_hunk_starts(lines: &[DiffLine]) -> Vec<usize> {
    let mut hunks = Vec::new();
    let mut in_hunk = false;

    for (i, line) in lines.iter().enumerate() {
        let is_change = !matches!(line.change_type, ChangeType::Equal);
        if is_change && !in_hunk {
            hunks.push(i);
            in_hunk = true;
        } else if !is_change {
            in_hunk = false;
        }
    }
    hunks
}
