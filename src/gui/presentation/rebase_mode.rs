use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::config::Theme;
use crate::git::rebase::RebaseAction;
use crate::gui::modes::rebase_mode::{EntryStatus, RebaseModeState, RebasePhase};

/// Get the semantic color for a rebase action.
fn action_color(action: RebaseAction, theme: &Theme) -> Color {
    match action {
        RebaseAction::Pick => theme.rebase_pick,
        RebaseAction::Reword => theme.rebase_reword,
        RebaseAction::Edit => theme.rebase_edit,
        RebaseAction::Squash => theme.rebase_squash,
        RebaseAction::Fixup => theme.rebase_fixup,
        RebaseAction::Drop => theme.rebase_drop,
    }
}

/// Width of the action label box (e.g. " pick    ") including padding.
const ACTION_LABEL_WIDTH: usize = 9; // " {:7} " = 1 + 7 + 1

pub fn render(frame: &mut Frame, state: &mut RebaseModeState, theme: &Theme) {
    let area = frame.area();

    // Layout: Main bordered block (fill) | Status bar (1)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    render_main_block(frame, outer[0], state, theme);
    render_status_bar(frame, outer[1], state, theme);
}

fn render_main_block(frame: &mut Frame, area: Rect, state: &mut RebaseModeState, theme: &Theme) {
    // Build title line for the bordered block
    let mut title_spans = vec![
        Span::raw(" "),
        Span::styled(
            "Interactive Rebase",
            Style::default()
                .fg(theme.text_strong)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    match state.phase {
        RebasePhase::Planning => {
            title_spans.push(Span::styled(
                " ~ ",
                Style::default().fg(theme.text_dimmed),
            ));
            title_spans.push(Span::styled(
                format!("{} commits", state.entries.len()),
                Style::default().fg(theme.text_strong),
            ));
        }
        RebasePhase::InProgress => {
            title_spans.push(Span::styled(
                " ~ ",
                Style::default().fg(theme.text_dimmed),
            ));
            title_spans.push(Span::styled(
                format!("{}/{}", state.done_count, state.total_count),
                Style::default().fg(theme.text_strong),
            ));
        }
    }
    title_spans.push(Span::raw(" "));

    let block = Block::default()
        .title(Line::from(title_spans))
        .borders(Borders::ALL)
        .border_style(theme.active_border);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Inside the block: info line (1) | optional banner (2) | list (fill)
    let has_banner = state.phase == RebasePhase::InProgress;
    let banner_height = if has_banner { 2 } else { 0 };

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),              // branch + base info line
            Constraint::Length(banner_height),   // progress banner (InProgress only)
            Constraint::Min(1),                 // list
        ])
        .split(inner);

    render_info_line(frame, sections[0], state, theme);
    if has_banner {
        render_progress_banner(frame, sections[1], state, theme);
    }
    // Persist the actual list viewport height so keyboard/mouse handlers
    // scroll relative to what's currently rendered (terminal may resize).
    state.visible_height = sections[2].height as usize;
    // Clamp scroll offset against the new viewport size.
    let list_len = state.entries.len() + 1;
    let max_offset = list_len.saturating_sub(state.visible_height);
    if state.scroll > max_offset {
        state.scroll = max_offset;
    }
    render_list(frame, sections[2], state, theme);
}

fn render_info_line(frame: &mut Frame, area: Rect, state: &RebaseModeState, theme: &Theme) {
    let mut spans = vec![
        Span::styled(" ⎇ ", Style::default().fg(theme.accent)),
        Span::styled(
            &state.branch_name,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  onto  ", Style::default().fg(theme.text_dimmed)),
        Span::styled(
            &state.base_short_hash,
            Style::default().fg(theme.accent_secondary),
        ),
    ];

    if !state.base_message.is_empty() {
        spans.push(Span::styled(
            format!(" {}", state.base_message),
            Style::default().fg(theme.text_dimmed),
        ));
    }

    let paragraph = Paragraph::new(Line::from(spans));
    frame.render_widget(paragraph, area);
}

/// Render the "Rebase paused at ..." progress banner (InProgress only).
fn render_progress_banner(frame: &mut Frame, area: Rect, state: &RebaseModeState, theme: &Theme) {
    // Find the current (paused) entry
    let current = state.entries.iter().find(|e| e.status == EntryStatus::Current);
    let remaining = state.remaining_count();

    // Separator line above the banner
    let sep = "─".repeat(area.width as usize);
    let sep_area = Rect { height: 1, ..area };
    frame.render_widget(
        Paragraph::new(Span::styled(&sep, Style::default().fg(theme.text_dimmed))),
        sep_area,
    );

    // Banner content on the second line
    let banner_area = Rect {
        y: area.y + 1,
        height: 1,
        ..area
    };

    let mut spans = vec![
        Span::styled(
            " ⏸ ",
            Style::default()
                .fg(Color::Black)
                .bg(theme.accent_secondary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Paused", Style::default().fg(theme.accent_secondary)),
    ];

    if let Some(entry) = current {
        let action_desc = match entry.action {
            RebaseAction::Edit => "for editing",
            RebaseAction::Reword => "for rewording",
            _ => "due to conflict",
        };
        spans.push(Span::styled(
            format!(" {} at ", action_desc),
            Style::default().fg(theme.accent_secondary),
        ));
        spans.push(Span::styled(
            &entry.short_hash,
            Style::default()
                .fg(theme.text_strong)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans.push(Span::styled(
        format!("  {} remaining", remaining),
        Style::default().fg(theme.text_dimmed),
    ));

    let banner = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(theme.rebase_paused_bg));
    frame.render_widget(banner, banner_area);
}

/// Render entries + base commit as a single list so the base is always
/// directly below the last entry (no gap).
fn render_list(frame: &mut Frame, area: Rect, state: &RebaseModeState, theme: &Theme) {
    let sel_bg = theme.selected_line;
    let is_in_progress = state.phase == RebasePhase::InProgress;

    // Pre-compute which entries are squash/fixup targets (Planning phase only).
    let len = state.entries.len();
    let mut squash_target_color: Vec<Option<Color>> = vec![None; len + 1];
    if !is_in_progress {
        for i in 0..len {
            let action = state.entries[i].action;
            if action == RebaseAction::Squash || action == RebaseAction::Fixup {
                let target_idx = i + 1;
                squash_target_color[target_idx] = Some(action_color(action, theme));
            }
        }
    }

    let mut items: Vec<ListItem> = state
        .entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let action = entry.action;
            let ac = action_color(action, theme);
            let is_drop = action == RebaseAction::Drop;
            let is_selected = i == state.selected;
            let entry_status = entry.status;

            // Determine styling based on entry status
            let is_done = entry_status == EntryStatus::Done;
            let is_current = entry_status == EntryStatus::Current;

            let mut spans: Vec<Span> = Vec::new();

            // Node indicator
            let node_color = if is_done {
                theme.text_dimmed
            } else if is_current {
                theme.accent_secondary
            } else {
                squash_target_color[i].unwrap_or(ac)
            };

            let is_pipe = !is_in_progress && (is_drop || action == RebaseAction::Squash);
            if is_pipe {
                let style = Style::default().fg(ac);
                spans.push(Span::styled(" │  ", style));
            } else if is_done {
                spans.push(Span::styled(" ✓  ", Style::default().fg(theme.accent)));
            } else if is_current {
                spans.push(Span::styled(
                    " ▶  ",
                    Style::default()
                        .fg(theme.accent_secondary)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(" ◯  ", Style::default().fg(node_color)));
            }

            // Action label
            let action_label = format!(" {:7} ", action.as_str());
            if is_done {
                spans.push(Span::styled(
                    action_label,
                    Style::default()
                        .fg(theme.text_dimmed)
                        .bg(theme.selected_bg),
                ));
            } else if is_current {
                spans.push(Span::styled(
                    action_label,
                    Style::default().fg(Color::Black).bg(theme.accent_secondary),
                ));
            } else {
                spans.push(Span::styled(
                    action_label,
                    Style::default().fg(Color::Black).bg(ac),
                ));
            }

            // Separator
            spans.push(Span::raw(" "));

            // Short hash
            let hash_style = if is_done {
                Style::default().fg(theme.text_dimmed)
            } else if is_current {
                Style::default()
                    .fg(theme.accent_secondary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.accent_secondary)
            };
            spans.push(Span::styled(format!("{} ", entry.short_hash), hash_style));

            // Commit message
            let msg_style = if is_done {
                Style::default().fg(theme.text_dimmed)
            } else if is_drop {
                Style::default()
                    .fg(theme.text_dimmed)
                    .add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default().fg(theme.text_strong)
            };
            spans.push(Span::styled(entry.message.clone(), msg_style));

            // Author (if available)
            if !entry.author_name.is_empty() {
                let author_style = if is_done {
                    Style::default().fg(theme.diff_line_number)
                } else {
                    theme.commit_author
                };
                spans.push(Span::styled(
                    format!("  {}", entry.author_name),
                    author_style,
                ));
            }

            let item = ListItem::new(Line::from(spans));
            if is_selected {
                item.style(sel_bg)
            } else {
                item
            }
        })
        .collect();

    // Append the base commit as the last row (not selectable, just visual).
    {
        let base_pad = " ".repeat(ACTION_LABEL_WIDTH + 1);
        let base_node_color = if is_in_progress {
            theme.text_dimmed
        } else {
            squash_target_color[len].unwrap_or(theme.text_dimmed)
        };
        let base_spans = vec![
            Span::styled(" ◯  ", Style::default().fg(base_node_color)),
            Span::styled(base_pad, Style::default()),
            Span::styled(
                format!("{} ", state.base_short_hash),
                Style::default().fg(theme.text_dimmed),
            ),
            Span::styled(
                &state.base_message,
                Style::default().fg(theme.text_dimmed),
            ),
        ];
        items.push(ListItem::new(Line::from(base_spans)));
    }

    let list = List::new(items);

    // Note: deliberately *not* calling list_state.select(...) — ratatui's
    // stateful List adjusts the offset to keep the selected item visible,
    // which fights our manual scroll model where mouse-scroll just moves
    // the viewport without changing the selection. Per-item highlighting
    // is already baked into the items via `is_selected` above.
    let mut list_state = ListState::default();
    *list_state.offset_mut() = state.scroll;

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_status_bar(frame: &mut Frame, area: Rect, state: &RebaseModeState, theme: &Theme) {
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

    let key_style = Style::default().fg(theme.text).add_modifier(ratatui::style::Modifier::BOLD);
    let desc_style = Style::default().fg(theme.text_dimmed);
    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(format!(" {} ", key), key_style),
                Span::styled(format!("{} ", desc), desc_style),
            ]
        })
        .collect();

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}
