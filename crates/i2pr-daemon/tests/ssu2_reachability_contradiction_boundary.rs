//! Plan 527's guard -- the router's reachability has one owner and one reader.
//!
//! Before this plan the router's reachability had **two** trackers: the
//! runtime owned one and the product read it, and the peer-relay service
//! owned another that **nothing** read. Every contradicting signal was
//! written into the second one, so the tracker the product read could only
//! ever be *confirmed* and `ReachabilityState::Firewalled` and
//! `Unreachable` were unreachable states.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the router's reachability has **one** tracker, and the runtime owns
//!    it;
//! 2. the peer-relay service owns **no** tracker at all;
//! 3. the transport keeps the observed endpoint it already decoded, and
//!    the transport's reachability model is untouched;
//! 4. the runtime compares the observation with the router's **own bound
//!    address**, and a disagreement contradicts;
//! 5. the runtime records **every** typed peer-test outcome, not only the
//!    confirming one;
//! 6. the runtime owns the relay-success recorder;
//! 7. the product reads the runtime's tracker, and the runtime's snapshot
//!    reads the one tracker;
//! 8. the plan adds no configuration key and no infrastructure.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, and every rule
//! **fails closed** when the item it names is absent.

#![forbid(unsafe_code)]

const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const PEER_RELAY_MODULE: &str = "crates/i2pr-runtime/src/ssu2_peer_relay.rs";
const TRANSPORT_SESSION_MODULE: &str = "crates/i2pr-transport-ssu2/src/session.rs";
const TRANSPORT_REACHABILITY_MODULE: &str = "crates/i2pr-transport/src/reachability.rs";
const DAEMON_GATE_MODULE: &str = "crates/i2pr-daemon/src/introducer_request.rs";
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";

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

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

// ---- 1. the router has one reachability tracker -----------------------
fn check_the_router_has_one_reachability_tracker(
    runtime: &str,
    peer_relay: &str,
) -> Option<String> {
    let runtime = &code_view(runtime);
    let peer_relay = &code_view(peer_relay);
    let owned = runtime.matches("reachability: ReachabilityTracker").count();
    if owned != 1 {
        return Some(format!(
            "the runtime must own exactly one reachability tracker, found {owned}"
        ));
    }
    if peer_relay.contains("ReachabilityTracker") {
        return Some(
            "the peer-relay service must not own a second reachability tracker: every              contradicting signal was written into it and nothing read it"
                .to_owned(),
        );
    }
    None
}

// ---- 2. the peer-relay service owns no tracker ------------------------
fn check_the_peer_relay_service_owns_no_tracker(peer_relay: &str) -> Option<String> {
    let peer_relay = &code_view(peer_relay);

    if !peer_relay.contains("pub struct Ssu2PeerRelayService") {
        return Some("the peer-relay service is absent".to_owned());
    }
    for needle in [
        "reachability: Mutex<ReachabilityTracker>",
        "ReachabilityTracker::new(",
        "fn reachability_state(",
        "fn note_relay_firewalled(",
        "fn note_validated_path(",
        "fn mirror_peer_outcome(",
    ] {
        if peer_relay.contains(needle) {
            return Some(format!(
                "the peer-relay service must not own a reachability tracker: found {needle:?}"
            ));
        }
    }
    None
}

// ---- 3. the transport keeps the observed endpoint ----------------------
fn check_the_transport_keeps_the_observed_endpoint(session: &str) -> Option<String> {
    let session = &code_view(session);

    let Some(event) = item_text(session, "pub enum SessionEvent") else {
        return Some("the session's event enum is absent".to_owned());
    };
    if !event.contains("AddressObserved {") {
        return Some("the observation event must carry the observed endpoint".to_owned());
    }
    if !event.contains("observed:") {
        return Some("the observation event must name the observed endpoint".to_owned());
    }
    let Some(decode) = item_body(session, "DecodedBlock::Address(block)") else {
        return Some("the Address block's decode site is absent".to_owned());
    };
    if !decode.contains("block.endpoint()") {
        return Some("the decoded endpoint must survive into the event".to_owned());
    }
    None
}

// ---- 4. the transport's reachability model is untouched ----------------
fn check_the_transport_model_is_untouched(reachability: &str) -> Option<String> {
    let reachability = &code_view(reachability);

    for needle in [
        "pub enum PeerTestOutcomeKind",
        "Confirmed",
        "AddressMismatch",
        "FirewalledLikely",
        "Inconclusive",
        "Rejected",
        "fn contradicting_classes(&self) -> usize",
        "fn supporting_classes(&self) -> usize",
        "MIN_CORROBORATION_FLOOR",
    ] {
        if !reachability.contains(needle) {
            return Some(format!(
                "the transport's reachability model must not change: {needle:?} is absent"
            ));
        }
    }
    None
}

// ---- 5. the runtime compares with the bound address -------------------
fn check_the_runtime_compares_with_the_bound_address(runtime: &str) -> Option<String> {
    let runtime = &code_view(runtime);

    let Some(comparison) = item_body(runtime, "fn note_observed_external_address_locked(") else {
        return Some("the comparison is absent".to_owned());
    };
    if !comparison.contains("bound_endpoint(") {
        return Some(
            "the comparison must be made against the router's own bound address".to_owned(),
        );
    }
    if !comparison.contains("AddressMismatch") {
        return Some("a disagreement must contradict the router's reachability".to_owned());
    }
    if !comparison.contains("AuthenticatedPeerObservedExternalAddress") {
        return Some("an agreement must corroborate the router's reachability".to_owned());
    }
    let Some(bound) = item_body(runtime, "fn bound_endpoint(") else {
        return Some("the bound-address lookup is absent".to_owned());
    };
    if !bound.contains("sockets.v4") || !bound.contains("sockets.v6") {
        return Some("the bound address must come from the router's own socket".to_owned());
    }
    None
}

// ---- 6. every typed outcome is recorded -------------------------------
fn check_every_typed_outcome_is_recorded(runtime: &str) -> Option<String> {
    let runtime = &code_view(runtime);

    let Some(recorder) = item_body(runtime, "fn record_peer_test_outcome_locked(") else {
        return Some("the typed-outcome recorder is absent".to_owned());
    };
    for (outcome, kind) in [
        (
            "PeerTestOutcome::DirectReachabilityConfirmed",
            "PeerTestOutcomeKind::Confirmed",
        ),
        (
            "PeerTestOutcome::AddressMismatch",
            "PeerTestOutcomeKind::AddressMismatch",
        ),
        (
            "PeerTestOutcome::FirewalledLikely",
            "PeerTestOutcomeKind::FirewalledLikely",
        ),
        (
            "PeerTestOutcome::Inconclusive",
            "PeerTestOutcomeKind::Inconclusive",
        ),
        ("PeerTestOutcome::Rejected", "PeerTestOutcomeKind::Rejected"),
    ] {
        for needle in [outcome, kind] {
            if !recorder.contains(needle) {
                return Some(format!(
                    "every typed peer-test outcome must be recorded, and each must be                      recorded under its own kind: {needle:?} is absent"
                ));
            }
        }
    }
    None
}

// ---- 7. the runtime owns the relay-success recorder -------------------
fn check_the_runtime_owns_the_relay_success_recorder(
    runtime: &str,
    peer_relay: &str,
) -> Option<String> {
    let runtime = &code_view(runtime);
    let peer_relay = &code_view(peer_relay);

    let Some(recorder) = item_body(runtime, "pub fn note_relay_firewalled(") else {
        return Some("the runtime's relay-success recorder is absent".to_owned());
    };
    if !recorder.contains("RelayFirewalledSignal") {
        return Some("the runtime's recorder must record the relay-firewalled signal".to_owned());
    }
    if peer_relay.contains("fn note_relay_firewalled(") {
        return Some("the peer-relay service must not keep its own recorder".to_owned());
    }
    None
}

// ---- 8. the product reads the runtime's tracker -----------------------
fn check_the_product_reads_the_runtime_s_tracker(
    composition: &str,
    runtime: &str,
) -> Option<String> {
    let composition = &code_view(composition);
    let runtime = &code_view(runtime);
    let Some(gate) = item_body(composition, "fn drive_introducer_request(") else {
        return Some("the firewalled gate is absent".to_owned());
    };
    if !gate.contains("service().snapshot().reachability") {
        return Some(
            "the firewalled gate must read the runtime's tracker, not a second one".to_owned(),
        );
    }
    let Some(snapshot) = item_body(runtime, "pub fn snapshot(&self) -> Ssu2Snapshot") else {
        return Some("the runtime's snapshot is absent".to_owned());
    };
    if !snapshot.contains("state.reachability.state()") {
        return Some("the runtime's snapshot must read the runtime's own tracker".to_owned());
    }
    None
}

// ---- 9. the gate stays fail-closed ------------------------------------
fn check_the_gate_stays_fail_closed(gate: &str) -> Option<String> {
    let gate = &code_view(gate);

    let Some(decide) = item_body(gate, "pub fn decide(") else {
        return Some("the introducer request's decision is absent".to_owned());
    };
    if !decide.contains("if !firewalled") {
        return Some(
            "the gate must still refuse every request while the router is not firewalled"
                .to_owned(),
        );
    }
    if !decide.contains("requests_refused_not_firewalled") {
        return Some("the refusal must still be counted under its own reason".to_owned());
    }
    None
}

// ---- 10. no new key and no new infrastructure -------------------------
fn check_no_new_key_and_no_infrastructure(runtime: &str, config: &str) -> Option<String> {
    let runtime = &code_view(runtime);
    let config = &code_view(config);
    for needle in [
        "ssu2.reachability",
        "ssu2.contradiction",
        "ssu2.observed_address",
    ] {
        if config.contains(needle) {
            return Some(format!(
                "the plan must add no configuration key: found {needle:?}"
            ));
        }
    }
    // The plan's own new runtime code must add no task, no timer and no
    // listener. The runtime crate as a whole legitimately owns both, so
    // the check is scoped to the code this plan added.
    for needle in [
        "pub fn note_observed_external_address(&self",
        "fn note_observed_external_address_locked(",
        "fn bound_endpoint(",
        "fn record_peer_test_outcome_locked(",
        "pub fn note_relay_firewalled(",
        "pub fn reachability_evidence(",
    ] {
        let Some(body) = item_body(runtime, needle) else {
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

// ---- the rules ------------------------------------------------------
#[test]
fn the_router_has_one_reachability_tracker() {
    let reason = check_the_router_has_one_reachability_tracker(
        &source_of(RUNTIME_MODULE),
        &source_of(PEER_RELAY_MODULE),
    );
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_peer_relay_service_owns_no_tracker() {
    let reason = check_the_peer_relay_service_owns_no_tracker(&source_of(PEER_RELAY_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_transport_keeps_the_observed_endpoint() {
    let reason =
        check_the_transport_keeps_the_observed_endpoint(&source_of(TRANSPORT_SESSION_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_transport_model_is_untouched() {
    let reason = check_the_transport_model_is_untouched(&source_of(TRANSPORT_REACHABILITY_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_runtime_compares_with_the_bound_address() {
    let reason = check_the_runtime_compares_with_the_bound_address(&source_of(RUNTIME_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn every_typed_outcome_is_recorded() {
    let reason = check_every_typed_outcome_is_recorded(&source_of(RUNTIME_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_runtime_owns_the_relay_success_recorder() {
    let reason = check_the_runtime_owns_the_relay_success_recorder(
        &source_of(RUNTIME_MODULE),
        &source_of(PEER_RELAY_MODULE),
    );
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_product_reads_the_runtime_s_tracker() {
    let reason = check_the_product_reads_the_runtime_s_tracker(
        &source_of(COMPOSITION_MODULE),
        &source_of(RUNTIME_MODULE),
    );
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_gate_stays_fail_closed() {
    let reason = check_the_gate_stays_fail_closed(&source_of(DAEMON_GATE_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn no_new_key_and_no_infrastructure() {
    let reason = check_no_new_key_and_no_infrastructure(
        &source_of(RUNTIME_MODULE),
        &source_of("crates/i2pr-daemon/src/config.rs"),
    );
    assert!(reason.is_none(), "{reason:?}");
}

// ---- the inverted probes ---------------------------------------------
#[test]
fn the_guard_reports_a_second_reachability_tracker() {
    let mut violating = source_of(PEER_RELAY_MODULE);
    violating.push_str("\n    reachability: Mutex<ReachabilityTracker>,\n");
    let reason =
        check_the_router_has_one_reachability_tracker(&source_of(RUNTIME_MODULE), &violating);
    assert!(reason.is_some(), "a second tracker must be reported");
}

#[test]
fn the_guard_reports_a_peer_relay_service_that_owns_a_tracker() {
    let mut violating = source_of(PEER_RELAY_MODULE);
    violating.push_str("\n    fn note_relay_firewalled(&self) {}\n");
    let reason = check_the_peer_relay_service_owns_no_tracker(&violating);
    assert!(reason.is_some(), "a peer-relay tracker must be reported");
}

#[test]
fn the_guard_reports_an_observation_that_discards_the_endpoint() {
    let violating = source_of(TRANSPORT_SESSION_MODULE).replace(
        "AddressObserved {\n        /// The address at which the peer sees this router.\n        observed: crate::address::Ssu2Endpoint,\n    }",
        "AddressObserved",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_SESSION_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_transport_keeps_the_observed_endpoint(&violating);
    assert!(reason.is_some(), "a discarded endpoint must be reported");
}

#[test]
fn the_guard_reports_a_changed_transport_model() {
    let mut violating = source_of(TRANSPORT_REACHABILITY_MODULE);
    violating = violating.replace("FirewalledLikely", "FirewalledPerhaps");
    assert_ne!(
        violating,
        source_of(TRANSPORT_REACHABILITY_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_transport_model_is_untouched(&violating);
    assert!(reason.is_some(), "a changed model must be reported");
}

#[test]
fn the_guard_reports_a_comparison_that_is_not_against_the_bound_address() {
    let mut violating = source_of(RUNTIME_MODULE);
    violating = violating.replace("bound_endpoint(family)", "configured_endpoint(family)");
    assert_ne!(
        violating,
        source_of(RUNTIME_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_runtime_compares_with_the_bound_address(&violating);
    assert!(
        reason.is_some(),
        "a comparison against another address must be reported"
    );
}

#[test]
fn the_guard_reports_a_comparison_that_never_contradicts() {
    let mut violating = source_of(RUNTIME_MODULE);
    violating = violating.replace(
        "outcome: PeerTestOutcomeKind::AddressMismatch",
        "outcome: PeerTestOutcomeKind::Confirmed",
    );
    assert_ne!(
        violating,
        source_of(RUNTIME_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_runtime_compares_with_the_bound_address(&violating);
    assert!(
        reason.is_some(),
        "a comparison that never contradicts must be reported"
    );
}

#[test]
fn the_guard_reports_a_recorder_that_drops_the_contradicting_outcomes() {
    let mut violating = source_of(RUNTIME_MODULE);
    violating = violating.replace(
        "outcome: PeerTestOutcomeKind::AddressMismatch,",
        "outcome: PeerTestOutcomeKind::Confirmed,",
    );
    assert_ne!(
        violating,
        source_of(RUNTIME_MODULE),
        "the mutation must change the source"
    );
    let reason = check_every_typed_outcome_is_recorded(&violating);
    assert!(
        reason.is_some(),
        "a dropped contradicting outcome must be reported"
    );
}

#[test]
fn the_guard_reports_a_second_relay_success_recorder() {
    let mut violating = source_of(PEER_RELAY_MODULE);
    violating.push_str("\n    pub fn note_relay_firewalled(&self) {}\n");
    let reason =
        check_the_runtime_owns_the_relay_success_recorder(&source_of(RUNTIME_MODULE), &violating);
    assert!(reason.is_some(), "a second recorder must be reported");
}

#[test]
fn the_guard_reports_a_gate_that_reads_another_tracker() {
    let mut violating = source_of(COMPOSITION_MODULE);
    violating = violating.replace(
        "service().snapshot().reachability",
        "peer_relay_snapshot().reachability",
    );
    assert_ne!(
        violating,
        source_of(COMPOSITION_MODULE),
        "the mutation must change the source"
    );
    let reason =
        check_the_product_reads_the_runtime_s_tracker(&violating, &source_of(RUNTIME_MODULE));
    assert!(
        reason.is_some(),
        "a gate that reads another tracker must be reported"
    );
}

#[test]
fn the_guard_reports_a_gate_that_is_not_fail_closed() {
    let mut violating = source_of(DAEMON_GATE_MODULE);
    violating = violating.replace("if !firewalled {", "if false {");
    assert_ne!(
        violating,
        source_of(DAEMON_GATE_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_gate_stays_fail_closed(&violating);
    assert!(
        reason.is_some(),
        "a gate that is not fail-closed must be reported"
    );
}

#[test]
fn the_guard_reports_a_new_configuration_key() {
    let mut violating = source_of("crates/i2pr-daemon/src/config.rs");
    // the rule reads the code view, so the mutation must be code and not
    // a comment.
    violating.push_str("\nconst _SSU2_CONTRADICTION_KEY: &str = \"ssu2.contradiction\";\n");
    let reason = check_no_new_key_and_no_infrastructure(&source_of(RUNTIME_MODULE), &violating);
    assert!(reason.is_some(), "a new configuration key must be reported");
}

#[test]
fn the_guard_reports_new_infrastructure() {
    let mut violating = source_of(RUNTIME_MODULE);
    violating = violating.replace(
        "    fn bound_endpoint(&self, family: AddressFamily) -> Option<Ssu2Endpoint> {",
        "    fn bound_endpoint(&self, family: AddressFamily) -> Option<Ssu2Endpoint> {\n        tokio::spawn(async {});",
    );
    assert_ne!(
        violating,
        source_of(RUNTIME_MODULE),
        "the mutation must change the source"
    );
    let reason = check_no_new_key_and_no_infrastructure(
        &violating,
        &source_of("crates/i2pr-daemon/src/config.rs"),
    );
    assert!(reason.is_some(), "new infrastructure must be reported");
}

// ---- the missing-item probe -----------------------------------------
#[test]
fn the_guard_reports_a_missing_named_item() {
    let empty = String::new();
    let reasons = [
        check_the_router_has_one_reachability_tracker(&empty, &empty),
        check_the_peer_relay_service_owns_no_tracker(&empty),
        check_the_transport_keeps_the_observed_endpoint(&empty),
        check_the_transport_model_is_untouched(&empty),
        check_the_runtime_compares_with_the_bound_address(&empty),
        check_every_typed_outcome_is_recorded(&empty),
        check_the_runtime_owns_the_relay_success_recorder(&empty, &empty),
        check_the_product_reads_the_runtime_s_tracker(&empty, &empty),
        check_the_gate_stays_fail_closed(&empty),
    ];
    for reason in reasons {
        assert!(
            reason.is_some(),
            "a rule whose named item is absent must be reported, not passed"
        );
    }
}
