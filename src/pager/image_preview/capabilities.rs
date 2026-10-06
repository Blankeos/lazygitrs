//! Terminal capability detection and the shared image protocol picker.

use std::io::Write;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use ratatui_image::picker::cap_parser::{Parser, Response};
use ratatui_image::picker::{Picker, ProtocolType};

const PROBE_TIMEOUT: Duration = Duration::from_millis(450);
// Only CSI replies: Kitty/Ghostty are identified by hints, not an APC query.
// Graphics replies can arrive after the status reply and leak into key input.
const CAPABILITY_QUERY: &str = "\x1b[c\x1b[16t\x1b[5n";
const FALLBACK_KITTY_FONT: (u16, u16) = (8, 16);

static GLOBAL_PICKER: OnceLock<Option<Picker>> = OnceLock::new();

pub(super) fn global_picker_ref() -> Option<&'static Picker> {
    GLOBAL_PICKER.get().and_then(|o| o.as_ref())
}

/// Detect capabilities once, before the TUI input reader starts. Only the
/// first call probes; later calls are no-ops. Never touches stdin/stdout,
/// never mutates tmux settings.
pub fn initialize() {
    if GLOBAL_PICKER.get().is_some() {
        return;
    }
    let picker = detect_picker();
    let _ = GLOBAL_PICKER.set(picker);
}

fn preview_disabled_by_env() -> bool {
    match std::env::var("LAZYGITRS_IMAGE_PREVIEW") {
        Ok(v) => {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "off" | "0" | "false" | "no" | "disable" | "disabled" | "none"
            )
        }
        Err(_) => false,
    }
}

fn in_unsupported_multiplexer() -> bool {
    if std::env::var_os("TMUX").is_some() {
        return true;
    }
    if std::env::var_os("ZELLIJ").is_some() {
        return true;
    }
    if std::env::var_os("ZELLIJ_SESSION_NAME").is_some() {
        return true;
    }
    if std::env::var("TERM").is_ok_and(|t| t.starts_with("tmux") || t.starts_with("screen")) {
        return true;
    }
    if std::env::var("TERM_PROGRAM").is_ok_and(|p| p == "tmux") {
        return true;
    }
    false
}

fn is_konsole() -> bool {
    if std::env::var_os("KONSOLE_VERSION").is_some() {
        return true;
    }
    if std::env::var_os("KONSOLE_DBUS_SESSION").is_some() {
        return true;
    }
    if std::env::var("TERM").is_ok_and(|t| t.to_ascii_lowercase().contains("konsole")) {
        return true;
    }
    false
}

/// iTerm2/WezTerm hints. `rio` is known-broken and `Hyper` is unverified, so
/// neither enables graphics here.
fn iterm2_hint_from_env() -> bool {
    if let Ok(tp) = std::env::var("TERM_PROGRAM") {
        if tp.contains("iTerm")
            || tp.contains("WezTerm")
            || tp.contains("mintty")
            || tp.contains("vscode")
            || tp.contains("Tabby")
        {
            return true;
        }
    }
    if std::env::var("LC_TERMINAL").is_ok_and(|v| v.contains("iTerm")) {
        return true;
    }
    if std::env::var("ITERM_SESSION_ID").is_ok_and(|s| !s.is_empty()) {
        return true;
    }
    if std::env::var("WEZTERM_EXECUTABLE").is_ok_and(|s| !s.is_empty()) {
        return true;
    }
    false
}

fn strong_kitty_ghostty_hint() -> bool {
    if std::env::var("KITTY_WINDOW_ID").is_ok_and(|s| !s.is_empty()) {
        return true;
    }
    if std::env::var("TERM").is_ok_and(|t| t == "xterm-kitty" || t.contains("kitty")) {
        return true;
    }
    if std::env::var("TERM_PROGRAM")
        .is_ok_and(|p| p.contains("kitty") || p.contains("ghostty") || p.contains("Ghostty"))
    {
        return true;
    }
    if std::env::var_os("GHOSTTY_RESOURCES_DIR").is_some() {
        return true;
    }
    if std::env::var("GHOSTTY_BIN_DIR").is_ok_and(|s| !s.is_empty()) {
        return true;
    }
    false
}

#[cfg(unix)]
fn font_size_from_fd(fd: std::os::fd::RawFd) -> Option<(u16, u16)> {
    // SAFETY: ioctl with TIOCGWINSZ on a borrowed fd; `ws` is a plain POD out-param.
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) };
    if ret != 0 {
        return None;
    }
    if ws.ws_col == 0 || ws.ws_row == 0 || ws.ws_xpixel == 0 || ws.ws_ypixel == 0 {
        return None;
    }
    let w = ws.ws_xpixel / ws.ws_col;
    let h = ws.ws_ypixel / ws.ws_row;
    if w == 0 || h == 0 {
        return None;
    }
    if !(4..=100).contains(&w) || !(6..=200).contains(&h) {
        return None;
    }
    Some((w, h))
}

#[cfg(unix)]
fn font_size_from_winsize() -> Option<(u16, u16)> {
    use std::os::fd::AsRawFd;
    if let Ok(f) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
    {
        if let Some(fs) = font_size_from_fd(f.as_raw_fd()) {
            return Some(fs);
        }
    }
    font_size_from_fd(libc::STDOUT_FILENO)
}

#[cfg(not(unix))]
fn font_size_from_winsize() -> Option<(u16, u16)> {
    None
}

/// Bounded synchronous capability probe on `/dev/tty`. No threads, no
/// detached readers; termios is restored via a guard.
#[cfg(unix)]
fn probe_tty_responses(timeout: Duration) -> Option<Vec<Response>> {
    use std::os::fd::AsRawFd;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let fd = file.as_raw_fd();

    // SAFETY: tcgetattr on our own open tty fd with a valid out-pointer.
    let mut orig: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut orig) } != 0 {
        return None;
    }
    struct Guard {
        fd: i32,
        orig: libc::termios,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe {
                libc::tcsetattr(self.fd, libc::TCSANOW, &self.orig);
            }
        }
    }
    let _guard = Guard { fd, orig };

    // SAFETY: tcsetattr on our own tty fd.
    let mut raw = _guard.orig;
    raw.c_lflag &= !(libc::ICANON | libc::ECHO);
    raw.c_cc[libc::VMIN as usize] = 0;
    raw.c_cc[libc::VTIME as usize] = 0;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        return None;
    }

    {
        let mut w = &file;
        if w.write_all(CAPABILITY_QUERY.as_bytes()).is_err() {
            return None;
        }
        if w.flush().is_err() {
            return None;
        }
    }

    let mut parser = Parser::new();
    let mut out: Vec<Response> = Vec::new();
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 512];

    loop {
        let remain = deadline.saturating_duration_since(Instant::now());
        if remain.is_zero() {
            break;
        }
        let ms = remain.as_millis().min(100) as libc::c_int;
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll on our own tty fd with a valid pollfd pointer.
        let pr = unsafe { libc::poll(&mut pfd, 1, ms) };
        if pr < 0 {
            continue;
        }
        if pr == 0 {
            continue;
        }
        if pfd.revents & libc::POLLIN == 0 {
            if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                break;
            }
            continue;
        }
        // SAFETY: read into a valid stack buffer.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n <= 0 {
            continue;
        }
        for b in buf.iter().take(n as usize) {
            for r in parser.push(char::from(*b)) {
                if r == Response::Status {
                    return Some(out);
                }
                out.push(r);
            }
        }
        if out.len() > 32 {
            break;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(not(unix))]
fn probe_tty_responses(_timeout: Duration) -> Option<Vec<Response>> {
    None
}

fn interpret_probed(
    responses: Option<Vec<Response>>,
) -> (Option<ProtocolType>, Option<(u16, u16)>) {
    let Some(responses) = responses else {
        return (None, None);
    };
    let mut proto: Option<ProtocolType> = None;
    let mut font: Option<(u16, u16)> = None;
    for r in &responses {
        match r {
            Response::Kitty => {
                proto = Some(ProtocolType::Kitty);
            }
            Response::Sixel => {
                if proto.is_none() {
                    proto = Some(ProtocolType::Sixel);
                }
            }
            Response::CellSize(Some((w, h))) => {
                if *w != 0 && *h != 0 {
                    font = Some((*w, *h));
                }
            }
            Response::CellSize(None) => {}
            Response::RectangularOps | Response::CursorPositionReport(..) | Response::Status => {}
        }
    }
    (proto, font)
}

fn detect_picker() -> Option<Picker> {
    if preview_disabled_by_env() {
        return None;
    }
    if in_unsupported_multiplexer() {
        return None;
    }
    if is_konsole() {
        return None;
    }
    // Non-placeholder protocols cannot reliably clear graphics under popups
    // with this widget release. Keep them opt-in until verified end-to-end.
    let experimental = std::env::var("LAZYGITRS_IMAGE_PREVIEW").is_ok_and(|v| v == "experimental");
    if !strong_kitty_ghostty_hint() && !experimental {
        return None;
    }

    #[cfg(not(unix))]
    {
        return None;
    }

    #[cfg(unix)]
    {
        let nested = crate::os::tty::nested_tty_launch();
        let winsize_font = font_size_from_winsize();

        if nested {
            // No terminal query under nested launches; hints + ioctl only.
            if experimental && iterm2_hint_from_env() {
                let fs = winsize_font?;
                let mut p = Picker::from_fontsize(fs);
                p.set_protocol_type(ProtocolType::Iterm2);
                return Some(p);
            }
            if strong_kitty_ghostty_hint() {
                let fs = winsize_font.unwrap_or(FALLBACK_KITTY_FONT);
                if fs.0 == 0 || fs.1 == 0 {
                    return None;
                }
                let mut p = Picker::from_fontsize(fs);
                p.set_protocol_type(ProtocolType::Kitty);
                return Some(p);
            }
            return None;
        }

        // Known placeholder terminals need no protocol query. In particular,
        // don't create terminal replies when ioctl already supplies cell size.
        let probed = if strong_kitty_ghostty_hint() && winsize_font.is_some() {
            None
        } else {
            probe_tty_responses(PROBE_TIMEOUT)
        };
        let (probed_proto, probed_font) = interpret_probed(probed);
        let hint_iterm2 = experimental && iterm2_hint_from_env();
        let hint_kitty = strong_kitty_ghostty_hint();

        // iTerm2 hints win, then queried Sixel, then Kitty only with a strong
        // Kitty/Ghostty hint. A bare Kitty query response is not trusted: our
        // Kitty path needs unicode placeholders, which the query alone does
        // not guarantee.
        let proto: Option<ProtocolType> = if hint_iterm2 {
            Some(ProtocolType::Iterm2)
        } else if let Some(p) = probed_proto {
            match p {
                ProtocolType::Sixel if experimental => Some(ProtocolType::Sixel),
                ProtocolType::Kitty if hint_kitty => Some(ProtocolType::Kitty),
                _ => {
                    if hint_kitty {
                        Some(ProtocolType::Kitty)
                    } else {
                        None
                    }
                }
            }
        } else if hint_kitty {
            Some(ProtocolType::Kitty)
        } else {
            None
        };

        let proto = proto?;
        if proto == ProtocolType::Halfblocks {
            return None;
        }

        let font = winsize_font.or(probed_font).or_else(|| {
            if proto == ProtocolType::Kitty && hint_kitty {
                Some(FALLBACK_KITTY_FONT)
            } else {
                None
            }
        })?;
        if font.0 == 0 || font.1 == 0 {
            return None;
        }

        let mut picker = Picker::from_fontsize(font);
        picker.set_protocol_type(proto);
        if picker.protocol_type() == ProtocolType::Halfblocks {
            return None;
        }
        Some(picker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_query_never_requests_a_graphics_reply() {
        assert_eq!(CAPABILITY_QUERY, "\x1b[c\x1b[16t\x1b[5n");
        assert!(!CAPABILITY_QUERY.contains("\x1b_G"));
        assert!(!CAPABILITY_QUERY.contains("i=31"));
    }

    #[test]
    fn interpret_probed_without_responses_has_no_protocol_or_font() {
        assert_eq!(interpret_probed(None), (None, None));
        assert_eq!(interpret_probed(Some(Vec::new())), (None, None));
    }

    #[test]
    fn interpret_probed_recognizes_sixel() {
        assert_eq!(
            interpret_probed(Some(vec![Response::Sixel])),
            (Some(ProtocolType::Sixel), None)
        );
    }

    #[test]
    fn interpret_probed_prefers_kitty_regardless_of_response_order() {
        for responses in [
            vec![Response::Kitty, Response::Sixel],
            vec![Response::Sixel, Response::Kitty],
        ] {
            assert_eq!(
                interpret_probed(Some(responses)),
                (Some(ProtocolType::Kitty), None)
            );
        }
    }

    #[test]
    fn interpret_probed_recognizes_cell_size_without_a_protocol() {
        assert_eq!(
            interpret_probed(Some(vec![Response::CellSize(Some((8, 16)))])),
            (None, Some((8, 16)))
        );
    }

    #[test]
    fn interpret_probed_ignores_missing_and_zero_cell_sizes() {
        assert_eq!(
            interpret_probed(Some(vec![
                Response::CellSize(None),
                Response::CellSize(Some((0, 16))),
                Response::CellSize(Some((8, 0))),
                Response::CellSize(Some((0, 0))),
            ])),
            (None, None)
        );
        assert_eq!(
            interpret_probed(Some(vec![
                Response::CellSize(Some((8, 16))),
                Response::CellSize(None),
                Response::CellSize(Some((0, 16))),
                Response::CellSize(Some((8, 0))),
            ])),
            (None, Some((8, 16)))
        );
    }

    #[test]
    fn interpret_probed_keeps_last_valid_font_and_ignores_other_responses() {
        assert_eq!(
            interpret_probed(Some(vec![
                Response::Sixel,
                Response::CellSize(Some((8, 16))),
                Response::RectangularOps,
                Response::CellSize(Some((10, 20))),
                Response::Status,
            ])),
            (Some(ProtocolType::Sixel), Some((10, 20)))
        );
    }
}
