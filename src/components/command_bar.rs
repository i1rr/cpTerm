use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::InputMode;

pub fn draw_command_bar(frame: &mut Frame, area: Rect, input_mode: &InputMode) {
    let line = match input_mode {
        InputMode::Normal => Line::from(vec![
            Span::styled("F1", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw("Help "),
            Span::styled("F2", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw("Ren "),
            Span::styled("F4", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw("New "),
            Span::styled("F5", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw("Copy "),
            Span::styled("F6", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw("Move "),
            Span::styled("F7", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw("Mkdir "),
            Span::styled("F8", Style::default().fg(Color::Black).bg(Color::Cyan)),
            Span::raw("Del  "),
            Span::styled("Ctrl+F", Style::default().fg(Color::Cyan)),
            Span::raw(":Filter  "),
            Span::styled("Tab", Style::default().fg(Color::Cyan)),
            Span::raw(":Switch  "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(":Quit"),
        ]),
        InputMode::Filter(text) => Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(Color::Green)),
            Span::raw(text.as_str()),
            Span::styled("_", Style::default().fg(Color::Green)),
            Span::raw("  (Enter to confirm, Esc to cancel)"),
        ]),
        InputMode::Rename(text) => Line::from(vec![
            Span::styled("Rename: ", Style::default().fg(Color::Yellow)),
            Span::raw(text.as_str()),
            Span::styled("_", Style::default().fg(Color::Yellow)),
            Span::raw("  (Enter to confirm, Esc to cancel)"),
        ]),
        InputMode::MkDir(text) => Line::from(vec![
            Span::styled("New directory: ", Style::default().fg(Color::Yellow)),
            Span::raw(text.as_str()),
            Span::styled("_", Style::default().fg(Color::Yellow)),
            Span::raw("  (Enter to confirm, Esc to cancel)"),
        ]),
        InputMode::CreateFile(text) => Line::from(vec![
            Span::styled("New file: ", Style::default().fg(Color::Yellow)),
            Span::raw(text.as_str()),
            Span::styled("_", Style::default().fg(Color::Yellow)),
            Span::raw("  (Enter to confirm, Esc to cancel)"),
        ]),
        InputMode::Command(_) => Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::raw(":Run  "),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(":Cancel  "),
            Span::styled("cd <path>", Style::default().fg(Color::DarkGray)),
            Span::raw(" to navigate, or any shell command"),
        ]),
    };

    let paragraph =
        Paragraph::new(line).style(Style::default().bg(Color::Black).fg(Color::White));
    frame.render_widget(paragraph, area);
}
