use anyhow::Result;

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
    /// Uses GIT_SEQUENCE_EDITOR to non-interactively modify the todo list.
    pub fn rebase_interactive_action(
        &self,
        commit_hash: &str,
        action: RebaseAction,
    ) -> Result<()> {
        // Find the parent of the target commit for the rebase base
        let parent = self.commit_parent(commit_hash)?;

        // Build a sed command to change "pick <hash>" to "<action> <hash>"
        let short_hash = &commit_hash[..7.min(commit_hash.len())];
        let sed_cmd = format!(
            "sed -i '' 's/^pick {} /{} {} /'",
            short_hash,
            action.as_str(),
            short_hash
        );

        self.git()
            .args(&["rebase", "-i", &parent])
            .env("GIT_SEQUENCE_EDITOR", &sed_cmd)
            .run_expecting_success()?;
        Ok(())
    }

    /// Move a commit up in the history (swap with the one above it).
    pub fn move_commit_up(&self, commit_hash: &str) -> Result<()> {
        let parent = self.commit_parent(commit_hash)?;
        let grandparent = self.commit_parent(&parent)?;

        let short_hash = &commit_hash[..7.min(commit_hash.len())];
        let short_parent = &parent[..7.min(parent.len())];

        // Swap the two lines in the todo list
        let sed_cmd = format!(
            "sed -i '' '/^pick {}/{{ N; s/^\\(pick {}.*\\)\\n\\(pick {}.*\\)/\\2\\n\\1/ }}'",
            short_parent, short_parent, short_hash
        );

        self.git()
            .args(&["rebase", "-i", &grandparent])
            .env("GIT_SEQUENCE_EDITOR", &sed_cmd)
            .run_expecting_success()?;
        Ok(())
    }

    /// Move a commit down in the history (swap with the one below it).
    pub fn move_commit_down(&self, commit_hash: &str) -> Result<()> {
        // Find the commit below (child direction = parent in rebase list)
        let parent = self.commit_parent(commit_hash)?;
        let grandparent = self.commit_parent(&parent)?;

        let short_hash = &commit_hash[..7.min(commit_hash.len())];
        let short_parent = &parent[..7.min(parent.len())];

        // Swap: move the target commit below the parent
        let sed_cmd = format!(
            "sed -i '' '/^pick {}/{{ N; s/^\\(pick {}.*\\)\\n\\(pick {}.*\\)/\\2\\n\\1/ }}'",
            short_hash, short_hash, short_parent
        );

        self.git()
            .args(&["rebase", "-i", &grandparent])
            .env("GIT_SEQUENCE_EDITOR", &sed_cmd)
            .run_expecting_success()?;
        Ok(())
    }

    /// Reword a non-HEAD commit via interactive rebase.
    pub fn reword_commit_rebase(&self, commit_hash: &str, new_message: &str) -> Result<()> {
        let parent = self.commit_parent(commit_hash)?;
        let short_hash = &commit_hash[..7.min(commit_hash.len())];

        // First, set the action to "reword"
        let sed_cmd = format!(
            "sed -i '' 's/^pick {} /reword {} /'",
            short_hash, short_hash
        );

        // Use GIT_SEQUENCE_EDITOR for the todo list and EDITOR for the message
        let echo_cmd = format!("echo '{}' >", new_message.replace('\'', "'\\''"));

        self.git()
            .args(&["rebase", "-i", &parent])
            .env("GIT_SEQUENCE_EDITOR", &sed_cmd)
            .env("GIT_EDITOR", &echo_cmd)
            .run_expecting_success()?;
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
        self.git()
            .args(&["rebase", "--skip"])
            .run_expecting_success()?;
        Ok(())
    }

    /// Interactive rebase with a full todo list: apply multiple actions in one shot.
    /// `actions` must be in rebase-todo order (oldest commit first, newest last).
    /// Each entry is (commit_hash, action).
    pub fn rebase_interactive_batch(
        &self,
        base_hash: &str,
        actions: &[(String, RebaseAction)],
    ) -> Result<()> {
        // Build the replacement todo content.
        // Each line: "<action> <short_hash>"
        // Git will match the short hash to the full commit in the todo list.
        let mut todo_lines = Vec::new();
        for (hash, action) in actions {
            let short = &hash[..7.min(hash.len())];
            todo_lines.push(format!("{} {}", action.as_str(), short));
        }
        let todo_content = todo_lines.join("\n");

        // The sequence editor script replaces the todo file with our content.
        // Using printf for portability (avoids echo -e differences across platforms).
        let editor_script = format!(
            "printf '{}\\n' > \"$1\"",
            todo_content.replace('\'', "'\\''")
        );

        let result = self.git()
            .args(&["rebase", "-i", "--autostash", base_hash])
            .env("GIT_SEQUENCE_EDITOR", &editor_script)
            // Prevent git from opening an interactive editor for reword/edit
            // actions. `true` exits 0 without modifying COMMIT_EDITMSG, so
            // reword keeps the original message (reword message editing is
            // handled in the TUI before execution).
            .env("GIT_EDITOR", "true")
            .run()?;

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
            .map(|s| s.trim().strip_prefix("refs/heads/").unwrap_or(s.trim()).to_string())
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
        if progress.done_entries.is_empty()
            && progress.todo_entries.is_empty()
            && !need_onto
        {
            return;
        }

        let mut cmd = self.git();
        cmd = cmd.arg("log").arg("--no-walk").arg("--format=%H|%s|%an|%at");
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

        let apply = |entry: &mut TodoEntry, info: &std::collections::HashMap<String, (String, String, i64)>| {
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
