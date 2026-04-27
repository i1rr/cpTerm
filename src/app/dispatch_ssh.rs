use std::path::PathBuf;
use std::sync::Arc;

use crate::action::Action;
use crate::app::BackgroundedSession;
use crate::components::dialog::Dialog;
use crate::components::dual_pane::PaneContent;
use crate::components::explorer::Explorer;
use crate::components::remote_explorer::RemoteExplorer;
use crate::config::{LastSshConfig, PaneSide, SessionConfig};
use crate::task;

use super::{App, InputMode, SshConnectState};

/// Parse `ssh [user@]host[:/path]` command strings.
/// Returns `(user, host, path)` on success.
pub fn parse_ssh_command(cmd: &str) -> Option<(String, String, String)> {
    let rest = cmd.strip_prefix("ssh ")?.trim();
    if rest.is_empty() {
        return None;
    }

    let default_user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "root".to_string());

    // Split on ':' first to separate host-part from path
    let (host_user_part, path) = if let Some(colon_pos) = rest.find(':') {
        let (left, right) = rest.split_at(colon_pos);
        let path = right[1..].to_string(); // skip the ':'
        let path = if path.is_empty() {
            "/".to_string()
        } else {
            path
        };
        (left.trim(), path)
    } else {
        (rest, "/".to_string())
    };

    // Split host_user_part on '@'
    let (user, host) = if let Some(at_pos) = host_user_part.find('@') {
        let u = host_user_part[..at_pos].to_string();
        let h = host_user_part[at_pos + 1..].to_string();
        (u, h)
    } else {
        (default_user, host_user_part.to_string())
    };

    if host.is_empty() {
        return None;
    }

    log::debug!(
        "parse_ssh_command: user={:?} host={:?} path={:?}",
        user,
        host,
        path
    );
    Some((user, host, path))
}

pub(super) fn dirs_next_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

impl App {
    pub(super) fn do_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        log::debug!("do_command: {:?}", cmd);
        if cmd.is_empty() {
            self.input_mode = InputMode::Normal;
            return;
        }

        // Disconnect and remove all backgrounded SSH sessions
        if cmd == "disconnect" {
            if self.dual_pane.active_remote().is_some() {
                self.dispatch_ssh(Action::SshDisconnect);
            } else {
                self.ssh_sessions.clear();
            }
            self.input_mode = InputMode::Normal;
            return;
        }

        // Parse "ssh [user@]host[:/path]" commands
        if let Some((user, host, path)) = parse_ssh_command(cmd) {
            self.input_mode = InputMode::Normal;
            log::debug!("ssh: connecting to {}@{}{}", user, host, path);
            self.connecting_to = Some(format!("{}@{}", user, host));
            let tx = self.action_tx.clone();
            tokio::spawn(async move {
                match crate::ssh::session::SshSession::connect(&host, 22, &user, None).await {
                    Ok(session) => {
                        let session = std::sync::Arc::new(tokio::sync::Mutex::new(session));
                        let entries = session.lock().await.list_dir(&path).await;
                        match entries {
                            Ok(entries) => {
                                let _ = tx.send(Action::SshConnected {
                                    session,
                                    host,
                                    port: 22,
                                    user,
                                    initial_path: path,
                                    entries,
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(Action::SshConnectionFailed(e));
                            }
                        }
                    }
                    Err(e) => {
                        if e.contains("no valid key found and no password provided") {
                            let _ = tx.send(Action::SshPasswordRequired {
                                host,
                                port: 22,
                                user,
                                path,
                            });
                        } else {
                            let _ = tx.send(Action::SshConnectionFailed(e));
                        }
                    }
                }
            });
            return;
        }

        // Parse "cd <path>" commands
        if let Some(path_str) = cmd
            .strip_prefix("cd ")
            .or_else(|| if cmd == "cd" { Some("~") } else { None })
        {
            let path_str = path_str.trim();
            let path_str = if path_str == "~" {
                std::env::var("USERPROFILE")
                    .or_else(|_| std::env::var("HOME"))
                    .unwrap_or_else(|_| ".".to_string())
            } else if path_str.len() == 2 && path_str.ends_with(':') {
                format!("{}\\", path_str)
            } else {
                path_str.to_string()
            };

            let target = if PathBuf::from(&path_str).is_absolute() {
                PathBuf::from(&path_str)
            } else {
                self.dual_pane.active_dir().join(&path_str)
            };

            match crate::util::clean_canonicalize(&target) {
                Ok(resolved) if resolved.is_dir() => {
                    if let Some(explorer) = self.dual_pane.active_explorer_mut() {
                        explorer.current_dir = resolved;
                        explorer.filter_text = None;
                        explorer.cursor = 0;
                        explorer.refresh();
                    }
                }
                Ok(_) => {
                    self.dialog = Some(Dialog::error(format!(
                        "Not a directory: {}",
                        target.display()
                    )));
                }
                Err(e) => {
                    self.dialog = Some(Dialog::error(format!("cd: {}", e)));
                }
            }
            self.input_mode = InputMode::Normal;
            return;
        }

        // Run as async shell command in the task window
        if self.task.as_ref().map(|t| t.running).unwrap_or(false) {
            // Already have a running task - show it instead of starting another
            self.input_mode = InputMode::TaskOutput;
            return;
        }

        let cwd = self.dual_pane.active_dir();
        self.task = Some(crate::task::TaskState::new(cmd));
        self.input_mode = InputMode::TaskOutput;
        let tx = self.action_tx.clone();
        tokio::spawn(task::run_task(cmd.to_string(), cwd, tx));
    }

    /// Handle SSH and remote actions. Returns true if the action was consumed.
    pub(super) fn dispatch_ssh(&mut self, action: Action) -> bool {
        match action {
            Action::SshConnect => {
                if self.dual_pane.active_remote().is_some() {
                    // Remote pane is active - background the session, return to local
                    self.dispatch_ssh(Action::SshBackground);
                } else {
                    // Build pre-filled state from last saved config
                    let mut state = SshConnectState::new();
                    let last_ssh = SessionConfig::load().last_ssh;
                    if let Some(ref last) = last_ssh
                        && !last.host.is_empty()
                    {
                        state.fields[0] = last.host.clone();
                        state.fields[1] = last.port.to_string();
                        state.fields[2] = last.user.clone();
                        state.fields[3] = last.path.clone();
                    }
                    // Check if a backgrounded session exists for this host
                    if let Some(ref last) = last_ssh
                        && !last.host.is_empty()
                    {
                        let key = format!("{}@{}", last.user, last.host);
                        if let Some(entry) = self.ssh_sessions.get(&key) {
                            let session = entry.session.clone();
                            let path = entry.last_path.clone();
                            let host = last.host.clone();
                            let user = last.user.clone();
                            let port = last.port;
                            let tx = self.action_tx.clone();
                            let key_clone = key.clone();
                            tokio::spawn(async move {
                                let result = {
                                    let sess = session.lock().await;
                                    sess.list_dir(&path).await
                                };
                                match result {
                                    Ok(entries) => {
                                        let _ = tx.send(Action::SshConnected {
                                            session,
                                            host,
                                            port,
                                            user,
                                            initial_path: path,
                                            entries,
                                        });
                                    }
                                    Err(_) => {
                                        let _ =
                                            tx.send(Action::SshSessionExpired { key: key_clone });
                                    }
                                }
                            });
                            self.connecting_to = Some(format!("{}@{}", last.user, last.host));
                            return true;
                        }
                    }
                    self.input_mode = InputMode::SshConnect(state);
                }
                true
            }
            Action::SshDisconnect => {
                // Remove any backgrounded session for this pane's host
                if let Some(remote) = self.dual_pane.active_remote() {
                    let key = remote.host_label.trim_end_matches(':').to_string();
                    self.ssh_sessions.remove(&key);
                }
                let fallback = dirs_next_home().unwrap_or_else(|| PathBuf::from("/"));
                match self.dual_pane.active {
                    PaneSide::Left => {
                        self.dual_pane.left = PaneContent::Explorer(Explorer::new(fallback))
                    }
                    PaneSide::Right => {
                        self.dual_pane.right = PaneContent::Explorer(Explorer::new(fallback))
                    }
                }
                true
            }
            Action::SshBackground => {
                // Store session in background map, then swap pane back to local
                if let Some(remote) = self.dual_pane.active_remote() {
                    let key = remote.host_label.trim_end_matches(':').to_string();
                    let entry = BackgroundedSession {
                        session: remote.session.clone(),
                        last_path: remote.current_path.clone(),
                        host_label: remote.host_label.clone(),
                        port: SessionConfig::load()
                            .last_ssh
                            .as_ref()
                            .map(|s| s.port)
                            .unwrap_or(22),
                        user: key.split('@').next().unwrap_or("").to_string(),
                        host: key.split('@').nth(1).unwrap_or("").to_string(),
                    };
                    self.ssh_sessions.insert(key, entry);
                }
                let fallback = dirs_next_home().unwrap_or_else(|| PathBuf::from("/"));
                match self.dual_pane.active {
                    PaneSide::Left => {
                        self.dual_pane.left = PaneContent::Explorer(Explorer::new(fallback))
                    }
                    PaneSide::Right => {
                        self.dual_pane.right = PaneContent::Explorer(Explorer::new(fallback))
                    }
                }
                true
            }
            Action::SshConnectNextField => {
                if let InputMode::SshConnect(ref mut state) = self.input_mode {
                    state.active_field = (state.active_field + 1) % 5;
                }
                true
            }
            Action::SshConnectPrevField => {
                if let InputMode::SshConnect(ref mut state) = self.input_mode {
                    if state.active_field == 0 {
                        state.active_field = 4;
                    } else {
                        state.active_field -= 1;
                    }
                }
                true
            }
            Action::SshConnectConfirm => {
                if let InputMode::SshConnect(ref mut state) = self.input_mode {
                    if state.host().is_empty() {
                        self.dialog = Some(Dialog::error("Host cannot be empty"));
                        return true;
                    }
                    state.connecting = true;
                    let host = state.host().to_string();
                    let port = state.port();
                    let user = state.user().to_string();
                    let path = state.path().to_string();
                    let password = state.password().map(|s| s.to_string());
                    let tx = self.action_tx.clone();
                    tokio::spawn(async move {
                        match crate::ssh::SshSession::connect(
                            &host,
                            port,
                            &user,
                            password.as_deref(),
                        )
                        .await
                        {
                            Ok(session) => {
                                let session_arc = Arc::new(tokio::sync::Mutex::new(session));
                                match session_arc.lock().await.list_dir(&path).await {
                                    Ok(entries) => {
                                        let _ = tx.send(Action::SshConnected {
                                            session: session_arc.clone(),
                                            host,
                                            port,
                                            user,
                                            initial_path: path,
                                            entries,
                                        });
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::SshConnectionFailed(format!(
                                            "Connected but failed to list {}: {}",
                                            path, e
                                        )));
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Action::SshConnectionFailed(e));
                            }
                        }
                    });
                }
                true
            }
            Action::SshConnected {
                session,
                host,
                port,
                user,
                initial_path,
                entries,
            } => {
                self.input_mode = InputMode::Normal;
                self.connecting_to = None;
                log::debug!("ssh: connected to {}@{}", user, host);
                // Persist last successful connection for pre-filling next time
                let mut cfg = SessionConfig::load();
                cfg.last_ssh = Some(LastSshConfig {
                    host: host.clone(),
                    port,
                    user: user.clone(),
                    path: initial_path.clone(),
                });
                cfg.save();
                let host_label = format!("{}@{}:", user, host);
                // Remove from background map if it was there (now it's the active pane)
                let key = format!("{}@{}", user, host);
                self.ssh_sessions.remove(&key);
                let remote = RemoteExplorer::new(session, initial_path, entries, host_label);
                match self.dual_pane.active {
                    PaneSide::Left => self.dual_pane.left = PaneContent::Remote(remote),
                    PaneSide::Right => self.dual_pane.right = PaneContent::Remote(remote),
                }
                true
            }
            Action::SshConnectionFailed(msg) => {
                log::debug!("ssh: connection failed: {}", msg);
                self.connecting_to = None;
                if let InputMode::SshConnect(ref mut state) = self.input_mode {
                    state.connecting = false;
                }
                self.dialog = Some(Dialog::error(format!("SSH connection failed: {}", msg)));
                true
            }
            Action::SshPasswordRequired {
                host,
                port,
                user,
                path,
            } => {
                log::debug!(
                    "ssh: key auth failed for {}@{}, prompting for password",
                    user,
                    host
                );
                self.connecting_to = None;
                let mut state = SshConnectState::new();
                state.fields[0] = host;
                state.fields[1] = port.to_string();
                state.fields[2] = user;
                state.fields[3] = path;
                state.active_field = 4; // focus password field
                self.input_mode = InputMode::SshConnect(state);
                true
            }
            Action::RemoteNavigate(path) => {
                if let Some(remote) = self.dual_pane.active_remote() {
                    let session = remote.session.clone();
                    let tx = self.action_tx.clone();
                    let path_clone = path.clone();
                    tokio::spawn(async move {
                        let sess = session.lock().await;
                        match sess.list_dir(&path_clone).await {
                            Ok(entries) => {
                                let _ = tx.send(Action::RemoteListing {
                                    path: path_clone,
                                    entries,
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(Action::RemoteListingFailed(e));
                            }
                        }
                    });
                }
                true
            }
            Action::RemoteListing { path, entries } => {
                if let Some(remote) = self.dual_pane.active_remote_mut() {
                    remote.update_listing(path, entries);
                }
                true
            }
            Action::RemoteListingFailed(msg) => {
                if let Some(remote) = self.dual_pane.active_remote_mut() {
                    remote.loading = false;
                }
                self.dialog = Some(Dialog::error(format!("Remote listing failed: {}", msg)));
                true
            }
            Action::RemoteViewFile => {
                if let Some(remote) = self.dual_pane.active_remote()
                    && let Some(entry) = remote.current_entry()
                    && !entry.is_dir
                {
                    let session = remote.session.clone();
                    let remote_path = entry.remote_path.clone();
                    let name = entry.name.clone();
                    let tx = self.action_tx.clone();
                    tokio::spawn(async move {
                        // Determine file extension for tempfile suffix
                        let suffix = std::path::Path::new(&name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| format!(".{}", e))
                            .unwrap_or_default();
                        let tmp = match tempfile::Builder::new().suffix(&suffix).tempfile() {
                            Ok(f) => f,
                            Err(e) => {
                                let _ = tx
                                    .send(Action::RemoteOpError(format!("Tempfile error: {}", e)));
                                return;
                            }
                        };
                        let tmp_path = tmp.path().to_path_buf();
                        // Keep the tempfile from being deleted
                        let _ = tmp.keep();
                        let sess = session.lock().await;
                        match sess.download(&remote_path, &tmp_path).await {
                            Ok(()) => {
                                let _ = tx.send(Action::RemoteViewReady(tmp_path));
                            }
                            Err(e) => {
                                let _ = tx.send(Action::RemoteOpError(format!(
                                    "Download for view failed: {}",
                                    e
                                )));
                            }
                        }
                    });
                }
                true
            }
            Action::RemoteViewReady(path) => {
                if let Err(msg) = self.dual_pane.open_editor_in_active(path) {
                    self.dialog = Some(Dialog::error(msg));
                }
                true
            }
            Action::RemoteOpComplete(msg) => {
                self.dialog = Some(Dialog::info(msg.clone()));
                // Refresh remote listing
                if let Some(remote) = self.dual_pane.active_remote() {
                    let session = remote.session.clone();
                    let current_path = remote.current_path.clone();
                    let tx = self.action_tx.clone();
                    tokio::spawn(async move {
                        let sess = session.lock().await;
                        match sess.list_dir(&current_path).await {
                            Ok(entries) => {
                                let _ = tx.send(Action::RemoteListing {
                                    path: current_path,
                                    entries,
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(Action::RemoteListingFailed(e));
                            }
                        }
                    });
                }
                true
            }
            Action::SshSessionExpired { key } => {
                log::debug!("ssh: backgrounded session expired: {}", key);
                self.ssh_sessions.remove(&key);
                self.connecting_to = None;
                // Re-open connect dialog with last config pre-filled
                let mut state = SshConnectState::new();
                if let Some(last) = SessionConfig::load().last_ssh
                    && !last.host.is_empty()
                {
                    state.fields[0] = last.host;
                    state.fields[1] = last.port.to_string();
                    state.fields[2] = last.user;
                    state.fields[3] = last.path;
                }
                self.input_mode = InputMode::SshConnect(state);
                true
            }
            Action::RemoteOpError(msg) => {
                self.dialog = Some(Dialog::error(msg.clone()));
                // Refresh remote listing
                if let Some(remote) = self.dual_pane.active_remote() {
                    let session = remote.session.clone();
                    let current_path = remote.current_path.clone();
                    let tx = self.action_tx.clone();
                    tokio::spawn(async move {
                        let sess = session.lock().await;
                        match sess.list_dir(&current_path).await {
                            Ok(entries) => {
                                let _ = tx.send(Action::RemoteListing {
                                    path: current_path,
                                    entries,
                                });
                            }
                            Err(e) => {
                                let _ = tx.send(Action::RemoteListingFailed(e));
                            }
                        }
                    });
                }
                true
            }
            _ => false,
        }
    }
}
