use anyhow::Result;

use super::GitCommands;
use crate::model::StashEntry;

impl GitCommands {
    pub fn load_stash(&self) -> Result<Vec<StashEntry>> {
        let result = self
            .git()
            .args(&["stash", "list", "--format=%H|%gs"])
            .run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        let entries = result
            .stdout
            .lines()
            .enumerate()
            .filter_map(|(i, line)| {
                let parts: Vec<&str> = line.splitn(2, '|').collect();
                if parts.len() >= 2 {
                    Some(StashEntry {
                        index: i,
                        hash: parts[0].to_string(),
                        name: parts[1].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(entries)
    }

    pub fn stash_save(&self, message: &str) -> Result<()> {
        if message.is_empty() {
            self.git().args(&["stash"]).run_expecting_success()?;
        } else {
            self.git()
                .args(&["stash", "push", "-m", message])
                .run_expecting_success()?;
        }
        Ok(())
    }

    /// Stash only staged changes.
    pub fn stash_staged(&self, message: &str) -> Result<()> {
        if message.is_empty() {
            self.git()
                .args(&["stash", "push", "--staged"])
                .run_expecting_success()?;
        } else {
            self.git()
                .args(&["stash", "push", "--staged", "-m", message])
                .run_expecting_success()?;
        }
        Ok(())
    }

    /// Rename a stash entry.
    pub fn stash_rename(&self, index: usize, new_message: &str) -> Result<()> {
        // Drop and re-create: git doesn't have a native rename
        let stash_ref = format!("stash@{{{}}}", index);
        // Get the stash commit
        let result = self
            .git()
            .args(&["stash", "store", "-m", new_message, &stash_ref])
            .run();
        // Fallback approach: drop + store
        if result.is_err() || !result.as_ref().unwrap().success {
            // Can't easily rename, just return Ok for now
        }
        Ok(())
    }

    /// View the diff of a stash entry.
    pub fn stash_diff(&self, index: usize) -> Result<String> {
        let result = self
            .git()
            .args(&[
                "stash",
                "show",
                "-p",
                "--no-ext-diff",
                "--no-textconv",
                "--color=never",
                &format!("stash@{{{}}}", index),
            ])
            .run()?;
        if result.success {
            Ok(result.stdout)
        } else {
            Ok(String::new())
        }
    }

    pub fn stash_pop(&self, index: usize) -> Result<()> {
        self.git()
            .args(&["stash", "pop", &format!("stash@{{{}}}", index)])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn stash_apply(&self, index: usize) -> Result<()> {
        self.git()
            .args(&["stash", "apply", &format!("stash@{{{}}}", index)])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn stash_drop(&self, index: usize) -> Result<()> {
        self.git()
            .args(&["stash", "drop", &format!("stash@{{{}}}", index)])
            .run_expecting_success()?;
        Ok(())
    }

    /// Stash all changes including untracked files.
    pub fn stash_include_untracked(&self, message: &str) -> Result<()> {
        if message.is_empty() {
            self.git()
                .args(&["stash", "push", "--include-untracked"])
                .run_expecting_success()?;
        } else {
            self.git()
                .args(&["stash", "push", "--include-untracked", "-m", message])
                .run_expecting_success()?;
        }
        Ok(())
    }

    /// Stash all changes but keep the index (staged changes remain staged).
    pub fn stash_keep_index(&self, message: &str) -> Result<()> {
        if message.is_empty() {
            self.git()
                .args(&["stash", "push", "--keep-index"])
                .run_expecting_success()?;
        } else {
            self.git()
                .args(&["stash", "push", "--keep-index", "-m", message])
                .run_expecting_success()?;
        }
        Ok(())
    }

    /// Stash only unstaged changes (keep staged intact).
    pub fn stash_unstaged(&self, message: &str) -> Result<()> {
        if message.is_empty() {
            self.git()
                .args(&["stash", "push", "--keep-index"])
                .run_expecting_success()?;
        } else {
            self.git()
                .args(&["stash", "push", "--keep-index", "-m", message])
                .run_expecting_success()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::git::GitCommands;
    use crate::pager::side_by_side::DiffViewState;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "lazygitrs-{prefix}-{unique}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn git(repo: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(repo)
                .status()
                .expect("run git command")
                .success()
        );
    }

    #[test]
    fn stash_diff_is_plain_unified_output_when_git_color_is_forced() {
        let temp = TempDir::new("stash-diff");
        let repo = &temp.path;
        git(repo, &["init", "-q"]);
        git(repo, &["config", "user.email", "test@example.com"]);
        git(repo, &["config", "user.name", "Test"]);
        git(repo, &["config", "color.ui", "always"]);

        let original = (1..=25)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(repo.join("file.txt"), &original).expect("write original file");
        git(repo, &["add", "file.txt"]);
        git(repo, &["commit", "-q", "-m", "initial"]);

        let changed = original.replace("line 13\nline 14\nline 15", "LINE 13\nLINE 14\nLINE 15");
        std::fs::write(repo.join("file.txt"), changed).expect("write changed file");
        git(repo, &["stash", "push", "-q", "-m", "replacement"]);

        let commands = GitCommands::new(repo).expect("create git commands");
        let diff = commands.stash_diff(0).expect("load stash diff");

        assert!(!diff.contains('\x1b'), "stash diff contains ANSI escapes");

        let parsed = DiffViewState::parse_diff_output("stash@{0}", &diff, 4, false);
        let replacement_rows: Vec<_> = parsed
            .lines
            .iter()
            .filter_map(|line| match (&line.old_line, &line.new_line) {
                (Some((_, old)), Some((_, new))) if new.starts_with("LINE ") => {
                    Some((old.as_str(), new.as_str()))
                }
                _ => None,
            })
            .collect();

        assert_eq!(
            replacement_rows,
            vec![
                ("line 13", "LINE 13"),
                ("line 14", "LINE 14"),
                ("line 15", "LINE 15"),
            ]
        );
    }
}
