use anyhow::Result;

use super::GitCommands;
use crate::model::{Remote, RemoteBranch};

impl GitCommands {
    pub fn load_remotes(&self) -> Result<Vec<Remote>> {
        let result = self.git().args(&["remote", "-v"]).run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        let mut remotes: Vec<Remote> = Vec::new();
        for line in result.stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let name = parts[0].to_string();
            let url = parts[1].to_string();

            if let Some(existing) = remotes.iter_mut().find(|r| r.name == name) {
                if !existing.urls.contains(&url) {
                    existing.urls.push(url);
                }
            } else {
                remotes.push(Remote {
                    name,
                    urls: vec![url],
                    branches: Vec::new(),
                });
            }
        }

        // Load remote branches
        for remote in &mut remotes {
            remote.branches = self.load_remote_branches(&remote.name)?;
        }

        Ok(remotes)
    }

    fn load_remote_branches(&self, remote_name: &str) -> Result<Vec<RemoteBranch>> {
        let format = "%(refname:short)|%(objectname:short)";
        let result = self
            .git()
            .args(&[
                "for-each-ref",
                &format!("--format={}", format),
                &format!("refs/remotes/{}/", remote_name),
            ])
            .run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        let branches = result
            .stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(2, '|').collect();
                if parts.len() >= 2 {
                    let full_name = parts[0];
                    let branch_name = full_name
                        .strip_prefix(&format!("{}/", remote_name))
                        .unwrap_or(full_name);
                    // Filter out HEAD (explicit or symref that resolves to just the remote name)
                    if branch_name == "HEAD" || branch_name == remote_name {
                        return None;
                    }
                    Some(RemoteBranch {
                        name: branch_name.to_string(),
                        remote_name: remote_name.to_string(),
                        hash: parts[1].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(branches)
    }

    pub fn add_remote(&self, name: &str, url: &str) -> Result<()> {
        self.git()
            .args(&["remote", "add", name, url])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn delete_remote(&self, name: &str) -> Result<()> {
        self.git()
            .args(&["remote", "remove", name])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn fetch(&self, remote: &str) -> Result<()> {
        self.git()
            .args(&["fetch", remote])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn fetch_all(&self) -> Result<()> {
        self.git()
            .args(&["fetch", "--all"])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn pull(&self) -> Result<()> {
        self.git().args(&["pull"]).run_expecting_success()?;
        Ok(())
    }

    pub fn push(&self, force: bool) -> Result<()> {
        let mut cmd = self.git();
        cmd = cmd.arg("push");
        if force {
            cmd = cmd.arg("--force-with-lease");
        }
        cmd.run_expecting_success()?;
        Ok(())
    }

    pub fn checkout_remote_branch(&self, remote: &str, branch: &str) -> Result<()> {
        // Creates a local branch tracking the remote branch and checks it out
        self.git()
            .args(&["checkout", "-b", branch, &format!("{}/{}", remote, branch)])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn delete_remote_branch(&self, remote: &str, branch: &str) -> Result<()> {
        self.git()
            .args(&["push", remote, "--delete", branch])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn merge_remote_branch(&self, remote: &str, branch: &str, args: &str) -> Result<()> {
        let ref_name = format!("{}/{}", remote, branch);
        let mut cmd = self.git();
        cmd = cmd.arg("merge").arg(&ref_name);
        if !args.is_empty() {
            for arg in args.split_whitespace() {
                cmd = cmd.arg(arg);
            }
        }
        cmd.run_expecting_success()?;
        Ok(())
    }

    pub fn rebase_remote_branch(&self, remote: &str, branch: &str) -> Result<()> {
        self.git()
            .args(&["rebase", &format!("{}/{}", remote, branch)])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn push_with_upstream(&self, remote: &str, branch: &str) -> Result<()> {
        self.git()
            .args(&["push", "-u", remote, branch])
            .run_expecting_success()?;
        Ok(())
    }

    /// Build a web URL for a commit from the origin remote URL.
    pub fn get_commit_url(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["remote", "get-url", "origin"])
            .run_expecting_success()?;
        let remote_url = result.stdout.trim().to_string();
        let base = remote_url_to_https(&remote_url);
        Ok(format!("{}/commit/{}", base, hash))
    }

    /// Get the HTTPS URL for the origin remote repository.
    pub fn get_repo_url(&self) -> Result<String> {
        let result = self
            .git()
            .args(&["remote", "get-url", "origin"])
            .run_expecting_success()?;
        let remote_url = result.stdout.trim().to_string();
        Ok(remote_url_to_https(&remote_url))
    }

    /// Build a PR creation URL for a branch (GitHub compare URL).
    pub fn get_pr_create_url(&self, branch: &str) -> Result<String> {
        let base = self.get_repo_url()?;
        Ok(format!("{}/compare/{}?expand=1", base, branch))
    }

    /// Get the PR URL for a branch using `gh pr view`.
    pub fn get_pr_url(&self, branch: &str) -> Result<String> {
        let result = crate::os::cmd::CmdBuilder::new("gh")
            .args(&["pr", "view", branch, "--json", "url", "-q", ".url"])
            .cwd_path(self.repo_path())
            .run()?;
        if result.success && !result.stdout.trim().is_empty() {
            Ok(result.stdout.trim().to_string())
        } else {
            anyhow::bail!("No PR found for branch '{}'", branch)
        }
    }
}

/// Convert a git remote URL (SSH or HTTPS) to a plain HTTPS base URL.
fn remote_url_to_https(url: &str) -> String {
    let mut u = url.to_string();
    // git@github.com:user/repo.git -> https://github.com/user/repo
    if u.starts_with("git@") {
        u = u.replacen("git@", "https://", 1);
        u = u.replacen(':', "/", 1);
    }
    // Strip .git suffix
    if u.ends_with(".git") {
        u.truncate(u.len() - 4);
    }
    u.trim_end_matches('/').to_string()
}
