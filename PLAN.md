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
  tui.rs               - terminal init/restore (raw mode, alt screen)
  action.rs            - Action enum, all app messages
  event.rs             - event source: crossterm events + tick timer
  cli.rs               - clap CLI definitions
  config.rs            - session persistence via confy
  util.rs              - format_size(), format_date(), path helpers, UNC prefix stripping
  logger.rs            - in-memory session logger, dumps to stderr and log file on quit
  components/
    mod.rs             - Component trait
    explorer.rs        - single pane: file listing, navigation, selection, filter
    dual_pane.rs       - two Explorer instances, focus switching, cross-pane ops
    status_bar.rs      - command prompt (path> cmd), selection info, filter status
    command_bar.rs     - F-key hints / text input for filter, mkdir, rename, create file
    dialog.rs          - modal dialogs: confirm, conflict, error, info (scrollable)
    theme_editor.rs    - interactive theme editor overlay with live preview
  fs/
    mod.rs             - re-exports
    entry.rs           - FileEntry struct, sorting
    ops.rs             - async copy/move/delete with conflict detection and auto-rename
    open.rs            - open::that() wrapper
tests/
  unit_tests.rs        - 67 unit/integration tests
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
| Enter | Enter directory / open file in internal editor |
| F3 | View file (internal, read-only) |
| Shift+Enter | Open file with external editor ($EDITOR / system default) |
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

## TODO

### Async Task Runner with Live Output
Background long-running commands instead of blocking the UI.

**Current behavior:** shell commands block the app until complete, then show result in an info dialog.

**Goal:** run commands asynchronously, show live output in a floating window, allow minimizing to continue working.

- Spawn shell commands on a background Tokio task, capture stdout/stderr line-by-line
- Show a floating "Task Output" window with live-streaming lines (auto-scroll to bottom)
- Keybinding to minimize the window (e.g. Ctrl+Z or a key shown in the window chrome)
  - When minimized, show a small indicator in the command bar area (e.g. `[task running...]`)
  - When the task completes while minimized, update indicator to `[task done - press X to view]`
- When not minimized, output streams live; Esc closes the window (same as current info dialog)
- Only one background task at a time (for now) - if a task is running, block new command submission or queue it
- New InputMode variant: `TaskOutput { minimized: bool }`
- Stream capture: use `tokio::process::Command` with piped stdout/stderr, read lines via `BufReader::lines()`
- Store output lines in a `Vec<String>` (capped, e.g. 5000 lines) shared between task and UI via `Arc<Mutex<>>` or a channel
- The task window should be scrollable (reuse dialog scroll logic)

### Archive Pack/Unpack
Support packing and unpacking common archive formats.

**Formats:** .zip, .7z, .rar (unpack only for rar - no native Rust rar packer)

**Unpack:**
- Trigger: Enter on an archive file, or a dedicated key/menu option
- Prompt for destination (default: other pane's current directory)
- Run as async background task (uses the task runner above) so we get live progress
- Use external tools: `7z` (7-Zip CLI) as the universal backend
  - 7z supports zip, 7z, rar, tar, gz, and more
  - Fall back to Rust-native crates if 7z not found? Or just require 7z on PATH
- Show extraction progress in the task output window

**Pack:**
- Trigger: dedicated key on selected files (e.g. Alt+F5 or a menu)
- Prompt for archive name and format (zip, 7z)
- Run via `7z a <archive> <files...>` as background task
- Show packing progress in the task output window

**Implementation notes:**
- Detect `7z` on PATH at startup, store availability in App state
- If 7z not available, show error dialog suggesting installation
- Could later add Rust-native zip support via `zip` crate as fallback
- rar packing intentionally excluded (proprietary format)

### Internal Text Editor / File Viewer
Edit text files inside the TUI without leaving the app.

**Keybindings:**
- Enter on a file: open in internal editor (edit mode)
- F3: open in internal viewer (read-only, like `less`)
- Shift+Enter: open with external editor/app (bypass internal editor)

**Internal editor approach:**
- Suspend the TUI (restore terminal), spawn `$EDITOR` (or fallback chain: nano, vi, notepad on Windows) as a child process, wait for exit, then re-init the TUI
- This is the classic approach used by Midnight Commander, ranger, etc. - no need to build an editor from scratch
- Fallback chain: `$EDITOR` -> `$VISUAL` -> `nano` -> `vi` (Unix) / `notepad` (Windows)
- Runs synchronously (editor takes over the terminal) - this is expected and correct

**External open (Shift+Enter):**
- Use `open::that()` (already in the project) to open with OS-default app
- This launches the app detached, no terminal takeover
- Useful for images, PDFs, binary files, or when user prefers VS Code / GUI editors

**F3 viewer:**
- Same suspend-and-spawn approach but with a pager: `$PAGER` -> `less` -> `more`
- Read-only viewing, no edit risk

**Implementation notes:**
- Add `suspend_tui()` and `resume_tui()` helpers in tui.rs (leave alt screen, disable raw mode, then reverse)
- Detect Shift+Enter: crossterm reports this as `KeyCode::Enter` with `KeyModifiers::SHIFT`
- F3 is currently unused, good fit for viewer (matches Total Commander convention)
- Enter already handles directories - just add the file branch to open editor

### Bookmarks
Quick-access directory bookmarks, persisted across sessions.

**Keybinding:** Ctrl+B opens the bookmark panel.

**Bookmark panel (floating overlay, like theme editor):**
- Lists all saved bookmarks: name + path
- Enter on a bookmark: navigate the active pane to that path, close panel
- Ctrl+D (or Insert): add the active pane's current directory as a new bookmark
  - Defaults the name to the folder name, user can edit before confirming
- Shift+Del: remove the selected bookmark (two-key combo to prevent accidents)
- Up/Down to navigate, Esc to close

**Data model:**
- `BookmarkEntry { name: String, path: PathBuf }`
- Stored in a bookmarks.toml file alongside session config (via confy or direct toml)
- Loaded on startup, saved on any change

**Implementation:**
- src/components/bookmark_panel.rs - panel UI, navigation, add/remove
- src/bookmarks.rs - BookmarkList struct, load/save, add/remove
- Add `Ctrl+B` keybinding, `OpenBookmarks` action, bookmark panel state in App
- Bookmark name input: same pattern as theme editor naming (inline text input)
- If a bookmarked path no longer exists, show it dimmed; Enter shows error

### Theme Manager (done)
- src/theme.rs: Theme struct with 25 color fields, 2 built-in themes, file-based storage
- src/components/theme_editor.rs: interactive editor with 256-color picker, live preview
- Ctrl+T opens theme editor:
  - Enter: 16x16 color picker (all 256 terminal colors, live preview as you navigate)
  - Tab: cycle base theme (built-in + custom), n: save as new theme
  - F2: save (overwrites custom, prompts name for built-in), Del: delete custom theme
  - e: save and open .toml in external editor, Esc: close
- Themes stored in %APPDATA%/cpt/themes/{name}.toml, custom themes override built-in
- All components (explorer, status bar, command bar, dialogs) use Theme
