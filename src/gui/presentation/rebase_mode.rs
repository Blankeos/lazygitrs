use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::config::Theme;
use crate::git::rebase::RebaseAction;
use crate::gui::modes::rebase_mode::RebaseModeState;

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

    // Layout: Header (3) | List (fill) | Status bar (1)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(frame, outer[0], state, theme);
    render_list(frame, outer[1], state, theme);
    render_status_bar(frame, outer[2]);
}

fn render_header(frame: &mut Frame, area: Rect, state: &RebaseModeState, theme: &Theme) {
    let title_spans = vec![
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
        Span::styled(
            format!("Rebasing {} commits onto ", state.entries.len()),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!("◆ {}", state.base_short_hash),
            Style::default().fg(Color::Yellow),
        ),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.active_border);
    let paragraph = Paragraph::new(Line::from(title_spans)).block(block);
    frame.render_widget(paragraph, area);
}

/// Render entries + base commit as a single list so the base is always
/// directly below the last entry (no gap).
fn render_list(frame: &mut Frame, area: Rect, state: &RebaseModeState, theme: &Theme) {
    // We'll render manually line-by-line so we can control highlighting
    // per-span (keeping action label bg above the selection highlight).
    let highlight_bg = Color::Rgb(40, 40, 60);

    // Pre-compute which entries are squash/fixup targets.
    // In newest-first display order, if entry[i] is squash/fixup, then entry[i+1]
    // is the target it will be merged into. The node of the target gets tinted
    // with the squash/fixup color to visually indicate the merge.
    let len = state.entries.len();
    let mut squash_target_color: Vec<Option<Color>> = vec![None; len + 1]; // +1 for base
    for i in 0..len {
        let action = state.entries[i].action;
        if action == RebaseAction::Squash || action == RebaseAction::Fixup {
            let target_idx = i + 1; // the commit below in display order
            squash_target_color[target_idx] = Some(action_color(action));
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

            let mut spans: Vec<Span> = Vec::new();

            // Node indicator: matches commits view style (◯ hollow circle).
            // Drop and Squash = straight line │ (no node — they don't survive as
            // independent commits). Others = ◯.
            // If this entry is a squash/fixup target, tint the node with the source color.
            let node_color = squash_target_color[i].unwrap_or(ac);
            let is_pipe = is_drop || action == RebaseAction::Squash;
            if is_pipe {
                let pipe_color = ac; // use the action's own color for the pipe
                let style = if is_selected {
                    Style::default().fg(pipe_color).bg(highlight_bg)
                } else {
                    Style::default().fg(pipe_color)
                };
                spans.push(Span::styled(" │   ", style));
            } else {
                let style = if is_selected {
                    Style::default().fg(node_color).bg(highlight_bg)
                } else {
                    Style::default().fg(node_color)
                };
                spans.push(Span::styled(" ◯   ", style));
            }

            // Action label — always keep its bg color (above highlight)
            let action_label = format!(" {:7} ", action.as_str());
            spans.push(Span::styled(
                action_label,
                Style::default().fg(Color::Black).bg(ac),
            ));

            // Separator
            let sep_style = if is_selected {
                Style::default().bg(highlight_bg)
            } else {
                Style::default()
            };
            spans.push(Span::styled("  ", sep_style));

            // Short hash (before message)
            let hash_style = if is_selected {
                Style::default().fg(Color::Yellow).bg(highlight_bg)
            } else {
                Style::default().fg(Color::Yellow)
            };
            spans.push(Span::styled(format!("{} ", entry.short_hash), hash_style));

            // Commit message
            let msg_style = if is_drop {
                let s = Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT);
                if is_selected { s.bg(highlight_bg) } else { s }
            } else {
                let s = Style::default().fg(Color::White);
                if is_selected { s.bg(highlight_bg) } else { s }
            };
            spans.push(Span::styled(entry.message.clone(), msg_style));

            // Author
            let author_style = if is_selected {
                theme.commit_author.bg(highlight_bg)
            } else {
                theme.commit_author
            };
            spans.push(Span::styled(
                format!("  {}", entry.author_name),
                author_style,
            ));

            ListItem::new(Line::from(spans))
        })
        .collect();

    // Append the base commit as the last row (not selectable, just visual).
    // Pad to align with entries: node(5) + action_label(9) + sep(2) = 16 chars before hash.
    let base_pad = " ".repeat(ACTION_LABEL_WIDTH + 2); // action box width + separator
    let base_node_color = squash_target_color[len].unwrap_or(Color::DarkGray);
    let base_spans = vec![
        Span::styled(" ◯   ", Style::default().fg(base_node_color)),
        Span::styled(
            base_pad,
            Style::default(),
        ),
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

    // No highlight_style on the List — we handle highlighting per-span above
    let list = List::new(items);

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    *list_state.offset_mut() = state.scroll;

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_status_bar(frame: &mut Frame, area: Rect) {
    let hints = vec![
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
    ];

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
