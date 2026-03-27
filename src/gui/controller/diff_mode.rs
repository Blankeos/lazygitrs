use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::keybindings::parse_key;
use crate::gui::Gui;
use crate::gui::modes::diff_mode::{DiffModeFocus, DiffModeSelector};
use crate::pager::side_by_side::DiffPanelLayout;
use crate::gui::popup::{HelpEntry, HelpSection, MenuItem, PopupState};
use crate::model::FileChangeStatus;
use crate::os::platform::Platform;

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

    // q to exit diff mode
    if key.code == KeyCode::Char('q') {
        gui.diff_mode.exit();
        return Ok(());
    }

    // ? to show help
    if key.code == KeyCode::Char('?') {
        show_diff_mode_help(gui);
        return Ok(());
    }

    let keybindings = &gui.config.user_config.keybinding;

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

    // Tab to cycle focus
    if key.code == KeyCode::Tab {
        gui.diff_mode.focus = gui.diff_mode.focus.next();
        gui.needs_diff_refresh = true;
        return Ok(());
    }

    // Number keys 1-4 to jump to focus panel
    if let KeyCode::Char(c @ '1'..='4') = key.code {
        if let Some(focus) = DiffModeFocus::from_number(c.to_digit(10).unwrap()) {
            gui.diff_mode.focus = focus;
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
                gui.diff_mode.search_refs(&model.branches, &model.tags, &model.commits, &model.remotes);
            }
        }
        DiffModeFocus::SelectorB => {
            if key.code == KeyCode::Enter {
                gui.diff_mode.start_editing(DiffModeSelector::B);
                let model = gui.model.lock().unwrap();
                gui.diff_mode.search_refs(&model.branches, &model.tags, &model.commits, &model.remotes);
            }
        }
        DiffModeFocus::CommitFiles => {
            handle_commit_files_key(gui, key)?;
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
                gui.diff_mode.focus = DiffModeFocus::CommitFiles;
            } else if gui.diff_mode.ref_a.is_empty() {
                // B was just set, A still empty — jump to A and start editing
                gui.diff_mode.focus = DiffModeFocus::SelectorA;
                gui.diff_mode.start_editing(DiffModeSelector::A);
                let model = gui.model.lock().unwrap();
                gui.diff_mode.search_refs(&model.branches, &model.tags, &model.commits, &model.remotes);
            } else {
                // A was just set, B still empty — jump to B and start editing
                gui.diff_mode.focus = DiffModeFocus::SelectorB;
                gui.diff_mode.start_editing(DiffModeSelector::B);
                let model = gui.model.lock().unwrap();
                gui.diff_mode.search_refs(&model.branches, &model.tags, &model.commits, &model.remotes);
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
                ta.input(key);
            }
            // Re-search after any text change
            let model = gui.model.lock().unwrap();
            gui.diff_mode.search_refs(&model.branches, &model.tags, &model.commits, &model.remotes);
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
                    gui.diff_mode.diff_files_selected = gui.diff_mode.file_search_matches[0];
                }
                gui.diff_mode.file_search_textarea = None;
                gui.needs_diff_refresh = true;
            }
            _ => {
                ta.input(key);
                gui.diff_mode.file_search_query = ta.lines().join("");
                gui.diff_mode.update_file_search_matches();
                gui.needs_diff_refresh = true;
            }
        }
    }
    Ok(())
}

fn handle_commit_files_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    let keybindings = &gui.config.user_config.keybinding;

    // Toggle tree view (backtick)
    if matches_key(key, &keybindings.files.toggle_tree_view) {
        gui.diff_mode.show_tree = !gui.diff_mode.show_tree;
        update_diff_mode_tree(gui);
        gui.diff_mode.diff_files_selected = 0;
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
                gui.needs_diff_refresh = true;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if gui.diff_mode.diff_files_selected > 0 {
                gui.diff_mode.diff_files_selected -= 1;
                gui.needs_diff_refresh = true;
            }
        }
        KeyCode::Enter => {
            if gui.diff_mode.show_tree {
                // Toggle dir collapse or focus diff
                if let Some(node) = gui.diff_mode.tree_nodes.get(gui.diff_mode.diff_files_selected) {
                    if node.is_dir {
                        let path = node.path.clone();
                        if gui.diff_mode.collapsed_dirs.contains(&path) {
                            gui.diff_mode.collapsed_dirs.remove(&path);
                        } else {
                            gui.diff_mode.collapsed_dirs.insert(path);
                        }
                        update_diff_mode_tree(gui);
                        return Ok(());
                    }
                }
            }
            gui.diff_mode.focus = DiffModeFocus::DiffExploration;
            gui.needs_diff_refresh = true;
        }
        KeyCode::Char('g') => {
            gui.diff_mode.diff_files_selected = 0;
            gui.needs_diff_refresh = true;
        }
        KeyCode::Char('G') => {
            gui.diff_mode.diff_files_selected = len.saturating_sub(1);
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
        gui.diff_mode.tree_nodes.get(selected).and_then(|n| n.file_index)
    } else {
        Some(selected)
    };

    let Some(idx) = file_idx else { return Ok(()) };
    let Some(file) = gui.diff_mode.diff_files.get(idx) else { return Ok(()) };

    let file_name = file.name.clone();
    let status = file.status;
    let ref_a = gui.diff_mode.ref_a.clone();
    let ref_b = gui.diff_mode.ref_b.clone();
    let path_for_old = file_name.clone();
    let path_for_new = file_name.clone();
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
                label: "Old content (from A)".to_string(),
                description: if has_old { String::new() } else { "File was added — no old content".to_string() },
                key: Some("o".to_string()),
                action: if has_old {
                    Some(Box::new(move |gui| {
                        let content = gui.git.file_content_at_commit(&ref_a_for_old, &path_for_old)?;
                        Platform::copy_to_clipboard(&content)?;
                        Ok(())
                    }))
                } else {
                    None
                },
            },
            MenuItem {
                label: "New content (from B)".to_string(),
                description: if has_new { String::new() } else { "File was deleted — no new content".to_string() },
                key: Some("w".to_string()),
                action: if has_new {
                    Some(Box::new(move |gui| {
                        let content = gui.git.file_content_at_commit(&ref_b_for_new, &path_for_new)?;
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
                    let diff = gui.git.diff_refs_file(&ref_a_for_diff, &ref_b_for_diff, &path_for_diff)?;
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
                ta.input(key);
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
                let area = ratatui::layout::Rect::new(0, 0, gui.layout.width, gui.layout.height);
                let outer = ratatui::layout::Layout::default()
                    .direction(ratatui::layout::Direction::Vertical)
                    .constraints([ratatui::layout::Constraint::Min(1), ratatui::layout::Constraint::Length(1)])
                    .split(area);
                let content = ratatui::layout::Layout::default()
                    .direction(ratatui::layout::Direction::Horizontal)
                    .constraints([ratatui::layout::Constraint::Percentage(33), ratatui::layout::Constraint::Percentage(67)])
                    .split(outer[0]);
                let diff_rect = content[1];
                let pl = DiffPanelLayout::compute(diff_rect, &gui.diff_view);
                let (content_start, _) = pl.content_range(sel_ref.panel);
                let column = if top_col >= content_start {
                    (top_col - content_start) as usize + gui.diff_view.horizontal_scroll + 1
                } else {
                    1
                };
                // Resolve the actual filename for multi-file diffs
                let line_idx = if top_row >= pl.inner_y {
                    gui.diff_view.scroll_offset + (top_row - pl.inner_y) as usize
                } else {
                    0
                };
                let filename = gui.diff_view.file_at_line(line_idx).to_string();
                gui.diff_view.selection = None;
                let abs_path = gui.git.repo_path().join(&filename);
                if !filename.is_empty() && abs_path.exists() {
                    let abs_path = abs_path.to_string_lossy().to_string();
                    let os = &gui.config.user_config.os;
                    if let Some(ln) = line {
                        let tpl = if !os.edit_at_line.is_empty() { &os.edit_at_line } else { &os.edit };
                        let _ = crate::config::user_config::OsConfig::run_template_at_line(tpl, &abs_path, ln, column);
                    } else {
                        let _ = crate::config::user_config::OsConfig::run_template(&os.edit, &abs_path);
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

    match key.code {
        KeyCode::Esc => {
            if !gui.diff_view.search_query.is_empty() {
                gui.diff_view.clear_search();
            } else {
                gui.diff_mode.focus = DiffModeFocus::CommitFiles;
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
    let ref_a = gui.diff_mode.ref_a.clone();
    let ref_b = gui.diff_mode.ref_b.clone();
    if ref_a.is_empty() || ref_b.is_empty() {
        return Ok(());
    }
    // Clear the diff view since we're loading new files
    gui.diff_view = crate::pager::side_by_side::DiffViewState::new();

    match gui.git.diff_refs_files(&ref_a, &ref_b) {
        Ok(files) => {
            gui.diff_mode.diff_files = files;
            gui.diff_mode.diff_files_selected = 0;
            gui.diff_mode.diff_files_scroll = 0;
            if gui.diff_mode.show_tree {
                update_diff_mode_tree(gui);
            }
        }
        Err(e) => {
            gui.diff_mode.diff_files.clear();
            gui.popup = PopupState::Message {
                title: "Diff error".to_string(),
                message: format!("{}", e),
                kind: crate::gui::popup::MessageKind::Error,
            };
        }
    }
    Ok(())
}

fn update_diff_mode_tree(gui: &mut Gui) {
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
pub fn maybe_request_diff(gui: &mut Gui) {
    if !gui.diff_mode.has_both_refs() || gui.diff_mode.diff_files.is_empty() {
        return;
    }

    // Resolve file index (tree view maps node -> file index)
    let selected = gui.diff_mode.diff_files_selected;
    let file_idx = if gui.diff_mode.show_tree {
        gui.diff_mode.tree_nodes.get(selected).and_then(|n| n.file_index)
    } else {
        Some(selected)
    };

    let Some(idx) = file_idx else { return };
    let Some(file) = gui.diff_mode.diff_files.get(idx) else { return };

    let name = file.name.clone();
    let ref_a = gui.diff_mode.ref_a.clone();
    let ref_b = gui.diff_mode.ref_b.clone();

    match gui.git.diff_refs_file(&ref_a, &ref_b, &name) {
        Ok(diff) => {
            if diff.is_empty() {
                gui.diff_view = crate::pager::side_by_side::DiffViewState::new();
            } else {
                gui.diff_view.load_from_diff_output(&name, &diff);
                gui.diff_view.file_exists_on_disk = gui.git.repo_path().join(&name).exists();
            }
        }
        Err(_) => {
            gui.diff_view = crate::pager::side_by_side::DiffViewState::new();
        }
    }
}

fn show_diff_mode_help(gui: &mut Gui) {
    let diff_mode_section = HelpSection {
        title: "Compare / Diff Mode".into(),
        entries: vec![
            HelpEntry { key: "q".into(), description: "Exit diff mode".into() },
            HelpEntry { key: "Tab".into(), description: "Cycle focus (A → B → Files → Diff)".into() },
            HelpEntry { key: "1-4".into(), description: "Jump to panel".into() },
            HelpEntry { key: "<c-s>".into(), description: "Swap A and B".into() },
            HelpEntry { key: "<enter>".into(), description: "Edit selector / Focus diff".into() },
            HelpEntry { key: "`".into(), description: "Toggle file tree view".into() },
            HelpEntry { key: "j/k".into(), description: "Navigate files / Scroll diff".into() },
            HelpEntry { key: "{/}".into(), description: "Previous / next hunk".into() },
            HelpEntry { key: "[/]".into(), description: "Toggle old / new only view".into() },
            HelpEntry { key: "z".into(), description: "Toggle line wrap".into() },
            HelpEntry { key: "g/G".into(), description: "Go to top / bottom".into() },
            HelpEntry { key: "/".into(), description: "Search (files or diff content)".into() },
            HelpEntry { key: "n/N".into(), description: "Next / previous search match".into() },
            HelpEntry { key: "y".into(), description: "Copy to clipboard".into() },
            HelpEntry { key: "?".into(), description: "Show this help".into() },
        ],
    };

    let combobox_section = HelpSection {
        title: "Combobox (while editing A or B)".into(),
        entries: vec![
            HelpEntry { key: "<enter>".into(), description: "Confirm selection".into() },
            HelpEntry { key: "<esc>".into(), description: "Cancel".into() },
            HelpEntry { key: "Up/Down".into(), description: "Navigate results".into() },
            HelpEntry { key: "Type".into(), description: "Filter branches, tags, commits, remotes".into() },
        ],
    };

    gui.popup = PopupState::Help {
        sections: vec![diff_mode_section, combobox_section],
        selected: 0,
        search_textarea: crate::gui::popup::make_help_search_textarea(),
        scroll_offset: 0,
    };
}

fn matches_key(key: KeyEvent, binding: &str) -> bool {
    if let Some(expected) = parse_key(binding) {
        key.code == expected.code && key.modifiers == expected.modifiers
    } else {
        false
    }
}
