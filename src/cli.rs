//! CLI argument parsing via clap.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lembas")]
#[command(about = "Lembas CLI - Lifecycle Engineering Model-Based Analysis System")]
#[command(disable_help_flag = true, disable_version_flag = true)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Show help (delegated to Python runtime)
    #[arg(long, short)]
    pub help: bool,

    /// Show version
    #[arg(long)]
    pub version: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage the lembas CLI itself
    #[command(name = "self")]
    SelfCmd {
        #[command(subcommand)]
        command: SelfSubcommands,
    },

    /// Delegate to Python lembas runtime
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
pub enum SelfSubcommands {
    /// Update the lembas CLI to the latest version
    Update(UpdateArgs),
}

#[derive(Parser)]
#[command(args_conflicts_with_subcommands = true)]
pub struct UpdateArgs {
    #[command(subcommand)]
    pub action: Option<UpdateAction>,

    /// Version to install (e.g., v2026.7.1)
    pub version: Option<String>,

    /// Force reinstall even if already on target version
    #[arg(long)]
    pub force: bool,

    /// Skip signature verification (DANGEROUS - use only if verification is broken)
    #[arg(long)]
    pub skip_verify: bool,

    /// Print help
    #[arg(short, long)]
    pub help: bool,
}

#[derive(Subcommand)]
pub enum UpdateAction {
    /// Check if an update is available
    Check,
    /// List available versions
    List,
}

impl UpdateArgs {
    pub fn print_help() {
        tracing::info!("Update the lembas CLI to the latest version");
        tracing::info!("");
        tracing::info!("Usage: lembas self update [OPTIONS] [VERSION]");
        tracing::info!("       lembas self update <COMMAND>");
        tracing::info!("");
        tracing::info!("Commands:");
        tracing::info!("  check  Check if an update is available");
        tracing::info!("  list   List available versions");
        tracing::info!("");
        tracing::info!("Arguments:");
        tracing::info!("  [VERSION]  Version to install (e.g., v2026.7.1)");
        tracing::info!("");
        tracing::info!("Options:");
        tracing::info!("      --force        Force reinstall even if already on target version");
        tracing::info!("      --skip-verify  Skip signature verification (DANGEROUS)");
        tracing::info!("  -h, --help         Print help");
    }
}
