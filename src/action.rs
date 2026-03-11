/// All application actions (messages).
#[derive(Debug, Clone)]
pub enum Action {
    Tick,
    Render,
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
    InvertSelection,
    // Operations
    CopySelected,
    MoveSelected,
    DeleteSelected,
    OpenFile,
    Rename,
    MkDir,
    Refresh,
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
    ToggleHidden,
    // Input mode
    InputChar(char),
    InputBackspace,
    InputConfirm,
    InputCancel,
    Error(String),
    Noop,
}
