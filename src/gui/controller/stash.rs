use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::PopupState;
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Pop stash
    if matches_key(key, &keybindings.stash.pop_stash) {
        return pop_stash(gui);
    }

    // Apply stash with space
    if key.code == KeyCode::Char(' ') {
        return apply_stash(gui);
    }

    // Drop stash
    if key.code == KeyCode::Char('d') {
        return drop_stash(gui);
    }

    Ok(())
}

fn pop_stash(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(entry) = model.stash_entries.get(selected) {
        let index = entry.index;
        let name = entry.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Pop stash".to_string(),
            message: format!("Pop '{}'?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.stash_pop(index)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn apply_stash(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(entry) = model.stash_entries.get(selected) {
        let index = entry.index;
        drop(model);
        gui.git.stash_apply(index)?;
        gui.needs_refresh = true;
    }
    Ok(())
}

fn drop_stash(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(entry) = model.stash_entries.get(selected) {
        let index = entry.index;
        let name = entry.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Drop stash".to_string(),
            message: format!("Drop '{}'?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.stash_drop(index)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
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
