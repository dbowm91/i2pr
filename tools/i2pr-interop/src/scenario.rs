//! Strict, non-secret input for one disposable NTCP2 launcher run.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

pub const SCENARIO_SCHEMA: u16 = 1;
pub const MAX_SCENARIO_BYTES: u64 = 64 * 1024;
pub const MAX_SCENARIO_ID_BYTES: usize = 64;
pub const PRIVATE_NETWORK_ID: u16 = 99;
pub const MAX_DEADLINE_MILLIS: u64 = 3_600_000;

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
}

#[derive(Debug, Eq, PartialEq)]
pub enum ScenarioError {
    ReadFailed,
    TooLarge,
    InvalidToml,
    UnsupportedSchema,
    InvalidScenarioId,
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
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDocument {
    scenario: RawScenario,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawScenario {
    schema: u16,
    scenario_id: String,
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
        validate_scenario_id(&raw.scenario_id)?;
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
        let local_address = parse_synthetic_address(&raw.local_address, address_family)?;
        let local_port = validate_port(raw.local_port)?;

        let (peer_address, peer_port) = match (raw.peer_address, raw.peer_port) {
            (Some(address), Some(port)) if !address.is_empty() && port != 0 => {
                let address = parse_synthetic_address(&address, address_family)?;
                let port = validate_port(port)?;
                if address == local_address && port == local_port {
                    return Err(ScenarioError::DuplicateEndpoint);
                }
                (Some(address), Some(port))
            }
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

        Ok(Self {
            scenario_id: raw.scenario_id,
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

fn parse_synthetic_address(value: &str, family: AddressFamily) -> Result<IpAddr, ScenarioError> {
    let address = IpAddr::from_str(value).map_err(|_| ScenarioError::InvalidAddress)?;
    let family_matches = matches!(
        (family, address),
        (AddressFamily::Ipv4, IpAddr::V4(_)) | (AddressFamily::Ipv6, IpAddr::V6(_))
    );
    if !family_matches {
        return Err(ScenarioError::AddressFamilyMismatch);
    }
    let synthetic = match address {
        IpAddr::V4(value) => is_synthetic_ipv4(value),
        IpAddr::V6(value) => is_synthetic_ipv6(value),
    };
    if !synthetic {
        return Err(ScenarioError::AddressOutsideSyntheticRange);
    }
    Ok(address)
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
schema = 1
scenario_id = "synthetic-run"
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
"#;

    #[test]
    fn accepts_bounded_synthetic_initiator() {
        let root = std::env::temp_dir();
        let scenario = Scenario::parse_str(VALID, &root).expect("valid scenario");
        assert_eq!(scenario.network_id, 99);
        assert_eq!(scenario.peer_port, Some(45678));
        assert!(scenario.status_path.starts_with(root));
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
schema = 1
scenario_id = "plan045-direction"
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
}
