use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::gui::context::ContextId;
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, _keybindings: &KeybindingConfig) -> Result<()> {
    // Escape: go back to parent context (Branches or Tags)
    if key.code == KeyCode::Esc {
        let parent = gui.sub_commits_parent_context;
        gui.context_mgr.set_active(parent);
        {
            let mut model = gui.model.lock().unwrap();
            model.sub_commits.clear();
        }
        gui.branch_commits_name.clear();
        gui.sub_commits_parent_context = ContextId::Branches;
        gui.needs_diff_refresh = true;
        return Ok(());
    }

    // Enter: open commit files for the selected commit
    if key.code == KeyCode::Enter {
        return enter_branch_commit_files(gui);
    }

    Ok(())
}

fn enter_branch_commit_files(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.sub_commits.get(selected) {
        let hash = commit.hash.clone();
        let message = commit.name.clone();
        drop(model);

        // Load commit files
        let commit_files = gui.git.commit_files(&hash)?;
        {
            let mut model = gui.model.lock().unwrap();
            model.commit_files = commit_files;
        }
        gui.commit_files_hash = hash;
        gui.commit_files_message = message;

        // Build commit file tree
        if gui.show_commit_file_tree {
            let model = gui.model.lock().unwrap();
            gui.commit_file_tree_nodes = crate::model::file_tree::build_commit_file_tree(
                &model.commit_files,
                &gui.commit_files_collapsed_dirs,
            );
            gui.context_mgr.commit_files_list_len_override =
                Some(gui.commit_file_tree_nodes.len());
        } else {
            gui.commit_file_tree_nodes.clear();
            gui.context_mgr.commit_files_list_len_override = None;
        }

        // Switch to BranchCommitFiles context
        gui.context_mgr.set_active(ContextId::BranchCommitFiles);
        gui.context_mgr.set_selection(0);
        gui.needs_diff_refresh = true;
    }
    Ok(())
}
