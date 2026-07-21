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
    CancellationToken, DialOutcome, HandshakeClock, HandshakeDriverConfig, Ntcp2Deadline,
    Ntcp2RuntimeConfig, Ntcp2RuntimeDeadlines, Ntcp2RuntimeService,
    PaddingProfile as DriverPaddingProfile, bounded_timeout, run_blocking,
};
use i2pr_storage::{IdentityStore, StorageError, TransportStaticKeyStore};
use i2pr_transport::MAX_I2NP_MESSAGE_BYTES;
use i2pr_transport_ntcp2::block::{Block, DecodedBlock, I2npMessageBlock};
use i2pr_transport_ntcp2::constants::MAX_FRAME_LENGTH;
use i2pr_transport_ntcp2::crypto::PublicKeyBytes;
use i2pr_transport_ntcp2::frame::FrameAssemblyPolicy;
use i2pr_transport_ntcp2::handshake::ClockSkewPolicy;
use i2pr_transport_ntcp2::state_machine::{InitiatorState, ResponderState};
use i2pr_transport_ntcp2::{Ntcp2Endpoint, Ntcp2RouterAddress};

use crate::scenario::{DataPhaseMode, Role, Scenario};
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
    let (mut status, outcome, data_phase_mode) = run_blocking(execute_wire(scenario, status));
    let (result, reason, counters) = match outcome {
        Ok(counters) => {
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
            (result, reason, StatusCounters::default())
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
    Result<StatusCounters, LauncherError>,
    DataPhaseMode,
) {
    let data_phase_mode = scenario.data_phase_mode;
    let local = match prepare_local_state(&scenario) {
        Ok(local) => local,
        Err(error) => return (status, Err(error), data_phase_mode),
    };
    let peer = match scenario.role {
        Role::Initiator => match prepare_peer_state(&scenario) {
            Ok(peer) => Some(peer),
            Err(error) => return (status, Err(error), data_phase_mode),
        },
        Role::Responder => None,
    };
    let padding = match driver_padding(scenario.padding_profile) {
        Ok(padding) => padding,
        Err(error) => return (status, Err(error), data_phase_mode),
    };
    let deadlines = runtime_deadlines(&scenario);
    let service = match Ntcp2RuntimeService::new(Ntcp2RuntimeConfig {
        deadlines,
        ..Ntcp2RuntimeConfig::default()
    }) {
        Ok(service) => service,
        Err(_) => return (status, Err(LauncherError::StateInvalid), data_phase_mode),
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
                counters,
            )
            .is_err()
        {
            return finish_scope(
                status,
                scope,
                Err(LauncherError::StatusOutputUnavailable),
                data_phase_mode,
            )
            .await;
        }
        let result = execute_responder(
            &service,
            &scope,
            &root,
            &mut listener,
            local,
            &scenario,
            deadlines,
            padding,
            &mut counters,
        )
        .await;
        finish_scope(status, scope, result.map(|_| counters), data_phase_mode).await
    } else {
        let result = execute_initiator(
            &service,
            &scope,
            &root,
            local,
            peer.as_ref().expect("initiator peer was validated"),
            &scenario,
            deadlines,
            padding,
            &mut counters,
        )
        .await;
        finish_scope(status, scope, result.map(|_| counters), data_phase_mode).await
    }
}

async fn finish_scope(
    status: StatusWriter,
    scope: i2pr_runtime::ChildScope,
    result: Result<StatusCounters, LauncherError>,
    data_phase_mode: DataPhaseMode,
) -> (
    StatusWriter,
    Result<StatusCounters, LauncherError>,
    DataPhaseMode,
) {
    let cleanup = scope.shutdown().await;
    if cleanup.failed() || cleanup.remaining() != 0 {
        return (status, Err(LauncherError::CleanupFailed), data_phase_mode);
    }
    (status, result, data_phase_mode)
}

#[allow(clippy::too_many_arguments)]
async fn execute_responder(
    service: &Ntcp2RuntimeService,
    scope: &i2pr_runtime::ChildScope,
    cancellation: &CancellationToken,
    listener: &mut i2pr_runtime::ListenerHandle,
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
    let chunk = bounded_timeout(deadlines.handshake, listener.next())
        .await
        .map_err(|_| LauncherError::Timeout)?
        .ok_or(LauncherError::HandshakeFailed)?;
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
    .map_err(|_| LauncherError::HandshakeFailed)?;
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
        .map_err(|_| LauncherError::HandshakeFailed)?;
    counters.authenticated = 1;
    let mut link = service
        .promote_authenticated_inbound(scope, inbound, handshake, 1)
        .map_err(|_| LauncherError::DataPhaseFailed)?;
    let result = exchange_directional(
        &mut link,
        cancellation,
        deadlines,
        counters,
        scenario.data_phase_mode,
    )
    .await;
    link.close();
    result
}

#[allow(clippy::too_many_arguments)]
async fn execute_initiator(
    service: &Ntcp2RuntimeService,
    scope: &i2pr_runtime::ChildScope,
    cancellation: &CancellationToken,
    local: LocalState,
    peer: &PeerState,
    scenario: &Scenario,
    deadlines: Ntcp2RuntimeDeadlines,
    padding: DriverPaddingProfile,
    counters: &mut StatusCounters,
) -> Result<(), LauncherError> {
    let LocalState {
        router_info,
        router_hash: _,
        static_key,
        obfuscation_iv: _,
    } = local;
    let peer_address = SocketAddr::new(
        scenario.peer_address.expect("peer address validated"),
        scenario.peer_port.expect("peer port validated"),
    );
    let attempt = service
        .dial(peer_address, cancellation)
        .await
        .map_err(|outcome| {
            eprintln!("debug: dial outcome {:?}", outcome);
            LauncherError::DialFailed
        })?;
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
    .map_err(|_| LauncherError::HandshakeFailed)?;
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
        .map_err(|_| LauncherError::HandshakeFailed)?;
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
    )
    .await;
    link.close();
    result
}

/// Plan 045 D6: dispatch the data-phase exchange to the typed behavior
/// selected by the scenario.
async fn exchange_directional(
    link: &mut i2pr_runtime::AuthenticatedLink,
    cancellation: &CancellationToken,
    deadlines: Ntcp2RuntimeDeadlines,
    counters: &mut StatusCounters,
    mode: DataPhaseMode,
) -> Result<(), LauncherError> {
    match mode {
        DataPhaseMode::HandshakeOnly => Ok(()),
        DataPhaseMode::InitiatorDataOnly | DataPhaseMode::RoundTripDeliveryStatus => {
            send_i2np_block(link, cancellation, deadlines, counters).await?;
            if matches!(mode, DataPhaseMode::RoundTripDeliveryStatus) {
                receive_delivery_status(link, cancellation, deadlines, counters).await
            } else {
                Ok(())
            }
        }
        DataPhaseMode::ResponderDataOnly => {
            receive_delivery_status(link, cancellation, deadlines, counters).await
        }
    }
}

async fn send_i2np_block(
    link: &mut i2pr_runtime::AuthenticatedLink,
    cancellation: &CancellationToken,
    deadlines: Ntcp2RuntimeDeadlines,
    counters: &mut StatusCounters,
) -> Result<(), LauncherError> {
    let message_id = 0x0420_0001;
    let seconds = unix_seconds();
    let message = I2npMessage::new_short_transport(
        message_id,
        seconds,
        I2npBody::DeliveryStatus(DeliveryStatusMessage::new(
            message_id,
            Date::from_millis(unix_millis()),
        )),
    )
    .map_err(|_| LauncherError::DataPhaseFailed)?;
    let block = I2npMessageBlock::from_bytes(
        message
            .encode_short_transport_to_vec(MAX_I2NP_MESSAGE_BYTES)
            .map_err(|_| LauncherError::DataPhaseFailed)?,
    )
    .map_err(|_| LauncherError::DataPhaseFailed)?;
    let policy = FrameAssemblyPolicy::new(MAX_FRAME_LENGTH, 0, 0, 0, false)
        .map_err(|_| LauncherError::DataPhaseFailed)?;
    let queue_deadline =
        Ntcp2Deadline::after(deadlines.queue_wait).map_err(|_| LauncherError::DataPhaseFailed)?;
    link.send_blocks(
        vec![Block::I2np(block)],
        policy,
        queue_deadline,
        cancellation,
    )
    .await
    .map_err(|_| LauncherError::DataPhaseFailed)?;
    counters.frames_sent = 1;
    counters.i2np_sent = 1;
    Ok(())
}

async fn receive_delivery_status(
    link: &mut i2pr_runtime::AuthenticatedLink,
    cancellation: &CancellationToken,
    deadlines: Ntcp2RuntimeDeadlines,
    counters: &mut StatusCounters,
) -> Result<(), LauncherError> {
    let lease = bounded_timeout(deadlines.read_idle, link.recv(cancellation))
        .await
        .map_err(|_| LauncherError::Timeout)?
        .map_err(|_| LauncherError::DataPhaseFailed)?
        .ok_or(LauncherError::DataPhaseFailed)?;
    counters.frames_received = 1;
    let parsed = lease
        .frame()
        .plaintext()
        .parse()
        .map_err(|_| LauncherError::DataPhaseFailed)?;
    let mut found_delivery_status = false;
    for block in parsed.blocks() {
        if let DecodedBlock::I2np(message) = block {
            let decoded =
                I2npMessage::decode_short_transport(message.as_bytes(), MAX_I2NP_MESSAGE_BYTES)
                    .map_err(|_| LauncherError::DataPhaseFailed)?;
            if decoded.body().message_type() == MessageType::DeliveryStatus {
                found_delivery_status = true;
            }
        }
    }
    if !found_delivery_status {
        return Err(LauncherError::DataPhaseFailed);
    }
    counters.i2np_received = 1;
    Ok(())
}

fn prepare_local_state(scenario: &Scenario) -> Result<LocalState, LauncherError> {
    IdentityStore::prepare_directory(&scenario.state_dir).map_err(map_storage_error)?;
    if let Some(seed) = scenario.deterministic_seed {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        prepare_local_state_with_rng(scenario, &mut rng)
    } else {
        prepare_local_state_with_rng(scenario, &mut OsRng)
    }
}

fn prepare_local_state_with_rng<R>(
    scenario: &Scenario,
    rng: &mut R,
) -> Result<LocalState, LauncherError>
where
    R: rand_core::TryCryptoRng + ?Sized,
{
    let identity_store = IdentityStore::in_data_dir(&scenario.state_dir);
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
    let key_store = TransportStaticKeyStore::in_data_dir(&scenario.state_dir);
    let static_material = if key_store.path().exists() {
        key_store.load().map_err(map_storage_error)?
    } else {
        key_store.generate_new(rng).map_err(map_storage_error)?
    };
    let (static_key, obfuscation_iv) = static_material.into_parts();
    let static_public = static_key.public_bytes();
    let router_info_path = scenario.state_dir.join("router.info");
    let router_info_bytes = if router_info_path.exists() {
        read_private_file(&router_info_path).map_err(|_| LauncherError::StateInvalid)?
    } else {
        let info = signed_router_info(
            &identity,
            scenario.local_address,
            scenario.local_port,
            static_public,
            obfuscation_iv,
        )?;
        let bytes = info
            .encode_to_vec(MAX_LOCAL_ROUTER_INFO_BYTES)
            .map_err(|_| LauncherError::StateInvalid)?;
        write_private_file(&router_info_path, &bytes).map_err(|_| LauncherError::StateInvalid)?;
        bytes
    };
    let info =
        decode_verified_router_info(&router_info_bytes).map_err(|_| LauncherError::StateInvalid)?;
    let expected = SocketAddr::new(scenario.local_address, scenario.local_port);
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
    identity
        .sign_router_info(
            Date::from_millis(unix_millis()),
            vec![router_address],
            Vec::new(),
            Mapping::empty(),
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
        LauncherError::Timeout => (StatusResult::Timeout, StatusReason::Timeout),
        LauncherError::CleanupFailed => {
            (StatusResult::CleanupFailed, StatusReason::CleanupComplete)
        }
        LauncherError::StatusOutputUnavailable => (
            StatusResult::CleanupFailed,
            StatusReason::StatusOutputUnavailable,
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
schema = 1
scenario_id = "launcher-state"
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
}
