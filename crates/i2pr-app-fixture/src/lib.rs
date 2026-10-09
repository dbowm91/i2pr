//! Plan 369 §G — the managed-application fixture's scenario engine.
//!
//! # What this is, and what it is not
//!
//! This crate is **evidence tooling**. The binary in `src/main.rs` is a real
//! native process that `i2pr-apphost` execs directly, and it speaks the real
//! bootstrap contract and the real managed-app v1 wire protocol on stdin/stdout.
//! That is what makes the WP5 qualification black-box: nothing here reaches into
//! manager or daemon internals, so a passing transcript is evidence about the
//! product path rather than about a test double.
//!
//! It is **not** an application the router can be made to launch. Two independent
//! facts keep it that way, and both are asserted rather than assumed:
//!
//! 1. This crate is not a dependency of any production crate, and
//!    `scripts/check-dependency-direction.sh` fails closed on a new edge into it.
//! 2. The shipped manager never names or bundles this executable. It can only
//!    run after an operator explicitly packages, signs, installs, trusts,
//!    selects, grants, and enables autostart for it; the qualification does
//!    those steps in a temporary local store. The process-boundary checker
//!    asserts that no production module can name the fixture.
//!
//! # Why scenarios are data
//!
//! Every qualification in Plan 369 §G is a *deterministic* behavior: correct or
//! wrong hello, a permitted or denied capability, a SAM or I2CP open, a malformed
//! or oversized frame, an early close, a shutdown hang, a stderr flood, a
//! sibling stream-id misuse. Encoding those as named scenarios rather than as
//! ad-hoc per-test logic means the qualification surface is enumerable and can be
//! asserted complete — [`Scenario::ALL`] is the list, and
//! [`Scenario::parse`] is the only way a scenario is ever named.

/// Ceiling on bytes one fixture frame may put on the wire.
///
/// The fixture deliberately writes past the product's own
/// [`i2pr_app_proto::MAX_FRAME_PAYLOAD_BYTES`] to prove the oversize path is
/// actually reached. It is still bounded here: an unbounded write would prove
/// nothing a bounded one does not, and would turn a defect into a hang.
pub const FIXTURE_MAX_FRAME_PAYLOAD: usize = 128 * 1024;

/// Total bytes the stderr-flood scenario writes.
///
/// This is deliberately several multiples of the daemon's retained stderr
/// ceiling ([`STDERR_FLOOD_FACTOR`] documents why) so the *truncation* path is
/// exercised rather than merely the write path.
pub const STDERR_FLOOD_BYTES: usize = 512 * 1024;

/// How far past the retained-stderr ceiling the flood scenario writes.
///
/// The daemon retains a bounded prefix and accounts for the rest; a flood that
/// only reached the ceiling would not prove the accounting exists.
pub const STDERR_FLOOD_FACTOR: usize = 64;

/// One deterministic fixture behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scenario {
    /// Correct hello, then a SAM `HELLO`/`SESSION CREATE` round trip.
    SamHappyPath,
    /// Correct hello, then an I2CP `GetDate` round trip.
    I2cpHappyPath,
    /// A hello whose declared identity does not match the launch authority.
    HelloWrongIdentity,
    /// A non-hello control frame sent before `hello`.
    HelloNotFirst,
    /// A request for a capability the launch was never granted.
    DeniedCapability,
    /// Opens a service the launch was not granted, after a correct hello.
    DeniedService,
    /// A frame whose declared payload length exceeds the protocol ceiling.
    OversizedFrame,
    /// Bytes that are not a decodable frame at all.
    MalformedFrame,
    /// A correct hello followed immediately by process exit.
    CloseEarly,
    /// A correct hello, then a stream that never closes, so the manager's
    /// bounded shutdown deadline is the thing that ends it.
    ShutdownHang,
    /// Writes far more to stderr than the daemon retains.
    StderrFlood,
    /// Issues control traffic against a stream id this app never opened.
    SiblingStreamIdMisuse,
    /// Correct hello, then two SAM sessions declaring the *same* SAM session id,
    /// used to prove two instances stay isolated.
    DuplicateSamSessionId,
    /// Correct hello, then a data frame on a stream that was never opened.
    DataBeforeOpen,
    /// Opens one stream id twice; the second open must be refused.
    DuplicateStreamId,
}

impl Scenario {
    /// Every scenario this fixture can perform.
    ///
    /// WP5 asserts against this list, so removing a scenario is a deliberate,
    /// visible edit rather than a silent loss of qualification coverage.
    pub const ALL: &'static [Scenario] = &[
        Scenario::SamHappyPath,
        Scenario::I2cpHappyPath,
        Scenario::HelloWrongIdentity,
        Scenario::HelloNotFirst,
        Scenario::DeniedCapability,
        Scenario::DeniedService,
        Scenario::OversizedFrame,
        Scenario::MalformedFrame,
        Scenario::CloseEarly,
        Scenario::ShutdownHang,
        Scenario::StderrFlood,
        Scenario::SiblingStreamIdMisuse,
        Scenario::DuplicateSamSessionId,
        Scenario::DataBeforeOpen,
        Scenario::DuplicateStreamId,
    ];

    /// The stable wire name used in argv and in evidence files.
    pub const fn name(self) -> &'static str {
        match self {
            Scenario::SamHappyPath => "sam-happy-path",
            Scenario::I2cpHappyPath => "i2cp-happy-path",
            Scenario::HelloWrongIdentity => "hello-wrong-identity",
            Scenario::HelloNotFirst => "hello-not-first",
            Scenario::DeniedCapability => "denied-capability",
            Scenario::DeniedService => "denied-service",
            Scenario::OversizedFrame => "oversized-frame",
            Scenario::MalformedFrame => "malformed-frame",
            Scenario::CloseEarly => "close-early",
            Scenario::ShutdownHang => "shutdown-hang",
            Scenario::StderrFlood => "stderr-flood",
            Scenario::SiblingStreamIdMisuse => "sibling-stream-id-misuse",
            Scenario::DuplicateSamSessionId => "duplicate-sam-session-id",
            Scenario::DataBeforeOpen => "data-before-open",
            Scenario::DuplicateStreamId => "duplicate-stream-id",
        }
    }

    /// Parses a scenario name.
    ///
    /// Unknown input is an error rather than a default. A fixture that silently
    /// fell back to its happy path would turn a typo in an evidence run into a
    /// green transcript for a behavior nobody asked about.
    pub fn parse(value: &str) -> Option<Self> {
        Scenario::ALL
            .iter()
            .copied()
            .find(|scenario| scenario.name() == value)
    }

    /// Whether this scenario is expected to keep running until its host stops it.
    ///
    /// The manager's bounded shutdown deadline only matters for a scenario that
    /// does not end on its own, so this is what distinguishes the hang case from
    /// every other.
    pub const fn hangs_until_stopped(self) -> bool {
        matches!(self, Scenario::ShutdownHang)
    }
}

/// Refusal returned instead of guessing a scenario.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum FixtureError {
    /// No scenario argument was supplied.
    #[error("the fixture requires exactly one --scenario=<name> argument")]
    MissingScenario,
    /// A required option was absent.
    #[error("the fixture requires {0}=<value>")]
    MissingOption(&'static str),
    /// The scenario name is not one this build knows.
    #[error("unknown fixture scenario: {0}")]
    UnknownScenario(String),
    /// More than one scenario was requested, which is not expressible.
    #[error("the fixture performs exactly one scenario per process")]
    ConflictingScenarios,
    /// An argument other than `--scenario=<name>` was supplied.
    #[error("unsupported fixture argument: {0}")]
    UnsupportedArgument(String),
    /// The wire codec refused a frame the fixture tried to build.
    #[error("fixture framing failed: {0}")]
    Framing(String),
    /// Reading or writing the inherited protocol stream failed.
    #[error("fixture transport failed: {0}")]
    Transport(String),
    /// The evidence transcript could not be written.
    ///
    /// This is fatal rather than logged: a fixture that cannot record what it
    /// observed would exit zero and produce evidence that silently lacks the
    /// thing being claimed.
    #[error("fixture transcript failed: {0}")]
    Transcript(String),
}

/// The closed argument grammar shared by the fixture application and the fixture
/// manager.
///
/// Both binaries parse the *same* grammar, and neither accepts anything else. A
/// managed application has no configuration surface in Plan 369, so an extra
/// argument is refused rather than ignored: from outside, an ignored argument is
/// indistinguishable from one that was understood and had no effect.
///
/// The fixture application receives these from its launch authority's `argv`,
/// which is how it learns the identity it must declare in `hello` — a managed
/// application cannot discover its own identity any other way.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureArgs {
    /// The single behavior to perform.
    pub scenario: Scenario,
    /// The application id this launch was created with.
    pub app_id: String,
    /// The instance id this launch was created with.
    pub instance: u128,
    /// Absolute path of the evidence transcript this run appends to.
    pub transcript: String,
}

impl FixtureArgs {
    /// Renders this request as the launch authority's `argv`.
    pub fn to_argv(&self) -> Vec<String> {
        vec![
            format!("--scenario={}", self.scenario.name()),
            format!("--app-id={}", self.app_id),
            format!("--instance={}", self.instance),
            format!("--transcript={}", self.transcript),
        ]
    }

    /// Parses the closed grammar.
    pub fn parse<I, S>(args: I) -> Result<Self, FixtureError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut scenario: Option<Scenario> = None;
        let mut app_id: Option<String> = None;
        let mut instance: Option<u128> = None;
        let mut transcript: Option<String> = None;

        // Each option is handled separately rather than through a shared slot.
        // A single slot would have to be one type, and the two string options
        // and the numeric one would then need coercions that could silently
        // accept a value of the wrong shape.
        for argument in args {
            let argument = argument.as_ref();
            let Some((key, value)) = argument.split_once('=') else {
                return Err(FixtureError::UnsupportedArgument(argument.to_owned()));
            };
            let duplicate = match key {
                "--scenario" => {
                    if scenario
                        .replace(
                            Scenario::parse(value)
                                .ok_or_else(|| FixtureError::UnknownScenario(value.to_owned()))?,
                        )
                        .is_some()
                    {
                        true
                    } else {
                        continue;
                    }
                }
                "--app-id" => app_id.replace(value.to_owned()).is_some(),
                "--transcript" => transcript.replace(value.to_owned()).is_some(),
                "--instance" => {
                    let parsed = value
                        .parse::<u128>()
                        .map_err(|_| FixtureError::UnsupportedArgument(argument.to_owned()))?;
                    instance.replace(parsed).is_some()
                }
                _ => return Err(FixtureError::UnsupportedArgument(argument.to_owned())),
            };
            if duplicate {
                return Err(FixtureError::ConflictingScenarios);
            }
        }

        Ok(FixtureArgs {
            scenario: scenario.ok_or(FixtureError::MissingScenario)?,
            app_id: app_id.ok_or(FixtureError::MissingOption("--app-id"))?,
            instance: instance.ok_or(FixtureError::MissingOption("--instance"))?,
            transcript: transcript.ok_or(FixtureError::MissingOption("--transcript"))?,
        })
    }

    /// Parses only appd's reserved launch context for the Plan-374 persistent
    /// catalog qualification. Scenario selection is derived from the signed
    /// test package's AppId, and the transcript path is local evidence output;
    /// neither value is supplied by launch authority.
    pub fn parse_managed<I, S>(args: I) -> Result<Self, FixtureError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut app_id = None;
        let mut instance = None;
        for argument in args {
            let argument = argument.as_ref();
            let Some((key, value)) = argument.split_once('=') else {
                return Err(FixtureError::UnsupportedArgument(argument.to_owned()));
            };
            let duplicate = match key {
                "--i2pr-app-id" => app_id.replace(value.to_owned()).is_some(),
                "--i2pr-app-instance" => {
                    let parsed = value
                        .parse::<u128>()
                        .map_err(|_| FixtureError::UnsupportedArgument(argument.to_owned()))?;
                    instance.replace(parsed).is_some()
                }
                _ => return Err(FixtureError::UnsupportedArgument(argument.to_owned())),
            };
            if duplicate {
                return Err(FixtureError::ConflictingScenarios);
            }
        }
        let app_id = app_id.ok_or(FixtureError::MissingOption("--i2pr-app-id"))?;
        i2pr_app_proto::AppId::parse(app_id.clone())
            .map_err(|_| FixtureError::UnsupportedArgument("--i2pr-app-id".to_owned()))?;
        let instance = instance.ok_or(FixtureError::MissingOption("--i2pr-app-instance"))?;
        i2pr_app_proto::AppInstanceId::new(instance)
            .map_err(|_| FixtureError::UnsupportedArgument("--i2pr-app-instance".to_owned()))?;
        let scenario = if app_id.ends_with(".i2cp") {
            Scenario::I2cpHappyPath
        } else if app_id.ends_with(".sam") {
            Scenario::SamHappyPath
        } else {
            return Err(FixtureError::UnsupportedArgument(
                "--i2pr-app-id".to_owned(),
            ));
        };
        let data_root = std::env::var_os("I2PR_APP_DATA_DIR")
            .ok_or(FixtureError::MissingOption("I2PR_APP_DATA_DIR"))?;
        let transcript = std::path::PathBuf::from(data_root)
            .join(format!("transcript-{app_id}-{instance}.jsonl"));
        Ok(Self {
            scenario,
            app_id,
            instance,
            transcript: transcript.to_string_lossy().into_owned(),
        })
    }
}

/// Builds the app v1 hello this fixture sends.
///
/// `identity_override` exists so the wrong-identity scenario can produce a hello
/// that is well formed in every respect except the one that matters. A hello
/// that was malformed would be refused for the wrong reason and would prove
/// nothing about identity checking.
pub fn hello(
    app_id: &str,
    instance_id: u128,
    identity_override: Option<(String, u128)>,
) -> Vec<u8> {
    let (app_id, instance_id) =
        identity_override.unwrap_or_else(|| (app_id.to_owned(), instance_id));
    let hello = i2pr_app_proto::AppToHostMessage::Hello {
        request_id: i2pr_app_proto::RequestId::new(1).expect("request id 1 is nonzero"),
        app_id: i2pr_app_proto::AppId::parse(app_id).expect("fixture app id is valid"),
        instance_id: i2pr_app_proto::AppInstanceId::new(instance_id)
            .expect("fixture instance id is nonzero"),
        protocol_major: i2pr_app_proto::PROTOCOL_MAJOR,
        protocol_minor: i2pr_app_proto::PROTOCOL_MINOR,
    };
    frame_control(&hello)
}

/// Frames an app-to-host control message.
pub fn frame_control(message: &i2pr_app_proto::AppToHostMessage) -> Vec<u8> {
    let payload = i2pr_app_proto::encode_app_to_host_control(message)
        .expect("fixture control message encodes");
    frame(i2pr_app_proto::FrameKind::Control, 0, payload)
}

/// Frames raw bytes as one app-to-host frame.
///
/// `stream_id` is deliberately a parameter: several qualification cases are
/// about which stream id a frame claims, so the value cannot be hidden inside
/// the message constructors.
pub fn frame(kind: i2pr_app_proto::FrameKind, stream_id: u32, payload: Vec<u8>) -> Vec<u8> {
    let frame = i2pr_app_proto::Frame {
        kind,
        stream_id,
        payload,
    };
    frame.encode().expect("fixture frame encodes")
}

/// A frame whose payload is one byte past the protocol ceiling.
///
/// The header is written by hand rather than through [`frame`] because
/// `Frame::encode` refuses to build an oversize frame — which is the product
/// behaviour under test, not something the fixture should route around.
pub fn oversized_frame() -> Vec<u8> {
    let oversize = i2pr_app_proto::MAX_FRAME_PAYLOAD_BYTES + 1;
    // The header layout is spelled out rather than borrowed from `Frame`: the
    // layout *is* what this case is attacking, and constructing it through the
    // product's own encoder would refuse to build it.
    //
    // `version`, `kind`, two reserved bytes, a non-zero stream id (a data frame
    // with stream id 0 is malformed for a different reason and would prove
    // nothing), then the declared payload length.
    let mut bytes = Vec::with_capacity(i2pr_app_proto::FRAME_HEADER_BYTES);
    bytes.push(1);
    bytes.push(2);
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&(oversize as u32).to_be_bytes());
    bytes
}

/// Bytes that cannot be decoded as a frame at all.
///
/// The magic is deliberately wrong rather than merely truncated: a truncated
/// frame is still an *incomplete* frame, and a decoder may legitimately wait for
/// more bytes, which would make the fixture look like it had hung.
pub fn malformed_frame() -> Vec<u8> {
    let mut bytes = vec![0xFF_u8; i2pr_app_proto::FRAME_HEADER_BYTES];
    bytes.extend_from_slice(b"this is not a frame payload");
    bytes
}

/// Appends one JSONL evidence record to a run's transcript.
///
/// # Why a file, and why this file
///
/// The fixture runs in its own process with its own stdin/stdout carrying app v1
/// bytes, and stderr is bounded diagnostics that the daemon drains and caps. None
/// of those is a channel the qualification test can read back without either
/// corrupting the protocol or re-implementing the bound it is trying to observe.
/// A path handed in through the launch `argv` is the one channel that is inert,
/// per-run, and outside every product code path.
///
/// Records are one line each and appended, never rewritten, so a run that dies
/// mid-scenario still leaves the prefix that explains how it died.
pub mod transcript {
    use std::fs::OpenOptions;
    use std::io::{ErrorKind, Write};

    use crate::FixtureError;

    /// Appends `record` as one JSONL line.
    ///
    /// A write failure is reported rather than swallowed: a fixture that cannot
    /// record what it observed would otherwise exit zero and produce evidence
    /// that silently does not contain the thing being claimed.
    pub fn append(path: &str, record: &serde_json::Value) -> Result<(), FixtureError> {
        let mut line = serde_json::to_string(record)
            .map_err(|error| FixtureError::Transcript(error.to_string()))?;
        line.push('\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| FixtureError::Transcript(format!("open {path}: {error}")))?;
        file.write_all(line.as_bytes()).map_err(|error| {
            // A full or read-only transcript is still an error, but it is
            // tagged so the reason a run produced no evidence is visible.
            let _ = error.kind() == ErrorKind::Other;
            FixtureError::Transcript(format!("write {path}: {error}"))
        })
    }

    /// Records one observation.
    ///
    /// `detail` carries only bounded, already-redacted protocol facts — a
    /// capability list, a status, a byte count. Raw payloads are never recorded:
    /// a transcript is committed evidence and must not become a place where
    /// secrets or unbounded payload bytes accumulate.
    pub fn note(path: &str, step: &str, detail: serde_json::Value) -> Result<(), FixtureError> {
        append(path, &serde_json::json!({ "step": step, "detail": detail }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scenario_name_round_trips() {
        for scenario in Scenario::ALL {
            assert_eq!(
                Scenario::parse(scenario.name()),
                Some(*scenario),
                "a scenario must parse back to itself"
            );
        }
    }

    #[test]
    fn scenario_names_are_unique() {
        let mut names: Vec<&str> = Scenario::ALL.iter().map(|s| s.name()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two scenarios share a wire name");
    }

    #[test]
    fn plan_369_section_g_requirements_are_all_represented() {
        // Plan 369 §G enumerates the behaviors the fixture must be able to
        // perform. Each is asserted against a concrete scenario so dropping one
        // is a failing test rather than a silent coverage loss.
        for required in [
            "sam-happy-path",
            "i2cp-happy-path",
            "hello-wrong-identity",
            "hello-not-first",
            "denied-capability",
            "oversized-frame",
            "malformed-frame",
            "close-early",
            "shutdown-hang",
            "stderr-flood",
            "sibling-stream-id-misuse",
        ] {
            assert!(
                Scenario::parse(required).is_some(),
                "Plan 369 §G requires a `{required}` scenario"
            );
        }
    }

    fn args() -> Vec<String> {
        vec![
            "--scenario=sam-happy-path".to_owned(),
            "--app-id=fixture.app".to_owned(),
            "--instance=42".to_owned(),
            "--transcript=/tmp/transcript".to_owned(),
        ]
    }

    #[test]
    fn a_complete_argument_set_is_accepted() {
        let parsed = FixtureArgs::parse(args()).expect("fixture args");
        assert_eq!(parsed.scenario, Scenario::SamHappyPath);
        assert_eq!(parsed.app_id, "fixture.app");
        assert_eq!(parsed.instance, 42);
        assert_eq!(parsed.transcript, "/tmp/transcript");
    }

    #[test]
    fn the_argument_set_round_trips_through_argv() {
        // The fixture manager renders `argv` for the launch authority and the
        // fixture application parses it back. If those two disagreed, every
        // transcript would describe a different run than the one that happened.
        let parsed = FixtureArgs::parse(args()).expect("fixture args");
        assert_eq!(FixtureArgs::parse(parsed.to_argv()), Ok(parsed));
    }

    #[test]
    fn a_missing_scenario_is_refused_rather_than_defaulted() {
        let mut argv = args();
        argv.retain(|value| !value.starts_with("--scenario="));
        assert_eq!(FixtureArgs::parse(argv), Err(FixtureError::MissingScenario));
    }

    #[test]
    fn an_unknown_scenario_names_what_it_could_not_parse() {
        let mut argv = args();
        argv[0] = "--scenario=not-a-scenario".to_owned();
        assert_eq!(
            FixtureArgs::parse(argv),
            Err(FixtureError::UnknownScenario("not-a-scenario".to_owned()))
        );
    }

    #[test]
    fn two_scenarios_are_refused_rather_than_one_winning() {
        let mut argv = args();
        argv.push("--scenario=i2cp-happy-path".to_owned());
        assert_eq!(
            FixtureArgs::parse(argv),
            Err(FixtureError::ConflictingScenarios)
        );
    }

    #[test]
    fn any_other_argument_is_refused() {
        let mut argv = args();
        argv.push("--root=/".to_owned());
        assert_eq!(
            FixtureArgs::parse(argv),
            Err(FixtureError::UnsupportedArgument("--root=/".to_owned()))
        );
    }

    #[test]
    fn an_argument_without_a_value_is_refused() {
        let mut argv = args();
        argv.push("--scenario".to_owned());
        assert_eq!(
            FixtureArgs::parse(argv),
            Err(FixtureError::UnsupportedArgument("--scenario".to_owned()))
        );
    }

    #[test]
    fn a_non_numeric_instance_is_refused_rather_than_defaulted() {
        let mut argv = args();
        argv[2] = "--instance=not-a-number".to_owned();
        assert_eq!(
            FixtureArgs::parse(argv),
            Err(FixtureError::UnsupportedArgument(
                "--instance=not-a-number".to_owned()
            ))
        );
    }

    #[test]
    fn each_required_option_names_itself_when_absent() {
        for (index, option) in ["--scenario=", "--app-id=", "--instance=", "--transcript="]
            .iter()
            .enumerate()
        {
            let mut argv = args();
            argv.remove(index);
            assert!(
                FixtureArgs::parse(argv).is_err(),
                "a fixture launch without {option} must be refused"
            );
        }
    }

    #[test]
    fn the_hello_is_a_well_formed_control_frame() {
        let bytes = hello("fixture.app", 42, None);
        let (frame, consumed) =
            i2pr_app_proto::Frame::decode(&bytes).expect("fixture hello decodes");
        assert_eq!(consumed, bytes.len(), "the hello must be exactly one frame");
        assert_eq!(frame.kind, i2pr_app_proto::FrameKind::Control);
        let decoded =
            i2pr_app_proto::decode_app_to_host_control(&frame.payload).expect("hello decodes");
        assert!(matches!(
            decoded,
            i2pr_app_proto::AppToHostMessage::Hello {
                protocol_major,
                protocol_minor,
                ..
            } if protocol_major == i2pr_app_proto::PROTOCOL_MAJOR
                && protocol_minor == i2pr_app_proto::PROTOCOL_MINOR
        ));
    }

    #[test]
    fn the_identity_override_changes_only_the_identity() {
        let honest = hello("fixture.app", 42, None);
        let wrong = hello("fixture.app", 42, Some(("fixture.app".to_owned(), 43)));
        assert_ne!(honest, wrong);
        // The override must still produce a decodable hello, otherwise the
        // wrong-identity case would be refused for an unrelated reason.
        let (frame, _) = i2pr_app_proto::Frame::decode(&wrong).expect("still a frame");
        assert!(i2pr_app_proto::decode_app_to_host_control(&frame.payload).is_ok());
    }

    #[test]
    fn an_oversized_frame_declares_a_payload_past_the_ceiling() {
        let bytes = oversized_frame();
        assert_eq!(
            bytes.len(),
            i2pr_app_proto::FRAME_HEADER_BYTES,
            "only the header is sent; the oversize is in the *declared* length"
        );
        let error = i2pr_app_proto::Frame::decode(&bytes).expect_err("must be refused");
        assert!(
            matches!(error, i2pr_app_proto::ContractError::LimitExceeded(_)),
            "the product must refuse this as a limit, got {error:?}"
        );
    }

    #[test]
    fn a_malformed_frame_is_refused_rather_than_silently_accepted() {
        let bytes = malformed_frame();
        assert!(
            i2pr_app_proto::Frame::decode(&bytes).is_err(),
            "garbage bytes must not decode"
        );
    }

    #[test]
    fn only_the_hang_scenario_waits_for_its_host() {
        for scenario in Scenario::ALL {
            assert_eq!(
                scenario.hangs_until_stopped(),
                *scenario == Scenario::ShutdownHang,
                "{} misreports whether it waits for its host",
                scenario.name()
            );
        }
    }
}
