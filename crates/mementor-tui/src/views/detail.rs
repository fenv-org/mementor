use std::collections::HashMap;
use std::hash::BuildHasher;

use crossterm::event::{KeyCode, KeyEvent};
use mementor_lib::git::diff::FileStatus;
use mementor_lib::git::log::CommitInfo;
use mementor_lib::model::{CheckpointMeta, ContentBlock, MessageRole, TranscriptEntry};
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

/// Which panel has keyboard focus in the detail view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailPanel {
    Sessions,
    Files,
    Commits,
    Transcript,
}

impl DetailPanel {
    fn next(self) -> Self {
        match self {
            Self::Sessions => Self::Files,
            Self::Files => Self::Commits,
            Self::Commits => Self::Transcript,
            Self::Transcript => Self::Sessions,
        }
    }
}

/// Actions returned by detail view key handling.
pub enum DetailAction {
    None,
    Back,
    OpenTranscript { session_idx: usize },
    OpenDiff { commit_hash: String },
    OpenGitLog,
}

/// State for the checkpoint detail view.
pub struct DetailState {
    pub focus: DetailPanel,
    pub session_list_state: ListState,
    pub file_list_state: ListState,
    pub commit_list_state: ListState,
    pub transcript_scroll: u16,
}

impl DetailState {
    pub fn new(session_count: usize) -> Self {
        let mut session_list_state = ListState::default();
        if session_count > 0 {
            session_list_state.select(Some(0));
        }
        Self {
            focus: DetailPanel::Sessions,
            session_list_state,
            file_list_state: ListState::default(),
            commit_list_state: ListState::default(),
            transcript_scroll: 0,
        }
    }

    pub fn reset(&mut self, session_count: usize) {
        self.focus = DetailPanel::Sessions;
        self.transcript_scroll = 0;
        self.file_list_state = ListState::default();
        self.commit_list_state = ListState::default();
        self.session_list_state = ListState::default();
        if session_count > 0 {
            self.session_list_state.select(Some(0));
        }
    }
}

/// Render the checkpoint detail view.
///
/// Layout:
/// ```text
/// +--[ checkpoint_id: title ]----------------------------------------+
/// |                    |                                              |
/// | Sessions        3  |  [User]  ...                                |
/// |   Claude Code      |  message text                               |
/// |   ...              |                                              |
/// |                    |  [Assistant]                                 |
/// | Files           5  |  response text                              |
/// |   M schema.sql     |                                              |
/// |   A new_file.rs    |                                              |
/// |                    |                                              |
/// | Commits         2  |                                              |
/// |   c04a441 redesign |                                              |
/// |   727be48 update   |                                              |
/// +--------------------+----------------------------------------------+
/// ```
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn render<S: BuildHasher>(
    frame: &mut Frame,
    area: Rect,
    state: &mut DetailState,
    checkpoint: &CheckpointMeta,
    commits: &[CommitInfo],
    file_statuses: &HashMap<String, FileStatus, S>,
    transcript_entries: Option<&[TranscriptEntry]>,
) {
    // Header.
    let subject = checkpoint
        .commit_hashes
        .first()
        .and_then(|h| commits.iter().find(|c| &c.hash == h || &c.short_hash == h))
        .map_or_else(|| checkpoint.checkpoint_id.clone(), |c| c.subject.clone());
    let short_id = &checkpoint.checkpoint_id[..checkpoint.checkpoint_id.len().min(12)];
    let title = format!(" {short_id}: {subject} ");

    let outer_block = Block::default().borders(Borders::ALL).title(title);
    let inner_area = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    // Split into left sidebar (30%) and right transcript (70%).
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(inner_area);

    render_sidebar(
        frame,
        h_chunks[0],
        state,
        checkpoint,
        commits,
        file_statuses,
    );
    render_transcript_pane(frame, h_chunks[1], state, transcript_entries);
}

fn render_sidebar<S: BuildHasher>(
    frame: &mut Frame,
    area: Rect,
    state: &mut DetailState,
    checkpoint: &CheckpointMeta,
    commits: &[CommitInfo],
    file_statuses: &HashMap<String, FileStatus, S>,
) {
    // Split sidebar into 3 sections: Sessions, Files, Commits.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Percentage(35),
            Constraint::Percentage(30),
        ])
        .split(area);

    // -- Sessions --
    let session_items: Vec<ListItem> = checkpoint
        .sessions
        .iter()
        .map(|s| {
            let total_tok = s.token_usage.input_tokens + s.token_usage.output_tokens;
            let tok_str = format_tokens(total_tok);
            let line = format!("  {} — {tok_str}", s.agent);
            ListItem::new(Line::from(Span::raw(line)))
        })
        .collect();

    let sessions_title = format!(" Sessions  {} ", checkpoint.sessions.len());
    let session_border_style = if state.focus == DetailPanel::Sessions {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let session_list = List::new(session_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(session_border_style)
                .title(sessions_title),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");
    frame.render_stateful_widget(session_list, chunks[0], &mut state.session_list_state);

    // -- Files --
    let file_items: Vec<ListItem> = checkpoint
        .files_touched
        .iter()
        .map(|f| {
            let badge = match file_statuses.get(f.as_str()) {
                Some(FileStatus::Added) => "A",
                Some(FileStatus::Modified) | None => "M",
                Some(FileStatus::Deleted) => "D",
                Some(FileStatus::Renamed) => "R",
            };
            let line = format!("  {badge} {f}");
            ListItem::new(Line::from(Span::raw(line)))
        })
        .collect();

    let files_title = format!(" Files  {} ", checkpoint.files_touched.len());
    let file_border_style = if state.focus == DetailPanel::Files {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let file_list = List::new(file_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(file_border_style)
                .title(files_title),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");
    frame.render_stateful_widget(file_list, chunks[1], &mut state.file_list_state);

    // -- Commits --
    let relevant_commits: Vec<&CommitInfo> = checkpoint
        .commit_hashes
        .iter()
        .filter_map(|h| commits.iter().find(|c| &c.hash == h || &c.short_hash == h))
        .collect();

    let commit_items: Vec<ListItem> = relevant_commits
        .iter()
        .map(|c| {
            let line = format!("  {} {}", c.short_hash, truncate(&c.subject, 30));
            ListItem::new(Line::from(Span::raw(line)))
        })
        .collect();

    let commits_title = format!(" Commits  {} ", relevant_commits.len());
    let commit_border_style = if state.focus == DetailPanel::Commits {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let commit_list = List::new(commit_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(commit_border_style)
                .title(commits_title),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");
    frame.render_stateful_widget(commit_list, chunks[2], &mut state.commit_list_state);
}

fn render_transcript_pane(
    frame: &mut Frame,
    area: Rect,
    state: &DetailState,
    transcript_entries: Option<&[TranscriptEntry]>,
) {
    let border_style = if state.focus == DetailPanel::Transcript {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Transcript ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(entries) = transcript_entries else {
        let loading =
            Paragraph::new("Loading transcript...").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(loading, inner);
        return;
    };

    if entries.is_empty() {
        let empty =
            Paragraph::new("No transcript data").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, inner);
        return;
    }

    // Render transcript as styled lines.
    let mut lines: Vec<Line> = Vec::new();

    for entry in entries {
        match entry {
            TranscriptEntry::Message(msg) => {
                render_message_lines(&mut lines, msg);
            }
            TranscriptEntry::PrLink {
                pr_number, pr_url, ..
            } => {
                lines.push(Line::from(Span::styled(
                    format!("  PR #{pr_number}: {pr_url}"),
                    Style::default().fg(Color::Magenta),
                )));
            }
            TranscriptEntry::FileHistorySnapshot { files } => {
                lines.push(Line::from(Span::styled(
                    format!("  [Snapshot: {} files]", files.len()),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            TranscriptEntry::Progress(_) | TranscriptEntry::Other(_) => {}
        }
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((state.transcript_scroll, 0));
    frame.render_widget(paragraph, inner);
}

fn render_message_lines(lines: &mut Vec<Line>, msg: &mementor_lib::model::TranscriptMessage) {
    // Header line.
    let (role_label, role_style) = match msg.role {
        MessageRole::User => (
            "[User]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        MessageRole::Assistant => (
            "[Assistant]",
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
    };

    let timestamp_str = msg.timestamp.as_deref().unwrap_or("");
    lines.push(Line::from(vec![
        Span::styled(format!("  {role_label}"), role_style),
        Span::styled(
            format!("  {timestamp_str}"),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    // Content blocks.
    for block in &msg.content {
        match block {
            ContentBlock::Text(text) => {
                for line in text.lines() {
                    lines.push(Line::from(Span::raw(format!("  {line}"))));
                }
            }
            ContentBlock::Thinking(text) => {
                let preview = truncate(text, 80);
                lines.push(Line::from(Span::styled(
                    format!("  [thinking] {preview}"),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            ContentBlock::ToolUse { name, .. } => {
                lines.push(Line::from(Span::styled(
                    format!("  [{name}]"),
                    Style::default().fg(Color::Cyan),
                )));
            }
            ContentBlock::ToolResult { content, .. } => {
                let preview = truncate(content, 60);
                lines.push(Line::from(Span::styled(
                    format!("    → {preview}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }
}

/// Handle key events for the detail view.
pub fn handle_key(
    key: KeyEvent,
    state: &mut DetailState,
    checkpoint: &CheckpointMeta,
    commits: &[CommitInfo],
) -> DetailAction {
    match key.code {
        KeyCode::Esc => DetailAction::Back,
        KeyCode::Tab => {
            state.focus = state.focus.next();
            DetailAction::None
        }
        KeyCode::Char('t') => {
            let session_idx = state.session_list_state.selected().unwrap_or(0);
            DetailAction::OpenTranscript { session_idx }
        }
        KeyCode::Char('d') => {
            let hashes = relevant_commit_hashes(checkpoint, commits);
            if let Some(idx) = state.commit_list_state.selected() {
                let hash = hashes.get(idx).cloned().unwrap_or_default();
                DetailAction::OpenDiff { commit_hash: hash }
            } else if let Some(hash) = hashes.first() {
                DetailAction::OpenDiff {
                    commit_hash: hash.clone(),
                }
            } else {
                DetailAction::None
            }
        }
        KeyCode::Char('g') => DetailAction::OpenGitLog,
        KeyCode::Char('j') | KeyCode::Down => {
            handle_scroll_down(state, checkpoint, commits);
            DetailAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            handle_scroll_up(state, checkpoint, commits);
            DetailAction::None
        }
        KeyCode::Enter => handle_enter(state, checkpoint, commits),
        _ => DetailAction::None,
    }
}

fn handle_scroll_down(
    state: &mut DetailState,
    checkpoint: &CheckpointMeta,
    commits: &[CommitInfo],
) {
    match state.focus {
        DetailPanel::Sessions => {
            let len = checkpoint.sessions.len();
            scroll_list_down(&mut state.session_list_state, len);
        }
        DetailPanel::Files => {
            let len = checkpoint.files_touched.len();
            scroll_list_down(&mut state.file_list_state, len);
        }
        DetailPanel::Commits => {
            let relevant = checkpoint
                .commit_hashes
                .iter()
                .filter(|h| commits.iter().any(|c| &c.hash == *h || &c.short_hash == *h))
                .count();
            scroll_list_down(&mut state.commit_list_state, relevant);
        }
        DetailPanel::Transcript => {
            state.transcript_scroll = state.transcript_scroll.saturating_add(1);
        }
    }
}

fn handle_scroll_up(
    state: &mut DetailState,
    _checkpoint: &CheckpointMeta,
    _commits: &[CommitInfo],
) {
    match state.focus {
        DetailPanel::Sessions => scroll_list_up(&mut state.session_list_state),
        DetailPanel::Files => scroll_list_up(&mut state.file_list_state),
        DetailPanel::Commits => scroll_list_up(&mut state.commit_list_state),
        DetailPanel::Transcript => {
            state.transcript_scroll = state.transcript_scroll.saturating_sub(1);
        }
    }
}

fn handle_enter(
    state: &DetailState,
    checkpoint: &CheckpointMeta,
    commits: &[CommitInfo],
) -> DetailAction {
    let hashes = relevant_commit_hashes(checkpoint, commits);
    match state.focus {
        DetailPanel::Commits => {
            if let Some(idx) = state.commit_list_state.selected() {
                let hash = hashes.get(idx).cloned().unwrap_or_default();
                DetailAction::OpenDiff { commit_hash: hash }
            } else {
                DetailAction::None
            }
        }
        DetailPanel::Files => {
            if let Some(hash) = hashes.first() {
                DetailAction::OpenDiff {
                    commit_hash: hash.clone(),
                }
            } else {
                DetailAction::None
            }
        }
        DetailPanel::Sessions | DetailPanel::Transcript => {
            let session_idx = state.session_list_state.selected().unwrap_or(0);
            DetailAction::OpenTranscript { session_idx }
        }
    }
}

fn scroll_list_down(list_state: &mut ListState, len: usize) {
    if len == 0 {
        return;
    }
    let i = list_state
        .selected()
        .map_or(0, |i| if i + 1 < len { i + 1 } else { i });
    list_state.select(Some(i));
}

fn scroll_list_up(list_state: &mut ListState) {
    let i = list_state.selected().map_or(0, |i| i.saturating_sub(1));
    list_state.select(Some(i));
}

/// Return the commit hashes from the checkpoint that are actually present in
/// the `commits` list, in the same order as displayed in the sidebar.
fn relevant_commit_hashes(checkpoint: &CheckpointMeta, commits: &[CommitInfo]) -> Vec<String> {
    checkpoint
        .commit_hashes
        .iter()
        .filter(|h| commits.iter().any(|c| &c.hash == *h || &c.short_hash == *h))
        .cloned()
        .collect()
}

fn truncate(s: &str, max_width: usize) -> String {
    super::text_utils::truncate(s, max_width)
}

#[allow(clippy::cast_precision_loss)]
fn format_tokens(total: u64) -> String {
    if total >= 1_000_000 {
        let m = total as f64 / 1_000_000.0;
        format!("{m:.1}M tok")
    } else if total >= 1_000 {
        let k = total as f64 / 1_000.0;
        format!("{k:.1}K tok")
    } else {
        format!("{total} tok")
    }
}
