use crate::git::rebase::RebaseAction;
use crate::model::Commit;

/// A single entry in the interactive rebase todo list.
#[derive(Debug, Clone)]
pub struct RebaseEntry {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author_name: String,
    pub unix_timestamp: i64,
    pub action: RebaseAction,
}

/// State for the interactive rebase mode screen.
pub struct RebaseModeState {
    pub active: bool,
    /// The branch being rebased.
    pub branch_name: String,
    /// The base commit hash (rebasing onto this; not included in entries).
    pub base_hash: String,
    /// Short hash of the base commit for display.
    pub base_short_hash: String,
    /// Message of the base commit for display.
    pub base_message: String,
    /// The rebase todo entries, in display order (newest first, oldest last).
    /// Reversed to rebase-todo order (oldest first) when building actions for git.
    pub entries: Vec<RebaseEntry>,
    /// Currently selected entry index.
    pub selected: usize,
    /// Scroll offset for the list.
    pub scroll: usize,
}

impl RebaseModeState {
    pub fn new() -> Self {
        Self {
            active: false,
            branch_name: String::new(),
            base_hash: String::new(),
            base_short_hash: String::new(),
            base_message: String::new(),
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
        }
    }

    /// Enter interactive rebase mode.
    /// `commits` should be in newest-first order (as displayed in the commits panel).
    /// The base commit is the "onto" target (not included in the todo list).
    pub fn enter(
        &mut self,
        branch_name: String,
        base_commit: &Commit,
        commits: &[Commit],
    ) {
        self.active = true;
        self.branch_name = branch_name;
        self.base_hash = base_commit.hash.clone();
        self.base_short_hash = base_commit.short_hash().to_string();
        self.base_message = base_commit.name.clone();

        // Keep newest-first order (same as commits panel display).
        self.entries = commits
            .iter()
            .map(|c| RebaseEntry {
                hash: c.hash.clone(),
                short_hash: c.short_hash().to_string(),
                message: c.name.clone(),
                author_name: c.author_name.clone(),
                unix_timestamp: c.unix_timestamp,
                action: RebaseAction::Pick,
            })
            .collect();

        // Select the first entry (newest commit, at the top).
        self.selected = 0;
        self.scroll = 0;
    }

    pub fn exit(&mut self) {
        self.active = false;
        self.entries.clear();
        self.branch_name.clear();
        self.base_hash.clear();
        self.base_short_hash.clear();
        self.base_message.clear();
        self.selected = 0;
        self.scroll = 0;
    }

    /// Set the action on the currently selected entry.
    pub fn set_action(&mut self, action: RebaseAction) {
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.action = action;
        }
    }

    /// Cycle the selected entry's action forward.
    pub fn cycle_action_forward(&mut self) {
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.action = entry.action.next();
        }
    }

    /// Cycle the selected entry's action backward.
    pub fn cycle_action_backward(&mut self) {
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.action = entry.action.prev();
        }
    }

    /// Move the selected entry up (swap with the one above).
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.entries.swap(self.selected, self.selected - 1);
            self.selected -= 1;
        }
    }

    /// Move the selected entry down (swap with the one below).
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.entries.swap(self.selected, self.selected + 1);
            self.selected += 1;
        }
    }

    /// Build the actions list for `rebase_interactive_batch`.
    /// Returns in oldest-first order (git rebase todo order).
    pub fn build_actions(&self) -> Vec<(String, RebaseAction)> {
        self.entries
            .iter()
            .rev() // display is newest-first, git needs oldest-first
            .map(|e| (e.hash.clone(), e.action))
            .collect()
    }

    /// Ensure scroll keeps selected item visible.
    pub fn ensure_visible(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible_height {
            self.scroll = self.selected + 1 - visible_height;
        }
    }
}
