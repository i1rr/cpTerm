# CLAUDE.md

Guidance for Claude Code working in this repository.

## cpt - dual-pane TUI file explorer

Total Commander style file manager. Single binary, no runtime dependencies.

## Stack

Rust edition 2024, Ratatui 0.30, ratatui-textarea 0.8, Crossterm 0.29, Tokio 1, tokio-util 0.7, confy 1.x, russh 0.58.

## Commands

- `cargo build` / `cargo run` / `cargo run -- <left-path> [right-path]`
- `cargo run -- --debug` writes `cpt.log` to the confy config dir
- `cargo test` (all) / `cargo test <name>` (single)
- `cargo clippy` / `cargo fmt`

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
