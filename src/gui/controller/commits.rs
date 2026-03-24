use anyhow::Result;
use crossterm::event::KeyEvent;

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::{PopupState, MenuItem};
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    if matches_key(key, &keybindings.commits.revert_commit) {
        return revert_commit(gui);
    }

    if matches_key(key, &keybindings.commits.rename_commit) {
        return reword_commit(gui);
    }

    if matches_key(key, &keybindings.commits.view_reset_options) {
        return show_reset_menu(gui);
    }

    if matches_key(key, &keybindings.commits.cherry_pick_copy) {
        return cherry_pick_copy(gui);
    }

    if matches_key(key, &keybindings.commits.tag_commit) {
        return tag_commit(gui);
    }

    Ok(())
}

fn revert_commit(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        let short = commit.short_hash().to_string();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Revert commit".to_string(),
            message: format!("Revert commit {}?", short),
            on_confirm: Box::new(move |gui| {
                gui.git.revert_commit(&hash)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn reword_commit(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        let current_msg = commit.name.clone();
        drop(model);

        gui.popup = PopupState::Input {
            title: "Reword commit".to_string(),
            buffer: current_msg,
            on_confirm: Box::new(move |gui, message| {
                if !message.is_empty() {
                    gui.git.reword_commit(&hash, message)?;
                    gui.needs_refresh = true;
                }
                Ok(())
            }),
        };
    }
    Ok(())
}

fn show_reset_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        drop(model);

        let h1 = hash.clone();
        let h2 = hash.clone();
        let h3 = hash.clone();

        gui.popup = PopupState::Menu {
            title: "Reset to this commit".to_string(),
            items: vec![
                MenuItem {
                    label: "Soft reset".to_string(),
                    description: "Keep changes staged".to_string(),
                    key: Some("s".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.reset_to_commit(&h1, "--soft")?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Mixed reset".to_string(),
                    description: "Keep changes unstaged".to_string(),
                    key: Some("m".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.reset_to_commit(&h2, "--mixed")?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Hard reset".to_string(),
                    description: "Discard all changes".to_string(),
                    key: Some("h".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.reset_to_commit(&h3, "--hard")?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
            ],
            selected: 0,
        };
    }
    Ok(())
}

fn cherry_pick_copy(gui: &mut Gui) -> Result<()> {
    // Phase 3: full cherry-pick mode with multi-select
    // For now, just cherry-pick the selected commit
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        let short = commit.short_hash().to_string();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Cherry-pick".to_string(),
            message: format!("Cherry-pick commit {}?", short),
            on_confirm: Box::new(move |gui| {
                gui.git.cherry_pick(&[hash.clone()])?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn tag_commit(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let _hash = commit.hash.clone();
        drop(model);

        gui.popup = PopupState::Input {
            title: "Tag name".to_string(),
            buffer: String::new(),
            on_confirm: Box::new(|gui, name| {
                if !name.is_empty() {
                    gui.git.create_tag(name, "")?;
                    gui.needs_refresh = true;
                }
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
