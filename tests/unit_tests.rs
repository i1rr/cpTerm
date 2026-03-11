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

    cpt::fs::ops::copy_entries(sources, dst_dir.clone(), tx).await;

    // Drain messages
    let mut messages = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        messages.push(msg);
    }

    assert!(dst_dir.join("file1.txt").exists());
    assert!(dst_dir.join("file2.txt").exists());
    assert_eq!(fs::read_to_string(dst_dir.join("file1.txt")).unwrap(), "content1");

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

    cpt::fs::ops::copy_entries(sources, dst_dir.clone(), tx).await;

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

    cpt::fs::ops::move_entries(sources, dst_dir.clone(), tx).await;

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

    cpt::fs::ops::copy_entries(sources, dst_dir.clone(), tx).await;

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
fn explorer_toggle_hidden() {
    let dir = tempdir("explorer_hidden");
    fs::write(dir.join("visible.txt"), "").unwrap();
    fs::write(dir.join(".hidden"), "").unwrap();

    let mut explorer = cpt::components::explorer::Explorer::new(dir.clone());
    assert!(!explorer.show_hidden);

    // Count visible entries (hidden files filtered out)
    let visible_count = explorer.filtered.len();

    // Toggle hidden on
    explorer.handle_action(&cpt::action::Action::ToggleHidden);
    assert!(explorer.show_hidden);
    assert!(explorer.filtered.len() >= visible_count);

    // Toggle hidden off
    explorer.handle_action(&cpt::action::Action::ToggleHidden);
    assert!(!explorer.show_hidden);
    assert_eq!(explorer.filtered.len(), visible_count);

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

// ── Helpers ──────────────────────────────────────────────────

fn tempdir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cpt_test_{}", prefix));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
