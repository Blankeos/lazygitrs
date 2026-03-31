/// Commit graph layout engine matching lazygit's visual style.
///
/// Uses ◯ (hollow circle) for the HEAD node and ⬤ (solid circle) for others,
/// ╭─╮ for merge connectors, ╰─╯ for converging lines.
/// Each column is 2 chars wide for readability.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::config::Theme;

pub fn col_color(col: usize, theme: &Theme) -> Color {
    theme.graph_colors[col % theme.graph_colors.len()]
}

/// The computed graph data for one commit row.
#[derive(Debug, Clone)]
pub struct GraphRow {
    /// The column (0-based) where this commit's node sits.
    pub commit_col: usize,
    /// The graph cells to render before the commit info.
    pub cells: Vec<GraphCell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphCell {
    /// The commit node: ○
    Node,
    /// A vertical pipe passing through: │
    Pipe,
    /// Merge connector from right: ╮
    MergeRight,
    /// Closing col is RIGHT of commit — pipe from above bends left: ┛
    ConvergeFromRight,
    /// Closing col is LEFT of commit — pipe from above bends right: ┗
    ConvergeFromLeft,
    /// Horizontal connector: ━
    Horizontal,
    /// Left-going merge connector: ┛
    MergeLeft,
    /// Empty space
    Empty,
}

/// Compute graph layout for a list of commits.
///
/// `commits` is a slice of (hash, parent_hashes) in display order (newest first).
pub fn compute_graph(commits: &[(String, Vec<String>)]) -> Vec<GraphRow> {
    // `lanes[i]` = Some(hash) means lane i is an open line waiting for `hash`.
    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());

    for (hash, parents) in commits {
        // 1. Find which lane this commit lives in.
        let commit_col = if let Some(col) = lanes.iter().position(|l| l.as_deref() == Some(hash)) {
            col
        } else {
            // No lane waiting — find an empty slot or append.
            if let Some(empty) = lanes.iter().position(|l| l.is_none()) {
                empty
            } else {
                lanes.push(None);
                lanes.len() - 1
            }
        };

        // 2. Collect all lanes pointing to this commit. Close them all.
        let mut closing: Vec<usize> = Vec::new();
        for (i, lane) in lanes.iter().enumerate() {
            if lane.as_deref() == Some(hash) {
                closing.push(i);
            }
        }
        for &c in &closing {
            lanes[c] = None;
        }

        // 3. Assign parents.
        let first_parent = parents.first();
        let merge_parents = if parents.len() > 1 { &parents[1..] } else { &[] };

        // First parent continues in the commit's lane.
        if let Some(fp) = first_parent {
            lanes[commit_col] = Some(fp.clone());
        }

        // Merge parents get new lanes.
        let mut merge_cols: Vec<usize> = Vec::new();
        for mp in merge_parents {
            if let Some(existing) = lanes.iter().position(|l| l.as_deref() == Some(mp.as_str())) {
                merge_cols.push(existing);
            } else {
                let col = if let Some(empty) = lanes.iter().position(|l| l.is_none()) {
                    lanes[empty] = Some(mp.clone());
                    empty
                } else {
                    lanes.push(Some(mp.clone()));
                    lanes.len() - 1
                };
                merge_cols.push(col);
            }
        }

        // 4. Build cells for this row.
        let width = lanes.len().max(commit_col + 1);
        let mut cells = vec![GraphCell::Empty; width];

        // Draw vertical pipes for all active lanes except commit_col.
        for (i, lane) in lanes.iter().enumerate() {
            if lane.is_some() && i != commit_col {
                cells[i] = GraphCell::Pipe;
            }
        }

        // The commit node.
        cells[commit_col] = GraphCell::Node;

        // Draw merge lines from merge-parent lanes to the commit.
        for &mc in &merge_cols {
            if mc > commit_col {
                for c in (commit_col + 1)..mc {
                    if cells[c] == GraphCell::Empty {
                        cells[c] = GraphCell::Horizontal;
                    }
                }
                cells[mc] = GraphCell::MergeRight;
            } else if mc < commit_col {
                for c in (mc + 1)..commit_col {
                    if cells[c] == GraphCell::Empty {
                        cells[c] = GraphCell::Horizontal;
                    }
                }
                cells[mc] = GraphCell::MergeLeft;
            }
        }

        // Draw converging lines for closing columns (branches merging in).
        for &cc in &closing {
            if cc != commit_col {
                if cc > commit_col {
                    // Closing col is to the RIGHT — pipe bends left toward commit: ┛
                    for c in (commit_col + 1)..cc {
                        if cells[c] == GraphCell::Empty {
                            cells[c] = GraphCell::Horizontal;
                        }
                    }
                    if cells[cc] == GraphCell::Empty {
                        cells[cc] = GraphCell::ConvergeFromRight;
                    }
                } else if cc < commit_col {
                    // Closing col is to the LEFT — pipe bends right toward commit: ┗
                    for c in (cc + 1)..commit_col {
                        if cells[c] == GraphCell::Empty {
                            cells[c] = GraphCell::Horizontal;
                        }
                    }
                    if cells[cc] == GraphCell::Empty {
                        cells[cc] = GraphCell::ConvergeFromLeft;
                    }
                }
            }
        }

        // Trim trailing empties.
        while cells.last() == Some(&GraphCell::Empty) {
            cells.pop();
        }

        rows.push(GraphRow { commit_col, cells });

        // Trim trailing empty lanes.
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
    }

    rows
}

/// Render a GraphRow into Spans. Each cell is 2 chars wide (glyph + space)
/// to match lazygit's spacing.
///
/// `is_head` — when true the node uses a hollow circle (◯); otherwise a solid one (⬤).
pub fn render_graph_spans(row: &GraphRow, max_width: usize, is_head: bool, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    for (i, cell) in row.cells.iter().enumerate() {
        let (ch, style) = match cell {
            GraphCell::Node => {
                let color = col_color(row.commit_col, theme);
                let glyph = if is_head { "⬤" } else { "◯" };
                (glyph, Style::default().fg(color))
            }
            GraphCell::Pipe => {
                let color = col_color(i, theme);
                ("┃", Style::default().fg(color))
            }
            GraphCell::MergeRight => {
                let color = col_color(i, theme);
                ("╮", Style::default().fg(color))
            }
            GraphCell::ConvergeFromRight => {
                // Pipe from above bends left: ╯
                let color = col_color(i, theme);
                ("╯", Style::default().fg(color))
            }
            GraphCell::ConvergeFromLeft => {
                // Pipe from above bends right: ╰
                let color = col_color(i, theme);
                ("╰", Style::default().fg(color))
            }
            GraphCell::MergeLeft => {
                let color = col_color(i, theme);
                ("╯", Style::default().fg(color))
            }
            GraphCell::Horizontal => {
                let color = col_color(row.commit_col, theme);
                ("━", Style::default().fg(color))
            }
            GraphCell::Empty => (" ", Style::default()),
        };

        spans.push(Span::styled(ch.to_string(), style));
        // Each column is 2 chars wide: glyph + connector/space.
        if matches!(cell, GraphCell::Horizontal) {
            spans.push(Span::styled("━".to_string(), style));
        } else {
            spans.push(Span::raw(" "));
        }
    }

    // Pad to max_width so commit info aligns across rows.
    let current_cells = row.cells.len();
    if current_cells < max_width {
        let pad = (max_width - current_cells) * 2;
        spans.push(Span::raw(" ".repeat(pad)));
    }

    spans
}
