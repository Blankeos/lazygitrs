use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::context::ContextId;
use crate::gui::popup::PopupState;
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Escape: go back to Remotes list
    if key.code == KeyCode::Esc {
        gui.context_mgr.set_active(ContextId::Remotes);
        {
            let mut model = gui.model.lock().unwrap();
            model.sub_remote_branches.clear();
        }
        gui.remote_branches_name.clear();
        return Ok(());
    }

    // Enter: drill into commits for the selected remote branch
    if key.code == KeyCode::Enter {
        return enter_remote_branch_commits(gui);
    }

    // Space: checkout remote branch (creates local tracking branch)
    if key.code == KeyCode::Char(' ') {
        return checkout_remote_branch(gui);
    }

    // M: merge remote branch into current
    if matches_key(key, &keybindings.branches.merge_into_current_branch) {
        return merge_remote_branch(gui);
    }

    // r: rebase onto remote branch
    if matches_key(key, &keybindings.branches.rebase_branch) {
        return rebase_remote_branch(gui);
    }

    // d: delete remote branch
    if key.code == KeyCode::Char('d') {
        return delete_remote_branch(gui);
    }

    Ok(())
}

fn enter_remote_branch_commits(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(rb) = model.sub_remote_branches.get(selected) {
        let full_name = rb.full_name();
        drop(model);

        // Load commits for this remote branch
        let commits = gui.git.load_commits_for_branch(&full_name, 300)?;
        {
            let mut model = gui.model.lock().unwrap();
            model.sub_commits = commits;
        }
        gui.branch_commits_name = full_name;
        gui.sub_commits_parent_context = ContextId::RemoteBranches;

        gui.context_mgr.set_active(ContextId::BranchCommits);
        gui.context_mgr.set_selection(0);
        gui.needs_diff_refresh = true;
    }
    Ok(())
}

fn checkout_remote_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(rb) = model.sub_remote_branches.get(selected) {
        let remote = rb.remote_name.clone();
        let branch = rb.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Checkout".to_string(),
            message: format!("Checkout '{}/{}' as local branch '{}'?", remote, branch, branch),
            on_confirm: Box::new(move |gui| {
                gui.git.checkout_remote_branch(&remote, &branch)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn merge_remote_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(rb) = model.sub_remote_branches.get(selected) {
        let remote = rb.remote_name.clone();
        let branch = rb.name.clone();
        let merge_args = gui.config.user_config.git.merging.args.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Merge".to_string(),
            message: format!("Merge '{}/{}' into current branch?", remote, branch),
            on_confirm: Box::new(move |gui| {
                gui.git.merge_remote_branch(&remote, &branch, &merge_args)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn rebase_remote_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(rb) = model.sub_remote_branches.get(selected) {
        let remote = rb.remote_name.clone();
        let branch = rb.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Rebase".to_string(),
            message: format!("Rebase onto '{}/{}'?", remote, branch),
            on_confirm: Box::new(move |gui| {
                gui.git.rebase_remote_branch(&remote, &branch)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn delete_remote_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(rb) = model.sub_remote_branches.get(selected) {
        let remote = rb.remote_name.clone();
        let branch = rb.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Delete remote branch".to_string(),
            message: format!("Delete remote branch '{}/{}'?\nThis will push --delete to the remote.", remote, branch),
            on_confirm: Box::new(move |gui| {
                gui.git.delete_remote_branch(&remote, &branch)?;
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
