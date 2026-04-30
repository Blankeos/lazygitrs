use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use unicode_width::UnicodeWidthStr;

use std::collections::HashSet;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::{AppConfig, Theme};
use crate::git::GitCommands;
use crate::model::Model;
use crate::model::commit::{Commit, CommitStat};
use crate::model::file_tree::{CommitFileTreeNode, FileTreeNode};
use crate::pager::side_by_side::{self, DiffPanel, DiffPanelLayout, DiffViewState};

use super::ScreenMode;
use super::context::{ContextId, ContextManager, SideWindow};
use super::layout::{self, LayoutState};
use super::popup::{CommitInputFocus, PopupState};
use super::presentation;

pub fn render(
    frame: &mut Frame,
    model: &Model,
    ctx_mgr: &mut ContextManager,
    layout_state: &LayoutState,
    popup: &PopupState,
    config: &AppConfig,
    theme: &Theme,
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
    sub_commits_parent_context: ContextId,
    spinner_frame: usize,
    remote_op_label: Option<&str>,
    remote_op_success: bool,
    cherry_pick_clipboard: &[String],
    range_select_anchor: Option<usize>,
    diff_loading: bool,
    diff_loading_show: bool,
    commit_stats: &Arc<Mutex<HashMap<String, CommitStat>>>,
    commit_stats_inflight: &Arc<Mutex<std::collections::HashSet<String>>>,
    commit_messages: &Arc<Mutex<HashMap<String, String>>>,
    commit_messages_inflight: &Arc<Mutex<std::collections::HashSet<String>>>,
    git: &Arc<GitCommands>,
    commit_details_scroll: &mut u16,
    commit_details_scroll_hash: &mut String,
    show_commit_details: bool,
    ai_button_hovered: bool,
    ai_configured: bool,
) {
    let area = frame.area();
    let panel_count = SideWindow::ALL.len();

    // Determine which panel index is active so it gets expanded
    let active_window = ctx_mgr.active_window();
    let active_panel_index = SideWindow::ALL
        .iter()
        .position(|w| *w == active_window)
        .unwrap_or(1); // default to Files

    // Determine if the commit-details panel should be shown.  We show it when
    // the active context is a commit-listing context and a commit is selected.
    let current_commit: Option<&Commit> = resolve_current_commit(model, ctx_mgr, commit_files_hash);
    let show_details = show_commit_details && current_commit.is_some();

    let fl = layout::compute_layout_with_details(
        area,
        layout_state.side_panel_ratio,
        panel_count,
        active_panel_index,
        screen_mode,
        show_details,
        !diff_focused, // sidebar_focused_full: only meaningful in Full mode
    );

    // Full screen mode
    if screen_mode == ScreenMode::Full {
        if diff_focused {
            // Diff is focused: show diff fullscreen
            if !diff_view.is_empty() {
                let show_revert_markers = ctx_mgr.active() == ContextId::Files;
                side_by_side::render_diff(frame, fl.main_panel, diff_view, theme, true, diff_loading_show, show_revert_markers);
                side_by_side::render_diff_search_highlights(frame, fl.main_panel, diff_view, theme);
                side_by_side::render_diff_search_bar(frame, fl.main_panel, diff_view, theme);
            } else if diff_loading {
                let block = Block::default()
                    .title(" Diff ")
                    .borders(Borders::ALL)
                    .border_style(theme.active_border);
                if diff_loading_show {
                    let widget = Paragraph::new(" Loading diff...").block(block);
                    frame.render_widget(widget, fl.main_panel);
                } else {
                    frame.render_widget(block, fl.main_panel);
                }
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
                build_commit_files_title(ctx_id, commit_files_hash, commit_files_message, theme)
            } else if ctx_id == ContextId::BranchCommits {
                build_branch_commits_title(branch_commits_name, theme)
            } else if ctx_id == ContextId::Commits && !commit_branch_filter.is_empty() {
                let filter_label = commit_branch_filter.join(", ");
                Line::from(vec![
                    Span::raw(" Commits "),
                    Span::styled(
                        format!("[filter: {}] ", filter_label),
                        Style::default().fg(theme.accent_secondary),
                    ),
                ])
            } else {
                build_window_title(ctx_mgr.active_window(), ctx_id, ctx_mgr, theme)
            };
            let block = Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(theme.active_border);

            match ctx_id {
                ContextId::Status => {
                    render_status_main(frame, fl.main_panel, model, config, theme, block);
                }
                ContextId::Files => {
                    if show_file_tree {
                        let items = presentation::files::render_file_tree(
                            model,
                            theme,
                            file_tree_nodes,
                            collapsed_dirs,
                        );
                        render_list_ctx(frame, fl.main_panel, block, items, selected, true, theme, ctx_mgr, ctx_id);
                    } else {
                        let items = presentation::files::render_file_list(model, theme);
                        render_list_ctx(frame, fl.main_panel, block, items, selected, true, theme, ctx_mgr, ctx_id);
                    }
                }
                ContextId::Branches => {
                    let items = presentation::branches::render_branch_list(model, theme, remote_op_label, spinner_frame, remote_op_success);
                    render_list_ctx(frame, fl.main_panel, block, items, selected, true, theme, ctx_mgr, ctx_id);
                }
                ContextId::Remotes | ContextId::RemoteBranches => {
                    if ctx_mgr.active() == ContextId::RemoteBranches {
                        let rb_selected = ctx_mgr.selected(ContextId::RemoteBranches);
                        let rb_block = Block::default()
                            .title(format!(" Remote Branches ({}) ", remote_branches_name))
                            .borders(Borders::ALL)
                            .border_style(theme.active_border);
                        let items = presentation::remote_branches::render_remote_branch_list(&model.sub_remote_branches, &model.head_branch_name, theme);
                        render_list_ctx(frame, fl.main_panel, rb_block, items, rb_selected, true, theme, ctx_mgr, ContextId::RemoteBranches);
                    } else {
                        let items = presentation::remotes::render_remote_list(model, theme);
                        render_list_ctx(frame, fl.main_panel, block, items, selected, true, theme, ctx_mgr, ctx_id);
                    }
                }
                ContextId::Tags => {
                    let items = presentation::tags::render_tag_list(model, theme);
                    render_list_ctx(frame, fl.main_panel, block, items, selected, true, theme, ctx_mgr, ctx_id);
                }
                ContextId::Commits => {
                    let items = presentation::commits::render_commit_list(model, theme, cherry_pick_clipboard);
                    let range = range_select_anchor.map(|a| (a.min(selected), a.max(selected)));
                    render_list_with_range_ctx(frame, fl.main_panel, block, items, selected, true, theme, range, ctx_mgr, ctx_id);
                }
                ContextId::Stash => {
                    let items = presentation::stash::render_stash_list(model, theme);
                    render_list_ctx(frame, fl.main_panel, block, items, selected, true, theme, ctx_mgr, ctx_id);
                }
                ContextId::BranchCommits => {
                    let items = presentation::commits::render_sub_commit_list(model, theme);
                    render_list_ctx(frame, fl.main_panel, block, items, selected, true, theme, ctx_mgr, ctx_id);
                }
                ContextId::CommitFiles | ContextId::StashFiles | ContextId::BranchCommitFiles => {
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list_ctx(frame, fl.main_panel, block, items, selected, true, theme, ctx_mgr, ctx_id);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, theme);
                        render_list_ctx(frame, fl.main_panel, block, items, selected, true, theme, ctx_mgr, ctx_id);
                    }
                }
                _ => {
                    let widget = Paragraph::new("").block(block);
                    frame.render_widget(widget, fl.main_panel);
                }
            }
        }
        // Full-mode details strip (sidebar-focused only) — compact, above sidebar.
        if let (Some(details_rect), Some(commit)) = (fl.commit_details_panel, current_commit) {
            if commit_details_scroll_hash.as_str() != commit.hash.as_str() {
                *commit_details_scroll = 0;
                *commit_details_scroll_hash = commit.hash.clone();
            }
            render_commit_details_panel(
                frame,
                details_rect,
                commit,
                commit_stats,
                commit_stats_inflight,
                commit_messages,
                commit_messages_inflight,
                git,
                theme,
                true,
                commit_details_scroll,
            );
        }
        render_status_bar(frame, fl.status_bar, ctx_mgr, diff_view, theme, model, diff_focused);
        // Render text selection highlight overlay and tooltip (must be before popup)
        render_selection_overlay(frame, diff_view, fl.main_panel, theme);
        if *popup != PopupState::None {
            render_popup(frame, popup, area, spinner_frame, theme, ai_button_hovered, ai_configured);
        }
        render_command_log(frame, &fl, command_log, show_command_log, theme);
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
                    Style::default().fg(theme.accent_secondary),
                ),
            ])
        } else {
            build_window_title(*window, ctx_id, ctx_mgr, theme)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        match ctx_id {
            ContextId::Status => {
                let inner_width = rect.width.saturating_sub(2) as usize;
                let status_line = render_status_sidebar(model, config, inner_width, theme);
                let widget = Paragraph::new(status_line).block(block);
                frame.render_widget(widget, rect);
            }
            ContextId::Files => {
                if show_file_tree {
                    let items = presentation::files::render_file_tree(
                        model,
                        theme,
                        file_tree_nodes,
                        collapsed_dirs,
                    );
                    render_list_ctx(frame, rect, block, items, selected, is_active, theme, ctx_mgr, ctx_id);
                } else {
                    let items = presentation::files::render_file_list(model, theme);
                    render_list_ctx(frame, rect, block, items, selected, is_active, theme, ctx_mgr, ctx_id);
                }
            }
            ContextId::Worktrees => {
                let items = render_worktree_list(model, theme);
                render_list_ctx(frame, rect, block, items, selected, is_active, theme, ctx_mgr, ctx_id);
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
                                Style::default().fg(theme.accent),
                            ),
                            Span::styled(
                                sub.path.clone(),
                                Style::default().fg(theme.text_dimmed),
                            ),
                        ]);
                        ListItem::new(line)
                    }).collect();
                    render_list_ctx(frame, rect, block, items, selected, is_active, theme, ctx_mgr, ctx_id);
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
                        theme,
                    );
                    let cf_block = Block::default()
                        .title(cf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::BranchCommitFiles);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, theme);
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::BranchCommitFiles);
                    }
                } else if ctx_mgr.active() == ContextId::BranchCommits {
                    let bc_selected = ctx_mgr.selected(ContextId::BranchCommits);
                    let bc_title = build_branch_commits_title(branch_commits_name, theme);
                    let bc_block = Block::default()
                        .title(bc_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    let items = presentation::commits::render_sub_commit_list(model, theme);
                    render_list_ctx(frame, rect, bc_block, items, bc_selected, is_active, theme, ctx_mgr, ContextId::BranchCommits);
                } else {
                    let items = presentation::branches::render_branch_list(model, theme, remote_op_label, spinner_frame, remote_op_success);
                    render_list_ctx(frame, rect, block, items, selected, is_active, theme, ctx_mgr, ctx_id);
                }
            }
            ContextId::Remotes => {
                if ctx_mgr.active() == ContextId::BranchCommitFiles
                    && sub_commits_parent_context == ContextId::RemoteBranches
                {
                    let cf_selected = ctx_mgr.selected(ContextId::BranchCommitFiles);
                    let cf_title = build_commit_files_title(
                        ContextId::BranchCommitFiles,
                        commit_files_hash,
                        commit_files_message,
                        theme,
                    );
                    let cf_block = Block::default()
                        .title(cf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::BranchCommitFiles);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, theme);
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::BranchCommitFiles);
                    }
                } else if ctx_mgr.active() == ContextId::BranchCommits
                    && sub_commits_parent_context == ContextId::RemoteBranches
                {
                    let bc_selected = ctx_mgr.selected(ContextId::BranchCommits);
                    let bc_title = build_branch_commits_title(branch_commits_name, theme);
                    let bc_block = Block::default()
                        .title(bc_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    let items = presentation::commits::render_sub_commit_list(model, theme);
                    render_list_ctx(frame, rect, bc_block, items, bc_selected, is_active, theme, ctx_mgr, ContextId::BranchCommits);
                } else if ctx_mgr.active() == ContextId::RemoteBranches {
                    let rb_selected = ctx_mgr.selected(ContextId::RemoteBranches);
                    let rb_title = format!(" Remote Branches ({}) ", remote_branches_name);
                    let rb_block = Block::default()
                        .title(rb_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    let items = presentation::remote_branches::render_remote_branch_list(&model.sub_remote_branches, &model.head_branch_name, theme);
                    render_list_ctx(frame, rect, rb_block, items, rb_selected, is_active, theme, ctx_mgr, ContextId::RemoteBranches);
                } else {
                    let items = presentation::remotes::render_remote_list(model, theme);
                    render_list_ctx(frame, rect, block, items, selected, is_active, theme, ctx_mgr, ctx_id);
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
                        theme,
                    );
                    let cf_block = Block::default()
                        .title(cf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::BranchCommitFiles);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, theme);
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::BranchCommitFiles);
                    }
                } else if ctx_mgr.active() == ContextId::BranchCommits {
                    let bc_selected = ctx_mgr.selected(ContextId::BranchCommits);
                    let bc_title = build_branch_commits_title(branch_commits_name, theme);
                    let bc_block = Block::default()
                        .title(bc_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    let items = presentation::commits::render_sub_commit_list(model, theme);
                    render_list_ctx(frame, rect, bc_block, items, bc_selected, is_active, theme, ctx_mgr, ContextId::BranchCommits);
                } else {
                    let items = presentation::tags::render_tag_list(model, theme);
                    render_list_ctx(frame, rect, block, items, selected, is_active, theme, ctx_mgr, ctx_id);
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
                        theme,
                    );
                    let cf_block = Block::default()
                        .title(cf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::CommitFiles);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, theme);
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::CommitFiles);
                    }
                } else {
                    let items = presentation::commits::render_commit_list(model, theme, cherry_pick_clipboard);
                    let range = if is_active { range_select_anchor.map(|a| (a.min(selected), a.max(selected))) } else { None };
                    render_list_with_range_ctx(frame, rect, block, items, selected, is_active, theme, range, ctx_mgr, ctx_id);
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
                        theme,
                    );
                    let cf_block = Block::default()
                        .title(cf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::CommitFiles);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, theme);
                        render_list_ctx(frame, rect, cf_block, items, cf_selected, is_active, theme, ctx_mgr, ContextId::CommitFiles);
                    }
                } else {
                    let items = presentation::reflog::render_reflog_list(model, theme);
                    render_list_ctx(frame, rect, block, items, selected, is_active, theme, ctx_mgr, ctx_id);
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
                        theme,
                    );
                    let sf_block = Block::default()
                        .title(sf_title)
                        .borders(Borders::ALL)
                        .border_style(border_style);
                    if show_commit_file_tree {
                        let items = presentation::commit_files::render_commit_file_tree(
                            model,
                            theme,
                            commit_file_tree_nodes,
                            commit_files_collapsed_dirs,
                        );
                        render_list_ctx(frame, rect, sf_block, items, sf_selected, is_active, theme, ctx_mgr, ContextId::StashFiles);
                    } else {
                        let items =
                            presentation::commit_files::render_commit_file_list(model, theme);
                        render_list_ctx(frame, rect, sf_block, items, sf_selected, is_active, theme, ctx_mgr, ContextId::StashFiles);
                    }
                } else {
                    let items = presentation::stash::render_stash_list(model, theme);
                    render_list_ctx(frame, rect, block, items, selected, is_active, theme, ctx_mgr, ctx_id);
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
        let status_block = Block::default()
            .title(" Status ")
            .borders(Borders::ALL)
            .border_style(theme.active_border);
        render_status_main(frame, fl.main_panel, model, config, theme, status_block);
    } else if !diff_view.is_empty() {
        let show_revert_markers = ctx_mgr.active() == ContextId::Files;
        side_by_side::render_diff(frame, fl.main_panel, diff_view, theme, diff_focused, diff_loading_show, show_revert_markers);
        side_by_side::render_diff_search_highlights(frame, fl.main_panel, diff_view, theme);
        side_by_side::render_diff_search_bar(frame, fl.main_panel, diff_view, theme);
    } else if diff_loading {
        // Diff is being loaded — show empty panel during grace period, then "Loading..." after delay
        let block = Block::default()
            .title(" Diff ")
            .borders(Borders::ALL)
            .border_style(theme.inactive_border);
        if diff_loading_show {
            let widget = Paragraph::new(" Loading diff...").block(block);
            frame.render_widget(widget, fl.main_panel);
        } else {
            frame.render_widget(block, fl.main_panel);
        }
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

    // Normal/Half mode: compact details box sits at the bottom of the active
    // sidebar panel (layout carves the rect out of the active side panel).
    if let (Some(details_rect), Some(commit)) = (fl.commit_details_panel, current_commit) {
        if commit_details_scroll_hash.as_str() != commit.hash.as_str() {
            *commit_details_scroll = 0;
            *commit_details_scroll_hash = commit.hash.clone();
        }
        render_commit_details_panel(
            frame,
            details_rect,
            commit,
            commit_stats,
            commit_stats_inflight,
            commit_messages,
            commit_messages_inflight,
            git,
            theme,
            true,
            commit_details_scroll,
        );
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
            let prefix = Paragraph::new(Span::styled(" /", Style::default().fg(theme.accent_secondary)));
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
                    Style::default().fg(theme.accent_secondary),
                ));
                frame.render_widget(suffix, suffix_rect);
            }
        } else {
            let bar = Paragraph::new(Span::styled(
                format!(" /{}{}", query, match_info),
                Style::default().fg(theme.accent_secondary),
            ));
            frame.render_widget(bar, fl.status_bar);
        }
    } else {
        render_status_bar(frame, fl.status_bar, ctx_mgr, diff_view, theme, model, diff_focused);
    }

    // Render text selection highlight overlay and tooltip
    render_selection_overlay(frame, diff_view, fl.main_panel, theme);

    // Render popup overlay
    if *popup != PopupState::None {
        render_popup(frame, popup, area, spinner_frame, theme, ai_button_hovered, ai_configured);
    }

    // Render command log last so it appears above everything
    render_command_log(frame, &fl, command_log, show_command_log, theme);
}

fn render_command_log(
    frame: &mut Frame,
    fl: &layout::FrameLayout,
    command_log: &[String],
    show_command_log: bool,
    theme: &Theme,
) {
    if !show_command_log || command_log.is_empty() {
        return;
    }

    let log_height = command_log.len().min(5) as u16;
    let log_width = fl.main_panel.width.min(50);
    let log_x = fl.main_panel.x + fl.main_panel.width - log_width;
    let log_y = fl.main_panel.y + fl.main_panel.height - log_height - 1;
    let log_rect = Rect::new(log_x, log_y, log_width, log_height + 2);

    let border_color = theme.cmd_log_border;
    let title_color = theme.cmd_log_title;
    let hint_color = theme.cmd_log_hint;
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
                        .fg(theme.cmd_log_timestamp)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" toggle ", Style::default().fg(hint_color)),
            ])
            .alignment(ratatui::layout::Alignment::Right),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let cmd_color = theme.cmd_log_text;
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
                theme.cmd_log_timestamp
            } else {
                cmd_color
            };
            Line::from(vec![
                Span::styled(" $ ", Style::default().fg(theme.cmd_log_success)),
                Span::styled(s.to_string(), Style::default().fg(fg)),
            ])
        })
        .collect();

    frame.render_widget(Clear, log_rect);
    let log_widget = Paragraph::new(log_lines).block(log_block);
    frame.render_widget(log_widget, log_rect);
}

/// Build a window title like " 4 Commit Files (abc1234 feat: some change) ".
fn build_branch_commits_title<'a>(branch_name: &str, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::raw(" 3 Commits "),
        Span::styled(
            format!("({})", branch_name),
            Style::default().fg(theme.accent_secondary),
        ),
        Span::raw(" "),
    ])
}

fn build_commit_files_title<'a>(
    ctx: ContextId,
    commit_hash: &str,
    commit_message: &str,
    theme: &Theme,
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
        Span::styled(format!("({}", short), Style::default().fg(theme.accent_secondary)),
    ];
    if !commit_message.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            commit_message.to_string(),
            Style::default().fg(theme.text_dimmed),
        ));
    }
    spans.push(Span::styled(") ", Style::default().fg(theme.accent_secondary)));
    Line::from(spans)
}

fn build_window_title<'a>(
    window: SideWindow,
    active_ctx: ContextId,
    _ctx_mgr: &ContextManager,
    theme: &Theme,
) -> Line<'a> {
    let tabs = window.tabs();
    let key = window.key_label();

    if tabs.len() == 1 {
        return Line::from(format!(" {} {} ", key, tabs[0].title()));
    }

    let mut spans = vec![Span::raw(format!(" {} ", key))];

    for (i, ctx) in tabs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(theme.text_dimmed)));
        }
        if *ctx == active_ctx {
            spans.push(Span::styled(
                ctx.title(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                ctx.title(),
                Style::default().fg(theme.text_dimmed),
            ));
        }
    }

    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// Compact 1-line status for the sidebar: "reponame → branch          +N -N"
fn render_status_sidebar<'a>(model: &Model, _config: &AppConfig, inner_width: usize, theme: &Theme) -> Line<'a> {
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
        let padding = if has_changes {
            inner_width.saturating_sub(left_len + right_len).max(1)
        } else {
            inner_width.saturating_sub(left_len + right_len)
        };

        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            prefix,
            Style::default().fg(theme.accent_secondary),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("{} ", repo_name),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled("→ ", Style::default().fg(theme.text_dimmed)));
        spans.push(Span::styled(
            right_side,
            Style::default().fg(theme.accent_secondary),
        ));

        if has_changes {
            spans.push(Span::raw(" ".repeat(padding)));
            if additions > 0 {
                spans.push(Span::styled(
                    format!("+{}", additions),
                    Style::default().fg(theme.file_staged.fg.unwrap_or(theme.accent)),
                ));
            }
            if additions > 0 && deletions > 0 {
                spans.push(Span::raw(" "));
            }
            if deletions > 0 {
                spans.push(Span::styled(
                    format!("-{}", deletions),
                    Style::default().fg(theme.file_unstaged.fg.unwrap_or(theme.change_deleted)),
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
    let padding = if has_changes {
        inner_width.saturating_sub(left_len + right_len).max(1)
    } else {
        inner_width.saturating_sub(left_len + right_len)
    };

    if !ab_text.is_empty() {
        spans.push(Span::styled(
            format!(" {}", ab_text),
            Style::default().fg(theme.file_staged.fg.unwrap_or(theme.accent)),
        ));
    } else {
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!("{} ", repo_name),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled("→ ", Style::default().fg(theme.text_dimmed)));
    spans.push(Span::styled(branch_name, Style::default().fg(theme.branch_local.fg.unwrap_or(theme.accent))));

    if has_changes {
        spans.push(Span::raw(" ".repeat(padding)));
        if additions > 0 {
            spans.push(Span::styled(
                format!("+{}", additions),
                Style::default().fg(theme.file_staged.fg.unwrap_or(theme.accent)),
            ));
        }
        if additions > 0 && deletions > 0 {
            spans.push(Span::raw(" "));
        }
        if deletions > 0 {
            spans.push(Span::styled(
                format!("-{}", deletions),
                Style::default().fg(theme.file_unstaged.fg.unwrap_or(theme.change_deleted)),
            ));
        }
        spans.push(Span::raw(" "));
    }

    Line::from(spans)
}

/// Full status view for the main content area: logo + copyright + repo info
fn render_status_main<'a>(
    frame: &mut Frame,
    rect: Rect,
    model: &Model,
    _config: &AppConfig,
    theme: &crate::config::Theme,
    block: Block<'a>,
) {

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
                Style::default().fg(theme.accent),
            ))
        })
        .collect();

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Copyright 2026 Carlo Taleon (Blankeos)",
        Style::default().fg(theme.text_dimmed),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(format!(" Branch: {}", branch_name)));
    if !model.repo_url.is_empty() {
        lines.push(Line::from(format!(" Repo:   {}", model.repo_url)));
    }
    lines.push(Line::from(format!(" Commits: {}", model.commits.len())));
    lines.push(Line::from(format!(" Files: {}", model.files.len())));
    lines.push(Line::from(format!(" Version: v{}", env!("CARGO_PKG_VERSION"))));

    if !model.contributors.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Contributors",
            Style::default().fg(theme.text_dimmed),
        )));
        for (name, count) in model.contributors.iter().take(10) {
            lines.push(Line::from(format!("   {:>4}  {}", count, name)));
        }
    }

    // In-progress operation banners
    if model.is_rebasing {
        lines.push(Line::from(Span::styled(
            " REBASING",
            Style::default().fg(theme.accent_secondary),
        )));
    }
    if model.is_merging {
        lines.push(Line::from(Span::styled(
            " MERGING",
            Style::default().fg(theme.accent_secondary),
        )));
    }
    if model.is_cherry_picking {
        lines.push(Line::from(Span::styled(
            " CHERRY-PICKING",
            Style::default().fg(theme.accent_secondary),
        )));
    }
    if model.is_bisecting {
        lines.push(Line::from(Span::styled(
            " BISECTING",
            Style::default().fg(theme.accent_secondary),
        )));
    }

    let widget = Paragraph::new(lines).block(block);
    frame.render_widget(widget, rect);
}

fn render_worktree_list<'a>(model: &Model, theme: &Theme) -> Vec<ListItem<'a>> {
    model
        .worktrees
        .iter()
        .map(|wt| {
            let marker = if wt.is_current { "* " } else { "  " };
            let line = Line::from(vec![
                Span::styled(marker.to_string(), Style::default().fg(theme.accent)),
                Span::styled(wt.branch.clone(), Style::default().fg(theme.ref_head)),
                Span::styled(
                    format!(" {}", wt.path),
                    Style::default().fg(theme.text_dimmed),
                ),
            ]);
            ListItem::new(line)
        })
        .collect()
}

/// Render a list using persistent scroll offsets from ContextManager.
fn render_list_ctx(
    frame: &mut Frame,
    rect: Rect,
    block: Block<'_>,
    items: Vec<ListItem<'_>>,
    selected: usize,
    is_active: bool,
    theme: &crate::config::Theme,
    ctx_mgr: &mut ContextManager,
    ctx: ContextId,
) {
    render_list_with_range_ctx(frame, rect, block, items, selected, is_active, theme, None, ctx_mgr, ctx);
}

/// Render a list with range selection using persistent scroll offsets from ContextManager.
fn render_list_with_range_ctx(
    frame: &mut Frame,
    rect: Rect,
    block: Block<'_>,
    items: Vec<ListItem<'_>>,
    selected: usize,
    is_active: bool,
    theme: &crate::config::Theme,
    range: Option<(usize, usize)>,
    ctx_mgr: &mut ContextManager,
    ctx: ContextId,
) {
    let mut so = ctx_mgr.scroll_offset(ctx);
    let follow = !ctx_mgr.viewport_manually_scrolled;
    render_list_with_range_raw(frame, rect, block, items, selected, is_active, theme, range, &mut so, follow);
    ctx_mgr.set_scroll_offset(ctx, so);
}

fn render_list_with_range_raw(
    frame: &mut Frame,
    rect: Rect,
    block: Block<'_>,
    items: Vec<ListItem<'_>>,
    selected: usize,
    is_active: bool,
    theme: &crate::config::Theme,
    range: Option<(usize, usize)>,
    scroll_offset: &mut usize,
    follow_selection: bool,
) {
    if items.is_empty() {
        frame.render_widget(block, rect);
        return;
    }

    let inner = block.inner(rect);
    let visible_height = inner.height as usize;

    // Ensure selected item is visible, only adjusting scroll when necessary.
    // Skip when viewport was manually scrolled (mouse scroll) to avoid snapping back.
    if visible_height == 0 {
        frame.render_widget(block, rect);
        return;
    }
    if follow_selection {
        super::scroll::ensure_visible(selected, scroll_offset, visible_height);
    }
    // Clamp scroll offset to valid range
    let max_offset = items.len().saturating_sub(visible_height);
    if *scroll_offset > max_offset {
        *scroll_offset = max_offset;
    }
    let offset = *scroll_offset;

    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(offset)
        .take(visible_height)
        .enumerate()
        .map(|(i, item)| {
            let idx = i + offset;
            if is_active && idx == selected {
                item.style(theme.selected_line)
            } else if is_active && range.map_or(false, |(lo, hi)| idx >= lo && idx <= hi) {
                item.style(Style::default().bg(theme.selected_bg))
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
    diff_focused: bool,
) {
    let mut hints: Vec<(&str, &str)> = Vec::new();
    let mut emphasized: Vec<&str> = Vec::new();

    // When in a special state (rebasing/merging/cherry-picking), show those options prominently
    if model.is_rebasing {
        hints.push(("m", "continue/abort/skip rebase"));
    } else if model.is_merging {
        hints.push(("m", "continue/abort merge"));
    } else if model.is_cherry_picking {
        hints.push(("m", "continue/abort cherry-pick"));
    }

    if diff_focused && !diff_view.is_empty() {
        // Diff-focused hint set: only the diff-relevant keys, kept tight.
        // Revert-related keys are grouped together at the front so users see
        // c-r right next to its cycle keys. c-r itself only appears when a
        // hunk is actually selected (pressing it otherwise is a no-op).
        if ctx_mgr.active() == ContextId::Files {
            let has_selection = diff_view.selected_revert_hunk.is_some();
            let mut idx = 0;
            if has_selection {
                hints.insert(idx, ("c-r", "revert hunk"));
                emphasized.push("c-r");
                idx += 1;
            }
            hints.insert(idx, ("c-j/c-k", "cycle revert"));
        }
        hints.push(("{/}", "prev/next hunk"));
        hints.push(("[/]", "side view"));
    } else {
        // Sidebar-focused: context-specific hints.
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
                hints.extend([("enter", "commits"), ("space", "checkout"), ("M", "merge"), ("r", "rebase"), ("d", "delete")]);
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
        if !diff_view.is_empty() {
            hints.push(("J/K", "scroll diff"));
            hints.push(("{/}", "hunks"));
        }
    }

    // Global hints (always last)
    hints.extend([("q", "quit"), ("tab/1-5", "panels"), ("j/k", "nav")]);

    let key_style = Style::default().fg(_theme.text).add_modifier(ratatui::style::Modifier::BOLD);
    let key_emphasis_style = Style::default().fg(_theme.accent).add_modifier(ratatui::style::Modifier::BOLD);
    let desc_style = Style::default().fg(_theme.text_dimmed);
    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            let style = if emphasized.contains(key) { key_emphasis_style } else { key_style };
            vec![
                Span::styled(format!(" {} ", key), style),
                Span::styled(format!("{} ", desc), desc_style),
            ]
        })
        .collect();

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, rect);
}

/// Render mouse text selection highlight overlay and copy tooltip on the diff view.
/// `panel_rect` is the main diff panel Rect — selection is rendered only within the selected side.
pub fn render_selection_overlay(frame: &mut Frame, diff_view: &mut DiffViewState, panel_rect: Rect, theme: &Theme) {
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
                .bg(theme.popup_border)
                .fg(Color::Black);
            if let Some(cell) = buf.cell_mut((top_col, top_row)) {
                cell.set_style(highlight_style);
            }
        }

        if diff_view.file_exists_on_disk {
            let tooltip_style = Style::default()
                .bg(theme.selected_bg)
                .fg(theme.text_strong);
            let key_style = Style::default()
                .bg(theme.selected_bg)
                .fg(theme.accent_secondary)
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
        .bg(theme.popup_border)
        .fg(Color::Black);

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
            .bg(theme.selected_bg)
            .fg(theme.text_strong);
        let key_style = Style::default()
            .bg(theme.selected_bg)
            .fg(theme.accent_secondary)
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

const SPINNER_CHARS: &[char] = &['·', '✻', '✽', '✶', '✳', '✢'];

/// Geometry of the ✦ AI-generate button shown inside the commit-message popup.
/// The button sits on the "Summary" label row, right-aligned inside the popup
/// box (not on the border).
pub fn commit_ai_button_geometry(popup: &PopupState, area: Rect) -> Option<Rect> {
    if area.width < 10 || area.height < 6 {
        return None;
    }
    let popup_width = (area.width * 60 / 100).min(60).max(30).min(area.width);
    if popup_width < 16 {
        return None;
    }
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let ta_height: u16 = match popup {
        PopupState::CommitInput { .. } => 16,
        _ => return None,
    };
    let ta_height = ta_height.min(area.height);
    if ta_height < 7 {
        return None;
    }
    let ta_y = (area.height.saturating_sub(ta_height)) / 2;

    // Button: 3 cells (" ✦ ") on the Summary label row (inner.y = ta_y + 1),
    // right-aligned within the inner area (1 col border + 1 col right padding).
    let btn_w: u16 = 3;
    let btn_x = x + popup_width.saturating_sub(btn_w + 2);
    let btn_y = ta_y + 1;
    Some(Rect::new(btn_x, btn_y, btn_w, 1))
}

/// Tooltip rect placed one row above the popup, right-aligned with the button.
fn commit_ai_tooltip_rect(area: Rect, btn_rect: Rect, tip_w: u16) -> Rect {
    let popup_width = (area.width * 60 / 100).min(60).max(30).min(area.width);
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let ta_height: u16 = 16u16.min(area.height);
    let ta_y = (area.height.saturating_sub(ta_height)) / 2;

    let mut tip_x = (btn_rect.x + btn_rect.width).saturating_sub(tip_w);
    // Keep tooltip within the popup's horizontal bounds when possible.
    let popup_right = x + popup_width;
    if tip_x + tip_w > popup_right {
        tip_x = popup_right.saturating_sub(tip_w);
    }
    if tip_x + tip_w > area.width {
        tip_x = area.width.saturating_sub(tip_w);
    }
    let tip_y = if ta_y >= 1 {
        ta_y - 1
    } else if ta_y + ta_height < area.height {
        ta_y + ta_height
    } else {
        btn_rect.y
    };
    Rect::new(tip_x, tip_y, tip_w, 1)
}

pub fn render_popup(frame: &mut Frame, popup: &PopupState, area: Rect, spinner_frame: usize, theme: &Theme, ai_button_hovered: bool, ai_configured: bool) {
    // Bail out early on terminals too small to host any popup — better than
    // panicking inside a render with an out-of-bounds rect.
    if area.width < 4 || area.height < 4 {
        return;
    }
    let popup_width = (area.width * 60 / 100).min(60).max(30).min(area.width);
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
            let confirm_height = (wrapped.len() as u16) + 5; // border*2 + blank line + blank line + y yes / n no
            let cy = (area.height.saturating_sub(confirm_height)) / 2;
            let popup_rect = Rect::new(x, cy, popup_width, confirm_height);
            frame.render_widget(Clear, popup_rect);
            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent_secondary));

            let mut text: Vec<Line> = Vec::new();
            text.push(Line::from(""));
            for line in &wrapped {
                text.push(Line::from(format!(" {}", line)));
            }
            text.push(Line::from(""));
            let key_style = Style::default().fg(theme.accent_secondary);
            let desc_style = Style::default().fg(theme.text_dimmed);
            text.push(Line::from(vec![
                Span::styled(" y", key_style),
                Span::styled(": yes  ", desc_style),
                Span::styled("n", key_style),
                Span::styled(": no", desc_style),
            ]));

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
            let border_color = if is_error { Color::Red } else { theme.accent_secondary };
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
                Style::default().fg(theme.text_dimmed),
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
            let ta_height = ta_height.min(area.height);
            if ta_height < 3 || popup_width < 3 {
                return;
            }
            let ta_y = (area.height.saturating_sub(ta_height)) / 2;
            let ta_rect = Rect::new(x, ta_y, popup_width, ta_height);
            frame.render_widget(Clear, ta_rect);

            // Render a container block with title and hint
            let outer = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.popup_border));
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
                    let key_style = Style::default().fg(theme.text).add_modifier(ratatui::style::Modifier::BOLD);
                    let desc_style = Style::default().fg(theme.text_dimmed);
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
                            Style::default().fg(Color::Black).bg(theme.accent).add_modifier(ratatui::style::Modifier::BOLD),
                            " Confirm ",
                        )
                    } else {
                        (
                            Style::default().fg(theme.accent),
                            " Confirm ",
                        )
                    };
                    let btn_width = (btn_text.len() as u16).min(btn_area.width);
                    if btn_width > 0 {
                        let btn_x = btn_area.x + btn_area.width.saturating_sub(btn_width);
                        let btn_rect = Rect::new(btn_x, btn_area.y, btn_width, 1);
                        frame.render_widget(
                            Paragraph::new(Line::from(Span::styled(btn_text, btn_style))),
                            btn_rect,
                        );
                    }
                } else {
                    frame.render_widget(textarea, inner);
                }
            } else {
                // Non-commit: no button, just hint
                if inner.height > 2 {
                    let ta_area = Rect::new(inner.x, inner.y, inner.width, inner.height - 1);
                    frame.render_widget(textarea, ta_area);

                    let hint_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
                    let key_style = Style::default().fg(theme.text).add_modifier(ratatui::style::Modifier::BOLD);
                    let desc_style = Style::default().fg(theme.text_dimmed);
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
        PopupState::CommitInput {
            summary_textarea,
            body_textarea,
            focus,
            ..
        } => {
            // Two-field commit editor: summary (1 line) + body (multi-line)
            // Layout: border, summary label, summary input, body label, body textarea, hint, border
            let ta_height = 16u16;
            let ta_y = (area.height.saturating_sub(ta_height)) / 2;
            let ta_rect = Rect::new(x, ta_y, popup_width, ta_height);
            frame.render_widget(Clear, ta_rect);

            let border_color = match focus {
                CommitInputFocus::Summary => theme.popup_border,
                CommitInputFocus::Body => theme.popup_border,
            };
            let outer = Block::default()
                .title(" Commit message ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color));
            frame.render_widget(outer, ta_rect);

            let inner = ta_rect.inner(ratatui::layout::Margin {
                horizontal: 1,
                vertical: 1,
            });

            if inner.height > 6 {
                let focused_style = Style::default().fg(theme.accent_secondary).add_modifier(Modifier::BOLD);
                let unfocused_style = Style::default().fg(theme.text_dimmed);

                // Summary label
                let summary_label_area = Rect::new(inner.x, inner.y, inner.width, 1);
                let summary_label_style = if *focus == CommitInputFocus::Summary { focused_style } else { unfocused_style };
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled("Summary", summary_label_style))),
                    summary_label_area,
                );

                // Summary input (1 line)
                let summary_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
                frame.render_widget(summary_textarea, summary_area);

                // Body label
                let body_label_area = Rect::new(inner.x, inner.y + 3, inner.width, 1);
                let body_label_style = if *focus == CommitInputFocus::Body { focused_style } else { unfocused_style };
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled("Description", body_label_style))),
                    body_label_area,
                );

                // Body textarea (remaining space minus hint line and padding)
                let body_height = inner.height.saturating_sub(6); // 1 summary label + 1 summary + 1 gap + 1 body label + 1 hint + 1 padding
                let body_area = Rect::new(inner.x, inner.y + 4, inner.width, body_height);
                frame.render_widget(body_textarea, body_area);

                // Hint line at bottom (1 line padding above)
                let hint_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
                let key_style = Style::default().fg(theme.text).add_modifier(Modifier::BOLD);
                let desc_style = Style::default().fg(theme.text_dimmed);
                let hint_line = Line::from(vec![
                    Span::styled(" enter ", key_style),
                    Span::styled("confirm  ", desc_style),
                    Span::styled("tab ", key_style),
                    Span::styled("switch  ", desc_style),
                    Span::styled("ctrl+o ", key_style),
                    Span::styled("menu  ", desc_style),
                    Span::styled("esc ", key_style),
                    Span::styled("cancel", desc_style),
                ]);
                frame.render_widget(Paragraph::new(hint_line), hint_area);
            } else {
                // Fallback: just render summary
                frame.render_widget(summary_textarea, inner);
            }
        }
        PopupState::Menu {
            title,
            items,
            selected,
            loading_index,
        } => {
            let height = (items.len() as u16 + 2).min(area.height - 4);
            let my = (area.height.saturating_sub(height)) / 2;
            let popup_rect = Rect::new(x, my, popup_width, height);
            frame.render_widget(Clear, popup_rect);

            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent));

            let list_items: Vec<ListItem> = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let disabled = item.action.is_none();
                    let is_loading = *loading_index == Some(i);

                    let label = if let Some(ref key) = item.key {
                        format!(" {} {}", key, item.label)
                    } else {
                        format!("   {}", item.label)
                    };

                    if disabled {
                        let text_style = Style::default()
                            .fg(theme.text_dimmed)
                            .add_modifier(Modifier::CROSSED_OUT);
                        ListItem::new(Line::from(Span::styled(label, text_style)))
                    } else {
                        let selected_style = if i == *selected {
                            Style::default()
                                .bg(theme.selected_bg)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };
                        let line = if is_loading {
                            let spinner = SPINNER_CHARS[(spinner_frame / 8) % SPINNER_CHARS.len()];
                            Line::from(vec![
                                Span::styled(label, Style::default()),
                                Span::raw(" "),
                                Span::styled(
                                    format!("{}", spinner),
                                    Style::default().fg(theme.accent_secondary),
                                ),
                            ])
                        } else if !item.description.is_empty() {
                            Line::from(vec![
                                Span::styled(label, Style::default()),
                                Span::raw(" "),
                                Span::styled(&item.description, Style::default().fg(theme.accent_secondary)),
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
                                Style::default().fg(theme.text_dimmed),
                            )),
                            hint_rect,
                        );
                    }
                }
            }
        }
        PopupState::Loading { title, message } => {
            // Change symbol every ~8 frames (~128ms at 60fps)
            let spinner = SPINNER_CHARS[(spinner_frame / 8) % SPINNER_CHARS.len()];

            let height = 8u16;
            let ly = (area.height.saturating_sub(height)) / 2;
            let popup_rect = Rect::new(x, ly, popup_width, height);
            frame.render_widget(Clear, popup_rect);

            let block = Block::default()
                .title(format!(" {} ", title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent_secondary));

            let text = vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!(" {} ", message),
                    Style::default()
                        .fg(theme.accent_secondary)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!(" {} Please wait...", spinner),
                    Style::default().fg(theme.text_dimmed),
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
                .border_style(Style::default().fg(theme.accent));
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
                        Style::default().fg(theme.text_dimmed),
                    ))
                } else {
                    Line::from(vec![
                        Span::styled(" ", Style::default().fg(theme.accent_secondary)),
                        Span::styled(search.clone(), Style::default().fg(theme.accent_secondary)),
                        Span::styled("▏", Style::default().fg(theme.accent_secondary)),
                    ])
                };
                frame.render_widget(Paragraph::new(search_display), search_area);

                // Separator line
                let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
                let sep = "─".repeat(inner.width as usize);
                frame.render_widget(
                    Paragraph::new(Span::styled(sep, Style::default().fg(theme.text_dimmed))),
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
                            theme.accent
                        } else {
                            theme.text_dimmed
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
                                        .fg(theme.text_strong)
                                        .add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(theme.text_strong)
                                },
                            ),
                        ]);

                        if is_selected {
                            ListItem::new(line).style(Style::default().bg(theme.selected_bg))
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
                    Span::styled(" space", Style::default().fg(theme.accent_secondary)),
                    Span::styled(": toggle  ", Style::default().fg(theme.text_dimmed)),
                ];
                if any_checked {
                    hint_spans.push(Span::styled("ctrl-a", Style::default().fg(theme.accent_secondary)));
                    hint_spans.push(Span::styled(
                        ": clear  ",
                        Style::default().fg(theme.text_dimmed),
                    ));
                }
                hint_spans.push(Span::styled("enter", Style::default().fg(theme.accent_secondary)));
                hint_spans.push(Span::styled(
                    ": apply  ",
                    Style::default().fg(theme.text_dimmed),
                ));
                hint_spans.push(Span::styled("esc", Style::default().fg(theme.accent_secondary)));
                hint_spans.push(Span::styled(
                    ": cancel",
                    Style::default().fg(theme.text_dimmed),
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
            let popup_width = (area.width * 70 / 100).min(72).max(36).min(area.width);
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
                .border_style(Style::default().fg(theme.accent));
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
                Style::default().fg(theme.text_dimmed)
            } else {
                Style::default().fg(theme.accent_secondary)
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
                Paragraph::new(Span::styled(sep, Style::default().fg(theme.text_dimmed))),
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
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    )]);
                    list_items.push(ListItem::new(line));
                } else {
                    let is_selected = entry_idx == *selected;
                    entry_idx += 1;

                    let key_display = format!("  {:>width$}", key_or_title, width = key_col_width);
                    let desc_display = format!("  {}", desc);

                    let key_base_style = if is_selected {
                        Style::default()
                            .fg(theme.accent_secondary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.accent)
                    };

                    let desc_base_style = if is_selected {
                        Style::default()
                            .fg(theme.text_strong)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text)
                    };

                    let highlight_style = Style::default()
                        .fg(theme.accent_secondary)
                        .add_modifier(Modifier::BOLD);

                    let build_spans = |text: &str, base: Style| -> Vec<Span<'static>> {
                        if !has_search {
                            return vec![Span::styled(text.to_string(), base)];
                        }
                        let lower = text.to_lowercase();
                        if let Some(pos) = lower.find(&search_lower) {
                            let before = &text[..pos];
                            let matched = &text[pos..pos + search_lower.len()];
                            let after = &text[pos + search_lower.len()..];
                            let mut s = Vec::new();
                            if !before.is_empty() {
                                s.push(Span::styled(before.to_string(), base));
                            }
                            s.push(Span::styled(matched.to_string(), highlight_style));
                            if !after.is_empty() {
                                s.push(Span::styled(after.to_string(), base));
                            }
                            s
                        } else {
                            vec![Span::styled(text.to_string(), base)]
                        }
                    };

                    let mut spans = build_spans(&key_display, key_base_style);
                    spans.extend(build_spans(&desc_display, desc_base_style));
                    let line = Line::from(spans);

                    if is_selected {
                        list_items
                            .push(ListItem::new(line).style(Style::default().bg(theme.selected_bg)));
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
                Span::styled(" j/k", Style::default().fg(theme.accent_secondary)),
                Span::styled(": navigate  ", Style::default().fg(theme.text_dimmed)),
                Span::styled("type", Style::default().fg(theme.accent_secondary)),
                Span::styled(": search  ", Style::default().fg(theme.text_dimmed)),
                Span::styled("esc", Style::default().fg(theme.accent_secondary)),
                Span::styled(": close", Style::default().fg(theme.text_dimmed)),
            ]);
            frame.render_widget(Paragraph::new(hint), hint_area);
        }
        PopupState::RefPicker { title, core, .. } => {
            render_list_picker(
                frame, area, theme, core,
                title,
                70, 72, 36,
                &[("↑↓", "navigate"), ("type", "jump to"), ("enter", "select"), ("esc", "cancel")],
            );
        }
        PopupState::ThemePicker { core, .. } => {
            render_list_picker(
                frame, area, theme, core,
                "Color Theme",
                65, 70, 36,
                &[("↑↓", "preview"), ("type", "filter"), ("enter", "apply"), ("esc", "cancel")],
            );
        }
        PopupState::None => {}
    }

    // Overlay the ✦ AI-generate button (and tooltip when hovered) inside the
    // commit-message popup, on the Summary label row.
    if let Some(btn_rect) = commit_ai_button_geometry(popup, area) {
        let buf = frame.buffer_mut();
        // No bg fill — ✦ has off-center bearings in most fonts, so any colored
        // block exposes the asymmetry. Color/weight change is the hover feedback.
        let glyph_style = if ai_configured {
            if ai_button_hovered {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(theme.accent_secondary)
                    .add_modifier(Modifier::BOLD)
            }
        } else if ai_button_hovered {
            Style::default()
                .fg(theme.text)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_dimmed)
        };
        let glyph_col = btn_rect.x + 1;
        if let Some(cell) = buf.cell_mut((glyph_col, btn_rect.y)) {
            cell.set_char('\u{F0674}'); // Nerd Font: nf-md-creation (sparkle)
            cell.set_style(glyph_style);
        }

        if ai_button_hovered {
            let tip_style = Style::default()
                .bg(theme.selected_bg)
                .fg(theme.text_strong);
            let key_style = Style::default()
                .bg(theme.selected_bg)
                .fg(theme.accent_secondary)
                .add_modifier(Modifier::BOLD);
            let dim_style = Style::default()
                .bg(theme.selected_bg)
                .fg(theme.text_dimmed);

            let parts: Vec<(&str, Style)> = if ai_configured {
                vec![
                    (" ", tip_style),
                    ("c-g", key_style),
                    (" Generate w/ AI ", tip_style),
                ]
            } else {
                vec![
                    (" \u{F0674} Generate w/ AI ", tip_style),
                    ("(needs setup)", dim_style),
                    (" ", tip_style),
                ]
            };
            let tip_w: u16 = parts.iter().map(|(s, _)| s.chars().count() as u16).sum();
            let tip_rect = commit_ai_tooltip_rect(area, btn_rect, tip_w);

            let mut col = tip_rect.x;
            for (text, style) in &parts {
                for ch in text.chars() {
                    if col >= tip_rect.x + tip_rect.width {
                        break;
                    }
                    if col >= buf.area.x + buf.area.width {
                        break;
                    }
                    if let Some(cell) = buf.cell_mut((col, tip_rect.y)) {
                        cell.set_char(ch);
                        cell.set_style(*style);
                    }
                    col += 1;
                }
            }
        }
    }
}

use super::popup::ListPickerCore;

/// Shared rendering for searchable list picker popups (RefPicker, ThemePicker, etc.).
fn render_list_picker(
    frame: &mut Frame,
    area: Rect,
    theme: &Theme,
    core: &ListPickerCore,
    title: &str,
    width_pct: u16,
    width_max: u16,
    width_min: u16,
    hints: &[(&str, &str)],
) {
    let search = core.search_textarea.lines().join("");
    let search_lower = search.to_lowercase();

    // Build display rows: interleave category headers with items
    let has_categories = core.items.iter().any(|i| !i.category.is_empty());
    let mut display: Vec<(bool, String)> = Vec::new(); // (is_header, label)
    if has_categories {
        let mut last_cat = String::new();
        for item in core.items.iter() {
            if !item.category.is_empty() && item.category != last_cat {
                display.push((true, item.category.clone()));
                last_cat = item.category.clone();
            }
            display.push((false, item.label.clone()));
        }
    } else {
        for item in core.items.iter() {
            display.push((false, item.label.clone()));
        }
    }

    // Popup frame
    let popup_width = (area.width * width_pct / 100)
        .min(width_max)
        .max(width_min)
        .min(area.width);
    let max_popup = (area.height * 60 / 100).max(10);
    let popup_height = max_popup.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_rect = Rect::new(x, y, popup_width, popup_height);
    frame.render_widget(Clear, popup_rect);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
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
        Style::default().fg(theme.text_dimmed)
    } else {
        Style::default().fg(theme.accent_secondary)
    };
    frame.render_widget(
        Paragraph::new(Span::styled("  ", prefix_style)),
        prefix_rect,
    );
    let ta_width = inner.width.saturating_sub(prefix_width);
    let ta_rect = Rect::new(inner.x + prefix_width, inner.y, ta_width, 1);
    frame.render_widget(&core.search_textarea, ta_rect);

    // Separator
    let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
    let sep = "─".repeat(inner.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(sep, Style::default().fg(theme.text_dimmed))),
        sep_area,
    );

    // Content area
    let list_start = inner.y + 2;
    let list_height = inner.height.saturating_sub(3) as usize; // search + sep + hint
    let list_area = Rect::new(inner.x, list_start, inner.width, list_height as u16);

    let max_scroll = display.len().saturating_sub(list_height);
    let effective_scroll = core.scroll_offset.min(max_scroll);

    let visible_display: Vec<&(bool, String)> = display
        .iter()
        .skip(effective_scroll)
        .take(list_height)
        .collect();

    // Count how many non-header items are before the visible window
    let mut entry_idx = 0usize;
    for (is_header, _) in display.iter().take(effective_scroll) {
        if !is_header {
            entry_idx += 1;
        }
    }

    let mut list_items: Vec<ListItem> = Vec::new();
    for (is_header, label) in visible_display {
        if *is_header {
            let line = Line::from(vec![Span::styled(
                format!(" {} ", label),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )]);
            list_items.push(ListItem::new(line));
        } else {
            let is_selected = entry_idx == core.selected;
            entry_idx += 1;

            let base_fg = if is_selected { theme.text_strong } else { theme.text };
            let highlight_fg = theme.accent_secondary;

            // ▸ marker for selected item
            let marker = if is_selected { "▸ " } else { "  " };
            let mut spans = vec![Span::styled(marker, Style::default().fg(theme.accent_secondary))];

            // Build label spans with search match highlighting
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
                list_items.push(ListItem::new(line).style(Style::default().bg(theme.selected_bg)));
            } else {
                list_items.push(ListItem::new(line));
            }
        }
    }

    let list = List::new(list_items);
    frame.render_widget(list, list_area);

    // Hint bar
    let hint_area = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
    let mut hint_spans = Vec::new();
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i == 0 {
            hint_spans.push(Span::styled(format!(" {}", key), Style::default().fg(theme.accent_secondary)));
        } else {
            hint_spans.push(Span::styled(key.to_string(), Style::default().fg(theme.accent_secondary)));
        }
        hint_spans.push(Span::styled(format!(": {}  ", desc), Style::default().fg(theme.text_dimmed)));
    }
    frame.render_widget(Paragraph::new(Line::from(hint_spans)), hint_area);
}

/// Resolve the commit to display in the details panel based on the active
/// context.  Returns `None` when the context isn't commit-listing or nothing
/// is selected.  For `CommitFiles`/`BranchCommitFiles`/`StashFiles` we look
/// up the commit by the drilled-in hash.
fn resolve_current_commit<'a>(
    model: &'a Model,
    ctx_mgr: &ContextManager,
    commit_files_hash: &str,
) -> Option<&'a Commit> {
    let ctx = ctx_mgr.active();
    let sel = ctx_mgr.selected(ctx);
    match ctx {
        ContextId::Commits => model.commits.get(sel),
        ContextId::BranchCommits => model.sub_commits.get(sel),
        ContextId::Reflog => model.reflog_commits.get(sel),
        ContextId::CommitFiles | ContextId::BranchCommitFiles | ContextId::StashFiles => {
            if commit_files_hash.is_empty() {
                return None;
            }
            find_commit_by_hash(model, commit_files_hash)
        }
        _ => None,
    }
}

fn find_commit_by_hash<'a>(model: &'a Model, hash: &str) -> Option<&'a Commit> {
    model
        .commits
        .iter()
        .find(|c| c.hash == hash)
        .or_else(|| model.sub_commits.iter().find(|c| c.hash == hash))
        .or_else(|| model.reflog_commits.iter().find(|c| c.hash == hash))
}

/// Look up a cached shortstat, spawning a background worker to fetch it if
/// missing.  Render never blocks on git — the first frame for a new commit
/// shows no stat line; the next repaint after the worker finishes will pick
/// it up from the shared cache.
fn lookup_or_fetch_stat(
    cache: &Arc<Mutex<HashMap<String, CommitStat>>>,
    inflight: &Arc<Mutex<std::collections::HashSet<String>>>,
    git: &Arc<GitCommands>,
    hash: &str,
) -> Option<CommitStat> {
    if let Ok(map) = cache.lock() {
        if let Some(s) = map.get(hash) {
            return Some(*s);
        }
    }
    // Not cached: schedule a background fetch (once) and return None for now.
    let mut inflight_guard = match inflight.lock() {
        Ok(g) => g,
        Err(_) => return None,
    };
    if inflight_guard.contains(hash) {
        return None;
    }
    inflight_guard.insert(hash.to_string());
    drop(inflight_guard);

    let cache = Arc::clone(cache);
    let inflight = Arc::clone(inflight);
    let git = Arc::clone(git);
    let hash_owned = hash.to_string();
    std::thread::spawn(move || {
        // Only cache on success.  Errors leave the entry absent so a future
        // visit can retry; in-flight guard still prevents same-frame spam.
        if let Ok(stat) = git.commit_stat(&hash_owned) {
            if let Ok(mut map) = cache.lock() {
                map.insert(hash_owned.clone(), stat);
            }
        }
        if let Ok(mut set) = inflight.lock() {
            set.remove(&hash_owned);
        }
    });
    None
}

fn render_commit_details_panel(
    frame: &mut Frame,
    rect: Rect,
    commit: &Commit,
    commit_stats: &Arc<Mutex<HashMap<String, CommitStat>>>,
    commit_stats_inflight: &Arc<Mutex<std::collections::HashSet<String>>>,
    commit_messages: &Arc<Mutex<HashMap<String, String>>>,
    commit_messages_inflight: &Arc<Mutex<std::collections::HashSet<String>>>,
    git: &Arc<GitCommands>,
    theme: &Theme,
    compact: bool,
    scroll: &mut u16,
) {
    let stat_owned = lookup_or_fetch_stat(commit_stats, commit_stats_inflight, git, &commit.hash);
    let message_owned =
        lookup_or_fetch_message(commit_messages, commit_messages_inflight, git, &commit.hash);
    presentation::commit_details::render_commit_details(
        frame,
        rect,
        commit,
        stat_owned.as_ref(),
        message_owned.as_deref(),
        theme,
        compact,
        scroll,
    );
}

fn lookup_or_fetch_message(
    cache: &Arc<Mutex<HashMap<String, String>>>,
    inflight: &Arc<Mutex<std::collections::HashSet<String>>>,
    git: &Arc<GitCommands>,
    hash: &str,
) -> Option<String> {
    if let Ok(map) = cache.lock() {
        if let Some(m) = map.get(hash) {
            return Some(m.clone());
        }
    }
    let mut inflight_guard = match inflight.lock() {
        Ok(g) => g,
        Err(_) => return None,
    };
    if inflight_guard.contains(hash) {
        return None;
    }
    inflight_guard.insert(hash.to_string());
    drop(inflight_guard);

    let cache = Arc::clone(cache);
    let inflight = Arc::clone(inflight);
    let git = Arc::clone(git);
    let hash_owned = hash.to_string();
    std::thread::spawn(move || {
        if let Ok(msg) = git.commit_message_full(&hash_owned) {
            if let Ok(mut map) = cache.lock() {
                map.insert(hash_owned.clone(), msg);
            }
        }
        if let Ok(mut set) = inflight.lock() {
            set.remove(&hash_owned);
        }
    });
    None
}
