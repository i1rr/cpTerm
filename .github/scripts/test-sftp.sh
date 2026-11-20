#!/usr/bin/env bash
# Run on a disposable Linux runner with OpenSSH server installed.
set -euo pipefail
fixture=$(mktemp -d /tmp/cpt-sftp.XXXXXXXX)
password_user="cpt-fixture-$$"
password_user_created=false
password_uid=""
cleanup() {
    if [[ -s "$fixture/pid" ]]; then sudo kill "$(cat "$fixture/pid")" || true; fi
    if $password_user_created && [[ "$password_uid" =~ ^[1-9][0-9]*$ ]]; then
        # PAM may leave a user manager alive after the SSH connection closes.
        # This UID belongs only to the account created by this fixture.
        sudo loginctl terminate-user "$password_user" || true
        sudo pkill -KILL -u "$password_uid" || true
        sudo userdel --force "$password_user" || true
    fi
    # The password account can create private cache files under its test home.
    # Keep privileged deletion confined to the directory made above.
    if [[ "$fixture" == /tmp/cpt-sftp.* && "$fixture" != */*/* ]]; then
        sudo rm -rf -- "$fixture"
    else
        echo 'Refusing to remove an unexpected fixture path' >&2
        return 1
    fi
}
trap cleanup EXIT
mkdir -p "$fixture/home/.ssh" "$fixture/remote"
mkdir "$fixture/password-home"
chmod 711 "$fixture"
sudo useradd --no-create-home --home-dir "$fixture/password-home" --shell /bin/sh "$password_user"
password_user_created=true
password_uid=$(id -u "$password_user")
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
