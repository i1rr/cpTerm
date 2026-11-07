//! End-to-end integration tests.
//!
//! These tests exercise multi-step pipelines that span more than one module:
//! Explorer state -> selected paths -> conflict resolution -> async FS ops, and
//! archive resolution -> extraction -> filesystem verification.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use cpt::action::Action;
use cpt::components::explorer::Explorer;
use cpt::fs::archive::resolve_unpack_command;
use cpt::fs::ops::{build_pairs, copy_entries, find_conflicts, move_entries, rename_conflicts};
use tokio::sync::mpsc;

// ── helpers ──────────────────────────────────────────────────

fn tempdir(prefix: &str) -> PathBuf {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .unwrap()
        .keep()
}

fn drain<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> Vec<T> {
    let mut out = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        out.push(msg);
    }
    out
}

#[tokio::test]
async fn overlapping_copies_preserve_source_data() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    fs::write(&source, b"keep me").unwrap();
    let alias = dir.path().join("alias");
    fs::hard_link(&source, &alias).unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    copy_entries(vec![(source.clone(), source.clone())], tx.clone()).await;
    assert!(matches!(rx.try_recv(), Ok(Action::OperationError(_))));
    let pairs = vec![(source.clone(), alias.clone())];
    let approved = cpt::fs::ops::copy_conflicts(&pairs);
    cpt::fs::ops::copy_entries_with_overwrites(pairs, approved, tx.clone()).await;
    assert_eq!(fs::read(&source).unwrap(), b"keep me");
    assert_eq!(fs::read(&alias).unwrap(), b"keep me");
    assert!(
        drain(&mut rx)
            .iter()
            .any(|a| matches!(a, Action::OperationComplete(_)))
    );
    let nested = dir.path().join("nested");
    copy_entries(vec![(dir.path().to_owned(), nested.clone())], tx).await;
    assert!(matches!(rx.try_recv(), Ok(Action::OperationError(_))));
    assert!(!nested.exists());
}

#[tokio::test]
async fn approving_a_directory_merge_does_not_overwrite_a_late_child() {
    let src = tempfile::tempdir().unwrap();
    let dest = tempfile::tempdir().unwrap();
    let pairs = vec![(src.path().to_owned(), dest.path().to_owned())];
    fs::write(src.path().join("late"), "from source").unwrap();
    let approved = cpt::fs::ops::copy_conflicts(&pairs);
    fs::write(dest.path().join("late"), "new arrival").unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    cpt::fs::ops::copy_entries_with_overwrites(pairs, approved, tx).await;
    assert!(matches!(rx.try_recv(), Ok(Action::OperationError(_))));
    assert_eq!(fs::read(dest.path().join("late")).unwrap(), b"new arrival");
}

fn tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

// ── 1. End-to-end conflict resolution pipeline ──────────────

/// build_pairs -> find_conflicts -> rename_conflicts -> copy_entries
/// must preserve the original target and place the new content under a
/// sibling name with the increment suffix. Repeating the pipeline a second
/// time must keep both prior files and produce a "(2)" copy.
#[tokio::test]
async fn copy_pipeline_resolves_repeated_conflicts_via_rename() {
    let src = tempdir("conflict_src");
    let dst = tempdir("conflict_dst");
    fs::write(src.join("file.txt"), "v1").unwrap();
    fs::write(dst.join("file.txt"), "original").unwrap();

    // First pass: detect the conflict and rename.
    let mut pairs = build_pairs(&[src.join("file.txt")], &dst);
    assert_eq!(find_conflicts(&pairs), vec![0]);
    rename_conflicts(&mut pairs);
    assert!(
        find_conflicts(&pairs).is_empty(),
        "rename must clear conflicts"
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    copy_entries(pairs, tx).await;
    drain(&mut rx);

    assert_eq!(
        fs::read_to_string(dst.join("file.txt")).unwrap(),
        "original"
    );
    assert_eq!(fs::read_to_string(dst.join("file (1).txt")).unwrap(), "v1");

    // Second pass: a fresh source still copies under "(2)" leaving "(1)" intact.
    fs::write(src.join("file.txt"), "v2").unwrap();
    let mut pairs = build_pairs(&[src.join("file.txt")], &dst);
    rename_conflicts(&mut pairs);
    let (tx, mut rx) = mpsc::unbounded_channel();
    copy_entries(pairs, tx).await;
    drain(&mut rx);

    assert_eq!(
        fs::read_to_string(dst.join("file.txt")).unwrap(),
        "original"
    );
    assert_eq!(fs::read_to_string(dst.join("file (1).txt")).unwrap(), "v1");
    assert_eq!(fs::read_to_string(dst.join("file (2).txt")).unwrap(), "v2");

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}

// ── 2. Move heterogeneous payload with ordered progress ─────

/// Mixing files and a populated directory in a single move call must remove
/// every source, deliver progress events with a strictly increasing `done`
/// counter, and end with one OperationComplete carrying the final count.
#[tokio::test]
async fn move_mixed_files_and_directory_emits_ordered_progress() {
    let src = tempdir("move_mixed_src");
    let dst = tempdir("move_mixed_dst");

    fs::write(src.join("a.txt"), "alpha").unwrap();
    fs::write(src.join("b.txt"), "bravo").unwrap();
    let nested = src.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("inner.txt"), "deep").unwrap();
    fs::create_dir(nested.join("sub")).unwrap();
    fs::write(nested.join("sub").join("x.txt"), "x").unwrap();

    let sources = vec![src.join("a.txt"), src.join("b.txt"), nested.clone()];
    let pairs = build_pairs(&sources, &dst);

    let (tx, mut rx) = mpsc::unbounded_channel();
    move_entries(pairs, tx).await;
    let messages = drain(&mut rx);

    let mut progress = Vec::new();
    let mut completes = Vec::new();
    let mut errors = Vec::new();
    for msg in messages {
        match msg {
            Action::OperationProgress { done, total } => progress.push((done, total)),
            Action::OperationComplete(s) => completes.push(s),
            Action::OperationError(e) => errors.push(e),
            _ => {}
        }
    }

    assert!(errors.is_empty(), "no errors expected, got {:?}", errors);
    assert_eq!(
        progress,
        vec![(1, 3), (2, 3), (3, 3)],
        "progress must be ordered"
    );
    assert_eq!(completes.len(), 1, "exactly one completion event");
    assert!(
        completes[0].contains('3'),
        "complete msg names count: {}",
        completes[0]
    );

    for src_path in [src.join("a.txt"), src.join("b.txt"), nested] {
        assert!(
            !src_path.exists(),
            "{} must be moved away",
            src_path.display()
        );
    }
    assert_eq!(fs::read_to_string(dst.join("a.txt")).unwrap(), "alpha");
    assert_eq!(fs::read_to_string(dst.join("b.txt")).unwrap(), "bravo");
    assert_eq!(
        fs::read_to_string(dst.join("nested").join("inner.txt")).unwrap(),
        "deep"
    );
    assert_eq!(
        fs::read_to_string(dst.join("nested").join("sub").join("x.txt")).unwrap(),
        "x"
    );

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}

// ── 3. Explorer selection driving FS ops ────────────────────

/// A user typically multi-selects in the explorer and then triggers a copy.
/// This integration check wires Explorer.selected_paths() straight into the
/// copy pipeline, ensuring that what the UI exposes matches what FS ops see.
#[tokio::test]
async fn explorer_selection_feeds_copy_pipeline_end_to_end() {
    let src = tempdir("sel_src");
    let dst = tempdir("sel_dst");
    fs::write(src.join("keep_me.txt"), "keep").unwrap();
    fs::write(src.join("also.txt"), "also").unwrap();
    fs::write(src.join("ignore_me.bin"), "ignored").unwrap();

    let mut explorer = Explorer::new(src.clone());
    // Sorted alphabetically: also.txt, ignore_me.bin, keep_me.txt
    // Move down to "also.txt" and select; ToggleSelect advances to next.
    explorer.handle_action(&Action::MoveDown);
    explorer.handle_action(&Action::ToggleSelect); // selects also.txt
    // cursor is now on ignore_me.bin: skip it.
    explorer.handle_action(&Action::MoveDown);
    explorer.handle_action(&Action::ToggleSelect); // selects keep_me.txt

    let selected = explorer.selected_paths();
    assert_eq!(selected.len(), 2, "selected_paths reports two entries");

    let (tx, mut rx) = mpsc::unbounded_channel();
    copy_entries(build_pairs(&selected, &dst), tx).await;
    drain(&mut rx);

    assert!(dst.join("also.txt").exists());
    assert!(dst.join("keep_me.txt").exists());
    assert!(
        !dst.join("ignore_me.bin").exists(),
        "unselected file must not be copied"
    );

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}

// ── 4. Real tar archive round-trip ──────────────────────────

/// Read an archive produced by the system tar and check its extracted tree.
#[tokio::test]
async fn unpack_real_tar_archive_extracts_full_tree() {
    if !tool_available("tar") {
        eprintln!("skipping: tar not available on PATH");
        return;
    }

    let work = tempdir("tar_work");
    let payload = work.join("payload");
    fs::create_dir(&payload).unwrap();
    fs::write(payload.join("hello.txt"), "hello tar").unwrap();
    fs::create_dir(payload.join("sub")).unwrap();
    fs::write(payload.join("sub").join("nested.txt"), "nested data").unwrap();

    let archive = work.join("bundle.tar");
    let status = Command::new("tar")
        .arg("-cf")
        .arg(&archive)
        .arg("-C")
        .arg(&work)
        .arg("payload")
        .status()
        .expect("tar create failed");
    assert!(status.success(), "tar create must succeed");

    let dest = work.join("out");
    fs::create_dir(&dest).unwrap();

    let cmd = resolve_unpack_command(&archive, &dest).expect("tar resolve must succeed");
    cmd.run().await.expect("archive extraction failed");

    assert_eq!(
        fs::read_to_string(dest.join("payload").join("hello.txt")).unwrap(),
        "hello tar"
    );
    assert_eq!(
        fs::read_to_string(dest.join("payload").join("sub").join("nested.txt")).unwrap(),
        "nested data"
    );

    fs::remove_dir_all(&work).ok();
}

// ── 5. Real zip archive round-trip ──────────────────────────

/// Read a ZIP produced by a system tool, including a quoted filename.
#[tokio::test]
async fn unpack_real_zip_archive_extracts_files() {
    if !cfg!(windows) && !tool_available("zip") {
        eprintln!("skipping: zip not available on PATH");
        return;
    }
    let work = tempdir("zip_work");
    let payload_dir = work.join("payload");
    fs::create_dir(&payload_dir).unwrap();
    fs::write(payload_dir.join("doc.txt"), "zipped doc").unwrap();
    fs::write(payload_dir.join("notes.md"), "# notes").unwrap();

    let archive = work.join("a ‘quote’ & spaces.zip");
    let mut pack = if cfg!(windows) {
        let mut cmd = Command::new("tar");
        cmd.arg("-acf");
        cmd
    } else {
        let mut cmd = Command::new("zip");
        cmd.arg("-r");
        cmd
    };
    let status = pack
        .arg(&archive)
        .arg("payload")
        .current_dir(&work)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("zip create failed");
    assert!(status.success(), "zip create must succeed");

    let dest = work.join("out");
    fs::create_dir(&dest).unwrap();

    let cmd = resolve_unpack_command(&archive, &dest).expect("zip resolve must succeed");
    cmd.run().await.expect("archive extraction failed");

    assert_eq!(
        fs::read_to_string(dest.join("payload").join("doc.txt")).unwrap(),
        "zipped doc"
    );
    assert_eq!(
        fs::read_to_string(dest.join("payload").join("notes.md")).unwrap(),
        "# notes"
    );

    fs::remove_dir_all(&work).ok();
}

// ── 6. Deep recursive copy preserves the full tree ──────────

/// copy_dir_recursive (the private helper inside fs/ops) is reached via
/// copy_entries when a directory is passed. A multi-level tree with mixed
/// files exercises every recursion branch and verifies that no intermediate
/// directory is dropped.
#[tokio::test]
async fn copy_recursive_preserves_full_directory_tree() {
    let src = tempdir("deep_src");
    let dst = tempdir("deep_dst");

    let root = src.join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("README.md"), "# project").unwrap();
    let sub_a = root.join("a");
    fs::create_dir(&sub_a).unwrap();
    fs::write(sub_a.join("a.txt"), "a content").unwrap();
    let sub_b = root.join("a").join("b");
    fs::create_dir(&sub_b).unwrap();
    fs::write(sub_b.join("b.txt"), "b content").unwrap();
    let empty = root.join("empty_subdir");
    fs::create_dir(&empty).unwrap();

    let (tx, mut rx) = mpsc::unbounded_channel();
    copy_entries(build_pairs(std::slice::from_ref(&root), &dst), tx).await;
    drain(&mut rx);

    fn assert_files_match(src: &Path, dst: &Path) {
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let target = dst.join(entry.file_name());
            assert!(target.exists(), "missing in dst: {}", target.display());
            if entry.file_type().unwrap().is_dir() {
                assert!(target.is_dir());
                assert_files_match(&entry.path(), &target);
            } else {
                assert_eq!(
                    fs::read(entry.path()).unwrap(),
                    fs::read(&target).unwrap(),
                    "content mismatch at {}",
                    target.display()
                );
            }
        }
    }
    assert_files_match(&root, &dst.join("project"));

    fs::remove_dir_all(&src).ok();
    fs::remove_dir_all(&dst).ok();
}
