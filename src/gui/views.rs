use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use unicode_width::UnicodeWidthStr;

use std::collections::HashSet;

use crate::config::AppConfig;
use crate::model::Model;
use crate::model::file_tree::{CommitFileTreeNode, FileTreeNode};
use crate::pager::side_by_side::{self, DiffPanel, DiffPanelLayout, DiffViewState};

use super::ScreenMode;
use super::context::{ContextId, ContextManager, SideWindow};
use super::layout::{self, LayoutState};
use super::popup::PopupState;
use super::presentation;

pub fn render(
    frame: &mut Frame,
    model: &Model,
    ctx_mgr: &ContextManager,
    layout_state: &LayoutState,
    popup: &PopupState,
    config: &AppConfig,
    diff_view: &mut DiffViewState,
    screen_mode: ScreenMode,
    show_file_tree: bool,
    file_tree_nodes: &[FileTreeNode],
    collapsed_dirs: &HashSet<String>,
    diff_focused: bool,
    search_state: Option<(&str, usize, usize)>,
    search_textarea: Option<&tui_textarea::TextArea<'_>>,
    command_log: &[String],
    show_command_log: bool,
    commit_branch_filter: &[String],
    show_commit_file_tree: bool,
    commit_file_tree_nodes: &[CommitFileTreeNode],
    commit_files_collapsed_dirs: &HashSet<String>,
    commit_files_hash: &str,
    commit_files_message: &str,
    branch_commits_name: &str,
    remote_branches_name: &str,
    spinner_frame: usize,
    remote_op_label: Option<&str>,
    remote_op_success: bool,
) {
    let area = frame.area();
    let theme = config.user_config.theme();
    let panel_count = SideWindow::ALL.len();

    // Determine which panel index is active so it gets expanded
    let active_window = ctx_mgr.active_window();
    let active_panel_index = SideWindow::ALL
        .iter()
        .position(|w| *w == active_window)
        .unwrap_or(1); // default to Files

    let fl = layout::compute_layout(
        area,
        layout_state.side_panel_ratio,
        panel_count,
        active_panel_index,
        screen_mode,
    );

    // Full screen mode
    if screen_mode == ScreenMode::Full {
        if diff_focused {
            // Diff is focused: show diff fullscreen
            if !diff_view.is_empty() {
                side_by_side::render_diff(frame, fl.main_panel, diff_view, &theme, true);
                side_by_side::render_diff_search_highlights(frame, fl.main_panel, diff_view);
                side_by_side::render_diff_search_bar(frame, fl.main_panel, diff_view);
            } else {
                let block = Block::default()
                    .title(" Diff ")
                    .borders(Borders::ALL)
                    .border_style(theme.active_border);
                let widget = Paragraph::new(" No changes to display").block(block);
                frame.render_widget(widget, fl.main_panel);
            }
        } else {
            // Sidebar is focused: show active sidebar panel fullscreen
            let ctx_id = ctx_mgr.active();
            let selected = ctx_mgr.selected(ctx_id);
            let title = if ctx_id == ContextId::CommitFiles
                || ctx_id == ContextId::StashFiles
                || ctx_id == ContextId::BranchCommitFiles
            {
                build_commit_files_title(ctx_id, commit_files_hash, commit_files_message)
            } else if ctx_id == ContextId::BranchCommits {
                build_branch_commits_title(branch_commits_name)
            } else if ctx_id == ContextId::Commits && !commit_branch_filter.is_empty() {
                let filter_label = commit_branch_filter.join(", ");
                Line::from(vec![
                    Span::raw(" Commits "),
                    Span::styled(
                        format!("[filter: {}] ", filter_label),
                        Style::default().fg(Color::Yellow),
                    ),
                ])
            } else {
                build_window_title(ctx_mgr.active_window(), ctx_id, ctx_mgr)
            };
            let block = Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(theme.active_border);

            match ctx_id {
                ContextId::Status => {
                    render_status_main(frame, fl.main_panel, model, config, &theme);
                }
                ContextId::Files => {
                    if show_file_tree {
                        let items = presentation::files::render_file_tree(
                            model,
                            &theme,
                            file_tree_nodes,
                            collapsed_dirs,
                        );
                        render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                    } else {
                        let items = presentation::files::render_file_list(model, &theme);
                        render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                    }
                }
                ContextId::Branches => {
                    let items = presentation::branches::render_branch_list(model, &theme, remote_op_label, spinner_frame, remote_op_success);
                    render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                }
                ContextId::Remotes | ContextId::RemoteBranches => {
                    if ctx_mgr.active() == ContextId::RemoteBranches {
                        let rb_selected = ctx_mgr.selected(ContextId::RemoteBranches);
                        let rb_block = Block::default()
                            .title(format!(" Remote Branches ({}) ", remote_branches_name))
                            .borders(Borders::ALL)
                            .border_style(theme.active_border);
                        let items = presentation::remote_branches::render_remote_branch_list(&model.sub_remote_branches, &theme);
                        render_list(frame, fl.main_panel, rb_block, items, rb_selected, true, &theme);
                    } else {
                        let items = presentation::remotes::render_remote_list(model, &theme);
                        render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                    }
                }
                ContextId::Tags => {
                    let items = presentation::tags::render_tag_list(model, &theme);
                    render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                }
                ContextId::Commits => {
                    let items = presentation::commits::render_commit_list(model, &theme);
                    render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                }
                ContextId::Stash => {
                    let items = presentation::stash::render_stash_list(model, &theme);
                    render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                }
                ContextId::BranchCommits => {
                    let items = presentation::commits::render_sub_commit_list(model, &theme);
                    render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                }
                ContextId::CommitFiles | ContextId::StashFiles | ContextId::BranchCommitFiles => {
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            &theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, &theme);
                        render_list(frame, fl.main_panel, block, items, selected, true, &theme);
                    }
                }
                _ => {
                    let widget = Paragraph::new("").block(block);
                    frame.render_widget(widget, fl.main_panel);
                }
            }
        }
        render_status_bar(frame, fl.status_bar, ctx_mgr, diff_view, &theme, model);
        // Render text selection highlight overlay and tooltip (must be before popup)
        render_selection_overlay(frame, diff_view, fl.main_panel);
        if *popup != PopupState::None {
            render_popup(frame, popup, area, spinner_frame);
        }
        render_command_log(frame, &fl, command_log, show_command_log);
        return;
    }

    // Render sidebar panels — one per window
    for (i, window) in SideWindow::ALL.iter().enumerate() {
        if i >= fl.side_panels.len() {
            break;
        }
        let rect = fl.side_panels[i];
        let ctx_id = ctx_mgr.active_context_for_window(*window);
        let is_active = ctx_mgr.active_window() == *window;
        let selected = ctx_mgr.selected(ctx_id);

        let border_style = if is_active && !diff_focused {
            theme.active_border
        } else {
            theme.inactive_border
        };

        // Build title with tab indicators for multi-tab windows
        let title = if *window == SideWindow::Commits
            && ctx_id == ContextId::Commits
            && !commit_branch_filter.is_empty()
        {
            let filter_label = commit_branch_filter.join(", ");
            Line::from(vec![
                Span::raw(" Commits "),
                Span::styled(
                    format!("[filter: {}] ", filter_label),
                    Style::default().fg(Color::Yellow),
                ),
            ])
        } else {
            build_window_title(*window, ctx_id, ctx_mgr)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        match ctx_id {
            ContextId::Status => {
                let inner_width = rect.width.saturating_sub(2) as usize;
                let status_line = render_status_sidebar(model, config, inner_width);
                let widget = Paragraph::new(status_line).block(block);
                frame.render_widget(widget, rect);
            }
            ContextId::Files => {
                if show_file_tree {
                    let items = presentation::files::render_file_tree(
                        model,
                        &theme,
                        file_tree_nodes,
                        collapsed_dirs,
                    );
                    render_list(frame, rect, block, items, selected, is_active, &theme);
                } else {
                    let items = presentation::files::render_file_list(model, &theme);
                    render_list(frame, rect, block, items, selected, is_active, &theme);
                }
            }
            ContextId::Worktrees => {
                let items = render_worktree_list(model);
                render_list(frame, rect, block, items, selected, is_active, &theme);
            }
            ContextId::Submodules => {
                if model.submodules.is_empty() {
                    let widget = Paragraph::new(" (no submodules)").block(block);
                    frame.render_widget(widget, rect);
                } else {
                    let items: Vec<ListItem> = model.submodules.iter().map(|sub| {
                        let line = Line::from(vec![
                            Span::styled(
                                format!("  {} ", sub.name),
                                Style::default().fg(Color::Cyan),
                            ),
                            Span::styled(
                                sub.path.clone(),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]);
                        ListItem::new(line)
                    }).collect();
                    render_list(frame, rect, block, items, selected, is_active, &theme);
                }
            }
            ContextId::Branches => {
                // If BranchCommits or BranchCommitFiles is active, render that instead
                if ctx_mgr.active() == ContextId::BranchCommitFiles {
                    let cf_selected = ctx_mgr.selected(ContextId::BranchCommitFiles);
                    let cf_title = build_commit_files_title(
                        ContextId::BranchCommitFiles,
                        commit_files_hash,
                        commit_files_message,
                    );
                    let cf_block = Block::default()
                        .title(cf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            &theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list(frame, rect, cf_block, items, cf_selected, is_active, &theme);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, &theme);
                        render_list(frame, rect, cf_block, items, cf_selected, is_active, &theme);
                    }
                } else if ctx_mgr.active() == ContextId::BranchCommits {
                    let bc_selected = ctx_mgr.selected(ContextId::BranchCommits);
                    let bc_title = build_branch_commits_title(branch_commits_name);
                    let bc_block = Block::default()
                        .title(bc_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    let items = presentation::commits::render_sub_commit_list(model, &theme);
                    render_list(frame, rect, bc_block, items, bc_selected, is_active, &theme);
                } else {
                    let items = presentation::branches::render_branch_list(model, &theme, remote_op_label, spinner_frame, remote_op_success);
                    render_list(frame, rect, block, items, selected, is_active, &theme);
                }
            }
            ContextId::Remotes => {
                if ctx_mgr.active() == ContextId::RemoteBranches {
                    let rb_selected = ctx_mgr.selected(ContextId::RemoteBranches);
                    let rb_title = format!(" Remote Branches ({}) ", remote_branches_name);
                    let rb_block = Block::default()
                        .title(rb_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    let items = presentation::remote_branches::render_remote_branch_list(&model.sub_remote_branches, &theme);
                    render_list(frame, rect, rb_block, items, rb_selected, is_active, &theme);
                } else {
                    let items = presentation::remotes::render_remote_list(model, &theme);
                    render_list(frame, rect, block, items, selected, is_active, &theme);
                }
            }
            ContextId::Tags => {
                // If BranchCommits or BranchCommitFiles is active (drill-down from Tags), render that instead
                if ctx_mgr.active() == ContextId::BranchCommitFiles {
                    let cf_selected = ctx_mgr.selected(ContextId::BranchCommitFiles);
                    let cf_title = build_commit_files_title(
                        ContextId::BranchCommitFiles,
                        commit_files_hash,
                        commit_files_message,
                    );
                    let cf_block = Block::default()
                        .title(cf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            &theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list(frame, rect, cf_block, items, cf_selected, is_active, &theme);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, &theme);
                        render_list(frame, rect, cf_block, items, cf_selected, is_active, &theme);
                    }
                } else if ctx_mgr.active() == ContextId::BranchCommits {
                    let bc_selected = ctx_mgr.selected(ContextId::BranchCommits);
                    let bc_title = build_branch_commits_title(branch_commits_name);
                    let bc_block = Block::default()
                        .title(bc_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    let items = presentation::commits::render_sub_commit_list(model, &theme);
                    render_list(frame, rect, bc_block, items, bc_selected, is_active, &theme);
                } else {
                    let items = presentation::tags::render_tag_list(model, &theme);
                    render_list(frame, rect, block, items, selected, is_active, &theme);
                }
            }
            ContextId::Commits => {
                // If CommitFiles is active within this window, render that instead
                if ctx_mgr.active() == ContextId::CommitFiles {
                    let cf_selected = ctx_mgr.selected(ContextId::CommitFiles);
                    let cf_title = build_commit_files_title(
                        ContextId::CommitFiles,
                        commit_files_hash,
                        commit_files_message,
                    );
                    let cf_block = Block::default()
                        .title(cf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            &theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list(frame, rect, cf_block, items, cf_selected, is_active, &theme);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, &theme);
                        render_list(frame, rect, cf_block, items, cf_selected, is_active, &theme);
                    }
                } else {
                    let items = presentation::commits::render_commit_list(model, &theme);
                    render_list(frame, rect, block, items, selected, is_active, &theme);
                }
            }
            ContextId::Reflog => {
                // If CommitFiles is active (drill-down from Reflog), render that instead
                if ctx_mgr.active() == ContextId::CommitFiles {
                    let cf_selected = ctx_mgr.selected(ContextId::CommitFiles);
                    let cf_title = build_commit_files_title(
                        ContextId::CommitFiles,
                        commit_files_hash,
                        commit_files_message,
                    );
                    let cf_block = Block::default()
                        .title(cf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            &theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list(frame, rect, cf_block, items, cf_selected, is_active, &theme);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, &theme);
                        render_list(frame, rect, cf_block, items, cf_selected, is_active, &theme);
                    }
                } else {
                    let items = presentation::reflog::render_reflog_list(model, &theme);
                    render_list(frame, rect, block, items, selected, is_active, &theme);
                }
            }
            ContextId::Stash => {
                // If StashFiles is active within this window, render that instead
                if ctx_mgr.active() == ContextId::StashFiles {
                    let sf_selected = ctx_mgr.selected(ContextId::StashFiles);
                    let sf_title = build_commit_files_title(
                        ContextId::StashFiles,
                        commit_files_hash,
                        commit_files_message,
                    );
                    let sf_block = Block::default()
                        .title(sf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            &theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list(frame, rect, sf_block, items, sf_selected, is_active, &theme);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, &theme);
                        render_list(frame, rect, sf_block, items, sf_selected, is_active, &theme);
                    }
                } else {
                    let items = presentation::stash::render_stash_list(model, &theme);
                    render_list(frame, rect, block, items, selected, is_active, &theme);
                }
            }
            _ => {
                let widget = Paragraph::new("").block(block);
                frame.render_widget(widget, rect);
            }
        }
    }

    // Render main panel
    if ctx_mgr.active() == ContextId::Status {
        // Status view: show logo + copyright in the main content area
        render_status_main(frame, fl.main_panel, model, config, &theme);
    } else if !diff_view.is_empty() {
        side_by_side::render_diff(frame, fl.main_panel, diff_view, &theme, diff_focused);
        side_by_side::render_diff_search_highlights(frame, fl.main_panel, diff_view);
        side_by_side::render_diff_search_bar(frame, fl.main_panel, diff_view);
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

    // Render status bar (or search bar if search is active)
    if let Some((query, match_count, current_match)) = search_state {
        let match_info = if match_count > 0 {
            format!(" {}/{}", current_match + 1, match_count)
        } else if !query.is_empty() {
            " (no matches)".to_string()
        } else {
            String::new()
        };

        if let Some(ta) = search_textarea {
            // Render: "/" prefix + textarea + match info
            // Split the status bar into three parts
            let prefix_width = 2u16; // " /"
            let suffix_text = match_info;
            let suffix_width = suffix_text.len() as u16;
            let ta_width = fl
                .status_bar
                .width
                .saturating_sub(prefix_width + suffix_width);

            // Prefix " /"
            let prefix_rect = Rect::new(fl.status_bar.x, fl.status_bar.y, prefix_width, 1);
            let prefix = Paragraph::new(Span::styled(" /", Style::default().fg(Color::Yellow)));
            frame.render_widget(prefix, prefix_rect);

            // Textarea
            let ta_rect = Rect::new(fl.status_bar.x + prefix_width, fl.status_bar.y, ta_width, 1);
            frame.render_widget(ta, ta_rect);

            // Suffix (match info)
            if !suffix_text.is_empty() {
                let suffix_rect = Rect::new(
                    fl.status_bar.x + prefix_width + ta_width,
                    fl.status_bar.y,
                    suffix_width,
                    1,
                );
                let suffix = Paragraph::new(Span::styled(
                    suffix_text,
                    Style::default().fg(Color::Yellow),
                ));
                frame.render_widget(suffix, suffix_rect);
            }
        } else {
            let bar = Paragraph::new(Span::styled(
                format!(" /{}{}", query, match_info),
                Style::default().fg(Color::Yellow),
            ));
            frame.render_widget(bar, fl.status_bar);
        }
    } else {
        render_status_bar(frame, fl.status_bar, ctx_mgr, diff_view, &theme, model);
    }

    // Render text selection highlight overlay and tooltip
    render_selection_overlay(frame, diff_view, fl.main_panel);

    // Render popup overlay
    if *popup != PopupState::None {
        render_popup(frame, popup, area, spinner_frame);
    }

    // Render command log last so it appears above everything
    render_command_log(frame, &fl, command_log, show_command_log);
}

fn render_command_log(
    frame: &mut Frame,
    fl: &layout::FrameLayout,
    command_log: &[String],
    show_command_log: bool,
) {
    if !show_command_log || command_log.is_empty() {
        return;
    }

    let log_height = command_log.len().min(5) as u16;
    let log_width = fl.main_panel.width.min(50);
    let log_x = fl.main_panel.x + fl.main_panel.width - log_width;
    let log_y = fl.main_panel.y + fl.main_panel.height - log_height - 1;
    let log_rect = Rect::new(log_x, log_y, log_width, log_height + 2);

    let border_color = Color::Rgb(80, 80, 80);
    let title_color = Color::Rgb(140, 140, 140);
    let hint_color = Color::Rgb(90, 90, 90);
    let log_block = Block::default()
        .title(Line::from(vec![
            Span::styled(" ", Style::default().fg(title_color)),
            Span::styled(
                "Command Log",
                Style::default()
                    .fg(title_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default().fg(title_color)),
        ]))
        .title_bottom(
            Line::from(vec![
                Span::styled(" ", Style::default().fg(hint_color)),
                Span::styled(
                    ";",
                    Style::default()
                        .fg(Color::Rgb(160, 160, 160))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" toggle ", Style::default().fg(hint_color)),
            ])
            .alignment(ratatui::layout::Alignment::Right),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let cmd_color = Color::Rgb(100, 100, 100);
    let visible_count = command_log.len().min(log_height as usize);
    let log_lines: Vec<Line> = command_log
        .iter()
        .rev()
        .take(log_height as usize)
        .rev()
        .enumerate()
        .map(|(i, s)| {
            let is_latest = i == visible_count - 1;
            let fg = if is_latest {
                Color::Rgb(160, 160, 160)
            } else {
                cmd_color
            };
            Line::from(vec![
                Span::styled(" $ ", Style::default().fg(Color::Rgb(80, 130, 80))),
                Span::styled(s.to_string(), Style::default().fg(fg)),
            ])
        })
        .collect();

    frame.render_widget(Clear, log_rect);
    let log_widget = Paragraph::new(log_lines).block(log_block);
    frame.render_widget(log_widget, log_rect);
}

/// Build a window title like " 4 Commit Files (abc1234 feat: some change) ".
fn build_branch_commits_title<'a>(branch_name: &str) -> Line<'a> {
    Line::from(vec![
        Span::raw(" 3 Commits "),
        Span::styled(
            format!("({})", branch_name),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(" "),
    ])
}

fn build_commit_files_title<'a>(
    ctx: ContextId,
    commit_hash: &str,
    commit_message: &str,
) -> Line<'a> {
    let short = if commit_hash.len() > 7 {
        &commit_hash[..7]
    } else {
        commit_hash
    };
    let prefix = match ctx {
        ContextId::StashFiles => " 5 Stash Files ",
        ContextId::BranchCommitFiles => " 3 Commit Files ",
        _ => " 4 Commit Files ",
    };
    let mut spans = vec![
        Span::raw(prefix),
        Span::styled(format!("({}", short), Style::default().fg(Color::Yellow)),
    ];
    if !commit_message.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            commit_message.to_string(),
            Style::default().fg(Color::DarkGray),
        ));
    }
    spans.push(Span::styled(") ", Style::default().fg(Color::Yellow)));
    Line::from(spans)
}

fn build_window_title<'a>(
    window: SideWindow,
    active_ctx: ContextId,
    _ctx_mgr: &ContextManager,
) -> Line<'a> {
    let tabs = window.tabs();
    let key = window.key_label();

    if tabs.len() == 1 {
        return Line::from(format!(" {} {} ", key, tabs[0].title()));
    }

    let mut spans = vec![Span::raw(format!(" {} ", key))];

    for (i, ctx) in tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        }
        if *ctx == active_ctx {
            spans.push(Span::styled(
                ctx.title(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                ctx.title(),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// Compact 1-line status for the sidebar: "reponame → branch          +N -N"
fn render_status_sidebar<'a>(model: &Model, _config: &AppConfig, inner_width: usize) -> Line<'a> {
    // Determine the working-tree state prefix (rebasing/merging/cherry-picking)
    let state_prefix = if model.is_rebasing {
        Some("rebasing")
    } else if model.is_merging {
        Some("merging")
    } else if model.is_cherry_picking {
        Some("cherry-picking")
    } else {
        None
    };

    let head_branch = model.branches.iter().find(|b| b.head);
    let branch_name = head_branch
        .map(|b| b.name.clone())
        .unwrap_or_else(|| {
            if model.head_branch_name.is_empty() {
                "HEAD (no branch)".to_string()
            } else {
                model.head_branch_name.clone()
            }
        });
    let ahead_behind = head_branch.and_then(|b| b.ahead_behind());

    let repo_name = model.repo_name.clone();

    // Build the right-side stats string to measure its width
    let additions = model.total_additions;
    let deletions = model.total_deletions;
    let has_changes = additions > 0 || deletions > 0;

    let stats_text = if has_changes {
        let mut s = String::new();
        if additions > 0 {
            s.push_str(&format!("+{}", additions));
        }
        if additions > 0 && deletions > 0 {
            s.push(' ');
        }
        if deletions > 0 {
            s.push_str(&format!("-{}", deletions));
        }
        s
    } else {
        String::new()
    };

    // Build ahead/behind prefix
    let ab_text = match ahead_behind {
        Some((ahead, behind)) if ahead > 0 && behind > 0 => format!("↑{}↓{} ", ahead, behind),
        Some((ahead, _)) if ahead > 0 => format!("↑{} ", ahead),
        Some((_, behind)) if behind > 0 => format!("↓{} ", behind),
        _ => String::new(),
    };

    let mut spans = Vec::new();

    // When in a special state (rebasing, merging, etc.), show lazygit-style:
    //   (rebasing) reponame → <hash>
    if let Some(state) = state_prefix {
        let right_side = if model.is_rebasing && !model.rebase_onto_hash.is_empty() {
            model.rebase_onto_hash.clone()
        } else {
            branch_name.clone()
        };

        let prefix = format!("({})", state);
        let left_len = 1 + prefix.len() + 1 + repo_name.len() + 1
            + UnicodeWidthStr::width("→ ") + right_side.len();
        let right_len = if has_changes { stats_text.len() + 1 } else { 0 };
        let padding = inner_width.saturating_sub(left_len + right_len);

        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            prefix,
            Style::default().fg(Color::Yellow),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("{} ", repo_name),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled("→ ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            right_side,
            Style::default().fg(Color::Yellow),
        ));

        if has_changes {
            spans.push(Span::raw(" ".repeat(padding)));
            if additions > 0 {
                spans.push(Span::styled(
                    format!("+{}", additions),
                    Style::default().fg(Color::Green),
                ));
            }
            if additions > 0 && deletions > 0 {
                spans.push(Span::raw(" "));
            }
            if deletions > 0 {
                spans.push(Span::styled(
                    format!("-{}", deletions),
                    Style::default().fg(Color::Red),
                ));
            }
            spans.push(Span::raw(" "));
        }

        return Line::from(spans);
    }

    // Normal state: " ↑N reponame → branch"
    let left_len = 1
        + UnicodeWidthStr::width(ab_text.as_str())
        + repo_name.len()
        + 1
        + UnicodeWidthStr::width("→ ")
        + branch_name.len();
    let right_len = if has_changes { stats_text.len() + 1 } else { 0 };
    let padding = inner_width.saturating_sub(left_len + right_len);

    if !ab_text.is_empty() {
        spans.push(Span::styled(
            format!(" {}", ab_text),
            Style::default().fg(Color::Green),
        ));
    } else {
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!("{} ", repo_name),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled("→ ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(branch_name, Style::default().fg(Color::Green)));

    if has_changes {
        spans.push(Span::raw(" ".repeat(padding)));
        if additions > 0 {
            spans.push(Span::styled(
                format!("+{}", additions),
                Style::default().fg(Color::Green),
            ));
        }
        if additions > 0 && deletions > 0 {
            spans.push(Span::raw(" "));
        }
        if deletions > 0 {
            spans.push(Span::styled(
                format!("-{}", deletions),
                Style::default().fg(Color::Red),
            ));
        }
        spans.push(Span::raw(" "));
    }

    Line::from(spans)
}

/// Full status view for the main content area: logo + copyright + repo info
fn render_status_main(
    frame: &mut Frame,
    rect: Rect,
    model: &Model,
    _config: &AppConfig,
    theme: &crate::config::Theme,
) {
    let block = Block::default()
        .title(" Status ")
        .borders(Borders::ALL)
        .border_style(theme.active_border);

    let branch_name = model
        .branches
        .iter()
        .find(|b| b.head)
        .map(|b| b.name.as_str())
        .unwrap_or_else(|| {
            if model.head_branch_name.is_empty() {
                "HEAD (no branch)"
            } else {
                model.head_branch_name.as_str()
            }
        });

    let logo = include_str!("../../logo.txt");
    let mut lines: Vec<Line> = logo
        .lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(Color::Cyan),
            ))
        })
        .collect();

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Copyright 2026 Carlo Taleon (Blankeos)",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(format!(" Branch: {}", branch_name)));
    lines.push(Line::from(format!(" Commits: {}", model.commits.len())));
    lines.push(Line::from(format!(" Files: {}", model.files.len())));

    // In-progress operation banners
    if model.is_rebasing {
        lines.push(Line::from(Span::styled(
            " REBASING",
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

    let widget = Paragraph::new(lines).block(block);
    frame.render_widget(widget, rect);
}

fn render_worktree_list<'a>(model: &Model) -> Vec<ListItem<'a>> {
    model
        .worktrees
        .iter()
        .map(|wt| {
            let marker = if wt.is_current { "* " } else { "  " };
            let line = Line::from(vec![
                Span::styled(marker.to_string(), Style::default().fg(Color::Green)),
                Span::styled(wt.branch.clone(), Style::default().fg(Color::Cyan)),
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
    is_active: bool,
    theme: &crate::config::Theme,
) {
    if items.is_empty() {
        frame.render_widget(block, rect);
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
            if is_active && i + offset == selected {
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
                lines.push(Line::from(format!(" Branches: {}", remote.branches.len())));
                for branch in &remote.branches {
                    lines.push(Line::from(format!("   {} ({})", branch.name, branch.hash)));
                }
                lines
            } else {
                vec![Line::from(" No remotes")]
            }
        }
        ContextId::RemoteBranches => {
            if let Some(rb) = model.sub_remote_branches.get(selected) {
                vec![
                    Line::from(format!(" Branch: {}/{}", rb.remote_name, rb.name)),
                    Line::from(format!(" Hash: {}", rb.hash)),
                ]
            } else {
                vec![Line::from(" No remote branches")]
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
        ContextId::Submodules => {
            if let Some(sub) = model.submodules.get(selected) {
                vec![
                    Line::from(format!(" Submodule: {}", sub.name)),
                    Line::from(format!(" Path: {}", sub.path)),
                ]
            } else {
                vec![Line::from(" No submodules")]
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
    _theme: &crate::config::Theme,
    model: &Model,
) {
    let mut hints: Vec<(&str, &str)> = Vec::new();

    // When in a special state (rebasing/merging/cherry-picking), show those options prominently
    if model.is_rebasing {
        hints.push(("m", "continue/abort/skip rebase"));
    } else if model.is_merging {
        hints.push(("m", "continue/abort merge"));
    } else if model.is_cherry_picking {
        hints.push(("m", "continue/abort cherry-pick"));
    }

    // Context-specific hints
    match ctx_mgr.active() {
        ContextId::Files => {
            hints.extend([("c", "commit"), ("a", "stage all"), ("space", "toggle"), ("d", "discard"), ("e", "edit"), ("o", "open")]);
        }
        ContextId::Branches => {
            hints.extend([("space", "checkout"), ("n", "new"), ("d", "delete"), ("M", "merge"), ("r", "rebase")]);
        }
        ContextId::Commits => {
            hints.extend([("r", "reword"), ("g", "reset"), ("t", "revert"), ("C", "cherry-pick"), ("ctrl+l", "filter branch")]);
        }
        ContextId::Stash => {
            hints.extend([("g", "pop"), ("space", "apply"), ("d", "drop")]);
        }
        ContextId::Remotes => {
            hints.extend([("enter", "branches"), ("f", "fetch"), ("P", "push"), ("p", "pull")]);
        }
        ContextId::RemoteBranches => {
            hints.extend([("space", "checkout"), ("M", "merge"), ("r", "rebase"), ("d", "delete")]);
        }
        ContextId::Tags => {
            hints.extend([("n", "new"), ("d", "delete"), ("P", "push")]);
        }
        ContextId::Worktrees => {
            hints.extend([("space", "switch"), ("n", "new"), ("d", "remove")]);
        }
        ContextId::Submodules => {
            hints.extend([("space", "update"), ("a", "add"), ("d", "remove"), ("e", "enter")]);
        }
        _ => {}
    }

    // Global hints
    hints.extend([("q", "quit"), ("tab/1-5", "panels"), ("j/k", "nav")]);

    // Diff scroll info
    if !diff_view.is_empty() {
        hints.extend([("J/K", "scroll diff"), ("{/}", "hunks")]);
    }

    let key_style = Style::default().fg(Color::Gray).add_modifier(ratatui::style::Modifier::BOLD);
    let desc_style = Style::default().fg(Color::DarkGray);
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
    frame.render_widget(bar, rect);
}

/// Render mouse text selection highlight overlay and copy tooltip on the diff view.
/// `panel_rect` is the main diff panel Rect — selection is rendered only within the selected side.
pub fn render_selection_overlay(frame: &mut Frame, diff_view: &mut DiffViewState, panel_rect: Rect) {
    use crate::pager::ChangeType;

    let selection = match &diff_view.selection {
        Some(sel) => sel.clone(),
        None => return,
    };

    let (top_row, top_col, bot_row, bot_col) = selection.normalized();
    let is_click = selection.is_click;

    // For non-click selections, bail on empty (single point).
    if !is_click && top_row == bot_row && top_col == bot_col {
        return;
    }

    // Use the same centralized layout that render_diff and the mouse handler use.
    let pl = DiffPanelLayout::compute(panel_rect, diff_view);
    let (content_start, content_end) = pl.content_range(selection.panel);

    // Compute the actual file line number at the top of the selection/click for editAtLine.
    let edit_line_number: Option<usize> = if top_row >= pl.inner_y {
        let line_idx = diff_view.scroll_offset + (top_row - pl.inner_y) as usize;
        diff_view.file_line_number(line_idx, selection.panel)
    } else {
        None
    };
    // Compute the file column number (1-based) from the terminal click position.
    let edit_column_number: Option<usize> = if top_col >= content_start {
        Some((top_col - content_start) as usize + diff_view.horizontal_scroll + 1)
    } else {
        Some(1)
    };
    if let Some(ref mut sel) = diff_view.selection {
        sel.edit_line_number = edit_line_number;
        sel.edit_column_number = edit_column_number;
    }

    let buf = frame.buffer_mut();
    let buf_area = *buf.area();

    // --- Click state: highlight the clicked cell and show "e edit" tooltip ---
    if is_click {
        // Highlight the single clicked cell
        if top_row >= pl.inner_y && top_row < pl.inner_end_y
            && top_col >= content_start && top_col < content_end
            && top_row < buf_area.y + buf_area.height
        {
            let highlight_style = Style::default()
                .bg(Color::Rgb(100, 140, 200))
                .fg(Color::Rgb(20, 20, 30));
            if let Some(cell) = buf.cell_mut((top_col, top_row)) {
                cell.set_style(highlight_style);
            }
        }

        if diff_view.file_exists_on_disk {
            let tooltip_style = Style::default()
                .bg(Color::Rgb(60, 60, 70))
                .fg(Color::Rgb(200, 200, 210));
            let key_style = Style::default()
                .bg(Color::Rgb(60, 60, 70))
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD);

            let parts: &[(&str, Style)] = &[
                (" ", tooltip_style),
                ("e", key_style),
                (" edit ", tooltip_style),
            ];
            let tooltip_width: u16 = parts.iter().map(|(s, _)| s.len() as u16).sum();
            let tooltip_x = top_col
                .saturating_sub(tooltip_width / 2)
                .max(content_start)
                .min(content_end.saturating_sub(tooltip_width));
            let tooltip_y = (top_row + 1).min(pl.inner_end_y.saturating_sub(1));

            if tooltip_y < buf_area.y + buf_area.height {
                let mut col = tooltip_x;
                for (text, style) in parts {
                    for ch in text.chars() {
                        if col >= content_end {
                            break;
                        }
                        if let Some(cell) = buf.cell_mut((col, tooltip_y)) {
                            cell.set_char(ch);
                            cell.set_style(*style);
                        }
                        col += 1;
                    }
                }
            }
        }
        return;
    }

    // --- Drag selection: highlight text and show tooltip ---
    let mut extracted_text = String::new();

    let row_start = top_row.max(pl.inner_y);
    let row_end = bot_row.min(pl.inner_end_y.saturating_sub(1));

    let highlight_style = Style::default()
        .bg(Color::Rgb(100, 140, 200))
        .fg(Color::Rgb(20, 20, 30));

    for (i, row) in (row_start..=row_end).enumerate() {
        if row >= buf_area.y + buf_area.height {
            break;
        }

        // Map terminal row to diff line index.
        let line_idx = diff_view.scroll_offset + (row - pl.inner_y) as usize;
        if let Some(diff_line) = diff_view.lines.get(line_idx) {
            // Skip file header separator lines.
            if diff_line.file_header.is_some() {
                continue;
            }
            // Skip slash-fill rows (the empty side for Insert/Delete lines).
            let is_slash_fill = match selection.panel {
                DiffPanel::Old => diff_line.change_type == ChangeType::Insert,
                DiffPanel::New => diff_line.change_type == ChangeType::Delete,
            };
            if is_slash_fill {
                continue;
            }
        }

        // Column range: intersection of mouse selection cols with panel content cols.
        let sel_col_start = if row == top_row { top_col } else { 0 };
        let sel_col_end = if row == bot_row { bot_col } else { u16::MAX };
        let hl_start = sel_col_start.max(content_start);
        let hl_end = sel_col_end.min(content_end);

        if hl_start >= hl_end {
            continue;
        }

        let mut row_text = String::new();
        for col in hl_start..hl_end {
            if col >= buf_area.x + buf_area.width {
                break;
            }
            if let Some(cell) = buf.cell_mut((col, row)) {
                row_text.push_str(cell.symbol());
                cell.set_style(highlight_style);
            }
        }

        let trimmed = row_text.trim_end();
        if !trimmed.is_empty() {
            if !extracted_text.is_empty() {
                extracted_text.push('\n');
            }
            extracted_text.push_str(trimmed);
        } else if i > 0 && i < (row_end - row_start) as usize {
            // Preserve blank lines in the middle of the selection.
            extracted_text.push('\n');
        }
    }

    // Store extracted text for the copy action.
    if let Some(ref mut sel) = diff_view.selection {
        sel.text = extracted_text;
    }

    // Tooltip below the selection (only after drag finishes).
    if !selection.dragging {
        let tooltip_style = Style::default()
            .bg(Color::Rgb(60, 60, 70))
            .fg(Color::Rgb(200, 200, 210));
        let key_style = Style::default()
            .bg(Color::Rgb(60, 60, 70))
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);

        // Build parts conditionally: include "e edit" only if file is on disk.
        let mut parts: Vec<(&str, Style)> = Vec::new();
        if diff_view.file_exists_on_disk {
            parts.push((" ", tooltip_style));
            parts.push(("e", key_style));
            parts.push((" edit  ", tooltip_style));
        } else {
            parts.push((" ", tooltip_style));
        }
        parts.push(("y", key_style));
        parts.push((" copy  ", tooltip_style));
        parts.push(("esc", key_style));
        parts.push((" ", tooltip_style));

        let tooltip_width: u16 = parts.iter().map(|(s, _)| s.len() as u16).sum();
        let tooltip_x = bot_col
            .saturating_sub(tooltip_width / 2)
            .max(content_start)
            .min(content_end.saturating_sub(tooltip_width));
        let tooltip_y = (bot_row + 1).min(pl.inner_end_y.saturating_sub(1));

        if tooltip_y < buf_area.y + buf_area.height {
            let mut col = tooltip_x;
            for (text, style) in &parts {
                for ch in text.chars() {
                    if col >= content_end {
                        break;
                    }
                    if let Some(cell) = buf.cell_mut((col, tooltip_y)) {
                        cell.set_char(ch);
                        cell.set_style(*style);
                    }
                    col += 1;
                }
            }
        }
    }
}

pub fn render_popup(frame: &mut Frame, popup: &PopupState, area: Rect, spinner_frame: usize) {
    let popup_width = (area.width * 60 / 100).min(60).max(30);
    let x = (area.width.saturating_sub(popup_width)) / 2;

    match popup {
        PopupState::Confirm { title, message, .. } => {
            let inner_width = popup_width.saturating_sub(4) as usize; // borders + padding
            let wrapped: Vec<std::borrow::Cow<'_, str>> = message
                .lines()
                .flat_map(|line| {
                    if line.is_empty() {
                        vec![std::borrow::Cow::Borrowed("")]
                    } else {
                        textwrap::wrap(line, inner_width)
                    }
                })
                .collect();
            let confirm_height = (wrapped.len() as u16) + 5; // border*2 + blank line + blank line + [y]es/[n]o
            let cy = (area.height.saturating_sub(confirm_height)) / 2;
            let popup_rect = Rect::new(x, cy, popup_width, confirm_height);
            frame.render_widget(Clear, popup_rect);
            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));

            let mut text: Vec<Line> = Vec::new();
            text.push(Line::from(""));
            for line in &wrapped {
                text.push(Line::from(format!(" {}", line)));
            }
            text.push(Line::from(""));
            text.push(Line::from(Span::styled(
                " [y]es / [n]o",
                Style::default().fg(Color::Yellow),
            )));

            let widget = Paragraph::new(text).block(block);
            frame.render_widget(widget, popup_rect);
        }
        PopupState::Message { title, message, kind } => {
            let is_error = *kind == crate::gui::popup::MessageKind::Error;
            let icon = if is_error { "⚠ " } else { "" };
            let inner_width = popup_width.saturating_sub(4) as usize; // borders + padding
            let wrapped: Vec<std::borrow::Cow<'_, str>> = message
                .lines()
                .flat_map(|line| {
                    if line.is_empty() {
                        vec![std::borrow::Cow::Borrowed("")]
                    } else {
                        textwrap::wrap(line, inner_width)
                    }
                })
                .collect();
            let msg_height = (wrapped.len() as u16) + 4; // border*2 + blank line + dismiss line
            let cy = (area.height.saturating_sub(msg_height)) / 2;
            let popup_rect = Rect::new(x, cy, popup_width, msg_height);
            frame.render_widget(Clear, popup_rect);
            let border_color = if is_error { Color::Red } else { Color::Yellow };
            let block = Block::default()
                .title(format!(" {}{} ", icon, title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color));

            let mut text: Vec<Line> = Vec::new();
            text.push(Line::from(""));
            for line in &wrapped {
                text.push(Line::from(format!(" {}", line)));
            }
            text.push(Line::from(Span::styled(
                " Press any key to dismiss",
                Style::default().fg(Color::DarkGray),
            )));

            let widget = Paragraph::new(text).block(block);
            frame.render_widget(widget, popup_rect);
        }
        PopupState::Input {
            title,
            textarea,
            is_commit,
            confirm_focused,
            ..
        } => {
            // Textarea popup: taller to allow multiline editing
            // Add extra row for commit dialogs to fit the confirm button row
            let ta_height = if *is_commit { 14u16 } else { 12u16 };
            let ta_y = (area.height.saturating_sub(ta_height)) / 2;
            let ta_rect = Rect::new(x, ta_y, popup_width, ta_height);
            frame.render_widget(Clear, ta_rect);

            // Render a container block with title and hint
            let outer = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));
            frame.render_widget(outer, ta_rect);

            // Inner area for textarea + hint
            let inner = ta_rect.inner(ratatui::layout::Margin {
                horizontal: 1,
                vertical: 1,
            });

            if *is_commit {
                // Reserve 2 lines: one for hint, one for confirm button row
                if inner.height > 4 {
                    let ta_area = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
                    frame.render_widget(textarea, ta_area);

                    // Hint line (opencode-style: bold key, dim description)
                    let hint_area = Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 1);
                    let key_style = Style::default().fg(Color::Gray).add_modifier(ratatui::style::Modifier::BOLD);
                    let desc_style = Style::default().fg(Color::DarkGray);
                    let hint_line = Line::from(vec![
                        Span::styled(" ctrl+s ", key_style),
                        Span::styled("confirm  ", desc_style),
                        Span::styled("ctrl+o ", key_style),
                        Span::styled("menu  ", desc_style),
                        Span::styled("esc ", key_style),
                        Span::styled("cancel", desc_style),
                    ]);
                    frame.render_widget(Paragraph::new(hint_line), hint_area);

                    // Confirm button row (right-aligned)
                    let btn_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
                    let (btn_style, btn_text) = if *confirm_focused {
                        (
                            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(ratatui::style::Modifier::BOLD),
                            " Confirm ",
                        )
                    } else {
                        (
                            Style::default().fg(Color::Green),
                            " Confirm ",
                        )
                    };
                    let btn_width = btn_text.len() as u16;
                    let btn_x = btn_area.x + btn_area.width.saturating_sub(btn_width);
                    let btn_rect = Rect::new(btn_x, btn_area.y, btn_width, 1);
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(btn_text, btn_style))),
                        btn_rect,
                    );
                } else {
                    frame.render_widget(textarea, inner);
                }
            } else {
                // Non-commit: no button, just hint
                if inner.height > 2 {
                    let ta_area = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
                    frame.render_widget(textarea, ta_area);

                    let hint_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
                    let key_style = Style::default().fg(Color::Gray).add_modifier(ratatui::style::Modifier::BOLD);
                    let desc_style = Style::default().fg(Color::DarkGray);
                    let hint_line = Line::from(vec![
                        Span::styled(" enter ", key_style),
                        Span::styled("confirm  ", desc_style),
                        Span::styled("esc ", key_style),
                        Span::styled("cancel", desc_style),
                    ]);
                    frame.render_widget(Paragraph::new(hint_line), hint_area);
                } else {
                    frame.render_widget(textarea, inner);
                }
            }
        }
        PopupState::Menu {
            title,
            items,
            selected,
        } => {
            let height = (items.len() as u16 + 2).min(area.height - 4);
            let my = (area.height.saturating_sub(height)) / 2;
            let popup_rect = Rect::new(x, my, popup_width, height);
            frame.render_widget(Clear, popup_rect);

            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));

            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let disabled = item.action.is_none();

                    let label = if let Some(ref key) = item.key {
                        format!(" {} {}", key, item.label)
                    } else {
                        format!("   {}", item.label)
                    };

                    if disabled {
                        let text_style = Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::CROSSED_OUT);
                        ListItem::new(Line::from(Span::styled(label, text_style)))
                    } else {
                        let selected_style = if i == *selected {
                            Style::default()
                                .bg(Color::DarkGray)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };
                        let line = if !item.description.is_empty() {
                            Line::from(vec![
                                Span::styled(label, Style::default()),
                                Span::raw(" "),
                                Span::styled(&item.description, Style::default().fg(Color::Yellow)),
                            ])
                        } else {
                            Line::from(label)
                        };
                        ListItem::new(line).style(selected_style)
                    }
                })
                .collect();

            let list = List::new(list_items).block(block);
            frame.render_widget(list, popup_rect);

            // Show disabled item description only when the selected item is disabled
            if let Some(selected_item) = items.get(*selected) {
                if selected_item.action.is_none() && !selected_item.description.is_empty() {
                    let hint_y = popup_rect.y + popup_rect.height;
                    if hint_y < area.height {
                        let hint_text = format!("Disabled: {}", selected_item.description);
                        let hint_rect = Rect::new(popup_rect.x, hint_y, popup_rect.width, 1);
                        frame.render_widget(Clear, hint_rect);
                        frame.render_widget(
                            Paragraph::new(Span::styled(
                                hint_text,
                                Style::default().fg(Color::DarkGray),
                            )),
                            hint_rect,
                        );
                    }
                }
            }
        }
        PopupState::Loading { title, message } => {
            const SPINNER_CHARS: &[char] = &['·', '✻', '✽', '✶', '✳', '✢'];
            // Change symbol every ~8 frames (~128ms at 60fps)
            let spinner = SPINNER_CHARS[(spinner_frame / 8) % SPINNER_CHARS.len()];

            let height = 8u16;
            let ly = (area.height.saturating_sub(height)) / 2;
            let popup_rect = Rect::new(x, ly, popup_width, height);
            frame.render_widget(Clear, popup_rect);

            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));

            let text = vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!(" {} ", message),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!(" {} Please wait...", spinner),
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let widget = Paragraph::new(text).block(block);
            frame.render_widget(widget, popup_rect);
        }
        PopupState::Checklist {
            title,
            items,
            selected,
            search,
            ..
        } => {
            // Filter items by search query
            let visible: Vec<(usize, &super::popup::ChecklistItem)> = items
                .iter()
                .enumerate()
                .filter(|(_, it)| {
                    search.is_empty() || it.label.to_lowercase().contains(&search.to_lowercase())
                })
                .collect();

            // Height: search bar (1) + blank (1) + items + blank (1) + hint (1) + borders (2)
            let content_lines = visible.len().max(1);
            let height = (content_lines as u16 + 6).min(area.height - 4).max(8);
            let cy = (area.height.saturating_sub(height)) / 2;
            let popup_rect = Rect::new(x, cy, popup_width, height);
            frame.render_widget(Clear, popup_rect);

            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));
            frame.render_widget(block, popup_rect);

            let inner = popup_rect.inner(ratatui::layout::Margin {
                horizontal: 1,
                vertical: 1,
            });
            if inner.height < 3 {
                // Too small, skip
            } else {
                // Search bar row
                let search_area = Rect::new(inner.x, inner.y, inner.width, 1);
                let search_display = if search.is_empty() {
                    Line::from(Span::styled(
                        " Type to filter...",
                        Style::default().fg(Color::DarkGray),
                    ))
                } else {
                    Line::from(vec![
                        Span::styled(" ", Style::default().fg(Color::Yellow)),
                        Span::styled(search.clone(), Style::default().fg(Color::Yellow)),
                        Span::styled("▏", Style::default().fg(Color::Yellow)),
                    ])
                };
                frame.render_widget(Paragraph::new(search_display), search_area);

                // Separator line
                let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
                let sep = "─".repeat(inner.width as usize);
                frame.render_widget(
                    Paragraph::new(Span::styled(sep, Style::default().fg(Color::DarkGray))),
                    sep_area,
                );

                // Checklist items
                let list_start = inner.y + 2;
                let list_height = inner.height.saturating_sub(3); // search + sep + hint
                let list_area = Rect::new(inner.x, list_start, inner.width, list_height);

                let list_items: Vec<ListItem> = visible
                    .iter()
                    .enumerate()
                    .map(|(vi, (_, item))| {
                        let check_sym = if item.checked { "◉" } else { "○" };
                        let check_color = if item.checked {
                            Color::Green
                        } else {
                            Color::DarkGray
                        };
                        let is_selected = vi == *selected;

                        let line = Line::from(vec![
                            Span::raw("  "),
                            Span::styled(check_sym, Style::default().fg(check_color)),
                            Span::raw("  "),
                            Span::styled(
                                item.label.clone(),
                                if is_selected {
                                    Style::default()
                                        .fg(Color::White)
                                        .add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(Color::White)
                                },
                            ),
                        ]);

                        if is_selected {
                            ListItem::new(line).style(Style::default().bg(Color::DarkGray))
                        } else {
                            ListItem::new(line)
                        }
                    })
                    .collect();

                let list = List::new(list_items);
                frame.render_widget(list, list_area);

                // Hint at bottom
                let hint_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
                let any_checked = items.iter().any(|it| it.checked);
                let mut hint_spans = vec![
                    Span::styled(" space", Style::default().fg(Color::Yellow)),
                    Span::styled(": toggle  ", Style::default().fg(Color::DarkGray)),
                ];
                if any_checked {
                    hint_spans.push(Span::styled("ctrl-a", Style::default().fg(Color::Yellow)));
                    hint_spans.push(Span::styled(
                        ": clear  ",
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                hint_spans.push(Span::styled("enter", Style::default().fg(Color::Yellow)));
                hint_spans.push(Span::styled(
                    ": apply  ",
                    Style::default().fg(Color::DarkGray),
                ));
                hint_spans.push(Span::styled("esc", Style::default().fg(Color::Yellow)));
                hint_spans.push(Span::styled(
                    ": cancel",
                    Style::default().fg(Color::DarkGray),
                ));
                let hint = Line::from(hint_spans);
                frame.render_widget(Paragraph::new(hint), hint_area);
            }
        }
        PopupState::Help {
            sections,
            selected,
            search_textarea,
            scroll_offset,
        } => {
            // Collect all visible entries (filtered by search) as flat list with section headers
            let search = search_textarea.lines().join("");
            let search_lower = search.to_lowercase();
            let has_search = !search_lower.is_empty();

            // Build flat display list: (is_header, key, description, is_match)
            let mut display: Vec<(bool, String, String)> = Vec::new();
            for section in sections {
                let visible_entries: Vec<&super::popup::HelpEntry> = if has_search {
                    section
                        .entries
                        .iter()
                        .filter(|e| {
                            e.key.to_lowercase().contains(&search_lower)
                                || e.description.to_lowercase().contains(&search_lower)
                        })
                        .collect()
                } else {
                    section.entries.iter().collect()
                };

                if !visible_entries.is_empty() {
                    display.push((true, section.title.clone(), String::new()));
                    for entry in visible_entries {
                        display.push((false, entry.key.clone(), entry.description.clone()));
                    }
                }
            }

            // Sizing: use more of the screen for help
            let popup_width = (area.width * 70 / 100).min(72).max(36);
            let content_height = display.len().max(1);
            // search bar (1) + separator (1) + content + hint (1) + borders (2)
            let popup_height = (content_height as u16 + 5)
                .min(area.height.saturating_sub(4))
                .max(10);
            let x = (area.width.saturating_sub(popup_width)) / 2;
            let y = (area.height.saturating_sub(popup_height)) / 2;
            let popup_rect = Rect::new(x, y, popup_width, popup_height);
            frame.render_widget(Clear, popup_rect);

            let block = Block::default()
                .title(" Keybindings ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));
            frame.render_widget(block, popup_rect);

            let inner = popup_rect.inner(ratatui::layout::Margin {
                horizontal: 1,
                vertical: 1,
            });
            if inner.height < 3 {
                return;
            }

            // Search bar row: " " prefix + textarea
            let prefix_width = 2u16; // "  "
            let prefix_rect = Rect::new(inner.x, inner.y, prefix_width, 1);
            let prefix_style = if search.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Yellow)
            };
            frame.render_widget(
                Paragraph::new(Span::styled("  ", prefix_style)),
                prefix_rect,
            );
            let ta_width = inner.width.saturating_sub(prefix_width);
            let ta_rect = Rect::new(inner.x + prefix_width, inner.y, ta_width, 1);
            frame.render_widget(&*search_textarea, ta_rect);

            // Separator
            let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
            let sep = "─".repeat(inner.width as usize);
            frame.render_widget(
                Paragraph::new(Span::styled(sep, Style::default().fg(Color::DarkGray))),
                sep_area,
            );

            // Content area
            let list_start = inner.y + 2;
            let list_height = inner.height.saturating_sub(3) as usize; // search + sep + hint
            let list_area = Rect::new(inner.x, list_start, inner.width, list_height as u16);

            // Use the stored scroll_offset, clamped to valid range
            let max_scroll = display.len().saturating_sub(list_height);
            let so = *scroll_offset;
            let effective_scroll = if so > max_scroll { max_scroll } else { so };

            let visible_display: Vec<&(bool, String, String)> = display
                .iter()
                .skip(effective_scroll)
                .take(list_height)
                .collect();

            // Count non-header entries before scroll offset to track selection
            let mut entry_idx = 0usize;
            for (is_header, _, _) in display.iter().take(effective_scroll) {
                if !is_header {
                    entry_idx += 1;
                }
            }

            let key_col_width = 14usize;

            let mut list_items: Vec<ListItem> = Vec::new();
            for (is_header, key_or_title, desc) in visible_display {
                if *is_header {
                    let line = Line::from(vec![Span::styled(
                        format!(" {} ", key_or_title),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )]);
                    list_items.push(ListItem::new(line));
                } else {
                    let is_selected = entry_idx == *selected;
                    entry_idx += 1;

                    let key_display = format!("  {:>width$}", key_or_title, width = key_col_width);
                    let desc_display = format!("  {}", desc);

                    let key_style = if is_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else if has_search && key_or_title.to_lowercase().contains(&search_lower) {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Green)
                    };

                    let desc_style = if is_selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else if has_search && desc.to_lowercase().contains(&search_lower) {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::Gray)
                    };

                    let line = Line::from(vec![
                        Span::styled(key_display, key_style),
                        Span::styled(desc_display, desc_style),
                    ]);

                    if is_selected {
                        list_items
                            .push(ListItem::new(line).style(Style::default().bg(Color::DarkGray)));
                    } else {
                        list_items.push(ListItem::new(line));
                    }
                }
            }

            let list = List::new(list_items);
            frame.render_widget(list, list_area);

            // Hint bar at bottom
            let hint_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
            let hint = Line::from(vec![
                Span::styled(" j/k", Style::default().fg(Color::Yellow)),
                Span::styled(": navigate  ", Style::default().fg(Color::DarkGray)),
                Span::styled("type", Style::default().fg(Color::Yellow)),
                Span::styled(": search  ", Style::default().fg(Color::DarkGray)),
                Span::styled("esc", Style::default().fg(Color::Yellow)),
                Span::styled(": close", Style::default().fg(Color::DarkGray)),
            ]);
            frame.render_widget(Paragraph::new(hint), hint_area);
        }
        PopupState::RefPicker {
            title,
            items,
            selected,
            search_textarea,
            scroll_offset,
            ..
        } => {
            let search = search_textarea.lines().join("");
            let search_lower = search.to_lowercase();

            // All items are always shown (no filtering) — typing jumps to match
            // Group by category for display
            let mut display: Vec<(bool, String, String)> = Vec::new(); // (is_header, label, value)
            let mut last_cat = String::new();
            for item in items.iter() {
                if item.category != last_cat {
                    display.push((true, item.category.clone(), String::new()));
                    last_cat = item.category.clone();
                }
                display.push((false, item.label.clone(), item.value.clone()));
            }

            let popup_width = (area.width * 70 / 100).min(72).max(36);
            // Fixed max height — always scrollable for consistency
            let max_popup = (area.height * 60 / 100).max(10);
            let popup_height = max_popup.min(area.height.saturating_sub(4));
            let x = (area.width.saturating_sub(popup_width)) / 2;
            let y = (area.height.saturating_sub(popup_height)) / 2;
            let popup_rect = Rect::new(x, y, popup_width, popup_height);
            frame.render_widget(Clear, popup_rect);

            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));
            frame.render_widget(block, popup_rect);

            let inner = popup_rect.inner(ratatui::layout::Margin {
                horizontal: 1,
                vertical: 1,
            });
            if inner.height < 3 {
                return;
            }

            // Search bar
            let prefix_width = 2u16;
            let prefix_rect = Rect::new(inner.x, inner.y, prefix_width, 1);
            let prefix_style = if search.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Yellow)
            };
            frame.render_widget(
                Paragraph::new(Span::styled("  ", prefix_style)),
                prefix_rect,
            );
            let ta_width = inner.width.saturating_sub(prefix_width);
            let ta_rect = Rect::new(inner.x + prefix_width, inner.y, ta_width, 1);
            frame.render_widget(&*search_textarea, ta_rect);

            // Separator
            let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
            let sep = "─".repeat(inner.width as usize);
            frame.render_widget(
                Paragraph::new(Span::styled(sep, Style::default().fg(Color::DarkGray))),
                sep_area,
            );

            // Content area
            let list_start = inner.y + 2;
            let list_height = inner.height.saturating_sub(3) as usize;
            let list_area = Rect::new(inner.x, list_start, inner.width, list_height as u16);

            let max_scroll = display.len().saturating_sub(list_height);
            let so = *scroll_offset;
            let effective_scroll = if so > max_scroll { max_scroll } else { so };

            let visible_display: Vec<&(bool, String, String)> = display
                .iter()
                .skip(effective_scroll)
                .take(list_height)
                .collect();

            let mut entry_idx = 0usize;
            for (is_header, _, _) in display.iter().take(effective_scroll) {
                if !is_header {
                    entry_idx += 1;
                }
            }

            let mut list_items: Vec<ListItem> = Vec::new();
            for (is_header, label, _value) in visible_display {
                if *is_header {
                    let line = Line::from(vec![Span::styled(
                        format!(" {} ", label),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )]);
                    list_items.push(ListItem::new(line));
                } else {
                    let is_selected = entry_idx == *selected;
                    entry_idx += 1;

                    let base_fg = if is_selected { Color::White } else { Color::Gray };
                    let highlight_fg = Color::Yellow;

                    // Build spans with search match highlighting
                    let mut spans = vec![Span::styled("  ", Style::default().fg(base_fg))];
                    if !search_lower.is_empty() {
                        let label_lower = label.to_lowercase();
                        if let Some(pos) = label_lower.find(&search_lower) {
                            let before = &label[..pos];
                            let matched = &label[pos..pos + search_lower.len()];
                            let after = &label[pos + search_lower.len()..];
                            if !before.is_empty() {
                                spans.push(Span::styled(before.to_string(), Style::default().fg(base_fg)));
                            }
                            let match_style = Style::default().fg(highlight_fg).add_modifier(Modifier::BOLD);
                            spans.push(Span::styled(matched.to_string(), match_style));
                            if !after.is_empty() {
                                spans.push(Span::styled(after.to_string(), Style::default().fg(base_fg)));
                            }
                        } else {
                            spans.push(Span::styled(label.clone(), Style::default().fg(base_fg)));
                        }
                    } else {
                        let style = if is_selected {
                            Style::default().fg(base_fg).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(base_fg)
                        };
                        spans.push(Span::styled(label.clone(), style));
                    }

                    let line = Line::from(spans);

                    if is_selected {
                        list_items.push(ListItem::new(line).style(Style::default().bg(Color::DarkGray)));
                    } else {
                        list_items.push(ListItem::new(line));
                    }
                }
            }

            let list = List::new(list_items);
            frame.render_widget(list, list_area);

            // Hint bar at bottom
            let hint_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
            let hint = Line::from(vec![
                Span::styled(" ↑↓", Style::default().fg(Color::Yellow)),
                Span::styled(": navigate  ", Style::default().fg(Color::DarkGray)),
                Span::styled("type", Style::default().fg(Color::Yellow)),
                Span::styled(": jump to  ", Style::default().fg(Color::DarkGray)),
                Span::styled("enter", Style::default().fg(Color::Yellow)),
                Span::styled(": select  ", Style::default().fg(Color::DarkGray)),
                Span::styled("esc", Style::default().fg(Color::Yellow)),
                Span::styled(": cancel", Style::default().fg(Color::DarkGray)),
            ]);
            frame.render_widget(Paragraph::new(hint), hint_area);
        }
        PopupState::None => {}
    }
}
