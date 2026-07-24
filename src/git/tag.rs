use std::collections::HashSet;

use anyhow::Result;

use super::GitCommands;
use crate::model::Tag;

impl GitCommands {
    pub fn load_tags(&self) -> Result<Vec<Tag>> {
        // Peel annotated tags to the commit they point at.
        let format = "%(refname:short)|%(if)%(*objectname)%(then)%(*objectname:short)%(else)%(objectname:short)%(end)|%(subject)";
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

        let remote_tags = self.remote_tag_names();

        let tags = result
            .stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, '|').collect();
                if parts.len() >= 2 {
                    let name = parts[0].to_string();
                    let on_remote = remote_tags.contains(name.as_str());
                    Some(Tag {
                        name,
                        hash: parts[1].to_string(),
                        message: parts.get(2).unwrap_or(&"").to_string(),
                        on_remote,
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(tags)
    }

    /// Tag names known to exist on any configured remote (best-effort via ls-remote).
    ///
    /// Each remote is queried with a short wall-clock budget so an offline
    /// remote cannot block tag loading indefinitely.
    fn remote_tag_names(&self) -> HashSet<String> {
        let remotes = match self.git().args(&["remote"]).run() {
            Ok(r) if r.success => r
                .stdout
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>(),
            _ => return HashSet::new(),
        };

        let mut names = HashSet::new();
        for remote in remotes {
            let Some(stdout) = self.ls_remote_tags(&remote) else {
                continue;
            };
            for line in stdout.lines() {
                // "<hash>\trefs/tags/<name>" or "...\trefs/tags/<name>^{}"
                let Some(refname) = line.split_whitespace().nth(1) else {
                    continue;
                };
                let Some(name) = refname.strip_prefix("refs/tags/") else {
                    continue;
                };
                let name = name.strip_suffix("^{}").unwrap_or(name);
                if !name.is_empty() {
                    names.insert(name.to_string());
                }
            }
        }
        names
    }

    fn ls_remote_tags(&self, remote: &str) -> Option<String> {
        const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
        let repo_path = self.repo_path.clone();
        let remote = remote.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::os::cmd::CmdBuilder::git_no_optional_locks()
                .cwd_path(&repo_path)
                .env("GIT_TERMINAL_PROMPT", "0")
                .args(&["ls-remote", "--tags", &remote])
                .run();
            let _ = tx.send(result);
        });
        match rx.recv_timeout(TIMEOUT) {
            Ok(Ok(result)) if result.success => Some(result.stdout),
            _ => None,
        }
    }

    pub fn create_tag(&self, name: &str, message: &str) -> Result<()> {
        if message.is_empty() {
            self.git().args(&["tag", name]).run_expecting_success()?;
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

    pub fn delete_remote_tag(&self, remote: &str, name: &str) -> Result<()> {
        let refspec = format!("refs/tags/{}", name);
        self.git()
            .args(&["push", remote, "--delete", &refspec])
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
