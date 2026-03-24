use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;

pub fn render_file_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .files
        .iter()
        .map(|file| {
            let status_style = if file.has_merge_conflicts {
                theme.file_conflicted
            } else if file.has_staged_changes && !file.has_unstaged_changes {
                theme.file_staged
            } else if !file.tracked {
                theme.file_untracked
            } else {
                theme.file_unstaged
            };

            let status_icon = if file.has_staged_changes && file.has_unstaged_changes {
                "MM"
            } else if file.has_staged_changes {
                "A "
            } else {
                &file.short_status
            };

            let line = Line::from(vec![
                Span::styled(format!(" {} ", status_icon), status_style),
                Span::styled(
                    file.display_name.clone(),
                    Style::default().fg(Color::White),
                ),
            ]);

            ListItem::new(line)
        })
        .collect()
}
