use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, _app: &App) {
    let key_style = Style::default().fg(Color::Yellow);
    let desc_style = Style::default();

    let spans = vec![
        Span::styled("j/k", key_style),
        Span::styled(" Navigate  ", desc_style),
        Span::styled("Enter", key_style),
        Span::styled(" Detail  ", desc_style),
        Span::styled("b", key_style),
        Span::styled(" Branch  ", desc_style),
        Span::styled("/", key_style),
        Span::styled(" Search  ", desc_style),
        Span::styled("g", key_style),
        Span::styled(" Git log  ", desc_style),
        Span::styled("q", key_style),
        Span::styled(" Quit", desc_style),
    ];

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}
