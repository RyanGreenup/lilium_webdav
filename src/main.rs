mod cli;
mod commands;
mod webdav;

use clap::Parser;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    commands::execute(cli.command)
}
