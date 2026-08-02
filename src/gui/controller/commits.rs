use std::collections::HashSet;

use anyhow::Result;
use crossterm::event::KeyEvent;

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::git::rebase::RebaseAction;
use crate::gui::Gui;
use crate::gui::popup::{
    BodySoftWrap, CommitInputFocus, CommitInputKind, MenuItem, PopupState, make_textarea,
};
use crate::model::Branch;
use crate::os::platform::Platform;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Esc: cancel range select first, then clear clipboard
    if key.code == crossterm::event::KeyCode::Esc {
        if gui.range_select_anchor.is_some() {
            gui.range_select_anchor = None;
            return Ok(());
        }
        if !gui.cherry_pick_clipboard.is_empty() {
            gui.cherry_pick_clipboard.clear();
            return Ok(());
        }
    }

    // Enter: open commit files subview
    if key.code == crossterm::event::KeyCode::Enter {
        return enter_commit_files(gui);
    }

    if matches_key(key, &keybindings.commits.revert_commit) {
        return revert_commit(gui);
    }

    if matches_key(key, &keybindings.commits.rename_commit) {
        return reword_commit(gui);
    }

    if matches_key(key, &keybindings.commits.view_reset_options) {
        return show_reset_menu(gui);
    }

    if matches_key(key, &keybindings.commits.cherry_pick_copy) {
        return cherry_pick_copy(gui);
    }

    if matches_key(key, &keybindings.commits.paste_commits) {
        return paste_commits(gui);
    }

    if matches_key(key, &keybindings.commits.reset_cherry_pick) {
        gui.cherry_pick_clipboard.clear();
        return Ok(());
    }

    if matches_key(key, &keybindings.commits.squash_above_commits) {
        return squash_above_commits_menu(gui);
    }

    if matches_key(key, &keybindings.commits.tag_commit) {
        return tag_commit(gui);
    }

    // Squash down
    if matches_key(key, &keybindings.commits.squash_down) {
        return squash_commit(gui);
    }

    // Fixup
    if matches_key(key, &keybindings.commits.mark_commit_as_fixup) {
        return fixup_commit(gui);
    }

    // Drop commit
    if matches_key(key, &keybindings.commits.pick_commit) {
        return drop_commit(gui);
    }

    // Move commit up
    if matches_key(key, &keybindings.commits.move_up_commit) {
        return move_commit_up(gui);
    }

    // Move commit down
    if matches_key(key, &keybindings.commits.move_down_commit) {
        return move_commit_down(gui);
    }

    // Create fixup commit
    if matches_key(key, &keybindings.commits.create_fixup_commit) {
        return create_fixup_commit(gui);
    }

    // Amend to commit
    if matches_key(key, &keybindings.commits.amend_to_commit) {
        return amend_to_commit(gui);
    }

    // Bisect options
    if matches_key(key, &keybindings.commits.view_bisect_options) {
        return super::bisect::show_bisect_menu(gui);
    }

    // Checkout commit
    if matches_key(key, &keybindings.commits.checkout_commit) {
        return checkout_commit(gui);
    }

    // Drop commit(s) — opens interactive rebase planner with Drop pre-marked
    if key.code == crossterm::event::KeyCode::Char('d') {
        return apply_action_to_selection(gui, RebaseAction::Drop);
    }

    // Edit commit(s) — opens interactive rebase planner with Edit pre-marked
    if key.code == crossterm::event::KeyCode::Char('e') {
        return apply_action_to_selection(gui, RebaseAction::Edit);
    }

    // Open commit in browser
    if key.code == crossterm::event::KeyCode::Char('o') {
        return open_commit_in_browser_menu(gui);
    }

    // Copy to clipboard menu
    if key.code == crossterm::event::KeyCode::Char('y') {
        return copy_to_clipboard_menu(gui);
    }

    // Filter by branch
    if matches_key(key, &keybindings.commits.open_log_menu) {
        return show_branch_filter_menu(gui);
    }

    // Interactive rebase
    if matches_key(key, &keybindings.commits.interactive_rebase) {
        return enter_interactive_rebase(gui);
    }

    // Toggle range select
    if key.code == crossterm::event::KeyCode::Char('v') {
        return toggle_range_select(gui);
    }

    Ok(())
}

fn revert_commit(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        let short = commit.short_hash().to_string();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Revert commit".to_string(),
            message: format!("Revert commit {}?", short),
            on_confirm: Box::new(move |gui| {
                gui.git.revert_commit(&hash)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn reword_commit(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        let old_message = gui.git.commit_message_full(&hash)?;
        let (summary, body) = super::super::split_commit_message(&old_message);
        let is_head = selected == 0;
        drop(model);

        let mut summary_textarea = crate::gui::popup::make_commit_summary_textarea();
        summary_textarea.insert_str(&summary);
        let mut body_textarea = crate::gui::popup::make_commit_body_textarea();
        let body_state = BodySoftWrap::from_text(body);
        body_state.render_into(&mut body_textarea, gui.commit_body_wrap_width());

        gui.popup = PopupState::CommitInput {
            kind: CommitInputKind::Reword,
            summary_textarea,
            body_textarea,
            body_state,
            focus: CommitInputFocus::Summary,
            on_confirm: Box::new(move |gui, message| {
                if !message.is_empty() {
                    let message = message.to_string();
                    let hash = hash.clone();
                    if is_head {
                        gui.start_remote_op("Reword", "Rewording commit...", move |git| {
                            git.reword_commit(&hash, &message)?;
                            Ok(())
                        });
                    } else {
                        gui.start_remote_op("Reword", "Rewording commit...", move |git| {
                            git.reword_commit_rebase(&hash, &message)?;
                            Ok(())
                        });
                    }
                }
                Ok(())
            }),
        };
    }
    Ok(())
}

fn show_reset_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        drop(model);
        return show_reset_menu_for_ref(gui, &hash);
    }
    Ok(())
}

/// Shared soft/mixed/hard reset options for a branch, tag, or commit ref.
/// Used by contextual `g` handlers and the global `G` reset picker.
pub fn show_reset_menu_for_ref(gui: &mut Gui, ref_name: &str) -> Result<()> {
    let ref_name = ref_name.trim();
    if ref_name.is_empty() {
        return Ok(());
    }

    let display = reset_ref_display(ref_name);
    let r1 = ref_name.to_string();
    let r2 = ref_name.to_string();
    let r3 = ref_name.to_string();

    gui.popup = PopupState::Menu {
        title: format!("Reset current branch to {}", display),
        items: vec![
            MenuItem {
                label: "Soft reset".to_string(),
                description: "Keep changes staged".to_string(),
                key: Some("s".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.reset_to_commit(&r1, "--soft")?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Mixed reset".to_string(),
                description: "Keep changes unstaged".to_string(),
                key: Some("m".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.reset_to_commit(&r2, "--mixed")?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Hard reset".to_string(),
                description: "Discard all changes".to_string(),
                key: Some("h".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.reset_to_commit(&r3, "--hard")?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
        ],
        selected: 0,
        loading_index: None,
    };
    Ok(())
}

fn reset_ref_display(ref_name: &str) -> String {
    // Shorten bare commit hashes for the menu title; keep branch/tag names intact.
    if ref_name.len() >= 12 && ref_name.chars().all(|c| c.is_ascii_hexdigit()) {
        ref_name.chars().take(7).collect()
    } else {
        ref_name.to_string()
    }
}

fn cherry_pick_copy(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let (lo, hi) = if let Some(anchor) = gui.range_select_anchor {
        (anchor.min(selected), anchor.max(selected))
    } else {
        (selected, selected)
    };

    let model = gui.model.lock().unwrap();
    let mut added = 0;
    let mut removed = 0;
    for i in lo..=hi {
        if let Some(commit) = model.commits.get(i) {
            if let Some(index) = gui
                .cherry_pick_clipboard
                .iter()
                .position(|hash| hash == &commit.hash)
            {
                gui.cherry_pick_clipboard.remove(index);
                removed += 1;
            } else {
                gui.cherry_pick_clipboard.push(commit.hash.clone());
                added += 1;
            }
        }
    }
    drop(model);

    // Exit range select after copying
    gui.range_select_anchor = None;

    let n = gui.cherry_pick_clipboard.len();
    let message = match (added, removed) {
        (0, removed) => format!(
            "Uncopied {} commit{} ({} total)",
            removed,
            if removed == 1 { "" } else { "s" },
            n,
        ),
        (added, 0) => format!(
            "Copied {} commit{} ({} total)",
            added,
            if added == 1 { "" } else { "s" },
            n,
        ),
        (added, removed) => format!(
            "Copied {} and uncopied {} commit{} ({} total)",
            added,
            removed,
            if added + removed == 1 { "" } else { "s" },
            n,
        ),
    };
    gui.popup = PopupState::Message {
        title: "Cherry-pick".to_string(),
        message,
        kind: crate::gui::popup::MessageKind::Info,
    };
    Ok(())
}

fn toggle_range_select(gui: &mut Gui) -> Result<()> {
    if gui.range_select_anchor.is_some() {
        gui.range_select_anchor = None;
    } else {
        gui.range_select_anchor = Some(gui.context_mgr.selected_active());
    }
    Ok(())
}

fn paste_commits(gui: &mut Gui) -> Result<()> {
    if gui.cherry_pick_clipboard.is_empty() {
        gui.popup = PopupState::Message {
            title: "Cherry-pick".to_string(),
            message: "No commits copied. Use cherry-pick copy (C) first.".to_string(),
            kind: crate::gui::popup::MessageKind::Error,
        };
        return Ok(());
    }

    let n = gui.cherry_pick_clipboard.len();
    let mut hashes = gui.cherry_pick_clipboard.clone();
    let target_branch = {
        let model = gui.model.lock().unwrap();
        if model.head_branch_name.is_empty() {
            "detached HEAD".to_string()
        } else {
            format!("branch '{}'", model.head_branch_name)
        }
    };
    // The clipboard stores commits newest-first (matching the visual list order).
    // git cherry-pick applies commits in argument order, so we must reverse to
    // apply oldest-first and preserve the intended history.
    hashes.reverse();

    gui.popup = PopupState::Confirm {
        title: "Cherry-pick".to_string(),
        message: format!(
            "Cherry-pick {} copied commit{} onto {}?",
            n,
            if n == 1 { "" } else { "s" },
            target_branch,
        ),
        on_confirm: Box::new(move |gui| {
            gui.git.cherry_pick(&hashes)?;
            gui.cherry_pick_clipboard.clear();
            gui.needs_refresh = true;
            Ok(())
        }),
    };
    Ok(())
}

fn squash_above_commits_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    let commit = match model.commits.get(selected) {
        Some(c) => c.clone(),
        None => return Ok(()),
    };
    let commits_len = model.commits.len();
    drop(model);

    let hash_above = commit.hash.clone();

    // Find "in current branch" base: last commit before a merge-base boundary.
    // Use the last commit in the list as a simple heuristic.
    let model = gui.model.lock().unwrap();
    let last_hash = model
        .commits
        .last()
        .map(|c| c.hash.clone())
        .unwrap_or_default();
    drop(model);

    let last_hash_clone = last_hash.clone();

    gui.popup = PopupState::Menu {
        title: "Apply fixup commits".to_string(),
        items: vec![
            MenuItem {
                label: "Above the selected commit".to_string(),
                description: format!("Autosquash fixup! commits above {}", commit.short_hash()),
                key: Some("a".to_string()),
                action: if commits_len > 0 {
                    Some(Box::new(move |gui| {
                        gui.git.rebase_autosquash(&format!("{}^", hash_above))?;
                        gui.needs_refresh = true;
                        Ok(())
                    }))
                } else {
                    None
                },
            },
            MenuItem {
                label: "In current branch".to_string(),
                description: "Autosquash all fixup! commits in the branch".to_string(),
                key: Some("b".to_string()),
                action: if !last_hash.is_empty() {
                    Some(Box::new(move |gui| {
                        gui.git
                            .rebase_autosquash(&format!("{}^", last_hash_clone))?;
                        gui.needs_refresh = true;
                        Ok(())
                    }))
                } else {
                    None
                },
            },
        ],
        selected: 0,
        loading_index: None,
    };
    Ok(())
}

fn tag_commit(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let _hash = commit.hash.clone();
        drop(model);

        gui.popup = PopupState::Input {
            title: "Tag name".to_string(),
            textarea: make_textarea(""),
            on_confirm: Box::new(|gui, name| {
                if !name.is_empty() {
                    gui.git.create_tag(name, "")?;
                    gui.needs_refresh = true;
                }
                Ok(())
            }),
            is_commit: false,
            confirm_focused: false,
        };
    }
    Ok(())
}

fn squash_commit(gui: &mut Gui) -> Result<()> {
    apply_action_to_selection(gui, RebaseAction::Squash)
}

fn fixup_commit(gui: &mut Gui) -> Result<()> {
    apply_action_to_selection(gui, RebaseAction::Fixup)
}

fn drop_commit(gui: &mut Gui) -> Result<()> {
    apply_action_to_selection(gui, RebaseAction::Drop)
}

/// Open the Interactive Rebase planner with the user's current commit
/// selection pre-marked with `action`. Selection comes from range-select
/// (`v`) when active; otherwise it's just the focused commit. The rebase
/// base is the parent of the oldest selected commit, so unselected newer
/// commits are kept as `pick`.
fn apply_action_to_selection(gui: &mut Gui, action: RebaseAction) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let (lo, hi) = if let Some(anchor) = gui.range_select_anchor {
        (anchor.min(selected), anchor.max(selected))
    } else {
        (selected, selected)
    };

    let model = gui.model.lock().unwrap();

    if model.commits.get(lo).is_none() {
        return Ok(());
    }

    let base_idx = hi + 1;
    let base_commit = match model.commits.get(base_idx) {
        Some(c) => c.clone(),
        None => {
            drop(model);
            gui.popup = PopupState::Message {
                title: "Interactive rebase".to_string(),
                message: "Cannot rebase past the oldest visible commit.".to_string(),
                kind: crate::gui::popup::MessageKind::Error,
            };
            return Ok(());
        }
    };

    let commits_to_rebase: Vec<_> = model.commits[0..=hi].to_vec();
    let branch_name = model.head_branch_name.clone();
    let selected_hashes: HashSet<String> = model.commits[lo..=hi]
        .iter()
        .map(|c| c.hash.clone())
        .collect();
    drop(model);

    gui.range_select_anchor = None;

    gui.rebase_mode
        .enter(branch_name, &base_commit, &commits_to_rebase);

    for entry in gui.rebase_mode.entries.iter_mut() {
        if selected_hashes.contains(&entry.hash) {
            entry.action = action;
        }
    }

    gui.rebase_mode.selected = lo;

    Ok(())
}

fn move_commit_up(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    if selected == 0 {
        return Ok(());
    }
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        drop(model);
        gui.git.move_commit_up(&hash)?;
        gui.needs_refresh = true;
    }
    Ok(())
}

fn move_commit_down(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    let commits_len = model.commits.len();
    if let Some(commit) = model.commits.get(selected) {
        if selected >= commits_len - 1 {
            drop(model);
            return Ok(());
        }
        let hash = commit.hash.clone();
        drop(model);
        gui.git.move_commit_down(&hash)?;
        gui.needs_refresh = true;
    }
    Ok(())
}

fn create_fixup_commit(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        let short = commit.short_hash().to_string();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Create fixup commit".to_string(),
            message: format!("Create fixup commit for {}?", short),
            on_confirm: Box::new(move |gui| {
                gui.git.create_fixup_commit(&hash)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn amend_to_commit(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        if selected == 0 {
            // HEAD commit — just amend
            drop(model);
            gui.popup = PopupState::Confirm {
                title: "Amend".to_string(),
                message: "Amend staged changes to HEAD commit?".to_string(),
                on_confirm: Box::new(|gui| {
                    gui.start_remote_op("Amend", "Amending commit...", |git| {
                        git.amend_commit()?;
                        Ok(())
                    });
                    Ok(())
                }),
            };
        } else {
            // Non-HEAD: create fixup commit + autosquash
            let hash = commit.hash.clone();
            let short = commit.short_hash().to_string();
            drop(model);

            gui.popup = PopupState::Confirm {
                title: "Amend to commit".to_string(),
                message: format!("Amend staged changes to commit {}?", short),
                on_confirm: Box::new(move |gui| {
                    let hash = hash.clone();
                    gui.start_remote_op("Amend", "Amending commit...", move |git| {
                        git.create_fixup_commit(&hash)?;
                        git.rebase_autosquash(&format!("{}^", hash))?;
                        Ok(())
                    });
                    Ok(())
                }),
            };
        }
    }
    Ok(())
}

fn checkout_commit(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        let short = commit.short_hash().to_string();
        let branch_names = branches_at_commit(&hash, &model.branches, &model.head_branch_name);
        drop(model);

        let mut items = vec![MenuItem {
            label: format!("Checkout commit {} as detached head", short),
            description: String::new(),
            key: Some("d".to_string()),
            action: Some(Box::new(move |gui| {
                gui.git.checkout_branch(&hash)?;
                gui.needs_refresh = true;
                Ok(())
            })),
        }];

        if branch_names.is_empty() {
            items.push(MenuItem {
                label: "Checkout branch".to_string(),
                description: "No branches found at selected commit.".to_string(),
                key: Some("1".to_string()),
                action: None,
            });
        } else {
            items.extend(
                branch_names
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| MenuItem {
                        label: format!("Checkout branch '{}'", name),
                        description: String::new(),
                        key: checkout_branch_key(index),
                        action: Some(Box::new(move |gui| {
                            gui.git.checkout_branch(&name)?;
                            gui.needs_refresh = true;
                            Ok(())
                        })),
                    }),
            );
        }

        gui.popup = PopupState::Menu {
            title: "Checkout branch or commit".to_string(),
            items,
            selected: 0,
            loading_index: None,
        };
    }
    Ok(())
}

fn branches_at_commit(commit_hash: &str, branches: &[Branch], current_branch: &str) -> Vec<String> {
    branches
        .iter()
        .filter(|branch| {
            branch.name != current_branch
                && !branch.hash.is_empty()
                && (branch.hash == commit_hash || commit_hash.starts_with(&branch.hash))
        })
        .map(|branch| branch.name.clone())
        .collect()
}

fn checkout_branch_key(index: usize) -> Option<String> {
    (index < 9).then(|| (index + 1).to_string())
}

fn open_commit_in_browser_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        drop(model);
        open_commit_in_browser_menu_for(gui, hash);
    }
    Ok(())
}

pub fn open_commit_in_browser_menu_for(gui: &mut Gui, hash: String) {
    gui.popup = PopupState::Menu {
        title: "Open in browser".to_string(),
        items: vec![MenuItem {
            label: "Open commit URL".to_string(),
            description: String::new(),
            key: Some("c".to_string()),
            action: Some(Box::new(move |gui| {
                let url = gui.git.get_commit_url(&hash)?;
                Platform::open_file(&url)?;
                Ok(())
            })),
        }],
        selected: 0,
        loading_index: None,
    };
}

fn copy_to_clipboard_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        let subject = commit.name.clone();
        let author = commit.author_name.clone();
        let tags = commit.tags.clone();
        drop(model);
        copy_commit_to_clipboard_menu_for(gui, hash, subject, author, tags);
    }
    Ok(())
}

pub fn copy_commit_to_clipboard_menu_for(
    gui: &mut Gui,
    hash: String,
    subject: String,
    author: String,
    tag_list: Vec<String>,
) {
    {
        let tags = tag_list.join(", ");
        let has_tags = !tag_list.is_empty();
        let hash_for_url = hash.clone();
        let hash_for_msg = hash.clone();
        let hash_for_body = hash.clone();
        let hash_for_diff = hash.clone();

        // Check if commit has a body (for strikethrough on empty)
        let has_body = gui
            .git
            .commit_message_body(&hash)
            .map(|b| !b.trim().is_empty())
            .unwrap_or(false);

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
                MenuItem {
                    label: "Commit message (subject and body)".to_string(),
                    description: String::new(),
                    key: Some("m".to_string()),
                    action: Some(Box::new(move |gui| {
                        let msg = gui.git.commit_message_full(&hash_for_msg)?;
                        Platform::copy_to_clipboard(&msg)?;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Commit message body".to_string(),
                    description: if has_body {
                        String::new()
                    } else {
                        "Commit has no message body".to_string()
                    },
                    key: Some("b".to_string()),
                    action: if has_body {
                        Some(Box::new(move |gui| {
                            let body = gui.git.commit_message_body(&hash_for_body)?;
                            Platform::copy_to_clipboard(&body)?;
                            Ok(())
                        }))
                    } else {
                        None
                    },
                },
                MenuItem {
                    label: "Commit URL".to_string(),
                    description: String::new(),
                    key: Some("u".to_string()),
                    action: Some(Box::new(move |gui| {
                        if let Ok(url) = gui.git.get_commit_url(&hash_for_url) {
                            Platform::copy_to_clipboard(&url)?;
                        }
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Commit diff".to_string(),
                    description: String::new(),
                    key: Some("d".to_string()),
                    action: Some(Box::new(move |gui| {
                        let diff = gui.git.commit_diff(&hash_for_diff)?;
                        Platform::copy_to_clipboard(&diff)?;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Commit author".to_string(),
                    description: String::new(),
                    key: Some("a".to_string()),
                    action: Some(Box::new(move |_gui| {
                        Platform::copy_to_clipboard(&author)?;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Commit tags".to_string(),
                    description: if has_tags {
                        String::new()
                    } else {
                        "Commit has no tags".to_string()
                    },
                    key: Some("t".to_string()),
                    action: if has_tags {
                        Some(Box::new(move |_gui| {
                            Platform::copy_to_clipboard(&tags)?;
                            Ok(())
                        }))
                    } else {
                        None
                    },
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
    }
}

fn show_branch_filter_menu(gui: &mut Gui) -> Result<()> {
    use crate::gui::popup::ChecklistItem;

    let model = gui.model.lock().unwrap();
    let branches: Vec<String> = model.branches.iter().map(|b| b.name.clone()).collect();
    drop(model);

    let current_filter = &gui.commit_branch_filter;

    let items: Vec<ChecklistItem> = branches
        .into_iter()
        .map(|name| {
            let checked = current_filter.contains(&name);
            ChecklistItem {
                label: name,
                checked,
            }
        })
        .collect();

    gui.popup = PopupState::Checklist {
        title: "Filter commits by branch".to_string(),
        items,
        selected: 0,
        search: String::new(),
        on_confirm: Box::new(|gui: &mut Gui, checked: Vec<String>| {
            gui.commit_branch_filter = checked;
            gui.needs_refresh = true;
            gui.context_mgr.set_selection(0);
            Ok(())
        }),
    };

    Ok(())
}

fn enter_commit_files(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
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
            gui.context_mgr.commit_files_list_len_override = Some(gui.commit_file_tree_nodes.len());
        } else {
            gui.commit_file_tree_nodes.clear();
            let model = gui.model.lock().unwrap();
            gui.context_mgr.commit_files_list_len_override = None;
            let _ = model.commit_files.len(); // just ensure it compiles
        }

        // Switch to CommitFiles context
        gui.context_mgr
            .set_active(crate::gui::context::ContextId::CommitFiles);
        gui.context_mgr.set_selection(0);
        gui.needs_diff_refresh = true;
    }
    Ok(())
}

fn enter_interactive_rebase(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();

    let base_commit = match model.commits.get(selected) {
        Some(c) => c,
        None => return Ok(()),
    };

    if base_commit.hash == model.head_hash {
        gui.popup = PopupState::Message {
            title: "Interactive rebase".to_string(),
            message: "Cannot rebase HEAD onto itself. Select a different commit.".to_string(),
            kind: crate::gui::popup::MessageKind::Error,
        };
        drop(model);
        return Ok(());
    }

    let base_hash = base_commit.hash.clone();
    let branch_name = model.head_branch_name.clone();

    // Ask git which commits would be rebased (handles all cases correctly:
    // base above HEAD, base below HEAD, non-linear histories, etc.)
    let rebase_hashes = match gui.git.rebase_commit_range(&base_hash) {
        Ok(hashes) => hashes,
        Err(e) => {
            gui.popup = PopupState::Message {
                title: "Interactive rebase".to_string(),
                message: format!("Failed to determine rebase range: {}", e),
                kind: crate::gui::popup::MessageKind::Error,
            };
            drop(model);
            return Ok(());
        }
    };

    if rebase_hashes.is_empty() {
        gui.popup = PopupState::Message {
            title: "Interactive rebase".to_string(),
            message: "No commits to rebase.".to_string(),
            kind: crate::gui::popup::MessageKind::Error,
        };
        drop(model);
        return Ok(());
    }

    // Match the hashes to model commits (preserving git's order: newest-first)
    let commits_to_rebase: Vec<_> = rebase_hashes
        .iter()
        .filter_map(|hash| model.commits.iter().find(|c| c.hash == *hash))
        .cloned()
        .collect();

    if commits_to_rebase.is_empty() {
        gui.popup = PopupState::Message {
            title: "Interactive rebase".to_string(),
            message: "No commits to rebase.".to_string(),
            kind: crate::gui::popup::MessageKind::Error,
        };
        drop(model);
        return Ok(());
    }

    gui.rebase_mode
        .enter(branch_name, base_commit, &commits_to_rebase);
    drop(model);

    Ok(())
}

fn matches_key(key: KeyEvent, binding: &str) -> bool {
    if let Some(expected) = parse_key(binding) {
        key.code == expected.code && key.modifiers == expected.modifiers
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{branches_at_commit, checkout_branch_key, reset_ref_display};
    use crate::model::Branch;

    fn branch(name: &str, hash: &str) -> Branch {
        Branch {
            name: name.to_string(),
            hash: hash.to_string(),
            recency: String::new(),
            pushables: String::new(),
            pullables: String::new(),
            upstream: None,
            head: false,
        }
    }

    #[test]
    fn finds_local_branches_at_commit_in_model_order() {
        let branches = vec![
            branch("other", "9999999"),
            branch("feature", "b838172"),
            branch("release", "b838172aabbccdd"),
        ];

        assert_eq!(
            branches_at_commit("b838172aabbccdd", &branches, "main"),
            vec!["feature", "release"]
        );
    }

    #[test]
    fn excludes_the_current_branch_and_empty_hashes() {
        let branches = vec![
            branch("main", "b838172"),
            branch("feature", "b838172"),
            branch("invalid", ""),
        ];

        assert_eq!(
            branches_at_commit("b838172aabbccdd", &branches, "main"),
            vec!["feature"]
        );
    }

    #[test]
    fn assigns_number_shortcuts_to_the_first_nine_branches() {
        assert_eq!(checkout_branch_key(0).as_deref(), Some("1"));
        assert_eq!(checkout_branch_key(8).as_deref(), Some("9"));
        assert_eq!(checkout_branch_key(9), None);
    }

    #[test]
    fn reset_ref_display_shortens_full_hashes_but_keeps_names() {
        assert_eq!(reset_ref_display("b838172aabbccdd1122334455"), "b838172");
        assert_eq!(reset_ref_display("main"), "main");
        assert_eq!(reset_ref_display("v1.2.3"), "v1.2.3");
        // Short hashes and non-hex strings are left as-is.
        assert_eq!(reset_ref_display("b838172"), "b838172");
        assert_eq!(reset_ref_display("feature/foo"), "feature/foo");
    }
}
