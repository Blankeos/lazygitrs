use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::config::Theme;
use crate::git::rebase::RebaseAction;
use crate::gui::modes::rebase_mode::{EntryStatus, RebaseModeState, RebasePhase};

/// Get the semantic color for a rebase action.
fn action_color(action: RebaseAction) -> Color {
    match action {
        RebaseAction::Pick => Color::Green,
        RebaseAction::Reword => Color::LightBlue,
        RebaseAction::Edit => Color::Yellow,
        RebaseAction::Squash => Color::Rgb(255, 165, 0), // orange
        RebaseAction::Fixup => Color::Rgb(180, 130, 255), // violet
        RebaseAction::Drop => Color::Red,
    }
}

/// Width of the action label box (e.g. " pick    ") including padding.
const ACTION_LABEL_WIDTH: usize = 9; // " {:7} " = 1 + 7 + 1

pub fn render(frame: &mut Frame, state: &RebaseModeState, theme: &Theme) {
    let area = frame.area();

    // Layout: Header (3) | Progress banner if InProgress (1-2) | List (fill) | Status bar (1)
    let has_banner = state.phase == RebasePhase::InProgress;
    let banner_height = if has_banner { 2 } else { 0 };

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                          // header
            Constraint::Length(banner_height),              // progress banner
            Constraint::Min(1),                             // list
            Constraint::Length(1),                           // status bar
        ])
        .split(area);

    render_header(frame, outer[0], state, theme);
    if has_banner {
        render_progress_banner(frame, outer[1], state);
    }
    render_list(frame, outer[2], state, theme);
    render_status_bar(frame, outer[3], state);
}

fn render_header(frame: &mut Frame, area: Rect, state: &RebaseModeState, theme: &Theme) {
    let mut title_spans = vec![
        Span::styled(
            "Interactive Rebase",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("⎇ ", Style::default().fg(Color::Cyan)),
        Span::styled(
            &state.branch_name,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];

    match state.phase {
        RebasePhase::Planning => {
            title_spans.push(Span::styled(
                format!("Rebasing {} commits onto ", state.entries.len()),
                Style::default().fg(Color::DarkGray),
            ));
            title_spans.push(Span::styled(
                format!("◆ {}", state.base_short_hash),
                Style::default().fg(Color::Yellow),
            ));
        }
        RebasePhase::InProgress => {
            title_spans.push(Span::styled(
                "onto ",
                Style::default().fg(Color::DarkGray),
            ));
            title_spans.push(Span::styled(
                format!("◆ {}", state.base_short_hash),
                Style::default().fg(Color::Yellow),
            ));
            title_spans.push(Span::raw("  "));
            title_spans.push(Span::styled(
                format!("{}/{} commits", state.done_count, state.total_count),
                Style::default().fg(Color::White),
            ));
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.active_border);
    let paragraph = Paragraph::new(Line::from(title_spans)).block(block);
    frame.render_widget(paragraph, area);
}

/// Render the "Rebase paused at ..." progress banner (InProgress only).
fn render_progress_banner(frame: &mut Frame, area: Rect, state: &RebaseModeState) {
    // Find the current (paused) entry
    let current = state.entries.iter().find(|e| e.status == EntryStatus::Current);
    let remaining = state.remaining_count();

    let mut spans = vec![
        Span::styled(
            " ⏸ ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Rebase paused", Style::default().fg(Color::Yellow)),
    ];

    if let Some(entry) = current {
        let action_desc = match entry.action {
            RebaseAction::Edit => "for editing",
            RebaseAction::Reword => "for rewording",
            _ => "due to conflict",
        };
        spans.push(Span::styled(
            format!(" {} at ", action_desc),
            Style::default().fg(Color::Yellow),
        ));
        spans.push(Span::styled(
            format!("◆ {}", entry.short_hash),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Right-aligned progress
    let progress_text = format!(
        "  ({}/{})  {} remaining",
        state.done_count, state.total_count, remaining
    );
    spans.push(Span::styled(
        progress_text,
        Style::default().fg(Color::DarkGray),
    ));

    let banner = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::Rgb(50, 40, 10)));
    frame.render_widget(banner, area);
}

/// Render entries + base commit as a single list so the base is always
/// directly below the last entry (no gap).
fn render_list(frame: &mut Frame, area: Rect, state: &RebaseModeState, theme: &Theme) {
    let highlight_bg = Color::Rgb(40, 40, 60);
    let is_in_progress = state.phase == RebasePhase::InProgress;

    // Pre-compute which entries are squash/fixup targets (Planning phase only).
    let len = state.entries.len();
    let mut squash_target_color: Vec<Option<Color>> = vec![None; len + 1];
    if !is_in_progress {
        for i in 0..len {
            let action = state.entries[i].action;
            if action == RebaseAction::Squash || action == RebaseAction::Fixup {
                let target_idx = i + 1;
                squash_target_color[target_idx] = Some(action_color(action));
            }
        }
    }

    let mut items: Vec<ListItem> = state
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let action = entry.action;
            let ac = action_color(action);
            let is_drop = action == RebaseAction::Drop;
            let is_selected = i == state.selected;
            let entry_status = entry.status;

            // Determine styling based on entry status
            let is_done = entry_status == EntryStatus::Done;
            let is_current = entry_status == EntryStatus::Current;

            let mut spans: Vec<Span> = Vec::new();

            // Node indicator
            let node_color = if is_done {
                Color::DarkGray
            } else if is_current {
                Color::Yellow
            } else {
                squash_target_color[i].unwrap_or(ac)
            };

            let is_pipe = !is_in_progress && (is_drop || action == RebaseAction::Squash);
            if is_pipe {
                let pipe_color = ac;
                let style = if is_selected {
                    Style::default().fg(pipe_color).bg(highlight_bg)
                } else {
                    Style::default().fg(pipe_color)
                };
                spans.push(Span::styled(" │   ", style));
            } else if is_done {
                // Checkmark for done entries
                let style = if is_selected {
                    Style::default().fg(Color::Green).bg(highlight_bg)
                } else {
                    Style::default().fg(Color::Green)
                };
                spans.push(Span::styled(" ✓   ", style));
            } else if is_current {
                // Highlighted current entry
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(highlight_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                };
                spans.push(Span::styled(" ▶   ", style));
            } else {
                let style = if is_selected {
                    Style::default().fg(node_color).bg(highlight_bg)
                } else {
                    Style::default().fg(node_color)
                };
                spans.push(Span::styled(" ◯   ", style));
            }

            // Action label
            let action_label = format!(" {:7} ", action.as_str());
            if is_done {
                // Muted action for done entries
                spans.push(Span::styled(
                    action_label,
                    Style::default().fg(Color::DarkGray).bg(Color::Rgb(40, 40, 40)),
                ));
            } else if is_current {
                // Yellow-tinted bg for current entry
                spans.push(Span::styled(
                    action_label,
                    Style::default().fg(Color::Black).bg(Color::Yellow),
                ));
            } else {
                spans.push(Span::styled(
                    action_label,
                    Style::default().fg(Color::Black).bg(ac),
                ));
            }

            // Separator
            let sep_style = if is_selected {
                Style::default().bg(highlight_bg)
            } else {
                Style::default()
            };
            spans.push(Span::styled("  ", sep_style));

            // Short hash
            let hash_style = if is_done {
                let s = Style::default().fg(Color::Rgb(80, 80, 80));
                if is_selected { s.bg(highlight_bg) } else { s }
            } else if is_current {
                let s = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
                if is_selected { s.bg(highlight_bg) } else { s }
            } else {
                let s = Style::default().fg(Color::Yellow);
                if is_selected { s.bg(highlight_bg) } else { s }
            };
            spans.push(Span::styled(format!("{} ", entry.short_hash), hash_style));

            // Commit message
            let msg_style = if is_done {
                let s = Style::default().fg(Color::DarkGray);
                if is_selected { s.bg(highlight_bg) } else { s }
            } else if is_drop {
                let s = Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT);
                if is_selected { s.bg(highlight_bg) } else { s }
            } else {
                let s = Style::default().fg(Color::White);
                if is_selected { s.bg(highlight_bg) } else { s }
            };
            spans.push(Span::styled(entry.message.clone(), msg_style));

            // Author (if available)
            if !entry.author_name.is_empty() {
                let author_style = if is_done {
                    Style::default().fg(Color::Rgb(60, 60, 60))
                } else if is_selected {
                    theme.commit_author.bg(highlight_bg)
                } else {
                    theme.commit_author
                };
                spans.push(Span::styled(
                    format!("  {}", entry.author_name),
                    author_style,
                ));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    // Append the base commit as the last row (not selectable, just visual).
    {
        let base_pad = " ".repeat(ACTION_LABEL_WIDTH + 2);
        let base_node_color = if is_in_progress { Color::DarkGray } else { squash_target_color[len].unwrap_or(Color::DarkGray) };
        let base_spans = vec![
            Span::styled(" ◯   ", Style::default().fg(base_node_color)),
            Span::styled(base_pad, Style::default()),
            Span::styled(
                format!("{} ", state.base_short_hash),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                &state.base_message,
                Style::default().fg(Color::DarkGray),
            ),
        ];
        items.push(ListItem::new(Line::from(base_spans)));
    }

    let list = List::new(items);

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    *list_state.offset_mut() = state.scroll;

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_status_bar(frame: &mut Frame, area: Rect, state: &RebaseModeState) {
    let hints: Vec<(&str, &str)> = match state.phase {
        RebasePhase::Planning => vec![
            ("p", "pick"),
            ("r", "reword"),
            ("e", "edit"),
            ("s", "squash"),
            ("f", "fixup"),
            ("d", "drop"),
            ("[ ]", "swap"),
            ("Alt+↑↓", "move"),
            ("Enter", "start"),
            ("q", "abort"),
            ("?", "help"),
        ],
        RebasePhase::InProgress => vec![
            ("Enter/c", "continue"),
            ("S", "skip"),
            ("A", "abort"),
            ("j/k", "navigate"),
            ("q", "close"),
            ("?", "help"),
        ],
    };

    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(
                    format!(" {} ", key),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{} ", desc), Style::default().fg(Color::DarkGray)),
            ]
        })
        .collect();

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}
