use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{InputMode, SshConnectState};
use crate::task::TaskState;
use crate::theme::Theme;

pub fn draw_command_bar(
    frame: &mut Frame,
    area: Rect,
    input_mode: &InputMode,
    active_editor: bool,
    editor_fullscreen: bool,
    task: Option<&TaskState>,
    theme: &Theme,
) {
    let fkey_style = Style::default().fg(theme.fkey_fg).bg(theme.fkey_bg);
    let hint_style = Style::default().fg(theme.hint_fg);
    let input_style = Style::default().fg(theme.input_fg);
    let filter_style = Style::default().fg(theme.filter_fg);

    let line = match input_mode {
        InputMode::Normal => {
            if active_editor {
                if editor_fullscreen {
                    Line::from(vec![
                        Span::styled("Ctrl+S", hint_style),
                        Span::raw(":Save  "),
                        Span::styled("Esc", hint_style),
                        Span::raw(":Close  "),
                        Span::styled("Ctrl+E", hint_style),
                        Span::raw(":Exit full  "),
                        Span::styled("Alt+b/f", hint_style),
                        Span::raw(":word  "),
                        Span::styled("PgUp/Dn", hint_style),
                        Span::raw(":page  "),
                        Span::styled("Ctrl+Home/End", hint_style),
                        Span::raw(":top/bot"),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled("Ctrl+S", hint_style),
                        Span::raw(":Save  "),
                        Span::styled("Esc", hint_style),
                        Span::raw(":Close  "),
                        Span::styled("Ctrl+E", hint_style),
                        Span::raw(":Full  "),
                        Span::styled("Alt+b/f", hint_style),
                        Span::raw(":word  "),
                        Span::styled("PgUp/Dn", hint_style),
                        Span::raw(":page  "),
                        Span::styled("Ctrl+Home/End", hint_style),
                        Span::raw(":top/bot"),
                    ])
                }
            } else {
                let mut spans = vec![
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
                    Span::styled("F8", fkey_style),
                    Span::raw("Del  "),
                    Span::styled("Ctrl+F", hint_style),
                    Span::raw(":Filter  "),
                    Span::styled("Tab", hint_style),
                    Span::raw(":Switch  "),
                    Span::styled("Ctrl+O", hint_style),
                    Span::raw(":SSH  "),
                    Span::styled("Ctrl+T", hint_style),
                    Span::raw(":Theme  "),
                    Span::styled("Ctrl+Q", hint_style),
                    Span::raw(":Quit"),
                ];
                if let Some(t) = task {
                    let (label, color) = if t.running {
                        ("  [task running - Ctrl+Z to view]", theme.filter_fg)
                    } else if t.exit_code.unwrap_or(0) != 0 {
                        ("  [task failed - Ctrl+Z to view]", theme.error_border)
                    } else {
                        ("  [task done - Ctrl+Z to view]", theme.confirm_border)
                    };
                    spans.push(Span::styled(label, Style::default().fg(color)));
                }
                Line::from(spans)
            }
        }
        InputMode::TaskOutput => Line::from(vec![
            Span::styled("Up/Down", hint_style),
            Span::raw(":scroll  "),
            Span::styled("Ctrl+Z", hint_style),
            Span::raw(":minimize  "),
            Span::styled("Esc/Enter", hint_style),
            Span::raw(":close"),
        ]),
        InputMode::CreateTypeChoice => Line::from(vec![
            Span::styled("New: ", input_style),
            Span::styled("[f]", fkey_style),
            Span::raw("ile  "),
            Span::styled("[d]", fkey_style),
            Span::raw("ir   "),
            Span::styled("Esc", hint_style),
            Span::raw(":cancel"),
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
        InputMode::UnpackChoice { .. } => Line::from(vec![
            Span::styled("Unpack: ", input_style),
            Span::styled("[e]", fkey_style),
            Span::raw("xtract here  "),
            Span::styled("[f]", fkey_style),
            Span::raw("older  "),
            Span::styled("[c]", fkey_style),
            Span::raw("ustom path  "),
            Span::styled("Esc", hint_style),
            Span::raw(":cancel"),
        ]),
        InputMode::UnpackCustomPath { text, .. } => Line::from(vec![
            Span::styled("Extract to: ", input_style),
            Span::raw(text.as_str()),
            Span::styled("_", input_style),
            Span::raw("  (Enter to confirm, Esc to cancel)"),
        ]),
        InputMode::ContextMenu(_) => Line::from(vec![
            Span::styled("Up/Down", hint_style),
            Span::raw(":navigate  "),
            Span::styled("Enter", hint_style),
            Span::raw(":select  "),
            Span::styled("Esc", hint_style),
            Span::raw(":cancel"),
        ]),
        InputMode::Command(_) => Line::from(vec![
            Span::styled("Enter", hint_style),
            Span::raw(":Run  "),
            Span::styled("Esc", hint_style),
            Span::raw(":Cancel  "),
            Span::styled("cd <path>", Style::default().fg(theme.help_fg)),
            Span::raw(" to navigate, "),
            Span::styled("ssh user@host", Style::default().fg(theme.help_fg)),
            Span::raw(" to connect, or any shell command"),
        ]),
        InputMode::SshConnect(state) => {
            if state.connecting {
                Line::from(vec![Span::styled(
                    "Connecting...",
                    Style::default().fg(theme.filter_fg),
                )])
            } else {
                let field_label = SshConnectState::field_label(state.active_field);
                let display = if SshConnectState::is_password_field(state.active_field) {
                    "*".repeat(state.fields[state.active_field].len())
                } else {
                    state.fields[state.active_field].clone()
                };
                Line::from(vec![
                    Span::styled(format!("{}: ", field_label), input_style),
                    Span::raw(display),
                    Span::styled("_", input_style),
                    Span::raw("  "),
                    Span::styled("Tab/Up/Down", hint_style),
                    Span::raw(":field  "),
                    Span::styled("Enter", hint_style),
                    Span::raw(":connect  "),
                    Span::styled("Esc", hint_style),
                    Span::raw(":cancel"),
                ])
            }
        }
    };

    let paragraph =
        Paragraph::new(line).style(Style::default().bg(theme.cmdbar_bg).fg(theme.cmdbar_fg));
    frame.render_widget(paragraph, area);
}
