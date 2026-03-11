use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::util::format_size;

use super::dual_pane::DualPane;

pub fn draw_status_bar(frame: &mut Frame, area: Rect, dual_pane: &DualPane) {
    let explorer = dual_pane.active_explorer();
    let path = explorer.current_dir.to_string_lossy().to_string();
    let sel_count = explorer.selected.len();
    let total_size = explorer.selected_total_size();

    let mut spans = vec![
        Span::styled(&path, Style::default().fg(Color::White)),
    ];

    if sel_count > 0 {
        spans.push(Span::raw(" | "));
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

    // Show filter status
    if let Some(ref filter) = explorer.filter_text {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            format!("Filter: {}", filter),
            Style::default().fg(Color::Green),
        ));
    }

    let paragraph = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(paragraph, area);
}
