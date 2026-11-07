# cpTerm project instructions

cpTerm is a dual-pane terminal file manager. Work autonomously: investigate,
implement, test, commit and push routine fixes without waiting for confirmation.
Ask only when a decision materially changes scope, risks user data, or needs
credentials or access that you do not have.

## Workflow

- Check the current branch, working tree and upstream before editing. Fetch
  origin and fast-forward when possible. Preserve unrelated user changes.
- Make one small, independently useful change at a time. Validate it, stage
  only its files, commit and push directly to master before starting the next
  change. Verify the remote revision. Do not force push or rewrite history.
- Use the configured human author. Do not add assistant credits, co-author
  trailers, local tool settings, secrets or scratch files.
- Use lowercase imperative commit subjects. Follow rustfmt and Clippy.
- Run focused regressions for each fix. Before reporting a code change complete,
  check cargo fmt --all -- --check, cargo clippy --locked --all-targets
  --all-features -- -D warnings, and cargo test --locked. Report any gaps.
- Check dependency changes against current advisories, commit Cargo.lock, and
  exercise file operations in disposable directories, never against user data.

## Project map

- `src/app.rs` owns the event loop; `src/action.rs` defines messages.
  Input mapping, dispatch and rendering live under `src/app/`. Dialog actions
  must match the visible dialog; asynchronous results belong to their originating pane.
- `src/components/dual_pane.rs` holds two panes: local explorer, editor or
  remote explorer. Keep unsaved edits intact when switching panes or quitting.
- `src/fs/` handles local operations, archives and drive discovery.
  Use disposable trees for destructive tests. Do not follow destination links
  during copies or interpolate paths into shell commands.
- `src/ssh/session.rs` checks host keys and handles SFTP. The Linux CI fixture
  in `.github/scripts/test-sftp.sh` uses a disposable loopback server and keys.
- `src/task.rs` streams subprocess output over a bounded channel.
  `src/config.rs` resolves config paths; unit tests use isolated storage.
- Use [README.md](README.md) for shortcuts, supported transfers and config
  locations. The manifest and lockfile are authoritative for dependencies.

## Verification

Run the exact CI commands:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
python3 .github/scripts/audit_lock.py
```

The audit uses Python 3.11+ and sends public crate names and versions to OSV.
CI also opens the real binary in a terminal and runs the loopback SSH test on
Linux. Confirm the final push passes on Linux, macOS and Windows. Commit messages
use lowercase imperative subjects; avoid em dashes in text and commits.
