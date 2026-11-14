//! Plan 522's structural guard for the bounded outbound peer-session pool.
//!
//! The router dials per need and holds no session between needs, so the
//! publication's ordinary path is the handshake path. Plan 522 gives the
//! router a **bounded pool** of outbound sessions to the peers it already
//! knows, and the transport an **outbound-only** session keepalive that
//! holds a pooled session open.
//!
//! The guard asserts thirteen things about the **production** surface:
//!
//! 1. the opt-in defaults to **off**, and it is refused without the SSU2
//!    service and without an idle timeout the keepalive can fit inside;
//! 2. the pool's owner adds **no** task, no timer, no listener and no
//!    socket: the transport owns the sessions;
//! 3. the pool dials through the **one** existing narrow seam and through
//!    no other;
//! 4. the pool writes **no** peer profile: a local condition must never be
//!    written into a peer's profile;
//! 5. the pool's ceiling **refuses** a surplus candidate rather than
//!    evicting a peer it already holds;
//! 6. the pool's per-period dial budget is **bounded**, and a peer is
//!    dialled once at a time;
//! 7. the transport's keepalive carries **no** I2NP message: it is a
//!    padding-only packet;
//! 8. the transport's keepalive holds **one** outstanding keepalive at a
//!    time, so an unanswered keepalive reaches its deadline;
//! 9. the transport's keepalive respects the **congestion gate** and the
//!    payload budget;
//! 10. the keepalive is **outbound-only**: the runtime passes the interval
//!     to an outbound session and to nothing else;
//! 11. the transport's keepalive is **off** unless the pool is on: the
//!     daemon derives the runtime's interval from the pool's own switch;
//! 12. the **publication** is untouched: the publication's delivery path
//!     names no pool and changes no gate;
//! 13. the pool's task is spawned **only** under the opt-in, and only once:
//!     a router that did not ask for the pool spawns no task at all.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required **call**, with its open parenthesis. Comments
//! are stripped and `#[cfg(test)]` blocks are excluded, so a doc comment that
//! merely names a construct never satisfies a rule.
//!
//! Every rule **fails closed**. A rule whose named item is absent reports
//! that absence as a failure, because a missing function is a defect and not
//! a pass. A rule that only returned "no complaint" for an absent item would
//! let a deleted function satisfy every rule that named it.
//!
//! Every rule has an **inverted probe**: a mutation that must be reported. A
//! rule with no probe is a rule nobody has shown to fire.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The daemon's bounded outbound peer-session pool.
const POOL_MODULE: &str = "crates/i2pr-daemon/src/peer_pool.rs";
/// The pool's own task.
const POOL_TASK_MODULE: &str = "crates/i2pr-daemon/src/peer_pool_task.rs";
/// The daemon's configuration surface.
const CONFIG_MODULE: &str = "crates/i2pr-daemon/src/config.rs";
/// The daemon's composition root.
const DAEMON_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";
/// The daemon's SSU2 runtime composition.
const RUNTIME_COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/router_i2np.rs";
/// The router's own RouterInfo publication.
const PUBLICATION_MODULE: &str = "crates/i2pr-daemon/src/router_info_publication.rs";
/// The daemon's narrow dial seam.
const SERVICE_PRODUCT_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";
/// The runtime's session construction.
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
/// The transport's session.
const TRANSPORT_SESSION_MODULE: &str = "crates/i2pr-transport-ssu2/src/session.rs";

/// Removes every whitespace character from a code view.
///
/// A rule that names a field access must survive the formatter's line
/// wrapping, so the two texts are compared with their whitespace squeezed
/// out. The comments are already gone, so a squeezed match is a match on
/// code and not on prose.
fn squeezed(code: &str) -> String {
    code.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Extracts the body of the item whose **definition** is anchored at
/// `signature`, or reports the absence as a failure.
///
/// A rule that named a function which no longer exists must fail closed: the
/// absence is the defect the rule exists to catch.
fn required_item_body(code: &str, signature: &str) -> Result<String, String> {
    item_body(code, signature).ok_or_else(|| format!("the named item {signature} is absent"))
}

/// Extracts the body of the `impl` block whose header is `header`, or
/// reports the absence as a failure.
fn required_impl_body(code: &str, header: &str) -> Result<String, String> {
    impl_body(code, header).ok_or_else(|| format!("the named impl block {header} is absent"))
}

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

/// Extracts the body of the `impl` block whose header is `header`.
///
/// Some methods share a name across two types — the tracker and the owner
/// both have an `expire` — so a rule that is about **one** of them scopes
/// its lookup to that type's own `impl` block.
fn impl_body(code: &str, header: &str) -> Option<String> {
    let start = find_definition(code, header)?;
    let (open, close) = brace_balanced_block(code, start + header.len())?;
    Some(code[open + 1..close - 1].to_owned())
}

// ---------------------------------------------------------------------------
// Rule 1: the opt-in defaults to off, and it is refused without the SSU2
// service and without an idle timeout the keepalive can fit inside.
// ---------------------------------------------------------------------------

fn check_the_opt_in_defaults_off_and_needs_the_service(config: &str) -> Result<(), String> {
    let code = code_view(config);
    let default = required_item_body(&code, "fn default_ssu2_peer_pool(")?;
    if !default.contains("false") {
        return Err("the peer-pool default must be off".to_owned());
    }
    let normalize = required_item_body(&code, "fn normalize_ssu2(raw: &RawSsu2Config)")?;
    if !normalize.contains("raw.peer_pool && !raw.enabled") {
        return Err("the peer-pool opt-in must be refused when the SSU2 service is off".to_owned());
    }
    if !normalize.contains("ssu2.peer_pool") {
        return Err("the refusal must name the field it refuses".to_owned());
    }
    if !normalize.contains("KEEPALIVE_INTERVAL_MS") {
        return Err(
            "the pool's idle-timeout refusal must use the transport's own keepalive interval"
                .to_owned(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 2: the pool's owner adds no infrastructure.
// ---------------------------------------------------------------------------

fn check_the_owner_adds_no_infrastructure(pool: &str) -> Result<(), String> {
    let code = code_view(pool);
    for forbidden in [
        "tokio::spawn(",
        "std::thread::spawn(",
        "tokio::time::interval(",
        "tokio::time::sleep(",
        "std::net::",
        "TcpListener::",
        "UdpSocket::",
        "tokio::net::",
    ] {
        if code.contains(forbidden) {
            return Err(format!(
                "the pool's owner must not add infrastructure, found {forbidden}"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 3: the pool dials through the one existing narrow seam and through
// no other.
// ---------------------------------------------------------------------------

fn check_the_pool_dials_through_the_one_seam(task: &str, product: &str) -> Result<(), String> {
    let task_code = code_view(task);
    let product_code = code_view(product);
    let seam = "ensure_peer_session_for_record(";
    let calls = task_code.matches(seam).count();
    if calls != 1 {
        return Err(format!(
            "the pool must dial through the one narrow seam exactly once, found {calls}"
        ));
    }
    // The seam itself must still be the only outbound dial the daemon owns
    // for a first hop, and the pool must not name the transport's own dial
    // entry point.
    for forbidden in ["dial_ssu2(", ".dial("] {
        if task_code.contains(forbidden) {
            return Err(format!(
                "the pool must not name the transport's own dial entry point, found {forbidden}"
            ));
        }
    }
    if !product_code.contains(seam) {
        return Err("the narrow dial seam must still exist in the daemon".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 4: the pool writes no peer profile.
// ---------------------------------------------------------------------------

fn check_the_pool_writes_no_peer_profile(pool: &str) -> Result<(), String> {
    let code = code_view(pool);
    for forbidden in [
        "PeerParticipationHandle",
        "PeerParticipationDriver",
        "observe_answered",
        "observe_refused",
        "observe_misbehaving",
        "observe_unanswered",
        "observe_unreachable",
        "learn_all",
        "peer_profiles",
    ] {
        if code.contains(forbidden) {
            return Err(format!(
                "the pool must not write a peer profile, found {forbidden}"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 5: the pool's ceiling refuses a surplus candidate rather than
// evicting a peer it already holds.
// ---------------------------------------------------------------------------

fn check_the_ceiling_refuses_rather_than_evicts(pool: &str) -> Result<(), String> {
    let code = code_view(pool);
    let owner = required_impl_body(&code, "impl PeerPoolOwner")?;
    let body = required_item_body(&owner, "pub fn plan(")?;
    if !body.contains("MAX_POOLED_PEERS") {
        return Err("the pool's population must be bounded by its ceiling".to_owned());
    }
    if !body.contains("ceiling_refused") {
        return Err("the ceiling's refusal must be counted under its own reason".to_owned());
    }
    if body.contains("remove(") || body.contains("retain(") {
        return Err("the pool must refuse a surplus candidate, never evict a peer".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 6: the pool's per-period dial budget is bounded, and a peer is
// dialled once at a time.
// ---------------------------------------------------------------------------

fn check_the_dial_budget_is_bounded(pool: &str) -> Result<(), String> {
    let code = code_view(pool);
    let owner = required_impl_body(&code, "impl PeerPoolOwner")?;
    let body = required_item_body(&owner, "pub fn plan(")?;
    if !body.contains("MAX_DIALS_PER_PERIOD") {
        return Err("the pool's per-period dial budget must be enforced".to_owned());
    }
    if !body.contains("budget_refused") {
        return Err("the budget's refusal must be counted under its own reason".to_owned());
    }
    if !body.contains("dial_in_flight") {
        return Err("a peer that is dialling must not be dialled again".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 7: the transport's keepalive carries no I2NP message.
// ---------------------------------------------------------------------------

fn check_the_keepalive_carries_no_i2np_message(session: &str) -> Result<(), String> {
    let code = code_view(session);
    let body = required_item_body(&code, "fn seal_keepalive(")?;
    if !body.contains("Block::Padding(") {
        return Err("the keepalive must be a padding-only packet".to_owned());
    }
    for forbidden in ["Block::I2np(", "I2npMessageBlock", "FirstFragmentBlock"] {
        if body.contains(forbidden) {
            return Err(format!(
                "the keepalive must carry no I2NP message, found {forbidden}"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 8: the transport holds one outstanding keepalive at a time.
// ---------------------------------------------------------------------------

fn check_one_outstanding_keepalive(session: &str) -> Result<(), String> {
    let code = code_view(session);
    let body = required_item_body(&code, "pub fn poll(")?;
    if !body.contains("keepalive_pending_since_ms.is_none()") {
        return Err(
            "the session must hold one outstanding keepalive at a time, or an unanswered keepalive never reaches its deadline"
                .to_owned(),
        );
    }
    if !body.contains("keepalive_timeouts") {
        return Err("an unanswered keepalive must be counted under its own reason".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 9: the keepalive respects the congestion gate and the payload
// budget.
// ---------------------------------------------------------------------------

fn check_the_keepalive_respects_the_transport_policy(session: &str) -> Result<(), String> {
    let code = code_view(session);
    let body = required_item_body(&code, "pub fn poll(")?;
    if !body.contains("self.bytes_in_flight < self.cwnd_bytes") {
        return Err("the keepalive must not bypass the congestion gate".to_owned());
    }
    let seal = required_item_body(&code, "fn seal_keepalive(")?;
    if !seal.contains("self.seal_packet(") {
        return Err("the keepalive must be sealed by the transport's own seal path".to_owned());
    }
    let seal_path = required_item_body(&code, "fn seal_packet(")?;
    if !seal_path.contains("self.max_payload_bytes") {
        return Err("the seal path must keep the payload budget".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 10: the keepalive is outbound-only.
// ---------------------------------------------------------------------------

fn check_the_keepalive_is_outbound_only(runtime: &str) -> Result<(), String> {
    let code = code_view(runtime);
    let body = required_item_body(&code, "fn construct_session(")?;
    if !body.contains("Direction::Outbound") {
        return Err("the keepalive must reach an outbound session".to_owned());
    }
    if !body.contains("Direction::Inbound => None") {
        return Err("an inbound session must never receive a keepalive interval".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 11: the transport's keepalive is off unless the pool is on.
// ---------------------------------------------------------------------------

fn check_the_keepalive_is_off_without_the_pool(composition: &str) -> Result<(), String> {
    let code = code_view(composition);
    let body = squeezed(&required_item_body(
        &code,
        "pub fn ssu2_runtime_config_from(",
    )?);
    if !body.contains("keepalive:config.peer_pool.then(") {
        return Err(
            "the runtime's keepalive interval must be derived from the pool's own switch"
                .to_owned(),
        );
    }
    if !body.contains("KEEPALIVE_INTERVAL_MS") {
        return Err(
            "the keepalive interval must be the transport's own constant, never a second value"
                .to_owned(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 12: the publication is untouched.
// ---------------------------------------------------------------------------

fn check_the_publication_is_untouched(publication: &str) -> Result<(), String> {
    let code = code_view(publication);
    for forbidden in ["peer_pool", "PeerPoolOwner", "keepalive"] {
        if code.contains(forbidden) {
            return Err(format!(
                "the publication must not name the pool or the keepalive, found {forbidden}"
            ));
        }
    }
    // The publication's own gate must still be the session check it was.
    if !code.contains("session_peer_links(peer_id).is_empty()") {
        return Err(
            "the publication's delivery gate must still be the session check it was".to_owned(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 13: the pool's task is spawned only under the opt-in, and only once.
// ---------------------------------------------------------------------------

fn check_the_pool_task_runs_only_under_the_opt_in(daemon: &str) -> Result<(), String> {
    let code = code_view(daemon);
    let body = squeezed(&required_item_body(&code, "fn register_ssu2_service(")?);
    let gate = body
        .find("ifssu2_config.peer_pool{")
        .or_else(|| body.find("ifconfig.peer_pool{"));
    let Some(gate) = gate else {
        return Err("the pool's task must be spawned under the opt-in".to_owned());
    };
    let Some(spawn) = body.find("run_peer_pool_task(") else {
        return Err("the pool's task must be spawned under the opt-in".to_owned());
    };
    if spawn < gate {
        return Err("the pool's task must be spawned under the opt-in".to_owned());
    }
    if squeezed(&code).matches("run_peer_pool_task(").count() != 1 {
        return Err("the pool's task must be spawned exactly once".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The rules, run against the real tree.
// ---------------------------------------------------------------------------

#[test]
fn the_guard_accepts_the_real_tree() {
    let pool = source_of(POOL_MODULE);
    let task = source_of(POOL_TASK_MODULE);
    let config = source_of(CONFIG_MODULE);
    let daemon = source_of(DAEMON_MODULE);
    let composition = source_of(RUNTIME_COMPOSITION_MODULE);
    let publication = source_of(PUBLICATION_MODULE);
    let product = source_of(SERVICE_PRODUCT_MODULE);
    let runtime = source_of(RUNTIME_MODULE);
    let session = source_of(TRANSPORT_SESSION_MODULE);

    let checks: Vec<(&str, Result<(), String>)> = vec![
        (
            "the opt-in defaults off and needs the service",
            check_the_opt_in_defaults_off_and_needs_the_service(&config),
        ),
        (
            "the owner adds no infrastructure",
            check_the_owner_adds_no_infrastructure(&pool),
        ),
        (
            "the pool dials through the one seam",
            check_the_pool_dials_through_the_one_seam(&task, &product),
        ),
        (
            "the pool writes no peer profile",
            check_the_pool_writes_no_peer_profile(&pool),
        ),
        (
            "the ceiling refuses rather than evicts",
            check_the_ceiling_refuses_rather_than_evicts(&pool),
        ),
        (
            "the dial budget is bounded",
            check_the_dial_budget_is_bounded(&pool),
        ),
        (
            "the keepalive carries no I2NP message",
            check_the_keepalive_carries_no_i2np_message(&session),
        ),
        (
            "one outstanding keepalive at a time",
            check_one_outstanding_keepalive(&session),
        ),
        (
            "the keepalive respects the transport policy",
            check_the_keepalive_respects_the_transport_policy(&session),
        ),
        (
            "the keepalive is outbound-only",
            check_the_keepalive_is_outbound_only(&runtime),
        ),
        (
            "the keepalive is off without the pool",
            check_the_keepalive_is_off_without_the_pool(&composition),
        ),
        (
            "the publication is untouched",
            check_the_publication_is_untouched(&publication),
        ),
        (
            "the pool's task runs only under the opt-in",
            check_the_pool_task_runs_only_under_the_opt_in(&daemon),
        ),
    ];
    for (name, result) in checks {
        assert!(result.is_ok(), "rule failed: {name}: {result:?}");
    }
}

// ---------------------------------------------------------------------------
// The inverted probes. A rule with no probe is a rule nobody has shown to
// fire.
// ---------------------------------------------------------------------------

#[test]
fn the_guard_reports_an_opt_in_that_defaults_on() {
    let config = r#"
fn default_ssu2_peer_pool() -> bool {
    true
}
fn normalize_ssu2(raw: &RawSsu2Config) -> Result<Ssu2Config, ConfigError> {
    let _ = raw.peer_pool && !raw.enabled;
    let _ = "ssu2.peer_pool";
    let _ = KEEPALIVE_INTERVAL_MS;
    Ok(Ssu2Config { peer_pool: raw.peer_pool, ..Default::default() })
}
"#;
    assert!(
        check_the_opt_in_defaults_off_and_needs_the_service(config).is_err(),
        "an opt-in that defaults on must be reported"
    );
}

#[test]
fn the_guard_reports_a_missing_named_item() {
    // A rule whose named function is absent must **fail closed**: the
    // absence is a defect, not a pass.
    let config = r#"
fn normalize_ssu2(raw: &RawSsu2Config) -> Result<Ssu2Config, ConfigError> {
    let _ = raw.peer_pool && !raw.enabled;
    let _ = "ssu2.peer_pool";
    let _ = KEEPALIVE_INTERVAL_MS;
    Ok(Ssu2Config { peer_pool: raw.peer_pool, ..Default::default() })
}
"#;
    assert!(
        check_the_opt_in_defaults_off_and_needs_the_service(config).is_err(),
        "a rule whose named item is absent must report the absence"
    );
}

#[test]
fn the_guard_reports_a_pool_that_spawns_a_task() {
    let pool = r#"
impl PeerPoolOwner {
    pub fn plan(&mut self, store: &RouterInfoStore) -> Vec<RouterHash> {
        tokio::spawn(async move { do_work().await });
        Vec::new()
    }
}
"#;
    assert!(
        check_the_owner_adds_no_infrastructure(pool).is_err(),
        "a pool that spawns a task must be reported"
    );
}

#[test]
fn the_guard_reports_a_pool_that_binds_a_socket() {
    let pool = r#"
impl PeerPoolOwner {
    pub fn plan(&mut self, store: &RouterInfoStore) -> Vec<RouterHash> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let _ = listener;
        Vec::new()
    }
}
"#;
    assert!(
        check_the_owner_adds_no_infrastructure(pool).is_err(),
        "a pool that binds a socket must be reported"
    );
}

#[test]
fn the_guard_reports_a_pool_that_dials_the_transport_directly() {
    let task = r#"
pub async fn run_peer_pool_task(task: PeerPoolTask, cancellation: CancellationToken) -> Result<(), ChildTaskFailure> {
    let _ = task.handle.service().dial_ssu2(target, timeout, &cancellation).await;
    Ok(())
}
"#;
    let product = source_of(SERVICE_PRODUCT_MODULE);
    assert!(
        check_the_pool_dials_through_the_one_seam(task, &product).is_err(),
        "a pool that dials the transport directly must be reported"
    );
}

#[test]
fn the_guard_reports_a_pool_that_writes_a_peer_profile() {
    let pool = r#"
impl PeerPoolOwner {
    pub fn plan(&mut self, store: &RouterInfoStore) -> Vec<RouterHash> {
        self.profiles.observe_unanswered(&RouterHash::from_bytes([0; 32]));
        Vec::new()
    }
}
"#;
    assert!(
        check_the_pool_writes_no_peer_profile(pool).is_err(),
        "a pool that writes a peer profile must be reported"
    );
}

#[test]
fn the_guard_reports_a_pool_that_evicts_a_peer() {
    let pool = r#"
impl PeerPoolOwner {
    pub fn plan(&mut self, store: &RouterInfoStore) -> Vec<RouterHash> {
        let _ = MAX_POOLED_PEERS;
        let _ = ceiling_refused;
        self.peers.remove(&RouterHash::from_bytes([0; 32]));
        Vec::new()
    }
}
"#;
    assert!(
        check_the_ceiling_refuses_rather_than_evicts(pool).is_err(),
        "a pool that evicts a peer must be reported"
    );
}

#[test]
fn the_guard_reports_a_pool_with_no_dial_budget() {
    let pool = r#"
impl PeerPoolOwner {
    pub fn plan(&mut self, store: &RouterInfoStore) -> Vec<RouterHash> {
        let _ = dial_in_flight;
        Vec::new()
    }
}
"#;
    assert!(
        check_the_dial_budget_is_bounded(pool).is_err(),
        "a pool with no dial budget must be reported"
    );
}

#[test]
fn the_guard_reports_a_keepalive_that_carries_an_i2np_message() {
    let session = r#"
fn seal_keepalive(&mut self, now_ms: u64) -> Option<Vec<u8>> {
    let block = I2npMessageBlock::from_bytes(encoded).expect("block");
    self.seal_packet(now_ms, vec![Block::I2np(block)], true, true)
}
"#;
    assert!(
        check_the_keepalive_carries_no_i2np_message(session).is_err(),
        "a keepalive that carries an I2NP message must be reported"
    );
}

#[test]
fn the_guard_reports_a_session_that_keeps_two_keepalives() {
    let session = r#"
pub fn poll(&mut self, now_ms: u64, now_secs: u64) -> Vec<SessionAction> {
    let _ = keepalive_timeouts;
    let _ = self.bytes_in_flight < self.cwnd_bytes;
    Vec::new()
}
"#;
    assert!(
        check_one_outstanding_keepalive(session).is_err(),
        "a session that keeps two keepalives outstanding must be reported"
    );
}

#[test]
fn the_guard_reports_a_keepalive_that_bypasses_the_congestion_gate() {
    let session = r#"
pub fn poll(&mut self, now_ms: u64, now_secs: u64) -> Vec<SessionAction> {
    let _ = keepalive_pending_since_ms.is_none();
    let _ = keepalive_timeouts;
    Vec::new()
}
fn seal_keepalive(&mut self, now_ms: u64) -> Option<Vec<u8>> {
    let padding = PaddingBlock::new(vec![0_u8; 20]).ok()?;
    self.seal_packet(now_ms, vec![Block::Padding(padding)], true, true)
}
fn seal_packet(&mut self, now_ms: u64, blocks: Vec<Block>, force_ack_only: bool, force_immediate_ack: bool) -> Option<Vec<u8>> {
    let _ = now_ms;
    None
}
"#;
    assert!(
        check_the_keepalive_respects_the_transport_policy(session).is_err(),
        "a keepalive that bypasses the congestion gate must be reported"
    );
}

#[test]
fn the_guard_reports_a_keepalive_on_an_inbound_session() {
    let runtime = r#"
fn construct_session(&self, state: &mut ServiceState, auth: AuthenticatedSsu2Session, remote_intro: IntroKey, peer_addr: SocketAddr, direction: Direction) -> Option<Ssu2Session> {
    let keepalive_interval_ms = match direction {
        Direction::Outbound => Some(60_000),
        Direction::Inbound => Some(60_000),
    };
    let _ = keepalive_interval_ms;
    None
}
"#;
    assert!(
        check_the_keepalive_is_outbound_only(runtime).is_err(),
        "a keepalive on an inbound session must be reported"
    );
}

#[test]
fn the_guard_reports_a_keepalive_that_is_on_without_the_pool() {
    let composition = r#"
pub fn ssu2_runtime_config_from(config: &Ssu2Config) -> Result<Ssu2RuntimeConfig, Ssu2ServiceError> {
    let deadlines = Ssu2RuntimeDeadlines {
        keepalive: Some(Duration::from_millis(i2pr_runtime::KEEPALIVE_INTERVAL_MS)),
        ..Default::default()
    };
    Ok(Ssu2RuntimeConfig::default())
}
"#;
    assert!(
        check_the_keepalive_is_off_without_the_pool(composition).is_err(),
        "a keepalive that is on without the pool must be reported"
    );
}

#[test]
fn the_guard_reports_a_publication_that_names_the_pool() {
    let publication = r#"
async fn deliver_publication_attempt<R: RngCore>(coordinator: &mut PublicationCoordinator, attempt: &i2pr_netdb::PublicationAttemptRecord, records: &[(RouterHash, ValidatedRouterInfo)], ssu2_handle: &crate::router_i2np::Ssu2PublicationHandle, dial_timeout: Duration, qualification_profile: crate::config::Ssu2QualificationProfile, rng: &mut R, cancellation: &i2pr_runtime::CancellationToken) -> bool {
    let _ = ssu2_handle.service().session_peer_links(peer_id).is_empty();
    let _ = peer_pool;
    true
}
"#;
    assert!(
        check_the_publication_is_untouched(publication).is_err(),
        "a publication that names the pool must be reported"
    );
}

#[test]
fn the_guard_reports_a_pool_task_that_runs_without_the_opt_in() {
    let daemon = r#"
fn register_ssu2_service(config: &Ssu2Config) -> Result<(), Ssu2ServiceError> {
    let _ = run_peer_pool_task(peer_pool_task, cancellation);
    Ok(())
}
"#;
    assert!(
        check_the_pool_task_runs_only_under_the_opt_in(daemon).is_err(),
        "a pool task that runs without the opt-in must be reported"
    );
}
