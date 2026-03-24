use anyhow::Result;

use super::GitCommands;
use crate::model::Tag;

impl GitCommands {
    pub fn load_tags(&self) -> Result<Vec<Tag>> {
        let format = "%(refname:short)|%(objectname:short)|%(subject)";
        let result = self
            .git()
            .args(&[
                "for-each-ref",
                "--sort=-creatordate",
                &format!("--format={}", format),
                "refs/tags/",
            ])
            .run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        let tags = result
            .stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, '|').collect();
                if parts.len() >= 2 {
                    Some(Tag {
                        name: parts[0].to_string(),
                        hash: parts[1].to_string(),
                        message: parts.get(2).unwrap_or(&"").to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(tags)
    }

    pub fn create_tag(&self, name: &str, message: &str) -> Result<()> {
        if message.is_empty() {
            self.git()
                .args(&["tag", name])
                .run_expecting_success()?;
        } else {
            self.git()
                .args(&["tag", "-a", name, "-m", message])
                .run_expecting_success()?;
        }
        Ok(())
    }

    pub fn delete_tag(&self, name: &str) -> Result<()> {
        self.git()
            .args(&["tag", "-d", name])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn push_tag(&self, name: &str) -> Result<()> {
        self.git()
            .args(&["push", "origin", name])
            .run_expecting_success()?;
        Ok(())
    }
}
