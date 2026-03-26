use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::gui::popup::{PopupState, make_textarea};
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, _keybindings: &KeybindingConfig) -> Result<()> {
    // Space: update selected submodule
    if key.code == KeyCode::Char(' ') {
        return update_submodule(gui);
    }

    // a: add submodule
    if key.code == KeyCode::Char('a') {
        return add_submodule(gui);
    }

    // d: remove submodule
    if key.code == KeyCode::Char('d') {
        return remove_submodule(gui);
    }

    // e: enter submodule (open nested lazygitrs)
    if key.code == KeyCode::Char('e') {
        return enter_submodule(gui);
    }

    // u: update all submodules
    if key.code == KeyCode::Char('u') {
        return update_all_submodules(gui);
    }

    // i: init submodules
    if key.code == KeyCode::Char('i') {
        return init_submodules(gui);
    }

    Ok(())
}

fn update_submodule(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(sub) = model.submodules.get(selected) {
        let path = sub.path.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Update submodule".to_string(),
            message: format!("Update submodule '{}'?", path),
            on_confirm: Box::new(move |gui| {
                gui.git.update_submodule(&path)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn add_submodule(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "Add submodule (URL path)".to_string(),
        textarea: make_textarea("https://github.com/user/repo path"),
        on_confirm: Box::new(|gui, input| {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() >= 2 {
                gui.git.add_submodule(parts[0], parts[1])?;
                gui.needs_refresh = true;
            } else if parts.len() == 1 {
                // Derive path from URL
                let url = parts[0];
                let path = url
                    .rsplit('/')
                    .next()
                    .unwrap_or("submodule")
                    .trim_end_matches(".git");
                gui.git.add_submodule(url, path)?;
                gui.needs_refresh = true;
            }
            Ok(())
        }),
        is_commit: false,
    };
    Ok(())
}

fn remove_submodule(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(sub) = model.submodules.get(selected) {
        let path = sub.path.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Remove submodule".to_string(),
            message: format!("Remove submodule '{}'?", path),
            on_confirm: Box::new(move |gui| {
                gui.git.remove_submodule(&path)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn enter_submodule(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(sub) = model.submodules.get(selected) {
        let path = sub.path.clone();
        let name = sub.name.clone();
        drop(model);

        let abs_path = gui.git.repo_path().join(&path).to_string_lossy().to_string();

        gui.popup = PopupState::Confirm {
            title: "Enter submodule".to_string(),
            message: format!("Open lazygitrs in submodule '{}'?", name),
            on_confirm: Box::new(move |gui| {
                let exe = std::env::current_exe().unwrap_or_else(|_| "lazygitrs".into());
                std::process::Command::new(exe)
                    .arg("--path")
                    .arg(&abs_path)
                    .spawn()?;
                gui.should_quit = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn update_all_submodules(gui: &mut Gui) -> Result<()> {
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
