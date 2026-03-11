use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use tokio::sync::mpsc;

use crate::action::Action;
use crate::components::command_bar::draw_command_bar;
use crate::components::dialog::Dialog;
use crate::components::dual_pane::DualPane;
use crate::components::status_bar::draw_status_bar;
use crate::config::{PaneSide, SessionConfig};
use crate::event::{Event, EventHandler};
use crate::fs::ops;
use crate::tui;

#[derive(Debug, Clone)]
pub enum InputMode {
    Normal,
    Filter(String),
    Rename(String),
    MkDir(String),
}

enum PendingOp {
    Copy(Vec<PathBuf>, PathBuf),
    Move(Vec<PathBuf>, PathBuf),
    Delete(Vec<PathBuf>),
}

pub struct App {
    pub dual_pane: DualPane,
    pub input_mode: InputMode,
    pub dialog: Option<Dialog>,
    pub should_quit: bool,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    pending_op: Option<PendingOp>,
}

impl App {
    pub fn new(left_dir: PathBuf, right_dir: PathBuf, active: PaneSide) -> Self {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        Self {
            dual_pane: DualPane::new(left_dir, right_dir, active),
            input_mode: InputMode::Normal,
            dialog: None,
            should_quit: false,
            action_tx,
            action_rx,
            pending_op: None,
        }
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        let mut terminal = tui::init()?;
        let mut events = EventHandler::new(std::time::Duration::from_millis(250));

        loop {
            // Draw
            terminal.draw(|frame| self.draw(frame))?;

            // Handle events
            tokio::select! {
                event = events.next() => {
                    if let Some(event) = event {
                        let action = self.map_event(event);
                        self.dispatch(action);
                    }
                }
                action = self.action_rx.recv() => {
                    if let Some(action) = action {
                        self.dispatch(action);
                    }
                }
            }

            if self.should_quit {
                events.stop();
                break;
            }
        }

        tui::restore()?;
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),    // dual pane
                Constraint::Length(1), // status bar
                Constraint::Length(1), // command bar
            ])
            .split(frame.area());

        self.dual_pane.draw(frame, chunks[0]);
        draw_status_bar(frame, chunks[1], &self.dual_pane);
        draw_command_bar(frame, chunks[2], &self.input_mode);

        if let Some(ref dialog) = self.dialog {
            dialog.draw(frame, frame.area());
        }
    }

    fn map_event(&self, event: Event) -> Action {
        match event {
            Event::Tick => Action::Tick,
            Event::Resize(w, h) => Action::Resize(w, h),
            Event::Key(key) => self.map_key(key),
        }
    }

    fn map_key(&self, key: KeyEvent) -> Action {
        // Dialog intercepts keys first
        if self.dialog.is_some() {
            return match key.code {
                KeyCode::Enter => Action::ConfirmDialog,
                KeyCode::Esc => Action::DismissDialog,
                KeyCode::Up => Action::DialogScrollUp,
                KeyCode::Down => Action::DialogScrollDown,
                _ => Action::Noop,
            };
        }

        // Input mode intercepts keys
        match &self.input_mode {
            InputMode::Filter(_) => {
                return match key.code {
                    KeyCode::Char(c) => Action::FilterInput(c),
                    KeyCode::Backspace => Action::FilterBackspace,
                    KeyCode::Enter => Action::FilterConfirm,
                    KeyCode::Esc => Action::FilterCancel,
                    _ => Action::Noop,
                };
            }
            InputMode::Rename(_) | InputMode::MkDir(_) => {
                return match key.code {
                    KeyCode::Char(c) => Action::InputChar(c),
                    KeyCode::Backspace => Action::InputBackspace,
                    KeyCode::Enter => Action::InputConfirm,
                    KeyCode::Esc => Action::InputCancel,
                    _ => Action::Noop,
                };
            }
            InputMode::Normal => {}
        }

        // Normal mode
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Up) => Action::MoveUp,
            (KeyModifiers::NONE, KeyCode::Down) => Action::MoveDown,
            (KeyModifiers::NONE, KeyCode::Home) => Action::MoveToTop,
            (KeyModifiers::NONE, KeyCode::End) => Action::MoveToBottom,
            (KeyModifiers::NONE, KeyCode::PageUp) => Action::PageUp,
            (KeyModifiers::NONE, KeyCode::PageDown) => Action::PageDown,
            (KeyModifiers::NONE, KeyCode::Enter) => Action::EnterDir,
            (KeyModifiers::NONE, KeyCode::Backspace) => Action::ParentDir,
            (KeyModifiers::NONE, KeyCode::Tab) => Action::SwitchPane,
            (KeyModifiers::NONE, KeyCode::Char(' ')) | (KeyModifiers::NONE, KeyCode::Insert) => {
                Action::ToggleSelect
            }
            (KeyModifiers::CONTROL, KeyCode::Char('a')) => Action::SelectAll,
            (KeyModifiers::NONE, KeyCode::F(1)) => Action::ShowHelp,
            (KeyModifiers::NONE, KeyCode::F(2)) => Action::Rename,
            (KeyModifiers::NONE, KeyCode::F(5)) => Action::CopySelected,
            (KeyModifiers::NONE, KeyCode::F(6)) => Action::MoveSelected,
            (KeyModifiers::NONE, KeyCode::F(7)) => Action::MkDir,
            (KeyModifiers::NONE, KeyCode::F(8)) | (KeyModifiers::NONE, KeyCode::Delete) => {
                Action::DeleteSelected
            }
            (KeyModifiers::CONTROL, KeyCode::Char('f')) => Action::StartFilter,
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => Action::Refresh,
            (KeyModifiers::CONTROL, KeyCode::Char('h')) => Action::ToggleHidden,
            (KeyModifiers::NONE, KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('q')) => Action::Quit,
            (KeyModifiers::NONE, KeyCode::Esc) => Action::FilterCancel,
            _ => Action::Noop,
        }
    }

    fn dispatch(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.save_session();
                self.should_quit = true;
            }
            Action::Noop | Action::Tick => {}
            Action::Resize(_, _) => {}
            Action::DismissDialog => {
                self.dialog = None;
            }
            Action::DialogScrollUp => {
                if let Some(ref mut dialog) = self.dialog {
                    dialog.scroll_up();
                }
            }
            Action::DialogScrollDown => {
                if let Some(ref mut dialog) = self.dialog {
                    dialog.scroll_down();
                }
            }
            Action::ConfirmDialog => {
                if let Some(op) = self.pending_op.take() {
                    let tx = self.action_tx.clone();
                    match op {
                        PendingOp::Copy(sources, dest) => {
                            tokio::spawn(ops::copy_entries(sources, dest, tx));
                        }
                        PendingOp::Move(sources, dest) => {
                            tokio::spawn(ops::move_entries(sources, dest, tx));
                        }
                        PendingOp::Delete(sources) => {
                            tokio::spawn(ops::delete_entries(sources, tx));
                        }
                    }
                }
                self.dialog = None;
            }
            Action::CopySelected => {
                let sources = self.get_operation_sources();
                if sources.is_empty() {
                    return;
                }
                let dest = self.dual_pane.inactive_dir();
                let count = sources.len();
                self.pending_op = Some(PendingOp::Copy(sources, dest.clone()));
                self.dialog = Some(Dialog::confirm(
                    "Copy",
                    format!(
                        "Copy {} item(s) to {}?",
                        count,
                        dest.display()
                    ),
                ));
            }
            Action::MoveSelected => {
                let sources = self.get_operation_sources();
                if sources.is_empty() {
                    return;
                }
                let dest = self.dual_pane.inactive_dir();
                let count = sources.len();
                self.pending_op = Some(PendingOp::Move(sources, dest.clone()));
                self.dialog = Some(Dialog::confirm(
                    "Move",
                    format!(
                        "Move {} item(s) to {}?",
                        count,
                        dest.display()
                    ),
                ));
            }
            Action::DeleteSelected => {
                let sources = self.get_operation_sources();
                if sources.is_empty() {
                    return;
                }
                let count = sources.len();
                self.pending_op = Some(PendingOp::Delete(sources));
                self.dialog = Some(Dialog::confirm(
                    "Delete",
                    format!("Delete {} item(s)?", count),
                ));
            }
            Action::OpenFile => {
                if let Some(entry) = self.dual_pane.active_explorer().current_entry() {
                    if !entry.is_dir {
                        if let Err(msg) = crate::fs::open::open_file(&entry.path) {
                            self.dialog = Some(Dialog::error(msg));
                        }
                    }
                }
            }
            Action::Rename => {
                if let Some(entry) = self.dual_pane.active_explorer().current_entry() {
                    self.input_mode = InputMode::Rename(entry.name.clone());
                }
            }
            Action::MkDir => {
                self.input_mode = InputMode::MkDir(String::new());
            }
            Action::StartFilter => {
                self.input_mode = InputMode::Filter(String::new());
                self.dual_pane
                    .active_explorer_mut()
                    .handle_action(&Action::StartFilter);
            }
            Action::FilterInput(ch) => {
                if let InputMode::Filter(ref mut text) = self.input_mode {
                    text.push(ch);
                }
                self.dual_pane
                    .active_explorer_mut()
                    .handle_action(&Action::FilterInput(ch));
            }
            Action::FilterBackspace => {
                if let InputMode::Filter(ref mut text) = self.input_mode {
                    text.pop();
                }
                self.dual_pane
                    .active_explorer_mut()
                    .handle_action(&Action::FilterBackspace);
            }
            Action::FilterConfirm => {
                self.input_mode = InputMode::Normal;
            }
            Action::FilterCancel => {
                self.input_mode = InputMode::Normal;
                self.dual_pane
                    .active_explorer_mut()
                    .handle_action(&Action::FilterCancel);
            }
            Action::InputChar(ch) => match self.input_mode {
                InputMode::Rename(ref mut text) | InputMode::MkDir(ref mut text) => {
                    text.push(ch);
                }
                _ => {}
            },
            Action::InputBackspace => match self.input_mode {
                InputMode::Rename(ref mut text) | InputMode::MkDir(ref mut text) => {
                    text.pop();
                }
                _ => {}
            },
            Action::InputConfirm => {
                match self.input_mode.clone() {
                    InputMode::Rename(new_name) => {
                        self.do_rename(&new_name);
                    }
                    InputMode::MkDir(name) => {
                        self.do_mkdir(&name);
                    }
                    _ => {}
                }
                self.input_mode = InputMode::Normal;
            }
            Action::InputCancel => {
                self.input_mode = InputMode::Normal;
            }
            Action::OperationComplete(msg) => {
                self.dual_pane.left.refresh();
                self.dual_pane.right.refresh();
                self.dialog = Some(Dialog::info(msg));
            }
            Action::OperationError(msg) => {
                self.dual_pane.left.refresh();
                self.dual_pane.right.refresh();
                self.dialog = Some(Dialog::error(msg));
            }
            Action::OperationProgress { .. } => {
                // Could update a progress indicator in the future
            }
            Action::ShowHelp => {
                self.dialog = Some(Dialog::info(
                    "Shortcuts:\n\
                     Up/Down - navigate\n\
                     Home/End - top/bottom\n\
                     PgUp/PgDn - page scroll\n\
                     Enter - open dir/file\n\
                     Backspace - parent dir\n\
                     Tab - switch pane\n\
                     Space/Insert - toggle select\n\
                     Ctrl+A - select/deselect all\n\
                     F1 - this help\n\
                     F2 - rename\n\
                     F5 - copy to other pane\n\
                     F6 - move to other pane\n\
                     F7 - create directory\n\
                     F8/Del - delete\n\
                     Ctrl+F - quick filter\n\
                     Ctrl+H - toggle hidden\n\
                     Ctrl+R - refresh\n\
                     q/Ctrl+Q - quit",
                ));
            }
            Action::Error(msg) => {
                self.dialog = Some(Dialog::error(msg));
            }
            other => {
                if let Some(follow_up) = self.dual_pane.handle_action(&other) {
                    self.dispatch(follow_up);
                }
            }
        }
    }

    fn get_operation_sources(&self) -> Vec<PathBuf> {
        let explorer = self.dual_pane.active_explorer();
        let mut sources = explorer.selected_paths();
        if sources.is_empty() {
            // If nothing selected, use item under cursor
            if let Some(entry) = explorer.current_entry() {
                sources.push(entry.path.clone());
            }
        }
        sources
    }

    fn do_rename(&mut self, new_name: &str) {
        if new_name.is_empty() {
            return;
        }
        let explorer = self.dual_pane.active_explorer();
        if let Some(entry) = explorer.current_entry() {
            let old_path = entry.path.clone();
            let Some(parent) = old_path.parent() else {
                return;
            };
            let new_path = parent.join(new_name);
            if let Err(e) = std::fs::rename(&old_path, &new_path) {
                self.dialog = Some(Dialog::error(format!("Rename failed: {}", e)));
            }
            self.dual_pane.active_explorer_mut().refresh();
        }
    }

    fn do_mkdir(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        let dir = self.dual_pane.active_explorer().current_dir.clone();
        let new_dir = dir.join(name);
        if let Err(e) = std::fs::create_dir(&new_dir) {
            self.dialog = Some(Dialog::error(format!("Mkdir failed: {}", e)));
        }
        self.dual_pane.active_explorer_mut().refresh();
    }

    fn save_session(&self) {
        let config = SessionConfig {
            left_dir: self.dual_pane.left.current_dir.clone(),
            right_dir: self.dual_pane.right.current_dir.clone(),
            active_pane: self.dual_pane.active,
        };
        config.save();
    }
}
