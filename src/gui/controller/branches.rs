use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::{MenuItem, PopupState, make_textarea};
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
        gui.git.checkout_branch(&name)?;
        gui.needs_refresh = true;
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
        is_commit: false,
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
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Delete branch".to_string(),
            message: format!("Delete branch '{}'?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.delete_branch(&name, false)?;
                gui.needs_refresh = true;
                Ok(())
            }),
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
    if let Some(branch) = model.branches.get(selected) {
        let name = branch.name.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Rebase".to_string(),
            message: format!("Rebase onto '{}'?", name),
            on_confirm: Box::new(move |gui| {
                gui.git.rebase_branch(&name)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
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
            is_commit: false,
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
        let branch_for_pr_create = branch_name.clone();
        let branch_for_pr = branch_name.clone();
        drop(model);

        let mut items = vec![
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
        items.push(MenuItem {
            label: "Copy PR URL".to_string(),
            description: "(requires existing PR)".to_string(),
            key: Some("p".to_string()),
            action: Some(Box::new(move |gui| {
                match gui.git.get_pr_url(&pr_branch) {
                    Ok(url) => {
                        Platform::copy_to_clipboard(&url)?;
                    }
                    Err(_) => {
                        gui.popup = PopupState::Confirm {
                            title: "No PR found".to_string(),
                            message: format!("No pull request found for branch '{}'", branch_for_pr),
                            on_confirm: Box::new(|_| Ok(())),
                        };
                    }
                }
                Ok(())
            })),
        });

        gui.popup = PopupState::Menu {
            title: "Copy to clipboard".to_string(),
            items,
            selected: 0,
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
        items.push(MenuItem {
            label: "Open PR URL".to_string(),
            description: "(requires existing PR)".to_string(),
            key: Some("p".to_string()),
            action: Some(Box::new(move |gui| {
                match gui.git.get_pr_url(&pr_branch) {
                    Ok(url) => {
                        Platform::open_file(&url)?;
                    }
                    Err(_) => {
                        gui.popup = PopupState::Confirm {
                            title: "No PR found".to_string(),
                            message: format!("No pull request found for branch '{}'", branch_for_pr),
                            on_confirm: Box::new(|_| Ok(())),
                        };
                    }
                }
                Ok(())
            })),
        });

        gui.popup = PopupState::Menu {
            title: "Open in browser".to_string(),
            items,
            selected: 0,
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
