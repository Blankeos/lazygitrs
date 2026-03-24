use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;

pub fn render_stash_list<'a>(model: &Model, _theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .stash_entries
        .iter()
        .map(|entry| {
            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", entry.ref_name()),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    entry.name.clone(),
                    Style::default().fg(Color::White),
                ),
            ]);

            ListItem::new(line)
        })
        .collect()
}
