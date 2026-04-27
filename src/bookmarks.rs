use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookmarkEntry {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct BookmarkFile {
    bookmarks: Vec<BookmarkEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct BookmarkList {
    pub entries: Vec<BookmarkEntry>,
    /// Set when the on-disk file exists but could not be parsed.
    /// Saves are skipped to avoid overwriting potentially recoverable data.
    load_failed: bool,
}

impl BookmarkList {
    pub fn load() -> Self {
        match confy::load::<BookmarkFile>("cpt", Some("bookmarks")) {
            Ok(file) => Self {
                entries: file.bookmarks,
                load_failed: false,
            },
            Err(e) => {
                let path = confy::get_configuration_file_path("cpt", Some("bookmarks"))
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "unknown location".to_string());
                log::warn!(
                    "failed to load bookmarks from {}: {} - saves paused to protect the file",
                    path,
                    e
                );
                Self {
                    entries: Vec::new(),
                    load_failed: true,
                }
            }
        }
    }

    /// Returns true if the bookmarks file could not be read on last load.
    /// When true, saves are skipped to avoid overwriting the original file.
    pub fn load_failed(&self) -> bool {
        self.load_failed
    }

    /// Returns the path to the bookmarks file, for use in error messages.
    pub fn file_path() -> Option<String> {
        confy::get_configuration_file_path("cpt", Some("bookmarks"))
            .ok()
            .map(|p| p.display().to_string())
    }

    pub fn save(&self) {
        if self.load_failed {
            log::warn!(
                "bookmarks not saved: previous load failed - fix or delete {} to restore saving",
                Self::file_path().as_deref().unwrap_or("the bookmarks file")
            );
            return;
        }
        let file = BookmarkFile {
            bookmarks: self.entries.clone(),
        };
        if let Err(e) = confy::store("cpt", Some("bookmarks"), &file) {
            log::warn!("failed to save bookmarks: {}", e);
        }
    }

    /// Add a bookmark if no entry with the same path already exists.
    pub fn add(&mut self, name: String, path: PathBuf) {
        if !self.entries.iter().any(|e| e.path == path) {
            self.entries.push(BookmarkEntry { name, path });
            self.save();
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
            self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_list(paths: &[(&str, &str)]) -> BookmarkList {
        BookmarkList {
            entries: paths
                .iter()
                .map(|(name, path)| BookmarkEntry {
                    name: name.to_string(),
                    path: PathBuf::from(path),
                })
                .collect(),
            load_failed: false,
        }
    }

    #[test]
    fn add_new_bookmark() {
        let mut list = BookmarkList::default();
        list.add("home".to_string(), PathBuf::from("/home/user"));
        assert_eq!(list.entries.len(), 1);
        assert_eq!(list.entries[0].name, "home");
    }

    #[test]
    fn add_duplicate_path_ignored() {
        let mut list = BookmarkList::default();
        list.add("home".to_string(), PathBuf::from("/home/user"));
        list.add("also home".to_string(), PathBuf::from("/home/user"));
        assert_eq!(list.entries.len(), 1);
    }

    #[test]
    fn remove_by_index() {
        let mut list = make_list(&[("a", "/a"), ("b", "/b"), ("c", "/c")]);
        list.remove(1);
        assert_eq!(list.entries.len(), 2);
        assert_eq!(list.entries[0].name, "a");
        assert_eq!(list.entries[1].name, "c");
    }

    #[test]
    fn remove_out_of_bounds_is_noop() {
        let mut list = make_list(&[("a", "/a")]);
        list.remove(5);
        assert_eq!(list.entries.len(), 1);
    }

    #[test]
    fn remove_from_empty_is_noop() {
        let mut list = BookmarkList::default();
        list.remove(0);
        assert!(list.entries.is_empty());
    }

    #[test]
    fn load_failed_flag_is_false_on_default() {
        let list = BookmarkList::default();
        assert!(!list.load_failed());
    }

    #[test]
    fn load_failed_flag_blocks_save_but_allows_in_memory_add() {
        // Simulate a corrupt-file load: entries empty, load_failed set.
        let mut list = BookmarkList {
            entries: Vec::new(),
            load_failed: true,
        };
        // In-memory add still works (user can navigate to bookmarks in-session).
        list.entries.push(BookmarkEntry {
            name: "test".to_string(),
            path: PathBuf::from("/tmp/test"),
        });
        assert_eq!(list.entries.len(), 1);
        assert!(list.load_failed());
        // save() would log a warning and skip writing; we just verify it doesn't panic.
        list.save();
    }

    #[test]
    fn healthy_list_is_not_load_failed() {
        let list = make_list(&[("a", "/a"), ("b", "/b")]);
        assert!(!list.load_failed());
    }
}
