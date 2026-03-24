use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::gui::popup::PopupState;
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, _keybindings: &KeybindingConfig) -> Result<()> {
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

    Ok(())
}

fn create_tag(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "New tag name".to_string(),
        buffer: String::new(),
        on_confirm: Box::new(|gui, name| {
            if !name.is_empty() {
                gui.git.create_tag(name, "")?;
                gui.needs_refresh = true;
            }
            Ok(())
        }),
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
