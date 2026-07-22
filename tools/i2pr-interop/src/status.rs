//! Versioned, redacted launcher status records.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};

use crate::scenario::Scenario;

pub const STATUS_SCHEMA: u8 = 1;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusPhase {
    ListenerReady,
    Terminal,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusResult {
    Ready,
    Passed,
    Blocked,
    Rejected,
    Timeout,
    AuthenticationFailed,
    CleanupFailed,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusReason {
    ListenerBound,
    StateInvalid,
    PeerRouterInfoInvalid,
    UnsupportedPaddingProfile,
    ListenerFailed,
    HandshakeAuthenticated,
    I2npExchangeComplete,
    DirectionalDataPhaseComplete,
    HandshakeFailed,
    DialFailed,
    DataPhaseFailed,
    DataPhaseTimeout,
    DataPhaseObservationIncomplete,
    Timeout,
    CleanupComplete,
    InvalidScenarioConfig,
    ScenarioRoleMismatch,
    StatusOutputUnavailable,
    // Plan 052 G1: split the broad responder-handshake-failed reason
    // into bounded responder-stage classification. These are emitted only
    // by the responder side and only on a Terminal phase.
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
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StatusCounters {
    pub listener_ready: u32,
    pub authenticated: u32,
    pub frames_sent: u32,
    pub frames_received: u32,
    pub i2np_sent: u32,
    pub i2np_received: u32,
}

pub struct StatusWriter {
    file: File,
    scenario_id: String,
}

impl StatusWriter {
    pub fn new(scenario: &Scenario) -> io::Result<Self> {
        if let Some(parent) = scenario.status_path.parent() {
            std::fs::create_dir_all(parent)?;
            set_private_directory_permissions(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&scenario.status_path)?;
        set_private_permissions(&file)?;
        file.flush()?;
        Ok(Self {
            file,
            scenario_id: scenario.scenario_id.clone(),
        })
    }

    pub fn emit(
        &mut self,
        phase: StatusPhase,
        result: StatusResult,
        reason: StatusReason,
        counters: StatusCounters,
    ) -> io::Result<()> {
        let line = status_json(&self.scenario_id, phase, result, reason, counters);
        self.file.write_all(line.as_bytes())?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        emit_stdout_line(&line)
    }
}

pub fn emit_stdout_status(
    scenario_id: &str,
    phase: StatusPhase,
    result: StatusResult,
    reason: StatusReason,
    counters: StatusCounters,
) -> io::Result<()> {
    emit_stdout_line(&status_json(scenario_id, phase, result, reason, counters))
}

fn emit_stdout_line(line: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(line.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

fn status_json(
    scenario_id: &str,
    phase: StatusPhase,
    result: StatusResult,
    reason: StatusReason,
    counters: StatusCounters,
) -> String {
    format!(
        "{{\"schema\":{STATUS_SCHEMA},\"type\":\"i2pr-interop-status\",\"scenario_id\":\"{scenario_id}\",\"phase\":\"{}\",\"result\":\"{}\",\"reason_code\":\"{}\",\"counters\":{{\"listener_ready\":{},\"authenticated\":{},\"frames_sent\":{},\"frames_received\":{},\"i2np_sent\":{},\"i2np_received\":{}}}}}",
        phase_name(phase),
        result_name(result),
        reason_name(reason),
        counters.listener_ready,
        counters.authenticated,
        counters.frames_sent,
        counters.frames_received,
        counters.i2np_sent,
        counters.i2np_received,
    )
}

fn phase_name(value: StatusPhase) -> &'static str {
    match value {
        StatusPhase::ListenerReady => "listener_ready",
        StatusPhase::Terminal => "terminal",
    }
}

fn result_name(value: StatusResult) -> &'static str {
    match value {
        StatusResult::Ready => "ready",
        StatusResult::Passed => "passed",
        StatusResult::Blocked => "blocked",
        StatusResult::Rejected => "rejected",
        StatusResult::Timeout => "timeout",
        StatusResult::AuthenticationFailed => "authentication_failed",
        StatusResult::CleanupFailed => "cleanup_failed",
    }
}

fn reason_name(value: StatusReason) -> &'static str {
    match value {
        StatusReason::ListenerBound => "listener_bound",
        StatusReason::StateInvalid => "state_invalid",
        StatusReason::PeerRouterInfoInvalid => "peer_router_info_invalid",
        StatusReason::UnsupportedPaddingProfile => "unsupported_padding_profile",
        StatusReason::ListenerFailed => "listener_failed",
        StatusReason::HandshakeAuthenticated => "handshake_authenticated",
        StatusReason::I2npExchangeComplete => "i2np_exchange_complete",
        StatusReason::DirectionalDataPhaseComplete => "directional_data_phase_complete",
        StatusReason::DataPhaseTimeout => "data_phase_timeout",
        StatusReason::DataPhaseObservationIncomplete => "data_phase_observation_incomplete",
        StatusReason::HandshakeFailed => "handshake_failed",
        StatusReason::DialFailed => "dial_failed",
        StatusReason::DataPhaseFailed => "data_phase_failed",
        StatusReason::Timeout => "timeout",
        StatusReason::CleanupComplete => "cleanup_complete",
        StatusReason::InvalidScenarioConfig => "invalid_scenario_config",
        StatusReason::ScenarioRoleMismatch => "scenario_role_mismatch",
        StatusReason::StatusOutputUnavailable => "status_output_unavailable",
        StatusReason::ResponderTcpAcceptMissing => "responder_tcp_accept_missing",
        StatusReason::ResponderAdmissionRejected => "responder_admission_rejected",
        StatusReason::ResponderMessage1DecodeFailed => "responder_message1_decode_failed",
        StatusReason::ResponderMessage1OptionsInvalid => "responder_message1_options_invalid",
        StatusReason::ResponderNoiseStateFailed => "responder_noise_state_failed",
        StatusReason::ResponderSessionCreatedWriteFailed => "responder_session_created_write_failed",
        StatusReason::ResponderSessionConfirmedPart1Failed => "responder_session_confirmed_part1_failed",
        StatusReason::ResponderSessionConfirmedPart2Failed => "responder_session_confirmed_part2_failed",
        StatusReason::ResponderRouterIdentityVerificationFailed => {
            "responder_router_identity_verification_failed"
        }
        StatusReason::ResponderHandshakeTimeout => "responder_handshake_timeout",
        StatusReason::ResponderAuthenticatedLinkInstallFailed => {
            "responder_authenticated_link_install_failed"
        }
        StatusReason::ResponderDataFrameReadFailed => "responder_data_frame_read_failed",
        StatusReason::ResponderI2npDecodeFailed => "responder_i2np_decode_failed",
    }
}

fn set_private_permissions(file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn set_private_directory_permissions(path: &std::path::Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{
        AddressFamily, DataPhaseMode, DataPhasePeerAction, DeadlineMillis, ExpectedObservation,
        ExpectedResultClass, PaddingProfile, Role, SmokeMessageProfile,
    };
    use std::net::IpAddr;

    #[test]
    fn status_is_versioned_and_contains_only_fixed_categories() {
        let json = status_json(
            "synthetic-run",
            StatusPhase::Terminal,
            StatusResult::Blocked,
            StatusReason::StateInvalid,
            StatusCounters::default(),
        );
        assert!(json.contains("\"schema\":1"));
        assert!(json.contains("\"phase\":\"terminal\""));
        assert!(json.contains("\"result\":\"blocked\""));
        assert!(json.contains("state_invalid"));
        assert!(!json.contains("/"));
        assert!(!json.contains("blocked_missing_driver"));
    }

    #[test]
    fn status_writer_flushes_a_private_run_root_record() {
        let root = std::env::temp_dir().join(format!(
            "i2pr-status-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("test root");
        let scenario = Scenario {
            scenario_id: "synthetic-run".to_owned(),
            role: Role::Responder,
            address_family: AddressFamily::Ipv4,
            local_address: "192.0.2.1".parse::<IpAddr>().expect("address"),
            local_port: 45680,
            peer_address: None,
            peer_port: None,
            network_id: 99,
            run_root: root.clone(),
            state_dir: root.join("state"),
            peer_router_info: None,
            deadlines: DeadlineMillis {
                handshake: 1_000,
                read: 1_000,
                write: 1_000,
                queue: 1_000,
                drain: 1_000,
            },
            padding_profile: PaddingProfile::Representative,
            smoke_message_profile: SmokeMessageProfile::DeliveryStatus,
            deterministic_seed: None,
            expected_result_class: ExpectedResultClass::TypedRejectionWithBoundedCleanup,
            status_path: root.join("status/status.jsonl"),
            data_phase_mode: DataPhaseMode::RoundTripDeliveryStatus,
            data_phase_required_peer_action: DataPhasePeerAction::NonEchoCompletion,
            data_phase_timeout_ms: None,
            expected_observation: ExpectedObservation::I2prSentAndAcknowledged,
        };
        let mut writer = StatusWriter::new(&scenario).expect("status writer");
        writer
            .emit(
                StatusPhase::Terminal,
                StatusResult::Blocked,
                StatusReason::StateInvalid,
                StatusCounters::default(),
            )
            .expect("status record");
        let contents = std::fs::read_to_string(&scenario.status_path).expect("status file");
        assert_eq!(contents.lines().count(), 1);
        assert!(contents.contains("state_invalid"));
        std::fs::remove_dir_all(root).expect("test cleanup");
    }
}
