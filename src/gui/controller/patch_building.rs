use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::gui::popup::{MenuItem, PopupState};
use crate::gui::Gui;

/// Enter patch building mode from the commits panel.
pub fn enter_patch_building(gui: &mut Gui) -> Result<()> {
    let selected = gui.context_mgr.selected_active();
    let model = gui.model.lock().unwrap();
    if let Some(commit) = model.commits.get(selected) {
        let hash = commit.hash.clone();
        drop(model);

        // Load the files changed in this commit
        let result = gui.git.git_cmd()
            .args(&["diff-tree", "--no-commit-id", "--name-only", "-r", &hash])
            .run()?;

        if result.success {
            gui.patch_building.enter(hash);
            // Store the file list for display
            let _files: Vec<&str> = result.stdout.lines().collect();
        }
    }
    Ok(())
}

/// Show patch options menu (from commits panel, <c-p>).
pub fn show_patch_menu(gui: &mut Gui) -> Result<()> {
    if gui.patch_building.active {
        // Already in patch building mode — show apply options
        if !gui.patch_building.has_selections() {
            gui.popup = PopupState::Confirm {
                title: "No files selected".to_string(),
                message: "Toggle files with space to include them in the patch.".to_string(),
                on_confirm: Box::new(|_| Ok(())),
            };
            return Ok(());
        }

        gui.popup = PopupState::Menu {
            title: "Patch options".to_string(),
            items: vec![
                MenuItem {
                    label: "Apply patch to index".to_string(),
                    description: "Stage the selected changes".to_string(),
                    key: Some("a".to_string()),
                    action: Some(Box::new(|gui| {
                        apply_patch(gui, false)?;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Apply patch in reverse".to_string(),
                    description: "Remove the selected changes".to_string(),
                    key: Some("r".to_string()),
                    action: Some(Box::new(|gui| {
                        apply_patch(gui, true)?;
                        Ok(())
                    })),
                },
                MenuItem {
                    label: "Exit patch building mode".to_string(),
                    description: "Discard patch selections".to_string(),
                    key: Some("q".to_string()),
                    action: Some(Box::new(|gui| {
                        gui.patch_building.exit();
                        Ok(())
                    })),
                },
            ],
            selected: 0,
        };
    } else {
        // Enter patch building mode
        let selected = gui.context_mgr.selected_active();
        let model = gui.model.lock().unwrap();
        if let Some(commit) = model.commits.get(selected) {
            let hash = commit.hash.clone();
            let short = commit.short_hash().to_string();
            drop(model);

            gui.popup = PopupState::Confirm {
                title: "Patch building".to_string(),
                message: format!(
                    "Enter patch building mode for commit {}?\n\
                     Use space to toggle files, then <c-p> to apply.",
                    short
                ),
                on_confirm: Box::new(move |gui| {
                    gui.patch_building.enter(hash);
                    Ok(())
                }),
            };
        }
    }
    Ok(())
}

/// Toggle the current file in patch building mode.
pub fn toggle_file_in_patch(gui: &mut Gui) -> Result<()> {
    if !gui.patch_building.active {
        return Ok(());
    }

    let commit = gui.patch_building.source_commit.clone();
    let result = gui.git.git_cmd()
        .args(&["diff-tree", "--no-commit-id", "--name-only", "-r", &commit])
        .run()?;

    if result.success {
        let files: Vec<&str> = result.stdout.lines().collect();
        let selected = gui.context_mgr.selected_active();
        // The commit files context would map here; for now use commit panel selection
        // to select the nth changed file
        if let Some(file) = files.get(selected) {
            gui.patch_building.toggle_file(file);
        }
    }
    Ok(())
}

fn apply_patch(gui: &mut Gui, reverse: bool) -> Result<()> {
    let commit = gui.patch_building.source_commit.clone();
    let files: Vec<String> = gui.patch_building.selected_files.iter().cloned().collect();

    for file in &files {
        let mut cmd = gui.git.git_cmd();
        cmd = cmd.args(&["diff", &format!("{}^", commit), &commit, "--", file]);
        let diff_result = cmd.run()?;

        if diff_result.success && !diff_result.stdout.is_empty() {
            let mut apply = gui.git.git_cmd();
            apply = apply.arg("apply");
            if reverse {
                apply = apply.arg("--reverse");
            }
            apply = apply.arg("--cached").arg("-");
            apply = apply.stdin(diff_result.stdout);
            let _ = apply.run(); // Best effort
        }
    }

    gui.patch_building.exit();
    gui.needs_refresh = true;
    Ok(())
}
