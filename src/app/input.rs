use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::action::Action;
use crate::components::bookmark_panel::BookmarkPanel;
use crate::components::theme_editor::ThemeEditor;
use crate::event::Event;

use super::{App, InputMode};

impl App {
    pub(super) fn map_event(&self, event: Event) -> Action {
        match event {
            Event::Tick => Action::Tick,
            Event::Resize(w, h) => Action::Resize(w, h),
            Event::Key(key) => self.map_key(key),
        }
    }

    pub(super) fn map_key(&self, key: KeyEvent) -> Action {
        log::debug!("key: mods={:?} code={:?}", key.modifiers, key.code);
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
                    KeyCode::Up => Action::MoveUp,
                    KeyCode::Down => Action::MoveDown,
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
            InputMode::UnpackChoice { .. } => {
                return match key.code {
                    KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
                        Action::UnpackArchive
                    }
                    KeyCode::Char('f') | KeyCode::Char('F') => Action::UnpackArchiveTo {
                        dest: std::path::PathBuf::new(), // sentinel: "create folder" mode
                    },
                    KeyCode::Char('c') | KeyCode::Char('C') => Action::UnpackArchiveTo {
                        dest: std::path::PathBuf::from("\x00"), // sentinel: switch to custom path input
                    },
                    KeyCode::Esc => Action::InputCancel,
                    _ => Action::Noop,
                };
            }
            InputMode::UnpackCustomPath { .. } => {
                return match key.code {
                    KeyCode::Char(c) => Action::InputChar(c),
                    KeyCode::Backspace => Action::InputBackspace,
                    KeyCode::Enter => Action::InputConfirm,
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
            InputMode::SshConnect(state) => {
                if state.connecting {
                    return if key.code == KeyCode::Esc {
                        Action::InputCancel
                    } else {
                        Action::Noop
                    };
                }
                return match key.code {
                    KeyCode::Char(c) => Action::InputChar(c),
                    KeyCode::Backspace => Action::InputBackspace,
                    KeyCode::Tab | KeyCode::Down => Action::SshConnectNextField,
                    KeyCode::Up => Action::SshConnectPrevField,
                    KeyCode::Enter => Action::SshConnectConfirm,
                    KeyCode::Esc => Action::InputCancel,
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
            (KeyModifiers::NONE, KeyCode::F(7)) => Action::MkDir,
            (KeyModifiers::NONE, KeyCode::F(8)) | (KeyModifiers::NONE, KeyCode::Delete) => {
                Action::DeleteSelected
            }
            (KeyModifiers::CONTROL, KeyCode::Char('b')) => Action::OpenBookmarks,
            (KeyModifiers::CONTROL, KeyCode::Char('f')) => Action::StartFilter,
            (KeyModifiers::CONTROL, KeyCode::Char('o')) => Action::SshConnect,
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

    pub(super) fn map_theme_editor_key(&self, key: KeyEvent, editor: &ThemeEditor) -> Action {
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

    pub(super) fn map_bookmark_panel_key(
        &self,
        key: KeyEvent,
        panel: &BookmarkPanel,
    ) -> Action {
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
}
