use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::task::TaskState;
use crate::theme::Theme;

pub fn draw_task_window(frame: &mut Frame, area: Rect, task: &TaskState, theme: &Theme) {
    let height = ((area.height as f32 * 0.8) as u16).max(6);
    let popup_area = centered_rect(80, height, area);
    frame.render_widget(Clear, popup_area);

    let title = format!(" {} - {} ", task.cmd, task.exit_summary());
    let border_color = if task.running {
        theme.info_border
    } else if task.exit_code.unwrap_or(0) != 0 {
        theme.error_border
    } else {
        theme.confirm_border
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(title));

    let hint = if task.running {
        "Ctrl+Z: minimize  Esc: minimize"
    } else {
        "Up/Down: scroll  Esc/Enter: close"
    };

    let content = if task.lines.is_empty() {
        "(no output yet)".to_string()
    } else {
        task.lines.join("\n")
    };
    let text = format!("{}\n\n{}", content, hint);

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((task.scroll, 0))
        .style(Style::default().fg(theme.info_fg));
    frame.render_widget(paragraph, popup_area);
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
