use anyhow::Result;

use super::GitCommands;
use crate::model::Worktree;

impl GitCommands {
    pub fn load_worktrees(&self) -> Result<Vec<Worktree>> {
        let result = self
            .git()
            .args(&["worktree", "list", "--porcelain"])
            .run()?;
        if !result.success {
            return Ok(Vec::new());
        }

        let mut worktrees = Vec::new();
        let mut path = String::new();
        let mut branch = String::new();
        let mut hash = String::new();
        let mut is_bare = false;

        for line in result.stdout.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                if !path.is_empty() && !is_bare {
                    worktrees.push(Worktree {
                        path: path.clone(),
                        branch: branch.clone(),
                        hash: hash.clone(),
                        is_current: false,
                        is_main: worktrees.is_empty(),
                    });
                }
                path = p.to_string();
                branch.clear();
                hash.clear();
                is_bare = false;
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                hash = h.to_string();
            } else if let Some(b) = line.strip_prefix("branch ") {
                branch = b.strip_prefix("refs/heads/").unwrap_or(b).to_string();
            } else if line == "bare" {
                is_bare = true;
            } else if line == "detached" {
                branch = "(detached)".to_string();
            }
        }

        // Push the last one
        if !path.is_empty() && !is_bare {
            worktrees.push(Worktree {
                path: path.clone(),
                branch: branch.clone(),
                hash: hash.clone(),
                is_current: false,
                is_main: worktrees.is_empty(),
            });
        }

        // Mark the current worktree
        let repo_path = self.repo_path().to_string_lossy().to_string();
        for wt in &mut worktrees {
            if wt.path == repo_path {
                wt.is_current = true;
            }
        }

        Ok(worktrees)
    }

    pub fn resolve_worktree_sibling_path(&self, branch_name: &str) -> std::path::PathBuf {
        let clean_branch = branch_name.trim().trim_matches('/');
        let folder_name = clean_branch.replace('/', "-");
        if let Some(parent) = self.repo_path.parent() {
            parent.join(folder_name)
        } else {
            self.repo_path.join(folder_name)
        }
    }

    pub fn add_worktree(
        &self,
        branch: &str,
        base: Option<&str>,
        path_override: Option<&std::path::Path>,
    ) -> Result<std::path::PathBuf> {
        let clean_branch = branch.trim().trim_matches('/');
        if clean_branch.is_empty() {
            return Err(anyhow::anyhow!("Branch name cannot be empty"));
        }

        let target_path = path_override
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| self.resolve_worktree_sibling_path(clean_branch));

        if target_path.exists() {
            return Err(anyhow::anyhow!(
                "Target path '{}' already exists",
                target_path.display()
            ));
        }

        let path_str = target_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid target path unicode"))?;

        // Check if the branch exists locally
        let branch_exists = self
            .git()
            .args(&[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{}", clean_branch),
            ])
            .run()
            .map(|r| r.success)
            .unwrap_or(false);

        if branch_exists {
            self.git()
                .args(&["worktree", "add", path_str, clean_branch])
                .run_expecting_success()?;
        } else {
            let base_ref = base.unwrap_or("HEAD");
            self.git()
                .args(&["worktree", "add", "-b", clean_branch, path_str, base_ref])
                .run_expecting_success()?;

            // Set branch.<name>.base configuration for AWT interoperability
            let _ = self
                .git()
                .cwd_path(&target_path)
                .args(&["config", &format!("branch.{}.base", clean_branch), base_ref])
                .run();
        }

        // Propagate .env or .env.example from sibling main worktree if available
        if let Some(parent) = target_path.parent() {
            let main_env = parent.join("main").join(".env");
            let target_env = target_path.join(".env");
            if main_env.is_file() && !target_env.exists() {
                let _ = std::fs::copy(&main_env, &target_env);
            } else {
                let main_env_example = parent.join("main").join(".env.example");
                if main_env_example.is_file() && !target_env.exists() {
                    let _ = std::fs::copy(&main_env_example, &target_env);
                }
            }
        }

        // Run user lifecycle post-create hook if present (~/.config/lazygitrs/hooks/post-create.sh or ~/.config/matchmaker/hooks/post-create.sh)
        if let Some(home) = std::env::var_os("HOME") {
            let home_path = std::path::PathBuf::from(home);
            for hook in [
                home_path.join(".config/lazygitrs/hooks/post-create.sh"),
                home_path.join(".config/matchmaker/hooks/post-create.sh"),
            ] {
                if hook.is_file() {
                    let _ = std::process::Command::new(&hook)
                        .arg(&target_path)
                        .arg(clean_branch)
                        .arg(base.unwrap_or("main"))
                        .spawn();
                    break;
                }
            }
        }

        Ok(target_path)
    }

    pub fn create_worktree(&self, path: &str, branch: &str) -> Result<()> {
        self.git()
            .args(&["worktree", "add", path, branch])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn create_worktree_new_branch(&self, path: &str, new_branch: &str) -> Result<()> {
        self.git()
            .args(&["worktree", "add", "-b", new_branch, path])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn remove_worktree(&self, path: &str, force: bool) -> Result<()> {
        let mut cmd = self.git();
        cmd = cmd.args(&["worktree", "remove", path]);
        if force {
            cmd = cmd.arg("--force");
        }
        cmd.run_expecting_success()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_resolve_worktree_sibling_path() {
        let dummy = GitCommands {
            repo_path: PathBuf::from("/home/user/dev/project/fix-worktree"),
            repo_name: "project".to_string(),
        };

        let sibling = dummy.resolve_worktree_sibling_path("feat/login-page");
        assert_eq!(
            sibling,
            PathBuf::from("/home/user/dev/project/feat-login-page")
        );

        let nested = dummy.resolve_worktree_sibling_path("fix/ui/buttons");
        assert_eq!(
            nested,
            PathBuf::from("/home/user/dev/project/fix-ui-buttons")
        );
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "lazygitrs-{prefix}-{unique}-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_add_worktree_sibling_creation() {
        use std::process::Command;

        let temp = TempDir::new("add-wt");
        let main_repo = temp.path().join("main-worktree");
        std::fs::create_dir_all(&main_repo).expect("create repo dir");

        assert!(
            Command::new("git")
                .args(["init", "-b", "main"])
                .arg(&main_repo)
                .status()
                .expect("init git")
                .success()
        );
        assert!(
            Command::new("git")
                .args(["config", "user.email", "test@example.com"])
                .current_dir(&main_repo)
                .status()
                .expect("config email")
                .success()
        );
        assert!(
            Command::new("git")
                .args(["config", "user.name", "Test"])
                .current_dir(&main_repo)
                .status()
                .expect("config name")
                .success()
        );
        std::fs::write(main_repo.join("file.txt"), "hello").expect("write file");
        assert!(
            Command::new("git")
                .args(["add", "file.txt"])
                .current_dir(&main_repo)
                .status()
                .expect("git add")
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-m", "initial"])
                .current_dir(&main_repo)
                .status()
                .expect("git commit")
                .success()
        );

        let git = GitCommands::new(&main_repo).expect("create GitCommands");
        let created = git
            .add_worktree("feat/oauth-login", Some("main"), None)
            .expect("add worktree");

        let expected_path = temp.path().join("feat-oauth-login");
        assert_eq!(created, expected_path);
        assert!(created.is_dir());
        assert!(created.join("file.txt").is_file());

        // Verify base config
        let base_config = git
            .git()
            .cwd_path(&created)
            .args(&["config", "branch.feat/oauth-login.base"])
            .run()
            .expect("run git config")
            .stdout_trimmed()
            .to_string();
        assert_eq!(base_config, "main");
    }
}
