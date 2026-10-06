//! Plan 369 WP1 evidence for the AppManager ↔ apphost bootstrap contract.
//!
//! These are pure contract tests: no process, no filesystem, no exec. That is the
//! point — the Plan 369 gates must be provable without a sandbox backend, because
//! `Secured` exists precisely to be refused before one is qualified.

use std::collections::BTreeMap;

use i2pr_app_manager_proto::apphost::{
    APPHOST_BOOTSTRAP_BYTES, ApphostBootstrapError, ApphostFailureReason, ApphostHandshake,
    ApphostReply, Entrypoint, LaunchRequest, LaunchRoot, SanitizedEnvironment,
};
use i2pr_app_manager_proto::{ManagerInstanceId, ManagerPrincipal};
use i2pr_app_proto::{AppId, LaunchProfile};

fn principal() -> ManagerPrincipal {
    ManagerPrincipal {
        app_id: AppId::parse("fixture.app").expect("app id"),
        instance_id: ManagerInstanceId::new(11),
        publisher_id: None,
    }
}

fn environment() -> SanitizedEnvironment {
    SanitizedEnvironment::new(BTreeMap::from([(
        "I2PR_FIXTURE".to_owned(),
        "1".to_owned(),
    )]))
    .expect("environment")
}

fn request() -> LaunchRequest {
    LaunchRequest {
        principal: principal(),
        launch_profile: LaunchProfile::UnsafeDirect,
        root: LaunchRoot::new("/opt/i2pr/apps/fixture").expect("root"),
        entrypoint: Entrypoint::new("bin/app").expect("entrypoint"),
        argv: vec!["--selftest".to_owned()],
        environment: environment(),
        resources: i2pr_app_manager_proto::apphost::DescriptiveResourceRequest {
            requested_memory_bytes: 64 * 1024 * 1024,
            requested_open_files: 64,
        },
    }
}

// ---------------------------------------------------------------------------
// The Plan 369 gate: Secured fails closed before exec
// ---------------------------------------------------------------------------

#[test]
fn secured_launch_is_refused_before_any_exec() {
    let mut secured = request();
    secured.launch_profile = LaunchProfile::Secured;

    assert_eq!(
        secured.validate(),
        Err(ApphostBootstrapError::SecuredUnavailable),
        "Secured must fail closed: no qualified sandbox backend exists in Plan 369"
    );
    // ...and the refusal is not an encoding accident.
    assert_eq!(
        secured.encode(),
        Err(ApphostBootstrapError::SecuredUnavailable)
    );
    // A rejected request must not be recoverable by round-tripping the bytes.
    assert!(secured.encode().is_err());

    // UnsafeDirect remains the only admissible profile.
    assert_eq!(request().validate(), Ok(()));
}

#[test]
fn secured_cannot_be_smuggled_through_encoded_bytes() {
    // Encode an UnsafeDirect request, then rewrite the profile field to `secured`
    // and decode. The decoder re-validates, so the forgery is refused.
    let encoded = request().encode().expect("encode");
    let text = String::from_utf8(encoded).expect("utf-8");
    let forged = text.replace("\"unsafe_direct\"", "\"secured\"");
    assert_ne!(
        forged, text,
        "the fixture must actually have rewritten the profile"
    );
    assert_eq!(
        LaunchRequest::decode(forged.as_bytes()),
        Err(ApphostBootstrapError::SecuredUnavailable)
    );
}

// ---------------------------------------------------------------------------
// Entrypoint / root containment
// ---------------------------------------------------------------------------

#[test]
fn entrypoint_traversal_and_absolute_forms_are_rejected() {
    for bad in [
        "/bin/app",      // absolute
        "../escape",     // parent traversal
        "bin/../../etc", // embedded traversal
        "bin/./app",     // current-dir segment
        "bin//app",      // empty segment
        "bin\\app",      // backslash
        "",
    ] {
        assert_eq!(
            Entrypoint::new(bad),
            Err(ApphostBootstrapError::InvalidEntrypoint),
            "entrypoint must be rejected: {bad:?}"
        );
    }
    for good in ["app", "bin/app", "bin/sub/app"] {
        assert!(Entrypoint::new(good).is_ok(), "must be accepted: {good:?}");
    }
}

#[test]
fn root_relationship_is_checked_not_implied() {
    let root = LaunchRoot::new("/opt/i2pr/apps/fixture").expect("root");
    let inside = Entrypoint::new("bin/app").expect("entrypoint");
    assert!(root.contains_entrypoint(&inside));

    // Every entrypoint the type accepts is contained by construction, which is
    // what makes the relationship check a genuine second gate rather than the
    // only one: a future constructor cannot widen it without failing here.
    for accepted in ["app", "bin/app", "bin/sub/app"] {
        assert!(
            root.contains_entrypoint(&Entrypoint::new(accepted).expect("entrypoint")),
            "accepted entrypoint must be contained: {accepted}"
        );
    }
}

#[test]
fn launch_root_rejects_invalid_forms() {
    for bad in ["", "relative/root", "back\\slash", "null\0byte"] {
        assert_eq!(
            LaunchRoot::new(bad),
            Err(ApphostBootstrapError::InvalidRoot),
            "root must be rejected: {bad:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Bounded argv and environment
// ---------------------------------------------------------------------------

#[test]
fn argv_and_environment_are_bounded() {
    let mut wide = request();
    wide.argv = (0..33).map(|index| format!("arg{index}")).collect();
    assert_eq!(
        wide.validate(),
        Err(ApphostBootstrapError::LimitExceeded("argv items")),
        "argv item count must be bounded"
    );

    let mut long = request();
    long.argv = vec!["x".repeat(513)];
    assert_eq!(long.validate(), Err(ApphostBootstrapError::InvalidArgv));

    let mut nul = request();
    nul.argv = vec!["ok".to_owned(), "bad\0arg".to_owned()];
    assert_eq!(nul.validate(), Err(ApphostBootstrapError::InvalidArgv));

    let mut wide_env = BTreeMap::new();
    for index in 0..33 {
        wide_env.insert(format!("K{index}"), "v".to_owned());
    }
    assert_eq!(
        SanitizedEnvironment::new(wide_env),
        Err(ApphostBootstrapError::LimitExceeded("environment entries"))
    );

    for bad_key in ["", "has space", "has=equals", "has-dash"] {
        assert_eq!(
            SanitizedEnvironment::new(BTreeMap::from([(bad_key.to_owned(), "v".to_owned())])),
            Err(ApphostBootstrapError::InvalidEnvironment),
            "environment key must be rejected: {bad_key:?}"
        );
    }
    assert_eq!(
        SanitizedEnvironment::new(BTreeMap::from([("K".to_owned(), "bad\0value".to_owned())])),
        Err(ApphostBootstrapError::InvalidEnvironment)
    );
}

// ---------------------------------------------------------------------------
// Round trip, handshake, replies
// ---------------------------------------------------------------------------

#[test]
fn valid_request_round_trips_exactly() {
    let original = request();
    let encoded = original.encode().expect("encode");
    assert_eq!(LaunchRequest::decode(&encoded).expect("decode"), original);
}

#[test]
fn malformed_and_oversize_bootstrap_bytes_are_rejected() {
    assert_eq!(
        LaunchRequest::decode(b"{"),
        Err(ApphostBootstrapError::Malformed)
    );
    assert_eq!(
        LaunchRequest::decode(b""),
        Err(ApphostBootstrapError::Malformed)
    );
    // Trailing bytes are not consumed.
    let mut trailing = request().encode().expect("encode");
    trailing.push(b' ');
    assert_eq!(
        LaunchRequest::decode(&trailing),
        Err(ApphostBootstrapError::Malformed)
    );
    // An unknown field is refused rather than ignored.
    let encoded = String::from_utf8(request().encode().expect("encode")).expect("utf-8");
    let injected = encoded.replacen('{', "{\"surprise\":true,", 1);
    assert_eq!(
        LaunchRequest::decode(injected.as_bytes()),
        Err(ApphostBootstrapError::Malformed)
    );
}

#[test]
fn apphost_handshake_is_exact_and_version_gated() {
    let encoded = ApphostHandshake::encode();
    assert_eq!(encoded.len(), APPHOST_BOOTSTRAP_BYTES);
    assert_eq!(&encoded[..4], b"I2PA");
    assert_eq!(ApphostHandshake::decode(&encoded), Ok(ApphostHandshake));

    let mut wrong_version = encoded;
    wrong_version[4] = 9;
    assert_eq!(
        ApphostHandshake::decode(&wrong_version),
        Err(ApphostBootstrapError::UnsupportedVersion)
    );
    let mut wrong_role = encoded;
    wrong_role[6] = 2;
    assert_eq!(
        ApphostHandshake::decode(&wrong_role),
        Err(ApphostBootstrapError::RoleMismatch)
    );
    let mut wrong_magic = encoded;
    wrong_magic[0] = b'X';
    assert_eq!(
        ApphostHandshake::decode(&wrong_magic),
        Err(ApphostBootstrapError::BadMagic)
    );
    assert_eq!(
        ApphostHandshake::decode(&encoded[..4]),
        Err(ApphostBootstrapError::Truncated)
    );
}

#[test]
fn replies_are_typed_and_include_the_fail_closed_reason() {
    let ready: ApphostReply = i2pr_app_manager_proto::apphost::serde_json::from_str(
        r#"{"type":"ready","instance_id":"11"}"#,
    )
    .expect("decode ready");
    assert_eq!(
        ready,
        ApphostReply::Ready {
            instance_id: "11".to_owned()
        }
    );

    let failed: ApphostReply = i2pr_app_manager_proto::apphost::serde_json::from_str(
        r#"{"type":"failed","reason":"secured_unavailable","diagnostic":null}"#,
    )
    .expect("decode failed");
    assert_eq!(
        failed,
        ApphostReply::Failed {
            reason: ApphostFailureReason::SecuredUnavailable,
            diagnostic: None
        }
    );

    // An unknown failure reason is refused rather than defaulted, so an
    // unrecognised state cannot be mistaken for a successful launch.
    assert!(
        i2pr_app_manager_proto::apphost::serde_json::from_str::<ApphostReply>(
            r#"{"type":"ready"}"#
        )
        .is_err()
    );
    assert!(
        i2pr_app_manager_proto::apphost::serde_json::from_str::<ApphostReply>(
            r#"{"type":"failed","reason":"definitely_ok"}"#
        )
        .is_err()
    );
}
