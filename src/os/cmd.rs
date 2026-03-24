use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result};

#[derive(Debug)]
pub struct CmdResult {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub exit_code: Option<i32>,
}

impl CmdResult {
    pub fn from_output(output: Output) -> Self {
        Self {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
            exit_code: output.status.code(),
        }
    }

    pub fn stdout_trimmed(&self) -> &str {
        self.stdout.trim()
    }

    pub fn lines(&self) -> Vec<&str> {
        self.stdout.lines().collect()
    }
}

pub struct CmdBuilder {
    program: String,
    args: Vec<String>,
    cwd: Option<String>,
    env_vars: Vec<(String, String)>,
    stdin_data: Option<String>,
}

impl CmdBuilder {
    pub fn new(program: &str) -> Self {
        Self {
            program: program.to_string(),
            args: Vec::new(),
            cwd: None,
            env_vars: Vec::new(),
            stdin_data: None,
        }
    }

    pub fn git() -> Self {
        Self::new("git")
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }

    pub fn args(mut self, args: &[&str]) -> Self {
        self.args.extend(args.iter().map(|s| s.to_string()));
        self
    }

    pub fn cwd(mut self, dir: &str) -> Self {
        self.cwd = Some(dir.to_string());
        self
    }

    pub fn cwd_path(mut self, dir: &Path) -> Self {
        self.cwd = Some(dir.to_string_lossy().to_string());
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env_vars.push((key.to_string(), value.to_string()));
        self
    }

    pub fn stdin(mut self, data: String) -> Self {
        self.stdin_data = Some(data);
        self
    }

    pub fn run(&self) -> Result<CmdResult> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);

        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd);
        }

        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        if self.stdin_data.is_some() {
            cmd.stdin(Stdio::piped());
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = if let Some(ref stdin_data) = self.stdin_data {
            let mut child = cmd.spawn().with_context(|| {
                format!("Failed to spawn: {} {:?}", self.program, self.args)
            })?;

            if let Some(ref mut stdin) = child.stdin {
                use std::io::Write;
                stdin.write_all(stdin_data.as_bytes())?;
            }

            child.wait_with_output().with_context(|| {
                format!("Failed to wait: {} {:?}", self.program, self.args)
            })?
        } else {
            cmd.output().with_context(|| {
                format!("Failed to run: {} {:?}", self.program, self.args)
            })?
        };

        Ok(CmdResult::from_output(output))
    }

    pub fn run_expecting_success(&self) -> Result<CmdResult> {
        let result = self.run()?;
        if !result.success {
            anyhow::bail!(
                "Command failed (exit {}): {} {}\nstderr: {}",
                result.exit_code.unwrap_or(-1),
                self.program,
                self.args.join(" "),
                result.stderr.trim()
            );
        }
        Ok(result)
    }

    pub fn description(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}
