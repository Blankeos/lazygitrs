use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::keybindings::parse_key;
use crate::gui::Gui;
use crate::gui::modes::diff_mode::{DiffModeFocus, DiffModeSelector};
use crate::gui::popup::{HelpEntry, HelpSection, PopupState};

pub fn handle_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    // Popup takes priority (for ? help)
    if gui.popup != PopupState::None {
        return gui.handle_popup_key(key);
    }

    // If editing a combobox, route to combobox input handler
    if gui.diff_mode.editing.is_some() {
        return handle_combobox_key(gui, key);
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
        _ => {}
    }
    Ok(())
}

fn handle_diff_exploration_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    // Handle text selection keys first (y to copy, Esc to dismiss)
    if gui.diff_view.selection.is_some() {
        match key.code {
            KeyCode::Char('y') => {
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
            }
        }
    }

    match key.code {
        KeyCode::Esc => {
            gui.diff_mode.focus = DiffModeFocus::CommitFiles;
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
            HelpEntry { key: "g/G".into(), description: "Go to top / bottom".into() },
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
        search: String::new(),
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
