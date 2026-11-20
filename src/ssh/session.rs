use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::STANDARD};
use ssh2::{ErrorCode, FileStat, MethodType, OpenFlags, OpenType, RenameFlags, Session, Sftp};
use zeroize::Zeroizing;

#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub name: String,
    pub remote_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub is_hidden: bool,
}

fn pinned_key_matches(entries: &str, kind: &str, key: &[u8]) -> bool {
    let mut trusted = false;
    for line in entries.lines().filter(|line| !line.starts_with('#')) {
        let mut fields = line.split_whitespace();
        let first = fields.next().unwrap_or_default();
        let marked = first.starts_with('@');
        if marked {
            fields.next(); // host pattern, already checked by ssh-keygen
        }
        let (Some(stored_kind), Some(data)) = (fields.next(), fields.next()) else {
            continue;
        };
        // libssh2 returns the complete SSH key blob, including its algorithm.
        if stored_kind == kind && STANDARD.decode(data).is_ok_and(|stored| stored == key) {
            if first == "@revoked" {
                return false;
            }
            trusted |= !marked;
        }
    }
    trusted
}

async fn check_server_key(host: &str, port: u16, kind: &str, key: &[u8]) -> Result<(), String> {
    let host = if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
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
    if result.status.success()
        && pinned_key_matches(
            std::str::from_utf8(&result.stdout).map_err(|e| e.to_string())?,
            kind,
            key,
        )
    {
        return Ok(());
    }
    Err(format!(
        "Untrusted SSH host key for {host}. Verify its fingerprint with the server administrator and check {}.",
        path.display()
    ))
}

pub struct SshSession {
    pub host: String,
    pub port: u16,
    pub user: String,
    operations: tokio::sync::mpsc::Sender<Operation>,
    socket: Arc<TcpStream>,
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

impl Drop for SshSession {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

type Operation = Box<dyn FnOnce(&Sftp) + Send + 'static>;

struct PendingConnection(Arc<TcpStream>, bool);
impl Drop for PendingConnection {
    fn drop(&mut self) {
        if self.1 {
            let _ = self.0.shutdown(Shutdown::Both);
        }
    }
}

fn modern_algorithms(session: &Session) -> Result<(), String> {
    // Keep legacy SHA-1, CBC and small DH groups out of negotiation. Algorithm
    // names unsupported by the linked library are ignored by method_pref.
    for (method, algorithms) in [
        (
            MethodType::Kex,
            "curve25519-sha256,curve25519-sha256@libssh.org,ecdh-sha2-nistp256,ecdh-sha2-nistp384,ecdh-sha2-nistp521,diffie-hellman-group16-sha512,diffie-hellman-group18-sha512,diffie-hellman-group14-sha256",
        ),
        (
            MethodType::HostKey,
            "ssh-ed25519,ecdsa-sha2-nistp256,ecdsa-sha2-nistp384,ecdsa-sha2-nistp521,rsa-sha2-512,rsa-sha2-256",
        ),
        (
            MethodType::CryptCs,
            "chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,aes128-gcm@openssh.com,aes256-ctr,aes128-ctr",
        ),
        (
            MethodType::CryptSc,
            "chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,aes128-gcm@openssh.com,aes256-ctr,aes128-ctr",
        ),
        (
            MethodType::MacCs,
            "hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com,hmac-sha2-512,hmac-sha2-256",
        ),
        (
            MethodType::MacSc,
            "hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com,hmac-sha2-512,hmac-sha2-256",
        ),
    ] {
        session
            .method_pref(method, algorithms)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Read raw server filenames before joining them to a remote path. Using the
/// convenience readdir would join untrusted names before traversal validation.
fn read_dir(sftp: &Sftp, path: &str) -> Result<Vec<(String, FileStat)>, String> {
    let mut directory = sftp.opendir(Path::new(path)).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    loop {
        match directory.readdir() {
            Ok((name, stat)) => {
                let name = name.to_str().ok_or("Remote filename is not UTF-8")?;
                if matches!(name, "." | "..") {
                    continue;
                }
                entries.push((name.to_owned(), stat));
            }
            // ssh2::File::readdir uses LIBSSH2_ERROR_FILE for end-of-directory.
            Err(error) if error.code() == ErrorCode::Session(-16) => break,
            Err(error) => return Err(error.to_string()),
        }
    }
    directory.close().map_err(|e| e.to_string())?;
    Ok(entries)
}

fn remote_child(parent: &str, name: &str) -> String {
    if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

impl SshSession {
    pub async fn connect(
        host: &str,
        port: u16,
        user: &str,
        password: Option<&str>,
    ) -> Result<Self, String> {
        let tcp = tokio::time::timeout(
            Duration::from_secs(15),
            tokio::net::TcpStream::connect((host, port)),
        )
        .await
        .map_err(|_| "SSH connect timed out")?
        .map_err(|e| format!("SSH connect failed: {e}"))?
        .into_std()
        .map_err(|e| e.to_string())?;
        tcp.set_nonblocking(false).map_err(|e| e.to_string())?;
        let socket = Arc::new(tcp);
        let mut pending = PendingConnection(socket.clone(), true);
        let stream = socket.clone();
        let username = user.to_owned();
        let password = password.map(|value| Zeroizing::new(value.to_owned()));
        let (operations, mut requests) = tokio::sync::mpsc::channel::<Operation>(16);
        let (key_sender, key_receiver) = tokio::sync::oneshot::channel();
        let (trust_sender, trust_receiver) = tokio::sync::oneshot::channel();
        let (auth_sender, auth_receiver) = tokio::sync::oneshot::channel();
        // All native handles, including their destructors, stay on this worker.
        // Dropping the session closes the socket and its request channel.
        tokio::task::spawn_blocking(move || {
            let mut key_sender = Some(key_sender);
            let initialized = (|| {
                let mut session = Session::new().map_err(|e| e.to_string())?;
                session.set_timeout(30_000);
                modern_algorithms(&session)?;
                session.set_tcp_stream(stream.try_clone().map_err(|e| e.to_string())?);
                session
                    .handshake()
                    .map_err(|e| format!("SSH handshake failed: {e}"))?;
                let (key, kind) = session
                    .host_key()
                    .ok_or("SSH server supplied no host key")?;
                let kind = match kind {
                    ssh2::HostKeyType::Ed25519 => "ssh-ed25519",
                    ssh2::HostKeyType::Ecdsa256 => "ecdsa-sha2-nistp256",
                    ssh2::HostKeyType::Ecdsa384 => "ecdsa-sha2-nistp384",
                    ssh2::HostKeyType::Ecdsa521 => "ecdsa-sha2-nistp521",
                    ssh2::HostKeyType::Rsa => "ssh-rsa",
                    _ => return Err("Unsupported SSH host key type".into()),
                };
                key_sender
                    .take()
                    .unwrap()
                    .send(Ok((kind, key.to_vec())))
                    .map_err(|_| "SSH connection cancelled")?;
                // Authentication is gated by the async known-host check.
                if !trust_receiver.blocking_recv().unwrap_or(false) {
                    return Err("SSH host key was not trusted".into());
                }
                for path in std::env::home_dir().into_iter().flat_map(|home| {
                    ["id_ed25519", "id_ecdsa"].map(|name| home.join(".ssh").join(name))
                }) {
                    // Read keys using Rust's native path support, including
                    // Unicode Windows home paths; wipe the temporary buffer.
                    if path.is_file()
                        && let Ok(key) = std::fs::read_to_string(&path).map(Zeroizing::new)
                        && session
                            .userauth_pubkey_memory(&username, None, &key, None)
                            .is_ok()
                        && session.authenticated()
                    {
                        break;
                    }
                }
                if !session.authenticated()
                    && let Some(password) = password
                {
                    session
                        .userauth_password(&username, &password)
                        .map_err(|_| "Authentication failed: password rejected".to_owned())?;
                }
                if !session.authenticated() {
                    return Err(
                        "Authentication failed: no valid key found and no password provided".into(),
                    );
                }
                session
                    .sftp()
                    .map_err(|e| format!("Failed to create SFTP session: {e}"))
            })();
            match initialized {
                Ok(sftp) => {
                    if auth_sender.send(Ok(())).is_ok() {
                        while let Some(operation) = requests.blocking_recv() {
                            operation(&sftp);
                        }
                    }
                    let _ = stream.shutdown(Shutdown::Both);
                    drop(sftp);
                }
                Err(error) => {
                    let _ = stream.shutdown(Shutdown::Both);
                    if let Some(sender) = key_sender {
                        let _ = sender.send(Err(error.clone()));
                    }
                    let _ = auth_sender.send(Err(error));
                }
            }
        });
        let (kind, key) = key_receiver
            .await
            .map_err(|_| "SSH worker stopped during handshake")??;
        check_server_key(host, port, kind, &key).await?;
        trust_sender
            .send(true)
            .map_err(|_| "SSH worker stopped before authentication")?;
        auth_receiver
            .await
            .map_err(|_| "SSH worker stopped during authentication")??;
        pending.1 = false;
        Ok(Self {
            host: host.to_owned(),
            port,
            user: user.to_owned(),
            operations,
            socket,
        })
    }

    async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Sftp) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.operations
            .send(Box::new(move |sftp| {
                let _ = sender.send(work(sftp));
            }))
            .await
            .map_err(|_| "SSH connection is closed")?;
        receiver.await.map_err(|_| "SSH worker stopped")?
    }

    /// Close the transport immediately, including blocked SFTP operations.
    pub fn disconnect(&self) -> Result<(), String> {
        self.socket
            .shutdown(Shutdown::Both)
            .map_err(|e| e.to_string())
    }

    pub async fn list_dir(&self, path: &str) -> Result<Vec<RemoteEntry>, String> {
        let path = path.to_owned();
        let mut entries = self
            .run(move |sftp| {
                Ok(read_dir(sftp, &path)?
                    .into_iter()
                    .filter(|(name, _)| crate::util::valid_filename(name))
                    .map(|(name, stat)| RemoteEntry {
                        remote_path: remote_child(&path, &name),
                        is_dir: stat.is_dir(),
                        size: stat.size.unwrap_or(0),
                        modified: stat
                            .mtime
                            .and_then(|secs| UNIX_EPOCH.checked_add(Duration::from_secs(secs))),
                        is_hidden: name.starts_with('.'),
                        name,
                    })
                    .collect::<Vec<_>>())
            })
            .await?;
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), String> {
        let path = PathBuf::from(path);
        self.run(move |sftp| {
            sftp.mkdir(&path, 0o777)
                .map_err(|e| format!("SFTP mkdir failed: {e}"))
        })
        .await
    }

    pub async fn remove_file(&self, path: &str) -> Result<(), String> {
        let path = PathBuf::from(path);
        self.run(move |sftp| {
            sftp.unlink(&path)
                .map_err(|e| format!("SFTP remove_file failed: {e}"))
        })
        .await
    }

    pub async fn remove_dir_recursive(&self, path: &str) -> Result<(), String> {
        let path = path.to_owned();
        self.run(move |sftp| {
            let mut pending = vec![(path, false)];
            while let Some((path, visited)) = pending.pop() {
                if visited {
                    sftp.rmdir(Path::new(&path)).map_err(|e| e.to_string())?;
                } else {
                    let entries = read_dir(sftp, &path)?;
                    pending.push((path.clone(), true));
                    for (name, stat) in entries {
                        if !crate::util::valid_filename(&name) {
                            return Err("Invalid child name in remote directory".into());
                        }
                        let child = remote_child(&path, &name);
                        if stat.is_dir() {
                            pending.push((child, false));
                        } else {
                            sftp.unlink(Path::new(&child)).map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
            Ok(())
        })
        .await
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        let (from, to) = (PathBuf::from(from), PathBuf::from(to));
        self.run(move |sftp| {
            sftp.rename(&from, &to, Some(RenameFlags::empty()))
                .map_err(|e| format!("SFTP rename failed: {e}"))
        })
        .await
    }

    pub async fn download(&self, remote_path: &str, local_path: &Path) -> Result<(), String> {
        let (remote, local) = (PathBuf::from(remote_path), local_path.to_owned());
        self.run(move |sftp| {
            let mut input = sftp.open(&remote).map_err(|e| e.to_string())?;
            let mut output =
                tempfile::NamedTempFile::new_in(local.parent().ok_or("Invalid destination")?)
                    .map_err(|e| e.to_string())?;
            std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
            input.close().map_err(|e| e.to_string())?;
            output.flush().map_err(|e| e.to_string())?;
            output
                .persist_noclobber(&local)
                .map_err(|e| format!("Cannot create {}: {e}", local.display()))?;
            Ok(())
        })
        .await
    }

    pub async fn preview(&self, remote_path: &str) -> Result<String, String> {
        let remote = PathBuf::from(remote_path);
        self.run(move |sftp| {
            const LIMIT: u64 = 64 * 1024;
            let mut file = sftp.open(&remote).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            (&mut file)
                .take(LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            file.close().map_err(|e| e.to_string())?;
            if bytes.len() as u64 > LIMIT {
                return Err("Preview limit is 64 KiB; download the file to open it locally".into());
            }
            let text = String::from_utf8(bytes).map_err(|_| "Cannot preview non-UTF-8 data")?;
            Ok(text
                .chars()
                .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
                .collect())
        })
        .await
    }

    pub async fn upload(&self, local_path: &Path, remote_path: &str) -> Result<(), String> {
        if !local_path.is_file() {
            return Err("Upload source is not a regular file".into());
        }
        let source = tokio::fs::File::open(local_path)
            .await
            .map_err(|e| format!("Local read failed: {e}"))?;
        if !source
            .metadata()
            .await
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("Upload source is not a regular file".into());
        }
        upload_from(self, source, remote_path).await
    }
}

enum UploadMessage {
    Data(Vec<u8>),
    Finish,
}

struct CancelUpload(Arc<AtomicBool>);
impl Drop for CancelUpload {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn staged_upload(
    sftp: &Sftp,
    dest: &str,
    mut input: tokio::sync::mpsc::Receiver<UploadMessage>,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    match sftp.lstat(Path::new(dest)) {
        Ok(_) => return Err(format!("Upload destination already exists: {dest}")),
        Err(e) if e.code() == ErrorCode::SFTP(2) => {}
        Err(e) => return Err(format!("Cannot check upload destination: {e}")),
    }
    let prefix = dest.rfind('/').map_or("", |i| &dest[..=i]);
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let staging = format!(
        "{prefix}.cpt-upload-{:032x}.part",
        u128::from_le_bytes(nonce)
    );
    if staging == dest {
        return Err("Temporary name matches destination; retry upload".into());
    }
    let mut file = sftp
        .open_mode(
            Path::new(&staging),
            OpenFlags::CREATE | OpenFlags::EXCLUSIVE | OpenFlags::WRITE,
            0o666,
            OpenType::File,
        )
        .map_err(|e| format!("Cannot stage upload: {e}"))?;
    let result = (|| {
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err("Upload cancelled".to_owned());
            }
            match input.blocking_recv() {
                Some(UploadMessage::Data(bytes)) => file
                    .write_all(&bytes)
                    .map_err(|e| format!("SFTP write failed: {e}"))?,
                Some(UploadMessage::Finish) => break,
                None => return Err("Upload input interrupted before EOF".into()),
            }
        }
        file.close()
            .map_err(|e| format!("SFTP close failed: {e}"))?;
        if cancelled.load(Ordering::Acquire) {
            return Err("Upload cancelled".into());
        }
        // Flags=0 uses standard SFTP RENAME, which refuses an existing target.
        // The default ssh2 flags request overwrite and must never be used here.
        sftp.rename(
            Path::new(&staging),
            Path::new(dest),
            Some(RenameFlags::empty()),
        )
        .map_err(|e| format!("Cannot publish {dest}: {e}"))
    })();
    drop(file);
    if let Err(error) = result {
        return match sftp.unlink(Path::new(&staging)) {
            Ok(()) => Err(error),
            Err(e) => Err(format!("{error}. Could not clean up {staging}: {e}")),
        };
    }
    Ok(())
}

/// Bound memory and keep asynchronous input on Tokio while libssh2 runs on a
/// blocking worker. A dropped/erroring producer never sends the publish marker.
pub async fn upload_from(
    session: &SshSession,
    mut source: impl tokio::io::AsyncRead + Unpin,
    dest: &str,
) -> Result<(), String> {
    use tokio::io::AsyncReadExt;
    let dest = dest.to_owned();
    let (sender, receiver) = tokio::sync::mpsc::channel(4);
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel = CancelUpload(cancelled.clone());
    let worker = session.run(move |sftp| staged_upload(sftp, &dest, receiver, &cancelled));
    tokio::pin!(worker);
    let feed = async move {
        let result = async {
            let mut bytes = vec![0; 64 * 1024];
            loop {
                let size = source
                    .read(&mut bytes)
                    .await
                    .map_err(|e| format!("Upload input failed: {e}"))?;
                if size == 0 {
                    break;
                }
                sender
                    .send(UploadMessage::Data(bytes[..size].to_vec()))
                    .await
                    .map_err(|_| "Upload worker stopped")?;
            }
            sender
                .send(UploadMessage::Finish)
                .await
                .map_err(|_| "Upload worker stopped")?;
            Ok::<_, String>(())
        }
        .await;
        // Close the channel even when a source read fails, so the worker can
        // remove its staging file rather than wait forever for another chunk.
        drop(sender);
        result
    };
    tokio::pin!(feed);
    tokio::select! {
        result = &mut worker => result,
        fed = &mut feed => {
            let result = worker.await;
            match (fed, result) {
                (Err(input), Err(remote)) if input != "Upload worker stopped" => Err(format!("{input}. {remote}")),
                (_, Err(error)) => Err(error),
                (Err(error), _) => Err(error),
                _ => Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod host_key_tests {
    use super::*;

    #[test]
    fn native_backend_supports_existing_keys_and_modern_key_exchange() {
        let session = Session::new().unwrap();
        modern_algorithms(&session).unwrap();
        let keys = session.supported_algs(MethodType::HostKey).unwrap();
        assert!(keys.contains(&"ssh-ed25519"));
        assert!(keys.contains(&"ecdsa-sha2-nistp256"));
        let exchange = session.supported_algs(MethodType::Kex).unwrap();
        assert!(
            exchange.contains(&"curve25519-sha256")
                || exchange.contains(&"curve25519-sha256@libssh.org")
        );
    }

    #[test]
    fn only_a_matching_unrevoked_pin_authenticates_the_server() {
        let a = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ";
        let b = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF";
        let key = STANDARD
            .decode(a.split_whitespace().nth(1).unwrap())
            .unwrap();
        assert!(pinned_key_matches(
            &format!("server {a} comment"),
            "ssh-ed25519",
            &key
        ));
        for entries in [
            String::new(),
            format!("server {b}"),
            format!("@cert-authority server {a}"),
            format!("server {a}\n@revoked server {a}"),
            format!("server {}", a.replace("ssh-ed25519", "ssh-rsa")),
        ] {
            assert!(!pinned_key_matches(&entries, "ssh-ed25519", &key));
        }
    }
}
