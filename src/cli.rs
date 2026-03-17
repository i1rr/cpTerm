use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "cpt", about = "Dual-pane TUI file explorer")]
pub struct Cli {
    /// Directory for the left pane
    pub path: Option<PathBuf>,

    /// Directory for the right pane
    pub right: Option<PathBuf>,

    /// Enable debug logging to file (writes to cpt.log)
    #[arg(long)]
    pub debug: bool,
}
