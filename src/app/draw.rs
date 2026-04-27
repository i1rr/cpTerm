use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::components::command_bar::draw_command_bar;
use crate::components::status_bar::draw_status_bar;
use crate::components::task_window::draw_task_window;

use super::{App, ContextMenuState, InputMode, SshConnectState};

impl App {
    pub(super) fn draw(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(frame.area());

        self.dual_pane.draw(frame, chunks[0], &self.theme);
        draw_status_bar(
            frame,
            chunks[1],
            &self.dual_pane,
            &self.input_mode,
            self.connecting_to.as_deref(),
            &self.theme,
        );
        let active_editor = self.dual_pane.active_editor().is_some();
        let editor_fullscreen = self.dual_pane.editor_fullscreen;
        draw_command_bar(
            frame,
            chunks[2],
            &self.input_mode,
            active_editor,
            editor_fullscreen,
            self.task.as_ref(),
            &self.theme,
        );

        if let Some(ref mut panel) = self.bookmark_panel {
            panel.draw(frame, frame.area(), &self.bookmarks, &self.theme);
        }

        if let Some(ref mut editor) = self.theme_editor {
            editor.draw(frame, frame.area(), &self.theme, &self.theme_name);
        }

        if matches!(self.input_mode, InputMode::TaskOutput)
            && let Some(ref task) = self.task
        {
            draw_task_window(frame, frame.area(), task, &self.theme);
        }

        if let InputMode::ContextMenu(ref state) = self.input_mode {
            self.draw_context_menu(frame, frame.area(), state);
        }

        if let InputMode::SshConnect(ref state) = self.input_mode {
            self.draw_ssh_connect_dialog(frame, frame.area(), state);
        }

        if let Some(ref dialog) = self.dialog {
            dialog.draw(frame, frame.area(), &self.theme);
        }
    }

    pub(super) fn draw_ssh_connect_dialog(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &SshConnectState,
    ) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::Line;
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        let width = 50u16.min(area.width.saturating_sub(4));
        let height = 10u16.min(area.height.saturating_sub(2));
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_focused))
            .title(Line::from(" SSH Connect "));

        if state.connecting {
            let paragraph = Paragraph::new("Connecting...")
                .block(block)
                .style(Style::default().fg(self.theme.filter_fg));
            frame.render_widget(paragraph, popup);
            return;
        }

        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let field_names = ["Host", "Port", "User", "Path", "Password"];
        let mut lines: Vec<Line> = Vec::new();
        for (i, (label, value)) in field_names.iter().zip(state.fields.iter()).enumerate() {
            let display_value = if i == 4 && !value.is_empty() {
                "*".repeat(value.len())
            } else {
                value.clone()
            };
            let style = if i == state.active_field {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            lines.push(Line::styled(
                format!("{:>8}: {}", label, display_value),
                style,
            ));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Tab:next  Enter:connect  Esc:cancel",
            Style::default().fg(self.theme.hint_fg),
        ));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    pub(super) fn draw_context_menu(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &ContextMenuState,
    ) {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::Line;
        use ratatui::widgets::{Block, Borders, Clear, Paragraph};

        let item_count = state.items.len() as u16;
        let max_label = state
            .items
            .iter()
            .map(|i| i.label.len())
            .max()
            .unwrap_or(10);
        let width = (max_label as u16 + 4).min(area.width.saturating_sub(4));
        let height = (item_count + 2).min(area.height.saturating_sub(2)); // +2 for borders

        // Center the popup
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_focused))
            .title(Line::from(" Actions "));

        let lines: Vec<Line> = state
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == state.cursor {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else {
                    Style::default()
                };
                Line::styled(format!(" {} ", item.label), style)
            })
            .collect();

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(self.theme.info_fg));
        frame.render_widget(paragraph, popup);
    }
}
