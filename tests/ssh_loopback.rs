//! Run via .github/scripts/test-sftp.sh, never against a user's SSH configuration.

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
}
