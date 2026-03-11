# cpt - Dual-Pane TUI File Explorer

## Context

Minimal dual-pane file explorer (Total Commander / DOS Navigator style).
Stack: Rust + Ratatui. Simple, functional v1 that's architecturally scalable.

## User Preferences
- Arrow keys only (no vim keybindings)
- Single "details" view: 3 columns - Name (with extension), Size, Date Modified
- File size totals for selected items in status bar
- Quick filter (Ctrl+F) to filter current pane
- `cpt` restores session, `cpt <path>` sets left pane, `--right <path>` sets right pane
- Rust edition 2024

## Project Structure

```
src/
  main.rs              - entry point, CLI parsing, bootstrap
  app.rs               - App struct, main event loop, action dispatch
  tui.rs               - terminal init/restore (raw mode, alt screen)
  action.rs            - Action enum, all app messages
  event.rs             - event source: crossterm events + tick timer
  cli.rs               - clap CLI definitions
  config.rs            - session persistence via confy
  util.rs              - format_size(), format_date(), path helpers
  components/
    mod.rs             - Component trait
    explorer.rs        - single pane: file listing, navigation, selection, filter
    dual_pane.rs       - two Explorer instances, focus switching, cross-pane ops
    status_bar.rs      - path, selection count, total size of selected
    command_bar.rs     - F-key hints / text input for filter, mkdir, rename
    dialog.rs          - modal dialogs: confirm copy/delete, errors
  fs/
    mod.rs             - re-exports
    entry.rs           - FileEntry struct, sorting
    ops.rs             - async copy/move/delete with progress
    open.rs            - open::that() wrapper
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
+---------------------------------------------------------------------------+
 C:\Users\ivan | 2 selected | 5.7 KB total
 F2Ren F5Copy F6Move F7Mkdir F8Del  Ctrl+F:Filter  Tab:Switch  q:Quit
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
| Ctrl+A | Select all |
| * | Invert selection |
| F2 | Rename |
| F5 | Copy to other pane |
| F6 | Move to other pane |
| F7 | Create directory |
| F8 / Delete | Delete |
| Ctrl+F | Quick filter |
| Esc | Cancel filter / dismiss dialog |
| Ctrl+R | Refresh |
| Ctrl+H | Toggle hidden files |
| q / Ctrl+Q | Quit |
