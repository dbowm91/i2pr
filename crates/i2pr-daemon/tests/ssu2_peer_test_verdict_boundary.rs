//! Plan 528's guard -- the router's peer-test verdict must not be thrown away.
//!
//! Before this plan the tree ran a **complete** three-party SSU2 peer test
//! whose verdict carried exactly what the third party observed, and then
//! threw every verdict away except "the observation matched the router's own
//! bound address and two peers corroborated". A **disagreement** became an
//! **error**, not a **fact**. On top of that the transport applied its
//! **corroboration** floor to **contradictions**, so one contradicting fact
//! could never leave `ObservedUnconfirmed` and `Firewalled` and `Unreachable`
//! were unreachable states.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the **corroboration** floor is unchanged -- `Reachable` still needs
//!    three distinct supporting classes;
//! 2. the **contradiction** floor is separate, and follows the
//!    specification's own single-peer-test rule;
//! 3. the **firewalled verdict** is first class;
//! 4. the **neutral** verdicts move nothing;
//! 5. the specification's peer-test result table is **transcribed** and
//!    **pinned**;
//! 6. the mismatch arm is a **fact** and not an error;
//! 7. the comparison is against the router's **own bound socket**;
//! 8. the gate stays **fail-closed**;
//! 9. the plan adds no configuration key and no infrastructure;
//! 10. the transport's signal **vocabulary** and its corroboration classes
//!     are untouched.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, and every rule
//! **fails closed** when the item it names is absent.

#![forbid(unsafe_code)]

const TRANSPORT_REACHABILITY_MODULE: &str = "crates/i2pr-transport/src/reachability.rs";
const TRANSPORT_PEER_TEST_MODULE: &str = "crates/i2pr-transport-ssu2/src/peer_test.rs";
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const CONTROLLED_MODULE: &str = "crates/i2pr-runtime/src/ssu2_controlled_peer_test.rs";
const DAEMON_GATE_MODULE: &str = "crates/i2pr-daemon/src/introducer_request.rs";
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

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

// ---- 1. the corroboration floor is unchanged -------------------------
fn check_the_corroboration_floor_is_unchanged(reachability: &str) -> Option<String> {
    let reachability = &code_view(reachability);

    if !reachability.contains("pub const MIN_CORROBORATION_FLOOR: usize = 2;") {
        return Some(
            "the corroboration floor must stay at two distinct supporting classes: a router is              reachable only above it"
                .to_owned(),
        );
    }
    let Some(defaults) = item_body(reachability, "impl Default for ReachabilityPolicy") else {
        return Some("the policy's default is absent".to_owned());
    };
    if !defaults.contains("min_corroboration: 2") {
        return Some("the default corroboration floor must stay at two".to_owned());
    }
    let Some(recompute) = item_body(reachability, "fn recompute(&mut self, now: Duration)") else {
        return Some("the reachability recompute is absent".to_owned());
    };
    if !recompute.contains("self.policy.min_corroboration") {
        return Some("the reachable threshold must still read the corroboration floor".to_owned());
    }
    if !recompute.contains("ReachabilityState::Reachable") {
        return Some("the reachable state must still exist".to_owned());
    }
    None
}

// ---- 2. the contradiction floor is separate ---------------------------
fn check_the_contradiction_floor_is_separate(reachability: &str) -> Option<String> {
    let reachability = &code_view(reachability);

    if !reachability.contains("pub const MIN_CONTRADICTION_FLOOR: usize = 1;") {
        return Some(
            "the contradiction floor must be one distinct contradicting class: that is the              specification's own single-peer-test rule"
                .to_owned(),
        );
    }
    let Some(policy) = item_text(reachability, "pub struct ReachabilityPolicy") else {
        return Some("the reachability policy is absent".to_owned());
    };
    for needle in ["min_corroboration: usize", "min_contradiction: usize"] {
        if !policy.contains(needle) {
            return Some(format!(
                "the policy must carry both floors side by side: {needle:?} is absent"
            ));
        }
    }
    let Some(accessor) = item_body(
        reachability,
        "pub const fn min_contradiction(self) -> usize",
    ) else {
        return Some("the contradiction-floor accessor is absent".to_owned());
    };
    if !accessor.contains("self.min_contradiction") {
        return Some(
            "the contradiction-floor accessor must read the policy's own field".to_owned(),
        );
    }
    let Some(validate) = item_body(
        reachability,
        "pub fn validate(self) -> Result<Self, ReachabilityPolicyError>",
    ) else {
        return Some("the policy's validation is absent".to_owned());
    };
    if !validate.contains("MIN_CORROBORATION_FLOOR") {
        return Some("the corroboration floor must still be validated".to_owned());
    }
    if !validate.contains("MIN_CONTRADICTION_FLOOR") {
        return Some("the contradiction floor must be validated on its own".to_owned());
    }
    let Some(recompute) = item_body(reachability, "fn recompute(&mut self, now: Duration)") else {
        return Some("the reachability recompute is absent".to_owned());
    };
    if !recompute.contains("self.policy.min_contradiction") {
        return Some(
            "the contradiction threshold must read the contradiction floor, not the              corroboration floor"
                .to_owned(),
        );
    }
    None
}

// ---- 3. the firewalled verdict is first class -------------------------
fn check_the_firewalled_verdict_is_first_class(reachability: &str) -> Option<String> {
    let reachability = &code_view(reachability);

    let Some(verdict) = item_body(
        reachability,
        "pub const fn is_firewalled_verdict(self) -> bool",
    ) else {
        return Some("the firewalled-verdict classifier is absent".to_owned());
    };
    for needle in [
        "PeerTestOutcomeKind::FirewalledLikely",
        "Self::RelayFirewalledSignal",
    ] {
        if !verdict.contains(needle) {
            return Some(format!(
                "the firewalled verdict must be first class: {needle:?} is absent"
            ));
        }
    }
    let Some(recompute) = item_body(reachability, "fn recompute(&mut self, now: Duration)") else {
        return Some("the reachability recompute is absent".to_owned());
    };
    if !recompute.contains("is_firewalled_verdict()") {
        return Some("the reachability must consult the firewalled verdict".to_owned());
    }
    if !recompute.contains("ReachabilityState::Firewalled") {
        return Some("a firewalled verdict must put the router in the firewalled state".to_owned());
    }
    None
}

// ---- 4. the neutral verdicts move nothing -----------------------------
fn check_the_neutral_verdicts_move_nothing(reachability: &str) -> Option<String> {
    let reachability = &code_view(reachability);

    let Some(contradicts) = item_body(
        reachability,
        "const fn contradicts_reachability(self) -> bool",
    ) else {
        return Some("the contradicting classifier is absent".to_owned());
    };
    let Some(supports) = item_body(reachability, "const fn supports_reachability(self) -> bool")
    else {
        return Some("the supporting classifier is absent".to_owned());
    };
    for (name, body) in [("contradicting", &contradicts), ("supporting", &supports)] {
        for neutral in ["Inconclusive", "Rejected"] {
            if body.contains(neutral) {
                return Some(format!(
                    "the {name} classifier must leave the neutral verdict {neutral:?} alone: it              moves nothing"
                ));
            }
        }
    }
    None
}

// ---- 5. the specification's result table is transcribed and pinned -----
fn check_the_result_table_is_transcribed_and_pinned(peer_test: &str) -> Option<String> {
    let code = &code_view(peer_test);

    for needle in [
        "pub enum PeerTestResult",
        "pub struct PeerTestResultRow",
        "pub const PEER_TEST_RESULT_TABLE: [PeerTestResultRow; 8]",
        "pub const fn peer_test_result",
    ] {
        if !code.contains(needle) {
            return Some(format!(
                "the specification's result table must be transcribed: {needle:?} is absent"
            ));
        }
    }
    for variant in ["Unknown", "Ok", "Firewalled", "Ambiguous", "NotApplicable"] {
        if !code.contains(&format!("PeerTestResult::{variant}")) {
            return Some(format!(
                "the transcribed table must carry the {variant:?} result"
            ));
        }
    }
    // The pin lives in the crate's own test module, which the code view
    // strips, so the pin is checked against the raw source.
    for pin in [
        "fn the_specification_result_table_is_transcribed_and_pinned",
        "fn the_peer_test_result_lookup_is_total",
    ] {
        if !peer_test.contains(pin) {
            return Some(format!(
                "the transcribed table must be pinned: {pin:?} is absent"
            ));
        }
    }
    None
}

// ---- 6. the mismatch arm is a fact and not an error -------------------
fn check_the_mismatch_arm_is_a_fact(controlled: &str) -> Option<String> {
    let controlled = &code_view(controlled);

    let Some(outcome) = item_text(controlled, "pub enum ControlledPeerTestOutcome") else {
        return Some("the controlled outcome enum is absent".to_owned());
    };
    for variant in [
        "Confirmed",
        "ObservedElsewhere",
        "ObservationsDisagree",
        "FirewalledLikely",
        "Inconclusive",
        "Rejected",
    ] {
        if !outcome.contains(variant) {
            return Some(format!(
                "the controlled outcome must carry the {variant:?} verdict: a verdict the tree              threw away is a verdict it cannot state"
            ));
        }
    }
    let Some(driver) = item_body(controlled, "async fn drive_exchange(") else {
        return Some("the controlled driver is absent".to_owned());
    };
    if driver.contains("Err(ControlledPeerTestError::ExchangeNotConfirmed)") {
        return Some(
            "the driver's error arm must keep only the genuinely failed cases: a verdict is              not a failure"
                .to_owned(),
        );
    }
    for needle in [
        "PeerTestOutcome::AddressMismatch",
        "PeerTestOutcome::FirewalledLikely",
        "PeerTestOutcome::Inconclusive",
        "PeerTestOutcome::Rejected",
    ] {
        if !driver.contains(needle) {
            return Some(format!(
                "the driver must return the {needle:?} verdict as a fact"
            ));
        }
    }
    None
}

// ---- 7. the comparison is against the bound socket --------------------
fn check_the_comparison_is_against_the_bound_socket(runtime: &str) -> Option<String> {
    let runtime = &code_view(runtime);

    let Some(comparison) = item_body(runtime, "fn note_observed_external_address_locked(") else {
        return Some("the comparison is absent".to_owned());
    };
    if !comparison.contains("bound_endpoint(") {
        return Some(
            "the comparison must be made against the router's own bound address".to_owned(),
        );
    }
    if !comparison.contains("PeerTestOutcomeKind::FirewalledLikely") {
        return Some(
            "a disagreement is the router's own firewalled fact and must be recorded as              such"
                .to_owned(),
        );
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

// ---- 8. the gate stays fail-closed ------------------------------------
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

// ---- 9. no new key and no new infrastructure --------------------------
fn check_no_new_key_and_no_infrastructure(
    runtime: &str,
    peer_test: &str,
    config: &str,
) -> Option<String> {
    let runtime = &code_view(runtime);
    let peer_test = &code_view(peer_test);
    let config = &code_view(config);
    for needle in [
        "ssu2.contradiction",
        "ssu2.min_contradiction",
        "ssu2.peer_test_verdict",
        "ssu2.firewalled",
    ] {
        if config.contains(needle) {
            return Some(format!(
                "the plan must add no configuration key: found {needle:?}"
            ));
        }
    }
    // The plan's own new code must add no task, no timer and no listener.
    // The runtime crate as a whole legitimately owns both, so the check is
    // scoped to the code this plan added.
    for (code, needle) in [
        (runtime, "pub fn note_peer_test_verdict("),
        (runtime, "pub fn note_peer_test_outcome("),
        (runtime, "fn record_peer_test_outcome_locked("),
        (peer_test, "pub const fn peer_test_result"),
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

// ---- 10. the transport's vocabulary is untouched ----------------------
fn check_the_transport_vocabulary_is_untouched(reachability: &str) -> Option<String> {
    let reachability = &code_view(reachability);

    let Some(kind) = item_text(reachability, "pub enum PeerTestOutcomeKind") else {
        return Some("the peer-test outcome vocabulary is absent".to_owned());
    };
    for variant in [
        "Confirmed",
        "AddressMismatch",
        "FirewalledLikely",
        "Inconclusive",
        "Rejected",
    ] {
        if !kind.contains(variant) {
            return Some(format!(
                "the peer-test outcome vocabulary must keep the {variant:?} kind"
            ));
        }
    }
    for needle in [
        "fn supporting_classes(&self) -> usize",
        "fn contradicting_classes(&self) -> usize",
    ] {
        if !reachability.contains(needle) {
            return Some(format!(
                "the transport's corroboration classes must not change: {needle:?} is absent"
            ));
        }
    }
    None
}

// ---- the rules ------------------------------------------------------
#[test]
fn the_corroboration_floor_is_unchanged() {
    let reason =
        check_the_corroboration_floor_is_unchanged(&source_of(TRANSPORT_REACHABILITY_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_contradiction_floor_is_separate() {
    let reason =
        check_the_contradiction_floor_is_separate(&source_of(TRANSPORT_REACHABILITY_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_firewalled_verdict_is_first_class() {
    let reason =
        check_the_firewalled_verdict_is_first_class(&source_of(TRANSPORT_REACHABILITY_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_neutral_verdicts_move_nothing() {
    let reason = check_the_neutral_verdicts_move_nothing(&source_of(TRANSPORT_REACHABILITY_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_result_table_is_transcribed_and_pinned() {
    let reason =
        check_the_result_table_is_transcribed_and_pinned(&source_of(TRANSPORT_PEER_TEST_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_mismatch_arm_is_a_fact() {
    let reason = check_the_mismatch_arm_is_a_fact(&source_of(CONTROLLED_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_comparison_is_against_the_bound_socket() {
    let reason = check_the_comparison_is_against_the_bound_socket(&source_of(RUNTIME_MODULE));
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
        &source_of(TRANSPORT_PEER_TEST_MODULE),
        &source_of(CONFIG_MODULE),
    );
    assert!(reason.is_none(), "{reason:?}");
}

#[test]
fn the_transport_vocabulary_is_untouched() {
    let reason =
        check_the_transport_vocabulary_is_untouched(&source_of(TRANSPORT_REACHABILITY_MODULE));
    assert!(reason.is_none(), "{reason:?}");
}

// ---- the inverted probes ---------------------------------------------
#[test]
fn the_guard_reports_a_weakened_corroboration_floor() {
    let mut violating = source_of(TRANSPORT_REACHABILITY_MODULE);
    violating = violating.replace(
        "pub const MIN_CORROBORATION_FLOOR: usize = 2;",
        "pub const MIN_CORROBORATION_FLOOR: usize = 1;",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_REACHABILITY_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_corroboration_floor_is_unchanged(&violating);
    assert!(
        reason.is_some(),
        "a weakened corroboration floor must be reported"
    );
}

#[test]
fn the_guard_reports_a_merged_contradiction_floor() {
    let mut violating = source_of(TRANSPORT_REACHABILITY_MODULE);
    violating = violating.replace(
        "self.policy.min_contradiction",
        "self.policy.min_corroboration",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_REACHABILITY_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_contradiction_floor_is_separate(&violating);
    assert!(
        reason.is_some(),
        "a contradiction floor merged into the corroboration floor must be reported"
    );
}

#[test]
fn the_guard_reports_a_firewalled_verdict_that_is_not_first_class() {
    let mut violating = source_of(TRANSPORT_REACHABILITY_MODULE);
    violating = violating.replace(
        "PeerTestOutcomeKind::FirewalledLikely",
        "PeerTestOutcomeKind::Confirmed",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_REACHABILITY_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_firewalled_verdict_is_first_class(&violating);
    assert!(
        reason.is_some(),
        "a firewalled verdict that is not first class must be reported"
    );
}

#[test]
fn the_guard_reports_a_neutral_verdict_that_moves_something() {
    let mut violating = source_of(TRANSPORT_REACHABILITY_MODULE);
    violating = violating.replace(
        "PeerTestOutcomeKind::AddressMismatch | PeerTestOutcomeKind::FirewalledLikely",
        "PeerTestOutcomeKind::AddressMismatch | PeerTestOutcomeKind::Inconclusive",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_REACHABILITY_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_neutral_verdicts_move_nothing(&violating);
    assert!(
        reason.is_some(),
        "a neutral verdict that moves the state must be reported"
    );
}

#[test]
fn the_guard_reports_a_result_table_that_loses_a_row() {
    let mut violating = source_of(TRANSPORT_PEER_TEST_MODULE);
    violating = violating.replace(
        "pub const PEER_TEST_RESULT_TABLE: [PeerTestResultRow; 8]",
        "pub const PEER_TEST_RESULT_TABLE: [PeerTestResultRow; 7]",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_PEER_TEST_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_result_table_is_transcribed_and_pinned(&violating);
    assert!(
        reason.is_some(),
        "a result table that loses a row must be reported"
    );
}

#[test]
fn the_guard_reports_an_unpinned_result_table() {
    let mut violating = source_of(TRANSPORT_PEER_TEST_MODULE);
    violating = violating.replace(
        "fn the_specification_result_table_is_transcribed_and_pinned",
        "fn the_specification_result_table_is_transcribed",
    );
    assert_ne!(
        violating,
        source_of(TRANSPORT_PEER_TEST_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_result_table_is_transcribed_and_pinned(&violating);
    assert!(
        reason.is_some(),
        "an unpinned result table must be reported"
    );
}

#[test]
fn the_guard_reports_a_mismatch_arm_that_is_an_error() {
    let mut violating = source_of(CONTROLLED_MODULE);
    violating = violating.replace(
        "Ok(ControlledPeerTestOutcome::Rejected { family })",
        "Err(ControlledPeerTestError::ExchangeNotConfirmed)",
    );
    assert_ne!(
        violating,
        source_of(CONTROLLED_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_mismatch_arm_is_a_fact(&violating);
    assert!(
        reason.is_some(),
        "a verdict reported as an error must be reported"
    );
}

#[test]
fn the_guard_reports_a_comparison_against_another_address() {
    let mut violating = source_of(RUNTIME_MODULE);
    violating = violating.replace("bound_endpoint(family)", "configured_endpoint(family)");
    assert_ne!(
        violating,
        source_of(RUNTIME_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_comparison_is_against_the_bound_socket(&violating);
    assert!(
        reason.is_some(),
        "a comparison against another address must be reported"
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
    let mut violating = source_of(CONFIG_MODULE);
    // the rule reads the code view, so the mutation must be code and not
    // a comment.
    violating.push_str("\nconst _SSU2_CONTRADICTION_KEY: &str = \"ssu2.contradiction\";\n");
    let reason = check_no_new_key_and_no_infrastructure(
        &source_of(RUNTIME_MODULE),
        &source_of(TRANSPORT_PEER_TEST_MODULE),
        &violating,
    );
    assert!(reason.is_some(), "a new configuration key must be reported");
}

#[test]
fn the_guard_reports_new_infrastructure() {
    let mut violating = source_of(RUNTIME_MODULE);
    // The spawn goes **inside** the item's body, so the rule reads it as
    // the item's own code and not as text that sits outside it.
    let anchor = "pub fn note_peer_test_verdict(";
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
    let reason = check_no_new_key_and_no_infrastructure(
        &violating,
        &source_of(TRANSPORT_PEER_TEST_MODULE),
        &source_of(CONFIG_MODULE),
    );
    assert!(reason.is_some(), "new infrastructure must be reported");
}

#[test]
fn the_guard_reports_a_changed_peer_test_vocabulary() {
    let mut violating = source_of(TRANSPORT_REACHABILITY_MODULE);
    violating = violating.replace("FirewalledLikely", "FirewalledPerhaps");
    assert_ne!(
        violating,
        source_of(TRANSPORT_REACHABILITY_MODULE),
        "the mutation must change the source"
    );
    let reason = check_the_transport_vocabulary_is_untouched(&violating);
    assert!(
        reason.is_some(),
        "a changed peer-test vocabulary must be reported"
    );
}

// ---- the missing-item probe -----------------------------------------
#[test]
fn the_guard_reports_a_missing_named_item() {
    let empty = String::new();
    let reasons = [
        check_the_corroboration_floor_is_unchanged(&empty),
        check_the_contradiction_floor_is_separate(&empty),
        check_the_firewalled_verdict_is_first_class(&empty),
        check_the_neutral_verdicts_move_nothing(&empty),
        check_the_result_table_is_transcribed_and_pinned(&empty),
        check_the_mismatch_arm_is_a_fact(&empty),
        check_the_comparison_is_against_the_bound_socket(&empty),
        check_the_gate_stays_fail_closed(&empty),
        check_no_new_key_and_no_infrastructure(&empty, &empty, &empty),
        check_the_transport_vocabulary_is_untouched(&empty),
    ];
    for reason in reasons {
        assert!(
            reason.is_some(),
            "a rule whose named item is absent must be reported, not passed"
        );
    }
}
