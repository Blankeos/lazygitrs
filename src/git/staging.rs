use anyhow::{Context, Result, bail};

use super::GitCommands;

/// Represents a parsed hunk from a diff.
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<String>,
}

impl GitCommands {
    /// Parse the diff output into hunks.
    pub fn parse_diff_hunks(&self, diff_output: &str) -> Vec<DiffHunk> {
        let mut hunks = Vec::new();
        let mut current_hunk: Option<DiffHunk> = None;

        for line in diff_output.lines() {
            if line.starts_with("@@") {
                // Save previous hunk
                if let Some(hunk) = current_hunk.take() {
                    hunks.push(hunk);
                }

                // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
                let (old_start, old_count, new_start, new_count) = parse_hunk_header(line);
                current_hunk = Some(DiffHunk {
                    header: line.to_string(),
                    old_start,
                    old_count,
                    new_start,
                    new_count,
                    lines: vec![line.to_string()],
                });
            } else if let Some(ref mut hunk) = current_hunk {
                hunk.lines.push(line.to_string());
            }
        }

        if let Some(hunk) = current_hunk {
            hunks.push(hunk);
        }

        hunks
    }

    /// Stage a specific hunk by applying it as a patch.
    pub fn stage_hunk(&self, file_path: &str, hunk: &DiffHunk) -> Result<()> {
        let patch = build_patch(file_path, hunk);
        self.git()
            .args(&["apply", "--cached", "--unidiff-zero", "-"])
            .stdin(patch)
            .run_expecting_success()?;
        Ok(())
    }

    /// Unstage a specific hunk by reverse-applying it as a patch.
    pub fn unstage_hunk(&self, file_path: &str, hunk: &DiffHunk) -> Result<()> {
        let patch = build_patch(file_path, hunk);
        self.git()
            .args(&["apply", "--cached", "--reverse", "--unidiff-zero", "-"])
            .stdin(patch)
            .run_expecting_success()?;
        Ok(())
    }

    /// Get diff for a file and return it split into hunks.
    pub fn file_hunks(&self, path: &str, staged: bool) -> Result<Vec<DiffHunk>> {
        let diff = if staged {
            self.diff_file_staged(path)?
        } else {
            self.diff_file(path)?
        };
        Ok(self.parse_diff_hunks(&diff))
    }

    /// Given a unified diff and a hunk index, reverse-apply that single hunk
    /// to the working tree copy of `file_path`.
    pub fn revert_hunk_in_worktree_from_unified_diff(
        &self,
        file_path: &str,
        unified_diff: &str,
        hunk_index: usize,
    ) -> Result<()> {
        let hunks = self.parse_diff_hunks(unified_diff);
        let Some(hunk) = hunks.get(hunk_index) else {
            bail!(
                "hunk {} out of range ({} hunks)",
                hunk_index + 1,
                hunks.len()
            );
        };

        let patch = build_patch(file_path, hunk);
        self.git()
            .args(&["apply", "--reverse", "--unidiff-zero", "-"])
            .stdin(patch)
            .run_expecting_success()
            .with_context(|| {
                format!("failed to revert hunk {} in {}", hunk_index + 1, file_path)
            })?;
        Ok(())
    }
}

fn parse_hunk_header(header: &str) -> (usize, usize, usize, usize) {
    // @@ -1,5 +1,7 @@
    let parts: Vec<&str> = header.split_whitespace().collect();

    let parse_range = |s: &str| -> (usize, usize) {
        let s = s.trim_start_matches(['-', '+']);
        if let Some((start, count)) = s.split_once(',') {
            (
                start.parse().unwrap_or(1),
                count.parse().unwrap_or(1),
            )
        } else {
            (s.parse().unwrap_or(1), 1)
        }
    };

    let (old_start, old_count) = if parts.len() > 1 {
        parse_range(parts[1])
    } else {
        (1, 0)
    };

    let (new_start, new_count) = if parts.len() > 2 {
        parse_range(parts[2])
    } else {
        (1, 0)
    };

    (old_start, old_count, new_start, new_count)
}

fn build_patch(file_path: &str, hunk: &DiffHunk) -> String {
    let mut patch = String::new();
    patch.push_str(&format!("--- a/{}\n", file_path));
    patch.push_str(&format!("+++ b/{}\n", file_path));
    for line in &hunk.lines {
        patch.push_str(line);
        patch.push('\n');
    }
    patch
}
