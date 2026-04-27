use anyhow::Result;
use crossterm::event::KeyEvent;

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::{MenuItem, PopupState};
use crate::gui::Gui;
use crate::os::platform::Platform;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Enter: view commit files for reflog entry
    if key.code == crossterm::event::KeyCode::Enter {
        return enter_reflog_commit_files(gui);
    }

    // Checkout reflog entry
    if matches_key(key, &keybindings.commits.checkout_commit) {
        return checkout_reflog_entry(gui);
    }

    // View reset options
    if matches_key(key, &keybindings.commits.view_reset_options) {
        return show_reset_menu(gui);
    }

    // Cherry-pick
    if matches_key(key, &keybindings.commits.cherry_pick_copy) {
        return cherry_pick_reflog_entry(gui);
    }

    // Copy to clipboard
    if key.code == crossterm::event::KeyCode::Char('y') {
        return copy_to_clipboard_menu(gui);
    }

    // Open reflog commit in browser
    if key.code == crossterm::event::KeyCode::Char('o') {
        return open_in_browser(gui);
    }

    Ok(())
}

fn open_in_browser(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.reflog_commits.get(selected) {
        let hash = commit.hash.clone();
        drop(model);
        super::commits::open_commit_in_browser_menu_for(gui, hash);
    }
    Ok(())
}

fn enter_reflog_commit_files(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.reflog_commits.get(selected) {
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
        gui.commit_files_parent_context = Some(crate::gui::context::ContextId::Reflog);

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

        // Switch to CommitFiles context
        gui.context_mgr.set_active(crate::gui::context::ContextId::CommitFiles);
        gui.context_mgr.set_selection(0);
        gui.needs_diff_refresh = true;
    }
    Ok(())
}

fn checkout_reflog_entry(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.reflog_commits.get(selected) {
        let hash = commit.hash.clone();
        let short = commit.short_hash().to_string();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Checkout reflog entry".to_string(),
            message: format!("Checkout commit {}? (detached HEAD)", short),
            on_confirm: Box::new(move |gui| {
                gui.git.checkout_branch(&hash)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn show_reset_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.reflog_commits.get(selected) {
        let hash = commit.hash.clone();
        drop(model);

        let h1 = hash.clone();
        let h2 = hash.clone();
        let h3 = hash.clone();

        gui.popup = PopupState::Menu {
            title: "Reset to this commit".to_string(),
            items: vec![
                MenuItem {
                    label: "Soft reset".to_string(),
                    description: "Keep changes staged".to_string(),
                    key: Some("s".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.reset_to_commit(&h1, "--soft")?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Mixed reset".to_string(),
                    description: "Keep changes unstaged".to_string(),
                    key: Some("m".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.reset_to_commit(&h2, "--mixed")?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Hard reset".to_string(),
                    description: "Discard all changes".to_string(),
                    key: Some("h".to_string()),
                    action: Some(Box::new(move |gui| {
                        gui.git.reset_to_commit(&h3, "--hard")?;
                        gui.needs_refresh = true;
                        Ok(())
                    })),
                },
            ],
            selected: 0,
            loading_index: None,
        };
    }
    Ok(())
}

fn cherry_pick_reflog_entry(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.reflog_commits.get(selected) {
        let hash = commit.hash.clone();
        let short = commit.short_hash().to_string();
        drop(model);

        if !gui.cherry_pick_clipboard.contains(&hash) {
            gui.cherry_pick_clipboard.push(hash);
        }

        let n = gui.cherry_pick_clipboard.len();
        gui.popup = PopupState::Message {
            title: "Cherry-pick".to_string(),
            message: format!("Copied commit {} ({} commit{} copied)", short, n, if n == 1 { "" } else { "s" }),
            kind: crate::gui::popup::MessageKind::Info,
        };
    }
    Ok(())
}

fn copy_to_clipboard_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.reflog_commits.get(selected) {
        let hash = commit.hash.clone();
        let subject = commit.name.clone();
        drop(model);

        gui.popup = PopupState::Menu {
            title: "Copy to clipboard".to_string(),
            items: vec![
                MenuItem {
                    label: "Commit hash".to_string(),
                    description: String::new(),
                    key: None,
                    action: Some(Box::new(move |_gui| {
                        Platform::copy_to_clipboard(&hash)?;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Commit subject".to_string(),
                    description: String::new(),
                    key: Some("s".to_string()),
                    action: Some(Box::new(move |_gui| {
                        Platform::copy_to_clipboard(&subject)?;
                        Ok(())
                    })),
                },
            ],
            selected: 0,
            loading_index: None,
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
