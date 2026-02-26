use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{App, View};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let key_style = Style::default().fg(Color::Yellow);
    let desc_style = Style::default();

    let spans = match &app.view {
        View::CheckpointList => vec![
            Span::styled("j/k", key_style),
            Span::styled(" Navigate  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(" Detail  ", desc_style),
            Span::styled("b", key_style),
            Span::styled(" Branch  ", desc_style),
            Span::styled("g", key_style),
            Span::styled(" Git log  ", desc_style),
            Span::styled("q", key_style),
            Span::styled(" Quit", desc_style),
        ],
        View::CheckpointDetail(_) => vec![
            Span::styled("Tab", key_style),
            Span::styled(" Focus  ", desc_style),
            Span::styled("j/k", key_style),
            Span::styled(" Scroll  ", desc_style),
            Span::styled("t", key_style),
            Span::styled(" Transcript  ", desc_style),
            Span::styled("d", key_style),
            Span::styled(" Diffs  ", desc_style),
            Span::styled("g", key_style),
            Span::styled(" Git log  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(" Back", desc_style),
        ],
        View::Transcript { .. } => vec![
            Span::styled("j/k", key_style),
            Span::styled(" Scroll  ", desc_style),
            Span::styled("^d/^u", key_style),
            Span::styled(" Page  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(" Expand  ", desc_style),
            Span::styled("/", key_style),
            Span::styled(" Search  ", desc_style),
            Span::styled("o", key_style),
            Span::styled(" Sidebar  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(" Back", desc_style),
        ],
        View::DiffView(_) => vec![
            Span::styled("j/k", key_style),
            Span::styled(" Scroll  ", desc_style),
            Span::styled("n/N", key_style),
            Span::styled(" File  ", desc_style),
            Span::styled("f", key_style),
            Span::styled(" Files  ", desc_style),
            Span::styled("]/[", key_style),
            Span::styled(" Hunk  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(" Back", desc_style),
        ],
        View::GitLog => vec![
            Span::styled("j/k", key_style),
            Span::styled(" Navigate  ", desc_style),
            Span::styled("Enter", key_style),
            Span::styled(" Detail  ", desc_style),
            Span::styled("d", key_style),
            Span::styled(" Diff  ", desc_style),
            Span::styled("Esc", key_style),
            Span::styled(" Back", desc_style),
        ],
    };

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}
