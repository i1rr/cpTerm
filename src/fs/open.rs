use std::path::Path;

pub fn open_file(path: &Path) -> Result<(), String> {
    open::that(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))
}

/// Resolve the editor binary to use, following the chain:
/// $EDITOR -> $VISUAL -> nano -> vi (Unix) / notepad (Windows)
pub fn resolve_editor() -> String {
    if let Ok(e) = std::env::var("EDITOR") {
        if !e.trim().is_empty() {
            return e;
        }
    }
    if let Ok(e) = std::env::var("VISUAL") {
        if !e.trim().is_empty() {
            return e;
        }
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

/// Resolve the pager binary to use: $PAGER -> less -> more
pub fn resolve_pager() -> String {
    if let Ok(p) = std::env::var("PAGER") {
        if !p.trim().is_empty() {
            return p;
        }
    }
    if cfg!(windows) {
        "more".to_string()
    } else {
        "less".to_string()
    }
}

/// Open `path` in the user's editor (blocking - caller must suspend TUI first).
pub fn open_in_editor(path: &Path) -> Result<(), String> {
    let editor = resolve_editor();
    std::process::Command::new(&editor)
        .arg(path)
        .status()
        .map_err(|e| format!("Failed to launch editor '{}': {}", editor, e))?;
    Ok(())
}

/// Open `path` in the user's pager for read-only viewing (blocking - caller must suspend TUI first).
pub fn open_in_viewer(path: &Path) -> Result<(), String> {
    let pager = resolve_pager();
    std::process::Command::new(&pager)
        .arg(path)
        .status()
        .map_err(|e| format!("Failed to launch pager '{}': {}", pager, e))?;
    Ok(())
}
