use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::config::AppConfig;
use crate::model::Model;

use super::context::{ContextId, ContextManager};
use super::layout::{self, LayoutState};
use super::popup::PopupState;
use super::presentation;
use super::ScreenMode;

pub fn render(
    frame: &mut Frame,
    model: &Model,
    ctx_mgr: &ContextManager,
    layout_state: &LayoutState,
    popup: &PopupState,
    config: &AppConfig,
    screen_mode: ScreenMode,
) {
    let area = frame.area();
    let theme = config.user_config.theme();
    let panel_count = ContextId::SIDEBAR_ORDER.len();

    let fl = layout::compute_layout(area, layout_state.side_panel_ratio, panel_count);

    // Render sidebar panels
    for (i, ctx_id) in ContextId::SIDEBAR_ORDER.iter().enumerate() {
        if i >= fl.side_panels.len() {
            break;
        }
        let rect = fl.side_panels[i];
        let is_active = *ctx_id == ctx_mgr.active();
        let selected = ctx_mgr.selected(*ctx_id);

        let border_style = if is_active {
            theme.active_border
        } else {
            theme.inactive_border
        };

        let title = format!(
            " {} {} ",
            ctx_id.short_key(),
            ctx_id.title()
        );

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        match ctx_id {
            ContextId::Status => {
                let status_text = render_status_panel(model, config);
                let widget = Paragraph::new(status_text).block(block);
                frame.render_widget(widget, rect);
            }
            ContextId::Files => {
                let items = presentation::files::render_file_list(model, &theme);
                render_list(frame, rect, block, items, selected, &theme);
            }
            ContextId::Branches => {
                let items = presentation::branches::render_branch_list(model, &theme);
                render_list(frame, rect, block, items, selected, &theme);
            }
            ContextId::Commits => {
                let items = presentation::commits::render_commit_list(model, &theme);
                render_list(frame, rect, block, items, selected, &theme);
            }
            ContextId::Stash => {
                let items = presentation::stash::render_stash_list(model, &theme);
                render_list(frame, rect, block, items, selected, &theme);
            }
            _ => {
                let widget = Paragraph::new("").block(block);
                frame.render_widget(widget, rect);
            }
        }
    }

    // Render main panel (diff view placeholder)
    {
        let block = Block::default()
            .title(" Main ")
            .borders(Borders::ALL)
            .border_style(theme.inactive_border);

        let diff_text = get_main_panel_content(model, ctx_mgr);
        let widget = Paragraph::new(diff_text).block(block);
        frame.render_widget(widget, fl.main_panel);
    }

    // Render status bar
    render_status_bar(frame, fl.status_bar, ctx_mgr, &theme);

    // Render popup overlay
    if *popup != PopupState::None {
        render_popup(frame, popup, area);
    }
}

fn render_status_panel<'a>(model: &Model, config: &AppConfig) -> Vec<Line<'a>> {
    let branch_name = model
        .branches
        .iter()
        .find(|b| b.head)
        .map(|b| b.name.as_str())
        .unwrap_or("detached");

    vec![
        Line::from(Span::styled(
            format!(" {} ", config.user_config.gui.nerd_fonts_version),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(format!(" Branch: {}", branch_name)),
        Line::from(format!(" Commits: {}", model.commits.len())),
        Line::from(format!(" Files: {}", model.files.len())),
    ]
}

fn render_list(
    frame: &mut Frame,
    rect: Rect,
    block: Block<'_>,
    items: Vec<ListItem<'_>>,
    selected: usize,
    theme: &crate::config::Theme,
) {
    if items.is_empty() {
        let widget = Paragraph::new(" (empty)").block(block);
        frame.render_widget(widget, rect);
        return;
    }

    // Compute visible area inside the block
    let inner = block.inner(rect);
    let visible_height = inner.height as usize;

    // Determine scroll offset
    let offset = if selected >= visible_height {
        selected - visible_height + 1
    } else {
        0
    };

    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(offset)
        .enumerate()
        .map(|(i, item)| {
            if i + offset == selected {
                item.style(theme.selected_line)
            } else {
                item
            }
        })
        .collect();

    let list = List::new(visible_items).block(block);
    frame.render_widget(list, rect);
}

fn get_main_panel_content<'a>(model: &Model, ctx_mgr: &ContextManager) -> Vec<Line<'a>> {
    let active = ctx_mgr.active();
    let selected = ctx_mgr.selected_active();

    match active {
        ContextId::Files => {
            if let Some(file) = model.files.get(selected) {
                vec![
                    Line::from(format!(" File: {}", file.name)),
                    Line::from(format!(" Status: {}", file.short_status)),
                    Line::from(""),
                    Line::from(" (diff view coming in Phase 2)"),
                ]
            } else {
                vec![Line::from(" No files selected")]
            }
        }
        ContextId::Commits => {
            if let Some(commit) = model.commits.get(selected) {
                vec![
                    Line::from(format!(" Commit: {}", commit.short_hash())),
                    Line::from(format!(" Author: {} <{}>", commit.author_name, commit.author_email)),
                    Line::from(format!(" Message: {}", commit.name)),
                    Line::from(""),
                    Line::from(" (diff view coming in Phase 2)"),
                ]
            } else {
                vec![Line::from(" No commit selected")]
            }
        }
        ContextId::Branches => {
            if let Some(branch) = model.branches.get(selected) {
                let mut lines = vec![
                    Line::from(format!(" Branch: {}", branch.name)),
                    Line::from(format!(" Hash: {}", branch.hash)),
                ];
                if let Some(ref upstream) = branch.upstream {
                    lines.push(Line::from(format!(" Upstream: {}", upstream)));
                }
                if let Some((ahead, behind)) = branch.ahead_behind() {
                    lines.push(Line::from(format!(" Ahead: {}, Behind: {}", ahead, behind)));
                }
                lines
            } else {
                vec![Line::from(" No branch selected")]
            }
        }
        ContextId::Stash => {
            if let Some(entry) = model.stash_entries.get(selected) {
                vec![
                    Line::from(format!(" Stash: {}", entry.ref_name())),
                    Line::from(format!(" {}", entry.name)),
                ]
            } else {
                vec![Line::from(" No stash entries")]
            }
        }
        _ => vec![Line::from(" lazygitrs")],
    }
}

fn render_status_bar(
    frame: &mut Frame,
    rect: Rect,
    ctx_mgr: &ContextManager,
    theme: &crate::config::Theme,
) {
    let hints = match ctx_mgr.active() {
        ContextId::Files => "c: commit | a: stage all | <space>: toggle stage | d: discard | q: quit",
        ContextId::Branches => "<space>: checkout | n: new | d: delete | M: merge | r: rebase | q: quit",
        ContextId::Commits => "s: squash | r: reword | <space>: checkout | t: revert | q: quit",
        ContextId::Stash => "g: pop | <space>: apply | d: drop | q: quit",
        _ => "q: quit | tab: switch panel | j/k: navigate",
    };

    let bar = Paragraph::new(Span::styled(
        format!(" {}", hints),
        theme.status_bar,
    ));
    frame.render_widget(bar, rect);
}

fn render_popup(frame: &mut Frame, popup: &PopupState, area: Rect) {
    let popup_width = (area.width * 60 / 100).min(60).max(30);
    let popup_height = 10u16;

    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_rect = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_rect);

    match popup {
        PopupState::Confirm { title, message, .. } => {
            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));

            let text = vec![
                Line::from(""),
                Line::from(format!(" {}", message)),
                Line::from(""),
                Line::from(Span::styled(
                    " [y]es / [n]o",
                    Style::default().fg(Color::Yellow),
                )),
            ];

            let widget = Paragraph::new(text).block(block);
            frame.render_widget(widget, popup_rect);
        }
        PopupState::Input { title, buffer, .. } => {
            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));

            let text = vec![
                Line::from(""),
                Line::from(format!(" > {}_", buffer)),
                Line::from(""),
                Line::from(Span::styled(
                    " Enter to confirm, Esc to cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let widget = Paragraph::new(text).block(block);
            frame.render_widget(widget, popup_rect);
        }
        PopupState::Menu {
            title,
            items,
            selected,
        } => {
            let height = (items.len() as u16 + 4).min(area.height - 4);
            let popup_rect = Rect::new(x, y, popup_width, height);
            frame.render_widget(Clear, popup_rect);

            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta));

            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let style = if i == *selected {
                        Style::default()
                            .bg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    let label = if let Some(ref key) = item.key {
                        format!(" {} {}", key, item.label)
                    } else {
                        format!("  {}", item.label)
                    };

                    ListItem::new(label).style(style)
                })
                .collect();

            let list = List::new(list_items).block(block);
            frame.render_widget(list, popup_rect);
        }
        PopupState::None => {}
    }
}
