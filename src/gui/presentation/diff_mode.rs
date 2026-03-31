use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::config::Theme;
use crate::gui::modes::diff_mode::{DiffModeFocus, DiffModeState, RefKind};
use crate::model::{CommitFile, FileChangeStatus};
use crate::model::file_tree::CommitFileTreeNode;
use crate::pager::side_by_side::{self, DiffViewState};

/// Max items visible in the dropdown at once.
const DROPDOWN_MAX_VISIBLE: usize = 10;

pub fn render(
    frame: &mut Frame,
    state: &DiffModeState,
    diff_view: &mut DiffViewState,
    theme: &Theme,
) {
    let area = frame.area();

    // Overall layout: sidebar (left) | diff panel (right) | status bar at bottom
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(33), Constraint::Percentage(67)])
        .split(outer[0]);

    // Left sidebar: [A selector (3 lines)] [B selector (3 lines)] [Commit Files (rest)]
    let sidebar = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(content[0]);

    render_selector(frame, sidebar[0], state, DiffModeFocus::SelectorA, theme);
    render_selector(frame, sidebar[1], state, DiffModeFocus::SelectorB, theme);
    render_commit_files(frame, sidebar[2], state, theme);

    // Right panel: diff exploration
    render_diff_panel(frame, content[1], state, diff_view, theme);

    // Text selection highlight overlay and tooltip (must be before popups/dropdowns)
    crate::gui::views::render_selection_overlay(frame, diff_view, content[1], theme);

    // Status bar
    render_status_bar(frame, outer[1], state, theme);

    // Render combobox dropdown overlay on top of the sidebar
    if state.editing.is_some() {
        render_dropdown(frame, sidebar, state, theme);
    }
}

fn render_selector(
    frame: &mut Frame,
    area: Rect,
    state: &DiffModeState,
    which: DiffModeFocus,
    theme: &Theme,
) {
    let (is_a, focused, editing, display, number_label) = match which {
        DiffModeFocus::SelectorA => (
            true,
            state.focus == DiffModeFocus::SelectorA,
            matches!(state.editing, Some(crate::gui::modes::diff_mode::DiffModeSelector::A)),
            &state.ref_a_display,
            " 1 A ",
        ),
        DiffModeFocus::SelectorB => (
            false,
            state.focus == DiffModeFocus::SelectorB,
            matches!(state.editing, Some(crate::gui::modes::diff_mode::DiffModeSelector::B)),
            &state.ref_b_display,
            " 2 B ",
        ),
        _ => return,
    };

    let border = if focused || editing {
        theme.active_border
    } else {
        Style::default().fg(theme.text_dimmed)
    };
    let block = Block::default()
        .title(number_label)
        .borders(Borders::ALL)
        .border_style(border);

    if editing {
        // Render the textarea inside the block
        if let Some(ref ta) = state.textarea {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            frame.render_widget(&*ta, inner);
        }
    } else {
        let text = if display.is_empty() {
            "Press Enter to select ref..."
        } else {
            display.as_str()
        };
        let style = if display.is_empty() {
            Style::default().fg(theme.text_dimmed)
        } else {
            Style::default().fg(theme.accent)
        };
        let widget = Paragraph::new(Span::styled(format!(" {}", text), style)).block(block);
        frame.render_widget(widget, area);
    }
}

fn render_commit_files(
    frame: &mut Frame,
    area: Rect,
    state: &DiffModeState,
    theme: &Theme,
) {
    let focused = state.focus == DiffModeFocus::CommitFiles;
    let border = if focused {
        theme.active_border
    } else {
        Style::default().fg(theme.text_dimmed)
    };
    let tree_indicator = if state.show_tree { " (tree)" } else { "" };
    let title = format!(" 3 Commit Files ({}{}) ", state.diff_files.len(), tree_indicator);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border);

    if state.diff_files.is_empty() {
        let msg = if state.has_both_refs() {
            "No files changed"
        } else {
            "Select refs A and B to compare"
        };
        let widget = Paragraph::new(Span::styled(
            format!(" {}", msg),
            Style::default().fg(theme.text_dimmed),
        ))
        .block(block);
        frame.render_widget(widget, area);
        return;
    }

    if state.show_tree {
        let items: Vec<ListItem> = state
            .tree_nodes
            .iter()
            .map(|node| render_tree_node(node, state, theme))
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(theme.selected_line);

        let mut list_state = ListState::default();
        list_state.select(Some(state.diff_files_selected));
        frame.render_stateful_widget(list, area, &mut list_state);
    } else {
        let items: Vec<ListItem> = state
            .diff_files
            .iter()
            .map(|file| {
                let (status_style, status_icon) = commit_file_status_display(file, theme);
                let line = Line::from(vec![
                    Span::styled(format!(" {} ", status_icon), status_style),
                    Span::styled(file.name.clone(), Style::default().fg(theme.text_strong)),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(theme.selected_line);

        let mut list_state = ListState::default();
        list_state.select(Some(state.diff_files_selected));
        frame.render_stateful_widget(list, area, &mut list_state);
    }
}

fn render_tree_node<'a>(
    node: &CommitFileTreeNode,
    state: &DiffModeState,
    theme: &Theme,
) -> ListItem<'a> {
    let indent = "  ".repeat(node.depth);
    if node.is_dir {
        let is_collapsed = state.collapsed_dirs.contains(&node.path);
        let icon = if is_collapsed { "▶ " } else { "▼ " };
        let line = Line::from(vec![
            Span::styled(
                format!("  {}{}", indent, icon),
                Style::default().fg(theme.text_strong),
            ),
            Span::styled(node.name.clone(), Style::default().fg(theme.text_strong)),
        ]);
        ListItem::new(line)
    } else if let Some(file_idx) = node.file_index {
        if let Some(file) = state.diff_files.get(file_idx) {
            let (status_style, status_icon) = commit_file_status_display(file, theme);
            let line = Line::from(vec![
                Span::styled(format!(" {} ", status_icon), status_style),
                Span::raw(indent),
                Span::styled(node.name.clone(), Style::default().fg(theme.text_strong)),
            ]);
            ListItem::new(line)
        } else {
            ListItem::new(Line::raw(""))
        }
    } else {
        ListItem::new(Line::raw(""))
    }
}

fn render_diff_panel(
    frame: &mut Frame,
    area: Rect,
    state: &DiffModeState,
    diff_view: &mut DiffViewState,
    theme: &Theme,
) {
    let focused = state.focus == DiffModeFocus::DiffExploration;

    if !diff_view.is_empty() {
        side_by_side::render_diff(frame, area, diff_view, theme, focused);
        side_by_side::render_diff_search_highlights(frame, area, diff_view, theme);
        side_by_side::render_diff_search_bar(frame, area, diff_view, theme);
    } else {
        let border = if focused {
            theme.active_border
        } else {
            Style::default().fg(theme.text_dimmed)
        };
        let block = Block::default()
            .title(" 4 Diff ")
            .borders(Borders::ALL)
            .border_style(border);
        let widget = Paragraph::new(Span::styled(
            " Select a file to view diff",
            Style::default().fg(theme.text_dimmed),
        ))
        .block(block);
        frame.render_widget(widget, area);
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, state: &DiffModeState, theme: &Theme) {
    // If search is active or has results, show search bar instead of hints
    if state.file_search_active {
        if let Some(ref ta) = state.file_search_textarea {
            let match_info = if !state.file_search_matches.is_empty() {
                format!(
                    " {}/{}",
                    state.file_search_match_idx + 1,
                    state.file_search_matches.len()
                )
            } else if !state.file_search_query.is_empty() {
                " (no matches)".to_string()
            } else {
                String::new()
            };

            let prefix_width = 2u16; // " /"
            let suffix_width = match_info.len() as u16;
            let ta_width = area.width.saturating_sub(prefix_width + suffix_width);

            let prefix_rect = Rect::new(area.x, area.y, prefix_width, 1);
            let prefix = Paragraph::new(Span::styled(" /", Style::default().fg(theme.accent_secondary)));
            frame.render_widget(prefix, prefix_rect);

            let ta_rect = Rect::new(area.x + prefix_width, area.y, ta_width, 1);
            frame.render_widget(&*ta, ta_rect);

            if !match_info.is_empty() {
                let suffix_rect = Rect::new(area.x + prefix_width + ta_width, area.y, suffix_width, 1);
                let suffix = Paragraph::new(Span::styled(match_info, Style::default().fg(theme.accent_secondary)));
                frame.render_widget(suffix, suffix_rect);
            }
            return;
        }
    } else if !state.file_search_query.is_empty() {
        // Search dismissed but results persist — show query + match info
        let match_info = if !state.file_search_matches.is_empty() {
            format!(
                " {}/{}",
                state.file_search_match_idx + 1,
                state.file_search_matches.len()
            )
        } else {
            " (no matches)".to_string()
        };
        let bar = Paragraph::new(Span::styled(
            format!(" /{}{}", state.file_search_query, match_info),
            Style::default().fg(theme.accent_secondary),
        ));
        frame.render_widget(bar, area);
        return;
    }

    let hints = if state.editing.is_some() {
        vec![
            ("Enter", "select"),
            ("Esc", "cancel"),
            ("↑↓", "navigate"),
        ]
    } else {
        vec![
            ("q", "exit"),
            ("Tab", "cycle"),
            ("1-4", "panel"),
            ("<c-s>", "swap"),
            ("`", "tree"),
            ("?", "help"),
        ]
    };

    let key_style = Style::default()
        .fg(theme.text)
        .add_modifier(Modifier::BOLD);
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

fn render_dropdown(
    frame: &mut Frame,
    sidebar: std::rc::Rc<[Rect]>,
    state: &DiffModeState,
    theme: &Theme,
) {
    // Position dropdown below the relevant selector
    let anchor = if matches!(state.editing, Some(crate::gui::modes::diff_mode::DiffModeSelector::A)) {
        sidebar[0]
    } else {
        sidebar[1]
    };

    let total = state.search_results.len();
    if total == 0 {
        return;
    }

    let max_items = DROPDOWN_MAX_VISIBLE.min(total);
    let dropdown_height = (max_items as u16) + 2; // +2 for borders
    let available_height = frame.area().height.saturating_sub(anchor.y + anchor.height);
    let dropdown_area = Rect {
        x: anchor.x,
        y: anchor.y + anchor.height,
        width: anchor.width,
        height: dropdown_height.min(available_height),
    };

    if dropdown_area.height < 3 {
        return;
    }

    frame.render_widget(Clear, dropdown_area);

    // Compute visible window
    let visible_count = (dropdown_area.height as usize).saturating_sub(2); // -2 for borders
    let scroll = state.dropdown_scroll;
    let visible_end = (scroll + visible_count).min(total);

    let items: Vec<ListItem> = state
        .search_results
        .iter()
        .skip(scroll)
        .take(visible_end - scroll)
        .map(|candidate| {
            let kind_label = match candidate.kind {
                RefKind::RawRef => Span::styled("[ref] ", Style::default().fg(theme.text_strong)),
                RefKind::Branch => Span::styled("[branch] ", Style::default().fg(theme.ref_local)),
                RefKind::RemoteBranch => {
                    Span::styled("[remote] ", Style::default().fg(theme.ref_remote))
                }
                RefKind::Tag => Span::styled("[tag] ", Style::default().fg(theme.ref_tag)),
                RefKind::Commit => Span::styled("[commit] ", Style::default().fg(theme.reflog_hash)),
            };
            let line = Line::from(vec![
                Span::raw(" "),
                kind_label,
                Span::styled(candidate.display.clone(), Style::default().fg(theme.text_strong)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.active_border);

    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected_line);

    let mut list_state = ListState::default();
    // Selected index relative to the visible window
    let relative_selected = state.search_selected.saturating_sub(scroll);
    list_state.select(Some(relative_selected));
    frame.render_stateful_widget(list, dropdown_area, &mut list_state);
}

fn commit_file_status_display<'a>(file: &CommitFile, theme: &Theme) -> (Style, &'a str) {
    match file.status {
        FileChangeStatus::Added => (theme.file_staged, "A "),
        FileChangeStatus::Deleted => (Style::default().fg(theme.change_deleted), "D "),
        FileChangeStatus::Modified => (theme.file_unstaged, "M "),
        FileChangeStatus::Renamed => (Style::default().fg(theme.change_renamed), "R "),
        FileChangeStatus::Copied => (Style::default().fg(theme.change_copied), "C "),
        FileChangeStatus::Unmerged => (Style::default().fg(theme.change_unmerged), "U "),
    }
}
