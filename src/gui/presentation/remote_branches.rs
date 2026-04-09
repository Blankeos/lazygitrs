use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::RemoteBranch;

pub fn render_remote_branch_list<'a>(
    branches: &[RemoteBranch],
    head_branch_name: &str,
    theme: &Theme,
) -> Vec<ListItem<'a>> {
    branches
        .iter()
        .map(|branch| {
            let is_head = !head_branch_name.is_empty() && branch.name == head_branch_name;
            let marker = if is_head { "* " } else { "  " };
            let name_style = if is_head {
                theme.branch_head
            } else {
                Style::default().fg(theme.remote_branch_name)
            };
            let line = Line::from(vec![
                Span::styled(format!("{}{} ", marker, branch.name), name_style),
                Span::styled(
                    branch.hash.clone(),
                    Style::default().fg(theme.remote_branch_detail),
                ),
            ]);
            ListItem::new(line)
        })
        .collect()
}
