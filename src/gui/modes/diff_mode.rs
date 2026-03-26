use crate::model::{Branch, Commit, CommitFile, Remote, Tag};

/// Which panel is focused within diff mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffModeFocus {
    SelectorA,
    SelectorB,
    CommitFiles,
    DiffExploration,
}

impl DiffModeFocus {
    pub fn from_number(n: u32) -> Option<Self> {
        match n {
            1 => Some(Self::SelectorA),
            2 => Some(Self::SelectorB),
            3 => Some(Self::CommitFiles),
            4 => Some(Self::DiffExploration),
            _ => None,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Self::SelectorA => Self::SelectorB,
            Self::SelectorB => Self::CommitFiles,
            Self::CommitFiles => Self::DiffExploration,
            Self::DiffExploration => Self::SelectorA,
        }
    }
}

/// The kind of ref candidate shown in the search dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
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

    // Tree view for commit files
    pub show_tree: bool,
    pub tree_nodes: Vec<crate::model::file_tree::CommitFileTreeNode>,
    pub collapsed_dirs: std::collections::HashSet<String>,
}

impl DiffModeState {
    pub fn new() -> Self {
        Self {
            active: false,
            ref_a: String::new(),
            ref_b: String::new(),
            ref_a_display: String::new(),
            ref_b_display: String::new(),
            editing: None,
            textarea: None,
            search_results: Vec::new(),
            search_selected: 0,
            dropdown_scroll: 0,
            focus: DiffModeFocus::SelectorA,
            diff_files: Vec::new(),
            diff_files_selected: 0,
            diff_files_scroll: 0,
            show_tree: false,
            tree_nodes: Vec::new(),
            collapsed_dirs: std::collections::HashSet::new(),
        }
    }

    pub fn enter(&mut self) {
        self.active = true;
        self.ref_a.clear();
        self.ref_b.clear();
        self.ref_a_display.clear();
        self.ref_b_display.clear();
        self.editing = None;
        self.textarea = None;
        self.search_results.clear();
        self.search_selected = 0;
        self.dropdown_scroll = 0;
        self.focus = DiffModeFocus::SelectorA;
        self.diff_files.clear();
        self.diff_files_selected = 0;
        self.diff_files_scroll = 0;
        self.show_tree = false;
        self.tree_nodes.clear();
        self.collapsed_dirs.clear();
    }

    pub fn exit(&mut self) {
        self.active = false;
        self.editing = None;
        self.textarea = None;
        self.search_results.clear();
        self.diff_files.clear();
        self.tree_nodes.clear();
        self.collapsed_dirs.clear();
    }

    pub fn swap_refs(&mut self) {
        std::mem::swap(&mut self.ref_a, &mut self.ref_b);
        std::mem::swap(&mut self.ref_a_display, &mut self.ref_b_display);
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
        let (ref_value, display) = if let Some(candidate) = self.search_results.get(self.search_selected) {
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
    ) {
        self.search_results.clear();

        // Local branches
        for branch in branches {
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
            let hash_short = if commit.hash.len() >= 7 { &commit.hash[..7] } else { &commit.hash };
            let display = format!("{} {}", hash_short, commit.name);
            self.search_results.push(RefCandidate {
                display,
                ref_value: commit.hash.clone(),
                kind: RefKind::Commit,
            });
        }

        // Jump cursor to best match
        let q = self.query_text().to_lowercase();
        if !q.is_empty() {
            // Find first item whose display or ref_value matches
            if let Some(idx) = self.search_results.iter().position(|c| {
                c.display.to_lowercase().contains(&q)
                    || c.ref_value.to_lowercase().starts_with(&q)
            }) {
                self.search_selected = idx;
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
}
