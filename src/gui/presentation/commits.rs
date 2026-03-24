use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::{CommitStatus, Model};

pub fn render_commit_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .commits
        .iter()
        .map(|commit| {
            let hash_style = theme.commit_hash;

            let status_color = match commit.status {
                CommitStatus::Unpushed => Color::Green,
                CommitStatus::Pushed => Color::Yellow,
                CommitStatus::Merged => Color::Cyan,
                CommitStatus::Rebasing => Color::Magenta,
                CommitStatus::Conflicted => Color::Red,
                _ => Color::Yellow,
            };

            let mut spans = vec![
                Span::styled(
                    format!(" {} ", commit.short_hash()),
                    Style::default().fg(status_color),
                ),
                Span::styled(
                    commit.name.clone(),
                    Style::default().fg(Color::White),
                ),
            ];

            // Tags
            for tag in &commit.tags {
                spans.push(Span::styled(
                    format!(" [{}]", tag),
                    Style::default().fg(Color::Cyan),
                ));
            }

            // Author (compact)
            spans.push(Span::styled(
                format!(" {}", commit.author_name),
                theme.commit_author,
            ));

            ListItem::new(Line::from(spans))
        })
        .collect()
}
