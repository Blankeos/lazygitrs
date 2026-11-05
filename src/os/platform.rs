use std::io::Write;

use anyhow::Result;

use super::cmd::CmdBuilder;

pub struct Platform;

fn base64_encode(input: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(CHARSET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARSET[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

impl Platform {
    pub fn open_file(path: &str) -> Result<()> {
        let cmd = if cfg!(target_os = "macos") {
            CmdBuilder::new("open").arg(path)
        } else if cfg!(target_os = "windows") {
            CmdBuilder::new("cmd").args(&["/c", "start", "", path])
        } else {
            CmdBuilder::new("xdg-open").arg(path)
        };

        cmd.run()?;
        Ok(())
    }

    pub fn copy_to_clipboard(text: &str) -> Result<()> {
        let text_owned = text.to_string();
        Self::write_osc52_clipboard(&text_owned);

        std::thread::spawn(move || {
            let text = &text_owned;
            if cfg!(target_os = "macos") {
                let _ = CmdBuilder::new("pbcopy").stdin(text.to_string()).run();
                return;
            }

            if cfg!(target_os = "windows") {
                let _ = CmdBuilder::new("clip").stdin(text.to_string()).run();
                return;
            }

            // Linux / Unix: try Wayland (wl-copy) first, then xclip, then xsel
            if CmdBuilder::new("wl-copy")
                .stdin(text.to_string())
                .run()
                .is_ok()
            {
                return;
            }

            if CmdBuilder::new("xclip")
                .args(&["-selection", "clipboard"])
                .stdin(text.to_string())
                .run()
                .is_ok()
            {
                return;
            }

            let _ = CmdBuilder::new("xsel")
                .args(&["--clipboard", "--input"])
                .stdin(text.to_string())
                .run();
        });

        Ok(())
    }

    /// Writes OSC 52 terminal escape sequence to clipboard (\x1b]52;c;<base64>\x07).
    /// Works in tmux, Alacritty, Kitty, WezTerm, iTerm2, etc.
    fn write_osc52_clipboard(text: &str) {
        let encoded = base64_encode(text.as_bytes());
        let osc52 = if std::env::var_os("TMUX").is_some() {
            format!("\x1bPtmux;\x1b\x1b]52;c;{}\x07\x1b\\", encoded)
        } else {
            format!("\x1b]52;c;{}\x07", encoded)
        };
        let _ = std::io::stdout().write_all(osc52.as_bytes());
        let _ = std::io::stdout().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"hello world"), "aGVsbG8gd29ybGQ=");
    }
}
