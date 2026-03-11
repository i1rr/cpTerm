use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::action::Action;
use crate::config::PaneSide;
use crate::theme::Theme;

use super::explorer::Explorer;

pub struct DualPane {
    pub left: Explorer,
    pub right: Explorer,
    pub active: PaneSide,
}

impl DualPane {
    pub fn new(left_dir: PathBuf, right_dir: PathBuf, active: PaneSide) -> Self {
        Self {
            left: Explorer::new(left_dir),
            right: Explorer::new(right_dir),
            active,
        }
    }

    pub fn active_explorer(&self) -> &Explorer {
        match self.active {
            PaneSide::Left => &self.left,
            PaneSide::Right => &self.right,
        }
    }

    pub fn active_explorer_mut(&mut self) -> &mut Explorer {
        match self.active {
            PaneSide::Left => &mut self.left,
            PaneSide::Right => &mut self.right,
        }
    }

    pub fn inactive_explorer(&self) -> &Explorer {
        match self.active {
            PaneSide::Left => &self.right,
            PaneSide::Right => &self.left,
        }
    }

    pub fn inactive_dir(&self) -> PathBuf {
        self.inactive_explorer().current_dir.clone()
    }

    pub fn handle_action(&mut self, action: &Action) -> Option<Action> {
        match action {
            Action::SwitchPane => {
                self.active = match self.active {
                    PaneSide::Left => PaneSide::Right,
                    PaneSide::Right => PaneSide::Left,
                };
                None
            }
            Action::FocusLeft => {
                self.active = PaneSide::Left;
                None
            }
            Action::FocusRight => {
                self.active = PaneSide::Right;
                None
            }
            Action::Refresh => {
                self.left.refresh();
                self.right.refresh();
                None
            }
            _ => self.active_explorer_mut().handle_action(action),
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        self.left
            .draw(frame, chunks[0], self.active == PaneSide::Left, theme);
        self.right
            .draw(frame, chunks[1], self.active == PaneSide::Right, theme);
    }
}
