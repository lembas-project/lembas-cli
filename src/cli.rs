//! CLI argument parsing via clap.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lembas")]
#[command(about = "Lembas CLI - Lifecycle Engineering Model-Based Analysis System")]
pub struct Cli {
    #[command(subcommand)]
    pub command: SelfCommands,
}

#[derive(Subcommand)]
pub enum SelfCommands {
    /// Manage the lembas CLI itself
    #[command(name = "self")]
    SelfCmd {
        #[command(subcommand)]
        command: SelfSubcommands,
    },
}

#[derive(Subcommand)]
pub enum SelfSubcommands {
    /// Update the lembas CLI to the latest version
    Update {
        /// Version to install (e.g., v2026.7.1)
        version: Option<String>,

        /// Check if an update is available
        #[arg(long, conflicts_with_all = ["list", "version", "force"])]
        check: bool,

        /// List available versions
        #[arg(long, conflicts_with_all = ["check", "version", "force"])]
        list: bool,

        /// Force reinstall even if already on target version
        #[arg(long, conflicts_with_all = ["check", "list"])]
        force: bool,
    },
}
