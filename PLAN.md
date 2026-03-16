# cpt - Dual-Pane TUI File Explorer

## Context

Minimal dual-pane file explorer (Total Commander / DOS Navigator style).
Stack: Rust + Ratatui 0.30 + Crossterm 0.29 + Tokio 1. Edition 2024.

## User Preferences
- Arrow keys only (no vim keybindings)
- Hidden files always visible (dimmed with DIM modifier)
- Single "details" view: 3 columns - Name (with extension), Size, Date Modified
- File size totals for selected items in status bar
- Quick filter (Ctrl+F) to filter current pane
- `cpt` restores session, `cpt <path>` sets left pane, `--right <path>` sets right pane
- No em dashes in any text
- No Co-Authored-By lines in commits

## Project Structure

```
src/
  main.rs              - entry point, CLI parsing, bootstrap
  lib.rs               - public module re-exports for tests
  app.rs               - App struct, main event loop, action dispatch, input modes
  tui.rs               - terminal init/restore; suspend()/resume() for editor handoff
  action.rs            - Action enum, all app messages
  event.rs             - event source: crossterm events + tick timer
  cli.rs               - clap CLI definitions
  config.rs            - session persistence via confy
  util.rs              - format_size(), format_date(), path helpers, UNC prefix stripping,
                         is_archive(), find_sevenzip()
  logger.rs            - in-memory session logger, dumps to stderr and log file on quit
  task.rs              - TaskState, run_task() async task runner with live output streaming
  bookmarks.rs         - BookmarkList struct, load/save, add/remove
  components/
    mod.rs             - Component trait
    explorer.rs        - single pane: file listing, navigation, selection, filter
    dual_pane.rs       - PaneContent enum (Explorer | Box<EditorPane>), focus switching,
                         cross-pane ops; open_editor_in_active / close_editor_in_active
    editor_pane.rs     - EditorPane: embedded text editor using ratatui-textarea; load,
                         save, key handling; TextArea<'static> with line numbers
    status_bar.rs      - command prompt (path> cmd), selection info, filter status,
                         Ln/Col display when editor is active
    command_bar.rs     - F-key hints / text input for filter, mkdir, rename, create file
    dialog.rs          - modal dialogs: confirm, conflict, error, info (scrollable)
    theme_editor.rs    - interactive theme editor overlay with live preview
    bookmark_panel.rs  - bookmark panel overlay: list, navigate, add, remove
    task_window.rs     - floating task output window with live-streaming lines
  fs/
    mod.rs             - re-exports
    entry.rs           - FileEntry struct, sorting
    ops.rs             - async copy/move/delete with conflict detection and auto-rename
    open.rs            - open_file(), open_in_editor(), open_in_viewer(), editor/pager
                         resolution ($EDITOR/$VISUAL/nano/vi on Unix, notepad on Windows)
                         open_in_editor kept for tests; embedded editor is now primary
    archive.rs         - shell_quote(), resolve_unpack_command() per format/OS tool dispatch
tests/
  unit_tests.rs        - 149+ unit/integration tests
```

## UI Layout

```
+--[ C:\Users\ivan ]----------------++-- D:\Projects -----------------+
| Name              Size    Date    || Name              Size    Date  |
| ..                                || ..                              |
| Documents\        <DIR>  03-10   || src\              <DIR>  03-11  |
| Downloads\        <DIR>  03-11   || Cargo.toml       1.2 KB  03-11  |
| notes.txt        4.5 KB  03-09   || README.md          520 B 03-01  |
|                                   ||                                 |
+-------------------------------------+---------------------------------+
 C:\Users\ivan> _
 F1Help F2Ren F4New F5Copy F6Move F7Mkdir F8Del  Ctrl+F:Filter  Tab:Switch  q:Quit
```

## Keybindings

| Key | Action |
|-----|--------|
| Up/Down | Move cursor |
| Home/End | Top/bottom of list |
| PageUp/PageDown | Page scroll |
| Enter | Enter directory / open file in embedded in-pane editor |
| F3 | View file in pager ($PAGER / less, read-only, TUI suspends) |
| Shift+Enter | Open file with OS default application (detached) |
| Backspace | Parent directory |
| Tab | Switch pane |
| Space / Insert | Toggle select + move down |
| Ctrl+A | Select/deselect all (toggle) |
| F1 | Help dialog (scrollable) |
| F2 | Rename |
| F4 | Create file |
| F5 | Copy to other pane (with conflict resolution) |
| F6 | Move to other pane (with conflict resolution) |
| F7 | Create directory |
| F8 / Delete | Delete |
| Ctrl+F | Quick filter |
| Ctrl+B | Open bookmarks panel |
| Ctrl+T | Open theme editor |
| Esc | Cancel filter / dismiss dialog |
| Ctrl+R | Refresh |
| q / Ctrl+Q | Quit |
| Any other char | DOS Navigator-style command line (cd, shell commands) |

## Cross-Platform Support

### Archive Unpacking Tool Matrix

| Format | Linux / macOS | Windows 10+ |
|--------|---------------|-------------|
| .zip | unzip (fallback: 7z/7za/7zz) | tar (built-in since Win10) |
| .tar, .tar.gz, .tgz, .tar.bz2, .tbz2, .tar.xz | tar (built-in) | tar (built-in) |
| .7z | 7z / 7za / 7zz | 7z / 7za / 7zz |
| .rar | unrar (fallback: 7z) | unrar (fallback: 7z) |
| .gz (standalone) | gzip -dc (fallback: 7z) | 7z |
| .bz2 (standalone) | bzip2 -dc (fallback: 7z) | 7z |
| .xz (standalone) | xz -dc (fallback: 7z) | 7z |

Archive tool detection is done at command-resolution time (not at startup). If the preferred
tool is not found, the next best option is tried. A clear error dialog is shown if no tool
is available, suggesting the package manager install command.

### Editor / Pager Defaults

The embedded editor (Enter on a text file) runs directly inside the active pane - no
TUI suspend needed. The external pager (F3) still suspends the TUI:

| Platform | Pager chain |
|----------|-------------|
| Linux / macOS | $PAGER -> less |
| Windows | $PAGER -> more |

`resolve_editor()` and `open_in_editor()` are retained in `src/fs/open.rs` for test
coverage but are not used by the main binary at runtime.

### Path Handling

- **Windows**: UNC prefixes (`\\?\`) are stripped after canonicalization so they don't
  appear in the path bar. Hidden files use `FILE_ATTRIBUTE_HIDDEN` on Windows; dot-prefix
  detection is used on Unix.
- **Shell quoting**: commands sent to `sh -c` (Unix) use single-quote wrapping with `'\''`
  escaping. Commands sent to `cmd /C` (Windows) use double-quote wrapping.

### Hidden Files

- **Unix**: files starting with `.` are marked `is_hidden = true` and rendered with DIM modifier.
- **Windows**: files with `FILE_ATTRIBUTE_HIDDEN` set are marked hidden. Dot-prefix is also
  checked for portability.

## Completed Features

### Async Task Runner (done)
- `src/task.rs`: `TaskState` struct (lines capped at `MAX_LINES`, auto-scroll, scroll up/down)
- `run_task(cmd, cwd, tx)`: spawns via `sh -c` / `cmd /C`, streams stdout+stderr line-by-line
- `src/components/task_window.rs`: floating overlay with live output, minimize, scroll
- `InputMode::TaskOutput` variant, indicator in command bar when minimized

### Embedded In-Pane Editor (done)
- Enter on a text file replaces the active pane with `EditorPane` (other pane stays live)
- `EditorPane` wraps `ratatui-textarea::TextArea<'static>` - line numbers, undo/redo, search
- Ctrl+S saves; Ctrl+Q / Esc closes (confirm dialog if unsaved changes)
- Tab switches to the other pane without closing the editor
- `Action::OpenEditor { path }` carries the path; `CloseEditor`, `SaveEditor`,
  `EditorKeyInput(KeyEvent)` round out the new action variants
- `PaneContent` enum in `dual_pane.rs` lets each slot hold Explorer or Box<EditorPane>
- Status bar shows `Ln X, Col Y` when editor is active
- F3: suspend TUI, launch `$PAGER` (fallback: less / more), resume
- Shift+Enter: `open::that()` (OS-default, detached, no terminal takeover)
- `tui::suspend()` / `tui::resume()` helpers in `src/tui.rs` still used for F3
- `resolve_pager()`, `open_in_viewer()` in `src/fs/open.rs`

### Archive Unpack (done)
- Enter on an archive triggers `Action::UnpackArchive`
- `resolve_unpack_command(archive, dest)` in `src/fs/archive.rs` selects the best available
  tool for each format and OS (see tool matrix above)
- Extraction runs as an async background task with live output in the task window
- Destination: the inactive pane's current directory
- Supported formats: .zip, .tar, .tar.gz, .tgz, .tar.bz2, .tbz2, .tar.xz, .7z, .rar,
  .gz, .bz2, .xz

### Archive Pack (pending)
- Trigger: dedicated key on selected files (e.g. Alt+F5 or a menu)
- Prompt for archive name and format (zip, 7z)
- Run via `7z a <archive> <files...>` as background task
- Show packing progress in the task output window
- rar packing intentionally excluded (proprietary format)

### Bookmarks (done)
- `src/bookmarks.rs`: `BookmarkEntry { name, path }`, load/save via confy/TOML
- `src/components/bookmark_panel.rs`: panel overlay, navigate, add current dir, remove
- Ctrl+B opens the bookmark panel
- Bookmarked paths that no longer exist are shown dimmed; Enter shows error
- Persisted alongside session config across restarts

### Theme Manager (done)
- `src/theme.rs`: Theme struct with 25 color fields, 2 built-in themes, file-based storage
- `src/components/theme_editor.rs`: interactive editor with 256-color picker, live preview
- Ctrl+T opens theme editor:
  - Enter: 16x16 color picker (all 256 terminal colors, live preview as you navigate)
  - Tab: cycle base theme (built-in + custom), n: save as new theme
  - F2: save (overwrites custom, prompts name for built-in), Del: delete custom theme
  - e: save and open .toml in external editor, Esc: close
- Themes stored in `$XDG_CONFIG_HOME/cpt/themes/{name}.toml` (Linux),
  `%APPDATA%/cpt/themes/{name}.toml` (Windows), `~/Library/Application Support/cpt/themes/` (macOS)
- All components (explorer, status bar, command bar, dialogs) use Theme
