use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use mementor_lib::model::{ContentBlock, MessageRole, TranscriptEntry, TranscriptMessage};
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Actions returned by transcript view key handling.
pub enum TranscriptAction {
    None,
    Back,
    ScrollTo(usize),
}

/// Persistent state for the fullscreen transcript viewer.
pub struct TranscriptViewState {
    /// Current scroll position (line-based).
    pub scroll_offset: usize,
    /// Indices of expanded tool-call blocks.
    pub expanded_tools: HashSet<usize>,
    /// Whether the right-hand tool-index sidebar is visible.
    pub show_sidebar: bool,
    /// Active search text (set after pressing Enter in search mode).
    pub search_query: Option<String>,
    /// Line indices that match the current search query.
    pub search_matches: Vec<usize>,
    /// Index into `search_matches` for n/N cycling.
    pub search_index: usize,
    /// Whether `/` search-input mode is active.
    pub search_input_active: bool,
    /// Buffer for typing a search query.
    pub search_input_buf: String,

    // -- Internal bookkeeping (updated each render) --------------------------
    /// Total rendered line count from the last `render()` call.
    total_lines: usize,
    /// Maps tool sequential-index to its rendered-line position.
    tool_line_map: Vec<usize>,
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
            total_lines: 0,
            tool_line_map: Vec::new(),
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
        self.total_lines = 0;
        self.tool_line_map.clear();
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the fullscreen transcript view.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &mut TranscriptViewState,
    entries: &[TranscriptEntry],
) {
    // Build all displayable lines from transcript entries.
    let (lines, tools) = build_lines(entries, state);
    state.total_lines = lines.len();
    state.tool_line_map = tools.iter().map(|t| t.line_index).collect();

    // Recompute search matches against freshly-built lines.
    if let Some(query) = &state.search_query {
        state.search_matches = find_matches(&lines, query);
        if !state.search_matches.is_empty() {
            state.search_index = state.search_index.min(state.search_matches.len() - 1);
        }
    }

    // Two-panel layout when sidebar is visible.
    let (transcript_area, sidebar_area) = if state.show_sidebar {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    // Reserve a bottom row for search bar when relevant.
    let show_search_bar = state.search_input_active || state.search_query.is_some();
    let (content_area, search_area) = if show_search_bar {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(transcript_area);
        (chunks[0], Some(chunks[1]))
    } else {
        (transcript_area, None)
    };

    // The border takes 2 rows (top + bottom).
    let inner_height = content_area.height.saturating_sub(2) as usize;

    // Handle empty transcript.
    if entries.is_empty() {
        let block = Block::default().borders(Borders::ALL).title(" Transcript ");
        let inner = block.inner(content_area);
        frame.render_widget(block, content_area);
        let p = Paragraph::new("No transcript data").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, inner);
        if let Some(sa) = sidebar_area {
            render_sidebar(frame, sa, &[], state);
        }
        return;
    }

    // Clamp scroll offset.
    let max_scroll = lines.len().saturating_sub(inner_height);
    state.scroll_offset = state.scroll_offset.min(max_scroll);

    // Slice visible lines.
    let visible: Vec<Line<'_>> = lines
        .iter()
        .skip(state.scroll_offset)
        .take(inner_height)
        .cloned()
        .collect();

    // Apply search highlights to visible slice.
    let visible = if state.search_query.is_some() {
        apply_search_highlights(
            visible,
            state.search_query.as_deref().unwrap_or_default(),
            &state.search_matches,
            state.search_index,
            state.scroll_offset,
        )
    } else {
        visible
    };

    // Scroll position indicator.
    let scroll_info = {
        let end = (state.scroll_offset + inner_height).min(lines.len());
        format!(" {}-{}/{} ", state.scroll_offset + 1, end, lines.len())
    };

    let transcript_widget = Paragraph::new(Text::from(visible)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Transcript ")
            .title_bottom(Line::from(scroll_info).right_aligned()),
    );
    frame.render_widget(transcript_widget, content_area);

    // Search bar.
    if let Some(search_area) = search_area {
        render_search_bar(frame, search_area, state);
    }

    // Sidebar.
    if let Some(sidebar_area) = sidebar_area {
        render_sidebar(frame, sidebar_area, &tools, state);
    }
}

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

/// Handle key events for the transcript view.
pub fn handle_key(
    key: KeyEvent,
    state: &mut TranscriptViewState,
    _entry_count: usize,
) -> TranscriptAction {
    // Search input mode intercepts all keys.
    if state.search_input_active {
        return handle_search_input(key, state);
    }

    match key.code {
        // -- Scrolling -------------------------------------------------------
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
            state.scroll_offset = state.total_lines;
            TranscriptAction::None
        }

        // -- Tool expand / collapse ------------------------------------------
        KeyCode::Enter => {
            toggle_nearest_tool(state);
            TranscriptAction::None
        }

        // -- Search ----------------------------------------------------------
        KeyCode::Char('/') => {
            state.search_input_active = true;
            state.search_input_buf.clear();
            TranscriptAction::None
        }
        KeyCode::Char('n') => {
            advance_match(state, true);
            TranscriptAction::None
        }
        KeyCode::Char('N') => {
            advance_match(state, false);
            TranscriptAction::None
        }

        // -- Sidebar ---------------------------------------------------------
        KeyCode::Char('o') => {
            state.show_sidebar = !state.show_sidebar;
            TranscriptAction::None
        }

        // -- Back / Esc ------------------------------------------------------
        KeyCode::Esc => {
            if state.search_query.is_some() {
                // First Esc clears search; second Esc goes back.
                state.search_query = None;
                state.search_matches.clear();
                state.search_index = 0;
                TranscriptAction::None
            } else {
                TranscriptAction::Back
            }
        }

        _ => TranscriptAction::None,
    }
}

// ---------------------------------------------------------------------------
// Internal: line building
// ---------------------------------------------------------------------------

/// Metadata for a tool-use block tracked during line building.
struct ToolEntry {
    name: String,
    line_index: usize,
    tool_index: usize,
}

/// Convert transcript entries into a flat list of styled lines plus tool
/// metadata for the sidebar.
fn build_lines(
    entries: &[TranscriptEntry],
    state: &TranscriptViewState,
) -> (Vec<Line<'static>>, Vec<ToolEntry>) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut tools: Vec<ToolEntry> = Vec::new();
    let mut tool_seq: usize = 0;

    for entry in entries {
        match entry {
            TranscriptEntry::Message(msg) => {
                append_message(msg, state, &mut lines, &mut tools, &mut tool_seq);
            }
            TranscriptEntry::FileHistorySnapshot { files } => {
                lines.push(Line::from(Span::styled(
                    format!("--- file snapshot ({} files) ---", files.len()),
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::from(""));
            }
            TranscriptEntry::PrLink {
                pr_number, pr_url, ..
            } => {
                lines.push(Line::from(Span::styled(
                    format!("PR #{pr_number}: {pr_url}"),
                    Style::default().fg(Color::Cyan),
                )));
                lines.push(Line::from(""));
            }
            TranscriptEntry::Progress(raw) => {
                let display = truncate(raw, 80);
                lines.push(Line::from(Span::styled(
                    format!("[progress] {display}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            TranscriptEntry::Other(_) => {}
        }
    }

    (lines, tools)
}

fn append_message(
    msg: &TranscriptMessage,
    state: &TranscriptViewState,
    lines: &mut Vec<Line<'static>>,
    tools: &mut Vec<ToolEntry>,
    tool_seq: &mut usize,
) {
    // Header line.
    let (style, label) = match msg.role {
        MessageRole::User => (
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            "[User]",
        ),
        MessageRole::Assistant => (
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            "[Assistant]",
        ),
    };
    let ts = msg.timestamp.as_deref().unwrap_or("");
    lines.push(Line::from(vec![
        Span::styled(format!("{label} "), style),
        Span::styled(ts.to_owned(), Style::default().fg(Color::DarkGray)),
    ]));

    // Content blocks.
    for block in &msg.content {
        match block {
            ContentBlock::Text(text) => {
                for l in text.lines() {
                    lines.push(Line::from(format!("  {l}")));
                }
                if text.is_empty() {
                    lines.push(Line::from(""));
                }
            }
            ContentBlock::Thinking(text) => {
                append_thinking(text, lines);
            }
            ContentBlock::ToolUse { name, input } => {
                append_tool_use(name, input, *tool_seq, state, lines, tools);
                *tool_seq += 1;
            }
            ContentBlock::ToolResult { content, .. } => {
                append_tool_result(content, tool_seq.saturating_sub(1), state, lines);
            }
        }
    }

    // Blank separator after each message.
    lines.push(Line::from(""));
}

fn append_thinking(text: &str, lines: &mut Vec<Line<'static>>) {
    let first = text.lines().next().unwrap_or("");
    let multi = text.lines().count() > 1;
    let preview = if first.chars().count() > 60 {
        format!("{}...", first.chars().take(60).collect::<String>())
    } else if multi {
        format!("{first}...")
    } else {
        first.to_owned()
    };
    lines.push(Line::from(Span::styled(
        format!("  [thinking] {preview}"),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )));
}

fn append_tool_use(
    name: &str,
    input: &impl std::fmt::Display,
    idx: usize,
    state: &TranscriptViewState,
    lines: &mut Vec<Line<'static>>,
    tools: &mut Vec<ToolEntry>,
) {
    tools.push(ToolEntry {
        name: name.to_owned(),
        line_index: lines.len(),
        tool_index: idx,
    });

    let expanded = state.expanded_tools.contains(&idx);

    if expanded {
        // Top border with tool name.
        lines.push(Line::from(vec![
            Span::styled("  \u{250c}\u{2500} ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("[{idx}] {name}"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        // Args (compact JSON, split into wrapped chunks).
        let raw = input.to_string();
        for chunk in wrap_str(&raw, 100) {
            lines.push(Line::from(Span::styled(
                format!("  \u{2502} {chunk}"),
                Style::default().fg(Color::Yellow),
            )));
        }
        // Bottom border.
        lines.push(Line::from(Span::styled(
            "  \u{2514}\u{2500}".to_owned(),
            Style::default().fg(Color::Yellow),
        )));
    } else {
        // Collapsed: single line with tool name and truncated args.
        let preview = truncate(&input.to_string(), 60);
        lines.push(Line::from(vec![
            Span::styled(format!("  [{idx}] "), Style::default().fg(Color::Yellow)),
            Span::styled(
                name.to_owned(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {preview}"), Style::default().fg(Color::DarkGray)),
        ]));
    }
}

fn append_tool_result(
    content: &str,
    parent_idx: usize,
    state: &TranscriptViewState,
    lines: &mut Vec<Line<'static>>,
) {
    // Only show result when the parent tool is expanded.
    if !state.expanded_tools.contains(&parent_idx) {
        return;
    }
    lines.push(Line::from(Span::styled(
        "  Result:".to_owned(),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )));
    for l in content.lines().take(15) {
        lines.push(Line::from(Span::styled(
            format!("    {l}"),
            Style::default().fg(Color::DarkGray),
        )));
    }
    if content.lines().count() > 15 {
        lines.push(Line::from(Span::styled(
            "    ...".to_owned(),
            Style::default().fg(Color::DarkGray),
        )));
    }
}

// ---------------------------------------------------------------------------
// Internal: sidebar
// ---------------------------------------------------------------------------

fn render_sidebar(frame: &mut Frame, area: Rect, tools: &[ToolEntry], state: &TranscriptViewState) {
    let nearest = nearest_tool_index(state);

    let items: Vec<ListItem<'_>> = tools
        .iter()
        .map(|t| {
            let is_current = nearest == Some(t.tool_index);
            let is_expanded = state.expanded_tools.contains(&t.tool_index);

            let style = if is_current {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_expanded {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let marker = if is_expanded { "\u{25bc}" } else { "\u{25b6}" };
            ListItem::new(Line::from(Span::styled(
                format!("{marker} [{0}] {1}", t.tool_index, t.name),
                style,
            )))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Tools "));
    frame.render_widget(list, area);
}

// ---------------------------------------------------------------------------
// Internal: search bar
// ---------------------------------------------------------------------------

fn render_search_bar(frame: &mut Frame, area: Rect, state: &TranscriptViewState) {
    let text = if state.search_input_active {
        format!("/{}", state.search_input_buf)
    } else if let Some(q) = &state.search_query {
        let info = if state.search_matches.is_empty() {
            "no matches".to_owned()
        } else {
            format!("{}/{}", state.search_index + 1, state.search_matches.len())
        };
        format!("/{q}  [{info}]")
    } else {
        String::new()
    };

    let bar = Paragraph::new(text).style(Style::default().fg(Color::Yellow));
    frame.render_widget(bar, area);
}

// ---------------------------------------------------------------------------
// Internal: search helpers
// ---------------------------------------------------------------------------

fn find_matches(lines: &[Line<'_>], query: &str) -> Vec<usize> {
    let q = query.to_ascii_lowercase();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            text.to_ascii_lowercase().contains(&q)
        })
        .map(|(i, _)| i)
        .collect()
}

fn apply_search_highlights<'a>(
    lines: Vec<Line<'a>>,
    query: &str,
    matches: &[usize],
    current_idx: usize,
    scroll_offset: usize,
) -> Vec<Line<'a>> {
    let current_line = matches.get(current_idx).copied();
    let query_lower = query.to_ascii_lowercase();
    let qlen = query.len();

    lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let abs = scroll_offset + i;
            if !matches.contains(&abs) {
                return line;
            }
            let bg = if current_line == Some(abs) {
                Color::Yellow
            } else {
                Color::DarkGray
            };
            let new_spans: Vec<Span<'a>> = line
                .spans
                .into_iter()
                .flat_map(|span| highlight_span(span, &query_lower, qlen, bg))
                .collect();
            Line::from(new_spans)
        })
        .collect()
}

fn highlight_span<'a>(span: Span<'a>, query_lower: &str, qlen: usize, bg: Color) -> Vec<Span<'a>> {
    let text = span.content.to_string();
    let text_lower = text.to_ascii_lowercase();
    let positions: Vec<usize> = text_lower
        .match_indices(query_lower)
        .map(|(pos, _)| pos)
        .collect();

    if positions.is_empty() {
        return vec![span];
    }

    let base_style = span.style;
    let hl_style = base_style.bg(bg).fg(Color::Black);
    let mut out = Vec::new();
    let mut cursor = 0;

    for pos in positions {
        if pos > cursor {
            out.push(Span::styled(text[cursor..pos].to_owned(), base_style));
        }
        let end = (pos + qlen).min(text.len());
        out.push(Span::styled(text[pos..end].to_owned(), hl_style));
        cursor = end;
    }
    if cursor < text.len() {
        out.push(Span::styled(text[cursor..].to_owned(), base_style));
    }
    out
}

fn handle_search_input(key: KeyEvent, state: &mut TranscriptViewState) -> TranscriptAction {
    match key.code {
        KeyCode::Enter => {
            if state.search_input_buf.is_empty() {
                state.search_query = None;
                state.search_matches.clear();
            } else {
                state.search_query = Some(state.search_input_buf.clone());
            }
            state.search_input_active = false;
            state.search_index = 0;
            TranscriptAction::None
        }
        KeyCode::Esc => {
            state.search_input_active = false;
            state.search_input_buf.clear();
            TranscriptAction::None
        }
        KeyCode::Backspace => {
            state.search_input_buf.pop();
            TranscriptAction::None
        }
        KeyCode::Char(c) => {
            state.search_input_buf.push(c);
            TranscriptAction::None
        }
        _ => TranscriptAction::None,
    }
}

fn advance_match(state: &mut TranscriptViewState, forward: bool) {
    if state.search_matches.is_empty() {
        return;
    }
    let len = state.search_matches.len();
    state.search_index = if forward {
        (state.search_index + 1) % len
    } else if state.search_index == 0 {
        len - 1
    } else {
        state.search_index - 1
    };
    if let Some(&line) = state.search_matches.get(state.search_index) {
        state.scroll_offset = line;
    }
}

// ---------------------------------------------------------------------------
// Internal: tool toggle
// ---------------------------------------------------------------------------

fn nearest_tool_index(state: &TranscriptViewState) -> Option<usize> {
    let scroll = state.scroll_offset;
    let mut best: Option<(usize, usize)> = None;
    for (idx, &line_pos) in state.tool_line_map.iter().enumerate() {
        let dist = scroll.abs_diff(line_pos);
        if dist <= 5 && best.is_none_or(|(_, d)| dist < d) {
            best = Some((idx, dist));
        }
    }
    best.map(|(idx, _)| idx)
}

fn toggle_nearest_tool(state: &mut TranscriptViewState) {
    if let Some(idx) = nearest_tool_index(state) {
        if state.expanded_tools.contains(&idx) {
            state.expanded_tools.remove(&idx);
        } else {
            state.expanded_tools.insert(idx);
        }
    }
}

// ---------------------------------------------------------------------------
// Internal: text utilities
// ---------------------------------------------------------------------------

fn truncate(s: &str, max_width: usize) -> String {
    super::text_utils::truncate(s, max_width)
}

fn wrap_str(s: &str, width: usize) -> Vec<String> {
    super::text_utils::wrap_str(s, width)
}
