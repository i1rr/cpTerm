use std::fs;
use std::io::{Read, Seek, Write};
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
    original: String,
    /// Directory from which the editor was opened - restored when editor is closed.
    pub origin_dir: PathBuf,
}

impl EditorPane {
    pub fn open(path: PathBuf, origin_dir: PathBuf, read_only: bool) -> Result<Self, String> {
        const LIMIT: u64 = 8 * 1024 * 1024;
        let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() > LIMIT {
            return Err(
                "The editor accepts regular text files up to 8 MiB; use F3 for larger files".into(),
            );
        }
        let mut content = String::new();
        let read = fs::File::open(&path)
            .and_then(|f| f.take(LIMIT + 1).read_to_string(&mut content))
            .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;
        if read as u64 > LIMIT {
            return Err("File exceeds the 8 MiB editor limit".into());
        }

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
            original: content,
            origin_dir,
        })
    }

    /// Write the textarea contents back to the file.
    pub fn save(&mut self) -> Result<(), String> {
        if self.read_only {
            return Err("File is read-only".to_string());
        }
        let mut old_lines = self.original.split_inclusive('\n');
        let mut ending = self
            .original
            .split_inclusive('\n')
            .next()
            .map_or("\n", |s| if s.ends_with("\r\n") { "\r\n" } else { "\n" });
        let mut content = String::new();
        for (index, line) in self.textarea.lines().iter().enumerate() {
            if index > 0 {
                if let Some(previous) = old_lines.next() {
                    if previous.ends_with("\r\n") {
                        ending = "\r\n";
                    } else if previous.ends_with('\n') {
                        ending = "\n";
                    }
                }
                content.push_str(ending);
            }
            content.push_str(line);
        }
        if content != self.original {
            // Compare and update the same open file. Preserve permissions, hard links
            // and other metadata, and never recreate a file deleted by someone else.
            let mut file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.path)
                .map_err(|e| format!("Cannot save '{}': {e}", self.path.display()))?;
            let mut current = Vec::new();
            (&mut file)
                .take(self.original.len() as u64 + 1)
                .read_to_end(&mut current)
                .map_err(|e| e.to_string())?;
            if current != self.original.as_bytes() {
                return Err("File changed on disk; reopen it before saving".into());
            }
            file.rewind()
                .and_then(|()| file.write_all(content.as_bytes()))
                .and_then(|()| file.set_len(content.len() as u64))
                .map_err(|e| e.to_string())?;
            self.original = content;
        }
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
                    if matches!(
                        key.code,
                        KeyCode::Left
                            | KeyCode::Right
                            | KeyCode::Up
                            | KeyCode::Down
                            | KeyCode::Home
                            | KeyCode::End
                            | KeyCode::PageUp
                            | KeyCode::PageDown
                    ) {
                        self.textarea.input(key);
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_preserve_mixed_line_endings_and_reject_external_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("example.txt");
        fs::write(&path, "one\r\ntwo\n").unwrap();
        let mut editor = EditorPane::open(path.clone(), dir.path().into(), false).unwrap();
        editor.textarea.insert_str("new ");
        editor.save().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new one\r\ntwo\n");
        editor.textarea.insert_str("unsaved");
        fs::write(&path, "external edit").unwrap();
        assert!(editor.save().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external edit");
    }
}
