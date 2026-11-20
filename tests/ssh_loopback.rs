//! Run via .github/scripts/test-sftp.sh, never against a user's SSH configuration.

use cpt::ssh::SshSession;
use cpt::ssh::session::upload_from;
use std::{fs, path::Path, time::Duration};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Copy)]
enum StopUpload {
    Disconnect,
    Conflict,
    Cancel,
}

// Keep the input open until we can see bytes on the server. This deterministically
// interrupts a transfer before publication, independently of network speed.
async fn interrupt_or_conflict(session: &SshSession, dir: &Path, dest: &str, stop: StopUpload) {
    let before: std::collections::HashSet<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let (mut input, source) = tokio::io::duplex(65536);
    input.write_all(&[42; 32768]).await.unwrap();
    let mut upload = Box::pin(upload_from(session, source, dest));
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
    if matches!(stop, StopUpload::Cancel) {
        drop(upload);
        tokio::time::timeout(Duration::from_secs(25), async {
            while stage.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("cancelled upload did not clean up its staging file");
        assert!(!final_path.exists());
        return;
    }
    if matches!(stop, StopUpload::Disconnect) {
        session.disconnect().unwrap();
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
    if matches!(stop, StopUpload::Disconnect) {
        assert!(!final_path.exists());
    } else {
        assert_eq!(fs::read(&final_path).unwrap(), b"arrived later");
        assert!(!stage.exists());
        fs::remove_file(final_path).unwrap();
    }
}

async fn verify_retry(session: &SshSession, dir: &Path, dest: &str) {
    interrupt_or_conflict(session, dir, dest, StopUpload::Conflict).await;
    upload_from(session, &b"complete retry"[..], dest)
        .await
        .unwrap();
    assert_eq!(fs::read(dir.join("retry.bin")).unwrap(), b"complete retry");
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
    let password_user = std::env::var("CPT_SSH_PASSWORD_USER").unwrap();
    let password = std::env::var("CPT_SSH_PASSWORD").unwrap();
    let rejected = SshSession::connect(
        "127.0.0.1",
        port,
        &password_user,
        Some("incorrect fixture password"),
    )
    .await
    .unwrap_err();
    assert!(rejected.contains("Authentication failed"));
    let password_session = SshSession::connect("127.0.0.1", port, &password_user, Some(&password))
        .await
        .unwrap();
    drop(password_session);
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
    interrupt_or_conflict(&session, &remote_dir, retry, StopUpload::Disconnect).await;
    let reconnected = connect().await.unwrap();
    verify_retry(&reconnected, &remote_dir, retry).await;
    fs::remove_file(retry).unwrap();
    interrupt_or_conflict(&reconnected, &remote_dir, retry, StopUpload::Cancel).await;

    // An erroring async producer must wake the native worker, clean up, and
    // leave no published file. It must not be mistaken for a successful EOF.
    struct FailedInput;
    impl tokio::io::AsyncRead for FailedInput {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::Error::other("injected input failure")))
        }
    }
    let failed = remote_dir.join("failed-input");
    let before: std::collections::HashSet<_> = fs::read_dir(&remote_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(
        tokio::time::timeout(
            Duration::from_secs(25),
            upload_from(&reconnected, FailedInput, failed.to_str().unwrap())
        )
        .await
        .unwrap()
        .is_err()
    );
    let after: std::collections::HashSet<_> = fs::read_dir(&remote_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(before, after);

    let tree = remote_dir.join("tree");
    let child = tree.join("child");
    reconnected.mkdir(tree.to_str().unwrap()).await.unwrap();
    reconnected.mkdir(child.to_str().unwrap()).await.unwrap();
    let file = child.join("file");
    reconnected
        .upload(&source, file.to_str().unwrap())
        .await
        .unwrap();
    let listed = reconnected.list_dir(child.to_str().unwrap()).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].size, 2 * 1024 * 1024);
    assert!(listed[0].modified.is_some());
    let renamed = child.join("renamed");
    reconnected
        .rename(file.to_str().unwrap(), renamed.to_str().unwrap())
        .await
        .unwrap();
    assert!(renamed.exists() && !file.exists());
    reconnected
        .remove_dir_recursive(tree.to_str().unwrap())
        .await
        .unwrap();
    assert!(!tree.exists());
}
