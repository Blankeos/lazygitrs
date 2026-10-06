//! Bounded worktree/revision reads and decoding for image previews.

use std::io::Read;
use std::path::Path;

use image::{DynamicImage, ImageFormat};

pub(super) const MAX_COMPRESSED_BYTES: usize = 20 * 1024 * 1024;
pub(super) const MAX_IMAGE_DIMENSION: u32 = 16384;
const MAX_ALLOC_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const PREVIEW_MAX_WIDTH: u32 = 1600;
pub(super) const PREVIEW_MAX_HEIGHT: u32 = 1200;

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
    pub(super) fn width(&self) -> u32 {
        self.image.width()
    }
    pub(super) fn height(&self) -> u32 {
        self.image.height()
    }
    pub(super) fn format_label(&self) -> &'static str {
        format_label_for(self.format)
    }
}

pub(super) fn format_label_for(format: Option<ImageFormat>) -> &'static str {
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
pub(super) fn decode_bytes(bytes: &[u8]) -> Option<DecodedImage> {
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

pub(super) enum RevisionRead {
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
pub(super) fn read_revision_blob(cwd: &Path, revision: &str, path: &str) -> RevisionRead {
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
pub(super) fn read_worktree_bytes(repo_path: &Path, rel: &str) -> Result<Option<Vec<u8>>, ()> {
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

pub(super) fn load_one_side(repo: &Path, src: ImageSource<'_>) -> Option<Option<DecodedImage>> {
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
