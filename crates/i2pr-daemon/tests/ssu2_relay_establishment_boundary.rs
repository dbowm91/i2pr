//! Plan 533's guard -- the router must establish a session with the peer it
//! reached through the relay.
//!
//! Plan 531 gave the SSU2 relay protocol's requester side and Plan 532 gave
//! its target side, but the exchange ended at a hole punch: the accept's
//! token had **no consumer** -- the target drew it from the daemon's own
//! randomness while the only possible consumer is the target's responder,
//! whose authority is the runtime's token table -- and nothing dialled the
//! peer the router had been relayed to.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the owner and the task are the **daemon's** and the mechanics stay in
//!    the **runtime**;
//! 2. the accept's token is issued by the runtime's responder table, and the
//!    daemon draws no token of its own;
//! 3. the token is bound to the **requester's** exact source address;
//! 4. the establishment is the **ordinary** establishment path -- no
//!    relay-specific session and no bypass of the responder's verification;
//! 5. the dial target's keys come from the **validated** store through the
//!    transport's strict parser;
//! 6. the dial target obeys the **qualification profile**;
//! 7. the deferred punch is bounded and re-driven, and the transport's
//!    wrong-role refusal is the **deferral signal**, not a refusal;
//! 8. the handoff to the task is bounded, and a full handoff is never a
//!    silent drop;
//! 9. the daemon's `select!` loop never awaits a handshake on the relay
//!    tick;
//! 10. the configuration surface gains **no** key;
//! 11. the daemon writes **no** reachability signal;
//! 12. the transport's relay state machines and its token table are
//!     untouched.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, every rule has a
//! probe that removes the item it names so it **fails closed**, and every
//! rule reports a missing item rather than passing.

#![forbid(unsafe_code)]

const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const TRANSPORT_RELAY_MODULE: &str = "crates/i2pr-transport-ssu2/src/relay.rs";
const TRANSPORT_TOKEN_MODULE: &str = "crates/i2pr-transport-ssu2/src/token.rs";
const DAEMON_ESTABLISHMENT_MODULE: &str = "crates/i2pr-daemon/src/relay_establishment.rs";
const DAEMON_ESTABLISHMENT_TASK_MODULE: &str = "crates/i2pr-daemon/src/relay_establishment_task.rs";
const DAEMON_REQUESTER_MODULE: &str = "crates/i2pr-daemon/src/relay_requester.rs";
const DAEMON_TARGET_MODULE: &str = "crates/i2pr-daemon/src/relay_target.rs";
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

/// The sources the rules read.
///
/// Every rule and every probe works on one of these, so a probe mutates
/// exactly one source and nothing else.
struct Sources {
    daemon_establishment: String,
    daemon_establishment_task: String,
    daemon_requester: String,
    daemon_target: String,
    daemon_config: String,
    daemon_router_i2np: String,
    daemon_composition: String,
    daemon_drain: String,
    runtime: String,
    transport_relay: String,
    transport_token: String,
}

impl Sources {
    /// Reads every source from the real tree.
    fn from_tree() -> Self {
        Self {
            daemon_establishment: source_of(DAEMON_ESTABLISHMENT_MODULE),
            daemon_establishment_task: source_of(DAEMON_ESTABLISHMENT_TASK_MODULE),
            daemon_requester: source_of(DAEMON_REQUESTER_MODULE),
            daemon_target: source_of(DAEMON_TARGET_MODULE),
            daemon_config: source_of(DAEMON_CONFIG),
            daemon_router_i2np: source_of(DAEMON_ROUTER_I2NP),
            daemon_composition: source_of(DAEMON_COMPOSITION),
            daemon_drain: source_of(DAEMON_DRAIN),
            runtime: source_of(RUNTIME_MODULE),
            transport_relay: source_of(TRANSPORT_RELAY_MODULE),
            transport_token: source_of(TRANSPORT_TOKEN_MODULE),
        }
    }
}

// ---- 1. the owner and the task are the daemon's, the mechanics the runtime's

fn rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(
    sources: &Sources,
) -> Option<String> {
    let owner_code = code_view(&sources.daemon_establishment);
    let Some(structure) = item_body(&owner_code, "pub struct RelayEstablishmentOwner") else {
        return Some("the daemon's establishment owner is absent".to_owned());
    };
    // The owner holds no socket and no token table of its own.
    if structure.contains("UdpSocket") || structure.contains("TokenStore") {
        return Some("the establishment owner must hold no socket and no token table".to_owned());
    }
    // The daemon must not re-implement the transport's token table.
    if owner_code.contains("pub struct TokenStore") || owner_code.contains("pub struct TokenEntry")
    {
        return Some("the daemon must not re-implement the transport's token table".to_owned());
    }
    // The task is the daemon's, and it dials through the runtime.
    let task_code = code_view(&sources.daemon_establishment_task);
    let Some(task) = item_body(&task_code, "pub struct RelayEstablishmentTask") else {
        return Some("the daemon's establishment task is absent".to_owned());
    };
    if !squeeze(&task).contains("service: i2pr_runtime::Ssu2RuntimeService") {
        return Some("the task must dial through the runtime that owns the sockets".to_owned());
    }
    // The runtime owns the issuance and the table it issues from.
    let runtime_code = code_view(&sources.runtime);
    let Some(issuance) = item_body(&runtime_code, "pub fn issue_relay_establishment_token(") else {
        return Some("the runtime's establishment-token seam is absent".to_owned());
    };
    if !compact(&issuance).contains("state.token_store.issue(") {
        return Some("the runtime must issue the token from its own responder table".to_owned());
    }
    None
}

// ---- 2. the token comes from the runtime's table, never from the daemon ----

fn rule_the_token_is_issued_by_the_runtimes_responder_table(sources: &Sources) -> Option<String> {
    let target_code = code_view(&sources.daemon_target);
    let Some(answer) = item_body(&target_code, "fn answer_introduction(") else {
        return Some("the target's answer step is absent".to_owned());
    };
    let answer = compact(&answer);
    if !answer.contains("issue_relay_establishment_token(") {
        return Some("the accept's token must come from the runtime's issuance seam".to_owned());
    }
    if !answer.contains("RelayResponseBlock::accept(") {
        return Some("the accept must still be built by the transport's own block".to_owned());
    }
    // The daemon draws no token of its own: a token the daemon drew would
    // be in no table, and the responder would refuse it as unknown.
    if target_code.contains("draw_nonzero_token") || target_code.contains("try_fill_bytes") {
        return Some("the daemon must not draw a token a responder could never accept".to_owned());
    }
    None
}

// ---- 3. the token is bound to the requester's source address --------------

fn rule_the_token_is_bound_to_the_requesters_source(sources: &Sources) -> Option<String> {
    let runtime_code = code_view(&sources.runtime);
    let Some(issuance) = item_body(&runtime_code, "pub fn issue_relay_establishment_token(") else {
        return Some("the runtime's establishment-token seam is absent".to_owned());
    };
    if !compact(&issuance).contains("issue(requester,now_secs,bytes,None)") {
        return Some("the token must be issued against the requester's exact source".to_owned());
    }
    let target_code = code_view(&sources.daemon_target);
    let Some(answer) = item_body(&target_code, "fn answer_introduction(") else {
        return Some("the target's answer step is absent".to_owned());
    };
    let answer = compact(&answer);
    if !answer.contains("letrequester_endpoint=block.endpoint();") {
        return Some("the binding address must be the requester's own endpoint".to_owned());
    }
    if !answer
        .contains("issue_relay_establishment_token(requester_endpoint.socket_addr(),now_secs)")
    {
        return Some("the target must bind the token to the requester's endpoint".to_owned());
    }
    None
}

// ---- 4. the establishment is the ordinary establishment path --------------

fn rule_the_establishment_is_the_ordinary_establishment_path(sources: &Sources) -> Option<String> {
    let owner_code = code_view(&sources.daemon_establishment);
    let Some(drive) = item_body(&owner_code, "pub async fn drive_relay_establishment(") else {
        return Some("the establishment's drive step is absent".to_owned());
    };
    if !compact(&drive).contains("dial_ssu2_with_token(") {
        return Some("the establishment must use the runtime's token-bearing dial".to_owned());
    }
    // No relay-specific session and no bypass of the responder.
    for forbidden in [
        "queue_relay_response(",
        "stage_relay_hole_punch(",
        "Ssu2InboundRelay",
    ] {
        if owner_code.contains(forbidden) {
            return Some("the establishment must not build a relay-specific session".to_owned());
        }
    }
    let runtime_code = code_view(&sources.runtime);
    let Some(dial) = item_body(&runtime_code, "pub async fn dial_ssu2_with_token(") else {
        return Some("the runtime's token-bearing dial is absent".to_owned());
    };
    if !compact(&dial).contains("dial_ssu2_inner(target,Some(token),timeout,cancellation)") {
        return Some("the token must be handed to the one dial path".to_owned());
    }
    // The token is presented in the initiator, and the ordinary path is
    // the only path that presents it.
    let Some(inner) = item_body(&runtime_code, "async fn dial_ssu2_inner(") else {
        return Some("the runtime's one dial path is absent".to_owned());
    };
    let inner = compact(&inner);
    if !inner.contains("Some(value)=>Some(value),") {
        return Some("the explicit token must be presented as it is".to_owned());
    }
    if !inner.contains("None=>take_cached_token_locked(&mutstate,&target.peer(),now_secs),") {
        return Some("the ordinary dial must still use the cached-token path".to_owned());
    }
    None
}

// ---- 5. the dial target's keys come from the validated store --------------

fn rule_the_dial_target_comes_from_the_validated_store(sources: &Sources) -> Option<String> {
    let owner_code = code_view(&sources.daemon_establishment);
    let Some(prepare) = item_body(&owner_code, "pub fn prepare_relay_establishment(") else {
        return Some("the establishment's preparation step is absent".to_owned());
    };
    let prepare = compact(&prepare);
    for needle in [
        "store.get(&key)",
        "Ssu2RouterAddress::parse(address)",
        "parsed.static_public_key()",
        "parsed.intro_key()",
    ] {
        if !prepare.contains(&compact(needle)) {
            return Some(format!(
                "the dial target must read {needle} from the validated store"
            ));
        }
    }
    // It never parses unvalidated bytes.
    if prepare.contains("RouterInfo::decode") {
        return Some("the establishment must never parse unvalidated bytes".to_owned());
    }
    None
}

// ---- 6. the dial target obeys the qualification profile -------------------

fn rule_the_dial_target_obeys_the_qualification_profile(sources: &Sources) -> Option<String> {
    let owner_code = code_view(&sources.daemon_establishment);
    let Some(prepare) = item_body(&owner_code, "pub fn prepare_relay_establishment(") else {
        return Some("the establishment's preparation step is absent".to_owned());
    };
    if !compact(&prepare).contains("daemon_dial_target_for_profile(") {
        return Some("the dial target must be built under the qualification profile".to_owned());
    }
    let router_code = code_view(&sources.daemon_router_i2np);
    let Some(builder) = item_body(&router_code, "pub fn daemon_dial_target_for_profile(") else {
        return Some("the profile-aware dial target builder is absent".to_owned());
    };
    let builder = compact(&builder);
    if !builder.contains("Ssu2DialTarget::new(")
        || !builder.contains("Ssu2DialTarget::new_private_lan(")
    {
        return Some("the profile builder must keep the transport's own clamps".to_owned());
    }
    None
}

// ---- 7. the deferred punch is bounded and re-driven -----------------------

fn rule_the_deferred_punch_is_bounded_and_re_driven(sources: &Sources) -> Option<String> {
    let requester_code = code_view(&sources.daemon_requester);
    let Some(owner) = item_body(&requester_code, "pub struct RelayRequesterOwner") else {
        return Some("the daemon's requester owner is absent".to_owned());
    };
    if !squeeze(&owner).contains("deferred: BTreeMap<u32, DeferredPunch>") {
        return Some("the owner must hold the deferred punches".to_owned());
    }
    let Some(handle) = item_body(&requester_code, "fn handle_hole_punch(") else {
        return Some("the hole punch's correlation step is absent".to_owned());
    };
    let handle = compact(&handle);
    if !handle.contains("matches!(error,i2pr_runtime::RelayError::WrongRole)") {
        return Some("the transport's wrong-role refusal must be the deferral signal".to_owned());
    }
    if !handle.contains("defer_hole_punch(message,now_ms)") {
        return Some("an early punch must be held rather than refused".to_owned());
    }
    // The hold is bounded.
    let Some(defer) = item_body(&requester_code, "fn defer_hole_punch(") else {
        return Some("the deferral step is absent".to_owned());
    };
    if !compact(&defer).contains("self.deferred.len()>=MAX_DEFERRED_PUNCHES") {
        return Some("the deferral must be bounded".to_owned());
    }
    // The held punch is presented again once the accept is correlated.
    let Some(redrive) = item_body(&requester_code, "pub fn redrive_deferred(") else {
        return Some("the re-drive step is absent".to_owned());
    };
    if !compact(&redrive)
        .contains("handle_hole_punch(&deferred.message,signers,now_secs,now_ms,false)")
    {
        return Some("the held punch must be presented again".to_owned());
    }
    // The drain performs the re-drive.
    let drain_code = code_view(&sources.daemon_drain);
    if !compact(&drain_code)
        .contains("owner.redrive_deferred(block.nonce(),signers,now_secs,now_ms)")
    {
        return Some("the drain must release the held punch".to_owned());
    }
    None
}

// ---- 8. the handoff to the task is bounded and never a silent drop -------

fn rule_the_handoff_to_the_task_is_bounded(sources: &Sources) -> Option<String> {
    let owner_code = code_view(&sources.daemon_establishment);
    let Some(declaration) = owner_code
        .lines()
        .find(|line| line.contains("pub const RELAY_ESTABLISHMENT_QUEUE:"))
    else {
        return Some("the handoff must have a stated capacity".to_owned());
    };
    // The capacity is a literal bound: an unbounded or computed capacity
    // is not a bound.
    let Some((_, value)) = declaration.split_once('=') else {
        return Some("the handoff's capacity must be a literal bound".to_owned());
    };
    let value = value.trim().trim_end_matches(';').trim();
    if value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()) {
        return Some("the handoff's capacity must be a literal bound".to_owned());
    }
    let requester_code = code_view(&sources.daemon_requester);
    let Some(returned) = item_body(&requester_code, "pub fn return_establishment(") else {
        return Some("the owner's return step is absent".to_owned());
    };
    if !compact(&returned).contains("EstablishmentQueueFull") {
        return Some("a full handoff must be counted under its own reason".to_owned());
    }
    let composition = code_view(&sources.daemon_composition);
    if !composition.contains("try_send(") {
        return Some("the tick must hand the establishment to the task".to_owned());
    }
    if !composition.contains("return_establishment(") {
        return Some("a full handoff must return the material to the owner".to_owned());
    }
    None
}

// ---- 9. the tick never awaits a handshake --------------------------------

fn rule_the_tick_never_awaits_a_handshake(sources: &Sources) -> Option<String> {
    let composition = compact(&code_view(&sources.daemon_composition));
    // The task is spawned under the service's own child scope, so it is
    // owned, cancelled and joined with the service.
    if !composition.contains(
        "children.spawn(move|cancellation|asyncmove{crate::relay_establishment_task::run_relay_establishment_task(",
    ) {
        return Some("the establishment's task must be spawned under the service's child scope".to_owned());
    }
    // The tick never drives the establishment itself.
    if composition.contains("drive_relay_establishment(") {
        return Some("the relay tick must never await a handshake".to_owned());
    }
    // The dial lives in the task's module.
    let task_code = code_view(&sources.daemon_establishment_task);
    if !task_code.contains("drive_relay_establishment(") {
        return Some("the task must drive the establishment".to_owned());
    }
    None
}

// ---- 10. no configuration key is added ------------------------------------

fn rule_no_other_configuration_key(sources: &Sources) -> Option<String> {
    let config = code_view(&sources.daemon_config);
    if !config.contains("relay_requester: bool,") || !config.contains("relay_target: bool,") {
        return Some("the two relay opt-ins must stay".to_owned());
    }
    if config.contains("relay_establishment") {
        return Some("the establishment must add no configuration key of its own".to_owned());
    }
    None
}

// ---- 11. the daemon writes no reachability signal ------------------------

fn rule_the_daemon_writes_no_reachability_signal(sources: &Sources) -> Option<String> {
    let owner_code = code_view(&sources.daemon_establishment);
    let Some(counters) = item_body(&owner_code, "pub struct RelayEstablishmentCounters") else {
        return Some("the establishment's counters are absent".to_owned());
    };
    if counters.contains("reachability") {
        return Some("the counters must carry no reachability".to_owned());
    }
    for (name, code) in [
        ("the establishment owner", &owner_code),
        (
            "the establishment task",
            &code_view(&sources.daemon_establishment_task),
        ),
    ] {
        for forbidden in [
            "note_relay_firewalled",
            "reachability",
            "record_reachability",
            "note_observed_address",
        ] {
            if code.contains(forbidden) {
                return Some(format!("{name} must write no reachability signal"));
            }
        }
    }
    None
}

// ---- 12. the transport's relay machines and token table are untouched ----

fn rule_the_transport_is_untouched(sources: &Sources) -> Option<String> {
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
    // The transport's token table is intact.
    let token = code_view(&sources.transport_token);
    for needle in ["pub fn issue(", "pub fn consume("] {
        if !token.contains(needle) {
            return Some(format!("the transport's token seam {needle} is absent"));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted.
// ----------------------------------------------------------------------

#[test]
fn the_owner_is_the_daemons_and_the_mechanics_the_runtimes() {
    let reason =
        rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_token_is_issued_by_the_runtimes_responder_table() {
    let reason = rule_the_token_is_issued_by_the_runtimes_responder_table(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_token_is_bound_to_the_requesters_source() {
    let reason = rule_the_token_is_bound_to_the_requesters_source(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_establishment_is_the_ordinary_establishment_path() {
    let reason = rule_the_establishment_is_the_ordinary_establishment_path(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_dial_target_comes_from_the_validated_store() {
    let reason = rule_the_dial_target_comes_from_the_validated_store(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_dial_target_obeys_the_qualification_profile() {
    let reason = rule_the_dial_target_obeys_the_qualification_profile(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_deferred_punch_is_bounded_and_re_driven() {
    let reason = rule_the_deferred_punch_is_bounded_and_re_driven(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_handoff_to_the_task_is_bounded() {
    let reason = rule_the_handoff_to_the_task_is_bounded(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_tick_never_awaits_a_handshake() {
    let reason = rule_the_tick_never_awaits_a_handshake(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn no_configuration_key_is_added() {
    let reason = rule_no_other_configuration_key(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_daemon_writes_no_reachability_signal() {
    let reason = rule_the_daemon_writes_no_reachability_signal(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

#[test]
fn the_transports_relay_machines_and_token_table_are_untouched() {
    let reason = rule_the_transport_is_untouched(&Sources::from_tree());
    assert_eq!(reason, None, "{reason:?}");
}

// ----------------------------------------------------------------------
// The inverted probes. Each one mutates an in-memory copy, so no file on
// disk is written.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_a_runtime_that_issues_from_another_table() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources
        .runtime
        .replace("state.token_store.issue(", "state.own_table.issue(");
    let reason = rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(&sources);
    assert!(
        reason.is_some(),
        "a runtime that issues from another table must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_draws_its_own_token() {
    let mut sources = Sources::from_tree();
    sources.daemon_target = sources.daemon_target.replace(
        "issue_relay_establishment_token(requester_endpoint.socket_addr(), now_secs)",
        "draw_own_token(rng)",
    );
    let reason = rule_the_token_is_issued_by_the_runtimes_responder_table(&sources);
    assert!(
        reason.is_some(),
        "a daemon-side token draw must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_token_bound_to_another_source() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "state.token_store.issue(requester, now_secs, bytes, None)",
        "state.token_store.issue(other, now_secs, bytes, None)",
    );
    let reason = rule_the_token_is_bound_to_the_requesters_source(&sources);
    assert!(
        reason.is_some(),
        "a token bound to another source must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_establishment_that_skips_the_token() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources
        .daemon_establishment
        .replace("dial_ssu2_with_token(", "dial_ssu2_relay(");
    let reason = rule_the_establishment_is_the_ordinary_establishment_path(&sources);
    assert!(
        reason.is_some(),
        "an establishment that skips the token must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_daemon_side_token_table() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources.daemon_establishment.replace(
        "pub struct RelayEstablishmentOwner {",
        "pub struct TokenStore { entries: Vec<u8> }\npub struct RelayEstablishmentOwner {",
    );
    let reason = rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(&sources);
    assert!(
        reason.is_some(),
        "a daemon-side token table must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_dial_target_that_parses_unvalidated_bytes() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources.daemon_establishment.replace(
        "Ssu2RouterAddress::parse(address)",
        "parse_unvalidated(address)",
    );
    let reason = rule_the_dial_target_comes_from_the_validated_store(&sources);
    assert!(
        reason.is_some(),
        "a dial target that parses unvalidated bytes must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_dial_target_that_ignores_the_profile() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources
        .daemon_establishment
        .replace("daemon_dial_target_for_profile(", "daemon_dial_target(");
    let reason = rule_the_dial_target_obeys_the_qualification_profile(&sources);
    assert!(
        reason.is_some(),
        "a dial target outside the profile must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_punch_that_is_refused_instead_of_held() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources.daemon_requester.replace(
        "if allow_deferral && matches!(error, i2pr_runtime::RelayError::WrongRole) {",
        "if false {",
    );
    let reason = rule_the_deferred_punch_is_bounded_and_re_driven(&sources);
    assert!(
        reason.is_some(),
        "a punch that is refused instead of held must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_deferral_that_is_never_re_driven() {
    let mut sources = Sources::from_tree();
    sources.daemon_drain = sources
        .daemon_drain
        .replace("owner.redrive_deferred(", "owner.ignore_deferred(");
    let reason = rule_the_deferred_punch_is_bounded_and_re_driven(&sources);
    assert!(
        reason.is_some(),
        "a held punch that is never re-driven must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_unbounded_handoff() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources.daemon_establishment.replace(
        "pub const RELAY_ESTABLISHMENT_QUEUE: usize = 8;",
        "pub const RELAY_ESTABLISHMENT_QUEUE: usize = usize::MAX;",
    );
    let reason = rule_the_handoff_to_the_task_is_bounded(&sources);
    assert!(reason.is_some(), "an unbounded handoff must fail the rule");
}

#[test]
fn the_guard_reports_a_silent_handoff_drop() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources
        .daemon_composition
        .replace("return_establishment(", "drop_establishment(");
    let reason = rule_the_handoff_to_the_task_is_bounded(&sources);
    assert!(reason.is_some(), "a silent handoff drop must fail the rule");
}

#[test]
fn the_guard_reports_a_tick_that_awaits_the_handshake() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "crate::relay_establishment_task::run_relay_establishment_task(",
        "crate::relay_establishment_task::unused_task(",
    );
    let reason = rule_the_tick_never_awaits_a_handshake(&sources);
    assert!(
        reason.is_some(),
        "a task that is not spawned under the service's scope must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_configuration_key_for_the_establishment() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "    relay_requester: bool,",
        "    relay_requester: bool,\n    relay_establishment: bool,",
    );
    let reason = rule_no_other_configuration_key(&sources);
    assert!(
        reason.is_some(),
        "a configuration key for the establishment must fail the rule"
    );
}

#[test]
fn the_guard_reports_a_daemon_side_reachability_write() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources.daemon_establishment.replace(
        "pub struct RelayEstablishmentOwner {",
        "pub struct RelayEstablishmentOwner {\n    // probe\n    fn probe(&self) { self.runtime.note_relay_firewalled(family); }",
    );
    let reason = rule_the_daemon_writes_no_reachability_signal(&sources);
    assert!(
        reason.is_some(),
        "a daemon-side reachability write must fail the rule"
    );
}

#[test]
fn the_guard_reports_an_edited_transport_rule() {
    let mut sources = Sources::from_tree();
    sources.transport_relay = sources.transport_relay.replace(
        "if entry.state != RequesterState::AwaitingHolePunch {",
        "if false {",
    );
    let reason = rule_the_transport_is_untouched(&sources);
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
fn the_guard_fails_closed_when_the_owner_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources.daemon_establishment.replace(
        "pub struct RelayEstablishmentOwner",
        "pub struct UnusedEstablishmentOwner",
    );
    let reason = rule_the_owner_is_the_daemons_and_the_mechanics_the_runtimes(&sources);
    assert!(reason.is_some(), "an absent owner must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_issuance_seam_is_absent() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "pub fn issue_relay_establishment_token(",
        "pub fn unused_issuance_seam(",
    );
    let reason = rule_the_token_is_bound_to_the_requesters_source(&sources);
    assert!(
        reason.is_some(),
        "an absent issuance seam must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_drive_step_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources.daemon_establishment.replace(
        "pub async fn drive_relay_establishment(",
        "pub async fn unused_drive_step(",
    );
    let reason = rule_the_establishment_is_the_ordinary_establishment_path(&sources);
    assert!(reason.is_some(), "an absent drive step must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_preparation_step_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources.daemon_establishment.replace(
        "pub fn prepare_relay_establishment(",
        "pub fn unused_preparation_step(",
    );
    let reason = rule_the_dial_target_comes_from_the_validated_store(&sources);
    assert!(
        reason.is_some(),
        "an absent preparation step must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_profile_builder_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_router_i2np = sources.daemon_router_i2np.replace(
        "pub fn daemon_dial_target_for_profile(",
        "pub fn unused_profile_builder(",
    );
    let reason = rule_the_dial_target_obeys_the_qualification_profile(&sources);
    assert!(
        reason.is_some(),
        "an absent profile builder must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_re_drive_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_requester = sources
        .daemon_requester
        .replace("pub fn redrive_deferred(", "pub fn unused_redrive(");
    let reason = rule_the_deferred_punch_is_bounded_and_re_driven(&sources);
    assert!(reason.is_some(), "an absent re-drive must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_handoff_bound_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources
        .daemon_establishment
        .replace("pub const RELAY_ESTABLISHMENT_QUEUE: usize = 8;", "");
    let reason = rule_the_handoff_to_the_task_is_bounded(&sources);
    assert!(
        reason.is_some(),
        "an absent handoff bound must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_counters_are_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_establishment = sources.daemon_establishment.replace(
        "pub struct RelayEstablishmentCounters",
        "pub struct UnusedEstablishmentCounters",
    );
    let reason = rule_the_daemon_writes_no_reachability_signal(&sources);
    assert!(reason.is_some(), "absent counters must fail the rule");
}

#[test]
fn the_guard_fails_closed_when_the_transport_token_table_is_absent() {
    let mut sources = Sources::from_tree();
    sources.transport_token = sources
        .transport_token
        .replace("pub fn consume(", "pub fn unused_consume(");
    let reason = rule_the_transport_is_untouched(&sources);
    assert!(
        reason.is_some(),
        "an absent transport token seam must fail the rule"
    );
}
