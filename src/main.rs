mod app;
mod config;
mod converter;
mod recorder;
mod shortcuts;
mod tray;
mod types;

use anyhow::Result;

fn main() -> Result<()> {
    let exit_code = app::run();
    std::process::exit(exit_code);
}
