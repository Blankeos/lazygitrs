use anyhow::Result;

use super::GitCommands;
use crate::model::{File, FileStatus};

impl GitCommands {
    pub fn load_files(&self) -> Result<Vec<File>> {
        let result = self
            .git()
            .args(&["status", "--porcelain", "-u"])
            .run_expecting_success()?;

        let mut files = Vec::new();
        for line in result.stdout.lines() {
            if line.len() < 4 {
                continue;
            }

            let x = line.chars().nth(0).unwrap_or(' ');
            let y = line.chars().nth(1).unwrap_or(' ');
            let name = line[3..].to_string();

            let (has_staged, has_unstaged, tracked, status) = parse_status_codes(x, y);

            let display_name = if name.contains(" -> ") {
                // Renamed file: "old -> new"
                name.split(" -> ").last().unwrap_or(&name).to_string()
            } else {
                name.clone()
            };

            files.push(File {
                short_status: format!("{}{}", x, y),
                name,
                display_name,
                status,
                has_staged_changes: has_staged,
                has_unstaged_changes: has_unstaged,
                tracked,
                added: x == 'A' || y == 'A' || !tracked,
                deleted: x == 'D' || y == 'D',
                has_merge_conflicts: x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D'),
            });
        }

        Ok(files)
    }

    pub fn stage_file(&self, path: &str) -> Result<()> {
        self.git()
            .args(&["add", "--", path])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn unstage_file(&self, path: &str) -> Result<()> {
        self.git()
            .args(&["reset", "HEAD", "--", path])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn stage_all(&self) -> Result<()> {
        self.git().args(&["add", "-A"]).run_expecting_success()?;
        Ok(())
    }

    pub fn unstage_all(&self) -> Result<()> {
        self.git()
            .args(&["reset", "HEAD"])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn discard_file(&self, path: &str, added: bool) -> Result<()> {
        // Unstage first if needed (ignore errors — file may not be staged)
        let _ = self.git()
            .args(&["reset", "HEAD", "--", path])
            .run();

        if added {
            // New/untracked file: just delete it
            let full_path = self.repo_path().join(path);
            if full_path.is_dir() {
                std::fs::remove_dir_all(&full_path)?;
            } else {
                std::fs::remove_file(&full_path)?;
            }
        } else {
            // Tracked file: discard working tree changes
            self.git()
                .args(&["checkout", "--", path])
                .run_expecting_success()?;
        }
        Ok(())
    }

    pub fn ignore_file(&self, path: &str) -> Result<()> {
        let gitignore = self.repo_path().join(".gitignore");
        let mut contents = std::fs::read_to_string(&gitignore).unwrap_or_default();
        if !contents.ends_with('\n') && !contents.is_empty() {
            contents.push('\n');
        }
        contents.push_str(path);
        contents.push('\n');
        std::fs::write(gitignore, contents)?;
        Ok(())
    }
}

fn parse_status_codes(x: char, y: char) -> (bool, bool, bool, FileStatus) {
    match (x, y) {
        ('?', '?') => (false, true, false, FileStatus::Untracked),
        ('A', ' ') => (true, false, true, FileStatus::Added),
        ('A', 'M') => (true, true, true, FileStatus::Added),
        ('M', ' ') => (true, false, true, FileStatus::Modified),
        (' ', 'M') => (false, true, true, FileStatus::Modified),
        ('M', 'M') => (true, true, true, FileStatus::Modified),
        ('D', ' ') => (true, false, true, FileStatus::Deleted),
        (' ', 'D') => (false, true, true, FileStatus::Deleted),
        ('R', ' ') | ('R', 'M') => (true, false, true, FileStatus::Renamed),
        ('C', ' ') | ('C', 'M') => (true, false, true, FileStatus::Copied),
        ('U', 'U') | ('A', 'A') | ('D', 'D') | ('U', 'A') | ('A', 'U') | ('U', 'D') | ('D', 'U') => {
            (false, true, true, FileStatus::Unmerged)
        }
        _ => (x != ' ', y != ' ', true, FileStatus::Modified),
    }
}
