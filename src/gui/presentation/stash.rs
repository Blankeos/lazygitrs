use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use super::text::plain_text;
use crate::config::Theme;
use crate::model::Model;

pub fn render_stash_list<'a>(model: &'a Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .stash_entries
        .iter()
        .map(|entry| {
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", entry.ref_name()),
                    Style::default().fg(theme.stash_index),
                ),
                Span::styled(
                    plain_text(&entry.name),
                    Style::default().fg(theme.stash_message),
                ),
            ]);

            ListItem::new(line)
        })
        .collect()
}
