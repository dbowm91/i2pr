//! Strict, non-secret input for one disposable NTCP2 launcher run.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

/// Plan 065 scenario contract. The schema is named "i2pr-launcher-scenario-v2";
/// it requires the per-run DeliveryStatus ``message_id`` and the exact
/// 64-lowercase-hex sender/receiver Router Hashes that the i2pr sender and
/// receiver use to validate the data phase. The plan closes the historical
/// schema 1 path; primary directions must use schema v2.
///
/// Plan 086 extends the v2 schema with the optional ``topology_kind``
/// field. The default remains the synthetic RFC 5737 range; the bounded
/// ``host-loopback-development`` topology may carry literal IPv4
/// ``127.0.0.1`` endpoints. The schema keeps the v2 marker and the
/// parser accepts the new field without a schema bump.
pub const SCENARIO_SCHEMA: &str = "i2pr-launcher-scenario-v2";
pub const SCENARIO_SCHEMA_VERSION: u16 = 2;
pub const MAX_SCENARIO_BYTES: u64 = 64 * 1024;
pub const MAX_SCENARIO_ID_BYTES: usize = 64;
pub const PRIVATE_NETWORK_ID: u16 = 99;
pub const MAX_DEADLINE_MILLIS: u64 = 3_600_000;

/// Plan 086: the bounded topology kinds that the strict scenario parser
/// accepts. The synthetic RFC 5737 range is the default; the
/// ``host-loopback-development`` topology is the only path that may
/// carry literal IPv4 loopback addresses. Any other topology value
/// is refused by the strict parser.
pub const HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND: &str = "host-loopback-development";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopologyKind {
    /// Default synthetic RFC 5737 / 3849 range, used by the isolated
    /// Plan 046 rootless sealed-namespace lane and the Plan 080
    /// qualified Multipass guest.
    Synthetic,
    /// Plan 086 development-only lane. Literal IPv4 ``127.0.0.1`` is
    /// accepted only in this topology; the lane is never release or
    /// isolation qualified.
    HostLoopbackDevelopment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Initiator,
    Responder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressFamily {
    Ipv4,
    Ipv6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaddingProfile {
    MinimumVariableMaximum,
    Representative,
    BoundaryAndMaximumPlusOne,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmokeMessageProfile {
    DeliveryStatus,
    Fixed12BytePayload,
}

/// Plan 045 directional data-phase selector. The four allowlisted values map
/// one-to-one to the Python harness enum. The default matches the prior
/// round-trip `DeliveryStatus` behavior so a renderer that does not yet emit
/// the field still passes the strict parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataPhaseMode {
    HandshakeOnly,
    InitiatorDataOnly,
    ResponderDataOnly,
    RoundTripDeliveryStatus,
}

/// Required peer behavior during the data phase. The default value
/// `NonEchoCompletion` documents the no-echo-required behavior explicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataPhasePeerAction {
    ObserveReceive,
    IgnoreReceive,
    NonEchoCompletion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedObservation {
    I2prSentAndAcknowledged,
    I2prReceivedFromPeer,
    I2prSentOnly,
    I2prReceivedOnly,
    NoDataPhaseRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedResultClass {
    AuthenticatedHandshakeAndBoundedI2npExchange,
    AuthenticatedHandshakeAndBoundedI2npExchangeOrEnvironmentSkip,
    AuthenticatedHandshakeAndDirectionalDataPhase,
    TypedRejectionWithBoundedCleanup,
    DeterministicWinnerAndLoserDrain,
}

/// Plan 065 reference driver mode. Only the source-locked direct helpers
/// from Plan 063 (Java) and Plan 064 (i2pd) satisfy a primary direction;
/// SAM, HTTP, I2PControl, support-topology, and synthetic-fallback modes
/// are explicitly rejected for a primary direction by the strict parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceDriverMode {
    JavaDirectDriver,
    I2pdDirectDriver,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeadlineMillis {
    pub handshake: u64,
    pub read: u64,
    pub write: u64,
    pub queue: u64,
    pub drain: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scenario {
    pub scenario_id: String,
    pub run_id: String,
    pub role: Role,
    pub address_family: AddressFamily,
    pub local_address: IpAddr,
    pub local_port: u16,
    pub peer_address: Option<IpAddr>,
    pub peer_port: Option<u16>,
    pub network_id: u8,
    pub run_root: PathBuf,
    pub state_dir: PathBuf,
    pub peer_router_info: Option<PathBuf>,
    pub deadlines: DeadlineMillis,
    pub padding_profile: PaddingProfile,
    pub smoke_message_profile: SmokeMessageProfile,
    pub deterministic_seed: Option<u64>,
    pub expected_result_class: ExpectedResultClass,
    pub status_path: PathBuf,
    pub data_phase_mode: DataPhaseMode,
    pub data_phase_required_peer_action: DataPhasePeerAction,
    pub data_phase_timeout_ms: Option<u64>,
    pub expected_observation: ExpectedObservation,
    /// Plan 065: per-run DeliveryStatus message ID; the i2pr sender and
    /// receiver verify the exact value during the data phase.
    pub delivery_status_message_id: u32,
    /// Plan 065: 64-lowercase-hex Router Hash of the local i2pr router.
    pub expected_sender_router_hash_sha256: String,
    /// Plan 065: 64-lowercase-hex Router Hash of the reference driver.
    pub expected_receiver_router_hash_sha256: String,
    /// Plan 065: reference driver mode. Only the source-locked direct
    /// helpers from Plan 063 (Java) and Plan 064 (i2pd) satisfy a primary
    /// direction; SAM/HTTP/I2PControl/support-topology modes are rejected
    /// by the strict parser for primary directions.
    pub reference_driver_mode: ReferenceDriverMode,
    /// Plan 065: per-run identity digest. The plan052 pipeline cross-checks
    /// every direction record against the recorded run identity.
    pub run_identity_sha256: String,
    /// Plan 086: optional topology kind. The default is the synthetic
    /// RFC 5737 / 3849 range; the only accepted non-default value is
    /// ``host-loopback-development``, which permits literal IPv4
    /// ``127.0.0.1`` endpoints. The field is optional so existing
    /// scenario files do not need to be rewritten.
    pub topology_kind: TopologyKind,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ScenarioError {
    ReadFailed,
    TooLarge,
    InvalidToml,
    UnsupportedSchema,
    InvalidSchemaVersion,
    InvalidScenarioId,
    InvalidRunId,
    InvalidRole,
    InvalidAddressFamily,
    InvalidAddress,
    AddressOutsideSyntheticRange,
    AddressFamilyMismatch,
    InvalidPort,
    MissingPeer,
    UnexpectedPeer,
    DuplicateEndpoint,
    UnsupportedNetworkId,
    InvalidPath,
    StatePathIsFile,
    StatusPathIsDirectory,
    InvalidDeadline,
    InvalidPaddingProfile,
    InvalidSmokeMessageProfile,
    InvalidExpectedResultClass,
    InvalidDataPhaseMode,
    InvalidDataPhasePeerAction,
    InvalidDataPhaseTimeout,
    InvalidExpectedObservation,
    /// Plan 065: DeliveryStatus message ID is zero or out of range.
    InvalidDeliveryStatusMessageId,
    /// Plan 065: expected sender/receiver Router Hash is not 64 lowercase hex.
    InvalidExpectedRouterHash,
    /// Plan 065: reference_driver_mode is not a known source-locked helper.
    InvalidReferenceDriverMode,
    /// Plan 065: reference_driver_mode does not match the scenario direction.
    ReferenceDriverModeDirectionMismatch,
    /// Plan 065: run_identity_sha256 is not a 64-lowercase-hex digest.
    InvalidRunIdentitySha256,
    /// Plan 086: topology_kind is set to a value outside the bounded allowlist.
    InvalidTopologyKind,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDocument {
    scenario: RawScenario,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScenario {
    schema: String,
    schema_version: u16,
    scenario_id: String,
    run_id: String,
    role: String,
    address_family: String,
    local_address: String,
    local_port: u16,
    peer_address: Option<String>,
    peer_port: Option<u16>,
    network_id: u16,
    state_dir: String,
    peer_router_info: Option<String>,
    handshake_deadline_ms: u64,
    read_deadline_ms: u64,
    write_deadline_ms: u64,
    queue_deadline_ms: u64,
    drain_deadline_ms: u64,
    padding_profile: String,
    smoke_message_profile: String,
    deterministic_seed: Option<u64>,
    expected_result_class: String,
    status_path: String,
    data_phase_mode: Option<String>,
    data_phase_required_peer_action: Option<String>,
    data_phase_timeout_ms: Option<u64>,
    expected_observation: Option<String>,
    delivery_status_message_id: u32,
    expected_sender_router_hash_sha256: String,
    expected_receiver_router_hash_sha256: String,
    reference_driver_mode: String,
    run_identity_sha256: String,
    /// Plan 086: optional topology kind. The default is the synthetic
    /// RFC 5737 / 3849 range; the only accepted non-default value is
    /// ``host-loopback-development``.
    topology_kind: Option<String>,
}

impl Scenario {
    pub fn load(path: &Path) -> Result<Self, ScenarioError> {
        let metadata = std::fs::metadata(path).map_err(|_| ScenarioError::ReadFailed)?;
        if !metadata.is_file() {
            return Err(ScenarioError::ReadFailed);
        }
        if metadata.len() > MAX_SCENARIO_BYTES {
            return Err(ScenarioError::TooLarge);
        }
        let contents = std::fs::read_to_string(path).map_err(|_| ScenarioError::ReadFailed)?;
        if contents.len() > MAX_SCENARIO_BYTES as usize {
            return Err(ScenarioError::TooLarge);
        }
        let run_root = path
            .parent()
            .ok_or(ScenarioError::InvalidPath)
            .and_then(|root| std::fs::canonicalize(root).map_err(|_| ScenarioError::InvalidPath))?;
        Self::parse_str(&contents, &run_root)
    }

    pub fn parse_str(contents: &str, run_root: &Path) -> Result<Self, ScenarioError> {
        let raw: RawDocument = toml::from_str(contents).map_err(|_| ScenarioError::InvalidToml)?;
        let run_root = std::fs::canonicalize(run_root).map_err(|_| ScenarioError::InvalidPath)?;
        Self::from_raw(raw.scenario, run_root)
    }

    fn from_raw(raw: RawScenario, run_root: PathBuf) -> Result<Self, ScenarioError> {
        if raw.schema != SCENARIO_SCHEMA {
            return Err(ScenarioError::UnsupportedSchema);
        }
        if raw.schema_version != SCENARIO_SCHEMA_VERSION {
            return Err(ScenarioError::InvalidSchemaVersion);
        }
        validate_scenario_id(&raw.scenario_id)?;
        validate_run_id(&raw.run_id)?;
        let role = match raw.role.as_str() {
            "initiator" => Role::Initiator,
            "responder" => Role::Responder,
            _ => return Err(ScenarioError::InvalidRole),
        };
        let address_family = match raw.address_family.as_str() {
            "ipv4" => AddressFamily::Ipv4,
            "ipv6" => AddressFamily::Ipv6,
            _ => return Err(ScenarioError::InvalidAddressFamily),
        };
        let topology_kind = match raw.topology_kind.as_deref() {
            None => TopologyKind::Synthetic,
            Some(HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND) => TopologyKind::HostLoopbackDevelopment,
            Some(_) => return Err(ScenarioError::InvalidTopologyKind),
        };
        let local_address =
            parse_endpoint_address(&raw.local_address, address_family, topology_kind)?;
        let local_port = validate_port(raw.local_port)?;

        let (peer_address, peer_port) = match (raw.peer_address, raw.peer_port) {
            (Some(address), Some(port)) if !address.is_empty() && port != 0 => {
                let address = parse_endpoint_address(&address, address_family, topology_kind)?;
                let port = validate_port(port)?;
                if address == local_address && port == local_port {
                    return Err(ScenarioError::DuplicateEndpoint);
                }
                (Some(address), Some(port))
            }
            (Some(address), Some(port)) if address.is_empty() || port == 0 => (None, None),
            (None, None) | (Some(_), Some(_)) => (None, None),
            _ => return Err(ScenarioError::MissingPeer),
        };
        match role {
            Role::Initiator if peer_address.is_none() => return Err(ScenarioError::MissingPeer),
            Role::Responder if peer_address.is_some() => return Err(ScenarioError::UnexpectedPeer),
            _ => {}
        }
        if raw.network_id != PRIVATE_NETWORK_ID {
            return Err(ScenarioError::UnsupportedNetworkId);
        }

        let state_dir = confined_path(&run_root, &raw.state_dir)?;
        if state_dir.exists() && !state_dir.is_dir() {
            return Err(ScenarioError::StatePathIsFile);
        }
        let peer_router_info = raw
            .peer_router_info
            .filter(|path| !path.is_empty())
            .map(|path| confined_path(&run_root, &path))
            .transpose()?;
        if matches!(role, Role::Initiator) && peer_router_info.is_none() {
            return Err(ScenarioError::MissingPeer);
        }
        if matches!(role, Role::Responder) && peer_router_info.is_some() {
            return Err(ScenarioError::UnexpectedPeer);
        }

        let deadlines = DeadlineMillis {
            handshake: validate_deadline(raw.handshake_deadline_ms)?,
            read: validate_deadline(raw.read_deadline_ms)?,
            write: validate_deadline(raw.write_deadline_ms)?,
            queue: validate_deadline(raw.queue_deadline_ms)?,
            drain: validate_deadline(raw.drain_deadline_ms)?,
        };
        let padding_profile = match raw.padding_profile.as_str() {
            "minimum-variable-maximum" => PaddingProfile::MinimumVariableMaximum,
            "representative" => PaddingProfile::Representative,
            "boundary-and-maximum-plus-one" => PaddingProfile::BoundaryAndMaximumPlusOne,
            _ => return Err(ScenarioError::InvalidPaddingProfile),
        };
        let smoke_message_profile = match raw.smoke_message_profile.as_str() {
            "delivery-status" => SmokeMessageProfile::DeliveryStatus,
            "fixed-12-byte-payload" => SmokeMessageProfile::Fixed12BytePayload,
            _ => return Err(ScenarioError::InvalidSmokeMessageProfile),
        };
        let expected_result_class = match raw.expected_result_class.as_str() {
            "authenticated-handshake-and-bounded-i2np-exchange" => {
                ExpectedResultClass::AuthenticatedHandshakeAndBoundedI2npExchange
            }
            "authenticated-handshake-and-bounded-i2np-exchange-or-explicit-environment-skip" => {
                ExpectedResultClass::AuthenticatedHandshakeAndBoundedI2npExchangeOrEnvironmentSkip
            }
            "authenticated-handshake-and-directional-data-phase" => {
                ExpectedResultClass::AuthenticatedHandshakeAndDirectionalDataPhase
            }
            "typed-rejection-with-bounded-cleanup" => {
                ExpectedResultClass::TypedRejectionWithBoundedCleanup
            }
            "deterministic-winner-and-loser-drain" => {
                ExpectedResultClass::DeterministicWinnerAndLoserDrain
            }
            _ => return Err(ScenarioError::InvalidExpectedResultClass),
        };
        let data_phase_mode = match raw
            .data_phase_mode
            .as_deref()
            .unwrap_or("round-trip-delivery-status")
        {
            "handshake-only" => DataPhaseMode::HandshakeOnly,
            "initiator-data-only" => DataPhaseMode::InitiatorDataOnly,
            "responder-data-only" => DataPhaseMode::ResponderDataOnly,
            "round-trip-delivery-status" => DataPhaseMode::RoundTripDeliveryStatus,
            _ => return Err(ScenarioError::InvalidDataPhaseMode),
        };
        let data_phase_required_peer_action = match raw
            .data_phase_required_peer_action
            .as_deref()
            .unwrap_or("non-echo-completion")
        {
            "observe-receive" => DataPhasePeerAction::ObserveReceive,
            "ignore-receive" => DataPhasePeerAction::IgnoreReceive,
            "non-echo-completion" => DataPhasePeerAction::NonEchoCompletion,
            _ => return Err(ScenarioError::InvalidDataPhasePeerAction),
        };
        let expected_observation = match raw
            .expected_observation
            .as_deref()
            .unwrap_or("i2pr-sent-and-acknowledged")
        {
            "i2pr-sent-and-acknowledged" => ExpectedObservation::I2prSentAndAcknowledged,
            "i2pr-received-from-peer" => ExpectedObservation::I2prReceivedFromPeer,
            "i2pr-sent-only" => ExpectedObservation::I2prSentOnly,
            "i2pr-received-only" => ExpectedObservation::I2prReceivedOnly,
            "no-data-phase-required" => ExpectedObservation::NoDataPhaseRequired,
            _ => return Err(ScenarioError::InvalidExpectedObservation),
        };
        let data_phase_timeout_ms = match raw.data_phase_timeout_ms {
            Some(value) if value == 0 || value > MAX_DEADLINE_MILLIS => {
                return Err(ScenarioError::InvalidDataPhaseTimeout);
            }
            other => other,
        };
        let status_path = confined_path(&run_root, &raw.status_path)?;
        if status_path.exists() && status_path.is_dir() {
            return Err(ScenarioError::StatusPathIsDirectory);
        }
        if raw.delivery_status_message_id == 0 {
            return Err(ScenarioError::InvalidDeliveryStatusMessageId);
        }
        let expected_sender_router_hash_sha256 =
            validate_router_hash(&raw.expected_sender_router_hash_sha256)?;
        let expected_receiver_router_hash_sha256 =
            validate_router_hash(&raw.expected_receiver_router_hash_sha256)?;
        if raw.expected_sender_router_hash_sha256 == raw.expected_receiver_router_hash_sha256 {
            return Err(ScenarioError::InvalidExpectedRouterHash);
        }
        let reference_driver_mode = match raw.reference_driver_mode.as_str() {
            "java-direct-driver" => ReferenceDriverMode::JavaDirectDriver,
            "i2pd-direct-driver" => ReferenceDriverMode::I2pdDirectDriver,
            _ => return Err(ScenarioError::InvalidReferenceDriverMode),
        };
        // Plan 065: the reference driver mode must match the direction
        // encoded by ``scenario_id``. The four canonical directions end
        // with ``-java-ipv4`` (Java reference) or ``-i2pd-ipv4`` (i2pd
        // reference); any other suffix is rejected because primary
        // directions may not use SAM, HTTP, I2PControl, support-topology,
        // or synthetic fallback helpers.
        let direction_helper_kind = match raw.scenario_id.as_str() {
            "i2pr-to-java-ipv4" | "java-to-i2pr-ipv4" => ReferenceDriverMode::JavaDirectDriver,
            "i2pr-to-i2pd-ipv4" | "i2pd-to-i2pr-ipv4" => ReferenceDriverMode::I2pdDirectDriver,
            _ => return Err(ScenarioError::InvalidReferenceDriverMode),
        };
        if direction_helper_kind != reference_driver_mode {
            return Err(ScenarioError::ReferenceDriverModeDirectionMismatch);
        }
        let run_identity_sha256 = validate_run_identity(&raw.run_identity_sha256)?;

        Ok(Self {
            scenario_id: raw.scenario_id,
            run_id: raw.run_id,
            role,
            address_family,
            local_address,
            local_port,
            peer_address,
            peer_port,
            network_id: raw.network_id as u8,
            run_root,
            state_dir,
            peer_router_info,
            deadlines,
            padding_profile,
            smoke_message_profile,
            deterministic_seed: raw.deterministic_seed,
            expected_result_class,
            status_path,
            data_phase_mode,
            data_phase_required_peer_action,
            data_phase_timeout_ms,
            expected_observation,
            delivery_status_message_id: raw.delivery_status_message_id,
            expected_sender_router_hash_sha256,
            expected_receiver_router_hash_sha256,
            reference_driver_mode,
            run_identity_sha256,
            topology_kind,
        })
    }
}

fn validate_scenario_id(value: &str) -> Result<(), ScenarioError> {
    if value.is_empty()
        || value.len() > MAX_SCENARIO_ID_BYTES
        || value.starts_with('-')
        || value.ends_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(ScenarioError::InvalidScenarioId);
    }
    Ok(())
}

/// Plan 065: the per-run identity is bound into the scenario record. The
/// strict parser enforces the same lowercase hex pattern as the Python
/// harness so the Rust launcher and the Python pipeline agree on the
/// run identifier that the canonical mixed-runner assigns.
fn validate_run_id(value: &str) -> Result<(), ScenarioError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(ScenarioError::InvalidRunId);
    }
    Ok(())
}

/// Plan 065: 64-lowercase-hex validation for expected Router Hash fields.
/// The strict parser refuses any uppercase or non-hex input and any
/// all-zero provenance digest. The helper returns the validated string
/// so the caller can move it into the typed scenario record without
/// re-parsing.
fn validate_router_hash(value: &str) -> Result<String, ScenarioError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ScenarioError::InvalidExpectedRouterHash);
    }
    if value.chars().any(|ch| ch.is_ascii_uppercase()) {
        return Err(ScenarioError::InvalidExpectedRouterHash);
    }
    if value.chars().all(|ch| ch == '0') {
        return Err(ScenarioError::InvalidExpectedRouterHash);
    }
    Ok(value.to_owned())
}

/// Plan 065: 64-lowercase-hex validation for the per-run identity digest.
/// The strict parser refuses uppercase, non-hex, all-zero, or empty input.
/// The Plan 052 run-identity record carries the same shape; the launcher
/// rejects any value that does not round-trip through the Python pipeline.
fn validate_run_identity(value: &str) -> Result<String, ScenarioError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ScenarioError::InvalidRunIdentitySha256);
    }
    if value.chars().any(|ch| ch.is_ascii_uppercase()) {
        return Err(ScenarioError::InvalidRunIdentitySha256);
    }
    if value.chars().all(|ch| ch == '0') {
        return Err(ScenarioError::InvalidRunIdentitySha256);
    }
    Ok(value.to_owned())
}

/// Plan 086: parse a single endpoint address. The synthetic RFC 5737 /
/// 3849 range remains the default; the bounded
/// ``host-loopback-development`` topology is the only topology that
/// accepts literal IPv4 ``127.0.0.1``. Every other topology refuses
/// the address with the existing synthetic-range error so the
/// production daemon and the synthetic lanes never silently accept
/// loopback.
fn parse_endpoint_address(
    value: &str,
    family: AddressFamily,
    topology_kind: TopologyKind,
) -> Result<IpAddr, ScenarioError> {
    let address = IpAddr::from_str(value).map_err(|_| ScenarioError::InvalidAddress)?;
    let family_matches = matches!(
        (family, address),
        (AddressFamily::Ipv4, IpAddr::V4(_)) | (AddressFamily::Ipv6, IpAddr::V6(_))
    );
    if !family_matches {
        return Err(ScenarioError::AddressFamilyMismatch);
    }
    match topology_kind {
        TopologyKind::Synthetic => {
            let synthetic = match address {
                IpAddr::V4(value) => is_synthetic_ipv4(value),
                IpAddr::V6(value) => is_synthetic_ipv6(value),
            };
            if !synthetic {
                return Err(ScenarioError::AddressOutsideSyntheticRange);
            }
            Ok(address)
        }
        TopologyKind::HostLoopbackDevelopment => {
            // The host-loopback-development lane is IPv4 only and
            // accepts the literal ``127.0.0.1`` address only. RFC 5737
            // synthetic addresses are explicitly rejected under this
            // topology so the development-only lane never silently
            // falls back to the synthetic range.
            match address {
                IpAddr::V4(value) if value == Ipv4Addr::LOCALHOST => Ok(address),
                IpAddr::V4(_) => Err(ScenarioError::AddressOutsideSyntheticRange),
                IpAddr::V6(_) => Err(ScenarioError::AddressOutsideSyntheticRange),
            }
        }
    }
}

fn is_synthetic_ipv4(value: Ipv4Addr) -> bool {
    let octets = value.octets();
    octets[..3] == [192, 0, 2] && octets[3] != 0
}

fn is_synthetic_ipv6(value: Ipv6Addr) -> bool {
    let address = u128::from(value);
    let prefix = u128::from(Ipv6Addr::new(0x2001, 0xdb8, 0x36, 0, 0, 0, 0, 0));
    address & (!0_u128 << 64) == prefix && address != 0
}

fn validate_port(value: u16) -> Result<u16, ScenarioError> {
    if value == 0 {
        Err(ScenarioError::InvalidPort)
    } else {
        Ok(value)
    }
}

fn validate_deadline(value: u64) -> Result<u64, ScenarioError> {
    if value == 0 || value > MAX_DEADLINE_MILLIS {
        Err(ScenarioError::InvalidDeadline)
    } else {
        Ok(value)
    }
}

fn confined_path(run_root: &Path, value: &str) -> Result<PathBuf, ScenarioError> {
    if value.is_empty() || value.as_bytes().contains(&0) {
        return Err(ScenarioError::InvalidPath);
    }
    let relative = Path::new(value);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ScenarioError::InvalidPath);
    }
    let candidate = run_root.join(relative);
    let existing = if candidate.exists() {
        candidate.clone()
    } else {
        candidate
            .ancestors()
            .find(|path| path.exists())
            .ok_or(ScenarioError::InvalidPath)?
            .to_path_buf()
    };
    let canonical = std::fs::canonicalize(existing).map_err(|_| ScenarioError::InvalidPath)?;
    if !canonical.starts_with(run_root) {
        return Err(ScenarioError::InvalidPath);
    }
    Ok(run_root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-java-ipv4"
run_id = "test-run-id"
role = "initiator"
address_family = "ipv4"
local_address = "192.0.2.1"
local_port = 45680
peer_address = "192.0.2.2"
peer_port = 45678
network_id = 99
state_dir = "secrets"
peer_router_info = "exchange/peer.info"
handshake_deadline_ms = 30000
read_deadline_ms = 1000
write_deadline_ms = 1000
queue_deadline_ms = 1000
drain_deadline_ms = 1000
padding_profile = "representative"
smoke_message_profile = "delivery-status"
deterministic_seed = 1
expected_result_class = "authenticated-handshake-and-bounded-i2np-exchange"
status_path = "status.jsonl"
delivery_status_message_id = 12345
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "java-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
"#;

    #[test]
    fn accepts_bounded_synthetic_initiator() {
        let root = std::env::temp_dir();
        let scenario = Scenario::parse_str(VALID, &root).expect("valid scenario");
        assert_eq!(scenario.network_id, 99);
        assert_eq!(scenario.peer_port, Some(45678));
        let canonical_root = std::fs::canonicalize(&root).unwrap_or(root.clone());
        assert!(scenario.status_path.starts_with(&canonical_root));
        assert_eq!(
            scenario.data_phase_mode,
            DataPhaseMode::RoundTripDeliveryStatus
        );
        assert_eq!(
            scenario.data_phase_required_peer_action,
            DataPhasePeerAction::NonEchoCompletion
        );
        assert_eq!(
            scenario.expected_observation,
            ExpectedObservation::I2prSentAndAcknowledged
        );
        assert_eq!(scenario.delivery_status_message_id, 12345);
        assert_eq!(scenario.run_id, "test-run-id");
        assert_eq!(
            scenario.reference_driver_mode,
            ReferenceDriverMode::JavaDirectDriver
        );
        assert_eq!(
            scenario.expected_sender_router_hash_sha256,
            "1111111111111111111111111111111111111111111111111111111111111111"
        );
        assert_eq!(
            scenario.expected_receiver_router_hash_sha256,
            "2222222222222222222222222222222222222222222222222222222222222222"
        );
    }

    #[test]
    fn rejects_unknown_fields() {
        let input = VALID.replace(
            "status_path = \"status.jsonl\"",
            "status_path = \"status.jsonl\"\nextra = true",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidToml)
        );
    }

    #[test]
    fn rejects_path_escape_and_public_address() {
        let escaped = VALID.replace("state_dir = \"secrets\"", "state_dir = \"../secrets\"");
        assert_eq!(
            Scenario::parse_str(&escaped, &std::env::temp_dir()),
            Err(ScenarioError::InvalidPath)
        );
        let public = VALID.replace(
            "local_address = \"192.0.2.1\"",
            "local_address = \"10.0.0.1\"",
        );
        assert_eq!(
            Scenario::parse_str(&public, &std::env::temp_dir()),
            Err(ScenarioError::AddressOutsideSyntheticRange)
        );
    }

    #[test]
    fn accepts_optional_plan_045_data_phase_fields() {
        let input = r#"
[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-java-ipv4"
run_id = "test-run-id"
role = "initiator"
address_family = "ipv4"
local_address = "192.0.2.1"
local_port = 45680
peer_address = "192.0.2.2"
peer_port = 45678
network_id = 99
state_dir = "secrets"
peer_router_info = "exchange/peer.info"
handshake_deadline_ms = 30000
read_deadline_ms = 1000
write_deadline_ms = 1000
queue_deadline_ms = 1000
drain_deadline_ms = 1000
padding_profile = "representative"
smoke_message_profile = "fixed-12-byte-payload"
deterministic_seed = 1
expected_result_class = "authenticated-handshake-and-directional-data-phase"
status_path = "status.jsonl"
data_phase_mode = "initiator-data-only"
data_phase_required_peer_action = "ignore-receive"
data_phase_timeout_ms = 4000
expected_observation = "i2pr-sent-only"
delivery_status_message_id = 42
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "3333333333333333333333333333333333333333333333333333333333333333"
reference_driver_mode = "java-direct-driver"
run_identity_sha256 = "feedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface"
"#;
        let scenario = Scenario::parse_str(input, &std::env::temp_dir()).expect("plan045 scenario");
        assert_eq!(scenario.data_phase_mode, DataPhaseMode::InitiatorDataOnly);
        assert_eq!(
            scenario.data_phase_required_peer_action,
            DataPhasePeerAction::IgnoreReceive
        );
        assert_eq!(scenario.data_phase_timeout_ms, Some(4000));
        assert_eq!(
            scenario.expected_observation,
            ExpectedObservation::I2prSentOnly
        );
        assert_eq!(
            scenario.expected_result_class,
            ExpectedResultClass::AuthenticatedHandshakeAndDirectionalDataPhase
        );
        assert_eq!(
            scenario.smoke_message_profile,
            SmokeMessageProfile::Fixed12BytePayload
        );
        assert_eq!(scenario.delivery_status_message_id, 42);
    }

    #[test]
    fn rejects_invalid_optional_plan_045_fields() {
        let input = VALID.replace(
            "status_path = \"status.jsonl\"",
            "status_path = \"status.jsonl\"\ndata_phase_mode = \"bogus\"",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidDataPhaseMode)
        );
        let input = VALID.replace(
            "status_path = \"status.jsonl\"",
            "status_path = \"status.jsonl\"\ndata_phase_timeout_ms = 0",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidDataPhaseTimeout)
        );
    }

    #[test]
    fn rejects_zero_delivery_status_message_id() {
        let input = VALID.replace(
            "delivery_status_message_id = 12345",
            "delivery_status_message_id = 0",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidDeliveryStatusMessageId)
        );
    }

    #[test]
    fn rejects_short_router_hash() {
        let input = VALID.replace(
            "expected_sender_router_hash_sha256 = \"1111111111111111111111111111111111111111111111111111111111111111\"",
            "expected_sender_router_hash_sha256 = \"abc\"",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidExpectedRouterHash)
        );
    }

    #[test]
    fn rejects_uppercase_router_hash() {
        let input = VALID.replace(
            "expected_sender_router_hash_sha256 = \"1111111111111111111111111111111111111111111111111111111111111111\"",
            "expected_sender_router_hash_sha256 = \"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidExpectedRouterHash)
        );
    }

    #[test]
    fn rejects_unknown_reference_driver_mode() {
        let input = VALID.replace(
            "reference_driver_mode = \"java-direct-driver\"",
            "reference_driver_mode = \"sam-trigger\"",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidReferenceDriverMode)
        );
    }

    #[test]
    fn rejects_reference_driver_mode_direction_mismatch() {
        let input = VALID.replace(
            "scenario_id = \"i2pr-to-java-ipv4\"",
            "scenario_id = \"i2pr-to-i2pd-ipv4\"",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::ReferenceDriverModeDirectionMismatch)
        );
    }

    #[test]
    fn rejects_all_zero_run_identity() {
        let input = VALID.replace(
            "run_identity_sha256 = \"deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\"",
            "run_identity_sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidRunIdentitySha256)
        );
    }

    #[test]
    fn rejects_legacy_schema_marker() {
        let input = VALID.replace(
            "schema = \"i2pr-launcher-scenario-v2\"",
            "schema = \"i2pr-legacy-scenario-v1\"",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::UnsupportedSchema)
        );
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let input = VALID.replace("schema_version = 2", "schema_version = 1");
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidSchemaVersion)
        );
    }

    /// Plan 086: ``host-loopback-development`` topology accepts literal
    /// IPv4 ``127.0.0.1`` endpoints. The default synthetic range is
    /// still accepted under the development topology so existing
    /// flows can switch lanes without rewriting the scenario.
    #[test]
    fn accepts_loopback_endpoints_only_in_host_loopback_topology() {
        let input = r#"
[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-i2pd-ipv4"
run_id = "test-run-id"
role = "initiator"
address_family = "ipv4"
local_address = "127.0.0.1"
local_port = 45680
peer_address = "127.0.0.1"
peer_port = 45678
network_id = 99
state_dir = "secrets"
peer_router_info = "exchange/peer.info"
handshake_deadline_ms = 30000
read_deadline_ms = 1000
write_deadline_ms = 1000
queue_deadline_ms = 1000
drain_deadline_ms = 1000
padding_profile = "representative"
smoke_message_profile = "delivery-status"
deterministic_seed = 1
expected_result_class = "authenticated-handshake-and-bounded-i2np-exchange"
status_path = "status.jsonl"
delivery_status_message_id = 12345
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "i2pd-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
topology_kind = "host-loopback-development"
"#;
        let scenario =
            Scenario::parse_str(input, &std::env::temp_dir()).expect("loopback scenario");
        assert_eq!(
            scenario.topology_kind,
            TopologyKind::HostLoopbackDevelopment
        );
        assert_eq!(scenario.local_address.to_string(), "127.0.0.1");
        assert_eq!(
            scenario.peer_address.expect("peer").to_string(),
            "127.0.0.1"
        );
    }

    /// Plan 086: literal IPv4 ``127.0.0.1`` is rejected when the
    /// topology is left at the default synthetic range. The existing
    /// production daemon and the synthetic lanes remain untouched.
    #[test]
    fn rejects_loopback_endpoint_for_default_topology() {
        let input = VALID.replace(
            "local_address = \"192.0.2.1\"",
            "local_address = \"127.0.0.1\"",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::AddressOutsideSyntheticRange)
        );
    }

    /// Plan 086: synthetic RFC 5737 addresses (``192.0.2.x``) are
    /// rejected under the host-loopback-development topology. The
    /// development lane accepts literal ``127.0.0.1`` only.
    #[test]
    fn rejects_synthetic_address_in_host_loopback_topology() {
        let input = r#"
[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-i2pd-ipv4"
run_id = "test-run-id"
role = "initiator"
address_family = "ipv4"
local_address = "192.0.2.1"
local_port = 45680
peer_address = "127.0.0.1"
peer_port = 45678
network_id = 99
state_dir = "secrets"
peer_router_info = "exchange/peer.info"
handshake_deadline_ms = 30000
read_deadline_ms = 1000
write_deadline_ms = 1000
queue_deadline_ms = 1000
drain_deadline_ms = 1000
padding_profile = "representative"
smoke_message_profile = "delivery-status"
deterministic_seed = 1
expected_result_class = "authenticated-handshake-and-bounded-i2np-exchange"
status_path = "status.jsonl"
delivery_status_message_id = 12345
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "i2pd-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
topology_kind = "host-loopback-development"
"#;
        assert_eq!(
            Scenario::parse_str(input, &std::env::temp_dir()),
            Err(ScenarioError::AddressOutsideSyntheticRange)
        );
    }

    /// Plan 086: alternate loopback addresses such as ``127.0.0.2``
    /// and ``::1`` are rejected even under the development topology.
    /// Only literal ``127.0.0.1`` is accepted.
    #[test]
    fn rejects_alternate_loopback_addresses_in_host_loopback_topology() {
        let input = r#"
[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-i2pd-ipv4"
run_id = "test-run-id"
role = "initiator"
address_family = "ipv4"
local_address = "127.0.0.2"
local_port = 45680
peer_address = "127.0.0.1"
peer_port = 45678
network_id = 99
state_dir = "secrets"
peer_router_info = "exchange/peer.info"
handshake_deadline_ms = 30000
read_deadline_ms = 1000
write_deadline_ms = 1000
queue_deadline_ms = 1000
drain_deadline_ms = 1000
padding_profile = "representative"
smoke_message_profile = "delivery-status"
deterministic_seed = 1
expected_result_class = "authenticated-handshake-and-bounded-i2np-exchange"
status_path = "status.jsonl"
delivery_status_message_id = 12345
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "i2pd-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
topology_kind = "host-loopback-development"
"#;
        assert_eq!(
            Scenario::parse_str(input, &std::env::temp_dir()),
            Err(ScenarioError::AddressOutsideSyntheticRange)
        );
    }

    /// Plan 086: IPv6 addresses are rejected under the development
    /// topology; the lane is IPv4-only.
    #[test]
    fn rejects_ipv6_endpoint_in_host_loopback_topology() {
        let input = r#"
[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-i2pd-ipv4"
run_id = "test-run-id"
role = "responder"
address_family = "ipv6"
local_address = "::1"
local_port = 45680
network_id = 99
state_dir = "secrets"
handshake_deadline_ms = 30000
read_deadline_ms = 1000
write_deadline_ms = 1000
queue_deadline_ms = 1000
drain_deadline_ms = 1000
padding_profile = "representative"
smoke_message_profile = "delivery-status"
deterministic_seed = 1
expected_result_class = "authenticated-handshake-and-bounded-i2np-exchange"
status_path = "status.jsonl"
delivery_status_message_id = 12345
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "i2pd-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
topology_kind = "host-loopback-development"
"#;
        assert_eq!(
            Scenario::parse_str(input, &std::env::temp_dir()),
            Err(ScenarioError::AddressOutsideSyntheticRange)
        );
    }

    /// Plan 086: unknown topology_kind values are rejected.
    #[test]
    fn rejects_unknown_topology_kind() {
        let input = VALID.replace(
            "schema = \"i2pr-launcher-scenario-v2\"",
            "schema = \"i2pr-launcher-scenario-v2\"\ntopology_kind = \"unknown-topology\"",
        );
        assert_eq!(
            Scenario::parse_str(&input, &std::env::temp_dir()),
            Err(ScenarioError::InvalidTopologyKind)
        );
    }
}
