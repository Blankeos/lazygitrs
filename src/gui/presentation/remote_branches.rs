use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::RemoteBranch;

pub fn render_remote_branch_list<'a>(
    branches: &[RemoteBranch],
    theme: &Theme,
) -> Vec<ListItem<'a>> {
    branches
        .iter()
        .map(|branch| {
            let line = Line::from(vec![
                Span::styled(
                    format!("  {} ", branch.name),
                    Style::default().fg(theme.remote_branch_name),
                ),
                Span::styled(
                    branch.hash.clone(),
                    Style::default().fg(theme.remote_branch_detail),
                ),
            ]);
            ListItem::new(line)
        })
        .collect()
}
