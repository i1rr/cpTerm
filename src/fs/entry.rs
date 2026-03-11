use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<DateTime<Local>>,
    pub is_hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Name,
    Size,
    Date,
}

impl FileEntry {
    pub fn from_path(path: &Path) -> Option<Self> {
        let meta = path.metadata().ok()?;
        let name = path.file_name()?.to_string_lossy().to_string();
        let is_hidden = name.starts_with('.');

        #[cfg(windows)]
        let is_hidden = is_hidden || {
            use std::os::windows::fs::MetadataExt;
            meta.file_attributes() & 0x2 != 0 // FILE_ATTRIBUTE_HIDDEN
        };

        let modified = meta
            .modified()
            .ok()
            .map(|t| DateTime::<Local>::from(t));

        Some(Self {
            name,
            path: path.to_path_buf(),
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified,
            is_hidden,
        })
    }
}

pub fn read_directory(dir: &Path) -> Vec<FileEntry> {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            log::warn!("failed to read directory {}: {}", dir.display(), e);
            return Vec::new();
        }
    };

    let mut entries: Vec<FileEntry> = read_dir
        .filter_map(|e| e.ok())
        .filter_map(|e| FileEntry::from_path(&e.path()))
        .collect();

    sort_entries(&mut entries, SortColumn::Name, true);
    entries
}

pub fn sort_entries(entries: &mut [FileEntry], column: SortColumn, ascending: bool) {
    entries.sort_by(|a, b| {
        // Dirs always first
        let dir_cmp = b.is_dir.cmp(&a.is_dir);
        if dir_cmp != std::cmp::Ordering::Equal {
            return dir_cmp;
        }

        let cmp = match column {
            SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortColumn::Size => a.size.cmp(&b.size),
            SortColumn::Date => a.modified.cmp(&b.modified),
        };

        if ascending { cmp } else { cmp.reverse() }
    });
}
