use i2pr_proto::{I2npMessage, MAX_I2NP_PAYLOAD_SIZE, ProtocolErrorKind, STANDARD_HEADER_SIZE};

const MAX: usize = MAX_I2NP_PAYLOAD_SIZE + STANDARD_HEADER_SIZE;

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("../../../tests/fixtures/i2np/", $name))
    };
}

fn bytes(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|pair| u8::from_str_radix(pair, 16).expect("fixture hex"))
        .collect()
}

fn assert_positive(name: &str, text: &str, short: Option<&str>) {
    let input = bytes(text);
    let message = match short {
        Some("ssu") => I2npMessage::decode_short_ssu(&input, MAX).expect(name),
        Some("transport") => I2npMessage::decode_short_transport(&input, MAX).expect(name),
        None => I2npMessage::decode_standard(&input, MAX).expect(name),
        Some(other) => panic!("unknown fixture header variant {other}"),
    };
    let encoded = match short {
        Some("ssu") => message
            .encode_short_ssu_to_vec(MAX)
            .expect("short SSU re-encode"),
        Some("transport") => message
            .encode_short_transport_to_vec(MAX)
            .expect("short transport re-encode"),
        None => message
            .encode_standard_to_vec(MAX)
            .expect("standard re-encode"),
        Some(_) => unreachable!(),
    };
    assert_eq!(encoded, input, "canonical fixture changed: {name}");
}

#[test]
fn every_positive_fixture_decodes_and_reencodes_canonically() {
    assert_positive(
        "standard-delivery-status",
        fixture!("standard-delivery-status.hex"),
        None,
    );
    assert_positive(
        "obsolete-ssu-short",
        fixture!("positive-obsolete-ssu-short.hex"),
        Some("ssu"),
    );
    assert_positive(
        "ntcp2-ssu2-short",
        fixture!("positive-ntcp2-ssu2-short.hex"),
        Some("transport"),
    );
    assert_positive(
        "database-lookup-none",
        fixture!("positive-database-lookup-none.hex"),
        None,
    );
    assert_positive(
        "database-lookup-legacy",
        fixture!("positive-database-lookup-legacy.hex"),
        None,
    );
    assert_positive(
        "database-lookup-ecies",
        fixture!("positive-database-lookup-ecies.hex"),
        None,
    );
    assert_positive(
        "database-search-reply",
        fixture!("positive-database-search-reply.hex"),
        None,
    );
    assert_positive(
        "database-store-classic-leaseset",
        fixture!("positive-database-store-classic-leaseset.hex"),
        None,
    );
    assert_positive(
        "database-store-compressed-router-info",
        fixture!("positive-database-store-compressed-router-info.hex"),
        None,
    );
    assert_positive("tunnel-data", fixture!("positive-tunnel-data.hex"), None);
    assert_positive(
        "tunnel-gateway",
        fixture!("positive-tunnel-gateway-nested-standard.hex"),
        None,
    );
    assert_positive(
        "variable-tunnel-build",
        fixture!("positive-variable-tunnel-build.hex"),
        None,
    );
    assert_positive(
        "short-tunnel-build",
        fixture!("positive-short-tunnel-build.hex"),
        None,
    );
    assert_positive(
        "garlic-deferred-length",
        fixture!("positive-garlic-deferred-length.hex"),
        None,
    );
    assert_positive(
        "data-deferred-length",
        fixture!("positive-data-deferred-length.hex"),
        None,
    );
}

#[test]
fn positive_fixture_truncations_fail_without_panics() {
    let fixtures = [
        fixture!("standard-delivery-status.hex"),
        fixture!("positive-database-lookup-legacy.hex"),
        fixture!("positive-database-store-classic-leaseset.hex"),
        fixture!("positive-tunnel-gateway-nested-standard.hex"),
        fixture!("positive-variable-tunnel-build.hex"),
        fixture!("positive-garlic-deferred-length.hex"),
    ];
    for text in fixtures {
        let input = bytes(text);
        for end in 0..input.len() {
            assert!(I2npMessage::decode_standard(&input[..end], MAX).is_err());
        }
    }
}

#[test]
fn malformed_fixture_errors_remain_typed() {
    let cases = [
        (
            fixture!("malformed-checksum.hex"),
            ProtocolErrorKind::InvalidValue,
        ),
        (
            fixture!("malformed-truncated-standard-header.hex"),
            ProtocolErrorKind::Truncated,
        ),
        (
            fixture!("malformed-declared-payload-too-large.hex"),
            ProtocolErrorKind::Truncated,
        ),
        (
            fixture!("malformed-trailing-bytes.hex"),
            ProtocolErrorKind::TrailingBytes,
        ),
        (
            fixture!("malformed-unknown-message-type.hex"),
            ProtocolErrorKind::Unsupported,
        ),
        (
            fixture!("malformed-database-lookup-invalid-flags.hex"),
            ProtocolErrorKind::InvalidValue,
        ),
        (
            fixture!("malformed-database-lookup-zero-reply-tags.hex"),
            ProtocolErrorKind::InvalidValue,
        ),
        (
            fixture!("malformed-database-lookup-excessive-reply-tags.hex"),
            ProtocolErrorKind::InvalidValue,
        ),
        (
            fixture!("malformed-database-lookup-excessive-exclusions.hex"),
            ProtocolErrorKind::PolicyRejected,
        ),
        (
            fixture!("malformed-database-search-reply-excessive-peers.hex"),
            ProtocolErrorKind::PolicyRejected,
        ),
        (
            fixture!("malformed-tunnel-data-zero-id.hex"),
            ProtocolErrorKind::InvalidValue,
        ),
        (
            fixture!("malformed-tunnel-data-invalid-length.hex"),
            ProtocolErrorKind::Truncated,
        ),
        (
            fixture!("malformed-tunnel-build-zero-records.hex"),
            ProtocolErrorKind::InvalidValue,
        ),
        (
            fixture!("malformed-tunnel-build-excessive-records.hex"),
            ProtocolErrorKind::InvalidValue,
        ),
        (
            fixture!("malformed-tunnel-gateway-nested-message.hex"),
            ProtocolErrorKind::Unsupported,
        ),
        (
            fixture!("malformed-deferred-payload-maximum-plus-one.hex"),
            ProtocolErrorKind::LimitExceeded,
        ),
    ];
    for (index, (text, expected)) in cases.into_iter().enumerate() {
        let input = bytes(text);
        let error = I2npMessage::decode_standard(&input, MAX)
            .expect_err("malformed fixture unexpectedly decoded");
        assert_eq!(
            error.kind(),
            expected,
            "malformed fixture index {index}: {error:?}"
        );
    }
}

#[test]
fn reply_secret_debug_is_redacted_and_only_memory_hygiene_is_claimed() {
    for text in [
        fixture!("positive-database-lookup-legacy.hex"),
        fixture!("positive-database-lookup-ecies.hex"),
    ] {
        let message = I2npMessage::decode_standard(&bytes(text), MAX).expect("lookup fixture");
        let rendered = format!("{:?}", message.body());
        assert!(rendered.contains("tag_count"));
        assert!(!rendered.contains("202020"));
        assert!(!rendered.contains("222222"));
        assert!(!rendered.contains("232323"));
    }
}
