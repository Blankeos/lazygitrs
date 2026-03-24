use anyhow::Result;

use super::cmd::CmdBuilder;

pub struct Platform;

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

    pub fn open_editor(editor: &str, path: &str) -> Result<()> {
        let editor = if editor.is_empty() {
            std::env::var("EDITOR").unwrap_or_else(|_| {
                if cfg!(target_os = "windows") {
                    "notepad".to_string()
                } else {
                    "vi".to_string()
                }
            })
        } else {
            editor.to_string()
        };

        CmdBuilder::new(&editor).arg(path).run()?;
        Ok(())
    }

    pub fn copy_to_clipboard(text: &str) -> Result<()> {
        let cmd = if cfg!(target_os = "macos") {
            CmdBuilder::new("pbcopy")
        } else if cfg!(target_os = "windows") {
            CmdBuilder::new("clip")
        } else {
            CmdBuilder::new("xclip").args(&["-selection", "clipboard"])
        };

        cmd.stdin(text.to_string()).run()?;
        Ok(())
    }
}
