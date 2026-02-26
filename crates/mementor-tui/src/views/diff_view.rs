use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use mementor_lib::git::diff::{DiffLine, FileDiff, FileStatus};

/// State for the diff view.
pub struct DiffViewState {
    /// Index of the currently displayed file.
    pub file_index: usize,
    /// Scroll offset (line index) within the current file's rendered lines.
    pub scroll_offset: usize,
    /// Whether the file picker popup is open.
    pub file_picker_open: bool,
    /// Selection state for the file picker list.
    pub file_picker_state: ListState,
}

impl Default for DiffViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffViewState {
    pub fn new() -> Self {
        Self {
            file_index: 0,
            scroll_offset: 0,
            file_picker_open: false,
            file_picker_state: ListState::default(),
        }
    }

    /// Reset state for viewing a new diff.
    pub fn reset(&mut self) {
        self.file_index = 0;
        self.scroll_offset = 0;
        self.file_picker_open = false;
        self.file_picker_state.select(None);
    }
}

/// Actions returned by the diff view's key handler.
pub enum DiffViewAction {
    None,
    Back,
}

/// Render the diff view.
///
/// `header` is a summary string (e.g. commit short hash + subject) shown at
/// the top of the view.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &mut DiffViewState,
    diffs: &[FileDiff],
    header: &str,
) {
    if diffs.is_empty() {
        let empty = Paragraph::new("No file diffs available.")
            .block(Block::default().borders(Borders::ALL).title(" Diff "));
        frame.render_widget(empty, area);
        return;
    }

    // Clamp file index.
    if state.file_index >= diffs.len() {
        state.file_index = diffs.len() - 1;
    }

    let diff = &diffs[state.file_index];

    // Build the title: header + file info.
    let file_label = format!("[{}/{}] {}", state.file_index + 1, diffs.len(), diff.path,);
    let title = format!(" {header}  {file_label} ");

    // Build rendered lines from hunks.
    let rendered = build_rendered_lines(diff);
    let total_lines = rendered.len();

    // Clamp scroll offset.
    let visible_height = area.height.saturating_sub(2) as usize; // borders
    if total_lines > 0 && state.scroll_offset + visible_height > total_lines {
        state.scroll_offset = total_lines.saturating_sub(visible_height);
    }

    // Slice to visible window.
    let visible: Vec<Line<'_>> = rendered
        .iter()
        .skip(state.scroll_offset)
        .take(visible_height)
        .map(|(style, text)| Line::from(Span::styled(text.as_str(), *style)))
        .collect();

    let paragraph =
        Paragraph::new(visible).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(paragraph, area);

    // File picker popup.
    if state.file_picker_open {
        render_file_picker(frame, area, state, diffs);
    }
}

/// Handle a key event in the diff view.
pub fn handle_key(key: KeyEvent, state: &mut DiffViewState, diffs: &[FileDiff]) -> DiffViewAction {
    if state.file_picker_open {
        return handle_file_picker_key(key, state, diffs);
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            state.scroll_offset = state.scroll_offset.saturating_add(1);
            DiffViewAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.scroll_offset = state.scroll_offset.saturating_sub(1);
            DiffViewAction::None
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.scroll_offset = state.scroll_offset.saturating_add(15);
            DiffViewAction::None
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.scroll_offset = state.scroll_offset.saturating_sub(15);
            DiffViewAction::None
        }
        KeyCode::Char('n') => {
            if !diffs.is_empty() {
                state.file_index = (state.file_index + 1).min(diffs.len() - 1);
                state.scroll_offset = 0;
            }
            DiffViewAction::None
        }
        KeyCode::Char('N') => {
            state.file_index = state.file_index.saturating_sub(1);
            state.scroll_offset = 0;
            DiffViewAction::None
        }
        KeyCode::Char('f') => {
            state.file_picker_open = true;
            state.file_picker_state.select(Some(state.file_index));
            DiffViewAction::None
        }
        KeyCode::Char(']') => {
            jump_to_next_hunk(state, diffs);
            DiffViewAction::None
        }
        KeyCode::Char('[') => {
            jump_to_prev_hunk(state, diffs);
            DiffViewAction::None
        }
        KeyCode::Esc => DiffViewAction::Back,
        _ => DiffViewAction::None,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// A pre-rendered line: (style, formatted text).
type RenderedLine = (Style, String);

/// Parse the `old_start` and `new_start` from a hunk header like
/// `@@ -old_start,old_count +new_start,new_count @@`.
fn parse_hunk_header(header: &str) -> (usize, usize) {
    // Expected format: @@ -<old_start>[,<old_count>] +<new_start>[,<new_count>] @@...
    let mut old_start: usize = 1;
    let mut new_start: usize = 1;

    if let Some(rest) = header.strip_prefix("@@ -") {
        // rest: "old_start,old_count +new_start,new_count @@ ..."
        if let Some(plus_idx) = rest.find('+') {
            let old_part = rest[..plus_idx].trim().trim_end_matches(',');
            old_start = old_part
                .split(',')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);

            let after_plus = &rest[plus_idx + 1..];
            new_start = after_plus
                .split([',', ' '])
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
        }
    }

    (old_start, new_start)
}

/// Build all rendered lines for a single file diff, with dual line numbers.
fn build_rendered_lines(diff: &FileDiff) -> Vec<RenderedLine> {
    let mut lines = Vec::new();
    let hunk_style = Style::default().fg(Color::Blue);
    let add_style = Style::default().fg(Color::Green);
    let del_style = Style::default().fg(Color::Red);
    let ctx_style = Style::default();

    for hunk in &diff.hunks {
        // Hunk header line.
        lines.push((hunk_style, format!("          {}", hunk.header)));

        let (mut old_line, mut new_line) = parse_hunk_header(&hunk.header);

        for diff_line in &hunk.lines {
            match diff_line {
                DiffLine::Context(text) => {
                    lines.push((ctx_style, format!("{old_line:>4} {new_line:>4} | {text}")));
                    old_line += 1;
                    new_line += 1;
                }
                DiffLine::Added(text) => {
                    lines.push((add_style, format!("     {new_line:>4} |+{text}")));
                    new_line += 1;
                }
                DiffLine::Removed(text) => {
                    lines.push((del_style, format!("{old_line:>4}      |-{text}")));
                    old_line += 1;
                }
            }
        }
    }

    lines
}

/// Find rendered-line indices that correspond to hunk headers.
fn hunk_header_offsets(diff: &FileDiff) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut line_idx: usize = 0;

    for hunk in &diff.hunks {
        offsets.push(line_idx);
        // +1 for the header line, then lines in the hunk.
        line_idx += 1 + hunk.lines.len();
    }

    offsets
}

fn jump_to_next_hunk(state: &mut DiffViewState, diffs: &[FileDiff]) {
    if let Some(diff) = diffs.get(state.file_index) {
        let offsets = hunk_header_offsets(diff);
        if let Some(&next) = offsets.iter().find(|&&o| o > state.scroll_offset) {
            state.scroll_offset = next;
        }
    }
}

fn jump_to_prev_hunk(state: &mut DiffViewState, diffs: &[FileDiff]) {
    if let Some(diff) = diffs.get(state.file_index) {
        let offsets = hunk_header_offsets(diff);
        if let Some(&prev) = offsets.iter().rev().find(|&&o| o < state.scroll_offset) {
            state.scroll_offset = prev;
        }
    }
}

fn status_badge(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Added => "[A]",
        FileStatus::Modified => "[M]",
        FileStatus::Deleted => "[D]",
        FileStatus::Renamed => "[R]",
    }
}

fn render_file_picker(
    frame: &mut Frame,
    area: Rect,
    state: &mut DiffViewState,
    diffs: &[FileDiff],
) {
    let popup_width = area.width * 50 / 100;
    let popup_height = area.height * 50 / 100;
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = diffs
        .iter()
        .map(|d| {
            let badge = status_badge(d.status);
            let label = format!("{badge} {}", d.path);
            ListItem::new(Span::raw(label))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Files "))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, popup_area, &mut state.file_picker_state);
}

fn handle_file_picker_key(
    key: KeyEvent,
    state: &mut DiffViewState,
    diffs: &[FileDiff],
) -> DiffViewAction {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            let len = diffs.len();
            if len > 0 {
                let i = state
                    .file_picker_state
                    .selected()
                    .map_or(0, |i| if i + 1 < len { i + 1 } else { i });
                state.file_picker_state.select(Some(i));
            }
            DiffViewAction::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let len = diffs.len();
            if len > 0 {
                let i = state
                    .file_picker_state
                    .selected()
                    .map_or(0, |i| i.saturating_sub(1));
                state.file_picker_state.select(Some(i));
            }
            DiffViewAction::None
        }
        KeyCode::Enter => {
            if let Some(i) = state.file_picker_state.selected() {
                state.file_index = i;
                state.scroll_offset = 0;
            }
            state.file_picker_open = false;
            DiffViewAction::None
        }
        KeyCode::Esc | KeyCode::Char('f') => {
            state.file_picker_open = false;
            DiffViewAction::None
        }
        _ => DiffViewAction::None,
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mementor_lib::git::diff::DiffHunk;

    fn sample_diffs() -> Vec<FileDiff> {
        vec![
            FileDiff {
                path: "src/main.rs".to_owned(),
                status: FileStatus::Modified,
                additions: 2,
                deletions: 1,
                hunks: vec![DiffHunk {
                    header: "@@ -1,3 +1,4 @@".to_owned(),
                    lines: vec![
                        DiffLine::Context("fn main() {".to_owned()),
                        DiffLine::Removed("    println!(\"hello\");".to_owned()),
                        DiffLine::Added("    println!(\"hello world\");".to_owned()),
                        DiffLine::Added("    println!(\"goodbye\");".to_owned()),
                        DiffLine::Context("}".to_owned()),
                    ],
                }],
            },
            FileDiff {
                path: "new_file.txt".to_owned(),
                status: FileStatus::Added,
                additions: 1,
                deletions: 0,
                hunks: vec![DiffHunk {
                    header: "@@ -0,0 +1 @@".to_owned(),
                    lines: vec![DiffLine::Added("new content".to_owned())],
                }],
            },
        ]
    }

    #[test]
    fn parse_hunk_header_basic() {
        assert_eq!(parse_hunk_header("@@ -1,3 +1,4 @@"), (1, 1));
        assert_eq!(parse_hunk_header("@@ -10,5 +20,8 @@"), (10, 20));
        assert_eq!(parse_hunk_header("@@ -0,0 +1 @@"), (0, 1));
    }

    #[test]
    fn build_rendered_lines_has_correct_count() {
        let diffs = sample_diffs();
        let lines = build_rendered_lines(&diffs[0]);
        // 1 hunk header + 5 diff lines = 6
        assert_eq!(lines.len(), 6);
    }

    #[test]
    fn hunk_header_offsets_basic() {
        let diffs = sample_diffs();
        let offsets = hunk_header_offsets(&diffs[0]);
        assert_eq!(offsets, vec![0]);
    }

    #[test]
    fn status_badge_values() {
        assert_eq!(status_badge(FileStatus::Added), "[A]");
        assert_eq!(status_badge(FileStatus::Modified), "[M]");
        assert_eq!(status_badge(FileStatus::Deleted), "[D]");
        assert_eq!(status_badge(FileStatus::Renamed), "[R]");
    }

    #[test]
    fn handle_key_esc_returns_back() {
        let mut state = DiffViewState::new();
        let diffs = sample_diffs();
        let key = KeyEvent::from(KeyCode::Esc);
        let action = handle_key(key, &mut state, &diffs);
        assert!(matches!(action, DiffViewAction::Back));
    }

    #[test]
    fn handle_key_j_increments_scroll() {
        let mut state = DiffViewState::new();
        let diffs = sample_diffs();
        let key = KeyEvent::from(KeyCode::Char('j'));
        let _ = handle_key(key, &mut state, &diffs);
        assert_eq!(state.scroll_offset, 1);
    }

    #[test]
    fn handle_key_n_advances_file() {
        let mut state = DiffViewState::new();
        let diffs = sample_diffs();
        let key = KeyEvent::from(KeyCode::Char('n'));
        let _ = handle_key(key, &mut state, &diffs);
        assert_eq!(state.file_index, 1);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn handle_key_f_opens_file_picker() {
        let mut state = DiffViewState::new();
        let diffs = sample_diffs();
        let key = KeyEvent::from(KeyCode::Char('f'));
        let _ = handle_key(key, &mut state, &diffs);
        assert!(state.file_picker_open);
    }
}
