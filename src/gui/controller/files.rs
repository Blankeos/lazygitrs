use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::PopupState;
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
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

    Ok(())
}

fn toggle_stage(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(selected) {
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
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(selected) {
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
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(selected) {
        let name = file.name.clone();
        drop(model);
        gui.git.ignore_file(&name)?;
        gui.needs_refresh = true;
    }
    Ok(())
}

fn matches_key(key: KeyEvent, binding: &str) -> bool {
    if let Some(expected) = parse_key(binding) {
        key.code == expected.code && key.modifiers == expected.modifiers
    } else {
        false
    }
}
