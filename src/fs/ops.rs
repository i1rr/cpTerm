use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::action::Action;

/// Copy a list of files/dirs to a destination directory.
/// Sends progress and completion/error actions through the channel.
pub async fn copy_entries(
    sources: Vec<PathBuf>,
    dest_dir: PathBuf,
    tx: mpsc::UnboundedSender<Action>,
) {
    let total = sources.len() as u64;
    let mut done = 0u64;

    for src in &sources {
        let target = dest_dir.join(src.file_name().unwrap_or_default());
        let result = if src.is_dir() {
            copy_dir_recursive(src, &target)
        } else {
            std::fs::copy(src, &target).map(|_| ())
        };

        match result {
            Ok(()) => {
                done += 1;
                let _ = tx.send(Action::OperationProgress { done, total });
            }
            Err(e) => {
                let _ = tx.send(Action::OperationError(format!(
                    "Failed to copy {}: {}",
                    src.display(),
                    e
                )));
                return;
            }
        }
    }

    let _ = tx.send(Action::OperationComplete(format!(
        "Copied {} item(s)",
        done
    )));
}

/// Move a list of files/dirs to a destination directory.
pub async fn move_entries(
    sources: Vec<PathBuf>,
    dest_dir: PathBuf,
    tx: mpsc::UnboundedSender<Action>,
) {
    let total = sources.len() as u64;
    let mut done = 0u64;

    for src in &sources {
        let target = dest_dir.join(src.file_name().unwrap_or_default());
        let result = std::fs::rename(src, &target).or_else(|_| {
            // rename fails across drives, fall back to copy+delete
            if src.is_dir() {
                copy_dir_recursive(src, &target)?;
                std::fs::remove_dir_all(src)
            } else {
                std::fs::copy(src, &target)?;
                std::fs::remove_file(src)
            }
        });

        match result {
            Ok(()) => {
                done += 1;
                let _ = tx.send(Action::OperationProgress { done, total });
            }
            Err(e) => {
                let _ = tx.send(Action::OperationError(format!(
                    "Failed to move {}: {}",
                    src.display(),
                    e
                )));
                return;
            }
        }
    }

    let _ = tx.send(Action::OperationComplete(format!(
        "Moved {} item(s)",
        done
    )));
}

/// Delete a list of files/dirs.
pub async fn delete_entries(sources: Vec<PathBuf>, tx: mpsc::UnboundedSender<Action>) {
    let total = sources.len() as u64;
    let mut done = 0u64;

    for src in &sources {
        let result = if src.is_dir() {
            std::fs::remove_dir_all(src)
        } else {
            std::fs::remove_file(src)
        };

        match result {
            Ok(()) => {
                done += 1;
                let _ = tx.send(Action::OperationProgress { done, total });
            }
            Err(e) => {
                let _ = tx.send(Action::OperationError(format!(
                    "Failed to delete {}: {}",
                    src.display(),
                    e
                )));
                return;
            }
        }
    }

    let _ = tx.send(Action::OperationComplete(format!(
        "Deleted {} item(s)",
        done
    )));
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
