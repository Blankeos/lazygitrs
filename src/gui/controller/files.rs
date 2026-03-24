use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::PopupState;
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Enter to toggle directory collapse in tree view
    if key.code == KeyCode::Enter && gui.show_file_tree {
        let selected = gui.context_mgr.selected_active();
        if let Some(node) = gui.file_tree_nodes.get(selected) {
            if node.is_dir {
                let path = node.path.clone();
                if gui.collapsed_dirs.contains(&path) {
                    gui.collapsed_dirs.remove(&path);
                } else {
                    gui.collapsed_dirs.insert(path);
                }
                gui.update_file_tree_state();
                return Ok(());
            }
        }
    }

    // Stage/unstage toggle with space
    if key.code == KeyCode::Char(' ') {
        return toggle_stage(gui);
    }

    if matches_key(key, &keybindings.files.commit_changes) {
        return open_commit_prompt(gui);
    }

    if matches_key(key, &keybindings.files.toggle_staged_all) {
        return toggle_stage_all(gui);
    }

    if matches_key(key, &keybindings.files.stash_all_changes) {
        return stash_changes(gui);
    }

    if key.code == KeyCode::Char('d') {
        return discard_file(gui);
    }

    if matches_key(key, &keybindings.files.ignore_file) {
        return ignore_file(gui);
    }

    // Amend last commit
    if matches_key(key, &keybindings.files.amend_last_commit) {
        return amend_commit(gui);
    }

    // Commit with editor
    if matches_key(key, &keybindings.files.commit_changes_with_editor) {
        return commit_with_editor(gui);
    }

    // Toggle file tree view
    if matches_key(key, &keybindings.files.toggle_tree_view) {
        gui.show_file_tree = !gui.show_file_tree;
        gui.update_file_tree_state();
        // Reset selection when toggling view modes
        gui.context_mgr.set_selection(0);
        return Ok(());
    }

    // Fetch
    if matches_key(key, &keybindings.files.fetch) {
        gui.git.fetch_all()?;
        gui.needs_refresh = true;
        return Ok(());
    }

    Ok(())
}

fn toggle_stage(gui: &mut Gui) -> Result<()> {
    let Some(file_idx) = gui.selected_file_index() else {
        return Ok(()); // Directory node selected, nothing to do
    };
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(file_idx) {
        let name = file.name.clone();
        let has_staged = file.has_staged_changes;
        let has_unstaged = file.has_unstaged_changes;
        drop(model);

        if has_unstaged || !has_staged {
            gui.git.stage_file(&name)?;
        } else {
            gui.git.unstage_file(&name)?;
        }
        gui.needs_refresh = true;
    }
    Ok(())
}

fn toggle_stage_all(gui: &mut Gui) -> Result<()> {
    let model = gui.model.lock().unwrap();
    let any_unstaged = model.files.iter().any(|f| f.has_unstaged_changes || !f.tracked);
    drop(model);

    if any_unstaged {
        gui.git.stage_all()?;
    } else {
        gui.git.unstage_all()?;
    }
    gui.needs_refresh = true;
    Ok(())
}

fn open_commit_prompt(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "Commit message".to_string(),
        buffer: String::new(),
        on_confirm: Box::new(|gui, message| {
            if !message.is_empty() {
                gui.git.create_commit(message, false)?;
                gui.needs_refresh = true;
            }
            Ok(())
        }),
    };
    Ok(())
}

fn stash_changes(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "Stash message (leave empty for default)".to_string(),
        buffer: String::new(),
        on_confirm: Box::new(|gui, message| {
            gui.git.stash_save(message)?;
            gui.needs_refresh = true;
            Ok(())
        }),
    };
    Ok(())
}

fn discard_file(gui: &mut Gui) -> Result<()> {
    let Some(file_idx) = gui.selected_file_index() else {
        return Ok(());
    };
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(file_idx) {
        let name = file.name.clone();
        drop(model);

        if !gui.config.user_config.gui.skip_discard_change_warning {
            gui.popup = PopupState::Confirm {
                title: "Discard changes".to_string(),
                message: format!("Discard changes to '{}'?", name),
                on_confirm: Box::new(move |gui| {
                    gui.git.discard_file(&name)?;
                    gui.needs_refresh = true;
                    Ok(())
                }),
            };
        } else {
            gui.git.discard_file(&name)?;
            gui.needs_refresh = true;
        }
    }
    Ok(())
}

fn ignore_file(gui: &mut Gui) -> Result<()> {
    let Some(file_idx) = gui.selected_file_index() else {
        return Ok(());
    };
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(file_idx) {
        let name = file.name.clone();
        drop(model);
        gui.git.ignore_file(&name)?;
        gui.needs_refresh = true;
    }
    Ok(())
}

fn amend_commit(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Confirm {
        title: "Amend".to_string(),
        message: "Amend last commit with staged changes?".to_string(),
        on_confirm: Box::new(|gui| {
            gui.git.amend_commit()?;
            gui.needs_refresh = true;
            Ok(())
        }),
    };
    Ok(())
}

fn commit_with_editor(gui: &mut Gui) -> Result<()> {
    // Run git commit which opens $EDITOR
    // This requires suspending the TUI temporarily
    gui.popup = PopupState::Input {
        title: "Commit message (or leave empty to open editor)".to_string(),
        buffer: String::new(),
        on_confirm: Box::new(|gui, message| {
            if message.is_empty() {
                // For now, just create an empty commit message prompt
                // Full editor integration requires Phase 4 (subprocess management)
            } else {
                gui.git.create_commit(message, false)?;
            }
            gui.needs_refresh = true;
            Ok(())
        }),
    };
    Ok(())
}

fn matches_key(key: KeyEvent, binding: &str) -> bool {
    if let Some(expected) = parse_key(binding) {
        key.code == expected.code && key.modifiers == expected.modifiers
    } else {
        false
    }
}
