use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::gui::popup::{PopupState, make_textarea};
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, _keybindings: &KeybindingConfig) -> Result<()> {
    // Create new worktree
    if key.code == KeyCode::Char('n') {
        return create_worktree(gui);
    }

    // Remove worktree
    if key.code == KeyCode::Char('d') {
        return remove_worktree(gui);
    }

    Ok(())
}

fn create_worktree(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "New worktree (path branch)".to_string(),
        textarea: make_textarea("path branch-name"),
        on_confirm: Box::new(|gui, input| {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.len() >= 2 {
                gui.git.create_worktree(parts[0], parts[1])?;
                gui.needs_refresh = true;
            } else if parts.len() == 1 {
                // If only path given, create with new branch based on dir name
                let path = parts[0];
                let branch = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "worktree".to_string());

                gui.git.create_worktree_new_branch(path, &branch)?;
                gui.needs_refresh = true;
            }
            Ok(())
        }),
        is_commit: false,
    };
    Ok(())
}

fn remove_worktree(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(wt) = model.worktrees.get(selected) {
        if wt.is_current || wt.is_main {
            return Ok(()); // Can't remove current or main worktree
        }
        let path = wt.path.clone();
        let branch = wt.branch.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Remove worktree".to_string(),
            message: format!("Remove worktree '{}' ({})?\nThis won't delete the branch.", branch, path),
            on_confirm: Box::new(move |gui| {
                gui.git.remove_worktree(&path, false)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}
