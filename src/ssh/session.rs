use std::sync::Arc;

use chrono::{DateTime, Local};
use russh::client;
use russh::keys::PrivateKeyWithHashAlg;

/// A single directory entry on a remote server.
#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub name: String,
    pub remote_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<DateTime<Local>>,
    pub is_hidden: bool,
}

/// Check host patterns (including hashed entries) with the system OpenSSH tools.
pub struct SshClientHandler {
    host: String,
    port: u16,
}

fn pinned_key_matches(entries: &str, key: &russh::keys::PublicKey) -> bool {
    let mut trusted = false;
    for line in entries.lines().filter(|line| !line.starts_with('#')) {
        let mut fields = line.split_whitespace();
        let first = fields.next().unwrap_or_default();
        let marked = first.starts_with('@');
        if marked {
            fields.next(); // host pattern
        }
        let (Some(kind), Some(data)) = (fields.next(), fields.next()) else {
            continue;
        };
        if russh::keys::PublicKey::from_openssh(&format!("{kind} {data}"))
            .is_ok_and(|stored| stored.key_data() == key.key_data())
        {
            if first == "@revoked" {
                return false;
            }
            trusted |= !marked;
        }
    }
    trusted
}

impl client::Handler for SshClientHandler {
    type Error = Box<dyn std::error::Error + Send + Sync>;

    async fn check_server_key(
        &mut self,
        server_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return Err("SSH host certificates are not supported".into());
        };
        let host = if self.port == 22 {
            self.host.clone()
        } else {
            format!("[{}]:{}", self.host, self.port)
        };
        let path = std::env::home_dir()
            .ok_or("No home directory")?
            .join(".ssh/known_hosts");
        let result = tokio::process::Command::new("ssh-keygen")
            .arg("-F")
            .arg(&host)
            .arg("-f")
            .arg(&path)
            .output()
            .await
            .map_err(|e| format!("Cannot check host key using ssh-keygen: {e}"))?;
        if result.status.success() && pinned_key_matches(std::str::from_utf8(&result.stdout)?, key)
        {
            return Ok(true);
        }
        Err(format!("Untrusted SSH host key for {host}. Verify its fingerprint with the server administrator and check {}.", path.display()).into())
    }
}

/// Custom Debug impl since russh Handle doesn't implement Debug.
pub struct SshSession {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub _handle: client::Handle<SshClientHandler>,
    pub sftp: russh_sftp::client::SftpSession,
}

impl std::fmt::Debug for SshSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SshSession")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

impl SshSession {
    /// Connect to the remote host, trying key-based auth then password.
    pub async fn connect(
        host: &str,
        port: u16,
        user: &str,
        password: Option<&str>,
    ) -> Result<Self, String> {
        let config = Arc::new(client::Config::default());
        let handler = SshClientHandler {
            host: host.to_owned(),
            port,
        };

        let mut handle = client::connect(config, (host, port), handler)
            .await
            .map_err(|e| format!("SSH connect failed: {}", e))?;

        let mut authenticated = false;

        // Try key files first
        for path in std::env::home_dir()
            .into_iter()
            .flat_map(|home| ["id_ed25519", "id_ecdsa"].map(|name| home.join(".ssh").join(name)))
        {
            if !path.exists() {
                continue;
            }
            if let Ok(private_key) = russh::keys::load_secret_key(&path, None) {
                let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(private_key), None);
                match handle.authenticate_publickey(user, key_with_hash).await {
                    Ok(auth_result) if auth_result.success() => {
                        authenticated = true;
                        break;
                    }
                    _ => {}
                }
            }
        }

        // Fall back to password auth
        if !authenticated && let Some(pw) = password {
            let auth_result = handle
                .authenticate_password(user, pw)
                .await
                .map_err(|e| format!("Password auth failed: {}", e))?;
            if !auth_result.success() {
                return Err("Authentication failed: wrong password or key rejected".to_string());
            }
            authenticated = true;
        }

        if !authenticated {
            return Err(
                "Authentication failed: no valid key found and no password provided".to_string(),
            );
        }

        // Open SFTP subsystem
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| format!("Failed to open channel: {}", e))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| format!("Failed to open SFTP subsystem: {}", e))?;
        let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| format!("Failed to create SFTP session: {}", e))?;

        Ok(Self {
            host: host.to_string(),
            port,
            user: user.to_string(),
            _handle: handle,
            sftp,
        })
    }

    /// List directory contents, filtering out "." and "..", sorted dirs-first.
    pub async fn list_dir(&self, path: &str) -> Result<Vec<RemoteEntry>, String> {
        let dir_entries = self
            .sftp
            .read_dir(path)
            .await
            .map_err(|e| format!("SFTP read_dir failed: {}", e))?;

        let mut entries: Vec<RemoteEntry> = dir_entries
            .into_iter()
            .filter(|e| {
                let n = e.file_name();
                n != "." && n != ".."
            })
            .map(|e| {
                let name = e.file_name();
                let meta = e.metadata();
                let is_dir = meta
                    .permissions
                    .map(|p| p & 0o170000 == 0o040000)
                    .unwrap_or(false);
                let size = meta.size.unwrap_or(0);
                let modified = meta.mtime.and_then(|t| {
                    DateTime::from_timestamp(t as i64, 0).map(|dt| dt.with_timezone(&Local))
                });
                let is_hidden = name.starts_with('.');
                let remote_path = if path.ends_with('/') {
                    format!("{}{}", path, name)
                } else {
                    format!("{}/{}", path, name)
                };
                RemoteEntry {
                    name,
                    remote_path,
                    is_dir,
                    size,
                    modified,
                    is_hidden,
                }
            })
            .collect();

        // Sort: dirs first, then alphabetically by name
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        Ok(entries)
    }

    /// Create a remote directory.
    pub async fn mkdir(&self, path: &str) -> Result<(), String> {
        self.sftp
            .create_dir(path)
            .await
            .map_err(|e| format!("SFTP mkdir failed: {}", e))
    }

    /// Remove a single remote file.
    pub async fn remove_file(&self, path: &str) -> Result<(), String> {
        self.sftp
            .remove_file(path)
            .await
            .map_err(|e| format!("SFTP remove_file failed: {}", e))
    }

    /// Recursively remove a remote directory.
    pub async fn remove_dir_recursive(&self, path: &str) -> Result<(), String> {
        let entries = self
            .sftp
            .read_dir(path)
            .await
            .map_err(|e| format!("SFTP read_dir failed during recursive delete: {}", e))?;

        for entry in entries {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child_path = format!("{}/{}", path, name);
            let is_dir = entry
                .metadata()
                .permissions
                .map(|p| p & 0o170000 == 0o040000)
                .unwrap_or(false);
            if is_dir {
                Box::pin(self.remove_dir_recursive(&child_path)).await?;
            } else {
                self.remove_file(&child_path).await?;
            }
        }

        self.sftp
            .remove_dir(path)
            .await
            .map_err(|e| format!("SFTP remove_dir failed: {}", e))
    }

    /// Rename/move a remote path.
    pub async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        self.sftp
            .rename(from, to)
            .await
            .map_err(|e| format!("SFTP rename failed: {}", e))
    }

    /// Download a remote file to a local path.
    pub async fn download(
        &self,
        remote_path: &str,
        local_path: &std::path::Path,
    ) -> Result<(), String> {
        use tokio::io::AsyncReadExt;

        let mut remote_file = self
            .sftp
            .open(remote_path)
            .await
            .map_err(|e| format!("SFTP open failed: {}", e))?;

        let mut data = Vec::new();
        remote_file
            .read_to_end(&mut data)
            .await
            .map_err(|e| format!("SFTP read failed: {}", e))?;

        std::fs::write(local_path, &data).map_err(|e| format!("Local write failed: {}", e))
    }

    /// Upload a local file to a remote path.
    pub async fn upload(
        &self,
        local_path: &std::path::Path,
        remote_path: &str,
    ) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;

        let data = std::fs::read(local_path).map_err(|e| format!("Local read failed: {}", e))?;

        let mut remote_file = self
            .sftp
            .create(remote_path)
            .await
            .map_err(|e| format!("SFTP create failed: {}", e))?;

        remote_file
            .write_all(&data)
            .await
            .map_err(|e| format!("SFTP write failed: {}", e))?;

        remote_file
            .shutdown()
            .await
            .map_err(|e| format!("SFTP shutdown failed: {}", e))
    }
}

#[cfg(test)]
mod host_key_tests {
    use super::*;

    #[test]
    fn only_a_matching_unrevoked_pin_authenticates_the_server() {
        let a = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
        let b = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF";
        let key = russh::keys::PublicKey::from_openssh(a).unwrap();
        assert!(pinned_key_matches(&format!("server {a} comment"), &key));
        for entries in [
            String::new(),
            format!("server {b}"),
            format!("@cert-authority server {a}"),
            format!("server {a}\n@revoked server {a}"),
        ] {
            assert!(!pinned_key_matches(&entries, &key));
        }
    }
}
