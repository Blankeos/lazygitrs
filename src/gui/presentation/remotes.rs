use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;

pub fn render_remote_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .remotes
        .iter()
        .map(|remote| {
            let url = remote.urls.first().cloned().unwrap_or_default();
            let branch_count = remote.branches.len();

            let line = Line::from(vec![
                Span::styled(
                    format!(" {} ", remote.name),
                    Style::default().fg(theme.remote_name),
                ),
                Span::styled(
                    format!("({} branches) ", branch_count),
                    Style::default().fg(theme.text_dimmed),
                ),
                Span::styled(url, Style::default().fg(theme.remote_url)),
            ]);

            ListItem::new(line)
        })
        .collect()
}
