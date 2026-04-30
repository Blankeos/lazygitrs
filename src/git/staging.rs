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

    /// Reverse-apply only the lines of a single visual change block to the
    /// working tree copy of `file_path`. `want_old` and `want_new` are the
    /// inclusive old-file and new-file line-number ranges the visual block
    /// covers; either may be `None` for pure insertion or pure deletion.
    /// The block typically lives inside one of the `@@` hunks of
    /// `unified_diff`, but may be narrower than that `@@` — visual blocks
    /// can be split by 1–3 lines of context within a single `@@`.
    pub fn revert_visual_block_in_worktree(
        &self,
        file_path: &str,
        unified_diff: &str,
        want_old: Option<(usize, usize)>,
        want_new: Option<(usize, usize)>,
    ) -> Result<()> {
        let patch = build_visual_block_patch(file_path, unified_diff, want_old, want_new)?;
        self.git()
            .args(&["apply", "--reverse", "--unidiff-zero", "-"])
            .stdin(patch)
            .run_expecting_success()
            .with_context(|| format!("failed to revert hunk in {}", file_path))?;
        Ok(())
    }
}

fn build_visual_block_patch(
    file_path: &str,
    unified_diff: &str,
    want_old: Option<(usize, usize)>,
    want_new: Option<(usize, usize)>,
) -> Result<String> {
    if want_old.is_none() && want_new.is_none() {
        bail!("empty visual block");
    }

    let mut emitted: Vec<String> = Vec::new();
    let mut anchor_old: Option<usize> = None;
    let mut anchor_new: Option<usize> = None;
    let mut old_count = 0usize;
    let mut new_count = 0usize;

    let mut in_hunk = false;
    let mut old_counter = 0usize;
    let mut new_counter = 0usize;
    let mut last_emitted = false;

    for line in unified_diff.lines() {
        if line.starts_with("@@") {
            let (os, _, ns, _) = parse_hunk_header(line);
            old_counter = os;
            new_counter = ns;
            in_hunk = true;
            last_emitted = false;
            continue;
        }
        if !in_hunk {
            continue;
        }
        // A new file's preamble can interleave between hunks of multi-file
        // diffs; abandon the current hunk until we see a fresh `@@`.
        if line.starts_with("diff ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("index ")
            || line.starts_with("similarity ")
            || line.starts_with("rename ")
            || line.starts_with("new file ")
            || line.starts_with("deleted file ")
            || line.starts_with("Binary ")
        {
            in_hunk = false;
            last_emitted = false;
            continue;
        }
        // A "\ No newline at end of file" marker refers to the immediately
        // preceding diff line. Propagate it only when that line was emitted.
        if line.starts_with('\\') {
            if last_emitted {
                emitted.push(line.to_string());
            }
            continue;
        }
        if line.starts_with('-') {
            let in_range =
                want_old.is_some_and(|(lo, hi)| old_counter >= lo && old_counter <= hi);
            if in_range {
                if anchor_old.is_none() {
                    anchor_old = Some(old_counter);
                }
                if anchor_new.is_none() {
                    anchor_new = Some(new_counter);
                }
                emitted.push(line.to_string());
                old_count += 1;
                last_emitted = true;
            } else {
                last_emitted = false;
            }
            old_counter += 1;
        } else if line.starts_with('+') {
            let in_range =
                want_new.is_some_and(|(lo, hi)| new_counter >= lo && new_counter <= hi);
            if in_range {
                if anchor_old.is_none() {
                    anchor_old = Some(old_counter);
                }
                if anchor_new.is_none() {
                    anchor_new = Some(new_counter);
                }
                emitted.push(line.to_string());
                new_count += 1;
                last_emitted = true;
            } else {
                last_emitted = false;
            }
            new_counter += 1;
        } else if line.starts_with(' ') || line.is_empty() {
            old_counter += 1;
            new_counter += 1;
            last_emitted = false;
        }
    }

    if emitted.is_empty() {
        bail!("visual block matched no diff lines");
    }

    let old_start = anchor_old.unwrap_or(0);
    let new_start = anchor_new.unwrap_or(0);

    let mut patch = String::new();
    patch.push_str(&format!("--- a/{}\n", file_path));
    patch.push_str(&format!("+++ b/{}\n", file_path));
    patch.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        old_start, old_count, new_start, new_count
    ));
    for line in &emitted {
        patch.push_str(line);
        patch.push('\n');
    }
    Ok(patch)
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
