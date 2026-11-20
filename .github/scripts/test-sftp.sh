#!/usr/bin/env bash
# Run on a disposable Linux runner with OpenSSH server installed.
set -euo pipefail
fixture=$(mktemp -d)
password_user="cpt-fixture-$$"
password_user_created=false
cleanup() {
    if [[ -s "$fixture/pid" ]]; then sudo kill "$(cat "$fixture/pid")" || true; fi
    if $password_user_created; then sudo userdel "$password_user" || true; fi
    rm -rf -- "$fixture"
}
trap cleanup EXIT
mkdir -p "$fixture/home/.ssh" "$fixture/remote"
mkdir "$fixture/password-home"
chmod 711 "$fixture"
sudo useradd --no-create-home --home-dir "$fixture/password-home" --shell /bin/sh "$password_user"
password_user_created=true
sudo chown "$password_user" "$fixture/password-home"
password=$(python3 -c 'import secrets; print(secrets.token_urlsafe(32))')
printf '%s:%s\n' "$password_user" "$password" | sudo chpasswd
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
PasswordAuthentication yes
KbdInteractiveAuthentication no
AllowUsers $(id -un) $password_user
UsePAM yes
Subsystem sftp internal-sftp
Match User $password_user
    AuthorizedKeysFile none
EOF
sudo mkdir -p /run/sshd
sudo /usr/sbin/sshd -f "$fixture/sshd_config"
export CPT_SSH_FIXTURE="$fixture" CPT_SSH_USER="$(id -un)"
export CPT_SSH_PASSWORD_USER="$password_user" CPT_SSH_PASSWORD="$password"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
HOME="$fixture/home" cargo +stable test --locked --test ssh_loopback -- --ignored
HOME="$fixture/home" cargo +stable test --locked --lib upload_refreshes_inactive_remote_pane_without_changing_focus -- --ignored
