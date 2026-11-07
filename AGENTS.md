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

## Architecture

### Event loop and message bus

`App` (`src/app.rs`) is the central hub. The main loop reads `Event`s from `EventHandler` (crossterm key/resize + tick), maps them to `Action`s, and dispatches them. `Action` (`action.rs`) is the entire app's message bus: every user interaction and every async callback is an `Action` variant.

`InputMode` (`app/types.rs`) tracks the current editing state: `Normal`, `Filter`, `Rename`, `MkDir`, `CreateFile`, `Command`, `UnpackChoice`, `UnpackCustomPath`, `TaskOutput`, `ContextMenu`, `SshConnect`. Key events are interpreted per mode.

The `app` module is split:

| File | Responsibility |
|------|---------------|
| `app.rs` | `App` struct, `new()`, `run()`, `save_session()` |
| `app/types.rs` | `InputMode`, `PendingOp`, `SshConnectState`, `ContextMenuState` |
| `app/draw.rs` | `draw()`, dialog and overlay rendering |
| `app/input.rs` | `map_event()`, `map_key()`, overlay key mappers |
| `app/dispatch.rs` | Main `dispatch()` coordinator; enforces modal priority |
| `app/dispatch_bm.rs` | Bookmark panel sub-dispatcher |
| `app/dispatch_drives.rs` | Drives panel sub-dispatcher |
| `app/dispatch_theme.rs` | Theme editor sub-dispatcher |
| `app/dispatch_fs.rs` | Copy/move/delete/rename/mkdir/unpack |
| `app/dispatch_ssh.rs` | SSH connect, remote ops, `do_command()` |

**Dispatch priority** (must be preserved): task streaming early-return > bookmark panel > drives panel > theme editor > context menu > FS > SSH > core normal-mode arms.

### Pane model

`DualPane` (`components/dual_pane.rs`) holds two `PaneContent` slots, each `Explorer(Explorer)` or `Editor(Box<EditorPane>)`. It owns focus switching, cross-pane queries, new-file highlighting, and editor open/close.

`Explorer` (`components/explorer.rs`) handles listing, cursor, multi-select, and filter for one pane. `EditorPane` (`components/editor_pane.rs`) wraps `ratatui-textarea::TextArea<'static>`; opening an editor replaces the active `Explorer` slot, closing restores an `Explorer` at `origin_dir`.

### Async task runner

`task.rs` exposes `run_task()` / `run_task_direct()`, which spawn shell commands as Tokio tasks and stream stdout/stderr line by line back as `Action::TaskLine`. `TaskState` buffers output (capped at 5000 lines) and is rendered by `components/task_window.rs`.

### Filesystem operations

- `fs/ops.rs` - async copy/move/delete with conflict detection; emits `Action::OperationProgress` / `Action::OperationComplete`
- `fs/archive.rs` - `resolve_unpack_command()` picks the best available tool per format and OS
- `fs/open.rs` - OS-default opener (`open::that()`) and pager launcher for F3
- `fs/entry.rs` - `FileEntry` and sorting
- `fs/drives.rs` - cross-platform `list_drives()`: Windows letter scan, macOS `/Volumes`, Linux `/mnt` + `/media[/<user>]` + `/run/media/<user>`. No new deps. Always includes `/` as Root on Unix.

### Session persistence

`config.rs` persists `SessionConfig` (last dirs, active pane, theme) via `confy` as `cpt.toml`. Storage paths:

- Windows: `%APPDATA%\rs.cpt\`
- Linux: `~/.config/rs.cpt/`
- macOS: `~/Library/Application Support/rs.cpt/`

confy 1.x prepends the `rs.` qualifier to the app dir. Bookmarks live in `bookmarks.toml` at the same path; custom themes in `themes/<name>.toml`; debug log in `cpt.log`.

### UI components

| Component | Role |
|-----------|------|
| `status_bar` | Path prompt, selection total, filter indicator, Ln/Col in editor |
| `command_bar` | F-key hints and text input for filter/rename/mkdir/create |
| `dialog` | Modal dialogs: confirm, conflict, error, scrollable info |
| `bookmark_panel` | Overlay panel: navigate, add, remove bookmarks |
| `drives_panel` | Overlay panel: pick a drive (Win) or mount (`/Volumes`, `/media`, `/mnt`); also auto-opens when `ParentDir` is invoked at the FS root |
| `theme_editor` | Overlay with 256-color picker and live preview |
| `task_window` | Floating live-output window, scrollable, minimizable |

## Conventions

- No em dashes in text or commits (use commas, periods, or hyphens)
- Commit messages: imperative mood, lowercase
- Follow standard Rust conventions (rustfmt, clippy clean)
