//! `i2pr` executable entry point.

use std::process::ExitCode as ProcessExitCode;

use clap::Parser;
use i2pr_daemon::cli::{Cli, Command};
use i2pr_daemon::error::ExitCode;

fn main() -> ProcessExitCode {
    let cli = Cli::parse();
    match i2pr_daemon::execute(cli) {
        Ok(i2pr_daemon::CommandOutcome::Validated { dry_run, config }) => {
            i2pr_daemon::initialize_logging(&config.logging);
            if dry_run {
                println!(
                    "configuration is valid; dry run complete (no network or persistent state was touched)"
                );
            } else {
                println!("configuration is valid; no network or persistent state was touched");
            }
            ProcessExitCode::SUCCESS
        }
        Ok(i2pr_daemon::CommandOutcome::IdentityGenerated { path }) => {
            println!("router identity generated and stored at {}", path.display());
            ProcessExitCode::SUCCESS
        }
        Ok(i2pr_daemon::CommandOutcome::IdentityInspected { path, summary }) => {
            println!(
                "router identity is valid at {}; signing algorithm type {}, encryption algorithm type {}; private material was not displayed",
                path.display(),
                summary.signing_algorithm,
                summary.encryption_algorithm
            );
            ProcessExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            process_exit(error.exit_code())
        }
    }
}

fn process_exit(code: ExitCode) -> ProcessExitCode {
    ProcessExitCode::from(code.as_i32() as u8)
}

#[allow(dead_code)]
fn _command_name(command: &Command) -> &'static str {
    match command {
        Command::CheckConfig(_) => "check-config",
        Command::Identity { .. } => "identity",
        Command::Run(_) => "run",
    }
}
