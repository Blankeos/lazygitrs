use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::gui::popup::{MenuItem, PopupState, make_textarea};
use crate::gui::Gui;

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
        gui.context_mgr.set_active(crate::gui::context::ContextId::BranchCommits);
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
        is_commit: false, confirm_focused: false,
    };
    Ok(())
}

fn delete_tag(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(tag) = model.tags.get(selected) {
        let name = tag.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Delete tag".to_string(),
            message: format!("Delete tag '{}'?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.delete_tag(&name)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
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
                gui.git.push_tag(&name)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}
