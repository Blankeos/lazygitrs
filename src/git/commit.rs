use anyhow::Result;

use super::GitCommands;
use crate::model::{Commit, CommitStatus, commit::Divergence};

impl GitCommands {
    pub fn load_commits(&self, limit: usize) -> Result<Vec<Commit>> {
        let format = "%H|%s|%an|%ae|%at|%P|%D";
        let result = self
            .git()
            .args(&[
                "log",
                &format!("--max-count={}", limit),
                &format!("--format={}", format),
                "--no-show-signature",
            ])
            .run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        let head_hash = self.head_hash().unwrap_or_default();

        // Get the upstream to determine pushed/unpushed status
        let unpushed_hashes = self.unpushed_commit_hashes().unwrap_or_default();

        let mut commits = Vec::new();
        for line in result.stdout.lines() {
            let parts: Vec<&str> = line.splitn(7, '|').collect();
            if parts.len() < 6 {
                continue;
            }

            let hash = parts[0].to_string();
            let name = parts[1].to_string();
            let author_name = parts[2].to_string();
            let author_email = parts[3].to_string();
            let unix_timestamp = parts[4].parse::<i64>().unwrap_or(0);
            let parents: Vec<String> = parts[5].split_whitespace().map(String::from).collect();

            let tags = if parts.len() > 6 {
                extract_tags(parts[6])
            } else {
                Vec::new()
            };

            let status = if unpushed_hashes.contains(&hash) {
                CommitStatus::Unpushed
            } else {
                CommitStatus::Pushed
            };

            commits.push(Commit {
                hash,
                name,
                status,
                action: String::new(),
                tags,
                extra_info: String::new(),
                author_name,
                author_email,
                unix_timestamp,
                parents,
                divergence: Divergence::None,
            });
        }

        Ok(commits)
    }

    fn unpushed_commit_hashes(&self) -> Result<Vec<String>> {
        let result = self
            .git()
            .args(&["log", "@{u}..HEAD", "--format=%H"])
            .run()?;

        if !result.success {
            // No upstream or other error — treat all as unpushed
            return Ok(Vec::new());
        }

        Ok(result.stdout.lines().map(String::from).collect())
    }

    pub fn create_commit(&self, message: &str, skip_hooks: bool) -> Result<()> {
        let mut cmd = self.git();
        cmd = cmd.arg("commit").arg("-m").arg(message);
        if skip_hooks {
            cmd = cmd.arg("--no-verify");
        }
        cmd.run_expecting_success()?;
        Ok(())
    }

    pub fn amend_commit(&self) -> Result<()> {
        self.git()
            .args(&["commit", "--amend", "--no-edit"])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn reword_commit(&self, hash: &str, message: &str) -> Result<()> {
        // For HEAD commit, use amend
        let head = self.head_hash()?;
        if hash == head {
            self.git()
                .args(&["commit", "--amend", "-m", message])
                .run_expecting_success()?;
        } else {
            // For non-HEAD commits, delegate to interactive rebase
            self.reword_commit_rebase(hash, message)?;
        }
        Ok(())
    }

    pub fn revert_commit(&self, hash: &str) -> Result<()> {
        self.git()
            .args(&["revert", hash])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn cherry_pick(&self, hashes: &[String]) -> Result<()> {
        let mut cmd = self.git();
        cmd = cmd.arg("cherry-pick");
        for hash in hashes {
            cmd = cmd.arg(hash.as_str());
        }
        cmd.run_expecting_success()?;
        Ok(())
    }

    pub fn reset_to_commit(&self, hash: &str, mode: &str) -> Result<()> {
        self.git()
            .args(&["reset", mode, hash])
            .run_expecting_success()?;
        Ok(())
    }
}

fn extract_tags(decoration: &str) -> Vec<String> {
    decoration
        .split(", ")
        .filter_map(|d| {
            let d = d.trim();
            if let Some(tag) = d.strip_prefix("tag: ") {
                Some(tag.to_string())
            } else {
                None
            }
        })
        .collect()
}
