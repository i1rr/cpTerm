use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::fs::drives::DriveEntry;
use crate::theme::Theme;

pub struct DrivesPanel {
    pub cursor: usize,
    pub drives: Vec<DriveEntry>,
    scroll: usize,
}

impl DrivesPanel {
    pub fn new(drives: Vec<DriveEntry>) -> Self {
        Self {
            cursor: 0,
            drives,
            scroll: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.drives.len() {
            self.cursor += 1;
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let count = self.drives.len() as u16;
        let panel_height = (count + 2).max(6).min(area.height.saturating_sub(4));
        let panel = centered_rect(70, panel_height, area);
        frame.render_widget(Clear, panel);

        let inner_height = panel_height.saturating_sub(2) as usize;
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        }
        if self.cursor >= self.scroll + inner_height {
            self.scroll = self.cursor - inner_height + 1;
        }

        let mut lines: Vec<Line> = Vec::new();
        if self.drives.is_empty() {
            lines.push(Line::raw("  No drives or mounts detected."));
        } else {
            for (i, drive) in self.drives.iter().enumerate() {
                let path_str = drive.path.to_string_lossy();
                let kind = drive.kind.as_label();
                let is_cursor = i == self.cursor;
                let style = if is_cursor {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                let prefix = if is_cursor { " > " } else { "   " };
                lines.push(Line::from(vec![Span::styled(
                    format!("{}{:<24} [{:<9}]  {}", prefix, drive.label, kind, path_str),
                    style,
                )]));
            }
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_focused))
            .title(Line::from(" Drives "))
            .title_bottom(Line::raw(" Enter:Go  Esc:Close "));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((self.scroll as u16, 0));
        frame.render_widget(paragraph, panel);
    }
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
