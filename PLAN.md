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
| Enter | Enter directory / open file |
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
| Esc | Cancel filter / dismiss dialog |
| Ctrl+R | Refresh |
| q / Ctrl+Q | Quit |
| Any other char | DOS Navigator-style command line (cd, shell commands) |

## TODO

### Theme Manager
- Add a ThemeManager component (src/theme.rs) that defines all UI colors in one place
- Current hardcoded colors to extract:
  - Explorer: focused border (Cyan), unfocused border (DarkGray), dir (Blue+Bold),
    hidden (DIM), selected (Yellow bg/Black fg), cursor (Reversed)
  - Status bar: background (DarkGray), path prompt (Cyan), selection info (Yellow),
    filter text (Green)
  - Command bar: F-key labels (Black on Cyan), hints (Cyan), input prompts (Yellow/Green)
  - Dialogs: confirm border (Yellow), conflict border (LightRed), error (Red+Bold),
    info (Green)
- Support loading theme from a config file (toml or json alongside session config)
- Allow switching themes at runtime or via config
- Ship with at least 2 built-in themes: default (current colors) and a dark/muted variant
