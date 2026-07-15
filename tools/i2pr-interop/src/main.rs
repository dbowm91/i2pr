//! Deliberately non-production Plan 038 launcher seam.
//!
//! This binary is separate from `i2pr-daemon`. It validates the disposable
//! scenario boundary and reports the typed missing-driver result until the
//! runtime-owned socket adapter is complete. It never starts a router or
//! contacts a network.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "i2pr-interop",
    version,
    about = "non-production NTCP2 harness seam"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Ntcp2 {
        #[command(subcommand)]
        command: Ntcp2Command,
    },
}

#[derive(Debug, Subcommand)]
enum Ntcp2Command {
    Listen {
        #[arg(long = "scenario-config")]
        scenario_config: PathBuf,
    },
    Dial {
        #[arg(long = "scenario-config")]
        scenario_config: PathBuf,
    },
    Inspect {
        #[arg(long = "state-dir")]
        state_dir: PathBuf,
    },
}

fn emit(result: &str, reason: &str) -> ExitCode {
    println!(
        "{{\"schema\":1,\"type\":\"i2pr-interop\",\"result\":\"{result}\",\"reason\":\"{reason}\"}}"
    );
    if result == "passed" {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

fn validate_file(path: &PathBuf, limit: usize) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() <= limit as u64)
        .unwrap_or(false)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Keep the ownership dependency visible at this composition seam. The
    // actual socket driver must remain in i2pr-runtime; protocol crates stay
    // runtime-neutral and are not given Tokio or filesystem responsibilities.
    let runtime_config = i2pr_runtime::Ntcp2RuntimeConfig::default();
    let _ = runtime_config.validate();
    let _frame_limit = i2pr_transport_ntcp2::constants::MAX_FRAME_LENGTH;
    let _ = std::any::type_name::<i2pr_transport::LinkId>();
    let _ = std::any::type_name::<i2pr_proto::Hash>();
    let _ = std::any::type_name::<i2pr_crypto::OsRng>();
    let _ = std::any::type_name::<i2pr_storage::IdentityStore>();

    match cli.command {
        Command::Ntcp2 { command } => match command {
            Ntcp2Command::Listen { scenario_config } | Ntcp2Command::Dial { scenario_config } => {
                if !validate_file(&scenario_config, 64 * 1024) {
                    return emit("rejected", "invalid-scenario-config");
                }
                emit("blocked_missing_driver", "runtime-wire-driver-not-complete")
            }
            Ntcp2Command::Inspect { state_dir } => {
                if !state_dir.is_dir() {
                    return emit("rejected", "invalid-state-dir");
                }
                emit("blocked_missing_driver", "inspect-seam-only")
            }
        },
    }
}
