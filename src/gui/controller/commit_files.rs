use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::context::ContextId;
use crate::gui::Gui;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Escape: go back to commits list
    if key.code == KeyCode::Esc {
        gui.context_mgr.set_active(ContextId::Commits);
        gui.commit_file_tree_nodes.clear();
        gui.commit_files_hash.clear();
        gui.needs_diff_refresh = true;
        return Ok(());
    }

    // Enter: toggle directory collapse in tree view, or focus diff for files
    if key.code == KeyCode::Enter {
        if gui.show_commit_file_tree {
            let selected = gui.context_mgr.selected_active();
            if let Some(node) = gui.commit_file_tree_nodes.get(selected) {
                if node.is_dir {
                    let path = node.path.clone();
                    if gui.commit_files_collapsed_dirs.contains(&path) {
                        gui.commit_files_collapsed_dirs.remove(&path);
                    } else {
                        gui.commit_files_collapsed_dirs.insert(path);
                    }
                    update_commit_file_tree_state(gui);
                    return Ok(());
                }
            }
        }
        // Focus the diff panel for the selected file
        if !gui.diff_view.is_empty() {
            gui.diff_focused = true;
        }
        return Ok(());
    }

    // Toggle file tree view
    if matches_key(key, &keybindings.files.toggle_tree_view) {
        gui.show_commit_file_tree = !gui.show_commit_file_tree;
        update_commit_file_tree_state(gui);
        gui.context_mgr.set_selection(0);
        return Ok(());
    }

    Ok(())
}

pub fn update_commit_file_tree_state(gui: &mut Gui) {
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
}

fn matches_key(key: KeyEvent, binding: &str) -> bool {
    if let Some(expected) = parse_key(binding) {
        key.code == expected.code && key.modifiers == expected.modifiers
    } else {
        false
    }
}
