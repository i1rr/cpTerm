use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::InputMode;
use crate::util::format_size;

use super::dual_pane::DualPane;

pub fn draw_status_bar(
    frame: &mut Frame,
    area: Rect,
    dual_pane: &DualPane,
    input_mode: &InputMode,
) {
    let explorer = dual_pane.active_explorer();
    let path = explorer.current_dir.to_string_lossy().to_string();

    let mut spans = vec![
        Span::styled(format!("{}> ", path), Style::default().fg(Color::Cyan)),
    ];

    match input_mode {
        InputMode::Command(text) => {
            spans.push(Span::styled(text.as_str(), Style::default().fg(Color::White)));
            spans.push(Span::styled("_", Style::default().fg(Color::Cyan)));
        }
        _ => {
            let sel_count = explorer.selected.len();
            let total_size = explorer.selected_total_size();

            if sel_count > 0 {
                spans.push(Span::styled(
                    format!("{} selected", sel_count),
                    Style::default().fg(Color::Yellow),
                ));
                spans.push(Span::raw(" | "));
                spans.push(Span::styled(
                    format!("{} total", format_size(total_size)),
                    Style::default().fg(Color::Yellow),
                ));
            }

            if let Some(ref filter) = explorer.filter_text {
                if sel_count > 0 {
                    spans.push(Span::raw(" | "));
                }
                spans.push(Span::styled(
                    format!("Filter: {}", filter),
                    Style::default().fg(Color::Green),
                ));
            }
        }
    }

    let paragraph = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(paragraph, area);
}
