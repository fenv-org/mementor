use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mementor_lib::model::{ContentBlock, MessageRole, TranscriptEntry, TranscriptMessage};
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// Actions returned by transcript view key handling.
pub enum TranscriptAction {
    None,
    Back,
    ScrollTo(usize),
}

/// State for the fullscreen transcript view.
pub struct TranscriptViewState {
    pub scroll_offset: usize,
    pub expanded_tools: HashSet<usize>,
    pub show_sidebar: bool,
    pub search_query: Option<String>,
    pub search_matches: Vec<usize>,
    pub search_index: usize,
    pub search_input_active: bool,
    pub search_input_buf: String,
}

impl Default for TranscriptViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscriptViewState {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            expanded_tools: HashSet::new(),
            show_sidebar: true,
            search_query: None,
            search_matches: Vec::new(),
            search_index: 0,
            search_input_active: false,
            search_input_buf: String::new(),
        }
    }

    pub fn reset(&mut self) {
        self.scroll_offset = 0;
        self.expanded_tools.clear();
        self.show_sidebar = true;
        self.search_query = None;
        self.search_matches.clear();
        self.search_index = 0;
        self.search_input_active = false;
        self.search_input_buf.clear();
    }
}

/// Render the fullscreen transcript view.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &mut TranscriptViewState,
    entries: &[TranscriptEntry],
) {
    // Placeholder — will be replaced with agent output.
    let block = Block::default().borders(Borders::ALL).title(" Transcript ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if entries.is_empty() {
        let p = Paragraph::new("No transcript data").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, inner);
        return;
    }

    let _ = &state.show_sidebar;

    let mut lines: Vec<Line> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        render_entry(&mut lines, entry, i, state);
    }

    #[allow(clippy::cast_possible_truncation)]
    let scroll_y = state.scroll_offset as u16;
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y, 0));
    frame.render_widget(paragraph, inner);
}

fn render_entry(
    lines: &mut Vec<Line>,
    entry: &TranscriptEntry,
    _index: usize,
    state: &TranscriptViewState,
) {
    match entry {
        TranscriptEntry::Message(msg) => render_message(lines, msg, state),
        TranscriptEntry::Progress(text) => {
            lines.push(Line::from(Span::styled(
                format!("  {text}"),
                Style::default().fg(Color::DarkGray),
            )));
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
        TranscriptEntry::Other(_) => {}
    }
    lines.push(Line::from(""));
}

fn render_message(lines: &mut Vec<Line>, msg: &TranscriptMessage, _state: &TranscriptViewState) {
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

    let ts = msg.timestamp.as_deref().unwrap_or("");
    lines.push(Line::from(vec![
        Span::styled(format!("  {role_label}"), role_style),
        Span::styled(format!("  {ts}"), Style::default().fg(Color::DarkGray)),
    ]));

    for block in &msg.content {
        match block {
            ContentBlock::Text(text) => {
                for line in text.lines() {
                    lines.push(Line::from(Span::raw(format!("  {line}"))));
                }
            }
            ContentBlock::Thinking(text) => {
                let preview = if text.len() > 80 {
                    format!("{}...", &text[..80])
                } else {
                    text.clone()
                };
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
                let preview = if content.len() > 60 {
                    format!("{}...", &content[..60])
                } else {
                    content.clone()
                };
                lines.push(Line::from(Span::styled(
                    format!("    → {preview}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }
}

/// Handle key events for the transcript view.
pub fn handle_key(
    key: KeyEvent,
    state: &mut TranscriptViewState,
    _entry_count: usize,
) -> TranscriptAction {
    // Search input mode.
    if state.search_input_active {
        match key.code {
            KeyCode::Esc => {
                state.search_input_active = false;
                state.search_input_buf.clear();
            }
            KeyCode::Enter => {
                state.search_query = Some(state.search_input_buf.clone());
                state.search_input_active = false;
                // TODO: compute search matches
            }
            KeyCode::Backspace => {
                state.search_input_buf.pop();
            }
            KeyCode::Char(c) => {
                state.search_input_buf.push(c);
            }
            _ => {}
        }
        return TranscriptAction::None;
    }

    match key.code {
        KeyCode::Esc => TranscriptAction::Back,
        KeyCode::Char('j') | KeyCode::Down => {
            state.scroll_offset = state.scroll_offset.saturating_add(1);
            TranscriptAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.scroll_offset = state.scroll_offset.saturating_sub(1);
            TranscriptAction::None
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.scroll_offset = state.scroll_offset.saturating_add(20);
            TranscriptAction::None
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.scroll_offset = state.scroll_offset.saturating_sub(20);
            TranscriptAction::None
        }
        KeyCode::Char('g') => {
            state.scroll_offset = 0;
            TranscriptAction::None
        }
        KeyCode::Char('G') => {
            // Jump to bottom — use a large value; rendering will clamp.
            state.scroll_offset = usize::MAX / 2;
            TranscriptAction::None
        }
        KeyCode::Char('/') => {
            state.search_input_active = true;
            state.search_input_buf.clear();
            TranscriptAction::None
        }
        KeyCode::Char('o') => {
            state.show_sidebar = !state.show_sidebar;
            TranscriptAction::None
        }
        KeyCode::Char('n') => {
            if !state.search_matches.is_empty() {
                state.search_index = (state.search_index + 1) % state.search_matches.len();
                let line = state.search_matches[state.search_index];
                state.scroll_offset = line;
                return TranscriptAction::ScrollTo(line);
            }
            TranscriptAction::None
        }
        KeyCode::Char('N') => {
            if !state.search_matches.is_empty() {
                state.search_index = if state.search_index == 0 {
                    state.search_matches.len() - 1
                } else {
                    state.search_index - 1
                };
                let line = state.search_matches[state.search_index];
                state.scroll_offset = line;
                return TranscriptAction::ScrollTo(line);
            }
            TranscriptAction::None
        }
        _ => TranscriptAction::None,
    }
}
