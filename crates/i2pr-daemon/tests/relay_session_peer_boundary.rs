//! Plan 534's guard -- the router must build a tunnel toward the peer it
//! reached through the relay.
//!
//! Plan 533 gave the requester an authenticated session with the peer it
//! reached through a relay. What the router still could not do was **use**
//! that peer: the tunnel-build route drew its peer from the peer-profile
//! store's policy selection, so a peer the router had deliberately reached
//! through a relay was one candidate among every peer the router knew, and
//! the deliberate reach was dropped on the floor.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the route still selects through the **version-gated** selection, and a
//!    named target is gated by the **same** function;
//! 2. the driver owns no task, no timer, no listener and no socket;
//! 3. the observation table is **bounded**, and every full-table refusal is
//!    counted;
//! 4. the material is read from the **validated** store, and unvalidated
//!    bytes are never parsed;
//! 5. the peer profile is not written for a **local** condition, and no
//!    reachability signal is written;
//! 6. the build goes through the **existing** coordinator, and no second
//!    build path exists;
//! 7. the observation is **consumed**, and it **expires**;
//! 8. no configuration key is added;
//! 9. the transport's relay state machines and the reachability model are
//!    untouched.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written; every rule has a
//! probe that removes the item it names so it **fails closed**; and every
//! rule reports a missing item rather than passing.

#![forbid(unsafe_code)]

const DAEMON_DRIVER: &str = "crates/i2pr-daemon/src/exploratory_driver.rs";
const DAEMON_TABLE: &str = "crates/i2pr-daemon/src/relay_session_peer.rs";
const DAEMON_TASK: &str = "crates/i2pr-daemon/src/relay_establishment_task.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const DAEMON_COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const DAEMON_INSPECTION: &str = "crates/i2pr-daemon/src/i2pcontrol_inspection.rs";
const NETDB_SELECTION: &str = "crates/i2pr-netdb/src/selection.rs";
const NETDB_PROFILE: &str = "crates/i2pr-netdb/src/profile.rs";
const NETDB_PROFILE_STORE: &str = "crates/i2pr-netdb/src/profile_store.rs";
const TRANSPORT_RELAY: &str = "crates/i2pr-transport-ssu2/src/relay.rs";

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

/// Returns the byte range of the block that belongs to the definition
/// anchored at `needle`.
fn block_range(code: &str, needle: &str) -> Option<(usize, usize)> {
    let start = find_definition(code, needle)?;
    brace_balanced_block(code, start + needle.len())
}

/// Extracts the body of an item that is declared **inside** another item.
///
/// The trait declares the seam and the implementation carries the body, so a
/// rule that reads the implementation must look inside the implementation's
/// own block rather than at the first textual match.
fn item_body_in(code: &str, scope: &str, needle: &str) -> Option<String> {
    let (open, close) = block_range(code, scope)?;
    let inner = &code[open..close];
    let start = find_definition(inner, needle)?;
    let (body_open, body_close) = brace_balanced_block(inner, start + needle.len())?;
    Some(inner[body_open + 1..body_close - 1].to_owned())
}

/// The sources the rules read.
///
/// Every rule and every probe works on one of these, so a probe mutates
/// exactly one source and nothing else.
struct Sources {
    daemon_driver: String,
    daemon_table: String,
    daemon_task: String,
    daemon_config: String,
    daemon_composition: String,
    daemon_inspection: String,
    netdb_selection: String,
    netdb_profile: String,
    netdb_profile_store: String,
    transport_relay: String,
}

impl Sources {
    /// Reads every source from the real tree.
    fn from_tree() -> Self {
        Self {
            daemon_driver: source_of(DAEMON_DRIVER),
            daemon_table: source_of(DAEMON_TABLE),
            daemon_task: source_of(DAEMON_TASK),
            daemon_config: source_of(DAEMON_CONFIG),
            daemon_composition: source_of(DAEMON_COMPOSITION),
            daemon_inspection: source_of(DAEMON_INSPECTION),
            netdb_selection: source_of(NETDB_SELECTION),
            netdb_profile: source_of(NETDB_PROFILE),
            netdb_profile_store: source_of(NETDB_PROFILE_STORE),
            transport_relay: source_of(TRANSPORT_RELAY),
        }
    }
}

// ---- 1. the route selects through the gate, and a named target passes the
//         same gate

fn rule_the_route_selects_through_the_gate_and_a_named_target_passes_the_same_gate(
    sources: &Sources,
) -> Option<String> {
    let driver_code = code_view(&sources.daemon_driver);
    // The drawn candidate still comes from the version-gated selection, and
    // it comes from there exactly once: a second path around the gate would
    // be an ungated candidate.
    let selection_calls = driver_code.matches("select_tunnel_build_peers(").count();
    if selection_calls != 1 {
        return Some(format!(
            "the driver must select through the version-gated selection exactly once, found {selection_calls}"
        ));
    }
    let Some(select) = item_body(&driver_code, "fn select_peer") else {
        return Some("the driver's peer selection is absent".to_owned());
    };
    if !compact(&select).contains("select_tunnel_build_peers(1,seed)") {
        return Some("the drawn candidate must come from the version-gated selection".to_owned());
    }
    // The named target is gated by the **same** function the drawn candidate
    // passes.
    let Some(gate) = item_body(&driver_code, "fn relay_target_eligibility") else {
        return Some("the named target's gate is absent".to_owned());
    };
    let gate = compact(&gate);
    if !gate.contains("tunnel_peer_eligibility(") {
        return Some("the named target must pass the same version gate".to_owned());
    }
    if !gate.contains("published_version_of(") {
        return Some("the named target's version must come from the validated record".to_owned());
    }
    // The gate's verdict is carried faithfully: a named target below the
    // floor is not turned into an eligible one.
    if !gate.contains("VersionEligibility::BelowFloor{..}=>RelayTargetEligibility::BelowFloor,") {
        return Some("the named target's verdict must be carried faithfully".to_owned());
    }
    // The selection itself keeps using that gate.
    let selection_code = code_view(&sources.netdb_selection);
    if !selection_code.contains("tunnel_peer_eligibility(") {
        return Some("the version-gated selection must keep using the shared gate".to_owned());
    }
    None
}

// ---- 2. the driver owns no task, no timer, no listener and no socket ------

fn rule_the_driver_owns_no_task_no_timer_no_listener_and_no_socket(
    sources: &Sources,
) -> Option<String> {
    let driver_code = code_view(&sources.daemon_driver);
    let Some(driver) = item_body(&driver_code, "pub struct ExploratoryDriver") else {
        return Some("the exploratory driver is absent".to_owned());
    };
    for forbidden in [
        "UdpSocket",
        "TcpListener",
        "TcpStream",
        "JoinHandle",
        "tokio::",
        "spawn(",
        "interval(",
        "sleep(",
    ] {
        if driver.contains(forbidden) {
            return Some(format!("the driver must own no {forbidden}"));
        }
    }
    // The observation table owns none either: it is a value, not a service.
    let table_code = code_view(&sources.daemon_table);
    let Some(owner) = item_body(&table_code, "pub struct RelaySessionPeerOwner") else {
        return Some("the local observation table is absent".to_owned());
    };
    for forbidden in [
        "UdpSocket",
        "TcpListener",
        "TcpStream",
        "JoinHandle",
        "tokio::",
        "spawn(",
    ] {
        if owner.contains(forbidden) {
            return Some(format!("the observation table must own no {forbidden}"));
        }
    }
    None
}

// ---- 3. the observation table is bounded, and a full table is counted ----

fn rule_the_observation_table_is_bounded_and_every_full_table_refusal_is_counted(
    sources: &Sources,
) -> Option<String> {
    let table_code = code_view(&sources.daemon_table);
    let Some(declaration) = table_code
        .lines()
        .find(|line| line.contains("pub const MAX_RELAY_SESSION_PEERS:"))
    else {
        return Some("the observation table must have a stated capacity".to_owned());
    };
    // The capacity is a literal bound: an unbounded or computed capacity is
    // not a bound.
    let Some((_, value)) = declaration.split_once('=') else {
        return Some("the observation table's capacity must be a literal bound".to_owned());
    };
    let value = value.trim().trim_end_matches(';').trim();
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return Some("the observation table's capacity must be a literal bound".to_owned());
    }
    let Some(record) = item_body(
        &table_code,
        "pub fn record(&mut self, peer: RouterHash, now_ms: u64)",
    ) else {
        return Some("the observation table's record step is absent".to_owned());
    };
    let record = compact(&record);
    if !record.contains("self.peers.len()>=MAX_RELAY_SESSION_PEERS") {
        return Some("the observation table must be bounded".to_owned());
    }
    if !record.contains("RelaySessionPeerRefusal::TableFull") {
        return Some("a full table must be refused under its own reason".to_owned());
    }
    // The refusal is counted, not swallowed.
    let Some(refuse) = item_body(
        &table_code,
        "fn refuse(&mut self, reason: RelaySessionPeerRefusal)",
    ) else {
        return Some("the observation table's refusal step is absent".to_owned());
    };
    if !compact(&refuse).contains("self.counters.refusals_table_full+=1") {
        return Some("a full-table refusal must be counted".to_owned());
    }
    // The counts reach the operator through the inspection seam, and the
    // seam exposes the counts only.
    let inspection = code_view(&sources.daemon_inspection);
    if !inspection.contains("pub fn publish_relay_session_peers(") {
        return Some("the observation table must be publishable to the inspection seam".to_owned());
    }
    if !inspection.contains("pub fn relay_session_peer_counters_snapshot(") {
        return Some("the observation counts must reach the operator's inspection seam".to_owned());
    }
    None
}

// ---- 4. the material is read from the validated store --------------------

fn rule_the_material_is_read_from_the_validated_store(sources: &Sources) -> Option<String> {
    let driver_code = code_view(&sources.daemon_driver);
    let Some(gate) = item_body(&driver_code, "fn relay_target_eligibility") else {
        return Some("the named target's gate is absent".to_owned());
    };
    let gate = compact(&gate);
    if !gate.contains("profiles.store().peers().get(&peer)?") {
        return Some("the named target's material must come from the validated store".to_owned());
    }
    // The material is never parsed from unvalidated bytes.
    for forbidden in [
        "RouterInfo::decode",
        "decode_from_bytes",
        "parse_unvalidated",
    ] {
        if gate.contains(forbidden) {
            return Some(format!("the named target must never parse {forbidden}"));
        }
    }
    // The build's own material comes through the resolver, which reads the
    // very same validated store.
    let Some(resolve) = item_body_in(
        &driver_code,
        "impl PeerBuildIdentityResolver for ProfileStorePeerResolver",
        "fn resolve(&self, peer: &RouterHash)",
    ) else {
        return Some("the driver's peer-material resolver is absent".to_owned());
    };
    let resolve = compact(&resolve);
    if !resolve.contains("profiles.store().peers().get(peer)") {
        return Some("the build's material must come from the validated store".to_owned());
    }
    if resolve.contains("RouterInfo::decode") {
        return Some("the resolver must never parse unvalidated bytes".to_owned());
    }
    None
}

// ---- 5. no profile write and no reachability claim -----------------------

fn rule_the_peer_profile_is_not_written_and_no_reachability_is_claimed(
    sources: &Sources,
) -> Option<String> {
    let table_code = code_view(&sources.daemon_table);
    if item_body(&table_code, "pub struct RelaySessionPeerOwner").is_none() {
        return Some("the local observation table is absent".to_owned());
    }
    // The observation is a **local** fact: it writes nothing into a peer's
    // record and claims nothing about the peer's reachability.
    for forbidden in [
        "observe_answered",
        "observe_unreachable",
        "observe_refused",
        "record_observation",
        "note_relay_firewalled",
        "reachability",
    ] {
        if table_code.contains(forbidden) {
            return Some(format!("the observation table must write no {forbidden}"));
        }
    }
    // The route's own selection writes no profile: a local condition is
    // counted and the peer's record is left alone.
    let driver_code = code_view(&sources.daemon_driver);
    let Some(select) = item_body(&driver_code, "fn select_peer") else {
        return Some("the driver's peer selection is absent".to_owned());
    };
    if select.contains("record_observation") {
        return Some("the route's selection must write no peer observation".to_owned());
    }
    // The establishment's recording seam writes no profile either.
    let task_code = code_view(&sources.daemon_task);
    let Some(record) = item_body(&task_code, "pub fn record_established_peer(") else {
        return Some("the establishment's recording seam is absent".to_owned());
    };
    for forbidden in ["observe_", "record_observation", "note_relay_firewalled"] {
        if record.contains(forbidden) {
            return Some(format!("the recording seam must write no {forbidden}"));
        }
    }
    None
}

// ---- 6. the build goes through the existing coordinator ------------------

fn rule_the_build_goes_through_the_existing_coordinator(sources: &Sources) -> Option<String> {
    let driver_code = code_view(&sources.daemon_driver);
    let Some(drive) = item_body(&driver_code, "pub fn drive<R>") else {
        return Some("the driver's drive step is absent".to_owned());
    };
    if !compact(&drive).contains("coordinator.rebuild_exploratory(") {
        return Some("the build must go through the existing coordinator".to_owned());
    }
    // There is exactly one build path: a second submission would be a second
    // path around the coordinator's own bounds.
    let submissions = driver_code.matches("rebuild_exploratory(").count();
    if submissions != 1 {
        return Some(format!(
            "the driver must have exactly one build path, found {submissions}"
        ));
    }
    // The driver builds no message of its own: the coordinator owns the
    // build, the bridge and the pool.
    for forbidden in [
        "ShortBuildI2npBridge::new(",
        "I2npMessage::",
        "submit_build(",
    ] {
        if driver_code.contains(forbidden) {
            return Some(format!(
                "the driver must own no second build path: {forbidden}"
            ));
        }
    }
    None
}

// ---- 7. the observation is consumed, and it expires ----------------------

fn rule_the_observation_is_consumed_and_it_expires(sources: &Sources) -> Option<String> {
    let table_code = code_view(&sources.daemon_table);
    let Some(take) = item_body(&table_code, "pub fn take(&mut self, now_ms: u64)") else {
        return Some("the observation table's take step is absent".to_owned());
    };
    let take = compact(&take);
    // Taking is consuming: the observation is removed.
    if !take.contains("self.peers.remove(&oldest);") {
        return Some("the observation must be consumed by the route that used it".to_owned());
    }
    // Expired observations are dropped first, and the drop is counted.
    if !take.contains("self.expire(now_ms);") {
        return Some("an expired observation must never be taken".to_owned());
    }
    let Some(expire) = item_body(&table_code, "fn expire(&mut self, now_ms: u64)") else {
        return Some("the observation table's expiry step is absent".to_owned());
    };
    let expire = compact(&expire);
    if !expire.contains("RELAY_SESSION_PEER_TTL_MS") {
        return Some("the expiry must be bounded by the table's own lifetime".to_owned());
    }
    if !expire.contains("self.counters.observations_expired+=1;") {
        return Some("an expired observation must be counted".to_owned());
    }
    // The route takes the observation from the shared table.
    let driver_code = code_view(&sources.daemon_driver);
    let Some(take_peer) = item_body(&driver_code, "fn take_relay_session_peer") else {
        return Some("the route's take step is absent".to_owned());
    };
    if !compact(&take_peer).contains("crate::relay_session_peer::take_relay_session_peer(") {
        return Some("the route must take the observation from the shared table".to_owned());
    }
    // The observation exists because the router reached the peer, so the
    // establishment's own task records it when the handshake succeeds.
    let task_code = code_view(&sources.daemon_task);
    let Some(run) = item_body(&task_code, "pub async fn run_relay_establishment_task") else {
        return Some("the establishment's task is absent".to_owned());
    };
    let run = compact(&run);
    if !run.contains("RelayEstablishmentOutcome::Established") {
        return Some("the establishment's task must observe the established outcome".to_owned());
    }
    if !run.contains("record_established_peer(") {
        return Some("the establishment's task must record the observation".to_owned());
    }
    None
}

// ---- 8. no configuration key is added ------------------------------------

fn rule_no_configuration_key_is_added(sources: &Sources) -> Option<String> {
    let config = code_view(&sources.daemon_config);
    if !config.contains("relay_requester: bool,") || !config.contains("relay_target: bool,") {
        return Some("the two relay opt-ins must stay".to_owned());
    }
    // The observation table rides the requester's own opt-in; it adds no
    // key of its own.
    if config.contains("relay_session") {
        return Some("the observation table must add no configuration key of its own".to_owned());
    }
    let composition = compact(&code_view(&sources.daemon_composition));
    if !composition.contains("letrelay_sessions=ifssu2_config.relay_requester{") {
        return Some("the observation table must ride the requester's own opt-in".to_owned());
    }
    None
}

// ---- 9. the transport and the reachability model are untouched -----------

fn rule_the_transports_relay_machines_and_the_reachability_model_are_untouched(
    sources: &Sources,
) -> Option<String> {
    let relay = code_view(&sources.transport_relay);
    for needle in [
        "pub fn on_hole_punch(",
        "pub fn on_response(",
        "pub fn on_intro(",
    ] {
        if !relay.contains(needle) {
            return Some(format!("the transport's relay seam {needle} is absent"));
        }
    }
    // The transport's wrong-role rule is intact: the requester table
    // refuses a hole punch before its accept.
    if !relay.contains("if entry.state != RequesterState::AwaitingHolePunch {") {
        return Some("the transport's wrong-role rule must stay".to_owned());
    }
    // The transport knows nothing about the local observation table.
    if relay.contains("relay_session") {
        return Some("the transport must know nothing about the observation table".to_owned());
    }
    // The reachability model is untouched: the peer profile gains no state
    // for a peer this router reached through a relay.
    let profile = code_view(&sources.netdb_profile);
    if profile.contains("relay") {
        return Some("the peer profile must gain no relayed state".to_owned());
    }
    let profile_store = code_view(&sources.netdb_profile_store);
    for needle in [
        "pub fn observe_answered(",
        "pub fn observe_unreachable(",
        "pub fn observe_refused(",
    ] {
        if !profile_store.contains(needle) {
            return Some(format!("the reachability model's {needle} is absent"));
        }
    }
    if profile_store.contains("relay") {
        return Some("the reachability model must gain no relayed state".to_owned());
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted.
// ----------------------------------------------------------------------

#[test]
fn the_route_selects_through_the_gate_and_a_named_target_passes_the_same_gate() {
    let reason = rule_the_route_selects_through_the_gate_and_a_named_target_passes_the_same_gate(
        &Sources::from_tree(),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_driver_owns_no_task_no_timer_no_listener_and_no_socket() {
    let reason =
        rule_the_driver_owns_no_task_no_timer_no_listener_and_no_socket(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_observation_table_is_bounded_and_every_full_table_refusal_is_counted() {
    let reason = rule_the_observation_table_is_bounded_and_every_full_table_refusal_is_counted(
        &Sources::from_tree(),
    );
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_material_is_read_from_the_validated_store() {
    let reason = rule_the_material_is_read_from_the_validated_store(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_peer_profile_is_not_written_and_no_reachability_is_claimed() {
    let reason =
        rule_the_peer_profile_is_not_written_and_no_reachability_is_claimed(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_build_goes_through_the_existing_coordinator() {
    let reason = rule_the_build_goes_through_the_existing_coordinator(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_observation_is_consumed_and_it_expires() {
    let reason = rule_the_observation_is_consumed_and_it_expires(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn no_configuration_key_is_added() {
    let reason = rule_no_configuration_key_is_added(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_transports_relay_machines_and_the_reachability_model_are_untouched() {
    let reason = rule_the_transports_relay_machines_and_the_reachability_model_are_untouched(
        &Sources::from_tree(),
    );
    assert_eq!(reason, None, "{reason:?}");
}

// ----------------------------------------------------------------------
// The inverted probes. Each one mutates an in-memory copy, so no file on
// disk is written.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_a_named_target_that_bypasses_the_gate() {
    let mut sources = Sources::from_tree();
    sources.daemon_driver = sources.daemon_driver.replace(
        "tunnel_peer_eligibility(version)",
        "version_gate_bypassed(version)",
    );
    let reason =
        rule_the_route_selects_through_the_gate_and_a_named_target_passes_the_same_gate(&sources);
    assert!(
        reason.is_some(),
        "a named target that bypasses the version gate must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_driver_that_owns_a_socket() {
    let mut sources = Sources::from_tree();
    sources.daemon_driver = sources.daemon_driver.replace(
        "pub struct ExploratoryDriver {",
        "pub struct ExploratoryDriver {\n    socket: std::net::UdpSocket,",
    );
    let reason = rule_the_driver_owns_no_task_no_timer_no_listener_and_no_socket(&sources);
    assert!(
        reason.is_some(),
        "a driver that owns a socket must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_unbounded_observation_table() {
    let mut sources = Sources::from_tree();
    sources.daemon_table = sources
        .daemon_table
        .replace("self.peers.len() >= MAX_RELAY_SESSION_PEERS", "false");
    let reason =
        rule_the_observation_table_is_bounded_and_every_full_table_refusal_is_counted(&sources);
    assert!(
        reason.is_some(),
        "an unbounded observation table must fail the rule"
    );
}

#[test]
fn the_guard_reports_material_parsed_from_unvalidated_bytes() {
    let mut sources = Sources::from_tree();
    sources.daemon_driver = sources.daemon_driver.replace(
        "fn relay_target_eligibility(&self, peer: RouterHash) -> RelayTargetEligibility {",
        "fn relay_target_eligibility(&self, peer: RouterHash) -> RelayTargetEligibility {\n        let _probe = RouterInfo::decode(bytes);",
    );
    let reason = rule_the_material_is_read_from_the_validated_store(&sources);
    assert!(
        reason.is_some(),
        "material parsed from unvalidated bytes must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_profile_write_for_a_local_condition() {
    let mut sources = Sources::from_tree();
    sources.daemon_table = sources.daemon_table.replace(
        "pub fn record(&mut self, peer: RouterHash, now_ms: u64)",
        "pub fn record(&mut self, peer: RouterHash, now_ms: u64) { let _probe = observe_answered(0);",
    );
    let reason = rule_the_peer_profile_is_not_written_and_no_reachability_is_claimed(&sources);
    assert!(
        reason.is_some(),
        "a profile write for a local condition must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_second_build_path() {
    let mut sources = Sources::from_tree();
    sources.daemon_driver = sources.daemon_driver.replace(
        "coordinator.rebuild_exploratory(",
        "coordinator.submit_direct_build(",
    );
    let reason = rule_the_build_goes_through_the_existing_coordinator(&sources);
    assert!(reason.is_some(), "a second build path must fail the rule");
}

#[test]
fn the_guard_reports_an_observation_that_is_never_consumed() {
    let mut sources = Sources::from_tree();
    sources.daemon_table = sources
        .daemon_table
        .replace("self.peers.remove(&oldest);", "");
    let reason = rule_the_observation_is_consumed_and_it_expires(&sources);
    assert!(
        reason.is_some(),
        "an observation that is never consumed must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_configuration_key_for_the_observation_table() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "    relay_requester: bool,",
        "    relay_requester: bool,\n    relay_session: bool,",
    );
    let reason = rule_no_configuration_key_is_added(&sources);
    assert!(
        reason.is_some(),
        "a configuration key for the observation table must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_edited_transport_rule() {
    let mut sources = Sources::from_tree();
    sources.transport_relay = sources.transport_relay.replace(
        "if entry.state != RequesterState::AwaitingHolePunch {",
        "if false {",
    );
    let reason =
        rule_the_transports_relay_machines_and_the_reachability_model_are_untouched(&sources);
    assert!(
        reason.is_some(),
        "an edited transport rule must fail the rule"
    );
}

// ----------------------------------------------------------------------
// The fail-closed probes. Each one removes the item the rule names, so
// the rule must report the absence rather than pass.
// ----------------------------------------------------------------------

#[test]
fn the_guard_fails_closed_when_the_named_gate_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_driver = sources
        .daemon_driver
        .replace("fn relay_target_eligibility", "fn unused_named_gate");
    let reason =
        rule_the_route_selects_through_the_gate_and_a_named_target_passes_the_same_gate(&sources);
    assert!(reason.is_some(), "an absent named gate must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_driver_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_driver = sources.daemon_driver.replace(
        "pub struct ExploratoryDriver",
        "pub struct UnusedExploratoryDriver",
    );
    let reason = rule_the_driver_owns_no_task_no_timer_no_listener_and_no_socket(&sources);
    assert!(reason.is_some(), "an absent driver must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_table_bound_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_table = sources
        .daemon_table
        .replace("pub const MAX_RELAY_SESSION_PEERS: usize = 8;", "");
    let reason =
        rule_the_observation_table_is_bounded_and_every_full_table_refusal_is_counted(&sources);
    assert!(reason.is_some(), "an absent table bound must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_resolver_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_driver = sources.daemon_driver.replace(
        "impl PeerBuildIdentityResolver for ProfileStorePeerResolver",
        "impl UnusedResolver for ProfileStorePeerResolver",
    );
    let reason = rule_the_material_is_read_from_the_validated_store(&sources);
    assert!(reason.is_some(), "an absent resolver must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_observation_owner_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_table = sources.daemon_table.replace(
        "pub struct RelaySessionPeerOwner",
        "pub struct UnusedObservationOwner",
    );
    let reason = rule_the_peer_profile_is_not_written_and_no_reachability_is_claimed(&sources);
    assert!(
        reason.is_some(),
        "an absent observation owner must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_drive_step_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_driver = sources
        .daemon_driver
        .replace("pub fn drive<R>", "pub fn unused_drive<R>");
    let reason = rule_the_build_goes_through_the_existing_coordinator(&sources);
    assert!(reason.is_some(), "an absent drive step must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_take_step_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_table = sources.daemon_table.replace(
        "pub fn take(&mut self, now_ms: u64)",
        "pub fn unused_take(&mut self, now_ms: u64)",
    );
    let reason = rule_the_observation_is_consumed_and_it_expires(&sources);
    assert!(reason.is_some(), "an absent take step must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_requester_opt_in_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources
        .daemon_config
        .replace("relay_requester: bool,", "requester_opt_in: bool,");
    let reason = rule_no_configuration_key_is_added(&sources);
    assert!(reason.is_some(), "an absent opt-in must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_transport_relay_seam_is_absent() {
    let mut sources = Sources::from_tree();
    sources.transport_relay = sources
        .transport_relay
        .replace("pub fn on_hole_punch(", "pub fn unused_on_hole_punch(");
    let reason =
        rule_the_transports_relay_machines_and_the_reachability_model_are_untouched(&sources);
    assert!(
        reason.is_some(),
        "an absent transport relay seam must fail the rule"
    );
}
