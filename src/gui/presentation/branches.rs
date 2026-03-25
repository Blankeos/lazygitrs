use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::config::Theme;
use crate::model::Model;

const SPINNER_CHARS: &[char] = &['·', '✻', '✽', '✶', '✳', '✢'];

pub fn render_branch_list<'a>(
    model: &Model,
    theme: &Theme,
    remote_op_label: Option<&str>,
    spinner_frame: usize,
    remote_op_success: bool,
) -> Vec<ListItem<'a>> {
    model
        .branches
        .iter()
        .map(|branch| {
            let name_style = if branch.head {
                theme.branch_head
            } else {
                theme.branch_local
            };

            let head_marker = if branch.head { "* " } else { "  " };

            let mut spans = vec![
                Span::styled(head_marker.to_string(), name_style),
                Span::styled(branch.name.clone(), name_style),
            ];

            // Recency
            if !branch.recency.is_empty() {
                spans.insert(
                    0,
                    Span::styled(
                        format!("{:>3} ", branch.recency),
                        Style::default().fg(Color::DarkGray),
                    ),
                );
            }

            // Remote operation indicator on head branch (e.g. "Pushing ✻" or "✓" on success)
            if branch.head {
                if let Some(label) = remote_op_label {
                    let spinner = SPINNER_CHARS[(spinner_frame / 8) % SPINNER_CHARS.len()];
                    spans.push(Span::styled(
                        format!(" {} {}", label, spinner),
                        Style::default().fg(Color::Yellow),
                    ));
                } else if remote_op_success {
                    spans.push(Span::styled(
                        " ✓".to_string(),
                        Style::default().fg(Color::Green),
                    ));
                }
            }

            // Ahead/behind indicator (skip when remote op is active on head branch)
            if !(branch.head && remote_op_label.is_some()) {
                if let Some((ahead, behind)) = branch.ahead_behind() {
                    let indicator = match (ahead > 0, behind > 0) {
                        (true, true) => format!(" ↑{}↓{}", ahead, behind),
                        (true, false) => format!(" ↑{}", ahead),
                        (false, true) => format!(" ↓{}", behind),
                        _ => String::new(),
                    };
                    if !indicator.is_empty() {
                        spans.push(Span::styled(
                            indicator,
                            Style::default().fg(Color::Yellow),
                        ));
                    }
                }
            }

            ListItem::new(Line::from(spans))
        })
        .collect()
}
