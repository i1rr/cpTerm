use std::collections::HashSet;
use std::path::PathBuf;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::action::Action;
use crate::config::PaneSide;
use crate::theme::Theme;

use super::editor_pane::EditorPane;
use super::explorer::Explorer;

/// Contents of a single pane slot - either a file explorer or an embedded editor.
pub enum PaneContent {
    Explorer(Explorer),
    Editor(Box<EditorPane>),
}

impl PaneContent {
    fn refresh(&mut self) {
        if let PaneContent::Explorer(e) = self {
            e.refresh();
        }
    }

    /// Return the "current directory" for this pane regardless of its type.
    pub fn current_dir(&self) -> PathBuf {
        match self {
            PaneContent::Explorer(e) => e.current_dir.clone(),
            PaneContent::Editor(e) => e.origin_dir.clone(),
        }
    }
}

pub struct DualPane {
    pub left: PaneContent,
    pub right: PaneContent,
    pub active: PaneSide,
    /// When true, the active editor fills the whole dual-pane area.
    pub editor_fullscreen: bool,
}

impl DualPane {
    pub fn new(left_dir: PathBuf, right_dir: PathBuf, active: PaneSide) -> Self {
        Self {
            left: PaneContent::Explorer(Explorer::new(left_dir)),
            right: PaneContent::Explorer(Explorer::new(right_dir)),
            active,
            editor_fullscreen: false,
        }
    }

    fn active_content(&self) -> &PaneContent {
        match self.active {
            PaneSide::Left => &self.left,
            PaneSide::Right => &self.right,
        }
    }

    fn active_content_mut(&mut self) -> &mut PaneContent {
        match self.active {
            PaneSide::Left => &mut self.left,
            PaneSide::Right => &mut self.right,
        }
    }

    fn inactive_content(&self) -> &PaneContent {
        match self.active {
            PaneSide::Left => &self.right,
            PaneSide::Right => &self.left,
        }
    }

    pub fn active_explorer(&self) -> Option<&Explorer> {
        match self.active_content() {
            PaneContent::Explorer(e) => Some(e),
            PaneContent::Editor(_) => None,
        }
    }

    pub fn active_explorer_mut(&mut self) -> Option<&mut Explorer> {
        match self.active_content_mut() {
            PaneContent::Explorer(e) => Some(e),
            PaneContent::Editor(_) => None,
        }
    }

    pub fn active_editor(&self) -> Option<&EditorPane> {
        match self.active_content() {
            PaneContent::Editor(e) => Some(e),
            PaneContent::Explorer(_) => None,
        }
    }

    pub fn active_editor_mut(&mut self) -> Option<&mut EditorPane> {
        match self.active_content_mut() {
            PaneContent::Editor(e) => Some(e),
            PaneContent::Explorer(_) => None,
        }
    }

    pub fn active_dir(&self) -> PathBuf {
        self.active_content().current_dir()
    }

    pub fn inactive_dir(&self) -> PathBuf {
        self.inactive_content().current_dir()
    }

    pub fn left_dir(&self) -> PathBuf {
        self.left.current_dir()
    }

    pub fn right_dir(&self) -> PathBuf {
        self.right.current_dir()
    }

    pub fn refresh_both(&mut self) {
        self.left.refresh();
        self.right.refresh();
    }

    /// Mark paths as "new files" in whichever explorer pane contains them.
    /// These files will be highlighted until the user focuses on them.
    pub fn mark_new_files(&mut self, paths: &[PathBuf]) {
        for path in paths {
            let parent = path.parent().map(|p| p.to_path_buf());
            if let PaneContent::Explorer(e) = &mut self.left {
                if parent.as_ref() == Some(&e.current_dir) {
                    e.new_files.insert(path.clone());
                }
            }
            if let PaneContent::Explorer(e) = &mut self.right {
                if parent.as_ref() == Some(&e.current_dir) {
                    e.new_files.insert(path.clone());
                }
            }
        }
    }

    /// Snapshot file names in a directory across both panes (for diff after extraction).
    pub fn snapshot_dir(&self, dir: &std::path::Path) -> HashSet<PathBuf> {
        let mut paths = HashSet::new();
        for pane in [&self.left, &self.right] {
            if let PaneContent::Explorer(e) = pane {
                if e.current_dir == dir {
                    for entry in &e.entries {
                        paths.insert(entry.path.clone());
                    }
                }
            }
        }
        if paths.is_empty() {
            // Dir not currently shown, read it directly
            if let Ok(rd) = std::fs::read_dir(dir) {
                for entry in rd.flatten() {
                    paths.insert(entry.path());
                }
            }
        }
        paths
    }

    pub fn open_editor_in_active(&mut self, path: PathBuf) -> Result<(), String> {
        let origin_dir = self.active_dir();
        let editor = EditorPane::open(path, origin_dir, false)?;
        match self.active {
            PaneSide::Left => self.left = PaneContent::Editor(Box::new(editor)),
            PaneSide::Right => self.right = PaneContent::Editor(Box::new(editor)),
        }
        Ok(())
    }

    pub fn close_editor_in_active(&mut self) {
        let origin_dir = match self.active_content() {
            PaneContent::Editor(e) => e.origin_dir.clone(),
            PaneContent::Explorer(_) => return,
        };
        self.editor_fullscreen = false;
        match self.active {
            PaneSide::Left => self.left = PaneContent::Explorer(Explorer::new(origin_dir)),
            PaneSide::Right => self.right = PaneContent::Explorer(Explorer::new(origin_dir)),
        }
    }

    pub fn handle_editor_key(&mut self, key: KeyEvent) -> Option<Action> {
        if let Some(editor) = self.active_editor_mut() {
            editor.handle_key(key)
        } else {
            None
        }
    }

    pub fn handle_action(&mut self, action: &Action) -> Option<Action> {
        match action {
            Action::SwitchPane => {
                self.editor_fullscreen = false;
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
                self.refresh_both();
                None
            }
            _ => {
                if let PaneContent::Explorer(explorer) = self.active_content_mut() {
                    explorer.handle_action(action)
                } else {
                    None
                }
            }
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if self.editor_fullscreen {
            // Give the entire area to the active pane (must be an editor).
            match self.active {
                PaneSide::Left => match &mut self.left {
                    PaneContent::Editor(e) => e.draw(frame, area, true, theme),
                    PaneContent::Explorer(e) => e.draw(frame, area, true, theme),
                },
                PaneSide::Right => match &mut self.right {
                    PaneContent::Editor(e) => e.draw(frame, area, true, theme),
                    PaneContent::Explorer(e) => e.draw(frame, area, true, theme),
                },
            }
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let left_active = self.active == PaneSide::Left;
        let right_active = self.active == PaneSide::Right;

        match &mut self.left {
            PaneContent::Explorer(e) => e.draw(frame, chunks[0], left_active, theme),
            PaneContent::Editor(e) => e.draw(frame, chunks[0], left_active, theme),
        }
        match &mut self.right {
            PaneContent::Explorer(e) => e.draw(frame, chunks[1], right_active, theme),
            PaneContent::Editor(e) => e.draw(frame, chunks[1], right_active, theme),
        }
    }
}

impl PaneContent {
    #[allow(dead_code)]
    pub fn as_explorer(&self) -> Option<&Explorer> {
        match self {
            PaneContent::Explorer(e) => Some(e),
            PaneContent::Editor(_) => None,
        }
    }

    #[allow(dead_code)]
    pub fn as_explorer_mut(&mut self) -> Option<&mut Explorer> {
        match self {
            PaneContent::Explorer(e) => Some(e),
            PaneContent::Editor(_) => None,
        }
    }
}
