use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

/// Detach a child from the controlling terminal and keep it non-interactive.
///
/// Background `git fetch`/`ls-remote` can spawn `ssh`, which opens `/dev/tty`
/// and races the TUI for keystrokes — swallowing `q` and navigation. `setsid`
/// gives the child no controlling terminal so `/dev/tty` fails; null stdin
/// stops anything that reads the pipe we inherit.
fn make_non_interactive(cmd: &mut Command) {
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("SSH_ASKPASS_REQUIRE", "never");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
}

/// Spawn `cmd` detached from the controlling terminal (see
/// `make_non_interactive`) with stdin null and stdout/stderr piped.
///
/// Used by callers that need to poll/kill the child themselves (e.g. the
/// bounded `ls-remote` probe in git::tag) where `CmdBuilder::run`'s blocking
/// `wait_with_output` would not allow a wall-clock timeout.
pub(crate) fn spawn_non_interactive(cmd: &mut Command) -> std::io::Result<std::process::Child> {
    make_non_interactive(cmd);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.spawn()
}

/// Shared command log that CmdBuilder writes to when set.
pub type CommandLog = Arc<Mutex<Vec<String>>>;

/// Create a new shared command log.
pub fn new_command_log() -> CommandLog {
    Arc::new(Mutex::new(Vec::new()))
}

// Thread-local command log reference.
thread_local! {
    static CMD_LOG: std::cell::RefCell<Option<CommandLog>> = const { std::cell::RefCell::new(None) };
}

/// Set the shared command log for this thread.
pub fn set_thread_command_log(log: CommandLog) {
    CMD_LOG.with(|l| *l.borrow_mut() = Some(log));
}

pub fn log_command(desc: &str) {
    CMD_LOG.with(|l| {
        if let Some(ref log) = *l.borrow()
            && let Ok(mut entries) = log.lock()
        {
            entries.push(desc.to_string());
            // Keep last 100 entries
            if entries.len() > 100 {
                let excess = entries.len() - 100;
                entries.drain(..excess);
            }
        }
    });
}

/// True when a command log is installed for this thread. Lets callers skip
/// building a description string when nothing would record it.
fn command_log_enabled() -> bool {
    CMD_LOG.with(|l| l.borrow().is_some())
}

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
            stdout: utf8_or_lossy(output.stdout),
            stderr: utf8_or_lossy(output.stderr),
            success: output.status.success(),
            exit_code: output.status.code(),
        }
    }

    pub fn stdout_trimmed(&self) -> &str {
        self.stdout.trim()
    }

    #[allow(dead_code)]
    pub fn lines(&self) -> Vec<&str> {
        self.stdout.lines().collect()
    }

    /// stdout + stderr for error messages. Many git commands (stash pop/apply,
    /// merge, cherry-pick) report conflicts on stdout with empty stderr.
    pub fn combined_output(&self) -> String {
        let stdout = self.stdout.trim();
        let stderr = self.stderr.trim();
        match (stdout.is_empty(), stderr.is_empty()) {
            (true, true) => "No output from git.".to_string(),
            (false, true) => stdout.to_string(),
            (true, false) => stderr.to_string(),
            (false, false) => format!("{stdout}\n{stderr}"),
        }
    }
}

/// Convert process output bytes into a String.  Git output is almost always
/// valid UTF-8, so move the buffer in place (zero-copy) on the happy path and
/// only fall back to a lossy allocation when the bytes are invalid.
fn utf8_or_lossy(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
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

    pub fn git_no_optional_locks() -> Self {
        Self::git().env("GIT_OPTIONAL_LOCKS", "0")
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }

    pub fn args(mut self, args: &[&str]) -> Self {
        self.args.extend(args.iter().map(|s| s.to_string()));
        self
    }

    #[allow(dead_code)]
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
        // Skip building the description string when no log is installed —
        // otherwise every spawn allocates a String that is never read.
        if command_log_enabled() {
            log_command(&self.description());
        }

        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);

        if let Some(ref cwd) = self.cwd {
            cmd.current_dir(cwd);
        }

        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        make_non_interactive(&mut cmd);

        if self.stdin_data.is_some() {
            cmd.stdin(Stdio::piped());
        } else {
            // Don't inherit the TUI's stdin — a child that reads it races us
            // for keystrokes (notably `q` while background fetch/ssh runs).
            cmd.stdin(Stdio::null());
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = if let Some(ref stdin_data) = self.stdin_data {
            let mut child = cmd
                .spawn()
                .with_context(|| format!("Failed to spawn: {} {:?}", self.program, self.args))?;

            if let Some(ref mut stdin) = child.stdin {
                use std::io::Write;
                stdin.write_all(stdin_data.as_bytes())?;
            }

            child
                .wait_with_output()
                .with_context(|| format!("Failed to wait: {} {:?}", self.program, self.args))?
        } else {
            cmd.output()
                .with_context(|| format!("Failed to run: {} {:?}", self.program, self.args))?
        };

        Ok(CmdResult::from_output(output))
    }

    pub fn run_expecting_success(&self) -> Result<CmdResult> {
        let result = self.run()?;
        if !result.success {
            anyhow::bail!(
                "Command failed (exit {}): {} {}\n\n{}",
                result.exit_code.unwrap_or(-1),
                self.program,
                self.args.join(" "),
                result.combined_output(),
            );
        }
        Ok(result)
    }

    pub fn description(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}
