use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::Gui;
use crate::gui::popup::{MenuItem, MessageKind, PopupState, make_textarea};

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Enter: drill into remote branches
    if key.code == KeyCode::Enter {
        return enter_remote_branches(gui);
    }

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

    // Edit remote (name + URL) — matches lazygit 'e'
    if key.code == KeyCode::Char('e') {
        return edit_remote(gui);
    }

    // Add fork remote — matches lazygit 'F'
    if key.code == KeyCode::Char('F') {
        return add_fork_remote(gui);
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

fn enter_remote_branches(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(remote) = model.remotes.get(selected) {
        let name = remote.name.clone();
        let mut branches = remote.branches.clone();
        let head_branch = model.head_branch_name.clone();
        drop(model);

        // Put the current branch's remote counterpart first (like local branches)
        if !head_branch.is_empty() {
            if let Some(idx) = branches.iter().position(|b| b.name == head_branch) {
                if idx > 0 {
                    let head = branches.remove(idx);
                    branches.insert(0, head);
                }
            }
        }

        {
            let mut model = gui.model.lock().unwrap();
            model.sub_remote_branches = branches;
        }
        gui.remote_branches_name = name;

        gui.context_mgr
            .set_active(crate::gui::context::ContextId::RemoteBranches);
        gui.context_mgr.set_selection(0);
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
                    confirm_focused: false,
                };
            }
            Ok(())
        }),
        is_commit: false,
        confirm_focused: false,
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

fn edit_remote(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(remote) = model.remotes.get(selected) {
        let old_name = remote.name.clone();
        let old_url = remote.urls.first().cloned().unwrap_or_default();
        drop(model);

        let mut ta = make_textarea("");
        ta.insert_str(&old_name);
        gui.popup = PopupState::Input {
            title: format!("New name for remote '{}'", old_name),
            textarea: ta,
            on_confirm: Box::new(move |gui, new_name| {
                let new_name = new_name.trim().to_string();
                if new_name.is_empty() {
                    return Ok(());
                }

                let name_for_url = new_name.clone();
                let old_name_for_rename = old_name.clone();
                let mut url_ta = make_textarea("");
                url_ta.insert_str(&old_url);
                gui.popup = PopupState::Input {
                    title: format!("New url for remote '{}'", new_name),
                    textarea: url_ta,
                    on_confirm: Box::new(move |gui, new_url| {
                        let new_url = new_url.trim().to_string();
                        if name_for_url != old_name_for_rename {
                            gui.git.rename_remote(&old_name_for_rename, &name_for_url)?;
                        }
                        if !new_url.is_empty() {
                            gui.git.update_remote_url(&name_for_url, &new_url)?;
                        }
                        gui.needs_refresh = true;
                        Ok(())
                    }),
                    is_commit: false,
                    confirm_focused: false,
                };
                Ok(())
            }),
            is_commit: false,
            confirm_focused: false,
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

fn add_fork_remote(gui: &mut Gui) -> Result<()> {
    let model = gui.model.lock().unwrap();
    let origin = model.remotes.iter().find(|r| r.name == "origin");
    let Some(origin) = origin else {
        drop(model);
        gui.popup = PopupState::Message {
            title: "Error".to_string(),
            message: "No origin remote found".to_string(),
            kind: MessageKind::Error,
        };
        return Ok(());
    };
    let origin_url = origin.urls.first().cloned().unwrap_or_default();
    if origin_url.is_empty() {
        drop(model);
        gui.popup = PopupState::Message {
            title: "Error".to_string(),
            message: "Origin remote has no URL".to_string(),
            kind: MessageKind::Error,
        };
        return Ok(());
    }
    drop(model);

    gui.popup = PopupState::Input {
        title: "Fork owner (username/org). Use username:branch to check out a branch".to_string(),
        textarea: make_textarea(""),
        on_confirm: Box::new(move |gui, input| {
            let input = input.trim().to_string();
            if input.is_empty() {
                return Ok(());
            }

            let (fork_username, branch_to_checkout) = match input.split_once(':') {
                Some((user, branch)) => (user.to_string(), Some(branch.to_string())),
                None => (input, None),
            };

            let remote_url = match replace_fork_username(&origin_url, &fork_username) {
                Ok(url) => url,
                Err(e) => {
                    gui.popup = PopupState::Message {
                        title: "Error".to_string(),
                        message: e.to_string(),
                        kind: MessageKind::Error,
                    };
                    return Ok(());
                }
            };

            ensure_fork_remote_and_checkout(gui, &fork_username, &remote_url, branch_to_checkout)
        }),
        is_commit: false,
        confirm_focused: false,
    };
    Ok(())
}

/// Rewrites a Git remote URL to use the given fork username, keeping host and
/// repo name. Supports SCP-like SSH, ssh://, and https:// URLs.
fn replace_fork_username(origin_url: &str, fork_username: &str) -> Result<String> {
    // Manual parse — no regex crate dependency.
    // Patterns: git@host:owner/repo(.git)?
    //           ssh://host/owner/repo(.git)?
    //           http(s)://host/owner/repo(.git)?
    let (prefix, rest) = if let Some(rest) = origin_url.strip_prefix("git@") {
        // git@host:path
        if let Some(colon) = rest.find(':') {
            let host = &rest[..colon];
            (format!("git@{host}:"), rest[colon + 1..].to_string())
        } else {
            anyhow::bail!("unsupported or invalid remote URL: {origin_url}");
        }
    } else if let Some(rest) = origin_url
        .strip_prefix("ssh://")
        .or_else(|| origin_url.strip_prefix("https://"))
        .or_else(|| origin_url.strip_prefix("http://"))
    {
        let scheme = if origin_url.starts_with("ssh://") {
            "ssh://"
        } else if origin_url.starts_with("https://") {
            "https://"
        } else {
            "http://"
        };
        if let Some(slash) = rest.find('/') {
            let host = &rest[..slash];
            (format!("{scheme}{host}/"), rest[slash + 1..].to_string())
        } else {
            anyhow::bail!("unsupported or invalid remote URL: {origin_url}");
        }
    } else {
        anyhow::bail!("unsupported or invalid remote URL: {origin_url}");
    };

    // rest is owner/.../repo(.git)? — replace first path segment (owner)
    let mut parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 2 {
        anyhow::bail!("unsupported or invalid remote URL: {origin_url}");
    }
    parts[0] = fork_username;
    Ok(format!("{prefix}{}", parts.join("/")))
}

fn ensure_fork_remote_and_checkout(
    gui: &mut Gui,
    remote_name: &str,
    remote_url: &str,
    branch_to_checkout: Option<String>,
) -> Result<()> {
    let model = gui.model.lock().unwrap();
    if let Some((idx, remote)) = model
        .remotes
        .iter()
        .enumerate()
        .find(|(_, r)| r.name == remote_name)
    {
        let has_same_url = remote.urls.iter().any(|u| u == remote_url);
        if !has_same_url {
            drop(model);
            gui.popup = PopupState::Message {
                title: "Error".to_string(),
                message: format!(
                    "A remote named '{}' already exists with a different URL",
                    remote_name
                ),
                kind: MessageKind::Error,
            };
            return Ok(());
        }
        drop(model);
        gui.context_mgr.set_selection(idx);
        return fetch_and_checkout(gui, remote_name, branch_to_checkout);
    }
    drop(model);

    gui.git.add_remote(remote_name, remote_url)?;

    // Refresh remotes so we can select the new one.
    if let Ok(remotes) = gui.git.load_remotes() {
        let mut model = gui.model.lock().unwrap();
        if let Some(idx) = remotes.iter().position(|r| r.name == remote_name) {
            model.remotes = remotes;
            drop(model);
            gui.context_mgr.set_selection(idx);
        } else {
            model.remotes = remotes;
            drop(model);
        }
    }

    fetch_and_checkout(gui, remote_name, branch_to_checkout)
}

fn fetch_and_checkout(
    gui: &mut Gui,
    remote_name: &str,
    branch_to_checkout: Option<String>,
) -> Result<()> {
    let remote_name = remote_name.to_string();
    let msg = format!("Fetching {}...", remote_name);
    gui.start_remote_op("Fetch", &msg, move |git| {
        git.fetch(&remote_name)?;
        if let Some(branch) = branch_to_checkout.as_deref() {
            if !branch.is_empty() {
                git.checkout_remote_branch(&remote_name, branch)?;
            }
        }
        Ok(())
    });
    Ok(())
}

fn show_push_menu(gui: &mut Gui) -> Result<()> {
    let branch = gui.git.current_branch_name().unwrap_or_default();
    let b1 = branch.clone();
    let b2 = branch.clone();

    // Check if the current branch is tracking a remote
    let is_tracking = {
        let model = gui.model.lock().unwrap();
        model
            .branches
            .iter()
            .find(|b| b.head)
            .map(|b| b.is_tracking())
            .unwrap_or(false)
    };

    gui.popup = PopupState::Menu {
        title: "Push".to_string(),
        items: vec![
            MenuItem {
                label: "Push".to_string(),
                description: format!("Push {} to origin", branch),
                key: Some("p".to_string()),
                action: Some(Box::new(move |gui| {
                    let msg = format!("Pushing {} to origin...", b1);
                    if is_tracking {
                        gui.start_remote_op("Push", &msg, |git| {
                            git.push(false)?;
                            Ok(())
                        });
                    } else {
                        let branch = b1.clone();
                        gui.start_remote_op("Push", &msg, move |git| {
                            git.push_with_upstream("origin", &branch)?;
                            Ok(())
                        });
                    }
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
                    gui.start_remote_op(
                        "Push",
                        &format!("Pushing -u origin {}...", branch),
                        move |git| {
                            git.push_with_upstream("origin", &branch)?;
                            Ok(())
                        },
                    );
                    Ok(())
                })),
            },
        ],
        selected: 0,
        loading_index: None,
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
        loading_index: None,
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
