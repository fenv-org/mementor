use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let popup_width = area.width * 40 / 100;
    let popup_height = area.height * 50 / 100;
    let popup_area = centered_rect(popup_width, popup_height, area);

    frame.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = app
        .branches
        .iter()
        .map(|b| {
            let style = if b == &app.selected_branch {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Span::styled(b.as_str(), style))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Switch Branch "),
        )
        .highlight_style(Style::default().bg(Color::DarkGray))
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, popup_area, &mut app.branch_list_state);
}

pub async fn handle_key(key: KeyEvent, app: &mut App) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            let len = app.branches.len();
            if len == 0 {
                return;
            }
            let i = app
                .branch_list_state
                .selected()
                .map_or(0, |i| if i + 1 < len { i + 1 } else { i });
            app.branch_list_state.select(Some(i));
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let len = app.branches.len();
            if len == 0 {
                return;
            }
            let i = app
                .branch_list_state
                .selected()
                .map_or(0, |i| i.saturating_sub(1));
            app.branch_list_state.select(Some(i));
        }
        KeyCode::Enter => {
            if let Some(i) = app.branch_list_state.selected()
                && let Some(branch) = app.branches.get(i)
            {
                app.selected_branch.clone_from(branch);
                let _ = app.cache.refresh().await;
                app.clamp_selection();
            }
            app.branch_popup_open = false;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.branch_popup_open = false;
        }
        _ => {}
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
