use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;

pub fn render_stash_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
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
                    entry.name.clone(),
                    Style::default().fg(theme.stash_message),
                ),
            ]);

            ListItem::new(line)
        })
        .collect()
}
