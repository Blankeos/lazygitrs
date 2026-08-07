use std::collections::HashSet;

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::gui::presentation::files::append_file_stats;
use crate::model::file_tree::CommitFileTreeNode;
use crate::model::{CommitFile, FileChangeStatus, Model};

/// Render commit files as a flat list.
///
/// Filename is shown first in the strong style, followed by the directory
/// path in a dimmed style — Zed-style.
pub fn render_commit_file_list<'a>(
    model: &Model,
    theme: &Theme,
    width: usize,
) -> Vec<ListItem<'a>> {
    model
        .commit_files
        .iter()
        .map(|file| {
            let (status_style, status_icon) = commit_file_status_display(file, theme);
            let dim_style = Style::default().fg(theme.text_dimmed);
            let name_style = Style::default().fg(theme.text_strong);

            if file.rename_paths().is_some() {
                let spans = vec![
                    Span::styled(format!(" {} ", status_icon), status_style),
                    Span::styled(file.name.clone(), name_style),
                ];
                return ListItem::new(Line::from(append_file_stats(
                    spans,
                    file.hunk_count,
                    file.additions,
                    file.deletions,
                    theme,
                    width,
                )));
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

            ListItem::new(Line::from(append_file_stats(
                spans,
                file.hunk_count,
                file.additions,
                file.deletions,
                theme,
                width,
            )))
        })
        .collect()
}

/// Render commit file tree nodes into list items.
pub fn render_commit_file_tree<'a>(
    model: &Model,
    theme: &Theme,
    nodes: &[CommitFileTreeNode],
    collapsed_dirs: &HashSet<String>,
    width: usize,
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

                let spans = vec![
                    Span::raw(indent),
                    Span::styled(format!("{} ", status_icon), status_style),
                    Span::styled(node.name.clone(), Style::default().fg(theme.text_strong)),
                ];
                let line = Line::from(append_file_stats(
                    spans,
                    file.hunk_count,
                    file.additions,
                    file.deletions,
                    theme,
                    width,
                ));
                ListItem::new(line)
            } else {
                ListItem::new(Line::raw(""))
            }
        })
        .collect()
}

pub(crate) fn commit_file_status_display<'a>(file: &CommitFile, theme: &Theme) -> (Style, &'a str) {
    match file.status {
        FileChangeStatus::Added => (theme.file_staged, "A "),
        FileChangeStatus::Deleted => (Style::default().fg(theme.change_deleted), "D "),
        FileChangeStatus::Modified => (theme.file_unstaged, "M "),
        FileChangeStatus::Renamed => (Style::default().fg(theme.change_renamed), "R "),
        FileChangeStatus::Copied => (Style::default().fg(theme.change_copied), "C "),
        FileChangeStatus::Unmerged => (Style::default().fg(theme.change_unmerged), "U "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::widgets::{Block, Borders, List};

    #[test]
    fn commit_file_stats_are_right_aligned_and_survive_truncation() {
        let model = Model {
            commit_files: vec![CommitFile {
                name: "src/a-very-long-commit-file-name.rs".into(),
                status: FileChangeStatus::Modified,
                hunk_count: 2,
                additions: 143,
                deletions: 71,
            }],
            ..Model::default()
        };
        let theme = Theme::default();
        let mut terminal = Terminal::new(TestBackend::new(26, 3)).expect("test terminal");
        terminal
            .draw(|frame| {
                let items = render_commit_file_list(&model, &theme, 24);
                frame.render_widget(
                    List::new(items).block(Block::default().borders(Borders::ALL)),
                    Rect::new(0, 0, 26, 3),
                );
            })
            .expect("commit file list should render");
        let rendered: String = (1..25)
            .map(|x| {
                terminal
                    .backend()
                    .buffer()
                    .cell((x, 1))
                    .and_then(|cell| cell.symbol().chars().next())
                    .unwrap_or(' ')
            })
            .collect();

        assert_eq!(rendered.chars().count(), 24);
        assert!(rendered.ends_with("*2 +143 -71"), "{rendered:?}");
    }
}
