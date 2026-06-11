use std::collections::HashSet;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;
use crate::model::file_tree::FileTreeNode;

/// Render file list as a flat list (no tree structure).
///
/// Filename is shown first in the strong style, followed by the directory
/// path in a dimmed style — Zed-style.
pub fn render_file_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .files
        .iter()
        .map(|file| {
            let (status_style, status_icon) = file_status_display(file, theme);
            let name_style = file_name_style(file, theme);
            let dim_style = Style::default().fg(theme.text_dimmed);

            if file.rename_paths().is_some() {
                return ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", status_icon), status_style),
                    Span::styled(file.display_name.clone(), name_style),
                ]));
            }

            let path = file.display_name.as_str();
            let (dir, name) = match path.rfind('/') {
                Some(idx) => (&path[..=idx], &path[idx + 1..]),
                None => ("", path),
            };

            let mut spans = vec![
                Span::styled(format!(" {} ", status_icon), status_style),
                Span::styled(name.to_string(), name_style),
            ];
            if !dir.is_empty() {
                spans.push(Span::styled(format!(" {}", dir), dim_style));
            }

            ListItem::new(Line::from(spans))
        })
        .collect()
}

/// Render the cached file tree nodes into list items.
pub fn render_file_tree<'a>(
    model: &Model,
    theme: &Theme,
    nodes: &[FileTreeNode],
    collapsed_dirs: &HashSet<String>,
) -> Vec<ListItem<'a>> {
    nodes
        .iter()
        .map(|node| {
            let indent = "  ".repeat(node.depth);

            if node.is_dir {
                let is_collapsed = collapsed_dirs.contains(&node.path);
                let icon = if is_collapsed { "▶ " } else { "▼ " };

                // Directory is green if ALL child files are fully staged
                let all_staged = !node.child_file_indices.is_empty()
                    && node.child_file_indices.iter().all(|&idx| {
                        model
                            .files
                            .get(idx)
                            .is_some_and(|f| f.has_staged_changes && !f.has_unstaged_changes)
                    });

                let dir_style = if all_staged {
                    theme.file_staged
                } else {
                    Style::default().fg(theme.text_dimmed)
                };

                let is_root = node.path == ".";
                let line = if is_root {
                    Line::from(Span::styled(format!(" {} /", icon.trim_end()), dir_style))
                } else {
                    Line::from(vec![
                        Span::styled(format!(" {}{}", indent, icon), dir_style),
                        Span::styled(node.name.clone(), dir_style),
                    ])
                };
                ListItem::new(line)
            } else if let Some(file_idx) = node.file_index {
                let file = &model.files[file_idx];
                let (status_style, status_icon) = file_status_display(file, theme);
                let name_style = file_name_style(file, theme);

                let line = Line::from(vec![
                    Span::styled(format!("{} ", status_icon), status_style),
                    Span::raw(indent),
                    Span::styled(node.name.clone(), name_style),
                ]);
                ListItem::new(line)
            } else {
                ListItem::new(Line::raw(""))
            }
        })
        .collect()
}

/// File name color: green when fully staged, white otherwise.
fn file_name_style(file: &crate::model::File, theme: &Theme) -> Style {
    if file.has_staged_changes && !file.has_unstaged_changes {
        theme.file_staged
    } else {
        Style::default().fg(theme.text_strong)
    }
}

fn file_status_display<'a>(file: &crate::model::File, theme: &Theme) -> (Style, &'a str) {
    let status_style = if file.has_merge_conflicts {
        theme.file_conflicted
    } else if file.has_staged_changes && !file.has_unstaged_changes {
        theme.file_staged
    } else if !file.tracked {
        theme.file_untracked
    } else {
        theme.file_unstaged
    };

    let status_icon: &str = match file.short_status.as_str() {
        "??" => "??",
        "M " => "M ",
        " M" => " M",
        "MM" => "MM",
        "A " => "A ",
        "AM" => "AM",
        "D " => "D ",
        " D" => " D",
        "R " => "R ",
        "RM" => "RM",
        "C " => "C ",
        "CM" => "CM",
        "UU" => "UU",
        "AA" => "AA",
        "DD" => "DD",
        "AU" => "AU",
        "UA" => "UA",
        "DU" => "DU",
        "UD" => "UD",
        _ => "  ",
    };

    (status_style, status_icon)
}
