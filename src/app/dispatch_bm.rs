use crate::action::Action;
use crate::components::bookmark_panel::BookmarkPanel;
use crate::components::dialog::Dialog;

use super::App;

impl App {
    /// Handle actions when the bookmark panel is open.
    /// Returns (consumed, keep_panel). keep_panel false means the panel should close.
    pub(super) fn dispatch_bookmark_panel(
        &mut self,
        bp: &mut BookmarkPanel,
        action: Action,
    ) -> (bool, bool) {
        // Let dialog actions pass through so dialogs above the panel work.
        if self.dialog.is_some() {
            match action {
                Action::DismissDialog
                | Action::ConfirmDialog
                | Action::DialogScrollUp
                | Action::DialogScrollDown => return (false, true),
                _ => {}
            }
        }

        // Naming sub-state: user is typing a name for a new bookmark.
        if bp.naming.is_some() {
            return self.dispatch_bookmark_naming(bp, action);
        }

        match action {
            Action::MoveUp => {
                bp.move_up();
                (true, true)
            }
            Action::MoveDown => {
                let count = self.bookmarks.entries.len();
                bp.move_down(count);
                (true, true)
            }
            Action::MoveToTop => {
                bp.cursor = 0;
                (true, true)
            }
            Action::MoveToBottom => {
                let count = self.bookmarks.entries.len();
                if count > 0 {
                    bp.cursor = count - 1;
                }
                (true, true)
            }
            Action::BookmarkNavigate => {
                let cursor = bp.cursor;
                if let Some(entry) = self.bookmarks.entries.get(cursor) {
                    let path = entry.path.clone();
                    if path.is_dir() {
                        if let Some(explorer) = self.dual_pane.active_explorer_mut() {
                            explorer.current_dir = path;
                            explorer.filter_text = None;
                            explorer.cursor = 0;
                            explorer.refresh();
                            return (true, false); // close panel
                        }
                    } else {
                        self.dialog =
                            Some(Dialog::error(format!("Path not found: {}", path.display())));
                    }
                }
                (true, true)
            }
            Action::BookmarkAdd => {
                // Default name = folder name of active pane.
                let dir = self.dual_pane.active_dir();
                let default_name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                bp.naming = Some(default_name);
                (true, true)
            }
            Action::BookmarkRemove => {
                let cursor = bp.cursor;
                self.bookmarks.remove(cursor);
                let count = self.bookmarks.entries.len();
                bp.clamp_cursor(count);
                (true, true)
            }
            Action::BookmarkClose => {
                (true, false) // close panel
            }
            Action::Quit | Action::Tick | Action::Resize(_, _) | Action::Noop => (false, true),
            _ => (true, true),
        }
    }

    pub(super) fn dispatch_bookmark_naming(
        &mut self,
        bp: &mut BookmarkPanel,
        action: Action,
    ) -> (bool, bool) {
        match action {
            Action::InputChar(c) => {
                if let Some(ref mut name) = bp.naming {
                    name.push(c);
                }
                (true, true)
            }
            Action::InputBackspace => {
                if let Some(ref mut name) = bp.naming {
                    name.pop();
                }
                (true, true)
            }
            Action::InputConfirm => {
                let name = bp.naming.take().unwrap_or_default();
                if !name.is_empty() {
                    let path = self.dual_pane.active_dir();
                    self.bookmarks.add(name, path);
                    // Move cursor to newly added entry.
                    let count = self.bookmarks.entries.len();
                    if count > 0 {
                        bp.cursor = count - 1;
                    }
                }
                (true, true)
            }
            Action::InputCancel => {
                bp.naming = None;
                (true, true)
            }
            Action::Quit | Action::Tick | Action::Resize(_, _) | Action::Noop => (false, true),
            _ => (true, true),
        }
    }
}
