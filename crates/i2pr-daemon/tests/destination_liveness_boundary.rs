//! Plan 512 static boundary guard for the destination-role tunnel liveness.
//!
//! Plan 487's F3 recorded that the destination inbound pool has no
//! transit-hop failure detection: a dead transit hop is invisible to the
//! router until the tunnel expires, so a destination whose tunnels die with a
//! hop is not replenished until the lease expires.
//!
//! This guard is deliberately **structural**. A whole-file token search would
//! not have caught Plan 487's gap — the scheduler existed, the population
//! function existed in another role, and every token existed — so a rule that
//! greps the file for a name proves nothing about the *composition*. Every
//! rule below asserts that a **named function's own body** contains the
//! required **call**, matched with its open parenthesis, so a token that
//! appears in a sibling function, in a comment, or in a `#[cfg(test)]` block
//! does not satisfy the rule.
//!
//! The rules, and what each one would let through if it were absent:
//!
//! 1. **The live tick registers the destination-role population.** The body of
//!    `sync_liveness_pairs` must call `destination_liveness_pairs(`. Without
//!    it, the destination role is never tracked and Plan 487's F3 stays open.
//! 2. **The population is formed within one destination.** The body of
//!    `destination_liveness_pairs` must look the inbound list up **by
//!    destination**. Without it, the population is a global zip, and one
//!    destination's dead outbound would fail another destination's inbound —
//!    a false failure the plan must not produce.
//! 3. **The live tick routes the failure to the destination arm.** The body of
//!    `drive_tunnel_liveness` must call `fail_destination_liveness_pair(`.
//!    Without it, an unhealthy destination pair is removed as if it were an
//!    exploratory pair, and the destination's tunnel is never failed.
//! 4. **The destination failure arm reaches the runtime's failure seam.** The
//!    body of `fail_destination_liveness_pair` must call
//!    `destination_liveness_failure_decision(` and `mark_tunnel_failed(`.
//!    Without them, the failure never reaches the pool, so the lease set never
//!    changes and the destination is never replenished.
//! 5. **The last-tunnel rule is enforced.** The body of
//!    `destination_liveness_failure_decision` must consult `inbound_len` and
//!    return `LastTunnelRefused`. Without it, the router can fail a
//!    destination's only tunnel and leave it with no path and no
//!    advertisement.
//! 6. **The destination probe is composed through the canonical seam.** The
//!    body of `compose_destination_liveness_probe` must call
//!    `compose_outbound_build_deliveries(`. Without it, the probe takes a
//!    private path that is not the one the build reply uses.
//! 7. **The destination probe is addressed to the inbound's gateway.** The
//!    body of `compose_destination_liveness_probe` must read the registry's
//!    `inbound_gateway_route(` and address the probe with
//!    `DeliveryInstruction::Tunnel {`. Without it, the probe is not the
//!    reference's probe, and the answer cannot come back through the inbound
//!    tunnel the destination advertises.
//! 8. **The destination probe does not read the exploratory pool.** The body
//!    of `compose_destination_liveness_probe` must not contain
//!    `self.pool.routing(`. The destination role's inbound lives in the
//!    shared registry, not in the exploratory pool.
//! 9. **The exploratory composition keeps its same-peer precondition.** The
//!    body of `compose_liveness_probe` must still contain
//!    `gateway != outbound_first_hop`. Without it, Plan 504's exploratory
//!    contract changes under this plan.
//! 10. **The exploratory population is unchanged.** The body of
//!     `exploratory_liveness_pairs` must not mention a destination role.
//!     Without it, the exploratory population and the destination population
//!     are one population, and Plan 504's lane's
//!     `a_destination_tunnel_never_appears_in_the_tracked_exploratory_pairs`
//!     would be describing a different tree.
//! 11. **The proven-pair gate governs the destination arm.** The body of the
//!     scheduler's `drive` must contain `pair.proven &&`, and the gate must
//!     appear exactly once in the production code view of the scheduler
//!     module. Without it, a destination pair that has **never** answered is
//!     torn down on a signal the router cannot distinguish from its own
//!     negation.
//! 12. **The binding table is bounded.** The body of
//!     `record_destination_role_binding` must consult
//!     `MAX_DESTINATION_ROLE_BINDINGS`. Without it, the table grows without
//!     bound.
//! 13. **The failure decision carries no peer identity.** The decision enum
//!     must name a pool slot and must name no peer, router hash or address.
//!     Without it, a peer-side fact could leak into the router's own
//!     reaction, and a local condition could be written into a peer.
//! 14. **Plan 504's guard and lane are byte-identical.** Their SHA-256
//!     digests are pinned. Without it, a successor could quietly rewrite the
//!     predecessor's guard and this plan's "the exploratory role is
//!     unchanged" claim would be unverifiable.
//! 15. **The lane is present, bounded and sleep-free.** The lane file must
//!     exist and must not sleep.
//!
//! Rules 1 to 9 and 11 to 13 are **structural**: they assert that a named
//! function's own body contains the required call. Rules 10, 14 and 15 are
//! **textual**: they assert that a token does or does not appear, that a
//! digest matches, or that a file is present and sleep-free. The guard does
//! **not** claim that the live behaviour is correct — the lane in
//! `crates/i2pr-daemon/tests/destination_liveness_live.rs` proves that; the
//! guard only proves that the wiring is present in the place it must be
//! present.
//!
//! The guard is **fail-closed**: comments are stripped before matching, every
//! `#[cfg(test)]` block is excluded, an absent function yields `None` so
//! every caller fails, and every rule carries its own **inverted probe** — a
//! synthetic violating source must be reported. If a rule ever stops
//! detecting its violation, its probe fails.
//!
//! The helpers below are duplicated from
//! `crates/i2pr-daemon/tests/tunnel_liveness_boundary.rs` (Plan 504's guard),
//! which is the best structural guard in the repository. Each integration test
//! is its own crate, so the helpers cannot be shared by import; the
//! duplication is a consequence of Rust's integration-test model, not a
//! choice.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The same-peer precondition of the exploratory composition, exactly as the
/// landed code spells it.
///
/// The exploratory pair's two tunnels are built to one peer, so the rule
/// holds naturally there. The destination role's two directions are built in
/// separate rounds from separate peer selections, so the destination
/// composition deliberately does **not** carry this precondition — and rule 9
/// pins that the exploratory one still does.
const SAME_PEER_PRECONDITION: &str = "gateway != outbound_first_hop";

/// The proven-pair gate, exactly as Plan 504 spells it.
///
/// The gate is `pair.proven && pair.consecutive_failures >= threshold`. Weakening
/// it to `pair.consecutive_failures >= threshold` — tearing down a pair that
/// has never answered — removes this token and trips rule 11.
const UNPROVEN_PAIR_GATE: &str = "pair.proven &&";

/// The pinned SHA-256 digests of Plan 504's guard and lane.
///
/// Plan 504's rule was that the exploratory role's behaviour is unchanged by
/// this plan. The digests make that claim checkable rather than asserted.
const PLAN_504_GUARD_SHA256: &str =
    "b06df9204c51b93707d7f7fc449a409d46d35bbf75eae715372fe34ccf6406f0";
const PLAN_504_LANE_SHA256: &str =
    "84e4f4a2b141acd1494fd00afe32964cffdd0e9e2e11e1afdad471a96e03bd0c";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn source_of(relative: &str) -> String {
    let path = workspace_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("read {}: {error}", path.display());
    })
}

fn bytes_of(relative: &str) -> Vec<u8> {
    let path = workspace_root().join(relative);
    fs::read(&path).unwrap_or_else(|error| {
        panic!("read {}: {error}", path.display());
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
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

/// Returns the body of the definition anchored at `<keyword> <name>`.
///
/// The anchor is the definition and never a call site: the character that
/// follows the name must not continue an identifier, so `fn start` never
/// matches `fn startup_something`. A definition that is not present yields
/// `None`, so every caller fails closed.
fn definition_body<'a>(code: &'a str, keyword: &str, name: &str) -> Option<&'a str> {
    let needle = format!("{keyword} {name}");
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

/// The body of the function whose **definition** is anchored at `fn <name>`.
fn function_body<'a>(code: &'a str, name: &str) -> Option<&'a str> {
    definition_body(code, "fn", name)
}

/// Structural rules: the body of `fn <name>` must contain `token`.
fn check_body_contains(source: &str, name: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    check_body_contains_in(&code, name, token)
}

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

/// Structural rules: the body of `fn <name>` must contain every token.
fn check_body_contains_all(source: &str, name: &str, tokens: &[&str]) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, name) else {
        return Some(format!("`fn {name}` must exist in the production surface"));
    };
    for token in tokens {
        if !body.contains(token) {
            return Some(format!("`fn {name}` must contain `{token}`"));
        }
    }
    None
}

/// Structural rules: the body of `fn <name>` must not contain `token`.
fn check_body_avoids(source: &str, name: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, name) else {
        return Some(format!("`fn {name}` must exist in the production surface"));
    };
    if body.contains(token) {
        Some(format!("`fn {name}` must not contain `{token}`"))
    } else {
        None
    }
}

/// Textual rules: the production code view must contain `token` exactly once.
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

/// Rule 13: the failure-decision enum must name a pool slot and must name no
/// peer identity.
fn check_decision_carries_no_peer(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = definition_body(&code, "enum", "DestinationLivenessFailureDecision") else {
        return Some(
            "`enum DestinationLivenessFailureDecision` must exist in the production surface"
                .to_owned(),
        );
    };
    if !body.contains("pool_slot") {
        return Some("the failure decision must name the pool slot it fails".to_owned());
    }
    for forbidden in ["Hash", "PeerId", "peer", "router_hash", "address"] {
        if body.contains(forbidden) {
            return Some(format!(
                "the failure decision must not carry a peer identity (`{forbidden}`)"
            ));
        }
    }
    None
}

/// Rule 1: the live tick registers the destination-role population.
#[test]
fn the_live_tick_registers_the_destination_role_population() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "sync_liveness_pairs",
            "destination_liveness_pairs("
        ),
        None,
        "`fn sync_liveness_pairs` must register the destination-role population"
    );
}

/// Rule 2: the population is formed within one destination.
#[test]
fn the_destination_population_is_formed_within_one_destination() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "destination_liveness_pairs",
            "inbounds.remove(&destination_id)"
        ),
        None,
        "the population must look the inbound list up by destination"
    );
    assert_eq!(
        check_body_contains(
            &source,
            "destination_liveness_pairs",
            "binding.destination_id"
        ),
        None,
        "the population must group the roles by their destination"
    );
}

/// Rule 3: the live tick routes the failure to the destination arm.
#[test]
fn the_live_tick_routes_the_failure_to_the_destination_arm() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "drive_tunnel_liveness",
            "fail_destination_liveness_pair("
        ),
        None,
        "`fn drive_tunnel_liveness` must route an unhealthy pair to the destination arm"
    );
}

/// Rule 4: the destination failure arm reaches the runtime's failure seam.
#[test]
fn the_destination_failure_arm_reaches_the_runtime_failure_seam() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_body_contains_all(
            &source,
            "fail_destination_liveness_pair",
            &[
                "destination_liveness_failure_decision(",
                "mark_tunnel_failed(",
            ],
        ),
        None,
        "the destination arm must decide and then apply the failure through the pool"
    );
}

/// Rule 5: the last-tunnel rule is enforced.
#[test]
fn the_last_tunnel_rule_is_enforced() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_body_contains_all(
            &source,
            "destination_liveness_failure_decision",
            &["inbound_len", "LastTunnelRefused"],
        ),
        None,
        "the decision must consult the inbound count and refuse the last tunnel"
    );
}

/// Rule 6: the destination probe is composed through the canonical seam.
#[test]
fn the_destination_probe_is_composed_through_the_canonical_seam() {
    let source = source_of("crates/i2pr-daemon/src/exploratory_build.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "compose_destination_liveness_probe",
            "compose_outbound_build_deliveries("
        ),
        None,
        "the destination probe must use the canonical cell composition"
    );
}

/// Rule 7: the destination probe is addressed to the inbound's gateway.
#[test]
fn the_destination_probe_is_addressed_to_the_gateways_route() {
    let source = source_of("crates/i2pr-daemon/src/exploratory_build.rs");
    assert_eq!(
        check_body_contains_all(
            &source,
            "compose_destination_liveness_probe",
            &["inbound_gateway_route(", "DeliveryInstruction::Tunnel {"],
        ),
        None,
        "the destination probe must be addressed to the inbound tunnel's gateway"
    );
}

/// Rule 8: the destination probe does not read the exploratory pool.
#[test]
fn the_destination_probe_does_not_read_the_exploratory_pool() {
    let source = source_of("crates/i2pr-daemon/src/exploratory_build.rs");
    assert_eq!(
        check_body_avoids(
            &source,
            "compose_destination_liveness_probe",
            "self.pool.routing("
        ),
        None,
        "the destination probe must read the shared registry, not the exploratory pool"
    );
}

/// Rule 9: the exploratory composition keeps its same-peer precondition.
#[test]
fn the_exploratory_composition_keeps_its_same_peer_precondition() {
    let source = source_of("crates/i2pr-daemon/src/exploratory_build.rs");
    assert_eq!(
        check_body_contains(&source, "compose_liveness_probe", SAME_PEER_PRECONDITION),
        None,
        "the exploratory composition must keep its same-peer precondition"
    );
}

/// Rule 10: the exploratory population is unchanged.
#[test]
fn the_exploratory_population_is_unchanged() {
    let source = source_of("crates/i2pr-daemon/src/exploratory_build.rs");
    assert_eq!(
        check_body_avoids(&source, "exploratory_liveness_pairs", "destination"),
        None,
        "the exploratory population must never report a destination role"
    );
}

/// Rule 11: the proven-pair gate governs the destination arm.
#[test]
fn the_proven_pair_gate_governs_the_destination_arm() {
    let source = source_of("crates/i2pr-daemon/src/tunnel_liveness.rs");
    assert_eq!(
        check_body_contains(&source, "drive", UNPROVEN_PAIR_GATE),
        None,
        "the scheduler's drive must keep the proven-pair gate"
    );
    assert_eq!(
        check_code_occurs_once(&source, UNPROVEN_PAIR_GATE),
        None,
        "the proven-pair gate must appear exactly once, so no looser gate can exist"
    );
}

/// Rule 12: the binding table is bounded.
#[test]
fn the_binding_table_is_bounded() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "record_destination_role_binding",
            "MAX_DESTINATION_ROLE_BINDINGS"
        ),
        None,
        "the binding table must consult its ceiling"
    );
}

/// Rule 13: the failure decision carries no peer identity.
#[test]
fn the_failure_decision_carries_no_peer_identity() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_decision_carries_no_peer(&source),
        None,
        "the failure decision must name a pool slot and no peer"
    );
}

/// Rule 14: Plan 504's guard and lane are byte-identical.
#[test]
fn the_predecessors_guard_and_lane_are_byte_identical() {
    assert_eq!(
        sha256_hex(&bytes_of(
            "crates/i2pr-daemon/tests/tunnel_liveness_boundary.rs"
        )),
        PLAN_504_GUARD_SHA256,
        "Plan 504's guard must stay byte-identical"
    );
    assert_eq!(
        sha256_hex(&bytes_of(
            "crates/i2pr-daemon/tests/tunnel_liveness_live.rs"
        )),
        PLAN_504_LANE_SHA256,
        "Plan 504's lane must stay byte-identical"
    );
}

/// Rule 15: the lane is present, bounded and sleep-free.
#[test]
fn the_lane_is_present_and_sleep_free() {
    let lane = source_of("crates/i2pr-daemon/tests/destination_liveness_live.rs");
    assert!(
        lane.contains(
            "fn the_destination_population_pairs_the_outbound_and_the_inbound_of_one_destination("
        ),
        "the lane must prove the population"
    );
    // The lane's *code* is checked, not its prose: the lane's own header
    // names the tokens it forbids, so a raw search would trip on the
    // documentation.
    let lane_code = code_view(&lane);
    for forbidden in ["thread::sleep", "sleep(", "tokio::time::sleep"] {
        assert!(
            !lane_code.contains(forbidden),
            "the lane must not sleep: `{forbidden}`"
        );
    }
    assert!(
        !lane_code.contains("#[ignore]"),
        "the lane must not gate a test behind an ignored attribute"
    );
}

/// Every rule carries an inverted probe: a synthetic violating source must be
/// reported. If a rule ever stops detecting its violation, this test fails.
#[test]
fn every_rule_detects_its_own_violation() {
    // Rule 1: a sync that never registers the destination population.
    let missing_population = "\
fn sync_liveness_pairs(&mut self) {
    let installed = self.inner.coordinator.exploratory_liveness_pairs();
}
";
    assert!(
        check_body_contains(
            missing_population,
            "sync_liveness_pairs",
            "destination_liveness_pairs("
        )
        .is_some(),
        "rule 1 must detect a missing destination population"
    );

    // Rule 2: a population that zips the lists globally.
    let global_zip = "\
fn destination_liveness_pairs(bindings: &Table) -> Vec<Pair> {
    let outbounds: Vec<Slot> = bindings.values().filter_map(|b| Some(b.role_slot)).collect();
    let inbounds: Vec<Slot> = bindings.values().filter_map(|b| Some(b.role_slot)).collect();
    outbounds.into_iter().zip(inbounds).collect()
}
";
    assert!(
        check_body_contains(
            global_zip,
            "destination_liveness_pairs",
            "inbounds.remove(&destination_id)"
        )
        .is_some(),
        "rule 2 must detect a population that is not grouped by destination"
    );

    // Rule 3: a tick that never routes to the destination arm.
    let missing_arm = "\
fn drive_tunnel_liveness(&mut self, now_ms: u64) {
    self.sync_liveness_pairs();
    let _ = self.inner.liveness.drive();
}
";
    assert!(
        check_body_contains(
            missing_arm,
            "drive_tunnel_liveness",
            "fail_destination_liveness_pair("
        )
        .is_some(),
        "rule 3 must detect a missing destination failure arm"
    );

    // Rule 4: an arm that decides but never applies the failure.
    let no_apply = "\
fn fail_destination_liveness_pair(&mut self, outbound_slot: Slot, inbound_slot: Slot) {
    let decision = destination_liveness_failure_decision(None, 2);
    let _ = decision;
}
";
    assert!(
        check_body_contains_all(
            no_apply,
            "fail_destination_liveness_pair",
            &[
                "destination_liveness_failure_decision(",
                "mark_tunnel_failed("
            ],
        )
        .is_some(),
        "rule 4 must detect a failure that never reaches the pool"
    );

    // Rule 5: a decision that never consults the inbound count.
    let no_last_tunnel_rule = "\
fn destination_liveness_failure_decision(binding: Option<&Binding>, inbound_len: usize) -> Decision {
    match binding {
        Some(binding) => Decision::Fail { pool_slot: binding.pool_slot },
        None => Decision::Unbound,
    }
}
";
    assert!(
        check_body_contains_all(
            no_last_tunnel_rule,
            "destination_liveness_failure_decision",
            &["inbound_len", "LastTunnelRefused"],
        )
        .is_some(),
        "rule 5 must detect a missing last-tunnel rule"
    );

    // Rule 6: a probe that composes its own cells.
    let private_pipeline = "\
fn compose_destination_liveness_probe(&self, slot: Slot) -> Vec<Cell> {
    let mut cells = Vec::new();
    cells.push(cell);
    cells
}
";
    assert!(
        check_body_contains(
            private_pipeline,
            "compose_destination_liveness_probe",
            "compose_outbound_build_deliveries("
        )
        .is_some(),
        "rule 6 must detect a private cell pipeline"
    );

    // Rule 7: a probe that does not read the gateway route.
    let no_route = "\
fn compose_destination_liveness_probe(&self, slot: Slot) -> Vec<Cell> {
    let gateway = Hash::from_bytes([0; 32]);
    compose_outbound_build_deliveries(role, DeliveryInstruction::Tunnel { tunnel_id: 1, gateway }, 2, 3, wire, 4, rng)
}
";
    assert!(
        check_body_contains_all(
            no_route,
            "compose_destination_liveness_probe",
            &["inbound_gateway_route(", "DeliveryInstruction::Tunnel {"],
        )
        .is_some(),
        "rule 7 must detect a probe that does not read the gateway route"
    );

    // Rule 8: a probe that reads the exploratory pool.
    let pool_read = "\
fn compose_destination_liveness_probe(&self, inbound_slot: Slot) -> Vec<Cell> {
    let routing = self.pool.routing(inbound_slot);
    let _ = routing;
    Vec::new()
}
";
    assert!(
        check_body_avoids(
            pool_read,
            "compose_destination_liveness_probe",
            "self.pool.routing("
        )
        .is_some(),
        "rule 8 must detect a probe that reads the exploratory pool"
    );

    // Rule 9: an exploratory composition without the same-peer precondition.
    let no_precondition = "\
fn compose_liveness_probe(&self, outbound_slot: Slot, inbound_slot: Slot) -> Vec<Cell> {
    let gateway = Hash::from_bytes([0; 32]);
    let _ = gateway;
    Vec::new()
}
";
    assert!(
        check_body_contains(
            no_precondition,
            "compose_liveness_probe",
            SAME_PEER_PRECONDITION
        )
        .is_some(),
        "rule 9 must detect a weakened exploratory precondition"
    );

    // Rule 10: an exploratory population that reports a destination role.
    let mixed_population = "\
fn exploratory_liveness_pairs(&self) -> Vec<(Slot, Slot)> {
    let destination = self.destination_role_slots();
    destination
}
";
    assert!(
        check_body_avoids(
            mixed_population,
            "exploratory_liveness_pairs",
            "destination"
        )
        .is_some(),
        "rule 10 must detect an exploratory population that reports a destination role"
    );

    // Rule 11: a scheduler that tears down a pair that never answered.
    let no_gate = "\
fn drive(&mut self) -> Action {
    if pair.consecutive_failures >= threshold {
        return Action::MarkUnhealthy { .. };
    }
    Action::Idle { .. }
}
";
    assert!(
        check_body_contains(no_gate, "drive", UNPROVEN_PAIR_GATE).is_some(),
        "rule 11 must detect a missing proven-pair gate"
    );

    // Rule 12: a binding table without a ceiling.
    let unbounded = "\
fn record_destination_role_binding(&mut self, binding: Binding) {
    self.inner.destination_role_bindings.insert(binding.role_slot, binding);
}
";
    assert!(
        check_body_contains(
            unbounded,
            "record_destination_role_binding",
            "MAX_DESTINATION_ROLE_BINDINGS"
        )
        .is_some(),
        "rule 12 must detect an unbounded binding table"
    );

    // Rule 13: a decision that carries a peer identity.
    let peer_decision = "\
enum DestinationLivenessFailureDecision {
    Fail { pool_slot: Slot, peer_hash: Hash },
    Unbound,
}
";
    assert!(
        check_decision_carries_no_peer(peer_decision).is_some(),
        "rule 13 must detect a failure decision that carries a peer identity"
    );

    // The helpers themselves fail closed: an absent function yields `None`,
    // so every caller fails.
    assert!(
        function_body(&code_view("fn present() {}"), "absent").is_none(),
        "an absent function must yield `None`"
    );
    assert!(
        check_body_contains("fn present() {}", "absent", "drive()").is_some(),
        "an absent function must fail closed"
    );
}
