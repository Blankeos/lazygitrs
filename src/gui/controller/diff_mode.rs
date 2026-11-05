use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::matches_key;
use crate::gui::modes::diff_mode::{CompareDiffSource, DiffModeFocus, DiffModeSelector};
use crate::gui::popup::{CommandEntry, CommandSection, MenuItem, PopupState};
use crate::gui::{DiffPayload, Gui, textarea_input};
use crate::model::FileChangeStatus;
use crate::os::platform::Platform;
use crate::pager::side_by_side::{DiffPanelLayout, DiffViewState};

fn handle_commits_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    let selected = gui.diff_mode.commits_selected;
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if selected + 1 >= gui.diff_mode.commits.len() {
                load_more_commits(gui)?;
            }
            gui.diff_mode.commits_selected =
                (selected + 1).min(gui.diff_mode.commits.len().saturating_sub(1));
        }
        KeyCode::Char('k') | KeyCode::Up => {
            gui.diff_mode.commits_selected = selected.saturating_sub(1)
        }
        KeyCode::PageDown => {
            if selected + 10 >= gui.diff_mode.commits.len() {
                load_more_commits(gui)?;
            }
            gui.diff_mode.commits_selected =
                (selected + 10).min(gui.diff_mode.commits.len().saturating_sub(1));
        }
        KeyCode::PageUp => gui.diff_mode.commits_selected = selected.saturating_sub(10),
        KeyCode::Char('g') | KeyCode::Home => gui.diff_mode.commits_selected = 0,
        KeyCode::Char('G') | KeyCode::End => {
            gui.diff_mode.commits_selected = gui.diff_mode.commits.len().saturating_sub(1);
        }
        KeyCode::Enter => return open_selected_commit_files(gui),
        KeyCode::Char('y') => {
            if let Some(commit) = gui.diff_mode.selected_commit() {
                Platform::copy_to_clipboard(&commit.hash)?;
            }
        }
        _ => return Ok(()),
    }
    gui.diff_mode.commits_viewport_manually_scrolled = false;
    gui.needs_diff_refresh = true;
    Ok(())
}

pub fn load_more_commits(gui: &mut Gui) -> Result<()> {
    if !gui.diff_mode.has_more_commits() {
        return Ok(());
    }
    let commits = gui.git.load_compare_commits(
        &gui.diff_mode.ref_a,
        &gui.diff_mode.ref_b,
        crate::git::DEFAULT_COMMIT_LIMIT,
        gui.diff_mode.commits.len(),
    )?;
    gui.diff_mode.commits.extend(commits);
    gui.diff_mode.commits_revision = gui.diff_mode.commits_revision.wrapping_add(1);
    Ok(())
}

pub fn open_selected_commit_files(gui: &mut Gui) -> Result<()> {
    let Some(hash) = gui
        .diff_mode
        .selected_commit()
        .map(|commit| commit.hash.clone())
    else {
        return Ok(());
    };
    let files = gui.git.commit_files(&hash)?;
    set_diff_files(gui, files);
    gui.diff_mode.files_commit = Some(hash);
    gui.diff_mode.set_focus(DiffModeFocus::CommitFiles);
    gui.clear_diff_view();
    gui.needs_diff_refresh = true;
    gui.needs_diff_refresh = true;
    Ok(())
}

pub fn restore_comparison_files(gui: &mut Gui) -> Result<()> {
    let files = gui
        .git
        .diff_refs_files(&gui.diff_mode.ref_a, &gui.diff_mode.ref_b)?;
    set_diff_files(gui, files);
    gui.diff_mode.files_commit = None;
    gui.diff_mode.set_focus(DiffModeFocus::CommitFiles);
    gui.clear_diff_view();
    Ok(())
}

fn set_diff_files(gui: &mut Gui, files: Vec<crate::model::CommitFile>) {
    gui.diff_mode.diff_files = files;
    gui.diff_mode.diff_files_selected = 0;
    gui.diff_mode.diff_files_scroll = 0;
    gui.diff_mode.viewport_manually_scrolled = false;
    gui.diff_mode.collapsed_dirs.clear();
    gui.diff_mode.clear_list_search();
    update_diff_mode_tree(gui);
}

pub fn handle_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    // Popup takes priority (for ? help)
    if gui.popup != PopupState::None {
        return gui.handle_popup_key(key);
    }

    // If editing a combobox, route to combobox input handler
    if gui.diff_mode.editing.is_some() {
        return handle_combobox_key(gui, key);
    }

    // File search input mode takes priority
    if gui.diff_mode.file_search_active {
        return handle_file_search_key(gui, key);
    }

    // Keep all text entry local before dispatching configurable shortcuts.
    if gui.diff_mode.focus == DiffModeFocus::DiffExploration && gui.diff_view.search_active {
        return handle_diff_search_key(gui, key);
    }

    if super::custom_commands::try_handle_prompt_key(gui, key)? {
        return Ok(());
    }

    // Ctrl-F: same-context grep dialog over all hunk contents.
    if super::diff_grep::is_diff_grep_key(key) {
        return super::diff_grep::open_diff_grep_picker(gui);
    }

    // q to exit diff mode
    if key.code == KeyCode::Char('q') {
        gui.diff_mode.exit();
        return Ok(());
    }

    // ? to show help
    if key.code == KeyCode::Char('?') {
        show_diff_mode_command_palette(gui);
        return Ok(());
    }

    if gui.try_handle_layout_key(key) {
        return Ok(());
    }
    let keybindings = &gui.config.user_config.keybinding;

    if matches_key(key, &keybindings.universal.toggle_diff_view_layout) {
        gui.diff_view.toggle_view_layout();
        gui.persist_diff_view_layout();
        return Ok(());
    }

    // Start file search (/) — only when NOT focused on diff exploration
    // (diff exploration handles / for its own content search)
    if gui.diff_mode.focus != DiffModeFocus::DiffExploration
        && matches_key(key, &keybindings.universal.start_search)
    {
        gui.diff_mode.file_search_active = true;
        gui.diff_mode.file_search_query.clear();
        gui.diff_mode.file_search_matches.clear();
        gui.diff_mode.file_search_match_idx = 0;
        let mut ta = tui_textarea::TextArea::default();
        ta.set_cursor_line_style(ratatui::style::Style::default());
        gui.diff_mode.file_search_textarea = Some(ta);
        return Ok(());
    }

    // n/N to navigate file search matches, Esc to dismiss file search
    // (skipped when diff exploration is focused — it has its own search)
    if !gui.diff_mode.file_search_query.is_empty()
        && gui.diff_mode.focus != DiffModeFocus::DiffExploration
    {
        if key.code == KeyCode::Esc {
            gui.diff_mode.file_search_query.clear();
            gui.diff_mode.file_search_matches.clear();
            gui.diff_mode.file_search_match_idx = 0;
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.next_match) {
            gui.diff_mode.goto_next_file_search_match();
            gui.needs_diff_refresh = true;
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.prev_match) {
            gui.diff_mode.goto_prev_file_search_match();
            gui.needs_diff_refresh = true;
            return Ok(());
        }
    }

    // Use the same configurable forward/reverse bindings as the main view.
    let mut panel_key = key;
    if panel_key.code == KeyCode::BackTab
        || (panel_key.code == KeyCode::Tab && panel_key.modifiers.contains(KeyModifiers::SHIFT))
    {
        panel_key.code = KeyCode::BackTab;
        panel_key.modifiers.insert(KeyModifiers::SHIFT);
    }
    let forward = matches_key(panel_key, &keybindings.universal.toggle_panel);
    let reverse = matches_key(panel_key, &keybindings.universal.toggle_panel_reverse);
    if forward || reverse {
        let focus = if reverse {
            gui.diff_mode.focus.prev()
        } else {
            gui.diff_mode.focus.next()
        };
        gui.diff_mode.set_focus(focus);
        gui.needs_diff_refresh = true;
        return Ok(());
    }

    // Number keys 1-5 to jump to focus panel
    if let KeyCode::Char(c @ '1'..='5') = key.code {
        if let Some(focus) = DiffModeFocus::from_number(c.to_digit(10).unwrap()) {
            gui.diff_mode.set_focus(focus);
            gui.needs_diff_refresh = true;
            return Ok(());
        }
    }

    // Ctrl+S to swap refs
    if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
        gui.diff_mode.swap_refs();
        if gui.diff_mode.has_both_refs() {
            reload_diff_files(gui)?;
        }
        gui.needs_diff_refresh = true;
        return Ok(());
    }

    // Focus-specific keys
    match gui.diff_mode.focus {
        DiffModeFocus::SelectorA => {
            if key.code == KeyCode::Enter {
                gui.diff_mode.start_editing(DiffModeSelector::A);
                let model = gui.model.lock().unwrap();
                gui.diff_mode.search_refs(
                    &model.branches,
                    &model.tags,
                    &model.commits,
                    &model.remotes,
                    &model.head_branch_name,
                );
            }
        }
        DiffModeFocus::SelectorB => {
            if key.code == KeyCode::Enter {
                gui.diff_mode.start_editing(DiffModeSelector::B);
                let model = gui.model.lock().unwrap();
                gui.diff_mode.search_refs(
                    &model.branches,
                    &model.tags,
                    &model.commits,
                    &model.remotes,
                    &model.head_branch_name,
                );
            }
        }
        DiffModeFocus::CommitFiles => {
            handle_commit_files_key(gui, key)?;
        }
        DiffModeFocus::Commits => {
            handle_commits_key(gui, key)?;
        }
        DiffModeFocus::DiffExploration => {
            handle_diff_exploration_key(gui, key)?;
        }
    }

    Ok(())
}

fn handle_combobox_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            gui.diff_mode.cancel_editing();
        }
        KeyCode::Enter => {
            gui.diff_mode.confirm_selection();
            if gui.diff_mode.has_both_refs() {
                reload_diff_files(gui)?;
                // Both refs set — auto-focus commit files
                gui.diff_mode.set_focus(DiffModeFocus::CommitFiles);
            } else if gui.diff_mode.ref_a.is_empty() {
                // B was just set, A still empty — jump to A and start editing
                gui.diff_mode.set_focus(DiffModeFocus::SelectorA);
                gui.diff_mode.start_editing(DiffModeSelector::A);
                let model = gui.model.lock().unwrap();
                gui.diff_mode.search_refs(
                    &model.branches,
                    &model.tags,
                    &model.commits,
                    &model.remotes,
                    &model.head_branch_name,
                );
            } else {
                // A was just set, B still empty — jump to B and start editing
                gui.diff_mode.set_focus(DiffModeFocus::SelectorB);
                gui.diff_mode.start_editing(DiffModeSelector::B);
                let model = gui.model.lock().unwrap();
                gui.diff_mode.search_refs(
                    &model.branches,
                    &model.tags,
                    &model.commits,
                    &model.remotes,
                    &model.head_branch_name,
                );
            }
            gui.needs_diff_refresh = true;
        }
        KeyCode::Up => {
            if gui.diff_mode.search_selected > 0 {
                gui.diff_mode.search_selected -= 1;
                gui.diff_mode.ensure_dropdown_visible(10);
            }
        }
        KeyCode::Down => {
            let len = gui.diff_mode.search_results.len();
            if len > 0 && gui.diff_mode.search_selected < len - 1 {
                gui.diff_mode.search_selected += 1;
                gui.diff_mode.ensure_dropdown_visible(10);
            }
        }
        _ => {
            // Forward all other keys to the textarea (handles Backspace, Opt+Backspace, etc.)
            if let Some(ref mut ta) = gui.diff_mode.textarea {
                textarea_input(ta, key);
            }
            // Re-search after any text change
            let model = gui.model.lock().unwrap();
            gui.diff_mode.search_refs(
                &model.branches,
                &model.tags,
                &model.commits,
                &model.remotes,
                &model.head_branch_name,
            );
        }
    }
    Ok(())
}

fn handle_file_search_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    if let Some(ref mut ta) = gui.diff_mode.file_search_textarea {
        match key.code {
            KeyCode::Esc => {
                gui.diff_mode.file_search_active = false;
                gui.diff_mode.file_search_query.clear();
                gui.diff_mode.file_search_matches.clear();
                gui.diff_mode.file_search_match_idx = 0;
                gui.diff_mode.file_search_textarea = None;
            }
            KeyCode::Enter => {
                gui.diff_mode.file_search_active = false;
                // Jump to first match
                if !gui.diff_mode.file_search_matches.is_empty() {
                    gui.diff_mode.file_search_match_idx = 0;
                    gui.diff_mode
                        .select_list_match(gui.diff_mode.file_search_matches[0]);
                }
                gui.diff_mode.file_search_textarea = None;
                gui.needs_diff_refresh = true;
            }
            _ => {
                textarea_input(ta, key);
                gui.diff_mode.file_search_query = ta.lines().join("");
                gui.diff_mode.update_file_search_matches();
                gui.needs_diff_refresh = true;
            }
        }
    }
    Ok(())
}

fn handle_tree_navigation(gui: &mut Gui, key: KeyEvent) -> bool {
    if !gui.diff_mode.show_tree {
        return false;
    }
    if let Some(destination) = super::tree::destination(
        key,
        &gui.config.user_config.keybinding,
        &gui.diff_mode.tree_nodes,
        gui.diff_mode.diff_files_selected,
    ) {
        if let Some(idx) = destination {
            gui.diff_mode.diff_files_selected = idx;
            gui.diff_mode.viewport_manually_scrolled = false;
            gui.needs_diff_refresh = true;
        }
        return true;
    }
    false
}

fn handle_commit_files_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    if key.code == KeyCode::Esc && gui.diff_mode.files_commit.is_some() {
        restore_comparison_files(gui)?;
        gui.needs_diff_refresh = true;
        return Ok(());
    }
    let keybindings = &gui.config.user_config.keybinding;

    // Toggle tree view (backtick) — keep in sync with Files / Commit Files and persist
    if matches_key(key, &keybindings.files.toggle_tree_view) {
        gui.diff_mode.show_tree = !gui.diff_mode.show_tree;
        gui.show_file_tree = gui.diff_mode.show_tree;
        gui.show_commit_file_tree = gui.diff_mode.show_tree;
        gui.update_file_tree_state();
        gui.persist_file_tree_visibility();
        update_diff_mode_tree(gui);
        gui.diff_mode.diff_files_selected = 0;
        return Ok(());
    }

    if gui.diff_mode.show_tree && matches_key(key, &keybindings.universal.fold_directory) {
        if let Some(node) = gui
            .diff_mode
            .tree_nodes
            .get(gui.diff_mode.diff_files_selected)
        {
            if node.is_dir {
                let path = node.path.clone();
                if !gui.diff_mode.collapsed_dirs.remove(&path) {
                    gui.diff_mode.collapsed_dirs.insert(path);
                }
                update_diff_mode_tree(gui);
                gui.diff_mode.viewport_manually_scrolled = false;
                gui.needs_diff_refresh = true;
            }
        }
        return Ok(());
    }
    if handle_tree_navigation(gui, key) {
        return Ok(());
    }

    let len = gui.diff_mode.visible_files_len();
    if len == 0 {
        return Ok(());
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            if gui.diff_mode.diff_files_selected < len - 1 {
                gui.diff_mode.diff_files_selected += 1;
                gui.diff_mode.viewport_manually_scrolled = false;
                gui.needs_diff_refresh = true;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if gui.diff_mode.diff_files_selected > 0 {
                gui.diff_mode.diff_files_selected -= 1;
                gui.diff_mode.viewport_manually_scrolled = false;
                gui.needs_diff_refresh = true;
            }
        }
        KeyCode::Enter => {
            gui.diff_mode.set_focus(DiffModeFocus::DiffExploration);
            gui.needs_diff_refresh = true;
        }
        KeyCode::Char('g') => {
            gui.diff_mode.diff_files_selected = 0;
            gui.diff_mode.viewport_manually_scrolled = false;
            gui.needs_diff_refresh = true;
        }
        KeyCode::Char('G') => {
            gui.diff_mode.diff_files_selected = len.saturating_sub(1);
            gui.diff_mode.viewport_manually_scrolled = false;
            gui.needs_diff_refresh = true;
        }
        KeyCode::Char('y') => {
            return show_commit_file_copy_menu(gui);
        }
        _ => {}
    }
    Ok(())
}

fn show_commit_file_copy_menu(gui: &mut Gui) -> Result<()> {
    // Resolve file index (tree view maps node -> file index)
    let selected = gui.diff_mode.diff_files_selected;
    let file_idx = if gui.diff_mode.show_tree {
        gui.diff_mode
            .tree_nodes
            .get(selected)
            .and_then(|n| n.file_index)
    } else {
        Some(selected)
    };

    let Some(idx) = file_idx else { return Ok(()) };
    let Some(file) = gui.diff_mode.diff_files.get(idx) else {
        return Ok(());
    };

    let file_name = file.name.clone();
    let old_path = file
        .rename_paths()
        .map_or_else(|| file.name.clone(), |(old, _)| old.to_string());
    let new_path = file.current_path().to_string();
    let status = file.status;
    let (ref_a, ref_b) = match &gui.diff_mode.files_commit {
        Some(hash) => (format!("{hash}^1"), hash.clone()),
        None => (gui.diff_mode.ref_a.clone(), gui.diff_mode.ref_b.clone()),
    };
    let commit_for_diff = gui.diff_mode.files_commit.clone();
    let path_for_old = old_path.clone();
    let path_for_new = new_path.clone();
    let path_for_diff = file_name.clone();

    // Added files have no old content, Deleted files have no new content
    let has_old = !matches!(status, FileChangeStatus::Added);
    let has_new = !matches!(status, FileChangeStatus::Deleted);

    let ref_a_for_old = ref_a.clone();
    let ref_b_for_new = ref_b.clone();
    let ref_a_for_diff = ref_a.clone();
    let ref_b_for_diff = ref_b.clone();

    gui.popup = PopupState::Menu {
        title: "Copy to clipboard".to_string(),
        items: vec![
            MenuItem {
                label: "File name".to_string(),
                description: String::new(),
                key: Some("n".to_string()),
                action: Some(Box::new(move |_gui| {
                    Platform::copy_to_clipboard(&file_name)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: if commit_for_diff.is_some() {
                    "Old content (parent)"
                } else {
                    "Old content (from A)"
                }
                .to_string(),
                description: if has_old {
                    String::new()
                } else {
                    "File was added — no old content".to_string()
                },
                key: Some("o".to_string()),
                action: if has_old {
                    Some(Box::new(move |gui| {
                        let content = gui
                            .git
                            .file_content_at_commit(&ref_a_for_old, &path_for_old)?;
                        Platform::copy_to_clipboard(&content)?;
                        Ok(())
                    }))
                } else {
                    None
                },
            },
            MenuItem {
                label: if commit_for_diff.is_some() {
                    "New content (commit)"
                } else {
                    "New content (from B)"
                }
                .to_string(),
                description: if has_new {
                    String::new()
                } else {
                    "File was deleted — no new content".to_string()
                },
                key: Some("w".to_string()),
                action: if has_new {
                    Some(Box::new(move |gui| {
                        let content = gui
                            .git
                            .file_content_at_commit(&ref_b_for_new, &path_for_new)?;
                        Platform::copy_to_clipboard(&content)?;
                        Ok(())
                    }))
                } else {
                    None
                },
            },
            MenuItem {
                label: "Diff".to_string(),
                description: String::new(),
                key: Some("d".to_string()),
                action: Some(Box::new(move |gui| {
                    let diff = if let Some(hash) = &commit_for_diff {
                        gui.git.diff_commit_file(hash, &path_for_diff)?
                    } else {
                        gui.git
                            .diff_refs_file(&ref_a_for_diff, &ref_b_for_diff, &path_for_diff)?
                    };
                    Platform::copy_to_clipboard(&diff)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Cancel".to_string(),
                description: String::new(),
                key: None,
                action: Some(Box::new(|_| Ok(()))),
            },
        ],
        selected: 0,
        loading_index: None,
    };
    Ok(())
}

fn handle_diff_search_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    if let Some(ref mut ta) = gui.diff_view.search_textarea {
        match key.code {
            KeyCode::Esc => {
                gui.diff_view.dismiss_search();
            }
            KeyCode::Enter => {
                gui.diff_view.dismiss_search();
                // Jump to first match
                if !gui.diff_view.search_matches.is_empty() {
                    gui.diff_view.search_match_idx = 0;
                    gui.diff_view.scroll_to_current_match();
                }
            }
            _ => {
                textarea_input(ta, key);
                gui.diff_view.search_query = ta.lines().join("");
                gui.diff_view.update_search();
            }
        }
    }
    Ok(())
}

fn handle_diff_exploration_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    // Diff search input mode takes priority
    if gui.diff_view.search_active {
        return handle_diff_search_key(gui, key);
    }

    // Handle text selection keys first (y to copy, e to edit, Esc to dismiss)
    if gui.diff_view.selection.is_some() {
        let is_click = gui.diff_view.selection.as_ref().unwrap().is_click;
        let can_edit = gui.diff_view.file_exists_on_disk;
        match key.code {
            KeyCode::Char('e') if can_edit => {
                let sel_ref = gui.diff_view.selection.as_ref().unwrap();
                let line = sel_ref.edit_line_number;
                // Compute column from terminal position using the same layout as the mouse handler
                let (top_row, top_col, _, _) = sel_ref.normalized();
                let diff_rect = gui.compute_compare_layout().diff;
                let pl = DiffPanelLayout::compute(diff_rect, &gui.diff_view);
                let (content_start, _) = pl.content_range(sel_ref.panel);
                let column = if top_col >= content_start {
                    (top_col - content_start) as usize + gui.diff_view.horizontal_scroll + 1
                } else {
                    1
                };
                // Resolve the actual filename for multi-file diffs
                let (line_idx, line_panel) = if top_row >= pl.inner_y {
                    gui.diff_view
                        .line_chunk_panel_at_row(top_row, &pl, sel_ref.panel)
                        .map(|(line_idx, _, panel)| (line_idx, panel))
                        .unwrap_or_else(|| {
                            (
                                gui.diff_view.fallback_line_idx_for_row(top_row, &pl),
                                sel_ref.panel,
                            )
                        })
                } else {
                    (0, sel_ref.panel)
                };
                let filename = gui.diff_view.file_at_line(line_idx).to_string();
                gui.diff_view.selection = None;
                let abs_path = gui.git.repo_path().join(&filename);
                if !filename.is_empty() && abs_path.exists() {
                    let abs_path = abs_path.to_string_lossy().to_string();
                    let ln = line.or_else(|| gui.diff_view.file_line_number(line_idx, line_panel));
                    if let Ok(launch) =
                        gui.config
                            .user_config
                            .os
                            .plan_edit(&abs_path, ln, Some(column))
                    {
                        let _ = gui.launch_editor(launch);
                    }
                }
                return Ok(());
            }
            KeyCode::Char('y') if !is_click => {
                let text = gui.diff_view.selection.as_ref().unwrap().text.clone();
                gui.diff_view.selection = None;
                if !text.is_empty() {
                    crate::os::platform::Platform::copy_to_clipboard(&text)?;
                }
                return Ok(());
            }
            KeyCode::Esc => {
                gui.diff_view.selection = None;
                return Ok(());
            }
            _ => {
                gui.diff_view.selection = None;
                if is_click {
                    return Ok(());
                }
            }
        }
    }

    let keybindings = &gui.config.user_config.keybinding;

    // Start diff content search (/)
    if matches_key(key, &keybindings.universal.start_search) {
        gui.diff_view.start_search();
        return Ok(());
    }

    // n/N to navigate diff search matches
    if !gui.diff_view.search_query.is_empty() {
        if matches_key(key, &keybindings.universal.next_match) {
            gui.diff_view.next_search_match();
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.prev_match) {
            gui.diff_view.prev_search_match();
            return Ok(());
        }
    }

    if handle_tree_navigation(gui, key) {
        return Ok(());
    }

    match key.code {
        KeyCode::Esc => {
            if !gui.diff_view.search_query.is_empty() {
                gui.diff_view.clear_search();
            } else {
                gui.diff_mode.set_focus(
                    if gui.diff_mode.diff_source == CompareDiffSource::Commit {
                        DiffModeFocus::Commits
                    } else {
                        DiffModeFocus::CommitFiles
                    },
                );
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            gui.diff_view.scroll_down(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            gui.diff_view.scroll_up(1);
        }
        KeyCode::Char('h') | KeyCode::Left => {
            gui.diff_view.scroll_left(4);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            gui.diff_view.scroll_right(4);
        }
        KeyCode::Char('}') => {
            gui.diff_view.next_hunk();
        }
        KeyCode::Char('{') => {
            gui.diff_view.prev_hunk();
        }
        KeyCode::Char(']') => {
            use crate::pager::side_by_side::DiffSideView;
            gui.diff_view.side_view = match gui.diff_view.side_view {
                DiffSideView::NewOnly => DiffSideView::Both,
                _ => DiffSideView::NewOnly,
            };
        }
        KeyCode::Char('[') => {
            use crate::pager::side_by_side::DiffSideView;
            gui.diff_view.side_view = match gui.diff_view.side_view {
                DiffSideView::OldOnly => DiffSideView::Both,
                _ => DiffSideView::OldOnly,
            };
        }
        KeyCode::Char('z') => {
            gui.diff_view.wrap = !gui.diff_view.wrap;
            gui.diff_view.horizontal_scroll = 0;
            gui.persist_diff_line_wrap();
        }
        KeyCode::PageDown => {
            gui.diff_view.scroll_down(20);
        }
        KeyCode::PageUp => {
            gui.diff_view.scroll_up(20);
        }
        KeyCode::Char('g') => {
            gui.diff_view.scroll_offset = 0;
        }
        KeyCode::Char('G') => {
            let max = gui.diff_view.lines.len().saturating_sub(1);
            gui.diff_view.scroll_offset = max;
        }
        _ => {}
    }
    Ok(())
}

/// Reload the file list for the current A..B diff.
pub fn reload_diff_files(gui: &mut Gui) -> Result<()> {
    reload_diff_files_with_commit_limit(gui, crate::git::DEFAULT_COMMIT_LIMIT)
}

/// Completion refreshes retain the pages the user has already loaded. Ref
/// changes still use the normal first-page limit via `reload_diff_files`.
pub(in crate::gui) fn reload_diff_files_with_commit_limit(
    gui: &mut Gui,
    commit_limit: usize,
) -> Result<()> {
    let ref_a = gui.diff_mode.ref_a.clone();
    let ref_b = gui.diff_mode.ref_b.clone();
    gui.diff_mode.ahead_behind = None;
    gui.diff_mode.clear_commits();
    if ref_a.is_empty() || ref_b.is_empty() {
        return Ok(());
    }
    // Clear the diff view since we're loading new files
    gui.clear_diff_view();

    match gui.git.diff_refs_files(&ref_a, &ref_b) {
        Ok(files) => {
            // Tree refs can still be diffed even though they have no commit history.
            gui.diff_mode.ahead_behind = gui.git.diff_refs_ahead_behind(&ref_a, &ref_b).ok();
            if gui.diff_mode.ahead_behind.is_some() {
                gui.diff_mode.commits = gui
                    .git
                    .load_compare_commits(&ref_a, &ref_b, commit_limit.max(1), 0)
                    .unwrap_or_default();
                gui.diff_mode.commits_revision = gui.diff_mode.commits_revision.wrapping_add(1);
            }
            set_diff_files(gui, files);
        }
        Err(e) => {
            gui.diff_mode.diff_files.clear();
            gui.diff_mode.tree_nodes.clear();
            gui.show_error("Diff error", e);
        }
    }
    gui.diff_mode.set_focus(gui.diff_mode.focus);
    Ok(())
}

pub(in crate::gui) fn update_diff_mode_tree(gui: &mut Gui) {
    if gui.diff_mode.show_tree {
        gui.diff_mode.tree_nodes = crate::model::file_tree::build_commit_file_tree(
            &gui.diff_mode.diff_files,
            &gui.diff_mode.collapsed_dirs,
        );
    } else {
        gui.diff_mode.tree_nodes.clear();
    }
}

/// Called from the main loop to request diff loading for the currently selected file in diff mode.
/// Queues the request on the shared latest-only diff worker.
pub fn maybe_request_diff(gui: &mut Gui, generation: u64, diff_key: String) {
    if gui.diff_mode.diff_source == CompareDiffSource::Commit {
        let Some(hash) = gui
            .diff_mode
            .selected_commit()
            .map(|commit| commit.hash.clone())
        else {
            gui.diff_loading = false;
            gui.diff_loading_since = None;
            gui.clear_diff_view();
            return;
        };
        let git = Arc::clone(&gui.git);
        gui.queue_diff_job(generation, diff_key, move || {
            let diff = git.diff_commit(&hash).unwrap_or_default();
            if diff.is_empty() {
                DiffPayload::Empty
            } else {
                DiffPayload::Parsed(DiffViewState::parse_diff_output(
                    &hash[..8.min(hash.len())],
                    &diff,
                    4,
                    false,
                ))
            }
        });
        return;
    }
    if !gui.diff_mode.has_both_refs() || gui.diff_mode.diff_files.is_empty() {
        gui.diff_loading = false;
        gui.diff_loading_since = None;
        gui.clear_diff_view();
        return;
    }

    let ref_a = gui.diff_mode.ref_a.clone();
    let ref_b = gui.diff_mode.ref_b.clone();

    let files_commit = gui.diff_mode.files_commit.clone();

    // Resolve file index (tree view maps node -> file index)
    let selected = gui.diff_mode.diff_files_selected;
    let file_idx = if gui.diff_mode.show_tree {
        gui.diff_mode
            .tree_nodes
            .get(selected)
            .and_then(|n| n.file_index)
    } else {
        Some(selected)
    };

    let git = Arc::clone(&gui.git);
    let gen_counter = Arc::clone(&gui.diff_generation);

    if let Some(idx) = file_idx {
        // Single file diff
        let Some(file) = gui.diff_mode.diff_files.get(idx) else {
            gui.diff_loading = false;
            gui.diff_loading_since = None;
            gui.clear_diff_view();
            return;
        };
        let name = file.name.clone();
        let current_path = file.current_path().to_string();

        gui.queue_diff_job(generation, diff_key, move || {
            let result = if let Some(hash) = &files_commit {
                git.diff_commit_file(hash, &name)
            } else {
                git.diff_refs_file(&ref_a, &ref_b, &name)
            };
            match result {
                Ok(diff) if diff.is_empty() => DiffPayload::Empty,
                Ok(diff) => {
                    let exists = git.repo_path().join(&current_path).exists();
                    use crate::pager::image_preview::ImageSource;
                    let paths = crate::git::diff::diff_paths_for_label(&name);
                    if let Some(parsed) = super::super::image_diff_payload(
                        &git,
                        &name,
                        &diff,
                        exists,
                        ImageSource::Revision {
                            revision: &ref_a,
                            path: paths[0],
                        },
                        ImageSource::Revision {
                            revision: &ref_b,
                            path: &current_path,
                        },
                    ) {
                        return DiffPayload::Parsed(parsed);
                    }
                    // Pure renames between refs: show file content at ref_b.
                    if crate::pager::side_by_side::is_rename_only_diff(&diff) {
                        if let Ok(content) = git.file_content_at_commit(
                            files_commit.as_deref().unwrap_or(&ref_b),
                            &current_path,
                        ) {
                            if !content.is_empty() {
                                return DiffPayload::Parsed(DiffViewState::parse_content(
                                    &current_path,
                                    &content,
                                    &content,
                                    4,
                                    exists,
                                ));
                            }
                        }
                    }
                    DiffPayload::Parsed(DiffViewState::parse_diff_output(&name, &diff, 4, exists))
                }
                Err(_) => DiffPayload::Empty,
            }
        });
    } else if gui.diff_mode.show_tree {
        // Directory node: combined diff of all child files
        if let Some(node) = gui.diff_mode.tree_nodes.get(selected) {
            if node.is_dir && !node.child_file_indices.is_empty() {
                // One pathspec-filtered `git diff A B -- dir/` instead of N× files.
                let pathspec = if node.path.is_empty() || node.path == "." {
                    None
                } else if node.path.ends_with('/') {
                    Some(node.path.clone())
                } else {
                    Some(format!("{}/", node.path))
                };
                let dir_name = node.name.clone();

                gui.queue_diff_job(generation, diff_key, move || {
                    if gen_counter.load(Ordering::Relaxed) != generation {
                        return DiffPayload::Empty;
                    }
                    let paths: Vec<&str> = match pathspec.as_deref() {
                        Some(p) => vec![p],
                        None => Vec::new(),
                    };
                    let combined_diff = if let Some(hash) = &files_commit {
                        git.diff_commit_paths(hash, &paths)
                    } else {
                        git.diff_refs_paths(&ref_a, &ref_b, &paths)
                    }
                    .unwrap_or_default();
                    if combined_diff.is_empty() {
                        DiffPayload::Empty
                    } else {
                        let mut parsed =
                            DiffViewState::parse_diff_output(&dir_name, &combined_diff, 4, true);
                        crate::pager::image_preview::attach_inline_image_previews(
                            &mut parsed,
                            git.repo_path(),
                            &combined_diff,
                            &ref_a,
                            Some(&ref_b),
                            &std::collections::HashSet::new(),
                        );
                        DiffPayload::Parsed(parsed)
                    }
                });
            } else {
                gui.diff_loading = false;
                gui.diff_loading_since = None;
                gui.clear_diff_view();
            }
        } else {
            gui.diff_loading = false;
            gui.diff_loading_since = None;
            gui.clear_diff_view();
        }
    } else {
        gui.diff_loading = false;
        gui.diff_loading_since = None;
    }
}

fn show_diff_mode_command_palette(gui: &mut Gui) {
    let mut diff_mode_section = CommandSection {
        title: "Compare / Diff Mode".into(),
        entries: vec![
            CommandEntry::keybinding("q".into(), "Exit diff mode".into()),
            CommandEntry::keybinding(
                gui.config
                    .user_config
                    .keybinding
                    .universal
                    .toggle_panel
                    .clone(),
                "Cycle focus (A → B → Files → Commits → Diff)".into(),
            ),
            CommandEntry::keybinding(
                gui.config
                    .user_config
                    .keybinding
                    .universal
                    .toggle_panel_reverse
                    .clone(),
                "Cycle focus backward".into(),
            ),
            CommandEntry::keybinding(
                gui.config
                    .user_config
                    .keybinding
                    .universal
                    .next_screen_mode
                    .clone(),
                "Next screen mode (normal / half / full)".into(),
            ),
            CommandEntry::keybinding(
                gui.config
                    .user_config
                    .keybinding
                    .universal
                    .prev_screen_mode
                    .clone(),
                "Previous screen mode".into(),
            ),
            CommandEntry::keybinding(
                format!(
                    "{} / {}",
                    gui.config
                        .user_config
                        .keybinding
                        .universal
                        .shrink_side_panel,
                    gui.config
                        .user_config
                        .keybinding
                        .universal
                        .expand_side_panel
                ),
                "Shrink / expand sidebar (or drag its divider)".into(),
            ),
            CommandEntry::keybinding(
                gui.config
                    .user_config
                    .keybinding
                    .universal
                    .reset_side_panel
                    .clone(),
                "Reset sidebar size".into(),
            ),
            CommandEntry::keybinding("1-5".into(), "Jump to panel (4 Commits, 5 Diff)".into()),
            CommandEntry::keybinding("<c-s>".into(), "Swap A and B".into()),
            CommandEntry::keybinding(
                "<enter>".into(),
                "Edit ref / Open commit files / Focus diff".into(),
            ),
            CommandEntry::keybinding(
                "<esc>".into(),
                "Back from diff / Restore comparison files".into(),
            ),
            CommandEntry::keybinding("`".into(), "Toggle file tree view".into()),
            CommandEntry::keybinding(
                "j/k".into(),
                "Navigate files or commits / Scroll diff".into(),
            ),
            CommandEntry::keybinding("PgUp/PgDn".into(), "Page through commits".into()),
            CommandEntry::keybinding("{/}".into(), "Previous / next hunk".into()),
            CommandEntry::keybinding("[/]".into(), "Toggle old / new only view".into()),
            CommandEntry::keybinding(
                gui.config
                    .user_config
                    .keybinding
                    .universal
                    .toggle_diff_view_layout
                    .clone(),
                "Toggle unified / side-by-side view".into(),
            ),
            CommandEntry::keybinding("z".into(), "Toggle line wrap".into()),
            CommandEntry::keybinding("g/G".into(), "Go to top / bottom".into()),
            CommandEntry::keybinding(
                "/".into(),
                "Search files, loaded commits, or diff content".into(),
            ),
        ],
    };
    let prompt_key = &gui
        .config
        .user_config
        .keybinding
        .universal
        .custom_command_prompt;
    if !prompt_key.trim().is_empty() {
        diff_mode_section.entries.push(CommandEntry::keybinding(
            prompt_key.clone(),
            "Execute shell command".into(),
        ));
    }
    diff_mode_section.entries.extend([
        CommandEntry::keybinding("n/N".into(), "Next / previous search match".into()),
        CommandEntry::keybinding("<c-f>".into(), "Grep diff contents".into()),
        CommandEntry::keybinding("y".into(), "Copy to clipboard".into()),
        CommandEntry::keybinding("?".into(), "Show command palette".into()),
    ]);

    let combobox_section = CommandSection {
        title: "Combobox (while editing A or B)".into(),
        entries: vec![
            CommandEntry::keybinding("<enter>".into(), "Confirm selection".into()),
            CommandEntry::keybinding("<esc>".into(), "Cancel".into()),
            CommandEntry::keybinding("Up/Down".into(), "Navigate results".into()),
            CommandEntry::keybinding(
                "Type".into(),
                "Filter branches, tags, commits, remotes".into(),
            ),
        ],
    };

    let mut sections = vec![diff_mode_section, combobox_section];
    if gui.diff_mode.show_tree
        && matches!(
            gui.diff_mode.focus,
            DiffModeFocus::CommitFiles | DiffModeFocus::DiffExploration
        )
    {
        sections.insert(
            1,
            super::tree::command_section(
                &gui.config.user_config.keybinding,
                gui.diff_mode.focus == DiffModeFocus::CommitFiles,
            ),
        );
    }
    gui.popup = PopupState::CommandPalette {
        sections,
        selected: 0,
        search_textarea: crate::gui::popup::make_command_palette_search_textarea(),
        scroll_offset: 0,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::keybindings::parse_key;
    use crate::config::{AppConfig, AppState, UserConfig};
    use crate::git::GitCommands;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct TempRepo(PathBuf);
    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn prompt_test_gui() -> (TempRepo, Gui) {
        let path = std::env::temp_dir().join(format!(
            "lazygitrs-compare-prompt-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let repo = TempRepo(path);
        let output = Command::new("git")
            .current_dir(&repo.0)
            .args(["init", "-b", "main"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        let config = AppConfig {
            debug: false,
            version: String::new(),
            user_config: UserConfig::default(),
            app_state: AppState::default(),
            config_dir: repo.0.clone(),
            state_dir: repo.0.clone(),
            state_path: repo.0.join("state.yml"),
        };
        let gui = Gui::new(config, GitCommands::new(&repo.0).unwrap(), None, false).unwrap();
        (repo, gui)
    }

    #[test]
    fn compare_shell_prompt_takes_priority_over_shortcuts_unless_disabled() {
        let (_repo, mut gui) = prompt_test_gui();
        for binding in [":", "q", "?", "/", "<c-f>", "+", "<tab>", "1"] {
            Arc::get_mut(&mut gui.config)
                .unwrap()
                .user_config
                .keybinding
                .universal
                .custom_command_prompt = binding.into();
            for focus in [
                DiffModeFocus::SelectorA,
                DiffModeFocus::SelectorB,
                DiffModeFocus::CommitFiles,
                DiffModeFocus::Commits,
                DiffModeFocus::DiffExploration,
            ] {
                gui.popup = PopupState::None;
                gui.diff_mode.enter(false);
                gui.diff_mode.set_focus(focus);
                handle_key(&mut gui, parse_key(binding).unwrap()).unwrap();
                assert!(
                    matches!(gui.popup, PopupState::ShellCommand { .. }),
                    "{binding} {focus:?}"
                );
                assert!(gui.diff_mode.active);
                assert_eq!(gui.diff_mode.focus, focus);
                assert_eq!(gui.screen_mode, crate::gui::ScreenMode::Normal);
            }
        }
        Arc::get_mut(&mut gui.config)
            .unwrap()
            .user_config
            .keybinding
            .universal
            .custom_command_prompt
            .clear();
        gui.popup = PopupState::None;
        handle_key(&mut gui, parse_key(":").unwrap()).unwrap();
        assert!(gui.popup == PopupState::None);
        handle_key(&mut gui, parse_key("q").unwrap()).unwrap();
        assert!(!gui.diff_mode.active);
    }

    #[test]
    fn compare_text_entry_takes_priority_over_shell_prompt_and_shortcuts() {
        let (_repo, mut gui) = prompt_test_gui();
        for binding in [":", "q", "?", "<c-f>"] {
            Arc::get_mut(&mut gui.config)
                .unwrap()
                .user_config
                .keybinding
                .universal
                .custom_command_prompt = binding.into();
            for input_mode in ["ref A", "ref B", "file search", "content search"] {
                gui.diff_mode.enter(false);
                gui.diff_view = DiffViewState::default();
                match input_mode {
                    "ref A" => gui.diff_mode.start_editing(DiffModeSelector::A),
                    "ref B" => gui.diff_mode.start_editing(DiffModeSelector::B),
                    "file search" => {
                        gui.diff_mode.set_focus(DiffModeFocus::CommitFiles);
                        gui.diff_mode.file_search_active = true;
                        gui.diff_mode.file_search_textarea =
                            Some(tui_textarea::TextArea::default());
                    }
                    "content search" => {
                        gui.diff_mode.set_focus(DiffModeFocus::DiffExploration);
                        gui.diff_view.start_search();
                    }
                    _ => unreachable!(),
                }
                handle_key(&mut gui, parse_key(binding).unwrap()).unwrap();
                assert!(gui.popup == PopupState::None, "{binding} {input_mode}");
                assert!(gui.diff_mode.active);
                if binding != "<c-f>" {
                    let text = match input_mode {
                        "ref A" | "ref B" => gui.diff_mode.query_text(),
                        "file search" => gui.diff_mode.file_search_query.clone(),
                        "content search" => gui.diff_view.search_query.clone(),
                        _ => unreachable!(),
                    };
                    assert_eq!(text, binding, "{input_mode}");
                }
            }
        }
    }

    #[test]
    fn compare_palette_lists_only_enabled_shell_prompt_after_text_search() {
        use crate::gui::popup::CommandAction;
        let (_repo, mut gui) = prompt_test_gui();
        for binding in [":", "<c-x>", "", "  "] {
            Arc::get_mut(&mut gui.config)
                .unwrap()
                .user_config
                .keybinding
                .universal
                .custom_command_prompt = binding.into();
            show_diff_mode_command_palette(&mut gui);
            let PopupState::CommandPalette { sections, .. } = &gui.popup else {
                panic!("expected compare command palette");
            };
            let entries = &sections[0].entries;
            let prompt = entries
                .iter()
                .position(|entry| entry.description == "Execute shell command");
            if binding.trim().is_empty() {
                assert!(prompt.is_none());
            } else {
                let prompt = prompt.expect("enabled prompt entry");
                assert_eq!(entries[prompt].key, binding);
                assert_eq!(
                    entries[prompt].action,
                    CommandAction::Dispatch(parse_key(binding).unwrap())
                );
                assert_eq!(
                    entries[prompt - 1].description,
                    "Search files, loaded commits, or diff content"
                );
            }
        }
    }

    #[test]
    fn compare_controller_previews_commits_opens_files_and_routes_mouse() {
        let path = std::env::temp_dir().join(format!(
            "lazygitrs-compare-viewer-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let repo = TempRepo(path);
        let run = |args: &[&str]| {
            let output = Command::new("git")
                .current_dir(&repo.0)
                .args([
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run(&["init", "-b", "main"]);
        std::fs::write(repo.0.join("file.txt"), "base\n").unwrap();
        run(&["add", "file.txt"]);
        run(&["commit", "-m", "root"]);
        run(&["checkout", "-b", "feature"]);
        std::fs::write(repo.0.join("file.txt"), "feature\n").unwrap();
        run(&["add", "file.txt"]);
        run(&["commit", "-m", "feature change"]);
        std::fs::write(repo.0.join("file.txt"), "base\n").unwrap();
        run(&["add", "file.txt"]);
        run(&["commit", "-m", "restore content"]);
        let config = AppConfig {
            debug: false,
            version: String::new(),
            user_config: UserConfig::default(),
            app_state: AppState::default(),
            config_dir: repo.0.clone(),
            state_dir: repo.0.clone(),
            state_path: repo.0.join("state.yml"),
        };
        let mut gui = Gui::new(config, GitCommands::new(&repo.0).unwrap(), None, false).unwrap();
        gui.diff_mode.enter(false);
        gui.diff_mode.ref_a = "main".into();
        gui.diff_mode.ref_b = "feature".into();
        reload_diff_files(&mut gui).unwrap();
        assert_eq!(gui.diff_mode.commits.len(), 2);
        assert!(gui.diff_mode.diff_files.is_empty());
        gui.diff_mode.commits.truncate(1);
        load_more_commits(&mut gui).unwrap();
        assert_eq!(gui.diff_mode.commits.len(), 2);
        assert!(!gui.diff_mode.has_more_commits());
        let press =
            |gui: &mut Gui, key| handle_key(gui, KeyEvent::new(key, KeyModifiers::NONE)).unwrap();
        press(&mut gui, KeyCode::Char('4'));
        assert_eq!(gui.diff_mode.focus, DiffModeFocus::Commits);
        let commit_key = gui.current_diff_key();
        maybe_request_diff(&mut gui, 0, commit_key.clone());
        let result = gui.diff_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(result.diff_key, commit_key);
        assert!(matches!(result.payload, DiffPayload::Parsed(_)));
        press(&mut gui, KeyCode::Char('5'));
        assert_eq!(gui.current_diff_key(), commit_key);
        press(&mut gui, KeyCode::Esc);
        assert_eq!(gui.diff_mode.focus, DiffModeFocus::Commits);
        press(&mut gui, KeyCode::Enter);
        assert_eq!(gui.diff_mode.diff_source, CompareDiffSource::CommitFiles);
        assert_eq!(gui.diff_mode.diff_files.len(), 1);
        let file_key = gui.current_diff_key();
        maybe_request_diff(&mut gui, 0, file_key.clone());
        let result = gui.diff_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(result.diff_key, file_key);
        assert!(matches!(result.payload, DiffPayload::Parsed(_)));
        press(&mut gui, KeyCode::Esc);
        assert_eq!(gui.diff_mode.diff_source, CompareDiffSource::Comparison);
        assert!(gui.diff_mode.diff_files.is_empty());

        press(&mut gui, KeyCode::Char('4'));
        handle_key(
            &mut gui,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
        )
        .unwrap();
        assert_eq!(gui.diff_mode.focus, DiffModeFocus::Commits);
        assert_eq!(gui.diff_mode.diff_source, CompareDiffSource::Commit);
        assert_eq!(gui.diff_mode.ahead_behind, Some((2, 0)));

        gui.layout.update_size(150, 24);
        let rect = gui.compute_compare_layout().sidebar[3];
        gui.handle_diff_mode_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: rect.x + 1,
            row: rect.y + 2,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(gui.diff_mode.focus, DiffModeFocus::Commits);
        assert_eq!(gui.diff_mode.commits_selected, 1);
        assert_eq!(gui.diff_mode.diff_source, CompareDiffSource::Commit);
        let commit = gui.diff_mode.commits[0].clone();
        gui.diff_mode.commits = vec![commit; 30];
        gui.diff_mode.ahead_behind = Some((30, 0));
        for kind in [
            crossterm::event::MouseEventKind::ScrollDown,
            crossterm::event::MouseEventKind::ScrollUp,
        ] {
            gui.handle_diff_mode_mouse(crossterm::event::MouseEvent {
                kind,
                column: rect.x + 1,
                row: rect.y + 2,
                modifiers: KeyModifiers::NONE,
            });
            assert_eq!(gui.diff_mode.commits_selected, 1);
            assert_eq!(
                gui.diff_mode.commits_scroll,
                if kind == crossterm::event::MouseEventKind::ScrollDown {
                    3
                } else {
                    0
                }
            );
        }
        // Exercise the real GUI dispatch path, including normalized terminal
        // events, custom bindings, input priority and divider dragging.
        use crate::gui::ScreenMode;
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        gui.diff_mode.set_focus(DiffModeFocus::SelectorA);
        for modifiers in [KeyModifiers::SHIFT, KeyModifiers::NONE] {
            gui.handle_key(KeyEvent::new(KeyCode::BackTab, modifiers))
                .unwrap();
            assert_eq!(gui.diff_mode.focus, DiffModeFocus::DiffExploration);
            gui.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
                .unwrap();
            assert_eq!(gui.diff_mode.focus, DiffModeFocus::SelectorA);
        }
        gui.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT))
            .unwrap();
        assert_eq!(gui.diff_mode.focus, DiffModeFocus::DiffExploration);
        let stable_key = gui.current_diff_key();
        for (key, modifiers, expected) in [
            ('+', KeyModifiers::NONE, ScreenMode::Half),
            ('+', KeyModifiers::SHIFT, ScreenMode::Full),
            ('_', KeyModifiers::SHIFT, ScreenMode::Half),
            ('_', KeyModifiers::NONE, ScreenMode::Normal),
        ] {
            gui.handle_key(KeyEvent::new(KeyCode::Char(key), modifiers))
                .unwrap();
            assert_eq!(gui.screen_mode, expected);
            assert_eq!(gui.current_diff_key(), stable_key);
        }
        let config = Arc::get_mut(&mut gui.config).unwrap();
        config.user_config.keybinding.universal.toggle_panel_reverse = "<c-b>".into();
        Arc::get_mut(&mut gui.config)
            .unwrap()
            .user_config
            .keybinding
            .universal
            .next_screen_mode = "<c-n>".into();
        gui.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(gui.diff_mode.focus, DiffModeFocus::Commits);
        gui.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL))
            .unwrap();
        assert_eq!(gui.screen_mode, ScreenMode::Half);
        gui.screen_mode = ScreenMode::Normal;
        Arc::get_mut(&mut gui.config)
            .unwrap()
            .user_config
            .keybinding
            .universal
            .next_screen_mode = "+".into();
        gui.diff_mode.start_editing(DiffModeSelector::A);
        gui.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(gui.screen_mode, ScreenMode::Normal);
        gui.diff_mode.cancel_editing();
        gui.diff_mode.file_search_active = true;
        gui.diff_mode.file_search_textarea = Some(tui_textarea::TextArea::default());
        gui.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(gui.screen_mode, ScreenMode::Normal);
        gui.diff_mode.clear_list_search();
        gui.diff_mode.set_focus(DiffModeFocus::DiffExploration);
        gui.diff_view.search_active = true;
        gui.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE))
            .unwrap();
        assert_eq!(gui.screen_mode, ScreenMode::Normal);
        gui.diff_view.search_active = false;

        let mouse = |gui: &mut Gui, kind, column, row| {
            gui.handle_mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            })
        };
        Arc::get_mut(&mut gui.config)
            .unwrap()
            .user_config
            .gui
            .mouse_events = true;
        for (width, height) in [(150, 40), (80, 40)] {
            gui.layout.update_size(width, height);
            gui.layout.side_panel_ratio = 0.4;
            let layout = gui.compute_compare_layout();
            let (col, row) = if layout.portrait {
                (5, layout.diff.y)
            } else {
                (layout.diff.x, 5)
            };
            mouse(&mut gui, MouseEventKind::Down(MouseButton::Left), col, row);
            assert!(gui.sidebar_resizing);
            let (col, row) = if layout.portrait { (5, 25) } else { (100, 5) };
            mouse(&mut gui, MouseEventKind::Drag(MouseButton::Left), col, row);
            let expected = if layout.portrait {
                25.0 / 39.0
            } else {
                100.0 / 150.0
            };
            assert!((gui.layout.side_panel_ratio - expected).abs() < 1e-8);
            mouse(&mut gui, MouseEventKind::Up(MouseButton::Left), col, row);
            assert!(!gui.sidebar_resizing);
            // Grabbing the sidebar's bottom border must map to the same
            // portrait split, rather than jumping by one row on mouse-down.
            if layout.portrait {
                let split = gui.compute_compare_layout().diff.y;
                mouse(
                    &mut gui,
                    MouseEventKind::Down(MouseButton::Left),
                    5,
                    split - 1,
                );
                assert!(gui.sidebar_resizing);
                assert_eq!(gui.compute_compare_layout().diff.y, split);
                mouse(
                    &mut gui,
                    MouseEventKind::Up(MouseButton::Left),
                    5,
                    split - 1,
                );
            }
            // Half/Full mode and disabled mouse input must not start resizing.
            gui.screen_mode = ScreenMode::Half;
            let half = gui.compute_compare_layout();
            let (col, row) = if half.portrait {
                (5, half.diff.y)
            } else {
                (half.diff.x, 5)
            };
            mouse(&mut gui, MouseEventKind::Down(MouseButton::Left), col, row);
            assert!(!gui.sidebar_resizing);
            gui.screen_mode = ScreenMode::Normal;
            Arc::get_mut(&mut gui.config)
                .unwrap()
                .user_config
                .gui
                .mouse_events = false;
            let normal = gui.compute_compare_layout();
            let (col, row) = if normal.portrait {
                (5, normal.diff.y)
            } else {
                (normal.diff.x, 5)
            };
            mouse(&mut gui, MouseEventKind::Down(MouseButton::Left), col, row);
            assert!(!gui.sidebar_resizing);
            Arc::get_mut(&mut gui.config)
                .unwrap()
                .user_config
                .gui
                .mouse_events = true;
            let before = gui.layout.side_panel_ratio;
            gui.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT))
                .unwrap();
            assert!((gui.layout.side_panel_ratio - (before - 0.05)).abs() < 1e-8);
            gui.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::ALT))
                .unwrap();
            assert!((gui.layout.side_panel_ratio - before).abs() < 1e-8);
            gui.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::ALT))
                .unwrap();
            assert_eq!(
                gui.layout.side_panel_ratio,
                if layout.portrait { 0.0 } else { 1.0 }
            );
            gui.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::ALT))
                .unwrap();
            assert_eq!(
                gui.layout.side_panel_ratio,
                if layout.portrait { 1.0 } else { 0.0 }
            );
            gui.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::ALT))
                .unwrap();
            assert_eq!(
                gui.layout.side_panel_ratio,
                gui.config.user_config.gui.side_panel_width
            );
        }

        gui.diff_mode.ref_a = "missing".into();
        reload_diff_files(&mut gui).unwrap();
        assert!(gui.diff_mode.commits.is_empty());
        assert!(gui.diff_mode.tree_nodes.is_empty());
        assert!(gui.diff_mode.ahead_behind.is_none());
    }
}
