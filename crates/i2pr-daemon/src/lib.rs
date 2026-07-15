//! Non-networked CLI shell and future daemon composition root.
//!
//! This crate validates configuration and exposes lifecycle boundaries only.
//! It does not open listeners, create router identity, download reseed data,
//! or claim support for any I2P transport or application protocol.

#![forbid(unsafe_code)]

pub mod cli;
pub mod config;
pub mod error;

use cli::{CheckConfigArgs, Cli, Command, RunArgs};
use config::Config;
use error::DaemonError;

/// Result of a successful side-effect-free validation command.
#[derive(Debug)]
pub enum CommandOutcome {
    /// A configuration was validated for the requested command.
    Validated {
        /// Whether the validation came from `run --dry-run`.
        dry_run: bool,
        /// The normalized snapshot used for validation.
        config: Config,
    },
}

/// Executes a parsed CLI command without initializing runtime or network state.
pub fn execute(cli: Cli) -> Result<CommandOutcome, DaemonError> {
    match cli.command {
        Command::CheckConfig(CheckConfigArgs { config }) => Ok(CommandOutcome::Validated {
            dry_run: false,
            config: Config::load(&config)?,
        }),
        Command::Run(RunArgs { config, dry_run }) => {
            let config = Config::load(&config)?;
            if !dry_run {
                return Err(DaemonError::RuntimeNotImplemented);
            }
            Ok(CommandOutcome::Validated { dry_run, config })
        }
    }
}

/// Initializes the future daemon logging subscriber using validated settings.
///
/// Repeated initialization is intentionally harmless for embedding tests.  A
/// later composition plan will own subscriber layering and redaction policy.
pub fn initialize_logging(config: &config::LoggingConfig) {
    let filter = tracing_subscriber::EnvFilter::new(config.filter.clone());
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::*;
    use crate::cli::{CheckConfigArgs, Command, RunArgs};
    use crate::error::ExitCode;

    #[test]
    fn missing_file_has_unavailable_exit_code() {
        let cli = Cli {
            command: Command::CheckConfig(CheckConfigArgs {
                config: PathBuf::from("missing-config.toml"),
            }),
        };
        let error = execute(cli).expect_err("missing config must fail");
        assert_eq!(error.exit_code(), ExitCode::ConfigUnavailable);
    }

    #[test]
    fn live_run_is_explicitly_unimplemented_after_validation() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        std::fs::write(
            &path,
            "schema_version = 1\n[router]\ndata_dir = \"state\"\n",
        )
        .expect("write config");
        let cli = Cli {
            command: Command::Run(RunArgs {
                config: path,
                dry_run: false,
            }),
        };
        let error = execute(cli).expect_err("live runtime must fail");
        assert_eq!(error.exit_code(), ExitCode::RuntimeNotImplemented);
    }

    #[test]
    fn parser_exposes_required_commands_and_flags() {
        let cli = Cli::try_parse_from(["i2pr", "run", "--config", "config.toml", "--dry-run"])
            .expect("valid command");
        assert!(matches!(
            cli.command,
            Command::Run(RunArgs { dry_run: true, .. })
        ));
    }
}
