use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::context::ContextId;
use crate::gui::popup::{MenuItem, PopupState};
use crate::gui::Gui;
use crate::model::FileChangeStatus;
use crate::os::platform::Platform;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Escape: go back to parent list (Commits, Stash, BranchCommits, or Reflog)
    if key.code == KeyCode::Esc {
        let parent = if let Some(override_parent) = gui.commit_files_parent_context.take() {
            override_parent
        } else {
            match gui.context_mgr.active() {
                ContextId::StashFiles => ContextId::Stash,
                ContextId::BranchCommitFiles => ContextId::BranchCommits,
                _ => ContextId::Commits,
            }
        };
        gui.context_mgr.set_active(parent);
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
        gui.show_file_tree = gui.show_commit_file_tree;
        update_commit_file_tree_state(gui);
        gui.persist_file_tree_visibility();
        gui.context_mgr.set_selection(0);
        return Ok(());
    }

    // Copy to clipboard
    if key.code == KeyCode::Char('y') {
        return copy_to_clipboard_menu(gui);
    }

    Ok(())
}

fn copy_to_clipboard_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();

    // Resolve file index (tree view maps node -> file index)
    let file_idx = if gui.show_commit_file_tree {
        gui.commit_file_tree_nodes.get(selected).and_then(|n| n.file_index)
    } else {
        Some(selected)
    };

    let model = gui.model.lock().unwrap();
    let Some(idx) = file_idx else { return Ok(()) };
    let Some(file) = model.commit_files.get(idx) else { return Ok(()) };

    let file_name = file.name.clone();
    let status = file.status;
    let hash = gui.commit_files_hash.clone();
    drop(model);

    if hash.is_empty() {
        return Ok(());
    }

    let path_for_old = file_name.clone();
    let path_for_new = file_name.clone();
    let path_for_diff = file_name.clone();
    let hash_for_old = hash.clone();
    let hash_for_new = hash.clone();
    let hash_for_diff = hash.clone();

    // Added files have no old content, Deleted files have no new content
    let has_old = !matches!(status, FileChangeStatus::Added);
    let has_new = !matches!(status, FileChangeStatus::Deleted);

    gui.popup = PopupState::Menu {
        title: "Copy to clipboard".to_string(),
        items: vec![
            MenuItem {
                label: "File name".to_string(),
                description: String::new(),
                key: Some("n".to_string()),
                action: Some(Box::new(move |_gui| {
                    Platform::copy_to_clipboard(&file_name)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Old content (parent)".to_string(),
                description: if has_old {
                    String::new()
                } else {
                    "File was added — no old content".to_string()
                },
                key: Some("o".to_string()),
                action: if has_old {
                    Some(Box::new(move |gui| {
                        let parent_ref = format!("{}^1", hash_for_old);
                        let content =
                            gui.git.file_content_at_commit(&parent_ref, &path_for_old)?;
                        Platform::copy_to_clipboard(&content)?;
                        Ok(())
                    }))
                } else {
                    None
                },
            },
            MenuItem {
                label: "New content (commit)".to_string(),
                description: if has_new {
                    String::new()
                } else {
                    "File was deleted — no new content".to_string()
                },
                key: Some("w".to_string()),
                action: if has_new {
                    Some(Box::new(move |gui| {
                        let content =
                            gui.git.file_content_at_commit(&hash_for_new, &path_for_new)?;
                        Platform::copy_to_clipboard(&content)?;
                        Ok(())
                    }))
                } else {
                    None
                },
            },
            MenuItem {
                label: "Diff".to_string(),
                description: String::new(),
                key: Some("d".to_string()),
                action: Some(Box::new(move |gui| {
                    let diff = gui
                        .git
                        .diff_commit_file(&hash_for_diff, &path_for_diff)?;
                    Platform::copy_to_clipboard(&diff)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Cancel".to_string(),
                description: String::new(),
                key: None,
                action: Some(Box::new(|_| Ok(()))),
            },
        ],
        selected: 0,
        loading_index: None,
    };
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
