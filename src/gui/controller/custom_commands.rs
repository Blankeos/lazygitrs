use anyhow::Result;
use crossterm::event::KeyEvent;

use crate::config::keybindings::parse_key;
use crate::config::user_config::CustomCommand;
use crate::gui::context::ContextId;
use crate::gui::popup::PopupState;
use crate::gui::Gui;
use crate::os::cmd::CmdBuilder;

/// Try to handle a key as a custom command. Returns Ok(true) if handled.
pub fn try_handle_key(gui: &mut Gui, key: KeyEvent) -> Result<bool> {
    let active = gui.context_mgr.active();
    let context_name = context_id_to_name(active);
    let commands = gui.config.user_config.custom_commands.clone();

    for cmd in &commands {
        if cmd.key.is_empty() || cmd.command.is_empty() {
            continue;
        }

        // Match context: "global" matches everywhere, otherwise match the panel name
        let context_matches = cmd.context == "global"
            || cmd.context.is_empty()
            || cmd.context == context_name;

        if !context_matches {
            continue;
        }

        if let Some(expected) = parse_key(&cmd.key) {
            if key.code == expected.code && key.modifiers == expected.modifiers {
                return execute_custom_command(gui, cmd).map(|_| true);
            }
        }
    }

    Ok(false)
}

fn execute_custom_command(gui: &mut Gui, cmd: &CustomCommand) -> Result<()> {
    // Resolve template variables in the command string
    let resolved = resolve_template(gui, &cmd.command);

    if cmd.prompts.is_empty() {
        // No prompts — execute directly
        run_command(gui, &resolved, cmd.show_output)?;
    } else {
        // Has prompts — for now, show a simple input for the first prompt
        let prompt = cmd.prompts[0].clone();
        let title = prompt.title.unwrap_or_else(|| "Input".to_string());
        let show_output = cmd.show_output;
        let cmd_template = resolved;

        gui.popup = PopupState::Input {
            title,
            textarea: crate::gui::popup::make_textarea(""),
            on_confirm: Box::new(move |gui, input| {
                let final_cmd = cmd_template.replace("{{index .PromptResponses 0}}", input);
                run_command(gui, &final_cmd, show_output)?;
                Ok(())
            }),
            is_commit: false,
        };
    }

    Ok(())
}

fn run_command(gui: &mut Gui, command: &str, show_output: bool) -> Result<()> {
    let result = CmdBuilder::new("sh")
        .args(&["-c", command])
        .cwd_path(gui.git.repo_path())
        .run()?;

    if let Ok(mut log) = gui.command_log.lock() {
        log.push(format!("$ {}", command));
    }

    if show_output && !result.stdout.is_empty() {
        gui.popup = PopupState::Confirm {
            title: "Command output".to_string(),
            message: result.stdout_trimmed().to_string(),
            on_confirm: Box::new(|_| Ok(())),
        };
    } else if !result.success {
        gui.popup = PopupState::Confirm {
            title: "Command failed".to_string(),
            message: result.stderr.trim().to_string(),
            on_confirm: Box::new(|_| Ok(())),
        };
    }

    gui.needs_refresh = true;
    Ok(())
}

fn resolve_template(gui: &Gui, template: &str) -> String {
    let model = gui.model.lock().unwrap();
    let selected = gui.context_mgr.selected_active();
    let active = gui.context_mgr.active();

    let mut result = template.to_string();

    // Selected branch name
    let branch_name = model
        .branches
        .iter()
        .find(|b| b.head)
        .map(|b| b.name.as_str())
        .unwrap_or("");
    result = result.replace("{{.SelectedLocalBranch.Name}}", branch_name);
    result = result.replace("{{.CheckedOutBranch.Name}}", branch_name);

    // Selected item based on context
    match active {
        ContextId::Branches => {
            if let Some(branch) = model.branches.get(selected) {
                result = result.replace("{{.SelectedLocalBranch.Name}}", &branch.name);
            }
        }
        ContextId::Commits => {
            if let Some(commit) = model.commits.get(selected) {
                result = result.replace("{{.SelectedLocalCommit.Hash}}", &commit.hash);
                result = result.replace("{{.SelectedLocalCommit.Name}}", &commit.name);
            }
        }
        ContextId::Files => {
            let file_idx = gui.selected_file_index().unwrap_or(selected);
            if let Some(file) = model.files.get(file_idx) {
                result = result.replace("{{.SelectedFile.Name}}", &file.name);
            }
        }
        ContextId::Stash => {
            if let Some(entry) = model.stash_entries.get(selected) {
                result = result.replace("{{.SelectedStashEntry.Index}}", &entry.index.to_string());
                result = result.replace("{{.SelectedStashEntry.Name}}", &entry.name);
            }
        }
        ContextId::Tags => {
            if let Some(tag) = model.tags.get(selected) {
                result = result.replace("{{.SelectedTag.Name}}", &tag.name);
            }
        }
        _ => {}
    }

    result
}

fn context_id_to_name(ctx: ContextId) -> &'static str {
    match ctx {
        ContextId::Status => "status",
        ContextId::Files => "files",
        ContextId::Branches => "localBranches",
        ContextId::Remotes => "remotes",
        ContextId::Tags => "tags",
        ContextId::Commits => "commits",
        ContextId::Stash => "stash",
        ContextId::Worktrees => "worktrees",
        ContextId::Submodules => "submodules",
        ContextId::CommitFiles => "commitFiles",
        ContextId::Staging => "staging",
    }
}
