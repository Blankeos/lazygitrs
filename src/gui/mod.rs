pub mod context;
pub mod controller;
pub mod layout;
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
    pub command_log: Vec<String>,
    pub should_quit: bool,
    pub needs_refresh: bool,
    pub needs_diff_refresh: bool,
    pub search_query: String,
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
        let show_file_tree = config.user_config.gui.show_file_tree;

        Ok(Self {
            config: Arc::new(config),
            git: Arc::new(git),
            model: Arc::new(Mutex::new(model)),
            context_mgr: ContextManager::new(),
            layout: LayoutState::default(),
            popup: PopupState::None,
            diff_view: DiffViewState::new(),
            command_log: Vec::new(),
            should_quit: false,
            needs_refresh: false,
            needs_diff_refresh: true,
            search_query: String::new(),
            screen_mode: ScreenMode::Normal,
            show_file_tree,
            file_tree_nodes: Vec::new(),
            collapsed_dirs: HashSet::new(),
            diff_focused: false,
            last_diff_key: String::new(),
            diff_generation: Arc::new(AtomicU64::new(0)),
            diff_rx,
            diff_tx,
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

            // Render
            terminal.draw(|frame| {
                let model = self.model.lock().unwrap();
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

        // Context-specific keybindings
        self.handle_context_key(key)?;

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
                let selected = *selected;
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
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.popup = PopupState::None;
                    }
                    _ => {}
                }
            }
            PopupState::Input { buffer, .. } => {
                let mut buf = buffer.clone();
                match key.code {
                    KeyCode::Char(c) => {
                        buf.push(c);
                        if let PopupState::Input { buffer, .. } = &mut self.popup {
                            *buffer = buf;
                        }
                    }
                    KeyCode::Backspace => {
                        buf.pop();
                        if let PopupState::Input { buffer, .. } = &mut self.popup {
                            *buffer = buf;
                        }
                    }
                    KeyCode::Enter => {
                        let popup = std::mem::replace(&mut self.popup, PopupState::None);
                        if let PopupState::Input { buffer, on_confirm, .. } = popup {
                            on_confirm(self, &buffer)?;
                        }
                    }
                    KeyCode::Esc => {
                        self.popup = PopupState::None;
                    }
                    _ => {}
                }
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

    fn handle_mouse(&mut self, _mouse: MouseEvent) {
        // Mouse support will be implemented in Phase 4
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
