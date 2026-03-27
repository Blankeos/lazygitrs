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
use std::time::Instant;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, MouseEvent};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, cursor};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::config::{AppConfig, AppState};
use crate::config::keybindings::parse_key;
use crate::git::{GitCommands, ModelPart, MODEL_PART_COUNT};
use crate::model::Model;
use crate::model::file_tree::{build_file_tree, CommitFileTreeNode, FileTreeNode};
use crate::pager::side_by_side::{DiffPanelLayout, DiffViewState, TextSelection};

use self::context::{ContextId, ContextManager, SideWindow};
use self::layout::LayoutState;
use self::popup::{HelpEntry, HelpSection};
use self::modes::diff_mode::DiffModeState;
use self::modes::patch_building::PatchBuildingState;
use self::modes::rebase_mode::RebaseModeState;
use self::popup::{MessageKind, PopupState};

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
    pub needs_files_refresh: bool,
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
    /// Receiver for background remote operations (push, pull, fetch).
    remote_op_rx: mpsc::Receiver<Result<()>>,
    /// Sender cloned into background threads for remote operations.
    remote_op_tx: mpsc::Sender<Result<()>>,
    /// Undo stack: stores reflog hashes for undo/redo.
    undo_reflog_idx: usize,
    /// Patch building mode state.
    pub patch_building: PatchBuildingState,
    /// Diff/compare mode state.
    pub diff_mode: DiffModeState,
    /// Interactive rebase mode state.
    pub rebase_mode: RebaseModeState,
    /// Stashed commit editor popup while commit menu or AI generation is shown.
    pending_commit_popup: Option<PopupState>,
    /// Search bar textarea (1-line editor for search input).
    search_textarea: Option<tui_textarea::TextArea<'static>>,
    /// Last time a refresh occurred (for 10s background auto-refresh interval).
    last_refresh_at: Instant,
    /// Active branch filter for commits panel. When non-empty, only commits from these branches are shown.
    pub commit_branch_filter: Vec<String>,
    /// Hash of the commit whose files are being viewed in CommitFiles context.
    pub commit_files_hash: String,
    /// First line of the commit message for the commit being viewed.
    pub commit_files_message: String,
    /// Cached commit file tree nodes for the CommitFiles view.
    pub commit_file_tree_nodes: Vec<CommitFileTreeNode>,
    /// Set of collapsed directory paths in the commit file tree.
    pub commit_files_collapsed_dirs: HashSet<String>,
    /// Whether to show tree view for commit files (mirrors show_file_tree).
    pub show_commit_file_tree: bool,
    /// Name of the branch/tag whose commits are being viewed in BranchCommits context.
    pub branch_commits_name: String,
    /// Name of the remote whose branches are being viewed in RemoteBranches context.
    pub remote_branches_name: String,
    /// Parent context to return to when pressing Esc from BranchCommits.
    pub sub_commits_parent_context: context::ContextId,
    /// Parent context to return to when pressing Esc from CommitFiles.
    pub commit_files_parent_context: Option<context::ContextId>,
    /// Receiver for streamed model parts during initial load. Each git data
    /// type arrives independently so the UI can waterfall-display results.
    /// Set to `None` once all parts have been received.
    initial_load_rx: Option<mpsc::Receiver<ModelPart>>,
    /// How many model parts have arrived so far (out of MODEL_PART_COUNT).
    initial_load_received: usize,
    /// Frame counter for the loading spinner animation.
    spinner_frame: usize,
    /// Label shown on the head branch during a remote operation (e.g. "Pushing", "Pulling").
    remote_op_label: Option<String>,
    /// Timestamp when the last remote operation succeeded (for showing a temporary ✓).
    remote_op_success_at: Option<Instant>,
    /// History of previously submitted commit messages (most recent first).
    pub commit_message_history: Vec<String>,
    /// Current index into commit_message_history when cycling (None = not cycling).
    pub commit_history_idx: Option<usize>,
    /// Stashed current draft when cycling through history.
    commit_history_draft: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenMode {
    Normal,
    Half,
    Full,
}

impl Gui {
    pub fn new(config: AppConfig, git: GitCommands) -> Result<Self> {
        let (diff_tx, diff_rx) = mpsc::channel();
        let (ai_commit_tx, ai_commit_rx) = mpsc::channel();
        let (remote_op_tx, remote_op_rx) = mpsc::channel();
        let show_file_tree = config.user_config.gui.show_file_tree;
        let show_command_log_default = config
            .app_state
            .show_command_log
            .unwrap_or(config.user_config.gui.show_command_log);
        let command_log = crate::os::cmd::new_command_log();
        crate::os::cmd::set_thread_command_log(command_log.clone());

        // Start with an empty model — each piece of data loads in the
        // background and streams in as it becomes ready, so the UI can
        // paint immediately and waterfall-display results.
        let git = Arc::new(git);
        let mut model = Model::default();
        model.repo_name = git.repo_name();
        model.head_hash = git.head_hash().unwrap_or_default();
        model.head_branch_name = git.current_branch_name().unwrap_or_default();

        let (initial_load_tx, initial_load_rx) = mpsc::channel();
        git.load_model_streaming(&initial_load_tx);

        let commit_history = Self::load_commit_history(&config);

        Ok(Self {
            config: Arc::new(config),
            git,
            model: Arc::new(Mutex::new(model)),
            initial_load_rx: Some(initial_load_rx),
            initial_load_received: 0,
            context_mgr: ContextManager::new(),
            layout: LayoutState::default(),
            popup: PopupState::None,
            diff_view: DiffViewState::new(),
            command_log,
            show_command_log: show_command_log_default,
            should_quit: false,
            needs_refresh: false,
            needs_files_refresh: false,
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
            remote_op_rx,
            remote_op_tx,
            undo_reflog_idx: 0,
            patch_building: PatchBuildingState::new(),
            diff_mode: DiffModeState::new(),
            rebase_mode: RebaseModeState::new(),
            pending_commit_popup: None,
            search_textarea: None,
            last_refresh_at: Instant::now(),
            commit_branch_filter: Vec::new(),
            commit_files_hash: String::new(),
            commit_files_message: String::new(),
            commit_file_tree_nodes: Vec::new(),
            commit_files_collapsed_dirs: HashSet::new(),
            show_commit_file_tree: show_file_tree,
            branch_commits_name: String::new(),
            remote_branches_name: String::new(),
            sub_commits_parent_context: context::ContextId::Branches,
            commit_files_parent_context: None,
            spinner_frame: 0,
            remote_op_label: None,
            remote_op_success_at: None,
            commit_message_history: commit_history,
            commit_history_idx: None,
            commit_history_draft: String::new(),
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let (mut terminal, keyboard_enhanced) = setup_terminal()?;

        // Sync layout dimensions with actual terminal size so mouse handling
        // uses the correct geometry from the very first frame.
        let size = terminal.size()?;
        self.layout.update_size(size.width, size.height);

        let result = self.main_loop(&mut terminal);

        restore_terminal(&mut terminal, keyboard_enhanced)?;
        result
    }

    fn main_loop(&mut self, terminal: &mut Term) -> Result<()> {
        loop {
            // Drain any model parts that have arrived from the background load.
            if let Some(rx) = &self.initial_load_rx {
                let mut got_files = false;
                while let Ok(part) = rx.try_recv() {
                    let mut model = self.model.lock().unwrap();
                    match part {
                        ModelPart::Files(v) => { model.files = v; got_files = true; }
                        ModelPart::Branches(v) => model.branches = v,
                        ModelPart::Commits(v) => model.commits = v,
                        ModelPart::Stash(v) => model.stash_entries = v,
                        ModelPart::Remotes(v) => model.remotes = v,
                        ModelPart::Tags(v) => model.tags = v,
                        ModelPart::Worktrees(v) => model.worktrees = v,
                        ModelPart::Submodules(v) => model.submodules = v,
                        ModelPart::Reflog(v) => model.reflog_commits = v,
                        ModelPart::DiffStats { added, deleted } => {
                            model.total_additions = added;
                            model.total_deletions = deleted;
                        }
                        ModelPart::RepoStatus {
                            is_rebasing, is_merging, is_cherry_picking, is_bisecting, rebase_onto_hash,
                        } => {
                            model.is_rebasing = is_rebasing;
                            model.is_merging = is_merging;
                            model.is_cherry_picking = is_cherry_picking;
                            model.is_bisecting = is_bisecting;
                            model.rebase_onto_hash = rebase_onto_hash;
                        }
                    }
                    self.initial_load_received += 1;
                }
                // Rebuild file tree if files arrived this frame.
                if got_files && self.show_file_tree {
                    let model = self.model.lock().unwrap();
                    self.file_tree_nodes =
                        build_file_tree(&model.files, &self.collapsed_dirs);
                    self.context_mgr.files_list_len_override =
                        Some(self.file_tree_nodes.len());
                }
                // Trigger a diff load once any data arrives.
                if self.initial_load_received > 0 {
                    self.needs_diff_refresh = true;
                }
                // All parts received — done loading.
                if self.initial_load_received >= MODEL_PART_COUNT {
                    self.initial_load_rx = None;
                }
            }

            // Request diff loading on background thread if selection changed
            self.maybe_request_diff();

            // Check for completed background diff results
            self.receive_diff_results();

            // Check for AI commit message generation results
            self.receive_ai_commit_results();

            // Check for completed background remote operations
            self.receive_remote_op_results();

            // Advance spinner animation
            self.spinner_frame = self.spinner_frame.wrapping_add(1);

            // Render
            terminal.draw(|frame| {
                if self.rebase_mode.active {
                    let theme = self.config.user_config.theme();
                    presentation::rebase_mode::render(
                        frame,
                        &self.rebase_mode,
                        &theme,
                    );
                    // Render popup overlay on top of rebase mode
                    if self.popup != PopupState::None {
                        views::render_popup(frame, &self.popup, frame.area(), self.spinner_frame);
                    }
                } else if self.diff_mode.active {
                    let theme = self.config.user_config.theme();
                    presentation::diff_mode::render(
                        frame,
                        &self.diff_mode,
                        &mut self.diff_view,
                        &theme,
                    );
                    // Render popup overlay on top of diff mode (for ? help, errors, etc.)
                    if self.popup != PopupState::None {
                        views::render_popup(frame, &self.popup, frame.area(), self.spinner_frame);
                    }
                } else {
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
                        &mut self.diff_view,
                        self.screen_mode,
                        self.show_file_tree,
                        &self.file_tree_nodes,
                        &self.collapsed_dirs,
                        self.diff_focused,
                        search_state,
                        self.search_textarea.as_ref(),
                        &cmd_log,
                        self.show_command_log,
                        &self.commit_branch_filter,
                        self.show_commit_file_tree,
                        &self.commit_file_tree_nodes,
                        &self.commit_files_collapsed_dirs,
                        &self.commit_files_hash,
                        &self.commit_files_message,
                        &self.branch_commits_name,
                        &self.remote_branches_name,
                        self.spinner_frame,
                        self.remote_op_label.as_deref(),
                        self.remote_op_success_at
                            .map(|t| t.elapsed() < std::time::Duration::from_secs(5))
                            .unwrap_or(false),
                    );
                }
            })?;

            // Handle events
            if event::poll(std::time::Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Press => {
                        self.handle_key(key)?;
                    }
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    Event::Resize(w, h) => {
                        self.layout.update_size(w, h);
                    }
                    Event::FocusGained if self.config.user_config.git.auto_refresh => {
                        self.needs_refresh = true;
                    }
                    _ => {}
                }
            }

            // Background auto-refresh every 10s (like lazygit's refresher.refreshInterval)
            if self.config.user_config.git.auto_refresh
                && self.last_refresh_at.elapsed().as_secs() >= 10
            {
                self.needs_refresh = true;
            }

            // Refresh data if needed
            if self.needs_refresh {
                self.refresh()?;
                self.needs_refresh = false;
                self.needs_files_refresh = false;
                self.needs_diff_refresh = true;
                self.last_refresh_at = Instant::now();
            } else if self.needs_files_refresh {
                self.refresh_files_only()?;
                self.needs_files_refresh = false;
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
                    self.diff_view.file_exists_on_disk =
                        self.git.repo_path().join(&filename).exists();
                }
                DiffPayload::UnifiedDiff { filename, diff_output } => {
                    self.diff_view.load_from_diff_output(&filename, &diff_output);
                    self.diff_view.file_exists_on_disk =
                        self.git.repo_path().join(&filename).exists();
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
                            is_commit: true, confirm_focused: false,
                        };
                    }
                }
                Err(e) => {
                    // On failure, restore the stashed editor so user can type manually
                    if let Some(stashed) = self.pending_commit_popup.take() {
                        self.popup = stashed;
                    } else {
                        self.popup = PopupState::Message {
                            title: "AI generation failed".to_string(),
                            message: format!("{}", e),
                            kind: MessageKind::Error,
                        };
                    }
                }
            }
        }
    }

    /// Check for completed background remote operations (push, pull, fetch).
    fn receive_remote_op_results(&mut self) {
        if let Ok(result) = self.remote_op_rx.try_recv() {
            self.remote_op_label = None;
            match result {
                Ok(()) => {
                    self.popup = PopupState::None;
                    self.needs_refresh = true;
                    self.remote_op_success_at = Some(Instant::now());
                }
                Err(e) => {
                    self.popup = PopupState::Message {
                        title: "Error".to_string(),
                        message: format!("{}", e),
                        kind: MessageKind::Error,
                    };
                }
            }
        }
    }

    /// Run a remote operation (push/pull/fetch) on a background thread with a loading popup.
    pub fn start_remote_op<F>(&mut self, title: &str, message: &str, op: F)
    where
        F: FnOnce(&GitCommands) -> Result<()> + Send + 'static,
    {
        self.popup = PopupState::Loading {
            title: title.to_string(),
            message: message.to_string(),
        };
        // Show operation label on the head branch in the sidebar (e.g. "Pushing", "Pulling").
        let label = match title {
            "Push" => "Pushing",
            "Pull" => "Pulling",
            "Fetch" => "Fetching",
            other => other,
        };
        self.remote_op_label = Some(label.to_string());
        self.remote_op_success_at = None;
        let git = Arc::clone(&self.git);
        let tx = self.remote_op_tx.clone();
        std::thread::spawn(move || {
            let result = op(&git);
            let _ = tx.send(result);
        });
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
        // Rebase mode has no diff to load
        if self.rebase_mode.active {
            return;
        }

        // Diff mode has its own diff loading
        if self.diff_mode.active {
            let diff_key = format!("diffmode:{}", self.diff_mode.diff_files_selected);
            if diff_key == self.last_diff_key && !self.needs_diff_refresh {
                return;
            }
            self.last_diff_key = diff_key;
            self.needs_diff_refresh = false;
            controller::diff_mode::maybe_request_diff(self);
            return;
        }

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
                                    self.diff_view.file_exists_on_disk =
                                        self.git.repo_path().join(&name).exists();
                                } else {
                                    self.diff_view = DiffViewState::new();
                                }
                            }
                        } else if diff.is_empty() {
                            self.diff_view = DiffViewState::new();
                        } else {
                            self.diff_view.load_from_diff_output(&name, &diff);
                            self.diff_view.file_exists_on_disk =
                                self.git.repo_path().join(&name).exists();
                        }
                    }
                } else if self.show_file_tree {
                    // Directory node: show combined diff of all child files
                    if let Some(node) = self.file_tree_nodes.get(selected) {
                        if node.is_dir && !node.child_file_indices.is_empty() {
                            let child_names: Vec<(String, bool, bool, bool)> = node
                                .child_file_indices
                                .iter()
                                .filter_map(|&i| model.files.get(i))
                                .map(|f| (f.name.clone(), f.has_unstaged_changes, f.has_staged_changes, f.tracked))
                                .collect();
                            let dir_name = node.name.clone();
                            drop(model);

                            let mut combined_diff = String::new();
                            for (name, has_unstaged, has_staged, tracked) in &child_names {
                                let diff = if *has_unstaged {
                                    self.git.diff_file(name).unwrap_or_default()
                                } else if *has_staged {
                                    self.git.diff_file_staged(name).unwrap_or_default()
                                } else if !tracked {
                                    self.git.file_content(name).unwrap_or_default()
                                } else {
                                    String::new()
                                };
                                if !diff.is_empty() {
                                    if !combined_diff.is_empty() {
                                        combined_diff.push('\n');
                                    }
                                    combined_diff.push_str(&diff);
                                }
                            }

                            if combined_diff.is_empty() {
                                self.diff_view = DiffViewState::new();
                            } else {
                                self.diff_view.load_from_diff_output(&dir_name, &combined_diff);
                                self.diff_view.file_exists_on_disk = true;
                            }
                        } else {
                            drop(model);
                            self.diff_view = DiffViewState::new();
                        }
                    } else {
                        drop(model);
                        self.diff_view = DiffViewState::new();
                    }
                } else {
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
            ContextId::Reflog => {
                // Reflog: load commit diff async
                if let Some(commit) = model.reflog_commits.get(selected) {
                    let hash = commit.hash.clone();
                    drop(model);

                    let git = Arc::clone(&self.git);
                    let tx = self.diff_tx.clone();
                    let gen_counter = Arc::clone(&self.diff_generation);

                    std::thread::spawn(move || {
                        if gen_counter.load(Ordering::Relaxed) != generation {
                            return;
                        }
                        let payload = if let Ok(diff) = git.diff_commit(&hash) {
                            let filename = format!("reflog:{}", &hash[..7.min(hash.len())]);
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
            ContextId::BranchCommits => {
                // BranchCommits: load commit diff for the selected commit
                if let Some(commit) = model.sub_commits.get(selected) {
                    let hash = commit.hash.clone();
                    drop(model);

                    let git = Arc::clone(&self.git);
                    let tx = self.diff_tx.clone();
                    let gen_counter = Arc::clone(&self.diff_generation);

                    std::thread::spawn(move || {
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
                } else {
                    drop(model);
                }
            }
            ContextId::CommitFiles | ContextId::StashFiles | ContextId::BranchCommitFiles => {
                // CommitFiles/StashFiles/BranchCommitFiles: load diff for the selected file within the commit/stash
                let file_idx = if self.show_commit_file_tree {
                    self.commit_file_tree_nodes.get(selected).and_then(|n| n.file_index)
                } else {
                    Some(selected)
                };
                if let Some(commit_file) = file_idx.and_then(|i| model.commit_files.get(i)) {
                    let name = commit_file.name.clone();
                    let hash = self.commit_files_hash.clone();
                    drop(model);

                    let git = Arc::clone(&self.git);
                    let tx = self.diff_tx.clone();
                    let gen_counter = Arc::clone(&self.diff_generation);

                    std::thread::spawn(move || {
                        if gen_counter.load(Ordering::Relaxed) != generation {
                            return;
                        }
                        let payload = if let Ok(diff) = git.diff_commit_file(&hash, &name) {
                            if diff.is_empty() {
                                DiffPayload::Empty
                            } else {
                                DiffPayload::UnifiedDiff { filename: name, diff_output: diff }
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
                } else if self.show_commit_file_tree {
                    // Directory node in tree view: show combined diff of all child files
                    if let Some(node) = self.commit_file_tree_nodes.get(selected) {
                        if node.is_dir && !node.child_file_indices.is_empty() {
                            let child_names: Vec<String> = node
                                .child_file_indices
                                .iter()
                                .filter_map(|&i| model.commit_files.get(i))
                                .map(|f| f.name.clone())
                                .collect();
                            let dir_name = node.name.clone();
                            let hash = self.commit_files_hash.clone();
                            drop(model);

                            let git = Arc::clone(&self.git);
                            let tx = self.diff_tx.clone();
                            let gen_counter = Arc::clone(&self.diff_generation);

                            std::thread::spawn(move || {
                                if gen_counter.load(Ordering::Relaxed) != generation {
                                    return;
                                }
                                let mut combined_diff = String::new();
                                for name in &child_names {
                                    if let Ok(diff) = git.diff_commit_file(&hash, name) {
                                        if !diff.is_empty() {
                                            if !combined_diff.is_empty() {
                                                combined_diff.push('\n');
                                            }
                                            combined_diff.push_str(&diff);
                                        }
                                    }
                                }
                                let payload = if combined_diff.is_empty() {
                                    DiffPayload::Empty
                                } else {
                                    DiffPayload::UnifiedDiff {
                                        filename: dir_name,
                                        diff_output: combined_diff,
                                    }
                                };
                                let _ = tx.send(DiffResult {
                                    generation,
                                    diff_key,
                                    payload,
                                });
                            });
                        } else {
                            drop(model);
                            self.diff_view = DiffViewState::new();
                        }
                    } else {
                        drop(model);
                        self.diff_view = DiffViewState::new();
                    }
                } else {
                    // No file selected — clear diff
                    drop(model);
                    self.diff_view = DiffViewState::new();
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

        // Rebase mode takes priority over everything
        if self.rebase_mode.active {
            return controller::rebase_mode::handle_key(self, key);
        }

        // Diff mode takes priority over normal UI
        if self.diff_mode.active {
            return controller::diff_mode::handle_key(self, key);
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
                // If we're in a sub-context (CommitFiles), pressing the parent window's
                // number key should exit the sub-context first.
                if self.context_mgr.active() == ContextId::CommitFiles
                    && window == SideWindow::Commits
                {
                    self.context_mgr.set_active(ContextId::Commits);
                    return Ok(());
                }
                if self.context_mgr.active() == ContextId::StashFiles
                    && window == SideWindow::Stash
                {
                    self.context_mgr.set_active(ContextId::Stash);
                    return Ok(());
                }
                if (self.context_mgr.active() == ContextId::BranchCommits
                    || self.context_mgr.active() == ContextId::BranchCommitFiles)
                    && window == SideWindow::Branches
                {
                    if self.context_mgr.active() == ContextId::BranchCommitFiles {
                        self.context_mgr.set_active(ContextId::BranchCommits);
                    } else {
                        self.context_mgr.set_active(ContextId::Branches);
                    }
                    return Ok(());
                }
                if self.context_mgr.active() == ContextId::RemoteBranches
                    && window == SideWindow::Branches
                {
                    self.context_mgr.set_active(ContextId::Remotes);
                    return Ok(());
                }
                self.context_mgr.jump_to_window(window);
                return Ok(());
            }
        }

        // Tab to switch windows
        if matches_key(key, &keybindings.universal.toggle_panel) {
            self.exit_sub_contexts();
            self.context_mgr.next_window();
            return Ok(());
        }

        // Arrow keys / h/l to switch windows
        if matches_key(key, &keybindings.universal.prev_block)
            || matches_key(key, &keybindings.universal.prev_block_alt)
        {
            self.exit_sub_contexts();
            self.context_mgr.prev_window();
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.next_block)
            || matches_key(key, &keybindings.universal.next_block_alt)
        {
            self.exit_sub_contexts();
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

            // If rebasing, re-enter the interactive rebase view
            if is_rebasing {
                if !self.rebase_mode.active {
                    if let Some(mut progress) = self.git.parse_rebase_progress() {
                        self.git.hydrate_todo_entries(&mut progress.done_entries);
                        self.git.hydrate_todo_entries(&mut progress.todo_entries);
                        self.rebase_mode.enter_in_progress(&progress);
                    }
                }
                return Ok(());
            }

            if is_merging || is_cherry_picking {
                return self.show_rebase_options_menu(false, is_merging, is_cherry_picking);
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

        // Diff/Compare mode (W)
        if key.code == KeyCode::Char('W') {
            self.diff_mode.enter();
            self.diff_view = DiffViewState::new();
            return Ok(());
        }

        // Toggle command log (;)
        if key.code == KeyCode::Char(';') {
            self.show_command_log = !self.show_command_log;
            self.persist_command_log_visibility();
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

        // Help popup (?)
        if key.code == KeyCode::Char('?') {
            self.show_help();
            return Ok(());
        }

        // Start search
        if matches_key(key, &keybindings.universal.start_search) {
            self.search_active = true;
            self.search_query.clear();
            self.search_matches.clear();
            self.search_match_idx = 0;
            let mut ta = tui_textarea::TextArea::default();
            ta.set_cursor_line_style(ratatui::style::Style::default());
            self.search_textarea = Some(ta);
            return Ok(());
        }

        // Next/prev search match, or Esc to dismiss search results
        if !self.search_query.is_empty() {
            if key.code == KeyCode::Esc {
                self.search_query.clear();
                self.search_matches.clear();
                self.search_match_idx = 0;
                return Ok(());
            }
            if matches_key(key, &keybindings.universal.next_match) {
                self.goto_next_search_match();
                return Ok(());
            }
            if matches_key(key, &keybindings.universal.prev_match) {
                self.goto_prev_search_match();
                return Ok(());
            }
        }

        // Universal "I" key: interactive rebase picker
        if key.code == KeyCode::Char('I') {
            self.show_interactive_rebase_picker();
            return Ok(());
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
            ContextId::Reflog => {
                controller::reflog::handle_key(self, key, &keybindings)?;
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
            ContextId::RemoteBranches => {
                controller::remote_branches::handle_key(self, key, &keybindings)?;
            }
            ContextId::CommitFiles | ContextId::StashFiles | ContextId::BranchCommitFiles => {
                controller::commit_files::handle_key(self, key, &keybindings)?;
            }
            ContextId::BranchCommits => {
                controller::branch_commits::handle_key(self, key, &keybindings)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_diff_focused_search_key(&mut self, key: KeyEvent) -> Result<()> {
        if let Some(ref mut ta) = self.diff_view.search_textarea {
            match key.code {
                KeyCode::Esc => {
                    self.diff_view.dismiss_search();
                }
                KeyCode::Enter => {
                    self.diff_view.dismiss_search();
                    if !self.diff_view.search_matches.is_empty() {
                        self.diff_view.search_match_idx = 0;
                        self.diff_view.scroll_to_current_match();
                    }
                }
                _ => {
                    ta.input(key);
                    self.diff_view.search_query = ta.lines().join("");
                    self.diff_view.update_search();
                }
            }
        }
        Ok(())
    }

    fn handle_diff_focused_key(&mut self, key: KeyEvent) -> Result<()> {
        // Diff search input mode takes priority
        if self.diff_view.search_active {
            return self.handle_diff_focused_search_key(key);
        }

        // Handle text selection keys first (y to copy, e to edit, Esc to dismiss)
        if self.diff_view.selection.is_some() {
            let is_click = self.diff_view.selection.as_ref().unwrap().is_click;
            let can_edit = self.diff_view.file_exists_on_disk;
            match key.code {
                KeyCode::Char('e') if can_edit => {
                    let sel_ref = self.diff_view.selection.as_ref().unwrap();
                    let line = sel_ref.edit_line_number;
                    // Compute column from terminal position using the same layout as the mouse handler
                    let (top_row, top_col, _, _) = sel_ref.normalized();
                    let main_panel = self.compute_main_panel_rect();
                    let pl = DiffPanelLayout::compute(main_panel, &self.diff_view);
                    let (content_start, _) = pl.content_range(sel_ref.panel);
                    let column = if top_col >= content_start {
                        (top_col - content_start) as usize + self.diff_view.horizontal_scroll + 1
                    } else {
                        1
                    };
                    // Resolve the actual filename for multi-file diffs
                    let line_idx = if top_row >= pl.inner_y {
                        self.diff_view.scroll_offset + (top_row - pl.inner_y) as usize
                    } else {
                        0
                    };
                    let filename = self.diff_view.file_at_line(line_idx).to_string();
                    self.diff_view.selection = None;
                    let abs_path = self.git.repo_path().join(&filename);
                    if !filename.is_empty() && abs_path.exists() {
                        let abs_path = abs_path.to_string_lossy().to_string();
                        let os = &self.config.user_config.os;
                        if let Some(ln) = line {
                            let tpl = if !os.edit_at_line.is_empty() { &os.edit_at_line } else { &os.edit };
                            let _ = crate::config::user_config::OsConfig::run_template_at_line(tpl, &abs_path, ln, column);
                        } else {
                            let _ = crate::config::user_config::OsConfig::run_template(&os.edit, &abs_path);
                        }
                    }
                    return Ok(());
                }
                KeyCode::Char('y') if !is_click => {
                    let text = self.diff_view.selection.as_ref().unwrap().text.clone();
                    self.diff_view.selection = None;
                    if !text.is_empty() {
                        crate::os::platform::Platform::copy_to_clipboard(&text)?;
                    }
                    return Ok(());
                }
                KeyCode::Esc => {
                    self.diff_view.selection = None;
                    return Ok(());
                }
                _ => {
                    self.diff_view.selection = None;
                    if is_click {
                        // Don't propagate click-state dismissal as a real keypress
                        return Ok(());
                    }
                }
            }
        }

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

        // Start diff content search (/)
        if matches_key(key, &keybindings.universal.start_search) {
            self.diff_view.start_search();
            return Ok(());
        }

        // n/N to navigate diff search matches
        if !self.diff_view.search_query.is_empty() {
            if matches_key(key, &keybindings.universal.next_match) {
                self.diff_view.next_search_match();
                return Ok(());
            }
            if matches_key(key, &keybindings.universal.prev_match) {
                self.diff_view.prev_search_match();
                return Ok(());
            }
        }

        // Toggle command log (;)
        if key.code == KeyCode::Char(';') {
            self.show_command_log = !self.show_command_log;
            self.persist_command_log_visibility();
            return Ok(());
        }

        // Help popup
        if key.code == KeyCode::Char('?') {
            self.show_diff_help();
            return Ok(());
        }

        // Number keys 1-5 to jump to sidebar panels (unfocus diff)
        // Use set_window instead of jump_to_window to avoid cycling tabs,
        // since the user is "arriving" from diff focus, not pressing the same key again.
        if let KeyCode::Char(c @ '1'..='5') = key.code {
            let n = c.to_digit(10).unwrap();
            if let Some(window) = SideWindow::from_number(n) {
                self.diff_focused = false;
                self.context_mgr.set_window(window);
                return Ok(());
            }
        }

        match key.code {
            // Escape: clear search first, then unfocus diff
            KeyCode::Esc => {
                if !self.diff_view.search_query.is_empty() {
                    self.diff_view.clear_search();
                } else {
                    self.diff_focused = false;
                }
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
            // { and } jump between hunks
            KeyCode::Char('}') => {
                self.diff_view.next_hunk();
            }
            KeyCode::Char('{') => {
                self.diff_view.prev_hunk();
            }
            // [ and ] toggle old-only / new-only view
            KeyCode::Char(']') => {
                use crate::pager::side_by_side::DiffSideView;
                self.diff_view.side_view = match self.diff_view.side_view {
                    DiffSideView::NewOnly => DiffSideView::Both,
                    _ => DiffSideView::NewOnly,
                };
            }
            KeyCode::Char('[') => {
                use crate::pager::side_by_side::DiffSideView;
                self.diff_view.side_view = match self.diff_view.side_view {
                    DiffSideView::OldOnly => DiffSideView::Both,
                    _ => DiffSideView::OldOnly,
                };
            }
            // z toggles line wrapping
            KeyCode::Char('z') => {
                self.diff_view.wrap = !self.diff_view.wrap;
                self.diff_view.horizontal_scroll = 0;
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

    pub(crate) fn handle_popup_key(&mut self, key: KeyEvent) -> Result<()> {
        match &self.popup {
            PopupState::Confirm { .. } => {
                if key.code == KeyCode::Char('y') || key.code == KeyCode::Enter {
                    let popup = std::mem::replace(&mut self.popup, PopupState::None);
                    if let PopupState::Confirm { on_confirm, .. } = popup {
                        if let Err(e) = on_confirm(self) {
                            self.popup = PopupState::Message {
                                title: "Error".to_string(),
                                message: format!("{}", e),
                                kind: MessageKind::Error,
                            };
                        }
                    }
                } else {
                    self.popup = PopupState::None;
                }
            }
            PopupState::Message { .. } => {
                // Any key dismisses the message
                self.popup = PopupState::None;
            }
            PopupState::Menu { items, selected, .. } => {
                let _items_len = items.len();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if let PopupState::Menu { items, selected, .. } = &mut self.popup {
                            // Skip disabled items
                            let mut next = *selected + 1;
                            while next < items.len() && items[next].action.is_none() {
                                next += 1;
                            }
                            if next < items.len() {
                                *selected = next;
                            }
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if let PopupState::Menu { items, selected, .. } = &mut self.popup {
                            // Skip disabled items
                            if *selected > 0 {
                                let mut prev = *selected - 1;
                                while prev > 0 && items[prev].action.is_none() {
                                    prev -= 1;
                                }
                                if items[prev].action.is_some() {
                                    *selected = prev;
                                }
                            }
                        }
                    }
                    KeyCode::Enter => {
                        let popup = std::mem::replace(&mut self.popup, PopupState::None);
                        if let PopupState::Menu { items, selected, .. } = popup {
                            if let Some(item) = items.get(selected) {
                                if let Some(ref action) = item.action {
                                    if let Err(e) = action(self) {
                                        self.popup = PopupState::Message {
                                            title: "Error".to_string(),
                                            message: format!("{}", e),
                                            kind: MessageKind::Error,
                                        };
                                    }
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
                                        if let Err(e) = action(self) {
                                            self.popup = PopupState::Message {
                                                title: "Error".to_string(),
                                                message: format!("{}", e),
                                                kind: MessageKind::Error,
                                            };
                                        }
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
            PopupState::Input { is_commit, confirm_focused, .. } => {
                use crossterm::event::KeyModifiers;
                let is_commit = *is_commit;
                let confirm_focused = *confirm_focused;

                // Tab toggles focus between textarea and confirm button (commit only)
                if is_commit && key.code == KeyCode::Tab {
                    if let PopupState::Input { confirm_focused, .. } = &mut self.popup {
                        *confirm_focused = !*confirm_focused;
                    }
                }
                // Confirm: Ctrl+S for commit, Enter on confirm button, Enter for non-commit
                else if (is_commit
                    && key.code == KeyCode::Char('s')
                    && key.modifiers.contains(KeyModifiers::CONTROL))
                    || (is_commit && confirm_focused && key.code == KeyCode::Enter)
                    || (!is_commit && key.code == KeyCode::Enter)
                {
                    let popup = std::mem::replace(&mut self.popup, PopupState::None);
                    if let PopupState::Input { textarea, on_confirm, is_commit: was_commit, .. } = popup {
                        let text = textarea.lines().join("\n");
                        // Save to commit history before calling on_confirm
                        if was_commit && !text.trim().is_empty() {
                            // Remove duplicate if it exists
                            self.commit_message_history.retain(|m| m != &text);
                            self.commit_message_history.insert(0, text.clone());
                            // Keep history bounded
                            self.commit_message_history.truncate(50);
                            self.save_commit_history();
                        }
                        self.commit_history_idx = None;
                        if let Err(e) = on_confirm(self, &text) {
                            self.popup = PopupState::Message {
                                title: "Error".to_string(),
                                message: format!("{}", e),
                                kind: MessageKind::Error,
                            };
                        }
                    }
                } else if key.code == KeyCode::Esc {
                    self.popup = PopupState::None;
                    self.commit_history_idx = None;
                } else if is_commit
                    && !confirm_focused
                    && (key.code == KeyCode::Up || key.code == KeyCode::Down)
                    && !self.commit_message_history.is_empty()
                {
                    // Cycle through commit message history with Up/Down
                    if let PopupState::Input { textarea, .. } = &mut self.popup {
                        // Only cycle if on first line (Up) or last line (Down)
                        let cursor_row = textarea.cursor().0;
                        let line_count = textarea.lines().len();
                        let should_cycle = match key.code {
                            KeyCode::Up => cursor_row == 0,
                            KeyCode::Down => cursor_row >= line_count.saturating_sub(1),
                            _ => false,
                        };

                        if should_cycle {
                            let history_len = self.commit_message_history.len();
                            match key.code {
                                KeyCode::Up => {
                                    let new_idx = match self.commit_history_idx {
                                        None => {
                                            // Save current draft
                                            self.commit_history_draft = textarea.lines().join("\n");
                                            0
                                        }
                                        Some(idx) => (idx + 1).min(history_len - 1),
                                    };
                                    self.commit_history_idx = Some(new_idx);
                                    let msg = &self.commit_message_history[new_idx];
                                    let mut new_ta = popup::make_textarea("Enter commit message...");
                                    new_ta.insert_str(msg);
                                    *textarea = new_ta;
                                }
                                KeyCode::Down => {
                                    match self.commit_history_idx {
                                        Some(0) => {
                                            // Go back to draft
                                            self.commit_history_idx = None;
                                            let draft = self.commit_history_draft.clone();
                                            let mut new_ta = popup::make_textarea("Enter commit message...");
                                            new_ta.insert_str(&draft);
                                            *textarea = new_ta;
                                        }
                                        Some(idx) => {
                                            let new_idx = idx - 1;
                                            self.commit_history_idx = Some(new_idx);
                                            let msg = &self.commit_message_history[new_idx];
                                            let mut new_ta = popup::make_textarea("Enter commit message...");
                                            new_ta.insert_str(msg);
                                            *textarea = new_ta;
                                        }
                                        None => {
                                            // Already at draft, do nothing
                                        }
                                    }
                                }
                                _ => {}
                            }
                        } else {
                            // Not at boundary — forward to textarea for normal cursor movement
                            textarea.input(key);
                        }
                    }
                } else if is_commit
                    && !confirm_focused
                    && key.code == KeyCode::Char('o')
                    && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    // <c-o> in commit message editor: open commit menu
                    self.show_commit_editor_menu()?;
                } else if !confirm_focused {
                    // Forward all other keys to the textarea (only when textarea is focused)
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
            PopupState::Checklist { items, selected, search, .. } => {
                use crossterm::event::KeyModifiers;
                // Ctrl+A: clear all checks
                if key.code == KeyCode::Char('a') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    if let PopupState::Checklist { items, .. } = &mut self.popup {
                        for item in items.iter_mut() {
                            item.checked = false;
                        }
                    }
                    return Ok(());
                }
                let visible_count = items.iter()
                    .filter(|it| search.is_empty() || it.label.to_lowercase().contains(&search.to_lowercase()))
                    .count();
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') => {
                        if let PopupState::Checklist { selected, .. } = &mut self.popup {
                            if visible_count > 0 {
                                *selected = (*selected + 1).min(visible_count - 1);
                            }
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if let PopupState::Checklist { selected, .. } = &mut self.popup {
                            *selected = selected.saturating_sub(1);
                        }
                    }
                    KeyCode::Char(' ') => {
                        // Toggle checked state on the visible item at `selected`
                        if let PopupState::Checklist { items, selected, search, .. } = &mut self.popup {
                            let visible_indices: Vec<usize> = items.iter().enumerate()
                                .filter(|(_, it)| search.is_empty() || it.label.to_lowercase().contains(&search.to_lowercase()))
                                .map(|(i, _)| i)
                                .collect();
                            if let Some(&real_idx) = visible_indices.get(*selected) {
                                items[real_idx].checked = !items[real_idx].checked;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        let popup = std::mem::replace(&mut self.popup, PopupState::None);
                        if let PopupState::Checklist { items, on_confirm, .. } = popup {
                            let checked: Vec<String> = items.into_iter()
                                .filter(|it| it.checked)
                                .map(|it| it.label)
                                .collect();
                            if let Err(e) = on_confirm(self, checked) {
                                self.popup = PopupState::Message {
                                    title: "Error".to_string(),
                                    message: format!("{}", e),
                                    kind: MessageKind::Error,
                                };
                            }
                        }
                    }
                    KeyCode::Esc => {
                        self.popup = PopupState::None;
                    }
                    KeyCode::Backspace => {
                        if let PopupState::Checklist { search, selected, .. } = &mut self.popup {
                            search.pop();
                            *selected = 0;
                        }
                    }
                    KeyCode::Char(c) => {
                        // Type into search filter (but not j/k which are nav)
                        // j/k already handled above, this won't fire for them
                        if let PopupState::Checklist { search, selected, .. } = &mut self.popup {
                            search.push(c);
                            *selected = 0;
                        }
                    }
                    _ => {}
                }
            }
            PopupState::Loading { .. } => {
                // Block all input while loading — user must wait
            }
            PopupState::Help { .. } => {}
            PopupState::RefPicker { .. } => {}
            PopupState::None => {}
        }

        // Help popup is handled separately to avoid borrow conflicts
        if matches!(self.popup, PopupState::Help { .. }) {
            self.handle_help_popup_key(key);
        }

        // RefPicker popup is handled separately to avoid borrow conflicts
        if matches!(self.popup, PopupState::RefPicker { .. }) {
            self.handle_ref_picker_key(key)?;
        }

        Ok(())
    }

    fn handle_help_popup_key(&mut self, key: KeyEvent) {
        // Helper: compute display index for a given entry selection
        fn find_display_idx(sections: &[HelpSection], sel: usize, search_lower: &str) -> usize {
            let has_search = !search_lower.is_empty();
            let mut ei = 0usize;
            let mut di = 0usize;
            for section in sections {
                let mut section_has_visible = false;
                for entry in &section.entries {
                    let matches = !has_search
                        || entry.key.to_lowercase().contains(search_lower)
                        || entry.description.to_lowercase().contains(search_lower);
                    if matches {
                        if !section_has_visible {
                            section_has_visible = true;
                            di += 1; // header row
                        }
                        if ei == sel {
                            return di;
                        }
                        ei += 1;
                        di += 1;
                    }
                }
            }
            di
        }

        fn count_visible(sections: &[HelpSection], search_lower: &str) -> usize {
            let has_search = !search_lower.is_empty();
            sections.iter().map(|s| {
                if has_search {
                    s.entries.iter().filter(|e| {
                        e.key.to_lowercase().contains(search_lower)
                            || e.description.to_lowercase().contains(search_lower)
                    }).count()
                } else {
                    s.entries.len()
                }
            }).sum()
        }

        if let PopupState::Help { sections, selected, search_textarea, scroll_offset } = &mut self.popup {
            use crossterm::event::KeyModifiers;
            let search = search_textarea.lines().join("");
            let search_lower = search.to_lowercase();

            // Estimate list viewport height from terminal
            let popup_height = (self.layout.height as usize).saturating_sub(4).min(50);
            let list_height = popup_height.saturating_sub(5); // borders + search + sep + hint

            match key.code {
                KeyCode::Esc | KeyCode::Char('?') if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
                    self.popup = PopupState::None;
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') if key.modifiers.is_empty() => {
                    let total = count_visible(sections, &search_lower);
                    if total > 0 {
                        *selected = (*selected + 1).min(total.saturating_sub(1));
                    }
                    let sdi = find_display_idx(sections, *selected, &search_lower);
                    if sdi >= *scroll_offset + list_height {
                        *scroll_offset = sdi.saturating_sub(list_height - 1);
                    }
                }
                KeyCode::Up | KeyCode::Char('k') if key.modifiers.is_empty() => {
                    *selected = selected.saturating_sub(1);
                    if *selected == 0 {
                        // First item: always scroll to top so the section header is visible
                        *scroll_offset = 0;
                    } else {
                        let sdi = find_display_idx(sections, *selected, &search_lower);
                        if sdi <= *scroll_offset {
                            // Scroll up to show the section header too when possible
                            *scroll_offset = sdi.saturating_sub(1);
                        }
                    }
                }
                _ => {
                    search_textarea.input(key);
                    let new_search = search_textarea.lines().join("");
                    if new_search != search {
                        *selected = 0;
                        *scroll_offset = 0;
                    }
                }
            }
        }
    }

    fn handle_ref_picker_key(&mut self, key: KeyEvent) -> Result<()> {
        use crate::gui::popup::RefPickerItem;

        /// Compute the display row index for a given entry selection,
        /// accounting for category header rows.
        fn find_display_idx(items: &[RefPickerItem], sel: usize) -> usize {
            let mut di = 0usize;
            let mut last_cat = String::new();
            for (ei, item) in items.iter().enumerate() {
                if item.category != last_cat {
                    di += 1; // header row
                    last_cat = item.category.clone();
                }
                if ei == sel {
                    return di;
                }
                di += 1;
            }
            di
        }

        if let PopupState::RefPicker { items, selected, search_textarea, scroll_offset, .. } = &mut self.popup {
            let search = search_textarea.lines().join("");
            let total = items.len();

            // Must match the rendering formula exactly:
            // popup_height = (height * 60 / 100).max(10).min(height - 4)
            // inner.height = popup_height - 2 (borders)
            // list_height = inner.height - 3 (search + sep + hint)
            let h = self.layout.height as usize;
            let popup_h = (h * 60 / 100).max(10).min(h.saturating_sub(4));
            let list_height = popup_h.saturating_sub(2).saturating_sub(3);

            match key.code {
                KeyCode::Esc => {
                    self.popup = PopupState::None;
                    return Ok(());
                }
                KeyCode::Enter => {
                    // Use selected item if available, otherwise use the raw
                    // search text as a ref expression (e.g. HEAD~1, abc123~2).
                    let value = if let Some(item) = items.get(*selected) {
                        item.value.clone()
                    } else if !search.trim().is_empty() {
                        search.trim().to_string()
                    } else {
                        return Ok(());
                    };
                    let popup = std::mem::replace(&mut self.popup, PopupState::None);
                    if let PopupState::RefPicker { on_confirm, .. } = popup {
                        if let Err(e) = on_confirm(self, &value) {
                            self.popup = PopupState::Message {
                                title: "Error".to_string(),
                                message: format!("{}", e),
                                kind: MessageKind::Error,
                            };
                        }
                    }
                    return Ok(());
                }
                KeyCode::Down => {
                    if total > 0 {
                        *selected = (*selected + 1).min(total.saturating_sub(1));
                    }
                    let sdi = find_display_idx(items, *selected);
                    if sdi >= *scroll_offset + list_height {
                        *scroll_offset = sdi.saturating_sub(list_height - 1);
                    }
                }
                KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                    if *selected == 0 {
                        *scroll_offset = 0;
                    } else {
                        let sdi = find_display_idx(items, *selected);
                        if sdi <= *scroll_offset {
                            *scroll_offset = sdi.saturating_sub(1);
                        }
                    }
                }
                _ => {
                    search_textarea.input(key);
                    let new_search = search_textarea.lines().join("");
                    if new_search != search {
                        // Remove any previous raw-ref item at index 0
                        if !items.is_empty() && items[0].category == "[ref]" {
                            items.remove(0);
                        }

                        let new_lower = new_search.to_lowercase();
                        if !new_lower.is_empty() {
                            // Insert a raw-ref item at index 0 so the user
                            // can always select exactly what they typed
                            // (e.g. HEAD~4, abc123~1).
                            items.insert(0, RefPickerItem {
                                value: new_search.trim().to_string(),
                                label: new_search.trim().to_string(),
                                category: "[ref]".to_string(),
                            });

                            // Jump cursor to best match among real candidates
                            // (skip the raw ref at index 0)
                            if let Some(idx) = items.iter().skip(1).position(|i| {
                                i.label.to_lowercase().contains(&new_lower)
                                    || i.value.to_lowercase().contains(&new_lower)
                            }) {
                                *selected = idx + 1; // +1 to skip raw ref
                            } else {
                                // No match — stay on the raw ref option
                                *selected = 0;
                            }
                            let sdi = find_display_idx(items, *selected);
                            // Center the match in the viewport
                            *scroll_offset = sdi.saturating_sub(list_height / 2);
                        } else {
                            *selected = 0;
                            *scroll_offset = 0;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn show_interactive_rebase_picker(&mut self) {
        use crate::gui::popup::{RefPickerItem, make_help_search_textarea};

        let model = self.model.lock().unwrap();
        let mut items = Vec::new();

        // Add branches (skip current branch)
        for branch in &model.branches {
            if branch.head {
                continue;
            }
            items.push(RefPickerItem {
                value: branch.name.clone(),
                label: branch.name.clone(),
                category: "Branches".to_string(),
            });
        }

        // Add remote branches
        for remote in &model.remotes {
            for branch in &remote.branches {
                let full_name = format!("{}/{}", remote.name, branch.name);
                items.push(RefPickerItem {
                    value: full_name.clone(),
                    label: full_name,
                    category: "Remote Branches".to_string(),
                });
            }
        }

        // Add tags
        for tag in &model.tags {
            items.push(RefPickerItem {
                value: tag.name.clone(),
                label: tag.name.clone(),
                category: "Tags".to_string(),
            });
        }

        // Add commits (skip HEAD)
        for commit in model.commits.iter().skip(1) {
            items.push(RefPickerItem {
                value: commit.hash.clone(),
                label: format!("{} {}", commit.short_hash(), commit.name),
                category: "Commits".to_string(),
            });
        }

        drop(model);

        self.popup = PopupState::RefPicker {
            title: "Interactive rebase current branch onto".to_string(),
            items,
            selected: 0,
            search_textarea: make_help_search_textarea(),
            scroll_offset: 0,
            on_confirm: Box::new(|gui, ref_name| {
                controller::branches::enter_interactive_rebase_onto(gui, ref_name)
            }),
        };
    }

    fn show_help(&mut self) {
        let kb = &self.config.user_config.keybinding;
        let active = self.context_mgr.active();

        // Universal keybindings
        let universal = HelpSection {
            title: "Universal".into(),
            entries: vec![
                HelpEntry { key: kb.universal.quit.clone(), description: "Quit".into() },
                HelpEntry { key: kb.universal.quit_alt1.clone(), description: "Quit (alt)".into() },
                HelpEntry { key: kb.universal.return_key.clone(), description: "Return / Cancel".into() },
                HelpEntry { key: kb.universal.toggle_panel.clone(), description: "Next panel".into() },
                HelpEntry { key: kb.universal.prev_item.clone(), description: "Previous item".into() },
                HelpEntry { key: kb.universal.next_item.clone(), description: "Next item".into() },
                HelpEntry { key: kb.universal.prev_page.clone(), description: "Page up".into() },
                HelpEntry { key: kb.universal.next_page.clone(), description: "Page down".into() },
                HelpEntry { key: kb.universal.goto_top.clone(), description: "Go to top".into() },
                HelpEntry { key: kb.universal.goto_bottom.clone(), description: "Go to bottom".into() },
                HelpEntry { key: kb.universal.prev_block.clone(), description: "Previous panel".into() },
                HelpEntry { key: kb.universal.next_block.clone(), description: "Next panel".into() },
                HelpEntry { key: kb.universal.start_search.clone(), description: "Search".into() },
                HelpEntry { key: kb.universal.next_match.clone(), description: "Next search match".into() },
                HelpEntry { key: kb.universal.prev_match.clone(), description: "Previous search match".into() },
                HelpEntry { key: kb.universal.scroll_up_main_alt1.clone(), description: "Scroll diff up".into() },
                HelpEntry { key: kb.universal.scroll_down_main_alt1.clone(), description: "Scroll diff down".into() },
                HelpEntry { key: kb.universal.scroll_left.clone(), description: "Scroll left".into() },
                HelpEntry { key: kb.universal.scroll_right.clone(), description: "Scroll right".into() },
                HelpEntry { key: kb.universal.edit.clone(), description: "Edit file".into() },
                HelpEntry { key: kb.universal.open_file.clone(), description: "Open file".into() },
                HelpEntry { key: kb.universal.undo.clone(), description: "Undo".into() },
                HelpEntry { key: kb.universal.redo.clone(), description: "Redo".into() },
                HelpEntry { key: kb.universal.refresh.clone(), description: "Refresh".into() },
                HelpEntry { key: kb.universal.push_files.clone(), description: "Push".into() },
                HelpEntry { key: kb.universal.pull_files.clone(), description: "Pull".into() },
                HelpEntry { key: kb.universal.next_screen_mode.clone(), description: "Enlarge panel".into() },
                HelpEntry { key: kb.universal.prev_screen_mode.clone(), description: "Shrink panel".into() },
                HelpEntry { key: kb.universal.create_rebase_options_menu.clone(), description: "Rebase options".into() },
                HelpEntry { key: kb.universal.create_patch_options_menu.clone(), description: "Patch options".into() },
                HelpEntry { key: "{/}".into(), description: "Previous/next hunk".into() },
                HelpEntry { key: "[/]".into(), description: "Toggle old/new only view".into() },
                HelpEntry { key: ";".into(), description: "Toggle command log".into() },
                HelpEntry { key: "W".into(), description: "Compare / Diff mode".into() },
                HelpEntry { key: "I".into(), description: "Interactive rebase onto...".into() },
                HelpEntry { key: "1-5".into(), description: "Jump to panel".into() },
                HelpEntry { key: "?".into(), description: "Show this help".into() },
            ],
        };

        // Context-specific keybindings
        let context_section = match active {
            ContextId::Files => HelpSection {
                title: "Files".into(),
                entries: vec![
                    HelpEntry { key: "<enter>".into(), description: "Toggle dir / Focus diff".into() },
                    HelpEntry { key: "<space>".into(), description: "Stage / Unstage".into() },
                    HelpEntry { key: kb.files.commit_changes.clone(), description: "Commit".into() },
                    HelpEntry { key: kb.files.amend_last_commit.clone(), description: "Amend last commit".into() },
                    HelpEntry { key: kb.files.commit_changes_with_editor.clone(), description: "Commit with editor".into() },
                    HelpEntry { key: kb.files.toggle_staged_all.clone(), description: "Toggle stage all".into() },
                    HelpEntry { key: kb.files.stash_all_changes.clone(), description: "Stash changes".into() },
                    HelpEntry { key: kb.files.view_stash_options.clone(), description: "Stash options".into() },
                    HelpEntry { key: kb.files.toggle_tree_view.clone(), description: "Toggle tree view".into() },
                    HelpEntry { key: kb.files.fetch.clone(), description: "Fetch".into() },
                    HelpEntry { key: kb.files.ignore_file.clone(), description: "Ignore file".into() },
                    HelpEntry { key: "d".into(), description: "Discard changes".into() },
                    HelpEntry { key: kb.universal.edit.clone(), description: "Open in editor".into() },
                    HelpEntry { key: kb.universal.open_file.clone(), description: "Open in default program".into() },
                ],
            },
            ContextId::Worktrees => HelpSection {
                title: "Worktrees".into(),
                entries: vec![
                    HelpEntry { key: "<space>".into(), description: "Switch to worktree".into() },
                    HelpEntry { key: "n".into(), description: "Create worktree".into() },
                    HelpEntry { key: "d".into(), description: "Remove worktree".into() },
                ],
            },
            ContextId::Submodules => HelpSection {
                title: "Submodules".into(),
                entries: vec![
                    HelpEntry { key: "<space>".into(), description: "Update submodule".into() },
                    HelpEntry { key: "a".into(), description: "Add submodule".into() },
                    HelpEntry { key: "d".into(), description: "Remove submodule".into() },
                    HelpEntry { key: "e".into(), description: "Enter submodule".into() },
                    HelpEntry { key: "u".into(), description: "Update all submodules".into() },
                    HelpEntry { key: "i".into(), description: "Init submodules".into() },
                ],
            },
            ContextId::Branches | ContextId::BranchCommits | ContextId::BranchCommitFiles => HelpSection {
                title: "Branches".into(),
                entries: vec![
                    HelpEntry { key: "<enter>".into(), description: "View branch commits".into() },
                    HelpEntry { key: "<space>".into(), description: "Checkout branch".into() },
                    HelpEntry { key: "c".into(), description: "Checkout ref".into() },
                    HelpEntry { key: "-".into(), description: "Checkout previous branch".into() },
                    HelpEntry { key: "n".into(), description: "New branch".into() },
                    HelpEntry { key: "d".into(), description: "Delete branch".into() },
                    HelpEntry { key: kb.branches.merge_into_current_branch.clone(), description: "Merge into current".into() },
                    HelpEntry { key: kb.branches.rebase_branch.clone(), description: "Rebase".into() },
                    HelpEntry { key: kb.branches.rename_branch.clone(), description: "Rename branch".into() },
                    HelpEntry { key: kb.branches.fast_forward.clone(), description: "Fast-forward".into() },
                    HelpEntry { key: kb.branches.set_upstream.clone(), description: "Set upstream".into() },
                    HelpEntry { key: "y".into(), description: "Copy to clipboard menu".into() },
                    HelpEntry { key: kb.branches.create_pull_request.clone(), description: "Open in browser menu".into() },
                ],
            },
            ContextId::Commits | ContextId::CommitFiles => HelpSection {
                title: "Commits".into(),
                entries: vec![
                    HelpEntry { key: "<enter>".into(), description: "View commit files".into() },
                    HelpEntry { key: kb.commits.squash_down.clone(), description: "Squash down".into() },
                    HelpEntry { key: kb.commits.rename_commit.clone(), description: "Reword commit".into() },
                    HelpEntry { key: kb.commits.view_reset_options.clone(), description: "Reset options".into() },
                    HelpEntry { key: kb.commits.mark_commit_as_fixup.clone(), description: "Fixup commit".into() },
                    HelpEntry { key: kb.commits.create_fixup_commit.clone(), description: "Create fixup commit".into() },
                    HelpEntry { key: kb.commits.squash_above_commits.clone(), description: "Squash above commits".into() },
                    HelpEntry { key: kb.commits.move_up_commit.clone(), description: "Move commit up".into() },
                    HelpEntry { key: kb.commits.move_down_commit.clone(), description: "Move commit down".into() },
                    HelpEntry { key: kb.commits.amend_to_commit.clone(), description: "Amend to commit".into() },
                    HelpEntry { key: kb.commits.pick_commit.clone(), description: "Pick / Drop commit".into() },
                    HelpEntry { key: kb.commits.revert_commit.clone(), description: "Revert commit".into() },
                    HelpEntry { key: kb.commits.cherry_pick_copy.clone(), description: "Cherry-pick copy".into() },
                    HelpEntry { key: kb.commits.paste_commits.clone(), description: "Paste commits".into() },
                    HelpEntry { key: kb.commits.tag_commit.clone(), description: "Tag commit".into() },
                    HelpEntry { key: kb.commits.checkout_commit.clone(), description: "Checkout commit".into() },
                    HelpEntry { key: kb.commits.view_bisect_options.clone(), description: "Bisect options".into() },
                    HelpEntry { key: "o".into(), description: "Open in browser".into() },
                    HelpEntry { key: "y".into(), description: "Copy to clipboard menu".into() },
                    HelpEntry { key: kb.commits.interactive_rebase.clone(), description: "Interactive rebase".into() },
                ],
            },
            ContextId::Reflog => HelpSection {
                title: "Reflog".into(),
                entries: vec![
                    HelpEntry { key: "<enter>".into(), description: "View commit files".into() },
                    HelpEntry { key: kb.commits.checkout_commit.clone(), description: "Checkout commit".into() },
                    HelpEntry { key: kb.commits.view_reset_options.clone(), description: "Reset options".into() },
                    HelpEntry { key: kb.commits.cherry_pick_copy.clone(), description: "Cherry-pick".into() },
                    HelpEntry { key: "y".into(), description: "Copy to clipboard menu".into() },
                ],
            },
            ContextId::Stash | ContextId::StashFiles => HelpSection {
                title: "Stash".into(),
                entries: vec![
                    HelpEntry { key: "<enter>".into(), description: "View stash files".into() },
                    HelpEntry { key: "<space>".into(), description: "Apply stash".into() },
                    HelpEntry { key: kb.stash.pop_stash.clone(), description: "Pop stash".into() },
                    HelpEntry { key: kb.stash.rename_stash.clone(), description: "Rename stash".into() },
                    HelpEntry { key: "d".into(), description: "Drop stash".into() },
                ],
            },
            ContextId::Remotes => HelpSection {
                title: "Remotes".into(),
                entries: vec![
                    HelpEntry { key: "<enter>".into(), description: "View remote branches".into() },
                    HelpEntry { key: "f".into(), description: "Fetch from remote".into() },
                    HelpEntry { key: "n".into(), description: "Add new remote".into() },
                    HelpEntry { key: "d".into(), description: "Delete remote".into() },
                    HelpEntry { key: kb.universal.push_files.clone(), description: "Push".into() },
                    HelpEntry { key: kb.universal.pull_files.clone(), description: "Pull".into() },
                ],
            },
            ContextId::RemoteBranches => HelpSection {
                title: "Remote Branches".into(),
                entries: vec![
                    HelpEntry { key: "<space>".into(), description: "Checkout as local branch".into() },
                    HelpEntry { key: kb.branches.merge_into_current_branch.clone(), description: "Merge into current".into() },
                    HelpEntry { key: kb.branches.rebase_branch.clone(), description: "Rebase".into() },
                    HelpEntry { key: "d".into(), description: "Delete remote branch".into() },
                    HelpEntry { key: "<esc>".into(), description: "Back to remotes".into() },
                ],
            },
            ContextId::Tags => HelpSection {
                title: "Tags".into(),
                entries: vec![
                    HelpEntry { key: "<enter>".into(), description: "View tag commits".into() },
                    HelpEntry { key: "n".into(), description: "Create tag".into() },
                    HelpEntry { key: "d".into(), description: "Delete tag".into() },
                    HelpEntry { key: "P".into(), description: "Push tag".into() },
                    HelpEntry { key: "g".into(), description: "Reset options".into() },
                ],
            },
            ContextId::Status => HelpSection {
                title: "Status".into(),
                entries: vec![
                    HelpEntry { key: "<enter>".into(), description: "Recent repos".into() },
                    HelpEntry { key: kb.status.all_branches_log_graph.clone(), description: "All branches log".into() },
                ],
            },
            _ => HelpSection {
                title: "Navigation".into(),
                entries: vec![
                    HelpEntry { key: "<enter>".into(), description: "Select / Open".into() },
                    HelpEntry { key: "<space>".into(), description: "Toggle / Confirm".into() },
                ],
            },
        };

        let sections = vec![context_section, universal];

        self.popup = PopupState::Help {
            sections,
            selected: 0,
            search_textarea: popup::make_help_search_textarea(),
            scroll_offset: 0,
        };
    }

    fn show_diff_help(&mut self) {
        let diff_section = HelpSection {
            title: "Diff Viewer".into(),
            entries: vec![
                HelpEntry { key: "j/k".into(), description: "Scroll up / down".into() },
                HelpEntry { key: "h/l".into(), description: "Scroll left / right".into() },
                HelpEntry { key: "{/}".into(), description: "Previous / next hunk".into() },
                HelpEntry { key: "[".into(), description: "Toggle old-only view".into() },
                HelpEntry { key: "]".into(), description: "Toggle new-only view".into() },
                HelpEntry { key: "z".into(), description: "Toggle line wrap".into() },
                HelpEntry { key: "g/G".into(), description: "Go to top / bottom".into() },
                HelpEntry { key: "PgUp/PgDn".into(), description: "Page up / down".into() },
                HelpEntry { key: "/".into(), description: "Search in diff".into() },
                HelpEntry { key: "n/N".into(), description: "Next / previous search match".into() },
                HelpEntry { key: "y".into(), description: "Copy selected text".into() },
                HelpEntry { key: "+/_".into(), description: "Enlarge / shrink panel".into() },
                HelpEntry { key: ";".into(), description: "Toggle command log".into() },
                HelpEntry { key: "1-5".into(), description: "Jump to sidebar panel".into() },
                HelpEntry { key: "esc".into(), description: "Return to sidebar".into() },
                HelpEntry { key: "?".into(), description: "Show this help".into() },
            ],
        };

        self.popup = PopupState::Help {
            sections: vec![diff_section],
            selected: 0,
            search_textarea: popup::make_help_search_textarea(),
            scroll_offset: 0,
        };
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
                        is_commit: false, confirm_focused: false,
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
        if let PopupState::None = self.popup {
            // Search uses a textarea — forward keys to it
            if let Some(ref mut ta) = self.search_textarea {
                match key.code {
                    KeyCode::Esc => {
                        self.search_active = false;
                        self.search_query.clear();
                        self.search_matches.clear();
                        self.search_match_idx = 0;
                        self.search_textarea = None;
                    }
                    KeyCode::Enter => {
                        self.search_active = false;
                        // Jump to first match
                        if !self.search_matches.is_empty() {
                            self.search_match_idx = 0;
                            let idx = self.search_matches[0];
                            self.context_mgr.set_selection(idx);
                        }
                        self.search_textarea = None;
                    }
                    _ => {
                        ta.input(key);
                        // Sync textarea content back to search_query
                        self.search_query = ta.lines().join("");
                        self.update_search_matches();
                    }
                }
            }
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
                if self.show_file_tree {
                    // When file tree is active, indices are into file_tree_nodes
                    for (i, node) in self.file_tree_nodes.iter().enumerate() {
                        if node.path.to_lowercase().contains(&query)
                            || node.name.to_lowercase().contains(&query)
                        {
                            self.search_matches.push(i);
                        }
                    }
                } else {
                    for (i, file) in model.files.iter().enumerate() {
                        if file.name.to_lowercase().contains(&query) {
                            self.search_matches.push(i);
                        }
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
            ContextId::Reflog => {
                for (i, commit) in model.reflog_commits.iter().enumerate() {
                    if commit.name.to_lowercase().contains(&query)
                        || commit.hash.starts_with(&self.search_query)
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
            ContextId::RemoteBranches => {
                for (i, rb) in model.sub_remote_branches.iter().enumerate() {
                    if rb.name.to_lowercase().contains(&query) {
                        self.search_matches.push(i);
                    }
                }
            }
            ContextId::Worktrees => {
                for (i, wt) in model.worktrees.iter().enumerate() {
                    if wt.branch.to_lowercase().contains(&query)
                        || wt.path.to_lowercase().contains(&query)
                    {
                        self.search_matches.push(i);
                    }
                }
            }
            ContextId::Submodules => {
                for (i, sub) in model.submodules.iter().enumerate() {
                    if sub.name.to_lowercase().contains(&query)
                        || sub.path.to_lowercase().contains(&query)
                    {
                        self.search_matches.push(i);
                    }
                }
            }
            ContextId::CommitFiles | ContextId::StashFiles | ContextId::BranchCommitFiles => {
                if self.show_commit_file_tree {
                    for (i, node) in self.commit_file_tree_nodes.iter().enumerate() {
                        if node.path.to_lowercase().contains(&query)
                            || node.name.to_lowercase().contains(&query)
                        {
                            self.search_matches.push(i);
                        }
                    }
                } else {
                    for (i, file) in model.commit_files.iter().enumerate() {
                        if file.name.to_lowercase().contains(&query) {
                            self.search_matches.push(i);
                        }
                    }
                }
            }
            ContextId::BranchCommits => {
                for (i, commit) in model.sub_commits.iter().enumerate() {
                    if commit.name.to_lowercase().contains(&query)
                        || commit.hash.to_lowercase().contains(&query)
                        || commit.author_name.to_lowercase().contains(&query)
                    {
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
        use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};

        if !self.config.user_config.gui.mouse_events {
            return;
        }

        // Rebase mode: basic scroll support
        if self.rebase_mode.active {
            match mouse.kind {
                MouseEventKind::ScrollDown => {
                    let len = self.rebase_mode.entries.len();
                    if self.rebase_mode.selected + 1 < len {
                        self.rebase_mode.selected += 1;
                    }
                }
                MouseEventKind::ScrollUp => {
                    if self.rebase_mode.selected > 0 {
                        self.rebase_mode.selected -= 1;
                    }
                }
                _ => {}
            }
            return;
        }

        // Diff mode has its own mouse handling
        if self.diff_mode.active {
            self.handle_diff_mode_mouse(mouse);
            return;
        }

        // Help popup intercepts mouse scroll
        if let PopupState::Help { sections, scroll_offset, search_textarea, .. } = &mut self.popup {
            // Compute total display rows so we can clamp scroll
            let search_lower = search_textarea.lines().join("").to_lowercase();
            let has_search = !search_lower.is_empty();
            let total_rows: usize = sections.iter().map(|s| {
                let visible = if has_search {
                    s.entries.iter().filter(|e| {
                        e.key.to_lowercase().contains(&search_lower)
                            || e.description.to_lowercase().contains(&search_lower)
                    }).count()
                } else {
                    s.entries.len()
                };
                if visible > 0 { visible + 1 } else { 0 } // +1 for header
            }).sum();

            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    *scroll_offset = scroll_offset.saturating_sub(3);
                }
                MouseEventKind::ScrollDown => {
                    *scroll_offset = (*scroll_offset + 3).min(total_rows.saturating_sub(1));
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    // Clicking outside could close, but for now just ignore clicks
                }
                _ => {}
            }
            return;
        }

        // RefPicker popup intercepts mouse scroll — move selection like ↑/↓
        if let PopupState::RefPicker { items, selected, scroll_offset, search_textarea, .. } = &mut self.popup {
            let total = items.len();
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    *selected = selected.saturating_sub(3);
                    if *selected < *scroll_offset {
                        *scroll_offset = *selected;
                    }
                }
                MouseEventKind::ScrollDown => {
                    *selected = (*selected + 3).min(total.saturating_sub(1));
                    // Compute display idx to keep scroll in sync
                    let mut di = 0usize;
                    let mut ei = 0usize;
                    let mut last_cat = String::new();
                    for item in items.iter() {
                        if item.category != last_cat {
                            di += 1;
                            last_cat = item.category.clone();
                        }
                        if ei == *selected { break; }
                        ei += 1;
                        di += 1;
                    }
                    let _ = search_textarea;
                    let h = self.layout.height as usize;
                    let popup_h = (h * 60 / 100).max(10).min(h.saturating_sub(4));
                    let list_height = popup_h.saturating_sub(2).saturating_sub(3);
                    if di >= *scroll_offset + list_height {
                        *scroll_offset = di.saturating_sub(list_height - 1);
                    }
                }
                _ => {}
            }
            return;
        }

        let main_panel = self.compute_main_panel_rect();
        let pl = DiffPanelLayout::compute(main_panel, &self.diff_view);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let in_main = main_panel.x <= mouse.column
                    && mouse.column < main_panel.x + main_panel.width
                    && main_panel.y <= mouse.row
                    && mouse.row < main_panel.y + main_panel.height;

                // In Full screen mode, the main_panel covers everything.
                // If the sidebar is focused (not diff_focused), clicks should
                // go to the sidebar handler, not start a diff selection.
                let full_sidebar = self.screen_mode == ScreenMode::Full && !self.diff_focused;

                if in_main && !self.diff_view.is_empty() && !full_sidebar {
                    if let Some(panel) = pl.panel_at_x(mouse.column) {
                        self.diff_view.selection = Some(TextSelection {
                            panel,
                            start_col: mouse.column,
                            start_row: mouse.row,
                            end_col: mouse.column,
                            end_row: mouse.row,
                            dragging: true,
                            is_click: false,
                            text: String::new(),
                            edit_line_number: None,
                            edit_column_number: None,
                        });
                    } else {
                        self.diff_view.selection = None;
                    }
                    self.diff_focused = true;
                } else {
                    // Click outside diff — clear selection and handle normally
                    self.diff_view.selection = None;
                    self.handle_mouse_click(mouse.column, mouse.row);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(ref mut sel) = self.diff_view.selection {
                    if sel.dragging {
                        let (cmin, cmax) = pl.content_range(sel.panel);
                        // Allow dragging into gutter area of same panel (5 cols before content)
                        let col_min = cmin.saturating_sub(5);
                        sel.end_col = mouse.column.max(col_min).min(cmax.saturating_sub(1));
                        sel.end_row = mouse.row
                            .max(pl.inner_y)
                            .min(pl.inner_end_y.saturating_sub(1));
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // Finalize the selection
                if let Some(ref mut sel) = self.diff_view.selection {
                    sel.dragging = false;
                    // If start == end (just a click, no drag)
                    if sel.start_col == sel.end_col && sel.start_row == sel.end_row {
                        if self.diff_view.file_exists_on_disk {
                            // Keep as click-state to show the edit tooltip
                            sel.is_click = true;
                        } else {
                            self.diff_view.selection = None;
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                self.diff_view.selection = None;
                let in_diff = self.diff_focused || self.is_in_main_panel(mouse.column, mouse.row);
                if mouse.modifiers.contains(KeyModifiers::SHIFT) && in_diff {
                    self.diff_view.scroll_left(4);
                } else if in_diff {
                    self.diff_view.scroll_up(3);
                } else {
                    let model = self.model.lock().unwrap();
                    self.context_mgr.move_selection(-3, &model);
                }
            }
            MouseEventKind::ScrollDown => {
                self.diff_view.selection = None;
                let in_diff = self.diff_focused || self.is_in_main_panel(mouse.column, mouse.row);
                if mouse.modifiers.contains(KeyModifiers::SHIFT) && in_diff {
                    self.diff_view.scroll_right(4);
                } else if in_diff {
                    self.diff_view.scroll_down(3);
                } else {
                    let model = self.model.lock().unwrap();
                    self.context_mgr.move_selection(3, &model);
                }
            }
            MouseEventKind::ScrollLeft => {
                if self.diff_focused || self.is_in_main_panel(mouse.column, mouse.row) {
                    self.diff_view.scroll_left(4);
                }
            }
            MouseEventKind::ScrollRight => {
                if self.diff_focused || self.is_in_main_panel(mouse.column, mouse.row) {
                    self.diff_view.scroll_right(4);
                }
            }
            _ => {}
        }
    }

    fn handle_diff_mode_mouse(&mut self, mouse: MouseEvent) {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
        use ratatui::layout::{Constraint, Direction, Layout, Rect};
        use self::modes::diff_mode::DiffModeFocus;

        // Help popup intercepts mouse scroll
        if let PopupState::Help { sections, scroll_offset, search_textarea, .. } = &mut self.popup {
            let search_lower = search_textarea.lines().join("").to_lowercase();
            let has_search = !search_lower.is_empty();
            let total_rows: usize = sections.iter().map(|s| {
                let visible = if has_search {
                    s.entries.iter().filter(|e| {
                        e.key.to_lowercase().contains(&search_lower)
                            || e.description.to_lowercase().contains(&search_lower)
                    }).count()
                } else {
                    s.entries.len()
                };
                if visible > 0 { visible + 1 } else { 0 }
            }).sum();

            match mouse.kind {
                MouseEventKind::ScrollUp => { *scroll_offset = scroll_offset.saturating_sub(3); }
                MouseEventKind::ScrollDown => { *scroll_offset = (*scroll_offset + 3).min(total_rows.saturating_sub(1)); }
                _ => {}
            }
            return;
        }

        // RefPicker popup intercepts mouse scroll — move selection like ↑/↓
        if let PopupState::RefPicker { items, selected, scroll_offset, .. } = &mut self.popup {
            let total = items.len();
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    *selected = selected.saturating_sub(3);
                    if *selected < *scroll_offset {
                        *scroll_offset = *selected;
                    }
                }
                MouseEventKind::ScrollDown => {
                    *selected = (*selected + 3).min(total.saturating_sub(1));
                    let mut di = 0usize;
                    let mut ei = 0usize;
                    let mut last_cat = String::new();
                    for item in items.iter() {
                        if item.category != last_cat {
                            di += 1;
                            last_cat = item.category.clone();
                        }
                        if ei == *selected { break; }
                        ei += 1;
                        di += 1;
                    }
                    let h = self.layout.height as usize;
                    let popup_h = (h * 60 / 100).max(10).min(h.saturating_sub(4));
                    let list_height = popup_h.saturating_sub(2).saturating_sub(3);
                    if di >= *scroll_offset + list_height {
                        *scroll_offset = di.saturating_sub(list_height - 1);
                    }
                }
                _ => {}
            }
            return;
        }

        let area = Rect::new(0, 0, self.layout.width, self.layout.height);

        // Replicate the diff mode layout to determine regions
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
                Constraint::Min(1),
            ])
            .split(content[0]);

        let selector_a_rect = sidebar[0];
        let selector_b_rect = sidebar[1];
        let files_rect = sidebar[2];
        let diff_rect = content[1];

        let col = mouse.column;
        let row = mouse.row;

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if click is in the diff panel — start text selection
                if rect_contains(diff_rect, col, row) && !self.diff_view.is_empty() {
                    let pl = DiffPanelLayout::compute(diff_rect, &self.diff_view);
                    if let Some(panel) = pl.panel_at_x(col) {
                        self.diff_view.selection = Some(TextSelection {
                            panel,
                            start_col: col,
                            start_row: row,
                            end_col: col,
                            end_row: row,
                            dragging: true,
                            is_click: false,
                            text: String::new(),
                            edit_line_number: None,
                            edit_column_number: None,
                        });
                    } else {
                        self.diff_view.selection = None;
                    }
                    self.diff_mode.focus = DiffModeFocus::DiffExploration;
                } else {
                    self.diff_view.selection = None;

                    // Click on panels to switch focus
                    if rect_contains(selector_a_rect, col, row) {
                        self.diff_mode.focus = DiffModeFocus::SelectorA;
                    } else if rect_contains(selector_b_rect, col, row) {
                        self.diff_mode.focus = DiffModeFocus::SelectorB;
                    } else if rect_contains(files_rect, col, row) {
                        self.diff_mode.focus = DiffModeFocus::CommitFiles;
                        // Click to select a file
                        let inner_y = row.saturating_sub(files_rect.y + 1);
                        let len = self.diff_mode.visible_files_len();
                        let visible_height = files_rect.height.saturating_sub(2) as usize;
                        let scroll_offset = if self.diff_mode.diff_files_selected >= visible_height {
                            self.diff_mode.diff_files_selected - visible_height + 1
                        } else {
                            0
                        };
                        let clicked_idx = scroll_offset + inner_y as usize;
                        if clicked_idx < len {
                            self.diff_mode.diff_files_selected = clicked_idx;
                            self.needs_diff_refresh = true;
                        }
                    } else if rect_contains(diff_rect, col, row) {
                        self.diff_mode.focus = DiffModeFocus::DiffExploration;
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let pl = DiffPanelLayout::compute(diff_rect, &self.diff_view);
                if let Some(ref mut sel) = self.diff_view.selection {
                    if sel.dragging {
                        let (cmin, cmax) = pl.content_range(sel.panel);
                        let col_min = cmin.saturating_sub(5);
                        sel.end_col = col.max(col_min).min(cmax.saturating_sub(1));
                        sel.end_row = row
                            .max(pl.inner_y)
                            .min(pl.inner_end_y.saturating_sub(1));
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(ref mut sel) = self.diff_view.selection {
                    sel.dragging = false;
                    if sel.start_col == sel.end_col && sel.start_row == sel.end_row {
                        if self.diff_view.file_exists_on_disk {
                            sel.is_click = true;
                        } else {
                            self.diff_view.selection = None;
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if rect_contains(diff_rect, col, row) {
                    self.diff_view.selection = None;
                    if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                        self.diff_view.scroll_left(4);
                    } else {
                        self.diff_view.scroll_up(3);
                    }
                } else if rect_contains(files_rect, col, row) {
                    let len = self.diff_mode.visible_files_len();
                    if len > 0 {
                        self.diff_mode.diff_files_selected = self.diff_mode.diff_files_selected.saturating_sub(3);
                        self.needs_diff_refresh = true;
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                if rect_contains(diff_rect, col, row) {
                    self.diff_view.selection = None;
                    if mouse.modifiers.contains(KeyModifiers::SHIFT) {
                        self.diff_view.scroll_right(4);
                    } else {
                        self.diff_view.scroll_down(3);
                    }
                } else if rect_contains(files_rect, col, row) {
                    let len = self.diff_mode.visible_files_len();
                    if len > 0 {
                        self.diff_mode.diff_files_selected = (self.diff_mode.diff_files_selected + 3).min(len - 1);
                        self.needs_diff_refresh = true;
                    }
                }
            }
            MouseEventKind::ScrollLeft => {
                if rect_contains(diff_rect, col, row) {
                    self.diff_view.scroll_left(4);
                }
            }
            MouseEventKind::ScrollRight => {
                if rect_contains(diff_rect, col, row) {
                    self.diff_view.scroll_right(4);
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

        // In Full screen mode with sidebar focused, the sidebar is rendered
        // in main_panel — treat clicks there as sidebar item selection.
        if self.screen_mode == ScreenMode::Full && !self.diff_focused {
            let panel_rect = fl.main_panel;
            if panel_rect.x <= col
                && col < panel_rect.x + panel_rect.width
                && panel_rect.y <= row
                && row < panel_rect.y + panel_rect.height
            {
                let inner_y = row.saturating_sub(panel_rect.y + 1);
                let selected = self.context_mgr.selected_active();
                let model = self.model.lock().unwrap();
                let list_len = self.context_mgr.list_len(&model);
                drop(model);
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

    fn is_in_main_panel(&self, col: u16, row: u16) -> bool {
        let mp = self.compute_main_panel_rect();
        col >= mp.x && col < mp.x + mp.width && row >= mp.y && row < mp.y + mp.height
    }

    /// Compute the exact main panel Rect using the real layout engine.
    fn compute_main_panel_rect(&self) -> ratatui::layout::Rect {
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
        fl.main_panel
    }

    fn refresh(&mut self) -> Result<()> {
        let new_model = self.git.load_model()?;
        let mut model = self.model.lock().unwrap();
        *model = new_model;

        // If branch filters are active, reload commits for those branches only.
        if !self.commit_branch_filter.is_empty() {
            if let Ok(filtered) = self.git.load_commits_for_branches(&self.commit_branch_filter, 300) {
                model.commits = filtered;
            }
        }

        // Rebuild file tree inline to avoid borrow issues
        if self.show_file_tree {
            self.file_tree_nodes = build_file_tree(&model.files, &self.collapsed_dirs);
            self.context_mgr.files_list_len_override = Some(self.file_tree_nodes.len());
        } else {
            self.file_tree_nodes.clear();
            self.context_mgr.files_list_len_override = None;
        }

        // If we're viewing branch commits, re-load them (refresh wipes the model)
        if (self.context_mgr.active() == ContextId::BranchCommits
            || self.context_mgr.active() == ContextId::BranchCommitFiles)
            && !self.branch_commits_name.is_empty()
        {
            if let Ok(commits) = self.git.load_commits_for_branch(&self.branch_commits_name, 300) {
                model.sub_commits = commits;
            }
        }

        // If we're viewing remote branches, re-load them (refresh wipes the model)
        if self.context_mgr.active() == ContextId::RemoteBranches
            && !self.remote_branches_name.is_empty()
        {
            if let Some(remote) = model.remotes.iter().find(|r| r.name == self.remote_branches_name) {
                model.sub_remote_branches = remote.branches.clone();
            }
        }

        // If we're viewing commit/stash files, re-load them (refresh wipes the model)
        if (self.context_mgr.active() == ContextId::CommitFiles
            || self.context_mgr.active() == ContextId::StashFiles
            || self.context_mgr.active() == ContextId::BranchCommitFiles)
            && !self.commit_files_hash.is_empty()
        {
            if let Ok(cf) = self.git.commit_files(&self.commit_files_hash) {
                model.commit_files = cf;
            }
            if self.show_commit_file_tree {
                self.commit_file_tree_nodes = crate::model::file_tree::build_commit_file_tree(
                    &model.commit_files,
                    &self.commit_files_collapsed_dirs,
                );
                self.context_mgr.commit_files_list_len_override =
                    Some(self.commit_file_tree_nodes.len());
            }
        }

        let is_rebasing = model.is_rebasing;
        drop(model);

        // Auto-enter rebase InProgress mode when a rebase is detected on disk
        // and we're not already in rebase mode.
        if is_rebasing && !self.rebase_mode.active {
            if let Some(mut progress) = self.git.parse_rebase_progress() {
                // Hydrate entries with author/timestamp from git log
                self.git.hydrate_todo_entries(&mut progress.done_entries);
                self.git.hydrate_todo_entries(&mut progress.todo_entries);
                self.rebase_mode.enter_in_progress(&progress);
            }
        }
        // If rebase mode was active but the rebase completed, exit and show success.
        if !is_rebasing && self.rebase_mode.active {
            use crate::gui::modes::rebase_mode::RebasePhase;
            if self.rebase_mode.phase == RebasePhase::InProgress {
                let branch = self.rebase_mode.branch_name.clone();
                let count = self.rebase_mode.total_count;
                self.rebase_mode.exit();
                self.popup = crate::gui::popup::PopupState::Message {
                    title: "Rebase complete".to_string(),
                    message: format!(
                        "Successfully rebased '{}' ({} commit{}).",
                        branch,
                        count,
                        if count == 1 { "" } else { "s" },
                    ),
                    kind: crate::gui::popup::MessageKind::Info,
                };
            }
        }

        Ok(())
    }

    /// Lightweight refresh that only reloads files and diff stats.
    /// Use this after staging/unstaging operations where branches, commits,
    /// tags, etc. haven't changed.
    fn refresh_files_only(&mut self) -> Result<()> {
        let (files, shortstat) = std::thread::scope(|s| {
            let h_files = s.spawn(|| self.git.load_files());
            let h_stat = s.spawn(|| self.git.diff_shortstat());
            (h_files.join().unwrap(), h_stat.join().unwrap())
        });

        let mut model = self.model.lock().unwrap();
        if let Ok(f) = files {
            model.files = f;
        }
        if let Ok((added, deleted)) = shortstat {
            model.total_additions = added;
            model.total_deletions = deleted;
        }

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

    fn commit_history_path(config: &AppConfig) -> std::path::PathBuf {
        config.config_dir.join("commit_message_history")
    }

    fn persist_command_log_visibility(&self) {
        if let Ok(mut state) = AppState::load(&self.config.state_path) {
            state.show_command_log = Some(self.show_command_log);
            let _ = state.save(&self.config.state_path);
        }
    }

    fn load_commit_history(config: &AppConfig) -> Vec<String> {
        let path = Self::commit_history_path(config);
        match std::fs::read_to_string(&path) {
            Ok(contents) => contents
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn save_commit_history(&self) {
        let path = Self::commit_history_path(&self.config);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let contents = self.commit_message_history.join("\0");
        let _ = std::fs::write(&path, contents);
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

    /// Exit sub-contexts (like CommitFiles) back to their parent context
    /// before navigating away to another window.
    fn exit_sub_contexts(&mut self) {
        if self.context_mgr.active() == ContextId::CommitFiles {
            self.context_mgr.set_active(ContextId::Commits);
        }
        if self.context_mgr.active() == ContextId::StashFiles {
            self.context_mgr.set_active(ContextId::Stash);
        }
        if self.context_mgr.active() == ContextId::BranchCommitFiles {
            self.context_mgr.set_active(ContextId::BranchCommits);
        }
        if self.context_mgr.active() == ContextId::BranchCommits {
            self.context_mgr.set_active(ContextId::Branches);
        }
        if self.context_mgr.active() == ContextId::RemoteBranches {
            self.context_mgr.set_active(ContextId::Remotes);
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

fn rect_contains(r: ratatui::layout::Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

fn setup_terminal() -> Result<(Term, bool)> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
        crossterm::event::EnableFocusChange,
        cursor::Hide
    )?;
    let keyboard_enhanced =
        crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if keyboard_enhanced {
        execute!(
            stdout,
            crossterm::event::PushKeyboardEnhancementFlags(
                crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | crossterm::event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )?;
    }
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok((terminal, keyboard_enhanced))
}

fn restore_terminal(terminal: &mut Term, keyboard_enhanced: bool) -> Result<()> {
    terminal::disable_raw_mode()?;
    if keyboard_enhanced {
        execute!(terminal.backend_mut(), crossterm::event::PopKeyboardEnhancementFlags)?;
    }
    execute!(
        terminal.backend_mut(),
        crossterm::event::DisableFocusChange,
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
        cursor::Show
    )?;
    Ok(())
}
