use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::matches_key;
use crate::gui::Gui;
use crate::gui::popup::{MenuItem, PopupState};
use crate::os::platform::Platform;

pub fn handle_key(gui: &mut Gui, key: KeyEvent, keybindings: &KeybindingConfig) -> Result<()> {
    // Enter: show recent repos
    if key.code == KeyCode::Enter {
        return gui.show_recent_repos();
    }

    // y: copy menu (branch name, repo url, PR create url, PR url for current branch)
    if key.code == KeyCode::Char('y') {
        return copy_menu(gui);
    }

    // o: open in browser menu
    if matches_key(key, &keybindings.branches.create_pull_request) {
        return open_menu(gui);
    }

    Ok(())
}

fn current_branch_name(gui: &Gui) -> String {
    let model = gui.model.lock().unwrap();
    if let Some(b) = model.branches.iter().find(|b| b.head) {
        return b.name.clone();
    }
    model.head_branch_name.clone()
}

fn copy_menu(gui: &mut Gui) -> Result<()> {
    let branch = current_branch_name(gui);
    let branch_for_name = branch.clone();
    let branch_for_pr_create = branch.clone();
    let branch_for_pr = branch.clone();

    let mut items = vec![MenuItem {
        label: "Copy repo URL".to_string(),
        description: String::new(),
        key: Some("r".to_string()),
        action: Some(Box::new(move |gui| {
            let url = gui.git.get_repo_url()?;
            Platform::copy_to_clipboard(&url)?;
            Ok(())
        })),
    }];

    if !branch.is_empty() {
        items.insert(
            0,
            MenuItem {
                label: "Copy branch name".to_string(),
                description: String::new(),
                key: Some("n".to_string()),
                action: Some(Box::new(move |_gui| {
                    Platform::copy_to_clipboard(&branch_for_name)?;
                    Ok(())
                })),
            },
        );
        items.push(MenuItem {
            label: "Copy PR create URL".to_string(),
            description: String::new(),
            key: Some("c".to_string()),
            action: Some(Box::new(move |gui| {
                let url = gui.git.get_pr_create_url(&branch_for_pr_create)?;
                Platform::copy_to_clipboard(&url)?;
                Ok(())
            })),
        });

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
    }

    gui.popup = PopupState::Menu {
        title: "Copy to clipboard".to_string(),
        items,
        selected: 0,
        loading_index: None,
    };
    Ok(())
}

fn open_menu(gui: &mut Gui) -> Result<()> {
    let branch = current_branch_name(gui);
    let branch_for_pr_create = branch.clone();
    let branch_for_pr = branch.clone();

    let mut items = vec![MenuItem {
        label: "Open repo URL".to_string(),
        description: String::new(),
        key: Some("r".to_string()),
        action: Some(Box::new(move |gui| {
            let url = gui.git.get_repo_url()?;
            Platform::open_file(&url)?;
            Ok(())
        })),
    }];

    if !branch.is_empty() {
        items.push(MenuItem {
            label: "Open PR create URL".to_string(),
            description: String::new(),
            key: Some("c".to_string()),
            action: Some(Box::new(move |gui| {
                let url = gui.git.get_pr_create_url(&branch_for_pr_create)?;
                Platform::open_file(&url)?;
                Ok(())
            })),
        });

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
    }

    gui.popup = PopupState::Menu {
        title: "Open in browser".to_string(),
        items,
        selected: 0,
        loading_index: None,
    };
    Ok(())
}
