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
