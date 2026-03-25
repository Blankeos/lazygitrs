use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::{MenuItem, PopupState, make_textarea};
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

    // Rename stash
    if matches_key(key, &keybindings.stash.rename_stash) {
        return rename_stash(gui);
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

        gui.popup = PopupState::Menu {
            title: format!("Pop stash '{}'?", name),
            items: vec![
                MenuItem {
                    label: "Pop".to_string(),
                    description: "apply and drop this stash".to_string(),
                    key: Some("g".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.stash_pop(index)?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Cancel".to_string(),
                    description: String::new(),
                    key: Some("c".to_string()),
                    action: Some(Box::new(|_| Ok(()))),
                },
            ],
            selected: 0,
        };
    }
    Ok(())
}

fn apply_stash(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(entry) = model.stash_entries.get(selected) {
        let index = entry.index;
        let name = entry.name.clone();
        drop(model);

        gui.popup = PopupState::Menu {
            title: format!("Apply stash '{}'?", name),
            items: vec![
                MenuItem {
                    label: "Apply".to_string(),
                    description: "apply stash (keep in stash list)".to_string(),
                    key: Some("a".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.stash_apply(index)?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Cancel".to_string(),
                    description: String::new(),
                    key: Some("c".to_string()),
                    action: Some(Box::new(|_| Ok(()))),
                },
            ],
            selected: 0,
        };
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

        gui.popup = PopupState::Menu {
            title: format!("Drop stash '{}'?", name),
            items: vec![
                MenuItem {
                    label: "Drop".to_string(),
                    description: "permanently remove this stash".to_string(),
                    key: Some("d".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.stash_drop(index)?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Cancel".to_string(),
                    description: String::new(),
                    key: Some("c".to_string()),
                    action: Some(Box::new(|_| Ok(()))),
                },
            ],
            selected: 0,
        };
    }
    Ok(())
}

fn rename_stash(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(entry) = model.stash_entries.get(selected) {
        let index = entry.index;
        let current_name = entry.name.clone();
        drop(model);

        let mut ta = make_textarea("");
        ta.insert_str(&current_name);
        gui.popup = PopupState::Input {
            title: "Rename stash".to_string(),
            textarea: ta,
            on_confirm: Box::new(move |gui, new_name| {
                if !new_name.is_empty() {
                    gui.git.stash_rename(index, new_name)?;
                    gui.needs_refresh = true;
                }
                Ok(())
            }),
            is_commit: false,
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
