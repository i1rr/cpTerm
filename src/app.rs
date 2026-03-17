use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use tokio::sync::mpsc;

use crate::action::Action;
use crate::bookmarks::BookmarkList;
use crate::components::bookmark_panel::BookmarkPanel;
use crate::components::command_bar::draw_command_bar;
use crate::components::dialog::Dialog;
use crate::components::dual_pane::DualPane;
use crate::components::status_bar::draw_status_bar;
use crate::components::task_window::draw_task_window;
use crate::components::theme_editor::ThemeEditor;
use crate::config::{PaneSide, SessionConfig};
use crate::event::{Event, EventHandler};
use crate::fs::ops;
use crate::task::{self, TaskState};
use crate::theme::Theme;
use crate::tui;
use crate::util::clean_canonicalize;

#[derive(Debug, Clone)]
pub enum InputMode {
    Normal,
    Filter(String),
    Rename(String),
    MkDir(String),
    CreateFile(String),
    CreateTypeChoice,
    Command(String),
    TaskOutput,
    ContextMenu(ContextMenuState),
}

#[derive(Debug, Clone)]
pub struct ContextMenuState {
    pub items: Vec<ContextMenuItem>,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub action: Action,
}

enum PendingOp {
    Copy { pairs: Vec<(PathBuf, PathBuf)> },
    Move { pairs: Vec<(PathBuf, PathBuf)> },
    Delete(Vec<PathBuf>),
    CloseEditor,
}

pub struct App {
    pub dual_pane: DualPane,
    pub input_mode: InputMode,
    pub dialog: Option<Dialog>,
    pub should_quit: bool,
    pub theme: Theme,
    pub theme_name: String,
    pub theme_editor: Option<ThemeEditor>,
    pub bookmark_panel: Option<BookmarkPanel>,
    pub bookmarks: BookmarkList,
    pub task: Option<TaskState>,
    action_tx: mpsc::UnboundedSender<Action>,
    action_rx: mpsc::UnboundedReceiver<Action>,
    pending_op: Option<PendingOp>,
    /// Path to open in $PAGER after the next render cycle (requires TUI suspend).
    viewer_request: Option<PathBuf>,
}

impl App {
    pub fn new(
        left_dir: PathBuf,
        right_dir: PathBuf,
        active: PaneSide,
        theme_name: String,
    ) -> Self {
        let (action_tx, action_rx) = mpsc::unbounded_channel();
        let theme = Theme::by_name(&theme_name);
        Self {
            dual_pane: DualPane::new(left_dir, right_dir, active),
            input_mode: InputMode::Normal,
            dialog: None,
            should_quit: false,
            theme,
            theme_name,
            theme_editor: None,
            bookmark_panel: None,
            bookmarks: BookmarkList::load(),
            task: None,
            action_tx,
            action_rx,
            pending_op: None,
            viewer_request: None,
        }
    }

    pub async fn run(&mut self) -> color_eyre::Result<()> {
        let mut terminal = tui::init()?;
        let mut events = EventHandler::new(std::time::Duration::from_millis(250));

        loop {
            terminal.draw(|frame| self.draw(frame))?;

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

            // Handle viewer requests - requires temporarily releasing the terminal.
            if let Some(path) = self.viewer_request.take() {
                tui::suspend()?;
                if let Err(msg) = crate::fs::open::open_in_viewer(&path) {
                    self.dialog = Some(Dialog::error(msg));
                }
                tui::resume(&mut terminal)?;
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

        if let Some(ref dialog) = self.dialog {
            dialog.draw(frame, frame.area(), &self.theme);
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
        // Dialog intercepts first
        if self.dialog.is_some() {
            return match key.code {
                KeyCode::Enter => Action::ConfirmDialog,
                KeyCode::Esc => Action::DismissDialog,
                KeyCode::Up => Action::DialogScrollUp,
                KeyCode::Down => Action::DialogScrollDown,
                KeyCode::Char('o') | KeyCode::Char('O') => Action::ConflictOverwrite,
                KeyCode::Char('r') | KeyCode::Char('R') => Action::ConflictRename,
                KeyCode::Char('d') | KeyCode::Char('D') => Action::DiscardAndCloseEditor,
                _ => Action::Noop,
            };
        }

        // Bookmark panel intercepts
        if let Some(ref panel) = self.bookmark_panel {
            return self.map_bookmark_panel_key(key, panel);
        }

        // Theme editor intercepts (with sub-states)
        if let Some(ref editor) = self.theme_editor {
            return self.map_theme_editor_key(key, editor);
        }

        // Input mode intercepts
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
            InputMode::CreateTypeChoice => {
                return match key.code {
                    KeyCode::Char('f') | KeyCode::Char('F') => Action::CreateFile,
                    KeyCode::Char('d') | KeyCode::Char('D') => Action::MkDir,
                    KeyCode::Esc => Action::InputCancel,
                    _ => Action::Noop,
                };
            }
            InputMode::Rename(_) | InputMode::MkDir(_) | InputMode::CreateFile(_) => {
                return match key.code {
                    KeyCode::Char(c) => Action::InputChar(c),
                    KeyCode::Backspace => Action::InputBackspace,
                    KeyCode::Enter => Action::InputConfirm,
                    KeyCode::Esc => Action::InputCancel,
                    _ => Action::Noop,
                };
            }
            InputMode::Command(_) => {
                return match key.code {
                    KeyCode::Char(c) => Action::InputChar(c),
                    KeyCode::Backspace => Action::InputBackspace,
                    KeyCode::Enter => Action::InputConfirm,
                    KeyCode::Esc => Action::InputCancel,
                    _ => Action::Noop,
                };
            }
            InputMode::TaskOutput => {
                return match (key.modifiers, key.code) {
                    (KeyModifiers::NONE, KeyCode::Up) => Action::TaskScrollUp,
                    (KeyModifiers::NONE, KeyCode::Down) => Action::TaskScrollDown,
                    (KeyModifiers::CONTROL, KeyCode::Char('z')) => Action::TaskMinimize,
                    (KeyModifiers::NONE, KeyCode::Esc) | (KeyModifiers::NONE, KeyCode::Enter) => {
                        Action::TaskDismiss
                    }
                    _ => Action::Noop,
                };
            }
            InputMode::ContextMenu(state) => {
                return match key.code {
                    KeyCode::Up => Action::MoveUp,
                    KeyCode::Down => Action::MoveDown,
                    KeyCode::Enter => {
                        if let Some(item) = state.items.get(state.cursor) {
                            item.action.clone()
                        } else {
                            Action::Noop
                        }
                    }
                    KeyCode::Esc => Action::DismissDialog,
                    _ => Action::Noop,
                };
            }
            InputMode::Normal => {}
        }

        // If the active pane is an editor, intercept the fullscreen toggle before
        // routing everything else into the editor.
        if self.dual_pane.active_editor().is_some() {
            if let (KeyModifiers::CONTROL, KeyCode::Char('e')) = (key.modifiers, key.code) {
                return Action::ToggleEditorFullscreen;
            }
            return Action::EditorKeyInput(key);
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
            (KeyModifiers::CONTROL, KeyCode::Char('z')) => Action::TaskRestore,
            (KeyModifiers::NONE, KeyCode::F(1)) => Action::ShowHelp,
            (KeyModifiers::NONE, KeyCode::F(2)) => Action::Rename,
            (KeyModifiers::NONE, KeyCode::F(3)) => Action::ViewFile,
            (KeyModifiers::NONE, KeyCode::F(4)) => Action::CreateNew,
            (KeyModifiers::NONE, KeyCode::F(5)) => Action::CopySelected,
            (KeyModifiers::NONE, KeyCode::F(6)) => Action::MoveSelected,
            (KeyModifiers::NONE, KeyCode::F(8)) | (KeyModifiers::NONE, KeyCode::Delete) => {
                Action::DeleteSelected
            }
            (KeyModifiers::CONTROL, KeyCode::Char('b')) => Action::OpenBookmarks,
            (KeyModifiers::CONTROL, KeyCode::Char('f')) => Action::StartFilter,
            (KeyModifiers::CONTROL, KeyCode::Char('t')) => Action::OpenThemeEditor,
            (KeyModifiers::CONTROL, KeyCode::Char('r')) => Action::Refresh,
            (KeyModifiers::CONTROL, KeyCode::Char('q')) => Action::Quit,
            (KeyModifiers::NONE, KeyCode::Esc) => Action::FilterCancel,
            (KeyModifiers::SHIFT, KeyCode::Enter) => Action::OpenFile,
            (KeyModifiers::CONTROL, KeyCode::Enter) => Action::OpenContextMenu,
            (KeyModifiers::NONE, KeyCode::Char(c)) => Action::StartCommand(c),
            (KeyModifiers::SHIFT, KeyCode::Char(c)) => Action::StartCommand(c),
            _ => Action::Noop,
        }
    }

    fn map_theme_editor_key(&self, key: KeyEvent, editor: &ThemeEditor) -> Action {
        // Naming mode: text input
        if editor.naming.is_some() {
            return match key.code {
                KeyCode::Char(c) => Action::InputChar(c),
                KeyCode::Backspace => Action::InputBackspace,
                KeyCode::Enter => Action::InputConfirm,
                KeyCode::Esc => Action::InputCancel,
                _ => Action::Noop,
            };
        }

        // Color picker mode
        if editor.picker.is_some() {
            return match key.code {
                KeyCode::Up => Action::MoveUp,
                KeyCode::Down => Action::MoveDown,
                KeyCode::Left => Action::ThemeEditorPickerLeft,
                KeyCode::Right => Action::ThemeEditorPickerRight,
                KeyCode::Enter => Action::InputConfirm,
                KeyCode::Esc => Action::InputCancel,
                _ => Action::Noop,
            };
        }

        // Normal editor mode
        match key.code {
            KeyCode::Up => Action::MoveUp,
            KeyCode::Down => Action::MoveDown,
            KeyCode::Home => Action::MoveToTop,
            KeyCode::End => Action::MoveToBottom,
            KeyCode::PageUp => Action::PageUp,
            KeyCode::PageDown => Action::PageDown,
            KeyCode::Enter => Action::ThemeEditorOpenPicker,
            KeyCode::Tab => Action::ThemeEditorCycleBase,
            KeyCode::F(2) => Action::ThemeEditorSave,
            KeyCode::Char('n') => Action::ThemeEditorSaveAs,
            KeyCode::Char('e') => Action::ThemeEditorOpenFile,
            KeyCode::Delete => Action::ThemeEditorDelete,
            KeyCode::Esc => Action::ThemeEditorClose,
            _ => Action::Noop,
        }
    }

    fn map_bookmark_panel_key(&self, key: KeyEvent, panel: &BookmarkPanel) -> Action {
        // Naming sub-state: text input for bookmark name.
        if panel.naming.is_some() {
            return match key.code {
                KeyCode::Char(c) => Action::InputChar(c),
                KeyCode::Backspace => Action::InputBackspace,
                KeyCode::Enter => Action::InputConfirm,
                KeyCode::Esc => Action::InputCancel,
                _ => Action::Noop,
            };
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Up) => Action::MoveUp,
            (KeyModifiers::NONE, KeyCode::Down) => Action::MoveDown,
            (KeyModifiers::NONE, KeyCode::Home) => Action::MoveToTop,
            (KeyModifiers::NONE, KeyCode::End) => Action::MoveToBottom,
            (KeyModifiers::NONE, KeyCode::Enter) => Action::BookmarkNavigate,
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => Action::BookmarkAdd,
            (KeyModifiers::SHIFT, KeyCode::Delete) => Action::BookmarkRemove,
            (KeyModifiers::NONE, KeyCode::Esc) => Action::BookmarkClose,
            _ => Action::Noop,
        }
    }

    fn dispatch(&mut self, action: Action) {
        // Task stream actions are processed regardless of any modal state so that
        // background output keeps flowing while theme editor or dialogs are open.
        if let Action::TaskLine(ref line) = action {
            if let Some(ref mut task) = self.task {
                task.push_line(line.clone());
            }
            return;
        }
        if let Action::TaskComplete(code) = action {
            if let Some(ref mut task) = self.task {
                task.finish(code);
            }
            return;
        }
        if let Action::TaskError(ref msg) = action {
            if let Some(ref mut task) = self.task {
                task.push_line(format!("error: {}", msg));
                task.finish(-1);
            } else {
                self.dialog = Some(Dialog::error(msg));
            }
            return;
        }

        if self.bookmark_panel.is_some() && self.dispatch_bookmark_panel(&action) {
            return;
        }

        if self.theme_editor.is_some() && self.dispatch_theme_editor(&action) {
            return;
        }

        // Context menu navigation - must absorb Tick/Noop/Resize to stay open
        if let InputMode::ContextMenu(ref mut state) = self.input_mode {
            match action {
                Action::Tick | Action::Noop | Action::Resize(_, _) => {
                    return;
                }
                Action::MoveUp => {
                    if state.cursor > 0 {
                        state.cursor -= 1;
                    }
                    return;
                }
                Action::MoveDown => {
                    if state.cursor + 1 < state.items.len() {
                        state.cursor += 1;
                    }
                    return;
                }
                Action::DismissDialog => {
                    self.input_mode = InputMode::Normal;
                    return;
                }
                // Any selected action - close menu and dispatch
                other => {
                    self.input_mode = InputMode::Normal;
                    return self.dispatch(other);
                }
            }
        }

        match action {
            Action::Quit => {
                self.save_session();
                self.should_quit = true;
            }
            Action::Noop | Action::Tick => {}
            Action::Resize(_, _) => {}
            Action::OpenBookmarks => {
                self.bookmark_panel = Some(BookmarkPanel::new(0));
            }
            Action::OpenThemeEditor => {
                self.theme_editor = Some(ThemeEditor::new());
            }
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
                    match op {
                        PendingOp::CloseEditor => {
                            // Save then close
                            let result = self
                                .dual_pane
                                .active_editor_mut()
                                .map(|e| e.save())
                                .unwrap_or(Ok(()));
                            if let Err(msg) = result {
                                self.dialog = Some(Dialog::error(msg));
                                return;
                            }
                            self.dual_pane.close_editor_in_active();
                        }
                        other => self.execute_op(other),
                    }
                }
                self.dialog = None;
            }
            Action::ConflictOverwrite => {
                if let Some(op) = self.pending_op.take() {
                    self.execute_op(op);
                    self.dialog = None;
                }
            }
            Action::ConflictRename => {
                if let Some(op) = self.pending_op.take() {
                    let op = match op {
                        PendingOp::Copy { mut pairs } => {
                            ops::rename_conflicts(&mut pairs);
                            PendingOp::Copy { pairs }
                        }
                        PendingOp::Move { mut pairs } => {
                            ops::rename_conflicts(&mut pairs);
                            PendingOp::Move { pairs }
                        }
                        other => other,
                    };
                    self.execute_op(op);
                    self.dialog = None;
                }
            }
            Action::CopySelected => {
                let sources = self.get_operation_sources();
                if sources.is_empty() {
                    return;
                }
                let dest = self.dual_pane.inactive_dir();
                let pairs = ops::build_pairs(&sources, &dest);
                let conflicts = ops::find_conflicts(&pairs);
                let count = pairs.len();
                self.pending_op = Some(PendingOp::Copy { pairs });
                if conflicts.is_empty() {
                    self.dialog = Some(Dialog::confirm(
                        "Copy",
                        format!("Copy {} item(s) to {}?", count, dest.display()),
                    ));
                } else {
                    self.dialog = Some(Dialog::conflict(
                        "Copy",
                        format!(
                            "{} of {} item(s) already exist in {}\n\n\
                             (O)verwrite  (R)ename  Esc: Cancel",
                            conflicts.len(),
                            count,
                            dest.display()
                        ),
                    ));
                }
            }
            Action::MoveSelected => {
                let sources = self.get_operation_sources();
                if sources.is_empty() {
                    return;
                }
                let dest = self.dual_pane.inactive_dir();
                let pairs = ops::build_pairs(&sources, &dest);
                let conflicts = ops::find_conflicts(&pairs);
                let count = pairs.len();
                self.pending_op = Some(PendingOp::Move { pairs });
                if conflicts.is_empty() {
                    self.dialog = Some(Dialog::confirm(
                        "Move",
                        format!("Move {} item(s) to {}?", count, dest.display()),
                    ));
                } else {
                    self.dialog = Some(Dialog::conflict(
                        "Move",
                        format!(
                            "{} of {} item(s) already exist in {}\n\n\
                             (O)verwrite  (R)ename  Esc: Cancel",
                            conflicts.len(),
                            count,
                            dest.display()
                        ),
                    ));
                }
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
                if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                    && !entry.is_dir
                    && let Err(msg) = crate::fs::open::open_file(&entry.path)
                {
                    self.dialog = Some(Dialog::error(msg));
                }
            }
            Action::OpenEditor { path } => {
                if let Err(msg) = self.dual_pane.open_editor_in_active(path) {
                    self.dialog = Some(Dialog::error(msg));
                }
            }
            Action::OpenAsText => {
                if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                    && !entry.is_dir
                {
                    let path = entry.path.clone();
                    if let Err(msg) = self.dual_pane.open_editor_in_active(path) {
                        self.dialog = Some(Dialog::error(msg));
                    }
                }
            }
            Action::OpenWithDefault => {
                if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                    && !entry.is_dir
                    && let Err(msg) = crate::fs::open::open_file(&entry.path)
                {
                    self.dialog = Some(Dialog::error(msg));
                }
            }
            Action::OpenContextMenu => {
                if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                {
                    let is_dir = entry.is_dir;
                    let is_archive = !is_dir && crate::util::is_archive(&entry.name);
                    let mut items = Vec::new();

                    if !is_dir {
                        items.push(ContextMenuItem {
                            label: "Open with OS default".to_string(),
                            action: Action::OpenWithDefault,
                        });
                    }
                    if !is_dir {
                        items.push(ContextMenuItem {
                            label: "Open as text in editor".to_string(),
                            action: Action::OpenAsText,
                        });
                    }
                    if !is_dir {
                        items.push(ContextMenuItem {
                            label: "View in pager (F3)".to_string(),
                            action: Action::ViewFile,
                        });
                    }
                    if is_archive {
                        items.push(ContextMenuItem {
                            label: "Unpack archive".to_string(),
                            action: Action::UnpackArchive,
                        });
                    }

                    if !items.is_empty() {
                        self.input_mode =
                            InputMode::ContextMenu(ContextMenuState { items, cursor: 0 });
                    }
                }
            }
            Action::CloseEditor => {
                let modified = self
                    .dual_pane
                    .active_editor()
                    .map(|e| e.modified)
                    .unwrap_or(false);
                if modified {
                    self.pending_op = Some(PendingOp::CloseEditor);
                    self.dialog = Some(Dialog::close_editor_confirm(
                        "Close editor",
                        "File has unsaved changes.",
                    ));
                } else {
                    self.dual_pane.close_editor_in_active();
                }
            }
            Action::SaveEditor => {
                let result = self
                    .dual_pane
                    .active_editor_mut()
                    .map(|e| e.save())
                    .unwrap_or(Ok(()));
                if let Err(msg) = result {
                    self.dialog = Some(Dialog::error(msg));
                }
            }
            Action::SaveAndCloseEditor => {
                let result = self
                    .dual_pane
                    .active_editor_mut()
                    .map(|e| e.save())
                    .unwrap_or(Ok(()));
                match result {
                    Ok(()) => self.dual_pane.close_editor_in_active(),
                    Err(msg) => self.dialog = Some(Dialog::error(msg)),
                }
            }
            Action::DiscardAndCloseEditor => {
                if matches!(self.pending_op, Some(PendingOp::CloseEditor)) {
                    self.pending_op = None;
                    self.dialog = None;
                    self.dual_pane.close_editor_in_active();
                }
            }
            Action::ToggleEditorFullscreen => {
                if self.dual_pane.active_editor().is_some() {
                    self.dual_pane.editor_fullscreen = !self.dual_pane.editor_fullscreen;
                }
            }
            Action::EditorKeyInput(key) => {
                if let Some(inner_action) = self.dual_pane.handle_editor_key(key) {
                    self.dispatch(inner_action);
                }
            }
            Action::ViewFile => {
                if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                {
                    if !entry.is_dir {
                        self.viewer_request = Some(entry.path.clone());
                    }
                }
            }
            Action::UnpackArchive => {
                if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                {
                    if !entry.is_dir {
                        let dest = self.dual_pane.inactive_dir();
                        let cwd = self.dual_pane.active_dir();
                        match crate::fs::archive::resolve_unpack_command(&entry.path, &dest) {
                            Ok(unpack_cmd) => {
                                let tx = self.action_tx.clone();
                                let label = unpack_cmd.display();
                                self.task = Some(TaskState::new(&label));
                                self.input_mode = InputMode::TaskOutput;
                                match unpack_cmd {
                                    crate::fs::archive::UnpackCommand::Shell(cmd) => {
                                        tokio::spawn(task::run_task(cmd, cwd, tx));
                                    }
                                    crate::fs::archive::UnpackCommand::Direct { program, args } => {
                                        tokio::spawn(task::run_task_direct(program, args, cwd, tx));
                                    }
                                }
                            }
                            Err(msg) => {
                                self.dialog = Some(Dialog::error(msg));
                            }
                        }
                    }
                }
            }
            Action::Rename => {
                if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                {
                    self.input_mode = InputMode::Rename(entry.name.clone());
                }
            }
            Action::CreateNew => {
                self.input_mode = InputMode::CreateTypeChoice;
            }
            Action::MkDir => {
                self.input_mode = InputMode::MkDir(String::new());
            }
            Action::CreateFile => {
                self.input_mode = InputMode::CreateFile(String::new());
            }
            Action::StartFilter => {
                self.input_mode = InputMode::Filter(String::new());
                if let Some(e) = self.dual_pane.active_explorer_mut() {
                    e.handle_action(&Action::StartFilter);
                }
            }
            Action::FilterInput(ch) => {
                if let InputMode::Filter(ref mut text) = self.input_mode {
                    text.push(ch);
                }
                if let Some(e) = self.dual_pane.active_explorer_mut() {
                    e.handle_action(&Action::FilterInput(ch));
                }
            }
            Action::FilterBackspace => {
                if let InputMode::Filter(ref mut text) = self.input_mode {
                    text.pop();
                }
                if let Some(e) = self.dual_pane.active_explorer_mut() {
                    e.handle_action(&Action::FilterBackspace);
                }
            }
            Action::FilterConfirm => {
                self.input_mode = InputMode::Normal;
            }
            Action::FilterCancel => {
                self.input_mode = InputMode::Normal;
                if let Some(e) = self.dual_pane.active_explorer_mut() {
                    e.handle_action(&Action::FilterCancel);
                }
            }
            Action::InputChar(ch) => match self.input_mode {
                InputMode::Rename(ref mut text)
                | InputMode::MkDir(ref mut text)
                | InputMode::CreateFile(ref mut text)
                | InputMode::Command(ref mut text) => {
                    text.push(ch);
                }
                _ => {}
            },
            Action::InputBackspace => match self.input_mode {
                InputMode::Rename(ref mut text)
                | InputMode::MkDir(ref mut text)
                | InputMode::CreateFile(ref mut text)
                | InputMode::Command(ref mut text) => {
                    text.pop();
                }
                _ => {}
            },
            Action::InputConfirm => {
                match self.input_mode.clone() {
                    InputMode::Rename(new_name) => {
                        self.do_rename(&new_name);
                        self.input_mode = InputMode::Normal;
                    }
                    InputMode::MkDir(name) => {
                        self.do_mkdir(&name);
                        self.input_mode = InputMode::Normal;
                    }
                    InputMode::CreateFile(name) => {
                        self.do_create_file(&name);
                        self.input_mode = InputMode::Normal;
                    }
                    InputMode::Command(cmd) => {
                        // do_command sets input_mode itself (Normal or TaskOutput)
                        self.do_command(&cmd);
                    }
                    _ => {}
                }
            }
            Action::StartCommand(ch) => {
                if self.task.as_ref().map(|t| t.running).unwrap_or(false) {
                    // Task running - restore window so user can see it
                    self.input_mode = InputMode::TaskOutput;
                } else {
                    self.input_mode = InputMode::Command(String::from(ch));
                }
            }
            Action::InputCancel => {
                self.input_mode = InputMode::Normal;
            }
            Action::OperationComplete(msg) => {
                self.dual_pane.refresh_both();
                self.dialog = Some(Dialog::info(msg));
            }
            Action::OperationError(msg) => {
                self.dual_pane.refresh_both();
                self.dialog = Some(Dialog::error(msg));
            }
            Action::OperationProgress { .. } => {}
            Action::TaskMinimize => {
                // Esc/Ctrl+Z from task window - always minimize to Normal
                if self.task.is_some() {
                    self.input_mode = InputMode::Normal;
                }
            }
            Action::TaskRestore => {
                // Ctrl+Z from Normal mode - restore task window
                if self.task.is_some() {
                    self.input_mode = InputMode::TaskOutput;
                }
            }
            Action::TaskDismiss => {
                // Esc/Enter from task window
                if let Some(ref task) = self.task {
                    if task.running {
                        // Still running: minimize instead of close
                        self.input_mode = InputMode::Normal;
                    } else {
                        self.task = None;
                        self.input_mode = InputMode::Normal;
                    }
                }
            }
            Action::TaskScrollUp => {
                if let Some(ref mut task) = self.task {
                    task.scroll_up();
                }
            }
            Action::TaskScrollDown => {
                if let Some(ref mut task) = self.task {
                    task.scroll_down();
                }
            }
            // TaskLine/Complete/Error handled early above; these are unreachable
            Action::TaskLine(_) | Action::TaskComplete(_) | Action::TaskError(_) => {}
            Action::ShowHelp => {
                self.dialog = Some(Dialog::info(
                    "(Up/Down to scroll this help)\n\
                     \n\
                     Usage: cpt [left-path] [right-path]\n\
                     \n\
                     Shortcuts:\n\
                     Up/Down - navigate\n\
                     Home/End - top/bottom\n\
                     PgUp/PgDn - page scroll\n\
                     Enter - open dir / edit text file / unpack archive\n\
                     Shift+Enter - open with OS default app\n\
                     Ctrl+Enter - context menu (open as...)\n\
                     F3 - view in pager\n\
                     Backspace - parent dir\n\
                     Tab - switch pane\n\
                     Space/Insert - toggle select\n\
                     Ctrl+A - select/deselect all\n\
                     F1 - this help\n\
                     F2 - rename\n\
                     F4 - create new file or directory\n\
                     F5 - copy to other pane\n\
                     F6 - move to other pane\n\
                     F8/Del - delete\n\
                     Ctrl+B - bookmarks\n\
                     Ctrl+F - quick filter\n\
                     Ctrl+T - theme editor\n\
                     Ctrl+R - refresh\n\
                     Ctrl+Q - quit\n\
                     \n\
                     In editor (Enter on file to open):\n\
                     Ctrl+S - save\n\
                     Ctrl+Q or Esc - close editor\n\
                     Tab - switch to other pane\n\
                     Ctrl+I - insert tab character\n\
                     \n\
                     Type any character for command line (cd, shell commands)",
                ));
            }
            Action::Error(msg) => {
                self.dialog = Some(Dialog::error(msg));
            }
            // Bookmark panel actions without panel open are no-ops
            Action::BookmarkNavigate
            | Action::BookmarkAdd
            | Action::BookmarkRemove
            | Action::BookmarkClose => {}
            // Theme editor actions without editor open are no-ops
            Action::ThemeEditorClose
            | Action::ThemeEditorCycleBase
            | Action::ThemeEditorSave
            | Action::ThemeEditorSaveAs
            | Action::ThemeEditorOpenFile
            | Action::ThemeEditorDelete
            | Action::ThemeEditorOpenPicker
            | Action::ThemeEditorPickerLeft
            | Action::ThemeEditorPickerRight => {}
            other => {
                if let Some(follow_up) = self.dual_pane.handle_action(&other) {
                    self.dispatch(follow_up);
                }
            }
        }
    }

    /// Handle actions when the bookmark panel is open.
    /// Returns true if the action was consumed.
    fn dispatch_bookmark_panel(&mut self, action: &Action) -> bool {
        // Let dialog actions pass through so dialogs above the panel work.
        if self.dialog.is_some() {
            match action {
                Action::DismissDialog
                | Action::ConfirmDialog
                | Action::DialogScrollUp
                | Action::DialogScrollDown => return false,
                _ => {}
            }
        }

        // Naming sub-state: user is typing a name for a new bookmark.
        if self.bookmark_panel.as_ref().unwrap().naming.is_some() {
            return self.dispatch_bookmark_naming(action);
        }

        match action {
            Action::MoveUp => {
                self.bookmark_panel.as_mut().unwrap().move_up();
                true
            }
            Action::MoveDown => {
                let count = self.bookmarks.entries.len();
                self.bookmark_panel.as_mut().unwrap().move_down(count);
                true
            }
            Action::MoveToTop => {
                self.bookmark_panel.as_mut().unwrap().cursor = 0;
                true
            }
            Action::MoveToBottom => {
                let count = self.bookmarks.entries.len();
                if count > 0 {
                    self.bookmark_panel.as_mut().unwrap().cursor = count - 1;
                }
                true
            }
            Action::BookmarkNavigate => {
                let cursor = self.bookmark_panel.as_ref().unwrap().cursor;
                if let Some(entry) = self.bookmarks.entries.get(cursor) {
                    let path = entry.path.clone();
                    if path.is_dir() {
                        if let Some(explorer) = self.dual_pane.active_explorer_mut() {
                            explorer.current_dir = path;
                            explorer.filter_text = None;
                            explorer.cursor = 0;
                            explorer.refresh();
                            self.bookmark_panel = None;
                        }
                    } else {
                        self.dialog =
                            Some(Dialog::error(format!("Path not found: {}", path.display())));
                    }
                }
                true
            }
            Action::BookmarkAdd => {
                // Default name = folder name of active pane.
                let dir = self.dual_pane.active_dir();
                let default_name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let panel = self.bookmark_panel.as_mut().unwrap();
                panel.naming = Some(default_name);
                true
            }
            Action::BookmarkRemove => {
                let cursor = self.bookmark_panel.as_ref().unwrap().cursor;
                self.bookmarks.remove(cursor);
                let count = self.bookmarks.entries.len();
                self.bookmark_panel.as_mut().unwrap().clamp_cursor(count);
                true
            }
            Action::BookmarkClose => {
                self.bookmark_panel = None;
                true
            }
            Action::Quit | Action::Tick | Action::Resize(_, _) | Action::Noop => false,
            _ => true,
        }
    }

    fn dispatch_bookmark_naming(&mut self, action: &Action) -> bool {
        match action {
            Action::InputChar(c) => {
                self.bookmark_panel
                    .as_mut()
                    .unwrap()
                    .naming
                    .as_mut()
                    .unwrap()
                    .push(*c);
                true
            }
            Action::InputBackspace => {
                self.bookmark_panel
                    .as_mut()
                    .unwrap()
                    .naming
                    .as_mut()
                    .unwrap()
                    .pop();
                true
            }
            Action::InputConfirm => {
                let name = self
                    .bookmark_panel
                    .as_ref()
                    .unwrap()
                    .naming
                    .as_ref()
                    .unwrap()
                    .clone();
                self.bookmark_panel.as_mut().unwrap().naming = None;
                if !name.is_empty() {
                    let path = self.dual_pane.active_dir();
                    self.bookmarks.add(name, path);
                    // Move cursor to newly added entry.
                    let count = self.bookmarks.entries.len();
                    if count > 0 {
                        self.bookmark_panel.as_mut().unwrap().cursor = count - 1;
                    }
                }
                true
            }
            Action::InputCancel => {
                self.bookmark_panel.as_mut().unwrap().naming = None;
                true
            }
            Action::Quit | Action::Tick | Action::Resize(_, _) | Action::Noop => false,
            _ => true,
        }
    }

    /// Handle actions when the theme editor is open.
    /// Returns true if the action was consumed.
    fn dispatch_theme_editor(&mut self, action: &Action) -> bool {
        // Dialog actions always pass through so dialogs can dismiss
        if self.dialog.is_some() {
            match action {
                Action::DismissDialog
                | Action::ConfirmDialog
                | Action::DialogScrollUp
                | Action::DialogScrollDown
                | Action::ConflictOverwrite
                | Action::ConflictRename => return false,
                _ => {}
            }
        }
        // Naming sub-state
        if self.theme_editor.as_ref().unwrap().naming.is_some() {
            return self.dispatch_theme_naming(action);
        }
        // Picker sub-state
        if self.theme_editor.as_ref().unwrap().picker.is_some() {
            return self.dispatch_theme_picker(action);
        }
        // Normal editor
        self.dispatch_theme_normal(action)
    }

    fn dispatch_theme_naming(&mut self, action: &Action) -> bool {
        match action {
            Action::InputChar(c) => {
                // Only allow valid filename chars
                if c.is_alphanumeric() || *c == '-' || *c == '_' {
                    self.theme_editor
                        .as_mut()
                        .unwrap()
                        .naming
                        .as_mut()
                        .unwrap()
                        .push(*c);
                }
                true
            }
            Action::InputBackspace => {
                self.theme_editor
                    .as_mut()
                    .unwrap()
                    .naming
                    .as_mut()
                    .unwrap()
                    .pop();
                true
            }
            Action::InputConfirm => {
                let name = self
                    .theme_editor
                    .as_ref()
                    .unwrap()
                    .naming
                    .as_ref()
                    .unwrap()
                    .clone();
                self.theme_editor.as_mut().unwrap().naming = None;
                if !name.is_empty() {
                    self.theme_name = name.clone();
                    match self.theme.save_to_file(&name) {
                        Ok(path) => {
                            self.dialog = Some(Dialog::info(format!(
                                "Theme '{}' saved to:\n{}",
                                name,
                                path.display()
                            )));
                        }
                        Err(e) => {
                            self.dialog = Some(Dialog::error(e));
                        }
                    }
                }
                true
            }
            Action::InputCancel => {
                self.theme_editor.as_mut().unwrap().naming = None;
                true
            }
            Action::Quit | Action::Tick | Action::Resize(_, _) | Action::Noop => false,
            _ => true,
        }
    }

    fn dispatch_theme_picker(&mut self, action: &Action) -> bool {
        // Compute what to do without holding mutable borrows
        enum PickerOp {
            Move,
            Confirm,
            Cancel(ratatui::style::Color),
            Swallow,
            PassThrough,
        }

        let op = {
            let editor = self.theme_editor.as_mut().unwrap();
            let picker = editor.picker.as_mut().unwrap();
            match action {
                Action::MoveUp => {
                    picker.move_up();
                    PickerOp::Move
                }
                Action::MoveDown => {
                    picker.move_down();
                    PickerOp::Move
                }
                Action::ThemeEditorPickerLeft => {
                    picker.move_left();
                    PickerOp::Move
                }
                Action::ThemeEditorPickerRight => {
                    picker.move_right();
                    PickerOp::Move
                }
                Action::InputConfirm => PickerOp::Confirm,
                Action::InputCancel => PickerOp::Cancel(picker.original),
                Action::Quit | Action::Tick | Action::Resize(_, _) | Action::Noop => {
                    PickerOp::PassThrough
                }
                _ => PickerOp::Swallow,
            }
        };

        match op {
            PickerOp::Move => {
                let editor = self.theme_editor.as_ref().unwrap();
                let picker = editor.picker.as_ref().unwrap();
                let field_idx = editor.cursor;
                let color = picker.current_color();
                self.theme.set_field(field_idx, color);
                true
            }
            PickerOp::Confirm => {
                self.theme_editor.as_mut().unwrap().picker = None;
                true
            }
            PickerOp::Cancel(original) => {
                let field_idx = self.theme_editor.as_ref().unwrap().cursor;
                self.theme_editor.as_mut().unwrap().picker = None;
                self.theme.set_field(field_idx, original);
                true
            }
            PickerOp::Swallow => true,
            PickerOp::PassThrough => false,
        }
    }

    fn dispatch_theme_normal(&mut self, action: &Action) -> bool {
        match action {
            Action::MoveUp => {
                self.theme_editor.as_mut().unwrap().move_up();
                true
            }
            Action::MoveDown => {
                self.theme_editor.as_mut().unwrap().move_down();
                true
            }
            Action::MoveToTop => {
                self.theme_editor.as_mut().unwrap().move_to_top();
                true
            }
            Action::MoveToBottom => {
                self.theme_editor.as_mut().unwrap().move_to_bottom();
                true
            }
            Action::PageUp => {
                self.theme_editor.as_mut().unwrap().page_up();
                true
            }
            Action::PageDown => {
                self.theme_editor.as_mut().unwrap().page_down();
                true
            }
            Action::ThemeEditorOpenPicker => {
                let field_idx = self.theme_editor.as_ref().unwrap().cursor;
                let current_color = self.theme.get_field(field_idx);
                self.theme_editor
                    .as_mut()
                    .unwrap()
                    .open_picker(current_color);
                true
            }
            Action::ThemeEditorCycleBase => {
                // Auto-save custom theme before switching away
                if !Theme::is_builtin(&self.theme_name) {
                    let _ = self.theme.save_to_file(&self.theme_name);
                }
                self.theme_name = Theme::next_theme_name(&self.theme_name);
                self.theme = Theme::by_name(&self.theme_name);
                true
            }
            Action::ThemeEditorSave => {
                if Theme::is_builtin(&self.theme_name) {
                    // Built-in: enter naming mode
                    self.theme_editor.as_mut().unwrap().start_naming();
                } else {
                    // Custom: overwrite
                    let name = self.theme_name.clone();
                    match self.theme.save_to_file(&name) {
                        Ok(path) => {
                            self.dialog = Some(Dialog::info(format!(
                                "Theme '{}' saved to:\n{}",
                                name,
                                path.display()
                            )));
                        }
                        Err(e) => {
                            self.dialog = Some(Dialog::error(e));
                        }
                    }
                }
                true
            }
            Action::ThemeEditorSaveAs => {
                self.theme_editor.as_mut().unwrap().start_naming();
                true
            }
            Action::ThemeEditorOpenFile => {
                if Theme::is_builtin(&self.theme_name) {
                    // Built-in themes have no file - save as a custom theme first
                    self.theme_editor.as_mut().unwrap().start_naming();
                } else {
                    // Save current colors and open the file
                    let name = self.theme_name.clone();
                    match self.theme.save_to_file(&name) {
                        Ok(path) => {
                            if let Err(e) = open::that(&path) {
                                self.dialog = Some(Dialog::error(format!(
                                    "Failed to open editor: {}\n\nFile: {}",
                                    e,
                                    path.display()
                                )));
                            }
                        }
                        Err(e) => {
                            self.dialog = Some(Dialog::error(e));
                        }
                    }
                }
                true
            }
            Action::ThemeEditorDelete => {
                if Theme::is_builtin(&self.theme_name) {
                    self.dialog = Some(Dialog::error("Cannot delete a built-in theme"));
                } else {
                    let name = self.theme_name.clone();
                    match Theme::delete_file(&name) {
                        Ok(()) => {
                            self.dialog = Some(Dialog::info(format!("Deleted theme '{}'", name)));
                            self.theme_name = "default".to_string();
                            self.theme = Theme::by_name(&self.theme_name);
                        }
                        Err(e) => {
                            self.dialog = Some(Dialog::error(e));
                        }
                    }
                }
                true
            }
            Action::ThemeEditorClose => {
                // Auto-save custom theme on close
                if !Theme::is_builtin(&self.theme_name) {
                    let _ = self.theme.save_to_file(&self.theme_name);
                }
                self.theme_editor = None;
                true
            }
            Action::Quit | Action::Tick | Action::Resize(_, _) | Action::Noop => false,
            _ => true,
        }
    }

    fn execute_op(&self, op: PendingOp) {
        let tx = self.action_tx.clone();
        match op {
            PendingOp::Copy { pairs } => {
                tokio::spawn(ops::copy_entries(pairs, tx));
            }
            PendingOp::Move { pairs } => {
                tokio::spawn(ops::move_entries(pairs, tx));
            }
            PendingOp::Delete(sources) => {
                tokio::spawn(ops::delete_entries(sources, tx));
            }
            // CloseEditor is handled in ConfirmDialog dispatch directly; not routed here.
            PendingOp::CloseEditor => {}
        }
    }

    fn get_operation_sources(&self) -> Vec<PathBuf> {
        let Some(explorer) = self.dual_pane.active_explorer() else {
            return vec![];
        };
        let mut sources = explorer.selected_paths();
        if sources.is_empty() {
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
        let old_path = self
            .dual_pane
            .active_explorer()
            .and_then(|e| e.current_entry())
            .map(|entry| entry.path.clone());
        if let Some(old_path) = old_path {
            let Some(parent) = old_path.parent() else {
                return;
            };
            let new_path = parent.join(new_name);
            if let Err(e) = std::fs::rename(&old_path, &new_path) {
                self.dialog = Some(Dialog::error(format!("Rename failed: {}", e)));
            }
            if let Some(e) = self.dual_pane.active_explorer_mut() {
                e.refresh();
            }
        }
    }

    fn do_mkdir(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        let dir = self.dual_pane.active_dir();
        let new_dir = dir.join(name);
        if let Err(e) = std::fs::create_dir(&new_dir) {
            self.dialog = Some(Dialog::error(format!("Mkdir failed: {}", e)));
        }
        if let Some(e) = self.dual_pane.active_explorer_mut() {
            e.refresh();
        }
    }

    fn do_create_file(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        let dir = self.dual_pane.active_dir();
        let new_file = dir.join(name);
        if new_file.exists() {
            self.dialog = Some(Dialog::error(format!("Already exists: {}", name)));
        } else if let Err(e) = std::fs::File::create(&new_file) {
            self.dialog = Some(Dialog::error(format!("Create file failed: {}", e)));
        }
        if let Some(e) = self.dual_pane.active_explorer_mut() {
            e.refresh();
        }
    }

    fn do_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            self.input_mode = InputMode::Normal;
            return;
        }

        // Parse "cd <path>" commands
        if let Some(path_str) = cmd
            .strip_prefix("cd ")
            .or_else(|| if cmd == "cd" { Some("~") } else { None })
        {
            let path_str = path_str.trim();
            let path_str = if path_str == "~" {
                std::env::var("USERPROFILE")
                    .or_else(|_| std::env::var("HOME"))
                    .unwrap_or_else(|_| ".".to_string())
            } else if path_str.len() == 2 && path_str.ends_with(':') {
                format!("{}\\", path_str)
            } else {
                path_str.to_string()
            };

            let target = if PathBuf::from(&path_str).is_absolute() {
                PathBuf::from(&path_str)
            } else {
                self.dual_pane.active_dir().join(&path_str)
            };

            match clean_canonicalize(&target) {
                Ok(resolved) if resolved.is_dir() => {
                    if let Some(explorer) = self.dual_pane.active_explorer_mut() {
                        explorer.current_dir = resolved;
                        explorer.filter_text = None;
                        explorer.cursor = 0;
                        explorer.refresh();
                    }
                }
                Ok(_) => {
                    self.dialog = Some(Dialog::error(format!(
                        "Not a directory: {}",
                        target.display()
                    )));
                }
                Err(e) => {
                    self.dialog = Some(Dialog::error(format!("cd: {}", e)));
                }
            }
            self.input_mode = InputMode::Normal;
            return;
        }

        // Run as async shell command in the task window
        if self.task.as_ref().map(|t| t.running).unwrap_or(false) {
            // Already have a running task - show it instead of starting another
            self.input_mode = InputMode::TaskOutput;
            return;
        }

        let cwd = self.dual_pane.active_dir();
        self.task = Some(TaskState::new(cmd));
        self.input_mode = InputMode::TaskOutput;
        let tx = self.action_tx.clone();
        tokio::spawn(task::run_task(cmd.to_string(), cwd, tx));
    }

    fn draw_context_menu(&self, frame: &mut Frame, area: Rect, state: &ContextMenuState) {
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

    fn save_session(&self) {
        let config = SessionConfig {
            left_dir: self.dual_pane.left_dir(),
            right_dir: self.dual_pane.right_dir(),
            active_pane: self.dual_pane.active,
            theme_name: self.theme_name.clone(),
        };
        config.save();
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn make_app() -> App {
        let dir = std::env::current_dir().unwrap();
        App::new(dir.clone(), dir, PaneSide::Left, "default".to_string())
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    // ── Normal mode key mapping ───────────────────────────────

    #[test]
    fn f4_maps_to_create_new() {
        let app = make_app();
        assert!(matches!(app.map_key(key(KeyCode::F(4))), Action::CreateNew));
    }

    #[test]
    fn f7_is_unbound_in_normal_mode() {
        let app = make_app();
        assert!(matches!(app.map_key(key(KeyCode::F(7))), Action::Noop));
    }

    #[test]
    fn q_no_longer_quits() {
        let app = make_app();
        // 'q' now starts command mode (falls through to StartCommand)
        let action = app.map_key(key(KeyCode::Char('q')));
        assert!(!matches!(action, Action::Quit));
    }

    #[test]
    fn ctrl_q_quits() {
        let app = make_app();
        assert!(matches!(app.map_key(ctrl('q')), Action::Quit));
    }

    // ── CreateTypeChoice mode ─────────────────────────────────

    #[test]
    fn create_type_choice_f_creates_file() {
        let mut app = make_app();
        app.input_mode = InputMode::CreateTypeChoice;
        assert!(matches!(
            app.map_key(key(KeyCode::Char('f'))),
            Action::CreateFile
        ));
    }

    #[test]
    fn create_type_choice_uppercase_f_creates_file() {
        let mut app = make_app();
        app.input_mode = InputMode::CreateTypeChoice;
        assert!(matches!(
            app.map_key(key(KeyCode::Char('F'))),
            Action::CreateFile
        ));
    }

    #[test]
    fn create_type_choice_d_creates_dir() {
        let mut app = make_app();
        app.input_mode = InputMode::CreateTypeChoice;
        assert!(matches!(
            app.map_key(key(KeyCode::Char('d'))),
            Action::MkDir
        ));
    }

    #[test]
    fn create_type_choice_uppercase_d_creates_dir() {
        let mut app = make_app();
        app.input_mode = InputMode::CreateTypeChoice;
        assert!(matches!(
            app.map_key(key(KeyCode::Char('D'))),
            Action::MkDir
        ));
    }

    #[test]
    fn create_type_choice_esc_cancels() {
        let mut app = make_app();
        app.input_mode = InputMode::CreateTypeChoice;
        assert!(matches!(
            app.map_key(key(KeyCode::Esc)),
            Action::InputCancel
        ));
    }

    #[test]
    fn create_type_choice_other_keys_noop() {
        let mut app = make_app();
        app.input_mode = InputMode::CreateTypeChoice;
        assert!(matches!(app.map_key(key(KeyCode::Enter)), Action::Noop));
        assert!(matches!(app.map_key(key(KeyCode::Char('x'))), Action::Noop));
    }

    // ── Dispatch: CreateNew sets CreateTypeChoice mode ────────

    #[test]
    fn dispatch_create_new_enters_choice_mode() {
        let mut app = make_app();
        app.dispatch(Action::CreateNew);
        assert!(matches!(app.input_mode, InputMode::CreateTypeChoice));
    }

    #[test]
    fn dispatch_input_cancel_from_choice_returns_normal() {
        let mut app = make_app();
        app.input_mode = InputMode::CreateTypeChoice;
        app.dispatch(Action::InputCancel);
        assert!(matches!(app.input_mode, InputMode::Normal));
    }

    // ── Bookmark panel ────────────────────────────────────────

    #[test]
    fn ctrl_b_maps_to_open_bookmarks() {
        let app = make_app();
        assert!(matches!(app.map_key(ctrl('b')), Action::OpenBookmarks));
    }

    #[test]
    fn open_bookmarks_shows_panel() {
        let mut app = make_app();
        app.dispatch(Action::OpenBookmarks);
        assert!(app.bookmark_panel.is_some());
    }

    #[test]
    fn bookmark_close_hides_panel() {
        let mut app = make_app();
        app.dispatch(Action::OpenBookmarks);
        app.dispatch(Action::BookmarkClose);
        assert!(app.bookmark_panel.is_none());
    }

    #[test]
    fn bookmark_panel_intercepts_up_down() {
        let mut app = make_app();
        // Add a couple of bookmarks so we can navigate.
        app.bookmarks.entries.push(crate::bookmarks::BookmarkEntry {
            name: "a".to_string(),
            path: std::path::PathBuf::from("."),
        });
        app.bookmarks.entries.push(crate::bookmarks::BookmarkEntry {
            name: "b".to_string(),
            path: std::path::PathBuf::from("."),
        });
        app.dispatch(Action::OpenBookmarks);
        let panel = app.bookmark_panel.as_ref().unwrap();
        assert_eq!(panel.cursor, 0);
        app.dispatch(Action::MoveDown);
        let panel = app.bookmark_panel.as_ref().unwrap();
        assert_eq!(panel.cursor, 1);
        app.dispatch(Action::MoveUp);
        let panel = app.bookmark_panel.as_ref().unwrap();
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn bookmark_add_starts_naming_with_default_folder_name() {
        let mut app = make_app();
        app.dispatch(Action::OpenBookmarks);
        app.dispatch(Action::BookmarkAdd);
        let panel = app.bookmark_panel.as_ref().unwrap();
        // naming should be Some - default name is last segment of current dir
        assert!(panel.naming.is_some());
    }

    #[test]
    fn bookmark_naming_confirm_adds_entry() {
        let mut app = make_app();
        let initial_count = app.bookmarks.entries.len();
        app.dispatch(Action::OpenBookmarks);
        app.dispatch(Action::BookmarkAdd);
        // Confirm with the pre-filled default name.
        app.dispatch(Action::InputConfirm);
        assert!(app.bookmark_panel.as_ref().unwrap().naming.is_none());
        // An entry should have been added.
        assert!(app.bookmarks.entries.len() > initial_count);
    }

    #[test]
    fn bookmark_naming_cancel_discards_input() {
        let mut app = make_app();
        app.dispatch(Action::OpenBookmarks);
        app.dispatch(Action::BookmarkAdd);
        let initial_count = app.bookmarks.entries.len();
        app.dispatch(Action::InputChar('x'));
        app.dispatch(Action::InputCancel);
        assert!(app.bookmark_panel.as_ref().unwrap().naming.is_none());
        assert_eq!(app.bookmarks.entries.len(), initial_count);
    }

    #[test]
    fn bookmark_remove_deletes_entry() {
        let mut app = make_app();
        app.bookmarks.entries.push(crate::bookmarks::BookmarkEntry {
            name: "tmp".to_string(),
            path: std::path::PathBuf::from("."),
        });
        app.dispatch(Action::OpenBookmarks);
        let initial_count = app.bookmarks.entries.len();
        app.dispatch(Action::BookmarkRemove);
        assert_eq!(app.bookmarks.entries.len(), initial_count - 1);
    }

    #[test]
    fn bookmark_panel_key_esc_closes() {
        let mut app = make_app();
        app.dispatch(Action::OpenBookmarks);
        // Esc maps to BookmarkClose when panel is open
        let action = app.map_key(key(KeyCode::Esc));
        assert!(matches!(action, Action::BookmarkClose));
    }

    #[test]
    fn bookmark_panel_naming_key_enter_maps_to_confirm() {
        let mut app = make_app();
        app.dispatch(Action::OpenBookmarks);
        app.dispatch(Action::BookmarkAdd);
        // When naming is active, Enter maps to InputConfirm
        let action = app.map_key(key(KeyCode::Enter));
        assert!(matches!(action, Action::InputConfirm));
    }
}
