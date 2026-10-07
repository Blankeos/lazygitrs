use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::gui::Gui;
use crate::gui::popup::{MenuItem, MessageKind, PopupState, make_textarea};

pub fn handle_key(gui: &mut Gui, key: KeyEvent, _keybindings: &KeybindingConfig) -> Result<()> {
    // Switch to worktree (action menu)
    if key.code == KeyCode::Char(' ') {
        return switch_worktree(gui);
    }

    // Direct open in Tmux session
    if key.code == KeyCode::Char('t') {
        return open_selected_in_tmux(gui);
    }

    // Create new worktree
    if key.code == KeyCode::Char('n') {
        return create_worktree(gui);
    }

    // Remove worktree
    if key.code == KeyCode::Char('d') {
        return remove_worktree(gui);
    }

    Ok(())
}

pub fn open_in_tmux_session(path: &str) -> Result<()> {
    let target_path = path.to_string();
    let session_name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "worktree".to_string());

    // 1. Try wm connect or sesh connect if available
    for cmd in ["wm", "sesh"] {
        if let Ok(status) = std::process::Command::new(cmd)
            .arg("connect")
            .arg(&target_path)
            .status()
        {
            if status.success() {
                return Ok(());
            }
        }
    }

    // 2. Fallback to tmux commands
    if std::env::var_os("TMUX").is_some() {
        let _ = std::process::Command::new("tmux")
            .args(["new-session", "-d", "-s", &session_name, "-c", &target_path])
            .status();
        let _ = std::process::Command::new("tmux")
            .args(["switch-client", "-t", &session_name])
            .status();
    } else {
        let _ = std::process::Command::new("tmux")
            .args(["new-session", "-A", "-s", &session_name, "-c", &target_path])
            .spawn();
    }
    Ok(())
}

pub fn show_worktree_action_menu(gui: &mut Gui, branch_name: &str, path: &str, title_prefix: &str) {
    let b_name = branch_name.to_string();
    let p_for_tmux = path.to_string();
    let p_for_exe = path.to_string();

    let items = vec![
        MenuItem {
            label: "Open in new Tmux session".to_string(),
            description: format!("Create/switch Tmux session for {}", path),
            key: Some("t".to_string()),
            action: Some(Box::new(move |gui| {
                let _ = open_in_tmux_session(&p_for_tmux);
                gui.popup = PopupState::None;
                gui.needs_refresh = true;
                Ok(())
            })),
        },
        MenuItem {
            label: "Open lazygitrs in current terminal".to_string(),
            description: "Launch new lazygitrs instance here".to_string(),
            key: Some("o".to_string()),
            action: Some(Box::new(move |gui| {
                let exe = std::env::current_exe().unwrap_or_else(|_| "lazygitrs".into());
                std::process::Command::new(exe)
                    .arg("--path")
                    .arg(&p_for_exe)
                    .spawn()?;
                gui.should_quit = true;
                Ok(())
            })),
        },
        MenuItem {
            label: "Stay in current worktree".to_string(),
            description: "Keep current session active".to_string(),
            key: Some("s".to_string()),
            action: Some(Box::new(move |gui| {
                gui.popup = PopupState::None;
                gui.needs_refresh = true;
                Ok(())
            })),
        },
    ];

    gui.popup = PopupState::Menu {
        title: format!("{}: {}", title_prefix, b_name),
        items,
        selected: 0,
        loading_index: None,
    };
}

fn switch_worktree(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(wt) = model.worktrees.get(selected) {
        if wt.is_current {
            return Ok(()); // Already in this worktree
        }
        let path = wt.path.clone();
        let branch = wt.branch.clone();
        drop(model);

        show_worktree_action_menu(gui, &branch, &path, "Switch worktree");
    }
    Ok(())
}

fn open_selected_in_tmux(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(wt) = model.worktrees.get(selected) {
        let path = wt.path.clone();
        drop(model);

        open_in_tmux_session(&path)?;
        gui.needs_refresh = true;
    }
    Ok(())
}

fn create_worktree(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::Input {
        title: "New worktree branch name".to_string(),
        textarea: make_textarea(""),
        on_confirm: Box::new(|gui, input| {
            let parts: Vec<&str> = input.split_whitespace().collect();
            if parts.is_empty() {
                return Ok(());
            }

            let branch = parts[0];
            let base = if parts.len() >= 2 {
                Some(parts[1])
            } else {
                None
            };

            match gui.git.add_worktree(branch, base, None) {
                Ok(target_path) => {
                    gui.needs_refresh = true;
                    let path_display = target_path.display().to_string();
                    let branch_name = branch.to_string();

                    // Prompt action menu with Tmux option
                    show_worktree_action_menu(gui, &branch_name, &path_display, "Worktree created");
                }
                Err(e) => {
                    gui.popup = PopupState::Message {
                        title: "Create worktree error".to_string(),
                        message: format!("{}", e),
                        kind: MessageKind::Error,
                    };
                }
            }
            Ok(())
        }),
        is_commit: false,
        confirm_focused: false,
    };
    Ok(())
}

fn remove_worktree(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(wt) = model.worktrees.get(selected) {
        if wt.is_current || wt.is_main {
            return Ok(()); // Can't remove current or main worktree
        }
        let path = wt.path.clone();
        let branch = wt.branch.clone();
        drop(model);

        gui.popup = PopupState::Confirm {
            title: "Remove worktree".to_string(),
            message: format!(
                "Remove worktree '{}' ({})?\nThis won't delete the branch.",
                branch, path
            ),
            on_confirm: Box::new(move |gui| {
                gui.git.remove_worktree(&path, false)?;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
    }
    Ok(())
}
