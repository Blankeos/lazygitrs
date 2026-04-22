use anyhow::Result;

use super::GitCommands;
use crate::model::{Commit, CommitStatus, commit::{CommitStat, Divergence}};

impl GitCommands {
    /// Load commits from all branches (--all) so the graph shows the full topology.
    pub fn load_commits(&self, limit: usize) -> Result<Vec<Commit>> {
        self.load_commits_inner(limit, true, None)
    }

    /// Load commits reachable from a specific branch only.
    pub fn load_commits_for_branch(&self, branch: &str, limit: usize) -> Result<Vec<Commit>> {
        self.load_commits_for_branches(&[branch.to_string()], limit)
    }

    /// Load commits reachable from any of the given branches.
    pub fn load_commits_for_branches(&self, branches: &[String], limit: usize) -> Result<Vec<Commit>> {
        let format = "%H|%s|%an|%ae|%at|%P|%D";
        let mut cmd = self.git();
        cmd = cmd.arg("log");
        for b in branches {
            cmd = cmd.arg(b);
        }
        if limit > 0 {
            cmd = cmd.arg(&format!("--max-count={}", limit));
        }
        cmd = cmd
            .arg(&format!("--format={}", format))
            .arg("--no-show-signature")
            .arg("--topo-order");

        let result = cmd.run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        let _head_hash = self.head_hash().unwrap_or_default();
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

            let decoration = if parts.len() > 6 { parts[6] } else { "" };
            let tags = extract_tags(decoration);
            let refs = extract_refs(decoration);

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
                refs,
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

    fn load_commits_inner(
        &self,
        limit: usize,
        all: bool,
        _branch: Option<&str>,
    ) -> Result<Vec<Commit>> {
        let format = "%H|%s|%an|%ae|%at|%P|%D";
        let mut cmd = self.git();
        cmd = cmd.arg("log");
        if all {
            cmd = cmd.arg("--all");
        }
        if limit > 0 {
            cmd = cmd.arg(&format!("--max-count={}", limit));
        }
        cmd = cmd
            .arg(&format!("--format={}", format))
            .arg("--no-show-signature")
            .arg("--topo-order");

        let result = cmd.run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        let _head_hash = self.head_hash().unwrap_or_default();
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

            let decoration = if parts.len() > 6 { parts[6] } else { "" };
            let tags = extract_tags(decoration);
            let refs = extract_refs(decoration);

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
                refs,
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

    /// Load reflog entries using `git log -g`.
    pub fn load_reflog(&self, limit: usize) -> Result<Vec<Commit>> {
        let format = "%H|%gs|%an|%ae|%at|%P";
        let result = self
            .git()
            .args(&[
                "log",
                "-g",
                &format!("--max-count={}", limit),
                &format!("--format={}", format),
                "--no-show-signature",
            ])
            .run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        let mut commits = Vec::new();
        for line in result.stdout.lines() {
            let parts: Vec<&str> = line.splitn(6, '|').collect();
            if parts.len() < 5 {
                continue;
            }

            let hash = parts[0].to_string();
            let name = parts[1].to_string();
            let author_name = parts[2].to_string();
            let author_email = parts[3].to_string();
            let unix_timestamp = parts[4].parse::<i64>().unwrap_or(0);
            let parents: Vec<String> = if parts.len() > 5 {
                parts[5].split_whitespace().map(String::from).collect()
            } else {
                Vec::new()
            };

            commits.push(Commit {
                hash,
                name,
                status: CommitStatus::Reflog,
                action: String::new(),
                tags: Vec::new(),
                refs: Vec::new(),
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
            return Ok(Vec::new());
        }

        Ok(result.stdout.lines().map(String::from).collect())
    }

    pub fn create_commit(&self, message: &str, sign_off: bool) -> Result<()> {
        let mut cmd = self.git();
        cmd = cmd.arg("commit").arg("-m").arg(message);
        if sign_off {
            cmd = cmd.arg("--signoff");
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
        let head = self.head_hash()?;
        if hash == head {
            self.git()
                .args(&["commit", "--amend", "-m", message])
                .run_expecting_success()?;
        } else {
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
        cmd = cmd.arg("cherry-pick").arg("--allow-empty");
        for hash in hashes {
            cmd = cmd.arg(hash.as_str());
        }
        cmd.run_expecting_success()?;
        Ok(())
    }

    pub fn commit_message_full(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["log", "-1", "--format=%B", hash])
            .run_expecting_success()?;
        Ok(result.stdout.trim().to_string())
    }

    pub fn commit_message_body(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["log", "-1", "--format=%b", hash])
            .run_expecting_success()?;
        Ok(result.stdout.trim().to_string())
    }

    pub fn commit_diff(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["diff", &format!("{}^..{}", hash, hash)])
            .run_expecting_success()?;
        Ok(result.stdout)
    }

    pub fn reset_to_commit(&self, hash: &str, mode: &str) -> Result<()> {
        self.git()
            .args(&["reset", mode, hash])
            .run_expecting_success()?;
        Ok(())
    }

    /// Fetch the shortstat summary for a commit:
    /// "N files changed, A insertions(+), D deletions(-)".
    /// Uses `git log -1 --shortstat` because `git show --shortstat` dumps the
    /// full patch first (slow on large commits), while `git show --shortstat
    /// --no-patch` suppresses the stat entirely.  `git log -1` gives us the
    /// stat without the patch.
    pub fn commit_stat(&self, hash: &str) -> Result<CommitStat> {
        let result = self
            .git()
            .args(&["log", "-1", "--shortstat", "--format=", hash])
            .run()?;
        if !result.success {
            return Ok(CommitStat::default());
        }
        Ok(parse_shortstat(&result.stdout))
    }
}

fn parse_shortstat(output: &str) -> CommitStat {
    let line = output
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or("");
    let mut stat = CommitStat::default();
    for segment in line.split(',') {
        let segment = segment.trim();
        let num_str: String = segment.chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(n) = num_str.parse::<usize>() else { continue };
        if segment.contains("file") {
            stat.files_changed = n;
        } else if segment.contains("insertion") {
            stat.insertions = n;
        } else if segment.contains("deletion") {
            stat.deletions = n;
        }
    }
    stat
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

/// Extract ref decorations like "HEAD -> main", "origin/main", "origin/feature".
/// Excludes tags (handled separately).
fn extract_refs(decoration: &str) -> Vec<String> {
    if decoration.is_empty() {
        return Vec::new();
    }
    decoration
        .split(", ")
        .filter_map(|d| {
            let d = d.trim();
            if d.is_empty() || d.starts_with("tag: ") {
                None
            } else {
                Some(d.to_string())
            }
        })
        .collect()
}
