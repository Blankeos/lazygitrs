use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use super::text::plain_text;
use crate::config::Theme;
use crate::model::Model;

pub fn render_reflog_list<'a>(model: &'a Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .reflog_commits
        .iter()
        .map(|commit| {
            let spans = vec![
                Span::styled(
                    format!("{} ", commit.short_hash()),
                    Style::default().fg(theme.reflog_hash),
                ),
                Span::styled(
                    plain_text(&commit.name),
                    Style::default().fg(theme.reflog_message),
                ),
            ];
            ListItem::new(Line::from(spans))
        })
        .collect()
}
