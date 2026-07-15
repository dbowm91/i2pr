//! CLI vocabulary and command execution boundary.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Minimal foreground CLI for the workspace bootstrap.
#[derive(Debug, Parser)]
#[command(
    name = "i2pr",
    version,
    about = "Experimental I2P router workspace (runtime not implemented)"
)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Commands with a defined bootstrap output contract.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Parse and semantically validate a configuration without side effects.
    CheckConfig(CheckConfigArgs),
    /// Explicitly create or inspect the local router identity.
    Identity {
        /// Nested identity lifecycle command.
        #[command(subcommand)]
        command: IdentityCommand,
    },
    /// Validate configuration and optionally perform the future daemon startup path.
    Run(RunArgs),
}

/// Explicit router identity lifecycle operations.
#[derive(Debug, Subcommand)]
pub enum IdentityCommand {
    /// Generate and atomically persist a new router identity.
    Generate(IdentityArgs),
    /// Load and validate the existing router identity without displaying secrets.
    Inspect(IdentityArgs),
}

/// Arguments for `check-config`.
#[derive(Debug, Args)]
pub struct CheckConfigArgs {
    /// Configuration TOML path.
    #[arg(long)]
    pub config: PathBuf,
}

/// Arguments shared by identity lifecycle operations.
#[derive(Debug, Args)]
pub struct IdentityArgs {
    /// Configuration TOML path.
    #[arg(long)]
    pub config: PathBuf,
}

/// Arguments for `run`.
#[derive(Debug, Args)]
pub struct RunArgs {
    /// Configuration TOML path.
    #[arg(long)]
    pub config: PathBuf,
    /// Validate and normalize configuration without starting a router runtime.
    #[arg(long)]
    pub dry_run: bool,
}
