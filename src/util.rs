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

/// Return true if the filename looks like a supported archive.
pub fn is_archive(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".zip")
        || lower.ends_with(".7z")
        || lower.ends_with(".rar")
        || lower.ends_with(".tar")
        || lower.ends_with(".gz")
        || lower.ends_with(".bz2")
        || lower.ends_with(".xz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tar.xz")
}

/// Find the 7-Zip binary on PATH.
/// Tries `7z`, `7za`, and `7zz` in order - returns the first that is found.
pub fn find_sevenzip() -> Option<String> {
    for candidate in &["7z", "7za", "7zz"] {
        if std::process::Command::new(candidate)
            .arg("i")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            return Some((*candidate).to_string());
        }
    }
    None
}
