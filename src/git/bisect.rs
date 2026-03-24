use anyhow::Result;

use super::GitCommands;

#[derive(Debug, Clone)]
pub struct BisectInfo {
    pub started: bool,
    pub current: String,
    pub good: Vec<String>,
    pub bad: Vec<String>,
    pub remaining: Option<usize>,
}

impl GitCommands {
    /// Start a bisect session.
    pub fn bisect_start(&self) -> Result<()> {
        self.git()
            .args(&["bisect", "start"])
            .run_expecting_success()?;
        Ok(())
    }

    /// Mark the current (or given) commit as good.
    pub fn bisect_good(&self, hash: &str) -> Result<String> {
        let result = if hash.is_empty() {
            self.git().args(&["bisect", "good"]).run_expecting_success()?
        } else {
            self.git()
                .args(&["bisect", "good", hash])
                .run_expecting_success()?
        };
        Ok(result.stdout)
    }

    /// Mark the current (or given) commit as bad.
    pub fn bisect_bad(&self, hash: &str) -> Result<String> {
        let result = if hash.is_empty() {
            self.git().args(&["bisect", "bad"]).run_expecting_success()?
        } else {
            self.git()
                .args(&["bisect", "bad", hash])
                .run_expecting_success()?
        };
        Ok(result.stdout)
    }

    /// Skip the current commit during bisect.
    pub fn bisect_skip(&self, hash: &str) -> Result<String> {
        let result = if hash.is_empty() {
            self.git()
                .args(&["bisect", "skip"])
                .run_expecting_success()?
        } else {
            self.git()
                .args(&["bisect", "skip", hash])
                .run_expecting_success()?
        };
        Ok(result.stdout)
    }

    /// End the bisect session.
    pub fn bisect_reset(&self) -> Result<()> {
        self.git()
            .args(&["bisect", "reset"])
            .run_expecting_success()?;
        Ok(())
    }

    /// Get bisect log output.
    pub fn bisect_log(&self) -> Result<String> {
        let result = self.git().args(&["bisect", "log"]).run()?;
        if result.success {
            Ok(result.stdout)
        } else {
            Ok(String::new())
        }
    }

    /// Check if currently in a bisect session.
    pub fn is_bisecting(&self) -> bool {
        self.repo_path().join(".git/BISECT_LOG").exists()
    }

    /// Parse bisect info from the log.
    pub fn bisect_info(&self) -> Result<BisectInfo> {
        let started = self.is_bisecting();
        if !started {
            return Ok(BisectInfo {
                started: false,
                current: String::new(),
                good: Vec::new(),
                bad: Vec::new(),
                remaining: None,
            });
        }

        let log = self.bisect_log()?;
        let mut good = Vec::new();
        let mut bad = Vec::new();

        for line in log.lines() {
            if line.starts_with("# good: ") {
                if let Some(hash) = line.strip_prefix("# good: ") {
                    good.push(hash.trim().to_string());
                }
            } else if line.starts_with("# bad: ") {
                if let Some(hash) = line.strip_prefix("# bad: ") {
                    bad.push(hash.trim().to_string());
                }
            }
        }

        let current = self.head_hash().unwrap_or_default();

        Ok(BisectInfo {
            started,
            current,
            good,
            bad,
            remaining: None,
        })
    }
}
