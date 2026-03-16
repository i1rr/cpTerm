# cpt - dual-pane TUI file explorer

## Stack
Rust, Ratatui 0.30, ratatui-textarea 0.8, Crossterm 0.29, Tokio 1, tokio-util 0.7

## Commands
- Build: `cargo build`
- Run: `cargo run`
- Run with args: `cargo run -- <left-path> [right-path]`
- Test: `cargo test`
- Lint: `cargo clippy`
- Format: `cargo fmt`

## Conventions
- No em dashes in text (use hyphens or commas instead)
- Do not mention AI/assistant tooling in commits or code comments
- No Co-Authored-By lines in commits
- Commit style: imperative mood, lowercase, no em dashes
- Code style: follow standard Rust conventions (rustfmt, clippy)
- Architecture: component-based (see PLAN.md for details)
- Rust edition 2024

## Git
- Remote uses SSH alias `github-i1rr` (i1rr account)
- Push: `git push origin master`
