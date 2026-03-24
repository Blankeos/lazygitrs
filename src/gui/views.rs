use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::config::AppConfig;
use crate::model::Model;
use crate::pager::side_by_side::{self, DiffViewState};

use super::context::{ContextId, ContextManager, SideWindow};
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
    diff_view: &DiffViewState,
    _screen_mode: ScreenMode,
) {
    let area = frame.area();
    let theme = config.user_config.theme();
    let panel_count = SideWindow::ALL.len();

    let fl = layout::compute_layout(area, layout_state.side_panel_ratio, panel_count);

    // Render sidebar panels — one per window
    for (i, window) in SideWindow::ALL.iter().enumerate() {
        if i >= fl.side_panels.len() {
            break;
        }
        let rect = fl.side_panels[i];
        let ctx_id = ctx_mgr.active_context_for_window(*window);
        let is_active = ctx_mgr.active_window() == *window;
        let selected = ctx_mgr.selected(ctx_id);

        let border_style = if is_active {
            theme.active_border
        } else {
            theme.inactive_border
        };

        // Build title with tab indicators for multi-tab windows
        let title = build_window_title(*window, ctx_id, ctx_mgr);

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
            ContextId::Worktrees => {
                let items = render_worktree_list(model);
                render_list(frame, rect, block, items, selected, &theme);
            }
            ContextId::Submodules => {
                let widget = Paragraph::new(" (no submodules)").block(block);
                frame.render_widget(widget, rect);
            }
            ContextId::Branches => {
                let items = presentation::branches::render_branch_list(model, &theme);
                render_list(frame, rect, block, items, selected, &theme);
            }
            ContextId::Remotes => {
                let items = presentation::remotes::render_remote_list(model, &theme);
                render_list(frame, rect, block, items, selected, &theme);
            }
            ContextId::Tags => {
                let items = presentation::tags::render_tag_list(model, &theme);
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

    // Render main panel — side-by-side diff view
    if !diff_view.is_empty() {
        side_by_side::render_diff(frame, fl.main_panel, diff_view, &theme);
    } else {
        // Fallback: show info about selected item
        let block = Block::default()
            .title(" Diff ")
            .borders(Borders::ALL)
            .border_style(theme.inactive_border);

        let info = get_info_content(model, ctx_mgr);
        let widget = Paragraph::new(info).block(block);
        frame.render_widget(widget, fl.main_panel);
    }

    // Render status bar
    render_status_bar(frame, fl.status_bar, ctx_mgr, diff_view, &theme);

    // Render popup overlay
    if *popup != PopupState::None {
        render_popup(frame, popup, area);
    }
}

/// Build a window title like " 3 Branches | Remotes | Tags " with the active tab highlighted.
fn build_window_title(window: SideWindow, active_ctx: ContextId, _ctx_mgr: &ContextManager) -> String {
    let tabs = window.tabs();
    let key = window.key_label();

    if tabs.len() == 1 {
        return format!(" {} {} ", key, tabs[0].title());
    }

    let tab_parts: Vec<String> = tabs
        .iter()
        .map(|ctx| {
            if *ctx == active_ctx {
                format!("[{}]", ctx.title())
            } else {
                ctx.title().to_string()
            }
        })
        .collect();

    format!(" {} {} ", key, tab_parts.join(" | "))
}

fn render_status_panel<'a>(model: &Model, config: &AppConfig) -> Vec<Line<'a>> {
    let branch_name = model
        .branches
        .iter()
        .find(|b| b.head)
        .map(|b| b.name.as_str())
        .unwrap_or("detached");

    let mut lines = vec![
        Line::from(Span::styled(
            format!(" {} ", config.user_config.gui.nerd_fonts_version),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(format!(" Branch: {}", branch_name)),
        Line::from(format!(" Commits: {}", model.commits.len())),
        Line::from(format!(" Files: {}", model.files.len())),
    ];

    // Show in-progress operation status
    if model.is_rebasing {
        lines.push(Line::from(Span::styled(
            " REBASING (m: options)",
            Style::default().fg(Color::Yellow),
        )));
    }
    if model.is_merging {
        lines.push(Line::from(Span::styled(
            " MERGING",
            Style::default().fg(Color::Yellow),
        )));
    }
    if model.is_cherry_picking {
        lines.push(Line::from(Span::styled(
            " CHERRY-PICKING",
            Style::default().fg(Color::Yellow),
        )));
    }
    if model.is_bisecting {
        lines.push(Line::from(Span::styled(
            " BISECTING",
            Style::default().fg(Color::Magenta),
        )));
    }

    lines
}

fn render_worktree_list<'a>(model: &Model) -> Vec<ListItem<'a>> {
    model
        .worktrees
        .iter()
        .map(|wt| {
            let marker = if wt.is_current { "* " } else { "  " };
            let line = Line::from(vec![
                Span::styled(
                    marker.to_string(),
                    Style::default().fg(Color::Green),
                ),
                Span::styled(
                    wt.branch.clone(),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    format!(" {}", wt.path),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect()
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

    let inner = block.inner(rect);
    let visible_height = inner.height as usize;

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

fn get_info_content<'a>(model: &Model, ctx_mgr: &ContextManager) -> Vec<Line<'a>> {
    let active = ctx_mgr.active();
    let selected = ctx_mgr.selected_active();

    match active {
        ContextId::Files => {
            if model.files.is_empty() {
                vec![Line::from(" No modified files")]
            } else {
                vec![Line::from(" Select a file to view diff")]
            }
        }
        ContextId::Commits => {
            if let Some(commit) = model.commits.get(selected) {
                vec![
                    Line::from(format!(" Commit: {}", commit.short_hash())),
                    Line::from(format!(
                        " Author: {} <{}>",
                        commit.author_name, commit.author_email
                    )),
                    Line::from(format!(" Message: {}", commit.name)),
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
        ContextId::Remotes => {
            if let Some(remote) = model.remotes.get(selected) {
                let mut lines = vec![Line::from(format!(" Remote: {}", remote.name))];
                for url in &remote.urls {
                    lines.push(Line::from(format!(" URL: {}", url)));
                }
                lines.push(Line::from(format!(
                    " Branches: {}",
                    remote.branches.len()
                )));
                for branch in &remote.branches {
                    lines.push(Line::from(format!(
                        "   {} ({})",
                        branch.name, branch.hash
                    )));
                }
                lines
            } else {
                vec![Line::from(" No remotes")]
            }
        }
        ContextId::Tags => {
            if let Some(tag) = model.tags.get(selected) {
                let mut lines = vec![
                    Line::from(format!(" Tag: {}", tag.name)),
                    Line::from(format!(" Hash: {}", tag.hash)),
                ];
                if !tag.message.is_empty() {
                    lines.push(Line::from(format!(" Message: {}", tag.message)));
                }
                lines
            } else {
                vec![Line::from(" No tags")]
            }
        }
        ContextId::Worktrees => {
            if let Some(wt) = model.worktrees.get(selected) {
                vec![
                    Line::from(format!(" Worktree: {}", wt.branch)),
                    Line::from(format!(" Path: {}", wt.path)),
                    Line::from(format!(" Hash: {}", wt.hash)),
                ]
            } else {
                vec![Line::from(" No worktrees")]
            }
        }
        _ => vec![Line::from(" lazygitrs")],
    }
}

fn render_status_bar(
    frame: &mut Frame,
    rect: Rect,
    ctx_mgr: &ContextManager,
    diff_view: &DiffViewState,
    theme: &crate::config::Theme,
) {
    let context_hints = match ctx_mgr.active() {
        ContextId::Files => "c: commit | a: stage all | <space>: toggle | d: discard",
        ContextId::Branches => "<space>: checkout | n: new | d: delete | M: merge | r: rebase",
        ContextId::Commits => "r: reword | g: reset | t: revert | C: cherry-pick | T: tag",
        ContextId::Stash => "g: pop | <space>: apply | d: drop",
        ContextId::Remotes => "f: fetch | P: push | p: pull",
        ContextId::Tags => "n: new | d: delete | P: push",
        _ => "",
    };

    let scroll_info = if !diff_view.is_empty() {
        format!(
            " | J/K: scroll diff | {{}}/}}: hunks | L{}/{}",
            diff_view.scroll_offset + 1,
            diff_view.lines.len()
        )
    } else {
        String::new()
    };

    let bar = Paragraph::new(Span::styled(
        format!(
            " {} | q: quit | tab/1-5: panels | j/k: nav{}",
            context_hints, scroll_info
        ),
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
