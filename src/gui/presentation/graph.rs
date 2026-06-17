/// Commit graph layout engine matching lazygit's visual style.
///
/// Each cell is modeled by four connection booleans (up/down/left/right) plus a
/// type (Connection/Commit/Merge), so any combination of incoming and outgoing
/// lines renders correctly. This lets a continuing pipe coexist with a merge
/// connector in the same cell (`up+down+right` → `│─`) without breaking the
/// vertical line.
use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::config::Theme;

pub fn col_color(col: usize, theme: &Theme) -> Color {
    theme.graph_colors[col % theme.graph_colors.len()]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellType {
    #[default]
    Connection,
    Commit,
    Merge,
}

#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub cell_type: CellType,
    /// Color index for the first glyph.
    pub style_col: usize,
    /// Color index for the right-extension glyph; falls back to style_col.
    pub right_style_col: Option<usize>,
}

impl Cell {
    fn is_empty(&self) -> bool {
        !self.up
            && !self.down
            && !self.left
            && !self.right
            && self.cell_type == CellType::Connection
    }
}

#[derive(Debug, Clone)]
pub struct GraphRow {
    pub commit_col: usize,
    pub cells: Vec<Cell>,
}

/// Per-lane state: which commit the lane is waiting on, plus any merge
/// connectors that should be drawn when the lane finally terminates.
#[derive(Clone, Default)]
struct LaneInfo {
    target: Option<String>,
    /// Columns of past merge commits whose horizontal connector to this lane
    /// was deferred until the lane closes (matches lazygit's behavior of
    /// drawing the merge stroke at the parent's row, not the merge commit's
    /// row).
    deferred_merges: Vec<usize>,
}

/// Compute graph layout. `commits` is a slice of (hash, parent_hashes) in
/// display order (newest first).
pub fn compute_graph(commits: &[(String, Vec<String>)]) -> Vec<GraphRow> {
    let mut lanes: Vec<LaneInfo> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());

    for (hash, parents) in commits {
        let lanes_before: Vec<Option<String>> = lanes.iter().map(|l| l.target.clone()).collect();

        // Pick the column this commit lives in: an existing lane waiting for it,
        // else the first empty slot, else a fresh lane on the right.
        let commit_col =
            if let Some(col) = lanes.iter().position(|l| l.target.as_deref() == Some(hash)) {
                col
            } else if let Some(empty) = lanes.iter().position(|l| l.target.is_none()) {
                empty
            } else {
                lanes.push(LaneInfo::default());
                lanes.len() - 1
            };

        // All lanes that were tracking this commit terminate here. Collect any
        // deferred-merge cols they were holding so we can draw those connectors
        // at this row.
        let closing: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                if l.target.as_deref() == Some(hash) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();
        let mut deferred_cols: Vec<usize> = Vec::new();
        for &c in &closing {
            deferred_cols.extend(lanes[c].deferred_merges.drain(..));
            lanes[c].target = None;
        }

        // First parent continues in the commit's lane.
        let first_parent = parents.first();
        if let Some(fp) = first_parent {
            lanes[commit_col].target = Some(fp.clone());
            lanes[commit_col].deferred_merges.clear();
        }

        // Additional parents: if a lane is already tracking the parent, defer
        // the horizontal connector to that lane's eventual termination row.
        // Otherwise allocate a new lane and draw the connector at THIS row.
        let merge_parents: &[String] = if parents.len() > 1 {
            &parents[1..]
        } else {
            &[]
        };
        let mut new_merge_cols: Vec<usize> = Vec::new();
        for mp in merge_parents {
            if let Some(existing) = lanes
                .iter()
                .position(|l| l.target.as_deref() == Some(mp.as_str()))
            {
                if existing != commit_col {
                    lanes[existing].deferred_merges.push(commit_col);
                }
            } else if let Some(empty) = lanes.iter().position(|l| l.target.is_none()) {
                lanes[empty] = LaneInfo {
                    target: Some(mp.clone()),
                    deferred_merges: Vec::new(),
                };
                new_merge_cols.push(empty);
            } else {
                lanes.push(LaneInfo {
                    target: Some(mp.clone()),
                    deferred_merges: Vec::new(),
                });
                new_merge_cols.push(lanes.len() - 1);
            }
        }

        // Build cells from a side-by-side comparison of lanes_before vs lanes.
        let width = lanes.len().max(lanes_before.len()).max(commit_col + 1);
        let mut cells: Vec<Cell> = vec![Cell::default(); width];

        let is_merge = !merge_parents.is_empty();

        // Commit cell.
        cells[commit_col].cell_type = if is_merge {
            CellType::Merge
        } else {
            CellType::Commit
        };
        cells[commit_col].up = closing.contains(&commit_col);
        cells[commit_col].down = first_parent.is_some();
        cells[commit_col].style_col = commit_col;

        // Pipes for every other column, derived from the before/after lane state.
        for i in 0..width {
            if i == commit_col {
                continue;
            }
            let was = lanes_before.get(i).and_then(|o| o.as_ref()).is_some();
            let now = lanes.get(i).and_then(|l| l.target.as_ref()).is_some();
            let is_new_merge_lane = !was && now && new_merge_cols.contains(&i);

            if was && now {
                cells[i].up = true;
                cells[i].down = true;
                cells[i].style_col = i;
            } else if was {
                // Lane closes here (skipping commit_col itself).
                cells[i].up = true;
                cells[i].style_col = i;
            } else if is_new_merge_lane {
                cells[i].down = true;
                cells[i].style_col = i;
            }
        }

        // Horizontal connector from each closing column (≠ commit_col) into the
        // commit. Colored by the closing lane.
        for &c in &closing {
            if c == commit_col {
                continue;
            }
            draw_horizontal(&mut cells, c, commit_col, c);
        }

        // Deferred connectors: past merge commits whose connector to this lane
        // was held until now. Color by the merge commit's column.
        for &dc in &deferred_cols {
            draw_horizontal(&mut cells, commit_col, dc, dc);
        }

        // New merge-parent lanes are born here, so their connector is drawn at
        // this row. Color by the commit (the new pipe's source).
        for &mc in &new_merge_cols {
            if mc == commit_col {
                continue;
            }
            draw_horizontal(&mut cells, commit_col, mc, commit_col);
        }

        while cells.last().map_or(false, Cell::is_empty) {
            cells.pop();
        }

        rows.push(GraphRow { commit_col, cells });

        while lanes.last().map_or(false, |l| {
            l.target.is_none() && l.deferred_merges.is_empty()
        }) {
            lanes.pop();
        }
    }

    rows
}

/// Mark a horizontal run between two columns and record the connector color on
/// every cell whose right-extension is part of the run.
fn draw_horizontal(cells: &mut [Cell], from: usize, to: usize, color_col: usize) {
    if from == to {
        return;
    }
    let (lo, hi) = if from < to { (from, to) } else { (to, from) };

    cells[lo].right = true;
    cells[hi].left = true;
    for j in (lo + 1)..hi {
        cells[j].left = true;
        cells[j].right = true;
    }

    // The right-extension of cells lo..hi belongs to this connector.
    for j in lo..hi {
        cells[j].right_style_col = Some(color_col);
    }
    // Cells that are purely horizontal (no vertical run) take their first-glyph
    // color from the connector too.
    for j in lo..=hi {
        if !cells[j].up && !cells[j].down && cells[j].cell_type == CellType::Connection {
            cells[j].style_col = color_col;
        }
    }
}

/// Pick the two glyphs (first char + right-extension) for a cell from its
/// up/down/left/right flags. Mirrors lazygit's getBoxDrawingChars table.
fn box_drawing(up: bool, down: bool, left: bool, right: bool) -> (&'static str, &'static str) {
    match (up, down, left, right) {
        (true, true, true, true) => ("│", "─"),
        (true, true, true, false) => ("│", " "),
        (true, true, false, true) => ("│", "─"),
        (true, true, false, false) => ("│", " "),
        (true, false, true, true) => ("┴", "─"),
        (true, false, true, false) => ("╯", " "),
        (true, false, false, true) => ("╰", "─"),
        (true, false, false, false) => ("╵", " "),
        (false, true, true, true) => ("┬", "─"),
        (false, true, true, false) => ("╮", " "),
        (false, true, false, true) => ("╭", "─"),
        (false, true, false, false) => ("╷", " "),
        (false, false, true, true) => ("─", "─"),
        (false, false, true, false) => ("─", " "),
        (false, false, false, true) => ("╶", "─"),
        (false, false, false, false) => (" ", " "),
    }
}

/// Render a GraphRow into spans. Each cell occupies two terminal columns
/// (first glyph + right extension).
///
/// `is_head` swaps the commit glyph to a filled circle for HEAD.
pub fn render_graph_spans(
    row: &GraphRow,
    max_width: usize,
    is_head: bool,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(row.cells.len() * 2 + 1);

    for cell in &row.cells {
        let (first, second) = box_drawing(cell.up, cell.down, cell.left, cell.right);

        let first_glyph: &'static str = match cell.cell_type {
            CellType::Commit | CellType::Merge => {
                if is_head && cell.style_col == row.commit_col {
                    "⬤"
                } else {
                    match cell.cell_type {
                        CellType::Commit => "◯",
                        CellType::Merge => "⏣",
                        _ => unreachable!(),
                    }
                }
            }
            CellType::Connection => first,
        };

        let first_style = Style::default().fg(col_color(cell.style_col, theme));
        let right_color = cell.right_style_col.unwrap_or(cell.style_col);
        let second_style = if second == " " {
            Style::default()
        } else {
            Style::default().fg(col_color(right_color, theme))
        };

        spans.push(Span::styled(first_glyph.to_string(), first_style));
        spans.push(Span::styled(second.to_string(), second_style));
    }

    // Pad to max_width so commit info aligns across rows.
    if row.cells.len() < max_width {
        let pad = (max_width - row.cells.len()) * 2;
        spans.push(Span::raw(" ".repeat(pad)));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_plain(row: &GraphRow) -> String {
        let mut out = String::new();
        for cell in &row.cells {
            let (first, second) = box_drawing(cell.up, cell.down, cell.left, cell.right);
            let g = match cell.cell_type {
                CellType::Commit => "◯",
                CellType::Merge => "⏣",
                CellType::Connection => first,
            };
            out.push_str(g);
            out.push_str(second);
        }
        out.trim_end().to_string()
    }

    #[test]
    fn user_scenario_continuous_main_lane() {
        // From the user's repo screenshot:
        //   6975eec (main)        -> [1b91554]
        //   0d129e4 (feature, M)  -> [9902457, 1b91554]
        //   1b91554 (main)        -> [f6ecf6f]
        //   9902457 (feature, M)  -> [22d0113, f6ecf6f]
        //   f6ecf6f (main)        -> [93897e5]
        //   22d0113 (feature)     -> [214f465]
        let commits = vec![
            ("6975eec".into(), vec!["1b91554".into()]),
            ("0d129e4".into(), vec!["9902457".into(), "1b91554".into()]),
            ("1b91554".into(), vec!["f6ecf6f".into()]),
            ("9902457".into(), vec!["22d0113".into(), "f6ecf6f".into()]),
            ("f6ecf6f".into(), vec!["93897e5".into()]),
            ("22d0113".into(), vec!["214f465".into()]),
        ];
        let rows = compute_graph(&commits);
        let rendered: Vec<String> = rows.iter().map(render_plain).collect();
        assert_eq!(
            rendered,
            vec![
                "◯",   // 6975eec: main lane only
                "│ ⏣", // 0d129e4: main pipe continues, merge symbol on feature lane
                "◯─│", // 1b91554: merge stroke drawn HERE (parent's row), into col 1
                "│ ⏣", // 9902457: main pipe continues, merge symbol on feature lane
                "◯─│", // f6ecf6f: merge stroke drawn HERE, into col 1
                "│ ◯", // 22d0113: feature tip
            ]
        );
    }
}
