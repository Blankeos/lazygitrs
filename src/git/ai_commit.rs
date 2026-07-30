use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};

/// Generate a commit message by piping `git diff --cached` via stdin to the configured command.
#[allow(dead_code)]
pub fn generate_commit_message(repo_path: &Path, generate_command: &str) -> Result<String> {
    let cancel = Arc::new(AtomicBool::new(false));
    match generate_commit_message_cancellable(repo_path, generate_command, cancel)? {
        Some(message) => Ok(message),
        None => bail!("AI commit generation cancelled"),
    }
}

/// Generate a commit message like [`generate_commit_message`], but return
/// `Ok(None)` when `cancel` is set while the external generator is running.
pub fn generate_commit_message_cancellable(
    repo_path: &Path,
    generate_command: &str,
    cancel: Arc<AtomicBool>,
) -> Result<Option<String>> {
    if generate_command.is_empty() {
        bail!("No generateCommand configured. Set git.commit.generateCommand in your config.");
    }

    // Get the staged diff
    let diff_output = Command::new("git")
        .args(["diff", "--cached"])
        .current_dir(repo_path)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if !diff_output.status.success() {
        bail!("Failed to get staged diff");
    }

    let diff = String::from_utf8_lossy(&diff_output.stdout);
    generate_commit_message_from_diff_cancellable(repo_path, &diff, generate_command, cancel)
}

/// Generate a commit message from a provided diff, returning `Ok(None)` when cancelled.
pub fn generate_commit_message_from_diff_cancellable(
    repo_path: &Path,
    diff: &str,
    generate_command: &str,
    cancel: Arc<AtomicBool>,
) -> Result<Option<String>> {
    if generate_command.is_empty() {
        bail!("No generateCommand configured. Set git.commit.generateCommand in your config.");
    }

    if diff.trim().is_empty() {
        bail!("No changes to generate a commit message for");
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }

    let input_mode = GenerateInputMode::for_command(generate_command);

    // Run the generate command via shell. Traditional model wrappers receive
    // the staged diff on stdin; agent CLIs can inspect the repo themselves.
    let mut child = Command::new("sh")
        .args(["-c", generate_command])
        .current_dir(repo_path)
        .stdin(match input_mode {
            GenerateInputMode::StdinDiff => Stdio::piped(),
            GenerateInputMode::RepoInspection => Stdio::null(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LAZYGITRS_AI_COMMIT_INPUT", input_mode.as_env_value())
        .env("LAZYGITRS_AI_COMMIT_DIFF_BYTES", diff.len().to_string())
        .spawn()?;

    if input_mode == GenerateInputMode::StdinDiff
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin.write_all(diff.as_bytes())?;
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_handle = thread::spawn(move || read_pipe(stdout));
    let stderr_handle = thread::spawn(move || read_pipe(stderr));

    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_handle.join();
            let _ = stderr_handle.join();
            return Ok(None);
        }

        if let Some(status) = child.try_wait()? {
            break status;
        }

        thread::sleep(Duration::from_millis(50));
    };

    let stdout = stdout_handle.join().unwrap_or_else(|_| {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "stdout reader panicked",
        ))
    })?;
    let stderr = stderr_handle.join().unwrap_or_else(|_| {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "stderr reader panicked",
        ))
    })?;

    if !status.success() {
        bail!("Generate command failed: {}", stderr.trim());
    }

    Ok(Some(clean_generated_message(&stdout, &stderr)?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GenerateInputMode {
    StdinDiff,
    RepoInspection,
}

impl GenerateInputMode {
    fn for_command(generate_command: &str) -> Self {
        if command_prefers_repo_inspection(generate_command) {
            Self::RepoInspection
        } else {
            Self::StdinDiff
        }
    }

    fn as_env_value(self) -> &'static str {
        match self {
            Self::StdinDiff => "stdin-diff",
            Self::RepoInspection => "repo-inspection",
        }
    }
}

fn command_prefers_repo_inspection(generate_command: &str) -> bool {
    let Some(command_name) = first_command_name(generate_command) else {
        return false;
    };

    matches!(
        command_name.as_str(),
        "claude" | "codex" | "crabcode" | "opencode"
    )
}

fn first_command_name(generate_command: &str) -> Option<String> {
    let words = shell_like_words(generate_command);
    let mut index = 0;

    while words.get(index).is_some_and(|word| is_env_assignment(word)) {
        index += 1;
    }

    if words.get(index).is_some_and(|word| word == "env") {
        index += 1;
        while let Some(word) = words.get(index) {
            if word.starts_with('-') || is_env_assignment(word) {
                index += 1;
            } else {
                break;
            }
        }
    }

    let command = words.get(index)?;
    let name = Path::new(command).file_name()?.to_str()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn is_env_assignment(word: &str) -> bool {
    let Some((name, _value)) = word.split_once('=') else {
        return false;
    };

    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn shell_like_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        match (quote, ch) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('\''), _) => current.push(ch),
            (Some('"'), '\\') => escaped = true,
            (Some('"'), _) => current.push(ch),
            (Some(_), _) => current.push(ch),
            (None, '\'') | (None, '"') => quote = Some(ch),
            (None, '\\') => escaped = true,
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            (None, ch) => current.push(ch),
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn read_pipe(pipe: Option<impl Read>) -> std::io::Result<String> {
    let mut output = String::new();
    if let Some(mut pipe) = pipe {
        pipe.read_to_string(&mut output)?;
    }
    Ok(output)
}

fn clean_generated_message(stdout: &str, stderr: &str) -> Result<String> {
    let message = strip_markdown_fences(stdout);
    if message.trim().is_empty() {
        let stderr = stderr.trim();
        if stderr.is_empty() {
            bail!("Generate command produced no commit message");
        }

        bail!("Generate command produced no commit message: {}", stderr);
    }

    Ok(message)
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

    // Strip single backticks from the first line (e.g. `feat: blah blah`)
    // The AI sometimes wraps only the subject line in backticks.
    let mut lines: Vec<&str> = trimmed.lines().collect();
    if let Some(first) = lines.first_mut() {
        if let Some(stripped) = first.strip_prefix('`').and_then(|s| s.strip_suffix('`')) {
            *first = stripped;
        }
    }

    lines.join("\n").trim().to_string()
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
    fn test_strip_single_backticks() {
        assert_eq!(
            strip_markdown_fences("`feat: blah blah blah`"),
            "feat: blah blah blah"
        );
    }

    #[test]
    fn test_strip_single_backticks_first_line_only() {
        let input =
            "`feat: something something`\n\nother content of the commit here stuff\nblah blah blah";
        assert_eq!(
            strip_markdown_fences(input),
            "feat: something something\n\nother content of the commit here stuff\nblah blah blah"
        );
    }

    #[test]
    fn test_strip_markdown_fences_with_language() {
        let input = "```text\nfix: resolve race condition\n```";
        assert_eq!(strip_markdown_fences(input), "fix: resolve race condition");
    }

    #[test]
    fn test_clean_generated_message_rejects_empty_stdout() {
        let err = clean_generated_message("\n\t\n", "").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Generate command produced no commit message"
        );
    }

    #[test]
    fn test_clean_generated_message_rejects_empty_fence() {
        let err = clean_generated_message("```text\n\n```", "model returned nothing").unwrap_err();
        assert_eq!(
            err.to_string(),
            "Generate command produced no commit message: model returned nothing"
        );
    }

    #[test]
    fn test_generate_input_mode_detects_crabcode() {
        assert_eq!(
            GenerateInputMode::for_command(
                "crabcode -p 'Examine staged changes.' --no-session-persistence"
            ),
            GenerateInputMode::RepoInspection
        );
    }

    #[test]
    fn test_generate_input_mode_detects_agent_path() {
        assert_eq!(
            GenerateInputMode::for_command("/Users/carlo/.cargo/bin/crabcode -p hi"),
            GenerateInputMode::RepoInspection
        );
    }

    #[test]
    fn test_generate_input_mode_skips_env_prefixes() {
        assert_eq!(
            GenerateInputMode::for_command("env FOO=bar crabcode -p hi"),
            GenerateInputMode::RepoInspection
        );
        assert_eq!(
            GenerateInputMode::for_command("FOO=bar opencode run 'Commit this'"),
            GenerateInputMode::RepoInspection
        );
    }

    #[test]
    fn test_generate_input_mode_keeps_unknown_commands_on_stdin_diff() {
        assert_eq!(
            GenerateInputMode::for_command("modelcli 'Generate a commit message'"),
            GenerateInputMode::StdinDiff
        );
    }

    #[test]
    fn test_generate_from_diff_rejects_empty_diff() {
        let err = generate_commit_message_from_diff_cancellable(
            Path::new("."),
            " \n\t ",
            "modelcli",
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap_err();

        assert_eq!(
            err.to_string(),
            "No changes to generate a commit message for"
        );
    }

    #[test]
    fn test_generate_from_diff_honors_cancellation_before_spawning() {
        let result = generate_commit_message_from_diff_cancellable(
            Path::new("."),
            "diff --git a/file b/file",
            "command-that-must-not-run",
            Arc::new(AtomicBool::new(true)),
        )
        .unwrap();

        assert_eq!(result, None);
    }
}
