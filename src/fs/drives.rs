use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Mount,
    Root,
}

impl DriveKind {
    pub fn as_label(&self) -> &'static str {
        match self {
            DriveKind::Fixed => "Fixed",
            DriveKind::Removable => "Removable",
            DriveKind::Network => "Network",
            DriveKind::Mount => "Mount",
            DriveKind::Root => "Root",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DriveEntry {
    pub label: String,
    pub path: PathBuf,
    pub kind: DriveKind,
}

#[cfg(target_os = "windows")]
pub fn list_drives() -> Vec<DriveEntry> {
    let mut out = Vec::new();
    for letter in b'A'..=b'Z' {
        let root = format!("{}:\\", letter as char);
        let path = PathBuf::from(&root);
        if path.exists() {
            out.push(DriveEntry {
                label: format!("{}:", letter as char),
                path,
                kind: DriveKind::Fixed,
            });
        }
    }
    out
}

#[cfg(target_os = "macos")]
pub fn list_drives() -> Vec<DriveEntry> {
    let mut out = vec![DriveEntry {
        label: "/".to_string(),
        path: PathBuf::from("/"),
        kind: DriveKind::Root,
    }];
    if let Ok(rd) = std::fs::read_dir("/Volumes") {
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let label = entry.file_name().to_string_lossy().into_owned();
            out.push(DriveEntry {
                label,
                path,
                kind: DriveKind::Mount,
            });
        }
    }
    out.sort_by_key(|d| d.label.to_lowercase());
    out
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn list_drives() -> Vec<DriveEntry> {
    let mut out = vec![DriveEntry {
        label: "/".to_string(),
        path: PathBuf::from("/"),
        kind: DriveKind::Root,
    }];

    let user = std::env::var("USER").ok();

    let mut roots: Vec<PathBuf> = vec![PathBuf::from("/mnt"), PathBuf::from("/media")];
    if let Some(ref u) = user {
        roots.push(PathBuf::from(format!("/media/{}", u)));
        roots.push(PathBuf::from(format!("/run/media/{}", u)));
    }

    let mut seen = std::collections::HashSet::new();
    seen.insert(PathBuf::from("/"));

    for root in &roots {
        let Ok(rd) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_dir() || !seen.insert(path.clone()) {
                continue;
            }
            let label = entry.file_name().to_string_lossy().into_owned();
            out.push(DriveEntry {
                label,
                path,
                kind: DriveKind::Mount,
            });
        }
    }

    // Sort everything except the leading Root entry.
    let tail = &mut out[1..];
    tail.sort_by_key(|d| d.label.to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_drives_returns_entries() {
        let drives = list_drives();
        assert!(!drives.is_empty(), "expected at least one drive entry");
    }

    #[cfg(unix)]
    #[test]
    fn unix_includes_root() {
        use std::path::Path;
        let drives = list_drives();
        assert!(
            drives.iter().any(|d| d.path == Path::new("/")),
            "expected root '/' in drives list"
        );
    }
}
