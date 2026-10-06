use crate::model::{Branch, Commit, CommitFile, Remote, Tag};

/// Which panel is focused within diff mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffModeFocus {
    SelectorA,
    SelectorB,
    CommitFiles,
    Commits,
    DiffExploration,
}

/// The diff being previewed remains stable when focus moves to the diff panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareDiffSource {
    Comparison,
    Commit,
    CommitFiles,
}

/// Shared by rendering and mouse hit-testing.
pub struct CompareLayout {
    pub sidebar: std::rc::Rc<[ratatui::layout::Rect]>,
    pub diff: ratatui::layout::Rect,
    pub status: ratatui::layout::Rect,
}

impl CompareLayout {
    pub fn new(area: ratatui::layout::Rect) -> Self {
        use ratatui::layout::{Constraint, Direction, Layout};
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let content = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(33), Constraint::Percentage(67)])
            .split(outer[0]);
        let sidebar = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Fill(1),
                Constraint::Fill(1),
            ])
            .split(content[0]);
        Self {
            sidebar,
            diff: content[1],
            status: outer[1],
        }
    }
}

impl DiffModeState {
    pub fn clear_commits(&mut self) {
        self.commits.clear();
        self.commits_selected = 0;
        self.commits_scroll = 0;
        self.commits_viewport_manually_scrolled = false;
        self.commits_revision = self.commits_revision.wrapping_add(1);
        self.diff_source = CompareDiffSource::Comparison;
        self.files_commit = None;
        self.clear_list_search();
    }

    pub fn clear_list_search(&mut self) {
        self.file_search_active = false;
        self.file_search_query.clear();
        self.file_search_matches.clear();
        self.file_search_match_idx = 0;
        self.file_search_textarea = None;
    }

    pub fn set_focus(&mut self, focus: DiffModeFocus) {
        if (self.focus == DiffModeFocus::Commits) != (focus == DiffModeFocus::Commits) {
            self.clear_list_search();
        }
        self.focus = focus;
        match focus {
            DiffModeFocus::Commits => self.diff_source = CompareDiffSource::Commit,
            DiffModeFocus::CommitFiles => {
                self.diff_source = if self.files_commit.is_some() {
                    CompareDiffSource::CommitFiles
                } else {
                    CompareDiffSource::Comparison
                };
            }
            _ => {}
        }
    }

    pub fn selected_commit(&self) -> Option<&Commit> {
        self.commits.get(self.commits_selected)
    }

    pub fn has_more_commits(&self) -> bool {
        self.ahead_behind
            .is_some_and(|(ahead, behind)| self.commits.len() < ahead + behind)
    }

    pub fn select_list_match(&mut self, index: usize) {
        if self.focus == DiffModeFocus::Commits {
            self.commits_selected = index;
            self.commits_viewport_manually_scrolled = false;
        } else {
            self.diff_files_selected = index;
            self.viewport_manually_scrolled = false;
        }
    }

    pub fn diff_key(&self) -> String {
        if self.diff_source == CompareDiffSource::Commit {
            return format!(
                "DiffMode:commit:{}",
                self.selected_commit()
                    .map(|c| c.hash.as_str())
                    .unwrap_or("none")
            );
        }
        let item = if self.show_tree {
            self.tree_nodes.get(self.diff_files_selected).map(|node| {
                node.file_index
                    .and_then(|i| self.diff_files.get(i))
                    .map(|file| format!("file:{}", file.name))
                    .unwrap_or_else(|| format!("dir:{}", node.path))
            })
        } else {
            self.diff_files
                .get(self.diff_files_selected)
                .map(|file| format!("file:{}", file.name))
        }
        .unwrap_or_else(|| "none".into());
        if self.diff_source == CompareDiffSource::CommitFiles {
            format!(
                "DiffMode:commit-files:{}:{item}",
                self.files_commit.as_deref().unwrap_or("none")
            )
        } else {
            format!("DiffMode:{}..{}:{item}", self.ref_a, self.ref_b)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str, name: &str) -> Commit {
        Commit {
            hash: hash.into(),
            name: name.into(),
            status: crate::model::CommitStatus::Pushed,
            action: String::new(),
            tags: vec![],
            refs: vec![],
            extra_info: String::new(),
            author_name: "Test".into(),
            author_email: String::new(),
            unix_timestamp: 0,
            parents: vec![],
            divergence: crate::model::commit::Divergence::Left,
        }
    }

    #[test]
    fn compare_four_focuses_commits_and_tab_visits_all_five_panels() {
        let panels = [
            DiffModeFocus::SelectorA,
            DiffModeFocus::SelectorB,
            DiffModeFocus::CommitFiles,
            DiffModeFocus::Commits,
            DiffModeFocus::DiffExploration,
        ];
        for (i, panel) in panels.iter().enumerate() {
            assert_eq!(DiffModeFocus::from_number(i as u32 + 1), Some(*panel));
            assert_eq!(panel.next(), panels[(i + 1) % panels.len()]);
        }
        assert_eq!(DiffModeFocus::from_number(6), None);
    }

    #[test]
    fn compare_commit_preview_and_files_have_independent_diff_keys() {
        let mut state = DiffModeState::new();
        state.ref_a = "main".into();
        state.ref_b = "feature".into();
        state.commits = vec![commit("a", "first"), commit("b", "second")];
        let comparison_key = state.diff_key();
        state.set_focus(DiffModeFocus::Commits);
        assert_eq!(state.diff_key(), "DiffMode:commit:a");
        state.commits_selected = 1;
        state.set_focus(DiffModeFocus::DiffExploration);
        assert_eq!(state.diff_key(), "DiffMode:commit:b");
        state.set_focus(DiffModeFocus::CommitFiles);
        assert_eq!(state.diff_key(), comparison_key);
        state.files_commit = Some("b".into());
        state.set_focus(DiffModeFocus::CommitFiles);
        let commit_file_key = state.diff_key();
        assert_ne!(commit_file_key, comparison_key);
        state.set_focus(DiffModeFocus::DiffExploration);
        assert_eq!(state.diff_key(), commit_file_key);
    }

    #[test]
    fn compare_commit_search_targets_commits_and_clears_on_focus_change() {
        let mut state = DiffModeState::new();
        state.commits = vec![commit("a", "first"), commit("b", "second")];
        state.set_focus(DiffModeFocus::Commits);
        state.file_search_query = "second".into();
        state.update_file_search_matches();
        assert_eq!(state.file_search_matches, vec![1]);
        assert_eq!(state.commits_selected, 1);
        assert_eq!(state.diff_files_selected, 0);
        state.set_focus(DiffModeFocus::CommitFiles);
        assert!(state.file_search_query.is_empty());
        assert!(state.file_search_matches.is_empty());
    }

    #[test]
    fn compare_ref_changes_clear_commit_selection_files_and_search() {
        let mut state = DiffModeState::new();
        state.commits = vec![commit("a", "first")];
        state.files_commit = Some("a".into());
        state.commits_scroll = 5;
        state.file_search_query = "stale".into();
        state.start_editing(DiffModeSelector::A);
        state.textarea.as_mut().unwrap().insert_str("HEAD");
        state.confirm_selection();
        assert!(state.commits.is_empty());
        assert!(state.files_commit.is_none());
        assert_eq!(state.commits_scroll, 0);
        assert!(state.file_search_query.is_empty());
        assert_eq!(state.diff_source, CompareDiffSource::Comparison);
    }

    #[test]
    fn compare_counts_reverse_when_swapping_refs() {
        let mut state = DiffModeState::new();
        state.ref_a = "main".into();
        state.ref_b = "feature".into();
        state.ahead_behind = Some((1, 3));
        state.swap_refs();
        assert_eq!(state.ref_a, "feature");
        assert_eq!(state.ref_b, "main");
        assert_eq!(state.ahead_behind, Some((3, 1)));
    }

    #[test]
    fn compare_counts_clear_when_selecting_refs_or_reentering() {
        let mut state = DiffModeState::new();
        state.ahead_behind = Some((2, 1));
        state.start_editing(DiffModeSelector::A);
        state.textarea.as_mut().unwrap().insert_str("HEAD");
        state.confirm_selection();
        assert_eq!(state.ref_a, "HEAD");
        assert_eq!(state.ahead_behind, None);
        state.ahead_behind = Some((2, 1));
        state.exit();
        assert_eq!(state.ahead_behind, None);
        state.ahead_behind = Some((2, 1));
        state.enter(false);
        assert_eq!(state.ahead_behind, None);
    }
}

impl DiffModeFocus {
    pub fn from_number(n: u32) -> Option<Self> {
        match n {
            1 => Some(Self::SelectorA),
            2 => Some(Self::SelectorB),
            3 => Some(Self::CommitFiles),
            4 => Some(Self::Commits),
            5 => Some(Self::DiffExploration),
            _ => None,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::SelectorA => Self::SelectorB,
            Self::SelectorB => Self::CommitFiles,
            Self::CommitFiles => Self::Commits,
            Self::Commits => Self::DiffExploration,
            Self::DiffExploration => Self::SelectorA,
        }
    }
}

/// The kind of ref candidate shown in the search dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    RawRef,
    Branch,
    RemoteBranch,
    Tag,
    Commit,
}

/// A single candidate in the ref search dropdown.
#[derive(Debug, Clone)]
pub struct RefCandidate {
    pub display: String,
    pub ref_value: String,
    pub kind: RefKind,
}

/// Which selector combobox is being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffModeSelector {
    A,
    B,
}

/// State for the diff/compare mode screen.
pub struct DiffModeState {
    pub active: bool,

    // A and B refs
    pub ref_a: String,
    pub ref_b: String,
    pub ref_a_display: String,
    pub ref_b_display: String,
    /// Commits unique to A and B. None when refs aren't both commit-ish.
    pub ahead_behind: Option<(usize, usize)>,
    pub commits: Vec<Commit>,
    pub commits_selected: usize,
    pub commits_scroll: usize,
    pub commits_viewport_manually_scrolled: bool,
    pub commits_revision: u64,
    pub diff_source: CompareDiffSource,
    /// When inspecting a commit's files, the file list is relative to its first parent.
    pub files_commit: Option<String>,

    // Combobox editing state
    pub editing: Option<DiffModeSelector>,
    pub textarea: Option<tui_textarea::TextArea<'static>>,
    pub search_results: Vec<RefCandidate>,
    pub search_selected: usize,
    pub dropdown_scroll: usize,

    // Focus
    pub focus: DiffModeFocus,

    // Commit files for A..B diff
    pub diff_files: Vec<CommitFile>,
    pub diff_files_selected: usize,
    pub diff_files_scroll: usize,
    /// When true, render skips ensure_visible so viewport-only mouse scroll isn't undone.
    pub viewport_manually_scrolled: bool,

    // Tree view for commit files
    pub show_tree: bool,
    pub tree_nodes: Vec<crate::model::file_tree::CommitFileTreeNode>,
    pub collapsed_dirs: std::collections::HashSet<String>,

    // Search within commit files
    pub file_search_active: bool,
    pub file_search_query: String,
    pub file_search_matches: Vec<usize>,
    pub file_search_match_idx: usize,
    pub file_search_textarea: Option<tui_textarea::TextArea<'static>>,
}

impl DiffModeState {
    pub fn new() -> Self {
        Self {
            active: false,
            ref_a: String::new(),
            ref_b: String::new(),
            ref_a_display: String::new(),
            ref_b_display: String::new(),
            ahead_behind: None,
            commits: Vec::new(),
            commits_selected: 0,
            commits_scroll: 0,
            commits_viewport_manually_scrolled: false,
            commits_revision: 0,
            diff_source: CompareDiffSource::Comparison,
            files_commit: None,
            editing: None,
            textarea: None,
            search_results: Vec::new(),
            search_selected: 0,
            dropdown_scroll: 0,
            focus: DiffModeFocus::SelectorA,
            diff_files: Vec::new(),
            diff_files_selected: 0,
            diff_files_scroll: 0,
            viewport_manually_scrolled: false,
            show_tree: false,
            tree_nodes: Vec::new(),
            collapsed_dirs: std::collections::HashSet::new(),
            file_search_active: false,
            file_search_query: String::new(),
            file_search_matches: Vec::new(),
            file_search_match_idx: 0,
            file_search_textarea: None,
        }
    }

    pub fn enter(&mut self, show_tree: bool) {
        self.clear_commits();
        self.active = true;
        self.ref_a.clear();
        self.ref_b.clear();
        self.ref_a_display.clear();
        self.ref_b_display.clear();
        self.ahead_behind = None;
        self.editing = None;
        self.textarea = None;
        self.search_results.clear();
        self.search_selected = 0;
        self.dropdown_scroll = 0;
        self.focus = DiffModeFocus::SelectorA;
        self.diff_files.clear();
        self.diff_files_selected = 0;
        self.diff_files_scroll = 0;
        // Use the same persisted showFileTree preference as Files / Commit Files.
        self.show_tree = show_tree;
        self.tree_nodes.clear();
        self.collapsed_dirs.clear();
        self.file_search_active = false;
        self.file_search_query.clear();
        self.file_search_matches.clear();
        self.file_search_match_idx = 0;
        self.file_search_textarea = None;
    }

    pub fn exit(&mut self) {
        self.clear_commits();
        self.active = false;
        self.ahead_behind = None;
        self.editing = None;
        self.textarea = None;
        self.search_results.clear();
        self.diff_files.clear();
        self.tree_nodes.clear();
        self.collapsed_dirs.clear();
    }

    pub fn swap_refs(&mut self) {
        self.clear_commits();
        std::mem::swap(&mut self.ref_a, &mut self.ref_b);
        std::mem::swap(&mut self.ref_a_display, &mut self.ref_b_display);
        self.ahead_behind = self.ahead_behind.map(|(ahead, behind)| (behind, ahead));
        self.diff_files.clear();
        self.diff_files_selected = 0;
        self.diff_files_scroll = 0;
    }

    pub fn has_both_refs(&self) -> bool {
        !self.ref_a.is_empty() && !self.ref_b.is_empty()
    }

    /// Get the current query text from the textarea.
    pub fn query_text(&self) -> String {
        self.textarea
            .as_ref()
            .map(|ta| ta.lines()[0].clone())
            .unwrap_or_default()
    }

    /// Start editing a selector combobox with a textarea.
    pub fn start_editing(&mut self, selector: DiffModeSelector) {
        self.editing = Some(selector);
        // Pre-fill with the ref value (e.g. short hash), not the display string
        // (which may include a long commit message)
        let prefill = match selector {
            DiffModeSelector::A => &self.ref_a,
            DiffModeSelector::B => &self.ref_b,
        };
        let mut ta = crate::gui::popup::make_textarea("Type a branch, tag, commit, or ref...");
        if !prefill.is_empty() {
            ta.insert_str(prefill);
        }
        self.textarea = Some(ta);
        self.search_results.clear();
        self.search_selected = 0;
        self.dropdown_scroll = 0;
    }

    /// Cancel editing without applying.
    pub fn cancel_editing(&mut self) {
        self.editing = None;
        self.textarea = None;
        self.search_results.clear();
        self.search_selected = 0;
        self.dropdown_scroll = 0;
    }

    /// Apply the selected search result (or raw query) to the active selector.
    pub fn confirm_selection(&mut self) {
        let Some(selector) = self.editing else { return };

        let query = self.query_text();
        let (ref_value, display) =
            if let Some(candidate) = self.search_results.get(self.search_selected) {
                (candidate.ref_value.clone(), candidate.display.clone())
            } else if !query.is_empty() {
                // Allow raw input like HEAD~1, commit hashes, etc.
                (query.clone(), query)
            } else {
                self.editing = None;
                self.textarea = None;
                return;
            };

        match selector {
            DiffModeSelector::A => {
                self.ref_a = ref_value;
                self.ref_a_display = display;
            }
            DiffModeSelector::B => {
                self.ref_b = ref_value;
                self.ref_b_display = display;
            }
        }

        self.clear_commits();
        self.ahead_behind = None;
        self.editing = None;
        self.textarea = None;
        self.search_results.clear();
        self.search_selected = 0;
        self.dropdown_scroll = 0;
        self.diff_files.clear();
        self.diff_files_selected = 0;
        self.diff_files_scroll = 0;
    }

    /// Build all ref candidates and scroll to the best match for the current query.
    /// All items are always shown — the query just moves the cursor to the best match.
    pub fn search_refs(
        &mut self,
        branches: &[Branch],
        tags: &[Tag],
        commits: &[Commit],
        remotes: &[Remote],
        head_branch_name: &str,
    ) {
        self.search_results.clear();

        // Current branch first (if it exists)
        if !head_branch_name.is_empty() {
            if let Some(branch) = branches.iter().find(|b| b.name == head_branch_name) {
                self.search_results.push(RefCandidate {
                    display: branch.name.clone(),
                    ref_value: branch.name.clone(),
                    kind: RefKind::Branch,
                });
            }
        }

        // Local branches (skip the head branch we already added)
        for branch in branches {
            if branch.name == head_branch_name {
                continue;
            }
            self.search_results.push(RefCandidate {
                display: branch.name.clone(),
                ref_value: branch.name.clone(),
                kind: RefKind::Branch,
            });
        }

        // Remote branches
        for remote in remotes {
            for rb in &remote.branches {
                let full = rb.full_name();
                self.search_results.push(RefCandidate {
                    display: full.clone(),
                    ref_value: full,
                    kind: RefKind::RemoteBranch,
                });
            }
        }

        // Tags
        for tag in tags {
            self.search_results.push(RefCandidate {
                display: tag.name.clone(),
                ref_value: tag.name.clone(),
                kind: RefKind::Tag,
            });
        }

        // Commits
        for commit in commits.iter().take(200) {
            let hash_short = if commit.hash.len() >= 7 {
                &commit.hash[..7]
            } else {
                &commit.hash
            };
            let display = format!("{} {}", hash_short, commit.name);
            self.search_results.push(RefCandidate {
                display,
                ref_value: commit.hash.clone(),
                kind: RefKind::Commit,
            });
        }

        // When there's a query, add a raw ref option at the top so the user
        // can always select exactly what they typed (e.g. HEAD~1, HEAD^2).
        let q = self.query_text();
        if !q.is_empty() {
            self.search_results.insert(
                0,
                RefCandidate {
                    display: q.clone(),
                    ref_value: q.clone(),
                    kind: RefKind::RawRef,
                },
            );

            // Jump cursor to best match among the real candidates (skip the raw ref at 0)
            let q_lower = q.to_lowercase();
            if let Some(idx) = self.search_results.iter().skip(1).position(|c| {
                c.display.to_lowercase().contains(&q_lower)
                    || c.ref_value.to_lowercase().starts_with(&q_lower)
            }) {
                self.search_selected = idx + 1; // +1 because we skipped raw ref
            } else {
                // No match — stay on the raw ref option
                self.search_selected = 0;
            }
        } else {
            self.search_selected = 0;
        }

        self.ensure_dropdown_visible(10);
    }

    /// Ensure the dropdown scroll keeps the selected item visible.
    pub fn ensure_dropdown_visible(&mut self, max_visible: usize) {
        if max_visible == 0 {
            return;
        }
        if self.search_selected < self.dropdown_scroll {
            self.dropdown_scroll = self.search_selected;
        } else if self.search_selected >= self.dropdown_scroll + max_visible {
            self.dropdown_scroll = self.search_selected + 1 - max_visible;
        }
    }

    /// Number of visible commit files (accounts for tree view).
    pub fn visible_files_len(&self) -> usize {
        if self.show_tree {
            self.tree_nodes.len()
        } else {
            self.diff_files.len()
        }
    }

    /// Update file search matches based on current query.
    pub fn update_file_search_matches(&mut self) {
        self.file_search_matches.clear();
        if self.file_search_query.is_empty() {
            return;
        }

        let query = self.file_search_query.to_lowercase();

        if self.focus == DiffModeFocus::Commits {
            for (i, commit) in self.commits.iter().enumerate() {
                if [&commit.hash, &commit.name, &commit.author_name]
                    .iter()
                    .any(|text| text.to_lowercase().contains(&query))
                {
                    self.file_search_matches.push(i);
                }
            }
        } else if self.show_tree {
            for (i, node) in self.tree_nodes.iter().enumerate() {
                if node.path.to_lowercase().contains(&query)
                    || node.name.to_lowercase().contains(&query)
                {
                    self.file_search_matches.push(i);
                }
            }
        } else {
            for (i, file) in self.diff_files.iter().enumerate() {
                if file.name.to_lowercase().contains(&query) {
                    self.file_search_matches.push(i);
                }
            }
        }

        // Auto-jump to first match
        if !self.file_search_matches.is_empty() {
            self.file_search_match_idx = 0;
            self.select_list_match(self.file_search_matches[0]);
        }
    }

    /// Go to next file search match.
    pub fn goto_next_file_search_match(&mut self) {
        if self.file_search_matches.is_empty() {
            return;
        }
        self.file_search_match_idx =
            (self.file_search_match_idx + 1) % self.file_search_matches.len();
        self.select_list_match(self.file_search_matches[self.file_search_match_idx]);
    }

    /// Go to previous file search match.
    pub fn goto_prev_file_search_match(&mut self) {
        if self.file_search_matches.is_empty() {
            return;
        }
        if self.file_search_match_idx == 0 {
            self.file_search_match_idx = self.file_search_matches.len() - 1;
        } else {
            self.file_search_match_idx -= 1;
        }
        self.select_list_match(self.file_search_matches[self.file_search_match_idx]);
    }
}
