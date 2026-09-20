use std::collections::HashSet;

use anyhow::Result;

use super::GitCommands;
use crate::model::Tag;

/// How `load_tags` should determine which tags exist on a remote.
///
/// `Query` probes every configured remote with `git ls-remote --tags`
/// (network I/O, bounded per remote). `Cached` reuses a set captured by an
/// earlier query — used by periodic refreshes so an offline remote can't
/// stall the model stream.
pub enum RemoteTagMode {
    Query,
    Cached(HashSet<String>),
}

impl GitCommands {
    pub fn load_tags(&self, mode: &RemoteTagMode) -> Result<Vec<Tag>> {
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

        let queried;
        let remote_tags = match mode {
            RemoteTagMode::Query => {
                queried = self.remote_tag_names();
                &queried
            }
            RemoteTagMode::Cached(set) => set,
        };

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

    /// `git ls-remote --tags <remote>` with a hard wall-clock timeout.
    ///
    /// The child is spawned directly (not via CmdBuilder) so we can poll
    /// `try_wait` and `kill` on timeout — a detached thread + `wait_with_output`
    /// would leak the process past the deadline. A reader thread drains stdout
    /// so a tag list larger than the pipe buffer can't stall the child.
    fn ls_remote_tags(&self, remote: &str) -> Option<String> {
        const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
        const POLL: std::time::Duration = std::time::Duration::from_millis(25);

        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(self.repo_path())
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args(["ls-remote", "--tags", remote]);
        let mut child = crate::os::cmd::spawn_non_interactive(&mut cmd).ok()?;

        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
            let _ = tx.send(buf);
        });

        let deadline = std::time::Instant::now() + TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        return None;
                    }
                    let buf = rx.recv().unwrap_or_default();
                    return Some(String::from_utf8_lossy(&buf).into_owned());
                }
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(POLL);
                }
                _ => {
                    // Timeout or wait error: kill and reap so no git/ssh child
                    // is left running past the budget.
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
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
