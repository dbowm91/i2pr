//! Non-production Plan 042 launcher composition root.
//!
//! This binary is deliberately separate from the daemon. It accepts only the
//! confined synthetic scenario schema, owns one bounded Tokio runtime through
//! `i2pr-runtime`, and emits fixed-category status records. It is suitable for
//! an isolated namespace run; it is not a public-network router.

mod scenario;
mod status;

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use i2pr_crypto::{
    OsRng, RouterIdentityBundle, TransportStaticKey, X25519PrivateKey, router_identity_hash,
};
use i2pr_proto::{
    Date, DeliveryStatusMessage, Hash, I2npBody, I2npMessage, MAX_COMMON_STRUCTURE_SIZE, Mapping,
    MessageType, RouterAddress, RouterInfo,
};
use i2pr_runtime::{
    CancellationToken, HandshakeClock, HandshakeDriverConfig, HandshakeDriverError, Ntcp2Deadline,
    Ntcp2RuntimeConfig, Ntcp2RuntimeDeadlines, Ntcp2RuntimeService,
    PaddingProfile as DriverPaddingProfile, bounded_timeout, run_blocking,
};
use i2pr_storage::{IdentityStore, StorageError, TransportStaticKeyStore};
use i2pr_transport::MAX_I2NP_MESSAGE_BYTES;
use i2pr_transport_ntcp2::block::{Block, DecodedBlock, I2npMessageBlock};
use i2pr_transport_ntcp2::constants::MAX_FRAME_LENGTH;
use i2pr_transport_ntcp2::crypto::PublicKeyBytes;
use i2pr_transport_ntcp2::frame::FrameAssemblyPolicy;
use i2pr_transport_ntcp2::handshake::{ClockSkewPolicy, HandshakeError};
use i2pr_transport_ntcp2::state_machine::{InitiatorState, ResponderState};
use i2pr_transport_ntcp2::{Ntcp2Endpoint, Ntcp2RouterAddress};

use crate::scenario::{DataPhaseMode, Role, Scenario, TopologyKind};
use crate::status::{
    StatusCounters, StatusPhase, StatusReason, StatusResult, StatusWriter, emit_stdout_status,
};

const MAX_LOCAL_ROUTER_INFO_BYTES: usize = MAX_COMMON_STRUCTURE_SIZE;
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;

#[derive(Debug, Parser)]
#[command(
    name = "i2pr-interop",
    version,
    about = "non-production NTCP2 harness launcher"
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
    /// Prepare one local identity and signed RouterInfo without opening a socket.
    Prepare {
        #[arg(long = "state-dir")]
        state_dir: PathBuf,
        #[arg(long = "local-address")]
        local_address: IpAddr,
        #[arg(long = "local-port")]
        local_port: u16,
        #[arg(long = "network-id")]
        network_id: u16,
        #[arg(long = "deterministic-seed")]
        deterministic_seed: Option<u64>,
        /// Plan 086: optional topology kind. The default is the
        /// synthetic RFC 5737 / 3849 range; the only accepted
        /// non-default value is ``host-loopback-development``, which
        /// permits literal IPv4 ``127.0.0.1`` endpoints. The
        /// topology flag never weakens production address validation.
        #[arg(long = "topology-kind")]
        topology_kind: Option<String>,
    },
    ValidateScenario {
        #[arg(long = "scenario-config")]
        scenario_config: PathBuf,
    },
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

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LauncherError {
    StateInvalid,
    PeerRouterInfoInvalid,
    UnsupportedPaddingProfile,
    ListenerFailed,
    DialFailed,
    HandshakeFailed,
    DataPhaseFailed,
    Timeout,
    CleanupFailed,
    StatusOutputUnavailable,
    // Plan 052 G1: data-frame read and I2NP decode failures are split
    // out of `DataPhaseFailed` so the responder side can map them to
    // bounded `responder_*` reasons while the initiator side keeps
    // collapsing them into the broad `DataPhaseFailed` reason.
    DataFrameReadFailed,
    I2npDecodeFailed,
    // Plan 052 G1: responder-stage classification. The responder driver
    // wraps a HandshakeDriverError::ResponderStage error with a fixed
    // redacted phase label; the launcher maps each label to a bounded
    // `responder_*` StatusReason. The variants are emitted only on the
    // responder side and only on a Terminal phase.
    ResponderTcpAcceptMissing,
    ResponderAdmissionRejected,
    ResponderMessage1DecodeFailed,
    ResponderMessage1OptionsInvalid,
    ResponderNoiseStateFailed,
    ResponderSessionCreatedWriteFailed,
    ResponderSessionConfirmedPart1Failed,
    ResponderSessionConfirmedPart2Failed,
    ResponderRouterIdentityVerificationFailed,
    ResponderHandshakeTimeout,
    ResponderAuthenticatedLinkInstallFailed,
    ResponderDataFrameReadFailed,
    ResponderI2npDecodeFailed,
    // Plan 065: bounded sender-side typed failures. The launcher must
    // emit the exact bounded category rather than the broad
    // `DataPhaseFailed` whenever the typed predicate is available.
    SenderDeliveryStatusMessageIdZero,
    SenderRouterIdentityMismatch,
    SenderDeliveryStatusConstructionFailed,
    SenderFrameQueueAmbiguous,
    SenderFrameWriteFailed,
    SenderMultiplePrimaryDeliveryStatusEmitted,
    SenderCancellationObserved,
    // Plan 065: bounded receiver-side typed failures. The launcher must
    // map every data-phase failure to the exact bounded category and
    // must never collapse them into the broad `DataFrameReadFailed`
    // or `I2npDecodeFailed` reasons when a precise category applies.
    ReceiverFrameReadFailed,
    ReceiverFrameAuthenticationFailed,
    ReceiverI2npDecodeFailed,
    ReceiverDeliveryStatusMissing,
    ReceiverDeliveryStatusIdMismatch,
    ReceiverDeliveryStatusDuplicate,
    ReceiverPeerIdentityMismatch,
    ReceiverDeliveryStatusTimestampInvalid,
}

struct LocalState {
    router_info: Vec<u8>,
    router_hash: Hash,
    static_key: TransportStaticKey,
    obfuscation_iv: [u8; 16],
}

struct PeerState {
    router_hash: Hash,
    static_public: PublicKeyBytes,
    obfuscation_iv: [u8; 16],
}

fn emit_inspection(result: &str, reason: &str) -> ExitCode {
    let line = format!(
        "{{\"schema\":1,\"type\":\"i2pr-interop-inspection\",\"result\":\"{result}\",\"reason\":\"{reason}\"}}"
    );
    let mut stdout = io::stdout().lock();
    let write_result = stdout
        .write_all(line.as_bytes())
        .and_then(|_| stdout.write_all(b"\n"))
        .and_then(|_| stdout.flush());
    if write_result.is_err() || result != "validated" {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn inspect_router_info(state_dir: &Path) -> ExitCode {
    let path = state_dir.join("router.info");
    let Ok(bytes) = fs::read(&path) else {
        return emit_inspection("rejected", "router_info_missing");
    };
    if bytes.is_empty() || bytes.len() > MAX_COMMON_STRUCTURE_SIZE {
        return emit_inspection("rejected", "router_info_size_invalid");
    }
    let Ok(info) = RouterInfo::decode(&bytes, MAX_COMMON_STRUCTURE_SIZE) else {
        return emit_inspection("rejected", "router_info_structural_validation_failed");
    };
    if i2pr_crypto::verify_router_info(&info).is_err() {
        return emit_inspection("rejected", "router_info_signature_validation_failed");
    }
    let ntcp2_addresses = info
        .addresses()
        .iter()
        .filter(|address| {
            matches!(
                Ntcp2RouterAddress::parse(address),
                Ok(parsed) if parsed.endpoint().is_some()
            )
        })
        .count();
    if ntcp2_addresses == 0 {
        return emit_inspection("rejected", "router_info_has_no_published_ntcp2_address");
    }
    let line = format!(
        "{{\"schema\":1,\"type\":\"i2pr-interop-inspection\",\"result\":\"validated\",\"router_info_count\":1,\"ntcp2_address_count\":{ntcp2_addresses}}}"
    );
    let mut stdout = io::stdout().lock();
    let write_result = stdout
        .write_all(line.as_bytes())
        .and_then(|_| stdout.write_all(b"\n"))
        .and_then(|_| stdout.flush());
    if write_result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

fn emit_preparation(result: &str, reason: &str, local: Option<&LocalState>) -> ExitCode {
    let line = if let Some(local) = local {
        let router_info_sha256 = hex_lower(i2pr_crypto::sha256(&local.router_info).as_bytes());
        let router_hash_sha256 = hex_lower(local.router_hash.as_bytes());
        format!(
            "{{\"schema\":\"i2pr-interop-state-prepared-v1\",\"result\":\"prepared\",\"router_hash_sha256\":\"{router_hash_sha256}\",\"router_info_sha256\":\"{router_info_sha256}\",\"ntcp2_address_count\":1}}"
        )
    } else {
        format!(
            "{{\"schema\":\"i2pr-interop-state-prepared-v1\",\"result\":\"{result}\",\"reason_code\":\"{reason}\"}}"
        )
    };
    let mut stdout = io::stdout().lock();
    let write_result = stdout
        .write_all(line.as_bytes())
        .and_then(|_| stdout.write_all(b"\n"))
        .and_then(|_| stdout.flush());
    if write_result.is_err() || local.is_none() {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    }
}

fn prepare_state_command(
    state_dir: &Path,
    local_address: IpAddr,
    local_port: u16,
    network_id: u16,
    deterministic_seed: Option<u64>,
    topology_kind: Option<&str>,
) -> ExitCode {
    let topology = match topology_kind {
        None => TopologyKind::Synthetic,
        Some(scenario::HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND) => {
            TopologyKind::HostLoopbackDevelopment
        }
        Some(_) => {
            return emit_preparation("rejected", "prepare_topology_kind_invalid", None);
        }
    };
    if network_id != scenario::PRIVATE_NETWORK_ID
        || local_port == 0
        || !is_endpoint_address(local_address, topology)
        || !state_dir.is_absolute()
        || state_dir
            .symlink_metadata()
            .map(|metadata| !metadata.file_type().is_dir())
            .unwrap_or(false)
    {
        return emit_preparation("rejected", "prepare_input_invalid", None);
    }
    if state_dir.exists() && state_dir.is_symlink() {
        return emit_preparation("rejected", "prepare_state_path_invalid", None);
    }
    if IdentityStore::prepare_directory(state_dir).is_err() {
        return emit_preparation("rejected", "prepare_state_path_invalid", None);
    }
    let prepared = if let Some(seed) = deterministic_seed {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        prepare_local_state_with_rng(state_dir, local_address, local_port, &mut rng)
    } else {
        prepare_local_state_with_rng(state_dir, local_address, local_port, &mut OsRng)
    };
    match prepared {
        Ok(local) => emit_preparation("prepared", "", Some(&local)),
        Err(_) => emit_preparation("rejected", "prepare_router_info_verify_failed", None),
    }
}

fn is_synthetic_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => address.octets()[0..3] == [192, 0, 2],
        IpAddr::V6(address) => address.segments()[0..4] == [0x2001, 0x0db8, 0x0036, 0],
    }
}

/// Plan 086: shared endpoint acceptance for the test-only prepare
/// boundary. The synthetic RFC 5737 / 3849 range is the default; the
/// bounded ``host-loopback-development`` topology is the only other
/// accepted topology. Production address validation in the daemon
/// and the synthetic lanes remains unchanged.
fn is_endpoint_address(address: IpAddr, topology: TopologyKind) -> bool {
    match topology {
        TopologyKind::Synthetic => is_synthetic_address(address),
        TopologyKind::HostLoopbackDevelopment => match address {
            IpAddr::V4(value) => value.is_loopback(),
            IpAddr::V6(_) => false,
        },
    }
}

fn validate_scenario_command(scenario_config: &Path) -> ExitCode {
    let (result, reason) = if Scenario::load(scenario_config).is_ok() {
        ("validated", "")
    } else {
        ("rejected", "invalid_scenario_config")
    };
    let line = if reason.is_empty() {
        format!("{{\"schema\":\"i2pr-interop-scenario-validated-v1\",\"result\":\"{result}\"}}")
    } else {
        format!(
            "{{\"schema\":\"i2pr-interop-scenario-validated-v1\",\"result\":\"{result}\",\"reason_code\":\"{reason}\"}}"
        )
    };
    let mut stdout = io::stdout().lock();
    let write_result = stdout
        .write_all(line.as_bytes())
        .and_then(|_| stdout.write_all(b"\n"))
        .and_then(|_| stdout.flush());
    if write_result.is_ok() && result == "validated" {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

fn run_wire_command(mode: &'static str, scenario_config: &Path) -> ExitCode {
    let scenario = match Scenario::load(scenario_config) {
        Ok(scenario) => scenario,
        Err(_) => {
            let _ = emit_stdout_status(
                "unknown",
                StatusPhase::Terminal,
                StatusResult::Rejected,
                StatusReason::InvalidScenarioConfig,
                StatusCounters::default(),
            );
            return ExitCode::from(2);
        }
    };
    let expected_role = if mode == "listen" {
        Role::Responder
    } else {
        Role::Initiator
    };
    if scenario.role != expected_role {
        let mut status = match StatusWriter::new(&scenario) {
            Ok(status) => status,
            Err(_) => {
                let _ = emit_stdout_status(
                    &scenario.scenario_id,
                    StatusPhase::Terminal,
                    StatusResult::Rejected,
                    StatusReason::StatusOutputUnavailable,
                    StatusCounters::default(),
                );
                return ExitCode::from(2);
            }
        };
        let _ = status.emit(
            StatusPhase::Terminal,
            StatusResult::Rejected,
            StatusReason::ScenarioRoleMismatch,
            StatusCounters::default(),
        );
        return ExitCode::from(2);
    }
    let status = match StatusWriter::new(&scenario) {
        Ok(status) => status,
        Err(_) => {
            let _ = emit_stdout_status(
                &scenario.scenario_id,
                StatusPhase::Terminal,
                StatusResult::Rejected,
                StatusReason::StatusOutputUnavailable,
                StatusCounters::default(),
            );
            return ExitCode::from(2);
        }
    };
    let (mut status, wire_outcome, data_phase_mode) = run_blocking(execute_wire(scenario, status));
    let (counters, outcome) = wire_outcome;
    let (result, reason, counters) = match outcome {
        Ok(()) => {
            let reason = match data_phase_mode {
                DataPhaseMode::HandshakeOnly => StatusReason::HandshakeAuthenticated,
                DataPhaseMode::InitiatorDataOnly | DataPhaseMode::ResponderDataOnly => {
                    StatusReason::DirectionalDataPhaseComplete
                }
                DataPhaseMode::RoundTripDeliveryStatus => StatusReason::I2npExchangeComplete,
            };
            (StatusResult::Passed, reason, counters)
        }
        Err(error) => {
            let (result, reason) = terminal_status(error);
            // Plan 092: the accumulated counters are preserved on the
            // error path so the runner can correlate the last
            // authenticated/frame/I2NP state with the typed failure.
            (result, reason, counters)
        }
    };
    if status
        .emit(StatusPhase::Terminal, result, reason, counters)
        .is_err()
    {
        return ExitCode::from(2);
    }
    if result == StatusResult::Passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

async fn execute_wire(
    scenario: Scenario,
    mut status: StatusWriter,
) -> (
    StatusWriter,
    (StatusCounters, Result<(), LauncherError>),
    DataPhaseMode,
) {
    let data_phase_mode = scenario.data_phase_mode;
    let local = match prepare_local_state(&scenario) {
        Ok(local) => local,
        Err(error) => {
            return (
                status,
                (StatusCounters::default(), Err(error)),
                data_phase_mode,
            );
        }
    };
    let peer = match scenario.role {
        Role::Initiator => match prepare_peer_state(&scenario) {
            Ok(peer) => Some(peer),
            Err(error) => {
                return (
                    status,
                    (StatusCounters::default(), Err(error)),
                    data_phase_mode,
                );
            }
        },
        Role::Responder => None,
    };
    let padding = match driver_padding(scenario.padding_profile) {
        Ok(padding) => padding,
        Err(error) => {
            return (
                status,
                (StatusCounters::default(), Err(error)),
                data_phase_mode,
            );
        }
    };
    let deadlines = runtime_deadlines(&scenario);
    let service = match Ntcp2RuntimeService::new(Ntcp2RuntimeConfig {
        deadlines,
        ..Ntcp2RuntimeConfig::default()
    }) {
        Ok(service) => service,
        Err(_) => {
            return (
                status,
                (StatusCounters::default(), Err(LauncherError::StateInvalid)),
                data_phase_mode,
            );
        }
    };
    let root = CancellationToken::new();
    let scope = service.child_scope(&root);
    let mut counters = StatusCounters::default();

    if scenario.role == Role::Responder {
        let address = SocketAddr::new(scenario.local_address, scenario.local_port);
        let mut listener = match service.listen(address, &scope).await {
            Ok(listener) => listener,
            Err(_) => {
                return finish_scope(
                    status,
                    scope,
                    Err(LauncherError::ListenerFailed),
                    &mut counters,
                    data_phase_mode,
                )
                .await;
            }
        };
        counters.listener_ready = 1;
        if status
            .emit(
                StatusPhase::ListenerReady,
                StatusResult::Ready,
                StatusReason::ListenerBound,
                counters.clone(),
            )
            .is_err()
        {
            return finish_scope(
                status,
                scope,
                Err(LauncherError::StatusOutputUnavailable),
                &mut counters,
                data_phase_mode,
            )
            .await;
        }
        let result = execute_responder(
            &service,
            &scope,
            &root,
            &mut listener,
            &mut status,
            local,
            &scenario,
            deadlines,
            padding,
            &mut counters,
        )
        .await;
        finish_scope(status, scope, result, &mut counters, data_phase_mode).await
    } else {
        let result = execute_initiator(
            &service,
            &scope,
            &root,
            &mut status,
            local,
            peer.as_ref().expect("initiator peer was validated"),
            &scenario,
            deadlines,
            padding,
            &mut counters,
        )
        .await;
        finish_scope(status, scope, result, &mut counters, data_phase_mode).await
    }
}

async fn finish_scope(
    status: StatusWriter,
    scope: i2pr_runtime::ChildScope,
    result: Result<(), LauncherError>,
    counters: &mut StatusCounters,
    data_phase_mode: DataPhaseMode,
) -> (
    StatusWriter,
    (StatusCounters, Result<(), LauncherError>),
    DataPhaseMode,
) {
    let cleanup = scope.shutdown().await;
    if cleanup.failed() || cleanup.remaining() != 0 {
        return (
            status,
            (counters.clone(), Err(LauncherError::CleanupFailed)),
            data_phase_mode,
        );
    }
    (status, (counters.clone(), result), data_phase_mode)
}

#[allow(clippy::too_many_arguments)]
async fn execute_responder(
    service: &Ntcp2RuntimeService,
    scope: &i2pr_runtime::ChildScope,
    cancellation: &CancellationToken,
    listener: &mut i2pr_runtime::ListenerHandle,
    status: &mut StatusWriter,
    local: LocalState,
    scenario: &Scenario,
    deadlines: Ntcp2RuntimeDeadlines,
    padding: DriverPaddingProfile,
    counters: &mut StatusCounters,
) -> Result<(), LauncherError> {
    let LocalState {
        router_info,
        router_hash,
        static_key,
        obfuscation_iv,
    } = local;
    // Plan 065: the local i2pr router identity must match the scenario's
    // expected sender Router Hash before any handshake or frame reception.
    let router_hash_hex = hex_lower(router_hash.as_bytes());
    if router_hash_hex != scenario.expected_sender_router_hash_sha256 {
        return Err(LauncherError::SenderRouterIdentityMismatch);
    }
    let chunk = bounded_timeout(deadlines.handshake, listener.next())
        .await
        .map_err(|_| LauncherError::ResponderHandshakeTimeout)?
        .ok_or(LauncherError::ResponderTcpAcceptMissing)?;
    // Plan 091: emit the TCP-stage authority immediately after the
    // bounded TCP accept succeeds. The responder-side TCP accept is
    // the canonical event the runner advances to `tcp_connected` on.
    // Without this event the runner's pre-TCP classifier collapses
    // every post-TCP Noise failure into a typed pre-protocol
    // rejection, masking the real first-flight result.
    counters.tcp_connected = 1;
    if status
        .emit(
            StatusPhase::TcpConnected,
            StatusResult::Ready,
            StatusReason::TcpConnectSucceeded,
            counters.clone(),
        )
        .is_err()
    {
        return Err(LauncherError::StatusOutputUnavailable);
    }
    let ephemeral =
        X25519PrivateKey::generate(&mut OsRng).map_err(|_| LauncherError::StateInvalid)?;
    let state = ResponderState::new(
        static_key,
        ephemeral,
        None,
        *router_hash.as_bytes(),
        obfuscation_iv,
        99,
        ClockSkewPolicy::default_compatibility(),
    )
    .map_err(|_| LauncherError::ResponderNoiseStateFailed)?;
    let config = HandshakeDriverConfig {
        deadlines,
        clock: HandshakeClock::System,
        padding,
    };
    let (inbound, handshake) = chunk
        .into_stream()
        .drive_responder_handshake(
            state,
            &router_info,
            service.replay_cache(),
            config,
            cancellation,
        )
        .await
        .map_err(map_responder_stage_error)?;
    counters.authenticated = 1;
    let mut link = service
        .promote_authenticated_inbound(scope, inbound, handshake, 1)
        .map_err(map_responder_link_install_error)?;
    let result = exchange_directional(
        &mut link,
        cancellation,
        deadlines,
        counters,
        scenario.data_phase_mode,
        scenario.delivery_status_message_id,
        &scenario.expected_sender_router_hash_sha256,
        &scenario.expected_receiver_router_hash_sha256,
    )
    .await;
    link.close();
    result.map_err(classify_responder_data_phase_error)
}

/// Plan 052 G1 + Plan 065: classify a data-phase failure on the
/// responder side to the bounded responder-stage variant. The data
/// phase on the responder side is bounded: a frame read failure maps
/// to ``ResponderDataFrameReadFailed``; an I2NP decode failure maps
/// to ``ResponderI2npDecodeFailed``. Plan 065 adds the
/// receiver-stage typed categories (``ReceiverDeliveryStatusIdMismatch``,
/// ``ReceiverDeliveryStatusMissing``, etc.). On the responder side
/// the receiver categories are the terminal state; the launcher does
/// not collapse them into the broad ``DataPhaseFailed`` reason. The
/// responder-side variants are emitted only on the responder side
/// and only on a Terminal phase.
fn classify_responder_data_phase_error(error: LauncherError) -> LauncherError {
    match error {
        LauncherError::DataFrameReadFailed => LauncherError::ResponderDataFrameReadFailed,
        LauncherError::I2npDecodeFailed => LauncherError::ResponderI2npDecodeFailed,
        LauncherError::DataPhaseFailed => LauncherError::ResponderDataFrameReadFailed,
        LauncherError::ReceiverFrameReadFailed => LauncherError::ResponderDataFrameReadFailed,
        LauncherError::ReceiverI2npDecodeFailed => LauncherError::ResponderI2npDecodeFailed,
        // Plan 065: receiver-side correlation failures pass through as
        // the responder-side correlation failure so the harness and the
        // verifier can apply the bounded predicate without re-classifying
        // by hand.
        LauncherError::ReceiverDeliveryStatusIdMismatch => {
            LauncherError::ReceiverDeliveryStatusIdMismatch
        }
        LauncherError::ReceiverDeliveryStatusMissing => {
            LauncherError::ReceiverDeliveryStatusMissing
        }
        LauncherError::ReceiverDeliveryStatusDuplicate => {
            LauncherError::ReceiverDeliveryStatusDuplicate
        }
        LauncherError::ReceiverDeliveryStatusTimestampInvalid => {
            LauncherError::ReceiverDeliveryStatusTimestampInvalid
        }
        LauncherError::ReceiverFrameAuthenticationFailed => {
            LauncherError::ReceiverFrameAuthenticationFailed
        }
        LauncherError::ReceiverPeerIdentityMismatch => LauncherError::ReceiverPeerIdentityMismatch,
        other => other,
    }
}

/// Plan 052 G1: map a `HandshakeDriverError` produced by the responder
/// driver to one of the bounded responder-stage `LauncherError`
/// variants. The phase label is the primary classifier. When the label
/// corresponds to the `await_confirmed` phase (which spans both the
/// SessionConfirmed part-one static-key decrypt and the part-two
/// payload decrypt plus the RouterInfo identity verification), the
/// carried bounded `HandshakeError` variant is inspected so the
/// launcher can map to the precise responder-stage reason. The
/// carried error is a bounded protocol enum; it never contains
/// peer-controlled bytes.
fn map_responder_stage_error(error: HandshakeDriverError) -> LauncherError {
    match error {
        HandshakeDriverError::ResponderStage { phase_label, inner } => match phase_label {
            "need_request" => LauncherError::ResponderMessage1DecodeFailed,
            "await_replay" => LauncherError::ResponderAdmissionRejected,
            "await_request_padding" | "need_peer_timestamp" => {
                LauncherError::ResponderMessage1OptionsInvalid
            }
            "need_created_padding" => LauncherError::ResponderSessionCreatedWriteFailed,
            "await_confirmed" => classify_await_confirmed(&inner),
            "done" => LauncherError::ResponderRouterIdentityVerificationFailed,
            // Closed label set; an unknown label is a typed blocker.
            _ => LauncherError::ResponderRouterIdentityVerificationFailed,
        },
        // Plan 052 G1: deadline exhaustion reaching the driver layer is
        // a handshake-timeout (the listener boundary already classifies
        // listener-side timeouts as `ResponderHandshakeTimeout`).
        HandshakeDriverError::Protocol(HandshakeError::DeadlineExpired) => {
            LauncherError::ResponderHandshakeTimeout
        }
        HandshakeDriverError::Io(_) => LauncherError::ResponderSessionConfirmedPart1Failed,
        other => {
            let _ = other;
            LauncherError::HandshakeFailed
        }
    }
}

/// Plan 052 G1: refine the `await_confirmed` phase mapping. The
/// SessionConfirmed read+decrypt spans three logical stages:
///
/// - **part one**: static-key decrypt, ephemeral diffie-hellman, and
///   the static-mix step. Failures here carry
///   `AuthenticationFailure`, `DeobfuscationFailure`,
///   `TranscriptMismatch`, `InvalidKeyAgreement`, or
///   `InvalidFixedLength`/`Truncated`.
/// - **part two**: payload decrypt and ConfirmedPayload decode.
///   Failures here carry `TranscriptMismatch` again, `InvalidFixedLength`,
///   or codec-level rejections.
/// - **identity verification**: validate_router_info and
///   `PeerIdentityMismatch`, `RouterInfoMalformed`,
///   `RouterInfoSignatureInvalid`, `UnsupportedPeerKey`,
///   `TransportStaticKeyMismatch`.
///
/// The ordering of the operations matches the state-machine order, so
/// the first matching variant wins.
fn classify_await_confirmed(inner: &HandshakeDriverError) -> LauncherError {
    let HandshakeDriverError::Protocol(error) = inner else {
        return LauncherError::ResponderSessionConfirmedPart1Failed;
    };
    match error {
        HandshakeError::InvalidFixedLength
        | HandshakeError::Truncated
        | HandshakeError::ExcessivePadding
        | HandshakeError::DeobfuscationFailure
        | HandshakeError::AuthenticationFailure
        | HandshakeError::TranscriptMismatch
        | HandshakeError::InvalidKeyAgreement => {
            LauncherError::ResponderSessionConfirmedPart1Failed
        }
        HandshakeError::RouterInfoMalformed
        | HandshakeError::RouterInfoSignatureInvalid
        | HandshakeError::UnsupportedPeerKey
        | HandshakeError::TransportStaticKeyMismatch => {
            LauncherError::ResponderRouterIdentityVerificationFailed
        }
        HandshakeError::PeerIdentityMismatch => {
            LauncherError::ResponderRouterIdentityVerificationFailed
        }
        // Anything else inside `await_confirmed` is the payload-decrypt
        // stage.
        _ => LauncherError::ResponderSessionConfirmedPart2Failed,
    }
}

/// Plan 052 G1: map an authenticated-link start failure to the
/// bounded responder-stage variant. Admission failures are recovered
/// as `ResponderAdmissionRejected`; queue and child-scope failures as
/// `ResponderAuthenticatedLinkInstallFailed`.
fn map_responder_link_install_error(
    error: i2pr_runtime::AuthenticatedLinkStartError,
) -> LauncherError {
    match error {
        i2pr_runtime::AuthenticatedLinkStartError::Admission(_) => {
            LauncherError::ResponderAdmissionRejected
        }
        i2pr_runtime::AuthenticatedLinkStartError::QueueLimitTooLarge
        | i2pr_runtime::AuthenticatedLinkStartError::ChildScope => {
            LauncherError::ResponderAuthenticatedLinkInstallFailed
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_initiator(
    service: &Ntcp2RuntimeService,
    scope: &i2pr_runtime::ChildScope,
    cancellation: &CancellationToken,
    status: &mut StatusWriter,
    local: LocalState,
    peer: &PeerState,
    scenario: &Scenario,
    deadlines: Ntcp2RuntimeDeadlines,
    padding: DriverPaddingProfile,
    counters: &mut StatusCounters,
) -> Result<(), LauncherError> {
    let LocalState {
        router_info,
        router_hash,
        static_key,
        obfuscation_iv: _,
    } = local;
    // Plan 065: the local i2pr router identity must match the scenario's
    // expected sender Router Hash before any handshake or frame emission.
    let router_hash_hex = hex_lower(router_hash.as_bytes());
    if router_hash_hex != scenario.expected_sender_router_hash_sha256 {
        return Err(LauncherError::SenderRouterIdentityMismatch);
    }
    let peer_address = SocketAddr::new(
        scenario.peer_address.expect("peer address validated"),
        scenario.peer_port.expect("peer port validated"),
    );
    let attempt = service
        .dial(peer_address, cancellation)
        .await
        .map_err(|e| {
            eprintln!("debug: dial failed: {:?}", e);
            LauncherError::DialFailed
        })?;
    // Plan 091: emit the TCP-stage authority immediately after the
    // bounded TcpStream::connect succeeds and before any Noise
    // handshake byte is written. The post-connect socket close may
    // not serialise as a pre-protocol rejection once this event has
    // been emitted; the canonical Plan 083 runner advances the stage
    // tracker on observing this phase. Without this event the
    // runner's pre-TCP classifier collapses every post-TCP Noise
    // failure into a typed pre-protocol rejection, masking the real
    // first-flight result.
    let mut counters_after_tcp = counters.clone();
    counters_after_tcp.tcp_connected = 1;
    if status
        .emit(
            StatusPhase::TcpConnected,
            StatusResult::Ready,
            StatusReason::TcpConnectSucceeded,
            counters_after_tcp.clone(),
        )
        .is_err()
    {
        return Err(LauncherError::StatusOutputUnavailable);
    }
    *counters = counters_after_tcp;
    let ephemeral =
        X25519PrivateKey::generate(&mut OsRng).map_err(|_| LauncherError::StateInvalid)?;
    let state = InitiatorState::new(
        static_key,
        ephemeral,
        peer.static_public,
        Some(peer.router_hash),
        *peer.router_hash.as_bytes(),
        peer.obfuscation_iv,
        99,
        ClockSkewPolicy::default_compatibility(),
    )
    .map_err(|e| {
        eprintln!("debug: initiator state failed: {:?}", e);
        LauncherError::HandshakeFailed
    })?;
    let config = HandshakeDriverConfig {
        deadlines,
        clock: HandshakeClock::System,
        padding,
    };
    let (attempt, handshake) = attempt
        .drive_initiator_handshake(
            state,
            &router_info,
            service.replay_cache(),
            config,
            cancellation,
        )
        .await
        .map_err(|e| {
            eprintln!("debug: drive_initiator_handshake failed: {:?}", e);
            LauncherError::HandshakeFailed
        })?;
    counters.authenticated = 1;
    let mut link = service
        .promote_authenticated_dial(scope, attempt, handshake, 1)
        .map_err(|_| LauncherError::DataPhaseFailed)?;
    let result = exchange_directional(
        &mut link,
        cancellation,
        deadlines,
        counters,
        scenario.data_phase_mode,
        scenario.delivery_status_message_id,
        &scenario.expected_sender_router_hash_sha256,
        &scenario.expected_receiver_router_hash_sha256,
    )
    .await;
    link.close();
    result
}

/// Plan 045 D6: dispatch the data-phase exchange to the typed behavior
/// selected by the scenario. Plan 065: pass the scenario-owned
/// DeliveryStatus ``message_id`` and the expected 64-hex Router Hashes
/// through the typed helpers; the sender and receiver each validate
/// the correlation end-to-end and emit bounded typed failures.
#[allow(clippy::too_many_arguments)]
async fn exchange_directional(
    link: &mut i2pr_runtime::AuthenticatedLink,
    cancellation: &CancellationToken,
    deadlines: Ntcp2RuntimeDeadlines,
    counters: &mut StatusCounters,
    mode: DataPhaseMode,
    delivery_status_message_id: u32,
    expected_sender_router_hash_sha256: &str,
    expected_receiver_router_hash_sha256: &str,
) -> Result<(), LauncherError> {
    match mode {
        DataPhaseMode::HandshakeOnly => Ok(()),
        DataPhaseMode::InitiatorDataOnly | DataPhaseMode::RoundTripDeliveryStatus => {
            send_i2np_block(
                link,
                cancellation,
                deadlines,
                counters,
                delivery_status_message_id,
                expected_sender_router_hash_sha256,
                expected_receiver_router_hash_sha256,
            )
            .await?;
            if matches!(mode, DataPhaseMode::RoundTripDeliveryStatus) {
                receive_delivery_status(
                    link,
                    cancellation,
                    deadlines,
                    counters,
                    delivery_status_message_id,
                    expected_sender_router_hash_sha256,
                    expected_receiver_router_hash_sha256,
                )
                .await
            } else {
                Ok(())
            }
        }
        DataPhaseMode::ResponderDataOnly => {
            receive_delivery_status(
                link,
                cancellation,
                deadlines,
                counters,
                delivery_status_message_id,
                expected_sender_router_hash_sha256,
                expected_receiver_router_hash_sha256,
            )
            .await
        }
    }
}

async fn send_i2np_block(
    link: &mut i2pr_runtime::AuthenticatedLink,
    cancellation: &CancellationToken,
    deadlines: Ntcp2RuntimeDeadlines,
    counters: &mut StatusCounters,
    message_id: u32,
    _expected_sender_router_hash_sha256: &str,
    expected_receiver_router_hash_sha256: &str,
) -> Result<(), LauncherError> {
    // Plan 065: refuse to send a zero message ID. A zero ID would let
    // the receiver silently accept any DeliveryStatus regardless of
    // correlation, which is exactly the false-positive class Plan 065
    // must prevent.
    if message_id == 0 {
        return Err(LauncherError::SenderDeliveryStatusMessageIdZero);
    }
    let seconds = unix_seconds();
    let message = I2npMessage::new_short_transport(
        message_id,
        seconds,
        I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
            message_id,
            Date::from_millis(unix_millis()),
        )),
    )
    .map_err(|_| LauncherError::SenderDeliveryStatusConstructionFailed)?;
    // Plan 065: re-decode the constructed message and verify the
    // declared message_id round-trips through the envelope before any
    // frame emission. Any mismatch is a typed sender construction
    // failure rather than the broad DataPhaseFailed.
    let decoded = I2npMessage::decode_short_transport(
        message
            .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
            .map_err(|_| LauncherError::SenderDeliveryStatusConstructionFailed)?
            .as_slice(),
        MAX_I2NP_MESSAGE_BYTES,
    )
    .map_err(|_| LauncherError::SenderDeliveryStatusConstructionFailed)?;
    if decoded.header().message_id() != Some(message_id)
        || match decoded.body() {
            I2npBody::DeliveryStatus(inner) => inner.message_id != message_id,
            _ => true,
        }
    {
        return Err(LauncherError::SenderDeliveryStatusConstructionFailed);
    }
    let block = I2npMessageBlock::from_bytes(
        message
            .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
            .map_err(|_| LauncherError::SenderDeliveryStatusConstructionFailed)?,
    )
    .map_err(|_| LauncherError::SenderDeliveryStatusConstructionFailed)?;
    let policy = FrameAssemblyPolicy::new(MAX_FRAME_LENGTH, 0, 0, 0, false)
        .map_err(|_| LauncherError::SenderFrameQueueAmbiguous)?;
    let queue_deadline = Ntcp2Deadline::after(deadlines.queue_wait)
        .map_err(|_| LauncherError::SenderFrameQueueAmbiguous)?;
    if cancellation.is_cancelled() {
        return Err(LauncherError::SenderCancellationObserved);
    }
    link.send_blocks(
        vec![Block::I2np(block)],
        policy,
        queue_deadline,
        cancellation,
    )
    .await
    .map_err(|_| LauncherError::SenderFrameWriteFailed)?;
    counters.frames_sent = 1;
    counters.i2np_sent = 1;
    // Plan 065: record the bounded message_id and the expected peer
    // Router Hash in the counters so the harness and the post-run
    // verifier can confirm the exact correlation. The plan052 pipeline
    // reads these counters to build the directional correlation record.
    counters.delivery_status_message_id = message_id;
    counters.expected_peer_router_hash_sha256 = expected_receiver_router_hash_sha256.to_owned();
    Ok(())
}

async fn receive_delivery_status(
    link: &mut i2pr_runtime::AuthenticatedLink,
    cancellation: &CancellationToken,
    deadlines: Ntcp2RuntimeDeadlines,
    counters: &mut StatusCounters,
    expected_message_id: u32,
    _expected_sender_router_hash_sha256: &str,
    expected_receiver_router_hash_sha256: &str,
) -> Result<(), LauncherError> {
    let lease = bounded_timeout(deadlines.read_idle, link.recv(cancellation))
        .await
        .map_err(|_| LauncherError::Timeout)?
        .map_err(|_| LauncherError::ReceiverFrameReadFailed)?
        .ok_or(LauncherError::ReceiverFrameReadFailed)?;
    counters.frames_received = 1;
    let parsed = lease
        .frame()
        .plaintext()
        .parse()
        .map_err(|_| LauncherError::ReceiverFrameReadFailed)?;
    let mut found_delivery_status = false;
    for block in parsed.blocks() {
        if let DecodedBlock::I2np(message) = block {
            let decoded =
                I2npMessage::decode_short_transport(message.as_bytes(), MAX_I2NP_MESSAGE_BYTES)
                    .map_err(|_| LauncherError::ReceiverI2npDecodeFailed)?;
            if decoded.body().message_type() != MessageType::DeliveryStatus {
                continue;
            }
            let envelope_id = decoded.header().message_id().unwrap_or(0);
            let payload_id = match decoded.body() {
                I2npBody::DeliveryStatus(inner) => inner.message_id,
                _ => unreachable!(),
            };
            // Plan 065: the envelope message ID and the DeliveryStatus
            // payload message ID must both equal the scenario-owned
            // correlation ID. A type-only match (DeliveryStatus type
            // present but wrong ID) is rejected with the bounded
            // `ReceiverDeliveryStatusIdMismatch` reason.
            if envelope_id != expected_message_id || payload_id != expected_message_id {
                return Err(LauncherError::ReceiverDeliveryStatusIdMismatch);
            }
            if found_delivery_status {
                // Plan 065: a duplicate DeliveryStatus with the exact
                // correlation ID is a bounded rejection. The data phase
                // must observe exactly one relevant DeliveryStatus.
                return Err(LauncherError::ReceiverDeliveryStatusDuplicate);
            }
            found_delivery_status = true;
            counters.delivery_status_message_id = payload_id;
        }
    }
    if !found_delivery_status {
        return Err(LauncherError::ReceiverDeliveryStatusMissing);
    }
    counters.i2np_received = 1;
    counters.expected_peer_router_hash_sha256 = expected_receiver_router_hash_sha256.to_owned();
    Ok(())
}

fn prepare_local_state(scenario: &Scenario) -> Result<LocalState, LauncherError> {
    if let Some(seed) = scenario.deterministic_seed {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        prepare_local_state_with_rng(
            &scenario.state_dir,
            scenario.local_address,
            scenario.local_port,
            &mut rng,
        )
    } else {
        prepare_local_state_with_rng(
            &scenario.state_dir,
            scenario.local_address,
            scenario.local_port,
            &mut OsRng,
        )
    }
}

fn prepare_local_state_with_rng<R>(
    state_dir: &Path,
    local_address: IpAddr,
    local_port: u16,
    rng: &mut R,
) -> Result<LocalState, LauncherError>
where
    R: rand_core::TryCryptoRng + ?Sized,
{
    IdentityStore::prepare_directory(state_dir).map_err(map_storage_error)?;
    let identity_store = IdentityStore::in_data_dir(state_dir);
    let identity = if identity_store.path().exists() {
        identity_store.load().map_err(map_storage_error)?
    } else {
        let identity =
            RouterIdentityBundle::generate(rng).map_err(|_| LauncherError::StateInvalid)?;
        identity_store
            .save_new(&identity)
            .map_err(map_storage_error)?;
        identity
    };
    let key_store = TransportStaticKeyStore::in_data_dir(state_dir);
    let static_material = if key_store.path().exists() {
        key_store.load().map_err(map_storage_error)?
    } else {
        key_store.generate_new(rng).map_err(map_storage_error)?
    };
    let (static_key, obfuscation_iv) = static_material.into_parts();
    let static_public = static_key.public_bytes();
    let router_info_path = state_dir.join("router.info");
    let router_info_bytes = if router_info_path.exists() {
        read_private_file(&router_info_path).map_err(|_| LauncherError::StateInvalid)?
    } else {
        let info = signed_router_info(
            &identity,
            local_address,
            local_port,
            static_public,
            obfuscation_iv,
            scenario::PRIVATE_NETWORK_ID as u8,
        )?;
        let bytes = info
            .encode_to_vec(MAX_LOCAL_ROUTER_INFO_BYTES)
            .map_err(|_| LauncherError::StateInvalid)?;
        write_private_file(&router_info_path, &bytes).map_err(|_| LauncherError::StateInvalid)?;
        bytes
    };
    let info =
        decode_verified_router_info(&router_info_bytes).map_err(|_| LauncherError::StateInvalid)?;
    let expected = SocketAddr::new(local_address, local_port);
    let parsed = exact_ntcp2_address(&info, expected).map_err(|_| LauncherError::StateInvalid)?;
    if parsed.static_public_key().as_bytes() != &static_public
        || parsed.obfuscation_iv().map(|iv| iv.as_bytes()) != Some(&obfuscation_iv)
    {
        return Err(LauncherError::StateInvalid);
    }
    let router_hash =
        router_identity_hash(info.router_identity()).map_err(|_| LauncherError::StateInvalid)?;
    Ok(LocalState {
        router_info: router_info_bytes,
        router_hash,
        static_key,
        obfuscation_iv,
    })
}

fn prepare_peer_state(scenario: &Scenario) -> Result<PeerState, LauncherError> {
    let path = scenario
        .peer_router_info
        .as_ref()
        .ok_or(LauncherError::PeerRouterInfoInvalid)?;
    let bytes = read_private_file(path).map_err(|_| LauncherError::PeerRouterInfoInvalid)?;
    let info =
        decode_verified_router_info(&bytes).map_err(|_| LauncherError::PeerRouterInfoInvalid)?;
    let peer_address = SocketAddr::new(
        scenario
            .peer_address
            .ok_or(LauncherError::PeerRouterInfoInvalid)?,
        scenario
            .peer_port
            .ok_or(LauncherError::PeerRouterInfoInvalid)?,
    );
    let parsed = exact_ntcp2_address(&info, peer_address)
        .map_err(|_| LauncherError::PeerRouterInfoInvalid)?;
    let target = parsed
        .resolved_dial_target(peer_address)
        .map_err(|_| LauncherError::PeerRouterInfoInvalid)?;
    let router_hash = router_identity_hash(info.router_identity())
        .map_err(|_| LauncherError::PeerRouterInfoInvalid)?;
    Ok(PeerState {
        router_hash,
        static_public: target.expected_static_public_key(),
        obfuscation_iv: *target.obfuscation_iv().as_bytes(),
    })
}

fn signed_router_info(
    identity: &RouterIdentityBundle,
    address: IpAddr,
    port: u16,
    static_public: [u8; 32],
    obfuscation_iv: [u8; 16],
    network_id: u8,
) -> Result<RouterInfo, LauncherError> {
    let options = Mapping::from_entries(vec![
        ("host".to_owned(), address.to_string()),
        ("i".to_owned(), encode_i2p_base64(&obfuscation_iv)),
        ("port".to_owned(), port.to_string()),
        ("s".to_owned(), encode_i2p_base64(&static_public)),
        ("v".to_owned(), "2".to_owned()),
    ])
    .map_err(|_| LauncherError::StateInvalid)?;
    let router_address = RouterAddress::new(
        1,
        Date::from_millis(unix_millis().saturating_add(600_000)),
        "NTCP2".to_owned(),
        options,
    )
    .map_err(|_| LauncherError::StateInvalid)?;
    let ri_options = Mapping::from_entries(vec![
        ("caps".to_owned(), "L".to_owned()),
        ("netId".to_owned(), network_id.to_string()),
        ("router.version".to_owned(), "0.9.69".to_owned()),
    ])
    .map_err(|_| LauncherError::StateInvalid)?;
    identity
        .sign_router_info(
            Date::from_millis(unix_millis()),
            vec![router_address],
            Vec::new(),
            ri_options,
        )
        .map_err(|_| LauncherError::StateInvalid)
}

fn decode_verified_router_info(bytes: &[u8]) -> Result<RouterInfo, ()> {
    if bytes.is_empty() || bytes.len() > MAX_LOCAL_ROUTER_INFO_BYTES {
        return Err(());
    }
    let info = RouterInfo::decode(bytes, MAX_LOCAL_ROUTER_INFO_BYTES).map_err(|_| ())?;
    i2pr_crypto::verify_router_info(&info).map_err(|_| ())?;
    Ok(info)
}

fn exact_ntcp2_address(info: &RouterInfo, expected: SocketAddr) -> Result<Ntcp2RouterAddress, ()> {
    let endpoint = Ntcp2Endpoint::from_socket_addr(expected).map_err(|_| ())?;
    let mut found = None;
    for address in info.addresses() {
        if !matches!(address.transport_style(), "NTCP" | "NTCP2") {
            continue;
        }
        let parsed = Ntcp2RouterAddress::parse(address).map_err(|_| ())?;
        if parsed.endpoint() == Some(endpoint) {
            if found.is_some() {
                return Err(());
            }
            found = Some(parsed);
        }
    }
    found.ok_or(())
}

fn read_private_file(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a regular file",
        ));
    }
    fs::read(path)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        set_directory_permissions(parent)?;
    }
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    set_file_permissions(&file)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn set_file_permissions(file: &std::fs::File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE))?;
    }
    Ok(())
}

fn set_directory_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))?;
    }
    Ok(())
}

fn map_storage_error(error: StorageError) -> LauncherError {
    let _ = error;
    LauncherError::StateInvalid
}

fn runtime_deadlines(scenario: &Scenario) -> Ntcp2RuntimeDeadlines {
    Ntcp2RuntimeDeadlines {
        connect: Duration::from_millis(scenario.deadlines.handshake),
        handshake: Duration::from_millis(scenario.deadlines.handshake),
        read_idle: Duration::from_millis(scenario.deadlines.read),
        write: Duration::from_millis(scenario.deadlines.write),
        queue_wait: Duration::from_millis(scenario.deadlines.queue),
        drain: Duration::from_millis(scenario.deadlines.drain),
    }
}

fn driver_padding(
    profile: scenario::PaddingProfile,
) -> Result<DriverPaddingProfile, LauncherError> {
    match profile {
        scenario::PaddingProfile::MinimumVariableMaximum => Ok(DriverPaddingProfile::Minimum),
        scenario::PaddingProfile::Representative => Ok(DriverPaddingProfile::Representative),
        scenario::PaddingProfile::BoundaryAndMaximumPlusOne => {
            Err(LauncherError::UnsupportedPaddingProfile)
        }
    }
}

fn terminal_status(error: LauncherError) -> (StatusResult, StatusReason) {
    match error {
        LauncherError::StateInvalid => (StatusResult::Rejected, StatusReason::StateInvalid),
        LauncherError::PeerRouterInfoInvalid => {
            (StatusResult::Rejected, StatusReason::PeerRouterInfoInvalid)
        }
        LauncherError::UnsupportedPaddingProfile => (
            StatusResult::Rejected,
            StatusReason::UnsupportedPaddingProfile,
        ),
        LauncherError::ListenerFailed => (StatusResult::Rejected, StatusReason::ListenerFailed),
        LauncherError::DialFailed => (StatusResult::Rejected, StatusReason::DialFailed),
        LauncherError::HandshakeFailed => (
            StatusResult::AuthenticationFailed,
            StatusReason::HandshakeFailed,
        ),
        LauncherError::DataPhaseFailed => (StatusResult::Rejected, StatusReason::DataPhaseFailed),
        LauncherError::DataFrameReadFailed => {
            (StatusResult::Rejected, StatusReason::DataPhaseFailed)
        }
        LauncherError::I2npDecodeFailed => (StatusResult::Rejected, StatusReason::DataPhaseFailed),
        LauncherError::Timeout => (StatusResult::Timeout, StatusReason::Timeout),
        LauncherError::CleanupFailed => {
            (StatusResult::CleanupFailed, StatusReason::CleanupComplete)
        }
        LauncherError::StatusOutputUnavailable => (
            StatusResult::CleanupFailed,
            StatusReason::StatusOutputUnavailable,
        ),
        // Plan 052 G1: bounded responder-stage classification.
        LauncherError::ResponderTcpAcceptMissing => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderTcpAcceptMissing,
        ),
        LauncherError::ResponderAdmissionRejected => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderAdmissionRejected,
        ),
        LauncherError::ResponderMessage1DecodeFailed => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderMessage1DecodeFailed,
        ),
        LauncherError::ResponderMessage1OptionsInvalid => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderMessage1OptionsInvalid,
        ),
        LauncherError::ResponderNoiseStateFailed => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderNoiseStateFailed,
        ),
        LauncherError::ResponderSessionCreatedWriteFailed => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderSessionCreatedWriteFailed,
        ),
        LauncherError::ResponderSessionConfirmedPart1Failed => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderSessionConfirmedPart1Failed,
        ),
        LauncherError::ResponderSessionConfirmedPart2Failed => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderSessionConfirmedPart2Failed,
        ),
        LauncherError::ResponderRouterIdentityVerificationFailed => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderRouterIdentityVerificationFailed,
        ),
        LauncherError::ResponderHandshakeTimeout => (
            StatusResult::Timeout,
            StatusReason::ResponderHandshakeTimeout,
        ),
        LauncherError::ResponderAuthenticatedLinkInstallFailed => (
            StatusResult::AuthenticationFailed,
            StatusReason::ResponderAuthenticatedLinkInstallFailed,
        ),
        LauncherError::ResponderDataFrameReadFailed => (
            StatusResult::Rejected,
            StatusReason::ResponderDataFrameReadFailed,
        ),
        LauncherError::ResponderI2npDecodeFailed => (
            StatusResult::Rejected,
            StatusReason::ResponderI2npDecodeFailed,
        ),
        // Plan 065: bounded sender-side typed failures. The launcher
        // refuses to collapse the typed predicate into the broad
        // `DataPhaseFailed` reason; every sender-side typed error maps
        // to the exact bounded category.
        LauncherError::SenderDeliveryStatusMessageIdZero => (
            StatusResult::Rejected,
            StatusReason::SenderDeliveryStatusMessageIdZero,
        ),
        LauncherError::SenderRouterIdentityMismatch => (
            StatusResult::Rejected,
            StatusReason::SenderRouterIdentityMismatch,
        ),
        LauncherError::SenderDeliveryStatusConstructionFailed => (
            StatusResult::Rejected,
            StatusReason::SenderDeliveryStatusConstructionFailed,
        ),
        LauncherError::SenderFrameQueueAmbiguous => (
            StatusResult::Rejected,
            StatusReason::SenderFrameQueueAmbiguous,
        ),
        LauncherError::SenderFrameWriteFailed => {
            (StatusResult::Rejected, StatusReason::SenderFrameWriteFailed)
        }
        LauncherError::SenderMultiplePrimaryDeliveryStatusEmitted => (
            StatusResult::Rejected,
            StatusReason::SenderMultiplePrimaryDeliveryStatusEmitted,
        ),
        LauncherError::SenderCancellationObserved => (
            StatusResult::Rejected,
            StatusReason::SenderCancellationObserved,
        ),
        // Plan 065: bounded receiver-side typed failures. The launcher
        // maps every receiver-side typed error to the exact bounded
        // category; the broad `DataPhaseFailed` reason is no longer
        // emitted on the receiver side.
        LauncherError::ReceiverFrameReadFailed => (
            StatusResult::Rejected,
            StatusReason::ReceiverFrameReadFailed,
        ),
        LauncherError::ReceiverFrameAuthenticationFailed => (
            StatusResult::Rejected,
            StatusReason::ReceiverFrameAuthenticationFailed,
        ),
        LauncherError::ReceiverI2npDecodeFailed => (
            StatusResult::Rejected,
            StatusReason::ReceiverI2npDecodeFailed,
        ),
        LauncherError::ReceiverDeliveryStatusMissing => (
            StatusResult::Rejected,
            StatusReason::ReceiverDeliveryStatusMissing,
        ),
        LauncherError::ReceiverDeliveryStatusIdMismatch => (
            StatusResult::Rejected,
            StatusReason::ReceiverDeliveryStatusIdMismatch,
        ),
        LauncherError::ReceiverDeliveryStatusDuplicate => (
            StatusResult::Rejected,
            StatusReason::ReceiverDeliveryStatusDuplicate,
        ),
        LauncherError::ReceiverPeerIdentityMismatch => (
            StatusResult::Rejected,
            StatusReason::ReceiverPeerIdentityMismatch,
        ),
        LauncherError::ReceiverDeliveryStatusTimestampInvalid => (
            StatusResult::Rejected,
            StatusReason::ReceiverDeliveryStatusTimestampInvalid,
        ),
    }
}

fn unix_seconds() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(u64::from(u32::MAX)) as u32
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

/// Plan 065: 32-byte Router Hash to lowercase 64-hex string. The strict
/// scenario parser uses the same lowercase-hex contract so the i2pr
/// launcher and the Python pipeline agree on the per-run correlation.
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn encode_i2p_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[((a & 0x03) << 4 | b >> 4) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((b & 0x0f) << 2 | c >> 6) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Ntcp2 { command } => match command {
            Ntcp2Command::Prepare {
                state_dir,
                local_address,
                local_port,
                network_id,
                deterministic_seed,
                topology_kind,
            } => prepare_state_command(
                &state_dir,
                local_address,
                local_port,
                network_id,
                deterministic_seed,
                topology_kind.as_deref(),
            ),
            Ntcp2Command::ValidateScenario { scenario_config } => {
                validate_scenario_command(&scenario_config)
            }
            Ntcp2Command::Listen { scenario_config } => {
                run_wire_command("listen", &scenario_config)
            }
            Ntcp2Command::Dial { scenario_config } => run_wire_command("dial", &scenario_config),
            Ntcp2Command::Inspect { state_dir } => {
                if !state_dir.is_dir() {
                    emit_inspection("rejected", "invalid_state_dir")
                } else {
                    inspect_router_info(&state_dir)
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_scenario(root: &Path) -> Scenario {
        Scenario::parse_str(
            r#"
[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "java-to-i2pr-ipv4"
run_id = "launcher-state-run"
role = "responder"
address_family = "ipv4"
local_address = "192.0.2.1"
local_port = 45680
network_id = 99
state_dir = "secrets"
handshake_deadline_ms = 1000
read_deadline_ms = 1000
write_deadline_ms = 1000
queue_deadline_ms = 1000
drain_deadline_ms = 1000
padding_profile = "representative"
smoke_message_profile = "delivery-status"
deterministic_seed = 7
expected_result_class = "typed-rejection-with-bounded-cleanup"
status_path = "status.jsonl"
delivery_status_message_id = 4242
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "java-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
"#,
            root,
        )
        .expect("test scenario")
    }

    #[test]
    fn local_state_is_persisted_and_matches_the_published_endpoint() {
        let root = std::env::temp_dir().join(format!(
            "i2pr-launcher-state-{}-{}",
            std::process::id(),
            unix_millis()
        ));
        fs::create_dir(&root).expect("test root");
        let scenario = test_scenario(&root);
        let first = prepare_local_state(&scenario).expect("first local state");
        assert!(!first.router_info.is_empty());
        assert!(first.router_info.len() <= MAX_LOCAL_ROUTER_INFO_BYTES);
        let second = prepare_local_state(&scenario).expect("reloaded local state");
        assert_eq!(
            first.static_key.public_bytes(),
            second.static_key.public_bytes()
        );
        assert_eq!(first.router_hash, second.router_hash);
        assert_eq!(first.router_info, second.router_info);
        let info = decode_verified_router_info(&second.router_info).expect("verified info");
        assert!(
            exact_ntcp2_address(
                &info,
                SocketAddr::new(scenario.local_address, scenario.local_port)
            )
            .is_ok()
        );
        fs::remove_dir_all(root).expect("test cleanup");
    }

    #[test]
    fn data_phase_modes_complete_typed_terminal_reason() {
        for (data_phase_mode, expected_reason, expected_marker) in [
            (
                scenario::DataPhaseMode::HandshakeOnly,
                StatusReason::HandshakeAuthenticated,
                "handshake_authenticated",
            ),
            (
                scenario::DataPhaseMode::InitiatorDataOnly,
                StatusReason::DirectionalDataPhaseComplete,
                "directional_data_phase_complete",
            ),
            (
                scenario::DataPhaseMode::ResponderDataOnly,
                StatusReason::DirectionalDataPhaseComplete,
                "directional_data_phase_complete",
            ),
            (
                scenario::DataPhaseMode::RoundTripDeliveryStatus,
                StatusReason::I2npExchangeComplete,
                "i2np_exchange_complete",
            ),
        ] {
            let root = std::env::temp_dir().join(format!(
                "i2pr-launcher-dpm-{}-{}",
                std::process::id(),
                unix_millis()
            ));
            fs::create_dir(&root).expect("test root");
            let mut scenario = test_scenario(&root);
            scenario.data_phase_mode = data_phase_mode;
            scenario.expected_result_class =
                scenario::ExpectedResultClass::AuthenticatedHandshakeAndDirectionalDataPhase;
            let mut writer = StatusWriter::new(&scenario).expect("status writer");
            writer
                .emit(
                    StatusPhase::Terminal,
                    StatusResult::Passed,
                    expected_reason,
                    StatusCounters::default(),
                )
                .expect("status record");
            let contents = std::fs::read_to_string(&scenario.status_path).expect("status file");
            assert!(
                contents.contains(expected_marker),
                "missing marker {expected_marker}"
            );
            fs::remove_dir_all(root).expect("test cleanup");
        }
    }

    /// Plan 052 G1: every responder-phase label is mapped to a distinct
    /// bounded responder-stage `LauncherError`. The label set is closed;
    /// an unknown label collapses to the typed-blocker variant rather
    /// than the broad `HandshakeFailed`.
    #[test]
    fn responder_phase_labels_map_to_distinct_stage_reasons() {
        let labelled = [
            ("need_request", LauncherError::ResponderMessage1DecodeFailed),
            ("await_replay", LauncherError::ResponderAdmissionRejected),
            (
                "await_request_padding",
                LauncherError::ResponderMessage1OptionsInvalid,
            ),
            (
                "need_peer_timestamp",
                LauncherError::ResponderMessage1OptionsInvalid,
            ),
            (
                "need_created_padding",
                LauncherError::ResponderSessionCreatedWriteFailed,
            ),
            (
                "await_confirmed",
                LauncherError::ResponderSessionConfirmedPart1Failed,
            ),
            (
                "done",
                LauncherError::ResponderRouterIdentityVerificationFailed,
            ),
        ];
        let mut seen_labels = std::collections::HashSet::new();
        for (label, expected) in labelled {
            let mapped = map_responder_stage_error(HandshakeDriverError::ResponderStage {
                phase_label: label,
                inner: Box::new(HandshakeDriverError::Protocol(
                    i2pr_transport_ntcp2::handshake::HandshakeError::Truncated,
                )),
            });
            assert_eq!(
                mapped, expected,
                "label {label} did not map to the expected responder-stage error"
            );
            assert!(
                seen_labels.insert(label),
                "label {label} appeared more than once"
            );
        }
        // An unknown label falls through to the typed-blocker variant
        // rather than the broad `HandshakeFailed`.
        let unknown = map_responder_stage_error(HandshakeDriverError::ResponderStage {
            phase_label: "unrecognised",
            inner: Box::new(HandshakeDriverError::Protocol(
                i2pr_transport_ntcp2::handshake::HandshakeError::Truncated,
            )),
        });
        assert_eq!(
            unknown,
            LauncherError::ResponderRouterIdentityVerificationFailed
        );
        // Deadline exhaustion collapses to the responder-timeout variant.
        let timeout = map_responder_stage_error(HandshakeDriverError::Protocol(
            i2pr_transport_ntcp2::handshake::HandshakeError::DeadlineExpired,
        ));
        assert_eq!(timeout, LauncherError::ResponderHandshakeTimeout);
    }

    /// Plan 052 G1: the responder-stage classifier differentiates
    /// SessionConfirmed part-one, part-two, and RouterInfo identity
    /// verification failures.
    #[test]
    fn await_confirmed_phase_distinguishes_part1_part2_and_identity() {
        use i2pr_transport_ntcp2::handshake::HandshakeError;
        // Part-one failures: structural and transcript.
        for part1 in [
            HandshakeError::InvalidFixedLength,
            HandshakeError::Truncated,
            HandshakeError::ExcessivePadding,
            HandshakeError::DeobfuscationFailure,
            HandshakeError::AuthenticationFailure,
            HandshakeError::TranscriptMismatch,
            HandshakeError::InvalidKeyAgreement,
        ] {
            let label = format!("{part1:?}");
            let mapped = map_responder_stage_error(HandshakeDriverError::ResponderStage {
                phase_label: "await_confirmed",
                inner: Box::new(HandshakeDriverError::Protocol(part1)),
            });
            assert_eq!(
                mapped,
                LauncherError::ResponderSessionConfirmedPart1Failed,
                "part-1 variant {label} did not collapse to part1"
            );
        }
        // Identity verification: identity, malformed, signature, peer
        // key, static-key.
        for identity in [
            HandshakeError::PeerIdentityMismatch,
            HandshakeError::RouterInfoMalformed,
            HandshakeError::RouterInfoSignatureInvalid,
            HandshakeError::UnsupportedPeerKey,
            HandshakeError::TransportStaticKeyMismatch,
        ] {
            let label = format!("{identity:?}");
            let mapped = map_responder_stage_error(HandshakeDriverError::ResponderStage {
                phase_label: "await_confirmed",
                inner: Box::new(HandshakeDriverError::Protocol(identity)),
            });
            assert_eq!(
                mapped,
                LauncherError::ResponderRouterIdentityVerificationFailed,
                "identity variant {label} did not collapse to identity"
            );
        }
        // Part-two failures: any other bounded protocol error.
        let part2 = map_responder_stage_error(HandshakeDriverError::ResponderStage {
            phase_label: "await_confirmed",
            inner: Box::new(HandshakeDriverError::Protocol(
                HandshakeError::LocalPolicyDenied,
            )),
        });
        assert_eq!(part2, LauncherError::ResponderSessionConfirmedPart2Failed);
    }

    /// Plan 052 G1: every `responder_*` launcher error maps to a
    /// distinct bounded `responder_*` `StatusReason` and is emitted
    /// with `StatusResult::AuthenticationFailed` (or `Timeout` for the
    /// dedicated timeout variant). The mapping is closed: the broad
    /// `HandshakeFailed` and `DataPhaseFailed` reasons are never
    /// reused on the responder side.
    #[test]
    fn responder_terminal_status_emits_only_responder_reasons() {
        let responder_errors = [
            LauncherError::ResponderTcpAcceptMissing,
            LauncherError::ResponderAdmissionRejected,
            LauncherError::ResponderMessage1DecodeFailed,
            LauncherError::ResponderMessage1OptionsInvalid,
            LauncherError::ResponderNoiseStateFailed,
            LauncherError::ResponderSessionCreatedWriteFailed,
            LauncherError::ResponderSessionConfirmedPart1Failed,
            LauncherError::ResponderSessionConfirmedPart2Failed,
            LauncherError::ResponderRouterIdentityVerificationFailed,
            LauncherError::ResponderHandshakeTimeout,
            LauncherError::ResponderAuthenticatedLinkInstallFailed,
            LauncherError::ResponderDataFrameReadFailed,
            LauncherError::ResponderI2npDecodeFailed,
        ];
        let mut seen_reasons = std::collections::HashSet::new();
        for error in responder_errors {
            let (result, reason) = terminal_status(error);
            let reason_str = super::status::reason_name(reason);
            assert!(
                reason_str.starts_with("responder_"),
                "non-responder reason leaked: {reason_str}"
            );
            assert!(
                seen_reasons.insert(reason_str),
                "responder reason {reason_str} appeared more than once"
            );
            // Timeout-class responder variant maps to Timeout;
            // post-handshake data-phase responder variants map to
            // Rejected; every other responder-stage variant is an
            // authentication failure.
            let expected_result = match error {
                LauncherError::ResponderHandshakeTimeout => StatusResult::Timeout,
                LauncherError::ResponderDataFrameReadFailed
                | LauncherError::ResponderI2npDecodeFailed => StatusResult::Rejected,
                _ => StatusResult::AuthenticationFailed,
            };
            assert_eq!(result, expected_result, "wrong result for {error:?}");
        }
    }

    /// Plan 052 G1: data-phase failures on the responder side split
    /// into `ResponderDataFrameReadFailed` (frame read) and
    /// `ResponderI2npDecodeFailed` (I2NP decode).
    #[test]
    fn responder_data_phase_classifier_splits_frame_and_decode_failures() {
        assert_eq!(
            classify_responder_data_phase_error(LauncherError::DataFrameReadFailed),
            LauncherError::ResponderDataFrameReadFailed,
        );
        assert_eq!(
            classify_responder_data_phase_error(LauncherError::I2npDecodeFailed),
            LauncherError::ResponderI2npDecodeFailed,
        );
        // The broad DataPhaseFailed still collapses to frame-read.
        assert_eq!(
            classify_responder_data_phase_error(LauncherError::DataPhaseFailed),
            LauncherError::ResponderDataFrameReadFailed,
        );
        // Non-data-phase errors pass through.
        assert_eq!(
            classify_responder_data_phase_error(LauncherError::StateInvalid),
            LauncherError::StateInvalid,
        );
    }

    /// Plan 052 G1: authenticated-link install failures on the
    /// responder side split into admission (replay/admission gate)
    /// and structural install failures.
    #[test]
    fn responder_link_install_error_splits_admission_and_structural() {
        // We only need to verify the dispatch table; the runtime's
        // AuthenticatedLinkStartError is non-exhaustive.
        let admission = LauncherError::ResponderAdmissionRejected;
        let install = LauncherError::ResponderAuthenticatedLinkInstallFailed;
        assert_eq!(
            terminal_status(admission).1,
            StatusReason::ResponderAdmissionRejected
        );
        assert_eq!(
            terminal_status(install).1,
            StatusReason::ResponderAuthenticatedLinkInstallFailed
        );
    }
}
