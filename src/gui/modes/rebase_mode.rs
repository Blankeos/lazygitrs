use crate::git::rebase::{RebaseAction, RebaseProgress, TodoEntry};
use crate::model::Commit;

/// Phase of the interactive rebase mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebasePhase {
    /// Planning: user is configuring actions before starting the rebase.
    Planning,
    /// InProgress: rebase is running but paused (conflict, edit, etc.).
    InProgress,
}

/// Status of each entry in the rebase list during InProgress phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryStatus {
    /// Already processed by git.
    Done,
    /// The commit where the rebase paused (conflict/edit).
    Current,
    /// Not yet processed.
    Pending,
}

/// A single entry in the interactive rebase todo list.
#[derive(Debug, Clone)]
pub struct RebaseEntry {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author_name: String,
    pub unix_timestamp: i64,
    pub action: RebaseAction,
    /// Status during InProgress phase (Done/Current/Pending).
    /// In Planning phase, all entries are Pending.
    pub status: EntryStatus,
}

/// State for the interactive rebase mode screen.
pub struct RebaseModeState {
    pub active: bool,
    pub phase: RebasePhase,
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
    /// Height of the list viewport in rows. Updated on each render so that
    /// keyboard / mouse handlers can scroll relative to the actual rendered size.
    pub visible_height: usize,
    // Progress counters for InProgress phase
    pub done_count: usize,
    pub total_count: usize,
}

impl RebaseModeState {
    pub fn new() -> Self {
        Self {
            active: false,
            phase: RebasePhase::Planning,
            branch_name: String::new(),
            base_hash: String::new(),
            base_short_hash: String::new(),
            base_message: String::new(),
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            visible_height: 0,
            done_count: 0,
            total_count: 0,
        }
    }

    /// Enter interactive rebase mode (Planning phase).
    /// `commits` should be in newest-first order (as displayed in the commits panel).
    /// The base commit is the "onto" target (not included in the todo list).
    pub fn enter(
        &mut self,
        branch_name: String,
        base_commit: &Commit,
        commits: &[Commit],
    ) {
        self.active = true;
        self.phase = RebasePhase::Planning;
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
                status: EntryStatus::Pending,
            })
            .collect();

        // Select the first entry (newest commit, at the top).
        self.selected = 0;
        self.scroll = 0;
        self.done_count = 0;
        self.total_count = self.entries.len();
    }

    /// Enter InProgress phase from a rebase-in-progress state detected on disk.
    /// Entries are displayed in newest-first order (same as Planning phase),
    /// with the base commit shown at the bottom.
    pub fn enter_in_progress(&mut self, progress: &RebaseProgress) {
        self.active = true;
        self.phase = RebasePhase::InProgress;
        self.branch_name = progress.head_name.clone();
        self.base_hash = progress.onto_hash.clone();
        self.base_short_hash = progress.onto_short.clone();
        self.base_message = progress.onto_message.clone();

        // Build entries in oldest-first (git order), then reverse for display.
        let mut entries = Vec::new();

        // Done entries
        for todo in &progress.done_entries {
            entries.push(entry_from_todo(todo, EntryStatus::Done));
        }

        // Mark the last done entry as Current if it matches stopped_sha
        if !progress.stopped_sha.is_empty() && !entries.is_empty() {
            let last = entries.len() - 1;
            if entries[last].hash.starts_with(&progress.stopped_sha)
                || progress.stopped_sha.starts_with(&entries[last].hash)
            {
                entries[last].status = EntryStatus::Current;
            }
        }

        // Remaining entries
        for todo in &progress.todo_entries {
            entries.push(entry_from_todo(todo, EntryStatus::Pending));
        }

        self.done_count = progress.done_entries.len();
        self.total_count = progress.done_entries.len() + progress.todo_entries.len();

        // Reverse to newest-first for display (same convention as Planning)
        entries.reverse();

        // Select the current (paused) entry
        let current_idx = entries.iter().position(|e| e.status == EntryStatus::Current);
        self.selected = current_idx.unwrap_or(0);

        self.entries = entries;
        self.scroll = 0;
    }

    pub fn exit(&mut self) {
        self.active = false;
        self.phase = RebasePhase::Planning;
        self.entries.clear();
        self.branch_name.clear();
        self.base_hash.clear();
        self.base_short_hash.clear();
        self.base_message.clear();
        self.selected = 0;
        self.scroll = 0;
        self.done_count = 0;
        self.total_count = 0;
    }

    /// Set the action on the currently selected entry (Planning phase only).
    pub fn set_action(&mut self, action: RebaseAction) {
        if self.phase != RebasePhase::Planning {
            return;
        }
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.action = action;
        }
    }

    /// Cycle the selected entry's action forward (Planning phase only).
    pub fn cycle_action_forward(&mut self) {
        if self.phase != RebasePhase::Planning {
            return;
        }
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.action = entry.action.next();
        }
    }

    /// Cycle the selected entry's action backward (Planning phase only).
    pub fn cycle_action_backward(&mut self) {
        if self.phase != RebasePhase::Planning {
            return;
        }
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.action = entry.action.prev();
        }
    }

    /// Move the selected entry up (Planning phase only).
    pub fn move_up(&mut self) {
        if self.phase != RebasePhase::Planning {
            return;
        }
        if self.selected > 0 {
            self.entries.swap(self.selected, self.selected - 1);
            self.selected -= 1;
        }
    }

    /// Move the selected entry down (Planning phase only).
    pub fn move_down(&mut self) {
        if self.phase != RebasePhase::Planning {
            return;
        }
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

    /// Ensure scroll keeps selected item visible. When selected is the last
    /// entry, also keep the trailing base-commit row visible (rendered just
    /// below the last entry as a dim "onto" target).
    pub fn ensure_visible(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        // Treat the row below the selected entry as also-needed-visible when
        // selected is the last entry, so the base commit doesn't get clipped.
        let on_last = !self.entries.is_empty() && self.selected + 1 == self.entries.len();
        let bottom_target = if on_last { self.selected + 1 } else { self.selected };
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if bottom_target >= self.scroll + visible_height {
            self.scroll = bottom_target + 1 - visible_height;
        }
    }

    /// Number of remaining (pending) entries.
    pub fn remaining_count(&self) -> usize {
        self.total_count.saturating_sub(self.done_count)
    }
}

/// Helper: create a RebaseEntry from a TodoEntry with a given status.
fn entry_from_todo(todo: &TodoEntry, status: EntryStatus) -> RebaseEntry {
    RebaseEntry {
        hash: todo.hash.clone(),
        short_hash: todo.short_hash.clone(),
        message: todo.message.clone(),
        author_name: todo.author_name.clone(),
        unix_timestamp: todo.unix_timestamp,
        action: todo.action,
        status,
    }
}
