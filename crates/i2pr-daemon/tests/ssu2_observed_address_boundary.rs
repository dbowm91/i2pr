//! Plan 529's guard -- the router must learn the address at which a peer sees it.
//!
//! The SSU2 specification's Address block carries "Alice's address, sent to
//! Alice by Bob, or Bob's address, sent to Bob by Alice" -- the address at
//! which the **recipient** sees the **sender**. The tree sent the block with
//! the **wrong value**: the responder put its own bound address there, which
//! told the initiator that the initiator's address was the responder's. And
//! the tree **threw the answer away**: the initiator decoded the Session
//! Created's payload and never read the block.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the responder's Address block carries the **observed source address**;
//! 2. the initiator **reads** the Address block out of the Session Created
//!    payload;
//! 3. the observation reaches Plan 527's comparison, against the router's
//!    **own bound socket**;
//! 4. the transport's reachability **model** is untouched;
//! 5. the plan adds no configuration key and no infrastructure;
//! 6. the Retry's Address block is still **not** a reachability observation.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, and every rule
//! **fails closed** when the item it names is absent.

#![forbid(unsafe_code)]

const TRANSPORT_SSU2_STATE_MACHINE: &str = "crates/i2pr-transport-ssu2/src/state_machine.rs";
const TRANSPORT_SSU2_HANDSHAKE: &str = "crates/i2pr-transport-ssu2/src/handshake.rs";
const TRANSPORT_REACHABILITY_MODULE: &str = "crates/i2pr-transport/src/reachability.rs";
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const CONFIG_MODULE: &str = "crates/i2pr-daemon/src/config.rs";

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Reads one source file from the real tree.
fn source_of(relative: &str) -> String {
    let path = workspace_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("read {}: {error}", path.display());
    })
}

/// Removes comments from Rust source text.
///
/// The scanner must not report a doc comment that merely *names* a
/// construct, so the text is reduced to its code before the tokens are
/// matched.
fn strip_comments(source: &str) -> String {
    let bytes: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut index = 0usize;
    let mut in_string = false;
    let mut in_char = false;
    let mut in_line_comment = false;
    let mut block_depth = 0usize;
    while index < bytes.len() {
        let current = bytes[index];
        let next = bytes.get(index + 1).copied();
        if in_line_comment {
            if current == '\n' {
                in_line_comment = false;
                out.push('\n');
            }
            index += 1;
            continue;
        }
        if block_depth > 0 {
            if current == '/' && next == Some('*') {
                block_depth += 1;
                index += 2;
                continue;
            }
            if current == '*' && next == Some('/') {
                block_depth -= 1;
                index += 2;
                continue;
            }
            if current == '\n' {
                out.push('\n');
            }
            index += 1;
            continue;
        }
        if in_string {
            out.push(current);
            if current == '\\' {
                if let Some(escaped) = next {
                    out.push(escaped);
                }
                index += 2;
                continue;
            }
            if current == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if in_char {
            out.push(current);
            if current == '\\' {
                if let Some(escaped) = next {
                    out.push(escaped);
                }
                index += 2;
                continue;
            }
            if current == '\'' {
                in_char = false;
            }
            index += 1;
            continue;
        }
        if current == '/' && next == Some('/') {
            in_line_comment = true;
            index += 2;
            continue;
        }
        if current == '/' && next == Some('*') {
            block_depth = 1;
            index += 2;
            continue;
        }
        if current == '"' {
            in_string = true;
            out.push(current);
            index += 1;
            continue;
        }
        if current == '\'' {
            in_char = true;
            out.push(current);
            index += 1;
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
}

/// Removes `#[cfg(test)]` blocks from source text.
///
/// A test-only helper must never satisfy a production rule.
fn strip_cfg_test_blocks(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut index = 0usize;
    while index < code.len() {
        let rest = &code[index..];
        if rest.starts_with("#[cfg(test)]") {
            let after = index + "#[cfg(test)]".len();
            let Some(open) = code[after..].find('{') else {
                break;
            };
            let open = after + open;
            let Some((_, close)) = brace_balanced_block(code, open) else {
                break;
            };
            out.push(' ');
            index = close;
            continue;
        }
        let character = rest.chars().next().expect("character");
        out.push(character);
        index += character.len_utf8();
    }
    out
}

/// The code view of a source file: no comments and no test-only blocks.
fn code_view(source: &str) -> String {
    strip_cfg_test_blocks(&strip_comments(source))
}

/// Returns the offset of a **definition** whose text is `needle`.
fn find_definition(code: &str, needle: &str) -> Option<usize> {
    let mut offset = 0usize;
    while let Some(position) = code[offset..].find(needle) {
        let start = offset + position;
        let after = start + needle.len();
        offset = after;
        let continues_identifier = code[after..]
            .chars()
            .next()
            .is_some_and(|character| character.is_alphanumeric() || character == '_');
        if continues_identifier {
            continue;
        }
        return Some(start);
    }
    None
}

/// Returns the byte range of the brace-balanced block that opens at or after
/// `start`, as `(open, close)`; `close` is one past the closing brace.
fn brace_balanced_block(code: &str, start: usize) -> Option<(usize, usize)> {
    let bytes = code.as_bytes();
    let open = code[start..].find('{')? + start;
    let mut depth = 0usize;
    let mut cursor = open;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' => {
                cursor = skip_string_literal(bytes, cursor);
                continue;
            }
            b'\'' => {
                if let Some(end) = skip_char_literal(bytes, cursor) {
                    cursor = end;
                    continue;
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((open, cursor + 1));
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

/// Returns the offset just past the string literal that starts at `start`.
fn skip_string_literal(bytes: &[u8], start: usize) -> usize {
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor += 2,
            b'"' => return cursor + 1,
            _ => cursor += 1,
        }
    }
    cursor
}

/// Returns the offset just past the character literal that starts at
/// `start`, or `None` when the quote opens a lifetime instead.
fn skip_char_literal(bytes: &[u8], start: usize) -> Option<usize> {
    let is_char_literal = match bytes.get(start + 1) {
        Some(b'\\') => true,
        Some(_) => bytes.get(start + 2) == Some(&b'\''),
        None => false,
    };
    if !is_char_literal {
        return None;
    }
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor += 2,
            b'\'' => return Some(cursor + 1),
            _ => cursor += 1,
        }
    }
    Some(cursor)
}

/// Extracts the body of the item whose **definition** is anchored at
/// `signature`.
fn item_body(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (open, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[open + 1..close - 1].to_owned())
}

/// Extracts the item's **whole** text: its signature and its body.
///
/// A rule that is about what an item *takes* -- a permit, an argument --
/// must read the signature too, which the body alone does not carry.
fn item_text(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (_, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[start..close].to_owned())
}

// ---- 1. the responder reports the observed source address --------------
fn check_the_responder_reports_the_observed_source_address(state_machine: &str) -> Option<String> {
    let code = &code_view(state_machine);

    let Some(helper) = item_body(
        code,
        "fn observed_address_block(source: SocketAddr) -> Option<AddressBlock>",
    ) else {
        return Some("the observed-address block builder is absent".to_owned());
    };
    if !helper.contains("Ssu2Endpoint::from_socket_addr(source)") {
        return Some(
            "the Address block must be built from the request's observed source address, and
             never from a configured or bound value"
                .to_owned(),
        );
    }
    let Some(responder) = item_text(code, "pub struct ResponderConfig") else {
        return Some("the responder configuration is absent".to_owned());
    };
    if responder.contains("local_address") {
        return Some(
            "the responder must not publish its own bound address as the initiator's".to_owned(),
        );
    }
    let Some(session_request) = item_body(code, "pub fn on_session_request(") else {
        return Some("the responder's session-request handler is absent".to_owned());
    };
    if !session_request.contains("observed_address_block(source)") {
        return Some(
            "the Session Created must carry the address at which the responder sees the
             initiator"
                .to_owned(),
        );
    }
    None
}

// ---- 2. the initiator reads the address block ---------------------------
fn check_the_initiator_reads_the_address_block(
    state_machine: &str,
    handshake: &str,
) -> Option<String> {
    let code = &code_view(state_machine);
    let handshake = &code_view(handshake);

    let Some(reader) = item_body(
        handshake,
        "pub fn find_observed_address(payload: &[u8]) -> Option<crate::address::Ssu2Endpoint>",
    ) else {
        return Some("the Address block reader is absent".to_owned());
    };
    if !reader.contains("DecodedBlock::Address") {
        return Some("the Address block reader must read the Address block".to_owned());
    }
    let Some(created) = item_body(code, "pub fn on_session_created(") else {
        return Some("the initiator's session-created handler is absent".to_owned());
    };
    if !created.contains("find_observed_address(&created_payload)") {
        return Some(
            "the initiator must read the Address block out of the decrypted Session Created
             payload"
                .to_owned(),
        );
    }
    let Some(session) = item_text(code, "pub struct AuthenticatedSsu2Session") else {
        return Some("the established session is absent".to_owned());
    };
    if !session.contains("observed_address: Option<Ssu2Endpoint>") {
        return Some("the established session must carry the observation".to_owned());
    }
    let Some(accessor) = item_body(
        code,
        "pub const fn observed_address(&self) -> Option<Ssu2Endpoint>",
    ) else {
        return Some("the observation accessor is absent".to_owned());
    };
    if !accessor.contains("self.observed_address") {
        return Some("the accessor must read the session's own field".to_owned());
    }
    None
}

// ---- 3. the observation reaches Plan 527's comparison -------------------
fn check_the_observation_reaches_the_comparison(runtime: &str) -> Option<String> {
    let code = &code_view(runtime);

    let Some(construction) = item_body(code, "fn construct_session(") else {
        return Some("the promotion seam is absent".to_owned());
    };
    if !construction.contains("auth.observed_address()") {
        return Some("the promotion seam must read the session's observation".to_owned());
    }
    if !construction.contains("note_observed_external_address_locked(state, observed)") {
        return Some("the observation must reach Plan 527's comparison".to_owned());
    }
    let Some(comparison) = item_body(code, "fn note_observed_external_address_locked(") else {
        return Some("the comparison is absent".to_owned());
    };
    if !comparison.contains("bound_endpoint(family)") {
        return Some(
            "the comparison must be made against the router's own bound socket".to_owned(),
        );
    }
    None
}

// ---- 4. the transport's reachability model is untouched -----------------
fn check_the_reachability_model_is_untouched(reachability: &str) -> Option<String> {
    let code = &code_view(reachability);

    if !code.contains("pub const MIN_CORROBORATION_FLOOR: usize = 2;") {
        return Some(
            "the corroboration floor must stay at two distinct supporting classes: a router is
             reachable only above it"
                .to_owned(),
        );
    }
    let Some(signals) = item_text(code, "pub enum ReachabilitySignal") else {
        return Some("the reachability signal vocabulary is absent".to_owned());
    };
    for variant in [
        "LocalConfiguredBind",
        "AuthenticatedPeerObservedExternalAddress",
        "ValidatedPath",
        "PeerTestResult",
        "RelayFirewalledSignal",
    ] {
        if !signals.contains(variant) {
            return Some(format!(
                "the signal vocabulary must keep the {variant:?} kind"
            ));
        }
    }
    let Some(classes) = item_body(code, "const fn corroboration_class(self) -> u8") else {
        return Some("the corroboration classes are absent".to_owned());
    };
    for needle in [
        "Self::LocalConfiguredBind { .. } => 0",
        "Self::AuthenticatedPeerObservedExternalAddress { .. } => 1",
        "Self::ValidatedPath { .. } => 2",
        "Self::PeerTestResult { .. } => 3",
        "Self::RelayFirewalledSignal { .. } => 4",
    ] {
        if !classes.contains(needle) {
            return Some(format!(
                "the corroboration class must stay exactly as it was: {needle:?} is absent"
            ));
        }
    }
    if classes.matches("=>").count() != 5 {
        return Some(
            "the transport's reachability model must keep exactly five corroboration classes:
             the plan adds none"
                .to_owned(),
        );
    }
    None
}

// ---- 5. no new key and no new infrastructure ---------------------------
fn check_no_new_key_and_no_new_infrastructure(
    runtime: &str,
    state_machine: &str,
    config: &str,
) -> Option<String> {
    let runtime = &code_view(runtime);
    let state_machine = &code_view(state_machine);
    let config = &code_view(config);

    for needle in [
        "ssu2.observed",
        "ssu2.external_address",
        "ssu2.address_observation",
        "ssu2.observed_address",
    ] {
        if config.contains(needle) {
            return Some(format!(
                "the plan must add no configuration key: found {needle:?}"
            ));
        }
    }
    // The plan's own new code must add no task, no timer and no listener.
    // The runtime crate as a whole legitimately owns all three, so the
    // check is scoped to the code this plan added.
    for (code, needle) in [
        (runtime, "fn construct_session("),
        (runtime, "fn responder_config("),
        (
            state_machine,
            "fn observed_address_block(source: SocketAddr) -> Option<AddressBlock>",
        ),
        (state_machine, "pub fn on_session_created("),
        (state_machine, "pub fn on_session_request("),
    ] {
        let Some(body) = item_body(code, needle) else {
            return Some(format!("the plan's own code is absent: {needle:?}"));
        };
        for forbidden in [
            "tokio::spawn",
            "thread::spawn",
            "UdpSocket::bind",
            "sleep(",
            "interval(",
        ] {
            if body.contains(forbidden) {
                return Some(format!(
                    "the plan must add no infrastructure: {needle:?} contains {forbidden:?}"
                ));
            }
        }
    }
    None
}

// ---- 6. the Retry's Address block is not a reachability observation -----
fn check_the_retry_is_not_a_reachability_observation(
    handshake: &str,
    runtime: &str,
) -> Option<String> {
    let handshake = &code_view(handshake);
    let runtime = &code_view(runtime);

    let Some(parser) = item_body(handshake, "pub fn parse_retry(") else {
        return Some("the Retry parser is absent".to_owned());
    };
    if !parser.contains("seen_address = true;") {
        return Some("the Retry parser must keep discarding the Address block".to_owned());
    }
    if parser.contains("find_observed_address") {
        return Some(
            "the Retry's Address block is sealed outside the Noise transcript: it is a weaker
             fact and must never become a reachability observation"
                .to_owned(),
        );
    }
    let Some(retry_path) = item_body(runtime, "fn drive_pending_outbound(") else {
        return Some("the runtime's retry path is absent".to_owned());
    };
    if retry_path.contains("note_observed_external_address_locked") {
        return Some(
            "the runtime must not record a reachability observation from a Retry".to_owned(),
        );
    }
    None
}

// ---- the rules --------------------------------------------------------
#[test]
fn the_responder_reports_the_observed_source_address() {
    let reason = check_the_responder_reports_the_observed_source_address(&source_of(
        TRANSPORT_SSU2_STATE_MACHINE,
    ));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_initiator_reads_the_address_block() {
    let reason = check_the_initiator_reads_the_address_block(
        &source_of(TRANSPORT_SSU2_STATE_MACHINE),
        &source_of(TRANSPORT_SSU2_HANDSHAKE),
    );
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_observation_reaches_the_comparison() {
    let reason = check_the_observation_reaches_the_comparison(&source_of(RUNTIME_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_reachability_model_is_untouched() {
    let reason =
        check_the_reachability_model_is_untouched(&source_of(TRANSPORT_REACHABILITY_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn no_new_key_and_no_infrastructure() {
    let reason = check_no_new_key_and_no_new_infrastructure(
        &source_of(RUNTIME_MODULE),
        &source_of(TRANSPORT_SSU2_STATE_MACHINE),
        &source_of(CONFIG_MODULE),
    );
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_retry_is_not_a_reachability_observation() {
    let reason = check_the_retry_is_not_a_reachability_observation(
        &source_of(TRANSPORT_SSU2_HANDSHAKE),
        &source_of(RUNTIME_MODULE),
    );
    assert!(reason.is_none(), "{reason:?}");
}

// ---- the inverted probes ----------------------------------------------
#[test]
fn the_guard_reports_a_responder_that_publishes_its_own_address() {
    let mut violating = source_of(TRANSPORT_SSU2_STATE_MACHINE);
    violating = violating.replace(
        "    pub local_mtu: u16,\n}",
        "    pub local_mtu: u16,\n    /// Responder address block echoed in Retry payloads.\n    pub local_address: AddressBlock,\n}",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_SSU2_STATE_MACHINE),
        "the mutation must change the source"
    );
    let reason = check_the_responder_reports_the_observed_source_address(&violating);
    assert!(
        reason.is_some(),
        "a responder that publishes its own address must be reported"
    );
}

#[test]
fn the_guard_reports_a_block_built_from_something_else() {
    let mut violating = source_of(TRANSPORT_SSU2_STATE_MACHINE);
    violating = violating.replace(
        "Ssu2Endpoint::from_socket_addr(source)",
        "Ssu2Endpoint::from_socket_addr(bound)",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_SSU2_STATE_MACHINE),
        "the mutation must change the source"
    );
    let reason = check_the_responder_reports_the_observed_source_address(&violating);
    assert!(
        reason.is_some(),
        "a block built from anything but the observed source must be reported"
    );
}

#[test]
fn the_guard_reports_an_initiator_that_discards_the_address_block() {
    let mut violating = source_of(TRANSPORT_SSU2_STATE_MACHINE);
    violating = violating.replace(
        "find_observed_address(&created_payload)",
        "find_establishment_token(&created_payload)",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_SSU2_STATE_MACHINE),
        "the mutation must change the source"
    );
    let reason = check_the_initiator_reads_the_address_block(
        &violating,
        &source_of(TRANSPORT_SSU2_HANDSHAKE),
    );
    assert!(
        reason.is_some(),
        "an initiator that discards the Address block must be reported"
    );
}

#[test]
fn the_guard_reports_an_observation_that_never_reaches_the_comparison() {
    let mut violating = source_of(RUNTIME_MODULE);
    violating = violating.replace(
        "note_observed_external_address_locked(state, observed)",
        "drop_observed_address(state, observed)",
    );
    assert_ne!(
        violating,
        source_of(RUNTIME_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_observation_reaches_the_comparison(&violating);
    assert!(
        reason.is_some(),
        "an observation that never reaches the comparison must be reported"
    );
}

#[test]
fn the_guard_reports_a_changed_reachability_model() {
    let mut violating = source_of(TRANSPORT_REACHABILITY_MODULE);
    violating = violating.replace(
        "Self::AuthenticatedPeerObservedExternalAddress { .. } => 1,",
        "Self::AuthenticatedPeerObservedExternalAddress { .. } => 6,",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_REACHABILITY_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_reachability_model_is_untouched(&violating);
    assert!(
        reason.is_some(),
        "a changed reachability model must be reported"
    );
}

#[test]
fn the_guard_reports_a_new_configuration_key() {
    let mut violating = source_of(CONFIG_MODULE);
    // The rule reads the code view, so the mutation must be code and not a
    // comment.
    violating.push_str("\nconst _SSU2_OBSERVED_ADDRESS_KEY: &str = \"ssu2.observed_address\";\n");
    let reason = check_no_new_key_and_no_new_infrastructure(
        &source_of(RUNTIME_MODULE),
        &source_of(TRANSPORT_SSU2_STATE_MACHINE),
        &violating,
    );
    assert!(reason.is_some(), "a new configuration key must be reported");
}

#[test]
fn the_guard_reports_new_infrastructure() {
    let mut violating = source_of(RUNTIME_MODULE);
    // The spawn goes **inside** the item's body, so the rule reads it as
    // the item's own code and not as text that sits outside it.
    let anchor = "fn construct_session(";
    let start = violating.find(anchor).expect("the anchor is present");
    let brace = violating[start..]
        .find('{')
        .expect("the item opens a brace")
        + start;
    violating.insert_str(brace + 1, "\n        tokio::spawn(async {});");
    assert_ne!(
        violating,
        source_of(RUNTIME_MODULE),
        "the mutation must change the source"
    );
    let reason = check_no_new_key_and_no_new_infrastructure(
        &violating,
        &source_of(TRANSPORT_SSU2_STATE_MACHINE),
        &source_of(CONFIG_MODULE),
    );
    assert!(reason.is_some(), "new infrastructure must be reported");
}

#[test]
fn the_guard_reports_a_retry_that_becomes_a_reachability_observation() {
    let mut violating = source_of(TRANSPORT_SSU2_HANDSHAKE);
    violating = violating.replace(
        "seen_address = true;",
        "seen_address = true;\n                let _ = find_observed_address(payload);",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_SSU2_HANDSHAKE),
        "the mutation must change the source"
    );
    let reason =
        check_the_retry_is_not_a_reachability_observation(&violating, &source_of(RUNTIME_MODULE));
    assert!(
        reason.is_some(),
        "a Retry that became a reachability observation must be reported"
    );
}

// ---- the missing-item probe -------------------------------------------
#[test]
fn the_guard_reports_a_missing_named_item() {
    let empty = String::new();
    let reasons = [
        check_the_responder_reports_the_observed_source_address(&empty),
        check_the_initiator_reads_the_address_block(&empty, &empty),
        check_the_observation_reaches_the_comparison(&empty),
        check_the_reachability_model_is_untouched(&empty),
        check_no_new_key_and_no_new_infrastructure(&empty, &empty, &empty),
        check_the_retry_is_not_a_reachability_observation(&empty, &empty),
    ];
    for reason in reasons {
        assert!(
            reason.is_some(),
            "a rule whose named item is absent must be reported, not passed"
        );
    }
}
