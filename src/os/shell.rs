//! Non-interactive, bounded shell jobs. A job owns its child until it is reaped;
//! dropping the job cancels it and waits for the worker to finish cleanup.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const OUTPUT_LIMIT: usize = 1024 * 1024;
const TRUNCATION_NOTICE: &str = "\n[output truncated]\n";

#[derive(Debug)]
pub struct ShellResult {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub cancelled: bool,
    pub timed_out: bool,
}

pub struct ShellJob {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<ShellResult>>,
    worker: Option<JoinHandle<()>>,
    result_taken: bool,
}

impl ShellJob {
    /// Start a shell job with a five-minute deadline. Launch/I/O errors are
    /// delivered through `try_result`; setup and thread creation errors are
    /// returned here. An empty shell is not replaced with a default here.
    ///
    /// With config loading disabled, use `sh -c`, regardless of `shell`, to
    /// preserve custom-command semantics. Otherwise bash/zsh load their rc
    /// files; fish uses a native wrapper, and other shells receive native `-c`.
    /// Remaining processes in the job's group are terminated on shell exit.
    /// Currently requires Unix process-group support.
    pub fn spawn(
        repo: &Path,
        command: &str,
        shell: &OsStr,
        load_shell_config: bool,
    ) -> Result<Self> {
        Self::spawn_with_timeout(repo, command, shell, load_shell_config, DEFAULT_TIMEOUT)
    }

    fn spawn_with_timeout(
        repo: &Path,
        command: &str,
        shell: &OsStr,
        load_shell_config: bool,
        timeout: Duration,
    ) -> Result<Self> {
        Self::spawn_command(
            shell_command(repo, command, shell, load_shell_config)?,
            timeout,
        )
    }

    fn spawn_command(command: Command, timeout: Duration) -> Result<Self> {
        // Without process-group termination and nonblocking pipes we cannot
        // uphold the cancellation/Drop contract. Fail explicitly rather than
        // pretending killing only a shell is safe on unsupported platforms.
        #[cfg(not(unix))]
        {
            let _ = (command, timeout);
            anyhow::bail!("Shell jobs require Unix process-group support");
        }

        #[cfg(unix)]
        {
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = Arc::clone(&cancel);
            let (sender, result) = mpsc::channel();
            let worker = thread::Builder::new()
                .name("shell-job".into())
                .spawn(move || {
                    let outcome = unix::run(command, &worker_cancel, timeout);
                    let _ = sender.send(outcome);
                })
                .context("Failed to start shell worker")?;
            Ok(Self {
                cancel,
                result,
                worker: Some(worker),
                result_taken: false,
            })
        }
    }

    /// Poll without blocking. The result (including errors) is returned once.
    pub fn try_result(&mut self) -> Option<Result<ShellResult>> {
        if self.result_taken {
            return None;
        }
        let result = match self.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(anyhow!("Shell worker stopped unexpectedly"))
            }
        };
        self.result_taken = true;
        Some(result)
    }

    /// Request cancellation. Idempotent; the worker terminates the whole
    /// process group and reaps the shell before publishing its result.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Drop for ShellJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

// The user command and absolute repo path are arguments/environment values,
// never interpolated into these scripts. Save them before sourcing: rc files may
// change positional parameters or cwd. `eval` parses the command AFTER aliases
// and functions have been defined. Stay non-interactive to avoid job-control
// process groups and attempts to acquire the TUI's terminal.
const BASH_WRAPPER: &str = r#"
builtin readonly _lazygitrs_repo="$1" _lazygitrs_command="$2"
builtin shopt -s expand_aliases
if [ -r "$HOME/.bash_aliases" ]; then
    builtin source "$HOME/.bash_aliases"
fi
if [ -r "$HOME/.bashrc" ]; then
    builtin source "$HOME/.bashrc"
fi
builtin shopt -s expand_aliases
builtin cd -- "$_lazygitrs_repo" || exit
builtin set --
builtin eval -- "$_lazygitrs_command"
"#;

const ZSH_WRAPPER: &str = r#"
builtin readonly _lazygitrs_repo="$_LAZYGITRS_JOB_REPO" _lazygitrs_command="$_LAZYGITRS_JOB_COMMAND"
builtin unset _LAZYGITRS_JOB_REPO _LAZYGITRS_JOB_COMMAND
if [ -r "${ZDOTDIR:-$HOME}/.zshrc" ]; then
    builtin source "${ZDOTDIR:-$HOME}/.zshrc"
fi
builtin setopt aliases
builtin cd -- "$_lazygitrs_repo" || exit
builtin set --
builtin eval -- "$_lazygitrs_command"
"#;

// fish reads its config before -c. Environment transport survives startup
// changes to $argv; clear $argv before eval so the command sees no job metadata
// or startup arguments. Quoted expansions preserve spaces, quotes and newlines.
const FISH_WRAPPER: &str = r#"
builtin set --local _lazygitrs_repo "$_LAZYGITRS_JOB_REPO"
builtin set --local _lazygitrs_command "$_LAZYGITRS_JOB_COMMAND"
builtin set --erase --global _LAZYGITRS_JOB_REPO _LAZYGITRS_JOB_COMMAND
builtin cd "$_lazygitrs_repo"; or exit
builtin set --global argv
builtin eval "$_lazygitrs_command"
"#;

fn shell_command(
    repo: &Path,
    command: &str,
    shell: &OsStr,
    load_shell_config: bool,
) -> Result<Command> {
    let repo = repo
        .canonicalize()
        .context("Failed to resolve shell job directory")?;
    let shell = if load_shell_config {
        shell
    } else {
        OsStr::new("sh")
    };
    let mut cmd = Command::new(shell);
    match Path::new(shell).file_name().and_then(OsStr::to_str) {
        Some("bash") if load_shell_config => {
            // Load only the explicit rc file, not an inherited BASH_ENV script
            // that runs before we can preserve the positional arguments.
            cmd.env_remove("BASH_ENV");
            cmd.args(["--noprofile", "--norc", "-c", BASH_WRAPPER, "lazygitrs"])
                .arg(&repo)
                .arg(command);
        }
        Some("zsh") if load_shell_config => {
            // Native non-interactive startup loads .zshenv (including ZDOTDIR).
            // Transport via env instead of $1/$2: .zshenv can run `set --`
            // before our wrapper starts. Do not disable it with -f.
            cmd.env("_LAZYGITRS_JOB_REPO", &repo)
                .env("_LAZYGITRS_JOB_COMMAND", command)
                .args(["-c", ZSH_WRAPPER, "lazygitrs"]);
        }
        Some("fish") if load_shell_config => {
            cmd.env("_LAZYGITRS_JOB_REPO", &repo)
                .env("_LAZYGITRS_JOB_COMMAND", command)
                .args(["-c", FISH_WRAPPER]);
        }
        _ => {
            cmd.arg("-c").arg(command);
        }
    }
    cmd.current_dir(repo);
    Ok(cmd)
}

#[cfg(unix)]
mod unix {
    use std::io::{self, Read};
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, ExitStatus, Stdio};
    use std::time::Instant;

    use super::*;

    const POLL_INTERVAL: Duration = Duration::from_millis(20);
    // Even a descendant that deliberately leaves the session cannot keep us
    // waiting on its inherited output pipes indefinitely.
    const DRAIN_GRACE: Duration = Duration::from_millis(100);
    const READS_PER_TURN: usize = 16;

    pub(super) fn run(
        mut command: Command,
        cancel: &AtomicBool,
        timeout: Duration,
    ) -> Result<ShellResult> {
        if cancel.load(Ordering::Acquire) {
            return Ok(ShellResult {
                stdout: String::new(),
                stderr: String::new(),
                success: false,
                exit_code: None,
                cancelled: true,
                timed_out: false,
            });
        }

        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // SAFETY: setsid is async-signal-safe; this closure allocates nothing
        // and touches no locks between fork and exec. The new session also
        // gives the shell a process group whose id is its pid, and no tty.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let started = Instant::now();
        let child = command.spawn().context("Failed to spawn shell")?;
        let mut process = Process {
            child,
            reaped: false,
        };
        let mut stdout = Capture::new(process.child.stdout.take().expect("piped stdout"))?;
        let mut stderr = Capture::new(process.child.stderr.take().expect("piped stderr"))?;
        let mut completion: Option<(ExitStatus, Instant)> = None;
        let mut cancelled = false;
        let mut timed_out = false;

        loop {
            // A bounded slice for EACH pipe keeps a prolific writer from
            // starving the other stream, cancellation, or deadline checks.
            stdout.drain().context("Failed to read shell stdout")?;
            stderr.drain().context("Failed to read shell stderr")?;

            if completion.is_none() {
                let exited = process.exited().context("Failed to inspect shell status")?;
                if !exited {
                    cancelled = cancel.load(Ordering::Acquire);
                    timed_out = !cancelled && started.elapsed() >= timeout;
                }
                if exited || cancelled || timed_out {
                    // Clean up background descendants on normal completion
                    // too. Kill BEFORE reaping so the group id cannot be
                    // recycled and accidentally signal an unrelated job.
                    let status = process.finish().context("Failed to terminate/reap shell")?;
                    completion = Some((status, Instant::now()));
                }
            }

            let wait = if let Some((status, ended)) = completion {
                if stdout.pipe.is_none() && stderr.pipe.is_none() || ended.elapsed() >= DRAIN_GRACE
                {
                    return Ok(ShellResult {
                        stdout: stdout.finish(),
                        stderr: stderr.finish(),
                        success: status.success() && !cancelled && !timed_out,
                        exit_code: status.code(),
                        cancelled,
                        timed_out,
                    });
                }
                POLL_INTERVAL.min(DRAIN_GRACE.saturating_sub(ended.elapsed()))
            } else {
                POLL_INTERVAL.min(timeout.saturating_sub(started.elapsed()))
            };
            poll(&stdout, &stderr, wait).context("Failed to poll shell output")?;
        }
    }

    /// Owns cleanup on every path, including pipe setup/read failures and
    /// unwinding. Nothing after `finish` may signal the now-reusable child pid.
    struct Process {
        child: Child,
        reaped: bool,
    }

    impl Process {
        fn exited(&self) -> io::Result<bool> {
            // WNOWAIT observes exit without releasing the leader's pid. Using
            // Child::try_wait here would reap it before group cleanup.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.child.id() as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if result == -1 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    return Ok(false);
                }
                return Err(error);
            }
            Ok(info.si_signo != 0)
        }

        fn kill_group(&self) -> io::Result<()> {
            // setsid established pgid == pid. Never use a shared/TUI group.
            if unsafe { libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL) } == -1 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error);
                }
            }
            Ok(())
        }

        fn finish(&mut self) -> io::Result<ExitStatus> {
            let killed = self.kill_group().or_else(|error| {
                // Darwin reports EPERM, not ESRCH, for a group containing
                // only its zombie leader. Once the leader has exited, this
                // cleanup is best effort; a live leader still reports errors.
                #[cfg(target_os = "macos")]
                if error.raw_os_error() == Some(libc::EPERM) && self.exited()? {
                    return Ok(());
                }
                Err(error)
            });
            // Fallback also ensures the leader is killed if group signalling
            // failed. Always reap, even when returning that signalling error.
            let _ = self.child.kill();
            let status = self.child.wait()?;
            self.reaped = true;
            killed?;
            Ok(status)
        }
    }

    impl Drop for Process {
        fn drop(&mut self) {
            if !self.reaped {
                let _ = self.finish();
            }
        }
    }

    struct Capture<P> {
        pipe: Option<P>,
        bytes: Vec<u8>,
        truncated: bool,
    }

    impl<P: Read + AsRawFd> Capture<P> {
        fn new(pipe: P) -> io::Result<Self> {
            let fd = pipe.as_raw_fd();
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags == -1
                || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
            {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                pipe: Some(pipe),
                bytes: Vec::new(),
                truncated: false,
            })
        }

        fn drain(&mut self) -> io::Result<()> {
            let Some(pipe) = &mut self.pipe else {
                return Ok(());
            };
            let mut buffer = [0; 8192];
            for _ in 0..READS_PER_TURN {
                match pipe.read(&mut buffer) {
                    Ok(0) => {
                        self.pipe = None;
                        break;
                    }
                    Ok(count) => {
                        let retained = count.min(OUTPUT_LIMIT - self.bytes.len());
                        self.bytes.extend_from_slice(&buffer[..retained]);
                        self.truncated |= retained != count;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
            Ok(())
        }

        fn finish(self) -> String {
            // Invalid UTF-8 can expand to replacement characters; cap the
            // returned String as well as the underlying raw-byte buffer.
            let mut output = String::from_utf8_lossy(&self.bytes).into_owned();
            let mut end = output.len().min(OUTPUT_LIMIT);
            while !output.is_char_boundary(end) {
                end -= 1;
            }
            let truncated = self.truncated || self.pipe.is_some() || output.len() > end;
            output.truncate(end);
            if truncated {
                output.push_str(TRUNCATION_NOTICE);
            }
            output
        }

        fn fd(&self) -> libc::c_int {
            self.pipe.as_ref().map_or(-1, AsRawFd::as_raw_fd)
        }
    }

    #[cfg(test)]
    #[test]
    fn invalid_utf8_is_lossy_and_still_bounded() {
        let capture = Capture::<std::fs::File> {
            pipe: None,
            bytes: vec![0xff; OUTPUT_LIMIT],
            truncated: false,
        };
        let output = capture.finish();
        assert!(output.len() <= OUTPUT_LIMIT + TRUNCATION_NOTICE.len());
        assert!(output.starts_with('\u{fffd}'));
        assert!(output.ends_with(TRUNCATION_NOTICE));
    }

    fn poll<A: Read + AsRawFd, B: Read + AsRawFd>(
        stdout: &Capture<A>,
        stderr: &Capture<B>,
        timeout: Duration,
    ) -> io::Result<()> {
        let mut pipes = [stdout.fd(), stderr.fd()].map(|fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        });
        let result = unsafe {
            libc::poll(
                pipes.as_mut_ptr(),
                pipes.len() as libc::nfds_t,
                timeout.as_millis() as libc::c_int,
            )
        };
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[cfg(not(unix))]
    #[test]
    fn unsupported_platform_returns_an_error() {
        let repo = std::env::current_dir().unwrap();
        assert!(ShellJob::spawn(&repo, ":", OsStr::new("sh"), false).is_err());
    }

    #[test]
    fn non_posix_shells_receive_only_native_arguments() {
        let repo = std::env::current_dir().unwrap();
        let input = "printf '%s' 'quotes; $variables; $(substitution)'";
        for shell in ["/usr/bin/nu", "/bin/dash"] {
            let cmd = shell_command(&repo, input, OsStr::new(shell), true).unwrap();
            assert_eq!(cmd.get_program(), shell);
            assert_eq!(cmd.get_args().collect::<Vec<_>>(), ["-c", input]);
        }
    }

    #[test]
    fn custom_commands_always_use_sh_without_a_wrapper() {
        let repo = std::env::current_dir().unwrap();
        let cmd = shell_command(&repo, "echo custom", OsStr::new("fish"), false).unwrap();
        assert_eq!(cmd.get_program(), "sh");
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), ["-c", "echo custom"]);
    }

    #[test]
    fn wrappers_keep_paths_and_commands_in_separate_arguments() {
        let repo = std::env::current_dir().unwrap().canonicalize().unwrap();
        let input = "printf '%s' '\"; touch should-not-exist; #'";
        for shell in ["/bin/bash"] {
            let cmd = shell_command(&repo, input, OsStr::new(shell), true).unwrap();
            let args = cmd.get_args().map(OsStr::to_os_string).collect::<Vec<_>>();
            assert_eq!(args[args.len() - 2], repo.as_os_str());
            assert_eq!(args[args.len() - 1], OsString::from(input));
            assert!(!args[args.len() - 4].to_string_lossy().contains(input));
        }
    }

    #[test]
    fn startup_safe_wrappers_transport_literal_values_in_environment() {
        let repo = std::env::current_dir().unwrap().canonicalize().unwrap();
        let input = "printf '%s' '\"; literal $value; #'";
        for (shell, wrapper) in [("/bin/zsh", ZSH_WRAPPER), ("/usr/bin/fish", FISH_WRAPPER)] {
            let cmd = shell_command(&repo, input, OsStr::new(shell), true).unwrap();
            let args = cmd.get_args().collect::<Vec<_>>();
            assert_eq!(args[0], "-c");
            assert_eq!(args[1], wrapper);
            assert!(!wrapper.contains(input));
            let env = cmd.get_envs().collect::<Vec<_>>();
            assert!(env.contains(&(OsStr::new("_LAZYGITRS_JOB_REPO"), Some(repo.as_os_str()))));
            assert!(env.contains(&(
                OsStr::new("_LAZYGITRS_JOB_COMMAND"),
                Some(OsStr::new(input))
            )));
        }
    }

    #[test]
    fn wrappers_clear_positional_arguments_before_eval() {
        for wrapper in [BASH_WRAPPER, ZSH_WRAPPER] {
            assert!(wrapper.contains("builtin set --\nbuiltin eval"));
        }
        assert!(FISH_WRAPPER.contains("builtin set --global argv\nbuiltin eval"));
    }

    #[cfg(unix)]
    mod execution {
        use std::fs;
        use std::path::PathBuf;
        use std::sync::atomic::AtomicUsize;
        use std::time::Instant;

        use super::*;

        struct TestDir(PathBuf);

        impl TestDir {
            fn new() -> Self {
                static NEXT: AtomicUsize = AtomicUsize::new(0);
                let path = std::env::temp_dir().join(format!(
                    "lazygitrs-shell-{}-{} space's $dir",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path.canonicalize().unwrap())
            }

            fn spawn(&self, script: &str) -> ShellJob {
                ShellJob::spawn(&self.0, script, OsStr::new("/bin/sh"), false).unwrap()
            }
        }

        impl Drop for TestDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        fn result(job: &mut ShellJob) -> Result<ShellResult> {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(result) = job.try_result() {
                    assert!(job.try_result().is_none(), "result must be one-shot");
                    return result;
                }
                assert!(
                    Instant::now() < deadline,
                    "shell job did not complete promptly"
                );
                thread::sleep(Duration::from_millis(5));
            }
        }

        fn ready_pids(dir: &TestDir) -> Vec<libc::pid_t> {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Ok(text) = fs::read_to_string(dir.0.join("ready")) {
                    let pids = text
                        .split_whitespace()
                        .filter_map(|s| s.parse().ok())
                        .collect::<Vec<_>>();
                    if pids.len() == 2 {
                        return pids;
                    }
                }
                assert!(Instant::now() < deadline, "shell did not signal readiness");
                thread::sleep(Duration::from_millis(5));
            }
        }

        fn assert_reaped(pid: libc::pid_t) {
            let mut status = 0;
            assert_eq!(
                unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
                -1
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ECHILD)
            );
        }

        fn assert_descendant_stopped(pid: libc::pid_t) {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if unsafe { libc::kill(pid, 0) } == -1 {
                    assert_eq!(
                        std::io::Error::last_os_error().raw_os_error(),
                        Some(libc::ESRCH)
                    );
                    return;
                }
                // A reparented zombie is dead, but a container's pid 1 may not
                // reap it promptly. We can only reap our direct child.
                let state = Command::new("ps")
                    .args(["-o", "stat=", "-p", &pid.to_string()])
                    .output()
                    .unwrap();
                if String::from_utf8_lossy(&state.stdout)
                    .trim()
                    .starts_with('Z')
                {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "descendant {pid} is still running"
                );
                thread::sleep(Duration::from_millis(5));
            }
        }

        const WAIT_WITH_DESCENDANT: &str =
            "sleep 30 & descendant=$!; printf '%s %s\\n' \"$$\" \"$descendant\" > ready; wait";

        #[test]
        fn cwd_and_both_output_streams() {
            let dir = TestDir::new();
            let outcome = result(&mut dir.spawn("pwd -P; printf out; printf err >&2")).unwrap();
            assert_eq!(outcome.stdout, format!("{}\nout", dir.0.display()));
            assert_eq!(outcome.stderr, "err");
            assert!(outcome.success);
            assert_eq!(outcome.exit_code, Some(0));
            assert!(!outcome.cancelled && !outcome.timed_out);
        }

        #[test]
        fn nonzero_exit_and_no_output() {
            let dir = TestDir::new();
            let outcome = result(&mut dir.spawn("printf out; printf err >&2; exit 7")).unwrap();
            assert!(!outcome.success);
            assert_eq!(outcome.exit_code, Some(7));
            assert_eq!(outcome.stdout, "out");
            assert_eq!(outcome.stderr, "err");
            let empty = result(&mut dir.spawn(":")).unwrap();
            assert!(empty.success);
            assert!(empty.stdout.is_empty() && empty.stderr.is_empty());
        }

        #[test]
        fn stdin_is_eof_and_there_is_no_controlling_tty() {
            let dir = TestDir::new();
            let script = "if read value; then exit 1; fi; if ( : </dev/tty ) 2>/dev/null; then exit 2; fi; printf eof";
            let outcome = result(&mut dir.spawn(script)).unwrap();
            assert!(outcome.success, "{outcome:?}");
            assert_eq!(outcome.stdout, "eof");
            assert!(outcome.stderr.is_empty());
        }

        #[test]
        fn missing_shell_reports_an_error() {
            let dir = TestDir::new();
            let mut job =
                ShellJob::spawn(&dir.0, ":", dir.0.join("missing-shell").as_os_str(), true)
                    .unwrap();
            assert!(
                result(&mut job)
                    .unwrap_err()
                    .to_string()
                    .contains("Failed to spawn shell")
            );
            let mut empty = ShellJob::spawn(&dir.0, ":", OsStr::new(""), true).unwrap();
            assert!(
                result(&mut empty).is_err(),
                "the caller must choose the fallback shell"
            );
        }

        #[test]
        fn cancellation_before_launch_does_not_run_the_command() {
            let dir = TestDir::new();
            let cmd =
                shell_command(&dir.0, "touch should-not-exist", OsStr::new("sh"), false).unwrap();
            let outcome = unix::run(cmd, &AtomicBool::new(true), DEFAULT_TIMEOUT).unwrap();
            assert!(outcome.cancelled && !outcome.timed_out && !outcome.success);
            assert_eq!(outcome.exit_code, None);
            assert!(!dir.0.join("should-not-exist").exists());
        }

        #[test]
        fn cancellation_kills_group_and_reaps_shell() {
            let dir = TestDir::new();
            let mut job = dir.spawn(WAIT_WITH_DESCENDANT);
            assert!(job.try_result().is_none());
            let pids = ready_pids(&dir);
            job.cancel();
            job.cancel();
            let outcome = result(&mut job).unwrap();
            assert!(outcome.cancelled && !outcome.timed_out && !outcome.success);
            assert_eq!(outcome.exit_code, None);
            assert_reaped(pids[0]);
            assert_descendant_stopped(pids[1]);
        }

        #[test]
        fn escaped_descendant_cannot_hold_the_output_pipes_open() {
            use std::os::fd::AsRawFd;
            use std::os::unix::process::CommandExt;

            struct EscapedProcess(libc::pid_t);
            impl Drop for EscapedProcess {
                fn drop(&mut self) {
                    unsafe { libc::kill(self.0, libc::SIGKILL) };
                }
            }

            let dir = TestDir::new();
            let ready = fs::File::create(dir.0.join("escaped")).unwrap();
            let fd = ready.as_raw_fd();
            let fd_limit = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
            assert!(fd_limit > 0 && fd_limit <= libc::c_int::MAX as libc::c_long);
            let mut cmd = shell_command(&dir.0, "printf done", OsStr::new("sh"), false).unwrap();
            // Make a descendant that inherits the redirected output fds, but
            // leaves the job's session. Between fork and _exit use ONLY
            // async-signal-safe libc functions, never the Rust runtime.
            unsafe {
                cmd.pre_exec(move || {
                    let pid = libc::fork();
                    if pid == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if pid == 0 {
                        if libc::setsid() == -1 {
                            libc::_exit(1);
                        }
                        // Unlike an exec, a bare fork retains even CLOEXEC
                        // fds: close the spawn handshake and unrelated jobs'
                        // pipe ends before holding ONLY stdout/stderr open.
                        for fd in 3..fd_limit as libc::c_int {
                            libc::close(fd);
                        }
                        libc::sleep(30);
                        libc::_exit(0);
                    }
                    if libc::write(
                        fd,
                        (&pid as *const libc::pid_t).cast(),
                        std::mem::size_of_val(&pid),
                    ) == -1
                    {
                        libc::kill(pid, libc::SIGKILL);
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let mut job = ShellJob::spawn_command(cmd, DEFAULT_TIMEOUT).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let escaped = loop {
                let bytes = fs::read(dir.0.join("escaped")).unwrap();
                if let Ok(bytes) = bytes.try_into() {
                    break EscapedProcess(libc::pid_t::from_ne_bytes(bytes));
                }
                assert!(
                    Instant::now() < deadline,
                    "escaped descendant did not start"
                );
                thread::sleep(Duration::from_millis(5));
            };
            let outcome = result(&mut job).unwrap();
            assert!(outcome.success, "{outcome:?}");
            assert_eq!(outcome.stdout, format!("done{TRUNCATION_NOTICE}"));
            assert_eq!(outcome.stderr, TRUNCATION_NOTICE);
            assert_eq!(
                unsafe { libc::kill(escaped.0, 0) },
                0,
                "descendant really escaped the group"
            );
            drop(escaped);
        }

        #[test]
        fn cancellation_is_not_starved_by_continuously_readable_streams() {
            let dir = TestDir::new();
            let mut job = dir.spawn("printf '%s %s\\n' \"$$\" \"$$\" > ready; while :; do printf abcdefghijklmnopqrstuvwxyz; printf abcdefghijklmnopqrstuvwxyz >&2; done");
            let pids = ready_pids(&dir);
            job.cancel();
            let outcome = result(&mut job).unwrap();
            assert!(outcome.cancelled && !outcome.success);
            assert_reaped(pids[0]);
        }

        #[test]
        fn timeout_kills_group_and_reaps_shell() {
            let dir = TestDir::new();
            let mut job = ShellJob::spawn_with_timeout(
                &dir.0,
                WAIT_WITH_DESCENDANT,
                OsStr::new("/bin/sh"),
                false,
                Duration::from_millis(250),
            )
            .unwrap();
            let pids = ready_pids(&dir);
            let outcome = result(&mut job).unwrap();
            assert!(outcome.timed_out && !outcome.cancelled && !outcome.success);
            assert_eq!(outcome.exit_code, None);
            assert_reaped(pids[0]);
            assert_descendant_stopped(pids[1]);
        }

        #[test]
        fn drop_cancels_joins_and_reaps() {
            let dir = TestDir::new();
            let job = dir.spawn(WAIT_WITH_DESCENDANT);
            let pids = ready_pids(&dir);
            let started = Instant::now();
            drop(job);
            assert!(started.elapsed() < Duration::from_secs(2));
            assert_reaped(pids[0]);
            assert_descendant_stopped(pids[1]);
        }

        #[test]
        fn large_output_is_drained_but_bounded_per_stream() {
            let dir = TestDir::new();
            let script = "awk 'BEGIN { for (i=0; i<32768; i++) { print \"0123456789012345678901234567890123456789012345678901234567890123\"; print \"abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijkl\" > \"/dev/stderr\" } }'";
            let outcome = result(&mut dir.spawn(script)).unwrap();
            assert!(outcome.success, "{outcome:?}");
            for output in [&outcome.stdout, &outcome.stderr] {
                assert_eq!(output.len(), OUTPUT_LIMIT + TRUNCATION_NOTICE.len());
                assert!(output.ends_with(TRUNCATION_NOTICE));
            }
        }

        #[test]
        fn closed_pipes_do_not_disable_timeout() {
            let dir = TestDir::new();
            let mut job = ShellJob::spawn_with_timeout(
                &dir.0,
                "exec >/dev/null 2>&1; sleep 30",
                OsStr::new("/bin/sh"),
                false,
                Duration::from_millis(100),
            )
            .unwrap();
            let outcome = result(&mut job).unwrap();
            assert!(outcome.timed_out && !outcome.success);
        }

        #[test]
        fn shell_exit_does_not_wait_for_background_output_pipes() {
            let dir = TestDir::new();
            let script = "sleep 30 & descendant=$!; printf '%s %s\\n' \"$$\" \"$descendant\" > ready; printf done";
            let mut job = dir.spawn(script);
            let outcome = result(&mut job).unwrap();
            assert!(outcome.success);
            assert_eq!(outcome.stdout, "done");
            let pids = ready_pids(&dir);
            assert_reaped(pids[0]);
            assert_descendant_stopped(pids[1]);
        }

        fn installed(shell: &str) -> bool {
            Command::new(shell)
                .arg("--version")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        }

        fn startup_test(shell: &str, rc: &str, zdotdir: bool) {
            if !installed(shell) {
                return;
            }
            let dir = TestDir::new();
            let home = TestDir::new();
            fs::write(home.0.join(rc), "alias shell_alias='printf alias'; shell_function() { printf function; }; cd /; set -- changed\n").unwrap();
            let mut cmd = shell_command(&dir.0, "shell_alias; shell_function; printf '\\n'; pwd -P; printf '%s' '\"; literal $value; #'; printf '\\nargs:%s' \"$#\"; printf '<%s>' \"$@\"", OsStr::new(shell), true).unwrap();
            cmd.env("HOME", &home.0).env_remove("ZDOTDIR");
            if zdotdir {
                cmd.env("HOME", &dir.0).env("ZDOTDIR", &home.0);
            }
            let outcome =
                result(&mut ShellJob::spawn_command(cmd, DEFAULT_TIMEOUT).unwrap()).unwrap();
            assert!(outcome.success, "{outcome:?}");
            assert_eq!(
                outcome.stdout,
                format!(
                    "aliasfunction\n{}\n\"; literal $value; #\nargs:0<>",
                    dir.0.display()
                )
            );
            assert!(outcome.stderr.is_empty(), "{outcome:?}");
        }

        #[test]
        fn bash_loads_aliases_functions_and_restores_cwd() {
            startup_test("/bin/bash", ".bashrc", false);
            startup_test("/bin/bash", ".bash_aliases", false);
        }

        #[test]
        fn zsh_loads_aliases_functions_and_restores_cwd() {
            startup_test("/bin/zsh", ".zshrc", false);
            startup_test("/bin/zsh", ".zshrc", true);
        }

        #[test]
        fn bash_and_zsh_commands_have_no_wrapper_arguments_without_rc() {
            for shell in ["/bin/bash", "/bin/zsh"] {
                if !installed(shell) {
                    continue;
                }
                let dir = TestDir::new();
                let home = TestDir::new();
                let mut cmd = shell_command(
                    &dir.0,
                    "printf 'args:%s' \"$#\"; printf '<%s>' \"$@\"",
                    OsStr::new(shell),
                    true,
                )
                .unwrap();
                cmd.env("HOME", &home.0).env_remove("ZDOTDIR");
                let outcome =
                    result(&mut ShellJob::spawn_command(cmd, DEFAULT_TIMEOUT).unwrap()).unwrap();
                assert!(outcome.success, "{outcome:?}");
                assert_eq!(outcome.stdout, "args:0<>");
                assert!(outcome.stderr.is_empty(), "{outcome:?}");
            }
        }

        #[test]
        fn zshenv_can_replace_arguments_and_redirect_zdotdir() {
            if !installed("/bin/zsh") {
                return;
            }
            let dir = TestDir::new();
            let home = TestDir::new();
            let config = home.0.join("redirected config");
            fs::create_dir(&config).unwrap();
            fs::write(
                home.0.join(".zshenv"),
                "set -- startup changed; cd /; export ZDOTDIR=\"$HOME/redirected config\"\n",
            )
            .unwrap();
            fs::write(
                config.join(".zshrc"),
                "alias shell_alias='printf alias'; set -- rc changed; cd /\n",
            )
            .unwrap();
            let mut cmd = shell_command(
                &dir.0,
                "shell_alias; printf '\\n'; pwd -P; printf 'args:%s' \"$#\"",
                OsStr::new("/bin/zsh"),
                true,
            )
            .unwrap();
            cmd.env("HOME", &home.0).env_remove("ZDOTDIR");
            let outcome =
                result(&mut ShellJob::spawn_command(cmd, DEFAULT_TIMEOUT).unwrap()).unwrap();
            assert!(outcome.success, "{outcome:?}");
            assert_eq!(
                outcome.stdout,
                format!("alias\n{}\nargs:0", dir.0.display())
            );
            assert!(outcome.stderr.is_empty(), "{outcome:?}");
        }

        #[test]
        fn custom_commands_do_not_load_aliases_or_functions() {
            let dir = TestDir::new();
            let home = TestDir::new();
            fs::write(
                home.0.join(".bashrc"),
                "alias shell_alias='printf alias'; shell_function() { printf function; }\n",
            )
            .unwrap();
            let mut cmd = shell_command(
                &dir.0,
                "command -v shell_alias; command -v shell_function",
                OsStr::new("/bin/bash"),
                false,
            )
            .unwrap();
            cmd.env("HOME", &home.0);
            let outcome =
                result(&mut ShellJob::spawn_command(cmd, DEFAULT_TIMEOUT).unwrap()).unwrap();
            assert!(!outcome.success);
            assert!(outcome.stdout.is_empty());
        }

        #[test]
        fn fish_runs_native_syntax_when_installed() {
            let Some(shell) = ["fish", "/opt/homebrew/bin/fish", "/usr/local/bin/fish"]
                .into_iter()
                .find(|shell| installed(shell))
            else {
                eprintln!(
                    "Skipping fish execution test: fish is not installed (including Homebrew paths)"
                );
                return;
            };
            let dir = TestDir::new();
            let home = TestDir::new();
            fs::create_dir(home.0.join("fish")).unwrap();
            fs::write(home.0.join("fish/config.fish"), "function shell_function; printf function; end\ncd /\nset --global argv startup changed\n").unwrap();
            let mut cmd = shell_command(
                &dir.0,
                "set value fish; printf '%s' $value; shell_function; printf '\\n'; pwd -P; printf 'args:%s\\n' (count $argv); printf '%s' '\"; literal $value; #'; printf '%s' \"single'quote\"",
                OsStr::new(shell),
                true,
            )
            .unwrap();
            cmd.env("HOME", &home.0).env("XDG_CONFIG_HOME", &home.0);
            let outcome =
                result(&mut ShellJob::spawn_command(cmd, DEFAULT_TIMEOUT).unwrap()).unwrap();
            assert!(outcome.success, "{outcome:?}");
            assert_eq!(
                outcome.stdout,
                format!(
                    "fishfunction\n{}\nargs:0\n\"; literal $value; #single'quote",
                    dir.0.display()
                )
            );
            assert!(outcome.stderr.is_empty(), "{outcome:?}");
        }
    }
}
