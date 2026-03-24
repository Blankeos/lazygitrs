use crate::model::Model;

/// Identifies which panel/context is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextId {
    Status,
    Files,
    Branches,
    Commits,
    Stash,
    Remotes,
    Tags,
    CommitFiles,
    Staging,
}

impl ContextId {
    /// The sidebar panels in order (for tab navigation).
    pub const SIDEBAR_ORDER: &[ContextId] = &[
        ContextId::Status,
        ContextId::Files,
        ContextId::Branches,
        ContextId::Commits,
        ContextId::Stash,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Files => "Files",
            Self::Branches => "Branches",
            Self::Commits => "Commits",
            Self::Stash => "Stash",
            Self::Remotes => "Remotes",
            Self::Tags => "Tags",
            Self::CommitFiles => "Commit Files",
            Self::Staging => "Staging",
        }
    }

    pub fn short_key(&self) -> &'static str {
        match self {
            Self::Status => "1",
            Self::Files => "2",
            Self::Branches => "3",
            Self::Commits => "4",
            Self::Stash => "5",
            _ => "",
        }
    }
}

/// Manages which context is active and selection state per context.
pub struct ContextManager {
    active: ContextId,
    selections: std::collections::HashMap<ContextId, usize>,
    scroll_offsets: std::collections::HashMap<ContextId, usize>,
}

impl ContextManager {
    pub fn new() -> Self {
        let mut selections = std::collections::HashMap::new();
        let scroll_offsets = std::collections::HashMap::new();

        for ctx in ContextId::SIDEBAR_ORDER {
            selections.insert(*ctx, 0);
        }

        Self {
            active: ContextId::Files,
            selections,
            scroll_offsets,
        }
    }

    pub fn active(&self) -> ContextId {
        self.active
    }

    pub fn set_active(&mut self, ctx: ContextId) {
        self.active = ctx;
    }

    pub fn next_context(&mut self) {
        let order = ContextId::SIDEBAR_ORDER;
        if let Some(idx) = order.iter().position(|c| *c == self.active) {
            self.active = order[(idx + 1) % order.len()];
        }
    }

    pub fn prev_context(&mut self) {
        let order = ContextId::SIDEBAR_ORDER;
        if let Some(idx) = order.iter().position(|c| *c == self.active) {
            self.active = order[(idx + order.len() - 1) % order.len()];
        }
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
            ContextId::Files => model.files.len(),
            ContextId::Branches => model.branches.len(),
            ContextId::Commits => model.commits.len(),
            ContextId::Stash => model.stash_entries.len(),
            ContextId::Remotes => model.remotes.len(),
            ContextId::Tags => model.tags.len(),
            _ => 0,
        }
    }

    /// Clamp selection after data refresh (list may have shrunk).
    pub fn clamp_selections(&mut self, model: &Model) {
        for ctx in ContextId::SIDEBAR_ORDER {
            let len = match ctx {
                ContextId::Status => 1,
                ContextId::Files => model.files.len(),
                ContextId::Branches => model.branches.len(),
                ContextId::Commits => model.commits.len(),
                ContextId::Stash => model.stash_entries.len(),
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
