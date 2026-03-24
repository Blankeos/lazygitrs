use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::{MenuItem, PopupState};
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Fetch from selected remote
    if key.code == KeyCode::Char('f') {
        return fetch_remote(gui);
    }

    // Push
    if matches_key(key, &keybindings.universal.push_files) {
        return show_push_menu(gui);
    }

    // Pull
    if matches_key(key, &keybindings.universal.pull_files) {
        return show_pull_menu(gui);
    }

    Ok(())
}

fn fetch_remote(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(remote) = model.remotes.get(selected) {
        let name = remote.name.clone();
        drop(model);

        gui.git.fetch(&name)?;
        gui.needs_refresh = true;
    }
    Ok(())
}

fn show_push_menu(gui: &mut Gui) -> Result<()> {
    let branch = gui.git.current_branch_name().unwrap_or_default();
    let b1 = branch.clone();
    let b2 = branch.clone();

    gui.popup = PopupState::Menu {
        title: "Push".to_string(),
        items: vec![
            MenuItem {
                label: "Push".to_string(),
                description: format!("Push {} to origin", branch),
                key: Some("p".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.push(false)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Push (force-with-lease)".to_string(),
                description: "Force push with safety check".to_string(),
                key: Some("f".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.push(true)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Push and set upstream".to_string(),
                description: format!("Push -u origin {}", b1),
                key: Some("u".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.push_with_upstream("origin", &b2)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
        ],
        selected: 0,
    };
    Ok(())
}

fn show_pull_menu(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Menu {
        title: "Pull".to_string(),
        items: vec![
            MenuItem {
                label: "Pull".to_string(),
                description: "Pull from upstream".to_string(),
                key: Some("p".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.pull()?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Fetch all".to_string(),
                description: "Fetch from all remotes".to_string(),
                key: Some("f".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.fetch_all()?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
        ],
        selected: 0,
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
