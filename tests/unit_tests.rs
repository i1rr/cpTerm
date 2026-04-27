use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Local};

// We test the public/internal logic by importing from the crate
use cpt::fs::entry::{FileEntry, SortColumn, read_directory, sort_entries};
use cpt::util::{format_date, format_size};

// ── format_size ──────────────────────────────────────────────

#[test]
fn format_size_bytes() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(1023), "1023 B");
}

#[test]
fn format_size_kilobytes() {
    assert_eq!(format_size(1024), "1.0 KB");
    assert_eq!(format_size(1536), "1.5 KB");
    assert_eq!(format_size(1024 * 1024 - 1), "1024.0 KB");
}

#[test]
fn format_size_megabytes() {
    assert_eq!(format_size(1024 * 1024), "1.0 MB");
    assert_eq!(format_size(1024 * 1024 * 500), "500.0 MB");
}

#[test]
fn format_size_gigabytes() {
    assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    assert_eq!(format_size(1024u64 * 1024 * 1024 * 3), "3.0 GB");
}

// ── format_date ──────────────────────────────────────────────

#[test]
fn format_date_produces_expected_pattern() {
    let dt: DateTime<Local> = Local::now();
    let result = format_date(&dt);
    // format is "MM-DD HH:MM"
    assert_eq!(result.len(), 11);
    assert_eq!(result.as_bytes()[2], b'-');
    assert_eq!(result.as_bytes()[5], b' ');
    assert_eq!(result.as_bytes()[8], b':');
}

// ── FileEntry::from_path ─────────────────────────────────────

#[test]
fn file_entry_from_existing_file() {
    let dir = tempdir("entry_file");
    let file_path = dir.join("test.txt");
    fs::write(&file_path, "hello").unwrap();

    let entry = FileEntry::from_path(&file_path).unwrap();
    assert_eq!(entry.name, "test.txt");
    assert!(!entry.is_dir);
    assert_eq!(entry.size, 5);
    assert!(entry.modified.is_some());
    assert_eq!(entry.path, file_path);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn file_entry_from_directory() {
    let dir = tempdir("entry_dir");
    let sub = dir.join("subdir");
    fs::create_dir(&sub).unwrap();

    let entry = FileEntry::from_path(&sub).unwrap();
    assert_eq!(entry.name, "subdir");
    assert!(entry.is_dir);
    assert_eq!(entry.size, 0);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn file_entry_from_nonexistent_returns_none() {
    let result = FileEntry::from_path(&PathBuf::from("/nonexistent_path_abc123"));
    assert!(result.is_none());
}

#[test]
fn file_entry_hidden_dot_prefix() {
    let dir = tempdir("entry_hidden");
    let hidden = dir.join(".hidden");
    fs::write(&hidden, "").unwrap();

    let entry = FileEntry::from_path(&hidden).unwrap();
    assert!(entry.is_hidden);

    fs::remove_dir_all(&dir).ok();
}

// ── sort_entries ─────────────────────────────────────────────

fn make_entry(name: &str, is_dir: bool, size: u64) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: PathBuf::from(name),
        is_dir,
        size,
        modified: None,
        is_hidden: false,
    }
}

#[test]
fn sort_dirs_always_first() {
    let mut entries = vec![
        make_entry("file_a.txt", false, 100),
        make_entry("dir_b", true, 0),
        make_entry("file_c.txt", false, 50),
        make_entry("dir_a", true, 0),
    ];

    sort_entries(&mut entries, SortColumn::Name, true);

    assert!(entries[0].is_dir);
    assert!(entries[1].is_dir);
    assert!(!entries[2].is_dir);
    assert!(!entries[3].is_dir);
}

#[test]
fn sort_by_name_ascending() {
    let mut entries = vec![
        make_entry("Zebra", false, 0),
        make_entry("apple", false, 0),
        make_entry("Mango", false, 0),
    ];

    sort_entries(&mut entries, SortColumn::Name, true);

    assert_eq!(entries[0].name, "apple");
    assert_eq!(entries[1].name, "Mango");
    assert_eq!(entries[2].name, "Zebra");
}

#[test]
fn sort_by_name_descending() {
    let mut entries = vec![
        make_entry("apple", false, 0),
        make_entry("Zebra", false, 0),
        make_entry("Mango", false, 0),
    ];

    sort_entries(&mut entries, SortColumn::Name, false);

    assert_eq!(entries[0].name, "Zebra");
    assert_eq!(entries[1].name, "Mango");
    assert_eq!(entries[2].name, "apple");
}

#[test]
fn sort_by_size() {
    let mut entries = vec![
        make_entry("big", false, 1000),
        make_entry("small", false, 10),
        make_entry("medium", false, 500),
    ];

    sort_entries(&mut entries, SortColumn::Size, true);

    assert_eq!(entries[0].name, "small");
    assert_eq!(entries[1].name, "medium");
    assert_eq!(entries[2].name, "big");
}

#[test]
fn sort_by_size_descending() {
    let mut entries = vec![
        make_entry("big", false, 1000),
        make_entry("small", false, 10),
        make_entry("medium", false, 500),
    ];

    sort_entries(&mut entries, SortColumn::Size, false);

    assert_eq!(entries[0].name, "big");
    assert_eq!(entries[1].name, "medium");
    assert_eq!(entries[2].name, "small");
}

// ── read_directory ───────────────────────────────────────────

#[test]
fn read_directory_returns_entries() {
    let dir = tempdir("read_dir");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bb").unwrap();
    fs::create_dir(dir.join("subdir")).unwrap();

    let entries = read_directory(&dir);
    assert_eq!(entries.len(), 3);

    // Dirs should be first (sorted)
    assert!(entries[0].is_dir);
    assert_eq!(entries[0].name, "subdir");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_directory_nonexistent_returns_empty() {
    let entries = read_directory(&PathBuf::from("/no_such_dir_abc123"));
    assert!(entries.is_empty());
}

#[test]
fn read_directory_empty_dir() {
    let dir = tempdir("read_empty");
    let entries = read_directory(&dir);
    assert!(entries.is_empty());

    fs::remove_dir_all(&dir).ok();
}

// ── copy_dir_recursive (via integration) ─────────────────────

#[tokio::test]
async fn copy_entries_copies_files() {
    let src_dir = tempdir("copy_src");
    let dst_dir = tempdir("copy_dst");
    fs::write(src_dir.join("file1.txt"), "content1").unwrap();
    fs::write(src_dir.join("file2.txt"), "content2").unwrap();

    let sources = vec![src_dir.join("file1.txt"), src_dir.join("file2.txt")];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::copy_entries(cpt::fs::ops::build_pairs(&sources, &dst_dir), tx).await;

    // Drain messages
    let mut messages = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        messages.push(msg);
    }

    assert!(dst_dir.join("file1.txt").exists());
    assert!(dst_dir.join("file2.txt").exists());
    assert_eq!(
        fs::read_to_string(dst_dir.join("file1.txt")).unwrap(),
        "content1"
    );

    fs::remove_dir_all(&src_dir).ok();
    fs::remove_dir_all(&dst_dir).ok();
}

#[tokio::test]
async fn copy_entries_copies_directory_recursively() {
    let src_dir = tempdir("copy_dir_src");
    let dst_dir = tempdir("copy_dir_dst");
    let sub = src_dir.join("mydir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("inner.txt"), "inner").unwrap();

    let sources = vec![sub.clone()];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::copy_entries(cpt::fs::ops::build_pairs(&sources, &dst_dir), tx).await;

    while let Ok(_) = rx.try_recv() {}

    assert!(dst_dir.join("mydir").is_dir());
    assert!(dst_dir.join("mydir").join("inner.txt").exists());

    fs::remove_dir_all(&src_dir).ok();
    fs::remove_dir_all(&dst_dir).ok();
}

#[tokio::test]
async fn move_entries_moves_files() {
    let src_dir = tempdir("move_src");
    let dst_dir = tempdir("move_dst");
    fs::write(src_dir.join("moveme.txt"), "data").unwrap();

    let sources = vec![src_dir.join("moveme.txt")];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::move_entries(cpt::fs::ops::build_pairs(&sources, &dst_dir), tx).await;

    while let Ok(_) = rx.try_recv() {}

    assert!(!src_dir.join("moveme.txt").exists());
    assert!(dst_dir.join("moveme.txt").exists());

    fs::remove_dir_all(&src_dir).ok();
    fs::remove_dir_all(&dst_dir).ok();
}

#[tokio::test]
async fn delete_entries_deletes_files() {
    let dir = tempdir("delete_test");
    let file = dir.join("deleteme.txt");
    fs::write(&file, "bye").unwrap();

    let sources = vec![file.clone()];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::delete_entries(sources, tx).await;

    while let Ok(_) = rx.try_recv() {}

    assert!(!file.exists());

    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn delete_entries_deletes_directories() {
    let dir = tempdir("delete_dir_test");
    let sub = dir.join("subdir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("inner.txt"), "data").unwrap();

    let sources = vec![sub.clone()];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::delete_entries(sources, tx).await;

    while let Ok(_) = rx.try_recv() {}

    assert!(!sub.exists());

    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn copy_nonexistent_source_reports_error() {
    let dst_dir = tempdir("copy_err_dst");
    let sources = vec![PathBuf::from("/no_such_file_xyz_123")];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::copy_entries(cpt::fs::ops::build_pairs(&sources, &dst_dir), tx).await;

    let mut found_error = false;
    while let Ok(msg) = rx.try_recv() {
        if let cpt::action::Action::OperationError(_) = msg {
            found_error = true;
        }
    }
    assert!(found_error);

    fs::remove_dir_all(&dst_dir).ok();
}

// ── Explorer logic (non-draw) ────────────────────────────────

#[test]
fn explorer_navigation_basic() {
    let dir = tempdir("explorer_nav");
    fs::write(dir.join("a.txt"), "a").unwrap();
    fs::write(dir.join("b.txt"), "b").unwrap();
    fs::write(dir.join("c.txt"), "c").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());

    // Starts at cursor 0 (the ".." row)
    assert_eq!(explorer.cursor, 0);
    assert!(explorer.current_entry().is_none()); // ".." has no FileEntry

    // Move down to first file
    explorer.handle_action(&cpt::action::Action::MoveDown);
    assert_eq!(explorer.cursor, 1);
    assert!(explorer.current_entry().is_some());

    // Move down again
    explorer.handle_action(&cpt::action::Action::MoveDown);
    assert_eq!(explorer.cursor, 2);

    // Move up
    explorer.handle_action(&cpt::action::Action::MoveUp);
    assert_eq!(explorer.cursor, 1);

    // Move to top
    explorer.handle_action(&cpt::action::Action::MoveToTop);
    assert_eq!(explorer.cursor, 0);

    // Move to bottom
    explorer.handle_action(&cpt::action::Action::MoveToBottom);
    assert_eq!(explorer.cursor, 3); // ".." + 3 files = 4 items, last index = 3

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_move_up_at_top_stays() {
    let dir = tempdir("explorer_top");
    fs::write(dir.join("x.txt"), "x").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    assert_eq!(explorer.cursor, 0);

    explorer.handle_action(&cpt::action::Action::MoveUp);
    assert_eq!(explorer.cursor, 0); // Should not go negative

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_move_down_at_bottom_stays() {
    let dir = tempdir("explorer_bottom");
    fs::write(dir.join("x.txt"), "x").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    // display_len = 2 (".." + 1 file)
    explorer.handle_action(&cpt::action::Action::MoveToBottom);
    assert_eq!(explorer.cursor, 1);

    explorer.handle_action(&cpt::action::Action::MoveDown);
    assert_eq!(explorer.cursor, 1); // Should stay at bottom

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_selection() {
    let dir = tempdir("explorer_sel");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join("b.txt"), "bb").unwrap();
    fs::write(dir.join("c.txt"), "c").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());

    // Move to first file and toggle select
    explorer.handle_action(&cpt::action::Action::MoveDown);
    assert_eq!(explorer.cursor, 1);
    explorer.handle_action(&cpt::action::Action::ToggleSelect);
    assert_eq!(explorer.selected.len(), 1);
    // ToggleSelect also moves down
    assert_eq!(explorer.cursor, 2);

    // Ctrl+A with partial selection -> deselects all
    explorer.handle_action(&cpt::action::Action::SelectAll);
    assert_eq!(explorer.selected.len(), 0);

    // Ctrl+A with nothing selected -> selects all
    explorer.handle_action(&cpt::action::Action::SelectAll);
    assert_eq!(explorer.selected.len(), 3);

    // Ctrl+A again deselects all (toggle behavior)
    explorer.handle_action(&cpt::action::Action::SelectAll);
    assert_eq!(explorer.selected.len(), 0);

    // Select all, then deselect via DeselectAll
    explorer.handle_action(&cpt::action::Action::SelectAll);
    assert_eq!(explorer.selected.len(), 3);
    explorer.handle_action(&cpt::action::Action::DeselectAll);
    assert_eq!(explorer.selected.len(), 0);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_toggle_select_on_dotdot_skips() {
    let dir = tempdir("explorer_sel_dotdot");
    fs::write(dir.join("a.txt"), "a").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    assert_eq!(explorer.cursor, 0); // On ".."

    explorer.handle_action(&cpt::action::Action::ToggleSelect);
    assert_eq!(explorer.selected.len(), 0); // Nothing selected since we're on ".."

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_selected_total_size() {
    let dir = tempdir("explorer_size");
    fs::write(dir.join("a.txt"), "aaaa").unwrap(); // 4 bytes
    fs::write(dir.join("b.txt"), "bb").unwrap(); // 2 bytes

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::SelectAll);

    let total = explorer.selected_total_size();
    assert_eq!(total, 6);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_filter() {
    let dir = tempdir("explorer_filter");
    fs::write(dir.join("apple.txt"), "").unwrap();
    fs::write(dir.join("banana.txt"), "").unwrap();
    fs::write(dir.join("avocado.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    assert_eq!(explorer.filtered.len(), 3);

    // Start filter
    explorer.handle_action(&cpt::action::Action::StartFilter);
    assert!(explorer.filter_text.is_some());

    // Type "a" - matches apple and avocado and banana (all contain 'a')
    explorer.handle_action(&cpt::action::Action::FilterInput('b'));
    assert_eq!(explorer.filtered.len(), 1); // only banana

    // Backspace
    explorer.handle_action(&cpt::action::Action::FilterBackspace);
    assert_eq!(explorer.filtered.len(), 3); // back to all

    // Cancel filter
    explorer.handle_action(&cpt::action::Action::FilterCancel);
    assert!(explorer.filter_text.is_none());
    assert_eq!(explorer.filtered.len(), 3);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_filter_case_insensitive() {
    let dir = tempdir("explorer_filter_ci");
    fs::write(dir.join("Apple.txt"), "").unwrap();
    fs::write(dir.join("banana.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::StartFilter);
    explorer.handle_action(&cpt::action::Action::FilterInput('a'));
    explorer.handle_action(&cpt::action::Action::FilterInput('p'));

    // "ap" should match "Apple.txt" case-insensitively
    assert_eq!(explorer.filtered.len(), 1);
    let matched = &explorer.entries[explorer.filtered[0]];
    assert_eq!(matched.name, "Apple.txt");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_shows_hidden_files() {
    let dir = tempdir("explorer_hidden");
    fs::write(dir.join("visible.txt"), "").unwrap();
    fs::write(dir.join(".hidden"), "").unwrap();

    let explorer = cpt::components::explorer::Explorer::new(dir.clone());

    // Hidden files should be visible by default
    let names: Vec<&str> = explorer
        .filtered
        .iter()
        .map(|&i| explorer.entries[i].name.as_str())
        .collect();
    assert!(names.contains(&".hidden"));
    assert!(names.contains(&"visible.txt"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_enter_dir_and_parent() {
    let dir = tempdir("explorer_enterdir");
    let sub = dir.join("child");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("inner.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());

    // Find the "child" entry - dirs come first, so cursor 1 after ".."
    explorer.handle_action(&cpt::action::Action::MoveDown);
    let entry = explorer.current_entry().unwrap();
    assert_eq!(entry.name, "child");
    assert!(entry.is_dir);

    // Enter the directory
    explorer.handle_action(&cpt::action::Action::EnterDir);
    assert_eq!(explorer.current_dir, sub);

    // Go back to parent
    explorer.handle_action(&cpt::action::Action::ParentDir);
    assert_eq!(explorer.current_dir, dir);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_enter_on_dotdot_goes_parent() {
    let dir = tempdir("explorer_dotdot_enter");
    let sub = dir.join("child");
    fs::create_dir(&sub).unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(sub.clone());
    assert_eq!(explorer.cursor, 0); // On ".."

    let action = explorer.handle_action(&cpt::action::Action::EnterDir);
    // Should return ParentDir action
    assert!(matches!(action, Some(cpt::action::Action::ParentDir)));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_page_up_down() {
    let dir = tempdir("explorer_page");
    for i in 0..50 {
        fs::write(dir.join(format!("file_{:03}.txt", i)), "").unwrap();
    }

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    assert_eq!(explorer.cursor, 0);

    explorer.handle_action(&cpt::action::Action::PageDown);
    assert_eq!(explorer.cursor, 20);

    explorer.handle_action(&cpt::action::Action::PageDown);
    assert_eq!(explorer.cursor, 40);

    explorer.handle_action(&cpt::action::Action::PageUp);
    assert_eq!(explorer.cursor, 20);

    explorer.handle_action(&cpt::action::Action::PageUp);
    assert_eq!(explorer.cursor, 0);

    // Page up from 0 stays at 0 (saturating_sub)
    explorer.handle_action(&cpt::action::Action::PageUp);
    assert_eq!(explorer.cursor, 0);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_refresh_clears_selection() {
    let dir = tempdir("explorer_refresh");
    fs::write(dir.join("a.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::SelectAll);
    assert!(!explorer.selected.is_empty());

    explorer.handle_action(&cpt::action::Action::Refresh);
    assert!(explorer.selected.is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_selected_paths() {
    let dir = tempdir("explorer_paths");
    fs::write(dir.join("a.txt"), "").unwrap();
    fs::write(dir.join("b.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::SelectAll);

    let paths = explorer.selected_paths();
    assert_eq!(paths.len(), 2);

    fs::remove_dir_all(&dir).ok();
}

// ── DualPane ─────────────────────────────────────────────────

#[test]
fn dual_pane_switch() {
    let dir = tempdir("dual_switch");

    let mut dual = cpt::components::dual_pane::DualPane::new(
        dir.clone(),
        dir.clone(),
        cpt::config::PaneSide::Left,
    );

    assert_eq!(dual.active, cpt::config::PaneSide::Left);

    dual.handle_action(&cpt::action::Action::SwitchPane);
    assert_eq!(dual.active, cpt::config::PaneSide::Right);

    dual.handle_action(&cpt::action::Action::SwitchPane);
    assert_eq!(dual.active, cpt::config::PaneSide::Left);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn dual_pane_focus_left_right() {
    let dir = tempdir("dual_focus");

    let mut dual = cpt::components::dual_pane::DualPane::new(
        dir.clone(),
        dir.clone(),
        cpt::config::PaneSide::Left,
    );

    dual.handle_action(&cpt::action::Action::FocusRight);
    assert_eq!(dual.active, cpt::config::PaneSide::Right);

    dual.handle_action(&cpt::action::Action::FocusLeft);
    assert_eq!(dual.active, cpt::config::PaneSide::Left);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn dual_pane_inactive_dir() {
    let left_dir = tempdir("dual_left");
    let right_dir = tempdir("dual_right");

    let dual = cpt::components::dual_pane::DualPane::new(
        left_dir.clone(),
        right_dir.clone(),
        cpt::config::PaneSide::Left,
    );

    assert_eq!(dual.inactive_dir(), right_dir);

    fs::remove_dir_all(&left_dir).ok();
    fs::remove_dir_all(&right_dir).ok();
}

// ── SessionConfig ────────────────────────────────────────────

#[test]
fn session_config_default() {
    let config = cpt::config::SessionConfig::default();
    assert_eq!(config.active_pane, cpt::config::PaneSide::Left);
    // Should point to some valid directory (home or ".")
    assert!(!config.left_dir.as_os_str().is_empty());
}

// ── strip_unc_prefix ─────────────────────────────────────────

#[cfg(windows)]
#[test]
fn strip_unc_prefix_removes_windows_prefix() {
    let path = PathBuf::from(r"\\?\C:\Users\test");
    let result = cpt::util::strip_unc_prefix(path);
    assert_eq!(result, PathBuf::from(r"C:\Users\test"));
}

#[test]
fn strip_unc_prefix_leaves_normal_path() {
    let path = PathBuf::from(r"C:\Users\test");
    let result = cpt::util::strip_unc_prefix(path.clone());
    assert_eq!(result, path);
}

#[test]
fn clean_canonicalize_existing_dir() {
    let dir = tempdir("canon_test");
    let result = cpt::util::clean_canonicalize(&dir).unwrap();
    // Should not have \\?\ prefix
    assert!(!result.to_string_lossy().starts_with(r"\\?\"));
    assert!(result.is_dir());
    fs::remove_dir_all(&dir).ok();
}

// ── sort_entries by date ─────────────────────────────────────

#[test]
fn sort_by_date() {
    use chrono::{Local, TimeZone};

    let mut entries = vec![
        FileEntry {
            name: "old".to_string(),
            path: PathBuf::from("old"),
            is_dir: false,
            size: 0,
            modified: Some(Local.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap()),
            is_hidden: false,
        },
        FileEntry {
            name: "new".to_string(),
            path: PathBuf::from("new"),
            is_dir: false,
            size: 0,
            modified: Some(Local.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap()),
            is_hidden: false,
        },
        FileEntry {
            name: "mid".to_string(),
            path: PathBuf::from("mid"),
            is_dir: false,
            size: 0,
            modified: Some(Local.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap()),
            is_hidden: false,
        },
    ];

    sort_entries(&mut entries, SortColumn::Date, true);
    assert_eq!(entries[0].name, "old");
    assert_eq!(entries[1].name, "mid");
    assert_eq!(entries[2].name, "new");

    sort_entries(&mut entries, SortColumn::Date, false);
    assert_eq!(entries[0].name, "new");
    assert_eq!(entries[1].name, "mid");
    assert_eq!(entries[2].name, "old");
}

// ── Explorer: enter non-archive file returns OpenEditor ──────

#[test]
fn explorer_enter_on_file_returns_open_editor() {
    let dir = tempdir("explorer_open_file");
    fs::write(dir.join("readme.txt"), "hi").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::MoveDown); // onto the file

    let result = explorer.handle_action(&cpt::action::Action::EnterDir);
    assert!(matches!(
        result,
        Some(cpt::action::Action::OpenEditor { .. })
    ));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_enter_on_archive_returns_unpack() {
    let dir = tempdir("explorer_archive");
    fs::write(dir.join("backup.zip"), "fake zip").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::MoveDown);

    let result = explorer.handle_action(&cpt::action::Action::EnterDir);
    assert!(
        matches!(result, Some(cpt::action::Action::UnpackArchive)),
        "expected UnpackArchive, got {:?}",
        result
    );

    fs::remove_dir_all(&dir).ok();
}

// ── Explorer: parent dir places cursor on previous dir ───────

#[test]
fn explorer_parent_dir_cursor_placement() {
    let dir = tempdir("explorer_parent_cursor");
    let child = dir.join("target_child");
    fs::create_dir(&child).unwrap();
    // Add another dir so there are multiple entries
    fs::create_dir(dir.join("aaa_first")).unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(child.clone());
    explorer.handle_action(&cpt::action::Action::ParentDir);

    assert_eq!(explorer.current_dir, dir);
    // Cursor should be on "target_child", not at 0
    let entry = explorer.current_entry();
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().name, "target_child");

    fs::remove_dir_all(&dir).ok();
}

// ── Explorer: filter confirm keeps filter text ───────────────

#[test]
fn explorer_filter_confirm_keeps_filter() {
    let dir = tempdir("explorer_filter_confirm");
    fs::write(dir.join("apple.txt"), "").unwrap();
    fs::write(dir.join("banana.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::StartFilter);
    explorer.handle_action(&cpt::action::Action::FilterInput('a'));
    explorer.handle_action(&cpt::action::Action::FilterInput('p'));

    // Confirm should keep the filter text active
    explorer.handle_action(&cpt::action::Action::FilterConfirm);
    assert!(explorer.filter_text.is_some());
    assert_eq!(explorer.filter_text.as_deref(), Some("ap"));
    // Still filtered
    assert_eq!(explorer.filtered.len(), 1);

    fs::remove_dir_all(&dir).ok();
}

// ── Explorer: empty directory ────────────────────────────────

#[test]
fn explorer_empty_dir_has_dotdot_only() {
    let dir = tempdir("explorer_empty_dir");

    let explorer = cpt::components::explorer::Explorer::new(dir.clone());
    assert_eq!(explorer.entries.len(), 0);
    assert_eq!(explorer.filtered.len(), 0);
    assert_eq!(explorer.cursor, 0);
    // current_entry at cursor 0 is "..", returns None
    assert!(explorer.current_entry().is_none());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_empty_dir_move_down_stays() {
    let dir = tempdir("explorer_empty_move");

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::MoveDown);
    assert_eq!(explorer.cursor, 0); // Only ".." row, can't go further

    fs::remove_dir_all(&dir).ok();
}

// ── Move: directory ──────────────────────────────────────────

#[tokio::test]
async fn move_entries_moves_directory() {
    let src_dir = tempdir("move_dir_src");
    let dst_dir = tempdir("move_dir_dst");
    let sub = src_dir.join("mydir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("file.txt"), "data").unwrap();

    let sources = vec![sub.clone()];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::move_entries(cpt::fs::ops::build_pairs(&sources, &dst_dir), tx).await;

    while let Ok(_) = rx.try_recv() {}

    assert!(!sub.exists());
    assert!(dst_dir.join("mydir").is_dir());
    assert_eq!(
        fs::read_to_string(dst_dir.join("mydir").join("file.txt")).unwrap(),
        "data"
    );

    fs::remove_dir_all(&src_dir).ok();
    fs::remove_dir_all(&dst_dir).ok();
}

// ── Move/Delete: error on nonexistent ────────────────────────

#[tokio::test]
async fn move_nonexistent_source_reports_error() {
    let dst_dir = tempdir("move_err_dst");
    let sources = vec![PathBuf::from("/no_such_file_move_xyz")];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::move_entries(cpt::fs::ops::build_pairs(&sources, &dst_dir), tx).await;

    let mut found_error = false;
    while let Ok(msg) = rx.try_recv() {
        if let cpt::action::Action::OperationError(_) = msg {
            found_error = true;
        }
    }
    assert!(found_error);

    fs::remove_dir_all(&dst_dir).ok();
}

#[tokio::test]
async fn delete_nonexistent_source_reports_error() {
    let sources = vec![PathBuf::from("/no_such_file_del_xyz")];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::delete_entries(sources, tx).await;

    let mut found_error = false;
    while let Ok(msg) = rx.try_recv() {
        if let cpt::action::Action::OperationError(_) = msg {
            found_error = true;
        }
    }
    assert!(found_error);
}

// ── Ops: progress messages ───────────────────────────────────

#[tokio::test]
async fn copy_entries_sends_progress_and_complete() {
    let src_dir = tempdir("copy_progress");
    let dst_dir = tempdir("copy_progress_dst");
    fs::write(src_dir.join("a.txt"), "a").unwrap();
    fs::write(src_dir.join("b.txt"), "b").unwrap();

    let sources = vec![src_dir.join("a.txt"), src_dir.join("b.txt")];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::copy_entries(cpt::fs::ops::build_pairs(&sources, &dst_dir), tx).await;

    let mut progress_count = 0;
    let mut complete = false;
    while let Ok(msg) = rx.try_recv() {
        match msg {
            cpt::action::Action::OperationProgress { .. } => progress_count += 1,
            cpt::action::Action::OperationComplete(_) => complete = true,
            _ => {}
        }
    }
    assert_eq!(progress_count, 2);
    assert!(complete);

    fs::remove_dir_all(&src_dir).ok();
    fs::remove_dir_all(&dst_dir).ok();
}

// ── DualPane: refresh both panes ─────────────────────────────

#[test]
fn dual_pane_refresh_both() {
    let left = tempdir("dual_refresh_l");
    let right = tempdir("dual_refresh_r");
    fs::write(left.join("a.txt"), "").unwrap();

    let mut dual = cpt::components::dual_pane::DualPane::new(
        left.clone(),
        right.clone(),
        cpt::config::PaneSide::Left,
    );
    assert_eq!(dual.left.as_explorer().unwrap().filtered.len(), 1);

    // Add a file, then refresh
    fs::write(left.join("b.txt"), "").unwrap();
    dual.handle_action(&cpt::action::Action::Refresh);
    assert_eq!(dual.left.as_explorer().unwrap().filtered.len(), 2);

    fs::remove_dir_all(&left).ok();
    fs::remove_dir_all(&right).ok();
}

// ── DualPane: action forwarded to active ─────────────────────

#[test]
fn dual_pane_forwards_to_active() {
    let left = tempdir("dual_fwd_l");
    let right = tempdir("dual_fwd_r");
    fs::write(left.join("a.txt"), "").unwrap();
    fs::write(right.join("b.txt"), "").unwrap();

    let mut dual = cpt::components::dual_pane::DualPane::new(
        left.clone(),
        right.clone(),
        cpt::config::PaneSide::Left,
    );

    // MoveDown on left pane
    dual.handle_action(&cpt::action::Action::MoveDown);
    assert_eq!(dual.left.as_explorer().unwrap().cursor, 1);
    assert_eq!(dual.right.as_explorer().unwrap().cursor, 0); // Unchanged

    // Switch and move on right pane
    dual.handle_action(&cpt::action::Action::SwitchPane);
    dual.handle_action(&cpt::action::Action::MoveDown);
    assert_eq!(dual.right.as_explorer().unwrap().cursor, 1);
    assert_eq!(dual.left.as_explorer().unwrap().cursor, 1); // Still unchanged

    fs::remove_dir_all(&left).ok();
    fs::remove_dir_all(&right).ok();
}

// ── Dialog ───────────────────────────────────────────────────

#[test]
fn dialog_scroll() {
    let mut dialog = cpt::components::dialog::Dialog::info("line1\nline2\nline3");
    assert_eq!(dialog.scroll, 0);

    dialog.scroll_down();
    assert_eq!(dialog.scroll, 1);

    dialog.scroll_down();
    assert_eq!(dialog.scroll, 2);

    dialog.scroll_up();
    assert_eq!(dialog.scroll, 1);

    // Can't go below 0
    dialog.scroll_up();
    dialog.scroll_up();
    assert_eq!(dialog.scroll, 0);
}

#[test]
fn dialog_constructors() {
    let d = cpt::components::dialog::Dialog::confirm("Title", "Are you sure?");
    assert!(matches!(
        d.kind,
        cpt::components::dialog::DialogKind::Confirm { .. }
    ));
    assert_eq!(d.scroll, 0);

    let d = cpt::components::dialog::Dialog::error("something broke");
    assert!(matches!(
        d.kind,
        cpt::components::dialog::DialogKind::Error(_)
    ));

    let d = cpt::components::dialog::Dialog::info("hello");
    assert!(matches!(
        d.kind,
        cpt::components::dialog::DialogKind::Info(_)
    ));
}

// ── read_directory includes hidden ───────────────────────────

#[test]
fn read_directory_includes_hidden_files() {
    let dir = tempdir("read_hidden");
    fs::write(dir.join(".secret"), "").unwrap();
    fs::write(dir.join("visible.txt"), "").unwrap();

    let entries = read_directory(&dir);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&".secret"));
    assert!(names.contains(&"visible.txt"));

    fs::remove_dir_all(&dir).ok();
}

// ── Conflict detection and unique naming ─────────────────────

#[test]
fn build_pairs_creates_correct_targets() {
    let dest = PathBuf::from("/dest");
    let sources = vec![PathBuf::from("/src/a.txt"), PathBuf::from("/src/b.txt")];
    let pairs = cpt::fs::ops::build_pairs(&sources, &dest);
    assert_eq!(pairs.len(), 2);
    assert_eq!(
        pairs[0],
        (PathBuf::from("/src/a.txt"), PathBuf::from("/dest/a.txt"))
    );
    assert_eq!(
        pairs[1],
        (PathBuf::from("/src/b.txt"), PathBuf::from("/dest/b.txt"))
    );
}

#[test]
fn find_conflicts_detects_existing_targets() {
    let dir = tempdir("conflicts_detect");
    fs::write(dir.join("existing.txt"), "data").unwrap();

    let pairs = vec![
        (PathBuf::from("/src/existing.txt"), dir.join("existing.txt")),
        (PathBuf::from("/src/new.txt"), dir.join("new.txt")),
    ];
    let conflicts = cpt::fs::ops::find_conflicts(&pairs);
    assert_eq!(conflicts, vec![0]);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_conflicts_empty_when_no_conflicts() {
    let dir = tempdir("no_conflicts");
    let pairs = vec![
        (PathBuf::from("/src/a.txt"), dir.join("a.txt")),
        (PathBuf::from("/src/b.txt"), dir.join("b.txt")),
    ];
    let conflicts = cpt::fs::ops::find_conflicts(&pairs);
    assert!(conflicts.is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unique_target_appends_number() {
    let dir = tempdir("unique_target");
    fs::write(dir.join("file.txt"), "").unwrap();

    let result = cpt::fs::ops::unique_target(&dir.join("file.txt"));
    assert_eq!(result, dir.join("file (1).txt"));

    // Create that too, should get (2)
    fs::write(dir.join("file (1).txt"), "").unwrap();
    let result = cpt::fs::ops::unique_target(&dir.join("file.txt"));
    assert_eq!(result, dir.join("file (2).txt"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unique_target_no_extension() {
    let dir = tempdir("unique_noext");
    fs::write(dir.join("readme"), "").unwrap();

    let result = cpt::fs::ops::unique_target(&dir.join("readme"));
    assert_eq!(result, dir.join("readme (1)"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn rename_conflicts_renames_only_existing() {
    let dir = tempdir("rename_conflicts");
    fs::write(dir.join("exists.txt"), "").unwrap();

    let mut pairs = vec![
        (PathBuf::from("/src/exists.txt"), dir.join("exists.txt")),
        (PathBuf::from("/src/new.txt"), dir.join("new.txt")),
    ];
    cpt::fs::ops::rename_conflicts(&mut pairs);

    assert_eq!(pairs[0].1, dir.join("exists (1).txt"));
    assert_eq!(pairs[1].1, dir.join("new.txt")); // Unchanged

    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn copy_with_rename_avoids_overwrite() {
    let src_dir = tempdir("copy_rename_src");
    let dst_dir = tempdir("copy_rename_dst");
    fs::write(src_dir.join("file.txt"), "new content").unwrap();
    fs::write(dst_dir.join("file.txt"), "original").unwrap();

    let sources = vec![src_dir.join("file.txt")];
    let mut pairs = cpt::fs::ops::build_pairs(&sources, &dst_dir);
    cpt::fs::ops::rename_conflicts(&mut pairs);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    cpt::fs::ops::copy_entries(pairs, tx).await;
    while let Ok(_) = rx.try_recv() {}

    // Original preserved
    assert_eq!(
        fs::read_to_string(dst_dir.join("file.txt")).unwrap(),
        "original"
    );
    // New file renamed
    assert_eq!(
        fs::read_to_string(dst_dir.join("file (1).txt")).unwrap(),
        "new content"
    );

    fs::remove_dir_all(&src_dir).ok();
    fs::remove_dir_all(&dst_dir).ok();
}

#[tokio::test]
async fn copy_overwrite_replaces_existing() {
    let src_dir = tempdir("copy_overwrite_src");
    let dst_dir = tempdir("copy_overwrite_dst");
    fs::write(src_dir.join("file.txt"), "updated").unwrap();
    fs::write(dst_dir.join("file.txt"), "old").unwrap();

    let sources = vec![src_dir.join("file.txt")];
    let pairs = cpt::fs::ops::build_pairs(&sources, &dst_dir);
    // No rename - overwrite
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    cpt::fs::ops::copy_entries(pairs, tx).await;
    while let Ok(_) = rx.try_recv() {}

    assert_eq!(
        fs::read_to_string(dst_dir.join("file.txt")).unwrap(),
        "updated"
    );

    fs::remove_dir_all(&src_dir).ok();
    fs::remove_dir_all(&dst_dir).ok();
}

// ── Logger ───────────────────────────────────────────────────

#[test]
fn logger_dump_returns_zero_when_empty() {
    // Logger may have entries from other tests, but init is idempotent
    // Just verify dump doesn't panic
    let count = cpt::logger::dump();
    // count can be 0 or more depending on test ordering
    assert!(count < 10000); // sanity check
}

// ── Explorer: edge cases ─────────────────────────────────────

#[test]
fn explorer_select_all_on_empty_dir() {
    let dir = tempdir("explorer_sel_empty");
    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());

    // SelectAll on empty should not panic, selected stays empty
    explorer.handle_action(&cpt::action::Action::SelectAll);
    assert!(explorer.selected.is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_page_down_clamped_to_bottom() {
    let dir = tempdir("explorer_pgdn_clamp");
    fs::write(dir.join("a.txt"), "").unwrap();
    fs::write(dir.join("b.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    // display_len = 3 (".." + 2 files), page down by 20 should clamp
    explorer.handle_action(&cpt::action::Action::PageDown);
    assert_eq!(explorer.cursor, 2); // last item

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_current_entry_returns_correct_entry() {
    let dir = tempdir("explorer_current");
    fs::write(dir.join("alpha.txt"), "").unwrap();
    fs::write(dir.join("beta.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::MoveDown);
    let entry = explorer.current_entry().unwrap();
    assert_eq!(entry.name, "alpha.txt");

    explorer.handle_action(&cpt::action::Action::MoveDown);
    let entry = explorer.current_entry().unwrap();
    assert_eq!(entry.name, "beta.txt");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_selected_entries_returns_refs() {
    let dir = tempdir("explorer_sel_entries");
    fs::write(dir.join("a.txt"), "").unwrap();
    fs::write(dir.join("b.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::SelectAll);
    let entries = explorer.selected_entries();
    assert_eq!(entries.len(), 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_filter_empty_string_shows_all() {
    let dir = tempdir("explorer_filter_empty");
    fs::write(dir.join("a.txt"), "").unwrap();
    fs::write(dir.join("b.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::StartFilter);
    // Empty filter string shows all entries
    assert_eq!(explorer.filtered.len(), 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_filter_no_match_shows_empty() {
    let dir = tempdir("explorer_filter_nomatch");
    fs::write(dir.join("a.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::StartFilter);
    explorer.handle_action(&cpt::action::Action::FilterInput('z'));
    explorer.handle_action(&cpt::action::Action::FilterInput('z'));
    explorer.handle_action(&cpt::action::Action::FilterInput('z'));
    assert_eq!(explorer.filtered.len(), 0);
    // Cursor clamped to 0
    assert_eq!(explorer.cursor, 0);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_enter_dir_clears_filter() {
    let dir = tempdir("explorer_enter_clears_filter");
    let sub = dir.join("child");
    fs::create_dir(&sub).unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&cpt::action::Action::StartFilter);
    explorer.handle_action(&cpt::action::Action::FilterInput('c'));
    assert!(explorer.filter_text.is_some());

    explorer.handle_action(&cpt::action::Action::MoveDown);
    explorer.handle_action(&cpt::action::Action::EnterDir);
    assert!(explorer.filter_text.is_none());

    fs::remove_dir_all(&dir).ok();
}

// ── DualPane: enter on text file opens editor in active pane ──

#[test]
fn dual_pane_enter_on_text_file_sets_active_editor() {
    let dir = tempdir("dual_editor_open");
    fs::write(dir.join("readme.txt"), "hello world").unwrap();

    let mut dual = cpt::components::dual_pane::DualPane::new(
        dir.clone(),
        dir.clone(),
        cpt::config::PaneSide::Left,
    );

    // Move cursor to the file (index 0 is the file in this single-file dir)
    dual.handle_action(&cpt::action::Action::MoveDown);
    let follow_up = dual.handle_action(&cpt::action::Action::EnterDir);

    let path = match follow_up {
        Some(cpt::action::Action::OpenEditor { path }) => path,
        other => panic!("Expected OpenEditor, got {:?}", other),
    };

    dual.open_editor_in_active(path).expect("open_editor_in_active failed");

    assert!(
        dual.active_editor().is_some(),
        "active_editor() must return Some after open_editor_in_active"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── DualPane: navigation dispatched to active only ───────────

#[test]
fn dual_pane_actions_only_affect_active() {
    let left = tempdir("dual_active_l");
    let right = tempdir("dual_active_r");
    fs::write(left.join("a.txt"), "aaa").unwrap();
    fs::write(right.join("b.txt"), "bbb").unwrap();

    let mut dual = cpt::components::dual_pane::DualPane::new(
        left.clone(),
        right.clone(),
        cpt::config::PaneSide::Left,
    );

    // Select all on left
    dual.handle_action(&cpt::action::Action::SelectAll);
    assert_eq!(dual.left.as_explorer().unwrap().selected.len(), 1);
    assert_eq!(dual.right.as_explorer().unwrap().selected.len(), 0);

    // Switch and select all on right
    dual.handle_action(&cpt::action::Action::SwitchPane);
    dual.handle_action(&cpt::action::Action::SelectAll);
    assert_eq!(dual.left.as_explorer().unwrap().selected.len(), 1); // unchanged
    assert_eq!(dual.right.as_explorer().unwrap().selected.len(), 1);

    fs::remove_dir_all(&left).ok();
    fs::remove_dir_all(&right).ok();
}

// ── sort_entries: stability with mixed dirs and files ─────────

#[test]
fn sort_dirs_first_preserves_name_order() {
    let mut entries = vec![
        make_entry("z_dir", true, 0),
        make_entry("a_dir", true, 0),
        make_entry("m_file", false, 0),
        make_entry("a_file", false, 0),
    ];

    sort_entries(&mut entries, SortColumn::Name, true);
    assert_eq!(entries[0].name, "a_dir");
    assert_eq!(entries[1].name, "z_dir");
    assert_eq!(entries[2].name, "a_file");
    assert_eq!(entries[3].name, "m_file");
}

// ── FileEntry: dir has zero size ─────────────────────────────

#[test]
fn file_entry_dir_has_zero_size() {
    let dir = tempdir("entry_dir_size");
    let sub = dir.join("subdir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("file.txt"), "data inside").unwrap();

    let entry = FileEntry::from_path(&sub).unwrap();
    assert!(entry.is_dir);
    assert_eq!(entry.size, 0); // Dir size is always 0

    fs::remove_dir_all(&dir).ok();
}

// ── Unique target: directory naming ──────────────────────────

#[test]
fn unique_target_for_directory() {
    let dir = tempdir("unique_dir");
    fs::create_dir(dir.join("mydir")).unwrap();

    let result = cpt::fs::ops::unique_target(&dir.join("mydir"));
    assert_eq!(result, dir.join("mydir (1)"));

    fs::remove_dir_all(&dir).ok();
}

// ── Dialog: conflict constructor ─────────────────────────────

#[test]
fn dialog_conflict_constructor() {
    let d = cpt::components::dialog::Dialog::conflict("Copy", "Files exist");
    assert!(matches!(
        d.kind,
        cpt::components::dialog::DialogKind::Conflict { .. }
    ));
    assert_eq!(d.scroll, 0);
}

// ── Ops: partial failure stops early ─────────────────────────

#[tokio::test]
async fn copy_stops_on_first_error() {
    let src_dir = tempdir("copy_partial_src");
    let dst_dir = tempdir("copy_partial_dst");
    fs::write(src_dir.join("good.txt"), "ok").unwrap();
    // Second source doesn't exist
    let sources = vec![src_dir.join("good.txt"), src_dir.join("nonexistent.txt")];
    let pairs = cpt::fs::ops::build_pairs(&sources, &dst_dir);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::copy_entries(pairs, tx).await;

    let mut progress_count = 0;
    let mut error_count = 0;
    let mut complete = false;
    while let Ok(msg) = rx.try_recv() {
        match msg {
            cpt::action::Action::OperationProgress { .. } => progress_count += 1,
            cpt::action::Action::OperationError(_) => error_count += 1,
            cpt::action::Action::OperationComplete(_) => complete = true,
            _ => {}
        }
    }
    assert_eq!(progress_count, 1); // First succeeded
    assert_eq!(error_count, 1); // Second failed
    assert!(!complete); // Did not complete

    fs::remove_dir_all(&src_dir).ok();
    fs::remove_dir_all(&dst_dir).ok();
}

// ── Config: PaneSide equality ────────────────────────────────

#[test]
fn pane_side_equality() {
    assert_eq!(cpt::config::PaneSide::Left, cpt::config::PaneSide::Left);
    assert_ne!(cpt::config::PaneSide::Left, cpt::config::PaneSide::Right);
}

// ── Theme ───────────────────────────────────────────────────

#[test]
fn theme_default_and_muted_differ() {
    let d = cpt::theme::Theme::default();
    let m = cpt::theme::Theme::muted();
    // At least border_focused should differ
    assert_ne!(
        format!("{:?}", d.border_focused),
        format!("{:?}", m.border_focused)
    );
}

#[test]
fn theme_by_name_returns_builtin() {
    let d = cpt::theme::Theme::by_name("default");
    assert_eq!(
        format!("{:?}", d.border_focused),
        format!("{:?}", ratatui::style::Color::Cyan)
    );
    let m = cpt::theme::Theme::by_name("muted");
    assert_eq!(
        format!("{:?}", m.border_focused),
        format!("{:?}", ratatui::style::Color::White)
    );
}

#[test]
fn theme_by_name_unknown_returns_default() {
    let t = cpt::theme::Theme::by_name("nonexistent");
    let d = cpt::theme::Theme::default();
    assert_eq!(
        format!("{:?}", t.border_focused),
        format!("{:?}", d.border_focused)
    );
}

#[test]
fn theme_is_builtin() {
    assert!(cpt::theme::Theme::is_builtin("default"));
    assert!(cpt::theme::Theme::is_builtin("muted"));
    assert!(!cpt::theme::Theme::is_builtin("my-custom"));
}

#[test]
fn theme_field_get_set() {
    let mut t = cpt::theme::Theme::default();
    assert_eq!(
        format!("{:?}", t.get_field(0)),
        format!("{:?}", t.border_focused)
    );
    t.set_field(0, ratatui::style::Color::Red);
    assert_eq!(
        format!("{:?}", t.get_field(0)),
        format!("{:?}", ratatui::style::Color::Red)
    );
    assert_eq!(
        format!("{:?}", t.border_focused),
        format!("{:?}", ratatui::style::Color::Red)
    );
}

#[test]
fn theme_field_get_set_all() {
    let mut t = cpt::theme::Theme::default();
    for i in 0..cpt::theme::FIELD_COUNT {
        let original = t.get_field(i);
        t.set_field(i, ratatui::style::Color::Magenta);
        assert_eq!(
            format!("{:?}", t.get_field(i)),
            format!("{:?}", ratatui::style::Color::Magenta)
        );
        t.set_field(i, original);
    }
}

#[test]
fn theme_serialize_roundtrip_named_colors() {
    let theme = cpt::theme::Theme::default();
    let serialized = toml::to_string_pretty(&theme).expect("serialize failed");
    let deserialized: cpt::theme::Theme = toml::from_str(&serialized).expect("deserialize failed");
    // Verify all 25 fields match
    for i in 0..cpt::theme::FIELD_COUNT {
        assert_eq!(
            format!("{:?}", theme.get_field(i)),
            format!("{:?}", deserialized.get_field(i)),
            "field {} mismatch after roundtrip",
            i
        );
    }
}

#[test]
fn theme_serialize_roundtrip_indexed_colors() {
    let mut theme = cpt::theme::Theme::default();
    // Set some fields to indexed colors (from the color picker)
    theme.set_field(0, cpt::theme::indexed_to_color(42));
    theme.set_field(1, cpt::theme::indexed_to_color(200));
    theme.set_field(2, cpt::theme::indexed_to_color(232));
    let serialized = toml::to_string_pretty(&theme).expect("serialize failed");
    let deserialized: cpt::theme::Theme = toml::from_str(&serialized).expect("deserialize failed");
    for i in 0..cpt::theme::FIELD_COUNT {
        assert_eq!(
            format!("{:?}", theme.get_field(i)),
            format!("{:?}", deserialized.get_field(i)),
            "field {} mismatch after indexed roundtrip",
            i
        );
    }
}

#[test]
fn theme_save_load_file_roundtrip() {
    let dir = tempdir("theme_save_load");
    let themes_dir = dir.join("themes");
    fs::create_dir_all(&themes_dir).unwrap();

    let mut theme = cpt::theme::Theme::default();
    theme.set_field(0, ratatui::style::Color::Red);

    // Save manually (since save_to_file uses confy path)
    let path = themes_dir.join("test-theme.toml");
    let content = toml::to_string_pretty(&theme).unwrap();
    fs::write(&path, &content).unwrap();

    // Load back
    let loaded_content = fs::read_to_string(&path).unwrap();
    let loaded: cpt::theme::Theme = toml::from_str(&loaded_content).unwrap();
    assert_eq!(
        format!("{:?}", loaded.border_focused),
        format!("{:?}", ratatui::style::Color::Red)
    );

    // Clean up
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn theme_color_index_roundtrip() {
    use cpt::theme::{color_to_index, indexed_to_color};
    // Named colors roundtrip through index
    for idx in 0..=15u8 {
        let color = indexed_to_color(idx);
        let back = color_to_index(color);
        assert_eq!(idx, back, "named color index {} didn't roundtrip", idx);
    }
    // Indexed colors stay as indexed
    for idx in 16..=255u8 {
        let color = indexed_to_color(idx);
        let back = color_to_index(color);
        assert_eq!(idx, back, "indexed color {} didn't roundtrip", idx);
    }
}

#[test]
fn theme_next_includes_builtins() {
    // next_theme_name always includes "default" and "muted" (plus any custom files on disk)
    let names = cpt::theme::Theme::all_theme_names();
    assert!(names.contains(&"default".to_string()));
    assert!(names.contains(&"muted".to_string()));
    // Cycling from default should reach muted eventually
    let mut current = "default".to_string();
    let mut found_muted = false;
    for _ in 0..names.len() {
        current = cpt::theme::Theme::next_theme_name(&current);
        if current == "muted" {
            found_muted = true;
            break;
        }
    }
    assert!(found_muted, "cycling from default never reached muted");
}

#[test]
fn theme_next_unknown_wraps() {
    // Unknown name falls back to index 0, so next is index 1
    let n = cpt::theme::Theme::next_theme_name("nonexistent");
    let names = cpt::theme::Theme::all_theme_names();
    assert_eq!(n, names[1], "should wrap to second theme");
}

#[test]
fn theme_color_display_name_named() {
    assert_eq!(
        cpt::theme::color_display_name(ratatui::style::Color::Cyan),
        "Cyan"
    );
    assert_eq!(
        cpt::theme::color_display_name(ratatui::style::Color::Black),
        "Black"
    );
}

#[test]
fn theme_color_display_name_indexed() {
    // Indexed colors in the cube show R:G:B
    let name = cpt::theme::color_display_name(ratatui::style::Color::Indexed(16));
    assert!(name.contains("0:0:0"), "got: {}", name);
    // Grayscale
    let name = cpt::theme::color_display_name(ratatui::style::Color::Indexed(232));
    assert!(name.contains("gray"), "got: {}", name);
}

// ── TaskState unit tests ──────────────────────────────────────

#[test]
fn task_state_initial_values() {
    let task = cpt::task::TaskState::new("echo hi");
    assert_eq!(task.cmd, "echo hi");
    assert!(task.lines.is_empty());
    assert!(task.running);
    assert_eq!(task.exit_code, None);
    assert_eq!(task.scroll, 0);
    assert!(task.auto_scroll);
}

#[test]
fn task_state_push_line_appends_and_auto_scrolls() {
    let mut task = cpt::task::TaskState::new("cmd");
    task.push_line("line 1".into());
    task.push_line("line 2".into());
    task.push_line("line 3".into());
    assert_eq!(task.lines.len(), 3);
    // auto_scroll is true, so scroll should track the last line
    assert_eq!(task.scroll, 2);
}

#[test]
fn task_state_push_line_no_auto_scroll_when_user_scrolled_up() {
    let mut task = cpt::task::TaskState::new("cmd");
    task.push_line("line 1".into());
    task.push_line("line 2".into());
    task.scroll_up(); // user scrolled up, disables auto_scroll
    assert!(!task.auto_scroll);
    task.push_line("line 3".into());
    // scroll should NOT have jumped to bottom
    assert_eq!(task.scroll, 0);
}

#[test]
fn task_state_push_line_caps_at_max() {
    let mut task = cpt::task::TaskState::new("cmd");
    for i in 0..cpt::task::MAX_LINES + 5 {
        task.push_line(format!("line {}", i));
    }
    assert_eq!(task.lines.len(), cpt::task::MAX_LINES);
}

#[test]
fn task_state_scroll_up_disables_auto_scroll() {
    let mut task = cpt::task::TaskState::new("cmd");
    task.push_line("a".into());
    task.push_line("b".into());
    assert!(task.auto_scroll);
    task.scroll_up();
    assert!(!task.auto_scroll);
    assert_eq!(task.scroll, 0); // was at 1, saturating_sub gives 0
}

#[test]
fn task_state_scroll_down_re_enables_auto_scroll_at_bottom() {
    let mut task = cpt::task::TaskState::new("cmd");
    task.push_line("a".into());
    task.push_line("b".into());
    task.push_line("c".into());
    task.scroll_up();
    task.scroll_up();
    assert!(!task.auto_scroll);
    // scroll down past the bottom edge
    task.scroll_down();
    task.scroll_down();
    task.scroll_down();
    assert!(task.auto_scroll);
}

#[test]
fn task_state_scroll_clamps_to_line_count() {
    let mut task = cpt::task::TaskState::new("cmd");
    task.push_line("only".into());
    task.scroll_down();
    task.scroll_down();
    task.scroll_down();
    // Must not exceed lines.len() - 1
    assert_eq!(task.scroll, 0);
}

#[test]
fn task_state_finish_sets_state() {
    let mut task = cpt::task::TaskState::new("cmd");
    task.finish(0);
    assert!(!task.running);
    assert_eq!(task.exit_code, Some(0));
}

#[test]
fn task_state_exit_summary_while_running() {
    let task = cpt::task::TaskState::new("cmd");
    assert!(task.exit_summary().contains("running"));
}

#[test]
fn task_state_exit_summary_success() {
    let mut task = cpt::task::TaskState::new("cmd");
    task.finish(0);
    assert_eq!(task.exit_summary(), "done");
}

#[test]
fn task_state_exit_summary_nonzero() {
    let mut task = cpt::task::TaskState::new("cmd");
    task.finish(2);
    let summary = task.exit_summary();
    assert!(summary.contains('2'), "expected exit code in: {}", summary);
}

// ── run_task integration tests ────────────────────────────────

#[tokio::test]
async fn run_task_captures_stdout_and_sends_complete() {
    use cpt::action::Action;
    use cpt::task::run_task;
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let cwd = std::env::current_dir().unwrap();

    #[cfg(windows)]
    let cmd = "echo hello_world".to_string();
    #[cfg(not(windows))]
    let cmd = "echo hello_world".to_string();

    run_task(cmd, cwd, tx).await;

    let mut lines: Vec<String> = vec![];
    let mut exit_code: Option<i32> = None;
    while let Ok(action) = rx.try_recv() {
        match action {
            Action::TaskLine(line) => lines.push(line),
            Action::TaskComplete(code) => exit_code = Some(code),
            _ => {}
        }
    }

    assert!(
        lines.iter().any(|l| l.trim().contains("hello_world")),
        "expected 'hello_world' in output lines: {:?}",
        lines
    );
    assert_eq!(exit_code, Some(0));
}

#[tokio::test]
async fn run_task_nonzero_exit_code() {
    use cpt::action::Action;
    use cpt::task::run_task;
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let cwd = std::env::current_dir().unwrap();

    #[cfg(windows)]
    let cmd = "exit 1".to_string();
    #[cfg(not(windows))]
    let cmd = "exit 1".to_string();

    run_task(cmd, cwd, tx).await;

    let mut exit_code: Option<i32> = None;
    while let Ok(action) = rx.try_recv() {
        if let Action::TaskComplete(code) = action {
            exit_code = Some(code);
        }
    }

    // exit 1 through the shell gives exit code 1
    assert_eq!(exit_code, Some(1));
}

#[tokio::test]
async fn run_task_invalid_command_sends_error() {
    use cpt::action::Action;
    use cpt::task::run_task;
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::unbounded_channel();

    // Point to a non-existent working directory to trigger an error.
    let bad_cwd = PathBuf::from("/nonexistent_dir_cpt_test_xyz");
    run_task("echo hi".to_string(), bad_cwd, tx).await;

    let mut got_error_or_complete = false;
    while let Ok(action) = rx.try_recv() {
        match action {
            Action::TaskError(_) | Action::TaskComplete(_) => {
                got_error_or_complete = true;
            }
            _ => {}
        }
    }
    assert!(
        got_error_or_complete,
        "expected TaskError or TaskComplete for bad cwd"
    );
}

#[tokio::test]
async fn run_task_captures_stderr() {
    use cpt::action::Action;
    use cpt::task::run_task;
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let cwd = std::env::current_dir().unwrap();

    #[cfg(windows)]
    let cmd = "echo stderr_test 1>&2".to_string();
    #[cfg(not(windows))]
    let cmd = "echo stderr_test >&2".to_string();

    run_task(cmd, cwd, tx).await;

    let mut lines: Vec<String> = vec![];
    while let Ok(action) = rx.try_recv() {
        if let Action::TaskLine(line) = action {
            lines.push(line);
        }
    }

    assert!(
        lines.iter().any(|l| l.trim().contains("stderr_test")),
        "expected 'stderr_test' in stderr lines: {:?}",
        lines
    );
}

// ── contrast_fg ───────────────────────────────────────────────

#[test]
fn contrast_fg_dark_named_colors_return_white() {
    use cpt::theme::contrast_fg;
    use ratatui::style::Color;
    // Indices 0-6 and 8 are dark - should return white
    assert_eq!(
        contrast_fg(0),
        Color::White,
        "index 0 (Black) should give White"
    );
    assert_eq!(
        contrast_fg(1),
        Color::White,
        "index 1 (Red) should give White"
    );
    assert_eq!(
        contrast_fg(4),
        Color::White,
        "index 4 (Blue) should give White"
    );
    assert_eq!(
        contrast_fg(8),
        Color::White,
        "index 8 (DarkGray) should give White"
    );
}

#[test]
fn contrast_fg_light_named_colors_return_black() {
    use cpt::theme::contrast_fg;
    use ratatui::style::Color;
    // Indices 7 and 9-15 are light - should return black
    assert_eq!(
        contrast_fg(7),
        Color::Black,
        "index 7 (Gray) should give Black"
    );
    assert_eq!(
        contrast_fg(15),
        Color::Black,
        "index 15 (White) should give Black"
    );
    assert_eq!(
        contrast_fg(11),
        Color::Black,
        "index 11 (LightYellow) should give Black"
    );
}

#[test]
fn contrast_fg_grayscale_boundary() {
    use cpt::theme::contrast_fg;
    use ratatui::style::Color;
    // 232-243 are dark grays - should return white
    assert_eq!(contrast_fg(232), Color::White);
    assert_eq!(contrast_fg(243), Color::White);
    // 244-255 are light grays - should return black
    assert_eq!(contrast_fg(244), Color::Black);
    assert_eq!(contrast_fg(255), Color::Black);
}

#[test]
fn contrast_fg_cube_dark_returns_white() {
    use cpt::theme::contrast_fg;
    use ratatui::style::Color;
    // Index 16 = (0,0,0) in the 6x6x6 cube - very dark, should give white
    assert_eq!(contrast_fg(16), Color::White);
    // Index 17 = (0,0,1) - also dark
    assert_eq!(contrast_fg(17), Color::White);
}

#[test]
fn contrast_fg_cube_bright_returns_black() {
    use cpt::theme::contrast_fg;
    use ratatui::style::Color;
    // Index 231 = (5,5,5) in the 6x6x6 cube - very bright, should give black
    assert_eq!(contrast_fg(231), Color::Black);
}

// ── color_display_name edge cases ────────────────────────────

#[test]
fn color_display_name_indexed_low_does_not_panic() {
    use cpt::theme::color_display_name;
    use ratatui::style::Color;
    // Color::Indexed(n) for n < 16 must not underflow or panic.
    // These are not the same as the named variants (Color::Black != Color::Indexed(0)).
    for n in 0u8..16 {
        let name = color_display_name(Color::Indexed(n));
        // Should contain the numeric index at minimum
        assert!(
            name.contains(&n.to_string()),
            "display name for Indexed({}) should include the number, got: {}",
            n,
            name
        );
    }
}

#[test]
fn color_display_name_rgb() {
    use cpt::theme::color_display_name;
    use ratatui::style::Color;
    let name = color_display_name(Color::Rgb(255, 128, 0));
    assert_eq!(name, "#FF8000");
}

// ── TaskState: cap removes oldest lines ──────────────────────

#[test]
fn task_state_push_line_caps_removes_oldest() {
    let mut task = cpt::task::TaskState::new("cmd");
    // Fill to capacity
    for i in 0..cpt::task::MAX_LINES {
        task.push_line(format!("line {}", i));
    }
    assert_eq!(task.lines[0], "line 0");
    assert_eq!(
        task.lines[cpt::task::MAX_LINES - 1],
        format!("line {}", cpt::task::MAX_LINES - 1)
    );

    // Push one more - line 0 should be evicted
    task.push_line("new line".to_string());
    assert_eq!(task.lines.len(), cpt::task::MAX_LINES);
    assert_eq!(
        task.lines[0], "line 1",
        "oldest line should have been removed"
    );
    assert_eq!(task.lines[cpt::task::MAX_LINES - 1], "new line");
}

#[test]
fn task_state_push_line_scroll_adjusts_when_capping_with_manual_scroll() {
    let mut task = cpt::task::TaskState::new("cmd");
    // Fill to capacity
    for i in 0..cpt::task::MAX_LINES {
        task.push_line(format!("line {}", i));
    }
    // Manually scroll to position 10 and disable auto_scroll
    task.scroll = 10;
    task.auto_scroll = false;

    // Push one more line - scroll should decrement to compensate for removed line
    task.push_line("extra".to_string());
    assert_eq!(
        task.scroll, 9,
        "scroll should decrement when oldest line is removed"
    );
}

// ── copy_entries progress message contains item count ─────────

#[tokio::test]
async fn copy_entries_complete_message_contains_count() {
    use cpt::action::Action;

    let src_dir = tempdir("copy_count_src");
    let dst_dir = tempdir("copy_count_dst");
    fs::write(src_dir.join("a.txt"), "a").unwrap();
    fs::write(src_dir.join("b.txt"), "b").unwrap();
    fs::write(src_dir.join("c.txt"), "c").unwrap();

    let sources = vec![
        src_dir.join("a.txt"),
        src_dir.join("b.txt"),
        src_dir.join("c.txt"),
    ];
    let pairs = cpt::fs::ops::build_pairs(&sources, &dst_dir);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::copy_entries(pairs, tx).await;

    let mut complete_msg: Option<String> = None;
    let mut last_done = 0u64;
    while let Ok(action) = rx.try_recv() {
        match action {
            Action::OperationProgress { done, total } => {
                assert_eq!(total, 3);
                last_done = done;
            }
            Action::OperationComplete(msg) => complete_msg = Some(msg),
            _ => {}
        }
    }
    assert_eq!(last_done, 3, "final progress should report 3 items done");
    let msg = complete_msg.expect("expected OperationComplete");
    assert!(
        msg.contains('3'),
        "complete message should mention count: {}",
        msg
    );

    fs::remove_dir_all(&src_dir).ok();
    fs::remove_dir_all(&dst_dir).ok();
}

#[tokio::test]
async fn delete_entries_complete_message_contains_count() {
    use cpt::action::Action;

    let dir = tempdir("delete_count");
    fs::write(dir.join("x.txt"), "x").unwrap();
    fs::write(dir.join("y.txt"), "y").unwrap();

    let sources = vec![dir.join("x.txt"), dir.join("y.txt")];
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    cpt::fs::ops::delete_entries(sources, tx).await;

    let mut complete_msg: Option<String> = None;
    while let Ok(action) = rx.try_recv() {
        if let Action::OperationComplete(msg) = action {
            complete_msg = Some(msg);
        }
    }
    let msg = complete_msg.expect("expected OperationComplete");
    assert!(
        msg.contains('2'),
        "complete message should mention count: {}",
        msg
    );

    fs::remove_dir_all(&dir).ok();
}

// ── build_pairs edge cases ────────────────────────────────────

#[test]
fn build_pairs_empty_sources_returns_empty() {
    let dest = PathBuf::from("/dest");
    let pairs = cpt::fs::ops::build_pairs(&[], &dest);
    assert!(pairs.is_empty());
}

#[test]
fn find_conflicts_all_conflict() {
    let dir = tempdir("all_conflicts");
    fs::write(dir.join("a.txt"), "").unwrap();
    fs::write(dir.join("b.txt"), "").unwrap();

    let pairs = vec![
        (PathBuf::from("/src/a.txt"), dir.join("a.txt")),
        (PathBuf::from("/src/b.txt"), dir.join("b.txt")),
    ];
    let conflicts = cpt::fs::ops::find_conflicts(&pairs);
    assert_eq!(conflicts, vec![0, 1]);

    fs::remove_dir_all(&dir).ok();
}

// ── is_archive ───────────────────────────────────────────────

#[test]
fn is_archive_detects_common_formats() {
    use cpt::util::is_archive;
    assert!(is_archive("archive.zip"));
    assert!(is_archive("backup.7z"));
    assert!(is_archive("data.rar"));
    assert!(is_archive("dist.tar"));
    assert!(is_archive("dist.tar.gz"));
    assert!(is_archive("dist.tar.bz2"));
    assert!(is_archive("dist.tar.xz"));
    assert!(is_archive("dist.tgz"));
    assert!(is_archive("dist.tbz2"));
    assert!(is_archive("compressed.gz"));
    assert!(is_archive("compressed.bz2"));
    assert!(is_archive("compressed.xz"));
}

#[test]
fn is_archive_case_insensitive() {
    use cpt::util::is_archive;
    assert!(is_archive("BACKUP.ZIP"));
    assert!(is_archive("Archive.Zip"));
    assert!(is_archive("DATA.TAR.GZ"));
}

#[test]
fn is_archive_rejects_non_archives() {
    use cpt::util::is_archive;
    assert!(!is_archive("readme.txt"));
    assert!(!is_archive("photo.jpg"));
    assert!(!is_archive("main.rs"));
    assert!(!is_archive("Cargo.toml"));
    assert!(!is_archive("noextension"));
    assert!(!is_archive("file.zip.bak")); // not ending in a supported ext
}

#[test]
fn is_archive_empty_and_dot_names() {
    use cpt::util::is_archive;
    assert!(!is_archive(""));
    // ".zip" ends with ".zip" so it is detected as an archive
    assert!(is_archive(".zip"));
    assert!(!is_archive(".hidden"));
}

// ── resolve_editor ───────────────────────────

#[test]
fn resolve_editor_uses_editor_env() {
    use cpt::fs::open::resolve_editor;
    // Temporarily set $EDITOR to a known value
    unsafe {
        std::env::set_var("EDITOR", "my_custom_editor");
    }
    let result = resolve_editor();
    unsafe {
        std::env::remove_var("EDITOR");
    }
    assert_eq!(result, "my_custom_editor");
}

#[test]
fn resolve_editor_falls_back_when_editor_empty() {
    use cpt::fs::open::resolve_editor;
    unsafe {
        std::env::set_var("EDITOR", "");
        std::env::set_var("VISUAL", "my_visual");
    }
    let result = resolve_editor();
    unsafe {
        std::env::remove_var("EDITOR");
        std::env::remove_var("VISUAL");
    }
    assert_eq!(result, "my_visual");
}

#[test]
fn resolve_editor_falls_back_to_system_default_when_both_empty() {
    use cpt::fs::open::resolve_editor;
    // Unset both - should fall back to nano/vi/notepad
    let old_editor = std::env::var("EDITOR").ok();
    let old_visual = std::env::var("VISUAL").ok();
    unsafe {
        std::env::remove_var("EDITOR");
        std::env::remove_var("VISUAL");
    }
    let result = resolve_editor();
    // Restore
    unsafe {
        if let Some(e) = old_editor {
            std::env::set_var("EDITOR", e);
        }
        if let Some(v) = old_visual {
            std::env::set_var("VISUAL", v);
        }
    }
    // Should return nano, vi, or notepad - not empty
    assert!(!result.is_empty());
    #[cfg(windows)]
    assert_eq!(result, "notepad");
}


// ── resolve_unpack_command ────────────────────────────────────

#[test]
fn resolve_unpack_tar_always_uses_tar() {
    use cpt::fs::archive::resolve_unpack_command;
    let cases = [
        "archive.tar",
        "archive.tar.gz",
        "archive.tgz",
        "archive.tar.bz2",
        "archive.tbz2",
        "archive.tar.xz",
    ];
    for name in &cases {
        let archive = PathBuf::from(format!("/src/{}", name));
        let dest = PathBuf::from("/dest");
        let unpack = resolve_unpack_command(&archive, &dest)
            .unwrap_or_else(|e| panic!("{} should resolve, got: {}", name, e));
        let cmd = unpack.display();
        assert!(cmd.contains("tar"), "{} should use tar, got: {}", name, cmd);
        assert!(
            cmd.contains("/dest"),
            "command should reference dest: {}",
            cmd
        );
    }
}

#[test]
fn resolve_unpack_zip_on_current_os() {
    use cpt::fs::archive::resolve_unpack_command;
    let archive = PathBuf::from("/src/files.zip");
    let dest = PathBuf::from("/dest");
    let result = resolve_unpack_command(&archive, &dest);
    // On Linux/macOS: unzip or 7z. On Windows: PowerShell Expand-Archive.
    // Either way it should succeed on a typical dev machine.
    match result {
        Ok(unpack) => {
            let cmd = unpack.display();
            #[cfg(windows)]
            assert!(
                cmd.contains("Expand-Archive"),
                "Windows ZIP should use Expand-Archive: {}",
                cmd
            );
            #[cfg(not(windows))]
            assert!(
                cmd.contains("unzip") || cmd.contains("7z") || cmd.contains("7za"),
                "Unix ZIP should use unzip or 7z, got: {}",
                cmd
            );
        }
        Err(e) => {
            // Acceptable only if truly no tool is available
            assert!(
                e.contains("unzip") || e.contains("7z"),
                "error message should name the missing tool: {}",
                e
            );
        }
    }
}

#[test]
fn resolve_unpack_7z_returns_err_or_7z_command() {
    use cpt::fs::archive::resolve_unpack_command;
    let archive = PathBuf::from("/src/data.7z");
    let dest = PathBuf::from("/dest");
    match resolve_unpack_command(&archive, &dest) {
        Ok(unpack) => {
            let cmd = unpack.display();
            assert!(
                cmd.contains("7z") || cmd.contains("7za") || cmd.contains("7zz"),
                "should use a 7z binary: {}",
                cmd
            );
        }
        Err(e) => {
            assert!(
                e.contains("7z") || e.contains("7-Zip"),
                "error should mention 7z: {}",
                e
            );
        }
    }
}

#[test]
fn resolve_unpack_rar_returns_err_or_unrar_or_7z() {
    use cpt::fs::archive::resolve_unpack_command;
    let archive = PathBuf::from("/src/archive.rar");
    let dest = PathBuf::from("/dest");
    match resolve_unpack_command(&archive, &dest) {
        Ok(unpack) => {
            let cmd = unpack.display();
            assert!(
                cmd.contains("unrar") || cmd.contains("7z"),
                "should use unrar or 7z: {}",
                cmd
            );
        }
        Err(e) => {
            assert!(
                e.contains("unrar") || e.contains("7z"),
                "error should name the missing tool: {}",
                e
            );
        }
    }
}

#[test]
fn resolve_unpack_unknown_format_returns_err() {
    use cpt::fs::archive::resolve_unpack_command;
    let archive = PathBuf::from("/src/file.docx");
    let dest = PathBuf::from("/dest");
    let result = resolve_unpack_command(&archive, &dest);
    assert!(result.is_err(), "unsupported format should return Err");
    assert!(
        result.unwrap_err().contains("unsupported"),
        "error should say unsupported"
    );
}

#[test]
fn resolve_unpack_command_embeds_paths() {
    use cpt::fs::archive::resolve_unpack_command;
    // TAR always uses tar so we can predict the command reliably
    let archive = PathBuf::from("/my/archive.tar");
    let dest = PathBuf::from("/my/dest dir");
    let unpack = resolve_unpack_command(&archive, &dest).unwrap();
    let cmd = unpack.display();
    assert!(
        cmd.contains("archive.tar"),
        "command should contain archive name: {}",
        cmd
    );
    assert!(
        cmd.contains("dest dir"),
        "command should contain dest path: {}",
        cmd
    );
}

#[cfg(not(windows))]
#[test]
fn shell_quote_wraps_in_single_quotes() {
    use cpt::fs::archive::shell_quote;
    let p = PathBuf::from("/path/to/my file.txt");
    let q = shell_quote(&p);
    assert!(q.starts_with('\''));
    assert!(q.ends_with('\''));
    assert!(q.contains("my file.txt"));
}

#[cfg(not(windows))]
#[test]
fn shell_quote_escapes_single_quotes_in_path() {
    use cpt::fs::archive::shell_quote;
    let p = PathBuf::from("/path/it's here/file.txt");
    let q = shell_quote(&p);
    // Should not contain a bare single quote inside the outer quotes
    // The inner ' must be escaped as '\''
    assert!(q.contains("'\\''"), "single quote should be escaped: {}", q);
}

// ── find_sevenzip ─────────────────────────────────────────────

#[test]
fn find_sevenzip_returns_option() {
    // Just verify it doesn't panic and returns a valid-looking result.
    let result = cpt::util::find_sevenzip();
    if let Some(bin) = &result {
        assert!(!bin.is_empty());
        assert!(bin == "7z" || bin == "7za" || bin == "7zz");
    }
    // None is also acceptable if 7z isn't installed in the test env.
    let _ = result;
}

// ── Explorer: archive extension variants ─────────────────────

#[test]
fn explorer_enter_on_various_archive_extensions() {
    use cpt::action::Action;

    let dir = tempdir("explorer_archive_variants");
    let archives = ["a.zip", "b.7z", "c.tar.gz", "d.tgz", "e.rar"];
    for name in &archives {
        fs::write(dir.join(name), b"fake").unwrap();
    }

    for name in &archives {
        let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
        // Navigate down until we find the target entry (skip ".." at cursor 0)
        let mut found = false;
        for _ in 0..50 {
            explorer.handle_action(&Action::MoveDown);
            if explorer
                .current_entry()
                .map(|e| e.name == *name)
                .unwrap_or(false)
            {
                found = true;
                break;
            }
        }
        assert!(found, "could not find {} in explorer", name);

        let result = explorer.handle_action(&Action::EnterDir);
        assert!(
            matches!(result, Some(Action::UnpackArchive)),
            "{} should trigger UnpackArchive, got {:?}",
            name,
            result
        );
    }

    fs::remove_dir_all(&dir).ok();
}

// ── Theme file I/O ────────────────────────────────────────────

#[test]
fn theme_themes_dir_returns_some() {
    // themes_dir() uses confy to locate the config dir - should succeed on all platforms
    let dir = cpt::theme::Theme::themes_dir();
    assert!(
        dir.is_some(),
        "themes_dir should return Some on a normal system"
    );
    let dir = dir.unwrap();
    // The last component should be "themes"
    assert_eq!(
        dir.file_name().and_then(|n| n.to_str()),
        Some("themes"),
        "themes dir should end with 'themes', got: {}",
        dir.display()
    );
}

#[test]
fn theme_save_to_file_and_delete_file() {
    // Use a unique name unlikely to clash with real themes
    let name = "cpt-test-temp-delete-me";

    // Save a modified theme
    let mut theme = cpt::theme::Theme::default();
    theme.set_field(0, ratatui::style::Color::Indexed(42));

    let save_result = theme.save_to_file(name);
    assert!(
        save_result.is_ok(),
        "save_to_file failed: {:?}",
        save_result.err()
    );

    let saved_path = save_result.unwrap();
    assert!(
        saved_path.exists(),
        "saved theme file should exist at {}",
        saved_path.display()
    );

    // Verify content roundtrips
    let content = fs::read_to_string(&saved_path).unwrap();
    let loaded: cpt::theme::Theme = toml::from_str(&content).unwrap();
    assert_eq!(
        format!("{:?}", loaded.get_field(0)),
        format!("{:?}", ratatui::style::Color::Indexed(42)),
        "loaded theme field 0 should match saved value"
    );

    // Delete the file
    let delete_result = cpt::theme::Theme::delete_file(name);
    assert!(
        delete_result.is_ok(),
        "delete_file failed: {:?}",
        delete_result.err()
    );
    assert!(!saved_path.exists(), "theme file should be deleted");
}

#[test]
fn theme_delete_file_nonexistent_returns_ok() {
    // Deleting a theme that doesn't exist should return Ok (idempotent)
    let result = cpt::theme::Theme::delete_file("cpt-test-nonexistent-theme-xyz");
    assert!(
        result.is_ok(),
        "delete_file on nonexistent should return Ok: {:?}",
        result.err()
    );
}


// ── Explorer: new file highlighting ──────────────────────────

#[test]
fn explorer_new_files_highlighted_then_cleared_on_focus() {
    let dir = tempdir("explorer_new_highlight");
    fs::write(dir.join("existing.txt"), "old").unwrap();
    fs::write(dir.join("new_file.txt"), "new").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    // Mark new_file.txt as new
    explorer.new_files.insert(dir.join("new_file.txt"));

    assert!(explorer.new_files.contains(&dir.join("new_file.txt")));

    // Navigate to the new file (entries sorted alphabetically: existing.txt, new_file.txt)
    explorer.handle_action(&cpt::action::Action::MoveDown); // existing.txt
    explorer.handle_action(&cpt::action::Action::MoveDown); // new_file.txt

    // After focusing on the new file, it should be cleared from new_files
    assert!(
        !explorer.new_files.contains(&dir.join("new_file.txt")),
        "new file highlight should be cleared after cursor lands on it"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_new_files_empty_by_default() {
    let dir = tempdir("explorer_new_empty");
    fs::write(dir.join("a.txt"), "").unwrap();

    let explorer = cpt::components::explorer::Explorer::new(dir.clone());
    assert!(explorer.new_files.is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_new_files_not_cleared_for_other_entries() {
    let dir = tempdir("explorer_new_other");
    fs::write(dir.join("a.txt"), "").unwrap();
    fs::write(dir.join("b.txt"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.new_files.insert(dir.join("b.txt"));

    // Move down to a.txt (first entry after "..")
    explorer.handle_action(&cpt::action::Action::MoveDown);

    // b.txt should still be highlighted since cursor is on a.txt
    assert!(
        explorer.new_files.contains(&dir.join("b.txt")),
        "b.txt highlight should remain while cursor is on a.txt"
    );

    fs::remove_dir_all(&dir).ok();
}

// ── Theme: new_file_fg field ─────────────────────────────────

#[test]
fn theme_new_file_fg_exists_in_default() {
    let theme = cpt::theme::Theme::default();
    assert_eq!(
        format!("{:?}", theme.new_file_fg),
        format!("{:?}", ratatui::style::Color::LightGreen)
    );
}

#[test]
fn theme_new_file_fg_get_set_field() {
    let mut theme = cpt::theme::Theme::default();
    // new_file_fg is at index 5
    theme.set_field(5, ratatui::style::Color::Red);
    assert_eq!(
        format!("{:?}", theme.get_field(5)),
        format!("{:?}", ratatui::style::Color::Red)
    );
    assert_eq!(
        format!("{:?}", theme.new_file_fg),
        format!("{:?}", ratatui::style::Color::Red)
    );
}

#[test]
fn theme_field_count_is_26() {
    assert_eq!(cpt::theme::FIELD_COUNT, 26);
    assert_eq!(cpt::theme::FIELD_INFO.len(), 26);
}

// ── DualPane: mark_new_files ─────────────────────────────────

#[test]
fn dual_pane_mark_new_files_applies_to_matching_pane() {
    let dir = tempdir("dual_pane_new_files");
    fs::write(dir.join("file.txt"), "content").unwrap();

    let mut dual = cpt::components::dual_pane::DualPane::new(
        dir.clone(),
        std::env::temp_dir(),
        cpt::config::PaneSide::Left,
    );

    let new_path = dir.join("file.txt");
    dual.mark_new_files(&[new_path.clone()]);

    // Left pane should have the new file marked
    if let cpt::components::dual_pane::PaneContent::Explorer(e) = &dual.left {
        assert!(
            e.new_files.contains(&new_path),
            "left pane should mark file.txt as new"
        );
    } else {
        panic!("expected Explorer in left pane");
    }

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn dual_pane_snapshot_dir_captures_entries() {
    let dir = tempdir("dual_pane_snapshot");
    fs::write(dir.join("a.txt"), "").unwrap();
    fs::write(dir.join("b.txt"), "").unwrap();

    let dual = cpt::components::dual_pane::DualPane::new(
        dir.clone(),
        std::env::temp_dir(),
        cpt::config::PaneSide::Left,
    );

    let snapshot = dual.snapshot_dir(&dir);
    assert!(snapshot.contains(&dir.join("a.txt")));
    assert!(snapshot.contains(&dir.join("b.txt")));

    fs::remove_dir_all(&dir).ok();
}

// ── is_text_file ─────────────────────────────────────────────

#[test]
fn is_text_file_returns_true_for_plain_text() {
    use cpt::util::is_text_file;
    let dir = tempdir("is_text_plain");
    let p = dir.join("hello.txt");
    fs::write(&p, "hello world\n").unwrap();
    assert!(is_text_file(&p));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn is_text_file_returns_true_for_empty_file() {
    use cpt::util::is_text_file;
    let dir = tempdir("is_text_empty");
    let p = dir.join("empty.txt");
    fs::write(&p, b"").unwrap();
    assert!(is_text_file(&p));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn is_text_file_returns_false_for_binary_with_null_bytes() {
    use cpt::util::is_text_file;
    let dir = tempdir("is_text_binary");
    let p = dir.join("data.bin");
    fs::write(&p, b"\x00\x01\x02\x03binary data\x00").unwrap();
    assert!(!is_text_file(&p));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn is_text_file_returns_false_for_known_binary_extension() {
    use cpt::util::is_text_file;
    let dir = tempdir("is_text_ext");
    // PNG extension is rejected immediately without reading content
    let p = dir.join("image.png");
    fs::write(&p, "this is actually text but has png extension").unwrap();
    assert!(!is_text_file(&p));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn is_text_file_returns_false_for_nonexistent_path() {
    use cpt::util::is_text_file;
    assert!(!is_text_file(&PathBuf::from("/nonexistent_path_cpt_xyz")));
}

#[test]
fn is_text_file_returns_true_for_rust_source() {
    use cpt::util::is_text_file;
    let dir = tempdir("is_text_rs");
    let p = dir.join("main.rs");
    fs::write(&p, "fn main() { println!(\"hi\"); }\n").unwrap();
    assert!(is_text_file(&p));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn is_text_file_returns_false_for_zip_extension() {
    use cpt::util::is_text_file;
    let dir = tempdir("is_text_zip");
    let p = dir.join("archive.zip");
    fs::write(&p, "fake zip content no nulls").unwrap();
    assert!(!is_text_file(&p));
    fs::remove_dir_all(&dir).ok();
}

// ── resolve_pager ─────────────────────────────────────────────

#[test]
fn resolve_pager_uses_pager_env() {
    use cpt::fs::open::resolve_pager;
    let old = std::env::var("PAGER").ok();
    unsafe {
        std::env::set_var("PAGER", "my_custom_pager");
    }
    let result = resolve_pager();
    unsafe {
        match &old {
            Some(v) => std::env::set_var("PAGER", v),
            None => std::env::remove_var("PAGER"),
        }
    }
    assert_eq!(result, "my_custom_pager");
}

#[test]
fn resolve_pager_falls_back_when_pager_empty() {
    use cpt::fs::open::resolve_pager;
    let old = std::env::var("PAGER").ok();
    unsafe {
        std::env::set_var("PAGER", "");
    }
    let result = resolve_pager();
    unsafe {
        match &old {
            Some(v) => std::env::set_var("PAGER", v),
            None => std::env::remove_var("PAGER"),
        }
    }
    assert!(!result.is_empty());
    #[cfg(windows)]
    assert_eq!(result, "more");
    #[cfg(not(windows))]
    assert_eq!(result, "less");
}

#[test]
fn resolve_pager_falls_back_when_pager_unset() {
    use cpt::fs::open::resolve_pager;
    let old = std::env::var("PAGER").ok();
    unsafe {
        std::env::remove_var("PAGER");
    }
    let result = resolve_pager();
    unsafe {
        if let Some(v) = old {
            std::env::set_var("PAGER", v);
        }
    }
    assert!(!result.is_empty());
    #[cfg(windows)]
    assert_eq!(result, "more");
    #[cfg(not(windows))]
    assert_eq!(result, "less");
}

// ── Explorer: binary file enter -> OpenFile ───────────────────

#[test]
fn explorer_enter_on_binary_file_returns_open_file() {
    use cpt::action::Action;
    let dir = tempdir("explorer_binary_enter");
    // File with null bytes is detected as binary
    let p = dir.join("data.dat");
    fs::write(&p, b"\x00\x01\x02binary\x00").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    explorer.handle_action(&Action::MoveDown);

    let result = explorer.handle_action(&Action::EnterDir);
    assert!(
        matches!(result, Some(Action::OpenFile)),
        "binary file should return OpenFile, got {:?}",
        result
    );

    fs::remove_dir_all(&dir).ok();
}

// ── Helpers ──────────────────────────────────────────────────

fn tempdir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cpt_test_{}", prefix));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
