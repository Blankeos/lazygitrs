use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::{MenuItem, MessageKind, PopupState, make_textarea};
use crate::gui::Gui;
use crate::os::platform::Platform;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Enter: view branch commits
    if key.code == KeyCode::Enter {
        return enter_branch_commits(gui);
    }

    // Checkout with space
    if key.code == KeyCode::Char(' ') {
        return checkout_branch(gui);
    }

    if key.code == KeyCode::Char('n') {
        return new_branch(gui);
    }

    // c: checkout (ref picker)
    if key.code == KeyCode::Char('c') {
        return checkout_picker(gui);
    }

    // -: checkout previous branch (git checkout -)
    if key.code == KeyCode::Char('-') {
        return checkout_previous(gui);
    }

    if key.code == KeyCode::Char('d') {
        return delete_branch(gui);
    }

    if matches_key(key, &keybindings.branches.merge_into_current_branch) {
        return merge_branch(gui);
    }

    if matches_key(key, &keybindings.branches.rebase_branch) {
        return rebase_branch(gui);
    }

    if matches_key(key, &keybindings.branches.rename_branch) {
        return rename_branch(gui);
    }

    if matches_key(key, &keybindings.branches.fast_forward) {
        return fast_forward(gui);
    }

    if matches_key(key, &keybindings.branches.set_upstream) {
        return set_upstream(gui);
    }

    // y: Copy to clipboard menu (repo url, PR create url, PR url)
    if key.code == KeyCode::Char('y') {
        return copy_to_clipboard_menu(gui);
    }

    // o: Open in browser menu (repo url, PR create url, PR url)
    if matches_key(key, &keybindings.branches.create_pull_request) {
        return open_in_browser_menu(gui);
    }

    Ok(())
}

fn enter_branch_commits(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let name = branch.name.clone();
        drop(model);

        // Load commits for this branch
        let commits = gui.git.load_commits_for_branch(&name, 300)?;
        {
            let mut model = gui.model.lock().unwrap();
            model.sub_commits = commits;
        }
        gui.branch_commits_name = name;

        // Switch to BranchCommits context
        gui.context_mgr
            .set_active(crate::gui::context::ContextId::BranchCommits);
        gui.context_mgr.set_selection(0);
        gui.needs_diff_refresh = true;
    }
    Ok(())
}

fn checkout_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        if branch.head {
            return Ok(()); // Already on this branch
        }
        let name = branch.name.clone();
        drop(model);
        show_checkout_error_or_refresh(gui, &name)?;
    }
    Ok(())
}

fn checkout_previous(gui: &mut Gui) -> Result<()> {
    show_checkout_error_or_refresh(gui, "-")?;
    Ok(())
}

fn checkout_picker(gui: &mut Gui) -> Result<()> {
    use crate::gui::popup::{ListPickerItem, ListPickerCore, make_help_search_textarea};

    let model = gui.model.lock().unwrap();
    let mut items = Vec::new();

    if let Some(prev) = gui.git.previous_branch_name() {
        items.push(ListPickerItem {
            value: "-".to_string(),
            // Label includes both "previous branch" and "prev branch" so that
            // typing either phrase (or "-") jumps to this entry.
            label: format!("Go to previous branch — {}", prev),
            category: "Quick Actions".to_string(),
        });
    }

    for branch in &model.branches {
        if branch.head {
            continue;
        }
        items.push(ListPickerItem {
            value: branch.name.clone(),
            label: branch.name.clone(),
            category: "Branches".to_string(),
        });
    }

    for remote in &model.remotes {
        for branch in &remote.branches {
            let full_name = format!("{}/{}", remote.name, branch.name);
            items.push(ListPickerItem {
                value: full_name.clone(),
                label: full_name,
                category: "Remote Branches".to_string(),
            });
        }
    }

    for tag in &model.tags {
        items.push(ListPickerItem {
            value: tag.name.clone(),
            label: tag.name.clone(),
            category: "Tags".to_string(),
        });
    }

    for commit in &model.commits {
        items.push(ListPickerItem {
            value: commit.hash.clone(),
            label: format!("{} {}", commit.short_hash(), commit.name),
            category: "Commits".to_string(),
        });
    }

    drop(model);

    gui.popup = PopupState::RefPicker {
        title: "Checkout".to_string(),
        core: ListPickerCore {
            items,
            selected: 0,
            search_textarea: make_help_search_textarea(),
            scroll_offset: 0,
        },
        on_confirm: Box::new(|gui, ref_name| {
            show_checkout_error_or_refresh(gui, ref_name)?;
            Ok(())
        }),
    };
    Ok(())
}

fn show_checkout_error_or_refresh(gui: &mut Gui, name: &str) -> Result<()> {
    match gui.git.checkout_branch(name) {
        Ok(()) => {
            gui.needs_refresh = true;
        }
        Err(e) => {
            gui.popup = PopupState::Message {
                title: "Checkout error".to_string(),
                message: format!("{}", e),
                kind: MessageKind::Error,
            };
        }
    }
    Ok(())
}

fn new_branch(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "New branch name".to_string(),
        textarea: make_textarea(""),
        on_confirm: Box::new(|gui, name| {
            if !name.is_empty() {
                gui.git.create_branch(name)?;
                gui.needs_refresh = true;
            }
            Ok(())
        }),
        is_commit: false, confirm_focused: false,
    };
    Ok(())
}

fn delete_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        if branch.head {
            return Ok(()); // Can't delete current branch
        }
        let name = branch.name.clone();
        let has_remote = branch.upstream.is_some();
        let upstream = branch.upstream.clone();
        drop(model);

        let name_local = name.clone();
        let name_remote = name.clone();
        let name_both = name.clone();
        let upstream_for_remote = upstream.clone();
        let upstream_for_both = upstream.clone();

        let mut items = vec![
            MenuItem {
                label: "Delete local branch".to_string(),
                description: String::new(),
                key: Some("c".to_string()),
                action: Some(Box::new(move |gui| {
                    match gui.git.delete_branch(&name_local, false) {
                        Ok(()) => {
                            gui.needs_refresh = true;
                        }
                        Err(e) => {
                            let err_msg = format!("{}", e);
                            if err_msg.contains("not fully merged") {
                                let name_force = name_local.clone();
                                gui.popup = PopupState::Confirm {
                                    title: "Force delete?".to_string(),
                                    message: format!(
                                        "'{}' is not fully merged. Are you sure you want to delete it?",
                                        name_local
                                    ),
                                    on_confirm: Box::new(move |gui| {
                                        gui.git.delete_branch(&name_force, true)?;
                                        gui.needs_refresh = true;
                                        Ok(())
                                    }),
                                };
                            } else {
                                return Err(e);
                            }
                        }
                    }
                    Ok(())
                })),
            },
        ];

        if has_remote {
            items.push(MenuItem {
                label: "Delete remote branch".to_string(),
                description: String::new(),
                key: Some("r".to_string()),
                action: Some(Box::new(move |gui| {
                    // Parse remote name from upstream (e.g. "origin/branch" -> "origin")
                    let remote = upstream_for_remote
                        .as_deref()
                        .and_then(|u| u.split('/').next())
                        .unwrap_or("origin");
                    gui.git.delete_remote_branch(remote, &name_remote)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            });
        } else {
            items.push(MenuItem {
                label: "Delete remote branch".to_string(),
                description: "No remote tracking branch".to_string(),
                key: Some("r".to_string()),
                action: None,
            });
        }

        if has_remote {
            items.push(MenuItem {
                label: "Delete local and remote branch".to_string(),
                description: String::new(),
                key: Some("b".to_string()),
                action: Some(Box::new(move |gui| {
                    let remote = upstream_for_both
                        .as_deref()
                        .and_then(|u| u.split('/').next())
                        .unwrap_or("origin");
                    // Delete local first (force, since we're deleting remote too)
                    gui.git.delete_branch(&name_both, true)?;
                    gui.git.delete_remote_branch(remote, &name_both)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            });
        } else {
            items.push(MenuItem {
                label: "Delete local and remote branch".to_string(),
                description: "No remote tracking branch".to_string(),
                key: Some("b".to_string()),
                action: None,
            });
        }

        items.push(MenuItem {
            label: "Cancel".to_string(),
            description: String::new(),
            key: None,
            action: Some(Box::new(|_| Ok(()))),
        });

        gui.popup = PopupState::Menu {
            title: format!("Delete branch '{}'?", name),
            items,
            selected: 0,
            loading_index: None,
        };
    }
    Ok(())
}

fn merge_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let name = branch.name.clone();
        let merge_args = gui.config.user_config.git.merging.args.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Merge".to_string(),
            message: format!("Merge '{}' into current branch?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.merge_branch(&name, &merge_args)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn rebase_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    let current_branch = model.head_branch_name.clone();
    if let Some(branch) = model.branches.get(selected) {
        let name = branch.name.clone();
        let is_same_branch = name == current_branch;
        let name_for_simple = name.clone();
        let name_for_interactive = name.clone();
        let name_for_base = name.clone();
        drop(model);

        let items = vec![
            MenuItem {
                label: format!("Simple rebase onto '{}'", name_for_simple),
                description: if is_same_branch { "Already on this branch".to_string() } else { String::new() },
                key: Some("s".to_string()),
                action: if is_same_branch { None } else { Some(Box::new(move |gui| {
                    gui.git.rebase_branch(&name_for_simple)?;
                    gui.needs_refresh = true;
                    Ok(())
                })) },
            },
            MenuItem {
                label: format!("Interactive rebase onto '{}'", name_for_interactive),
                description: if is_same_branch { "Already on this branch".to_string() } else { String::new() },
                key: Some("i".to_string()),
                action: if is_same_branch { None } else { Some(Box::new(move |gui| {
                    enter_interactive_rebase_onto(gui, &name_for_interactive)?;
                    Ok(())
                })) },
            },
            MenuItem {
                label: format!("Rebase onto base branch ({})", name_for_base),
                description: String::new(),
                key: Some("b".to_string()),
                action: Some(Box::new(move |gui| {
                    gui.git.rebase_branch(&name_for_base)?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Cancel".to_string(),
                description: String::new(),
                key: None,
                action: Some(Box::new(|_gui| Ok(()))),
            },
        ];

        gui.popup = PopupState::Menu {
            title: format!("Rebase '{}'", current_branch),
            items,
            selected: 0,
            loading_index: None,
        };
    }
    Ok(())
}

/// Enter interactive rebase mode onto a specific branch/ref.
pub fn enter_interactive_rebase_onto(gui: &mut Gui, onto_ref: &str) -> Result<()> {
    // Resolve the ref to a commit hash
    let base_hash = gui.git.resolve_ref(onto_ref)?;

    let model = gui.model.lock().unwrap();
    let branch_name = model.head_branch_name.clone();

    // Find the base commit in the model
    let base_commit = model.commits.iter().find(|c| c.hash == base_hash).cloned();

    // Get commits to rebase
    let rebase_hashes = match gui.git.rebase_commit_range(&base_hash) {
        Ok(h) => h,
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

    let commits_to_rebase: Vec<_> = rebase_hashes
        .iter()
        .filter_map(|hash| model.commits.iter().find(|c| c.hash == *hash))
        .cloned()
        .collect();

    if commits_to_rebase.is_empty() {
        gui.popup = PopupState::Message {
            title: "Interactive rebase".to_string(),
            message: "Commits not found in current view.".to_string(),
            kind: crate::gui::popup::MessageKind::Error,
        };
        drop(model);
        return Ok(());
    }

    // Build a base commit — use model commit if available, otherwise create a minimal one
    let base = match base_commit {
        Some(c) => c,
        None => {
            // Base commit might not be in the loaded commits list — create a minimal one
            let msg = gui.git.commit_subject(&base_hash).unwrap_or_default();
            use crate::model::commit::{CommitStatus, Divergence};
            crate::model::Commit {
                hash: base_hash.clone(),
                name: msg,
                status: CommitStatus::Pushed,
                action: String::new(),
                tags: Vec::<String>::new(),
                refs: Vec::<String>::new(),
                extra_info: String::new(),
                author_name: String::new(),
                author_email: String::new(),
                unix_timestamp: 0,
                parents: Vec::new(),
                divergence: Divergence::None,
            }
        }
    };

    gui.rebase_mode.enter(branch_name, &base, &commits_to_rebase);
    drop(model);
    Ok(())
}

fn rename_branch(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let old_name = branch.name.clone();
        drop(model);

        let mut ta = make_textarea("");
        ta.insert_str(&old_name);
        gui.popup = PopupState::Input {
            title: format!("Rename branch '{}'", old_name),
            textarea: ta,
            on_confirm: Box::new(move |gui, new_name| {
                if !new_name.is_empty() && new_name != old_name {
                    gui.git.rename_branch(&old_name, new_name)?;
                    gui.needs_refresh = true;
                }
                Ok(())
            }),
            is_commit: false, confirm_focused: false,
        };
    }
    Ok(())
}

fn fast_forward(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        if branch.upstream.is_some() {
            let _name = branch.name.clone();
            drop(model);
            gui.git.fetch("origin")?;
            gui.needs_refresh = true;
        }
    }
    Ok(())
}

fn set_upstream(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let name = branch.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Set upstream".to_string(),
            message: format!("Set upstream of '{}' to origin/{}?", name, name),
            on_confirm: Box::new(move |gui| {
                gui.git.push_with_upstream("origin", &name)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}

fn copy_to_clipboard_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let branch_name = branch.name.clone();
        let branch_for_name = branch_name.clone();
        let branch_for_pr_create = branch_name.clone();
        let branch_for_pr = branch_name.clone();
        drop(model);

        let mut items = vec![
            MenuItem {
                label: "Copy branch name".to_string(),
                description: String::new(),
                key: Some("n".to_string()),
                action: Some(Box::new(move |_gui| {
                    Platform::copy_to_clipboard(&branch_for_name)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Copy repo URL".to_string(),
                description: String::new(),
                key: Some("r".to_string()),
                action: Some(Box::new(move |gui| {
                    let url = gui.git.get_repo_url()?;
                    Platform::copy_to_clipboard(&url)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Copy PR create URL".to_string(),
                description: String::new(),
                key: Some("c".to_string()),
                action: Some(Box::new(move |gui| {
                    let url = gui.git.get_pr_create_url(&branch_for_pr_create)?;
                    Platform::copy_to_clipboard(&url)?;
                    Ok(())
                })),
            },
        ];

        // PR URL may not be available — try to detect if branch has a PR
        let pr_branch = branch_for_pr.clone();
        let copy_pr_index = items.len();
        items.push(MenuItem {
            label: "Copy PR URL".to_string(),
            description: "(requires existing PR)".to_string(),
            key: Some("p".to_string()),
            action: Some(Box::new(move |gui| {
                use crate::gui::popup::MenuAsyncResult;
                let branch = pr_branch.clone();
                gui.start_menu_async(copy_pr_index, move |git| {
                    let url = git.get_pr_url(&branch)?;
                    Ok(MenuAsyncResult::CopyToClipboard(url))
                });
                Ok(())
            })),
        });

        gui.popup = PopupState::Menu {
            title: "Copy to clipboard".to_string(),
            items,
            selected: 0,
            loading_index: None,
        };
    }
    Ok(())
}

fn open_in_browser_menu(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(branch) = model.branches.get(selected) {
        let branch_name = branch.name.clone();
        let branch_for_pr_create = branch_name.clone();
        let branch_for_pr = branch_name.clone();
        drop(model);

        let mut items = vec![
            MenuItem {
                label: "Open repo URL".to_string(),
                description: String::new(),
                key: Some("r".to_string()),
                action: Some(Box::new(move |gui| {
                    let url = gui.git.get_repo_url()?;
                    Platform::open_file(&url)?;
                    Ok(())
                })),
            },
            MenuItem {
                label: "Open PR create URL".to_string(),
                description: String::new(),
                key: Some("c".to_string()),
                action: Some(Box::new(move |gui| {
                    let url = gui.git.get_pr_create_url(&branch_for_pr_create)?;
                    Platform::open_file(&url)?;
                    Ok(())
                })),
            },
        ];

        let pr_branch = branch_for_pr.clone();
        let open_pr_index = items.len();
        items.push(MenuItem {
            label: "Open PR URL".to_string(),
            description: "(requires existing PR)".to_string(),
            key: Some("p".to_string()),
            action: Some(Box::new(move |gui| {
                use crate::gui::popup::MenuAsyncResult;
                let branch = pr_branch.clone();
                gui.start_menu_async(open_pr_index, move |git| {
                    let url = git.get_pr_url(&branch)?;
                    Ok(MenuAsyncResult::OpenUrl(url))
                });
                Ok(())
            })),
        });

        gui.popup = PopupState::Menu {
            title: "Open in browser".to_string(),
            items,
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
