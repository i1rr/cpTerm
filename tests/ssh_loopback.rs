//! Run via .github/scripts/test-sftp.sh, never against a user's SSH configuration.

use cpt::ssh::session::upload_from;
use russh_sftp::client::SftpSession;
use std::{fs, path::Path, time::Duration};
use tokio::io::AsyncWriteExt;

// Keep the input open until we can see bytes on the server. This deterministically
// interrupts a transfer before publication, independently of network speed.
async fn interrupt_or_conflict(sftp: &SftpSession, dir: &Path, dest: &str, disconnect: bool) {
    let before: std::collections::HashSet<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let (mut input, source) = tokio::io::duplex(65536);
    input.write_all(&[42; 32768]).await.unwrap();
    let upload = upload_from(sftp, source, dest);
    tokio::pin!(upload);
    let observe = async {
        loop {
            let stage = fs::read_dir(dir).unwrap().filter_map(Result::ok).find(|e| {
                !before.contains(&e.path())
                    && e.file_name().to_string_lossy().starts_with(".cpt-upload-")
                    && e.metadata().is_ok_and(|m| m.len() > 0)
            });
            if let Some(stage) = stage {
                break stage.path();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    let stage = tokio::select! {
        r = &mut upload => panic!("upload finished before EOF: {r:?}"),
        stage = tokio::time::timeout(Duration::from_secs(10), observe) => stage.expect("no partial upload observed"),
    };
    let final_path = dir.join("retry.bin");
    assert!(!final_path.exists());
    if disconnect {
        sftp.close().await.unwrap();
    } else {
        fs::write(&final_path, b"arrived later").unwrap();
    }
    drop(input);
    assert!(
        tokio::time::timeout(Duration::from_secs(25), upload)
            .await
            .unwrap()
            .is_err()
    );
    if disconnect {
        assert!(!final_path.exists());
    } else {
        assert_eq!(fs::read(&final_path).unwrap(), b"arrived later");
        assert!(!stage.exists());
        fs::remove_file(final_path).unwrap();
    }
}

async fn verify_retry(sftp: &SftpSession, dir: &Path, dest: &str) {
    interrupt_or_conflict(sftp, dir, dest, false).await;
    upload_from(sftp, &b"complete retry"[..], dest)
        .await
        .unwrap();
    assert_eq!(fs::read(dir.join("retry.bin")).unwrap(), b"complete retry");
}

#[tokio::test]
#[ignore = "requires CPT_TEST_SFTP_SERVER pointing to an OpenSSH sftp-server executable"]
async fn standalone_sftp_interruption_and_retry() {
    let dir = tempfile::tempdir().unwrap();
    let program = std::env::var_os("CPT_TEST_SFTP_SERVER").unwrap();
    let start = || async {
        let mut child = tokio::process::Command::new(&program)
            .current_dir(dir.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let stream = tokio::io::join(child.stdout.take().unwrap(), child.stdin.take().unwrap());
        (SftpSession::new(stream).await.unwrap(), child)
    };
    let (sftp, _child) = start().await;
    interrupt_or_conflict(&sftp, dir.path(), "retry.bin", true).await;
    let (sftp, _retry_child) = start().await;
    verify_retry(&sftp, dir.path(), "retry.bin").await;
}

#[tokio::test]
#[ignore = "requires the isolated loopback OpenSSH fixture"]
async fn verify_host_before_auth_and_transfer_without_overwrite() {
    use cpt::ssh::SshSession;
    use std::{fs, path::PathBuf};

    let fixture = PathBuf::from(std::env::var_os("CPT_SSH_FIXTURE").unwrap());
    let home = fixture.join("home");
    assert_eq!(std::env::home_dir().unwrap(), home);
    let pins = home.join(".ssh/known_hosts");
    let user = std::env::var("CPT_SSH_USER").unwrap();
    let port = 22222;
    let connect = || SshSession::connect("127.0.0.1", port, &user, None);
    assert!(
        connect()
            .await
            .unwrap_err()
            .contains("Untrusted SSH host key")
    );
    let pin = |key: &str| {
        format!(
            "[127.0.0.1]:{port} {}",
            fs::read_to_string(fixture.join(key)).unwrap()
        )
    };
    fs::write(&pins, pin("other.pub")).unwrap();
    assert!(
        connect()
            .await
            .unwrap_err()
            .contains("Untrusted SSH host key")
    );
    fs::write(
        &pins,
        format!("{}@revoked {}", pin("host.pub"), pin("host.pub")),
    )
    .unwrap();
    assert!(
        connect()
            .await
            .unwrap_err()
            .contains("Untrusted SSH host key")
    );
    fs::write(&pins, pin("host.pub")).unwrap();
    let session = connect().await.unwrap();
    let remote = fixture.join("remote").join("contents");
    let remote = remote.to_str().unwrap();
    let source = fixture.join("source");
    let destination = fixture.join("download");
    fs::write(&source, "preview text").unwrap();
    session.upload(&source, remote).await.unwrap();
    assert_eq!(session.preview(remote).await.unwrap(), "preview text");
    fs::write(&source, vec![42; 2 * 1024 * 1024]).unwrap();
    assert!(session.upload(&source, remote).await.is_err());
    assert_eq!(session.preview(remote).await.unwrap(), "preview text");
    session.remove_file(remote).await.unwrap();
    session.upload(&source, remote).await.unwrap();
    assert!(
        session
            .preview(remote)
            .await
            .unwrap_err()
            .contains("64 KiB")
    );
    session.download(remote, &destination).await.unwrap();
    assert_eq!(fs::read(&destination).unwrap(), fs::read(&source).unwrap());
    fs::write(&destination, "original").unwrap();
    assert!(session.download(remote, &destination).await.is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"original");
    let remote_dir = fixture.join("remote");
    let retry = remote_dir.join("retry.bin");
    let retry = retry.to_str().unwrap();
    interrupt_or_conflict(&session.sftp, &remote_dir, retry, true).await;
    let reconnected = connect().await.unwrap();
    verify_retry(&reconnected.sftp, &remote_dir, retry).await;
}
