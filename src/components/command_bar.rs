use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::InputMode;
use crate::theme::Theme;

pub fn draw_command_bar(frame: &mut Frame, area: Rect, input_mode: &InputMode, theme: &Theme) {
    let fkey_style = Style::default().fg(theme.fkey_fg).bg(theme.fkey_bg);
    let hint_style = Style::default().fg(theme.hint_fg);
    let input_style = Style::default().fg(theme.input_fg);
    let filter_style = Style::default().fg(theme.filter_fg);

    let line = match input_mode {
        InputMode::Normal => Line::from(vec![
            Span::styled("F1", fkey_style),
            Span::raw("Help "),
            Span::styled("F2", fkey_style),
            Span::raw("Ren "),
            Span::styled("F4", fkey_style),
            Span::raw("New "),
            Span::styled("F5", fkey_style),
            Span::raw("Copy "),
            Span::styled("F6", fkey_style),
            Span::raw("Move "),
            Span::styled("F7", fkey_style),
            Span::raw("Mkdir "),
            Span::styled("F8", fkey_style),
            Span::raw("Del  "),
            Span::styled("Ctrl+F", hint_style),
            Span::raw(":Filter  "),
            Span::styled("Tab", hint_style),
            Span::raw(":Switch  "),
            Span::styled("Ctrl+T", hint_style),
            Span::raw(":Theme  "),
            Span::styled("q", hint_style),
            Span::raw(":Quit"),
        ]),
        InputMode::Filter(text) => Line::from(vec![
            Span::styled("Filter: ", filter_style),
            Span::raw(text.as_str()),
            Span::styled("_", filter_style),
            Span::raw("  (Enter to confirm, Esc to cancel)"),
        ]),
        InputMode::Rename(text) => Line::from(vec![
            Span::styled("Rename: ", input_style),
            Span::raw(text.as_str()),
            Span::styled("_", input_style),
            Span::raw("  (Enter to confirm, Esc to cancel)"),
        ]),
        InputMode::MkDir(text) => Line::from(vec![
            Span::styled("New directory: ", input_style),
            Span::raw(text.as_str()),
            Span::styled("_", input_style),
            Span::raw("  (Enter to confirm, Esc to cancel)"),
        ]),
        InputMode::CreateFile(text) => Line::from(vec![
            Span::styled("New file: ", input_style),
            Span::raw(text.as_str()),
            Span::styled("_", input_style),
            Span::raw("  (Enter to confirm, Esc to cancel)"),
        ]),
        InputMode::Command(_) => Line::from(vec![
            Span::styled("Enter", hint_style),
            Span::raw(":Run  "),
            Span::styled("Esc", hint_style),
            Span::raw(":Cancel  "),
            Span::styled("cd <path>", Style::default().fg(theme.help_fg)),
            Span::raw(" to navigate, or any shell command"),
        ]),
    };

    let paragraph =
        Paragraph::new(line).style(Style::default().bg(theme.cmdbar_bg).fg(theme.cmdbar_fg));
    frame.render_widget(paragraph, area);
}
