use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::{CommitInputFocus, MenuItem, PopupState, make_textarea, make_commit_summary_textarea, make_commit_body_textarea};
use crate::gui::Gui;
use crate::os::platform::Platform;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Enter: toggle directory collapse in tree view, or focus diff for files
    if key.code == KeyCode::Enter {
        if gui.show_file_tree {
            let selected = gui.context_mgr.selected_active();
            if let Some(node) = gui.file_tree_nodes.get(selected) {
                if node.is_dir {
                    let path = node.path.clone();
                    if gui.collapsed_dirs.contains(&path) {
                        gui.collapsed_dirs.remove(&path);
                    } else {
                        gui.collapsed_dirs.insert(path);
                    }
                    gui.update_file_tree_state();
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

    // Stage/unstage toggle with space
    if key.code == KeyCode::Char(' ') {
        return toggle_stage(gui);
    }

    if matches_key(key, &keybindings.files.commit_changes) {
        return open_commit_prompt(gui);
    }

    if matches_key(key, &keybindings.files.toggle_staged_all) {
        return toggle_stage_all(gui);
    }

    if matches_key(key, &keybindings.files.stash_all_changes) {
        return stash_changes(gui);
    }

    if matches_key(key, &keybindings.files.view_stash_options) {
        return open_stash_options(gui);
    }

    if key.code == KeyCode::Char('d') {
        return discard_file(gui);
    }

    if matches_key(key, &keybindings.files.ignore_file) {
        return ignore_file(gui);
    }

    // Amend last commit
    if matches_key(key, &keybindings.files.amend_last_commit) {
        return amend_commit(gui);
    }

    // Commit with editor
    if matches_key(key, &keybindings.files.commit_changes_with_editor) {
        return commit_with_editor(gui);
    }

    // Toggle file tree view
    if matches_key(key, &keybindings.files.toggle_tree_view) {
        gui.show_file_tree = !gui.show_file_tree;
        gui.show_commit_file_tree = gui.show_file_tree;
        gui.update_file_tree_state();
        gui.persist_file_tree_visibility();
        // Reset selection when toggling view modes
        gui.context_mgr.set_selection(0);
        return Ok(());
    }

    // Open file in editor
    if matches_key(key, &keybindings.universal.edit) {
        return open_in_editor(gui);
    }

    // Open file in default program
    if matches_key(key, &keybindings.universal.open_file) {
        return open_in_default_program(gui);
    }

    // Copy to clipboard
    if key.code == KeyCode::Char('y') {
        return copy_to_clipboard_menu(gui);
    }

    // Fetch
    if matches_key(key, &keybindings.files.fetch) {
        gui.git.fetch_all()?;
        gui.needs_refresh = true;
        return Ok(());
    }

    Ok(())
}

fn toggle_stage(gui: &mut Gui) -> Result<()> {
    // If in tree view and a directory is selected, stage/unstage all child files
    if gui.show_file_tree {
        let selected = gui.context_mgr.selected_active();
        if let Some(node) = gui.file_tree_nodes.get(selected) {
            if node.is_dir {
                let child_indices = node.child_file_indices.clone();
                let model = gui.model.lock().unwrap();
                // Check if any child has unstaged changes
                let any_unstaged = child_indices.iter().any(|&i| {
                    model.files.get(i).map_or(false, |f| f.has_unstaged_changes || !f.tracked)
                });
                let names: Vec<String> = child_indices
                    .iter()
                    .filter_map(|&i| model.files.get(i).map(|f| f.name.clone()))
                    .collect();
                drop(model);

                if any_unstaged {
                    gui.git.stage_files(&names)?;
                } else {
                    gui.git.unstage_files(&names)?;
                }
                gui.needs_files_refresh = true;
                return Ok(());
            }
        }
    }

    let Some(file_idx) = gui.selected_file_index() else {
        return Ok(());
    };
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(file_idx) {
        let name = file.name.clone();
        let has_staged = file.has_staged_changes;
        let has_unstaged = file.has_unstaged_changes;
        drop(model);

        if has_unstaged || !has_staged {
            gui.git.stage_file(&name)?;
        } else {
            gui.git.unstage_file(&name)?;
        }
        gui.needs_files_refresh = true;
    }
    Ok(())
}

fn toggle_stage_all(gui: &mut Gui) -> Result<()> {
    let model = gui.model.lock().unwrap();
    let any_unstaged = model.files.iter().any(|f| f.has_unstaged_changes || !f.tracked);
    drop(model);

    if any_unstaged {
        gui.git.stage_all()?;
    } else {
        gui.git.unstage_all()?;
    }
    gui.needs_files_refresh = true;
    Ok(())
}

fn open_commit_prompt(gui: &mut Gui) -> Result<()> {
    let model = gui.model.lock().unwrap();
    let any_staged = model.files.iter().any(|f| f.has_staged_changes);
    drop(model);

    if !any_staged {
        // No files staged — ask to commit all, like lazygit
        gui.popup = PopupState::Confirm {
            title: "No files staged".to_string(),
            message: "You have not staged any files. Commit all files?".to_string(),
            on_confirm: Box::new(|gui| {
                gui.git.stage_all()?;
                gui.popup = PopupState::CommitInput {
                    summary_textarea: make_commit_summary_textarea(),
                    body_textarea: make_commit_body_textarea(),
                    focus: CommitInputFocus::Summary,
                    on_confirm: Box::new(|gui, message| {
                        if !message.is_empty() {
                            gui.git.create_commit(message, false)?;
                            gui.needs_refresh = true;
                        }
                        Ok(())
                    }),
                };
                Ok(())
            }),
        };
        return Ok(());
    }

    gui.popup = PopupState::CommitInput {
        summary_textarea: make_commit_summary_textarea(),
        body_textarea: make_commit_body_textarea(),
        focus: CommitInputFocus::Summary,
        on_confirm: Box::new(|gui, message| {
            if !message.is_empty() {
                gui.git.create_commit(message, false)?;
                gui.needs_refresh = true;
            }
            Ok(())
        }),
    };
    Ok(())
}

fn copy_to_clipboard_menu(gui: &mut Gui) -> Result<()> {
    let Some(file_idx) = gui.selected_file_index() else {
        return Ok(());
    };
    let model = gui.model.lock().unwrap();
    let Some(file) = model.files.get(file_idx) else {
        return Ok(());
    };
    let file_name = file.display_name.clone();
    let rel_path = file.name.clone();
    let is_added = file.added;
    let is_deleted = file.deleted;
    drop(model);

    let abs_path = gui.git.repo_path().join(&rel_path).to_string_lossy().to_string();
    let rel_for_diff = rel_path.clone();
    let file_name_copy = file_name.clone();
    let rel_path_copy = rel_path.clone();
    let path_for_old = rel_path.clone();
    let path_for_new = rel_path.clone();

    gui.popup = PopupState::Menu {
        title: "Copy to clipboard".to_string(),
        items: vec![
            MenuItem {
                label: "File name".to_string(),
                description: String::new(),
                key: Some("n".to_string()),
                action: Some(Box::new(move |_gui| {
                    Platform::copy_to_clipboard(&file_name_copy)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Relative path".to_string(),
                description: String::new(),
                key: Some("p".to_string()),
                action: Some(Box::new(move |_gui| {
                    Platform::copy_to_clipboard(&rel_path_copy)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Absolute path".to_string(),
                description: String::new(),
                key: Some("P".to_string()),
                action: Some(Box::new(move |_gui| {
                    Platform::copy_to_clipboard(&abs_path)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Old content (HEAD)".to_string(),
                description: if !is_added {
                    String::new()
                } else {
                    "File is new — no old content".to_string()
                },
                key: Some("o".to_string()),
                action: if !is_added {
                    Some(Box::new(move |gui| {
                        let content = gui.git.file_content_at_commit("HEAD", &path_for_old)?;
                        Platform::copy_to_clipboard(&content)?;
                        Ok(())
                    }))
                } else {
                    None
                },
            },
            MenuItem {
                label: "New content (working tree)".to_string(),
                description: if !is_deleted {
                    String::new()
                } else {
                    "File was deleted — no new content".to_string()
                },
                key: Some("w".to_string()),
                action: if !is_deleted {
                    Some(Box::new(move |gui| {
                        let content = gui.git.file_content(&path_for_new)?;
                        Platform::copy_to_clipboard(&content)?;
                        Ok(())
                    }))
                } else {
                    None
                },
            },
            MenuItem {
                label: "Diff of selected file".to_string(),
                description: String::new(),
                key: Some("s".to_string()),
                action: Some(Box::new(move |gui| {
                    let mut diff = gui.git.diff_file(&rel_for_diff).unwrap_or_default();
                    let staged = gui.git.diff_file_staged(&rel_for_diff).unwrap_or_default();
                    if !staged.is_empty() {
                        if !diff.is_empty() {
                            diff.push('\n');
                        }
                        diff.push_str(&staged);
                    }
                    Platform::copy_to_clipboard(&diff)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Diff of all files".to_string(),
                description: String::new(),
                key: Some("a".to_string()),
                action: Some(Box::new(|gui| {
                    let diff = gui.git.diff_all().unwrap_or_default();
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
    };
    Ok(())
}

fn open_in_editor(gui: &mut Gui) -> Result<()> {
    let Some(file_idx) = gui.selected_file_index() else {
        return Ok(());
    };
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(file_idx) {
        let rel_path = file.name.clone();
        drop(model);

        let abs_path = gui.git.repo_path().join(&rel_path).to_string_lossy().to_string();
        let edit_template = &gui.config.user_config.os.edit;
        if !edit_template.is_empty() {
            crate::config::user_config::OsConfig::run_template(edit_template, &abs_path)?;
        } else {
            // Fallback: use $EDITOR or platform open
            Platform::open_file(&abs_path)?;
        }
    }
    Ok(())
}

fn open_in_default_program(gui: &mut Gui) -> Result<()> {
    let Some(file_idx) = gui.selected_file_index() else {
        return Ok(());
    };
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(file_idx) {
        let rel_path = file.name.clone();
        drop(model);

        let abs_path = gui.git.repo_path().join(&rel_path).to_string_lossy().to_string();
        let open_template = &gui.config.user_config.os.open;
        crate::config::user_config::OsConfig::run_template(open_template, &abs_path)?;
    }
    Ok(())
}

fn open_stash_options(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Menu {
        title: "Stash options".to_string(),
        items: vec![
            MenuItem {
                label: "Stash all changes".to_string(),
                description: String::new(),
                key: Some("a".to_string()),
                action: Some(Box::new(|gui| {
                    open_stash_message_prompt(gui, StashKind::All);
                    Ok(())
                })),
            },
            MenuItem {
                label: "Stash all changes and keep index".to_string(),
                description: String::new(),
                key: Some("i".to_string()),
                action: Some(Box::new(|gui| {
                    open_stash_message_prompt(gui, StashKind::KeepIndex);
                    Ok(())
                })),
            },
            MenuItem {
                label: "Stash all changes including untracked files".to_string(),
                description: String::new(),
                key: Some("U".to_string()),
                action: Some(Box::new(|gui| {
                    open_stash_message_prompt(gui, StashKind::IncludeUntracked);
                    Ok(())
                })),
            },
            MenuItem {
                label: "Stash staged changes".to_string(),
                description: String::new(),
                key: Some("s".to_string()),
                action: Some(Box::new(|gui| {
                    open_stash_message_prompt(gui, StashKind::Staged);
                    Ok(())
                })),
            },
            MenuItem {
                label: "Stash unstaged changes".to_string(),
                description: String::new(),
                key: Some("u".to_string()),
                action: Some(Box::new(|gui| {
                    open_stash_message_prompt(gui, StashKind::Unstaged);
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
    };
    Ok(())
}

enum StashKind {
    All,
    KeepIndex,
    IncludeUntracked,
    Staged,
    Unstaged,
}

fn open_stash_message_prompt(gui: &mut Gui, kind: StashKind) {
    gui.popup = PopupState::Input {
        title: "Stash message (leave empty for default)".to_string(),
        textarea: make_textarea(""),
        on_confirm: Box::new(move |gui, message| {
            match kind {
                StashKind::All => gui.git.stash_save(message)?,
                StashKind::KeepIndex => gui.git.stash_keep_index(message)?,
                StashKind::IncludeUntracked => gui.git.stash_include_untracked(message)?,
                StashKind::Staged => gui.git.stash_staged(message)?,
                StashKind::Unstaged => gui.git.stash_unstaged(message)?,
            }
            gui.needs_refresh = true;
            Ok(())
        }),
        is_commit: false, confirm_focused: false,
    };
}

fn stash_changes(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "Stash message (leave empty for default)".to_string(),
        textarea: make_textarea(""),
        on_confirm: Box::new(|gui, message| {
            gui.git.stash_save(message)?;
            gui.needs_refresh = true;
            Ok(())
        }),
        is_commit: false, confirm_focused: false,
    };
    Ok(())
}

fn discard_file(gui: &mut Gui) -> Result<()> {
    // If in tree view and a directory is selected, discard all child files
    if gui.show_file_tree {
        let selected = gui.context_mgr.selected_active();
        if let Some(node) = gui.file_tree_nodes.get(selected) {
            if node.is_dir {
                let child_indices = node.child_file_indices.clone();
                let model = gui.model.lock().unwrap();
                let files_info: Vec<(String, bool)> = child_indices
                    .iter()
                    .filter_map(|&i| model.files.get(i).map(|f| (f.name.clone(), f.added)))
                    .collect();
                let dir_name = node.name.clone();
                drop(model);

                if files_info.is_empty() {
                    return Ok(());
                }

                if !gui.config.user_config.gui.skip_discard_change_warning {
                    let files_info_clone = files_info.clone();
                    gui.popup = PopupState::Menu {
                        title: format!("Discard all changes in '{}'?", dir_name),
                        items: vec![
                            MenuItem {
                                label: "Discard".to_string(),
                                description: "discard all changes".to_string(),
                                key: Some("d".to_string()),
                                action: Some(Box::new(move |gui| {
                                    for (name, added) in &files_info_clone {
                                        gui.git.discard_file(name, *added)?;
                                    }
                                    gui.needs_refresh = true;
                                    Ok(())
                                })),
                            },
                            MenuItem {
                                label: "Cancel".to_string(),
                                description: String::new(),
                                key: Some("c".to_string()),
                                action: Some(Box::new(|_| Ok(()))),
                            },
                        ],
                        selected: 0,
                    };
                } else {
                    for (name, added) in &files_info {
                        gui.git.discard_file(name, *added)?;
                    }
                    gui.needs_refresh = true;
                }
                return Ok(());
            }
        }
    }

    let Some(file_idx) = gui.selected_file_index() else {
        return Ok(());
    };
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(file_idx) {
        let name = file.name.clone();
        let added = file.added;
        drop(model);

        if !gui.config.user_config.gui.skip_discard_change_warning {
            let name_clone = name.clone();
            gui.popup = PopupState::Menu {
                title: format!("Discard changes to '{}'?", name),
                items: vec![
                    MenuItem {
                        label: "Discard".to_string(),
                        description: "discard all changes".to_string(),
                        key: Some("d".to_string()),
                        action: Some(Box::new(move |gui| {
                            gui.git.discard_file(&name_clone, added)?;
                            gui.needs_refresh = true;
                            Ok(())
                        })),
                    },
                    MenuItem {
                        label: "Cancel".to_string(),
                        description: String::new(),
                        key: Some("c".to_string()),
                        action: Some(Box::new(|_| Ok(()))),
                    },
                ],
                selected: 0,
            };
        } else {
            gui.git.discard_file(&name, added)?;
            gui.needs_refresh = true;
        }
    }
    Ok(())
}

fn ignore_file(gui: &mut Gui) -> Result<()> {
    let Some(file_idx) = gui.selected_file_index() else {
        return Ok(());
    };
    let model = gui.model.lock().unwrap();
    if let Some(file) = model.files.get(file_idx) {
        let name = file.name.clone();
        let display = file.display_name.clone();
        drop(model);

        let name_for_exclude = name.clone();
        gui.popup = PopupState::Menu {
            title: format!("Ignore '{}'", display),
            items: vec![
                MenuItem {
                    label: "Add to .gitignore".to_string(),
                    description: String::new(),
                    key: Some("i".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.ignore_file(&name)?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Add to .git/info/exclude".to_string(),
                    description: String::new(),
                    key: Some("e".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.exclude_file(&name_for_exclude)?;
                        gui.needs_refresh = true;
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
        };
    }
    Ok(())
}

fn amend_commit(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Confirm {
        title: "Amend".to_string(),
        message: "Amend last commit with staged changes?".to_string(),
        on_confirm: Box::new(|gui| {
            gui.git.amend_commit()?;
            gui.needs_refresh = true;
            Ok(())
        }),
    };
    Ok(())
}

fn commit_with_editor(gui: &mut Gui) -> Result<()> {
    // Run git commit which opens $EDITOR
    // This requires suspending the TUI temporarily
    gui.popup = PopupState::Input {
        title: "Commit message (or leave empty to open editor)".to_string(),
        textarea: make_textarea("Enter commit message..."),
        on_confirm: Box::new(|gui, message| {
            if message.is_empty() {
                // For now, just create an empty commit message prompt
                // Full editor integration requires Phase 4 (subprocess management)
            } else {
                gui.git.create_commit(message, false)?;
            }
            gui.needs_refresh = true;
            Ok(())
        }),
        is_commit: false, confirm_focused: false,
    };
    Ok(())
}

fn matches_key(key: KeyEvent, binding: &str) -> bool {
    if let Some(expected) = parse_key(binding) {
        key.code == expected.code && key.modifiers == expected.modifiers
    } else {
        false
    }
}
