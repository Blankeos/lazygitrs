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

use self::context::{ContextId, ContextManager};
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
    pub command_log: Vec<String>,
    pub should_quit: bool,
    pub needs_refresh: bool,
    pub search_query: String,
    pub screen_mode: ScreenMode,
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
            command_log: Vec::new(),
            should_quit: false,
            needs_refresh: false,
            search_query: String::new(),
            screen_mode: ScreenMode::Normal,
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
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
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

        // Tab to switch panels
        if matches_key(key, &keybindings.universal.toggle_panel) {
            self.context_mgr.next_context();
            return Ok(());
        }

        // Arrow keys to switch panels
        if matches_key(key, &keybindings.universal.prev_block)
            || matches_key(key, &keybindings.universal.prev_block_alt)
        {
            self.context_mgr.prev_context();
            return Ok(());
        }
        if matches_key(key, &keybindings.universal.next_block)
            || matches_key(key, &keybindings.universal.next_block_alt)
        {
            self.context_mgr.next_context();
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

        // Refresh
        if matches_key(key, &keybindings.universal.refresh) {
            self.needs_refresh = true;
            return Ok(());
        }

        // Screen mode toggle ([ and ] or + and _)
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
