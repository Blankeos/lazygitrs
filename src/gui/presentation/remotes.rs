use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;

pub fn render_remote_list<'a>(model: &Model, _theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .remotes
        .iter()
        .map(|remote| {
            let url = remote.urls.first().cloned().unwrap_or_default();
            let branch_count = remote.branches.len();

            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", remote.name),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!("({} branches) ", branch_count),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(url, Style::default().fg(Color::White)),
            ]);

            ListItem::new(line)
        })
        .collect()
}
