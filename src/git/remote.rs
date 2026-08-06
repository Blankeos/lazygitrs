use anyhow::Result;

use super::GitCommands;
use crate::model::{Remote, RemoteBranch};
use crate::os::cmd::{CmdBuilder, CmdResult};

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubRemoteRepo {
    remote_name: String,
    repo: String,
    owner: String,
}

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
        // --no-write-fetch-head: still updates remote-tracking refs, but avoids
        // racing a concurrent `git pull` on .git/FETCH_HEAD (can surface as a
        // spurious "divergent branches" / merge failure on the first p).
        self.git()
            .args(&["fetch", "--no-write-fetch-head", remote])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn fetch_all(&self) -> Result<()> {
        self.git()
            .args(&["fetch", "--all", "--no-write-fetch-head"])
            .run_expecting_success()?;
        Ok(())
    }

    /// Non-interactive fetch for the periodic auto-fetch loop. Suppresses any
    /// terminal/credential prompts so a missing SSH passphrase or stored
    /// credential can't hang the TUI.
    pub fn fetch_all_background(&self) -> Result<bool> {
        let result = self
            .git()
            .args(&["fetch", "--all", "--no-write-fetch-head"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .run_expecting_success()?;
        Ok(!result.stdout.trim().is_empty() || !result.stderr.trim().is_empty())
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
        if let Some(url) = self.gh_pr_view_url(branch, None)? {
            return Ok(url);
        }

        let preferred_remote = self.branch_upstream_remote(branch);
        let repos = order_github_remote_repos(
            self.load_github_remote_repos()?,
            preferred_remote.as_deref(),
        );
        let head_selectors = github_pr_head_selectors(branch, &repos);

        for repo in &repos {
            if let Some(url) = self.gh_pr_view_url(branch, Some(&repo.repo))? {
                return Ok(url);
            }

            for head in &head_selectors {
                if let Some(url) = self.gh_pr_list_url(&repo.repo, head)? {
                    return Ok(url);
                }
            }
        }

        anyhow::bail!("No PR found for branch '{}'", branch)
    }

    fn gh_pr_view_url(&self, branch: &str, repo: Option<&str>) -> Result<Option<String>> {
        let mut cmd = CmdBuilder::new("gh")
            .args(&["pr", "view", branch, "--json", "url", "-q", ".url"])
            .cwd_path(self.repo_path());
        if let Some(repo) = repo {
            cmd = cmd.args(&["--repo", repo]);
        }

        Ok(gh_url_from_result(&cmd.run()?))
    }

    fn gh_pr_list_url(&self, repo: &str, head: &str) -> Result<Option<String>> {
        let result = CmdBuilder::new("gh")
            .args(&[
                "pr", "list", "--repo", repo, "--head", head, "--json", "url", "--limit", "1",
                "-q", ".[0].url",
            ])
            .cwd_path(self.repo_path())
            .run()?;

        Ok(gh_url_from_result(&result))
    }

    fn load_github_remote_repos(&self) -> Result<Vec<GithubRemoteRepo>> {
        let result = self.git().args(&["remote", "-v"]).run()?;
        if !result.success {
            return Ok(Vec::new());
        }

        Ok(github_remote_repos_from_remote_v_output(&result.stdout))
    }

    fn branch_upstream_remote(&self, branch: &str) -> Option<String> {
        let ref_name = format!("refs/heads/{}", branch);
        let result = self
            .git()
            .args(&["for-each-ref", "--format=%(upstream:short)", &ref_name])
            .run()
            .ok()?;
        if !result.success {
            return None;
        }

        result
            .stdout
            .trim()
            .split_once('/')
            .map(|(remote, _)| remote.to_string())
            .filter(|remote| !remote.is_empty())
    }
}

impl GitCommands {
    /// Get the origin repo HTTPS URL, returning an empty string on failure.
    pub fn load_repo_url(&self) -> String {
        self.get_repo_url().unwrap_or_default()
    }

    /// Load top contributors using `git shortlog -sn` capped to recent commits
    /// to avoid traversing the entire history on large repos.
    pub fn load_contributors(&self, max_commits: usize, top_n: usize) -> Vec<(String, usize)> {
        let max_arg = format!("--max-count={}", max_commits);
        let result = self
            .git()
            .args(&["shortlog", "-sn", "-e", "HEAD", &max_arg])
            .env("GIT_PAGER", "cat")
            .run();
        let Ok(result) = result else {
            return Vec::new();
        };
        if !result.success {
            return Vec::new();
        }

        let mut out: Vec<(String, usize)> = Vec::new();
        for line in result.stdout.lines() {
            let trimmed = line.trim_start();
            let mut parts = trimmed.splitn(2, char::is_whitespace);
            let count_str = parts.next().unwrap_or("");
            let rest = parts.next().unwrap_or("").trim();
            let Ok(count) = count_str.parse::<usize>() else {
                continue;
            };
            // Strip "<email>" suffix if present.
            let name = match rest.rfind('<') {
                Some(i) => rest[..i].trim().to_string(),
                None => rest.to_string(),
            };
            if name.is_empty() {
                continue;
            }
            out.push((name, count));
            if out.len() >= top_n {
                break;
            }
        }
        out
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

fn gh_url_from_result(result: &CmdResult) -> Option<String> {
    let url = result.stdout.trim();
    if result.success && !url.is_empty() && url != "null" {
        Some(url.to_string())
    } else {
        None
    }
}

fn github_remote_repos_from_remote_v_output(output: &str) -> Vec<GithubRemoteRepo> {
    let mut repos = Vec::new();

    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let Some(remote_name) = parts.next() else {
            continue;
        };
        let Some(url) = parts.next() else {
            continue;
        };
        let Some(repo) = github_repo_slug_from_remote_url(url) else {
            continue;
        };
        let Some(owner) = repo.split('/').next().map(|owner| owner.to_string()) else {
            continue;
        };

        if repos
            .iter()
            .any(|existing: &GithubRemoteRepo| existing.repo == repo)
        {
            continue;
        }

        repos.push(GithubRemoteRepo {
            remote_name: remote_name.to_string(),
            repo,
            owner,
        });
    }

    repos
}

fn github_repo_slug_from_remote_url(url: &str) -> Option<String> {
    let path = github_path_from_remote_url(url)?;
    let mut parts = path.trim_matches('/').split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim().trim_end_matches(".git");

    if owner.is_empty() || repo.is_empty() {
        return None;
    }

    Some(format!("{}/{}", owner, repo))
}

fn github_path_from_remote_url(url: &str) -> Option<&str> {
    let url = url.trim().trim_end_matches('/');

    if let Some(path) = url.strip_prefix("git@github.com:") {
        return Some(path);
    }

    for scheme in ["https://", "http://", "ssh://"] {
        let Some(rest) = url.strip_prefix(scheme) else {
            continue;
        };
        let Some((authority, path)) = rest.split_once('/') else {
            continue;
        };
        let host = authority.rsplit('@').next().unwrap_or(authority);
        if host == "github.com" {
            return Some(path);
        }
    }

    None
}

fn order_github_remote_repos(
    mut repos: Vec<GithubRemoteRepo>,
    preferred_remote: Option<&str>,
) -> Vec<GithubRemoteRepo> {
    let mut ordered = Vec::new();

    if let Some(preferred_remote) = preferred_remote {
        move_repos_for_remote(&mut repos, &mut ordered, preferred_remote);
    }
    move_repos_for_remote(&mut repos, &mut ordered, "upstream");
    move_repos_for_remote(&mut repos, &mut ordered, "origin");
    ordered.extend(repos);

    ordered
}

fn move_repos_for_remote(
    repos: &mut Vec<GithubRemoteRepo>,
    ordered: &mut Vec<GithubRemoteRepo>,
    remote_name: &str,
) {
    let mut i = 0;
    while i < repos.len() {
        if repos[i].remote_name == remote_name {
            ordered.push(repos.remove(i));
        } else {
            i += 1;
        }
    }
}

fn github_pr_head_selectors(branch: &str, repos: &[GithubRemoteRepo]) -> Vec<String> {
    let mut selectors = Vec::from([branch.to_string()]);

    for repo in repos {
        let selector = format!("{}:{}", repo.owner, branch);
        if !selectors.contains(&selector) {
            selectors.push(selector);
        }
    }

    selectors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_repo_slugs_from_common_remote_urls() {
        assert_eq!(
            github_repo_slug_from_remote_url("https://github.com/Blankeos/zed.git"),
            Some("Blankeos/zed".to_string())
        );
        assert_eq!(
            github_repo_slug_from_remote_url("git@github.com:zed-industries/zed.git"),
            Some("zed-industries/zed".to_string())
        );
        assert_eq!(
            github_repo_slug_from_remote_url("ssh://git@github.com/zed-industries/zed.git"),
            Some("zed-industries/zed".to_string())
        );
        assert_eq!(
            github_repo_slug_from_remote_url("https://token@github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            github_repo_slug_from_remote_url("https://gitlab.com/owner/repo.git"),
            None
        );
    }

    #[test]
    fn loads_unique_github_remote_repos_from_remote_v_output() {
        let output = "\
origin\thttps://github.com/Blankeos/zed.git (fetch)
origin\thttps://github.com/Blankeos/zed.git (push)
upstream\thttps://github.com/zed-industries/zed.git (fetch)
upstream\thttps://github.com/zed-industries/zed.git (push)
gitlab\thttps://gitlab.com/example/zed.git (fetch)
";

        let repos = github_remote_repos_from_remote_v_output(output);

        assert_eq!(
            repos,
            vec![
                GithubRemoteRepo {
                    remote_name: "origin".to_string(),
                    repo: "Blankeos/zed".to_string(),
                    owner: "Blankeos".to_string(),
                },
                GithubRemoteRepo {
                    remote_name: "upstream".to_string(),
                    repo: "zed-industries/zed".to_string(),
                    owner: "zed-industries".to_string(),
                },
            ]
        );
    }

    #[test]
    fn orders_repos_by_preferred_remote_then_common_fallbacks() {
        let repos = vec![
            GithubRemoteRepo {
                remote_name: "origin".to_string(),
                repo: "me/project".to_string(),
                owner: "me".to_string(),
            },
            GithubRemoteRepo {
                remote_name: "mirror".to_string(),
                repo: "mirror/project".to_string(),
                owner: "mirror".to_string(),
            },
            GithubRemoteRepo {
                remote_name: "upstream".to_string(),
                repo: "org/project".to_string(),
                owner: "org".to_string(),
            },
        ];

        let ordered = order_github_remote_repos(repos, Some("origin"));
        let names: Vec<_> = ordered
            .iter()
            .map(|repo| repo.remote_name.as_str())
            .collect();

        assert_eq!(names, vec!["origin", "upstream", "mirror"]);
    }

    #[test]
    fn builds_head_selectors_from_remote_owners() {
        let repos = vec![
            GithubRemoteRepo {
                remote_name: "upstream".to_string(),
                repo: "zed-industries/zed".to_string(),
                owner: "zed-industries".to_string(),
            },
            GithubRemoteRepo {
                remote_name: "origin".to_string(),
                repo: "Blankeos/zed".to_string(),
                owner: "Blankeos".to_string(),
            },
        ];

        assert_eq!(
            github_pr_head_selectors("feature/foo", &repos),
            vec![
                "feature/foo".to_string(),
                "zed-industries:feature/foo".to_string(),
                "Blankeos:feature/foo".to_string(),
            ]
        );
    }
}
