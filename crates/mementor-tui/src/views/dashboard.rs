use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let checkpoints = app.cache.checkpoints();
    let commits = app.cache.commits();

    let items: Vec<ListItem> = checkpoints
        .iter()
        .map(|cp| {
            let first_commit = cp
                .commit_hashes
                .first()
                .and_then(|hash| super::find_commit_by_hash(commits, hash));

            // Line 1: first commit subject or checkpoint_id fallback.
            let subject =
                first_commit.map_or_else(|| cp.checkpoint_id.clone(), |c| c.subject.clone());

            // Line 2: details.
            let short_hash = first_commit.map_or_else(String::new, |c| c.short_hash.clone());

            let date_str = cp
                .sessions
                .first()
                .map_or_else(String::new, |s| format_relative_time(&s.created_at));

            let agent = cp.sessions.first().map_or("unknown", |s| &s.agent);

            let files_count = cp.files_touched.len();

            let total_tokens = cp.token_usage.input_tokens + cp.token_usage.output_tokens;
            let token_display = format_tokens(total_tokens);

            let detail_line = format!(
                "  {short_hash}  {date_str}  {agent}  {files_count} files  {token_display}",
            );

            ListItem::new(vec![
                Line::from(Span::raw(subject)),
                Line::from(Span::styled(
                    detail_line,
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect();

    let title = format!(" Checkpoints [{}] ", app.selected_branch);
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn format_tokens(total: u64) -> String {
    super::text_utils::format_tokens(total)
}

fn format_relative_time(iso_str: &str) -> String {
    super::text_utils::format_relative_time(iso_str)
}
