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
}

impl BookmarkList {
    pub fn load() -> Self {
        let file: BookmarkFile = confy::load("cpt", Some("bookmarks")).unwrap_or_default();
        Self {
            entries: file.bookmarks,
        }
    }

    pub fn save(&self) {
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
}
