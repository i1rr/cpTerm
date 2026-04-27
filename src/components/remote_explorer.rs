use std::collections::HashSet;
use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table, TableState};
use tokio::sync::Mutex;

use crate::action::Action;
use crate::ssh::{RemoteEntry, SshSession};
use crate::theme::Theme;
use crate::util::{format_date, format_size};

pub struct RemoteExplorer {
    pub session: Arc<Mutex<SshSession>>,
    pub current_path: String,
    pub entries: Vec<RemoteEntry>,
    pub filtered: Vec<usize>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub selected: HashSet<usize>,
    pub filter_text: Option<String>,
    pub loading: bool,
    pub host_label: String,
}

impl RemoteExplorer {
    pub fn new(
        session: Arc<Mutex<SshSession>>,
        initial_path: String,
        entries: Vec<RemoteEntry>,
        host_label: String,
    ) -> Self {
        let mut explorer = Self {
            session,
            current_path: initial_path,
            entries,
            filtered: Vec::new(),
            cursor: 0,
            scroll_offset: 0,
            selected: HashSet::new(),
            filter_text: None,
            loading: false,
            host_label,
        };
        explorer.apply_filter();
        explorer
    }

    fn apply_filter(&mut self) {
        self.filtered = (0..self.entries.len())
            .filter(|&i| {
                let entry = &self.entries[i];
                if let Some(ref filter) = self.filter_text
                    && !filter.is_empty()
                {
                    return entry.name.to_lowercase().contains(&filter.to_lowercase());
                }
                true
            })
            .collect();
    }

    fn clamp_cursor(&mut self) {
        let len = self.display_len();
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    fn display_len(&self) -> usize {
        1 + self.filtered.len() // 1 for ".."
    }

    pub fn current_entry(&self) -> Option<&RemoteEntry> {
        if self.cursor == 0 {
            return None;
        }
        self.filtered
            .get(self.cursor - 1)
            .map(|&i| &self.entries[i])
    }

    #[allow(dead_code)]
    pub fn selected_entries(&self) -> Vec<&RemoteEntry> {
        if self.selected.is_empty() {
            if let Some(entry) = self.current_entry() {
                return vec![entry];
            }
            return vec![];
        }
        self.selected.iter().map(|&i| &self.entries[i]).collect()
    }

    pub fn selected_remote_entries(&self) -> Vec<&RemoteEntry> {
        if self.selected.is_empty() {
            if let Some(e) = self.current_entry() {
                return vec![e];
            }
            return vec![];
        }
        self.selected.iter().map(|&i| &self.entries[i]).collect()
    }

    /// Returns the parent path string, or None if already at root.
    pub fn parent_path(&self) -> Option<String> {
        let path = self.current_path.trim_end_matches('/');
        if path.is_empty() {
            return None;
        }
        if let Some(pos) = path.rfind('/') {
            let parent = &path[..pos];
            if parent.is_empty() {
                Some("/".to_string())
            } else {
                Some(parent.to_string())
            }
        } else {
            None
        }
    }

    pub fn update_listing(&mut self, path: String, entries: Vec<RemoteEntry>) {
        self.current_path = path;
        self.entries = entries;
        self.cursor = 0;
        self.scroll_offset = 0;
        self.selected.clear();
        self.filter_text = None;
        self.loading = false;
        self.apply_filter();
    }

    fn entry_index_at_cursor(&self) -> Option<usize> {
        if self.cursor == 0 {
            return None;
        }
        self.filtered.get(self.cursor - 1).copied()
    }

    pub fn handle_action(&mut self, action: &Action) -> Option<Action> {
        match action {
            Action::MoveUp => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                None
            }
            Action::MoveDown => {
                if self.cursor + 1 < self.display_len() {
                    self.cursor += 1;
                }
                None
            }
            Action::MoveToTop => {
                self.cursor = 0;
                None
            }
            Action::MoveToBottom => {
                let len = self.display_len();
                if len > 0 {
                    self.cursor = len - 1;
                }
                None
            }
            Action::PageUp => {
                self.cursor = self.cursor.saturating_sub(20);
                None
            }
            Action::PageDown => {
                let len = self.display_len();
                self.cursor = (self.cursor + 20).min(if len > 0 { len - 1 } else { 0 });
                None
            }
            Action::EnterDir => {
                if self.cursor == 0 {
                    // ".." row - go to parent
                    return Some(Action::ParentDir);
                }
                if let Some(entry) = self.current_entry() {
                    if entry.is_dir {
                        let path = entry.remote_path.clone();
                        self.loading = true;
                        return Some(Action::RemoteNavigate(path));
                    } else {
                        return Some(Action::RemoteViewFile);
                    }
                }
                None
            }
            Action::ParentDir => {
                if let Some(parent) = self.parent_path() {
                    self.loading = true;
                    Some(Action::RemoteNavigate(parent))
                } else {
                    None
                }
            }
            Action::ToggleSelect => {
                if let Some(idx) = self.entry_index_at_cursor() {
                    if self.selected.contains(&idx) {
                        self.selected.remove(&idx);
                    } else {
                        self.selected.insert(idx);
                    }
                }
                if self.cursor + 1 < self.display_len() {
                    self.cursor += 1;
                }
                None
            }
            Action::SelectAll => {
                if self.selected.is_empty() {
                    for &i in &self.filtered {
                        self.selected.insert(i);
                    }
                } else {
                    self.selected.clear();
                }
                None
            }
            Action::DeselectAll => {
                self.selected.clear();
                None
            }
            Action::StartFilter => {
                self.filter_text = Some(String::new());
                None
            }
            Action::FilterInput(ch) => {
                if let Some(ref mut filter) = self.filter_text {
                    filter.push(*ch);
                    self.apply_filter();
                    self.cursor = 0;
                }
                None
            }
            Action::FilterBackspace => {
                if let Some(ref mut filter) = self.filter_text {
                    filter.pop();
                    self.apply_filter();
                    self.cursor = 0;
                }
                None
            }
            Action::FilterCancel => {
                self.filter_text = None;
                self.apply_filter();
                self.clamp_cursor();
                None
            }
            _ => None,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, focused: bool, theme: &Theme) {
        let border_style = if focused {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_unfocused)
        };

        let title_str = format!("{}{}", self.host_label, self.current_path);
        let max_title = (area.width as usize).saturating_sub(4);
        let title = if max_title < 3 {
            String::new()
        } else if title_str.len() > max_title {
            let keep = max_title.saturating_sub(2);
            let start = title_str.len().saturating_sub(keep);
            format!("..{}", &title_str[start..])
        } else {
            title_str
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(&title, border_style),
                Span::raw(" "),
            ]));

        if self.loading {
            let paragraph = Paragraph::new("Loading...")
                .block(block)
                .style(Style::default().fg(theme.filter_fg));
            frame.render_widget(paragraph, area);
            return;
        }

        let mut rows = Vec::new();
        let inner_height = area.height.saturating_sub(2) as usize;

        // ".." row
        let dotdot_style = if self.cursor == 0 && focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        rows.push(
            Row::new(vec!["..".to_string(), String::new(), String::new()]).style(dotdot_style),
        );

        // Entry rows
        for (display_idx, &entry_idx) in self.filtered.iter().enumerate() {
            let entry = &self.entries[entry_idx];
            let cursor_pos = display_idx + 1;

            let is_cursor = cursor_pos == self.cursor && focused;
            let is_selected = self.selected.contains(&entry_idx);

            let name = if entry.is_dir {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };

            let size = if entry.is_dir {
                "<DIR>".to_string()
            } else {
                format_size(entry.size)
            };

            let date = entry.modified.as_ref().map(format_date).unwrap_or_default();

            let mut style = Style::default();
            if entry.is_dir {
                style = style.fg(theme.new_file_fg).add_modifier(Modifier::BOLD);
            }
            if entry.is_hidden {
                style = style.add_modifier(Modifier::DIM);
            }
            if is_selected {
                style = style.bg(theme.selected_bg).fg(theme.selected_fg);
            }
            if is_cursor {
                style = style.add_modifier(Modifier::REVERSED);
            }

            rows.push(Row::new(vec![name, size, date]).style(style));
        }

        // Adjust scroll
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        }
        if self.cursor >= self.scroll_offset + inner_height {
            self.scroll_offset = self.cursor - inner_height + 1;
        }

        let widths = [
            ratatui::layout::Constraint::Min(10),
            ratatui::layout::Constraint::Length(10),
            ratatui::layout::Constraint::Length(12),
        ];

        let header = Row::new(vec!["Name", "Size", "Date"])
            .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED));

        let mut table_state = TableState::default();
        table_state.select(Some(self.cursor));

        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .row_highlight_style(Style::default());

        *table_state.offset_mut() = self.scroll_offset;
        frame.render_stateful_widget(table, area, &mut table_state);
    }
}
