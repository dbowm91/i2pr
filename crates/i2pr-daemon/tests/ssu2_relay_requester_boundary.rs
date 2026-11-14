//! Plan 531's guard -- the router must reach a peer through its introducer.
//!
//! The tree implemented the SSU2 relay protocol's introducer side and the
//! target's tag acquisition, but the requester side was unwired: the
//! transport's requester state machine, the runtime's relay-request seam
//! and the runtime's hole-punch handler all existed and all had zero
//! production callers. A router could therefore not reach a peer that is
//! only reachable through an introducer, and the transport's contradicting
//! *relay-firewalled* class had no producer at all.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the owner lives in the **daemon** and the mechanics stay in the
//!    **runtime**;
//! 2. the owner holds no task, no timer, no listener and no socket;
//! 3. the request is started only from **validated** material;
//! 4. the opt-in is **off** by default and refused without the SSU2
//!    service;
//! 5. the transport's relay state machines and the reachability model are
//!    untouched;
//! 6. the relay-firewalled signal is recorded as a **contradiction**,
//!    never as a corroboration, and the daemon never writes into the
//!    reachability tracker itself;
//! 7. the configuration surface gains no key beside the one opt-in;
//! 8. the owner is constructed and driven on the daemon's **existing**
//!    tick, so the requester is never dead code again.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, and every rule
//! **fails closed** when the item it names is absent.

#![forbid(unsafe_code)]

const TRANSPORT_REACHABILITY_MODULE: &str = "crates/i2pr-transport/src/reachability.rs";
const TRANSPORT_RELAY_MODULE: &str = "crates/i2pr-transport-ssu2/src/relay.rs";
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const RUNTIME_PEER_RELAY_MODULE: &str = "crates/i2pr-runtime/src/ssu2_peer_relay.rs";
const DAEMON_REQUESTER_MODULE: &str = "crates/i2pr-daemon/src/relay_requester.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const DAEMON_ROUTER_I2NP: &str = "crates/i2pr-daemon/src/router_i2np.rs";
const DAEMON_COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const DAEMON_DRAIN: &str = "crates/i2pr-daemon/src/ssu2_relay_drain.rs";

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

/// Collapses every run of whitespace into a single space.
///
/// A rule that reads a call site must not depend on where the formatter
/// happened to break the line.
fn squeeze(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_space = false;
    for character in source.chars() {
        if character.is_whitespace() {
            in_space = true;
            continue;
        }
        if in_space {
            out.push(' ');
            in_space = false;
        }
        out.push(character);
    }
    if in_space {
        out.push(' ');
    }
    out
}

/// Removes every whitespace character.
///
/// A rule that reads a call site must not depend on where the formatter
/// happened to break the line, and a broken call is `self`, `.runtime`
/// and `.start_relay_request(` on three lines.
fn compact(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
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

// ---- 1. the owner is the daemon's, the mechanics are the runtime's ----

fn check_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(
    owner: &str,
    runtime: &str,
    peer_relay: &str,
) -> Option<String> {
    let owner_code = code_view(owner);

    let Some(structure) = item_body(&owner_code, "pub struct RelayRequesterOwner") else {
        return Some("the daemon's requester owner is absent".to_owned());
    };
    if !squeeze(&structure).contains("runtime: i2pr_runtime::Ssu2RuntimeService") {
        return Some("the owner must hold the runtime that owns the sessions".to_owned());
    }
    if owner_code.contains("pub struct RelayRequester {") {
        return Some("the daemon must not re-implement the transport's requester table".to_owned());
    }

    let Some(start) = item_body(&owner_code, "pub fn start_request(") else {
        return Some("the owner's start step is absent".to_owned());
    };
    let start = compact(&start);
    if !start.contains("self.runtime.start_relay_request(") {
        return Some("the owner must start the request through the runtime's seam".to_owned());
    }
    if !start.contains("self.runtime.queue_relay_request(") {
        return Some("the owner must carry the request through the runtime's seam".to_owned());
    }

    let runtime_code = code_view(runtime);
    let Some(runtime_start) = item_body(&runtime_code, "pub fn start_relay_request(") else {
        return Some("the runtime's relay-request seam is absent".to_owned());
    };
    if !compact(&runtime_start).contains("self.shared.peer_relay.start_relay_request(") {
        return Some("the runtime's seam must drive the peer-relay service".to_owned());
    }

    let peer_relay_code = code_view(peer_relay);
    let Some(service_start) = item_body(&peer_relay_code, "pub fn start_relay_request(") else {
        return Some("the peer-relay service's relay-request seam is absent".to_owned());
    };
    if !compact(&service_start)
        .contains(".start(nonce,tag,bob_hash,charlie_hash,alice_endpoint,now_ms)")
    {
        return Some(
            "the peer-relay service must drive the transport's requester table".to_owned(),
        );
    }
    None
}

// ---- 2. the owner adds no infrastructure ------------------------------

fn check_the_owner_adds_no_infrastructure(owner: &str) -> Option<String> {
    let code = code_view(owner);
    for forbidden in [
        "tokio::",
        "UdpSocket",
        "TcpListener",
        "TcpStream",
        "spawn(",
        "sleep(",
        "interval(",
        "std::thread",
    ] {
        if code.contains(forbidden) {
            return Some(format!("the owner must not add {forbidden}"));
        }
    }
    None
}

// ---- 3. the request is started only from validated material ----------

fn check_the_request_is_started_only_from_validated_material(owner: &str) -> Option<String> {
    let code = code_view(owner);

    let Some(read) = item_body(&code, "pub fn read_published_introducers(") else {
        return Some("the published introducer read is absent".to_owned());
    };
    if !compact(&read).contains("store.get(") {
        return Some("the published set must be read from the validated store".to_owned());
    }
    if !compact(&read).contains("i2pr_runtime::Ssu2RouterAddress::parse(") {
        return Some("the address must be parsed with the transport's strict parser".to_owned());
    }

    let Some(resolve) = item_body(&code, "pub fn read_introducer_endpoint(") else {
        return Some("the introducer resolution is absent".to_owned());
    };
    if !compact(&resolve).contains("store.get(") {
        return Some("the introducer must be resolved from the validated store".to_owned());
    }

    let Some(build) = item_body(&code, "pub fn build_request(") else {
        return Some("the request builder is absent".to_owned());
    };
    if !compact(&build).contains("relay_request_preimage(") {
        return Some("the request must be signed over the specification's preimage".to_owned());
    }

    let Some(response) = item_body(&code, "pub fn on_relay_response(") else {
        return Some("the response handler is absent".to_owned());
    };
    if !compact(&response).contains("signers.signing_key(") {
        return Some("the target's signing key must come from the validated store".to_owned());
    }
    None
}

// ---- 4. the opt-in defaults off and needs the service ----------------

fn check_the_opt_in_defaults_off_and_needs_the_service(
    config: &str,
    router_i2np: &str,
) -> Option<String> {
    let code = code_view(config);

    let Some(default) = item_body(&code, "const fn default_ssu2_relay_requester() -> bool") else {
        return Some("the requester's default is absent".to_owned());
    };
    if !squeeze(&default).contains("false") {
        return Some("the requester must be off by default".to_owned());
    }
    if !code.contains("\"ssu2.relay_requester\"") {
        return Some("the refusal must name the requester's key".to_owned());
    }
    if !squeeze(&code).contains("if raw.relay_requester && !raw.enabled") {
        return Some("the requester must be refused without the SSU2 service".to_owned());
    }

    let mapping = code_view(router_i2np);
    let Some(runtime_config) = item_body(&mapping, "pub fn ssu2_runtime_config_from(") else {
        return Some("the daemon's SSU2 runtime mapping is absent".to_owned());
    };
    if !squeeze(&runtime_config).contains("relay_requester: config.relay_requester,") {
        return Some("the daemon must map the requester's opt-in into the runtime".to_owned());
    }
    None
}

// ---- 5. the transport's state machines and the model are untouched ---

fn check_the_transport_is_untouched(relay: &str, reachability: &str) -> Option<String> {
    let code = code_view(relay);
    let Some(start) = item_body(&code, "pub fn start(") else {
        return Some("the transport's requester start is absent".to_owned());
    };
    let start = squeeze(&start);
    // The transport's refusal order is the transport's own: the nonce and
    // tag's zero-ness, then the duplicate correlation, then the global
    // ceiling, then the per-peer quota.
    let zero = start.find("if nonce == 0 || tag == 0");
    let duplicate = start.find("self.entries.contains_key(&nonce)");
    let global = start.find("MAX_RELAY_REQUESTS_GLOBAL");
    let per_peer = start.find("MAX_RELAY_REQUESTS_PER_PEER");
    let (Some(zero), Some(duplicate), Some(global), Some(per_peer)) =
        (zero, duplicate, global, per_peer)
    else {
        return Some("the transport's requester refusals are absent".to_owned());
    };
    if !(zero < duplicate && duplicate < global && global < per_peer) {
        return Some("the transport's refusal order is changed".to_owned());
    }
    if !code.contains("pub fn build_hole_punch(") || !code.contains("pub fn parse_hole_punch(") {
        return Some("the transport's hole-punch codec is absent".to_owned());
    }

    let model = code_view(reachability);
    if !model.contains("pub const MIN_CORROBORATION_FLOOR: usize = 2;") {
        return Some("the corroboration floor is moved".to_owned());
    }
    if !model.contains("Self::RelayFirewalledSignal { .. } => false,") {
        return Some("the relay-firewalled signal must never support reachability".to_owned());
    }
    if !model.contains("Self::RelayFirewalledSignal { .. } => true,") {
        return Some("the relay-firewalled signal must contradict reachability".to_owned());
    }
    if !model.contains("Self::RelayFirewalledSignal { .. } => 4,") {
        return Some("the relay-firewalled signal's corroboration class is moved".to_owned());
    }
    None
}

// ---- 6. the relay-firewalled fact is a contradiction ------------------

fn check_the_firewalled_fact_is_a_contradiction(
    owner: &str,
    runtime: &str,
    reachability: &str,
) -> Option<String> {
    let code = code_view(owner);
    if code.contains("ReachabilitySignal::") {
        return Some("the daemon's owner must not write a reachability signal".to_owned());
    }
    // The tracker needle is **split on purpose**. The m12 boundary checker
    // scans every crate for the tracker call outside the runtime's own
    // owner, and this guard's own source must not read as a call site. The
    // rule the guard enforces is unchanged: a daemon that reaches into the
    // tracker is still reported.
    let tracker_call = concat!(".reachability", ".record(");
    if code.contains("state.reachability") || code.contains(tracker_call) {
        return Some("the daemon's owner must not touch the reachability tracker".to_owned());
    }

    let Some(punch) = item_body(&code, "pub fn on_hole_punch(") else {
        return Some("the hole-punch handler is absent".to_owned());
    };
    if !compact(&punch).contains("self.runtime.note_relay_firewalled(") {
        return Some("a completed exchange must record the relay-firewalled fact".to_owned());
    }

    let runtime_code = code_view(runtime);
    let Some(recorder) = item_body(&runtime_code, "pub fn note_relay_firewalled(") else {
        return Some("the runtime's relay-firewalled recorder is absent".to_owned());
    };
    if !compact(&recorder).contains("ReachabilitySignal::RelayFirewalledSignal") {
        return Some("the runtime's recorder must record the transport's own signal".to_owned());
    }

    let model = code_view(reachability);
    if !model.contains("Self::RelayFirewalledSignal { .. } => false,") {
        return Some("the relay-firewalled signal must never support reachability".to_owned());
    }
    if !model.contains("Self::RelayFirewalledSignal { .. } => true,") {
        return Some("the relay-firewalled signal must contradict reachability".to_owned());
    }
    None
}

// ---- 7. no configuration key beside the one opt-in -------------------

fn check_no_other_configuration_key(config: &str, owner: &str) -> Option<String> {
    let code = code_view(config);
    let Some(surface) = item_body(&code, "struct RawSsu2Config") else {
        return Some("the SSU2 configuration surface is absent".to_owned());
    };
    if surface.contains("HashMap") || surface.contains("#[serde(flatten)]") {
        return Some("the configuration must gain no free-form key".to_owned());
    }
    let requester_keys = surface
        .lines()
        .map(|line| line.trim())
        .filter(|line| line.contains("relay_requester") && !line.starts_with("#["))
        .count();
    if requester_keys != 1 {
        return Some("the requester's configuration must be exactly one key".to_owned());
    }

    let owner_code = code_view(owner);
    for forbidden in ["std::env", "config::"] {
        if owner_code.contains(forbidden) {
            return Some(format!("the owner must not read {forbidden}"));
        }
    }
    None
}

// ---- 8. the owner is constructed and driven on the existing tick ------

fn check_the_owner_is_wired_to_the_existing_tick(composition: &str, drain: &str) -> Option<String> {
    let code = code_view(composition);
    if !code.contains("crate::relay_requester::RelayRequesterOwner::new(") {
        return Some("the daemon must construct the requester owner".to_owned());
    }
    if !code.contains("ssu2_config.relay_requester") {
        return Some("the owner must be gated on the operator's opt-in".to_owned());
    }

    let Some(tick) = item_body(&code, "_ = relay_tick.tick() =>") else {
        return Some("the daemon's relay tick is absent".to_owned());
    };
    let tick = squeeze(&tick);
    if !tick.contains("crate::relay_requester::drive_relay_request(") {
        return Some("the existing tick must drive the requester's outbound step".to_owned());
    }
    if !tick.contains("crate::ssu2_relay_drain::drain_requester_events(") {
        return Some("the existing tick must drain the requester's inbound handoff".to_owned());
    }

    let drain_code = code_view(drain);
    let Some(drainer) = item_body(&drain_code, "pub fn drain_requester_events(") else {
        return Some("the requester's drain is absent".to_owned());
    };
    let drainer = compact(&drainer);
    if !drainer.contains("owner.on_relay_response(") {
        return Some("the drain must hand the response to the requester owner".to_owned());
    }
    if !drainer.contains("owner.on_hole_punch(") {
        return Some("the drain must hand the hole punch to the requester owner".to_owned());
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted.
// ----------------------------------------------------------------------

#[test]
fn the_owner_is_the_daemons_and_the_mechanics_are_the_runtimes() {
    let reason = check_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(
        &source_of(DAEMON_REQUESTER_MODULE),
        &source_of(RUNTIME_MODULE),
        &source_of(RUNTIME_PEER_RELAY_MODULE),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_owner_adds_no_infrastructure() {
    let reason = check_the_owner_adds_no_infrastructure(&source_of(DAEMON_REQUESTER_MODULE));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_request_is_started_only_from_validated_material() {
    let reason = check_the_request_is_started_only_from_validated_material(&source_of(
        DAEMON_REQUESTER_MODULE,
    ));
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_opt_in_defaults_off_and_needs_the_service() {
    let reason = check_the_opt_in_defaults_off_and_needs_the_service(
        &source_of(DAEMON_CONFIG),
        &source_of(DAEMON_ROUTER_I2NP),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_transports_state_machines_and_model_are_untouched() {
    let reason = check_the_transport_is_untouched(
        &source_of(TRANSPORT_RELAY_MODULE),
        &source_of(TRANSPORT_REACHABILITY_MODULE),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_relay_firewalled_fact_is_a_contradiction() {
    let reason = check_the_firewalled_fact_is_a_contradiction(
        &source_of(DAEMON_REQUESTER_MODULE),
        &source_of(RUNTIME_MODULE),
        &source_of(TRANSPORT_REACHABILITY_MODULE),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn no_configuration_key_besides_the_one_opt_in() {
    let reason = check_no_other_configuration_key(
        &source_of(DAEMON_CONFIG),
        &source_of(DAEMON_REQUESTER_MODULE),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_owner_is_wired_to_the_existing_tick() {
    let reason = check_the_owner_is_wired_to_the_existing_tick(
        &source_of(DAEMON_COMPOSITION),
        &source_of(DAEMON_DRAIN),
    );
    assert_eq!(reason, None, "{reason:?}");
}

// ----------------------------------------------------------------------
// The inverted probes. Each one mutates an in-memory copy, so no file on
// disk is written.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_an_owner_that_does_not_use_the_runtime_seam() {
    let mut owner = source_of(DAEMON_REQUESTER_MODULE);
    owner = owner.replace("start_relay_request(", "start_request(");
    let reason = check_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(
        &owner,
        &source_of(RUNTIME_MODULE),
        &source_of(RUNTIME_PEER_RELAY_MODULE),
    );
    assert!(
        reason.is_some(),
        "an owner that re-implements the request must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_daemon_side_requester_table() {
    let mut owner = source_of(DAEMON_REQUESTER_MODULE);
    owner = owner.replace(
        "pub struct RelayRequesterOwner {",
        "pub struct RelayRequester { entries: BTreeMap<u32, u32> }\npub struct RelayRequesterOwner {",
    );
    let reason = check_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(
        &owner,
        &source_of(RUNTIME_MODULE),
        &source_of(RUNTIME_PEER_RELAY_MODULE),
    );
    assert!(
        reason.is_some(),
        "a daemon-side requester table must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_owns_a_socket() {
    let mut owner = source_of(DAEMON_REQUESTER_MODULE);
    owner = owner.replace(
        "use std::collections::{BTreeMap, BTreeSet};",
        "use std::collections::{BTreeMap, BTreeSet};\nuse tokio::net::UdpSocket;",
    );
    let reason = check_the_owner_adds_no_infrastructure(&owner);
    assert!(
        reason.is_some(),
        "an owner that owns a socket must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_spawns_a_task() {
    let mut owner = source_of(DAEMON_REQUESTER_MODULE);
    owner = owner.replace("pub fn new(", "fn spawn() {}\n    pub fn new(");
    let reason = check_the_owner_adds_no_infrastructure(&owner);
    assert!(
        reason.is_some(),
        "an owner that spawns a task must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_read_that_leaves_the_validated_store() {
    let mut owner = source_of(DAEMON_REQUESTER_MODULE);
    owner = owner.replace("store.get(&key)", "self.anything.get(&key)");
    let reason = check_the_request_is_started_only_from_validated_material(&owner);
    assert!(
        reason.is_some(),
        "a read that leaves the validated store must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_request_that_is_not_signed_over_the_preimage() {
    let mut owner = source_of(DAEMON_REQUESTER_MODULE);
    owner = owner.replace("relay_request_preimage(", "some_other_preimage(");
    let reason = check_the_request_is_started_only_from_validated_material(&owner);
    assert!(
        reason.is_some(),
        "a request signed over another preimage must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_opt_in_that_defaults_on() {
    let mut config = source_of(DAEMON_CONFIG);
    config = config.replace(
        "const fn default_ssu2_relay_requester() -> bool {\n    false\n}",
        "const fn default_ssu2_relay_requester() -> bool {\n    true\n}",
    );
    let reason = check_the_opt_in_defaults_off_and_needs_the_service(
        &config,
        &source_of(DAEMON_ROUTER_I2NP),
    );
    assert!(
        reason.is_some(),
        "an opt-in that defaults on must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_opt_in_that_runs_without_the_service() {
    let mut config = source_of(DAEMON_CONFIG);
    config = config.replace(
        "if raw.relay_requester && !raw.enabled {",
        "if false && !raw.enabled {",
    );
    let reason = check_the_opt_in_defaults_off_and_needs_the_service(
        &config,
        &source_of(DAEMON_ROUTER_I2NP),
    );
    assert!(
        reason.is_some(),
        "an opt-in that runs without the service must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_unmapped_opt_in() {
    let mut router_i2np = source_of(DAEMON_ROUTER_I2NP);
    router_i2np = router_i2np.replace(
        "relay_requester: config.relay_requester,",
        "relay_requester: false,",
    );
    let reason = check_the_opt_in_defaults_off_and_needs_the_service(
        &source_of(DAEMON_CONFIG),
        &router_i2np,
    );
    assert!(reason.is_some(), "an unmapped opt-in must fail the rule");
}

#[test]
fn the_guard_reports_a_changed_transport_refusal_order() {
    let mut relay = source_of(TRANSPORT_RELAY_MODULE);
    relay = relay.replace(
        "        if self.entries.contains_key(&nonce) {\n            return Err(RelayError::DuplicateCorrelation);\n        }\n",
        "        if self.entries.len() >= MAX_RELAY_REQUESTS_GLOBAL {\n            return Err(RelayError::TooManyRequests);\n        }\n        if self.entries.contains_key(&nonce) {\n            return Err(RelayError::DuplicateCorrelation);\n        }\n",
    );
    let reason =
        check_the_transport_is_untouched(&relay, &source_of(TRANSPORT_REACHABILITY_MODULE));
    assert!(
        reason.is_some(),
        "a changed refusal order must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_moved_corroboration_class() {
    let mut reachability = source_of(TRANSPORT_REACHABILITY_MODULE);
    reachability = reachability.replace(
        "Self::RelayFirewalledSignal { .. } => 4,",
        "Self::RelayFirewalledSignal { .. } => 3,",
    );
    let reason =
        check_the_transport_is_untouched(&source_of(TRANSPORT_RELAY_MODULE), &reachability);
    assert!(
        reason.is_some(),
        "a moved corroboration class must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_signal_that_supports_reachability() {
    let mut reachability = source_of(TRANSPORT_REACHABILITY_MODULE);
    reachability = reachability.replace(
        "Self::RelayFirewalledSignal { .. } => false,",
        "Self::RelayFirewalledSignal { .. } => true,",
    );
    let reason = check_the_firewalled_fact_is_a_contradiction(
        &source_of(DAEMON_REQUESTER_MODULE),
        &source_of(RUNTIME_MODULE),
        &reachability,
    );
    assert!(
        reason.is_some(),
        "a signal that supports reachability must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_writes_a_reachability_signal() {
    let mut owner = source_of(DAEMON_REQUESTER_MODULE);
    owner = owner.replace(
        "use i2pr_transport::{AddressFamily, PeerId};",
        "use i2pr_transport::{AddressFamily, PeerId, ReachabilitySignal};\n    let _probe = ReachabilitySignal::RelayFirewalledSignal { family: AddressFamily::Ipv4 };",
    );
    let reason = check_the_firewalled_fact_is_a_contradiction(
        &owner,
        &source_of(RUNTIME_MODULE),
        &source_of(TRANSPORT_REACHABILITY_MODULE),
    );
    assert!(
        reason.is_some(),
        "a daemon that writes a reachability signal must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_completed_exchange_that_records_nothing() {
    let mut owner = source_of(DAEMON_REQUESTER_MODULE);
    owner = owner.replace(
        "self.runtime.note_relay_firewalled(entry.context.family);",
        "",
    );
    let reason = check_the_firewalled_fact_is_a_contradiction(
        &owner,
        &source_of(RUNTIME_MODULE),
        &source_of(TRANSPORT_REACHABILITY_MODULE),
    );
    assert!(
        reason.is_some(),
        "a completed exchange that records nothing must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_second_configuration_key() {
    let mut config = source_of(DAEMON_CONFIG);
    config = config.replace(
        "    relay_requester: bool,",
        "    relay_requester: bool,\n    relay_requester_extra: bool,",
    );
    let reason = check_no_other_configuration_key(&config, &source_of(DAEMON_REQUESTER_MODULE));
    assert!(
        reason.is_some(),
        "a second configuration key must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_is_never_driven() {
    let mut composition = source_of(DAEMON_COMPOSITION);
    composition = composition.replace(
        "crate::relay_requester::drive_relay_request(",
        "crate::relay_requester::unused_drive(",
    );
    let reason =
        check_the_owner_is_wired_to_the_existing_tick(&composition, &source_of(DAEMON_DRAIN));
    assert!(
        reason.is_some(),
        "an owner that is never driven must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_owner_that_is_never_constructed() {
    let mut composition = source_of(DAEMON_COMPOSITION);
    composition = composition.replace(
        "crate::relay_requester::RelayRequesterOwner::new(",
        "crate::relay_requester::UnusedOwner::new(",
    );
    let reason =
        check_the_owner_is_wired_to_the_existing_tick(&composition, &source_of(DAEMON_DRAIN));
    assert!(
        reason.is_some(),
        "an owner that is never constructed must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_drain_that_drops_the_hole_punch() {
    let mut drain = source_of(DAEMON_DRAIN);
    drain = drain.replace("owner.on_hole_punch(", "drop_hole_punch(");
    let reason =
        check_the_owner_is_wired_to_the_existing_tick(&source_of(DAEMON_COMPOSITION), &drain);
    assert!(
        reason.is_some(),
        "a drain that drops the hole punch must fail the rule"
    );
}
