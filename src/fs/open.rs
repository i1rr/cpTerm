use std::path::Path;

pub fn open_file(path: &Path) -> Result<(), String> {
    open::that(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))
}

/// Resolve the pager binary to use: $PAGER -> less (Unix) / more (Windows).
pub fn resolve_pager() -> String {
    if let Ok(p) = std::env::var("PAGER")
        && !p.trim().is_empty()
    {
        return p;
    }
    if cfg!(windows) {
        "more".to_string()
    } else {
        "less".to_string()
    }
}

/// Open `path` in the external pager. The caller is responsible for
/// suspending and resuming the TUI around this call.
pub fn open_in_viewer(path: &Path) -> Result<(), String> {
    let pager = resolve_pager();
    std::process::Command::new(&pager)
        .arg(path)
        .status()
        .map(|_| ())
        .map_err(|e| format!("Failed to open pager '{}': {}", pager, e))
}

/// Resolve the editor binary to use, following the chain:
/// $EDITOR -> $VISUAL -> nano -> vi (Unix) / notepad (Windows)
#[allow(dead_code)]
pub fn resolve_editor() -> String {
    if let Ok(e) = std::env::var("EDITOR")
        && !e.trim().is_empty()
    {
        return e;
    }
    if let Ok(e) = std::env::var("VISUAL")
        && !e.trim().is_empty()
    {
        return e;
    }
    if cfg!(windows) {
        "notepad".to_string()
    } else {
        // Try nano first, fall back to vi
        if std::process::Command::new("nano")
            .arg("--version")
            .output()
            .is_ok()
        {
            "nano".to_string()
        } else {
            "vi".to_string()
        }
    }
}
