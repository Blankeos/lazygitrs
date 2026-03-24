use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::PopupState;
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Checkout with space
    if key.code == KeyCode::Char(' ') {
        return checkout_branch(gui);
    }

    if key.code == KeyCode::Char('n') {
        return new_branch(gui);
    }

    if key.code == KeyCode::Char('d') {
        return delete_branch(gui);
    }

    if matches_key(key, &keybindings.branches.merge_into_current_branch) {
        return merge_branch(gui);
    }

    if matches_key(key, &keybindings.branches.rebase_branch) {
        return rebase_branch(gui);
    }

    if matches_key(key, &keybindings.branches.rename_branch) {
        return rename_branch(gui);
    }

    if matches_key(key, &keybindings.branches.fast_forward) {
        return fast_forward(gui);
    }

    if matches_key(key, &keybindings.branches.set_upstream) {
        return set_upstream(gui);
    }

    Ok(())
}

fn checkout_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        if branch.head {
            return Ok(()); // Already on this branch
        }
        let name = branch.name.clone();
        drop(model);
        gui.git.checkout_branch(&name)?;
        gui.needs_refresh = true;
    }
    Ok(())
}

fn new_branch(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "New branch name".to_string(),
        buffer: String::new(),
        on_confirm: Box::new(|gui, name| {
            if !name.is_empty() {
                gui.git.create_branch(name)?;
                gui.needs_refresh = true;
            }
            Ok(())
        }),
    };
    Ok(())
}

fn delete_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        if branch.head {
            return Ok(()); // Can't delete current branch
        }
        let name = branch.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Delete branch".to_string(),
            message: format!("Delete branch '{}'?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.delete_branch(&name, false)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn merge_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let name = branch.name.clone();
        let merge_args = gui.config.user_config.git.merging.args.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Merge".to_string(),
            message: format!("Merge '{}' into current branch?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.merge_branch(&name, &merge_args)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn rebase_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let name = branch.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Rebase".to_string(),
            message: format!("Rebase onto '{}'?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.rebase_branch(&name)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn rename_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let old_name = branch.name.clone();
        drop(model);

        gui.popup = PopupState::Input {
            title: format!("Rename branch '{}'", old_name),
            buffer: old_name.clone(),
            on_confirm: Box::new(move |gui, new_name| {
                if !new_name.is_empty() && new_name != old_name {
                    gui.git.rename_branch(&old_name, new_name)?;
                    gui.needs_refresh = true;
                }
                Ok(())
            }),
        };
    }
    Ok(())
}

fn fast_forward(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        if branch.upstream.is_some() {
            let name = branch.name.clone();
            drop(model);
            gui.git.fetch("origin")?;
            gui.needs_refresh = true;
        }
    }
    Ok(())
}

fn set_upstream(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let name = branch.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Set upstream".to_string(),
            message: format!("Set upstream of '{}' to origin/{}?", name, name),
            on_confirm: Box::new(move |gui| {
                gui.git.push_with_upstream("origin", &name)?;
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
