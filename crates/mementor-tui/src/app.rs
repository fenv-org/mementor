use std::io::Stdout;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use mementor_lib::cache::DataCache;
use mementor_lib::git::branch::list_branches;
use mementor_lib::model::TranscriptEntry;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::prelude::*;
use ratatui::widgets::ListState;

use crate::views::{branch_popup, dashboard, detail, diff_view, git_log, status_bar, transcript};

/// The active view in the application.
pub enum View {
    CheckpointList,
    /// Checkpoint detail — carries the checkpoint index into `cache.checkpoints()`.
    CheckpointDetail(usize),
    /// Fullscreen transcript — checkpoint index + session index.
    Transcript {
        checkpoint_idx: usize,
        session_idx: usize,
    },
    /// Diff view — commit hash.
    DiffView(String),
    /// Git log.
    GitLog,
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

    // Per-view state.
    pub detail_state: detail::DetailState,
    pub transcript_state: transcript::TranscriptViewState,
    pub diff_state: diff_view::DiffViewState,
    pub git_log_state: git_log::GitLogState,

    /// Cached transcript entries for the currently viewed checkpoint session.
    pub loaded_transcript: Option<Vec<TranscriptEntry>>,
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
            detail_state: detail::DetailState::new(0),
            transcript_state: transcript::TranscriptViewState::new(),
            diff_state: diff_view::DiffViewState::new(),
            git_log_state: git_log::GitLogState::new(),
            loaded_transcript: None,
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

        match &self.view {
            View::CheckpointList => dashboard::render(frame, chunks[0], self),
            View::CheckpointDetail(idx) => {
                let idx = *idx;
                let checkpoints = self.cache.checkpoints();
                if let Some(cp) = checkpoints.get(idx) {
                    let cp = cp.clone();
                    let commits = self.cache.commits().to_vec();
                    let transcript_ref = self.loaded_transcript.as_deref();
                    detail::render(
                        frame,
                        chunks[0],
                        &mut self.detail_state,
                        &cp,
                        &commits,
                        transcript_ref,
                    );
                }
            }
            View::Transcript { .. } => {
                let entries = self.loaded_transcript.as_deref().unwrap_or(&[]);
                transcript::render(frame, chunks[0], &mut self.transcript_state, entries);
            }
            View::DiffView(hash) => {
                let header = hash.clone();
                let diffs = self
                    .cache
                    .cached_diffs(&header)
                    .map(<[mementor_lib::git::diff::FileDiff]>::to_vec)
                    .unwrap_or_default();
                diff_view::render(frame, chunks[0], &mut self.diff_state, &diffs, &header);
            }
            View::GitLog => {
                let commits = self.cache.commits().to_vec();
                git_log::render(frame, chunks[0], &mut self.git_log_state, &commits);
            }
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

        match &self.view {
            View::CheckpointList => self.handle_dashboard_key(key).await,
            View::CheckpointDetail(idx) => {
                let idx = *idx;
                self.handle_detail_key(key, idx).await;
            }
            View::Transcript {
                checkpoint_idx,
                session_idx,
            } => {
                let cp_idx = *checkpoint_idx;
                let s_idx = *session_idx;
                self.handle_transcript_key(key, cp_idx, s_idx);
            }
            View::DiffView(hash) => {
                let hash = hash.clone();
                self.handle_diff_key(key, &hash);
            }
            View::GitLog => self.handle_git_log_key(key).await,
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
            KeyCode::Char('g') => {
                self.git_log_state.reset(self.cache.commits().len());
                self.view = View::GitLog;
            }
            KeyCode::Enter => {
                if let Some(idx) = self.list_state.selected() {
                    self.open_detail(idx).await;
                }
            }
            _ => {}
        }
    }

    async fn handle_detail_key(&mut self, key: KeyEvent, checkpoint_idx: usize) {
        let checkpoints = self.cache.checkpoints();
        let Some(cp) = checkpoints.get(checkpoint_idx).cloned() else {
            self.view = View::CheckpointList;
            return;
        };
        let commits = self.cache.commits().to_vec();

        let action = detail::handle_key(key, &mut self.detail_state, &cp, &commits);
        match action {
            detail::DetailAction::Back => {
                self.view = View::CheckpointList;
                self.loaded_transcript = None;
            }
            detail::DetailAction::OpenTranscript { session_idx } => {
                self.transcript_state.reset();
                self.view = View::Transcript {
                    checkpoint_idx,
                    session_idx,
                };
                // Load transcript if not already loaded for this session.
                self.load_transcript_for_session(&cp, session_idx).await;
            }
            detail::DetailAction::OpenDiff { commit_hash } => {
                self.open_diff(&commit_hash).await;
            }
            detail::DetailAction::OpenGitLog => {
                self.git_log_state.reset(self.cache.commits().len());
                self.view = View::GitLog;
            }
            detail::DetailAction::None => {}
        }
    }

    fn handle_transcript_key(&mut self, key: KeyEvent, checkpoint_idx: usize, _session_idx: usize) {
        let entries = self.loaded_transcript.as_deref().unwrap_or(&[]);
        let action = transcript::handle_key(key, &mut self.transcript_state, entries.len());
        match action {
            transcript::TranscriptAction::Back => {
                self.view = View::CheckpointDetail(checkpoint_idx);
            }
            transcript::TranscriptAction::None | transcript::TranscriptAction::ScrollTo(_) => {}
        }
    }

    fn handle_diff_key(&mut self, key: KeyEvent, hash: &str) {
        let diffs = self
            .cache
            .cached_diffs(hash)
            .map(<[mementor_lib::git::diff::FileDiff]>::to_vec)
            .unwrap_or_default();
        let action = diff_view::handle_key(key, &mut self.diff_state, &diffs);
        match action {
            diff_view::DiffViewAction::Back => {
                // Go back to wherever we came from. For simplicity, go to
                // checkpoint list. A proper back-stack could be added later.
                self.view = View::CheckpointList;
            }
            diff_view::DiffViewAction::None => {}
        }
    }

    async fn handle_git_log_key(&mut self, key: KeyEvent) {
        let commits = self.cache.commits().to_vec();
        let action = git_log::handle_key(key, &mut self.git_log_state, &commits);
        match action {
            git_log::GitLogAction::Back => {
                self.view = View::CheckpointList;
            }
            git_log::GitLogAction::OpenCheckpoint(cp_id) => {
                // Find checkpoint index by checkpoint_id.
                if let Some(idx) = self
                    .cache
                    .checkpoints()
                    .iter()
                    .position(|c| c.checkpoint_id == cp_id)
                {
                    self.open_detail(idx).await;
                }
            }
            git_log::GitLogAction::OpenDiff(hash) => {
                self.open_diff(&hash).await;
            }
            git_log::GitLogAction::None => {}
        }
    }

    async fn open_detail(&mut self, checkpoint_idx: usize) {
        let session_count = self
            .cache
            .checkpoints()
            .get(checkpoint_idx)
            .map_or(0, |cp| cp.sessions.len());
        self.detail_state.reset(session_count);
        self.view = View::CheckpointDetail(checkpoint_idx);

        // Eagerly load transcript for the first session.
        if let Some(cp) = self.cache.checkpoints().get(checkpoint_idx).cloned() {
            self.load_transcript_for_session(&cp, 0).await;
        }
    }

    async fn open_diff(&mut self, commit_hash: &str) {
        // Pre-load diffs.
        let _ = self.cache.diffs(commit_hash).await;
        self.diff_state.reset();
        self.view = View::DiffView(commit_hash.to_owned());
    }

    async fn load_transcript_for_session(
        &mut self,
        cp: &mementor_lib::model::CheckpointMeta,
        session_idx: usize,
    ) {
        if let Some(session) = cp.sessions.get(session_idx)
            && !session.blob_path.is_empty()
            && let Ok(entries) = self.cache.transcript(&session.blob_path).await
        {
            self.loaded_transcript = Some(entries.to_vec());
            return;
        }
        self.loaded_transcript = None;
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
