//! Plan 504 static boundary guard for the live tunnel-liveness composition.
//!
//! Plan 185 delivered a tunnel-liveness scheduler
//! (`crates/i2pr-daemon/src/tunnel_liveness.rs`) that the production daemon
//! **never constructed**. It was the only public module in the daemon crate
//! with zero production users, so every behavioural test of the scheduler
//! passed against a scheduler no router could ever own. Plan 504 installs it;
//! this guard is the control that keeps it installed.
//!
//! The guard is deliberately **structural**. A whole-file token search would
//! not have caught Plan 185's gap — the module existed, the names existed,
//! the tokens existed — so a rule that greps the file for a name proves
//! nothing about the *composition*. Every rule below asserts that a
//! **named function's own body** contains the required **call**, matched with
//! its open parenthesis, so a token that appears in a sibling function, in a
//! comment, or in a `#[cfg(test)]` block does not satisfy the rule.
//!
//! The rules, and what each one would let through if it were absent:
//!
//! 1. **The live composition constructs the scheduler.** The bodies of
//!    `ServiceProduct::start` and `ServiceProduct::start_over_existing_daemon`
//!    must contain `TunnelLivenessScheduler::new(`. Without it, the daemon
//!    ships a scheduler module with zero production owners again — exactly
//!    Plan 185's gap, reintroduced.
//! 2. **The live inbound path routes through the liveness seam.** Each live
//!    inbound-routing function must contain a liveness-seam call. Without it,
//!    an inbound `DeliveryStatus` never reaches the scheduler, so the
//!    correlation is dead code and every pair stays unproven forever.
//! 3. **The live inbound path does not bypass the liveness seam.** The
//!    production code view of `service_product.rs` must not contain the bare
//!    `.route_inbound_i2np(` call — the call Plan 504 replaced. Its
//!    reappearance means the seam was bypassed again.
//! 4. **The live tick drives the scheduler.** The tick must call
//!    `drive_tunnel_liveness(`, and the driver must call the scheduler's
//!    `drive()`. Without it, the scheduler owns pairs but never emits a probe,
//!    so liveness is never observed.
//! 5. **The probe is emitted through the canonical composition.** The body of
//!    `compose_liveness_probe` must call `compose_outbound_build_deliveries(`.
//!    Without it, the probe takes a private path that is not the one the
//!    build reply uses, and the probe and the reply can disagree about the
//!    cell layout.
//! 6. **The probe has no private parallel cell pipeline.** The coordinator
//!    module must compose outbound cells at exactly one call site. A second
//!    one is a private pipeline that can drift from the canonical one.
//! 7. **The unproven-pair gate exists.** The production code view of
//!    `tunnel_liveness.rs` must contain the gate.
//! 8. **The unproven-pair gate guards the unhealthy verdict.** The body of
//!    `fn drive` — the function that decides to emit `MarkUnhealthy` — must
//!    contain the gate. Without it, a pair that has **never** answered is torn
//!    down on the strength of a signal the router cannot distinguish from its
//!    own negation: Plan 504 invariant 9, the safety property.
//! 9. **The `DeliveryStatus` classification carries the acknowledged id.**
//!    Without it, the dispatcher discards the only authenticated link
//!    between a probe and the answer it caused.
//! 10. **The scheduler matches on the acknowledged id.** Without it, the
//!     scheduler credits "any DeliveryStatus" to "the only pending test",
//!     which is not a correlation.
//! 11. **The seam reads the acknowledged id out of the outcome.** Without it,
//!     the dispatcher carries the id and the scheduler never receives it.
//!
//! Rules 1, 2, 4, 5, 8, 9, 10 and 11 are **structural**: they assert that a
//! named function's own body contains the required call. Rules 3, 6 and 7 are
//! **textual**: they assert that a token does or does not appear in the
//! production code view of a module. The guard does **not** claim that the
//! live behaviour is correct — the lane in
//! `crates/i2pr-daemon/tests/tunnel_liveness_live.rs` proves that; the guard
//! only proves that the wiring is present in the place it must be present.
//!
//! The guard is **fail-closed**: comments are stripped before matching, every
//! `#[cfg(test)]` block is excluded, an absent function yields `None` so
//! every caller fails, and every rule carries its own **inverted probe** — a
//! synthetic violating source must be reported. If a rule ever stops
//! detecting its violation, its probe fails.
//!
//! The helpers below are duplicated from
//! `crates/i2pr-daemon/tests/version_floor_boundary.rs` (Plan 503's guard),
//! which is the best structural guard in the repository. Each integration test
//! is its own crate, so the helpers cannot be shared by import; the
//! duplication is a consequence of Rust's integration-test model, not a
//! choice.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The live inbound-routing functions of `service_product.rs`.
///
/// Every one of them must route through the liveness seam: a `DeliveryStatus`
/// that arrives on one of these paths and never reaches the scheduler is a
/// response the router cannot attribute to the probe that caused it.
const LIVE_INBOUND_ROUTING_FUNCTIONS: &[&str] = &[
    "process_inbound",
    "await_destination_build_install",
    "provision_all_service_router_material",
];

/// The two spellings of the liveness seam.
///
/// `route_inbound_with_liveness_classified(` is the same seam as
/// `route_inbound_with_liveness(`, not a second one: the live pump decodes
/// the inbound message once and classifies it once, and the classified variant
/// correlates that *same* outcome instead of running a second decoder. The
/// two are **not** interchangeable textually — the classified spelling does
/// not contain the plain spelling's trailing parenthesis — so the rule
/// accepts either call, each matched with its own open parenthesis.
const LIVENESS_SEAM_CALLS: &[&str] = &[
    "route_inbound_with_liveness(",
    "route_inbound_with_liveness_classified(",
];

/// The cell-composition entry point of the canonical outbound role.
///
/// `compose_outbound_build_deliveries` is the only function in the
/// coordinator module that composes outbound cells, so it is the only
/// `.forward_cells(` call site. A second one is a private parallel pipeline.
const CELL_COMPOSITION_CALL: &str = ".forward_cells(";

/// The unproven-pair gate, exactly as the landed code spells it.
///
/// The gate is `pair.proven && pair.consecutive_failures >= threshold`. This
/// token is the *whole* gate: weakening it to
/// `pair.consecutive_failures >= threshold` — tearing down a pair that has
/// never answered — removes this token and trips rule 8.
const UNPROVEN_PAIR_GATE: &str = "pair.proven &&";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

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
            let is_char_literal = match next {
                Some('\\') => true,
                Some(_) => bytes.get(index + 2) == Some(&'\''),
                None => false,
            };
            if is_char_literal {
                in_char = true;
            }
            out.push(current);
            index += 1;
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
}

/// Removes every `#[cfg(test)]` block from the code view.
///
/// The crates' own unit tests may use whatever they need; the invariants are
/// about the production surface.
fn strip_cfg_test_blocks(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(position) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..position]);
        let after_attribute = &rest[position + "#[cfg(test)]".len()..];
        let Some(open) = after_attribute.find('{') else {
            out.push_str(after_attribute);
            rest = "";
            break;
        };
        let mut depth = 0usize;
        let mut cursor = open;
        let bytes = after_attribute.as_bytes();
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        cursor += 1;
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        for _ in after_attribute[..cursor].matches('\n') {
            out.push('\n');
        }
        rest = &after_attribute[cursor..];
    }
    out.push_str(rest);
    out
}

/// The production code view of a source text.
fn code_view(source: &str) -> String {
    strip_cfg_test_blocks(&strip_comments(source))
}

/// Returns the byte range of the brace-balanced block that opens at or after
/// `start`, as `(open, close)`; `close` is one past the closing brace.
///
/// The scan is **string-aware**: a brace inside a string literal or inside a
/// character literal does not change the nesting depth, so a body that holds
/// `"}"` does not end the block early. A lifetime (`'a`) is not a character
/// literal and is scanned as ordinary text.
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

/// Returns the offset just past the character literal that starts at `start`,
/// or `None` when the quote opens a lifetime instead.
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

/// Returns the body of the function whose **definition** is anchored at
/// `fn <name>`.
///
/// The anchor is the definition and never a call site: the needle carries the
/// `fn` keyword, and the character that follows it must not continue an
/// identifier, so `fn candidate_hashes` never matches
/// `fn version_gated_candidate_hashes`.
///
/// The body is the text between the function's own braces. A function that is
/// not present yields `None`, so every caller fails closed.
fn function_body<'a>(code: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("fn {name}");
    let mut offset = 0usize;
    while let Some(position) = code[offset..].find(needle.as_str()) {
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
        let (open, close) = brace_balanced_block(code, after)?;
        return Some(&code[open + 1..close - 1]);
    }
    None
}

/// Rule 1, 2, 4, 5, 8, 9, 10, 11 (structural): the body of `fn <name>` must
/// contain `token`.
///
/// The token is matched with its open parenthesis, so the rule asserts a
/// **call** and not a bare identifier: a dead helper, a type name, or an
/// unreachable branch does not satisfy it.
fn check_body_contains(source: &str, name: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    check_body_contains_in(&code, name, token)
}

/// The code-view form of [`check_body_contains`].
fn check_body_contains_in(code: &str, name: &str, token: &str) -> Option<String> {
    let Some(body) = function_body(code, name) else {
        return Some(format!("`fn {name}` must exist in the production surface"));
    };
    if body.contains(token) {
        None
    } else {
        Some(format!("`fn {name}` must contain `{token}`"))
    }
}

/// Rule 2 (structural): the body of `fn <name>` must contain at least one of
/// the supplied call tokens.
///
/// The rule accepts either spelling of the one liveness seam, because the
/// classified spelling is the same seam and not a second path.
fn check_body_contains_any(source: &str, name: &str, tokens: &[&str]) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, name) else {
        return Some(format!("`fn {name}` must exist in the production surface"));
    };
    if tokens.iter().any(|token| body.contains(token)) {
        None
    } else {
        Some(format!(
            "`fn {name}` must contain one of {}",
            tokens.join(", ")
        ))
    }
}

/// Rule 3, 6, 7 (textual): the production code view must not contain `token`.
fn check_code_avoids(source: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    if code.contains(token) {
        Some(format!(
            "the production code view must not contain `{token}`"
        ))
    } else {
        None
    }
}

/// Rule 7 (textual): the production code view must contain `token`.
fn check_code_contains(source: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    if code.contains(token) {
        None
    } else {
        Some(format!("the production code view must contain `{token}`"))
    }
}

/// Rule 6 (textual): the production code view must contain `token` exactly
/// once.
///
/// A second occurrence is a second cell-composition pipeline, which is the
/// private parallel path the rule forbids.
fn check_code_occurs_once(source: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let occurrences = code.matches(token).count();
    if occurrences == 1 {
        None
    } else {
        Some(format!(
            "the production code view must contain `{token}` exactly once, found {occurrences}"
        ))
    }
}

/// Rule 1: the live composition constructs the scheduler.
///
/// `ServiceProduct::start` and `ServiceProduct::start_over_existing_daemon`
/// are the two entry points that build a product, and both must own a
/// scheduler. A scheduler that only one of them constructs is a scheduler
/// the other composition never has.
#[test]
fn the_live_composition_constructs_the_liveness_scheduler() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    for function in ["start", "start_over_existing_daemon"] {
        assert_eq!(
            check_body_contains(&source, function, "TunnelLivenessScheduler::new("),
            None,
            "`fn {function}` must construct the liveness scheduler"
        );
    }
}

/// Rule 2: the live inbound path routes through the liveness seam.
#[test]
fn the_live_inbound_path_routes_through_the_liveness_seam() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    for function in LIVE_INBOUND_ROUTING_FUNCTIONS {
        assert_eq!(
            check_body_contains_any(&source, function, LIVENESS_SEAM_CALLS),
            None,
            "`fn {function}` must route inbound messages through the liveness seam"
        );
    }
}

/// Rule 3: the live inbound path does not bypass the liveness seam.
///
/// The discriminator is the **leading dot**. The bare call reads
/// `.route_inbound_i2np(`; the liveness helper is a free function, so the
/// text `route_inbound_with_liveness(` can never match `.route_inbound_i2np(`.
#[test]
fn the_live_inbound_path_does_not_bypass_the_liveness_seam() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_code_avoids(&source, ".route_inbound_i2np("),
        None,
        "the live inbound path must not call the bare `.route_inbound_i2np(`"
    );
}

/// Rule 4: the live tick drives the scheduler.
#[test]
fn the_live_tick_drives_the_scheduler() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "advance_destination_pools_at",
            "drive_tunnel_liveness("
        ),
        None,
        "the live tick must call the liveness driver"
    );
    assert_eq!(
        check_body_contains(&source, "drive_tunnel_liveness", "drive()"),
        None,
        "the liveness driver must call the scheduler's `drive()`"
    );
}

/// Rule 5: the probe is emitted through the canonical composition.
#[test]
fn the_probe_is_emitted_through_the_canonical_composition() {
    let source = source_of("crates/i2pr-daemon/src/exploratory_build.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "compose_liveness_probe",
            "compose_outbound_build_deliveries("
        ),
        None,
        "the liveness probe must be composed by the canonical outbound path"
    );
}

/// Rule 6: the probe has no private parallel cell pipeline.
#[test]
fn the_probe_has_no_private_parallel_cell_pipeline() {
    let source = source_of("crates/i2pr-daemon/src/exploratory_build.rs");
    assert_eq!(
        check_code_occurs_once(&source, CELL_COMPOSITION_CALL),
        None,
        "outbound cells must be composed at exactly one call site"
    );
}

/// Rule 7: the unproven-pair gate exists.
#[test]
fn the_unproven_pair_gate_exists() {
    let source = source_of("crates/i2pr-daemon/src/tunnel_liveness.rs");
    assert_eq!(
        check_code_contains(&source, UNPROVEN_PAIR_GATE),
        None,
        "the scheduler must keep the unproven-pair gate"
    );
}

/// Rule 8: the unproven-pair gate guards the unhealthy verdict.
///
/// `fn drive` is the function that decides to emit `MarkUnhealthy`. The gate
/// must be *inside* that body: a gate elsewhere in the module would not
/// guard the verdict.
#[test]
fn the_unproven_pair_gate_guards_the_unhealthy_verdict() {
    let source = source_of("crates/i2pr-daemon/src/tunnel_liveness.rs");
    assert_eq!(
        check_body_contains(&source, "drive", UNPROVEN_PAIR_GATE),
        None,
        "`fn drive` must guard `MarkUnhealthy` with the unproven-pair gate"
    );
}

/// Rule 9: the `DeliveryStatus` classification carries the acknowledged id.
#[test]
fn the_delivery_status_classification_carries_the_acknowledged_id() {
    let source = source_of("crates/i2pr-daemon/src/router_i2np.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "classify_message",
            "acknowledged_message_id: Some(status.message_id)"
        ),
        None,
        "the `DeliveryStatus` classification must carry the acknowledged message id"
    );
}

/// Rule 10: the scheduler matches on the acknowledged id.
#[test]
fn the_scheduler_matches_on_the_acknowledged_id() {
    let source = source_of("crates/i2pr-daemon/src/tunnel_liveness.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "record_authenticated_response",
            "test.probe_message_id == acknowledged_message_id"
        ),
        None,
        "the scheduler must match the acknowledged id against the pending probe"
    );
}

/// Rule 11: the seam reads the acknowledged id out of the dispatcher outcome.
#[test]
fn the_liveness_seam_reads_the_acknowledged_id_from_the_outcome() {
    let source = source_of("crates/i2pr-daemon/src/tunnel_liveness.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "record_authenticated_outcome",
            "outcome.acknowledged_message_id()?"
        ),
        None,
        "the liveness seam must read the acknowledged id out of the outcome"
    );
}

/// The inverted probe for rule 1: a product that never constructs the
/// scheduler must be reported.
#[test]
fn the_guard_reports_a_composition_without_the_scheduler() {
    let violating = "\
pub async fn start(spec: ServiceProductSpec) -> Result<Self, ServiceProductError> {
    let product = build_product(spec)?;
    Ok(product)
}
";
    assert!(
        check_body_contains(violating, "start", "TunnelLivenessScheduler::new(").is_some(),
        "a composition that never constructs the scheduler must be reported"
    );
}

/// The inverted probe for rule 2: an inbound path that bypasses the seam
/// must be reported.
#[test]
fn the_guard_reports_an_inbound_path_without_the_seam() {
    let violating = "\
pub(crate) async fn process_inbound(&mut self, inbound: &Ssu2InboundI2np, now_ms: u64) {
    let routed = self.inner.coordinator.route_inbound_i2np(inbound, now_ms)?;
    let _ = routed;
}
";
    assert!(
        check_body_contains_any(violating, "process_inbound", LIVENESS_SEAM_CALLS).is_some(),
        "an inbound path that bypasses the liveness seam must be reported"
    );
}

/// The inverted probe for rule 3: the bare coordinator call must be reported.
#[test]
fn the_guard_reports_the_bare_inbound_call() {
    let violating = "\
fn route(inbound: &Ssu2InboundI2np, now_ms: u64) {
    let routed = coordinator.route_inbound_i2np(inbound, now_ms);
    let _ = routed;
}
";
    assert!(
        check_code_avoids(violating, ".route_inbound_i2np(").is_some(),
        "the bare `.route_inbound_i2np(` call must be reported"
    );
}

/// The inverted probe for rule 4: a tick that never drives the scheduler must
/// be reported.
#[test]
fn the_guard_reports_a_tick_that_does_not_drive_the_scheduler() {
    let violating_tick = "\
async fn advance_destination_pools_at(&mut self, now_ms: u64) {
    self.advance_pools(now_ms);
}
";
    assert!(
        check_body_contains(
            violating_tick,
            "advance_destination_pools_at",
            "drive_tunnel_liveness("
        )
        .is_some(),
        "a tick that never drives the scheduler must be reported"
    );

    let violating_driver = "\
async fn drive_tunnel_liveness(&mut self, now_ms: u64) {
    self.sync_liveness_pairs();
}
";
    assert!(
        check_body_contains(violating_driver, "drive_tunnel_liveness", "drive()").is_some(),
        "a driver that never calls the scheduler must be reported"
    );
}

/// The inverted probe for rule 5: a probe composed by a private pipeline must
/// be reported.
#[test]
fn the_guard_reports_a_probe_outside_the_canonical_composition() {
    let violating = "\
pub fn compose_liveness_probe(&mut self, outbound_slot: TunnelSlot, probe_message_id: u32) {
    let deliveries = self.private_probe_cells(outbound_slot, probe_message_id);
    let _ = deliveries;
}
";
    assert!(
        check_body_contains(
            violating,
            "compose_liveness_probe",
            "compose_outbound_build_deliveries("
        )
        .is_some(),
        "a probe composed outside the canonical path must be reported"
    );
}

/// The inverted probe for rule 6: a second cell-composition pipeline must be
/// reported.
#[test]
fn the_guard_reports_a_private_parallel_cell_pipeline() {
    let violating = "\
fn compose_outbound_build_deliveries(role: &OutboundGatewayRole) -> Vec<Cell> {
    let cells = role.forward_cells(&header, wire, rng, now_ms);
    cells
}

fn compose_private_probe_cells(role: &OutboundGatewayRole) -> Vec<Cell> {
    let cells = role.forward_cells(&header, wire, rng, now_ms);
    cells
}
";
    assert!(
        check_code_occurs_once(violating, CELL_COMPOSITION_CALL).is_some(),
        "a second cell-composition pipeline must be reported"
    );
}

/// The inverted probe for rule 7: a module without the gate must be reported.
#[test]
fn the_guard_reports_a_module_without_the_unproven_pair_gate() {
    let violating = "\
fn drive(&mut self) -> LivenessAction {
    for pair in self.pairs.values() {
        if pair.consecutive_failures >= self.config.failure_threshold {
            return LivenessAction::MarkUnhealthy {
                outbound_slot: pair.outbound_slot,
                inbound_slot: pair.inbound_slot,
                consecutive_failures: pair.consecutive_failures,
            };
        }
    }
    LivenessAction::Idle { active_pairs: self.pairs.len() }
}
";
    assert!(
        check_code_contains(violating, UNPROVEN_PAIR_GATE).is_some(),
        "a module without the unproven-pair gate must be reported"
    );
}

/// The inverted probe for rule 8: a weakened gate inside `fn drive` must be
/// reported.
///
/// This is the probe that matters for Plan 504 invariant 9: it is a *real*
/// negative, because the weakened gate is exactly the change that would tear
/// down a pair that has never answered.
#[test]
fn the_guard_reports_a_weakened_gate_in_the_drive_function() {
    let violating = "\
fn drive(&mut self) -> LivenessAction {
    for pair in self.pairs.values() {
        if pair.consecutive_failures >= self.config.failure_threshold {
            return LivenessAction::MarkUnhealthy {
                outbound_slot: pair.outbound_slot,
                inbound_slot: pair.inbound_slot,
                consecutive_failures: pair.consecutive_failures,
            };
        }
    }
    LivenessAction::Idle { active_pairs: self.pairs.len() }
}
";
    assert!(
        check_body_contains(violating, "drive", UNPROVEN_PAIR_GATE).is_some(),
        "a weakened gate inside `fn drive` must be reported"
    );
}

/// The inverted probe for rule 9: a `DeliveryStatus` classification that
/// discards the acknowledged id must be reported.
#[test]
fn the_guard_reports_a_classification_without_the_acknowledged_id() {
    let violating = "\
fn classify_message(message: &I2npMessage) -> RouterI2npOutcome {
    match message.body() {
        I2npBody::DeliveryStatus(status) => Ok(RouterI2npOutcome::RouterControl {
            kind: RouterI2npKind::DeliveryStatus,
            acknowledged_message_id: None,
        }),
        other => Ok(RouterI2npOutcome::Unsupported {
            type_byte: other.message_type().code(),
        }),
    }
}
";
    assert!(
        check_body_contains(
            violating,
            "classify_message",
            "acknowledged_message_id: Some(status.message_id)"
        )
        .is_some(),
        "a classification that discards the acknowledged id must be reported"
    );
}

/// The inverted probe for rule 10: a scheduler that credits any response must
/// be reported.
#[test]
fn the_guard_reports_an_unauthenticated_scheduler_match() {
    let violating = "\
pub fn record_authenticated_response(&mut self, acknowledged_message_id: u32) -> Option<LivenessTestId> {
    let test_id = self.pending.keys().next().copied();
    let _ = acknowledged_message_id;
    test_id
}
";
    assert!(
        check_body_contains(
            violating,
            "record_authenticated_response",
            "test.probe_message_id == acknowledged_message_id"
        )
        .is_some(),
        "a scheduler that credits any response must be reported"
    );
}

/// The inverted probe for rule 11: a seam that never reads the acknowledged
/// id must be reported.
#[test]
fn the_guard_reports_a_seam_that_ignores_the_acknowledged_id() {
    let violating = "\
fn record_authenticated_outcome(
    scheduler: &mut TunnelLivenessScheduler,
    outcome: &RouterI2npOutcome,
) -> Option<LivenessTestId> {
    let _ = outcome;
    scheduler.next_pending_test_id()
}
";
    assert!(
        check_body_contains(
            violating,
            "record_authenticated_outcome",
            "outcome.acknowledged_message_id()?"
        )
        .is_some(),
        "a seam that ignores the acknowledged id must be reported"
    );
}

/// The guard must not report a construct that only appears in a comment or in
/// a `#[cfg(test)]` block.
#[test]
fn the_guard_ignores_comments_and_test_blocks() {
    // Rule 2: the seam call appears in a comment and in a test block only,
    // so the production body does not route through the seam.
    let commented = "\
pub(crate) async fn process_inbound(&mut self, inbound: &Ssu2InboundI2np) {
    // the live path calls route_inbound_with_liveness( here
    let routed = self.inner.coordinator.route_inbound_i2np(inbound, 0);
    let _ = routed;
}

#[cfg(test)]
mod tests {
    #[test]
    fn uses_the_seam() {
        let _ = route_inbound_with_liveness(
            coordinator,
            scheduler,
            inbound,
            0,
        );
    }
}
";
    assert!(
        check_body_contains_any(commented, "process_inbound", LIVENESS_SEAM_CALLS).is_some(),
        "a seam call that appears only in a comment or a test block must not satisfy the rule"
    );

    // Rule 3: the bare call appears in a comment only, so it must not be
    // reported.
    let bare_in_comment = "\
fn route(inbound: &Ssu2InboundI2np, now_ms: u64) {
    // the old path was coordinator.route_inbound_i2np(inbound, now_ms)
    let _ = (inbound, now_ms);
}
";
    assert_eq!(
        check_code_avoids(bare_in_comment, ".route_inbound_i2np("),
        None,
        "a bare call that appears only in a comment must not be reported"
    );

    // Rule 7: the gate appears in a doc comment only, so it must not satisfy
    // the rule.
    let gate_in_comment = "\
/// The pair is torn down only when `pair.proven &&` holds.
fn drive(&mut self) -> LivenessAction {
    LivenessAction::Idle { active_pairs: 0 }
}
";
    assert!(
        check_code_contains(gate_in_comment, UNPROVEN_PAIR_GATE).is_some(),
        "a gate that appears only in a doc comment must not satisfy the rule"
    );

    // Rule 6: a second cell-composition call inside a test block does not
    // make the module hold two pipelines.
    let second_in_test_block = "\
fn compose_outbound_build_deliveries(role: &OutboundGatewayRole) -> Vec<Cell> {
    let cells = role.forward_cells(&header, wire, rng, now_ms);
    cells
}

#[cfg(test)]
mod tests {
    #[test]
    fn composes_cells() {
        let cells = role.forward_cells(&header, wire, rng, now_ms);
        let _ = cells;
    }
}
";
    assert_eq!(
        check_code_occurs_once(second_in_test_block, CELL_COMPOSITION_CALL),
        None,
        "a cell-composition call inside a test block must not count as a second pipeline"
    );
}

/// The rules are **structural**, not textual: a token that lives in a
/// sibling function must not satisfy a rule that names one function's body.
///
/// This is the property a whole-file token search cannot have, and it is the
/// property Plan 185's gap needed: the scheduler module existed with every
/// token in it, and a whole-file search would have found all of them while
/// the daemon still owned no scheduler at all.
#[test]
fn a_token_in_a_sibling_function_does_not_satisfy_a_rule() {
    // Rule 1: the scheduler is constructed by a sibling function.
    let rule_1 = "\
fn helper() {
    let scheduler = TunnelLivenessScheduler::new(LivenessConfig::plan_185_defaults());
    let _ = scheduler;
}
pub async fn start(spec: ServiceProductSpec) -> Result<Self, ServiceProductError> {
    build(spec)
}
";
    assert!(
        check_body_contains(rule_1, "start", "TunnelLivenessScheduler::new(").is_some(),
        "a scheduler constructed by a sibling function must not satisfy rule 1"
    );

    // Rule 2: the seam call sits in a sibling function.
    let rule_2 = "\
fn helper() {
    let _ = route_inbound_with_liveness(coordinator, scheduler, inbound, 0);
}
pub(crate) async fn process_inbound(&mut self, inbound: &Ssu2InboundI2np, now_ms: u64) {
    let routed = self.inner.coordinator.route_inbound_i2np(inbound, now_ms)?;
    let _ = routed;
}
";
    assert!(
        check_body_contains_any(rule_2, "process_inbound", LIVENESS_SEAM_CALLS).is_some(),
        "a seam call in a sibling function must not satisfy rule 2"
    );

    // Rule 4: the tick call and the `drive()` call each sit in a sibling
    // function.
    let rule_4 = "\
fn helper(&mut self, now_ms: u64) {
    self.drive_tunnel_liveness(now_ms);
    let _ = self.inner.liveness.drive();
}
async fn advance_destination_pools_at(&mut self, now_ms: u64) {
    self.advance_pools(now_ms);
}
async fn drive_tunnel_liveness(&mut self, now_ms: u64) {
    self.sync_liveness_pairs();
}
";
    assert!(
        check_body_contains(
            rule_4,
            "advance_destination_pools_at",
            "drive_tunnel_liveness("
        )
        .is_some(),
        "a tick call in a sibling function must not satisfy rule 4"
    );
    assert!(
        check_body_contains(rule_4, "drive_tunnel_liveness", "drive()").is_some(),
        "a `drive()` call in a sibling function must not satisfy rule 4"
    );

    // Rule 5: the canonical composition call sits in a sibling function.
    let rule_5 = "\
fn helper(role: &OutboundGatewayRole) {
    let _ = compose_outbound_build_deliveries(
        role,
        instruction,
        message_id,
        expiration_ms,
        &wire,
        now_ms,
        &mut rng,
    );
}
pub fn compose_liveness_probe(&mut self, outbound_slot: TunnelSlot, probe_message_id: u32) {
    let _ = (outbound_slot, probe_message_id);
}
";
    assert!(
        check_body_contains(
            rule_5,
            "compose_liveness_probe",
            "compose_outbound_build_deliveries("
        )
        .is_some(),
        "a canonical composition call in a sibling function must not satisfy rule 5"
    );

    // Rule 8: the gate sits in a sibling function. Rule 7 is the module-wide
    // half and is satisfied by this fixture; rule 8 is the body-scoped half
    // and is not.
    let rule_8 = "\
fn helper(pair: &PairState, threshold: u8) -> bool {
    pair.proven && pair.consecutive_failures >= threshold
}
fn drive(&mut self) -> LivenessAction {
    LivenessAction::Idle { active_pairs: 0 }
}
";
    assert_eq!(
        check_code_contains(rule_8, UNPROVEN_PAIR_GATE),
        None,
        "the module-wide half of the gate rule must accept a gate in a sibling function"
    );
    assert!(
        check_body_contains(rule_8, "drive", UNPROVEN_PAIR_GATE).is_some(),
        "a gate in a sibling function must not satisfy rule 8"
    );

    // Rule 9: the acknowledged id is carried by a sibling function.
    let rule_9 = "\
fn helper(status: &DeliveryStatus) -> Option<u32> {
    let acknowledged = Some(status.message_id);
    acknowledged
}
fn classify_message(message: &I2npMessage) -> RouterI2npOutcome {
    RouterI2npOutcome::Unsupported { type_byte: 0 }
}
";
    assert!(
        check_body_contains(
            rule_9,
            "classify_message",
            "acknowledged_message_id: Some(status.message_id)"
        )
        .is_some(),
        "an acknowledged id carried by a sibling function must not satisfy rule 9"
    );

    // Rule 10: the authenticated match sits in a sibling function.
    let rule_10 = "\
fn helper(test: &PendingTest, acknowledged_message_id: u32) -> bool {
    test.probe_message_id == acknowledged_message_id
}
pub fn record_authenticated_response(
    &mut self,
    acknowledged_message_id: u32,
) -> Option<LivenessTestId> {
    let _ = acknowledged_message_id;
    None
}
";
    assert!(
        check_body_contains(
            rule_10,
            "record_authenticated_response",
            "test.probe_message_id == acknowledged_message_id"
        )
        .is_some(),
        "an authenticated match in a sibling function must not satisfy rule 10"
    );

    // Rule 11: the acknowledged id is read by a sibling function.
    let rule_11 = "\
fn helper(outcome: &RouterI2npOutcome) -> Option<u32> {
    outcome.acknowledged_message_id()
}
fn record_authenticated_outcome(
    scheduler: &mut TunnelLivenessScheduler,
    outcome: &RouterI2npOutcome,
) -> Option<LivenessTestId> {
    let _ = outcome;
    scheduler.next_pending_test_id()
}
";
    assert!(
        check_body_contains(
            rule_11,
            "record_authenticated_outcome",
            "outcome.acknowledged_message_id()?"
        )
        .is_some(),
        "an acknowledged id read by a sibling function must not satisfy rule 11"
    );
}

/// The guards fire on a realistic weakening of the **landed** source, not
/// only on a synthetic fixture.
///
/// Each mutation is the smallest edit that would reintroduce the defect the
/// rule exists to catch. The guard must report every one of them; the
/// `assert_ne!` before each check proves the mutation actually applied, so a
/// probe cannot pass because the edit silently did nothing.
#[test]
fn the_guard_fires_on_a_weakened_copy_of_the_landed_source() {
    let product = source_of("crates/i2pr-daemon/src/service_product.rs");
    let liveness = source_of("crates/i2pr-daemon/src/tunnel_liveness.rs");
    let router = source_of("crates/i2pr-daemon/src/router_i2np.rs");

    // Rule 1: the composition stops constructing the scheduler.
    let weakened = product.replace(
        "TunnelLivenessScheduler::new(",
        "TunnelLivenessScheduler::resumed(",
    );
    assert_ne!(
        weakened, product,
        "the scheduler mutation must have applied"
    );
    for function in ["start", "start_over_existing_daemon"] {
        assert!(
            check_body_contains(&weakened, function, "TunnelLivenessScheduler::new(").is_some(),
            "a composition that stops constructing the scheduler must be reported"
        );
    }

    // Rule 2: the live inbound path stops routing through the seam.
    let weakened = product.replace(
        "route_inbound_with_liveness_classified(",
        "route_inbound_i2np_classified(",
    );
    assert_ne!(weakened, product, "the seam mutation must have applied");
    assert!(
        check_body_contains_any(&weakened, "process_inbound", LIVENESS_SEAM_CALLS).is_some(),
        "an inbound path that stops routing through the seam must be reported"
    );

    // Rule 4: the live tick stops driving the scheduler.
    let weakened = product.replace(
        "self.drive_tunnel_liveness(now_ms).await;",
        "self.advance_destination_pools(now_ms);",
    );
    assert_ne!(weakened, product, "the tick mutation must have applied");
    assert!(
        check_body_contains(
            &weakened,
            "advance_destination_pools_at",
            "drive_tunnel_liveness("
        )
        .is_some(),
        "a tick that stops driving the scheduler must be reported"
    );

    // Rule 8: the unproven-pair gate is weakened in the landed source.
    let weakened = liveness.replace(
        "pair.proven && pair.consecutive_failures",
        "pair.consecutive_failures",
    );
    assert_ne!(weakened, liveness, "the gate mutation must have applied");
    assert!(
        check_body_contains(&weakened, "drive", UNPROVEN_PAIR_GATE).is_some(),
        "a weakened unproven-pair gate must be reported"
    );

    // Rule 10: the scheduler stops matching on the acknowledged id.
    let weakened = liveness.replace("test.probe_message_id == acknowledged_message_id", "true");
    assert_ne!(
        weakened, liveness,
        "the correlation mutation must have applied"
    );
    assert!(
        check_body_contains(
            &weakened,
            "record_authenticated_response",
            "test.probe_message_id == acknowledged_message_id"
        )
        .is_some(),
        "an unauthenticated correlation must be reported"
    );

    // Rule 9: the dispatcher stops carrying the acknowledged id.
    let weakened = router.replace(
        "acknowledged_message_id: Some(status.message_id),",
        "acknowledged_message_id: None,",
    );
    assert_ne!(
        weakened, router,
        "the classification mutation must have applied"
    );
    assert!(
        check_body_contains(
            &weakened,
            "classify_message",
            "acknowledged_message_id: Some(status.message_id)"
        )
        .is_some(),
        "a classification that drops the acknowledged id must be reported"
    );

    // Rule 5: the probe stops using the canonical composition.
    let exploratory = source_of("crates/i2pr-daemon/src/exploratory_build.rs");
    let weakened = exploratory.replace(
        "compose_outbound_build_deliveries(",
        "compose_private_probe_deliveries(",
    );
    assert_ne!(
        weakened, exploratory,
        "the composition mutation must have applied"
    );
    assert!(
        check_body_contains(
            &weakened,
            "compose_liveness_probe",
            "compose_outbound_build_deliveries("
        )
        .is_some(),
        "a probe outside the canonical composition must be reported"
    );
}

/// The body extractor must resolve the right body in the awkward cases.
///
/// Each case is a separate failure mode: a nested block, a string that holds
/// a closing brace, a character literal that holds an opening brace, a
/// trailing item after the function, and an absent function.
#[test]
fn the_body_extractor_resolves_the_right_body() {
    // A nested block: the inner function's body is not the outer one's.
    let nested = "\
fn outer() {
    fn inner() {
        let value = 1;
    }
    inner();
}
";
    let nested_code = code_view(nested);
    let inner_body = function_body(&nested_code, "inner").expect("inner body");
    assert!(
        inner_body.contains("let value = 1;"),
        "a nested function must resolve to its own body"
    );
    assert!(
        !inner_body.contains("inner();"),
        "a nested function's body must not swallow the outer body"
    );

    // A string that holds a closing brace must not end the block early.
    let string_brace = "\
fn holds_brace() {
    let text = \"}\";
    let value = 2;
}
";
    let string_code = code_view(string_brace);
    let string_body = function_body(&string_code, "holds_brace").expect("body");
    assert!(
        string_body.contains("let value = 2;"),
        "a string holding a closing brace must not end the body early"
    );

    // A character literal that holds an opening brace must not open a block.
    let char_brace = "\
fn holds_char() {
    let brace = '{';
    let value = 3;
}
";
    let char_code = code_view(char_brace);
    let char_body = function_body(&char_code, "holds_char").expect("body");
    assert!(
        char_body.contains("let value = 3;"),
        "a character literal holding an opening brace must not open a block"
    );

    // A trailing item must not be swallowed by the preceding function.
    let trailing = "\
fn first() {
    let value = 4;
}
fn second() {
    let other = 5;
}
";
    let trailing_code = code_view(trailing);
    let first_body = function_body(&trailing_code, "first").expect("body");
    assert!(
        first_body.contains("let value = 4;") && !first_body.contains("let other = 5;"),
        "a trailing item must not be swallowed by the preceding function"
    );

    // An absent function yields `None`, so every caller fails closed.
    assert!(
        function_body(&code_view(trailing), "absent").is_none(),
        "an absent function must yield `None`"
    );
    assert!(
        check_body_contains(trailing, "absent", "drive()").is_some(),
        "an absent function must fail closed"
    );

    // The definition anchor is not a prefix match: `fn start` must not
    // resolve to `fn startup_...`.
    let prefixed = "\
fn startup_something() {
    let value = 6;
}
fn start(spec: Spec) {
    let value = 7;
}
";
    let prefixed_code = code_view(prefixed);
    let start_body = function_body(&prefixed_code, "start").expect("body");
    assert!(
        start_body.contains("let value = 7;") && !start_body.contains("let value = 6;"),
        "the definition anchor must not match a longer identifier"
    );

    // The same rule applies to a *suffix* extension, which is the real
    // shape in the product: `fn start` and `fn start_over_existing_daemon`
    // are two different functions, and the anchor must resolve each of them
    // to its own body.
    let suffixed = "\
fn start(spec: Spec) {
    let value = 8;
}
fn start_over_existing_daemon(spec: Spec) {
    let value = 9;
}
";
    let suffixed_code = code_view(suffixed);
    let start_body = function_body(&suffixed_code, "start").expect("body");
    let over_body = function_body(&suffixed_code, "start_over_existing_daemon").expect("body");
    assert!(
        start_body.contains("let value = 8;") && !start_body.contains("let value = 9;"),
        "`fn start` must not resolve to `fn start_over_existing_daemon`"
    );
    assert!(
        over_body.contains("let value = 9;") && !over_body.contains("let value = 8;"),
        "`fn start_over_existing_daemon` must resolve to its own body"
    );
}
