use std::collections::HashSet;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::file_tree::CommitFileTreeNode;
use crate::model::{CommitFile, FileChangeStatus, Model};

/// Render commit files as a flat list.
///
/// Filename is shown first in the strong style, followed by the directory
/// path in a dimmed style — Zed-style.
pub fn render_commit_file_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .commit_files
        .iter()
        .map(|file| {
            let (status_style, status_icon) = commit_file_status_display(file, theme);
            let dim_style = Style::default().fg(theme.text_dimmed);
            let name_style = Style::default().fg(theme.text_strong);

            if file.rename_paths().is_some() {
                return ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", status_icon), status_style),
                    Span::styled(file.name.clone(), name_style),
                ]));
            }

            let path = file.name.as_str();
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

/// Render commit file tree nodes into list items.
pub fn render_commit_file_tree<'a>(
    model: &Model,
    theme: &Theme,
    nodes: &[CommitFileTreeNode],
    collapsed_dirs: &HashSet<String>,
) -> Vec<ListItem<'a>> {
    nodes
        .iter()
        .map(|node| {
            let indent = "  ".repeat(node.depth);

            if node.is_dir {
                let is_collapsed = collapsed_dirs.contains(&node.path);
                let icon = if is_collapsed { "▶ " } else { "▼ " };
                let is_root = node.path == ".";
                let dir_style = Style::default().fg(theme.text_dimmed);

                let line = if is_root {
                    Line::from(Span::styled(format!("{} /", icon.trim_end()), dir_style))
                } else {
                    Line::from(vec![
                        Span::styled(format!("{}{}", indent, icon), dir_style),
                        Span::styled(node.name.clone(), dir_style),
                    ])
                };
                ListItem::new(line)
            } else if let Some(file_idx) = node.file_index {
                let Some(file) = model.commit_files.get(file_idx) else {
                    return ListItem::new(Line::raw(""));
                };
                let (status_style, status_icon) = commit_file_status_display(file, theme);

                let line = Line::from(vec![
                    Span::styled(format!("{} ", status_icon), status_style),
                    Span::raw(indent),
                    Span::styled(node.name.clone(), Style::default().fg(theme.text_strong)),
                ]);
                ListItem::new(line)
            } else {
                ListItem::new(Line::raw(""))
            }
        })
        .collect()
}

fn commit_file_status_display<'a>(file: &CommitFile, theme: &Theme) -> (Style, &'a str) {
    match file.status {
        FileChangeStatus::Added => (theme.file_staged, "A "),
        FileChangeStatus::Deleted => (Style::default().fg(theme.change_deleted), "D "),
        FileChangeStatus::Modified => (theme.file_unstaged, "M "),
        FileChangeStatus::Renamed => (Style::default().fg(theme.change_renamed), "R "),
        FileChangeStatus::Copied => (Style::default().fg(theme.change_copied), "C "),
        FileChangeStatus::Unmerged => (Style::default().fg(theme.change_unmerged), "U "),
    }
}
