use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

/// Actions returned by the search overlay key handler.
pub enum SearchOverlayAction {
    /// Close the search overlay.
    Close,
    /// Open the checkpoint at this index.
    OpenCheckpoint(usize),
    /// Open a commit that has no associated checkpoint.
    OpenCommit(String),
    /// User pressed Enter — trigger an AI search with the current query.
    TriggerSearch,
    /// No action needed.
    None,
}

/// Display-ready search result (converted from `AiSearchResult` by the App layer).
#[derive(Debug, Clone)]
pub struct SearchMatchDisplay {
    /// Checkpoint index (for navigation). `None` when the commit has no checkpoint.
    pub checkpoint_idx: Option<usize>,
    /// Commit SHA.
    pub commit_sha: String,
    /// Commit subject or fallback description.
    pub title: String,
    /// Relative time string (e.g., "2h ago").
    pub time_ago: String,
    /// AI-generated answer explaining the match.
    pub answer: String,
    /// Associated pull request (e.g., "#42").
    pub pr: Option<String>,
}

/// State for the search overlay.
pub struct SearchOverlayState {
    /// Current search input text.
    pub input: String,
    /// Search results (updated when AI search completes).
    pub results: Vec<SearchMatchDisplay>,
    /// Selected result index in the list.
    pub selected: usize,
    /// List state for scrolling/selection.
    pub list_state: ListState,
    /// Whether an AI search is currently running.
    pub loading: bool,
    /// Error message from the last search attempt.
    pub error: Option<String>,
    /// Elapsed ticks since search started (for spinner animation).
    pub elapsed_ticks: usize,
    /// The query that was last submitted for search.
    pub last_searched_query: Option<String>,
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
            list_state: ListState::default(),
            loading: false,
            error: None,
            elapsed_ticks: 0,
            last_searched_query: None,
        }
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.results.clear();
        self.selected = 0;
        self.list_state.select(None);
        self.loading = false;
        self.error = None;
        self.elapsed_ticks = 0;
        self.last_searched_query = None;
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

    let block = Block::default().borders(Borders::ALL).title(" AI Search ");
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
    // Loading state.
    if state.loading {
        let spinner_chars = ['|', '/', '-', '\\'];
        let spinner = spinner_chars[state.elapsed_ticks % spinner_chars.len()];
        let elapsed_secs = state.elapsed_ticks / 10; // ~100ms per tick
        let text = format!("{spinner} Searching with AI... ({elapsed_secs}s)");
        let hint = Paragraph::new(text)
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);
        let y_offset = area.height / 2;
        let hint_area = Rect::new(area.x, area.y + y_offset, area.width, 1);
        frame.render_widget(hint, hint_area);
        return;
    }

    // Error state.
    if let Some(err) = &state.error {
        let line1 = Paragraph::new(truncate(err, area.width as usize - 4))
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        let line2 = Paragraph::new("Press Enter to retry")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        let y_offset = area.height / 2;
        let area1 = Rect::new(area.x, area.y + y_offset.saturating_sub(1), area.width, 1);
        let area2 = Rect::new(area.x, area.y + y_offset + 1, area.width, 1);
        frame.render_widget(line1, area1);
        frame.render_widget(line2, area2);
        return;
    }

    // Empty state — no query submitted yet.
    if state.input.is_empty() && state.last_searched_query.is_none() {
        let hint = Paragraph::new("Type a query and press Enter to search with AI")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        let y_offset = area.height / 2;
        let hint_area = Rect::new(area.x, area.y + y_offset, area.width, 1);
        frame.render_widget(hint, hint_area);
        return;
    }

    // No results.
    if state.results.is_empty() && state.last_searched_query.is_some() {
        let hint = Paragraph::new("No results found")
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

            // Line 1: [N] id — title (time_ago)
            // Show PR number when available, otherwise commit SHA prefix.
            let id_label = if let Some(pr) = &result.pr {
                format!("PR{pr}")
            } else {
                let sha_len = result.commit_sha.len().min(12);
                result.commit_sha[..sha_len].to_owned()
            };

            let line1 = Line::from(vec![
                Span::styled(
                    format!("[{num}] "),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    id_label,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        " \u{2014} {} ({})",
                        truncate(&result.title, max_width.saturating_sub(30)),
                        result.time_ago,
                    ),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]);

            // Line 2: answer.
            let line2 = Line::from(Span::styled(
                format!(
                    "    {}",
                    truncate(&result.answer, max_width.saturating_sub(6))
                ),
                Style::default().fg(Color::Green),
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

    let result_count = format!(
        "{} result{}",
        state.results.len(),
        if state.results.len() == 1 { "" } else { "s" }
    );

    let left_spans = if state.loading {
        vec![
            Span::styled("Esc", key_style),
            Span::styled(" Cancel", desc_style),
        ]
    } else if state.results.is_empty() {
        vec![
            Span::styled("Enter", key_style),
            Span::styled(" Search  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(" Close", desc_style),
        ]
    } else {
        vec![
            Span::styled("Enter", key_style),
            Span::styled(" Open  ", desc_style),
            Span::styled("\u{2191}/\u{2193}", key_style),
            Span::styled(" Navigate  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(" Close", desc_style),
        ]
    };

    let right_span = Span::styled(result_count, desc_style);

    let left = Paragraph::new(Line::from(left_spans));
    let right = Paragraph::new(Line::from(right_span)).alignment(Alignment::Right);

    frame.render_widget(left, area);
    frame.render_widget(right, area);
}

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

/// Handle key events for the search overlay.
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
                SearchOverlayAction::None
            }
            _ => SearchOverlayAction::None,
        };
    }

    match key.code {
        KeyCode::Esc => SearchOverlayAction::Close,

        KeyCode::Enter => {
            // If results exist, open the selected one.
            if !state.results.is_empty()
                && let Some(result) = state.results.get(state.selected)
            {
                if let Some(idx) = result.checkpoint_idx {
                    return SearchOverlayAction::OpenCheckpoint(idx);
                }
                return SearchOverlayAction::OpenCommit(result.commit_sha.clone());
            }
            // Otherwise, trigger search if there's a query.
            if state.input.is_empty() {
                SearchOverlayAction::None
            } else {
                SearchOverlayAction::TriggerSearch
            }
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
            // Clear results when query changes.
            if state.input.is_empty() {
                state.results.clear();
                state.last_searched_query = None;
                state.list_state.select(None);
                state.error = None;
            }
            SearchOverlayAction::None
        }

        // All other printable chars go to the input buffer.
        KeyCode::Char(c) => {
            state.input.push(c);
            SearchOverlayAction::None
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
