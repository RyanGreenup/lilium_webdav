use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "webdav_server")]
#[command(about = "A WebDAV server for SQLite-backed notes", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the WebDAV server
    Serve(ServeArgs),
}

/// Arguments for the serve command
#[derive(Parser)]
pub struct ServeArgs {
    /// Path to the SQLite database
    #[arg(short, long)]
    pub database: PathBuf,

    /// Host address to bind to
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to listen on
    #[arg(short, long, default_value = "4918")]
    pub port: u16,

    /// Login username for Basic Auth
    #[arg(short, long)]
    pub username: String,

    /// Password for Basic Auth
    #[arg(short = 'P', long)]
    pub password: String,

    /// User ID in the database (defaults to username if not specified)
    #[arg(long)]
    pub user_id: Option<String>,
}
