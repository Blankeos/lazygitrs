use anyhow::Result;
use crossterm::event::KeyEvent;

use crate::config::keybindings::parse_key;
use crate::config::user_config::CustomCommand;
use crate::gui::Gui;
use crate::gui::context::ContextId;
use crate::gui::popup::{MessageKind, PopupState};
use crate::os::shell::{ShellJob, ShellResult};

pub(in crate::gui) struct RunningCommand {
    job: ShellJob,
    show_output: bool,
}

fn comparison_contains_commit(gui: &Gui, hash: &str) -> bool {
    // Symmetric-difference membership, without materializing the history.
    // A failed reload (or tree-only comparison) has no valid commit range.
    // is_ancestor returns false on errors, so XOR alone would misclassify a
    // deleted ref and resurrect stale drilled-down files.
    gui.diff_mode.ahead_behind.is_some()
        && (gui.git.is_ancestor(hash, &gui.diff_mode.ref_a)
            != gui.git.is_ancestor(hash, &gui.diff_mode.ref_b))
}

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
        let context_matches =
            cmd.context == "global" || cmd.context.is_empty() || cmd.context == context_name;

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
            confirm_focused: false,
        };
    }

    Ok(())
}

fn run_command(gui: &mut Gui, command: &str, show_output: bool) -> Result<()> {
    start_command(gui, command, show_output, false)
}

fn start_command(
    gui: &mut Gui,
    command: &str,
    show_output: bool,
    shell_prompt: bool,
) -> Result<()> {
    anyhow::ensure!(
        gui.shell_command_job.is_none(),
        "A shell command is already running"
    );
    let shell = std::env::var_os("SHELL")
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "sh".into());
    let job = ShellJob::spawn(gui.git.repo_path(), command, &shell, shell_prompt)?;
    append_log(gui, format!("$ {command}"));
    gui.shell_command_job = Some(RunningCommand { job, show_output });
    gui.popup = PopupState::Loading {
        title: "Shell command".into(),
        message: "Esc cancel · 5m timeout".into(),
    };
    Ok(())
}

fn append_log(gui: &Gui, message: String) {
    if let Ok(mut log) = gui.command_log.lock() {
        log.push(message);
        let excess = log.len().saturating_sub(100);
        log.drain(..excess);
    }
}

pub fn cancel_running_command(gui: &mut Gui) {
    if let Some(command) = &gui.shell_command_job {
        command.job.cancel();
        if let PopupState::Loading { message, .. } = &mut gui.popup {
            *message = "Cancelling command…".into();
        }
    }
}

pub fn receive_command_result(gui: &mut Gui) {
    let Some(outcome) = gui
        .shell_command_job
        .as_mut()
        .and_then(|command| command.job.try_result())
    else {
        return;
    };
    // Retain job/modal ownership through refresh: a refresh failure should be
    // queued, not overwritten by the command result shown below.
    // Even failures/cancellation can leave repository changes behind.
    gui.needs_refresh = true;
    gui.needs_diff_refresh = true;
    if gui.diff_mode.active {
        refresh_comparison(gui);
    }
    let command = gui.shell_command_job.take().unwrap();
    gui.popup = PopupState::None;
    match outcome {
        Ok(result) => {
            let (title, message, kind) = result_message(&result);
            // The shared log is also rendered every frame; retain a bounded
            // preview rather than up to two MiB per entry. Full output remains
            // available in the scrollable popup.
            let mut end = message.len().min(16 * 1024);
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            let suffix = if end < message.len() {
                "\n[log output truncated; see command popup]"
            } else {
                ""
            };
            append_log(gui, format!("{title}: {}{suffix}", &message[..end]));
            if command.show_output || !result.success {
                gui.popup = PopupState::CommandOutput {
                    title,
                    message,
                    kind,
                    scroll: 0,
                };
            }
        }
        Err(error) => {
            let message = format!("{error:#}");
            append_log(gui, format!("Command failed: {message}"));
            gui.show_error("Command failed", error);
        }
    }
}

/// Refresh refs after mutations without throwing away comparison navigation.
fn refresh_comparison(gui: &mut Gui) {
    use crate::gui::modes::diff_mode::CompareDiffSource;

    let focus = gui.diff_mode.focus;
    let sidebar_focus = gui.diff_mode.sidebar_focus;
    let source = gui.diff_mode.diff_source;
    let files_commit = gui.diff_mode.files_commit.clone();
    let selected_commit = gui.diff_mode.selected_commit().map(|c| c.hash.clone());
    let selected_index = gui.diff_mode.commits_selected;
    let loaded_count = gui
        .diff_mode
        .commits
        .len()
        .max(crate::git::DEFAULT_COMMIT_LIMIT);
    let selected_file = if gui.diff_mode.show_tree {
        gui.diff_mode
            .tree_nodes
            .get(gui.diff_mode.diff_files_selected)
            .map(|n| n.path.clone())
    } else {
        gui.diff_mode
            .diff_files
            .get(gui.diff_mode.diff_files_selected)
            .map(|f| f.name.clone())
    };
    let collapsed = gui.diff_mode.collapsed_dirs.clone();
    let search_query = gui.diff_mode.file_search_query.clone();
    let search_textarea = gui.diff_mode.file_search_textarea.clone();
    let _ = super::diff_mode::reload_diff_files_with_commit_limit(gui, loaded_count);
    // New commits can push the selection past the retained pages. Only walk
    // further when the hash actually remains in A...B, and stop as soon as it
    // is found. Rewritten/removed commits must not cause an unbounded reload.
    if let Some(hash) = selected_commit.as_deref() {
        if !gui.diff_mode.commits.iter().any(|c| c.hash == hash)
            && comparison_contains_commit(gui, hash)
        {
            while gui.diff_mode.has_more_commits() {
                let before = gui.diff_mode.commits.len();
                if super::diff_mode::load_more_commits(gui).is_err()
                    || gui.diff_mode.commits.len() == before
                    || gui.diff_mode.commits.iter().any(|c| c.hash == hash)
                {
                    break;
                }
            }
        }
    }
    if let Some(hash) = files_commit {
        if comparison_contains_commit(gui, &hash)
            && let Ok(files) = gui.git.commit_files(&hash)
        {
            gui.diff_mode.diff_files = files;
            gui.diff_mode.files_commit = Some(hash);
        }
    }
    gui.diff_mode.collapsed_dirs = collapsed;
    super::diff_mode::update_diff_mode_tree(gui);
    // If the selected hash genuinely left the comparison, keep the nearest
    // available row rather than silently jumping to the newest commit.
    gui.diff_mode.commits_selected = selected_commit
        .and_then(|hash| gui.diff_mode.commits.iter().position(|c| c.hash == hash))
        .unwrap_or_else(|| selected_index.min(gui.diff_mode.commits.len().saturating_sub(1)));
    if let Some(path) = selected_file {
        let index = if gui.diff_mode.show_tree {
            gui.diff_mode.tree_nodes.iter().position(|n| n.path == path)
        } else {
            gui.diff_mode.diff_files.iter().position(|f| f.name == path)
        };
        gui.diff_mode.diff_files_selected = index.unwrap_or(0);
    }
    gui.diff_mode.set_focus(focus);
    gui.diff_mode.sidebar_focus = sidebar_focus;
    gui.diff_mode.diff_source = match source {
        CompareDiffSource::CommitFiles if gui.diff_mode.files_commit.is_none() => {
            CompareDiffSource::Comparison
        }
        CompareDiffSource::Commit if gui.diff_mode.selected_commit().is_none() => {
            CompareDiffSource::Comparison
        }
        _ => source,
    };
    gui.diff_mode.file_search_query = search_query;
    gui.diff_mode.file_search_textarea = search_textarea;
    gui.diff_mode.update_file_search_matches();
}

fn result_message(result: &ShellResult) -> (String, String, MessageKind) {
    let title = if result.cancelled {
        "Command cancelled".into()
    } else if result.timed_out {
        "Command timed out (5 minutes)".into()
    } else if result.success {
        "Command output (exit 0)".into()
    } else if let Some(code) = result.exit_code {
        format!("Command failed (exit {code})")
    } else {
        "Command failed (terminated by signal)".into()
    };
    // Raw terminal controls must never reach the popup or copy/log view.
    let clean = |text: &str| {
        String::from_utf8_lossy(&strip_ansi_escapes::strip(text.replace('\t', "    ")))
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
            .collect::<String>()
    };
    let stdout = clean(&result.stdout);
    let stderr = clean(&result.stderr);
    let message = match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "No output.".into(),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("{stdout}\n--- stderr ---\n{stderr}"),
    };
    let kind = if result.success {
        MessageKind::Info
    } else {
        MessageKind::Error
    };
    (title, message, kind)
}

/// Explicit universal binding takes precedence over panel shortcuts, not text entry.
pub fn try_handle_prompt_key(gui: &mut Gui, key: KeyEvent) -> Result<bool> {
    if super::super::matches_key(
        key,
        &gui.config
            .user_config
            .keybinding
            .universal
            .custom_command_prompt,
    ) {
        open_custom_command_prompt(gui)?;
        return Ok(true);
    }
    Ok(false)
}

pub fn confirm_shell_prompt(gui: &mut Gui, input: &str) -> Result<()> {
    if !input.trim().is_empty() {
        start_command(gui, input, true, true)?;
    }
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
        ContextId::Reflog => "reflogCommits",
        ContextId::Stash => "stash",
        ContextId::Worktrees => "worktrees",
        ContextId::Submodules => "submodules",
        ContextId::CommitFiles => "commitFiles",
        ContextId::StashFiles => "stashFiles",
        ContextId::BranchCommits => "branchCommits",
        ContextId::BranchCommitFiles => "branchCommitFiles",
        ContextId::RemoteBranches => "remoteBranches",
        ContextId::Staging => "staging",
    }
}

pub fn open_custom_command_prompt(gui: &mut Gui) -> Result<()> {
    gui.popup = PopupState::ShellCommand {
        textarea: crate::gui::popup::make_textarea(
            "Command runs in repository; Enter executes, Esc cancels",
        ),
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, AppState, UserConfig};
    use crate::git::GitCommands;
    use crate::gui::modes::diff_mode::{CompareDiffSource, DiffModeFocus};
    use std::process::Command;

    struct HistoryRepo(std::path::PathBuf);

    impl Drop for HistoryRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn git_script(repo: &HistoryRepo, script: &str) {
        let output = Command::new("sh")
            .args(["-eu", "-c", script])
            .current_dir(&repo.0)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn comparison_history() -> (HistoryRepo, Gui) {
        let path = std::env::temp_dir().join(format!(
            "lazygitrs-comparison-refresh-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let repo = HistoryRepo(path);
        // Reuse two trees: commit-tree avoids 950 working-tree/index updates.
        git_script(
            &repo,
            r#"
            git init -q -b main
            printf 'base\n' > file.txt
            git add file.txt
            a=$(git write-tree)
            printf 'changed\n' > file.txt
            git add file.txt
            b=$(git write-tree)
            root=$(git -c commit.gpgsign=false commit-tree "$a" -m root)
            git update-ref refs/heads/main "$root"
            parent=$root
            i=1
            while [ "$i" -le 950 ]; do
                if [ $((i % 2)) -eq 0 ]; then tree=$a; else tree=$b; fi
                parent=$(git -c commit.gpgsign=false commit-tree "$tree" -p "$parent" -m "commit $i")
                i=$((i + 1))
            done
            git update-ref refs/heads/feature "$parent"
            git reset -q --hard main
        "#,
        );
        let config = AppConfig {
            debug: false,
            version: String::new(),
            user_config: UserConfig::default(),
            app_state: AppState::default(),
            config_dir: repo.0.clone(),
            state_dir: repo.0.clone(),
            state_path: repo.0.join("state.yml"),
        };
        let mut gui = Gui::new(config, GitCommands::new(&repo.0).unwrap(), None, false).unwrap();
        gui.diff_mode.enter(false);
        gui.diff_mode.ref_a = "main".into();
        gui.diff_mode.ref_b = "feature".into();
        super::super::diff_mode::reload_diff_files(&mut gui).unwrap();
        super::super::diff_mode::load_more_commits(&mut gui).unwrap();
        assert_eq!(gui.diff_mode.commits.len(), 600);
        gui.diff_mode.commits_selected = 550;
        gui.diff_mode.set_focus(DiffModeFocus::Commits);
        (repo, gui)
    }

    #[test]
    fn comparison_completion_preserves_paged_commit_and_drilled_files() {
        let (repo, mut gui) = comparison_history();
        let hash = gui.diff_mode.selected_commit().unwrap().hash.clone();
        refresh_comparison(&mut gui);
        assert_eq!(gui.diff_mode.commits.len(), 600);
        assert_eq!(gui.diff_mode.selected_commit().unwrap().hash, hash);
        assert_eq!(gui.diff_mode.diff_source, CompareDiffSource::Commit);
        super::super::diff_mode::open_selected_commit_files(&mut gui).unwrap();
        gui.diff_mode.set_focus(DiffModeFocus::DiffExploration);
        let file = gui.diff_mode.diff_files[0].name.clone();
        git_script(
            &repo,
            r#"
            parent=$(git rev-parse feature)
            tree=$(git rev-parse 'feature^{tree}')
            i=1
            while [ "$i" -le 60 ]; do
                parent=$(git -c commit.gpgsign=false commit-tree "$tree" -p "$parent" -m "new $i")
                i=$((i + 1))
            done
            git update-ref refs/heads/feature "$parent"
        "#,
        );
        refresh_comparison(&mut gui);
        assert_eq!(gui.diff_mode.selected_commit().unwrap().hash, hash);
        assert_eq!(gui.diff_mode.commits_selected, 610);
        assert_eq!(gui.diff_mode.commits.len(), 900);
        assert!(gui.diff_mode.has_more_commits());
        assert_eq!(gui.diff_mode.files_commit.as_deref(), Some(hash.as_str()));
        assert_eq!(gui.diff_mode.diff_source, CompareDiffSource::CommitFiles);
        assert_eq!(gui.diff_mode.focus, DiffModeFocus::DiffExploration);
        assert_eq!(
            gui.diff_mode.diff_files[gui.diff_mode.diff_files_selected].name,
            file
        );
    }

    #[test]
    fn comparison_completion_removed_hash_or_ref_drops_stale_drilled_files() {
        let (repo, mut gui) = comparison_history();
        super::super::diff_mode::open_selected_commit_files(&mut gui).unwrap();
        let hash = gui.diff_mode.files_commit.clone().unwrap();
        // The object still exists, but main now contains it: it has genuinely
        // left A...B and must not retain an unrelated drilled file context.
        git_script(&repo, &format!("git update-ref refs/heads/main {hash}"));
        refresh_comparison(&mut gui);
        assert_eq!(gui.diff_mode.commits.len(), 550);
        assert_eq!(gui.diff_mode.commits_selected, 549);
        assert_ne!(gui.diff_mode.selected_commit().unwrap().hash, hash);
        assert!(gui.diff_mode.files_commit.is_none());
        assert_eq!(gui.diff_mode.diff_source, CompareDiffSource::Comparison);
        assert_eq!(
            gui.diff_mode
                .diff_files
                .iter()
                .map(|f| &f.name)
                .collect::<Vec<_>>(),
            gui.git
                .diff_refs_files("main", "feature")
                .unwrap()
                .iter()
                .map(|f| &f.name)
                .collect::<Vec<_>>()
        );
        super::super::diff_mode::open_selected_commit_files(&mut gui).unwrap();
        assert!(gui.diff_mode.files_commit.is_some());
        // The remaining ref still contains the drilled commit. An invalid
        // ref must not be mistaken for "not an ancestor" by the XOR check.
        git_script(&repo, "git update-ref -d refs/heads/main");
        refresh_comparison(&mut gui);
        assert!(gui.diff_mode.ahead_behind.is_none());
        assert!(gui.diff_mode.files_commit.is_none());
        assert!(gui.diff_mode.diff_files.is_empty());
        assert!(gui.diff_mode.commits.is_empty());
        assert_eq!(gui.diff_mode.diff_source, CompareDiffSource::Comparison);
    }

    fn outcome(success: bool, stdout: &str, stderr: &str) -> ShellResult {
        ShellResult {
            stdout: stdout.into(),
            stderr: stderr.into(),
            success,
            exit_code: Some(if success { 0 } else { 7 }),
            cancelled: false,
            timed_out: false,
        }
    }

    #[test]
    fn command_failure_takes_precedence_over_nonempty_stdout() {
        let (title, message, kind) =
            result_message(&outcome(false, "partial result", "failure reason"));
        assert_eq!(title, "Command failed (exit 7)");
        assert_eq!(kind, MessageKind::Error);
        assert!(message.contains("partial result"));
        assert!(message.contains("failure reason"));
    }

    #[test]
    fn stderr_only_success_and_no_output_are_reported() {
        let (title, message, kind) = result_message(&outcome(true, "", "warning"));
        assert_eq!(title, "Command output (exit 0)");
        assert_eq!(kind, MessageKind::Info);
        assert_eq!(message, "warning");
        assert_eq!(result_message(&outcome(true, "", "")).1, "No output.");
        assert_eq!(
            result_message(&outcome(false, "", "")).2,
            MessageKind::Error
        );
    }

    #[test]
    fn cancellation_timeout_and_signal_have_explicit_status() {
        let mut result = outcome(false, "", "");
        result.cancelled = true;
        assert_eq!(result_message(&result).0, "Command cancelled");
        result.cancelled = false;
        result.timed_out = true;
        assert!(result_message(&result).0.contains("timed out"));
        result.timed_out = false;
        result.exit_code = None;
        assert!(result_message(&result).0.contains("signal"));
    }

    #[test]
    fn shell_output_strips_terminal_controls_but_keeps_lines() {
        let message = result_message(&outcome(true, "\x1b[31mred\x1b[0m\nline\tvalue\x07\r", "")).1;
        assert_eq!(message, "red\nline    value");
    }
}
