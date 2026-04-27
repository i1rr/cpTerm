use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};
use ratatui_textarea::{CursorMove, TextArea};

use crate::action::Action;
use crate::theme::Theme;

pub struct EditorPane {
    pub path: PathBuf,
    pub textarea: TextArea<'static>,
    pub read_only: bool,
    pub modified: bool,
    /// Directory from which the editor was opened - restored when editor is closed.
    pub origin_dir: PathBuf,
}

impl EditorPane {
    pub fn open(path: PathBuf, origin_dir: PathBuf, read_only: bool) -> Result<Self, String> {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;

        // Split into lines preserving empty trailing lines
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            let mut ls: Vec<String> = content.lines().map(str::to_string).collect();
            // If file ends with newline, add an empty line so cursor can sit there
            if content.ends_with('\n') {
                ls.push(String::new());
            }
            ls
        };

        let mut textarea: TextArea<'static> = TextArea::new(lines);

        // Dim line numbers
        textarea.set_line_number_style(Style::default().add_modifier(Modifier::DIM));
        // No special highlighting on the cursor line itself
        textarea.set_cursor_line_style(Style::default());
        // Tab width = 4
        textarea.set_tab_length(4);

        Ok(Self {
            path,
            textarea,
            read_only,
            modified: false,
            origin_dir,
        })
    }

    /// Write the textarea contents back to the file.
    pub fn save(&mut self) -> Result<(), String> {
        if self.read_only {
            return Err("File is read-only".to_string());
        }
        let lines = self.textarea.lines();
        // Join lines with newline; strip the synthetic trailing empty line if present
        let content = if lines.last().map(|l| l.is_empty()).unwrap_or(false) && lines.len() > 1 {
            let joined = lines[..lines.len() - 1].join("\n");
            format!("{}\n", joined)
        } else {
            lines.join("\n")
        };
        fs::write(&self.path, content)
            .map_err(|e| format!("Cannot save '{}': {}", self.path.display(), e))?;
        self.modified = false;
        Ok(())
    }

    /// Process a key event from the app event loop.
    /// Returns `Some(Action)` for app-level actions; `None` if handled internally.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match (key.modifiers, key.code) {
            // Ctrl+S - save
            (KeyModifiers::CONTROL, KeyCode::Char('s')) => {
                return Some(Action::SaveEditor);
            }
            // Ctrl+Q or Esc - close editor
            (KeyModifiers::CONTROL, KeyCode::Char('q')) | (_, KeyCode::Esc) => {
                return Some(Action::CloseEditor);
            }
            // Tab - switch to other pane (Ctrl+I inserts a literal tab)
            (KeyModifiers::NONE, KeyCode::Tab) => {
                return Some(Action::SwitchPane);
            }
            // Ctrl+Home / Ctrl+End - jump to top/bottom of file
            (KeyModifiers::CONTROL, KeyCode::Home) => {
                self.textarea.move_cursor(CursorMove::Top);
                return None;
            }
            (KeyModifiers::CONTROL, KeyCode::End) => {
                self.textarea.move_cursor(CursorMove::Bottom);
                return None;
            }
            // Alt+Left / Alt+b - word jump back (Ctrl+Left/Right is taken by macOS)
            (KeyModifiers::ALT, KeyCode::Left) | (KeyModifiers::ALT, KeyCode::Char('b')) => {
                self.textarea.move_cursor(CursorMove::WordBack);
                return None;
            }
            // Alt+Right / Alt+f - word jump forward
            (KeyModifiers::ALT, KeyCode::Right) | (KeyModifiers::ALT, KeyCode::Char('f')) => {
                self.textarea.move_cursor(CursorMove::WordForward);
                return None;
            }
            _ => {
                if self.read_only {
                    // Read-only: allow navigation keys via textarea (scrolling)
                    self.textarea.input(key);
                } else {
                    let changed = self.textarea.input(key);
                    if changed {
                        self.modified = true;
                    }
                }
            }
        }
        None
    }

    /// Returns the current `(row, col)` cursor position (0-indexed).
    pub fn cursor(&self) -> (usize, usize) {
        self.textarea.cursor()
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, is_active: bool, theme: &Theme) {
        let border_style = if is_active {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_unfocused)
        };

        // Build title: filename [+] [RO]  line:col
        let filename = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned());

        let mut label = filename;
        if self.modified {
            label.push_str(" [+]");
        }
        if self.read_only {
            label.push_str(" [RO]");
        }

        let (row, col) = self.textarea.cursor();
        let pos = format!(" {}:{} ", row + 1, col + 1);

        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled(label, border_style),
            Span::styled(pos, Style::default().fg(theme.border_unfocused)),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title);

        // Render the border block first, then put the textarea inside it
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Toggle cursor visibility based on focus
        if is_active {
            self.textarea
                .set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        } else {
            // Hide cursor in inactive pane
            self.textarea.set_cursor_style(Style::default());
        }

        frame.render_widget(&self.textarea, inner);
    }
}
