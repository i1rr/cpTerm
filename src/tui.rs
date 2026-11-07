use std::io::{self, stdout};

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, prelude::CrosstermBackend};

pub type Tui = Terminal<CrosstermBackend<io::Stdout>>;

/// Restore the terminal on early return or unwinding, including partial setup.
pub struct RestoreGuard;
impl Drop for RestoreGuard {
    fn drop(&mut self) {
        let _ = restore();
    }
}

pub fn init() -> io::Result<(Tui, RestoreGuard)> {
    enable_raw_mode()?;
    let guard = RestoreGuard;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    Ok((Terminal::new(backend)?, guard))
}

pub fn restore() -> io::Result<()> {
    let raw = disable_raw_mode();
    let screen = execute!(stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
    raw.and(screen)
}

/// Temporarily suspend the TUI so an external program can use the terminal.
pub fn suspend() -> io::Result<()> {
    restore()
}

/// Resume the TUI after a call to `suspend()`.
pub fn resume() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    Ok(())
}
