use std::path::PathBuf;
use std::sync::Arc;

use crate::action::Action;

#[derive(Debug, Clone)]
pub struct SshConnectState {
    pub fields: Vec<String>, // [host, port, user, path, password]
    pub active_field: usize,
    pub connecting: bool,
}

impl Default for SshConnectState {
    fn default() -> Self {
        Self {
            fields: vec![
                String::new(),
                "22".to_string(),
                std::env::var("USER").unwrap_or_default(),
                "/".to_string(),
                String::new(),
            ],
            active_field: 0,
            connecting: false,
        }
    }
}

impl SshConnectState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn host(&self) -> &str {
        &self.fields[0]
    }

    pub fn port(&self) -> u16 {
        self.fields[1].parse().unwrap_or(22)
    }

    pub fn user(&self) -> &str {
        &self.fields[2]
    }

    pub fn path(&self) -> &str {
        &self.fields[3]
    }

    pub fn password(&self) -> Option<&str> {
        if self.fields[4].is_empty() {
            None
        } else {
            Some(&self.fields[4])
        }
    }

    pub fn field_label(idx: usize) -> &'static str {
        match idx {
            0 => "Host",
            1 => "Port",
            2 => "User",
            3 => "Path",
            4 => "Password",
            _ => "",
        }
    }

    pub fn is_password_field(idx: usize) -> bool {
        idx == 4
    }
}

#[derive(Debug, Clone)]
pub enum InputMode {
    Normal,
    Filter(String),
    Rename(String),
    MkDir(String),
    CreateFile(String),
    CreateTypeChoice,
    /// Choose how to unpack: (E)xtract here, create (F)older, (C)ustom path.
    UnpackChoice {
        archive: PathBuf,
    },
    /// Typing a custom extraction path.
    UnpackCustomPath {
        archive: PathBuf,
        text: String,
    },
    Command(String),
    TaskOutput,
    ContextMenu(ContextMenuState),
    SshConnect(SshConnectState),
}

#[derive(Debug, Clone)]
pub struct ContextMenuState {
    pub items: Vec<ContextMenuItem>,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub struct ContextMenuItem {
    pub label: String,
    pub action: Action,
}

pub(crate) enum PendingOp {
    Copy { pairs: Vec<(PathBuf, PathBuf)> },
    Move { pairs: Vec<(PathBuf, PathBuf)> },
    Delete(Vec<PathBuf>),
    CloseEditor,
    RemoteDelete {
        entries: Vec<(String, bool)>,
        session: Arc<tokio::sync::Mutex<crate::ssh::session::SshSession>>,
    },
    RemoteDownload {
        paths: Vec<String>,
        session: Arc<tokio::sync::Mutex<crate::ssh::session::SshSession>>,
        dest: PathBuf,
    },
    RemoteUpload {
        paths: Vec<PathBuf>,
        session: Arc<tokio::sync::Mutex<crate::ssh::session::SshSession>>,
        remote_dest: String,
    },
}
