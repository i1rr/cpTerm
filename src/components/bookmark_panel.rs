use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::bookmarks::BookmarkList;
use crate::theme::Theme;

pub struct BookmarkPanel {
    pub cursor: usize,
    /// Some(text) when in naming mode (adding a new bookmark).
    pub naming: Option<String>,
    scroll: usize,
}

impl BookmarkPanel {
    pub fn new(initial_cursor: usize) -> Self {
        Self {
            cursor: initial_cursor,
            naming: None,
            scroll: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_down(&mut self, count: usize) {
        if count > 0 && self.cursor + 1 < count {
            self.cursor += 1;
        }
    }

    /// Clamp cursor so it stays within the current entry count.
    pub fn clamp_cursor(&mut self, count: usize) {
        if count == 0 {
            self.cursor = 0;
        } else if self.cursor >= count {
            self.cursor = count - 1;
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, bookmarks: &BookmarkList, theme: &Theme) {
        let entries = &bookmarks.entries;
        let count = entries.len() as u16;
        // Panel height: entries + 2 borders, minimum 6, capped to available space.
        let panel_height = (count + 2).max(6).min(area.height.saturating_sub(4));
        let panel = centered_rect(70, panel_height, area);
        frame.render_widget(Clear, panel);

        let inner_height = panel_height.saturating_sub(2) as usize;

        // Scroll so cursor stays visible.
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        }
        if self.cursor >= self.scroll + inner_height {
            self.scroll = self.cursor - inner_height + 1;
        }

        let mut lines: Vec<Line> = Vec::new();

        if entries.is_empty() {
            lines.push(Line::raw(
                "  No bookmarks yet. Press Ctrl+D to add the current directory.",
            ));
        } else {
            for (i, entry) in entries.iter().enumerate() {
                let exists = entry.path.exists();
                let path_str = entry.path.to_string_lossy();

                if i == self.cursor {
                    let hl = Style::default().add_modifier(Modifier::REVERSED);
                    let dim_hl = Style::default().add_modifier(Modifier::REVERSED | Modifier::DIM);
                    lines.push(Line::from(vec![
                        Span::styled(format!(" > {:<20}", entry.name), hl),
                        if exists {
                            Span::styled(format!("  {}", path_str), hl)
                        } else {
                            Span::styled(format!("  {} (not found)", path_str), dim_hl)
                        },
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(format!("   {:<20}", entry.name)),
                        if exists {
                            Span::raw(format!("  {}", path_str))
                        } else {
                            Span::styled(
                                format!("  {} (not found)", path_str),
                                Style::default().add_modifier(Modifier::DIM),
                            )
                        },
                    ]));
                }
            }
        }

        let bottom_line = if let Some(ref name_text) = self.naming {
            Line::from(vec![
                Span::styled(" Name: ", Style::default().fg(theme.input_fg)),
                Span::raw(name_text.as_str()),
                Span::styled("\u{2588}", Style::default().fg(theme.input_fg)),
                Span::raw("  Enter:Save  Esc:Cancel "),
            ])
        } else {
            Line::raw(" Enter:Go  Ctrl+D:Add  Shift+Del:Remove  Esc:Close ")
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_focused))
            .title(Line::from(" Bookmarks "))
            .title_bottom(bottom_line);

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
