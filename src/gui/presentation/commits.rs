use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;
use crate::model::commit::{Commit, CommitStatus};

use super::graph;

pub fn render_sub_commit_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
    render_commits(&model.sub_commits, &model.head_hash, theme, &[])
}

pub fn render_commit_list<'a>(
    model: &Model,
    theme: &Theme,
    cherry_picked: &[String],
) -> Vec<ListItem<'a>> {
    render_commits(&model.commits, &model.head_hash, theme, cherry_picked)
}

fn render_commits<'a>(
    commits: &[Commit],
    head_hash: &str,
    theme: &Theme,
    cherry_picked: &[String],
) -> Vec<ListItem<'a>> {
    // Build graph data from commits.
    let graph_input: Vec<(String, Vec<String>)> = commits
        .iter()
        .map(|c| (c.hash.clone(), c.parents.clone()))
        .collect();

    let graph_rows = graph::compute_graph(&graph_input);

    // Compute max graph width for alignment.
    let max_graph_width = graph_rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);

    commits
        .iter()
        .enumerate()
        .map(|(i, commit)| {
            let graph_row = graph_rows.get(i);
            let is_head = commit.hash == *head_hash;

            // Start with graph spans.
            let mut spans: Vec<Span<'a>> = if let Some(row) = graph_row {
                graph::render_graph_spans(row, max_graph_width, is_head, theme)
            } else {
                vec![Span::raw(" ".repeat(max_graph_width * 2))]
            };

            // Hash — color by push status, overridden to cyan+bold if cherry-picked
            let is_cherry_picked = cherry_picked.iter().any(|h| *h == commit.hash);
            let hash_style = if is_cherry_picked {
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
            } else {
                match commit.status {
                    CommitStatus::Unpushed => theme.commit_hash,
                    CommitStatus::Pushed => Style::default().fg(theme.commit_hash_pushed),
                    CommitStatus::Merged => Style::default().fg(theme.commit_hash_merged),
                    _ => theme.commit_hash,
                }
            };
            spans.push(Span::styled(
                format!("{} ", commit.short_hash()),
                hash_style,
            ));

            // Ref decorations (HEAD -> main, origin/main, etc.)
            for r in &commit.refs {
                let (label, color) = if r.starts_with("HEAD -> ") {
                    (r.clone(), theme.ref_head)
                } else if r == "HEAD" {
                    (r.clone(), theme.ref_head)
                } else if r.contains('/') {
                    // Remote ref like origin/main
                    (r.clone(), theme.ref_remote)
                } else {
                    // Local branch
                    (r.clone(), theme.ref_local)
                };
                spans.push(Span::styled(
                    format!("({}) ", label),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ));
            }

            // Tags (before message so they're visible in compact views)
            for tag in &commit.tags {
                spans.push(Span::styled(
                    format!("[{}] ", tag),
                    Style::default().fg(theme.ref_tag).add_modifier(Modifier::BOLD),
                ));
            }

            // Commit message
            spans.push(Span::styled(
                commit.name.clone(),
                Style::default().fg(theme.text_strong),
            ));

            // Author (compact)
            spans.push(Span::styled(
                format!(" {}", commit.author_name),
                theme.commit_author,
            ));

            ListItem::new(Line::from(spans))
        })
        .collect()
}
