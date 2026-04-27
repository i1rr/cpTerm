use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Local, TimeZone};

use cpt::action::Action;
use cpt::components::dialog::{Dialog, DialogKind};
use cpt::components::dual_pane::{DualPane, PaneContent};
use cpt::components::explorer::Explorer;
use cpt::config::PaneSide;
use cpt::fs::archive::{UnpackCommand, resolve_unpack_command};
use cpt::fs::entry::{FileEntry, SortColumn, read_directory, sort_entries};
use cpt::fs::ops::{
    build_pairs, copy_entries, delete_entries, find_conflicts, move_entries, rename_conflicts,
    unique_target,
};
use cpt::task::{MAX_LINES, TaskState, run_task};
use cpt::theme::{
    FIELD_COUNT, FIELD_INFO, Theme, color_display_name, color_to_index, contrast_fg,
    indexed_to_color,
};
use cpt::util::{
    clean_canonicalize, find_sevenzip, format_date, format_size, is_archive, is_text_file,
    strip_unc_prefix,
};
use ratatui::style::Color;
use tokio::sync::mpsc;

// ── helpers ──────────────────────────────────────────────────

fn tempdir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cpt_test_{}", prefix));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

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

fn dated_entry(name: &str, year: i32) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: PathBuf::from(name),
        is_dir: false,
        size: 0,
        modified: Some(Local.with_ymd_and_hms(year, 1, 1, 0, 0, 0).unwrap()),
        is_hidden: false,
    }
}

fn drain<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> Vec<T> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        out.push(msg);
    }
    out
}

// ── util: format ────────────────────────────────────────────

#[test]
fn format_size_covers_unit_boundaries() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(1023), "1023 B");
    assert_eq!(format_size(1024), "1.0 KB");
    assert_eq!(format_size(1536), "1.5 KB");
    assert_eq!(format_size(1024 * 1024), "1.0 MB");
    assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    assert_eq!(format_size(1024u64 * 1024 * 1024 * 3), "3.0 GB");
}

#[test]
fn format_date_produces_expected_pattern() {
    let dt: DateTime<Local> = Local::now();
    let result = format_date(&dt);
    assert_eq!(result.len(), 11);
    assert_eq!(result.as_bytes()[2], b'-');
    assert_eq!(result.as_bytes()[5], b' ');
    assert_eq!(result.as_bytes()[8], b':');
}

#[test]
fn strip_unc_prefix_leaves_normal_path() {
    let path = PathBuf::from(r"C:\Users\test");
    assert_eq!(strip_unc_prefix(path.clone()), path);
}

#[cfg(windows)]
#[test]
fn strip_unc_prefix_removes_windows_prefix() {
    let path = PathBuf::from(r"\\?\C:\Users\test");
    assert_eq!(strip_unc_prefix(path), PathBuf::from(r"C:\Users\test"));
}

#[test]
fn clean_canonicalize_strips_unc_prefix() {
    let dir = tempdir("canon_test");
    let result = clean_canonicalize(&dir).unwrap();
    assert!(!result.to_string_lossy().starts_with(r"\\?\"));
    assert!(result.is_dir());
    fs::remove_dir_all(&dir).ok();
}

// ── util: archive / text detection ──────────────────────────

#[test]
fn is_archive_detects_known_extensions() {
    let archives = [
        "archive.zip",
        "BACKUP.ZIP",
        "Archive.Zip",
        "backup.7z",
        "data.rar",
        "dist.tar",
        "DATA.TAR.GZ",
        "dist.tar.bz2",
        "dist.tar.xz",
        "dist.tgz",
        "dist.tbz2",
        "compressed.gz",
        "compressed.bz2",
        "compressed.xz",
        ".zip",
    ];
    for name in archives {
        assert!(is_archive(name), "{} should be archive", name);
    }
    let non_archives = [
        "readme.txt",
        "photo.jpg",
        "main.rs",
        "Cargo.toml",
        "noextension",
        "file.zip.bak",
        "",
        ".hidden",
    ];
    for name in non_archives {
        assert!(!is_archive(name), "{} should not be archive", name);
    }
}

#[test]
fn is_text_file_classifies_text_inputs() {
    let dir = tempdir("is_text_pos");
    let plain = dir.join("hello.txt");
    fs::write(&plain, "hello world\n").unwrap();
    let empty = dir.join("empty.txt");
    fs::write(&empty, b"").unwrap();
    let rust = dir.join("main.rs");
    fs::write(&rust, "fn main() {}\n").unwrap();

    for p in [&plain, &empty, &rust] {
        assert!(is_text_file(p), "{} should be text", p.display());
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn is_text_file_rejects_binary_inputs() {
    let dir = tempdir("is_text_neg");
    let null_bytes = dir.join("data.bin");
    fs::write(&null_bytes, b"\x00\x01\x02binary\x00").unwrap();
    let png_ext = dir.join("image.png");
    fs::write(&png_ext, "actually text but png ext").unwrap();
    let zip_ext = dir.join("archive.zip");
    fs::write(&zip_ext, "no nulls but zip ext").unwrap();

    for p in [&null_bytes, &png_ext, &zip_ext] {
        assert!(!is_text_file(p), "{} should not be text", p.display());
    }
    assert!(!is_text_file(&PathBuf::from("/nonexistent_path_xyz_abc")));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn find_sevenzip_returns_known_binary_or_none() {
    if let Some(bin) = find_sevenzip() {
        assert!(matches!(bin.as_str(), "7z" | "7za" | "7zz"));
    }
}

// ── FileEntry / sort / read_directory ───────────────────────

#[test]
fn file_entry_from_path_classifies_files_and_dirs() {
    let dir = tempdir("entry_classify");
    let file = dir.join("test.txt");
    fs::write(&file, "hello").unwrap();
    let sub = dir.join("subdir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("inner.txt"), "data").unwrap();
    let hidden = dir.join(".hidden");
    fs::write(&hidden, "").unwrap();

    let f = FileEntry::from_path(&file).unwrap();
    assert_eq!(f.name, "test.txt");
    assert!(!f.is_dir);
    assert_eq!(f.size, 5);
    assert!(f.modified.is_some());
    assert!(!f.is_hidden);

    let d = FileEntry::from_path(&sub).unwrap();
    assert!(d.is_dir);
    assert_eq!(d.size, 0, "directory size is always reported as 0");

    let h = FileEntry::from_path(&hidden).unwrap();
    assert!(h.is_hidden);

    assert!(FileEntry::from_path(&PathBuf::from("/no_such_path_abc123")).is_none());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn sort_entries_dirs_first_then_by_column() {
    let mut entries = vec![
        make_entry("file_a.txt", false, 100),
        make_entry("dir_b", true, 0),
        make_entry("file_c.txt", false, 50),
        make_entry("dir_a", true, 0),
    ];
    sort_entries(&mut entries, SortColumn::Name, true);
    assert_eq!(
        entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["dir_a", "dir_b", "file_a.txt", "file_c.txt"]
    );
}

#[test]
fn sort_by_name_respects_direction() {
    let mut entries = vec![
        make_entry("Zebra", false, 0),
        make_entry("apple", false, 0),
        make_entry("Mango", false, 0),
    ];
    sort_entries(&mut entries, SortColumn::Name, true);
    assert_eq!(entries[0].name, "apple");
    assert_eq!(entries[2].name, "Zebra");

    sort_entries(&mut entries, SortColumn::Name, false);
    assert_eq!(entries[0].name, "Zebra");
    assert_eq!(entries[2].name, "apple");
}

#[test]
fn sort_by_size_respects_direction() {
    let mut entries = vec![
        make_entry("big", false, 1000),
        make_entry("small", false, 10),
        make_entry("medium", false, 500),
    ];
    sort_entries(&mut entries, SortColumn::Size, true);
    assert_eq!(
        entries.iter().map(|e| e.size).collect::<Vec<_>>(),
        vec![10, 500, 1000]
    );

    sort_entries(&mut entries, SortColumn::Size, false);
    assert_eq!(
        entries.iter().map(|e| e.size).collect::<Vec<_>>(),
        vec![1000, 500, 10]
    );
}

#[test]
fn sort_by_date_orders_by_modified() {
    let mut entries = vec![
        dated_entry("old", 2020),
        dated_entry("new", 2025),
        dated_entry("mid", 2023),
    ];
    sort_entries(&mut entries, SortColumn::Date, true);
    assert_eq!(
        entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["old", "mid", "new"]
    );
    sort_entries(&mut entries, SortColumn::Date, false);
    assert_eq!(
        entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["new", "mid", "old"]
    );
}

#[test]
fn read_directory_returns_entries_or_empty() {
    let dir = tempdir("read_dir");
    fs::write(dir.join("a.txt"), "aaa").unwrap();
    fs::write(dir.join(".secret"), "").unwrap();
    fs::create_dir(dir.join("subdir")).unwrap();

    let entries = read_directory(&dir);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(entries.len(), 3);
    assert!(entries[0].is_dir, "directories sorted first");
    assert!(names.contains(&".secret"), "hidden files included");

    let empty_dir = tempdir("read_empty");
    assert!(read_directory(&empty_dir).is_empty());
    assert!(read_directory(&PathBuf::from("/no_such_dir_xyz_abc")).is_empty());

    fs::remove_dir_all(&dir).ok();
    fs::remove_dir_all(&empty_dir).ok();
}

// ── fs/ops: pure helpers ────────────────────────────────────

#[test]
fn build_pairs_creates_pairs_or_empty() {
    let dest = PathBuf::from("/dest");
    let pairs = build_pairs(
        &[PathBuf::from("/src/a.txt"), PathBuf::from("/src/b.txt")],
        &dest,
    );
    assert_eq!(
        pairs,
        vec![
            (PathBuf::from("/src/a.txt"), PathBuf::from("/dest/a.txt")),
            (PathBuf::from("/src/b.txt"), PathBuf::from("/dest/b.txt")),
        ]
    );
    assert!(build_pairs(&[], &dest).is_empty());
}

#[test]
fn find_conflicts_reports_existing_targets() {
    let dir = tempdir("conflicts");
    fs::write(dir.join("a.txt"), "").unwrap();
    fs::write(dir.join("b.txt"), "").unwrap();

    let some_conflict = vec![
        (PathBuf::from("/src/a.txt"), dir.join("a.txt")),
        (PathBuf::from("/src/new.txt"), dir.join("new.txt")),
    ];
    assert_eq!(find_conflicts(&some_conflict), vec![0]);

    let all_conflict = vec![
        (PathBuf::from("/src/a.txt"), dir.join("a.txt")),
        (PathBuf::from("/src/b.txt"), dir.join("b.txt")),
    ];
    assert_eq!(find_conflicts(&all_conflict), vec![0, 1]);

    let none = vec![
        (PathBuf::from("/src/x.txt"), dir.join("x.txt")),
        (PathBuf::from("/src/y.txt"), dir.join("y.txt")),
    ];
    assert!(find_conflicts(&none).is_empty());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unique_target_appends_increment_until_free() {
    let dir = tempdir("unique");
    fs::write(dir.join("file.txt"), "").unwrap();
    assert_eq!(
        unique_target(&dir.join("file.txt")),
        dir.join("file (1).txt")
    );

    fs::write(dir.join("file (1).txt"), "").unwrap();
    assert_eq!(
        unique_target(&dir.join("file.txt")),
        dir.join("file (2).txt")
    );

    fs::write(dir.join("readme"), "").unwrap();
    assert_eq!(unique_target(&dir.join("readme")), dir.join("readme (1)"));

    fs::create_dir(dir.join("mydir")).unwrap();
    assert_eq!(unique_target(&dir.join("mydir")), dir.join("mydir (1)"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn rename_conflicts_only_touches_existing() {
    let dir = tempdir("rename_conflicts");
    fs::write(dir.join("exists.txt"), "").unwrap();

    let mut pairs = vec![
        (PathBuf::from("/src/exists.txt"), dir.join("exists.txt")),
        (PathBuf::from("/src/new.txt"), dir.join("new.txt")),
    ];
    rename_conflicts(&mut pairs);
    assert_eq!(pairs[0].1, dir.join("exists (1).txt"));
    assert_eq!(pairs[1].1, dir.join("new.txt"));

    fs::remove_dir_all(&dir).ok();
}

// ── fs/ops: copy / move / delete (basic async) ──────────────

#[tokio::test]
async fn copy_entries_copies_files_and_directories_recursively() {
    let src = tempdir("copy_src");
    let dst = tempdir("copy_dst");
    fs::write(src.join("file.txt"), "content").unwrap();
    let sub = src.join("nested");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("inner.txt"), "inner").unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let pairs = build_pairs(&[src.join("file.txt"), sub.clone()], &dst);
    copy_entries(pairs, tx).await;
    drop(rx.try_recv().ok()); // discard messages

    assert_eq!(fs::read_to_string(dst.join("file.txt")).unwrap(), "content");
    assert!(dst.join("nested").is_dir());
    assert_eq!(
        fs::read_to_string(dst.join("nested").join("inner.txt")).unwrap(),
        "inner"
    );

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}

#[tokio::test]
async fn copy_entries_emits_progress_then_complete_message() {
    let src = tempdir("copy_progress_src");
    let dst = tempdir("copy_progress_dst");
    for n in ["a.txt", "b.txt", "c.txt"] {
        fs::write(src.join(n), n).unwrap();
    }

    let (tx, mut rx) = mpsc::unbounded_channel();
    let sources: Vec<PathBuf> = ["a.txt", "b.txt", "c.txt"]
        .iter()
        .map(|n| src.join(n))
        .collect();
    copy_entries(build_pairs(&sources, &dst), tx).await;

    let mut last_done = 0u64;
    let mut total_seen = 0u64;
    let mut complete_msg = None;
    for msg in drain(&mut rx) {
        match msg {
            Action::OperationProgress { done, total } => {
                total_seen = total;
                last_done = done;
            }
            Action::OperationComplete(s) => complete_msg = Some(s),
            _ => {}
        }
    }
    assert_eq!(total_seen, 3);
    assert_eq!(last_done, 3);
    let msg = complete_msg.expect("complete event missing");
    assert!(
        msg.contains('3'),
        "complete msg should mention count: {}",
        msg
    );

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}

#[tokio::test]
async fn copy_entries_stops_on_first_error() {
    let src = tempdir("copy_partial_src");
    let dst = tempdir("copy_partial_dst");
    fs::write(src.join("good.txt"), "ok").unwrap();

    let sources = vec![src.join("good.txt"), src.join("missing.txt")];
    let (tx, mut rx) = mpsc::unbounded_channel();
    copy_entries(build_pairs(&sources, &dst), tx).await;

    let mut progress = 0;
    let mut errors = 0;
    let mut completed = false;
    for msg in drain(&mut rx) {
        match msg {
            Action::OperationProgress { .. } => progress += 1,
            Action::OperationError(_) => errors += 1,
            Action::OperationComplete(_) => completed = true,
            _ => {}
        }
    }
    assert_eq!(progress, 1);
    assert_eq!(errors, 1);
    assert!(!completed, "must not signal complete after error");

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}

#[tokio::test]
async fn copy_with_rename_keeps_original_and_writes_renamed() {
    let src = tempdir("copy_rename_src");
    let dst = tempdir("copy_rename_dst");
    fs::write(src.join("file.txt"), "new").unwrap();
    fs::write(dst.join("file.txt"), "original").unwrap();

    let mut pairs = build_pairs(&[src.join("file.txt")], &dst);
    rename_conflicts(&mut pairs);
    let (tx, mut rx) = mpsc::unbounded_channel();
    copy_entries(pairs, tx).await;
    drain(&mut rx);

    assert_eq!(
        fs::read_to_string(dst.join("file.txt")).unwrap(),
        "original"
    );
    assert_eq!(fs::read_to_string(dst.join("file (1).txt")).unwrap(), "new");

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}

#[tokio::test]
async fn copy_overwrite_replaces_existing() {
    let src = tempdir("copy_ow_src");
    let dst = tempdir("copy_ow_dst");
    fs::write(src.join("file.txt"), "updated").unwrap();
    fs::write(dst.join("file.txt"), "old").unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel();
    copy_entries(build_pairs(&[src.join("file.txt")], &dst), tx).await;
    drain(&mut rx);
    assert_eq!(fs::read_to_string(dst.join("file.txt")).unwrap(), "updated");

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}

#[tokio::test]
async fn move_entries_moves_directory_with_contents() {
    let src = tempdir("move_dir_src");
    let dst = tempdir("move_dir_dst");
    let sub = src.join("mydir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("file.txt"), "data").unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel();
    move_entries(build_pairs(std::slice::from_ref(&sub), &dst), tx).await;
    drain(&mut rx);

    assert!(!sub.exists(), "source removed after move");
    assert_eq!(
        fs::read_to_string(dst.join("mydir").join("file.txt")).unwrap(),
        "data"
    );

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}

#[tokio::test]
async fn delete_entries_removes_files_and_directories() {
    let dir = tempdir("delete");
    let file = dir.join("file.txt");
    fs::write(&file, "x").unwrap();
    let sub = dir.join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("inner.txt"), "y").unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel();
    delete_entries(vec![file.clone(), sub.clone()], tx).await;

    let mut count_msg = None;
    for msg in drain(&mut rx) {
        if let Action::OperationComplete(s) = msg {
            count_msg = Some(s);
        }
    }
    assert!(!file.exists());
    assert!(!sub.exists());
    let msg = count_msg.expect("complete event missing");
    assert!(
        msg.contains('2'),
        "complete msg should mention count: {}",
        msg
    );

    fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn ops_report_error_for_missing_sources() {
    let dst = tempdir("err_dst");

    let (tx, mut rx) = mpsc::unbounded_channel();
    move_entries(
        build_pairs(&[PathBuf::from("/nonexistent_move_xyz")], &dst),
        tx,
    )
    .await;
    let move_err = drain(&mut rx)
        .into_iter()
        .any(|m| matches!(m, Action::OperationError(_)));
    assert!(move_err, "move should report error for missing source");

    let (tx, mut rx) = mpsc::unbounded_channel();
    delete_entries(vec![PathBuf::from("/nonexistent_del_xyz")], tx).await;
    let del_err = drain(&mut rx)
        .into_iter()
        .any(|m| matches!(m, Action::OperationError(_)));
    assert!(del_err, "delete should report error for missing source");

    fs::remove_dir_all(&dst).ok();
}

// ── Explorer ────────────────────────────────────────────────

#[test]
fn explorer_navigation_moves_within_bounds() {
    let dir = tempdir("nav");
    for n in ["a.txt", "b.txt", "c.txt"] {
        fs::write(dir.join(n), "").unwrap();
    }

    let mut e = Explorer::new(dir.clone());
    assert_eq!(e.cursor, 0);
    assert!(e.current_entry().is_none(), "cursor 0 is the .. row");

    e.handle_action(&Action::MoveDown);
    assert_eq!(e.cursor, 1);
    assert_eq!(e.current_entry().unwrap().name, "a.txt");

    e.handle_action(&Action::MoveToBottom);
    assert_eq!(e.cursor, 3);
    e.handle_action(&Action::MoveDown);
    assert_eq!(e.cursor, 3, "MoveDown clamps at bottom");

    e.handle_action(&Action::MoveToTop);
    e.handle_action(&Action::MoveUp);
    assert_eq!(e.cursor, 0, "MoveUp clamps at top");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_page_up_down_pages_and_clamps() {
    let dir = tempdir("page");
    for i in 0..50 {
        fs::write(dir.join(format!("f_{:03}.txt", i)), "").unwrap();
    }

    let mut e = Explorer::new(dir.clone());
    e.handle_action(&Action::PageDown);
    assert_eq!(e.cursor, 20);
    e.handle_action(&Action::PageDown);
    assert_eq!(e.cursor, 40);
    e.handle_action(&Action::PageUp);
    assert_eq!(e.cursor, 20);
    e.handle_action(&Action::PageUp);
    e.handle_action(&Action::PageUp);
    assert_eq!(e.cursor, 0, "PageUp clamps to top");

    let small = tempdir("page_clamp");
    fs::write(small.join("a.txt"), "").unwrap();
    fs::write(small.join("b.txt"), "").unwrap();
    let mut s = Explorer::new(small.clone());
    s.handle_action(&Action::PageDown);
    assert_eq!(s.cursor, 2, "PageDown clamps to last item in short list");

    fs::remove_dir_all(&dir).ok();
    fs::remove_dir_all(&small).ok();
}

#[test]
fn explorer_selection_actions_track_state() {
    let dir = tempdir("sel");
    for n in ["a.txt", "b.txt", "c.txt"] {
        fs::write(dir.join(n), "xx").unwrap();
    }

    let mut e = Explorer::new(dir.clone());

    // ToggleSelect on .. must not select anything.
    e.handle_action(&Action::ToggleSelect);
    assert!(e.selected.is_empty(), "ToggleSelect on .. does not select");

    let mut e = Explorer::new(dir.clone());
    e.handle_action(&Action::MoveDown);
    e.handle_action(&Action::ToggleSelect);
    assert_eq!(e.selected.len(), 1);
    assert_eq!(
        e.cursor, 2,
        "ToggleSelect advances cursor past selected entry"
    );

    e.handle_action(&Action::SelectAll);
    assert!(
        e.selected.is_empty(),
        "SelectAll with partial selection deselects"
    );
    e.handle_action(&Action::SelectAll);
    assert_eq!(e.selected.len(), 3, "SelectAll on empty selects all");
    assert_eq!(e.selected_total_size(), 6);
    assert_eq!(e.selected_paths().len(), 3);
    assert_eq!(e.selected_entries().len(), 3);

    e.handle_action(&Action::DeselectAll);
    assert!(e.selected.is_empty());

    e.handle_action(&Action::SelectAll);
    e.handle_action(&Action::Refresh);
    assert!(e.selected.is_empty(), "Refresh clears selection");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_filter_lifecycle() {
    let dir = tempdir("filter");
    for n in ["Apple.txt", "apricot.txt", "banana.txt"] {
        fs::write(dir.join(n), "").unwrap();
    }

    let mut e = Explorer::new(dir.clone());
    assert_eq!(e.filtered.len(), 3);

    e.handle_action(&Action::StartFilter);
    assert!(e.filter_text.is_some());
    assert_eq!(e.filtered.len(), 3, "empty filter shows all");

    e.handle_action(&Action::FilterInput('a'));
    e.handle_action(&Action::FilterInput('p'));
    assert_eq!(
        e.filtered.len(),
        2,
        "'ap' matches Apple and apricot case-insensitively"
    );

    e.handle_action(&Action::FilterInput('z'));
    assert_eq!(e.filtered.len(), 0);
    assert_eq!(e.cursor, 0, "cursor clamped when filter empties result");

    e.handle_action(&Action::FilterBackspace);
    e.handle_action(&Action::FilterConfirm);
    assert_eq!(e.filter_text.as_deref(), Some("ap"));
    assert_eq!(e.filtered.len(), 2, "Confirm preserves filter text");

    e.handle_action(&Action::FilterCancel);
    assert!(e.filter_text.is_none());
    assert_eq!(e.filtered.len(), 3);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_filter_cleared_on_directory_change() {
    let dir = tempdir("filter_change");
    let sub = dir.join("child");
    fs::create_dir(&sub).unwrap();

    let mut e = Explorer::new(dir.clone());
    e.handle_action(&Action::StartFilter);
    e.handle_action(&Action::FilterInput('c'));
    assert!(e.filter_text.is_some());

    e.handle_action(&Action::MoveDown);
    e.handle_action(&Action::EnterDir);
    assert!(e.filter_text.is_none(), "EnterDir clears filter");
    assert_eq!(e.current_dir, sub);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_enter_dir_and_parent_navigation() {
    let dir = tempdir("enter_parent");
    let target = dir.join("target");
    fs::create_dir(&target).unwrap();
    fs::create_dir(dir.join("aaa_sibling")).unwrap();

    let mut e = Explorer::new(target.clone());
    assert_eq!(e.cursor, 0);

    let action = e.handle_action(&Action::EnterDir);
    assert!(
        matches!(action, Some(Action::ParentDir)),
        "Enter on .. yields ParentDir"
    );

    e.handle_action(&Action::ParentDir);
    assert_eq!(e.current_dir, dir);
    let cur = e
        .current_entry()
        .expect("cursor should land on previous dir");
    assert_eq!(
        cur.name, "target",
        "ParentDir restores cursor on previous child"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_enter_dispatches_by_file_kind() {
    let dir = tempdir("enter_kinds");
    fs::write(dir.join("readme.txt"), "hi").unwrap();
    fs::write(dir.join("data.bin"), b"\x00\x01binary\x00").unwrap();
    let archives = ["a.zip", "b.7z", "c.tar.gz", "d.tgz", "e.rar"];
    for name in &archives {
        fs::write(dir.join(name), b"fake").unwrap();
    }

    let go_to = |target: &str| -> Option<Action> {
        let mut e = Explorer::new(dir.clone());
        for _ in 0..50 {
            e.handle_action(&Action::MoveDown);
            if e.current_entry().map(|x| x.name == target).unwrap_or(false) {
                return e.handle_action(&Action::EnterDir);
            }
        }
        panic!("entry {} not found", target);
    };

    assert!(matches!(
        go_to("readme.txt"),
        Some(Action::OpenEditor { .. })
    ));
    assert!(matches!(go_to("data.bin"), Some(Action::OpenFile)));
    for name in &archives {
        assert!(
            matches!(go_to(name), Some(Action::UnpackArchive)),
            "{} should yield UnpackArchive",
            name
        );
    }

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_shows_hidden_files() {
    let dir = tempdir("hidden");
    fs::write(dir.join("visible.txt"), "").unwrap();
    fs::write(dir.join(".hidden"), "").unwrap();

    let e = Explorer::new(dir.clone());
    let names: Vec<&str> = e
        .filtered
        .iter()
        .map(|&i| e.entries[i].name.as_str())
        .collect();
    assert!(names.contains(&".hidden"));
    assert!(names.contains(&"visible.txt"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explorer_new_files_highlighted_until_cursor_focuses() {
    let dir = tempdir("new_highlight");
    fs::write(dir.join("old.txt"), "").unwrap();
    fs::write(dir.join("new.txt"), "").unwrap();

    let mut e = Explorer::new(dir.clone());
    assert!(e.new_files.is_empty(), "no new files by default");

    let new_path = dir.join("new.txt");
    e.new_files.insert(new_path.clone());
    assert!(e.new_files.contains(&new_path));

    // sorted: new.txt, old.txt — moving down once lands on new.txt
    e.handle_action(&Action::MoveDown);
    assert!(
        !e.new_files.contains(&new_path),
        "highlight cleared on focus"
    );

    // not cleared when cursor is on a different entry
    let mut e2 = Explorer::new(dir.clone());
    let other = dir.join("old.txt");
    e2.new_files.insert(other.clone());
    e2.handle_action(&Action::MoveDown); // lands on new.txt
    assert!(e2.new_files.contains(&other));

    fs::remove_dir_all(&dir).ok();
}

// ── DualPane ────────────────────────────────────────────────

#[test]
fn dual_pane_switch_and_inactive_dir() {
    let l = tempdir("dual_l");
    let r = tempdir("dual_r");

    let mut d = DualPane::new(l.clone(), r.clone(), PaneSide::Left);
    assert_eq!(d.active, PaneSide::Left);
    assert_eq!(d.inactive_dir(), r);

    d.handle_action(&Action::SwitchPane);
    assert_eq!(d.active, PaneSide::Right);
    d.handle_action(&Action::FocusLeft);
    assert_eq!(d.active, PaneSide::Left);

    fs::remove_dir_all(&l).ok();
    fs::remove_dir_all(&r).ok();
}

#[test]
fn dual_pane_dispatches_only_to_active_pane() {
    let l = tempdir("dual_active_l");
    let r = tempdir("dual_active_r");
    fs::write(l.join("a.txt"), "aaa").unwrap();
    fs::write(r.join("b.txt"), "bbb").unwrap();

    let mut d = DualPane::new(l.clone(), r.clone(), PaneSide::Left);
    d.handle_action(&Action::SelectAll);
    assert_eq!(d.left.as_explorer().unwrap().selected.len(), 1);
    assert_eq!(d.right.as_explorer().unwrap().selected.len(), 0);

    d.handle_action(&Action::SwitchPane);
    d.handle_action(&Action::SelectAll);
    assert_eq!(d.left.as_explorer().unwrap().selected.len(), 1);
    assert_eq!(d.right.as_explorer().unwrap().selected.len(), 1);

    fs::remove_dir_all(&l).ok();
    fs::remove_dir_all(&r).ok();
}

#[test]
fn dual_pane_refresh_picks_up_new_files() {
    let l = tempdir("dual_refresh_l");
    let r = tempdir("dual_refresh_r");
    fs::write(l.join("a.txt"), "").unwrap();

    let mut d = DualPane::new(l.clone(), r.clone(), PaneSide::Left);
    assert_eq!(d.left.as_explorer().unwrap().filtered.len(), 1);

    fs::write(l.join("b.txt"), "").unwrap();
    d.handle_action(&Action::Refresh);
    assert_eq!(d.left.as_explorer().unwrap().filtered.len(), 2);

    fs::remove_dir_all(&l).ok();
    fs::remove_dir_all(&r).ok();
}

#[test]
fn dual_pane_enter_on_text_file_opens_editor_in_active() {
    let dir = tempdir("dual_open_editor");
    fs::write(dir.join("readme.txt"), "hi").unwrap();

    let mut d = DualPane::new(dir.clone(), dir.clone(), PaneSide::Left);
    d.handle_action(&Action::MoveDown);
    let follow = d.handle_action(&Action::EnterDir);
    let path = match follow {
        Some(Action::OpenEditor { path }) => path,
        other => panic!("expected OpenEditor, got {:?}", other),
    };

    d.open_editor_in_active(path)
        .expect("open_editor_in_active failed");
    assert!(d.active_editor().is_some());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn dual_pane_mark_new_files_and_snapshot() {
    let dir = tempdir("dual_new_snap");
    fs::write(dir.join("file.txt"), "data").unwrap();
    fs::write(dir.join("other.txt"), "x").unwrap();

    let mut d = DualPane::new(dir.clone(), std::env::temp_dir(), PaneSide::Left);

    let new_path = dir.join("file.txt");
    d.mark_new_files(std::slice::from_ref(&new_path));
    let PaneContent::Explorer(e) = &d.left else {
        panic!("expected Explorer in left pane");
    };
    assert!(e.new_files.contains(&new_path));

    let snapshot = d.snapshot_dir(&dir);
    assert!(snapshot.contains(&new_path));
    assert!(snapshot.contains(&dir.join("other.txt")));

    fs::remove_dir_all(&dir).ok();
}

// ── Dialog ──────────────────────────────────────────────────

#[test]
fn dialog_constructors_produce_expected_kinds() {
    assert!(matches!(
        Dialog::confirm("t", "?").kind,
        DialogKind::Confirm { .. }
    ));
    assert!(matches!(Dialog::error("e").kind, DialogKind::Error(_)));
    assert!(matches!(Dialog::info("i").kind, DialogKind::Info(_)));
    assert!(matches!(
        Dialog::conflict("Copy", "exists").kind,
        DialogKind::Conflict { .. }
    ));
}

#[test]
fn dialog_scroll_bounds() {
    let mut d = Dialog::info("a\nb\nc");
    assert_eq!(d.scroll, 0);
    d.scroll_down();
    d.scroll_down();
    assert_eq!(d.scroll, 2);
    d.scroll_up();
    d.scroll_up();
    d.scroll_up();
    assert_eq!(d.scroll, 0, "scroll cannot go below 0");
}

// ── Config ──────────────────────────────────────────────────

#[test]
fn session_config_default_is_populated() {
    let c = cpt::config::SessionConfig::default();
    assert_eq!(c.active_pane, PaneSide::Left);
    assert!(!c.left_dir.as_os_str().is_empty());
}

// ── Theme ───────────────────────────────────────────────────

#[test]
fn theme_field_count_matches_info_table() {
    assert_eq!(FIELD_COUNT, 26);
    assert_eq!(FIELD_INFO.len(), 26);
}

#[test]
fn theme_default_and_muted_have_distinct_focus_colors() {
    let d = Theme::default();
    let m = Theme::muted();
    assert_ne!(
        format!("{:?}", d.border_focused),
        format!("{:?}", m.border_focused)
    );
    assert_eq!(
        format!("{:?}", d.new_file_fg),
        format!("{:?}", Color::LightGreen)
    );
}

#[test]
fn theme_by_name_resolves_builtins_and_falls_back() {
    assert_eq!(
        format!("{:?}", Theme::by_name("default").border_focused),
        format!("{:?}", Color::Cyan)
    );
    assert_eq!(
        format!("{:?}", Theme::by_name("muted").border_focused),
        format!("{:?}", Color::White)
    );
    let unknown = Theme::by_name("nonexistent");
    assert_eq!(
        format!("{:?}", unknown.border_focused),
        format!("{:?}", Theme::default().border_focused)
    );
    assert!(Theme::is_builtin("default") && Theme::is_builtin("muted"));
    assert!(!Theme::is_builtin("my-custom"));
}

#[test]
fn theme_field_get_set_roundtrips_for_every_index() {
    let mut t = Theme::default();
    for i in 0..FIELD_COUNT {
        let original = t.get_field(i);
        t.set_field(i, Color::Magenta);
        assert_eq!(
            format!("{:?}", t.get_field(i)),
            format!("{:?}", Color::Magenta)
        );
        t.set_field(i, original);
    }
}

#[test]
fn theme_serialize_roundtrip_preserves_named_and_indexed_colors() {
    let mut theme = Theme::default();
    theme.set_field(0, indexed_to_color(42));
    theme.set_field(1, indexed_to_color(200));
    theme.set_field(2, indexed_to_color(232));

    let serialized = toml::to_string_pretty(&theme).unwrap();
    let deserialized: Theme = toml::from_str(&serialized).unwrap();
    for i in 0..FIELD_COUNT {
        assert_eq!(
            format!("{:?}", theme.get_field(i)),
            format!("{:?}", deserialized.get_field(i)),
            "field {} mismatch after roundtrip",
            i
        );
    }
}

#[test]
fn theme_color_index_roundtrip_covers_full_range() {
    for idx in 0..=255u8 {
        let color = indexed_to_color(idx);
        let back = color_to_index(color);
        assert_eq!(idx, back, "index {} did not roundtrip", idx);
    }
}

#[test]
fn theme_next_includes_builtins_and_wraps_on_unknown() {
    let names = Theme::all_theme_names();
    assert!(names.contains(&"default".to_string()));
    assert!(names.contains(&"muted".to_string()));

    let mut current = "default".to_string();
    let mut found_muted = false;
    for _ in 0..names.len() {
        current = Theme::next_theme_name(&current);
        if current == "muted" {
            found_muted = true;
            break;
        }
    }
    assert!(found_muted);
    assert_eq!(Theme::next_theme_name("nonexistent"), names[1]);
}

#[test]
fn theme_save_load_and_delete_file() {
    let dir = tempdir("theme_save");
    let path = dir.join("test-theme.toml");

    let mut theme = Theme::default();
    theme.set_field(0, Color::Red);
    fs::write(&path, toml::to_string_pretty(&theme).unwrap()).unwrap();

    let loaded: Theme = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        format!("{:?}", loaded.border_focused),
        format!("{:?}", Color::Red)
    );

    let name = "cpt-test-temp-delete-me";
    let saved = theme.save_to_file(name).expect("save_to_file failed");
    assert!(saved.exists());
    Theme::delete_file(name).expect("delete_file failed");
    assert!(!saved.exists());
    Theme::delete_file("cpt-test-nonexistent-xyz").expect("delete is idempotent");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn theme_themes_dir_resolves_under_config() {
    let dir = Theme::themes_dir().expect("themes_dir should resolve");
    assert_eq!(dir.file_name().and_then(|n| n.to_str()), Some("themes"));
}

#[test]
fn theme_color_display_name_covers_all_color_kinds() {
    assert_eq!(color_display_name(Color::Cyan), "Cyan");
    assert_eq!(color_display_name(Color::Black), "Black");
    assert_eq!(color_display_name(Color::Rgb(255, 128, 0)), "#FF8000");
    let cube = color_display_name(Color::Indexed(16));
    assert!(
        cube.contains("0:0:0"),
        "cube color name should be R:G:B, got {}",
        cube
    );
    let gray = color_display_name(Color::Indexed(232));
    assert!(
        gray.contains("gray"),
        "grayscale label expected, got {}",
        gray
    );
    for n in 0u8..16 {
        let name = color_display_name(Color::Indexed(n));
        assert!(
            name.contains(&n.to_string()),
            "Indexed({}) name should include number",
            n
        );
    }
}

#[test]
fn contrast_fg_picks_readable_color_per_index() {
    // Dark named colors and dark grayscale -> white foreground
    for idx in [0u8, 1, 4, 8, 16, 17, 232, 243] {
        assert_eq!(
            contrast_fg(idx),
            Color::White,
            "index {} should pick White",
            idx
        );
    }
    // Light named, light grayscale, bright cube corner -> black foreground
    for idx in [7u8, 11, 15, 231, 244, 255] {
        assert_eq!(
            contrast_fg(idx),
            Color::Black,
            "index {} should pick Black",
            idx
        );
    }
}

// ── TaskState ───────────────────────────────────────────────

#[test]
fn task_state_initial_values() {
    let task = TaskState::new("echo hi");
    assert_eq!(task.cmd, "echo hi");
    assert!(task.lines.is_empty());
    assert!(task.running);
    assert_eq!(task.exit_code, None);
    assert_eq!(task.scroll, 0);
    assert!(task.auto_scroll);
}

#[test]
fn task_state_push_line_auto_scroll_and_capping() {
    let mut t = TaskState::new("cmd");
    t.push_line("a".into());
    t.push_line("b".into());
    t.push_line("c".into());
    assert_eq!(t.scroll, 2, "auto_scroll tracks last line");

    t.scroll_up();
    assert!(!t.auto_scroll, "manual scroll_up disables auto_scroll");
    let scroll_after_up = t.scroll;
    t.push_line("d".into());
    assert_eq!(
        t.scroll, scroll_after_up,
        "no auto-scroll after manual interaction"
    );

    // scroll_down past the bottom edge re-enables auto_scroll
    for _ in 0..10 {
        t.scroll_down();
    }
    assert!(t.auto_scroll);

    // single-line clamp
    let mut single = TaskState::new("cmd");
    single.push_line("only".into());
    for _ in 0..5 {
        single.scroll_down();
    }
    assert_eq!(single.scroll, 0, "scroll cannot exceed lines.len() - 1");
}

#[test]
fn task_state_push_line_caps_and_adjusts_scroll() {
    let mut t = TaskState::new("cmd");
    for i in 0..MAX_LINES {
        t.push_line(format!("line {}", i));
    }
    assert_eq!(t.lines.len(), MAX_LINES);
    assert_eq!(t.lines[0], "line 0");

    t.scroll = 10;
    t.auto_scroll = false;

    t.push_line("evicted_old".into());
    assert_eq!(t.lines.len(), MAX_LINES, "len capped at MAX_LINES");
    assert_eq!(t.lines[0], "line 1", "oldest line evicted");
    assert_eq!(t.lines[MAX_LINES - 1], "evicted_old");
    assert_eq!(t.scroll, 9, "scroll decremented to follow eviction");

    // pushing many more past cap without auto_scroll keeps len constant
    for i in 0..50 {
        t.push_line(format!("extra {}", i));
    }
    assert_eq!(t.lines.len(), MAX_LINES);
}

#[test]
fn task_state_finish_and_exit_summary() {
    let running = TaskState::new("cmd");
    assert!(running.exit_summary().contains("running"));

    let mut ok = TaskState::new("cmd");
    ok.finish(0);
    assert!(!ok.running);
    assert_eq!(ok.exit_code, Some(0));
    assert_eq!(ok.exit_summary(), "done");

    let mut bad = TaskState::new("cmd");
    bad.finish(2);
    assert!(bad.exit_summary().contains('2'));
}

// ── run_task ────────────────────────────────────────────────

#[tokio::test]
async fn run_task_streams_stdout_and_signals_complete() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cwd = std::env::current_dir().unwrap();
    run_task("echo hello_world".to_string(), cwd, tx).await;

    let mut lines = Vec::new();
    let mut exit_code = None;
    for action in drain(&mut rx) {
        match action {
            Action::TaskLine(line) => lines.push(line),
            Action::TaskComplete(code) => exit_code = Some(code),
            _ => {}
        }
    }
    assert!(
        lines.iter().any(|l| l.trim().contains("hello_world")),
        "no stdout: {:?}",
        lines
    );
    assert_eq!(exit_code, Some(0));
}

#[tokio::test]
async fn run_task_streams_stderr() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cwd = std::env::current_dir().unwrap();
    #[cfg(windows)]
    let cmd = "echo stderr_test 1>&2".to_string();
    #[cfg(not(windows))]
    let cmd = "echo stderr_test >&2".to_string();
    run_task(cmd, cwd, tx).await;

    let lines: Vec<String> = drain(&mut rx)
        .into_iter()
        .filter_map(|a| {
            if let Action::TaskLine(l) = a {
                Some(l)
            } else {
                None
            }
        })
        .collect();
    assert!(
        lines.iter().any(|l| l.trim().contains("stderr_test")),
        "no stderr: {:?}",
        lines
    );
}

#[tokio::test]
async fn run_task_reports_nonzero_exit_or_error_for_bad_cwd() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cwd = std::env::current_dir().unwrap();
    run_task("exit 1".to_string(), cwd, tx).await;
    let exit = drain(&mut rx).into_iter().find_map(|a| match a {
        Action::TaskComplete(c) => Some(c),
        _ => None,
    });
    assert_eq!(exit, Some(1));

    let (tx, mut rx) = mpsc::unbounded_channel();
    run_task(
        "echo hi".to_string(),
        PathBuf::from("/nonexistent_cwd_cpt_xyz"),
        tx,
    )
    .await;
    let signaled = drain(&mut rx)
        .into_iter()
        .any(|a| matches!(a, Action::TaskError(_) | Action::TaskComplete(_)));
    assert!(signaled, "must signal something for bad cwd");
}

// ── resolve_unpack_command (pure resolution) ────────────────

#[test]
fn resolve_unpack_tar_uses_tar_for_all_tar_variants() {
    for name in [
        "archive.tar",
        "archive.tar.gz",
        "archive.tgz",
        "archive.tar.bz2",
        "archive.tbz2",
        "archive.tar.xz",
    ] {
        let archive = PathBuf::from(format!("/src/{}", name));
        let dest = PathBuf::from("/my/dest dir");
        let unpack = resolve_unpack_command(&archive, &dest)
            .unwrap_or_else(|e| panic!("{} should resolve, got: {}", name, e));
        let cmd = unpack.display();
        assert!(cmd.contains("tar"), "{} should use tar, got: {}", name, cmd);
        assert!(
            cmd.contains(name),
            "command should mention archive: {}",
            cmd
        );
        assert!(
            cmd.contains("dest dir"),
            "command should mention dest: {}",
            cmd
        );
    }
}

#[test]
fn resolve_unpack_zip_uses_unzip_or_7z() {
    let archive = PathBuf::from("/src/files.zip");
    let dest = PathBuf::from("/dest");
    match resolve_unpack_command(&archive, &dest) {
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
        Err(e) => assert!(
            e.contains("unzip") || e.contains("7z"),
            "error names tool: {}",
            e
        ),
    }
}

#[test]
fn resolve_unpack_7z_and_rar_when_tools_present() {
    let dest = PathBuf::from("/dest");

    match resolve_unpack_command(&PathBuf::from("/src/data.7z"), &dest) {
        Ok(c) => {
            let cmd = c.display();
            assert!(
                cmd.contains("7z") || cmd.contains("7za") || cmd.contains("7zz"),
                "{}",
                cmd
            );
        }
        Err(e) => assert!(e.contains("7z") || e.contains("7-Zip"), "{}", e),
    }

    match resolve_unpack_command(&PathBuf::from("/src/archive.rar"), &dest) {
        Ok(c) => {
            let cmd = c.display();
            assert!(cmd.contains("unrar") || cmd.contains("7z"), "{}", cmd);
        }
        Err(e) => assert!(e.contains("unrar") || e.contains("7z"), "{}", e),
    }
}

#[test]
fn resolve_unpack_unknown_format_returns_err() {
    let result = resolve_unpack_command(&PathBuf::from("/src/file.docx"), &PathBuf::from("/dest"));
    let err = result.expect_err("unsupported format should be Err");
    assert!(err.contains("unsupported"), "{}", err);
}

#[cfg(not(windows))]
#[test]
fn shell_quote_wraps_and_escapes_single_quotes() {
    use cpt::fs::archive::shell_quote;
    let plain = shell_quote(&PathBuf::from("/path/to/my file.txt"));
    assert!(plain.starts_with('\'') && plain.ends_with('\''));
    assert!(plain.contains("my file.txt"));

    let tricky = shell_quote(&PathBuf::from("/path/it's here/file.txt"));
    assert!(
        tricky.contains("'\\''"),
        "single quote should be escaped: {}",
        tricky
    );
}

// ── resolve_editor / resolve_pager (env-driven) ─────────────
//
// These tests mutate process-wide env vars. They are sequenced manually by
// saving and restoring the original values; they are kept compact to minimise
// the window of cross-test interference.

#[test]
fn resolve_editor_prefers_editor_then_visual_then_builtin() {
    use cpt::fs::open::resolve_editor;
    let saved_editor = std::env::var_os("EDITOR");
    let saved_visual = std::env::var_os("VISUAL");

    unsafe {
        std::env::set_var("EDITOR", "my_custom_editor");
    }
    assert_eq!(resolve_editor(), "my_custom_editor");

    unsafe {
        std::env::set_var("EDITOR", "");
        std::env::set_var("VISUAL", "my_visual");
    }
    assert_eq!(resolve_editor(), "my_visual");

    unsafe {
        std::env::remove_var("EDITOR");
        std::env::remove_var("VISUAL");
    }
    let fallback = resolve_editor();
    assert!(!fallback.is_empty());
    #[cfg(windows)]
    assert_eq!(fallback, "notepad");

    unsafe {
        match saved_editor {
            Some(v) => std::env::set_var("EDITOR", v),
            None => std::env::remove_var("EDITOR"),
        }
        match saved_visual {
            Some(v) => std::env::set_var("VISUAL", v),
            None => std::env::remove_var("VISUAL"),
        }
    }
}

#[test]
fn resolve_pager_prefers_pager_then_builtin() {
    use cpt::fs::open::resolve_pager;
    let saved = std::env::var_os("PAGER");

    unsafe {
        std::env::set_var("PAGER", "my_custom_pager");
    }
    assert_eq!(resolve_pager(), "my_custom_pager");

    unsafe {
        std::env::remove_var("PAGER");
    }
    let fallback = resolve_pager();
    assert!(!fallback.is_empty());
    #[cfg(windows)]
    assert_eq!(fallback, "more");
    #[cfg(not(windows))]
    assert_eq!(fallback, "less");

    unsafe {
        match saved {
            Some(v) => std::env::set_var("PAGER", v),
            None => std::env::remove_var("PAGER"),
        }
    }
}

// Compile-time smoke test that UnpackCommand variants are still constructable;
// without this the `Direct` variant can be silently removed by refactors.
#[test]
fn unpack_command_display_renders_both_variants() {
    let shell = UnpackCommand::Shell("tar -xf foo".to_string());
    assert_eq!(shell.display(), "tar -xf foo");
    let direct = UnpackCommand::Direct {
        program: "tar".to_string(),
        args: vec!["-xf".to_string(), "with space".to_string()],
    };
    let s = direct.display();
    assert!(s.contains("tar") && s.contains("\"with space\""), "{}", s);
}
