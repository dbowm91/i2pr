//! Plan 369 §G — the managed-application fixture process.
//!
//! `i2pr-apphost` execs this binary directly: no shell, no `PATH` search, and no
//! inherited environment beyond the launch authority's sanitized set. It speaks
//! managed-app v1 on stdin/stdout exactly as a real application would, and
//! appends what it observed to a JSONL transcript whose path arrived in its own
//! launch `argv`.
//!
//! # Why this is a real process and not an in-memory harness
//!
//! The qualification is only evidence if the thing under test is the product
//! path: manager → apphost → **separate OS process** → app v1 → gateway → SAM /
//! I2CP. A fixture living inside the manager could share a decoder, a runtime, or
//! a bug with the code it is meant to qualify. This binary shares none of those.
//! Its only workspace dependency is the wire contract.
//!
//! # Why plain `std`
//!
//! This is an *application*, not router code. It has no async runtime and no
//! dependency on any router crate. That is deliberate: an application written
//! against the same Tokio plumbing the manager uses could accidentally be
//! relying on the very behaviour it is supposed to be testing.

use std::io::{Read, Write};
use std::process::ExitCode;
use std::time::Duration;

use i2pr_app_fixture::{
    FixtureArgs, FixtureError, STDERR_FLOOD_BYTES, Scenario, frame, frame_control, hello,
    malformed_frame, oversized_frame, transcript,
};
use i2pr_app_proto::{
    AppRequestOutcome, AppService, AppToHostMessage, Capability, Frame, FrameKind, Handshake,
    HostToAppMessage, PermissionStatus, RequestErrorCode, RequestId, RequestedCapability, Role,
};

/// I2CP's leading protocol byte (`i2pr_api::i2cp::PROTOCOL_BYTE`).
///
/// Duplicated rather than imported because the fixture must not depend on a
/// router crate. A wrong value here fails as an I2CP protocol refusal, which is
/// exactly what the qualification asserts, so the duplication cannot hide.
const I2CP_PROTOCOL_BYTE: u8 = 0x2a;

/// I2CP `GetDate` / `SetDate` message types.
const I2CP_GET_DATE: u8 = 32;
const I2CP_SET_DATE: u8 = 33;

/// Bytes in one I2CP frame header: 4-byte big-endian length, 1-byte type.
const I2CP_FRAME_HEADER: usize = 5;

/// Ceiling on any single backend read. Bounded so a host that streams without
/// framing cannot drive an unbounded allocation in the fixture.
const READ_CHUNK: usize = 64 * 1024;

/// Ceiling on the hang scenario's own lifetime.
///
/// The scenario's whole point is that the *manager's* bounded shutdown is what
/// ends it. This bound exists only so that a defect which removes that shutdown
/// fails the test with a clear outcome instead of leaving an orphan.
const HANG_SELF_LIMIT: Duration = Duration::from_secs(120);

/// The SAM session id both sibling instances declare, for the isolation case.
const SHARED_SAM_SESSION_ID: &str = "5e6e1a2b-0000-4000-8000-000000000001";

/// A SAM reply the fixture accepts as success.
///
/// Spelled as "the reply reports success" rather than "the reply starts with
/// some prefix": this router answers `HELLO VERSION` with
/// `HELLO REPLY RESULT=OK VERSION=3.1`, and a check written against a different
/// SAM dialect's greeting line would fail on a perfectly correct product.
fn sam_reply_succeeded(line: &str, expected_version: &str) -> bool {
    line.contains("RESULT=OK") && line.contains(&format!("VERSION={expected_version}"))
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let managed_catalog_context = argv.len() == 2
        && argv.iter().all(|arg| {
            arg.starts_with("--i2pr-app-id=") || arg.starts_with("--i2pr-app-instance=")
        });
    let request = match if managed_catalog_context {
        FixtureArgs::parse_managed(argv)
    } else {
        FixtureArgs::parse(argv)
    } {
        Ok(request) => request,
        Err(error) => {
            eprintln!("i2pr-app-fixture: {error}");
            return ExitCode::from(2);
        }
    };

    match run(&request) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The transcript is the evidence, so a failure is recorded *and*
            // reported. Recording only would let a failed run read as a clean
            // one; reporting only would leave the qualification guessing.
            let _ = transcript::note(
                &request.transcript,
                "fixture-error",
                serde_json::json!(error.to_string()),
            );
            eprintln!("i2pr-app-fixture: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(request: &FixtureArgs) -> Result<(), FixtureError> {
    let mut host = Host::new(request)?;
    transcript::note(
        &request.transcript,
        "start",
        serde_json::json!({
            "scenario": request.scenario.name(),
            "app_id": request.app_id,
            "instance": request.instance.to_string(),
            "pid": std::process::id(),
        }),
    )?;

    // Plan 369 §8: the application speaks first, and `hello` may not precede the
    // 9-byte app v1 greeting.
    host.send(
        &Handshake {
            role: Role::Application,
            major: i2pr_app_proto::PROTOCOL_MAJOR,
            minor: i2pr_app_proto::PROTOCOL_MINOR,
        }
        .encode(),
    )?;

    match request.scenario {
        // Each of these is refused during the greeting/hello phase, so sending
        // the offending bytes and exiting is the whole behavior. Waiting for a
        // reply would make a correct refusal look like a hang.
        Scenario::HelloNotFirst => {
            host.control(&AppToHostMessage::UiMessage {
                message_id: 1,
                payload: "sent before hello".to_owned(),
            })?;
            host.await_verdict(request, "verdict-hello-not-first")?;
        }
        Scenario::HelloWrongIdentity => {
            host.send(&hello(
                &request.app_id,
                request.instance,
                // Well formed in every respect except the declared instance: a
                // hello that were also malformed would be refused for the wrong
                // reason and would prove nothing about identity checking.
                Some((request.app_id.clone(), request.instance + 1)),
            ))?;
            host.await_verdict(request, "verdict-wrong-identity")?;
        }
        Scenario::OversizedFrame => {
            host.send(&oversized_frame())?;
            host.await_verdict(request, "verdict-oversized")?;
        }
        Scenario::MalformedFrame => {
            host.send(&malformed_frame())?;
            host.await_verdict(request, "verdict-malformed")?;
        }
        Scenario::CloseEarly => host.send(&hello(&request.app_id, request.instance, None))?,
        Scenario::StderrFlood => {
            host.send(&hello(&request.app_id, request.instance, None))?;
            flood_stderr(request)?;
        }
        _ => {
            host.send(&hello(&request.app_id, request.instance, None))?;
            let granted = host.await_capabilities()?;
            exercise(request, &mut host, &granted)?;
        }
    }

    transcript::note(
        &request.transcript,
        "complete",
        serde_json::json!({ "scenario": request.scenario.name() }),
    )?;
    Ok(())
}

/// Every behavior that needs a live, capability-carrying session.
fn exercise(
    request: &FixtureArgs,
    host: &mut Host,
    granted: &[Capability],
) -> Result<(), FixtureError> {
    match request.scenario {
        Scenario::SamHappyPath | Scenario::DuplicateSamSessionId => sam_round_trip(request, host),
        Scenario::I2cpHappyPath => i2cp_round_trip(request, host),
        Scenario::DeniedCapability => denied_capability(request, host),
        Scenario::DeniedService => denied_service(request, host, granted),
        Scenario::SiblingStreamIdMisuse => sibling_stream_id_misuse(request, host),
        Scenario::DataBeforeOpen => data_before_open(request, host),
        Scenario::DuplicateStreamId => duplicate_stream_id(request, host),
        Scenario::ShutdownHang => shutdown_hang(request, host, granted),
        other => Err(FixtureError::Transport(format!(
            "scenario {} has no exercised path",
            other.name()
        ))),
    }
}

// -- Plan 369 §G behaviors ---------------------------------------------------

/// The required black-box SAM path: `HELLO VERSION`, then `SESSION CREATE`,
/// entirely over the private gateway with no listener bound.
fn sam_round_trip(request: &FixtureArgs, host: &mut Host) -> Result<(), FixtureError> {
    let stream = host.open_service(request, 1, AppService::Sam, "sam")?;
    host.backend(
        &request.transcript,
        stream,
        b"HELLO VERSION MIN=3.1 MAX=3.1\r\n",
        "sam-hello",
    )?;
    let line = host.backend_line(&request.transcript, stream, "sam-hello-reply")?;
    if !sam_reply_succeeded(&line, "3.1") {
        return Err(FixtureError::Transport(format!(
            "SAM HELLO must be answered with an OK reply naming version 3.1, got {line:?}"
        )));
    }
    transcript::note(
        &request.transcript,
        "sam-version",
        serde_json::json!({ "reply": line }),
    )?;

    let session_id = if request.scenario == Scenario::DuplicateSamSessionId {
        SHARED_SAM_SESSION_ID
    } else {
        "3f2a9c1d-0000-4000-8000-0000000000ff"
    };
    host.backend(
        &request.transcript,
        stream,
        format!("SESSION CREATE STYLE=STREAM ID={session_id} DESTINATION=TRANSIENT\r\n").as_bytes(),
        "sam-session-create",
    )?;
    let result = host.backend_line(&request.transcript, stream, "sam-session-result")?;
    if !sam_reply_succeeded(&result, "3.1") && !result.contains("RESULT=OK") {
        return Err(FixtureError::Transport(format!(
            "SESSION CREATE must be accepted over the private gateway, got {result:?}"
        )));
    }
    transcript::note(
        &request.transcript,
        "sam-session",
        serde_json::json!({ "id": session_id, "result": result }),
    )?;

    transcript::note(&request.transcript, "closing-sam", serde_json::json!({}))?;
    host.close(stream)?;
    transcript::note(&request.transcript, "closed-sam", serde_json::json!({}))?;
    Ok(())
}

/// The required black-box I2CP path: the protocol byte, `GetDate`, `SetDate`.
fn i2cp_round_trip(request: &FixtureArgs, host: &mut Host) -> Result<(), FixtureError> {
    let stream = host.open_service(request, 1, AppService::I2cp, "i2cp")?;

    let mut bytes = vec![I2CP_PROTOCOL_BYTE];
    bytes.extend(i2cp_frame(I2CP_GET_DATE, &i2cp_string("0.9.67")));
    host.backend(&request.transcript, stream, &bytes, "i2cp-get-date")?;

    let reply = host.backend_frame(&request.transcript, stream, "i2cp-set-date")?;
    // 4-byte big-endian body length, then the one-byte message type.
    let message_type = reply.get(I2CP_FRAME_HEADER - 1).copied();
    if message_type != Some(I2CP_SET_DATE) {
        return Err(FixtureError::Transport(format!(
            "I2CP GetDate must be answered with SetDate ({I2CP_SET_DATE}), got type {message_type:?}"
        )));
    }
    transcript::note(
        &request.transcript,
        "i2cp-set-date",
        serde_json::json!({ "body_bytes": reply.len() }),
    )?;

    host.close(stream)?;
    Ok(())
}

/// A permission request for something this launch was never granted.
///
/// Plan 369 invariant 8: it must be denied *and* must change nothing. The
/// transcript therefore records the granted set before and after, so the test can
/// assert the capability message the host sends is still the authority's.
fn denied_capability(request: &FixtureArgs, host: &mut Host) -> Result<(), FixtureError> {
    let request_id = host.next_request_id();
    host.control(&AppToHostMessage::PermissionRequest {
        request_id,
        capabilities: vec![RequestedCapability {
            capability: Capability::Lifecycle,
        }],
    })?;

    match host.read_control()? {
        HostToAppMessage::PermissionReply {
            request_id: seen,
            status,
        } => {
            if seen != request_id {
                return Err(FixtureError::Transport(format!(
                    "permission reply answered {request_id:?} with {seen:?}"
                )));
            }
            if status != PermissionStatus::Denied {
                return Err(FixtureError::Transport(format!(
                    "an ungranted capability must be denied, got {status:?}"
                )));
            }
            transcript::note(
                &request.transcript,
                "permission-denied",
                serde_json::json!({ "capability": "lifecycle", "status": "denied" }),
            )?;
        }
        other => {
            return Err(FixtureError::Transport(format!(
                "expected a permission reply, got {other:?}"
            )));
        }
    }
    Ok(())
}

/// Opens a service whose capability this launch was never granted.
///
/// The refusal is the assertion: if this open ever *succeeds*, the fixture exits
/// non-zero rather than recording a happy path, because "the app opened SAM
/// without a SAM grant" is exactly the defect this case exists to catch.
fn denied_service(
    request: &FixtureArgs,
    host: &mut Host,
    granted: &[Capability],
) -> Result<(), FixtureError> {
    let target = ungranted_service(granted)?;
    let code = host.expect_open_refusal(1, target)?;
    transcript::note(
        &request.transcript,
        "denied-service",
        serde_json::json!({
            "service": format!("{target:?}"),
            "refusal": format!("{code:?}"),
        }),
    )?;
    Ok(())
}

/// Control traffic naming a stream id this application never opened.
///
/// Plan 369: one app cannot use another's logical stream id, and the manager
/// translates an application's stream id into a session-private backend handle,
/// so a foreign id must have no reach at all.
///
/// Resetting an id the session does not hold is a **no-op** by design — a close
/// or reset routinely arrives after a backend already ended that stream, so
/// refusing it would punish the application for the daemon's timing. The
/// property under test is therefore not "the reset is rejected" but "the stream
/// this application *does* hold keeps working afterwards". That is the only
/// reading of isolation that does not depend on a refusal nobody promised.
fn sibling_stream_id_misuse(request: &FixtureArgs, host: &mut Host) -> Result<(), FixtureError> {
    let stream = host.open_service(request, 1, AppService::Sam, "sam")?;
    host.backend(
        &request.transcript,
        stream,
        b"HELLO VERSION MIN=3.1 MAX=3.1\r\n",
        "sam-hello",
    )?;
    let version = host.backend_line(&request.transcript, stream, "sam-hello-reply")?;
    if !sam_reply_succeeded(&version, "3.1") {
        return Err(FixtureError::Transport(format!(
            "SAM HELLO must be answered with an OK reply naming version 3.1, got {version:?}"
        )));
    }

    // The foreign id. Nothing this application holds may be affected by it.
    host.control(&AppToHostMessage::Reset {
        stream_id: 9_999,
        reason: "a stream id this application never opened".to_owned(),
    })?;
    transcript::note(
        &request.transcript,
        "sibling-stream-id-misuse",
        serde_json::json!({ "stream_id": 9_999, "held_stream": stream }),
    )?;

    // The real stream must still work. If the foreign reset had torn it down,
    // this exchange fails here.
    host.backend(
        &request.transcript,
        stream,
        b"SESSION CREATE STYLE=STREAM ID=8a41d0e2-0000-4000-8000-0000000000ab DESTINATION=TRANSIENT\r\n",
        "sam-session-create",
    )?;
    let result = host.backend_line(&request.transcript, stream, "sam-session-result")?;
    if !result.contains("RESULT=OK") {
        return Err(FixtureError::Transport(format!(
            "the live stream must survive a foreign reset, got {result:?}"
        )));
    }
    transcript::note(
        &request.transcript,
        "sam-session",
        serde_json::json!({ "id": "8a41d0e2-0000-4000-8000-0000000000ab", "result": result }),
    )?;

    host.close(stream)?;
    Ok(())
}

/// A data frame on a stream that was never opened.
fn data_before_open(request: &FixtureArgs, host: &mut Host) -> Result<(), FixtureError> {
    transcript::note(
        &request.transcript,
        "data-before-open",
        serde_json::json!({ "stream_id": 1 }),
    )?;
    host.send(&frame(FrameKind::Data, 1, b"data before open".to_vec()))?;
    host.await_verdict(request, "verdict-data-before-open")
}

/// Opening the same stream id twice must be refused, not silently aliased.
fn duplicate_stream_id(request: &FixtureArgs, host: &mut Host) -> Result<(), FixtureError> {
    host.open_service(request, 1, AppService::Sam, "sam")?;
    let code = host.expect_open_refusal(1, AppService::Sam)?;
    transcript::note(
        &request.transcript,
        "duplicate-stream-id",
        serde_json::json!({ "stream_id": 1, "refusal": format!("{code:?}") }),
    )?;
    Ok(())
}

/// Never returns on its own: the manager's bounded shutdown must end this.
fn shutdown_hang(
    request: &FixtureArgs,
    host: &mut Host,
    granted: &[Capability],
) -> Result<(), FixtureError> {
    let service = ungranted_service(granted).unwrap_or(AppService::Sam);
    let _ = host.open_service(request, 1, service, "hang");
    host.send(&frame(
        FrameKind::Data,
        1,
        b"this stream never closes".to_vec(),
    ))?;
    transcript::note(
        &request.transcript,
        "hang-entered",
        serde_json::json!({ "self_limit_seconds": HANG_SELF_LIMIT.as_secs() }),
    )?;

    // Bounded only by the fixture's own backstop, so a removed manager shutdown
    // fails loudly instead of leaking an orphan process.
    std::thread::sleep(HANG_SELF_LIMIT);
    Err(FixtureError::Transport(
        "the hang scenario outlived its own backstop: nothing stopped this process".to_owned(),
    ))
}

// -- wire helpers ------------------------------------------------------------

/// Encodes one I2CP length-prefixed frame.
fn i2cp_frame(message_type: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(I2CP_FRAME_HEADER + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.push(message_type);
    out.extend_from_slice(body);
    out
}

/// Encodes an I2CP length-prefixed string: one length byte, then the bytes.
fn i2cp_string(text: &str) -> Vec<u8> {
    assert!(
        text.len() < u8::MAX as usize,
        "i2cp string must fit one length byte"
    );
    let mut out = Vec::with_capacity(1 + text.len());
    out.push(text.len() as u8);
    out.extend_from_slice(text.as_bytes());
    out
}

/// The service whose capability is absent from `granted`.
///
/// Panics rather than returning an error when both are granted: a fixture asked
/// to prove a denial under a launch that granted everything is a *composition*
/// error in the qualification, and silently skipping the case would hide it.
fn ungranted_service(granted: &[Capability]) -> Result<AppService, FixtureError> {
    let sam = granted.contains(&Capability::Sam);
    let i2cp = granted.contains(&Capability::I2cp);
    match (sam, i2cp) {
        (true, false) => Ok(AppService::I2cp),
        (false, true) => Ok(AppService::Sam),
        _ => Err(FixtureError::Transport(
            "this scenario needs a launch that withheld exactly one of sam/i2cp".to_owned(),
        )),
    }
}

/// Writes far past any bounded retention, so the *accounting* for the discarded
/// tail is exercised rather than merely the write path.
fn flood_stderr(request: &FixtureArgs) -> Result<(), FixtureError> {
    let block = [b'x'; 1024];
    let mut written = 0_usize;
    let stderr = std::io::stderr();
    let mut handle = stderr.lock();
    while written < STDERR_FLOOD_BYTES {
        let take = block.len().min(STDERR_FLOOD_BYTES - written);
        handle
            .write_all(&block[..take])
            .map_err(|e| FixtureError::Transport(e.to_string()))?;
        written += take;
    }
    handle
        .flush()
        .map_err(|e| FixtureError::Transport(e.to_string()))?;
    eprintln!("stderr flood complete: {written} bytes");
    transcript::note(
        &request.transcript,
        "stderr-flood",
        serde_json::json!({ "bytes": written }),
    )?;
    Ok(())
}

// -- the host conversation ---------------------------------------------------

struct Host {
    request: FixtureArgs,
    reader: std::io::Stdin,
    writer: std::io::Stdout,
    next_request: u32,
}

impl Host {
    fn new(request: &FixtureArgs) -> Result<Self, FixtureError> {
        Ok(Self {
            request: request.clone(),
            reader: std::io::stdin(),
            writer: std::io::stdout(),
            next_request: 1,
        })
    }

    fn next_request_id(&mut self) -> RequestId {
        let value = self.next_request;
        self.next_request += 1;
        RequestId::new(value).expect("request ids start at 1 and never wrap here")
    }

    fn send(&mut self, bytes: &[u8]) -> Result<(), FixtureError> {
        self.writer
            .write_all(bytes)
            .and_then(|()| self.writer.flush())
            .map_err(|e| FixtureError::Transport(e.to_string()))
    }

    fn control(&mut self, message: &AppToHostMessage) -> Result<(), FixtureError> {
        self.send(&frame_control(message))
    }

    fn close(&mut self, stream: u32) -> Result<(), FixtureError> {
        self.control(&AppToHostMessage::Close { stream_id: stream })
    }

    /// Reads the `hello` acknowledgement and then the one-shot capability set.
    fn await_capabilities(&mut self) -> Result<Vec<Capability>, FixtureError> {
        match self.read_control()? {
            HostToAppMessage::Reply {
                outcome: AppRequestOutcome::Succeeded,
                ..
            } => {}
            other => {
                return Err(FixtureError::Transport(format!(
                    "hello was not acknowledged: {other:?}"
                )));
            }
        }
        let capabilities = match self.read_control()? {
            HostToAppMessage::Capabilities { capabilities } => capabilities,
            other => {
                return Err(FixtureError::Transport(format!(
                    "expected the one-shot capability set, got {other:?}"
                )));
            }
        };
        transcript::note(
            &self.request.transcript,
            "capabilities",
            serde_json::json!(
                capabilities
                    .iter()
                    .map(|c| format!("{c:?}"))
                    .collect::<Vec<_>>()
            ),
        )?;
        Ok(capabilities)
    }

    /// Opens a service, requiring the acknowledgement to be a success.
    fn open_service(
        &mut self,
        request: &FixtureArgs,
        stream: u32,
        service: AppService,
        step: &str,
    ) -> Result<u32, FixtureError> {
        let request_id = self.next_request_id();
        self.control(&AppToHostMessage::Open {
            request_id,
            stream_id: stream,
            service,
        })?;

        match self.read_control()? {
            HostToAppMessage::Reply {
                request_id: seen,
                outcome: AppRequestOutcome::Succeeded,
            } if seen == request_id => {
                transcript::note(
                    &request.transcript,
                    step,
                    serde_json::json!({ "stream_id": stream, "service": format!("{service:?}") }),
                )?;
                Ok(stream)
            }
            HostToAppMessage::Reply {
                outcome: AppRequestOutcome::Failed(error),
                ..
            } => Err(FixtureError::Transport(format!(
                "opening {service:?} on stream {stream} was refused: {:?}",
                error.code
            ))),
            other => Err(FixtureError::Transport(format!(
                "expected an open acknowledgement, got {other:?}"
            ))),
        }
    }

    /// Opens a service expecting a specific refusal, used by the denial case.
    #[allow(dead_code)]
    fn expect_open_refusal(
        &mut self,
        stream: u32,
        service: AppService,
    ) -> Result<RequestErrorCode, FixtureError> {
        let request_id = self.next_request_id();
        self.control(&AppToHostMessage::Open {
            request_id,
            stream_id: stream,
            service,
        })?;
        match self.read_control()? {
            HostToAppMessage::Reply {
                outcome: AppRequestOutcome::Failed(error),
                ..
            } => Ok(error.code),
            other => Err(FixtureError::Transport(format!(
                "expected a refusal, got {other:?}"
            ))),
        }
    }

    /// Reads exactly one frame: the fixed header, then the declared payload.
    ///
    /// The header is parsed here rather than through `Frame::decode`, because
    /// `decode` expects a *whole* frame — header plus payload — and this loop has
    /// only the first 12 bytes at this point. Handing it a header alone would
    /// report `TruncatedFrame` for every reply.
    fn read_frame(&mut self) -> Result<Frame, FixtureError> {
        let mut header = [0_u8; i2pr_app_proto::FRAME_HEADER_BYTES];
        self.reader
            .read_exact(&mut header)
            .map_err(|e| FixtureError::Transport(e.to_string()))?;
        if header[0] != 1 {
            return Err(FixtureError::Framing(
                "unsupported frame version".to_owned(),
            ));
        }
        let kind = match header[1] {
            1 => FrameKind::Control,
            2 => FrameKind::Data,
            other => {
                return Err(FixtureError::Framing(format!("unknown frame kind {other}")));
            }
        };
        let stream_id = u32::from_be_bytes(header[4..8].try_into().expect("4 bytes"));
        let declared = u32::from_be_bytes(header[8..12].try_into().expect("4 bytes")) as usize;
        if declared > i2pr_app_proto::MAX_FRAME_PAYLOAD_BYTES {
            return Err(FixtureError::Framing(
                "the host declared a payload past the contract ceiling".to_owned(),
            ));
        }
        let mut payload = vec![0_u8; declared];
        self.reader
            .read_exact(&mut payload)
            .map_err(|e| FixtureError::Transport(e.to_string()))?;
        Ok(Frame {
            kind,
            stream_id,
            payload,
        })
    }

    /// Blocks for the host's answer and records whether one ever came.
    ///
    /// This is what makes a refusal observable from the transcript alone. The
    /// offending byte is sent, a `verdict-requested` record is written, and then
    /// the fixture waits. If the product refuses, the session is terminated and
    /// this process is killed before any verdict can be written, so the run ends
    /// with `verdict-requested` and nothing after it. If the product wrongly
    /// accepts, the verdict *and* `complete` appear. Neither outcome has to be
    /// inferred from a log line.
    fn await_verdict(&mut self, request: &FixtureArgs, step: &str) -> Result<(), FixtureError> {
        transcript::note(
            &request.transcript,
            "verdict-requested",
            serde_json::json!({ "step": step }),
        )?;
        // An absent verdict is an *error*, not a neutral observation. Returning
        // Ok here would let the run write `complete` whether or not the host
        // answered, which is precisely the distinction the record exists to make.
        let frame = self.read_frame()?;
        let verdict = serde_json::json!({
            "kind": format!("{:?}", frame.kind),
            "payload_bytes": frame.payload.len(),
        });
        transcript::note(&request.transcript, step, verdict)?;
        Ok(())
    }

    fn read_control(&mut self) -> Result<HostToAppMessage, FixtureError> {
        let frame = self.read_frame()?;
        if frame.kind != FrameKind::Control {
            return Err(FixtureError::Transport(
                "host sent data where a control frame was required".to_owned(),
            ));
        }
        i2pr_app_proto::decode_host_to_app_control(&frame.payload)
            .map_err(|e| FixtureError::Framing(format!("{e:?}")))
    }

    /// Sends backend bytes on an opened stream.
    fn backend(
        &mut self,
        transcript_path: &str,
        stream: u32,
        bytes: &[u8],
        step: &str,
    ) -> Result<(), FixtureError> {
        self.send(&frame(FrameKind::Data, stream, bytes.to_vec()))?;
        transcript::note(
            transcript_path,
            step,
            serde_json::json!({ "stream_id": stream, "bytes": bytes.len() }),
        )
    }

    /// Reads backend bytes on an opened stream.
    fn backend_frame(
        &mut self,
        transcript_path: &str,
        stream: u32,
        step: &str,
    ) -> Result<Vec<u8>, FixtureError> {
        let frame = self.read_frame()?;
        if frame.kind != FrameKind::Data {
            return Err(FixtureError::Transport(format!(
                "expected backend data on stream {stream}, got {:?}",
                frame.kind
            )));
        }
        transcript::note(
            transcript_path,
            step,
            serde_json::json!({ "stream_id": stream, "bytes": frame.payload.len() }),
        )?;
        Ok(frame.payload)
    }

    /// Reads one CRLF/LF-terminated backend line, bounded.
    fn backend_line(
        &mut self,
        transcript_path: &str,
        stream: u32,
        step: &str,
    ) -> Result<String, FixtureError> {
        let mut line = Vec::new();
        loop {
            if line.len() > READ_CHUNK {
                return Err(FixtureError::Transport(
                    "backend line exceeded the fixture read ceiling".to_owned(),
                ));
            }
            let mut chunk = [0_u8; 512];
            let read = self
                .reader
                .read(&mut chunk)
                .map_err(|e| FixtureError::Transport(e.to_string()))?;
            if read == 0 {
                break;
            }
            line.extend_from_slice(&chunk[..read]);
            if line.ends_with(b"\n") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&line).trim_end().to_owned();
        transcript::note(
            transcript_path,
            step,
            serde_json::json!({ "stream_id": stream, "line": text }),
        )?;
        Ok(text)
    }
}
