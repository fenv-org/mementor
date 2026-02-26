use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
pub use mementor_lib::search::SearchScope;
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

/// Actions returned by the search overlay key handler.
pub enum SearchOverlayAction {
    /// Close the search overlay.
    Close,
    /// Open the checkpoint at this index.
    OpenCheckpoint(usize),
    /// Search query changed — App should re-run search.
    QueryChanged,
    /// Scope toggled — App should re-run search.
    ScopeChanged,
    /// No action needed.
    None,
}

/// Display-ready search result (converted from `SearchMatch` by the App layer).
#[derive(Debug, Clone)]
pub struct SearchMatchDisplay {
    /// Checkpoint index (for navigation).
    pub checkpoint_idx: usize,
    /// Short checkpoint ID (first 12 chars).
    pub checkpoint_id: String,
    /// Commit subject or fallback description.
    pub title: String,
    /// Relative time string (e.g., "2h ago").
    pub time_ago: String,
    /// Matching text snippet with surrounding context.
    pub snippet: String,
    /// Number of matches in this checkpoint.
    pub match_count: usize,
}

/// State for the search overlay.
pub struct SearchOverlayState {
    /// Current search input text.
    pub input: String,
    /// Search results (updated on every input change by the App).
    pub results: Vec<SearchMatchDisplay>,
    /// Selected result index in the list.
    pub selected: usize,
    /// Search scope toggle.
    pub scope: SearchScope,
    /// List state for scrolling/selection.
    pub list_state: ListState,
}

impl Default for SearchOverlayState {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchOverlayState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            results: Vec::new(),
            selected: 0,
            scope: SearchScope::AllBranches,
            list_state: ListState::default(),
        }
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.results.clear();
        self.selected = 0;
        self.scope = SearchScope::AllBranches;
        self.list_state.select(None);
    }

    fn select_next(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let next = if self.selected + 1 < self.results.len() {
            self.selected + 1
        } else {
            self.selected
        };
        self.selected = next;
        self.list_state.select(Some(next));
    }

    fn select_prev(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let prev = self.selected.saturating_sub(1);
        self.selected = prev;
        self.list_state.select(Some(prev));
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the search overlay on top of the given area.
pub fn render(frame: &mut Frame, area: Rect, state: &mut SearchOverlayState) {
    let popup_width = area.width * 80 / 100;
    let popup_height = area.height * 80 / 100;
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default().borders(Borders::ALL).title(" Search ");
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split interior: input line (1) + results (rest) + help bar (1).
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    render_input_line(frame, chunks[0], state);
    render_results(frame, chunks[1], state);
    render_help_bar(frame, chunks[2], state);
}

fn render_input_line(frame: &mut Frame, area: Rect, state: &SearchOverlayState) {
    let spans = vec![
        Span::styled("Query: ", Style::default().fg(Color::DarkGray)),
        Span::styled(&state.input, Style::default().fg(Color::White)),
    ];
    let paragraph = Paragraph::new(Line::from(spans));
    frame.render_widget(paragraph, area);

    // Position the cursor at the end of the input.
    #[allow(clippy::cast_possible_truncation)]
    let cursor_x = area.x + 7 + state.input.len() as u16;
    let cursor_y = area.y;
    if cursor_x < area.x + area.width {
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn render_results(frame: &mut Frame, area: Rect, state: &mut SearchOverlayState) {
    if state.input.is_empty() {
        let hint = Paragraph::new("Type to search across all transcripts")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        // Center vertically.
        let y_offset = area.height / 2;
        let hint_area = Rect::new(area.x, area.y + y_offset, area.width, 1);
        frame.render_widget(hint, hint_area);
        return;
    }

    if state.results.is_empty() {
        let hint = Paragraph::new("No results")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        let y_offset = area.height / 2;
        let hint_area = Rect::new(area.x, area.y + y_offset, area.width, 1);
        frame.render_widget(hint, hint_area);
        return;
    }

    // Sync selection.
    if !state.results.is_empty() && state.list_state.selected().is_none() {
        state.list_state.select(Some(0));
        state.selected = 0;
    }

    let max_width = area.width as usize;

    let items: Vec<ListItem<'_>> = state
        .results
        .iter()
        .enumerate()
        .map(|(i, result)| {
            let num = i + 1;
            let match_suffix = if result.match_count > 1 {
                format!(" ({} matches)", result.match_count)
            } else {
                String::new()
            };

            // Line 1: [N] checkpoint_id — title (time_ago)
            let line1 = Line::from(vec![
                Span::styled(
                    format!("[{num}] "),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    result.checkpoint_id.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        " \u{2014} {} ({}){match_suffix}",
                        truncate(&result.title, max_width.saturating_sub(30)),
                        result.time_ago,
                    ),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]);

            // Line 2: "...snippet..."
            let snippet = truncate(&result.snippet, max_width.saturating_sub(10));
            let line2 = Line::from(Span::styled(
                format!("    \"{snippet}\""),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ));

            // Blank line separator.
            let blank = Line::from("");

            ListItem::new(vec![line1, line2, blank])
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut state.list_state);
}

fn render_help_bar(frame: &mut Frame, area: Rect, state: &SearchOverlayState) {
    let key_style = Style::default().fg(Color::Yellow);
    let desc_style = Style::default().fg(Color::DarkGray);

    let scope_label = match state.scope {
        SearchScope::AllBranches => "all",
        SearchScope::CurrentBranch => "branch",
    };

    let result_count = format!(
        "{} result{}",
        state.results.len(),
        if state.results.len() == 1 { "" } else { "s" }
    );

    let left_spans = vec![
        Span::styled("Enter", key_style),
        Span::styled(" Open  ", desc_style),
        Span::styled("\u{2191}/\u{2193}", key_style),
        Span::styled(" Navigate  ", desc_style),
        Span::styled("Tab", key_style),
        Span::styled(format!(" Scope: {scope_label}  "), desc_style),
        Span::styled("Esc", key_style),
        Span::styled(" Close", desc_style),
    ];

    let right_span = Span::styled(result_count, desc_style);

    // Render left-aligned hints and right-aligned count.
    let left = Paragraph::new(Line::from(left_spans));
    let right = Paragraph::new(Line::from(right_span)).alignment(Alignment::Right);

    frame.render_widget(left, area);
    frame.render_widget(right, area);
}

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

/// Handle key events for the search overlay.
///
/// The search overlay is always in "input mode": printable characters are
/// appended to the query. Arrow keys and Ctrl-n/Ctrl-p navigate results.
pub fn handle_key(key: KeyEvent, state: &mut SearchOverlayState) -> SearchOverlayAction {
    // Ctrl-modified chars first.
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('n') => {
                state.select_next();
                SearchOverlayAction::None
            }
            KeyCode::Char('p') => {
                state.select_prev();
                SearchOverlayAction::None
            }
            KeyCode::Char('u') => {
                state.input.clear();
                state.selected = 0;
                state.list_state.select(None);
                SearchOverlayAction::QueryChanged
            }
            _ => SearchOverlayAction::None,
        };
    }

    match key.code {
        KeyCode::Esc => SearchOverlayAction::Close,

        KeyCode::Enter => {
            if let Some(result) = state.results.get(state.selected) {
                SearchOverlayAction::OpenCheckpoint(result.checkpoint_idx)
            } else {
                SearchOverlayAction::None
            }
        }

        KeyCode::Tab => {
            state.scope = match state.scope {
                SearchScope::AllBranches => SearchScope::CurrentBranch,
                SearchScope::CurrentBranch => SearchScope::AllBranches,
            };
            SearchOverlayAction::ScopeChanged
        }

        KeyCode::Down => {
            state.select_next();
            SearchOverlayAction::None
        }

        KeyCode::Up => {
            state.select_prev();
            SearchOverlayAction::None
        }

        KeyCode::Backspace => {
            state.input.pop();
            state.selected = 0;
            if !state.results.is_empty() {
                state.list_state.select(Some(0));
            }
            SearchOverlayAction::QueryChanged
        }

        // All other printable chars go to the input buffer.
        KeyCode::Char(c) => {
            state.input.push(c);
            state.selected = 0;
            if !state.results.is_empty() {
                state.list_state.select(Some(0));
            }
            SearchOverlayAction::QueryChanged
        }

        _ => SearchOverlayAction::None,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn truncate(s: &str, max_width: usize) -> String {
    super::text_utils::truncate(s, max_width)
}
