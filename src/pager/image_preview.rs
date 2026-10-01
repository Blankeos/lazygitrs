//! Before/after image preview for binary diffs.
//!
//! Draws a centered, aspect-preserving image per pane inside the diff panel.
//! The caller owns the outer border/title and hides the preview under
//! overlays via `image_preview_hidden`. Only scoped Kitty deletes are
//! supported here (`ImagePreview::cleanup`); Sixel/iTerm2 have no scoped
//! delete in ratatui-image 8.0.1 and persist once drawn.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use image::{DynamicImage, ImageFormat};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui_image::picker::cap_parser::{Parser, Response};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocolType;
use ratatui_image::thread::{ResizeRequest, ResizeResponse, ThreadProtocol};
use ratatui_image::{Resize, ResizeEncodeRender, StatefulImage};

use super::side_by_side::DiffSideView;
use crate::config::Theme;

const MAX_COMPRESSED_BYTES: usize = 20 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 16384;
const MAX_ALLOC_BYTES: u64 = 64 * 1024 * 1024;
const PREVIEW_MAX_WIDTH: u32 = 1600;
const PREVIEW_MAX_HEIGHT: u32 = 1200;
const PROBE_TIMEOUT: Duration = Duration::from_millis(450);
// Only CSI replies: Kitty/Ghostty are identified by hints, not an APC query.
// Graphics replies can arrive after the status reply and leak into key input.
const CAPABILITY_QUERY: &str = "\x1b[c\x1b[16t\x1b[5n";
const FALLBACK_KITTY_FONT: (u16, u16) = (8, 16);

static GLOBAL_PICKER: OnceLock<Option<Picker>> = OnceLock::new();
static PENDING_DELETES: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

/// Flush scoped deletes on the UI thread, never on decoding/resize workers.
pub fn flush_cleanup(out: &mut dyn Write) {
    if let Ok(mut pending) = PENDING_DELETES.lock() {
        for id in pending.drain(..) {
            let _ = write!(out, "\x1b_Ga=d,d=I,i={id}\x1b\\");
        }
    }
    let _ = out.flush();
}

fn global_picker_ref() -> Option<&'static Picker> {
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

/// Where one side of the before/after preview comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSource<'a> {
    /// Worktree file at this repo-relative path.
    Worktree(&'a str),
    /// `revision:path` via `git cat-file`.
    Revision { revision: &'a str, path: &'a str },
    /// Legitimately absent (added/deleted).
    Missing,
}

/// Decoded + downsized image with its detected format.
#[derive(Debug, Clone)]
pub(crate) struct DecodedImage {
    /// Downsized to `PREVIEW_MAX_*` (aspect preserved).
    pub(crate) image: DynamicImage,
    pub(crate) format: Option<ImageFormat>,
    pub(crate) orig_width: u32,
    pub(crate) orig_height: u32,
}

impl DecodedImage {
    pub(crate) fn width(&self) -> u32 {
        self.image.width()
    }
    pub(crate) fn height(&self) -> u32 {
        self.image.height()
    }
    pub(crate) fn format_label(&self) -> &'static str {
        format_label_for(self.format)
    }
}

pub(crate) fn format_label_for(format: Option<ImageFormat>) -> &'static str {
    match format {
        Some(ImageFormat::Png) => "PNG",
        Some(ImageFormat::Jpeg) => "JPEG",
        Some(ImageFormat::Gif) => "GIF",
        Some(ImageFormat::WebP) => "WEBP",
        Some(ImageFormat::Bmp) => "BMP",
        Some(ImageFormat::Ico) => "ICO",
        Some(ImageFormat::Tiff) => "TIFF",
        Some(_) => "IMG",
        None => "IMG",
    }
}

/// Decode bounded bytes, then downsize for preview. Returns `None` for empty,
/// oversize, unsupported, or over-limit inputs.
pub(crate) fn decode_bytes(bytes: &[u8]) -> Option<DecodedImage> {
    if bytes.is_empty() || bytes.len() > MAX_COMPRESSED_BYTES {
        return None;
    }
    let cursor = std::io::Cursor::new(bytes);
    let mut reader = image::ImageReader::new(cursor).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_ALLOC_BYTES);
    reader.limits(limits);
    let format = reader.format();
    let decoded = reader.decode().ok()?;
    let (ow, oh) = (decoded.width(), decoded.height());
    if ow == 0 || oh == 0 || ow > MAX_IMAGE_DIMENSION || oh > MAX_IMAGE_DIMENSION {
        return None;
    }
    let image = if ow > PREVIEW_MAX_WIDTH || oh > PREVIEW_MAX_HEIGHT {
        decoded.thumbnail(PREVIEW_MAX_WIDTH, PREVIEW_MAX_HEIGHT)
    } else {
        decoded
    };
    Some(DecodedImage {
        image,
        format,
        orig_width: ow,
        orig_height: oh,
    })
}

enum RevisionRead {
    Absent,
    Present(Vec<u8>),
    Failed,
}

fn git_env_no_interactive(cmd: &mut std::process::Command) {
    cmd.env("GIT_OPTIONAL_LOCKS", "0");
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("SSH_ASKPASS_REQUIRE", "never");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    }
}

fn git_blob_exists(cwd: &Path, spec: &str) -> Option<bool> {
    use std::process::Stdio;
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(cwd)
        .args(["cat-file", "-e", spec])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    git_env_no_interactive(&mut cmd);
    match cmd.status() {
        Ok(s) => Some(s.success()),
        Err(_) => None,
    }
}

/// Bounded `git cat-file -p` read. Kills the child when the cap is exceeded
/// so a large blob cannot deadlock on a full pipe.
fn read_revision_blob(cwd: &Path, revision: &str, path: &str) -> RevisionRead {
    use std::process::Stdio;
    if path.is_empty() {
        return RevisionRead::Failed;
    }
    if revision.contains('\0') || path.contains('\0') {
        return RevisionRead::Failed;
    }
    let spec = format!("{revision}:{path}");
    match git_blob_exists(cwd, &spec) {
        Some(false) => return RevisionRead::Absent,
        None => return RevisionRead::Failed,
        Some(true) => {}
    }
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(cwd)
        .args(["cat-file", "-p", &spec])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped());
    git_env_no_interactive(&mut cmd);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return RevisionRead::Failed,
    };
    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return RevisionRead::Failed;
        }
    };
    let mut limited = stdout.take((MAX_COMPRESSED_BYTES as u64) + 1);
    let mut buf = Vec::with_capacity(8192.min(MAX_COMPRESSED_BYTES));
    if limited.read_to_end(&mut buf).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return RevisionRead::Failed;
    }
    if buf.len() > MAX_COMPRESSED_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        return RevisionRead::Failed;
    }
    match child.wait() {
        Ok(s) if s.success() => RevisionRead::Present(buf),
        _ => {
            if buf.is_empty() {
                RevisionRead::Absent
            } else {
                RevisionRead::Failed
            }
        }
    }
}

/// Bounded worktree read. `Ok(None)` = legitimately missing; `Err(())` =
/// present but unreadable/oversize.
fn read_worktree_bytes(repo_path: &Path, rel: &str) -> Result<Option<Vec<u8>>, ()> {
    if rel.is_empty() || rel.contains('\0') {
        return Err(());
    }
    let full = repo_path.join(rel);
    let file = match std::fs::File::open(&full) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    if file.metadata().is_ok_and(|m| m.is_dir()) {
        return Ok(None);
    }
    let mut limited = file.take((MAX_COMPRESSED_BYTES as u64) + 1);
    let mut buf = Vec::new();
    if limited.read_to_end(&mut buf).is_err() {
        return Err(());
    }
    if buf.len() > MAX_COMPRESSED_BYTES {
        return Err(());
    }
    Ok(Some(buf))
}

struct PreviewSide {
    width: u32,
    height: u32,
    orig_width: u32,
    orig_height: u32,
    format: Option<ImageFormat>,
    thread: ThreadProtocol,
    rx: Receiver<ResizeResponse>,
    created: Instant,
    // Stored before the protocol moves into the worker so scoped Kitty
    // deletes stay possible while a resize is inflight.
    kitty_id: Option<u32>,
}

impl PreviewSide {
    fn new(picker: &Picker, decoded: DecodedImage) -> Self {
        let (tx_req, rx_req) = mpsc::channel::<ResizeRequest>();
        let (tx_resp, rx_resp) = mpsc::channel::<ResizeResponse>();
        // Move the image into the protocol: no retained clone, so memory and
        // cache accounting stay single-copy.
        let width = decoded.image.width();
        let height = decoded.image.height();
        let orig_width = decoded.orig_width;
        let orig_height = decoded.orig_height;
        let format = decoded.format;
        let proto = picker.new_resize_protocol(decoded.image);
        let kitty_id = match proto.protocol_type() {
            StatefulProtocolType::Kitty(k) => Some(k.unique_id),
            _ => None,
        };
        let thread = ThreadProtocol::new(tx_req, Some(proto));
        // Worker only resize+encodes (CPU); it never touches the terminal.
        // It exits when the preview is dropped (sender disconnect).
        std::thread::Builder::new()
            .name("lazygitrs-image-preview".into())
            .spawn(move || {
                while let Ok(req) = rx_req.recv() {
                    match req.resize_encode() {
                        Ok(resp) => {
                            let _ = tx_resp.send(resp);
                        }
                        Err(_) => {
                            // Encode failed; the protocol is consumed by the
                            // ratatui-image API. The UI shows a fallback
                            // message instead of hanging (see `render_side`).
                        }
                    }
                }
            })
            .ok();
        Self {
            width,
            height,
            orig_width,
            orig_height,
            format,
            thread,
            rx: rx_resp,
            created: Instant::now(),
            kitty_id,
        }
    }

    fn poll_completed(&mut self) {
        while let Ok(resp) = self.rx.try_recv() {
            let _ = self.thread.update_resized_protocol(resp);
        }
    }

    fn estimated_bytes(&self) -> usize {
        // Only the thumbnail and its encoded payload are retained.
        let dec = (self.width as usize) * (self.height as usize) * 4;
        dec + dec * 4 / 3 + 1024
    }

    fn format_label(&self) -> &'static str {
        format_label_for(self.format)
    }
}

/// Before/after image preview. `Send` so the diff loader can move it across
/// threads; rendering stays on the UI thread. Drop queues scoped deletes;
/// the UI flushes them without any worker performing terminal I/O.
pub struct ImagePreview {
    old: Option<PreviewSide>,
    new: Option<PreviewSide>,
    font_size: (u16, u16),
}

impl Drop for ImagePreview {
    fn drop(&mut self) {
        if let Ok(mut pending) = PENDING_DELETES.lock() {
            pending.extend(
                [&self.old, &self.new]
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.kitty_id),
            );
        }
    }
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<ImagePreview>();
};

impl ImagePreview {
    /// Load both sides with the global picker. `None` when graphics are
    /// unavailable, both sides are absent, or any present side fails.
    pub fn load(
        git: &crate::git::GitCommands,
        old: ImageSource<'_>,
        new: ImageSource<'_>,
    ) -> Option<Self> {
        let picker = global_picker_ref().cloned()?;
        Self::load_with_picker(git, old, new, &picker)
    }

    /// Picker-injecting load (tests use this, never the global).
    pub(crate) fn load_with_picker(
        git: &crate::git::GitCommands,
        old: ImageSource<'_>,
        new: ImageSource<'_>,
        picker: &Picker,
    ) -> Option<Self> {
        if picker.protocol_type() == ProtocolType::Halfblocks {
            return None;
        }
        let repo = git.repo_path();
        let old_dec = load_one_side(repo, old)?;
        let new_dec = load_one_side(repo, new)?;
        Self::from_decoded(old_dec, new_dec, picker)
    }

    /// Build from already-decoded sides. `None` when both are `None` or the
    /// picker is Halfblocks.
    pub(crate) fn from_decoded(
        old: Option<DecodedImage>,
        new: Option<DecodedImage>,
        picker: &Picker,
    ) -> Option<Self> {
        if picker.protocol_type() == ProtocolType::Halfblocks {
            return None;
        }
        if old.is_none() && new.is_none() {
            return None;
        }
        let font_size = picker.font_size();
        Some(Self {
            old: old.map(|d| PreviewSide::new(picker, d)),
            new: new.map(|d| PreviewSide::new(picker, d)),
            font_size,
        })
    }

    /// Approx memory held: decoded + encoded estimates for both sides.
    pub fn estimated_bytes(&self) -> usize {
        let mut n = 0usize;
        if let Some(s) = &self.old {
            n += s.estimated_bytes();
        }
        if let Some(s) = &self.new {
            n += s.estimated_bytes();
        }
        n + std::mem::size_of::<Self>()
    }

    pub fn uses_placeholders(&self) -> bool {
        [&self.old, &self.new]
            .into_iter()
            .flatten()
            .all(|side| side.kitty_id.is_some())
    }

    /// Scoped cleanup: deletes only this preview's Kitty image ids. Safe on
    /// any output; non-Kitty sides are ignored. Never issues a global
    /// delete-all. Call on the UI thread (never from a worker).
    #[cfg(test)]
    fn cleanup(&self, out: &mut dyn Write) {
        for id in [&self.old, &self.new]
            .into_iter()
            .flatten()
            .filter_map(|s| s.kitty_id)
        {
            let _ = write!(out, "\x1b_Ga=d,d=I,i={id}\x1b\\");
        }
        let _ = out.flush();
    }
}

/// Free-function load (forwards to [`ImagePreview::load`]).
pub fn load(
    git: &crate::git::GitCommands,
    old: ImageSource<'_>,
    new: ImageSource<'_>,
) -> Option<ImagePreview> {
    ImagePreview::load(git, old, new)
}

fn load_one_side(repo: &Path, src: ImageSource<'_>) -> Option<Option<DecodedImage>> {
    match src {
        ImageSource::Missing => Some(None),
        ImageSource::Worktree(rel) => match read_worktree_bytes(repo, rel) {
            Ok(None) => Some(None),
            Ok(Some(bytes)) => {
                if bytes.is_empty() {
                    return None;
                }
                Some(Some(decode_bytes(&bytes)?))
            }
            Err(()) => None,
        },
        ImageSource::Revision { revision, path } => {
            match read_revision_blob(repo, revision, path) {
                RevisionRead::Absent => Some(None),
                RevisionRead::Present(bytes) => {
                    if bytes.is_empty() {
                        return None;
                    }
                    Some(Some(decode_bytes(&bytes)?))
                }
                RevisionRead::Failed => None,
            }
        }
    }
}

/// Split `area` into `(old_pane, new_pane)` honoring `side`.
pub(crate) fn split_preview_area(area: Rect, side: DiffSideView) -> (Option<Rect>, Option<Rect>) {
    if area.width == 0 || area.height == 0 {
        return (None, None);
    }
    match side {
        DiffSideView::OldOnly => (Some(area), None),
        DiffSideView::NewOnly => (None, Some(area)),
        DiffSideView::Both => {
            if area.width < 2 {
                return (Some(area), None);
            }
            let left_w = area.width / 2;
            let right_w = area.width - left_w;
            (
                Some(Rect::new(area.x, area.y, left_w, area.height)),
                Some(Rect::new(area.x + left_w, area.y, right_w, area.height)),
            )
        }
    }
}

/// Center `inner` inside `outer`.
pub(crate) fn centered_rect(outer: Rect, inner_w: u16, inner_h: u16) -> Rect {
    let w = inner_w.min(outer.width);
    let h = inner_h.min(outer.height);
    let x = outer.x + outer.width.saturating_sub(w) / 2;
    let y = outer.y + outer.height.saturating_sub(h) / 2;
    Rect::new(x, y, w, h)
}

/// Cell-space fit preserving aspect, given decoded pixels + font size.
pub(crate) fn fit_cells(
    img_w: u32,
    img_h: u32,
    font: (u16, u16),
    pane_w: u16,
    pane_h: u16,
) -> (u16, u16) {
    if img_w == 0 || img_h == 0 || font.0 == 0 || font.1 == 0 || pane_w == 0 || pane_h == 0 {
        return (pane_w.min(1), pane_h.min(1));
    }
    let max_px_w = pane_w as u64 * font.0 as u64;
    let max_px_h = pane_h as u64 * font.1 as u64;
    let scale = (max_px_w as f64 / img_w as f64).min(max_px_h as f64 / img_h as f64);
    let scale = scale.min(1.0);
    let fit_px_w = ((img_w as f64 * scale).round() as u64).max(1);
    let fit_px_h = ((img_h as f64 * scale).round() as u64).max(1);
    let cells_w = ((fit_px_w + font.0 as u64 - 1) / font.0 as u64).min(pane_w as u64) as u16;
    let cells_h = ((fit_px_h + font.1 as u64 - 1) / font.1 as u64).min(pane_h as u64) as u16;
    (cells_w.max(1), cells_h.max(1))
}

/// Draw the image body (outer border/title owned by the caller).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    preview: &mut ImagePreview,
    side: DiffSideView,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if let Some(s) = preview.old.as_mut() {
        s.poll_completed();
    }
    if let Some(s) = preview.new.as_mut() {
        s.poll_completed();
    }

    let added = preview.old.is_none() && preview.new.is_some();
    let deleted = preview.new.is_none() && preview.old.is_some();
    // A missing version isn't a comparison: let the available image use the
    // full pane, just like added text files. Explicit side controls still win.
    let effective_side = match (side, added, deleted) {
        (DiffSideView::Both, true, _) => DiffSideView::NewOnly,
        (DiffSideView::Both, _, true) => DiffSideView::OldOnly,
        _ => side,
    };
    let (old_area, new_area) = split_preview_area(area, effective_side);
    if let Some(a) = old_area {
        render_side(
            frame,
            a,
            preview.old.as_mut(),
            true,
            if deleted { "Deleted" } else { "Before" },
            preview.font_size,
            theme,
        );
    }
    if let Some(a) = new_area {
        render_side(
            frame,
            a,
            preview.new.as_mut(),
            false,
            if added { "Added" } else { "After" },
            preview.font_size,
            theme,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_side(
    frame: &mut Frame,
    pane: Rect,
    side: Option<&mut PreviewSide>,
    is_old: bool,
    title: &str,
    font_size: (u16, u16),
    theme: &Theme,
) {
    if pane.width == 0 || pane.height == 0 {
        return;
    }

    let Some(state) = side else {
        let status = if is_old { "Added" } else { "Deleted" };
        let header = Line::from(vec![
            Span::styled(format!("{title} "), Style::default().fg(theme.text_strong)),
            Span::styled("Missing", Style::default().fg(theme.text_dimmed)),
            Span::styled(
                format!(" · {status}"),
                Style::default().fg(theme.accent_secondary),
            ),
        ]);
        if pane.height >= 1 {
            frame.render_widget(
                Paragraph::new(header),
                Rect::new(pane.x, pane.y, pane.width, 1),
            );
        }
        if pane.height >= 3 {
            let msg = Paragraph::new(Line::from(Span::styled(
                "—",
                Style::default().fg(theme.text_dimmed),
            )));
            let centered = Rect::new(
                pane.x,
                pane.y + pane.height / 2,
                pane.width,
                1.min(pane.height.saturating_sub(1)),
            );
            frame.render_widget(msg, centered);
        }
        return;
    };

    let dims = format!("{}x{}", state.orig_width, state.orig_height);
    let fmt = state.format_label();
    let header = Line::from(vec![
        Span::styled(format!("{title} "), Style::default().fg(theme.text_strong)),
        Span::styled(dims, Style::default().fg(theme.text)),
        Span::styled(format!(" · {fmt}"), Style::default().fg(theme.text_dimmed)),
    ]);
    let header_h: u16 = 1;
    if pane.height <= header_h {
        frame.render_widget(Paragraph::new(header), pane);
        return;
    }
    frame.render_widget(
        Paragraph::new(header),
        Rect::new(pane.x, pane.y, pane.width, header_h),
    );
    let img_area = Rect::new(
        pane.x,
        pane.y + header_h,
        pane.width,
        pane.height.saturating_sub(header_h),
    );
    if img_area.width == 0 || img_area.height == 0 {
        return;
    }

    let fitted: Option<Rect> = state
        .thread
        .size_for(Resize::Fit(None), img_area)
        .map(|r| centered_rect(img_area, r.width, r.height))
        .or_else(|| {
            let (cw, ch) = fit_cells(
                state.width,
                state.height,
                font_size,
                img_area.width,
                img_area.height,
            );
            Some(centered_rect(img_area, cw, ch))
        });
    let Some(dst) = fitted else {
        return;
    };
    if dst.width == 0 || dst.height == 0 {
        return;
    }

    // Pending first encode: brief Loading, then a bounded fallback message.
    // Encode failures drop the protocol (ratatui-image API), so this never
    // blocks; it just shows the message.
    let pending = state.thread.protocol_type().is_none();
    if pending {
        let waiting = state.created.elapsed();
        let label = if waiting > Duration::from_secs(2) {
            "Preview unavailable"
        } else {
            "Loading…"
        };
        let w = (label.chars().count() as u16).min(dst.width).max(1);
        let msg_area = centered_rect(dst, w, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                label,
                Style::default().fg(theme.text_dimmed),
            ))),
            msg_area,
        );
        state.thread.resize_encode(&Resize::Fit(None), dst);
        frame.render_stateful_widget(
            StatefulImage::new().resize(Resize::Fit(None)),
            dst,
            &mut state.thread,
        );
        return;
    }

    frame.render_stateful_widget(
        StatefulImage::new().resize(Resize::Fit(None)),
        dst,
        &mut state.thread,
    );
}

/// Fixed scroll height of an inline image section, including labels.
pub const INLINE_IMAGE_ROWS: usize = 12;
const MAX_INLINE_LOADS: usize = 2;
static INLINE_LOADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Owned source identity; delayed loads must not borrow a diff job's buffers.
#[derive(Clone)]
pub(crate) enum InlineImageSource {
    Worktree(String),
    Revision { revision: String, path: String },
    Missing,
}

impl InlineImageSource {
    fn borrowed(&self) -> ImageSource<'_> {
        match self {
            Self::Worktree(path) => ImageSource::Worktree(path),
            Self::Revision { revision, path } => ImageSource::Revision { revision, path },
            Self::Missing => ImageSource::Missing,
        }
    }
}

enum InlineLoadState {
    Idle,
    Loading(Receiver<Option<ImagePreview>>),
    Ready(ImagePreview),
    Unavailable,
}

/// Lazy, viewport-driven image state for one file in a multi-file diff.
/// Offscreen sections keep only source metadata; at most two decodes run at once.
pub struct InlineImagePreview {
    repo: std::path::PathBuf,
    old: InlineImageSource,
    new: InlineImageSource,
    picker: Picker,
    state: InlineLoadState,
}

impl InlineImagePreview {
    pub(crate) fn new(repo: &Path, old: InlineImageSource, new: InlineImageSource) -> Option<Self> {
        let picker = global_picker_ref()?.clone();
        // Inline overlays need placeholder semantics to clear individual rows
        // without erasing adjacent text. Experimental Sixel/iTerm2 stay striped.
        if picker.protocol_type() != ProtocolType::Kitty {
            return None;
        }
        Some(Self::with_picker(repo, old, new, picker))
    }

    fn with_picker(
        repo: &Path,
        old: InlineImageSource,
        new: InlineImageSource,
        picker: Picker,
    ) -> Self {
        Self {
            repo: repo.to_path_buf(),
            old,
            new,
            picker,
            state: InlineLoadState::Idle,
        }
    }

    /// Release pixel/protocol memory when this section leaves the viewport.
    pub fn unload(&mut self) {
        if let InlineLoadState::Loading(rx) = &self.state {
            match rx.try_recv() {
                Err(mpsc::TryRecvError::Empty) => return, // Keep a pending job; don't duplicate it.
                Ok(None) => {
                    self.state = InlineLoadState::Unavailable;
                    return;
                }
                _ => {} // Discard offscreen pixels, including a completed result.
            }
        }
        if !matches!(self.state, InlineLoadState::Unavailable) {
            self.state = InlineLoadState::Idle;
        }
    }

    /// Cached folders retain identities, not image pixels or in-flight results.
    pub fn unload_for_cache(&mut self) {
        if !matches!(self.state, InlineLoadState::Unavailable) {
            self.state = InlineLoadState::Idle;
        }
    }

    #[cfg(test)]
    pub(crate) fn from_preview_for_test(preview: ImagePreview) -> Self {
        let mut picker = Picker::from_fontsize((8, 16));
        picker.set_protocol_type(ProtocolType::Kitty);
        let mut inline = Self::with_picker(
            Path::new("."),
            InlineImageSource::Missing,
            InlineImageSource::Missing,
            picker,
        );
        inline.state = InlineLoadState::Ready(preview);
        inline
    }

    pub fn estimated_bytes(&self) -> usize {
        let pixels = match &self.state {
            InlineLoadState::Ready(preview) => preview.estimated_bytes(),
            _ => 0,
        };
        pixels
            + std::mem::size_of::<Self>()
            + self.repo.as_os_str().len()
            + source_bytes(&self.old)
            + source_bytes(&self.new)
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, side: DiffSideView, theme: &Theme) {
        if let InlineLoadState::Loading(rx) = &self.state {
            match rx.try_recv() {
                Ok(Some(preview)) => self.state = InlineLoadState::Ready(preview),
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.state = InlineLoadState::Unavailable
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if matches!(self.state, InlineLoadState::Idle) && area.height > 1 {
            self.start_load();
        }
        match &mut self.state {
            InlineLoadState::Ready(preview) => render(frame, area, preview, side, theme),
            InlineLoadState::Unavailable => {
                inline_placeholder(frame, area, "Binary cannot be previewed", theme)
            }
            _ => inline_placeholder(frame, area, "Loading image…", theme),
        }
    }

    fn start_load(&mut self) {
        use std::sync::atomic::Ordering;
        if INLINE_LOADS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_INLINE_LOADS).then_some(n + 1)
            })
            .is_err()
        {
            return;
        }
        struct LoadPermit;
        impl Drop for LoadPermit {
            fn drop(&mut self) {
                INLINE_LOADS.fetch_sub(1, Ordering::AcqRel);
            }
        }
        let permit = LoadPermit;
        let repo = self.repo.clone();
        let old = self.old.clone();
        let new = self.new.clone();
        let picker = self.picker.clone();
        let (tx, rx) = mpsc::channel();
        match std::thread::Builder::new()
            .name("inline-image-load".into())
            .spawn(move || {
                let _permit = permit;
                let preview = (|| {
                    let old = load_one_side(&repo, old.borrowed())?;
                    let new = load_one_side(&repo, new.borrowed())?;
                    ImagePreview::from_decoded(old, new, &picker)
                })();
                let _ = tx.send(preview);
            }) {
            Ok(_) => self.state = InlineLoadState::Loading(rx),
            Err(_) => self.state = InlineLoadState::Unavailable,
        }
    }
}

fn source_bytes(source: &InlineImageSource) -> usize {
    match source {
        InlineImageSource::Worktree(path) => path.len(),
        InlineImageSource::Revision { revision, path } => revision.len() + path.len(),
        InlineImageSource::Missing => 0,
    }
}

fn inline_placeholder(frame: &mut Frame, area: Rect, message: &str, theme: &Theme) {
    let style = Style::default().fg(theme.text_dimmed);
    frame.render_widget(
        Paragraph::new("╱".repeat(area.width as usize)).style(style),
        area,
    );
    if area.height > 0 {
        let label = Rect::new(area.x, area.y + area.height / 2, area.width, 1);
        frame.render_widget(
            Paragraph::new(message)
                .alignment(ratatui::layout::Alignment::Center)
                .style(style),
            label,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    #[test]
    fn startup_query_never_requests_a_graphics_reply() {
        assert_eq!(CAPABILITY_QUERY, "\x1b[c\x1b[16t\x1b[5n");
        assert!(!CAPABILITY_QUERY.contains("\x1b_G"));
        assert!(!CAPABILITY_QUERY.contains("i=31"));
    }

    fn halfblocks_picker() -> Picker {
        let mut p = Picker::from_fontsize((8, 16));
        p.set_protocol_type(ProtocolType::Halfblocks);
        p
    }

    fn graphics_picker() -> Picker {
        let mut p = Picker::from_fontsize((8, 16));
        p.set_protocol_type(ProtocolType::Kitty);
        p
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            w,
            h,
            image::Rgb([200, 30, 30]),
        ));
        let mut buf = Vec::new();
        let mut cur = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cur, ImageFormat::Png).unwrap();
        buf
    }

    fn jpeg_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            w,
            h,
            image::Rgb([30, 200, 30]),
        ));
        let mut buf = Vec::new();
        let mut cur = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cur, ImageFormat::Jpeg).unwrap();
        buf
    }

    fn gif_bytes() -> Vec<u8> {
        vec![
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xff, 0xff, 0xff, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2c,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00,
            0x3b,
        ]
    }

    #[test]
    fn decode_png_detects_type_and_dims() {
        let b = png_bytes(4, 3);
        let d = decode_bytes(&b).expect("png decodes");
        assert_eq!((d.width(), d.height()), (4, 3));
        assert_eq!(d.format, Some(ImageFormat::Png));
        assert_eq!(d.format_label(), "PNG");
    }

    #[test]
    fn decode_jpeg_detects_type() {
        let b = jpeg_bytes(6, 5);
        let d = decode_bytes(&b).expect("jpeg decodes");
        assert_eq!(d.format, Some(ImageFormat::Jpeg));
        assert_eq!(d.format_label(), "JPEG");
    }

    #[test]
    fn decode_gif_detects_type() {
        let d = decode_bytes(&gif_bytes()).expect("gif decodes");
        assert_eq!(d.format, Some(ImageFormat::Gif));
        assert_eq!(d.format_label(), "GIF");
    }

    #[test]
    fn decode_garbage_returns_none() {
        assert!(decode_bytes(b"not an image at all").is_none());
        assert!(decode_bytes(&[]).is_none());
        assert!(decode_bytes(&[0u8; 16]).is_none());
    }

    #[test]
    fn decode_rejects_oversize_compressed() {
        let big = vec![0u8; MAX_COMPRESSED_BYTES + 1];
        assert!(decode_bytes(&big).is_none());
    }

    #[test]
    fn downsize_preserves_aspect_within_box() {
        let b = png_bytes(3000, 2000);
        let d = decode_bytes(&b).expect("large png decodes");
        assert_eq!((d.orig_width, d.orig_height), (3000, 2000));
        assert!(d.width() <= PREVIEW_MAX_WIDTH && d.height() <= PREVIEW_MAX_HEIGHT);
        let r = d.width() as f64 / d.height() as f64;
        assert!((r - 1.5).abs() < 0.05, "aspect kept, got {r}");
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let b = png_bytes(8, 6);
        let d = decode_bytes(&b).unwrap();
        assert_eq!((d.width(), d.height()), (8, 6));
    }

    #[test]
    fn format_labels_cover_contract() {
        assert_eq!(format_label_for(Some(ImageFormat::Png)), "PNG");
        assert_eq!(format_label_for(Some(ImageFormat::Jpeg)), "JPEG");
        assert_eq!(format_label_for(Some(ImageFormat::Gif)), "GIF");
        assert_eq!(format_label_for(Some(ImageFormat::WebP)), "WEBP");
        assert_eq!(format_label_for(Some(ImageFormat::Bmp)), "BMP");
        assert_eq!(format_label_for(Some(ImageFormat::Ico)), "ICO");
        assert_eq!(format_label_for(Some(ImageFormat::Tiff)), "TIFF");
        assert_eq!(format_label_for(None), "IMG");
    }

    #[test]
    fn split_layout_honors_side_view() {
        let area = Rect::new(0, 0, 100, 20);
        let (o, n) = split_preview_area(area, DiffSideView::Both);
        let (o, n) = (o.unwrap(), n.unwrap());
        assert_eq!(o.width + n.width, 100);
        assert_eq!((o.x, o.y, o.height), (0, 0, 20));
        assert_eq!((n.x, n.y, n.height), (50, 0, 20));

        let (o, n) = split_preview_area(area, DiffSideView::OldOnly);
        assert!(o.is_some() && n.is_none());
        let (o, n) = split_preview_area(area, DiffSideView::NewOnly);
        assert!(o.is_none() && n.is_some());

        let empty = Rect::new(0, 0, 0, 0);
        assert_eq!(split_preview_area(empty, DiffSideView::Both), (None, None));
    }

    #[test]
    fn centered_rect_is_centered_and_clamped() {
        let outer = Rect::new(10, 5, 20, 10);
        let c = centered_rect(outer, 8, 4);
        assert_eq!((c.x, c.y, c.width, c.height), (16, 8, 8, 4));
        let big = centered_rect(outer, 100, 100);
        assert_eq!((big.x, big.y, big.width, big.height), (10, 5, 20, 10));
    }

    #[test]
    fn fit_cells_preserves_aspect_and_clamps() {
        let (w, h) = fit_cells(160, 120, (8, 16), 20, 10);
        assert!(w <= 20 && h <= 10 && w >= 1 && h >= 1);
        let (w2, h2) = fit_cells(8, 8, (8, 16), 40, 20);
        assert_eq!((w2, h2), (1, 1));
        assert_eq!(fit_cells(0, 0, (8, 16), 10, 10), (1, 1));
    }

    #[test]
    fn from_decoded_needs_picker_and_a_side() {
        let proto = graphics_picker();
        let d = decode_bytes(&png_bytes(4, 4)).unwrap();
        assert!(ImagePreview::from_decoded(None, None, &proto).is_none());
        assert!(ImagePreview::from_decoded(Some(d.clone()), None, &proto).is_some());
        assert!(ImagePreview::from_decoded(None, Some(d), &proto).is_some());
        let half = halfblocks_picker();
        let d2 = decode_bytes(&png_bytes(4, 4)).unwrap();
        assert!(ImagePreview::from_decoded(Some(d2), None, &half).is_none());
    }

    #[test]
    fn estimated_bytes_counts_decoded_plus_encoded() {
        let proto = graphics_picker();
        let d = decode_bytes(&png_bytes(8, 8)).unwrap();
        let p = ImagePreview::from_decoded(Some(d), None, &proto).unwrap();
        let dec = 8usize * 8 * 4;
        let est = p.estimated_bytes();
        assert!(est >= dec + dec * 4 / 3, "est {est} covers decoded+encoded");
        assert!(est < dec * 10 + 8192, "est {est} stays plausible");
        let d2 = decode_bytes(&png_bytes(8, 8)).unwrap();
        let d3 = decode_bytes(&jpeg_bytes(8, 8)).unwrap();
        let p2 = ImagePreview::from_decoded(Some(d2), Some(d3), &proto).unwrap();
        assert!(p2.estimated_bytes() > est);
    }

    #[test]
    fn kitty_id_stored_for_inflight_cleanup() {
        let picker = graphics_picker();
        let d = decode_bytes(&png_bytes(8, 8)).unwrap();
        let p = ImagePreview::from_decoded(Some(d), None, &picker).unwrap();
        // Stored id is available even before any encode completes.
        let id = p.old.as_ref().unwrap().kitty_id.expect("kitty id stored");
        let mut buf = Vec::new();
        p.cleanup(&mut buf);
        let out = String::from_utf8(buf).expect("cleanup writes ascii");
        assert!(out.contains("\x1b_Ga=d,d=I"), "scoped delete, got {out:?}");
        assert!(out.contains(&format!("i={id}")), "own id only, got {out:?}");
    }

    #[test]
    fn image_source_variants_exist() {
        let a = ImageSource::Worktree("a.png");
        let b = ImageSource::Revision {
            revision: "HEAD",
            path: "a.png",
        };
        let c = ImageSource::Missing;
        assert_ne!(a, b);
        assert_ne!(b, c);
    }

    #[test]
    fn render_halfblocks_body_does_not_panic() {
        let picker = halfblocks_picker();
        let img = decode_bytes(&png_bytes(16, 16)).unwrap().image;
        let mut proto = picker.new_resize_protocol(img);
        let backend = TestBackend::new(40, 12);
        let mut term = Terminal::new(backend).expect("test terminal");
        term.draw(|f| {
            let area = f.area();
            f.render_stateful_widget(
                StatefulImage::new().resize(Resize::Fit(None)),
                area,
                &mut proto,
            );
        })
        .unwrap();
    }

    #[test]
    fn render_preview_body_splits_and_labels() {
        let picker = graphics_picker();
        let old = decode_bytes(&png_bytes(16, 16)).unwrap();
        let new = decode_bytes(&jpeg_bytes(16, 16)).unwrap();
        let mut preview = ImagePreview::from_decoded(Some(old), Some(new), &picker).unwrap();
        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).expect("test terminal");
        term.draw(|f| {
            render(f, f.area(), &mut preview, DiffSideView::Both, &theme);
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(text.contains("Before"), "has Before label");
        assert!(text.contains("After"), "has After label");
        assert!(text.contains("PNG") || text.contains("JPEG"));

        term.draw(|f| {
            render(f, f.area(), &mut preview, DiffSideView::OldOnly, &theme);
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        let text: String = buf
            .content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect();
        assert!(text.contains("Before"));
        assert!(!text.contains("After"));
    }

    #[test]
    fn added_and_deleted_images_use_a_single_full_width_pane() {
        for added in [true, false] {
            let picker = graphics_picker();
            let image = decode_bytes(&png_bytes(8, 8)).unwrap();
            let (old, new) = if added {
                (None, Some(image))
            } else {
                (Some(image), None)
            };
            let mut preview = ImagePreview::from_decoded(old, new, &picker).unwrap();
            let theme = Theme::default();
            let mut term = Terminal::new(TestBackend::new(80, 12)).unwrap();
            term.draw(|f| render(f, f.area(), &mut preview, DiffSideView::Both, &theme))
                .unwrap();
            let text = buffer_text(term.backend().buffer());
            assert!(text.starts_with(if added { "Added " } else { "Deleted " }));
            assert!(!text.contains("Missing"));
            assert!(!text.contains("Before"));
            assert!(!text.contains("After"));

            // An explicit request for the absent side is still respected.
            let absent = if added {
                DiffSideView::OldOnly
            } else {
                DiffSideView::NewOnly
            };
            term.draw(|f| render(f, f.area(), &mut preview, absent, &theme))
                .unwrap();
            assert!(buffer_text(term.backend().buffer()).contains("Missing"));
        }
    }

    #[test]
    fn tiny_area_never_panics() {
        let picker = graphics_picker();
        let d = decode_bytes(&png_bytes(8, 8)).unwrap();
        let mut preview = ImagePreview::from_decoded(Some(d.clone()), Some(d), &picker).unwrap();
        let theme = Theme::default();
        for (w, h) in [(0, 0), (1, 1), (2, 1), (1, 2), (3, 3)] {
            let backend = TestBackend::new(w, h);
            let mut term = Terminal::new(backend).expect("test terminal");
            term.draw(|f| {
                render(f, f.area(), &mut preview, DiffSideView::Both, &theme);
            })
            .unwrap();
        }
    }

    #[test]
    fn preview_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ImagePreview>();
    }

    // ------------------------------------------------------------------
    // Regression helpers: isolated temp git fixtures, no global mutation.
    // ------------------------------------------------------------------

    fn iterm2_picker() -> Picker {
        let mut p = Picker::from_fontsize((8, 16));
        p.set_protocol_type(ProtocolType::Iterm2);
        p
    }

    fn colored_png(rgb: [u8; 3], w: u32, h: u32) -> Vec<u8> {
        let img = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(w, h, image::Rgb(rgb)));
        let mut buf = Vec::new();
        let mut cur = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cur, ImageFormat::Png).unwrap();
        buf
    }

    fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
        buf.content()
            .iter()
            .map(|c| c.symbol().chars().next().unwrap_or(' '))
            .collect()
    }

    fn git_cmd(repo: &Path, args: &[&str]) {
        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(repo).args(args);
        // Isolated, non-interactive reads/writes; per-command env only,
        // never mutates the test process environment.
        cmd.env("GIT_OPTIONAL_LOCKS", "0");
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        cmd.env("SSH_ASKPASS_REQUIRE", "never");
        if std::env::var_os("GIT_SSH_COMMAND").is_none() {
            cmd.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
        }
        cmd.env_remove("GIT_DIR");
        cmd.env_remove("GIT_WORK_TREE");
        cmd.env_remove("GIT_INDEX_FILE");
        cmd.env_remove("GIT_PREFIX");
        let status = cmd.status().expect("run git");
        assert!(
            status.success(),
            "git {args:?} failed in {}",
            repo.display()
        );
    }

    fn git_output(repo: &Path, args: &[&str]) -> String {
        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(repo).args(args);
        cmd.env("GIT_OPTIONAL_LOCKS", "0");
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        cmd.env("SSH_ASKPASS_REQUIRE", "never");
        if std::env::var_os("GIT_SSH_COMMAND").is_none() {
            cmd.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
        }
        cmd.env_remove("GIT_DIR");
        cmd.env_remove("GIT_WORK_TREE");
        cmd.env_remove("GIT_INDEX_FILE");
        cmd.env_remove("GIT_PREFIX");
        let out = cmd.output().expect("git output");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    struct TempRepo {
        path: PathBuf,
    }

    impl TempRepo {
        fn new(prefix: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "lazygitrs-img-{prefix}-{}-{nanos}-{n}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("create temp dir");
            git_cmd(&path, &["init", "-q"]);
            Self { path }
        }

        fn git(&self, args: &[&str]) {
            git_cmd(&self.path, args);
        }

        fn commit(&self, msg: &str) {
            self.git(&[
                "-c",
                "user.email=test@example.com",
                "-c",
                "user.name=Test",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                msg,
            ]);
        }

        fn write(&self, rel: &str, bytes: &[u8]) {
            let full = self.path.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("mkdir parent");
            }
            std::fs::write(&full, bytes).expect("write fixture");
        }

        fn git_commands(&self) -> crate::git::GitCommands {
            crate::git::GitCommands::new(&self.path).expect("GitCommands")
        }

        fn repo_path(&self) -> PathBuf {
            self.git_commands().repo_path().to_path_buf()
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn head_index_worktree_preserve_raw_bytes() {
        let repo = TempRepo::new("raw-bytes");
        let red = colored_png([220, 30, 30], 16, 16);
        let green = colored_png([30, 220, 30], 16, 16);
        let blue = colored_png([30, 30, 220], 16, 16);
        assert_ne!(red, green);
        assert_ne!(green, blue);

        // HEAD = red.
        repo.write("img.png", &red);
        repo.git(&["add", "img.png"]);
        repo.commit("head red");

        // Index = green (staged), worktree = blue (unstaged).
        repo.write("img.png", &green);
        repo.git(&["add", "img.png"]);
        repo.write("img.png", &blue);

        let repo_path = repo.repo_path();
        // Raw bytes preserved per location via private helpers.
        match read_revision_blob(&repo_path, "HEAD", "img.png") {
            RevisionRead::Present(b) => assert_eq!(b, red, "HEAD bytes preserved"),
            other => panic!(
                "HEAD should be present, got {}",
                matches!(other, RevisionRead::Absent) as u8
            ),
        }
        match read_revision_blob(&repo_path, "", "img.png") {
            RevisionRead::Present(b) => assert_eq!(b, green, "index (:path) bytes preserved"),
            _ => panic!("empty revision should read index"),
        }
        match read_worktree_bytes(&repo_path, "img.png") {
            Ok(Some(b)) => assert_eq!(b, blue, "worktree bytes preserved"),
            other => panic!("worktree should be present, got {other:?}"),
        }

        // Decoded sides keep distinct colors (no downscale at 16x16).
        let old = load_one_side(
            &repo_path,
            ImageSource::Revision {
                revision: "HEAD",
                path: "img.png",
            },
        )
        .expect("HEAD loads")
        .expect("HEAD decodes");
        let staged = load_one_side(
            &repo_path,
            ImageSource::Revision {
                revision: "",
                path: "img.png",
            },
        )
        .expect("index loads")
        .expect("index decodes");
        let work = load_one_side(&repo_path, ImageSource::Worktree("img.png"))
            .expect("worktree loads")
            .expect("worktree decodes");
        assert_eq!((old.width(), old.height()), (16, 16));
        assert_eq!(
            old.image.to_rgb8().get_pixel(0, 0).0,
            [220, 30, 30],
            "HEAD is red"
        );
        assert_eq!(
            staged.image.to_rgb8().get_pixel(0, 0).0,
            [30, 220, 30],
            "index is green"
        );
        assert_eq!(
            work.image.to_rgb8().get_pixel(0, 0).0,
            [30, 30, 220],
            "worktree is blue"
        );

        // Full preview loads both sides with an injected picker.
        let git = repo.git_commands();
        let picker = graphics_picker();
        let preview = ImagePreview::load_with_picker(
            &git,
            ImageSource::Revision {
                revision: "HEAD",
                path: "img.png",
            },
            ImageSource::Worktree("img.png"),
            &picker,
        )
        .expect("preview loads HEAD vs worktree");
        assert!(preview.old.is_some() && preview.new.is_some());
        assert!(preview.estimated_bytes() > 0);
    }

    #[test]
    fn empty_revision_means_index_not_head() {
        let repo = TempRepo::new("empty-rev");
        let red = colored_png([200, 30, 30], 12, 12);
        let green = colored_png([30, 200, 30], 12, 12);
        repo.write("a.png", &red);
        repo.git(&["add", "a.png"]);
        repo.commit("init");

        // No staged change: index == HEAD.
        let repo_path = repo.repo_path();
        let head = match read_revision_blob(&repo_path, "HEAD", "a.png") {
            RevisionRead::Present(b) => b,
            _ => panic!("HEAD present"),
        };
        let index = match read_revision_blob(&repo_path, "", "a.png") {
            RevisionRead::Present(b) => b,
            _ => panic!("index present"),
        };
        assert_eq!(head, index, "clean index matches HEAD");
        assert_eq!(head, red);

        // Stage a different image: empty revision follows the index.
        repo.write("a.png", &green);
        repo.git(&["add", "a.png"]);
        let head2 = match read_revision_blob(&repo_path, "HEAD", "a.png") {
            RevisionRead::Present(b) => b,
            _ => panic!("HEAD present"),
        };
        let index2 = match read_revision_blob(&repo_path, "", "a.png") {
            RevisionRead::Present(b) => b,
            _ => panic!("index present"),
        };
        assert_eq!(head2, red, "HEAD unchanged");
        assert_eq!(index2, green, "empty revision reads staged bytes");

        let via_helper = load_one_side(
            &repo_path,
            ImageSource::Revision {
                revision: "",
                path: "a.png",
            },
        )
        .expect("index loads")
        .expect("index decodes");
        assert_eq!(via_helper.image.to_rgb8().get_pixel(0, 0).0, [30, 200, 30]);

        // Injected Iterm2 picker behaves like Kitty here.
        let git = repo.git_commands();
        for picker in [graphics_picker(), iterm2_picker()] {
            let preview = ImagePreview::load_with_picker(
                &git,
                ImageSource::Revision {
                    revision: "",
                    path: "a.png",
                },
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "a.png",
                },
                &picker,
            )
            .expect("index vs HEAD preview loads");
            assert!(preview.old.is_some() && preview.new.is_some());
            let _ = picker.protocol_type();
        }
    }

    #[test]
    fn root_commit_missing_parent_is_absent() {
        let repo = TempRepo::new("root-parent");
        let red = colored_png([200, 30, 30], 10, 10);
        repo.write("img.png", &red);
        repo.git(&["add", "img.png"]);
        repo.commit("root");

        let repo_path = repo.repo_path();
        let hash = git_output(&repo.path, &["rev-parse", "HEAD"]);
        assert!(!hash.is_empty());
        let parent_spec = format!("{hash}^1");

        // Missing parent blob is legitimately absent, not a failure.
        match read_revision_blob(&repo_path, &parent_spec, "img.png") {
            RevisionRead::Absent => {}
            RevisionRead::Present(_) => panic!("parent should be absent"),
            RevisionRead::Failed => panic!("parent should be absent, not failed"),
        }
        assert!(matches!(
            load_one_side(
                &repo_path,
                ImageSource::Revision {
                    revision: &parent_spec,
                    path: "img.png",
                }
            ),
            Some(None)
        ));
        // HEAD^1 shorthand behaves the same on a root commit.
        assert!(matches!(
            load_one_side(
                &repo_path,
                ImageSource::Revision {
                    revision: "HEAD^1",
                    path: "img.png",
                }
            ),
            Some(None)
        ));

        // Preview treats absent old as added (new present).
        let git = repo.git_commands();
        let picker = graphics_picker();
        let preview = ImagePreview::load_with_picker(
            &git,
            ImageSource::Revision {
                revision: &parent_spec,
                path: "img.png",
            },
            ImageSource::Revision {
                revision: &hash,
                path: "img.png",
            },
            &picker,
        )
        .expect("root preview loads");
        assert!(preview.old.is_none(), "old absent on root");
        assert!(preview.new.is_some(), "new present on root");
    }

    #[test]
    fn added_and_deleted_sides_via_missing() {
        // Pure helper contract without git.
        let picker = graphics_picker();
        let decoded = decode_bytes(&colored_png([10, 200, 10], 8, 8)).unwrap();
        assert!(ImagePreview::from_decoded(None, Some(decoded.clone()), &picker).is_some());
        assert!(ImagePreview::from_decoded(Some(decoded), None, &picker).is_some());
        assert!(ImagePreview::from_decoded(None, None, &picker).is_none());

        // Missing via private helper is Some(None), never a failure.
        let repo = TempRepo::new("added-deleted");
        let bytes = colored_png([200, 30, 30], 8, 8);
        repo.write("img.png", &bytes);
        repo.git(&["add", "img.png"]);
        repo.commit("init");
        let repo_path = repo.repo_path();
        assert!(matches!(
            load_one_side(&repo_path, ImageSource::Missing),
            Some(None)
        ));
        // Worktree file removed => legitimately missing.
        std::fs::remove_file(repo.path.join("img.png")).expect("remove worktree file");
        assert!(matches!(
            read_worktree_bytes(&repo_path, "img.png"),
            Ok(None)
        ));
        assert!(matches!(
            load_one_side(&repo_path, ImageSource::Worktree("img.png")),
            Some(None)
        ));

        // Added (old missing) and deleted (new missing) previews load.
        let git = repo.git_commands();
        let added = ImagePreview::load_with_picker(
            &git,
            ImageSource::Missing,
            ImageSource::Revision {
                revision: "HEAD",
                path: "img.png",
            },
            &picker,
        )
        .expect("added loads");
        assert!(added.old.is_none() && added.new.is_some());
        // Worktree file is gone, so deleted shows old only.
        let deleted = ImagePreview::load_with_picker(
            &git,
            ImageSource::Revision {
                revision: "HEAD",
                path: "img.png",
            },
            ImageSource::Missing,
            &picker,
        )
        .expect("deleted loads");
        assert!(deleted.old.is_some() && deleted.new.is_none());
        // Both missing => no preview at all.
        assert!(
            ImagePreview::load_with_picker(
                &git,
                ImageSource::Missing,
                ImageSource::Missing,
                &picker
            )
            .is_none()
        );
    }

    #[test]
    fn rename_uses_before_path_after_path() {
        let repo = TempRepo::new("rename-paths");
        let bytes = colored_png([30, 30, 220], 14, 14);
        repo.write("before.png", &bytes);
        repo.git(&["add", "before.png"]);
        repo.commit("add before");
        repo.git(&["mv", "before.png", "after.png"]);
        repo.commit("rename");

        let repo_path = repo.repo_path();
        // Old path only exists at the parent, new path only at HEAD.
        let old_bytes = match read_revision_blob(&repo_path, "HEAD^", "before.png") {
            RevisionRead::Present(b) => b,
            _ => panic!("before.png at HEAD^"),
        };
        let new_bytes = match read_revision_blob(&repo_path, "HEAD", "after.png") {
            RevisionRead::Present(b) => b,
            _ => panic!("after.png at HEAD"),
        };
        assert_eq!(old_bytes, bytes);
        assert_eq!(new_bytes, bytes);
        assert!(matches!(
            load_one_side(
                &repo_path,
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "before.png",
                }
            ),
            Some(None)
        ));
        assert!(matches!(
            load_one_side(
                &repo_path,
                ImageSource::Revision {
                    revision: "HEAD^",
                    path: "after.png",
                }
            ),
            Some(None)
        ));

        let git = repo.git_commands();
        let picker = graphics_picker();
        let preview = ImagePreview::load_with_picker(
            &git,
            ImageSource::Revision {
                revision: "HEAD^",
                path: "before.png",
            },
            ImageSource::Revision {
                revision: "HEAD",
                path: "after.png",
            },
            &picker,
        )
        .expect("rename preview loads before/after paths");
        assert!(preview.old.is_some() && preview.new.is_some());
    }

    #[test]
    fn corrupt_side_makes_entire_preview_none() {
        let repo = TempRepo::new("corrupt-side");
        let valid = colored_png([200, 30, 30], 12, 12);
        repo.write("img.png", &valid);
        repo.git(&["add", "img.png"]);
        repo.commit("valid");

        // Corrupt worktree: entire load fails, not a partial preview.
        repo.write("img.png", b"definitely not an image");
        let repo_path = repo.repo_path();
        assert!(decode_bytes(b"definitely not an image").is_none());
        assert!(load_one_side(&repo_path, ImageSource::Worktree("img.png")).is_none());

        let git = repo.git_commands();
        let picker = graphics_picker();
        assert!(
            ImagePreview::load_with_picker(
                &git,
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "img.png",
                },
                ImageSource::Worktree("img.png"),
                &picker,
            )
            .is_none(),
            "corrupt new side must fail the whole preview"
        );
        assert!(
            ImagePreview::load_with_picker(
                &git,
                ImageSource::Worktree("img.png"),
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "img.png",
                },
                &picker,
            )
            .is_none(),
            "corrupt old side must fail the whole preview"
        );

        // Corrupt revision side behaves the same.
        repo.write("img.png", b"still not an image");
        repo.git(&["add", "img.png"]);
        repo.commit("corrupt revision");
        assert!(
            ImagePreview::load_with_picker(
                &git,
                ImageSource::Revision {
                    revision: "HEAD^",
                    path: "img.png",
                },
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "img.png",
                },
                &picker,
            )
            .is_none(),
            "corrupt revision must fail the whole preview"
        );
        let _ = picker.protocol_type();
    }

    #[test]
    fn decode_rejects_over_dimension_limit() {
        // Width far over MAX_IMAGE_DIMENSION with tiny height keeps the
        // allocation small while still exercising the dimension guard.
        let wide = colored_png([10, 10, 10], MAX_IMAGE_DIMENSION + 1000, 16);
        assert!(
            decode_bytes(&wide).is_none(),
            "width over limit must not decode"
        );
        let tall = colored_png([10, 10, 10], 16, MAX_IMAGE_DIMENSION + 1000);
        assert!(
            decode_bytes(&tall).is_none(),
            "height over limit must not decode"
        );

        // Same failure surfaces through the worktree helper without a repo.
        let repo = TempRepo::new("over-dim");
        repo.write("wide.png", &wide);
        let repo_path = repo.repo_path();
        assert!(load_one_side(&repo_path, ImageSource::Worktree("wide.png")).is_none());
        let git = repo.git_commands();
        let picker = graphics_picker();
        assert!(
            ImagePreview::load_with_picker(
                &git,
                ImageSource::Worktree("wide.png"),
                ImageSource::Missing,
                &picker,
            )
            .is_none()
        );
    }

    #[test]
    fn decode_rejects_over_alloc_limit() {
        // 5000x4500 RGB is ~67MiB decoded (>64MiB cap) while staying inside
        // the 16384 dimension cap. Uniform color keeps the PNG itself small.
        let big = colored_png([40, 40, 200], 5000, 4500);
        assert!(
            decode_bytes(&big).is_none(),
            "decoded alloc over limit must not decode"
        );
    }

    #[test]
    fn worktree_oversize_is_bounded_without_writing_20mib() {
        let repo = TempRepo::new("oversize-worktree");
        let small = colored_png([200, 30, 30], 8, 8);
        repo.write("big.png", &small);
        // Sparse extension: no 20MiB write, just a size bump past the cap.
        let full = repo.path.join("big.png");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&full)
            .expect("open");
        file.set_len((MAX_COMPRESSED_BYTES as u64) + 1)
            .expect("sparse oversize");
        drop(file);
        assert_eq!(
            std::fs::metadata(&full).expect("meta").len(),
            (MAX_COMPRESSED_BYTES as u64) + 1
        );

        let repo_path = repo.repo_path();
        assert!(matches!(
            read_worktree_bytes(&repo_path, "big.png"),
            Err(())
        ));
        assert!(load_one_side(&repo_path, ImageSource::Worktree("big.png")).is_none());
        let git = repo.git_commands();
        let picker = graphics_picker();
        assert!(
            ImagePreview::load_with_picker(
                &git,
                ImageSource::Worktree("big.png"),
                ImageSource::Missing,
                &picker,
            )
            .is_none()
        );
    }

    #[test]
    fn headers_show_original_dimensions_not_downscaled() {
        // 3000x2000 downsizes to ~1600x1066 for the protocol, but headers
        // and cache accounting must preserve the original size.
        let bytes = colored_png([200, 30, 30], 3000, 2000);
        let decoded = decode_bytes(&bytes).expect("large png decodes");
        assert_eq!((decoded.orig_width, decoded.orig_height), (3000, 2000));
        assert!(
            decoded.width() <= PREVIEW_MAX_WIDTH && decoded.height() <= PREVIEW_MAX_HEIGHT,
            "protocol image is downsized"
        );
        assert!(
            decoded.width() != decoded.orig_width,
            "fixture must actually downscale"
        );

        let picker = graphics_picker();
        let preview = ImagePreview::from_decoded(Some(decoded), None, &picker).unwrap();
        let side = preview.old.as_ref().unwrap();
        assert_eq!((side.orig_width, side.orig_height), (3000, 2000));
        // Only the thumbnail is retained; original dimensions are metadata.
        let dec_orig = side.width as usize * side.height as usize * 4;
        assert!(
            preview.estimated_bytes() >= dec_orig + dec_orig * 4 / 3,
            "estimate covers original, got {}",
            preview.estimated_bytes()
        );

        let mut live = preview;
        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).expect("test terminal");
        term.draw(|f| {
            render(f, f.area(), &mut live, DiffSideView::Both, &theme);
        })
        .unwrap();
        let text = buffer_text(term.backend().buffer());
        assert!(
            text.contains("3000x2000"),
            "header shows original dims, got {text:?}"
        );
        assert!(
            !text.contains(&format!(
                "{}x{}",
                live.old.as_ref().unwrap().width,
                live.old.as_ref().unwrap().height
            )) || text.contains("3000x2000"),
            "must not show only downsized dims"
        );
    }

    #[test]
    fn async_render_eventually_encodes_with_test_backend() {
        let picker = graphics_picker();
        let decoded = decode_bytes(&colored_png([30, 200, 30], 32, 32)).unwrap();
        let mut preview = ImagePreview::from_decoded(Some(decoded), None, &picker).unwrap();
        let theme = Theme::default();
        let backend = TestBackend::new(80, 24);
        let mut term = Terminal::new(backend).expect("test terminal");

        // First frame kicks the background resize+encode.
        term.draw(|f| {
            render(f, f.area(), &mut preview, DiffSideView::Both, &theme);
        })
        .unwrap();
        let first = buffer_text(term.backend().buffer());
        assert!(first.contains("Deleted"), "header visible while loading");

        // Poll via repeated renders until the worker returns the protocol.
        let mut encoded = false;
        for _ in 0..250 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            term.draw(|f| {
                render(f, f.area(), &mut preview, DiffSideView::Both, &theme);
            })
            .unwrap();
            if preview
                .old
                .as_ref()
                .expect("old side")
                .thread
                .protocol_type()
                .is_some()
            {
                encoded = true;
                break;
            }
        }
        assert!(encoded, "background encode should complete");
        let text = buffer_text(term.backend().buffer());
        assert!(text.contains("Deleted"));
        assert!(text.contains("32x32"));
    }

    #[test]
    fn iterm2_picker_loads_images_like_kitty() {
        let repo = TempRepo::new("iterm2-picker");
        let bytes = colored_png([30, 30, 220], 20, 20);
        repo.write("img.png", &bytes);
        repo.git(&["add", "img.png"]);
        repo.commit("init");
        let git = repo.git_commands();

        for picker in [graphics_picker(), iterm2_picker()] {
            assert_ne!(picker.protocol_type(), ProtocolType::Halfblocks);
            let preview = ImagePreview::load_with_picker(
                &git,
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "img.png",
                },
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "img.png",
                },
                &picker,
            )
            .expect("iterm2/kitty loads");
            // No global picker touched; injected picker drives both sides.
            assert!(preview.old.is_some() && preview.new.is_some());
            assert_eq!(
                preview.old.as_ref().unwrap().orig_width,
                preview.new.as_ref().unwrap().orig_width
            );
        }
    }

    #[test]
    fn diff_view_state_clears_preview_to_text_fallback() {
        use crate::pager::side_by_side::DiffViewState;

        let picker = graphics_picker();
        let decoded = decode_bytes(&colored_png([200, 30, 30], 8, 8)).unwrap();
        let preview = ImagePreview::from_decoded(Some(decoded), None, &picker).unwrap();

        // Setting a preview then loading text clears back to the fallback.
        let mut state = DiffViewState::new();
        state.image_preview = Some(preview);
        assert!(state.image_preview.is_some());
        state.load("file.txt", "old\n", "new\n");
        assert!(
            state.image_preview.is_none(),
            "load() must clear the image preview"
        );
        assert!(!state.lines.is_empty(), "text fallback has lines");

        let decoded2 = decode_bytes(&colored_png([30, 200, 30], 8, 8)).unwrap();
        state.image_preview =
            Some(ImagePreview::from_decoded(Some(decoded2), None, &picker).unwrap());
        state.load_from_diff_output(
            "file.txt",
            "diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n",
        );
        assert!(
            state.image_preview.is_none(),
            "load_from_diff_output must clear the preview"
        );
        assert!(
            state.lines.iter().any(
                |l| l.old_line.as_ref().is_some_and(|(_, t)| t.contains("old"))
                    || l.new_line.as_ref().is_some_and(|(_, t)| t.contains("new"))
            ),
            "fallback text diff is viewable"
        );

        // Parsed text without a preview stays a plain text diff.
        let parsed = DiffViewState::parse_diff_output(
            "file.txt",
            "diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n",
            4,
            true,
        );
        assert!(parsed.image_preview.is_none());
        let mut applied = DiffViewState::new();
        applied.apply_parsed(parsed);
        assert!(applied.image_preview.is_none());
        assert!(!applied.lines.is_empty());
    }

    #[test]
    fn load_one_side_edge_cases_are_safe() {
        let repo = TempRepo::new("edge-cases");
        let bytes = colored_png([200, 30, 30], 8, 8);
        repo.write("img.png", &bytes);
        repo.git(&["add", "img.png"]);
        repo.commit("init");
        std::fs::create_dir_all(repo.path.join("subdir")).expect("mkdir");
        let repo_path = repo.repo_path();

        // Empty and NUL paths never touch git/disk beyond a failure.
        assert!(load_one_side(&repo_path, ImageSource::Worktree("")).is_none());
        assert!(load_one_side(&repo_path, ImageSource::Worktree("a\0b")).is_none());
        assert!(
            load_one_side(
                &repo_path,
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "",
                }
            )
            .is_none()
        );
        assert!(
            load_one_side(
                &repo_path,
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "a\0b",
                }
            )
            .is_none()
        );
        assert!(
            load_one_side(
                &repo_path,
                ImageSource::Revision {
                    revision: "HEAD\0",
                    path: "img.png",
                }
            )
            .is_none()
        );

        // Missing worktree file and missing blob are absent, not failed.
        assert!(matches!(
            read_worktree_bytes(&repo_path, "missing.png"),
            Ok(None)
        ));
        assert!(matches!(
            load_one_side(&repo_path, ImageSource::Worktree("missing.png")),
            Some(None)
        ));
        assert!(matches!(
            read_revision_blob(&repo_path, "HEAD", "missing.png"),
            RevisionRead::Absent
        ));
        assert!(matches!(
            load_one_side(
                &repo_path,
                ImageSource::Revision {
                    revision: "HEAD",
                    path: "missing.png",
                }
            ),
            Some(None)
        ));

        // Directories read as absent.
        assert!(matches!(
            read_worktree_bytes(&repo_path, "subdir"),
            Ok(None)
        ));
        assert!(matches!(
            load_one_side(&repo_path, ImageSource::Worktree("subdir")),
            Some(None)
        ));

        // Direct helper coverage for bounded reads.
        assert!(matches!(
            read_worktree_bytes(&repo_path, "img.png"),
            Ok(Some(_))
        ));
        assert!(matches!(
            read_revision_blob(&repo_path, "HEAD", "img.png"),
            RevisionRead::Present(_)
        ));
    }
    #[test]
    fn image_diff_renders_labels_and_hides_for_overlay() {
        use crate::pager::side_by_side::{DiffViewState, render_diff};
        let picker = graphics_picker();
        let mut parsed = DiffViewState::parse_diff_output(
            "image.png",
            "Binary files a/image.png and b/image.png differ\n",
            4,
            true,
        );
        parsed.image_preview = ImagePreview::from_decoded(
            Some(decode_bytes(&png_bytes(32, 32)).unwrap()),
            Some(decode_bytes(&png_bytes(64, 64)).unwrap()),
            &picker,
        );
        assert!(parsed.image_preview.as_ref().unwrap().uses_placeholders());
        let mut view = DiffViewState::new();
        view.apply_parsed(parsed);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let theme = Theme::default();
        terminal
            .draw(|f| render_diff(f, f.area(), &mut view, &theme, true, false, true))
            .unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Before") && text.contains("After"));
        assert!(!text.contains("Binary cannot be previewed"));
        view.image_preview_hidden = true;
        terminal
            .draw(|f| render_diff(f, f.area(), &mut view, &theme, true, false, true))
            .unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Binary cannot be previewed"));
        assert!(!text.contains("Before"));
    }

    // ------------------------------------------------------------------
    // Inline mixed-diff helpers (isolated, no global env/detection).
    // ------------------------------------------------------------------

    fn mixed_three_file_diff() -> String {
        let long_a_old = "a".repeat(200);
        let long_a_new = "b".repeat(200);
        let long_c_old = "c".repeat(200);
        let long_c_new = "d".repeat(200);
        format!(
            "diff --git a/first.txt b/first.txt\n--- a/first.txt\n+++ b/first.txt\n@@ -1,3 +1,3 @@\n-{long_a_old}\n+{long_a_new}\n context line one\n@@ -10,3 +10,3 @@\n context line two\n-{long_c_old}\n+{long_c_new}\n context line three\ndiff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\ndiff --git a/last.txt b/last.txt\n--- a/last.txt\n+++ b/last.txt\n@@ -1,2 +1,2 @@\n-tail old one\n+tail new one\n tail context\n"
        )
    }

    fn inline_ready_both_sides() -> InlineImagePreview {
        let picker = graphics_picker();
        let old = decode_bytes(&png_bytes(32, 32)).unwrap();
        let new = decode_bytes(&png_bytes(32, 32)).unwrap();
        let preview = ImagePreview::from_decoded(Some(old), Some(new), &picker).unwrap();
        InlineImagePreview::from_preview_for_test(preview)
    }

    fn inline_ready_single_sided(added: bool) -> InlineImagePreview {
        let picker = graphics_picker();
        let img = decode_bytes(&png_bytes(16, 16)).unwrap();
        let (old, new) = if added {
            (None, Some(img))
        } else {
            (Some(img), None)
        };
        let preview = ImagePreview::from_decoded(old, new, &picker).unwrap();
        InlineImagePreview::from_preview_for_test(preview)
    }

    fn render_state_buffer(
        state: &mut crate::pager::side_by_side::DiffViewState,
        w: u16,
        h: u16,
    ) -> ratatui::buffer::Buffer {
        use crate::pager::side_by_side::render_diff;
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| {
            render_diff(
                f,
                f.area(),
                state,
                &crate::config::Theme::default(),
                true,
                false,
                true,
            )
        })
        .unwrap();
        term.backend().buffer().clone()
    }

    fn test_row_string(buf: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect()
    }

    fn test_header_idx(state: &crate::pager::side_by_side::DiffViewState, name: &str) -> usize {
        state
            .lines
            .iter()
            .position(|l| l.file_header.as_ref().is_some_and(|h| h.contains(name)))
            .expect("header present")
    }

    fn middle_image_section(state: &crate::pager::side_by_side::ParsedDiff) -> usize {
        state
            .lines
            .iter()
            .find_map(|l| {
                if l.preview_placeholder.is_some() {
                    Some(l.section_index)
                } else {
                    None
                }
            })
            .expect("binary section present")
    }

    #[test]
    fn inline_with_picker_and_ready_fixture_uses_injected_picker() {
        let picker = graphics_picker();
        let idle = InlineImagePreview::with_picker(
            std::path::Path::new("."),
            InlineImageSource::Missing,
            InlineImageSource::Missing,
            picker,
        );
        assert!(matches!(idle.state, InlineLoadState::Idle));
        let bytes = png_bytes(4, 4);
        assert!(!bytes.is_empty());
        let decoded = decode_bytes(&bytes).expect("png decodes");
        assert_eq!((decoded.width(), decoded.height()), (4, 4));
        let picker2 = graphics_picker();
        let d = decode_bytes(&png_bytes(8, 8)).unwrap();
        let preview = ImagePreview::from_decoded(Some(d), None, &picker2).unwrap();
        let ready = InlineImagePreview::from_preview_for_test(preview);
        assert!(matches!(ready.state, InlineLoadState::Ready(_)));
        assert!(ready.estimated_bytes() > idle.estimated_bytes());
        use crate::pager::side_by_side::DiffViewState;
        let diff = mixed_three_file_diff();
        let mut parsed = DiffViewState::parse_diff_output("mixed", &diff, 4, true);
        let middle = middle_image_section(&parsed);
        let mut map = std::collections::HashMap::new();
        map.insert(middle, inline_ready_both_sides());
        let before = parsed.lines.len();
        parsed.attach_inline_images(map);
        assert_eq!(parsed.inline_images.len(), 1);
        assert!(parsed.lines.len() > before);
        assert_eq!(
            parsed
                .lines
                .iter()
                .filter(|l| l.preview_placeholder.is_some() && l.section_index == middle)
                .count(),
            INLINE_IMAGE_ROWS
        );
    }

    #[test]
    fn inline_lazy_idle_has_no_pixels() {
        let picker = graphics_picker();
        let inline = InlineImagePreview::with_picker(
            std::path::Path::new("."),
            InlineImageSource::Missing,
            InlineImageSource::Missing,
            picker,
        );
        assert!(matches!(inline.state, InlineLoadState::Idle));
        let est = inline.estimated_bytes();
        assert!(est < 4096, "idle estimate small, got {est}");
        let ready = inline_ready_both_sides();
        assert!(ready.estimated_bytes() > est + 1000);
    }

    #[test]
    fn inline_initial_render_triggers_background_load() {
        use crate::pager::side_by_side::DiffSideView;
        let repo = TempRepo::new("inline-initial-load");
        let bytes = png_bytes(16, 16);
        repo.write("img.png", &bytes);
        let repo_path = repo.repo_path();
        let picker = graphics_picker();
        let mut inline = InlineImagePreview::with_picker(
            &repo_path,
            InlineImageSource::Missing,
            InlineImageSource::Worktree("img.png".to_string()),
            picker,
        );
        assert!(matches!(inline.state, InlineLoadState::Idle));
        let theme = crate::config::Theme::default();
        let mut term = Terminal::new(TestBackend::new(60, 14)).unwrap();
        let start = std::time::Instant::now();
        term.draw(|f| {
            inline.render(f, f.area(), DiffSideView::Both, &theme);
        })
        .unwrap();
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "initial render must not block"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut settled = false;
        while std::time::Instant::now() < deadline {
            term.draw(|f| {
                inline.render(f, f.area(), DiffSideView::Both, &theme);
            })
            .unwrap();
            match &inline.state {
                InlineLoadState::Ready(_) => {
                    settled = true;
                    break;
                }
                InlineLoadState::Unavailable => panic!("valid png must not become unavailable"),
                _ => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(settled, "background load should settle to Ready within 2s");
        assert!(inline.estimated_bytes() > 4096);
    }

    #[test]
    fn inline_corrupt_source_shows_fallback_without_blocking() {
        use crate::pager::side_by_side::DiffSideView;
        let repo = TempRepo::new("inline-corrupt-fallback");
        repo.write("img.png", b"definitely not an image");
        let repo_path = repo.repo_path();
        let picker = graphics_picker();
        let mut inline = InlineImagePreview::with_picker(
            &repo_path,
            InlineImageSource::Missing,
            InlineImageSource::Worktree("img.png".to_string()),
            picker,
        );
        let theme = crate::config::Theme::default();
        let mut term = Terminal::new(TestBackend::new(60, 14)).unwrap();
        let start = std::time::Instant::now();
        term.draw(|f| {
            inline.render(f, f.area(), DiffSideView::Both, &theme);
        })
        .unwrap();
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "corrupt render must not block"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut unavailable = false;
        while std::time::Instant::now() < deadline {
            term.draw(|f| {
                inline.render(f, f.area(), DiffSideView::Both, &theme);
            })
            .unwrap();
            if matches!(inline.state, InlineLoadState::Unavailable) {
                unavailable = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(unavailable, "corrupt must settle to Unavailable");
        let text = buffer_text(term.backend().buffer());
        assert!(
            text.contains("Binary cannot be previewed") || text.contains("╱"),
            "fallback stripes, got {text:?}"
        );
        assert!(inline.estimated_bytes() < 4096);
    }

    #[test]
    fn inline_pending_unload_retains_loading_receiver() {
        let picker = graphics_picker();
        let mut inline = InlineImagePreview::with_picker(
            std::path::Path::new("."),
            InlineImageSource::Missing,
            InlineImageSource::Missing,
            picker,
        );
        let (_tx, rx) = std::sync::mpsc::channel::<Option<ImagePreview>>();
        inline.state = InlineLoadState::Loading(rx);
        inline.unload();
        assert!(
            matches!(inline.state, InlineLoadState::Loading(_)),
            "pending load must be retained"
        );
        let picker2 = graphics_picker();
        let mut inline2 = InlineImagePreview::with_picker(
            std::path::Path::new("."),
            InlineImageSource::Missing,
            InlineImageSource::Missing,
            picker2,
        );
        let (tx2, rx2) = std::sync::mpsc::channel::<Option<ImagePreview>>();
        tx2.send(None).unwrap();
        inline2.state = InlineLoadState::Loading(rx2);
        inline2.unload();
        assert!(
            matches!(inline2.state, InlineLoadState::Unavailable),
            "completed None must become Unavailable"
        );
    }

    #[test]
    fn inline_ready_unload_and_cache_unload_free_pixels() {
        let mut ready = inline_ready_both_sides();
        assert!(matches!(ready.state, InlineLoadState::Ready(_)));
        let before = ready.estimated_bytes();
        assert!(before > 4096, "ready carries pixels, got {before}");
        ready.unload();
        assert!(matches!(ready.state, InlineLoadState::Idle));
        assert!(ready.estimated_bytes() < before);
        assert!(ready.estimated_bytes() < 4096);
        let mut cached = inline_ready_both_sides();
        cached.unload_for_cache();
        assert!(matches!(cached.state, InlineLoadState::Idle));
        assert!(cached.estimated_bytes() < 4096);
        let picker = graphics_picker();
        let mut unavailable = InlineImagePreview::with_picker(
            std::path::Path::new("."),
            InlineImageSource::Missing,
            InlineImageSource::Missing,
            picker,
        );
        unavailable.state = InlineLoadState::Unavailable;
        unavailable.unload();
        assert!(matches!(unavailable.state, InlineLoadState::Unavailable));
        unavailable.unload_for_cache();
        assert!(matches!(unavailable.state, InlineLoadState::Unavailable));
    }

    #[test]
    fn inline_offscreen_stays_idle_until_viewport() {
        use crate::pager::side_by_side::{DiffViewLayout, DiffViewState, render_diff};
        let repo = TempRepo::new("inline-offscreen-idle");
        let bytes = png_bytes(16, 16);
        repo.write("img.png", &bytes);
        let repo_path = repo.repo_path();
        let diff = mixed_three_file_diff();
        let mut parsed = DiffViewState::parse_diff_output("mixed", &diff, 4, true);
        let middle = middle_image_section(&parsed);
        let picker = graphics_picker();
        let inline = InlineImagePreview::with_picker(
            &repo_path,
            InlineImageSource::Missing,
            InlineImageSource::Worktree("img.png".to_string()),
            picker,
        );
        assert!(matches!(inline.state, InlineLoadState::Idle));
        let mut map = std::collections::HashMap::new();
        map.insert(middle, inline);
        parsed.attach_inline_images(map);
        let mut state = DiffViewState::new();
        state.apply_parsed(parsed);
        state.view_layout = DiffViewLayout::SideBySide;
        state.wrap = false;
        state.scroll_offset = 0;
        let img_hdr = test_header_idx(&state, "img.png");
        assert!(img_hdr >= 6, "image below small viewport, got {img_hdr}");
        let theme = crate::config::Theme::default();
        let mut term = Terminal::new(TestBackend::new(60, 8)).unwrap();
        term.draw(|f| {
            render_diff(f, f.area(), &mut state, &theme, true, false, true);
        })
        .unwrap();
        assert!(
            matches!(
                state.inline_images.get(&middle).unwrap().state,
                InlineLoadState::Idle
            ),
            "offscreen must stay Idle"
        );
        state.scroll_offset = img_hdr;
        let mut term2 = Terminal::new(TestBackend::new(60, 20)).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut started = false;
        while std::time::Instant::now() < deadline {
            term2
                .draw(|f| {
                    render_diff(f, f.area(), &mut state, &theme, true, false, true);
                })
                .unwrap();
            if !matches!(
                state.inline_images.get(&middle).unwrap().state,
                InlineLoadState::Idle
            ) {
                started = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(started, "viewport render must start load");
    }

    #[test]
    fn inline_mixed_three_file_sticky_headers_and_bounded_image() {
        use crate::pager::side_by_side::{DiffViewLayout, DiffViewState};
        for wrap in [false, true] {
            for layout in [DiffViewLayout::SideBySide, DiffViewLayout::Unified] {
                let diff = mixed_three_file_diff();
                let mut parsed = DiffViewState::parse_diff_output("mixed", &diff, 4, true);
                let middle = middle_image_section(&parsed);
                let mut map = std::collections::HashMap::new();
                map.insert(middle, inline_ready_both_sides());
                parsed.attach_inline_images(map);
                assert_eq!(
                    parsed
                        .lines
                        .iter()
                        .filter(|l| l.preview_placeholder.is_some() && l.section_index == middle)
                        .count(),
                    INLINE_IMAGE_ROWS,
                    "layout {layout:?} wrap {wrap}"
                );
                let mut state = DiffViewState::new();
                state.apply_parsed(parsed);
                state.wrap = wrap;
                let img_hdr = test_header_idx(&state, "img.png");
                let tail_hdr = test_header_idx(&state, "last.txt");
                assert!(tail_hdr > img_hdr, "tail after image");
                if layout == DiffViewLayout::Unified {
                    state.view_layout = DiffViewLayout::Unified;
                    state.scroll_offset = 0;
                    let _ = render_state_buffer(&mut state, 60, 30);
                    state.view_layout = DiffViewLayout::SideBySide;
                    state.scroll_offset = img_hdr + 1;
                    let _ = render_state_buffer(&mut state, 60, 30);
                    state.toggle_view_layout();
                    assert_eq!(state.view_layout, DiffViewLayout::Unified);
                } else {
                    state.view_layout = DiffViewLayout::SideBySide;
                    let _ = render_state_buffer(&mut state, 60, 30);
                    state.scroll_offset = img_hdr + 1;
                }
                assert!(
                    state
                        .sticky_file_header()
                        .is_some_and(|h| h.contains("img.png")),
                    "layout {layout:?} wrap {wrap} sticky"
                );
                assert_eq!(state.sticky_header_line_idx(), Some(img_hdr));
                let buf = render_state_buffer(&mut state, 60, 30);
                let text = buffer_text(&buf);
                assert!(
                    text.contains("Before") && text.contains("After"),
                    "layout {layout:?} wrap {wrap} labels, got {text:?}"
                );
                assert!(text.contains("last.txt"), "tail header visible");
                assert!(text.contains("tail"), "tail content visible");
                let mut before_y = None;
                let mut tail_y = None;
                for y in 0..buf.area.height {
                    let row = test_row_string(&buf, y);
                    if row.contains("Before") && before_y.is_none() {
                        before_y = Some(y);
                    }
                    if row.contains("last.txt") && tail_y.is_none() {
                        tail_y = Some(y);
                    }
                }
                let (b, t) = (before_y.expect("Before row"), tail_y.expect("tail row"));
                assert!(b < t, "image above tail layout {layout:?} wrap {wrap}");
                assert!(
                    (t as usize).saturating_sub(b as usize) <= INLINE_IMAGE_ROWS + 4,
                    "tail just after image b={b} t={t}"
                );
                for y in (t + 1)..buf.area.height {
                    let row = test_row_string(&buf, y);
                    assert!(
                        !row.contains("Before") && !row.contains("After"),
                        "bounded y={y} {row:?}"
                    );
                }
                if layout == DiffViewLayout::Unified {
                    state.toggle_view_layout();
                    assert_eq!(state.view_layout, DiffViewLayout::SideBySide);
                    assert!(
                        (state.scroll_offset as isize - (img_hdr as isize + 1)).abs() <= 2,
                        "toggle back near image, got {}",
                        state.scroll_offset
                    );
                }
            }
        }
    }

    #[test]
    fn inline_mixed_added_deleted_full_width_and_scroll_unloads() {
        use crate::pager::side_by_side::{DiffSideView, DiffViewLayout, DiffViewState};
        for added in [true, false] {
            let diff = mixed_three_file_diff();
            let mut parsed = DiffViewState::parse_diff_output("mixed", &diff, 4, true);
            let middle = middle_image_section(&parsed);
            let mut map = std::collections::HashMap::new();
            map.insert(middle, inline_ready_single_sided(added));
            parsed.attach_inline_images(map);
            let mut state = DiffViewState::new();
            state.apply_parsed(parsed);
            state.view_layout = DiffViewLayout::SideBySide;
            state.side_view = DiffSideView::Both;
            state.wrap = false;
            let img_hdr = test_header_idx(&state, "img.png");
            state.scroll_offset = img_hdr + 1;
            let buf = render_state_buffer(&mut state, 60, 30);
            let text = buffer_text(&buf);
            let label = if added { "Added" } else { "Deleted" };
            assert!(text.contains(label), "added={added} label, got {text:?}");
            assert!(
                !text.contains("Before") && !text.contains("After"),
                "single-sided no Before/After"
            );
            let tail_hdr = test_header_idx(&state, "last.txt");
            state.scroll_offset = tail_hdr;
            let buf2 = render_state_buffer(&mut state, 60, 30);
            let text2 = buffer_text(&buf2);
            assert!(
                !text2.contains("Before") && !text2.contains("After"),
                "no labels past image"
            );
            assert!(
                !text2.contains(label),
                "label {label} gone after scroll, got {text2:?}"
            );
            let preview = state.inline_images.get(&middle).unwrap();
            assert!(
                matches!(preview.state, InlineLoadState::Idle),
                "scrolled past must unload to Idle"
            );
            assert!(preview.estimated_bytes() < 4096);
        }
    }

    #[test]
    fn inline_concurrent_loads_settle_without_global_counter() {
        use crate::pager::side_by_side::DiffSideView;
        let repo = TempRepo::new("inline-concurrent-loads");
        for (name, color) in [
            ("a.png", [200, 30, 30]),
            ("b.png", [30, 200, 30]),
            ("c.png", [30, 30, 200]),
        ] {
            repo.write(name, &colored_png(color, 12, 12));
        }
        let repo_path = repo.repo_path();
        let picker = graphics_picker();
        let mut inlines: Vec<InlineImagePreview> = ["a.png", "b.png", "c.png"]
            .into_iter()
            .map(|n| {
                InlineImagePreview::with_picker(
                    &repo_path,
                    InlineImageSource::Missing,
                    InlineImageSource::Worktree(n.to_string()),
                    picker.clone(),
                )
            })
            .collect();
        let theme = crate::config::Theme::default();
        let mut term = Terminal::new(TestBackend::new(60, 14)).unwrap();
        let start = std::time::Instant::now();
        for inline in inlines.iter_mut() {
            term.draw(|f| {
                inline.render(f, f.area(), DiffSideView::Both, &theme);
            })
            .unwrap();
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "kickoff must not block"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let mut all_ready = true;
            for inline in inlines.iter_mut() {
                term.draw(|f| {
                    inline.render(f, f.area(), DiffSideView::Both, &theme);
                })
                .unwrap();
                match &inline.state {
                    InlineLoadState::Ready(_) => {}
                    InlineLoadState::Unavailable => panic!("valid png unavailable"),
                    _ => {
                        all_ready = false;
                    }
                }
            }
            if all_ready {
                break;
            }
            if std::time::Instant::now() >= deadline {
                panic!("concurrent loads should settle within 2s");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
