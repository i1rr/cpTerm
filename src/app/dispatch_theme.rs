use crate::action::Action;
use crate::components::dialog::Dialog;
use crate::components::theme_editor::ThemeEditor;
use crate::theme::Theme;

use super::App;

impl App {
    /// Handle actions when the theme editor is open.
    /// Returns true if the action was consumed.
    pub(super) fn dispatch_theme_editor(
        &mut self,
        te: &mut ThemeEditor,
        action: Action,
    ) -> bool {
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
        if te.naming.is_some() {
            return self.dispatch_theme_naming(te, action);
        }
        // Picker sub-state
        if te.picker.is_some() {
            return self.dispatch_theme_picker(te, action);
        }
        // Normal editor
        self.dispatch_theme_normal(te, action)
    }

    pub(super) fn dispatch_theme_naming(
        &mut self,
        te: &mut ThemeEditor,
        action: Action,
    ) -> bool {
        match action {
            Action::InputChar(c) => {
                // Only allow valid filename chars
                if (c.is_alphanumeric() || c == '-' || c == '_')
                    && let Some(ref mut name) = te.naming {
                    name.push(c);
                }
                true
            }
            Action::InputBackspace => {
                if let Some(ref mut name) = te.naming {
                    name.pop();
                }
                true
            }
            Action::InputConfirm => {
                let name = te.naming.take().unwrap_or_default();
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
                te.naming = None;
                true
            }
            Action::Quit | Action::Tick | Action::Resize(_, _) | Action::Noop => false,
            _ => true,
        }
    }

    pub(super) fn dispatch_theme_picker(
        &mut self,
        te: &mut ThemeEditor,
        action: Action,
    ) -> bool {
        // Compute what to do without holding mutable borrows
        enum PickerOp {
            Move,
            Confirm,
            Cancel(ratatui::style::Color),
            Swallow,
            PassThrough,
        }

        let op = {
            let picker = te.picker.as_mut().unwrap();
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
                let picker = te.picker.as_ref().unwrap();
                let field_idx = te.cursor;
                let color = picker.current_color();
                self.theme.set_field(field_idx, color);
                true
            }
            PickerOp::Confirm => {
                te.picker = None;
                true
            }
            PickerOp::Cancel(original) => {
                let field_idx = te.cursor;
                te.picker = None;
                self.theme.set_field(field_idx, original);
                true
            }
            PickerOp::Swallow => true,
            PickerOp::PassThrough => false,
        }
    }

    pub(super) fn dispatch_theme_normal(
        &mut self,
        te: &mut ThemeEditor,
        action: Action,
    ) -> bool {
        match action {
            Action::MoveUp => {
                te.move_up();
                true
            }
            Action::MoveDown => {
                te.move_down();
                true
            }
            Action::MoveToTop => {
                te.move_to_top();
                true
            }
            Action::MoveToBottom => {
                te.move_to_bottom();
                true
            }
            Action::PageUp => {
                te.page_up();
                true
            }
            Action::PageDown => {
                te.page_down();
                true
            }
            Action::ThemeEditorOpenPicker => {
                let field_idx = te.cursor;
                let current_color = self.theme.get_field(field_idx);
                te.open_picker(current_color);
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
                    te.start_naming();
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
                te.start_naming();
                true
            }
            Action::ThemeEditorOpenFile => {
                if Theme::is_builtin(&self.theme_name) {
                    // Built-in themes have no file - save as a custom theme first
                    te.start_naming();
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
}
