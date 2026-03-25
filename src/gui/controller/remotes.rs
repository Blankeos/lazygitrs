use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::{MenuItem, PopupState, make_textarea};
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Fetch from selected remote
    if key.code == KeyCode::Char('f') {
        return fetch_remote(gui);
    }

    // Add new remote
    if key.code == KeyCode::Char('n') {
        return add_remote(gui);
    }

    // Delete remote
    if key.code == KeyCode::Char('d') {
        return delete_remote(gui);
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

fn add_remote(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "New remote name".to_string(),
        textarea: make_textarea(""),
        on_confirm: Box::new(|gui, name| {
            let name = name.trim().to_string();
            if !name.is_empty() {
                gui.popup = PopupState::Input {
                    title: format!("URL for remote '{}'", name),
                    textarea: make_textarea(""),
                    on_confirm: Box::new(move |gui, url| {
                        let url = url.trim().to_string();
                        if !url.is_empty() {
                            gui.git.add_remote(&name, &url)?;
                            gui.needs_refresh = true;
                        }
                        Ok(())
                    }),
                    is_commit: false,
                };
            }
            Ok(())
        }),
        is_commit: false,
    };
    Ok(())
}

fn delete_remote(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(remote) = model.remotes.get(selected) {
        let name = remote.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Delete remote".to_string(),
            message: format!("Delete remote '{}'?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.delete_remote(&name)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn fetch_remote(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(remote) = model.remotes.get(selected) {
        let name = remote.name.clone();
        drop(model);

        let msg = format!("Fetching {}...", name);
        gui.start_remote_op("Fetch", &msg, move |git| {
            git.fetch(&name)?;
            Ok(())
        });
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
                    let msg = format!("Pushing {} to origin...", b1);
                    gui.start_remote_op("Push", &msg, |git| {
                        git.push(false)?;
                        Ok(())
                    });
                    Ok(())
                })),
            },
            MenuItem {
                label: "Push (force-with-lease)".to_string(),
                description: "Force push with safety check".to_string(),
                key: Some("f".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.start_remote_op("Push", "Force pushing (with lease)...", |git| {
                        git.push(true)?;
                        Ok(())
                    });
                    Ok(())
                })),
            },
            MenuItem {
                label: "Push and set upstream".to_string(),
                description: format!("Push -u origin {}", b2),
                key: Some("u".to_string()),
                action: Some(Box::new(move |gui| {
                    let branch = b2.clone();
                    gui.start_remote_op("Push", &format!("Pushing -u origin {}...", branch), move |git| {
                        git.push_with_upstream("origin", &branch)?;
                        Ok(())
                    });
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
                    gui.start_remote_op("Pull", "Pulling from upstream...", |git| {
                        git.pull()?;
                        Ok(())
                    });
                    Ok(())
                })),
            },
            MenuItem {
                label: "Fetch all".to_string(),
                description: "Fetch from all remotes".to_string(),
                key: Some("f".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.start_remote_op("Fetch", "Fetching from all remotes...", |git| {
                        git.fetch_all()?;
                        Ok(())
                    });
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
