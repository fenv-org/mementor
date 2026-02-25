use std::io::Stdout;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use mementor_lib::cache::DataCache;
use mementor_lib::git::branch::list_branches;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::prelude::*;
use ratatui::widgets::ListState;

use crate::views::{branch_popup, dashboard, status_bar};

pub enum View {
    CheckpointList,
}

pub struct App {
    pub view: View,
    pub cache: DataCache,
    pub list_state: ListState,
    pub selected_branch: String,
    pub running: bool,
    pub branch_popup_open: bool,
    pub branches: Vec<String>,
    pub branch_list_state: ListState,
}

impl App {
    pub fn new(cache: DataCache, branch: String) -> Self {
        let mut list_state = ListState::default();
        if !cache.checkpoints().is_empty() {
            list_state.select(Some(0));
        }
        Self {
            view: View::CheckpointList,
            cache,
            list_state,
            selected_branch: branch,
            running: true,
            branch_popup_open: false,
            branches: Vec::new(),
            branch_list_state: ListState::default(),
        }
    }

    pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
        terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(terminal)
    }

    pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        terminal::disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        Ok(())
    }

    pub async fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if event::poll(std::time::Duration::from_millis(100))?
                && let Event::Key(key) = event::read()?
            {
                self.handle_key(key).await;
            }

            if !self.running {
                break;
            }
        }
        Ok(())
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        match self.view {
            View::CheckpointList => dashboard::render(frame, chunks[0], self),
        }
        status_bar::render(frame, chunks[1], self);

        if self.branch_popup_open {
            branch_popup::render(frame, area, self);
        }
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl-c always quits.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }

        if self.branch_popup_open {
            branch_popup::handle_key(key, self).await;
            return;
        }

        match self.view {
            View::CheckpointList => self.handle_dashboard_key(key).await,
        }
    }

    async fn handle_dashboard_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('j') | KeyCode::Down => self.select_next(),
            KeyCode::Char('k') | KeyCode::Up => self.select_prev(),
            KeyCode::Char('b') => self.open_branch_popup().await,
            KeyCode::Char('r') => {
                let _ = self.cache.refresh().await;
                self.clamp_selection();
            }
            // Enter will switch to detail view in a future phase.
            _ => {}
        }
    }

    fn select_next(&mut self) {
        let len = self.cache.checkpoints().len();
        if len == 0 {
            return;
        }
        let i = self
            .list_state
            .selected()
            .map_or(0, |i| if i + 1 < len { i + 1 } else { i });
        self.list_state.select(Some(i));
    }

    fn select_prev(&mut self) {
        let len = self.cache.checkpoints().len();
        if len == 0 {
            return;
        }
        let i = self
            .list_state
            .selected()
            .map_or(0, |i| i.saturating_sub(1));
        self.list_state.select(Some(i));
    }

    pub(crate) fn clamp_selection(&mut self) {
        let len = self.cache.checkpoints().len();
        if len == 0 {
            self.list_state.select(None);
        } else if let Some(i) = self.list_state.selected()
            && i >= len
        {
            self.list_state.select(Some(len - 1));
        }
    }

    async fn open_branch_popup(&mut self) {
        self.branches = list_branches().await.unwrap_or_default();
        let selected_index = self
            .branches
            .iter()
            .position(|b| b == &self.selected_branch)
            .unwrap_or(0);
        self.branch_list_state.select(Some(selected_index));
        self.branch_popup_open = true;
    }
}
