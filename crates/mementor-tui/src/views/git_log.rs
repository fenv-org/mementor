use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use mementor_lib::git::log::CommitInfo;

/// State for the git log view.
pub struct GitLogState {
    /// Selection state for the commit list.
    pub list_state: ListState,
}

impl Default for GitLogState {
    fn default() -> Self {
        Self::new()
    }
}

impl GitLogState {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
        }
    }

    /// Reset state, selecting the first commit if any exist.
    pub fn reset(&mut self, commit_count: usize) {
        if commit_count > 0 {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }
}

/// Actions returned by the git log view's key handler.
pub enum GitLogAction {
    None,
    Back,
    OpenCheckpoint(String),
    OpenDiff(String),
}

/// Render the git log view.
pub fn render(frame: &mut Frame, area: Rect, state: &mut GitLogState, commits: &[CommitInfo]) {
    if commits.is_empty() {
        let empty = ratatui::widgets::Paragraph::new("No commits found.")
            .block(Block::default().borders(Borders::ALL).title(" Git Log "));
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = commits
        .iter()
        .map(|commit| {
            // Line 1: * short_hash  subject
            let line1 = Line::from(vec![
                Span::styled("* ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    &commit.short_hash,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::raw(&commit.subject),
            ]);

            // Line 2: date  author  [checkpoint_id]
            let mut spans2 = vec![
                Span::raw("  "),
                Span::styled(
                    format_short_date(&commit.date),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(&commit.author, Style::default().fg(Color::DarkGray)),
            ];

            if let Some(cp_id) = &commit.checkpoint_id {
                spans2.push(Span::raw("  "));
                spans2.push(Span::styled(
                    format!("[{cp_id}]"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            let line2 = Line::from(spans2);
            ListItem::new(vec![line1, line2])
        })
        .collect();

    let title = format!(" Git Log [{}] ", commits.len());
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut state.list_state);
}

/// Handle a key event in the git log view.
pub fn handle_key(key: KeyEvent, state: &mut GitLogState, commits: &[CommitInfo]) -> GitLogAction {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            let len = commits.len();
            if len > 0 {
                let i = state
                    .list_state
                    .selected()
                    .map_or(0, |i| if i + 1 < len { i + 1 } else { i });
                state.list_state.select(Some(i));
            }
            GitLogAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let len = commits.len();
            if len > 0 {
                let i = state
                    .list_state
                    .selected()
                    .map_or(0, |i| i.saturating_sub(1));
                state.list_state.select(Some(i));
            }
            GitLogAction::None
        }
        KeyCode::Enter => {
            if let Some(i) = state.list_state.selected()
                && let Some(commit) = commits.get(i)
            {
                if let Some(cp_id) = &commit.checkpoint_id {
                    return GitLogAction::OpenCheckpoint(cp_id.clone());
                }
                return GitLogAction::OpenDiff(commit.hash.clone());
            }
            GitLogAction::None
        }
        KeyCode::Char('d') => {
            if let Some(i) = state.list_state.selected()
                && let Some(commit) = commits.get(i)
            {
                return GitLogAction::OpenDiff(commit.hash.clone());
            }
            GitLogAction::None
        }
        KeyCode::Esc => GitLogAction::Back,
        _ => GitLogAction::None,
    }
}

/// Format a git date string (e.g. "2026-02-20 10:30:00 +0900") into a
/// shorter display form (e.g. "2026-02-20 10:30").
fn format_short_date(date: &str) -> String {
    // Parse with jiff to validate and extract components, falling back to raw
    // string on failure.
    jiff::fmt::strtime::parse("%Y-%m-%d %H:%M:%S %z", date)
        .and_then(|bdt| bdt.to_datetime())
        .map_or_else(
            |_| date.to_owned(),
            |dt| {
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}",
                    dt.year(),
                    dt.month(),
                    dt.day(),
                    dt.hour(),
                    dt.minute(),
                )
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_commits() -> Vec<CommitInfo> {
        vec![
            CommitInfo {
                hash: "abc123def456789abcdef0123456789abcdef01".to_owned(),
                short_hash: "abc123d".to_owned(),
                subject: "add feature X".to_owned(),
                author: "Alice".to_owned(),
                date: "2026-02-20 10:30:00 +0900".to_owned(),
                checkpoint_id: Some("cp-001".to_owned()),
            },
            CommitInfo {
                hash: "def456789abcdef0123456789abcdef0123456789".to_owned(),
                short_hash: "def4567".to_owned(),
                subject: "fix bug Y".to_owned(),
                author: "Bob".to_owned(),
                date: "2026-02-19 09:00:00 +0900".to_owned(),
                checkpoint_id: None,
            },
        ]
    }

    #[test]
    fn format_short_date_trims() {
        assert_eq!(
            format_short_date("2026-02-20 10:30:00 +0900"),
            "2026-02-20 10:30"
        );
    }

    #[test]
    fn format_short_date_short_input() {
        assert_eq!(format_short_date("2026-02-20"), "2026-02-20");
    }

    #[test]
    fn handle_key_esc_returns_back() {
        let mut state = GitLogState::new();
        let commits = sample_commits();
        let key = KeyEvent::from(KeyCode::Esc);
        let action = handle_key(key, &mut state, &commits);
        assert!(matches!(action, GitLogAction::Back));
    }

    #[test]
    fn handle_key_j_selects_first() {
        let mut state = GitLogState::new();
        let commits = sample_commits();
        let key = KeyEvent::from(KeyCode::Char('j'));
        let _ = handle_key(key, &mut state, &commits);
        assert_eq!(state.list_state.selected(), Some(0));
    }

    #[test]
    fn handle_key_enter_with_checkpoint() {
        let mut state = GitLogState::new();
        state.list_state.select(Some(0));
        let commits = sample_commits();
        let key = KeyEvent::from(KeyCode::Enter);
        let action = handle_key(key, &mut state, &commits);
        assert!(matches!(action, GitLogAction::OpenCheckpoint(ref id) if id == "cp-001"));
    }

    #[test]
    fn handle_key_enter_without_checkpoint() {
        let mut state = GitLogState::new();
        state.list_state.select(Some(1));
        let commits = sample_commits();
        let key = KeyEvent::from(KeyCode::Enter);
        let action = handle_key(key, &mut state, &commits);
        assert!(matches!(action, GitLogAction::OpenDiff(ref hash) if hash.starts_with("def456")));
    }

    #[test]
    fn handle_key_d_opens_diff() {
        let mut state = GitLogState::new();
        state.list_state.select(Some(0));
        let commits = sample_commits();
        let key = KeyEvent::from(KeyCode::Char('d'));
        let action = handle_key(key, &mut state, &commits);
        assert!(matches!(action, GitLogAction::OpenDiff(ref hash) if hash.starts_with("abc123")));
    }
}
