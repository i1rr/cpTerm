use std::path::PathBuf;
use std::sync::Arc;

use crate::config::PaneSide;

/// All application actions (messages).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Action {
    Tick,
    Resize(u16, u16),
    Quit,
    // Navigation
    MoveUp,
    MoveDown,
    MoveToTop,
    MoveToBottom,
    PageUp,
    PageDown,
    EnterDir,
    ParentDir,
    // Focus
    SwitchPane,
    FocusLeft,
    FocusRight,
    // Selection
    ToggleSelect,
    SelectAll,
    DeselectAll,
    // Operations
    CopySelected,
    MoveSelected,
    DeleteSelected,
    OpenFile,
    /// Open `path` in the embedded in-pane editor.
    OpenEditor {
        path: PathBuf,
    },
    /// Close the active editor pane (restores the explorer).
    CloseEditor,
    /// Save the active editor pane's content to disk, then close it.
    SaveAndCloseEditor,
    /// Close the active editor pane without saving.
    DiscardAndCloseEditor,
    /// Save the active editor pane's content to disk.
    SaveEditor,
    /// Forward a raw key event to the active editor pane.
    EditorKeyInput(crossterm::event::KeyEvent),
    ViewFile,
    UnpackArchive,
    /// Unpack archive to a specific destination path.
    UnpackArchiveTo {
        dest: PathBuf,
    },
    /// Open the context menu for the current file.
    OpenContextMenu,
    /// Open the file as text in the embedded editor (forced, even if binary).
    OpenAsText,
    /// Open the file/directory with the OS default application.
    OpenWithDefault,
    Rename,
    MkDir,
    CreateFile,
    CreateNew,
    Refresh,
    ConflictOverwrite,
    ConflictRename,
    // Filter
    StartFilter,
    FilterInput(char),
    FilterBackspace,
    FilterConfirm,
    FilterCancel,
    // Async feedback
    OperationProgress {
        done: u64,
        total: u64,
    },
    OperationComplete(String),
    OperationError(String),
    // UI
    ShowHelp,
    DismissDialog,
    ConfirmDialog,
    DialogScrollUp,
    DialogScrollDown,
    // Input mode
    InputChar(char),
    InputBackspace,
    InputConfirm,
    InputCancel,
    StartCommand(char),
    // Bookmarks
    OpenBookmarks,
    BookmarkNavigate,
    BookmarkAdd,
    BookmarkRemove,
    BookmarkClose,
    // Drives / volumes
    /// Open the drives panel. If `for_side` is set, focus that pane first.
    OpenDrives {
        for_side: Option<PaneSide>,
    },
    DriveNavigate,
    DriveClose,
    // Theme editor
    OpenThemeEditor,
    ThemeEditorClose,
    ThemeEditorCycleBase,
    ThemeEditorSave,
    ThemeEditorSaveAs,
    ThemeEditorOpenFile,
    ThemeEditorDelete,
    ThemeEditorOpenPicker,
    ThemeEditorPickerLeft,
    ThemeEditorPickerRight,
    /// Toggle the active editor pane to fill the entire dual-pane area.
    ToggleEditorFullscreen,
    // Task runner
    TaskLine(String),
    TaskComplete(i32),
    TaskError(String),
    TaskMinimize,
    TaskRestore,
    TaskDismiss,
    TaskScrollUp,
    TaskScrollDown,
    Error(String),
    Noop,
    // SSH remote browsing
    SshConnect,
    SshDisconnect,
    SshConnectNextField,
    SshConnectPrevField,
    SshConnectConfirm,
    SshConnected {
        session: Arc<tokio::sync::Mutex<crate::ssh::session::SshSession>>,
        host: String,
        port: u16,
        user: String,
        initial_path: String,
        entries: Vec<crate::ssh::session::RemoteEntry>,
    },
    SshConnectionFailed(String),
    /// Move active remote pane to background (keep session alive) and return to local.
    SshBackground,
    /// Backgrounded session is no longer alive; remove it and open the connect dialog.
    SshSessionExpired {
        key: String,
    },
    /// Key auth failed when connecting via command bar; open dialog pre-filled for password entry.
    SshPasswordRequired {
        host: String,
        port: u16,
        user: String,
        path: String,
    },
    RemoteNavigate(String),
    RemoteListing {
        path: String,
        entries: Vec<crate::ssh::session::RemoteEntry>,
    },
    RemoteListingFailed(String),
    RemoteViewFile,
    RemoteViewReady(PathBuf),
    RemoteDownload,
    RemoteUpload,
    RemoteOpComplete(String),
    RemoteOpError(String),
}
