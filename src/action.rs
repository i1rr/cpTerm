use std::path::PathBuf;

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
    OpenEditor { path: PathBuf },
    /// Close the active editor pane (restores the explorer).
    CloseEditor,
    /// Save the active editor pane's content to disk.
    SaveEditor,
    /// Forward a raw key event to the active editor pane.
    EditorKeyInput(crossterm::event::KeyEvent),
    ViewFile,
    UnpackArchive,
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
    OperationProgress { done: u64, total: u64 },
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
}
