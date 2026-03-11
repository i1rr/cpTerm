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
    Rename,
    MkDir,
    CreateFile,
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
    Error(String),
    Noop,
}
