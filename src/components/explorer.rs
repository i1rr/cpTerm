use std::collections::HashSet;
use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Row, Table, TableState};

use crate::action::Action;
use crate::fs::entry::{FileEntry, read_directory};
use crate::theme::Theme;
use crate::util::{format_date, format_size};

pub struct Explorer {
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub filtered: Vec<usize>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub selected: HashSet<usize>,
    pub filter_text: Option<String>,
}

impl Explorer {
    pub fn new(dir: PathBuf) -> Self {
        let mut explorer = Self {
            current_dir: dir,
            entries: Vec::new(),
            filtered: Vec::new(),
            cursor: 0,
            scroll_offset: 0,
            selected: HashSet::new(),
            filter_text: None,
        };
        explorer.refresh();
        explorer
    }

    pub fn refresh(&mut self) {
        self.entries = read_directory(&self.current_dir);
        self.selected.clear();
        self.apply_filter();
        self.clamp_cursor();
    }

    fn apply_filter(&mut self) {
        self.filtered = (0..self.entries.len())
            .filter(|&i| {
                let entry = &self.entries[i];
                if let Some(ref filter) = self.filter_text {
                    if !filter.is_empty() {
                        return entry
                            .name
                            .to_lowercase()
                            .contains(&filter.to_lowercase());
                    }
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

    /// Total display rows: ".." + filtered entries
    fn display_len(&self) -> usize {
        1 + self.filtered.len() // 1 for ".."
    }

    /// Get the FileEntry at the current cursor, if it's not ".."
    pub fn current_entry(&self) -> Option<&FileEntry> {
        if self.cursor == 0 {
            return None; // ".." row
        }
        self.filtered
            .get(self.cursor - 1)
            .map(|&i| &self.entries[i])
    }

    /// Get the actual entry index (into self.entries) for cursor position
    fn entry_index_at_cursor(&self) -> Option<usize> {
        if self.cursor == 0 {
            return None;
        }
        self.filtered.get(self.cursor - 1).copied()
    }

    #[allow(dead_code)]
    pub fn selected_entries(&self) -> Vec<&FileEntry> {
        self.selected
            .iter()
            .map(|&i| &self.entries[i])
            .collect()
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected
            .iter()
            .map(|&i| self.entries[i].path.clone())
            .collect()
    }

    pub fn selected_total_size(&self) -> u64 {
        self.selected.iter().map(|&i| self.entries[i].size).sum()
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
                    // ".." - go to parent
                    return Some(Action::ParentDir);
                }
                if let Some(entry) = self.current_entry() {
                    if entry.is_dir {
                        let path = entry.path.clone();
                        self.current_dir = path;
                        self.filter_text = None;
                        self.cursor = 0;
                        self.refresh();
                    } else {
                        return Some(Action::OpenFile);
                    }
                }
                None
            }
            Action::ParentDir => {
                if let Some(parent) = self.current_dir.parent() {
                    let prev_dir = self.current_dir.clone();
                    self.current_dir = parent.to_path_buf();
                    self.filter_text = None;
                    self.refresh();
                    // Try to place cursor on the dir we just left
                    if let Some(prev_name) = prev_dir.file_name() {
                        let prev_name = prev_name.to_string_lossy();
                        for (i, &idx) in self.filtered.iter().enumerate() {
                            if self.entries[idx].name == *prev_name {
                                self.cursor = i + 1; // +1 for ".."
                                break;
                            }
                        }
                    }
                }
                None
            }
            Action::ToggleSelect => {
                if let Some(idx) = self.entry_index_at_cursor() {
                    if self.selected.contains(&idx) {
                        self.selected.remove(&idx);
                    } else {
                        self.selected.insert(idx);
                    }
                }
                // Move down after toggle
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
            Action::Refresh => {
                self.refresh();
                None
            }
            // Filter actions
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
            Action::FilterConfirm => {
                // Keep filter active but exit filter input mode
                // (handled by app - filter_text stays)
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

        let title = self.current_dir.to_string_lossy().to_string();
        let max_title = (area.width as usize).saturating_sub(4);
        let title = if max_title < 3 {
            String::new()
        } else if title.len() > max_title {
            let keep = max_title.saturating_sub(2);
            // Find a safe char boundary for the suffix
            let start = title.len() - keep;
            let start = title.ceil_char_boundary(start);
            format!("..{}", &title[start..])
        } else {
            title
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(&title, border_style),
                Span::raw(" "),
            ]));

        // Build rows
        let mut rows = Vec::new();
        let inner_height = area.height.saturating_sub(2) as usize; // borders

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
            let cursor_pos = display_idx + 1; // +1 for ".."

            let is_cursor = cursor_pos == self.cursor && focused;
            let is_selected = self.selected.contains(&entry_idx);

            let name = if entry.is_dir {
                format!("{}\\", entry.name)
            } else {
                entry.name.clone()
            };

            let size = if entry.is_dir {
                "<DIR>".to_string()
            } else {
                format_size(entry.size)
            };

            let date = entry
                .modified
                .as_ref()
                .map(|d| format_date(d))
                .unwrap_or_default();

            let mut style = Style::default();
            if entry.is_dir {
                style = style.fg(theme.dir_fg).add_modifier(Modifier::BOLD);
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
            .row_highlight_style(Style::default()); // we handle highlight manually

        // We render with offset
        *table_state.offset_mut() = self.scroll_offset;
        frame.render_stateful_widget(table, area, &mut table_state);
    }
}
