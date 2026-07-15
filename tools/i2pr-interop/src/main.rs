//! Deliberately non-production Plan 038/041 launcher seam.
//!
//! This binary is separate from `i2pr-daemon`. It validates the disposable
//! scenario boundary, reports the typed missing-driver result for listen/dial,
//! and provides a strict RouterInfo inspection helper for the reference-only
//! harness. It never starts a router or contacts a network.

use std::path::{Path, PathBuf};
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

fn inspect_router_info(state_dir: &Path) -> ExitCode {
    let path = state_dir.join("router.info");
    let Ok(bytes) = std::fs::read(&path) else {
        return emit("rejected", "router-info-missing");
    };
    if bytes.is_empty() || bytes.len() > i2pr_proto::MAX_COMMON_STRUCTURE_SIZE {
        return emit("rejected", "router-info-size-invalid");
    }
    let Ok(info) = i2pr_proto::RouterInfo::decode(&bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
    else {
        return emit("rejected", "router-info-structural-validation-failed");
    };
    if i2pr_crypto::verify_router_info(&info).is_err() {
        return emit("rejected", "router-info-signature-validation-failed");
    }
    let mut ntcp2_addresses = 0_u32;
    for address in info.addresses() {
        if matches!(
            i2pr_transport_ntcp2::Ntcp2RouterAddress::parse(address),
            Ok(parsed) if parsed.endpoint().is_some()
        ) {
            ntcp2_addresses = ntcp2_addresses.saturating_add(1);
        }
    }
    if ntcp2_addresses == 0 {
        return emit("rejected", "router-info-has-no-published-ntcp2-address");
    }
    println!(
        "{{\"schema\":1,\"type\":\"i2pr-interop-inspection\",\"result\":\"validated\",\"router_info_count\":1,\"ntcp2_address_count\":{ntcp2_addresses}}}"
    );
    ExitCode::SUCCESS
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
                inspect_router_info(&state_dir)
            }
        },
    }
}
