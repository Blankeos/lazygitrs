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
}
