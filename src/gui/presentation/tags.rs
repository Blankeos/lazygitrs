use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::{Model, Tag};

pub fn render_tag_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .tags
        .iter()
        .map(|tag| {
            let mut spans = vec![Span::styled(
                format!(" {} ", tag.name),
                Style::default().fg(theme.tag_name),
            )];

            if !tag.message.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", tag.message),
                    Style::default().fg(theme.tag_message),
                ));
            }

            spans.push(Span::styled(
                presence_label(tag).to_string(),
                Style::default().fg(theme.text_dimmed),
            ));

            ListItem::new(Line::from(spans))
        })
        .collect()
}

fn presence_label(tag: &Tag) -> &'static str {
    if tag.on_remote {
        "(local,remote)"
    } else {
        "(local)"
    }
}
