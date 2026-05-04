use crate::action::Action;
use crate::components::dialog::Dialog;
use crate::components::drives_panel::DrivesPanel;

use super::App;

impl App {
    /// Returns (consumed, keep_panel). keep_panel=false closes the panel.
    pub(super) fn dispatch_drives_panel(
        &mut self,
        dp: &mut DrivesPanel,
        action: Action,
    ) -> (bool, bool) {
        if self.dialog.is_some() {
            match action {
                Action::DismissDialog
                | Action::ConfirmDialog
                | Action::DialogScrollUp
                | Action::DialogScrollDown => return (false, true),
                _ => {}
            }
        }

        match action {
            Action::MoveUp => {
                dp.move_up();
                (true, true)
            }
            Action::MoveDown => {
                dp.move_down();
                (true, true)
            }
            Action::MoveToTop => {
                dp.cursor = 0;
                (true, true)
            }
            Action::MoveToBottom => {
                if !dp.drives.is_empty() {
                    dp.cursor = dp.drives.len() - 1;
                }
                (true, true)
            }
            Action::DriveNavigate => {
                if let Some(drive) = dp.drives.get(dp.cursor) {
                    let path = drive.path.clone();
                    if path.is_dir() {
                        if let Some(explorer) = self.dual_pane.active_explorer_mut() {
                            explorer.current_dir = path;
                            explorer.filter_text = None;
                            explorer.cursor = 0;
                            explorer.refresh();
                            return (true, false);
                        }
                    } else {
                        self.dialog = Some(Dialog::error(format!(
                            "Drive path not accessible: {}",
                            path.display()
                        )));
                    }
                }
                (true, true)
            }
            Action::DriveClose => (true, false),
            Action::Quit | Action::Tick | Action::Resize(_, _) | Action::Noop => (false, true),
            _ => (true, true),
        }
    }
}
