use super::{GitCommands, HeadState};
use crate::model::{
    Commit, CommitStatus,
    commit::{CommitStat, Divergence},
};
use anyhow::Result;
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct CommitFilter {
    pub branches: Vec<String>,
    pub path: Option<String>,
    pub authors: Vec<String>,
}

fn commit_filter_path_suggestions<'a>(paths: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut suggestions = HashSet::new();
    for path in paths.filter(|path| !path.is_empty()) {
        suggestions.insert(path.to_string());

        for (separator, _) in path.match_indices('/') {
            if separator > 0 {
                suggestions.insert(path[..separator].to_string());
            }
        }
    }
    let mut suggestions = suggestions.into_iter().collect::<Vec<_>>();
    suggestions.sort_unstable();
    suggestions
}

#[cfg(test)]
mod commit_filter_suggestion_tests {
    use super::{GitCommands, commit_filter_path_suggestions, parse_commit_details_output};
    use crate::git::HeadState;
    use crate::model::commit::{Commit, CommitStatus, Divergence};

    #[test]
    fn empty_unpushed_list_marks_previous_unpushed_commit_as_pushed() {
        let mut commits = vec![Commit {
            hash: "abc123".to_string(),
            name: "test commit".to_string(),
            status: CommitStatus::Unpushed,
            action: String::new(),
            tags: Vec::new(),
            refs: Vec::new(),
            extra_info: String::new(),
            author_name: String::new(),
            author_email: String::new(),
            unix_timestamp: 0,
            parents: Vec::new(),
            divergence: Divergence::None,
        }];

        GitCommands::apply_unpushed_status(&mut commits, &[]);

        assert_eq!(commits[0].status, CommitStatus::Pushed);
    }

    #[test]
    fn checkout_head_state_moves_head_decoration() {
        let mut commits = vec![
            Commit {
                hash: "old".into(),
                name: "old".into(),
                status: CommitStatus::Pushed,
                action: String::new(),
                tags: Vec::new(),
                refs: vec!["HEAD -> main".into(), "origin/main".into()],
                extra_info: String::new(),
                author_name: String::new(),
                author_email: String::new(),
                unix_timestamp: 0,
                parents: Vec::new(),
                divergence: Divergence::None,
            },
            Commit {
                hash: "new".into(),
                name: "new".into(),
                status: CommitStatus::Pushed,
                action: String::new(),
                tags: Vec::new(),
                refs: vec!["feature".into()],
                extra_info: String::new(),
                author_name: String::new(),
                author_email: String::new(),
                unix_timestamp: 0,
                parents: Vec::new(),
                divergence: Divergence::None,
            },
        ];

        let head = HeadState {
            hash: "new".into(),
            branch_name: "feature".into(),
        };

        GitCommands::apply_checkout_head_state(&mut commits, &head);

        assert_eq!(commits[0].refs, vec!["main", "origin/main"]);
        assert_eq!(commits[1].refs, vec!["HEAD -> feature"]);
    }

    #[test]
    fn checkout_head_state_handles_detached_head() {
        let mut commits = vec![
            Commit {
                hash: "old".into(),
                name: "old".into(),
                status: CommitStatus::Pushed,
                action: String::new(),
                tags: Vec::new(),
                refs: vec!["HEAD -> main".into()],
                extra_info: String::new(),
                author_name: String::new(),
                author_email: String::new(),
                unix_timestamp: 0,
                parents: Vec::new(),
                divergence: Divergence::None,
            },
            Commit {
                hash: "new".into(),
                name: "new".into(),
                status: CommitStatus::Pushed,
                action: String::new(),
                tags: Vec::new(),
                refs: Vec::new(),
                extra_info: String::new(),
                author_name: String::new(),
                author_email: String::new(),
                unix_timestamp: 0,
                parents: Vec::new(),
                divergence: Divergence::None,
            },
        ];

        let head = HeadState {
            hash: "new".into(),
            branch_name: String::new(),
        };

        GitCommands::apply_checkout_head_state(&mut commits, &head);

        assert_eq!(commits[0].refs, vec!["main"]);
        assert_eq!(commits[1].refs, vec!["HEAD"]);
    }

    #[test]
    fn path_suggestions_include_files_and_each_parent_directory() {
        let suggestions = commit_filter_path_suggestions(
            [
                "src/gui/controller/commits.rs",
                "README.md",
                "src/gui/mod.rs",
                "",
            ]
            .into_iter(),
        );

        assert_eq!(
            suggestions,
            [
                "README.md",
                "src",
                "src/gui",
                "src/gui/controller",
                "src/gui/controller/commits.rs",
                "src/gui/mod.rs",
            ]
        );
    }

    #[test]
    fn commit_details_output_preserves_message_and_parses_shortstat() {
        let output = concat!(
            "Fix scheduler regression\n",
            "\n",
            "Preserve the full commit body across multiple lines.\n",
            "This line contains additional context.\n",
            "\0",
            "\n",
            " 2 files changed, 7 insertions(+), 3 deletions(-)\n",
        );

        let (message, stat) = parse_commit_details_output(output);

        assert_eq!(
            message,
            concat!(
                "Fix scheduler regression\n",
                "\n",
                "Preserve the full commit body across multiple lines.\n",
                "This line contains additional context."
            )
        );
        assert_eq!(stat.files_changed, 2);
        assert_eq!(stat.insertions, 7);
        assert_eq!(stat.deletions, 3);
    }
}

impl GitCommands {
    /// Return repository paths suitable for history filtering.
    ///
    /// This mirrors lazygit's source of path suggestions: tracked files plus
    /// untracked, non-ignored files. Parent directories are included so users
    /// can filter a whole subtree without having to type its path.
    pub fn load_commit_filter_paths(&self) -> Result<Vec<String>> {
        let result = self
            .git()
            .args(&[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ])
            .run_expecting_success()?;
        Ok(commit_filter_path_suggestions(result.stdout.split('\0')))
    }

    /// Load recent commits reachable from any ref.
    pub fn load_commits(&self, limit: usize) -> Result<Vec<Commit>> {
        let unpushed = self.unpushed_commit_hashes().unwrap_or_default();
        let mut commits = self.load_commits_page(limit, 0)?;
        Self::apply_unpushed_status(&mut commits, &unpushed);
        Ok(commits)
    }

    /// Load a page of commits reachable from any ref.
    /// Status (unpushed) is applied by callers that need it.
    pub fn load_commits_page(&self, limit: usize, skip: usize) -> Result<Vec<Commit>> {
        self.load_filtered_commits_page(&CommitFilter::default(), limit, skip)
    }

    /// Load the symmetric difference of two commit revisions in topological order.
    /// Left marks A-only commits; Right marks B-only commits. A zero limit is
    /// unlimited, matching the other paged commit loaders.
    pub fn load_compare_commits(
        &self,
        ref_a: &str,
        ref_b: &str,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<Commit>> {
        let range = format!(
            "{}...{}",
            self.resolve_commit_ref(ref_a)?,
            self.resolve_commit_ref(ref_b)?
        );
        // NUL field separators preserve pipes in commit subjects and identities.
        // %m is '<' for the left side and '>' for the right side of A...B.
        let mut cmd = self.git().args(&[
            "log",
            "--left-right",
            "--topo-order",
            "--no-show-signature",
            "--format=%H%x00%s%x00%an%x00%ae%x00%at%x00%P%x00%D%x00%m",
        ]);
        if limit > 0 {
            cmd = cmd.arg(&format!("--max-count={limit}"));
        }
        if skip > 0 {
            cmd = cmd.arg(&format!("--skip={skip}"));
        }
        let result = cmd.arg(&range).arg("--").run_expecting_success()?;
        self.parse_commit_log_fields(&result, '\0')
    }

    pub fn load_filtered_commits_page(
        &self,
        filter: &CommitFilter,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<Commit>> {
        let format = "%H|%s|%an|%ae|%at|%P|%D";
        let mut cmd = self.git().arg("log");

        // Unfiltered and path-filtered lists both walk all refs (`--all`), matching
        // our unfiltered commits panel and what lazygit shows for `-f` when the
        // history for a path lives outside HEAD (e.g. checkpoint refs).
        if filter.branches.is_empty() {
            cmd = cmd.arg("--all");
        } else {
            for branch in &filter.branches {
                cmd = cmd.arg(branch);
            }
        }
        for author in &filter.authors {
            cmd = cmd.arg(&format!("--author={author}"));
        }
        if limit > 0 {
            cmd = cmd.arg(&format!("--max-count={limit}"));
        }
        if skip > 0 {
            cmd = cmd.arg(&format!("--skip={skip}"));
        }
        cmd = cmd
            .arg(&format!("--format={format}"))
            .arg("--no-show-signature")
            .arg("--topo-order");
        if let Some(path) = filter.path.as_deref() {
            cmd = cmd.arg("--follow").arg("--").arg(path);
        }

        self.parse_commit_log(&cmd.run()?)
    }

    /// Load commits reachable from a specific branch only.
    pub fn load_commits_for_branch(&self, branch: &str, limit: usize) -> Result<Vec<Commit>> {
        self.load_commits_for_branches(&[branch.to_string()], limit)
    }

    /// Load commits reachable from any of the given branches.
    pub fn load_commits_for_branches(
        &self,
        branches: &[String],
        limit: usize,
    ) -> Result<Vec<Commit>> {
        self.load_commits_for_branches_page(branches, limit, 0)
    }

    /// Load a page of commits reachable from any of the given branches.
    pub fn load_commits_for_branches_page(
        &self,
        branches: &[String],
        limit: usize,
        skip: usize,
    ) -> Result<Vec<Commit>> {
        self.load_filtered_commits_page(
            &CommitFilter {
                branches: branches.to_vec(),
                ..CommitFilter::default()
            },
            limit,
            skip,
        )
    }

    fn parse_commit_log(&self, result: &crate::os::cmd::CmdResult) -> Result<Vec<Commit>> {
        self.parse_commit_log_fields(result, '|')
    }

    fn parse_commit_log_fields(
        &self,
        result: &crate::os::cmd::CmdResult,
        separator: char,
    ) -> Result<Vec<Commit>> {
        if !result.success {
            return Ok(Vec::new());
        }

        // Unpushed status is applied by the caller so we don't spawn an extra
        // `git log @{u}..HEAD` per page parse (filter apply hot path).
        let mut commits = Vec::new();
        for line in result.stdout.lines() {
            let parts: Vec<&str> = line
                .splitn(if separator == '\0' { 8 } else { 7 }, separator)
                .collect();
            if parts.len() < 6 {
                continue;
            }

            let hash = parts[0].to_string();
            let name = parts[1].to_string();
            let author_name = parts[2].to_string();
            let author_email = parts[3].to_string();
            let unix_timestamp = parts[4].parse::<i64>().unwrap_or(0);
            let parents: Vec<String> = parts[5].split_whitespace().map(String::from).collect();

            let decoration = if parts.len() > 6 { parts[6] } else { "" };
            let tags = extract_tags(decoration);
            let refs = extract_refs(decoration);

            commits.push(Commit {
                hash,
                name,
                status: CommitStatus::Pushed,
                action: String::new(),
                tags,
                refs,
                extra_info: String::new(),
                author_name,
                author_email,
                unix_timestamp,
                parents,
                divergence: match parts.get(7).copied() {
                    Some("<") => Divergence::Left,
                    Some(">") => Divergence::Right,
                    _ => Divergence::None,
                },
            });
        }

        Ok(commits)
    }

    /// Load reflog entries using `git log -g`.
    pub fn load_reflog(&self, limit: usize) -> Result<Vec<Commit>> {
        let format = "%H|%gs|%an|%ae|%at|%P";
        let result = self
            .git()
            .args(&[
                "log",
                "-g",
                &format!("--max-count={}", limit),
                &format!("--format={}", format),
                "--no-show-signature",
            ])
            .run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        let mut commits = Vec::new();
        for line in result.stdout.lines() {
            let parts: Vec<&str> = line.splitn(6, '|').collect();
            if parts.len() < 5 {
                continue;
            }

            let hash = parts[0].to_string();
            let name = parts[1].to_string();
            let author_name = parts[2].to_string();
            let author_email = parts[3].to_string();
            let unix_timestamp = parts[4].parse::<i64>().unwrap_or(0);
            let parents: Vec<String> = if parts.len() > 5 {
                parts[5].split_whitespace().map(String::from).collect()
            } else {
                Vec::new()
            };

            commits.push(Commit {
                hash,
                name,
                status: CommitStatus::Reflog,
                action: String::new(),
                tags: Vec::new(),
                refs: Vec::new(),
                extra_info: String::new(),
                author_name,
                author_email,
                unix_timestamp,
                parents,
                divergence: Divergence::None,
            });
        }

        Ok(commits)
    }

    /// Hashes of commits ahead of upstream (`@{u}..HEAD`). Cheap enough to call
    /// once per filter/page load; avoid calling from every `parse_commit_log`.
    pub fn unpushed_commit_hashes(&self) -> Result<Vec<String>> {
        let result = self
            .git()
            .args(&["log", "@{u}..HEAD", "--format=%H"])
            .run()?;

        if !result.success {
            return Ok(Vec::new());
        }

        Ok(result.stdout.lines().map(String::from).collect())
    }

    pub fn apply_unpushed_status(commits: &mut [Commit], unpushed_hashes: &[String]) {
        for commit in commits {
            commit.status = if unpushed_hashes.contains(&commit.hash) {
                CommitStatus::Unpushed
            } else {
                CommitStatus::Pushed
            };
        }
    }

    pub(crate) fn apply_checkout_head_state(commits: &mut [Commit], head: &HeadState) {
        // Remove the old HEAD marker while preserving the underlying local branch
        // decoration. Detached HEAD is represented by a bare "HEAD" ref.
        for commit in commits.iter_mut() {
            commit.refs.retain(|reference| reference != "HEAD");

            for reference in &mut commit.refs {
                if let Some(branch) = reference.strip_prefix("HEAD -> ") {
                    *reference = branch.to_string();
                }
            }
        }

        let Some(commit) = commits.iter_mut().find(|commit| commit.hash == head.hash) else {
            return;
        };

        if head.branch_name.is_empty() {
            commit.refs.insert(0, "HEAD".to_string());
            return;
        }

        let head_ref = format!("HEAD -> {}", head.branch_name);

        if let Some(index) = commit
            .refs
            .iter()
            .position(|reference| reference == &head.branch_name)
        {
            commit.refs[index] = head_ref;
        } else {
            commit.refs.insert(0, head_ref);
        }
    }

    pub fn create_commit(&self, message: &str, sign_off: bool) -> Result<()> {
        let mut cmd = self.git();
        cmd = cmd.arg("commit").arg("-m").arg(message);
        if sign_off {
            cmd = cmd.arg("--signoff");
        }
        cmd.run_expecting_success()?;
        Ok(())
    }

    pub fn create_empty_commit(&self, message: &str) -> Result<()> {
        self.git()
            .args(&["commit", "--allow-empty", "-m", message])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn amend_commit(&self) -> Result<()> {
        self.git()
            .args(&["commit", "--amend", "--no-edit"])
            .run_expecting_success()?;
        Ok(())
    }

    pub fn reword_commit(&self, hash: &str, message: &str) -> Result<()> {
        let head = self.head_hash()?;
        if hash == head {
            // --allow-empty: reword empty commits (lazygit allows this)
            // --only: don't include staged changes in the amended commit
            self.git()
                .args(&[
                    "commit",
                    "--allow-empty",
                    "--only",
                    "--amend",
                    "-m",
                    message,
                ])
                .run_expecting_success()?;
        } else {
            self.reword_commit_rebase(hash, message)?;
        }
        Ok(())
    }

    pub fn revert_commit(&self, hash: &str) -> Result<()> {
        self.git().args(&["revert", hash]).run_expecting_success()?;
        Ok(())
    }

    pub fn cherry_pick(&self, hashes: &[String]) -> Result<()> {
        let mut cmd = self.git();
        cmd = cmd.arg("cherry-pick").arg("--allow-empty");
        for hash in hashes {
            cmd = cmd.arg(hash.as_str());
        }
        cmd.run_expecting_success()?;
        Ok(())
    }

    pub fn commit_message_full(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["log", "-1", "--format=%B", hash])
            .run_expecting_success()?;
        Ok(result.stdout.trim().to_string())
    }

    pub fn commit_details(&self, hash: &str) -> Result<(String, CommitStat)> {
        let result = self
            .git()
            .args(&["log", "-1", "--shortstat", "--format=%B%x00", hash])
            .run_expecting_success()?;

        Ok(parse_commit_details_output(&result.stdout))
    }

    pub fn commit_message_body(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&["log", "-1", "--format=%b", hash])
            .run_expecting_success()?;
        Ok(result.stdout.trim().to_string())
    }

    pub fn commit_diff(&self, hash: &str) -> Result<String> {
        let result = self
            .git()
            .args(&[
                "show",
                "--format=",
                "--find-renames",
                "--find-copies",
                "--binary",
                hash,
            ])
            .run_expecting_success()?;
        Ok(result.stdout)
    }

    pub fn reset_to_commit(&self, hash: &str, mode: &str) -> Result<()> {
        self.git()
            .args(&["reset", mode, hash])
            .run_expecting_success()?;
        Ok(())
    }

    /// Fetch the shortstat summary for a commit:
    /// "N files changed, A insertions(+), D deletions(-)".
    /// Uses `git log -1 --shortstat` because `git show --shortstat` dumps the
    /// full patch first (slow on large commits), while `git show --shortstat
    /// --no-patch` suppresses the stat entirely.  `git log -1` gives us the
    /// stat without the patch.
    pub fn commit_stat(&self, hash: &str) -> Result<CommitStat> {
        let result = self
            .git()
            .args(&["log", "-1", "--shortstat", "--format=", hash])
            .run()?;
        if !result.success {
            return Ok(CommitStat::default());
        }
        Ok(parse_shortstat(&result.stdout))
    }
}

fn parse_commit_details_output(output: &str) -> (String, CommitStat) {
    let (message, shortstat) = output.split_once('\0').unwrap_or((output, ""));

    (message.trim().to_string(), parse_shortstat(shortstat))
}

fn parse_shortstat(output: &str) -> CommitStat {
    let line = output
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or("");
    let mut stat = CommitStat::default();
    for segment in line.split(',') {
        let segment = segment.trim();
        let num_str: String = segment.chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(n) = num_str.parse::<usize>() else {
            continue;
        };
        if segment.contains("file") {
            stat.files_changed = n;
        } else if segment.contains("insertion") {
            stat.insertions = n;
        } else if segment.contains("deletion") {
            stat.deletions = n;
        }
    }
    stat
}

fn extract_tags(decoration: &str) -> Vec<String> {
    decoration
        .split(", ")
        .filter_map(|d| {
            let d = d.trim();
            if let Some(tag) = d.strip_prefix("tag: ") {
                Some(tag.to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Extract ref decorations like "HEAD -> main", "origin/main", "origin/feature".
/// Excludes tags (handled separately).
fn extract_refs(decoration: &str) -> Vec<String> {
    if decoration.is_empty() {
        return Vec::new();
    }
    decoration
        .split(", ")
        .filter_map(|d| {
            let d = d.trim();
            if d.is_empty() || d.starts_with("tag: ") {
                None
            } else {
                Some(d.to_string())
            }
        })
        .collect()
}
