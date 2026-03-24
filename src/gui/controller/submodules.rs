use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::gui::popup::PopupState;
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, _keybindings: &KeybindingConfig) -> Result<()> {
    // Update submodule
    if key.code == KeyCode::Char('u') {
        return update_submodule(gui);
    }

    // Init submodule
    if key.code == KeyCode::Char('i') {
        return init_submodules(gui);
    }

    Ok(())
}

fn update_submodule(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Confirm {
        title: "Update submodules".to_string(),
        message: "Update all submodules?".to_string(),
        on_confirm: Box::new(|gui| {
            gui.git.update_submodules()?;
            gui.needs_refresh = true;
            Ok(())
        }),
    };
    Ok(())
}

fn init_submodules(gui: &mut Gui) -> Result<()> {
    gui.git.init_submodules()?;
    gui.needs_refresh = true;
    Ok(())
}
