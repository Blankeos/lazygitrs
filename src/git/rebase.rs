use anyhow::Result;
use std::fs;
use std::path::PathBuf;

use super::GitCommands;

/// Actions that can be performed on commits during interactive rebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseAction {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl RebaseAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Reword => "reword",
            Self::Edit => "edit",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            Self::Drop => "drop",
        }
    }

    /// Cycle to the next action: Pick → Reword → Edit → Squash → Fixup → Drop → Pick.
    pub fn next(&self) -> Self {
        match self {
            Self::Pick => Self::Reword,
            Self::Reword => Self::Edit,
            Self::Edit => Self::Squash,
            Self::Squash => Self::Fixup,
            Self::Fixup => Self::Drop,
            Self::Drop => Self::Pick,
        }
    }

    /// Cycle to the previous action.
    pub fn prev(&self) -> Self {
        match self {
            Self::Pick => Self::Drop,
            Self::Reword => Self::Pick,
            Self::Edit => Self::Reword,
            Self::Squash => Self::Edit,
            Self::Fixup => Self::Squash,
            Self::Drop => Self::Fixup,
        }
    }
}

impl GitCommands {
    /// Interactive rebase: apply a single action to a specific commit.
    pub fn rebase_interactive_action(&self, commit_hash: &str, action: RebaseAction) -> Result<()> {
        let parent = self.commit_parent(commit_hash)?;
        let range = self.rebase_commit_range(&parent)?;
        // range is newest-first; convert to oldest-first actions.
        let mut actions: Vec<(String, RebaseAction)> = range
            .into_iter()
            .rev()
            .map(|hash| {
                let act = if hash == commit_hash
                    || hash.starts_with(commit_hash)
                    || commit_hash.starts_with(&hash)
                {
                    action
                } else {
                    RebaseAction::Pick
                };
                (hash, act)
            })
            .collect();
        if actions.is_empty() {
            actions.push((commit_hash.to_string(), action));
        }
        self.rebase_interactive_batch(&parent, &actions)
    }

    /// Move a commit up in the history (swap with its parent = older commit).
    /// In newest-first UI terms this moves the commit toward HEAD.
    pub fn move_commit_up(&self, commit_hash: &str) -> Result<()> {
        let parent = self.commit_parent(commit_hash)?;
        let grandparent = self.commit_parent(&parent)?;
        let range = self.rebase_commit_range(&grandparent)?; // newest-first
        let mut oldest_first: Vec<String> = range.into_iter().rev().collect();
        if let Some(idx) = oldest_first.iter().position(|h| h == commit_hash) {
            if idx + 1 < oldest_first.len() {
                oldest_first.swap(idx, idx + 1);
            }
        }
        let actions: Vec<(String, RebaseAction)> = oldest_first
            .into_iter()
            .map(|h| (h, RebaseAction::Pick))
            .collect();
        self.rebase_interactive_batch(&grandparent, &actions)
    }

    /// Move a commit down in the history (swap with its child = newer commit).
    pub fn move_commit_down(&self, commit_hash: &str) -> Result<()> {
        let parent = self.commit_parent(commit_hash)?;
        let grandparent = self.commit_parent(&parent)?;
        let range = self.rebase_commit_range(&grandparent)?;
        let mut oldest_first: Vec<String> = range.into_iter().rev().collect();
        if let Some(idx) = oldest_first.iter().position(|h| h == commit_hash) {
            if idx > 0 {
                oldest_first.swap(idx, idx - 1);
            }
        }
        let actions: Vec<(String, RebaseAction)> = oldest_first
            .into_iter()
            .map(|h| (h, RebaseAction::Pick))
            .collect();
        self.rebase_interactive_batch(&grandparent, &actions)
    }

    /// Reword a non-HEAD commit via interactive rebase.
    ///
    /// `--keep-empty` preserves empty commits through the replay so they can
    /// be reworded the same way lazygit does.
    pub fn reword_commit_rebase(&self, commit_hash: &str, new_message: &str) -> Result<()> {
        let parent = self.commit_parent(commit_hash)?;
        let range = self.rebase_commit_range(&parent)?;
        let actions: Vec<(String, RebaseAction)> = range
            .into_iter()
            .rev()
            .map(|hash| {
                let act = if hash == commit_hash
                    || hash.starts_with(commit_hash)
                    || commit_hash.starts_with(&hash)
                {
                    RebaseAction::Reword
                } else {
                    RebaseAction::Pick
                };
                (hash, act)
            })
            .collect();

        let todo_content = actions
            .iter()
            .map(|(hash, action)| format!("{} {}", action.as_str(), hash))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let (todo_path, script_path) = self.write_todo_editor(&todo_content)?;

        // Message editor: also needs an executable script (GIT_EDITOR is not
        // invoked via shell).
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let msg_path = std::env::temp_dir().join(format!(
            "lazygitrs-reword-msg-{}-{}",
            std::process::id(),
            unique
        ));
        let msg_script_path = std::env::temp_dir().join(format!(
            "lazygitrs-reword-editor-{}-{}",
            std::process::id(),
            unique
        ));
        fs::write(&msg_path, format!("{new_message}\n"))?;
        let escaped_msg = msg_path.display().to_string().replace('\'', "'\\''");
        fs::write(
            &msg_script_path,
            format!("#!/bin/sh\ncp '{escaped_msg}' \"$1\"\n"),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&msg_script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&msg_script_path, perms)?;
        }

        let result = self
            .git()
            .args(&["rebase", "-i", "--keep-empty", "--autostash", &parent])
            .env(
                "GIT_SEQUENCE_EDITOR",
                script_path.to_str().unwrap_or_default(),
            )
            .env("GIT_EDITOR", msg_script_path.to_str().unwrap_or_default())
            .run()?;

        let _ = fs::remove_file(&todo_path);
        let _ = fs::remove_file(&script_path);
        let _ = fs::remove_file(&msg_path);
        let _ = fs::remove_file(&msg_script_path);

        if !result.success {
            let git_dir = self.repo_path().join(".git");
            if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
                return Ok(());
            }
            anyhow::bail!(
                "Reword rebase failed (exit {}): {}",
                result.exit_code.unwrap_or(-1),
                result.stderr.trim()
            );
        }
        Ok(())
    }

    /// Create a fixup commit for the given target commit.
    pub fn create_fixup_commit(&self, target_hash: &str) -> Result<()> {
        self.git()
            .args(&["commit", "--fixup", target_hash])
            .run_expecting_success()?;
        Ok(())
    }

    /// Autosquash: rebase with --autosquash to apply fixup/squash commits.
    pub fn rebase_autosquash(&self, base_hash: &str) -> Result<()> {
        self.git()
            .args(&[
                "rebase",
                "-i",
                "--autosquash",
                "--autostash",
                "--rebase-merges",
                base_hash,
            ])
            .env("GIT_SEQUENCE_EDITOR", "true")
            .run_expecting_success()?;
        Ok(())
    }

    /// Skip during a rebase (when there's a conflict).
    pub fn rebase_skip(&self) -> Result<()> {
        let result = self.git().args(&["rebase", "--skip"]).run()?;
        self.handle_rebase_step_result("--skip", result)
    }

    /// Interactive rebase with a full todo list: apply multiple actions in one shot.
    /// `actions` must be in rebase-todo order (oldest commit first, newest last).
    /// Each entry is (commit_hash, action).
    pub fn rebase_interactive_batch(
        &self,
        base_hash: &str,
        actions: &[(String, RebaseAction)],
    ) -> Result<()> {
        let mut todo_lines = Vec::new();
        for (hash, action) in actions {
            // Prefer the full hash so matching is unambiguous across branches.
            todo_lines.push(format!("{} {}", action.as_str(), hash));
        }
        let todo_content = todo_lines.join("\n") + "\n";

        // GIT_SEQUENCE_EDITOR is invoked as an executable (not via shell), so
        // write a tiny script that copies our prepared todo into place.
        let (todo_path, script_path) = self.write_todo_editor(&todo_content)?;

        let result = self
            .git()
            .args(&["rebase", "-i", "--autostash", base_hash])
            .env(
                "GIT_SEQUENCE_EDITOR",
                script_path.to_str().unwrap_or_default(),
            )
            // Prevent git from opening an interactive editor for reword/edit
            // actions. `true` exits 0 without modifying COMMIT_EDITMSG, so
            // reword keeps the original message (reword message editing is
            // handled in the TUI before execution).
            .env("GIT_EDITOR", "true")
            .run()?;

        let _ = fs::remove_file(&todo_path);
        let _ = fs::remove_file(&script_path);

        if !result.success {
            // Exit code 1 with rebase-merge dir = rebase paused (edit/conflict).
            // This is expected — the caller should refresh to detect InProgress.
            let git_dir = self.repo_path().join(".git");
            if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
                // Rebase paused — not an error
                return Ok(());
            }
            // Real failure
            anyhow::bail!(
                "Rebase failed (exit {}): {}",
                result.exit_code.unwrap_or(-1),
                result.stderr.trim()
            );
        }
        Ok(())
    }

    /// Interactive rebase from the root (no parent base).
    pub fn rebase_interactive_batch_root(&self, actions: &[(String, RebaseAction)]) -> Result<()> {
        let mut todo_lines = Vec::new();
        for (hash, action) in actions {
            todo_lines.push(format!("{} {}", action.as_str(), hash));
        }
        let todo_content = todo_lines.join("\n") + "\n";
        let (todo_path, script_path) = self.write_todo_editor(&todo_content)?;

        let result = self
            .git()
            .args(&["rebase", "-i", "--autostash", "--root"])
            .env(
                "GIT_SEQUENCE_EDITOR",
                script_path.to_str().unwrap_or_default(),
            )
            .env("GIT_EDITOR", "true")
            .run()?;

        let _ = fs::remove_file(&todo_path);
        let _ = fs::remove_file(&script_path);

        if !result.success {
            let git_dir = self.repo_path().join(".git");
            if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
                return Ok(());
            }
            anyhow::bail!(
                "Rebase failed (exit {}): {}",
                result.exit_code.unwrap_or(-1),
                result.stderr.trim()
            );
        }
        Ok(())
    }

    /// Write the prepared todo content and a small executable editor script.
    /// Returns `(todo_path, script_path)`.
    fn write_todo_editor(&self, content: &str) -> Result<(PathBuf, PathBuf)> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let prefix = format!("lazygitrs-rebase-{}-{}", std::process::id(), unique);
        let todo_path = std::env::temp_dir().join(format!("{prefix}.todo"));
        let script_path = std::env::temp_dir().join(format!("{prefix}.sh"));

        fs::write(&todo_path, content)?;
        // Escape single quotes for safe embedding in the shell script.
        let escaped = todo_path.display().to_string().replace('\'', "'\\''");
        let script = format!("#!/bin/sh\ncp '{escaped}' \"$1\"\n");
        fs::write(&script_path, script)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms)?;
        }

        Ok((todo_path, script_path))
    }

    /// True when `candidate` is an ancestor of `commit` (or equal).
    pub fn is_ancestor(&self, candidate: &str, commit: &str) -> bool {
        self.git()
            .args(&["merge-base", "--is-ancestor", candidate, commit])
            .run()
            .map(|r| r.success)
            .unwrap_or(false)
    }

    /// Get the list of commit hashes that would be rebased when running
    /// `git rebase -i <base>`. Returns hashes in newest-first order.
    pub fn rebase_commit_range(&self, base_hash: &str) -> Result<Vec<String>> {
        let result = self
            .git()
            .args(&["rev-list", "--reverse", &format!("{}..HEAD", base_hash)])
            .run_expecting_success()?;
        let hashes: Vec<String> = result
            .stdout_trimmed()
            .lines()
            .rev() // reverse to newest-first for display
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(hashes)
    }

    /// Parse the state of a rebase that is currently in progress.
    /// Returns `None` if no rebase is in progress.
    pub fn parse_rebase_progress(&self) -> Option<RebaseProgress> {
        let git_dir = self.repo_path().join(".git");

        // Try rebase-merge first (interactive rebase), then rebase-apply
        let rebase_dir = if git_dir.join("rebase-merge").exists() {
            git_dir.join("rebase-merge")
        } else if git_dir.join("rebase-apply").exists() {
            git_dir.join("rebase-apply")
        } else {
            return None;
        };

        // Read head-name (branch being rebased)
        let head_name = std::fs::read_to_string(rebase_dir.join("head-name"))
            .ok()
            .map(|s| {
                s.trim()
                    .strip_prefix("refs/heads/")
                    .unwrap_or(s.trim())
                    .to_string()
            })
            .unwrap_or_default();

        // Read onto hash
        let onto_hash = std::fs::read_to_string(rebase_dir.join("onto"))
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let onto_short = onto_hash[..7.min(onto_hash.len())].to_string();

        // onto_message is left empty here; callers should invoke `hydrate_progress`
        // to fill it together with todo entries in a single batched `git log` call.
        let onto_message = String::new();

        // Parse "done" file — already-processed entries
        let done_entries = std::fs::read_to_string(rebase_dir.join("done"))
            .ok()
            .map(|content| parse_todo_entries(&content))
            .unwrap_or_default();

        // Parse "git-rebase-todo" — remaining entries
        let todo_entries = std::fs::read_to_string(rebase_dir.join("git-rebase-todo"))
            .ok()
            .map(|content| parse_todo_entries(&content))
            .unwrap_or_default();

        // Read stopped-sha (the commit where rebase paused)
        let stopped_sha = std::fs::read_to_string(rebase_dir.join("stopped-sha"))
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        Some(RebaseProgress {
            head_name,
            onto_hash,
            onto_short,
            onto_message,
            onto_author_name: String::new(),
            done_entries,
            todo_entries,
            stopped_sha,
        })
    }

    /// Hydrate every part of a `RebaseProgress` (done entries, todo entries,
    /// and onto-commit subject) using a single batched `git log` invocation.
    /// Each `git` subprocess spawn dominates the latency on entry to the
    /// InProgress view, so we coalesce what would otherwise be three calls.
    pub fn hydrate_progress(&self, progress: &mut RebaseProgress) {
        let need_onto = (progress.onto_message.is_empty() || progress.onto_author_name.is_empty())
            && !progress.onto_hash.is_empty();
        if progress.done_entries.is_empty() && progress.todo_entries.is_empty() && !need_onto {
            return;
        }

        let mut cmd = self.git();
        cmd = cmd
            .arg("log")
            .arg("--no-walk")
            .arg("--format=%H|%s|%an|%at");
        for e in &progress.done_entries {
            cmd = cmd.arg(&e.hash);
        }
        for e in &progress.todo_entries {
            cmd = cmd.arg(&e.hash);
        }
        if need_onto {
            cmd = cmd.arg(&progress.onto_hash);
        }
        let result = match cmd.run() {
            Ok(r) if r.success => r,
            _ => return,
        };

        let mut info: std::collections::HashMap<String, (String, String, i64)> =
            std::collections::HashMap::new();
        for line in result.stdout.lines() {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() >= 4 {
                let hash = parts[0].to_string();
                let subject = parts[1].to_string();
                let author = parts[2].to_string();
                let ts = parts[3].parse::<i64>().unwrap_or(0);
                info.insert(hash, (subject, author, ts));
            }
        }

        let apply =
            |entry: &mut TodoEntry,
             info: &std::collections::HashMap<String, (String, String, i64)>| {
                if let Some((subject, author, ts)) = info.get(&entry.hash) {
                    if !subject.is_empty() {
                        entry.message = subject.clone();
                    }
                    entry.author_name = author.clone();
                    entry.unix_timestamp = *ts;
                }
            };
        for e in progress.done_entries.iter_mut() {
            apply(e, &info);
        }
        for e in progress.todo_entries.iter_mut() {
            apply(e, &info);
        }
        if need_onto {
            if let Some((subject, author, _)) = info.get(&progress.onto_hash) {
                progress.onto_message = subject.clone();
                progress.onto_author_name = author.clone();
            }
        }
    }

    /// Persist the remaining todo entries for an in-progress rebase.
    /// Call this after the user reorders or changes actions on remaining
    /// commits so `git rebase --continue` honors those edits.
    pub fn write_rebase_todo(&self, entries: &[(String, RebaseAction)]) -> Result<()> {
        let git_dir = self.repo_path().join(".git");
        let rebase_dir = if git_dir.join("rebase-merge").exists() {
            git_dir.join("rebase-merge")
        } else if git_dir.join("rebase-apply").exists() {
            git_dir.join("rebase-apply")
        } else {
            anyhow::bail!("no rebase in progress");
        };

        let mut lines = Vec::with_capacity(entries.len());
        for (hash, action) in entries {
            // Newest-first UI → oldest-first todo file.
            lines.push(format!("{} {}", action.as_str(), hash));
        }
        // Callers pass newest-first (matching the TUI). Reverse for git.
        lines.reverse();
        let content = if lines.is_empty() {
            String::new()
        } else {
            lines.join("\n") + "\n"
        };
        fs::write(rebase_dir.join("git-rebase-todo"), content)?;
        Ok(())
    }

    /// Get the parent hash of a commit.
    fn commit_parent(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["rev-parse", &format!("{}^", hash)])
            .run_expecting_success()?;
        Ok(result.stdout_trimmed().to_string())
    }
}

/// Represents the state of a rebase in progress.
#[derive(Debug, Clone)]
pub struct RebaseProgress {
    /// Branch being rebased (e.g. "my-feature").
    pub head_name: String,
    /// Full hash of the commit being rebased onto.
    pub onto_hash: String,
    /// Short hash of the onto commit.
    pub onto_short: String,
    /// Subject of the onto commit.
    pub onto_message: String,
    /// Author name of the onto commit.
    pub onto_author_name: String,
    /// Entries that have already been processed.
    pub done_entries: Vec<TodoEntry>,
    /// Entries still to be processed.
    pub todo_entries: Vec<TodoEntry>,
    /// The commit hash where the rebase paused (conflict/edit).
    pub stopped_sha: String,
}

/// A single entry from a rebase todo/done file.
#[derive(Debug, Clone)]
pub struct TodoEntry {
    pub action: RebaseAction,
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author_name: String,
    pub unix_timestamp: i64,
}

/// Parse a git rebase todo/done file into entries.
/// Format: `<action> <hash> <message>`
fn parse_todo_entries(content: &str) -> Vec<TodoEntry> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') || line.starts_with("noop") {
                return None;
            }
            let mut parts = line.splitn(3, ' ');
            let action_str = parts.next()?;
            let hash = parts.next().unwrap_or("").to_string();
            let message = parts.next().unwrap_or("").to_string();

            let action = match action_str {
                "pick" | "p" => RebaseAction::Pick,
                "reword" | "r" => RebaseAction::Reword,
                "edit" | "e" => RebaseAction::Edit,
                "squash" | "s" => RebaseAction::Squash,
                "fixup" | "f" => RebaseAction::Fixup,
                "drop" | "d" => RebaseAction::Drop,
                _ => return None, // skip break, exec, label, etc.
            };

            let short_hash = hash[..7.min(hash.len())].to_string();

            Some(TodoEntry {
                action,
                hash,
                short_hash,
                message,
                author_name: String::new(),
                unix_timestamp: 0,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "lazygitrs-{prefix}-{unique}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("mkdir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn git_in(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo(dir: &Path) {
        git_in(dir, &["init"]);
        git_in(dir, &["config", "user.email", "t@t.com"]);
        git_in(dir, &["config", "user.name", "t"]);
    }

    fn commit(dir: &Path, msg: &str, content: &str) -> String {
        std::fs::write(dir.join("f"), content).unwrap();
        git_in(dir, &["add", "f"]);
        git_in(dir, &["commit", "-m", msg]);
        rev_parse(dir, "HEAD")
    }

    fn rev_parse(dir: &Path, rev: &str) -> String {
        let out = Command::new("git")
            .args(["rev-parse", rev])
            .current_dir(dir)
            .output()
            .expect("rev-parse");
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn edit_stops_on_selected_commit_not_parent() {
        let temp = TempDir::new("rebase-edit-target");
        let dir = temp.path();
        init_repo(dir);
        let _c1 = commit(dir, "c1", "1");
        let c2 = commit(dir, "c2", "2");
        let c3 = commit(dir, "c3", "3");
        let c4 = commit(dir, "c4", "4");

        let git = GitCommands::new(dir).expect("git");
        // Edit c3: base must be c2 (parent), actions oldest-first.
        let actions = vec![
            (c3.clone(), RebaseAction::Edit),
            (c4.clone(), RebaseAction::Pick),
        ];
        git.rebase_interactive_batch(&c2, &actions)
            .expect("rebase should pause at edit");

        assert!(
            git.is_rebase_in_progress() || dir.join(".git/rebase-merge").exists(),
            "rebase should be paused at edit"
        );

        let stopped = std::fs::read_to_string(dir.join(".git/rebase-merge/stopped-sha"))
            .expect("stopped-sha")
            .trim()
            .to_string();
        assert_eq!(
            stopped, c3,
            "edit must stop ON the selected commit, not its parent"
        );

        let _ = git.abort_rebase();
    }

    #[test]
    fn edit_targets_correct_commit_with_divergent_branches() {
        // Regression: using commits[idx+1] as base on an `--all` topo list
        // picks an unrelated branch tip as the parent and rebases the wrong
        // range. Parent-hash + base..HEAD must keep the edit on the selected
        // commit even when other branches interleave in the panel.
        let temp = TempDir::new("rebase-edit-divergent");
        let dir = temp.path();
        init_repo(dir);
        let _c1 = commit(dir, "c1", "1");
        let c2 = commit(dir, "c2", "2");
        let c3 = commit(dir, "c3", "3");

        git_in(dir, &["checkout", "-b", "feature"]);
        let _f1 = commit(dir, "feature1", "x");
        let _f2 = commit(dir, "feature2", "y");

        // Prefer master/main depending on init default.
        let main = if Command::new("git")
            .args(["rev-parse", "--verify", "main"])
            .current_dir(dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            "main"
        } else {
            "master"
        };
        git_in(dir, &["checkout", main]);
        let c4 = commit(dir, "c4", "4");

        let git = GitCommands::new(dir).expect("git");
        // Select c3 on the current branch — real parent is c2, not a feature tip.
        let actions = vec![
            (c3.clone(), RebaseAction::Edit),
            (c4.clone(), RebaseAction::Pick),
        ];
        git.rebase_interactive_batch(&c2, &actions)
            .expect("rebase should pause at edit");

        let stopped = std::fs::read_to_string(dir.join(".git/rebase-merge/stopped-sha"))
            .expect("stopped-sha")
            .trim()
            .to_string();
        assert_eq!(stopped, c3);

        let _ = git.abort_rebase();
    }

    #[test]
    fn reword_updates_selected_commit_message() {
        let temp = TempDir::new("rebase-reword");
        let dir = temp.path();
        init_repo(dir);
        let _c1 = commit(dir, "c1", "1");
        let c2 = commit(dir, "c2", "2");
        let _c3 = commit(dir, "c3", "3");

        let git = GitCommands::new(dir).expect("git");
        git.reword_commit_rebase(&c2, "c2-rewritten")
            .expect("reword");

        assert!(!git.is_rebase_in_progress());
        let msg = git.commit_subject(&rev_parse(dir, "HEAD~1")).unwrap();
        assert_eq!(msg, "c2-rewritten");
    }

    #[test]
    fn write_rebase_todo_persists_pending_actions() {
        let temp = TempDir::new("rebase-write-todo");
        let dir = temp.path();
        init_repo(dir);
        let _c1 = commit(dir, "c1", "1");
        let c2 = commit(dir, "c2", "2");
        let c3 = commit(dir, "c3", "3");
        let c4 = commit(dir, "c4", "4");

        let git = GitCommands::new(dir).expect("git");
        let actions = vec![
            (c3.clone(), RebaseAction::Edit),
            (c4.clone(), RebaseAction::Pick),
        ];
        git.rebase_interactive_batch(&c2, &actions)
            .expect("pause at edit");

        // After stopping on c3, only c4 remains pending. Change it to drop.
        git.write_rebase_todo(&[(c4.clone(), RebaseAction::Drop)])
            .expect("write todo");

        let todo =
            std::fs::read_to_string(dir.join(".git/rebase-merge/git-rebase-todo")).expect("todo");
        assert!(
            todo.contains(&format!("drop {c4}")),
            "todo should contain drop of remaining commit, got:\n{todo}"
        );

        let _ = git.abort_rebase();
    }
}
