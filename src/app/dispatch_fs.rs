use std::path::{Path, PathBuf};

use crate::action::Action;
use crate::components::dialog::Dialog;
use crate::fs::ops;
use crate::task;

use super::{App, InputMode, PendingOp};

impl App {
    pub(super) fn execute_op(&mut self, op: PendingOp) {
        let tx = self.action_tx.clone();
        match op {
            PendingOp::Copy { ref pairs } => {
                self.pending_new_files = pairs.iter().map(|(_, t)| t.clone()).collect();
                tokio::spawn(ops::copy_entries(pairs.clone(), tx));
            }
            PendingOp::Move { ref pairs } => {
                self.pending_new_files = pairs.iter().map(|(_, t)| t.clone()).collect();
                tokio::spawn(ops::move_entries(pairs.clone(), tx));
            }
            PendingOp::Delete(sources) => {
                tokio::spawn(ops::delete_entries(sources, tx));
            }
            // CloseEditor is handled in ConfirmDialog dispatch directly; not routed here.
            PendingOp::CloseEditor => {}
            PendingOp::RemoteDelete { entries, session } => {
                tokio::spawn(async move {
                    let sess = session.lock().await;
                    let mut errors = Vec::new();
                    let total = entries.len();
                    for (path, is_dir) in &entries {
                        let r = if *is_dir {
                            sess.remove_dir_recursive(path).await
                        } else {
                            sess.remove_file(path).await
                        };
                        if let Err(e) = r {
                            errors.push(e);
                        }
                    }
                    if errors.is_empty() {
                        let _ = tx.send(Action::RemoteOpComplete(format!(
                            "Deleted {} item(s)",
                            total
                        )));
                    } else {
                        let _ = tx.send(Action::RemoteOpError(errors.join("; ")));
                    }
                });
            }
            PendingOp::RemoteDownload {
                paths,
                session,
                dest,
            } => {
                tokio::spawn(async move {
                    let sess = session.lock().await;
                    let mut errors = Vec::new();
                    let total = paths.len();
                    for remote_path in &paths {
                        let filename = remote_path.rsplit('/').next().unwrap_or("file");
                        let local = dest.join(filename);
                        if let Err(e) = sess.download(remote_path, &local).await {
                            errors.push(e);
                        }
                    }
                    if errors.is_empty() {
                        let _ = tx.send(Action::OperationComplete(format!(
                            "Downloaded {} item(s)",
                            total
                        )));
                    } else {
                        let _ = tx.send(Action::RemoteOpError(errors.join("; ")));
                    }
                });
            }
            PendingOp::RemoteUpload {
                paths,
                session,
                remote_dest,
            } => {
                tokio::spawn(async move {
                    let sess = session.lock().await;
                    let mut errors = Vec::new();
                    let total = paths.len();
                    for local_path in &paths {
                        let filename = local_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file");
                        let rpath = format!("{}/{}", remote_dest.trim_end_matches('/'), filename);
                        if let Err(e) = sess.upload(local_path, &rpath).await {
                            errors.push(e);
                        }
                    }
                    if errors.is_empty() {
                        let _ = tx.send(Action::RemoteOpComplete(format!(
                            "Uploaded {} item(s)",
                            total
                        )));
                    } else {
                        let _ = tx.send(Action::RemoteOpError(errors.join("; ")));
                    }
                });
            }
        }
    }

    pub(super) fn get_operation_sources(&self) -> Vec<PathBuf> {
        let Some(explorer) = self.dual_pane.active_explorer() else {
            return vec![];
        };
        let mut sources = explorer.selected_paths();
        if sources.is_empty()
            && let Some(entry) = explorer.current_entry()
        {
            sources.push(entry.path.clone());
        }
        sources
    }

    pub(super) fn do_rename(&mut self, new_name: &str) {
        if new_name.is_empty() {
            return;
        }
        // Handle remote rename
        if let Some(remote) = self.dual_pane.active_remote()
            && let Some(entry) = remote.current_entry()
        {
            let old_remote_path = entry.remote_path.clone();
            let parent_path = remote.current_path.clone();
            let session = remote.session.clone();
            let new_name_owned = new_name.to_string();
            let new_remote_path = if parent_path.ends_with('/') {
                format!("{}{}", parent_path, new_name_owned)
            } else {
                format!("{}/{}", parent_path, new_name_owned)
            };
            let tx = self.action_tx.clone();
            tokio::spawn(async move {
                let sess = session.lock().await;
                match sess.rename(&old_remote_path, &new_remote_path).await {
                    Ok(()) => {
                        let _ = tx.send(Action::RemoteOpComplete(format!(
                            "Renamed to {}",
                            new_name_owned
                        )));
                    }
                    Err(e) => {
                        let _ = tx.send(Action::RemoteOpError(format!("Rename failed: {}", e)));
                    }
                }
            });
            return;
        }

        // Local rename
        let old_path = self
            .dual_pane
            .active_explorer()
            .and_then(|e| e.current_entry())
            .map(|entry| entry.path.clone());
        if let Some(old_path) = old_path {
            let Some(parent) = old_path.parent() else {
                return;
            };
            let new_path = parent.join(new_name);
            if let Err(e) = std::fs::rename(&old_path, &new_path) {
                self.dialog = Some(Dialog::error(format!("Rename failed: {}", e)));
            }
            if let Some(e) = self.dual_pane.active_explorer_mut() {
                e.refresh();
            }
        }
    }

    pub(super) fn do_mkdir(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        // Handle remote mkdir
        if let Some(remote) = self.dual_pane.active_remote() {
            let current_path = remote.current_path.clone();
            let session = remote.session.clone();
            let name_owned = name.to_string();
            let new_path = if current_path.ends_with('/') {
                format!("{}{}", current_path, name_owned)
            } else {
                format!("{}/{}", current_path, name_owned)
            };
            let tx = self.action_tx.clone();
            tokio::spawn(async move {
                let sess = session.lock().await;
                match sess.mkdir(&new_path).await {
                    Ok(()) => {
                        let _ = tx.send(Action::RemoteOpComplete(format!(
                            "Created directory {}",
                            name_owned
                        )));
                    }
                    Err(e) => {
                        let _ = tx.send(Action::RemoteOpError(format!("Mkdir failed: {}", e)));
                    }
                }
            });
            return;
        }

        // Local mkdir
        let dir = self.dual_pane.active_dir();
        let new_dir = dir.join(name);
        if let Err(e) = std::fs::create_dir(&new_dir) {
            self.dialog = Some(Dialog::error(format!("Mkdir failed: {}", e)));
        }
        if let Some(e) = self.dual_pane.active_explorer_mut() {
            e.refresh();
        }
    }

    pub(super) fn do_create_file(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        let dir = self.dual_pane.active_dir();
        let new_file = dir.join(name);
        if new_file.exists() {
            self.dialog = Some(Dialog::error(format!("Already exists: {}", name)));
        } else if let Err(e) = std::fs::File::create(&new_file) {
            self.dialog = Some(Dialog::error(format!("Create file failed: {}", e)));
        }
        if let Some(e) = self.dual_pane.active_explorer_mut() {
            e.refresh();
        }
    }

    pub(super) fn do_unpack(&mut self, archive: &Path, dest: &Path) {
        // Snapshot the destination directory before extraction for new-file highlighting
        let snapshot = self.dual_pane.snapshot_dir(dest);
        self.pre_extract_snapshot = Some((dest.to_path_buf(), snapshot));

        let cwd = self.dual_pane.active_dir();
        log::debug!(
            "unpack: archive={}, dest={}, cwd={}, active_pane={:?}",
            archive.display(),
            dest.display(),
            cwd.display(),
            self.dual_pane.active
        );
        match crate::fs::archive::resolve_unpack_command(archive, dest) {
            Ok(unpack_cmd) => {
                log::debug!("unpack: resolved command: {:?}", unpack_cmd);
                let tx = self.action_tx.clone();
                let label = unpack_cmd.display();
                self.task = Some(crate::task::TaskState::new(&label));
                self.input_mode = InputMode::TaskOutput;
                match unpack_cmd {
                    crate::fs::archive::UnpackCommand::Shell(cmd) => {
                        log::debug!("unpack: spawning shell task in cwd={}", cwd.display());
                        tokio::spawn(task::run_task(cmd, cwd, tx));
                    }
                    crate::fs::archive::UnpackCommand::Direct { program, args } => {
                        log::debug!(
                            "unpack: spawning direct task: {} {:?} in cwd={}",
                            program,
                            args,
                            cwd.display()
                        );
                        tokio::spawn(task::run_task_direct(program, args, cwd, tx));
                    }
                }
            }
            Err(msg) => {
                log::debug!("unpack: resolve error: {}", msg);
                self.input_mode = InputMode::Normal;
                self.dialog = Some(Dialog::error(msg));
            }
        }
    }

    /// Handle FS-related actions. Returns true if the action was consumed.
    pub(super) fn dispatch_fs(&mut self, action: Action) -> bool {
        match action {
            Action::CopySelected => {
                let active_is_remote = self.dual_pane.active_remote().is_some();
                let inactive_is_remote = self.dual_pane.inactive_remote().is_some();

                if active_is_remote && inactive_is_remote {
                    self.dialog = Some(Dialog::error("Cannot copy between two remote panes"));
                } else if active_is_remote && !inactive_is_remote {
                    // Download from remote to local
                    if let Some(remote) = self.dual_pane.active_remote() {
                        let selected = remote.selected_remote_entries();
                        if selected.is_empty() {
                            return true;
                        }
                        let paths: Vec<String> = selected
                            .iter()
                            .filter(|e| !e.is_dir)
                            .map(|e| e.remote_path.clone())
                            .collect();
                        if paths.is_empty() {
                            self.dialog = Some(Dialog::error(
                                "Cannot download directories (not yet supported)",
                            ));
                            return true;
                        }
                        let dest = self.dual_pane.inactive_dir();
                        let session = remote.session.clone();
                        let count = paths.len();
                        self.pending_op = Some(PendingOp::RemoteDownload {
                            paths,
                            session,
                            dest: dest.clone(),
                        });
                        self.dialog = Some(Dialog::confirm(
                            "Download",
                            format!("Download {} file(s) to {}?", count, dest.display()),
                        ));
                    }
                } else if !active_is_remote && inactive_is_remote {
                    // Upload from local to remote
                    let sources = self.get_operation_sources();
                    if sources.is_empty() {
                        return true;
                    }
                    if let Some(remote) = self.dual_pane.inactive_remote() {
                        let remote_dest = remote.current_path.clone();
                        let session = remote.session.clone();
                        let count = sources.len();
                        self.pending_op = Some(PendingOp::RemoteUpload {
                            paths: sources,
                            session,
                            remote_dest: remote_dest.clone(),
                        });
                        self.dialog = Some(Dialog::confirm(
                            "Upload",
                            format!("Upload {} item(s) to {}?", count, remote_dest),
                        ));
                    }
                } else {
                    // Both local
                    let sources = self.get_operation_sources();
                    if sources.is_empty() {
                        return true;
                    }
                    let dest = self.dual_pane.inactive_dir();
                    let pairs = ops::build_pairs(&sources, &dest);
                    let conflicts = ops::find_conflicts(&pairs);
                    let count = pairs.len();
                    self.pending_op = Some(PendingOp::Copy { pairs });
                    if conflicts.is_empty() {
                        self.dialog = Some(Dialog::confirm(
                            "Copy",
                            format!("Copy {} item(s) to {}?", count, dest.display()),
                        ));
                    } else {
                        self.dialog = Some(Dialog::conflict(
                            "Copy",
                            format!(
                                "{} of {} item(s) already exist in {}\n\n\
                                 (O)verwrite  (R)ename  Esc: Cancel",
                                conflicts.len(),
                                count,
                                dest.display()
                            ),
                        ));
                    }
                }
                true
            }
            Action::MoveSelected => {
                let active_is_remote = self.dual_pane.active_remote().is_some();
                let inactive_is_remote = self.dual_pane.inactive_remote().is_some();

                if active_is_remote || inactive_is_remote {
                    // For remote panes, treat Move as Copy (moving across hosts is complex)
                    self.dispatch_fs(Action::CopySelected);
                } else {
                    let sources = self.get_operation_sources();
                    if sources.is_empty() {
                        return true;
                    }
                    let dest = self.dual_pane.inactive_dir();
                    let pairs = ops::build_pairs(&sources, &dest);
                    let conflicts = ops::find_conflicts(&pairs);
                    let count = pairs.len();
                    self.pending_op = Some(PendingOp::Move { pairs });
                    if conflicts.is_empty() {
                        self.dialog = Some(Dialog::confirm(
                            "Move",
                            format!("Move {} item(s) to {}?", count, dest.display()),
                        ));
                    } else {
                        self.dialog = Some(Dialog::conflict(
                            "Move",
                            format!(
                                "{} of {} item(s) already exist in {}\n\n\
                                 (O)verwrite  (R)ename  Esc: Cancel",
                                conflicts.len(),
                                count,
                                dest.display()
                            ),
                        ));
                    }
                }
                true
            }
            Action::DeleteSelected => {
                if let Some(remote) = self.dual_pane.active_remote() {
                    let selected = remote.selected_remote_entries();
                    if selected.is_empty() {
                        return true;
                    }
                    let entries: Vec<(String, bool)> = selected
                        .iter()
                        .map(|e| (e.remote_path.clone(), e.is_dir))
                        .collect();
                    let count = entries.len();
                    let session = remote.session.clone();
                    self.pending_op = Some(PendingOp::RemoteDelete { entries, session });
                    self.dialog = Some(Dialog::confirm(
                        "Remote Delete",
                        format!("Delete {} remote item(s)?", count),
                    ));
                } else {
                    let sources = self.get_operation_sources();
                    if sources.is_empty() {
                        return true;
                    }
                    let count = sources.len();
                    self.pending_op = Some(PendingOp::Delete(sources));
                    self.dialog = Some(Dialog::confirm(
                        "Delete",
                        format!("Delete {} item(s)?", count),
                    ));
                }
                true
            }
            Action::ConflictOverwrite => {
                if let Some(op) = self.pending_op.take() {
                    self.execute_op(op);
                    self.dialog = None;
                }
                true
            }
            Action::ConflictRename => {
                if let Some(op) = self.pending_op.take() {
                    let op = match op {
                        PendingOp::Copy { mut pairs } => {
                            ops::rename_conflicts(&mut pairs);
                            PendingOp::Copy { pairs }
                        }
                        PendingOp::Move { mut pairs } => {
                            ops::rename_conflicts(&mut pairs);
                            PendingOp::Move { pairs }
                        }
                        other => other,
                    };
                    self.execute_op(op);
                    self.dialog = None;
                }
                true
            }
            Action::UnpackArchive => {
                // If we're in UnpackChoice mode, extract to archive's parent dir.
                // Otherwise show the choice dialog.
                if let InputMode::UnpackChoice { ref archive } = self.input_mode {
                    let archive = archive.clone();
                    let dest = archive
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| self.dual_pane.active_dir());
                    self.do_unpack(&archive, &dest);
                } else if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                    && !entry.is_dir
                {
                    self.input_mode = InputMode::UnpackChoice {
                        archive: entry.path.clone(),
                    };
                }
                true
            }
            Action::UnpackArchiveTo { dest } => {
                if let InputMode::UnpackChoice { ref archive } = self.input_mode {
                    let archive = archive.clone();
                    if dest.as_os_str() == "\x00" {
                        // Switch to custom path input
                        let default_path = archive
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        self.input_mode = InputMode::UnpackCustomPath {
                            archive,
                            text: default_path,
                        };
                    } else {
                        // Create folder mode: use archive stem as subfolder name
                        let parent = archive
                            .parent()
                            .map(|p| p.to_path_buf())
                            .unwrap_or_else(|| self.dual_pane.active_dir());
                        let stem = archive
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "extracted".to_string());
                        // Strip double extensions like .tar.gz
                        let stem = stem.strip_suffix(".tar").unwrap_or(&stem).to_string();
                        let folder_dest = parent.join(&stem);
                        self.do_unpack(&archive, &folder_dest);
                    }
                }
                true
            }
            Action::Rename => {
                if let Some(entry) = self
                    .dual_pane
                    .active_remote()
                    .and_then(|r| r.current_entry())
                {
                    self.input_mode = InputMode::Rename(entry.name.clone());
                } else if let Some(entry) = self
                    .dual_pane
                    .active_explorer()
                    .and_then(|e| e.current_entry())
                {
                    self.input_mode = InputMode::Rename(entry.name.clone());
                }
                true
            }
            Action::CreateNew => {
                self.input_mode = InputMode::CreateTypeChoice;
                true
            }
            Action::MkDir => {
                self.input_mode = InputMode::MkDir(String::new());
                true
            }
            Action::CreateFile => {
                if self.dual_pane.active_remote().is_some() {
                    self.dialog = Some(Dialog::error(
                        "Cannot create files in remote pane (not yet supported)",
                    ));
                } else {
                    self.input_mode = InputMode::CreateFile(String::new());
                }
                true
            }
            Action::OperationComplete(msg) => {
                let new_files = std::mem::take(&mut self.pending_new_files);
                self.dual_pane.refresh_both();
                if !new_files.is_empty() {
                    self.dual_pane.mark_new_files(&new_files);
                }
                self.dialog = Some(Dialog::info(msg));
                true
            }
            Action::OperationError(msg) => {
                self.dual_pane.refresh_both();
                self.dialog = Some(Dialog::error(msg));
                true
            }
            Action::OperationProgress { .. } => true,
            Action::RemoteDownload => {
                if let Some(op) = self.pending_op.take() {
                    self.execute_op(op);
                }
                self.dialog = None;
                true
            }
            Action::RemoteUpload => {
                if let Some(op) = self.pending_op.take() {
                    self.execute_op(op);
                }
                self.dialog = None;
                true
            }
            _ => false,
        }
    }
}
