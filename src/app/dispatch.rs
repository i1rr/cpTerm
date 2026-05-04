use std::path::PathBuf;

use crate::action::Action;
use crate::components::bookmark_panel::BookmarkPanel;
use crate::components::dialog::Dialog;
use crate::components::theme_editor::ThemeEditor;

use super::{App, ContextMenuItem, ContextMenuState, InputMode, PendingOp};

impl App {
    pub(super) fn dispatch(&mut self, action: Action) {
        // Task stream actions are processed regardless of any modal state so that
        // background output keeps flowing while theme editor or dialogs are open.
        if let Action::TaskLine(ref line) = action {
            log::debug!("task output: {}", line);
            if let Some(ref mut task) = self.task {
                task.push_line(line.clone());
            }
            return;
        }
        if let Action::TaskComplete(code) = action {
            log::debug!("task complete: exit code {}", code);
            if let Some(ref mut task) = self.task {
                task.finish(code);
            }

            // Diff pre-extraction snapshot to find new files for highlighting
            let snapshot = self.pre_extract_snapshot.take();

            // Refresh both panes so newly extracted files appear
            self.dual_pane.refresh_both();

            // Mark new files that appeared after extraction
            if code == 0
                && let Some((dir, old_files)) = snapshot
            {
                let mut new_paths = Vec::new();
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    for entry in rd.flatten() {
                        let path = entry.path();
                        if !old_files.contains(&path) {
                            new_paths.push(path);
                        }
                    }
                }
                if !new_paths.is_empty() {
                    log::debug!("marking {} new files in {}", new_paths.len(), dir.display());
                    self.dual_pane.mark_new_files(&new_paths);
                }
            }
            return;
        }
        if let Action::TaskError(ref msg) = action {
            log::debug!("task error: {}", msg);
            if let Some(ref mut task) = self.task {
                task.push_line(format!("error: {}", msg));
                task.finish(-1);
            } else {
                self.dialog = Some(Dialog::error(msg));
            }
            return;
        }

        if self.bookmark_panel.is_some() {
            let mut bp = self.bookmark_panel.take().unwrap();
            let (handled, keep) = self.dispatch_bookmark_panel(&mut bp, action.clone());
            if keep {
                self.bookmark_panel = Some(bp);
            }
            if handled {
                return;
            }
        }

        if self.drives_panel.is_some() {
            let mut dp = self.drives_panel.take().unwrap();
            let (handled, keep) = self.dispatch_drives_panel(&mut dp, action.clone());
            if keep {
                self.drives_panel = Some(dp);
            }
            if handled {
                return;
            }
        }

        if self.theme_editor.is_some() {
            let mut te = self.theme_editor.take().unwrap();
            let handled = self.dispatch_theme_editor(&mut te, action.clone());
            // Restore the editor unless it was closed (ThemeEditorClose sets self.theme_editor = None,
            // which is already None since we took it; we simply don't put it back).
            if !matches!(action, Action::ThemeEditorClose) {
                self.theme_editor = Some(te);
            }
            if handled {
                return;
            }
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

        // Confirm filter when navigating with arrow keys
        if matches!(self.input_mode, InputMode::Filter(_))
            && matches!(action, Action::MoveUp | Action::MoveDown)
        {
            self.input_mode = InputMode::Normal;
        }

        if self.dispatch_fs(action.clone()) {
            return;
        }

        if self.dispatch_ssh(action.clone()) {
            return;
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
            Action::OpenDrives { for_side } => {
                if let Some(side) = for_side {
                    self.dual_pane.active = side;
                }
                let drives = crate::fs::drives::list_drives();
                self.drives_panel = Some(crate::components::drives_panel::DrivesPanel::new(drives));
            }
            Action::DriveNavigate | Action::DriveClose => {}
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
                if self.dual_pane.active_remote().is_some() {
                    self.dispatch(Action::RemoteViewFile);
                } else if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                    && !entry.is_dir
                {
                    self.pending_view_file = Some(entry.path.clone());
                }
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
                | InputMode::Command(ref mut text)
                | InputMode::UnpackCustomPath { ref mut text, .. } => {
                    text.push(ch);
                }
                InputMode::SshConnect(ref mut state) => {
                    state.fields[state.active_field].push(ch);
                }
                _ => {}
            },
            Action::InputBackspace => match self.input_mode {
                InputMode::Rename(ref mut text)
                | InputMode::MkDir(ref mut text)
                | InputMode::CreateFile(ref mut text)
                | InputMode::Command(ref mut text)
                | InputMode::UnpackCustomPath { ref mut text, .. } => {
                    text.pop();
                }
                InputMode::SshConnect(ref mut state) => {
                    state.fields[state.active_field].pop();
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
                    InputMode::UnpackCustomPath { archive, text } => {
                        let dest = PathBuf::from(text.trim());
                        if dest.as_os_str().is_empty() {
                            self.input_mode = InputMode::Normal;
                        } else {
                            self.do_unpack(&archive, &dest);
                        }
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
                     Usage: cpt [left-path] [right-path] [--debug]\n\
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
                     Alt+F1 - drives panel (left pane)\n\
                     Alt+F2 - drives panel (right pane)\n\
                     Ctrl+\\ - drives panel (active pane)\n\
                     Ctrl+F - quick filter\n\
                     Ctrl+O - SSH connect (remote browsing)\n\
                     Ctrl+O on remote pane - disconnect\n\
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
                     Archive extraction:\n\
                     Enter on archive shows extraction options:\n\
                     [E] extract here, [F] create folder, [C] custom path\n\
                     \n\
                     New files (copied, moved, extracted) are highlighted\n\
                     until focused with cursor or app restart.\n\
                     \n\
                     Debug mode:\n\
                     --debug flag enables diagnostic logging to file.\n\
                     Log location: %APPDATA%/cpt/cpt.log (Windows)\n\
                                   ~/.config/cpt/cpt.log (Linux/macOS)\n\
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
}
