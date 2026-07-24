use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;

pub fn render_tag_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .tags
        .iter()
        .map(|tag| {
            let mut spans = vec![
                Span::styled(
                    format!(" {} ", tag.name),
                    Style::default().fg(theme.tag_name),
                ),
                Span::styled(
                    format!("{} ", &tag.hash),
                    Style::default().fg(theme.tag_hash),
                ),
            ];

            // Co-located local/remote branches at this tag's commit, e.g. (main,origin/main).
            let labels = colocated_branch_labels(model, &tag.hash);
            if !labels.is_empty() {
                spans.push(Span::styled(
                    format!("({}) ", labels.join(",")),
                    Style::default().fg(theme.text_dimmed),
                ));
            }

            if !tag.message.is_empty() {
                spans.push(Span::styled(
                    tag.message.clone(),
                    Style::default().fg(theme.tag_message),
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect()
}

/// Local branch names and remote branch full names that point at `hash`.
fn colocated_branch_labels(model: &Model, hash: &str) -> Vec<String> {
    let mut labels = Vec::new();

    for branch in &model.branches {
        if hashes_match(&branch.hash, hash) {
            labels.push(branch.name.clone());
        }
    }

    for remote in &model.remotes {
        for rb in &remote.branches {
            if hashes_match(&rb.hash, hash) {
                labels.push(format!("{}/{}", rb.remote_name, rb.name));
            }
        }
    }

    labels
}

fn hashes_match(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a == b || a.starts_with(b) || b.starts_with(a))
}
