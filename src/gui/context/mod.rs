use crate::model::Model;

/// Identifies which panel/context is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextId {
    Status,
    Files,
    Worktrees,
    Submodules,
    Branches,
    Remotes,
    Tags,
    Commits,
    Reflog,
    Stash,
    CommitFiles,
    StashFiles,
    BranchCommits,
    BranchCommitFiles,
    RemoteBranches,
    Staging,
}

/// The 5 side windows, matching lazygit's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SideWindow {
    Status,   // 1
    Files,    // 2: Files / Worktrees / Submodules
    Branches, // 3: Branches / Remotes / Tags
    Commits,  // 4: Commits
    Stash,    // 5
}

impl SideWindow {
    pub const ALL: &[SideWindow] = &[
        SideWindow::Status,
        SideWindow::Files,
        SideWindow::Branches,
        SideWindow::Commits,
        SideWindow::Stash,
    ];

    /// The sub-tabs within this window.
    pub fn tabs(&self) -> &[ContextId] {
        match self {
            Self::Status => &[ContextId::Status],
            Self::Files => &[ContextId::Files, ContextId::Worktrees, ContextId::Submodules],
            Self::Branches => &[ContextId::Branches, ContextId::Remotes, ContextId::Tags],
            Self::Commits => &[ContextId::Commits, ContextId::Reflog],
            Self::Stash => &[ContextId::Stash],
        }
    }

    /// Given an x-coordinate relative to the panel left edge, determine which
    /// tab was clicked in the title bar.  Returns `None` if the click didn't
    /// land on a tab label.
    pub fn tab_at_x(&self, x: u16) -> Option<ContextId> {
        let tabs = self.tabs();
        if tabs.len() <= 1 {
            return None;
        }
        // Title layout: " {key} {Tab0} | {Tab1} | {Tab2} "
        // The border takes 1 char, so content starts at x=1 inside the panel.
        let key = self.key_label();
        // " {key} " prefix length
        let mut cursor: u16 = 1 + key.len() as u16 + 1; // border + key + space

        for (i, ctx) in tabs.iter().enumerate() {
            if i > 0 {
                cursor += 3; // " | "
            }
            let label_len = ctx.title().len() as u16;
            if x >= cursor && x < cursor + label_len {
                return Some(*ctx);
            }
            cursor += label_len;
        }
        None
    }

    /// Number key label for this window.
    pub fn key_label(&self) -> &'static str {
        match self {
            Self::Status => "1",
            Self::Files => "2",
            Self::Branches => "3",
            Self::Commits => "4",
            Self::Stash => "5",
        }
    }

    /// Which window does a context belong to?
    pub fn for_context(ctx: ContextId) -> SideWindow {
        match ctx {
            ContextId::Status => SideWindow::Status,
            ContextId::Files | ContextId::Worktrees | ContextId::Submodules => SideWindow::Files,
            ContextId::Branches | ContextId::Remotes | ContextId::Tags | ContextId::BranchCommits | ContextId::BranchCommitFiles | ContextId::RemoteBranches => SideWindow::Branches,
            ContextId::Commits | ContextId::Reflog | ContextId::CommitFiles => SideWindow::Commits,
            ContextId::Stash | ContextId::StashFiles => SideWindow::Stash,
            ContextId::Staging => SideWindow::Files,
        }
    }

    /// From a 1-based number key.
    pub fn from_number(n: u32) -> Option<SideWindow> {
        match n {
            1 => Some(SideWindow::Status),
            2 => Some(SideWindow::Files),
            3 => Some(SideWindow::Branches),
            4 => Some(SideWindow::Commits),
            5 => Some(SideWindow::Stash),
            _ => None,
        }
    }
}

impl ContextId {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Files => "Files",
            Self::Worktrees => "Worktrees",
            Self::Submodules => "Submodules",
            Self::Branches => "Branches",
            Self::Remotes => "Remotes",
            Self::Tags => "Tags",
            Self::Commits => "Commits",
            Self::Reflog => "Reflog",
            Self::Stash => "Stash",
            Self::CommitFiles => "Commit Files",
            Self::StashFiles => "Stash Files",
            Self::BranchCommits => "Branch Commits",
            Self::BranchCommitFiles => "Branch Commit Files",
            Self::RemoteBranches => "Remote Branches",
            Self::Staging => "Staging",
        }
    }
}

/// Manages which context is active and selection state per context.
pub struct ContextManager {
    active: ContextId,
    /// Which tab is active within each window.
    window_tabs: std::collections::HashMap<SideWindow, usize>,
    /// The last active context per window (including sub-views like BranchCommits).
    window_last_context: std::collections::HashMap<SideWindow, ContextId>,
    selections: std::collections::HashMap<ContextId, usize>,
    scroll_offsets: std::collections::HashMap<ContextId, usize>,
    /// Override for files list length when tree view is active.
    pub files_list_len_override: Option<usize>,
    /// Override for commit files list length when tree view is active.
    pub commit_files_list_len_override: Option<usize>,
}

impl ContextManager {
    pub fn new() -> Self {
        let mut selections = std::collections::HashMap::new();
        let mut window_tabs = std::collections::HashMap::new();

        // Initialize all contexts with selection 0
        for window in SideWindow::ALL {
            window_tabs.insert(*window, 0);
            for ctx in window.tabs() {
                selections.insert(*ctx, 0);
            }
        }

        // Dynamic sub-contexts, initialize their selections
        selections.insert(ContextId::CommitFiles, 0);
        selections.insert(ContextId::StashFiles, 0);
        selections.insert(ContextId::BranchCommits, 0);
        selections.insert(ContextId::BranchCommitFiles, 0);
        selections.insert(ContextId::RemoteBranches, 0);

        Self {
            active: ContextId::Files,
            window_tabs,
            window_last_context: std::collections::HashMap::new(),
            selections,
            scroll_offsets: std::collections::HashMap::new(),
            files_list_len_override: None,
            commit_files_list_len_override: None,
        }
    }

    pub fn active(&self) -> ContextId {
        self.active
    }

    pub fn set_active(&mut self, ctx: ContextId) {
        self.active = ctx;
        let window = SideWindow::for_context(ctx);
        // Always remember the last active context for this window (including sub-views).
        self.window_last_context.insert(window, ctx);
        // Update the window tab index (only for root tabs).
        if let Some(idx) = window.tabs().iter().position(|c| *c == ctx) {
            self.window_tabs.insert(window, idx);
        }
    }

    /// Get the last active context for a window, including sub-views.
    /// Falls back to `active_context_for_window` if no sub-view was recorded.
    pub fn last_context_for_window(&self, window: SideWindow) -> ContextId {
        self.window_last_context
            .get(&window)
            .copied()
            .unwrap_or_else(|| self.active_context_for_window(window))
    }

    /// Get the active context for a given window.
    pub fn active_context_for_window(&self, window: SideWindow) -> ContextId {
        let tab_idx = self.window_tabs.get(&window).copied().unwrap_or(0);
        let tabs = window.tabs();
        tabs.get(tab_idx).copied().unwrap_or(tabs[0])
    }

    /// Get which window is currently active.
    pub fn active_window(&self) -> SideWindow {
        SideWindow::for_context(self.active)
    }

    /// Navigate to the next window (for tab/right arrow).
    pub fn next_window(&mut self) {
        let windows = SideWindow::ALL;
        let current = self.active_window();
        if let Some(idx) = windows.iter().position(|w| *w == current) {
            let next = windows[(idx + 1) % windows.len()];
            self.active = self.active_context_for_window(next);
        }
    }

    /// Navigate to the previous window (for left arrow).
    pub fn prev_window(&mut self) {
        let windows = SideWindow::ALL;
        let current = self.active_window();
        if let Some(idx) = windows.iter().position(|w| *w == current) {
            let prev = windows[(idx + windows.len() - 1) % windows.len()];
            self.active = self.active_context_for_window(prev);
        }
    }

    /// Jump to a window by number (1-5). If already in that window, cycle tabs.
    /// Jump to a window's active tab without cycling (even if already on that window).
    pub fn set_window(&mut self, window: SideWindow) {
        self.active = self.active_context_for_window(window);
    }

    pub fn jump_to_window(&mut self, window: SideWindow) {
        let current_window = self.active_window();
        if current_window == window {
            // Cycle to next tab within this window
            self.next_tab();
        } else {
            // Jump to this window's active tab
            self.active = self.active_context_for_window(window);
        }
    }

    /// Cycle to next tab within the current window.
    pub fn next_tab(&mut self) {
        let window = self.active_window();
        let tabs = window.tabs();
        if tabs.len() <= 1 {
            return;
        }
        let current_idx = self.window_tabs.get(&window).copied().unwrap_or(0);
        let next_idx = (current_idx + 1) % tabs.len();
        self.window_tabs.insert(window, next_idx);
        self.active = tabs[next_idx];
    }

    /// Cycle to previous tab within the current window.
    pub fn prev_tab(&mut self) {
        let window = self.active_window();
        let tabs = window.tabs();
        if tabs.len() <= 1 {
            return;
        }
        let current_idx = self.window_tabs.get(&window).copied().unwrap_or(0);
        let prev_idx = (current_idx + tabs.len() - 1) % tabs.len();
        self.window_tabs.insert(window, prev_idx);
        self.active = tabs[prev_idx];
    }

    pub fn selected(&self, ctx: ContextId) -> usize {
        self.selections.get(&ctx).copied().unwrap_or(0)
    }

    pub fn selected_active(&self) -> usize {
        self.selected(self.active)
    }

    pub fn set_selection(&mut self, idx: usize) {
        self.selections.insert(self.active, idx);
    }

    pub fn scroll_offset(&self, ctx: ContextId) -> usize {
        self.scroll_offsets.get(&ctx).copied().unwrap_or(0)
    }

    pub fn set_scroll_offset(&mut self, ctx: ContextId, offset: usize) {
        self.scroll_offsets.insert(ctx, offset);
    }

    /// Adjust the scroll offset for a context so that `selected` is visible
    /// within a viewport of `visible_height` rows.  Only scrolls when the
    /// cursor would otherwise be outside the visible window.
    pub fn ensure_scroll_visible(&mut self, ctx: ContextId, visible_height: usize) {
        let selected = self.selected(ctx);
        let mut offset = self.scroll_offset(ctx);
        super::scroll::ensure_visible(selected, &mut offset, visible_height);
        self.set_scroll_offset(ctx, offset);
    }

    pub fn move_selection(&mut self, delta: isize, model: &Model) {
        let len = self.list_len(model);
        if len == 0 {
            return;
        }
        let current = self.selected_active();
        let new_idx = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            (current + delta as usize).min(len - 1)
        };
        self.set_selection(new_idx);
    }

    pub fn list_len(&self, model: &Model) -> usize {
        match self.active {
            ContextId::Status => 1,
            ContextId::Files => self.files_list_len_override.unwrap_or(model.files.len()),
            ContextId::Branches => model.branches.len(),
            ContextId::Commits => model.commits.len(),
            ContextId::Reflog => model.reflog_commits.len(),
            ContextId::Stash => model.stash_entries.len(),
            ContextId::Remotes => model.remotes.len(),
            ContextId::Tags => model.tags.len(),
            ContextId::Worktrees => model.worktrees.len(),
            ContextId::Submodules => model.submodules.len(),
            ContextId::CommitFiles | ContextId::StashFiles | ContextId::BranchCommitFiles => self.commit_files_list_len_override.unwrap_or(model.commit_files.len()),
            ContextId::BranchCommits => model.sub_commits.len(),
            ContextId::RemoteBranches => model.sub_remote_branches.len(),
            _ => 0,
        }
    }

    /// Clamp selection after data refresh (list may have shrunk).
    pub fn clamp_selections(&mut self, model: &Model) {
        for window in SideWindow::ALL {
            for ctx in window.tabs() {
                let len = match ctx {
                    ContextId::Status => 1,
                    ContextId::Files => self.files_list_len_override.unwrap_or(model.files.len()),
                    ContextId::Branches => model.branches.len(),
                    ContextId::Commits => model.commits.len(),
                    ContextId::Reflog => model.reflog_commits.len(),
                    ContextId::Stash => model.stash_entries.len(),
                    ContextId::Remotes => model.remotes.len(),
                    ContextId::Tags => model.tags.len(),
                    ContextId::Worktrees => model.worktrees.len(),
                    _ => 0,
                };
                if let Some(sel) = self.selections.get_mut(ctx) {
                    if len == 0 {
                        *sel = 0;
                    } else if *sel >= len {
                        *sel = len - 1;
                    }
                }
            }
        }
    }
}
