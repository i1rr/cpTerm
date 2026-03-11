```
 ██████╗ ██████╗ ████████╗
██╔════╝ ██╔══██╗╚══██╔══╝
██║      ██████╔╝   ██║
██║      ██╔═══╝    ██║
╚██████╗ ██║        ██║
 ╚═════╝ ╚═╝        ╚═╝
```

**A dual-pane TUI file explorer - Total Commander style, built in Rust.**

---

## Powered by

```
         _~^~^~_                     (\(\
       \) /  o o  \ (/               ( -.-)
         '_   -   _'                 o_(")(")
         / '-----' \

       Ferris  (Rust)               rat  (Ratatui)
```

Built on **Rust** (edition 2024) + **Ratatui 0.30** + **Tokio 1**.
Compiled to a single native binary with no runtime dependencies.

---

## Preview

```
╔══[ C:\Users\ivan ]══════════════════╦══[ D:\Projects\cpTerm ]════════════╗
║ Name                  Size   Date   ║ Name                  Size   Date  ║
║ ..                                  ║ ..                                 ║
║ .config\              <DIR>  03-08  ║ src\                  <DIR>  03-11 ║
║ Documents\            <DIR>  03-10  ║ target\               <DIR>  03-11 ║
║ Downloads\            <DIR>  03-11  ║ Cargo.toml           1.4 KB  03-11 ║
║▶notes.txt            4.5 KB  03-09  ║ Cargo.lock          42.1 KB  03-11 ║
║ pictures\             <DIR>  03-07  ║▶README.md              520 B  03-01 ║
║ archive.zip          12.3 MB  03-06 ║ PLAN.md              8.2 KB  03-11 ║
║ budget.xlsx           2.1 KB  03-05 ║ .gitignore             120 B  03-01 ║
║                                     ║                                    ║
╠═════════════════════════════════════╩════════════════════════════════════╣
║ C:\Users\ivan> _                         1 selected  (4.5 KB)            ║
╠══════════════════════════════════════════════════════════════════════════╣
║ F1Help  F2Ren  F4New  F5Copy  F6Move  F8Del  Ctrl+B:Marks  Ctrl+Q:Quit  ║
╚══════════════════════════════════════════════════════════════════════════╝
```

### With bookmark panel open  (`Ctrl+B`)

```
╔══[ C:\Users\ivan ]══════════════════╦══[ D:\Projects\cpTerm ]════════════╗
║ Name                  Size   Date   ║ Name                  Size   Date  ║
║ ..                                  ║ ..                                 ║
║ .config\              <DIR>  03-08  ║ src\                  <DIR>  03-11 ║
║ Documents\            <DIR>  03-10  ║ target\               <DIR>  03-11 ║
║ Downloads\            <DIR>  03-11  ║ Cargo.toml           1.4 KB  03-11 ║
║▶notes.txt            4.5 KB  03-09  ║ Cargo.lock          42.1 KB  03-11 ║
║   ╔═ Bookmarks ══════════════════════════════════════════════╗            ║
║   ║  Projects              D:\Projects                       ║            ║
║   ║▶ cpTerm                D:\Projects\cpTerm               ║            ║
║   ║  Home                  C:\Users\ivan                     ║            ║
║   ║  Downloads             C:\Users\ivan\Downloads           ║            ║
║   ║  (not found)           C:\old\archive  (dimmed)          ║            ║
║   ╠══════════════════════════════════════════════════════════╣            ║
║   ║  Enter:Go  Ctrl+D:Add  Shift+Del:Remove  Esc:Close       ║            ║
║   ╚══════════════════════════════════════════════════════════╝            ║
╠══════════════════════════════════════════════════════════════════════════╣
║ F1Help  F2Ren  F4New  F5Copy  F6Move  F8Del  Ctrl+B:Marks  Ctrl+Q:Quit  ║
╚══════════════════════════════════════════════════════════════════════════╝
```

### With async task output window

```
╔══[ C:\Users\ivan ]══════════════════╦══[ D:\Projects\cpTerm ]════════════╗
║ ...                                 ║ ...                                ║
║   ╔═ Task: cargo build ─────────────────────────────────────╗            ║
║   ║  Compiling ratatui v0.30.0                              ║            ║
║   ║  Compiling crossterm v0.29.0                            ║            ║
║   ║  Compiling tokio v1.43.0                                ║            ║
║   ║  Compiling cpt v0.1.0 (D:\Projects\cpTerm)             ║            ║
║   ║   Finished `dev` profile in 4.21s                       ║            ║
║   ║                                                         ║            ║
║   ║  [exit 0]                                               ║            ║
║   ╠═════════════════════════════════════════════════════════╣            ║
║   ║  Up/Down:Scroll  Ctrl+Z:Minimize  Esc:Close             ║            ║
║   ╚═════════════════════════════════════════════════════════╝            ║
╠══════════════════════════════════════════════════════════════════════════╣
║  [task done]         Ctrl+Z to restore                                   ║
╠══════════════════════════════════════════════════════════════════════════╣
║ F1Help  F2Ren  F4New  F5Copy  F6Move  F8Del  Ctrl+B:Marks  Ctrl+Q:Quit  ║
╚══════════════════════════════════════════════════════════════════════════╝
```

---

## Features

### Dual-pane navigation
- Two independent directory listings, side by side
- Arrow key navigation - no modal keybindings
- Hidden files always visible, shown dimmed (`.dotfiles`)
- Details view: Name, Size, Date Modified
- **Session restore** - reopens your last two directories and active pane on launch

### File operations
- **Copy (F5)** - to the other pane, with conflict resolution (overwrite / auto-rename)
- **Move (F6)** - same conflict resolution as copy
- **Rename (F2)** - inline text input, confirm with Enter
- **Delete (F8 / Del)** - confirmation dialog
- **Create (F4)** - prompts for file (`f`) or directory (`d`)
- **Multi-select** - Space / Insert toggles; Ctrl+A toggles all
- **Selection total** - status bar shows the combined size of selected items

### Quick filter  (`Ctrl+F`)
- Instant substring filter on the active pane
- Case-insensitive; Esc clears and restores the full listing

### Bookmarks  (`Ctrl+B`)
- Floating panel overlay listing saved directories
- **Ctrl+D** - add the current directory (pre-fills the folder name, editable)
- **Shift+Del** - remove the selected bookmark
- **Enter** - navigate the active pane to the bookmark
- Paths that no longer exist are shown dimmed
- Persisted to `bookmarks.toml` between sessions

### Theme editor  (`Ctrl+T`)
- 25 individually configurable color fields covering every UI element
- Interactive **256-color picker** - navigate the 16x16 grid with live preview
- **Tab** - cycle through built-in and saved custom themes
- **n** - save current state as a new named theme
- **F2** - overwrite the current custom theme
- **e** - save and open the `.toml` in your external editor (`$EDITOR`)
- **Del** - delete a custom theme
- Two built-in themes: `default` (blue/cyan) and `muted` (low contrast)

### Async task runner
- Shell commands run as background Tokio tasks - the UI never blocks
- Live output streams line by line into a floating window
- **Ctrl+Z** - minimize the window; the command bar shows `[task running]`
- **Ctrl+Z** again (from Normal mode) - restore the window
- Window is scrollable; output capped at 5000 lines
- One task at a time; starting a new command while one runs shows the existing window

### Command line
- Type any printable character from Normal mode to open the inline prompt
- `cd <path>` - navigate the active pane
- Anything else is passed to the system shell (`cmd /C` on Windows, `sh -c` on Unix)
- Output shown live in the task output window

### File viewer / editor
- **Enter** on a file - suspends the TUI and opens `$EDITOR`
- **F3** - opens in `$PAGER` (read-only)
- **Shift+Enter** - opens with the OS default application (detached)
- Editor fallback chain: `$EDITOR` -> `$VISUAL` -> `nano` / `vi` (Unix) or `notepad` (Windows)

---

## Keybindings

### Navigation

| Key | Action |
|-----|--------|
| `Up` / `Down` | Move cursor |
| `Home` / `End` | Top / bottom of list |
| `PgUp` / `PgDn` | Page scroll |
| `Enter` | Enter directory or open file in `$EDITOR` |
| `Backspace` | Go to parent directory |
| `Tab` | Switch active pane |
| `Ctrl+R` | Refresh active pane |

### Selection

| Key | Action |
|-----|--------|
| `Space` / `Insert` | Toggle selection and move down |
| `Ctrl+A` | Select all / deselect all (toggle) |

### File operations

| Key | Action |
|-----|--------|
| `F2` | Rename |
| `F4` | Create new file or directory |
| `F5` | Copy to other pane |
| `F6` | Move to other pane |
| `F8` / `Delete` | Delete (with confirmation) |

### File viewing

| Key | Action |
|-----|--------|
| `Enter` | Open in `$EDITOR` (TUI suspends until editor exits) |
| `F3` | Open in `$PAGER` / `less` (read-only) |
| `Shift+Enter` | Open with OS default application |

### Overlays & panels

| Key | Action |
|-----|--------|
| `F1` | Help dialog |
| `Ctrl+F` | Quick filter |
| `Ctrl+B` | Bookmarks panel |
| `Ctrl+T` | Theme editor |
| `Esc` | Cancel / close / dismiss |

### Bookmark panel

| Key | Action |
|-----|--------|
| `Up` / `Down` | Navigate list |
| `Enter` | Go to selected bookmark |
| `Ctrl+D` | Add current directory |
| `Shift+Del` | Remove selected bookmark |
| `Esc` | Close panel |

### Task output window

| Key | Action |
|-----|--------|
| `Up` / `Down` | Scroll output |
| `Ctrl+Z` | Minimize to command bar |
| `Ctrl+Z` (minimized) | Restore window |
| `Esc` / `Enter` | Close (or minimize if still running) |

### General

| Key | Action |
|-----|--------|
| `Ctrl+Q` | Quit |
| any printable char | Open command line |

---

## Build & run

**Requires:** Rust stable (edition 2024), a 256-color terminal.

```sh
# Build release binary
cargo build --release

# Run - restores last session
cargo run

# Set left pane path
cargo run -- /path/to/dir

# Set both panes
cargo run -- /left /right
```

For development:

```sh
cargo test      # run all tests
cargo clippy    # lint
cargo fmt       # format
```

---

## Configuration & storage

All files are managed by [`confy`](https://crates.io/crates/confy) and stored per platform:

| Platform | Base path |
|----------|-----------|
| Windows | `%APPDATA%\cpt\` |
| Linux | `~/.config/cpt/` |
| macOS | `~/Library/Application Support/cpt/` |

| File | Contents |
|------|----------|
| `cpt.toml` | Last directories, active pane, theme name |
| `bookmarks.toml` | Saved bookmark entries |
| `themes/<name>.toml` | Custom color themes (25 fields each) |

---

## Project layout

```
src/
  main.rs               entry point, CLI parsing
  app.rs                App struct, event loop, all dispatch logic
  action.rs             Action enum - the app's message bus
  event.rs              crossterm events + tick timer
  cli.rs                clap argument definitions
  config.rs             session persistence
  tui.rs                terminal init / restore
  util.rs               format_size(), format_date(), path helpers
  logger.rs             in-memory session logger
  theme.rs              Theme struct, 25 fields, built-in themes, file I/O
  bookmarks.rs          BookmarkList, load/save, add/remove
  task.rs               TaskState, async shell runner, line streaming
  components/
    explorer.rs         single pane: listing, navigation, selection, filter
    dual_pane.rs        two Explorer instances, focus, cross-pane operations
    status_bar.rs       path prompt, selection info, filter indicator
    command_bar.rs      F-key hints, text input for filter/rename/mkdir
    dialog.rs           modal dialogs: confirm, conflict, error, info
    theme_editor.rs     theme editor overlay + 256-color picker
    bookmark_panel.rs   bookmark panel overlay
    task_window.rs      live output window
  fs/
    entry.rs            FileEntry struct, directory reading, sorting
    ops.rs              async copy/move/delete, conflict detection
    open.rs             OS-default file opener wrapper
tests/
  unit_tests.rs         unit and integration tests
```

---

## Stack

| Crate | Role |
|-------|------|
| [ratatui](https://ratatui.rs) 0.30 | TUI rendering |
| [crossterm](https://github.com/crossterm-rs/crossterm) 0.29 | Terminal I/O, key events |
| [tokio](https://tokio.rs) 1 | Async runtime, background tasks |
| [tokio-util](https://docs.rs/tokio-util) 0.7 | Async line reader |
| [clap](https://docs.rs/clap) 4 | CLI argument parsing |
| [confy](https://docs.rs/confy) 0.6 | Config file persistence |
| [serde](https://serde.rs) 1 | TOML serialization |
| [open](https://docs.rs/open) 5 | OS-default file/app opener |
| [chrono](https://docs.rs/chrono) 0.4 | Date/time formatting |

---

## License

MIT
