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
            // Line 1: first commit subject or checkpoint_id fallback.
            let subject = cp
                .commit_hashes
                .first()
                .and_then(|hash| commits.iter().find(|c| &c.hash == hash || &c.short_hash == hash))
                .map_or_else(
                    || cp.checkpoint_id.clone(),
                    |c| c.subject.clone(),
                );

            // Line 2: details.
            let short_hash = cp
                .commit_hashes
                .first()
                .and_then(|hash| commits.iter().find(|c| &c.hash == hash || &c.short_hash == hash))
                .map_or_else(String::new, |c| c.short_hash.clone());

            let date_str = cp
                .sessions
                .first()
                .map_or_else(String::new, |s| format_relative_time(&s.created_at));

            let agent = cp
                .sessions
                .first()
                .map_or("unknown", |s| &s.agent);

            let (additions, deletions) = commit_stats(cp, commits);

            let files_count = cp.files_touched.len();

            let total_tokens = cp.token_usage.input_tokens + cp.token_usage.output_tokens;
            let token_display = format_tokens(total_tokens);

            let detail_line = format!(
                "  {short_hash}  {date_str}  {agent}  +{additions}/-{deletions}  {files_count} files  {token_display}",
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

fn commit_stats(
    cp: &mementor_lib::model::CheckpointMeta,
    commits: &[mementor_lib::git::log::CommitInfo],
) -> (usize, usize) {
    // Sum additions/deletions from commit diffs is not available in CommitInfo.
    // Use files_touched length as a proxy; actual diff stats would require
    // loading diffs which is lazy. For now, show file count only via the
    // checkpoint's commit count as a rough indicator.
    let _ = (cp, commits);
    (0, 0)
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

fn format_relative_time(iso_str: &str) -> String {
    // Parse ISO 8601 timestamp and compute relative time.
    // Use jiff for parsing, falling back to raw string on failure.
    let Ok(timestamp) = iso_str.parse::<jiff::Timestamp>() else {
        return iso_str.to_owned();
    };
    let now = jiff::Timestamp::now();
    let span = timestamp.until(now);
    let Ok(span) = span else {
        return iso_str.to_owned();
    };

    let total_seconds = span.get_seconds();
    let hours = total_seconds / 3600;
    let days = hours / 24;

    if days > 0 {
        format!("{days}d ago")
    } else if hours > 0 {
        format!("{hours}h ago")
    } else {
        let mins = total_seconds / 60;
        if mins > 0 {
            format!("{mins}m ago")
        } else {
            "just now".to_owned()
        }
    }
}
