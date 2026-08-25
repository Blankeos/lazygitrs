//! Controlling-terminal helpers for TUI I/O.
//!
//! Editors like Helix launch external tools with `:insert-output`, which
//! redirects stdout into a pipe and stdin to `/dev/null`, while Helix's own
//! EventStream helper keeps reading `/dev/tty`. Bytes from the keyboard are
//! then consumed into Helix's queue and never reach the child — UI can paint
//! (if we draw on `/dev/tty`) but keys do nothing.
//!
//! On macOS there is a second trap: crossterm's default mio/kqueue backend
//! returns `POLLNVAL` for `/dev/tty`, so `event::poll` never wakes when stdin
//! is not a TTY. Enabling crossterm's `use-dev-tty` feature switches to
//! filedescriptor's `select()`-based reader, which does work on `/dev/tty`.
//!
//! Fix:
//! 1. Draw on a separate `/dev/tty` handle (do **not** `dup2` onto stdout —
//!    that closes Helix's pipe early so Helix resumes mid-session).
//! 2. Move into our own process group and become the tty foreground so Helix
//!    is backgrounded; its tty reads get `SIGTTIN` and stop racing us.
//! 3. On exit, restore Helix as foreground and `SIGCONT` it — and do **not**
//!    leave the alt-screen / disable raw mode / drop mouse: Helix still owns
//!    those for the rest of the session.
//! 4. Build with crossterm `use-dev-tty` (+ `libc`) so input is readable from
//!    `/dev/tty` under Helix's redirected stdio.

use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// Set when stdin/stdout are not both TTYs at startup (Helix `:insert-output`).
static NESTED_TTY_LAUNCH: AtomicBool = AtomicBool::new(false);

/// Helix (or parent) foreground process group to restore on exit.
static PREV_FOREGROUND_PGID: AtomicI32 = AtomicI32::new(-1);

/// Writer used for ratatui / crossterm screen output.
pub type TuiOutput = Box<dyn Write + Send>;

/// True when launched with redirected stdin/stdout (Helix `:insert-output`).
pub fn nested_tty_launch() -> bool {
    NESTED_TTY_LAUNCH.load(Ordering::Relaxed)
}

/// Back-compat alias used by the GUI keyboard-setup path.
pub fn reclaimed_controlling_tty() -> bool {
    nested_tty_launch()
}

/// Detect a nested/piped launch, then claim the tty foreground so Helix's
/// EventStream cannot steal keystrokes. Does **not** remap stdout.
pub fn reclaim_controlling_tty() {
    let stdin_ok = io::stdin().is_terminal();
    let stdout_ok = io::stdout().is_terminal();
    if stdin_ok && stdout_ok {
        return;
    }
    NESTED_TTY_LAUNCH.store(true, Ordering::Relaxed);

    #[cfg(unix)]
    {
        claim_foreground_tty();
    }
}

/// Open the best available writer for the TUI.
///
/// Under a nested launch always prefer `/dev/tty` so Helix's stdout pipe stays
/// open (Helix waits until we exit). Otherwise use stdout.
pub fn open_tui_output() -> io::Result<TuiOutput> {
    if nested_tty_launch() || !io::stdout().is_terminal() {
        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            match OpenOptions::new().read(true).write(true).open("/dev/tty") {
                Ok(tty) => return Ok(Box::new(tty)),
                Err(err) => {
                    tracing::debug!("open /dev/tty for TUI output failed: {err}");
                }
            }
        }
    }
    Ok(Box::new(io::stdout()))
}

/// Restore the previous foreground process group and continue the parent
/// (Helix may have been stopped by `SIGTTIN` while we owned the tty).
pub fn restore_foreground_tty() {
    #[cfg(unix)]
    {
        let prev = PREV_FOREGROUND_PGID.swap(-1, Ordering::Relaxed);
        if prev < 0 {
            return;
        }
        if let Ok(tty) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
        {
            use std::os::fd::AsRawFd;
            let old = unsafe { libc::signal(libc::SIGTTOU, libc::SIG_IGN) };
            unsafe {
                let _ = libc::tcsetpgrp(tty.as_raw_fd(), prev);
                libc::signal(libc::SIGTTOU, old);
                // Helix's EventStream may have stopped the whole editor via SIGTTIN.
                let _ = libc::kill(-prev, libc::SIGCONT);
            }
        }
    }
}

#[cfg(unix)]
fn claim_foreground_tty() {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

    let tty = match OpenOptions::new().read(true).write(true).open("/dev/tty") {
        Ok(tty) => tty,
        Err(err) => {
            tracing::debug!("open /dev/tty to claim foreground failed: {err}");
            return;
        }
    };
    let tty_fd = tty.as_raw_fd();

    unsafe {
        // Own process group, then make it the tty foreground.
        let pid = libc::getpid();
        if libc::setpgid(0, 0) < 0 {
            tracing::debug!(
                "setpgid for nested TUI failed: {}",
                io::Error::last_os_error()
            );
            return;
        }
        let pgid = libc::getpgid(0);
        let old = libc::signal(libc::SIGTTOU, libc::SIG_IGN);
        let prev = libc::tcgetpgrp(tty_fd);
        if prev > 0 && prev != pgid {
            PREV_FOREGROUND_PGID.store(prev, Ordering::Relaxed);
        }
        if libc::tcsetpgrp(tty_fd, pgid) < 0 {
            tracing::debug!(
                "tcsetpgrp for nested TUI failed: {}",
                io::Error::last_os_error()
            );
            // Roll back process-group change best-effort if we never claimed.
            if prev > 0 {
                let _ = libc::setpgid(0, prev);
            }
        }
        libc::signal(libc::SIGTTOU, old);
        let _ = pid; // silence unused in some cfgs
    }
}
