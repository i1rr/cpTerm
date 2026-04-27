use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::theme::Theme;

#[derive(Debug, Clone)]
pub enum DialogKind {
    Confirm { title: String, message: String },
    CloseEditorConfirm { title: String, message: String },
    Conflict { title: String, message: String },
    Error(String),
    Info(String),
}

#[derive(Debug, Clone)]
pub struct Dialog {
    pub kind: DialogKind,
    pub scroll: u16,
}

impl Dialog {
    pub fn confirm(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: DialogKind::Confirm {
                title: title.into(),
                message: message.into(),
            },
            scroll: 0,
        }
    }

    pub fn close_editor_confirm(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: DialogKind::CloseEditorConfirm {
                title: title.into(),
                message: message.into(),
            },
            scroll: 0,
        }
    }

    pub fn conflict(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: DialogKind::Conflict {
                title: title.into(),
                message: message.into(),
            },
            scroll: 0,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            kind: DialogKind::Error(message.into()),
            scroll: 0,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self {
            kind: DialogKind::Info(message.into()),
            scroll: 0,
        }
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let (text, block) = match &self.kind {
            DialogKind::Confirm { title, message } => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.confirm_border))
                    .title(Line::from(format!(" {} ", title)));
                let text = format!("{}\n\nEnter: Confirm  Esc: Cancel", message);
                (text, block)
            }
            DialogKind::CloseEditorConfirm { title, message } => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.confirm_border))
                    .title(Line::from(format!(" {} ", title)));
                let text = format!(
                    "{}\n\nEnter: Save and close  D: Discard  Esc: Cancel",
                    message
                );
                (text, block)
            }
            DialogKind::Conflict { title, message } => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.conflict_border))
                    .title(Line::from(format!(" {} - Conflict ", title)));
                (message.clone(), block)
            }
            DialogKind::Error(msg) => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.error_border))
                    .title(Line::from(" Error "));
                let text = format!("{}\n\nPress Esc to dismiss", msg);
                (text, block)
            }
            DialogKind::Info(msg) => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.info_border))
                    .title(Line::from(" Info "));
                let hint = if self.scroll > 0 || self.content_lines(msg) > 10 {
                    "Up/Down: scroll  Esc: dismiss"
                } else {
                    "Press Esc to dismiss"
                };
                let text = format!("{}\n\n{}", msg, hint);
                (text, block)
            }
        };

        let content_lines = text.lines().count() as u16;
        // Height: content lines + 2 for borders, capped to 80% of terminal
        let max_height = (area.height as f32 * 0.8) as u16;
        let desired_height = (content_lines + 2).min(max_height).max(5);

        let popup_area = centered_rect_with_height(60, desired_height, area);
        frame.render_widget(Clear, popup_area);

        let style = match &self.kind {
            DialogKind::Confirm { .. } | DialogKind::CloseEditorConfirm { .. } => {
                Style::default().fg(theme.confirm_fg)
            }
            DialogKind::Conflict { .. } => Style::default().fg(theme.conflict_fg),
            DialogKind::Error(_) => Style::default()
                .fg(theme.error_fg)
                .add_modifier(Modifier::BOLD),
            DialogKind::Info(_) => Style::default().fg(theme.info_fg),
        };

        let paragraph = Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: true })
            .scroll((self.scroll, 0))
            .style(style);
        frame.render_widget(paragraph, popup_area);
    }

    fn content_lines(&self, msg: &str) -> u16 {
        msg.lines().count() as u16
    }
}

fn centered_rect_with_height(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
