mod action;
mod app;
mod cli;
mod components;
mod config;
mod event;
mod fs;
mod logger;
mod task;
mod theme;
mod tui;
mod util;

use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::Result;

use crate::cli::Cli;
use crate::config::SessionConfig;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    logger::init();

    let args = Cli::parse();
    let session = SessionConfig::load();

    let left_dir = args
        .path
        .map(|p| canonicalize_or(p))
        .unwrap_or(session.left_dir);

    let right_dir = args
        .right
        .map(|p| canonicalize_or(p))
        .unwrap_or(session.right_dir);

    let active = session.active_pane;
    let theme_name = session.theme_name;

    let mut app = app::App::new(left_dir, right_dir, active, theme_name);
    app.run().await?;

    logger::dump();

    Ok(())
}

fn canonicalize_or(path: PathBuf) -> PathBuf {
    util::clean_canonicalize(&path).unwrap_or(path)
}
