use std::path::PathBuf;

use chrono::{DateTime, Local};

/// Canonicalize a path, stripping the Windows `\\?\` extended-length prefix.
pub fn clean_canonicalize(path: &std::path::Path) -> std::io::Result<PathBuf> {
    let resolved = std::fs::canonicalize(path)?;
    Ok(strip_unc_prefix(resolved))
}

/// Strip the `\\?\` prefix that Windows canonicalize produces.
pub fn strip_unc_prefix(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

pub fn format_date(dt: &DateTime<Local>) -> String {
    dt.format("%m-%d %H:%M").to_string()
}
