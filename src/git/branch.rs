use anyhow::Result;

use super::GitCommands;
use crate::model::Branch;

impl GitCommands {
    pub fn load_branches(&self) -> Result<Vec<Branch>> {
        let format = "%(HEAD)|%(refname:short)|%(objectname:short)|%(upstream:short)|%(upstream:track)";
        let result = self
            .git()
            .args(&[
                "for-each-ref",
                "--sort=-committerdate",
                &format!("--format={}", format),
                "refs/heads/",
            ])
            .run_expecting_success()?;

        let mut branches = Vec::new();
        for line in result.stdout.lines() {
            let parts: Vec<&str> = line.splitn(5, '|').collect();
            if parts.len() < 3 {
                continue;
            }

            let head = parts[0] == "*";
            let name = parts[1].to_string();
            let hash = parts[2].to_string();
            let upstream = if parts.len() > 3 && !parts[3].is_empty() {
                Some(parts[3].to_string())
            } else {
                None
            };

            let (pushables, pullables) = if parts.len() > 4 {
                parse_track_info(parts[4])
            } else {
                ("0".to_string(), "0".to_string())
            };

            // Get recency
            let recency = self.branch_recency(&name).unwrap_or_default();

            branches.push(Branch {
                name,
                hash,
                recency,
                pushables,
                pullables,
                upstream,
                head,
            });
        }

        Ok(branches)
    }

    fn branch_recency(&self, branch_name: &str) -> Result<String> {
        let result = self
            .git()
            .args(&[
                "log",
                "-1",
                "--format=%cr",
                branch_name,
                "--",
            ])
            .run()?;

        if result.success {
            Ok(shorten_recency(result.stdout_trimmed()))
        } else {
            Ok(String::new())
        }
    }

    pub fn checkout_branch(&self, name: &str) -> Result<()> {
        self.git()
            .args(&["checkout", name])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn create_branch(&self, name: &str) -> Result<()> {
        self.git()
            .args(&["checkout", "-b", name])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()> {
        let flag = if force { "-D" } else { "-d" };
        self.git()
            .args(&["branch", flag, name])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn rename_branch(&self, old_name: &str, new_name: &str) -> Result<()> {
        self.git()
            .args(&["branch", "-m", old_name, new_name])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn merge_branch(&self, name: &str, args: &str) -> Result<()> {
        let mut cmd = self.git();
        cmd = cmd.arg("merge").arg(name);
        if !args.is_empty() {
            for arg in args.split_whitespace() {
                cmd = cmd.arg(arg);
            }
        }
        cmd.run_expecting_success()?;
        Ok(())
    }

    pub fn rebase_branch(&self, name: &str) -> Result<()> {
        self.git()
            .args(&["rebase", name])
            .run_expecting_success()?;
        Ok(())
    }
}

fn parse_track_info(track: &str) -> (String, String) {
    let mut ahead = 0usize;
    let mut behind = 0usize;

    if let Some(start) = track.find("ahead ") {
        let rest = &track[start + 6..];
        if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
            ahead = rest[..end].parse().unwrap_or(0);
        } else {
            ahead = rest.trim_end_matches(']').parse().unwrap_or(0);
        }
    }
    if let Some(start) = track.find("behind ") {
        let rest = &track[start + 7..];
        if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
            behind = rest[..end].parse().unwrap_or(0);
        } else {
            behind = rest.trim_end_matches(']').parse().unwrap_or(0);
        }
    }

    (ahead.to_string(), behind.to_string())
}

fn shorten_recency(recency: &str) -> String {
    let recency = recency.trim();
    if recency.is_empty() {
        return String::new();
    }

    // "2 hours ago" → "2h", "3 days ago" → "3d", etc.
    let parts: Vec<&str> = recency.split_whitespace().collect();
    if parts.len() >= 2 {
        let num = parts[0];
        let unit = match parts[1] {
            s if s.starts_with("second") => "s",
            s if s.starts_with("minute") => "m",
            s if s.starts_with("hour") => "h",
            s if s.starts_with("day") => "d",
            s if s.starts_with("week") => "w",
            s if s.starts_with("month") => "mo",
            s if s.starts_with("year") => "y",
            _ => return recency.to_string(),
        };
        format!("{}{}", num, unit)
    } else {
        recency.to_string()
    }
}
