#!/usr/bin/env bash
# Run on a disposable Linux runner with OpenSSH server installed.
set -euo pipefail
fixture=$(mktemp -d)
cleanup() {
    if [[ -s "$fixture/pid" ]]; then sudo kill "$(cat "$fixture/pid")" || true; fi
    rm -rf -- "$fixture"
}
trap cleanup EXIT
mkdir -p "$fixture/home/.ssh" "$fixture/remote"
chmod 700 "$fixture/home/.ssh"
for key in host other home/.ssh/id_ed25519; do
    ssh-keygen -q -t ed25519 -N '' -f "$fixture/$key"
done
cat > "$fixture/sshd_config" <<EOF
ListenAddress 127.0.0.1
Port 22222
HostKey $fixture/host
PidFile $fixture/pid
AuthorizedKeysFile $fixture/home/.ssh/id_ed25519.pub
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
AllowUsers $(id -un)
UsePAM yes
Subsystem sftp internal-sftp
EOF
sudo mkdir -p /run/sshd
sudo /usr/sbin/sshd -f "$fixture/sshd_config"
export CPT_SSH_FIXTURE="$fixture" CPT_SSH_USER="$(id -un)"
export CPT_TEST_SFTP_SERVER=/usr/lib/openssh/sftp-server
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
HOME="$fixture/home" cargo +stable test --locked --test ssh_loopback -- --ignored
HOME="$fixture/home" cargo +stable test --locked --lib upload_refreshes_inactive_remote_pane_without_changing_focus -- --ignored
