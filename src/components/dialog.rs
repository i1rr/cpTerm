use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

#[derive(Debug, Clone)]
pub enum DialogKind {
    Confirm {
        title: String,
        message: String,
    },
    Error(String),
    Info(String),
}

#[derive(Debug, Clone)]
pub struct Dialog {
    pub kind: DialogKind,
}

impl Dialog {
    pub fn confirm(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: DialogKind::Confirm {
                title: title.into(),
                message: message.into(),
            },
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            kind: DialogKind::Error(message.into()),
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self {
            kind: DialogKind::Info(message.into()),
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let popup_area = centered_rect(60, 30, area);
        frame.render_widget(Clear, popup_area);

        match &self.kind {
            DialogKind::Confirm { title, message } => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(Line::from(format!(" {} ", title)));

                let text = format!("{}\n\nEnter: Confirm  Esc: Cancel", message);
                let paragraph = Paragraph::new(text)
                    .block(block)
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::White));
                frame.render_widget(paragraph, popup_area);
            }
            DialogKind::Error(msg) => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .title(Line::from(" Error "));

                let text = format!("{}\n\nPress Esc to dismiss", msg);
                let paragraph = Paragraph::new(text)
                    .block(block)
                    .wrap(Wrap { trim: true })
                    .style(
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(Modifier::BOLD),
                    );
                frame.render_widget(paragraph, popup_area);
            }
            DialogKind::Info(msg) => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green))
                    .title(Line::from(" Info "));

                let text = format!("{}\n\nPress Esc to dismiss", msg);
                let paragraph = Paragraph::new(text)
                    .block(block)
                    .wrap(Wrap { trim: true })
                    .style(Style::default().fg(Color::Green));
                frame.render_widget(paragraph, popup_area);
            }
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
