use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Result};

/// Generate a commit message by piping `git diff --cached` via stdin to the configured command.
pub fn generate_commit_message(repo_path: &Path, generate_command: &str) -> Result<String> {
    if generate_command.is_empty() {
        bail!("No generateCommand configured. Set git.commit.generateCommand in your config.");
    }

    // Get the staged diff
    let diff_output = Command::new("git")
        .args(["diff", "--cached"])
        .current_dir(repo_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if !diff_output.status.success() {
        bail!("Failed to get staged diff");
    }

    let diff = String::from_utf8_lossy(&diff_output.stdout);
    if diff.trim().is_empty() {
        bail!("No staged changes to generate a commit message for");
    }

    // Run the generate command via shell, piping diff via stdin
    let mut child = Command::new("sh")
        .args(["-c", generate_command])
        .current_dir(repo_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(diff.as_bytes())?;
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Generate command failed: {}", stderr.trim());
    }

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(strip_markdown_fences(&raw))
}

/// Strip markdown code fences and any preamble text before the commit message.
fn strip_markdown_fences(raw: &str) -> String {
    let trimmed = raw.trim();

    // If the output contains a code fence, extract content from within it
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        // Skip the language identifier on the opening fence line
        let content_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
        let content = &after_fence[content_start..];

        if let Some(end) = content.find("```") {
            return content[..end].trim().to_string();
        }
        // No closing fence — use everything after the opening
        return content.trim().to_string();
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_markdown_fences_plain() {
        assert_eq!(
            strip_markdown_fences("fix: update login"),
            "fix: update login"
        );
    }

    #[test]
    fn test_strip_markdown_fences_with_fences() {
        let input = "Here's a commit message:\n\n```\nfeat: add user auth\n\nAdded JWT-based authentication.\n```\n";
        assert_eq!(
            strip_markdown_fences(input),
            "feat: add user auth\n\nAdded JWT-based authentication."
        );
    }

    #[test]
    fn test_strip_markdown_fences_with_language() {
        let input = "```text\nfix: resolve race condition\n```";
        assert_eq!(strip_markdown_fences(input), "fix: resolve race condition");
    }
}
