use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::gui::Gui;
use crate::gui::popup::{
    ListPickerCore, ListPickerItem, MenuItem, PopupState, make_help_search_textarea, make_textarea,
};

pub fn handle_key(gui: &mut Gui, key: KeyEvent, _keybindings: &KeybindingConfig) -> Result<()> {
    // Enter: view tag commits
    if key.code == KeyCode::Enter {
        return enter_tag_commits(gui);
    }

    // Create tag
    if key.code == KeyCode::Char('n') {
        return create_tag(gui);
    }

    // Delete tag
    if key.code == KeyCode::Char('d') {
        return delete_tag(gui);
    }

    // Push tag to remote
    if key.code == KeyCode::Char('P') {
        return push_tag(gui);
    }

    // Reset options
    if key.code == KeyCode::Char('g') {
        return show_reset_menu(gui);
    }

    Ok(())
}

fn enter_tag_commits(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(tag) = model.tags.get(selected) {
        let name = tag.name.clone();
        drop(model);

        // Load commits reachable from this tag
        let commits = gui.git.load_commits_for_branch(&name, 300)?;
        {
            let mut model = gui.model.lock().unwrap();
            model.sub_commits = commits;
        }
        gui.branch_commits_name = name;
        gui.sub_commits_parent_context = crate::gui::context::ContextId::Tags;

        // Switch to BranchCommits context (reused for tag commits)
        gui.context_mgr
            .set_active(crate::gui::context::ContextId::BranchCommits);
        gui.context_mgr.set_selection(0);
        gui.needs_diff_refresh = true;
    }
    Ok(())
}

fn create_tag(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "New tag name".to_string(),
        textarea: make_textarea(""),
        on_confirm: Box::new(|gui, name| {
            if !name.is_empty() {
                gui.git.create_tag(name, "")?;
                gui.needs_refresh = true;
            }
            Ok(())
        }),
        is_commit: false,
        confirm_focused: false,
    };
    Ok(())
}

fn delete_tag(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(tag) = model.tags.get(selected) {
        let name = tag.name.clone();
        drop(model);

        let name_local = name.clone();
        let name_remote = name.clone();
        let name_both = name.clone();

        gui.popup = PopupState::Menu {
            title: format!("Delete tag '{}'?", name),
            items: vec![
                MenuItem {
                    label: "Delete local tag".to_string(),
                    description: String::new(),
                    key: Some("c".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.delete_tag(&name_local)?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Delete remote tag".to_string(),
                    description: String::new(),
                    key: Some("r".to_string()),
                    action: Some(Box::new(move |gui| {
                        prompt_remote_tag_delete(gui, name_remote.clone(), false)
                    })),
                },
                MenuItem {
                    label: "Delete local and remote tag".to_string(),
                    description: String::new(),
                    key: Some("b".to_string()),
                    action: Some(Box::new(move |gui| {
                        prompt_remote_tag_delete(gui, name_both.clone(), true)
                    })),
                },
            ],
            selected: 0,
            loading_index: None,
        };
    }
    Ok(())
}

fn prompt_remote_tag_delete(
    gui: &mut Gui,
    name: String,
    delete_local_after_remote: bool,
) -> Result<()> {
    let model = gui.model.lock().unwrap();
    let items: Vec<ListPickerItem> = model
        .remotes
        .iter()
        .map(|remote| {
            let url = remote.urls.first().map(String::as_str).unwrap_or("");
            let label = if url.is_empty() {
                remote.name.clone()
            } else {
                format!("{} — {}", remote.name, url)
            };

            ListPickerItem {
                value: remote.name.clone(),
                label,
                category: "Remotes".to_string(),
            }
        })
        .collect();
    drop(model);

    gui.popup = PopupState::RefPicker {
        title: format!("Remote from which to remove tag '{}'", name),
        core: ListPickerCore {
            items,
            selected: 0,
            search_textarea: make_help_search_textarea(),
            scroll_offset: 0,
        },
        on_confirm: Box::new(move |gui, remote| {
            confirm_remote_tag_delete(gui, name.clone(), remote, delete_local_after_remote)
        }),
    };

    Ok(())
}

fn confirm_remote_tag_delete(
    gui: &mut Gui,
    name: String,
    remote: &str,
    delete_local_after_remote: bool,
) -> Result<()> {
    let remote = remote.trim().to_string();
    if remote.is_empty() {
        return Ok(());
    }

    let title = format!("Delete tag '{}'?", name);
    let message = if delete_local_after_remote {
        format!(
            "Are you sure you want to delete '{}' from both your machine and from '{}'?",
            name, remote
        )
    } else {
        format!(
            "Are you sure you want to delete the remote tag '{}' from '{}'?",
            name, remote
        )
    };

    gui.popup = PopupState::Confirm {
        title,
        message,
        on_confirm: Box::new(move |gui| {
            let message = if delete_local_after_remote {
                format!("Deleting tag {} locally and from {}...", name, remote)
            } else {
                format!("Deleting tag {} from {}...", name, remote)
            };

            gui.start_remote_op("Delete", &message, move |git| {
                git.delete_remote_tag(&remote, &name)?;
                if delete_local_after_remote {
                    git.delete_tag(&name)?;
                }
                Ok(())
            });
            Ok(())
        }),
    };

    Ok(())
}

fn show_reset_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(tag) = model.tags.get(selected) {
        let hash = tag.hash.clone();
        let short_hash = if hash.len() > 7 { &hash[..7] } else { &hash };
        drop(model);

        let h1 = hash.clone();
        let h2 = hash.clone();
        let h3 = hash.clone();

        gui.popup = PopupState::Menu {
            title: "Reset to this tag".to_string(),
            items: vec![
                MenuItem {
                    label: "Soft reset".to_string(),
                    description: format!("reset --soft {}", short_hash),
                    key: Some("s".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.reset_to_commit(&h1, "--soft")?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Mixed reset".to_string(),
                    description: format!("reset --mixed {}", short_hash),
                    key: Some("m".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.reset_to_commit(&h2, "--mixed")?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Hard reset".to_string(),
                    description: format!("reset --hard {}", short_hash),
                    key: Some("h".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.reset_to_commit(&h3, "--hard")?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
            ],
            selected: 0,
            loading_index: None,
        };
    }
    Ok(())
}

fn push_tag(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(tag) = model.tags.get(selected) {
        let name = tag.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Push tag".to_string(),
            message: format!("Push tag '{}' to origin?", name),
            on_confirm: Box::new(move |gui| {
                let tag = name.clone();
                gui.start_remote_op(
                    "Push",
                    &format!("Pushing tag {} to origin...", tag),
                    move |git| {
                        git.push_tag(&tag)?;
                        Ok(())
                    },
                );
                Ok(())
            }),
        };
    }
    Ok(())
}
