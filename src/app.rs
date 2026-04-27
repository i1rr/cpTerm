use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::action::Action;
use crate::bookmarks::BookmarkList;
use crate::components::bookmark_panel::BookmarkPanel;
use crate::components::dialog::Dialog;
use crate::components::dual_pane::DualPane;
use crate::components::theme_editor::ThemeEditor;
use crate::config::{PaneSide, SessionConfig};
use crate::event::EventHandler;
use crate::task::TaskState;
use crate::theme::Theme;
use crate::tui;

mod types;
pub use types::*;
use types::PendingOp;

mod draw;
mod input;
mod dispatch_bm;
mod dispatch_theme;
mod dispatch_fs;
mod dispatch_ssh;
mod dispatch;

pub struct BackgroundedSession {
    pub session: Arc<tokio::sync::Mutex<crate::ssh::session::SshSession>>,
    pub last_path: String,
    // Stored for future display/reconnect; not read by current code.
    #[allow(dead_code)]
    pub host_label: String,
    #[allow(dead_code)]
    pub port: u16,
    #[allow(dead_code)]
    pub user: String,
    #[allow(dead_code)]
    pub host: String,
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
    /// Paths that should be highlighted as "new" after the current operation completes.
    pending_new_files: Vec<PathBuf>,
    /// Snapshot of files before archive extraction, for diffing after.
    pre_extract_snapshot: Option<(PathBuf, std::collections::HashSet<PathBuf>)>,
    /// Host currently being connected to (shown in status bar while async SSH connects).
    pub connecting_to: Option<String>,
    /// Live SSH sessions that have been backgrounded (user navigated back to local).
    /// Key: "user@host"
    pub ssh_sessions: HashMap<String, BackgroundedSession>,
    /// Path to open in external pager on next run() iteration (after TUI suspend).
    pub pending_view_file: Option<PathBuf>,
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
        let bookmarks = BookmarkList::load();
        let startup_dialog = if bookmarks.load_failed() {
            let path = BookmarkList::file_path()
                .unwrap_or_else(|| "unknown location".to_string());
            Some(Dialog::error(format!(
                "Could not read bookmarks file:\n  {}\n\nBookmarks will not be saved this session.\nFix or delete the file to restore normal operation.",
                path
            )))
        } else {
            None
        };
        Self {
            dual_pane: DualPane::new(left_dir, right_dir, active),
            input_mode: InputMode::Normal,
            dialog: startup_dialog,
            should_quit: false,
            theme,
            theme_name,
            theme_editor: None,
            bookmark_panel: None,
            bookmarks,
            task: None,
            action_tx,
            action_rx,
            pending_op: None,
            pending_new_files: Vec::new(),
            pre_extract_snapshot: None,
            connecting_to: None,
            ssh_sessions: HashMap::new(),
            pending_view_file: None,
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

            if let Some(path) = self.pending_view_file.take() {
                if let Err(e) = tui::suspend() {
                    self.dialog = Some(crate::components::dialog::Dialog::error(
                        format!("Failed to suspend TUI: {}", e),
                    ));
                } else {
                    let view_result = crate::fs::open::open_in_viewer(&path);
                    if let Err(e) = tui::resume() {
                        log::warn!("failed to resume TUI after pager: {}", e);
                    }
                    terminal.clear().ok();
                    if let Err(msg) = view_result {
                        self.dialog = Some(crate::components::dialog::Dialog::error(msg));
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

    fn save_session(&self) {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        let left = {
            let d = self.dual_pane.left_dir();
            if d.exists() { d } else { home.clone() }
        };
        let right = {
            let d = self.dual_pane.right_dir();
            if d.exists() { d } else { home }
        };
        // Preserve last_ssh from existing config so it survives session saves.
        let last_ssh = SessionConfig::load().last_ssh;
        let config = SessionConfig {
            left_dir: left,
            right_dir: right,
            active_pane: self.dual_pane.active,
            theme_name: self.theme_name.clone(),
            last_ssh,
        };
        config.save();
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::dispatch_ssh::parse_ssh_command;
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
    fn f3_maps_to_view_file() {
        let app = make_app();
        assert!(matches!(app.map_key(key(KeyCode::F(3))), Action::ViewFile));
    }

    #[test]
    fn f4_maps_to_create_new() {
        let app = make_app();
        assert!(matches!(app.map_key(key(KeyCode::F(4))), Action::CreateNew));
    }

    #[test]
    fn f7_maps_to_mkdir() {
        let app = make_app();
        assert!(matches!(app.map_key(key(KeyCode::F(7))), Action::MkDir));
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

    // ── UnpackChoice mode ─────────────────────────────────────

    #[test]
    fn unpack_choice_e_maps_to_unpack_archive() {
        let mut app = make_app();
        app.input_mode = InputMode::UnpackChoice {
            archive: PathBuf::from("/test/archive.zip"),
        };
        assert!(matches!(
            app.map_key(key(KeyCode::Char('e'))),
            Action::UnpackArchive
        ));
    }

    #[test]
    fn unpack_choice_enter_maps_to_unpack_archive() {
        let mut app = make_app();
        app.input_mode = InputMode::UnpackChoice {
            archive: PathBuf::from("/test/archive.zip"),
        };
        assert!(matches!(
            app.map_key(key(KeyCode::Enter)),
            Action::UnpackArchive
        ));
    }

    #[test]
    fn unpack_choice_f_maps_to_unpack_archive_to() {
        let mut app = make_app();
        app.input_mode = InputMode::UnpackChoice {
            archive: PathBuf::from("/test/archive.zip"),
        };
        assert!(matches!(
            app.map_key(key(KeyCode::Char('f'))),
            Action::UnpackArchiveTo { .. }
        ));
    }

    #[test]
    fn unpack_choice_c_maps_to_unpack_archive_to() {
        let mut app = make_app();
        app.input_mode = InputMode::UnpackChoice {
            archive: PathBuf::from("/test/archive.zip"),
        };
        assert!(matches!(
            app.map_key(key(KeyCode::Char('c'))),
            Action::UnpackArchiveTo { .. }
        ));
    }

    #[test]
    fn unpack_choice_esc_cancels() {
        let mut app = make_app();
        app.input_mode = InputMode::UnpackChoice {
            archive: PathBuf::from("/test/archive.zip"),
        };
        assert!(matches!(
            app.map_key(key(KeyCode::Esc)),
            Action::InputCancel
        ));
    }

    #[test]
    fn unpack_choice_other_keys_noop() {
        let mut app = make_app();
        app.input_mode = InputMode::UnpackChoice {
            archive: PathBuf::from("/test/archive.zip"),
        };
        assert!(matches!(
            app.map_key(key(KeyCode::Char('x'))),
            Action::Noop
        ));
    }

    #[test]
    fn unpack_custom_path_input_char() {
        let mut app = make_app();
        app.input_mode = InputMode::UnpackCustomPath {
            archive: PathBuf::from("/test/archive.zip"),
            text: String::new(),
        };
        assert!(matches!(
            app.map_key(key(KeyCode::Char('a'))),
            Action::InputChar('a')
        ));
    }

    #[test]
    fn unpack_custom_path_enter_confirms() {
        let mut app = make_app();
        app.input_mode = InputMode::UnpackCustomPath {
            archive: PathBuf::from("/test/archive.zip"),
            text: String::new(),
        };
        assert!(matches!(
            app.map_key(key(KeyCode::Enter)),
            Action::InputConfirm
        ));
    }

    // ── parse_ssh_command ─────────────────────────────────────

    #[test]
    fn ssh_command_host_only() {
        let (user, host, path) = parse_ssh_command("ssh 198.51.100.1").unwrap();
        assert_eq!(host, "198.51.100.1");
        assert_eq!(path, "/");
        assert!(!user.is_empty());
    }

    #[test]
    fn ssh_command_user_at_host() {
        let (user, host, path) = parse_ssh_command("ssh alice@198.51.100.1").unwrap();
        assert_eq!(user, "alice");
        assert_eq!(host, "198.51.100.1");
        assert_eq!(path, "/");
    }

    #[test]
    fn ssh_command_user_at_host_with_path() {
        let (user, host, path) = parse_ssh_command("ssh alice@198.51.100.1:/home/alice").unwrap();
        assert_eq!(user, "alice");
        assert_eq!(host, "198.51.100.1");
        assert_eq!(path, "/home/alice");
    }

    #[test]
    fn ssh_command_host_with_path_no_user() {
        let (user, host, path) = parse_ssh_command("ssh 198.51.100.1:/srv").unwrap();
        assert_eq!(host, "198.51.100.1");
        assert_eq!(path, "/srv");
        assert!(!user.is_empty());
    }

    #[test]
    fn ssh_command_empty_path_after_colon_defaults_to_root() {
        let (_, _, path) = parse_ssh_command("ssh alice@host:").unwrap();
        assert_eq!(path, "/");
    }

    #[test]
    fn ssh_command_returns_none_for_non_ssh() {
        assert!(parse_ssh_command("cd /tmp").is_none());
        assert!(parse_ssh_command("ls -la").is_none());
        assert!(parse_ssh_command("ssh").is_none());
        assert!(parse_ssh_command("ssh ").is_none());
    }

    #[test]
    fn ctrl_o_maps_to_ssh_connect() {
        let app = make_app();
        assert!(matches!(app.map_key(ctrl('o')), Action::SshConnect));
    }

    #[test]
    fn dispatch_ssh_connect_opens_dialog() {
        let mut app = make_app();
        app.dispatch(Action::SshConnect);
        assert!(matches!(app.input_mode, InputMode::SshConnect(_)));
    }
}
