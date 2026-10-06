//! Compile-exhaustive encode/decode coverage for every managed-app v1 control
//! message, plus the canonical-form table for `AppInstanceId`.
//!
//! Managed-runtime Plan 370. Plan 345 closed the v1 contract with a **sampled**
//! round-trip test that exercised `AppToHostMessage::Close` only. Nothing else
//! touched the codec, so `hello` — the one message that carries a 128-bit
//! `AppInstanceId` inside serde's internally tagged enum buffer, which has no
//! `visit_u128` — encoded cleanly and then failed to decode for *every* value,
//! including `1`. Plan 369 became the first runtime consumer and made it
//! observable.
//!
//! The guard below is a `match` over every variant of all four message enums.
//! Adding a variant makes the match non-exhaustive and **fails to compile**
//! until the new variant is exercised here. A sample could not have caught that
//! defect and is explicitly not offered in its place; likewise there is no
//! `scripts/check-*.py` for it, because a regex over Rust type syntax would read
//! like enforcement while enforcing nothing.

use i2pr_app_proto::{
    AdminToHostMessage, AppId, AppInstanceId, AppRequestOutcome, AppService, AppToHostMessage,
    Capability, ContractError, HostToAdminMessage, HostToAppMessage, MAX_DECIMAL_DIGITS,
    PROTOCOL_MAJOR, PROTOCOL_MINOR, PermissionStatus, PublisherId, RequestError, RequestErrorCode,
    RequestId, RequestedCapability, ReservedAdminOperation, decode_admin_to_host_control,
    decode_app_to_host_control, decode_host_to_admin_control, decode_host_to_app_control,
    encode_admin_to_host_control, encode_app_to_host_control, encode_host_to_admin_control,
    encode_host_to_app_control,
};

fn request_id(value: u32) -> RequestId {
    RequestId::new(value).expect("request id must be nonzero")
}

fn app_id() -> AppId {
    AppId::parse("sample-app").expect("valid app id")
}

fn instance(value: u128) -> AppInstanceId {
    AppInstanceId::new(value).expect("instance id must be nonzero")
}

// ---------------------------------------------------------------------------
// Sample sets, one instance per variant, each behind a compile-checked match.
// ---------------------------------------------------------------------------

fn all_app_to_host() -> Vec<AppToHostMessage> {
    let samples = vec![
        AppToHostMessage::Hello {
            request_id: request_id(1),
            app_id: app_id(),
            instance_id: instance(1),
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
        },
        AppToHostMessage::Open {
            request_id: request_id(2),
            stream_id: 3,
            service: AppService::Sam,
        },
        AppToHostMessage::PermissionRequest {
            request_id: request_id(4),
            capabilities: vec![
                RequestedCapability {
                    capability: Capability::Sam,
                },
                RequestedCapability {
                    capability: Capability::Lifecycle,
                },
            ],
        },
        AppToHostMessage::Close { stream_id: 5 },
        AppToHostMessage::Reset {
            stream_id: 6,
            reason: "closed by host".to_owned(),
        },
        AppToHostMessage::UiMessage {
            message_id: 7,
            payload: "{\"kind\":\"ready\"}".to_owned(),
        },
    ];
    // Exhaustive by construction: a new variant breaks this build.
    for message in &samples {
        match message {
            AppToHostMessage::Hello { .. }
            | AppToHostMessage::Open { .. }
            | AppToHostMessage::PermissionRequest { .. }
            | AppToHostMessage::Close { .. }
            | AppToHostMessage::Reset { .. }
            | AppToHostMessage::UiMessage { .. } => {}
        }
    }
    samples
}

fn all_host_to_app() -> Vec<HostToAppMessage> {
    let samples = vec![
        HostToAppMessage::Reply {
            request_id: request_id(1),
            outcome: AppRequestOutcome::Succeeded,
        },
        // `Reply` is one variant with two adjacent-tagged content forms, and it
        // is the only message carrying `AppRequestOutcome`; exercise both, or
        // the failed-outcome encoding is never proven to survive the buffer.
        HostToAppMessage::Reply {
            request_id: request_id(9),
            outcome: AppRequestOutcome::Failed(RequestError {
                code: RequestErrorCode::PermissionDenied,
                diagnostic: Some("denied".to_owned()),
            }),
        },
        HostToAppMessage::PermissionReply {
            request_id: request_id(2),
            status: PermissionStatus::Recorded,
        },
        HostToAppMessage::StreamClosed { stream_id: 3 },
        HostToAppMessage::StreamReset {
            stream_id: 4,
            reason: "reset by host".to_owned(),
        },
        HostToAppMessage::Capabilities {
            capabilities: vec![Capability::Sam, Capability::Health],
        },
        HostToAppMessage::Health {
            state: "ready".to_owned(),
            detail: Some("detail".to_owned()),
        },
    ];
    for message in &samples {
        match message {
            HostToAppMessage::Reply { .. }
            | HostToAppMessage::PermissionReply { .. }
            | HostToAppMessage::StreamClosed { .. }
            | HostToAppMessage::StreamReset { .. }
            | HostToAppMessage::Capabilities { .. }
            | HostToAppMessage::Health { .. } => {}
        }
    }
    samples
}

fn all_admin_to_host() -> Vec<AdminToHostMessage> {
    let samples = vec![AdminToHostMessage::ReservedRequest {
        request_id: request_id(1),
        operation: ReservedAdminOperation::Launch,
    }];
    for message in &samples {
        match message {
            AdminToHostMessage::ReservedRequest { .. } => {}
        }
    }
    samples
}

fn all_host_to_admin() -> Vec<HostToAdminMessage> {
    let samples = vec![HostToAdminMessage::Reply {
        request_id: request_id(1),
        error: RequestError {
            code: RequestErrorCode::UnsupportedOperation,
            diagnostic: Some("reserved".to_owned()),
        },
    }];
    for message in &samples {
        match message {
            HostToAdminMessage::Reply { .. } => {}
        }
    }
    samples
}

// ---------------------------------------------------------------------------
// Exhaustive round trips, both directions.
// ---------------------------------------------------------------------------

#[test]
fn every_app_to_host_variant_round_trips_in_both_directions() {
    for message in all_app_to_host() {
        let wire = encode_app_to_host_control(&message)
            .unwrap_or_else(|error| panic!("encode failed for {message:?}: {error}"));
        let decoded = decode_app_to_host_control(&wire)
            .unwrap_or_else(|error| panic!("decode failed for {message:?}: {error}"));
        assert_eq!(decoded, message, "round trip changed {message:?}");
        let re_encoded = encode_app_to_host_control(&decoded).expect("re-encode");
        assert_eq!(re_encoded, wire, "re-encode was not byte-stable");
    }
}

#[test]
fn every_host_to_app_variant_round_trips_in_both_directions() {
    for message in all_host_to_app() {
        let wire = encode_host_to_app_control(&message)
            .unwrap_or_else(|error| panic!("encode failed for {message:?}: {error}"));
        let decoded = decode_host_to_app_control(&wire)
            .unwrap_or_else(|error| panic!("decode failed for {message:?}: {error}"));
        assert_eq!(decoded, message, "round trip changed {message:?}");
        let re_encoded = encode_host_to_app_control(&decoded).expect("re-encode");
        assert_eq!(re_encoded, wire, "re-encode was not byte-stable");
    }
}

#[test]
fn every_admin_to_host_variant_round_trips_in_both_directions() {
    for message in all_admin_to_host() {
        let wire = encode_admin_to_host_control(&message)
            .unwrap_or_else(|error| panic!("encode failed for {message:?}: {error}"));
        let decoded = decode_admin_to_host_control(&wire)
            .unwrap_or_else(|error| panic!("decode failed for {message:?}: {error}"));
        assert_eq!(decoded, message, "round trip changed {message:?}");
        let re_encoded = encode_admin_to_host_control(&decoded).expect("re-encode");
        assert_eq!(re_encoded, wire, "re-encode was not byte-stable");
    }
}

#[test]
fn every_host_to_admin_variant_round_trips_in_both_directions() {
    for message in all_host_to_admin() {
        let wire = encode_host_to_admin_control(&message)
            .unwrap_or_else(|error| panic!("encode failed for {message:?}: {error}"));
        let decoded = decode_host_to_admin_control(&wire)
            .unwrap_or_else(|error| panic!("decode failed for {message:?}: {error}"));
        assert_eq!(decoded, message, "round trip changed {message:?}");
        let re_encoded = encode_host_to_admin_control(&decoded).expect("re-encode");
        assert_eq!(re_encoded, wire, "re-encode was not byte-stable");
    }
}

// ---------------------------------------------------------------------------
// Plan 370 acceptance criterion 1 and 2: the defect itself, as a regression.
// ---------------------------------------------------------------------------

#[test]
fn hello_decodes_for_every_valid_instance_id_including_the_pre_fix_failing_case() {
    // `1` is the exact case that encoded cleanly and then failed to decode with
    // `InvalidControl` under the pre-Plan-370 numeric representation.
    let boundaries: [u128; 5] = [1, 7, u64::MAX as u128, u64::MAX as u128 + 1, u128::MAX];
    for value in boundaries {
        let message = AppToHostMessage::Hello {
            request_id: request_id(1),
            app_id: app_id(),
            instance_id: instance(value),
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
        };
        let wire = encode_app_to_host_control(&message).expect("hello encodes");
        assert_eq!(
            decode_app_to_host_control(&wire).expect("hello decodes"),
            message,
            "hello must survive at {value}"
        );
        // Full 128-bit range retained, with no truncation.
        assert_eq!(decoded_instance_id(&wire).get(), value);
    }
    assert_eq!(u128::MAX.to_string().len(), MAX_DECIMAL_DIGITS);
}

#[test]
fn the_pre_fix_numeric_spelling_is_rejected_and_the_canonical_string_is_accepted() {
    // Before Plan 370 this numeric payload encoded and then failed to decode.
    // After Plan 370 it is a *rejected* spelling: the canonical form is a total
    // function, so exactly one encoding of an id is accepted.
    let numeric = hello_bytes("1");
    assert_eq!(
        decode_app_to_host_control(&numeric),
        Err(ContractError::InvalidControl)
    );
    let canonical = hello_bytes("\"1\"");
    let decoded = decode_app_to_host_control(&canonical).expect("canonical hello decodes");
    assert_eq!(decoded_instance_id_from(&decoded).get(), 1);
}

// ---------------------------------------------------------------------------
// Plan 370 acceptance criterion 3: canonical-form rejection, each case proven.
// ---------------------------------------------------------------------------

#[test]
fn non_canonical_and_over_length_instance_ids_fail_closed() {
    let rejected: [(&str, String); 15] = [
        ("empty string", "\"\"".to_owned()),
        ("zero", "\"0\"".to_owned()),
        ("leading zeros", "\"007\"".to_owned()),
        ("plus sign", "\"+1\"".to_owned()),
        ("minus sign", "\"-1\"".to_owned()),
        ("leading whitespace", "\" 1\"".to_owned()),
        ("trailing whitespace", "\"1 \"".to_owned()),
        ("embedded whitespace", "\"1 2\"".to_owned()),
        ("arabic-indic digit", "\"\u{0661}\"".to_owned()),
        ("fullwidth digit", "\"\u{ff11}\"".to_owned()),
        ("fractional", "\"1.0\"".to_owned()),
        ("exponent", "\"1e3\"".to_owned()),
        (
            "over length",
            format!("\"{}\"", "9".repeat(MAX_DECIMAL_DIGITS + 1)),
        ),
        ("json null", "null".to_owned()),
        ("json number", "1".to_owned()),
    ];
    for (label, raw) in rejected {
        let bytes = hello_bytes(&raw);
        assert_eq!(
            decode_app_to_host_control(&bytes),
            Err(ContractError::InvalidControl),
            "{label} must fail closed, but {raw} decoded"
        );
    }
}

#[test]
fn non_scalar_and_trailing_payloads_fail_closed() {
    for raw in ["true", "{}", "[]", "1.5"] {
        let bytes = hello_bytes(raw);
        assert!(
            decode_app_to_host_control(&bytes).is_err(),
            "{raw} must not decode as an instance id"
        );
    }
    let mut trailing = hello_bytes("\"1\"");
    trailing.extend_from_slice(b"{}");
    assert_eq!(
        decode_app_to_host_control(&trailing),
        Err(ContractError::InvalidControl)
    );
}

#[test]
fn hello_still_rejects_unknown_and_duplicate_fields() {
    assert!(
        decode_app_to_host_control(
            br#"{"type":"hello","request_id":1,"app_id":"sample-app","instance_id":"1","protocol_major":1,"protocol_minor":0,"extra":1}"#
        )
        .is_err()
    );
    assert!(
        decode_app_to_host_control(
            br#"{"type":"hello","request_id":1,"app_id":"sample-app","instance_id":"1","instance_id":"2","protocol_major":1,"protocol_minor":0}"#
        )
        .is_err()
    );
}

// ---------------------------------------------------------------------------
// Plan 370 acceptance criterion 4 and 5: goldens and one representation.
// ---------------------------------------------------------------------------

#[test]
fn hello_encodes_the_instance_id_as_canonical_decimal_digits() {
    let message = AppToHostMessage::Hello {
        request_id: request_id(1),
        app_id: app_id(),
        instance_id: instance(u128::MAX),
        protocol_major: PROTOCOL_MAJOR,
        protocol_minor: PROTOCOL_MINOR,
    };
    let wire = encode_app_to_host_control(&message).expect("hello encodes");
    assert_eq!(
        String::from_utf8(wire).expect("utf8"),
        format!(
            r#"{{"type":"hello","request_id":1,"app_id":"sample-app","instance_id":"{}","protocol_major":1,"protocol_minor":0}}"#,
            u128::MAX
        )
    );
}

#[test]
fn principal_and_owned_resource_present_exactly_one_representation() {
    use i2pr_app_proto::{AppPrincipal, OwnedResourceId, PrincipalOwnedResource};

    let principal = AppPrincipal {
        app_id: app_id(),
        instance_id: instance(7),
        publisher_id: Some(PublisherId::parse("sample-publisher").expect("publisher")),
    };
    let encoded = serde_json::to_string(&principal).expect("principal encodes");
    assert!(
        encoded.contains(r#""instance_id":"7""#),
        "principal must carry one decimal-digit spelling: {encoded}"
    );
    let decoded: AppPrincipal = serde_json::from_str(&encoded).expect("principal decodes");
    assert_eq!(decoded, principal);

    // The numeric spelling that `hello` used to carry is not accepted anywhere.
    assert!(
        serde_json::from_str::<AppPrincipal>(
            r#"{"app_id":"sample-app","instance_id":7,"publisher_id":null}"#
        )
        .is_err(),
        "the pre-Plan-370 numeric spelling must not be accepted"
    );

    let resource = PrincipalOwnedResource::new(
        principal,
        OwnedResourceId::new(u128::MAX).expect("nonzero resource id"),
    );
    let encoded = serde_json::to_string(&resource).expect("resource encodes");
    assert!(encoded.contains(r#""instance_id":"7""#));
    assert!(encoded.contains(&format!(r#""resource_id":{}"#, u128::MAX)));
    let decoded: PrincipalOwnedResource = serde_json::from_str(&encoded).expect("resource decodes");
    assert_eq!(decoded, resource);
}

// ---------------------------------------------------------------------------
// Plan 370 invariant 2: the id keeps its full range and still rejects zero.
// ---------------------------------------------------------------------------

#[test]
fn instance_id_keeps_the_full_128_bit_range_and_rejects_zero() {
    assert_eq!(AppInstanceId::new(0), Err(ContractError::InvalidOpaqueId));
    assert_eq!(
        AppInstanceId::parse("0"),
        Err(ContractError::InvalidOpaqueId)
    );
    let widest = AppInstanceId::new(u128::MAX).expect("widest id");
    assert_eq!(widest.get(), u128::MAX);
    assert_eq!(widest.as_str(), "340282366920938463463374607431768211455");
    assert_eq!(String::from(widest.clone()), widest.as_str());
    assert_eq!(AppInstanceId::parse(widest.as_str()), Ok(widest));
}

// ---------------------------------------------------------------------------
// Plan 370 invariant 3: no byte path yields effective authority.
// ---------------------------------------------------------------------------

#[test]
fn an_application_permission_request_cannot_mint_a_grant() {
    // `permission_request` decodes to `RequestedCapability`, which is a request.
    // There is no conversion from it to `GrantedCapability`; a grant requires an
    // `AdministratorPrincipal` obtained at the authenticated manager boundary.
    let message = AppToHostMessage::PermissionRequest {
        request_id: request_id(1),
        capabilities: vec![RequestedCapability {
            capability: Capability::Sam,
        }],
    };
    let wire = encode_app_to_host_control(&message).expect("permission request encodes");
    let decoded = decode_app_to_host_control(&wire).expect("permission request decodes");
    match decoded {
        AppToHostMessage::PermissionRequest { capabilities, .. } => {
            assert_eq!(
                capabilities,
                vec![RequestedCapability {
                    capability: Capability::Sam
                }]
            );
        }
        other => panic!("wrong variant: {other:?}"),
    }
    // `GrantedCapability` deliberately derives no `Deserialize`, so
    // `serde_json::from_str::<GrantedCapability>` does not compile and this
    // decoder is never reached from an application-supplied byte string.
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            r#"{"capabilities":[{"capability":"sam"},{"capability":"control_scoped"}]}"#
        )
        .expect("raw json parses"),
        serde_json::json!({"capabilities": [
            {"capability": "sam"},
            {"capability": "control_scoped"}
        ]})
    );
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn hello_bytes(raw_instance_id: &str) -> Vec<u8> {
    format!(
        r#"{{"type":"hello","request_id":1,"app_id":"sample-app","instance_id":{raw_instance_id},"protocol_major":1,"protocol_minor":0}}"#
    )
    .into_bytes()
}

fn decoded_instance_id(wire: &[u8]) -> AppInstanceId {
    decoded_instance_id_from(&decode_app_to_host_control(wire).expect("decode"))
}

fn decoded_instance_id_from(message: &AppToHostMessage) -> AppInstanceId {
    match message {
        AppToHostMessage::Hello { instance_id, .. } => instance_id.clone(),
        other => panic!("not a hello: {other:?}"),
    }
}
