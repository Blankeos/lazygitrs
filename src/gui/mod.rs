pub mod context;
pub mod controller;
pub mod layout;
pub mod modes;
pub mod popup;
pub mod presentation;
pub mod views;

use std::collections::HashSet;
use std::io::{self, Stdout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, MouseEvent};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, cursor};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::config::AppConfig;
use crate::config::keybindings::parse_key;
use crate::git::GitCommands;
use crate::model::Model;
use crate::model::file_tree::{build_file_tree, FileTreeNode};
use crate::pager::side_by_side::DiffViewState;

use self::context::{ContextId, ContextManager, SideWindow};
use self::layout::LayoutState;
use self::modes::patch_building::PatchBuildingState;
use self::popup::PopupState;

pub type Term = Terminal<CrosstermBackend<Stdout>>;

/// A completed diff result from the background thread.
struct DiffResult {
    /// Generation counter to discard stale results.
    generation: u64,
    /// The diff key this result corresponds to.
    diff_key: String,
    /// The computed diff data: (filename, old_content, new_content) or None for empty.
    payload: DiffPayload,
}

enum DiffPayload {
    /// Side-by-side diff from old/new content.
    Content { filename: String, old: String, new: String },
    /// Unified diff output from git.
    UnifiedDiff { filename: String, diff_output: String },
    /// No diff to show.
    Empty,
}

pub struct Gui {
    pub config: Arc<AppConfig>,
    pub git: Arc<GitCommands>,
    pub model: Arc<Mutex<Model>>,
    pub context_mgr: ContextManager,
    pub layout: LayoutState,
    pub popup: PopupState,
    pub diff_view: DiffViewState,
    pub command_log: crate::os::cmd::CommandLog,
    pub show_command_log: bool,
    pub should_quit: bool,
    pub needs_refresh: bool,
    pub needs_diff_refresh: bool,
    pub search_query: String,
    /// Whether search input mode is active (typing into search bar).
    pub search_active: bool,
    /// Indices of items matching the current search in the active panel.
    pub search_matches: Vec<usize>,
    /// Current position within search_matches.
    pub search_match_idx: usize,
    pub screen_mode: ScreenMode,
    pub show_file_tree: bool,
    /// Cached file tree nodes — rebuilt on refresh when tree view is active.
    pub file_tree_nodes: Vec<FileTreeNode>,
    /// Set of collapsed directory paths in the file tree.
    pub collapsed_dirs: HashSet<String>,
    /// Whether the diff/main panel is focused (entered via Enter on a file).
    pub diff_focused: bool,
    /// Track what we last loaded a diff for, to avoid reloading on every frame.
    last_diff_key: String,
    /// Generation counter — incremented on each diff request, used to discard stale results.
    diff_generation: Arc<AtomicU64>,
    /// Sender for background diff loading.
    diff_rx: mpsc::Receiver<DiffResult>,
    /// Keep sender around so we can clone it for background threads.
    diff_tx: mpsc::Sender<DiffResult>,
    /// Receiver for AI commit message generation results.
    ai_commit_rx: mpsc::Receiver<Result<String>>,
    /// Sender cloned into background threads for AI commit generation.
    ai_commit_tx: mpsc::Sender<Result<String>>,
    /// Undo stack: stores reflog hashes for undo/redo.
    undo_reflog_idx: usize,
    /// Patch building mode state.
    pub patch_building: PatchBuildingState,
    /// Stashed commit editor popup while commit menu or AI generation is shown.
    pending_commit_popup: Option<PopupState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenMode {
    Normal,
    Half,
    Full,
}

impl Gui {
    pub fn new(config: AppConfig, git: GitCommands) -> Result<Self> {
        let model = git.load_model()?;
        let (diff_tx, diff_rx) = mpsc::channel();
        let (ai_commit_tx, ai_commit_rx) = mpsc::channel();
        let show_file_tree = config.user_config.gui.show_file_tree;
        let show_command_log_default = config.user_config.gui.show_command_log;
        let command_log = crate::os::cmd::new_command_log();
        crate::os::cmd::set_thread_command_log(command_log.clone());

        Ok(Self {
            config: Arc::new(config),
            git: Arc::new(git),
            model: Arc::new(Mutex::new(model)),
            context_mgr: ContextManager::new(),
            layout: LayoutState::default(),
            popup: PopupState::None,
            diff_view: DiffViewState::new(),
            command_log,
            show_command_log: show_command_log_default,
            should_quit: false,
            needs_refresh: false,
            needs_diff_refresh: true,
            search_query: String::new(),
            search_active: false,
            search_matches: Vec::new(),
            search_match_idx: 0,
            screen_mode: ScreenMode::Normal,
            show_file_tree,
            file_tree_nodes: Vec::new(),
            collapsed_dirs: HashSet::new(),
            diff_focused: false,
            last_diff_key: String::new(),
            diff_generation: Arc::new(AtomicU64::new(0)),
            diff_rx,
            diff_tx,
            ai_commit_rx,
            ai_commit_tx,
            undo_reflog_idx: 0,
            patch_building: PatchBuildingState::new(),
            pending_commit_popup: None,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        // Build initial file tree if tree view is enabled
        if self.show_file_tree {
            let model = self.model.lock().unwrap();
            self.file_tree_nodes = build_file_tree(&model.files, &self.collapsed_dirs);
            self.context_mgr.files_list_len_override = Some(self.file_tree_nodes.len());
        }
        let mut terminal = setup_terminal()?;

        let result = self.main_loop(&mut terminal);

        restore_terminal(&mut terminal)?;
        result
    }

    fn main_loop(&mut self, terminal: &mut Term) -> Result<()> {
        loop {
            // Request diff loading on background thread if selection changed
            self.maybe_request_diff();

            // Check for completed background diff results
            self.receive_diff_results();

            // Check for AI commit message generation results
            self.receive_ai_commit_results();

            // Render
            terminal.draw(|frame| {
                let model = self.model.lock().unwrap();
                let search_state = if self.search_active || !self.search_query.is_empty() {
                    Some((
                        self.search_query.as_str(),
                        self.search_matches.len(),
                        self.search_match_idx,
                    ))
                } else {
                    None
                };
                let cmd_log = self.command_log.lock().unwrap();
                views::render(
                    frame,
                    &model,
                    &self.context_mgr,
                    &self.layout,
                    &self.popup,
                    &self.config,
                    &self.diff_view,
                    self.screen_mode,
                    self.show_file_tree,
                    &self.file_tree_nodes,
                    &self.collapsed_dirs,
                    self.diff_focused,
                    search_state,
                    &cmd_log,
                    self.show_command_log,
                );
            })?;

            // Handle events
            if event::poll(std::time::Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key)?,
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    Event::Resize(w, h) => {
                        self.layout.update_size(w, h);
                    }
                    _ => {}
                }
            }

            // Refresh data if needed
            if self.needs_refresh {
                self.refresh()?;
                self.needs_refresh = false;
                self.needs_diff_refresh = true;
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// Receive completed diff results from the background thread (non-blocking).
    fn receive_diff_results(&mut self) {
        // Drain all available results, keeping only the latest valid one
        let current_gen = self.diff_generation.load(Ordering::Relaxed);
        while let Ok(result) = self.diff_rx.try_recv() {
            // Discard stale results from older generations
            if result.generation != current_gen {
                continue;
            }
            match result.payload {
                DiffPayload::Content { filename, old, new } => {
                    self.diff_view.load(&filename, &old, &new);
                }
                DiffPayload::UnifiedDiff { filename, diff_output } => {
                    self.diff_view.load_from_diff_output(&filename, &diff_output);
                }
                DiffPayload::Empty => {
                    self.diff_view = DiffViewState::new();
                }
            }
        }
    }

    /// Check for completed AI commit message generation results.
    fn receive_ai_commit_results(&mut self) {
        if let Ok(result) = self.ai_commit_rx.try_recv() {
            match result {
                Ok(message) => {
                    let popup_width = (self.layout.width * 60 / 100).min(60).max(30);
                    let popup_inner = popup_width.saturating_sub(4) as usize;
                    let config_width = self.config.user_config.git.commit.auto_wrap_width;
                    let wrap = if config_width > 0 { popup_inner.min(config_width) } else { popup_inner };

                    // Restore the stashed commit editor, replacing its textarea content
                    if let Some(mut stashed) = self.pending_commit_popup.take() {
                        if let PopupState::Input { ref mut textarea, ref mut title, .. } = stashed {
                            textarea.select_all();
                            textarea.cut();
                            textarea.insert_str(&message);
                            if wrap > 0 {
                                auto_wrap_textarea(textarea, wrap);
                            }
                            *title = "Commit message".to_string();
                        }
                        self.popup = stashed;
                    } else {
                        let mut ta = popup::make_textarea("Enter commit message...");
                        ta.insert_str(&message);
                        if wrap > 0 {
                            auto_wrap_textarea(&mut ta, wrap);
                        }
                        self.popup = PopupState::Input {
                            title: "Commit message".to_string(),
                            textarea: ta,
                            on_confirm: Box::new(|gui, msg| {
                                if !msg.is_empty() {
                                    gui.git.create_commit(msg, false)?;
                                    gui.needs_refresh = true;
                                }
                                Ok(())
                            }),
                            is_commit: true,
                        };
                    }
                }
                Err(e) => {
                    // On failure, restore the stashed editor so user can type manually
                    if let Some(stashed) = self.pending_commit_popup.take() {
                        self.popup = stashed;
                    } else {
                        self.popup = PopupState::Confirm {
                            title: "AI generation failed".to_string(),
                            message: format!("{}", e),
                            on_confirm: Box::new(|_| Ok(())),
                        };
                    }
                }
            }
        }
    }

    /// Start AI commit message generation on a background thread.
    pub fn start_ai_commit_generation(&self) {
        let git = Arc::clone(&self.git);
        let tx = self.ai_commit_tx.clone();
        let cmd = self.config.user_config.git.commit.generate_command.clone();

        std::thread::spawn(move || {
            let result = crate::git::ai_commit::generate_commit_message(git.repo_path(), &cmd);
            let _ = tx.send(result);
        });
    }

    /// Request diff loading on a background thread if selection changed.
    fn maybe_request_diff(&mut self) {
        let active = self.context_mgr.active();
        let selected = self.context_mgr.selected_active();
        let diff_key = format!("{:?}:{}", active, selected);

        if diff_key == self.last_diff_key && !self.needs_diff_refresh {
            return;
        }
        self.last_diff_key = diff_key.clone();
        self.needs_diff_refresh = false;

        // Bump generation to invalidate any in-flight results
        let generation = self.diff_generation.fetch_add(1, Ordering::Relaxed) + 1;

        let model = self.model.lock().unwrap();
        match active {
            ContextId::Files => {
                // Files panel: load synchronously (usually fast, small diffs)
                let file_idx = if self.show_file_tree {
                    self.file_tree_nodes.get(selected).and_then(|n| n.file_index)
                } else {
                    Some(selected)
                };
                if let Some(file) = file_idx.and_then(|i| model.files.get(i)) {
                    let name = file.name.clone();
                    let has_staged = file.has_staged_changes;
                    let has_unstaged = file.has_unstaged_changes;
                    let tracked = file.tracked;
                    drop(model);

                    let diff_result = if has_unstaged {
                        self.git.diff_file(&name)
                    } else if has_staged {
                        self.git.diff_file_staged(&name)
                    } else {
                        Ok(String::new())
                    };

                    if let Ok(diff) = diff_result {
                        if diff.is_empty() && !tracked {
                            if let Ok(content) = self.git.file_content(&name) {
                                if !content.is_empty() {
                                    self.diff_view.load(&name, "", &content);
                                } else {
                                    self.diff_view = DiffViewState::new();
                                }
                            }
                        } else if diff.is_empty() {
                            self.diff_view = DiffViewState::new();
                        } else {
                            self.diff_view.load_from_diff_output(&name, &diff);
                        }
                    }
                } else {
                    // Directory node or no file selected — clear diff
                    drop(model);
                    self.diff_view = DiffViewState::new();
                }
            }
            ContextId::Commits => {
                // Commits: load async on background thread (can be slow for large diffs)
                if let Some(commit) = model.commits.get(selected) {
                    let hash = commit.hash.clone();
                    drop(model);

                    let git = Arc::clone(&self.git);
                    let tx = self.diff_tx.clone();
                    let gen_counter = Arc::clone(&self.diff_generation);

                    std::thread::spawn(move || {
                        // Check if still relevant before doing work
                        if gen_counter.load(Ordering::Relaxed) != generation {
                            return;
                        }
                        let payload = if let Ok(diff) = git.diff_commit(&hash) {
                            let filename = format!("commit:{}", &hash[..7.min(hash.len())]);
                            DiffPayload::UnifiedDiff { filename, diff_output: diff }
                        } else {
                            DiffPayload::Empty
                        };
                        let _ = tx.send(DiffResult {
                            generation,
                            diff_key,
                            payload,
                        });
                    });
                }
            }
            ContextId::Stash => {
                // Stash: also load async
                if let Some(entry) = model.stash_entries.get(selected) {
                    let index = entry.index;
                    drop(model);

                    let git = Arc::clone(&self.git);
                    let tx = self.diff_tx.clone();
                    let gen_counter = Arc::clone(&self.diff_generation);

                    std::thread::spawn(move || {
                        if gen_counter.load(Ordering::Relaxed) != generation {
                            return;
                        }
                        let payload = if let Ok(diff) = git.stash_diff(index) {
                            if diff.is_empty() {
                                DiffPayload::Empty
                            } else {
                                let filename = format!("stash@{{{}}}", index);
                                DiffPayload::UnifiedDiff { filename, diff_output: diff }
                            }
                        } else {
                            DiffPayload::Empty
                        };
                        let _ = tx.send(DiffResult {
                            generation,
                            diff_key,
                            payload,
                        });
                    });
                } else {
                    drop(model);
                }
            }
            _ => {
                drop(model);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // Popup takes priority
        if self.popup != PopupState::None {
            return self.handle_popup_key(key);
        }

        // Search input mode takes priority
        if self.search_active {
            return self.handle_search_key(key);
        }

        let keybindings = &self.config.user_config.keybinding;

        // When diff panel is focused, handle diff-specific keys
        if self.diff_focused {
            return self.handle_diff_focused_key(key);
        }

        // Global keybindings
        if matches_key(key, &keybindings.universal.quit)
            || matches_key(key, &keybindings.universal.quit_alt1)
        {
            self.should_quit = true;
            return Ok(());
        }

        // Number keys 1-5 to jump to window (press again to cycle tabs)
        if let KeyCode::Char(c @ '1'..='5') = key.code {
            let n = c.to_digit(10).unwrap();
            if let Some(window) = SideWindow::from_number(n) {
                self.context_mgr.jump_to_window(window);
                return Ok(());
            }
        }

        // Tab to switch windows
        if matches_key(key, &keybindings.universal.toggle_panel) {
            self.context_mgr.next_window();
            return Ok(());
        }

        // Arrow keys / h/l to switch windows
        if matches_key(key, &keybindings.universal.prev_block)
            || matches_key(key, &keybindings.universal.prev_block_alt)
        {
            self.context_mgr.prev_window();
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.next_block)
            || matches_key(key, &keybindings.universal.next_block_alt)
        {
            self.context_mgr.next_window();
            return Ok(());
        }

        // Navigation within current panel
        if matches_key(key, &keybindings.universal.prev_item)
            || matches_key(key, &keybindings.universal.prev_item_alt)
        {
            let model = self.model.lock().unwrap();
            self.context_mgr.move_selection(-1, &model);
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.next_item)
            || matches_key(key, &keybindings.universal.next_item_alt)
        {
            let model = self.model.lock().unwrap();
            self.context_mgr.move_selection(1, &model);
            return Ok(());
        }

        // Goto top/bottom
        if matches_key(key, &keybindings.universal.goto_top) {
            self.context_mgr.set_selection(0);
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.goto_bottom) {
            let model = self.model.lock().unwrap();
            let len = self.context_mgr.list_len(&model);
            if len > 0 {
                self.context_mgr.set_selection(len - 1);
            }
            return Ok(());
        }

        // Main panel scroll (J/K or shift+arrows for diff scrolling)
        if matches_key(key, &keybindings.universal.scroll_down_main_alt1) {
            self.diff_view.scroll_down(1);
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.scroll_up_main_alt1) {
            self.diff_view.scroll_up(1);
            return Ok(());
        }
        if key.code == KeyCode::PageDown {
            self.diff_view.scroll_down(20);
            return Ok(());
        }
        if key.code == KeyCode::PageUp {
            self.diff_view.scroll_up(20);
            return Ok(());
        }

        // Next/prev hunk with { and }
        if key.code == KeyCode::Char('{') {
            self.diff_view.prev_hunk();
            return Ok(());
        }
        if key.code == KeyCode::Char('}') {
            self.diff_view.next_hunk();
            return Ok(());
        }

        // Refresh
        if matches_key(key, &keybindings.universal.refresh) {
            self.needs_refresh = true;
            return Ok(());
        }

        // Rebase options menu (global — when rebasing/merging)
        if matches_key(key, &keybindings.universal.create_rebase_options_menu) {
            let model = self.model.lock().unwrap();
            let is_rebasing = model.is_rebasing;
            let is_merging = model.is_merging;
            let is_cherry_picking = model.is_cherry_picking;
            drop(model);

            if is_rebasing || is_merging || is_cherry_picking {
                return self.show_rebase_options_menu(is_rebasing, is_merging, is_cherry_picking);
            }
        }

        // Push (global)
        if matches_key(key, &keybindings.universal.push_files) {
            controller::remotes::handle_key(self, key, &self.config.user_config.keybinding.clone())?;
            return Ok(());
        }

        // Pull (global)
        if matches_key(key, &keybindings.universal.pull_files) {
            controller::remotes::handle_key(self, key, &self.config.user_config.keybinding.clone())?;
            return Ok(());
        }

        // Screen mode toggle (+ to enlarge, _ to shrink, matching lazygit)
        if matches_key(key, &keybindings.universal.next_screen_mode) {
            self.next_screen_mode();
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.prev_screen_mode) {
            self.prev_screen_mode();
            return Ok(());
        }

        // Undo (z)
        if matches_key(key, &keybindings.universal.undo) {
            return self.undo();
        }

        // Redo (ctrl-z)
        if matches_key(key, &keybindings.universal.redo) {
            return self.redo();
        }

        // Patch building mode (<c-p>)
        if matches_key(key, &keybindings.universal.create_patch_options_menu) {
            if self.context_mgr.active() == ContextId::Commits || self.patch_building.active {
                return controller::patch_building::show_patch_menu(self);
            }
        }

        // Start search
        if matches_key(key, &keybindings.universal.start_search) {
            self.search_active = true;
            self.search_query.clear();
            self.search_matches.clear();
            self.search_match_idx = 0;
            return Ok(());
        }

        // Next/prev search match
        if !self.search_query.is_empty() {
            if matches_key(key, &keybindings.universal.next_match) {
                self.goto_next_search_match();
                return Ok(());
            }
            if matches_key(key, &keybindings.universal.prev_match) {
                self.goto_prev_search_match();
                return Ok(());
            }
        }

        // Context-specific keybindings
        self.handle_context_key(key)?;

        // Custom commands (lowest priority — checked after built-in bindings)
        controller::custom_commands::try_handle_key(self, key)?;

        Ok(())
    }

    fn handle_context_key(&mut self, key: KeyEvent) -> Result<()> {
        let keybindings = self.config.user_config.keybinding.clone();
        let active = self.context_mgr.active();

        match active {
            ContextId::Files => {
                controller::files::handle_key(self, key, &keybindings)?;
            }
            ContextId::Branches => {
                controller::branches::handle_key(self, key, &keybindings)?;
            }
            ContextId::Commits => {
                controller::commits::handle_key(self, key, &keybindings)?;
            }
            ContextId::Stash => {
                controller::stash::handle_key(self, key, &keybindings)?;
            }
            ContextId::Remotes => {
                controller::remotes::handle_key(self, key, &keybindings)?;
            }
            ContextId::Tags => {
                controller::tags::handle_key(self, key, &keybindings)?;
            }
            ContextId::Status => {
                // Enter on status shows recent repos
                if key.code == KeyCode::Enter {
                    self.show_recent_repos()?;
                }
            }
            ContextId::Worktrees => {
                controller::worktrees::handle_key(self, key, &keybindings)?;
            }
            ContextId::Submodules => {
                controller::submodules::handle_key(self, key, &keybindings)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_diff_focused_key(&mut self, key: KeyEvent) -> Result<()> {
        let keybindings = &self.config.user_config.keybinding;

        // Screen mode cycling works even when diff is focused
        if matches_key(key, &keybindings.universal.next_screen_mode) {
            self.next_screen_mode();
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.prev_screen_mode) {
            self.prev_screen_mode();
            return Ok(());
        }

        match key.code {
            // Escape to unfocus diff, return to sidebar
            KeyCode::Esc => {
                self.diff_focused = false;
            }
            // q quits the app (same as global behavior)
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            // j/k/up/down scroll line by line
            KeyCode::Char('j') | KeyCode::Down => {
                self.diff_view.scroll_down(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.diff_view.scroll_up(1);
            }
            // h/l/left/right scroll horizontally
            KeyCode::Char('h') | KeyCode::Left => {
                self.diff_view.scroll_left(4);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.diff_view.scroll_right(4);
            }
            // ] and [ jump between hunks
            KeyCode::Char(']') => {
                self.diff_view.next_hunk();
            }
            KeyCode::Char('[') => {
                self.diff_view.prev_hunk();
            }
            // Page up/down for larger scrolling
            KeyCode::PageDown => {
                self.diff_view.scroll_down(20);
            }
            KeyCode::PageUp => {
                self.diff_view.scroll_up(20);
            }
            // g/G for top/bottom
            KeyCode::Char('g') => {
                self.diff_view.scroll_offset = 0;
            }
            KeyCode::Char('G') => {
                let max = self.diff_view.lines.len().saturating_sub(1);
                self.diff_view.scroll_offset = max;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_popup_key(&mut self, key: KeyEvent) -> Result<()> {
        match &self.popup {
            PopupState::Confirm { .. } => {
                if key.code == KeyCode::Char('y') || key.code == KeyCode::Enter {
                    let popup = std::mem::replace(&mut self.popup, PopupState::None);
                    if let PopupState::Confirm { on_confirm, .. } = popup {
                        on_confirm(self)?;
                    }
                } else {
                    self.popup = PopupState::None;
                }
            }
            PopupState::Menu { items, selected, .. } => {
                let items_len = items.len();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if let PopupState::Menu { selected, .. } = &mut self.popup {
                            *selected = (*selected + 1).min(items_len - 1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if let PopupState::Menu { selected, .. } = &mut self.popup {
                            *selected = selected.saturating_sub(1);
                        }
                    }
                    KeyCode::Enter => {
                        let popup = std::mem::replace(&mut self.popup, PopupState::None);
                        if let PopupState::Menu { items, selected, .. } = popup {
                            if let Some(item) = items.get(selected) {
                                if let Some(ref action) = item.action {
                                    action(self)?;
                                }
                            }
                        }
                    }
                    KeyCode::Esc => {
                        if let Some(stashed) = self.pending_commit_popup.take() {
                            self.popup = stashed;
                        } else {
                            self.popup = PopupState::None;
                        }
                    }
                    KeyCode::Char(c) => {
                        // Check if the typed char matches a menu item shortcut key
                        let key_str = c.to_string();
                        let matched_idx = items.iter().position(|item| {
                            item.key.as_deref() == Some(key_str.as_str())
                        });
                        if let Some(idx) = matched_idx {
                            // Check if the item has an action (not disabled)
                            let has_action = items[idx].action.is_some();
                            if has_action {
                                let popup = std::mem::replace(&mut self.popup, PopupState::None);
                                if let PopupState::Menu { items, .. } = popup {
                                    if let Some(ref action) = items[idx].action {
                                        action(self)?;
                                    }
                                }
                            }
                            // If disabled, do nothing (stay on menu)
                        }
                        // If no match, ignore the key (stay on menu)
                    }
                    _ => {}
                }
            }
            PopupState::Input { is_commit, .. } => {
                use crossterm::event::KeyModifiers;
                let is_commit = *is_commit;

                // Ctrl+Enter or Meta+Enter to confirm
                if key.code == KeyCode::Enter
                    && (key.modifiers.contains(KeyModifiers::CONTROL)
                        || key.modifiers.contains(KeyModifiers::SUPER))
                {
                    let popup = std::mem::replace(&mut self.popup, PopupState::None);
                    if let PopupState::Input { textarea, on_confirm, .. } = popup {
                        let text = textarea.lines().join("\n");
                        on_confirm(self, &text)?;
                    }
                } else if key.code == KeyCode::Esc {
                    self.popup = PopupState::None;
                } else if is_commit
                    && key.code == KeyCode::Char('o')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    // <c-o> in commit message editor: open commit menu
                    self.show_commit_editor_menu()?;
                } else {
                    // Forward all other keys to the textarea
                    if let PopupState::Input { textarea, .. } = &mut self.popup {
                        textarea.input(key);
                        // Auto-wrap: hard-wrap lines so they never exceed the visible width.
                        // Use the smaller of config wrap width and actual popup inner width.
                        let popup_width = (self.layout.width * 60 / 100).min(60).max(30);
                        let popup_inner = popup_width.saturating_sub(4) as usize; // borders + margin
                        let config_width = self.config.user_config.git.commit.auto_wrap_width;
                        let effective_width = if config_width > 0 {
                            popup_inner.min(config_width)
                        } else {
                            popup_inner
                        };
                        if effective_width > 0 {
                            auto_wrap_textarea(textarea, effective_width);
                        }
                    }
                }
            }
            PopupState::Loading { .. } => {
                // Block all input while loading — user must wait
            }
            PopupState::None => {}
        }
        Ok(())
    }

    fn show_rebase_options_menu(
        &mut self,
        is_rebasing: bool,
        is_merging: bool,
        _is_cherry_picking: bool,
    ) -> Result<()> {
        let mut items = Vec::new();

        if is_rebasing {
            items.push(popup::MenuItem {
                label: "Continue rebase".to_string(),
                description: "git rebase --continue".to_string(),
                key: Some("c".to_string()),
                action: Some(Box::new(|gui| {
                    gui.git.continue_rebase()?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            });
            items.push(popup::MenuItem {
                label: "Abort rebase".to_string(),
                description: "git rebase --abort".to_string(),
                key: Some("a".to_string()),
                action: Some(Box::new(|gui| {
                    gui.git.abort_rebase()?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            });
            items.push(popup::MenuItem {
                label: "Skip this commit".to_string(),
                description: "git rebase --skip".to_string(),
                key: Some("s".to_string()),
                action: Some(Box::new(|gui| {
                    gui.git.rebase_skip()?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            });
        }

        if is_merging {
            items.push(popup::MenuItem {
                label: "Abort merge".to_string(),
                description: "git merge --abort".to_string(),
                key: Some("a".to_string()),
                action: Some(Box::new(|gui| {
                    gui.git.abort_merge()?;
                    gui.needs_refresh = true;
                    Ok(())
                })),
            });
        }

        self.popup = PopupState::Menu {
            title: "Rebase/Merge options".to_string(),
            items,
            selected: 0,
        };
        Ok(())
    }

    /// Show the commit menu from within the commit message editor (<c-o>).
    fn show_commit_editor_menu(&mut self) -> Result<()> {
        // Stash the current commit editor popup
        let stashed = std::mem::replace(&mut self.popup, PopupState::None);
        self.pending_commit_popup = Some(stashed);

        let generate_cmd = self.config.user_config.git.commit.generate_command.clone();
        let has_generate = !generate_cmd.is_empty();

        let ai_label = if has_generate {
            format!("Generate w/ AI ({})", generate_cmd)
        } else {
            "Generate w/ AI (not configured)".to_string()
        };

        let mut items = vec![
            popup::MenuItem {
                label: "Open in editor".to_string(),
                description: String::new(),
                key: Some("e".to_string()),
                action: Some(Box::new(|gui| {
                    // Restore the stashed editor — user can continue typing
                    // TODO: full $EDITOR integration would suspend the TUI
                    if let Some(stashed) = gui.pending_commit_popup.take() {
                        gui.popup = stashed;
                    }
                    Ok(())
                })),
            },
            popup::MenuItem {
                label: "Add co-author".to_string(),
                description: String::new(),
                key: Some("c".to_string()),
                action: Some(Box::new(|gui| {
                    // Restore editor, then open a prompt for co-author
                    let stashed = gui.pending_commit_popup.take();
                    gui.popup = PopupState::Input {
                        title: "Co-author (Name <email>)".to_string(),
                        textarea: popup::make_textarea("Name <email@example.com>"),
                        on_confirm: Box::new(move |gui, coauthor| {
                            if let Some(mut editor) = stashed {
                                if !coauthor.is_empty() {
                                    // Append co-author trailer to the commit message
                                    if let PopupState::Input { ref mut textarea, .. } = editor {
                                        textarea.insert_str(&format!("\n\nCo-authored-by: {}", coauthor));
                                    }
                                }
                                gui.popup = editor;
                            }
                            Ok(())
                        }),
                        is_commit: false,
                    };
                    Ok(())
                })),
            },
            popup::MenuItem {
                label: "Paste commit message from clipboard".to_string(),
                description: String::new(),
                key: Some("p".to_string()),
                action: Some(Box::new(|gui| {
                    let clipboard_text = read_clipboard();
                    if let Some(mut editor) = gui.pending_commit_popup.take() {
                        if let Some(text) = clipboard_text {
                            if !text.is_empty() {
                                if let PopupState::Input { ref mut textarea, .. } = editor {
                                    textarea.select_all();
                                    textarea.cut();
                                    textarea.insert_str(&text);
                                    // Auto-wrap the pasted text
                                    let popup_width = (gui.layout.width * 60 / 100).min(60).max(30);
                                    let popup_inner = popup_width.saturating_sub(4) as usize;
                                    let config_width = gui.config.user_config.git.commit.auto_wrap_width;
                                    let wrap = if config_width > 0 { popup_inner.min(config_width) } else { popup_inner };
                                    if wrap > 0 {
                                        auto_wrap_textarea(textarea, wrap);
                                    }
                                }
                            }
                        }
                        gui.popup = editor;
                    }
                    Ok(())
                })),
            },
        ];

        if has_generate {
            items.push(popup::MenuItem {
                label: ai_label,
                description: String::new(),
                key: Some("g".to_string()),
                action: Some(Box::new(|gui| {
                    gui.popup = PopupState::Loading {
                        title: "AI Commit".to_string(),
                        message: "Generating commit message...".to_string(),
                    };
                    gui.start_ai_commit_generation();
                    Ok(())
                })),
            });
        } else {
            items.push(popup::MenuItem {
                label: ai_label,
                description: String::new(),
                key: Some("g".to_string()),
                action: None, // Disabled — no generateCommand configured
            });
        }

        self.popup = PopupState::Menu {
            title: "Commit menu".to_string(),
            items,
            selected: 0,
        };
        Ok(())
    }

    fn show_recent_repos(&mut self) -> Result<()> {
        let recent = self.config.app_state.recent_repos.clone();
        if recent.is_empty() {
            return Ok(());
        }

        let items: Vec<popup::MenuItem> = recent
            .into_iter()
            .map(|path| {
                let display = path.clone();
                let p = path.clone();
                popup::MenuItem {
                    label: display,
                    description: String::new(),
                    key: None,
                    action: Some(Box::new(move |gui| {
                        // Switch to the selected repo
                        let new_git = crate::git::GitCommands::new(std::path::Path::new(&p))?;
                        let new_model = new_git.load_model()?;
                        gui.git = std::sync::Arc::new(new_git);
                        *gui.model.lock().unwrap() = new_model;
                        gui.needs_refresh = false;
                        gui.needs_diff_refresh = true;
                        gui.context_mgr = context::ContextManager::new();
                        gui.diff_view = DiffViewState::new();
                        if gui.show_file_tree {
                            gui.update_file_tree_state();
                        }
                        Ok(())
                    })),
                }
            })
            .collect();

        self.popup = PopupState::Menu {
            title: "Recent repos".to_string(),
            items,
            selected: 0,
        };
        Ok(())
    }

    fn undo(&mut self) -> Result<()> {
        // Get reflog entries
        let result = self.git.git_cmd()
            .args(&["reflog", "--format=%H", "-n", "20"])
            .run()?;
        if !result.success {
            return Ok(());
        }
        let entries: Vec<&str> = result.stdout.lines().collect();
        let next_idx = self.undo_reflog_idx + 1;
        if next_idx >= entries.len() {
            return Ok(()); // Nothing more to undo
        }

        let target_hash = entries[next_idx].to_string();
        let short = &target_hash[..7.min(target_hash.len())];

        self.popup = PopupState::Confirm {
            title: "Undo".to_string(),
            message: format!("Undo to reflog entry {}? ({})", next_idx, short),
            on_confirm: Box::new(move |gui| {
                gui.git.reset_to_commit(&target_hash, "--mixed")?;
                gui.undo_reflog_idx = next_idx;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
        Ok(())
    }

    fn redo(&mut self) -> Result<()> {
        if self.undo_reflog_idx == 0 {
            return Ok(()); // Nothing to redo
        }

        let result = self.git.git_cmd()
            .args(&["reflog", "--format=%H", "-n", "20"])
            .run()?;
        if !result.success {
            return Ok(());
        }
        let entries: Vec<&str> = result.stdout.lines().collect();
        let prev_idx = self.undo_reflog_idx - 1;
        if prev_idx >= entries.len() {
            return Ok(());
        }

        let target_hash = entries[prev_idx].to_string();
        let short = &target_hash[..7.min(target_hash.len())];

        self.popup = PopupState::Confirm {
            title: "Redo".to_string(),
            message: format!("Redo to reflog entry {}? ({})", prev_idx, short),
            on_confirm: Box::new(move |gui| {
                gui.git.reset_to_commit(&target_hash, "--mixed")?;
                gui.undo_reflog_idx = prev_idx;
                gui.needs_refresh = true;
                Ok(())
            }),
        };
        Ok(())
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.search_active = false;
            }
            KeyCode::Enter => {
                self.search_active = false;
                // Jump to first match
                if !self.search_matches.is_empty() {
                    self.search_match_idx = 0;
                    let idx = self.search_matches[0];
                    self.context_mgr.set_selection(idx);
                }
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.update_search_matches();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.update_search_matches();
            }
            _ => {}
        }
        Ok(())
    }

    fn update_search_matches(&mut self) {
        self.search_matches.clear();
        if self.search_query.is_empty() {
            return;
        }

        let query = self.search_query.to_lowercase();
        let model = self.model.lock().unwrap();
        let active = self.context_mgr.active();

        match active {
            ContextId::Files => {
                for (i, file) in model.files.iter().enumerate() {
                    if file.name.to_lowercase().contains(&query) {
                        self.search_matches.push(i);
                    }
                }
            }
            ContextId::Branches => {
                for (i, branch) in model.branches.iter().enumerate() {
                    if branch.name.to_lowercase().contains(&query) {
                        self.search_matches.push(i);
                    }
                }
            }
            ContextId::Commits => {
                for (i, commit) in model.commits.iter().enumerate() {
                    if commit.name.to_lowercase().contains(&query)
                        || commit.hash.starts_with(&self.search_query)
                        || commit.author_name.to_lowercase().contains(&query)
                    {
                        self.search_matches.push(i);
                    }
                }
            }
            ContextId::Stash => {
                for (i, entry) in model.stash_entries.iter().enumerate() {
                    if entry.name.to_lowercase().contains(&query) {
                        self.search_matches.push(i);
                    }
                }
            }
            ContextId::Tags => {
                for (i, tag) in model.tags.iter().enumerate() {
                    if tag.name.to_lowercase().contains(&query) {
                        self.search_matches.push(i);
                    }
                }
            }
            ContextId::Remotes => {
                for (i, remote) in model.remotes.iter().enumerate() {
                    if remote.name.to_lowercase().contains(&query) {
                        self.search_matches.push(i);
                    }
                }
            }
            _ => {}
        }

        // Auto-jump to first match
        if !self.search_matches.is_empty() {
            self.search_match_idx = 0;
            let idx = self.search_matches[0];
            self.context_mgr.set_selection(idx);
        }
    }

    fn goto_next_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_idx = (self.search_match_idx + 1) % self.search_matches.len();
        let idx = self.search_matches[self.search_match_idx];
        self.context_mgr.set_selection(idx);
    }

    fn goto_prev_search_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.search_match_idx = if self.search_match_idx == 0 {
            self.search_matches.len() - 1
        } else {
            self.search_match_idx - 1
        };
        let idx = self.search_matches[self.search_match_idx];
        self.context_mgr.set_selection(idx);
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};

        if !self.config.user_config.gui.mouse_events {
            return;
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_mouse_click(mouse.column, mouse.row);
            }
            MouseEventKind::ScrollUp => {
                if self.diff_focused || self.is_in_main_panel(mouse.column) {
                    self.diff_view.scroll_up(3);
                } else {
                    let model = self.model.lock().unwrap();
                    self.context_mgr.move_selection(-3, &model);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.diff_focused || self.is_in_main_panel(mouse.column) {
                    self.diff_view.scroll_down(3);
                } else {
                    let model = self.model.lock().unwrap();
                    self.context_mgr.move_selection(3, &model);
                }
            }
            _ => {}
        }
    }

    fn handle_mouse_click(&mut self, col: u16, row: u16) {
        let area = ratatui::layout::Rect::new(0, 0, self.layout.width, self.layout.height);
        let panel_count = SideWindow::ALL.len();
        let active_window = self.context_mgr.active_window();
        let active_panel_index = SideWindow::ALL
            .iter()
            .position(|w| *w == active_window)
            .unwrap_or(1);

        let fl = layout::compute_layout(
            area,
            self.layout.side_panel_ratio,
            panel_count,
            active_panel_index,
            self.screen_mode,
        );

        // Check if click is in the main (diff) panel
        if fl.main_panel.x <= col
            && col < fl.main_panel.x + fl.main_panel.width
            && fl.main_panel.y <= row
            && row < fl.main_panel.y + fl.main_panel.height
        {
            if !self.diff_view.is_empty() {
                self.diff_focused = true;
            }
            return;
        }

        // Check which side panel was clicked
        for (i, &panel_rect) in fl.side_panels.iter().enumerate() {
            if panel_rect.x <= col
                && col < panel_rect.x + panel_rect.width
                && panel_rect.y <= row
                && row < panel_rect.y + panel_rect.height
            {
                self.diff_focused = false;
                if let Some(&window) = SideWindow::ALL.get(i) {
                    let ctx = self.context_mgr.active_context_for_window(window);
                    self.context_mgr.set_active(ctx);

                    // Calculate which item was clicked (row relative to panel inner area)
                    let inner_y = row.saturating_sub(panel_rect.y + 1); // +1 for border
                    let selected = self.context_mgr.selected_active();
                    let model = self.model.lock().unwrap();
                    let list_len = self.context_mgr.list_len(&model);
                    drop(model);

                    // Calculate scroll offset (same logic as render_list)
                    let visible_height = panel_rect.height.saturating_sub(2) as usize;
                    let scroll_offset = if selected >= visible_height {
                        selected - visible_height + 1
                    } else {
                        0
                    };
                    let clicked_idx = scroll_offset + inner_y as usize;
                    if clicked_idx < list_len {
                        self.context_mgr.set_selection(clicked_idx);
                    }
                }
                return;
            }
        }
    }

    fn is_in_main_panel(&self, col: u16) -> bool {
        let side_width = ((self.layout.width as f64)
            * self.config.user_config.gui.side_panel_width) as u16;
        col >= side_width
    }

    fn refresh(&mut self) -> Result<()> {
        let new_model = self.git.load_model()?;
        let mut model = self.model.lock().unwrap();
        *model = new_model;
        // Rebuild file tree inline to avoid borrow issues
        if self.show_file_tree {
            self.file_tree_nodes = build_file_tree(&model.files, &self.collapsed_dirs);
            self.context_mgr.files_list_len_override = Some(self.file_tree_nodes.len());
        } else {
            self.file_tree_nodes.clear();
            self.context_mgr.files_list_len_override = None;
        }
        Ok(())
    }

    /// Resolve the currently selected file index in the files panel.
    /// In tree view, maps the tree node selection to the actual file index.
    /// Returns None if a directory node is selected (no file to operate on).
    pub fn selected_file_index(&self) -> Option<usize> {
        let selected = self.context_mgr.selected_active();
        if self.show_file_tree {
            self.file_tree_nodes
                .get(selected)
                .and_then(|node| node.file_index)
        } else {
            Some(selected)
        }
    }

    pub fn update_file_tree_state(&mut self) {
        if self.show_file_tree {
            let model = self.model.lock().unwrap();
            self.file_tree_nodes = build_file_tree(&model.files, &self.collapsed_dirs);
            self.context_mgr.files_list_len_override = Some(self.file_tree_nodes.len());
        } else {
            self.file_tree_nodes.clear();
            self.context_mgr.files_list_len_override = None;
        }
    }

    fn next_screen_mode(&mut self) {
        self.screen_mode = match self.screen_mode {
            ScreenMode::Normal => ScreenMode::Half,
            ScreenMode::Half => ScreenMode::Full,
            ScreenMode::Full => ScreenMode::Normal,
        };
    }

    fn prev_screen_mode(&mut self) {
        self.screen_mode = match self.screen_mode {
            ScreenMode::Normal => ScreenMode::Full,
            ScreenMode::Half => ScreenMode::Normal,
            ScreenMode::Full => ScreenMode::Half,
        };
    }
}

/// Auto-wrap all lines in a textarea so no line exceeds `wrap_width`.
/// Rebuilds the entire textarea content with hard line breaks at word boundaries.
fn auto_wrap_textarea(textarea: &mut tui_textarea::TextArea<'static>, wrap_width: usize) {
    if wrap_width == 0 {
        return;
    }

    let needs_wrap = textarea.lines().iter().any(|l| l.len() > wrap_width);
    if !needs_wrap {
        return;
    }

    // Compute cursor's absolute char offset in the original text
    let (cursor_row, cursor_col) = textarea.cursor();
    let original_lines: Vec<String> = textarea.lines().iter().map(|s| s.to_string()).collect();

    let mut cursor_abs = 0usize;
    for (i, line) in original_lines.iter().enumerate() {
        if i < cursor_row {
            cursor_abs += line.len() + 1;
        } else {
            cursor_abs += cursor_col.min(line.len());
            break;
        }
    }

    // Word-wrap all lines
    let mut wrapped: Vec<String> = Vec::new();
    for line in &original_lines {
        if line.len() <= wrap_width {
            wrapped.push(line.clone());
        } else {
            let mut remaining = line.as_str();
            while remaining.len() > wrap_width {
                let break_at = remaining[..wrap_width].rfind(' ').unwrap_or(wrap_width);
                let break_at = if break_at == 0 { wrap_width } else { break_at };
                wrapped.push(remaining[..break_at].to_string());
                remaining = remaining[break_at..].trim_start();
            }
            if !remaining.is_empty() {
                wrapped.push(remaining.to_string());
            }
        }
    }

    let new_text = wrapped.join("\n");

    // Map the absolute cursor offset into the new wrapped text
    // The wrapping only adds newlines (replacing spaces), so character content
    // is preserved. Walk the new text to find the right row/col.
    let mut abs = 0usize;
    let mut new_row = 0;
    let mut new_col = 0;
    for (i, wline) in wrapped.iter().enumerate() {
        if abs + wline.len() >= cursor_abs {
            new_row = i;
            new_col = (cursor_abs - abs).min(wline.len());
            break;
        }
        abs += wline.len() + 1; // +1 for newline
        new_row = i + 1;
        new_col = 0;
    }

    // Replace content and restore cursor
    textarea.select_all();
    textarea.cut();
    textarea.insert_str(&new_text);

    textarea.move_cursor(tui_textarea::CursorMove::Top);
    textarea.move_cursor(tui_textarea::CursorMove::Head);
    for _ in 0..new_row {
        textarea.move_cursor(tui_textarea::CursorMove::Down);
    }
    for _ in 0..new_col {
        textarea.move_cursor(tui_textarea::CursorMove::Forward);
    }
}

/// Read text from the system clipboard.
fn read_clipboard() -> Option<String> {
    let cmd = if cfg!(target_os = "macos") {
        "pbpaste"
    } else if cfg!(target_os = "windows") {
        "powershell.exe -command Get-Clipboard"
    } else {
        "xclip -selection clipboard -o"
    };

    std::process::Command::new("sh")
        .args(["-c", cmd])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn matches_key(key: KeyEvent, binding: &str) -> bool {
    if let Some(expected) = parse_key(binding) {
        // Compare code and modifiers, ignore kind/state
        key.code == expected.code && key.modifiers == expected.modifiers
    } else {
        false
    }
}

fn setup_terminal() -> Result<Term> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
        cursor::Hide
    )?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Term) -> Result<()> {
    terminal::disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
        cursor::Show
    )?;
    Ok(())
}
