pub mod context;
pub mod controller;
pub mod layout;
pub mod popup;
pub mod presentation;
pub mod views;

use std::io::{self, Stdout};
use std::sync::{Arc, Mutex};

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
use crate::pager::side_by_side::DiffViewState;

use self::context::{ContextId, ContextManager, SideWindow};
use self::layout::LayoutState;
use self::popup::PopupState;

pub type Term = Terminal<CrosstermBackend<Stdout>>;

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
    /// Track what we last loaded a diff for, to avoid reloading on every frame.
    last_diff_key: String,
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
            last_diff_key: String::new(),
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let mut terminal = setup_terminal()?;

        let result = self.main_loop(&mut terminal);

        restore_terminal(&mut terminal)?;
        result
    }

    fn main_loop(&mut self, terminal: &mut Term) -> Result<()> {
        loop {
            // Load diff if selection changed
            self.maybe_load_diff();

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
                );
            })?;

            // Handle events
            if event::poll(std::time::Duration::from_millis(50))? {
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

    /// Load diff content for the currently selected item if it changed.
    fn maybe_load_diff(&mut self) {
        let active = self.context_mgr.active();
        let selected = self.context_mgr.selected_active();
        let diff_key = format!("{:?}:{}", active, selected);

        if diff_key == self.last_diff_key && !self.needs_diff_refresh {
            return;
        }
        self.last_diff_key = diff_key;
        self.needs_diff_refresh = false;

        let model = self.model.lock().unwrap();
        match active {
            ContextId::Files => {
                if let Some(file) = model.files.get(selected) {
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
                            // Untracked files: show full content as all-insertions
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
                }
            }
            ContextId::Commits => {
                if let Some(commit) = model.commits.get(selected) {
                    let hash = commit.hash.clone();
                    drop(model);

                    if let Ok(diff) = self.git.diff_commit(&hash) {
                        let filename = format!("commit:{}", &hash[..7.min(hash.len())]);
                        self.diff_view.load_from_diff_output(&filename, &diff);
                    }
                }
            }
            ContextId::Stash => {
                if let Some(entry) = model.stash_entries.get(selected) {
                    let index = entry.index;
                    drop(model);

                    if let Ok(diff) = self.git.stash_diff(index) {
                        if diff.is_empty() {
                            self.diff_view = DiffViewState::new();
                        } else {
                            let filename = format!("stash@{{{}}}", index);
                            self.diff_view.load_from_diff_output(&filename, &diff);
                        }
                    }
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

        // Screen mode toggle
        if key.code == KeyCode::Enter {
            self.cycle_screen_mode();
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
        Ok(())
    }

    fn cycle_screen_mode(&mut self) {
        self.screen_mode = match self.screen_mode {
            ScreenMode::Normal => ScreenMode::Half,
            ScreenMode::Half => ScreenMode::Full,
            ScreenMode::Full => ScreenMode::Normal,
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
