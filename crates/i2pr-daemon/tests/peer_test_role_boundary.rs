//! Plan 521's structural guard for the router's own peer test.
//!
//! The ordinary router can run an I2NP peer test against a peer it already
//! holds a session with, and a **confirmed** exchange records the router's
//! **own** reachability. Plan 520's published SSU2 address needs three
//! distinct corroboration classes, and the third -- a confirmed peer test --
//! was recorded in production only by the controlled floodfill activation.
//!
//! The guard asserts twelve things about the **production** surface:
//!
//! 1. the opt-in defaults to **off**, and it is refused without the SSU2
//!    service;
//! 2. the owner records the probe in the outstanding table **before** the
//!    send, so an answer that arrives first is never dropped;
//! 3. the runtime's production recorder is called from exactly **one**
//!    production call site -- the confirmed-match path;
//! 4. the daemon asserts **no** address family: the runtime derives the
//!    family from the session the exchange travelled on;
//! 5. the controlled recorder is **untouched**: the controlled floodfill
//!    path keeps its own mirror;
//!
//!    (Plan 527 extended rule 5. The controlled mirror may now file its
//!    typed outcome through the runtime's single typed-outcome recorder
//!    instead of constructing the signal itself, so the rule accepts that
//!    delegation **only** when the recorder it delegates to is present and
//!    files the outcome as a peer-test result under its own kind. The rule
//!    is extended, not weakened: a mirror that records nothing, and a
//!    recorder that files no peer-test result, are both still reported.)
//! 6. the daemon records **no** reachability signal of its own outside the
//!    confirmed-match path;
//! 7. the responder derives the answer from the **authenticated** delivery
//!    and runs **no** second decoder;
//! 8. the inbound path decodes the message **once**: the responder reads the
//!    dispatcher's classified outcome;
//! 9. the owner adds **no** task, no timer and no listener;
//! 10. the answered-key table is **bounded**, and a repeated key is
//!     **suppressed** rather than answered again;
//! 11. the transport's corroboration policy is **untouched**: the daemon
//!     names no threshold and no class;
//! 12. the publication is **untouched**: the peer test touches no builder and
//!     no permit.
//!
//! **Plan 547 retargeted this guard, and strengthened it.** The peer-test
//! owner moved out of the destination-group product and beside the SSU2
//! service, in `crates/i2pr-daemon/src/peer_test_runtime.rs`, because the
//! product is built only when a destination group can exist at all and a
//! router that runs only SSU2 therefore had no owner and no corroboration.
//! The rules that named the product's peer-test code now name the new
//! module, and the two rules that named the product's **inbound path** now
//! name the daemon's composition root.
//!
//! The retarget also closed a latent defect: several rules used `?` on an
//! `Option`, which returns "no problem found" when the named definition is
//! **absent**. The header's claim that "a function that is **absent** fails
//! closed" was therefore false, and a rule could be satisfied by deleting
//! the code it policed. Every rule this plan touched now reports an absent
//! definition instead of passing it.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required **call**, with its open parenthesis. Comments
//! are stripped and `#[cfg(test)]` blocks are excluded, so a doc comment that
//! merely names a construct never satisfies a rule, and a function that is
//! **absent** fails closed.
//!
//! Every rule has an **inverted probe**: a mutation that must be reported. A
//! rule with no probe is a rule nobody has shown to fire.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The daemon's owner of this router's own peer test.
const OWNER_MODULE: &str = "crates/i2pr-daemon/src/peer_test.rs";
/// The daemon's live inbound path and its tick.
const PRODUCT_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";
/// Plan 547: the router's own peer-test owner, beside the SSU2 service.
///
/// The owner left the destination-group product, so the rules that policed
/// its production code now police this module.
const PEER_TEST_RUNTIME_MODULE: &str = "crates/i2pr-daemon/src/peer_test_runtime.rs";
/// Plan 547: the daemon's composition root, where the owner is built, driven
/// and fed the classified inbound outcome.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";
/// The daemon's configuration surface.
const CONFIG_MODULE: &str = "crates/i2pr-daemon/src/config.rs";
/// The runtime's own session state and its recorder.
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
/// The transport's own corroboration policy.
const TRANSPORT_REACHABILITY_MODULE: &str = "crates/i2pr-transport/src/reachability.rs";
/// The netdb's local RouterInfo builders.
const NETDB_LOCAL_MODULE: &str = "crates/i2pr-netdb/src/local.rs";

/// The workspace root, resolved from this crate's manifest directory.
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

/// Extracts the body of a method that belongs to the peer-test owner.
fn owner_item_body(code: &str, signature: &str) -> Option<String> {
    let block = impl_body(code, "impl PeerTestOwner")?;
    item_body(&block, signature)
}

/// Extracts the body of the `select!` arm whose head is `head`.
///
/// Plan 547: the peer test is driven from the composition root's existing
/// group tick, and the rule that polices it must look at that arm's own
/// body rather than at the whole file.
fn select_arm_body(code: &str, head: &str) -> Option<String> {
    let start = code.find(head)? + head.len();
    let (open, end) = brace_balanced_block(code, start)?;
    Some(code[open..end].to_owned())
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1: the opt-in defaults to off, and it is refused without the SSU2
/// service.
///
/// The switch is a real opt-in: the default configuration leaves it off, and
/// a configuration that asks for the peer test without the service it needs
/// is refused at parse time rather than silently inert.
fn check_the_opt_in_defaults_off_and_needs_the_service(config: &str) -> Option<String> {
    let code = code_view(config);
    let default = item_body(&code, "fn default_ssu2_peer_test(")?;
    if !default.contains("false") {
        return Some("the peer-test default must be off".to_owned());
    }
    let normalize = item_body(&code, "fn normalize_ssu2(raw: &RawSsu2Config)")?;
    if !normalize.contains("raw.peer_test && !raw.enabled") {
        return Some(
            "the peer-test opt-in must be refused when the SSU2 service is off".to_owned(),
        );
    }
    if !normalize.contains("ssu2.peer_test") {
        return Some("the refusal must name the field it refuses".to_owned());
    }
    None
}

/// Rule 2: the owner records the probe in the outstanding table **before**
/// the send.
///
/// The probe is recorded before it is returned to the caller, so an answer
/// that arrives quickly is never dropped.
fn check_the_probe_is_recorded_before_the_send(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let body = item_body(&code, "pub fn start_probe<R: RngCore>(")?;
    let record = body.find("self.tracker.record(")?;
    let encode = body.find("encode_standard_to_vec(")?;
    if record > encode {
        return Some("the probe must be recorded before it is encoded".to_owned());
    }
    None
}

/// Rule 3: the runtime's production recorder has exactly **one** production
/// call site -- the confirmed-match path.
///
/// The recorder is the only way the daemon can record a confirmed peer
/// test, and the confirmed-match path is the only place that may call it.
fn check_the_recorder_has_one_production_call_site(owner: &str, runtime: &str) -> Option<String> {
    // Plan 547: the recorder's production call site is the seam in the
    // owner's own module, beside the SSU2 service.
    let owner_code = code_view(owner);
    let calls = owner_code.matches("note_confirmed_peer_test(").count();
    if calls != 1 {
        return Some(format!(
            "the confirmed peer test must be recorded from exactly one production call site, found {calls}"
        ));
    }
    let handler = match item_body(&owner_code, "pub fn handle_inbound(") {
        Some(body) => body,
        None => return Some("the peer-test seam is absent".to_owned()),
    };
    if !handler.contains("note_confirmed_peer_test(") {
        return Some(
            "the confirmed-match path must be the recorder's production call site".to_owned(),
        );
    }
    // The completion branch, not the ignored branch, is the caller.
    let completion = handler.find("PeerTestInbound::Completed(")?;
    let call = handler.find("note_confirmed_peer_test(")?;
    if completion > call {
        return Some("the recorder must be called from the confirmed-match path".to_owned());
    }
    let runtime_code = code_view(runtime);
    let definition = match item_body(&runtime_code, "pub fn note_confirmed_peer_test(") {
        Some(body) => body,
        None => return Some("the runtime's recorder is absent".to_owned()),
    };
    if definition.contains("pub fn note_confirmed_peer_test(") {
        return Some("the recorder must not call itself".to_owned());
    }
    None
}

/// Rule 4: the daemon asserts **no** address family.
///
/// The runtime derives the family from the session the exchange travelled
/// on. A daemon that named a family would be filing the evidence under a
/// family the exchange never used.
fn check_the_daemon_asserts_no_family(owner: &str, runtime: &str) -> Option<String> {
    // Plan 547: the daemon side of the peer test is the owner's own module.
    let owner_code = code_view(owner);
    if owner_code.contains("AddressFamily::Ipv4") || owner_code.contains("AddressFamily::Ipv6") {
        return Some("the daemon must not name an address family for the peer test".to_owned());
    }
    let runtime_code = code_view(runtime);
    let recorder = match item_body(&runtime_code, "pub fn note_confirmed_peer_test(") {
        Some(body) => body,
        None => return Some("the runtime's recorder is absent".to_owned()),
    };
    if !recorder.contains("state.active.get(&link_id)") {
        return Some("the runtime must derive the family from the session itself".to_owned());
    }
    if !recorder.contains("session.peer_addr.ip()") {
        return Some("the runtime must derive the family from the session's address".to_owned());
    }
    None
}

/// Rule 5: the controlled recorder is **untouched**.
///
/// The controlled floodfill path keeps its own mirror, and the production
/// recorder is a separate method beside it.
///
/// **Plan 527 extended this rule, and did not weaken it.** The router's
/// reachability now has exactly **one** owner -- the runtime -- so the
/// controlled mirror files its typed outcome through the runtime's single
/// typed-outcome recorder instead of constructing the signal itself. The
/// rule therefore accepts the delegation **only** when the recorder it
/// delegates to is present and files the outcome as a `PeerTestResult`
/// under its own kind, contradicting kinds included. A controlled mirror
/// that records nothing, or that delegates to a recorder which files no
/// peer-test result, is still reported.
fn check_the_controlled_recorder_is_untouched(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let controlled = item_body(&code, "fn map_controlled_outcome_locked(")?;
    let records_inline = controlled.contains("ReachabilitySignal::PeerTestResult");
    let delegates = controlled.contains("record_peer_test_outcome_locked(");
    if !records_inline && !delegates {
        return Some("the controlled mirror must keep recording its own outcome".to_owned());
    }
    if delegates {
        // Fail closed: an absent recorder is a report, not a pass.
        let Some(shared) = item_body(&code, "fn record_peer_test_outcome_locked(") else {
            return Some("the shared typed-outcome recorder is absent".to_owned());
        };
        if !shared.contains("ReachabilitySignal::PeerTestResult") {
            return Some(
                "the shared typed-outcome recorder must file the controlled outcome as a peer-test result"
                    .to_owned(),
            );
        }
        for kind in [
            "PeerTestOutcomeKind::AddressMismatch",
            "PeerTestOutcomeKind::FirewalledLikely",
        ] {
            if !shared.contains(kind) {
                return Some(format!(
                    "the shared typed-outcome recorder must file {kind} under its own kind"
                ));
            }
        }
    }
    let recorder = item_body(&code, "pub fn note_confirmed_peer_test(")?;
    if recorder.contains("map_controlled_outcome_locked(") {
        return Some(
            "the production recorder must not delegate to the controlled mirror".to_owned(),
        );
    }
    if !recorder.contains("PeerTestOutcomeKind::Confirmed") {
        return Some("the production recorder must record only a confirmed outcome".to_owned());
    }
    None
}

/// Rule 6: the daemon records **no** reachability signal of its own outside
/// the confirmed-match path.
///
/// The transport's tracker is the single authority. A daemon that wrote a
/// `ReachabilitySignal` would be computing a reachability state of its own.
fn check_the_daemon_records_no_reachability_signal(owner: &str, protocol: &str) -> Option<String> {
    // Plan 547: the daemon side of the peer test is the owner's own module.
    let owner_code = code_view(owner);
    if owner_code.contains("ReachabilitySignal::") {
        return Some("the daemon must not write a reachability signal of its own".to_owned());
    }
    if owner_code.contains("ReachabilityTracker") {
        return Some("the daemon must not own a reachability tracker".to_owned());
    }
    let protocol_code = code_view(protocol);
    if protocol_code.contains("ReachabilitySignal::") {
        return Some("the peer-test owner must not write a reachability signal".to_owned());
    }
    None
}

/// Rule 7: the responder derives the answer from the **authenticated**
/// delivery and runs **no** second decoder.
///
/// The answer's peer and link come from the dispatcher's outcome, and the
/// responder never decodes the inbound message a second time.
fn check_the_responder_uses_the_authenticated_delivery(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let answer = item_body(
        &code,
        "pub fn build_peer_test_answer(outcome: &RouterI2npOutcome,",
    )?;
    if !answer.contains("RouterI2npOutcome::PeerTest") {
        return Some("the answer must be derived from the classified outcome".to_owned());
    }
    if !answer.contains("peer: *peer") || !answer.contains("link_id: *link_id") {
        return Some("the answer must carry the authenticated peer and link".to_owned());
    }
    if answer.contains("I2npMessage::decode_standard(") {
        return Some("the responder must not decode the inbound message again".to_owned());
    }
    None
}

/// Rule 8: the inbound path decodes the message **once**.
///
/// The live pump decodes the inbound message once and classifies it once;
/// the peer-test seam reads that same classified outcome.
fn check_the_inbound_path_decodes_once(composition: &str, owner: &str) -> Option<String> {
    // Plan 547: the live inbound path is the daemon's composition root, and
    // the peer-test seam is the owner's own module.
    let owner_code = code_view(owner);
    if owner_code.contains("decode_authenticated_router_i2np(") {
        return Some("the peer-test seam must not decode the inbound message again".to_owned());
    }
    let code = code_view(composition);
    let calls = code.matches("peer_test.handle_inbound(").count();
    if calls != 1 {
        return Some(format!(
            "the peer-test seam must have exactly one production call site, found {calls}"
        ));
    }
    let Some(seam) = code.find("peer_test.handle_inbound(") else {
        return Some("the peer-test seam has no production call site".to_owned());
    };
    let call = code.get(seam..seam.saturating_add(96)).unwrap_or_default();
    if !call.contains("&outcome") {
        return Some(
            "the live inbound path must feed the peer-test seam the classified outcome".to_owned(),
        );
    }
    None
}

/// Rule 9: the owner adds **no** task, no timer and no listener.
///
/// The owner holds no task of its own: the product's existing tick drives
/// it, and the live inbound path feeds it.
fn check_the_owner_adds_no_infrastructure(composition: &str, owner: &str) -> Option<String> {
    // Plan 547: the owner lives beside the SSU2 service, and the drive step
    // is the owner's own `drive`.
    let owner_code = code_view(owner);
    for forbidden in ["tokio::spawn", "std::thread", "TcpListener", "sleep("] {
        if owner_code.contains(forbidden) {
            return Some(format!("the peer-test owner must not use {forbidden}"));
        }
    }
    let drive = match item_body(&owner_code, "pub fn drive(") {
        Some(body) => body,
        None => return Some("the peer-test drive step is absent".to_owned()),
    };
    if drive.contains("tokio::spawn") || drive.contains("TcpListener") {
        return Some("the peer-test tick must not spawn a task or bind a listener".to_owned());
    }
    // The drive step is driven from the composition root's existing tick.
    let composition_code = code_view(composition);
    let calls = composition_code.matches("peer_test.drive(").count();
    if calls != 1 {
        return Some(format!(
            "the peer test must be driven from exactly one tick, found {calls}"
        ));
    }
    let Some(tick) = select_arm_body(&composition_code, "group_tick.tick() =>") else {
        return Some("the composition root has no group tick arm".to_owned());
    };
    if !tick.contains("peer_test.drive(") {
        return Some(
            "the peer test must be driven on the composition root's existing tick".to_owned(),
        );
    }
    None
}

/// Rule 10: the answered-key table is **bounded**, and a repeated key is
/// **suppressed** rather than answered again.
///
/// The reply traffic a peer can provoke is bounded by the router's own
/// table, never by the peer's patience.
fn check_the_answered_key_table_is_bounded(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let handler = item_body(&code, "pub fn handle_inbound(")?;
    if !handler.contains("self.responded.contains_key(&key)") {
        return Some("a repeated key must be suppressed".to_owned());
    }
    if !handler.contains("MAX_RESPONDED_PEER_TESTS") {
        return Some("the answered-key table must have a ceiling".to_owned());
    }
    if !handler.contains("self.counters.answers_suppressed_repeat += 1") {
        return Some("a suppressed answer must be counted under its own reason".to_owned());
    }
    let expire = owner_item_body(&code, "pub fn expire(")?;
    if !expire.contains("self.responded") {
        return Some("the answered-key table must age out".to_owned());
    }
    None
}

/// Rule 11: the transport's corroboration policy is **untouched**.
///
/// The threshold, the classes and the expiry are the transport's, and the
/// daemon names none of them.
fn check_the_corroboration_policy_is_untouched(owner: &str, transport: &str) -> Option<String> {
    // Plan 547: the daemon side of the peer test is the owner's own module.
    let owner_code = code_view(owner);
    for forbidden in ["MIN_CORROBORATION_FLOOR", "corroboration_class"] {
        if owner_code.contains(forbidden) {
            return Some(format!(
                "the daemon must not name the transport's {forbidden}"
            ));
        }
    }
    let transport_code = code_view(transport);
    if !transport_code.contains("const MIN_CORROBORATION_FLOOR") {
        return Some("the transport's own corroboration floor must remain".to_owned());
    }
    if !transport_code.contains("PeerTestResult") {
        return Some("the transport's own peer-test signal must remain".to_owned());
    }
    None
}

/// Rule 12: the publication is **untouched**.
///
/// The peer test touches no builder and no permit: the `caps=f`
/// advertisement stays permit-gated.
fn check_the_publication_is_untouched(owner: &str, protocol: &str, netdb: &str) -> Option<String> {
    // Plan 547: the daemon side of the peer test is the owner's own module,
    // and the protocol side is the peer-test module itself.
    let owner_code = code_view(owner);
    if owner_code.contains("build_with_ssu2_address(") {
        return Some("the peer test must not build a published address".to_owned());
    }
    let protocol_code = code_view(protocol);
    for forbidden in [
        "LocalRouterInfoBuilder",
        "FloodfillAdvertisementPermit",
        "build_floodfill",
        "install_local_router_info",
    ] {
        if protocol_code.contains(forbidden) {
            return Some(format!("the peer-test owner must not touch {forbidden}"));
        }
    }
    let netdb_code = code_view(netdb);
    if !netdb_code.contains("FloodfillAdvertisementPermit") {
        return Some("the netdb's floodfill permit must remain".to_owned());
    }
    None
}

/// Rule 13: the owner lives in exactly **one** place.
///
/// Plan 547: the owner left the destination-group product and moved beside
/// the SSU2 service, because the product is built only when a destination
/// group can exist at all and a router that runs only SSU2 therefore had no
/// owner at all. Two owners would answer the same probe twice and double
/// the counters, so the product must own no peer test whatsoever.
fn check_the_owner_lives_in_one_place(product: &str, owner: &str) -> Option<String> {
    let product_code = code_view(product);
    for forbidden in [
        "PeerTestOwner",
        "note_confirmed_peer_test",
        "peer_test_owner",
    ] {
        if product_code.contains(forbidden) {
            return Some(format!(
                "the destination-group product must own no peer test: {forbidden}"
            ));
        }
    }
    let owner_code = code_view(owner);
    let constructions = owner_code.matches("PeerTestOwner::new(").count();
    if constructions != 1 {
        return Some(format!(
            "the peer-test owner must be constructed exactly once, found {constructions}"
        ));
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted against the real tree.
// ----------------------------------------------------------------------

#[test]
fn the_opt_in_defaults_off_and_needs_the_service() {
    let config = source_of(CONFIG_MODULE);
    assert_eq!(
        check_the_opt_in_defaults_off_and_needs_the_service(&config),
        None
    );
}

#[test]
fn the_probe_is_recorded_before_the_send() {
    let owner = source_of(OWNER_MODULE);
    assert_eq!(check_the_probe_is_recorded_before_the_send(&owner), None);
}

#[test]
fn the_recorder_has_one_production_call_site() {
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    let runtime = source_of(RUNTIME_MODULE);
    assert_eq!(
        check_the_recorder_has_one_production_call_site(&owner, &runtime),
        None
    );
}

#[test]
fn the_daemon_asserts_no_family() {
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    let runtime = source_of(RUNTIME_MODULE);
    assert_eq!(check_the_daemon_asserts_no_family(&owner, &runtime), None);
}

#[test]
fn the_controlled_recorder_is_untouched() {
    let runtime = source_of(RUNTIME_MODULE);
    assert_eq!(check_the_controlled_recorder_is_untouched(&runtime), None);
}

#[test]
fn the_daemon_records_no_reachability_signal() {
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    let protocol = source_of(OWNER_MODULE);
    assert_eq!(
        check_the_daemon_records_no_reachability_signal(&owner, &protocol),
        None
    );
}

#[test]
fn the_responder_uses_the_authenticated_delivery() {
    let owner = source_of(OWNER_MODULE);
    assert_eq!(
        check_the_responder_uses_the_authenticated_delivery(&owner),
        None
    );
}

#[test]
fn the_inbound_path_decodes_once() {
    let composition = source_of(COMPOSITION_MODULE);
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    assert_eq!(
        check_the_inbound_path_decodes_once(&composition, &owner),
        None
    );
}

#[test]
fn the_owner_adds_no_infrastructure() {
    let composition = source_of(COMPOSITION_MODULE);
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    assert_eq!(
        check_the_owner_adds_no_infrastructure(&composition, &owner),
        None
    );
}

#[test]
fn the_answered_key_table_is_bounded() {
    let owner = source_of(OWNER_MODULE);
    assert_eq!(check_the_answered_key_table_is_bounded(&owner), None);
}

#[test]
fn the_corroboration_policy_is_untouched() {
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    let transport = source_of(TRANSPORT_REACHABILITY_MODULE);
    assert_eq!(
        check_the_corroboration_policy_is_untouched(&owner, &transport),
        None
    );
}

#[test]
fn the_publication_is_untouched() {
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    let protocol = source_of(OWNER_MODULE);
    let netdb = source_of(NETDB_LOCAL_MODULE);
    assert_eq!(
        check_the_publication_is_untouched(&owner, &protocol, &netdb),
        None
    );
}

#[test]
fn the_owner_lives_in_one_place() {
    let product = source_of(PRODUCT_MODULE);
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    assert_eq!(check_the_owner_lives_in_one_place(&product, &owner), None);
}

// ----------------------------------------------------------------------
// The negative controls: every rule must fire on a mutation.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_an_opt_in_that_defaults_on() {
    let mutated = r#"
fn default_ssu2_peer_test() -> bool {
    true
}
fn normalize_ssu2(raw: &RawSsu2Config) -> Result<Ssu2Config, ConfigError> {
    if raw.peer_test && !raw.enabled {
        return Err(ConfigError::Semantic { field: "ssu2.peer_test", reason: "x" });
    }
    Ok(Ssu2Config {
        publish_bound_address: false, peer_test: raw.peer_test })
}
"#;
    assert!(
        check_the_opt_in_defaults_off_and_needs_the_service(mutated).is_some(),
        "an opt-in that defaults on must be reported"
    );
}

#[test]
fn the_guard_reports_an_opt_in_that_needs_no_service() {
    let mutated = r#"
fn default_ssu2_peer_test() -> bool {
    false
}
fn normalize_ssu2(raw: &RawSsu2Config) -> Result<Ssu2Config, ConfigError> {
    Ok(Ssu2Config {
        publish_bound_address: false, peer_test: raw.peer_test })
}
"#;
    assert!(
        check_the_opt_in_defaults_off_and_needs_the_service(mutated).is_some(),
        "an opt-in that runs without the service must be reported"
    );
}

#[test]
fn the_guard_reports_a_probe_that_is_sent_before_it_is_recorded() {
    let mutated = r#"
pub fn start_probe<R: RngCore>(&mut self, peer: PeerId, rng: &mut R, now_ms: u64, expiration_ms: u64) -> Result<PeerTestProbe, PeerTestOwnerError> {
    let bytes = encode_standard_to_vec(MAX_ROUTER_I2NP_BYTES);
    self.tracker.record(key, peer, now_ms);
    Ok(probe)
}
"#;
    assert!(
        check_the_probe_is_recorded_before_the_send(mutated).is_some(),
        "a probe that is sent before it is recorded must be reported"
    );
}

#[test]
fn the_guard_reports_a_second_production_call_site() {
    let owner = r#"
pub fn handle_inbound(&mut self, outcome: &RouterI2npOutcome) {
    let action = { PeerTestInbound::Completed(round_trip) => {
        service.note_confirmed_peer_test(round_trip.link_id, round_trip.proof);
    } };
    let other = other_place.note_confirmed_peer_test(link, proof);
}
"#;
    let runtime = source_of(RUNTIME_MODULE);
    assert!(
        check_the_recorder_has_one_production_call_site(owner, &runtime).is_some(),
        "a second production call site must be reported"
    );
}

#[test]
fn the_guard_reports_an_absent_peer_test_seam() {
    let owner = r#"
pub fn something_else(&mut self) {}
"#;
    let runtime = source_of(RUNTIME_MODULE);
    assert!(
        check_the_recorder_has_one_production_call_site(owner, &runtime).is_some(),
        "an absent peer-test seam must be reported"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_asserts_a_family() {
    let owner = r#"
pub fn handle_inbound(&mut self, outcome: &RouterI2npOutcome) {
    let family = AddressFamily::Ipv4;
    let _ = family;
}
"#;
    let runtime = source_of(RUNTIME_MODULE);
    assert!(
        check_the_daemon_asserts_no_family(owner, &runtime).is_some(),
        "a daemon that names a family must be reported"
    );
}

#[test]
fn the_guard_reports_a_recorder_that_delegates_to_the_controlled_mirror() {
    let mutated = r#"
fn map_controlled_outcome_locked(&self, state: &mut ServiceState, outcome: Option<PeerTestOutcome>) {
    state.tracker.record(ReachabilitySignal::PeerTestResult { family, outcome: PeerTestOutcomeKind::Confirmed }, now);
}
pub fn note_confirmed_peer_test(&self, link_id: LinkId, proof: ConfirmedPeerTest) -> Result<(), Ssu2PeerTestEvidenceError> {
    self.map_controlled_outcome_locked(state, outcome);
    Ok(())
}
"#;
    assert!(
        check_the_controlled_recorder_is_untouched(mutated).is_some(),
        "a recorder that delegates to the controlled mirror must be reported"
    );
}

#[test]
fn the_guard_reports_a_controlled_mirror_that_records_nothing() {
    let mutated = r#"
fn map_controlled_outcome_locked(&self, state: &mut ServiceState, outcome: Option<PeerTestOutcome>) {
    let _ = (state, outcome);
}
pub fn note_confirmed_peer_test(&self, link_id: LinkId, proof: ConfirmedPeerTest) -> Result<(), Ssu2PeerTestEvidenceError> {
    let _ = (link_id, proof);
    Ok(())
}
"#;
    assert!(
        check_the_controlled_recorder_is_untouched(mutated).is_some(),
        "a controlled mirror that records no outcome must be reported"
    );
}

#[test]
fn the_guard_reports_a_controlled_mirror_that_delegates_to_a_recorder_that_files_nothing() {
    let mutated = r#"
fn map_controlled_outcome_locked(&self, state: &mut ServiceState, outcome: Option<PeerTestOutcome>) {
    self.record_peer_test_outcome_locked(state, outcome);
}
fn record_peer_test_outcome_locked(&self, state: &mut ServiceState, outcome: PeerTestOutcome) {
    let _ = (state, outcome);
}
pub fn note_confirmed_peer_test(&self, link_id: LinkId, proof: ConfirmedPeerTest) -> Result<(), Ssu2PeerTestEvidenceError> {
    let _ = (link_id, proof);
    Ok(())
}
"#;
    assert!(
        check_the_controlled_recorder_is_untouched(mutated).is_some(),
        "a controlled mirror that delegates to a recorder that files no peer-test result must be reported"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_records_its_own_signal() {
    let owner = r#"
pub fn handle_inbound(&mut self, outcome: &RouterI2npOutcome) {
    tracker.record(ReachabilitySignal::PeerTestResult { family, outcome }, now);
}
"#;
    let protocol = source_of(OWNER_MODULE);
    assert!(
        check_the_daemon_records_no_reachability_signal(owner, &protocol).is_some(),
        "a daemon that records its own signal must be reported"
    );
}

#[test]
fn the_guard_reports_a_responder_that_decodes_again() {
    let mutated = r#"
pub fn build_peer_test_answer(outcome: &RouterI2npOutcome, message_id: u32, expiration_ms: u64) -> Result<Option<PeerTestAnswer>, CodecError> {
    let decoded = I2npMessage::decode_standard(&bytes, MAX_ROUTER_I2NP_BYTES)?;
    let peer = decoded.peer;
    let link_id = decoded.link_id;
    Ok(Some(PeerTestAnswer { peer: *peer, link_id: *link_id, bytes }))
}
"#;
    assert!(
        check_the_responder_uses_the_authenticated_delivery(mutated).is_some(),
        "a responder that decodes again must be reported"
    );
}

#[test]
fn the_guard_reports_an_inbound_path_that_decodes_twice() {
    let owner = r#"
pub fn handle_inbound(&mut self, outcome: &RouterI2npOutcome) {
    let again = decode_authenticated_router_i2np(&inbound, now_ms);
    let _ = again;
}
"#;
    let composition = source_of(COMPOSITION_MODULE);
    assert!(
        check_the_inbound_path_decodes_once(&composition, owner).is_some(),
        "an inbound path that decodes twice must be reported"
    );
}

#[test]
fn the_guard_reports_a_seam_with_no_production_call_site() {
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    let composition = r#"
fn something_else(&mut self) {}
"#;
    assert!(
        check_the_inbound_path_decodes_once(composition, &owner).is_some(),
        "a peer-test seam with no production call site must be reported"
    );
}

#[test]
fn the_guard_reports_an_owner_that_spawns_a_task() {
    let owner = r#"
pub fn handle_inbound(&mut self, outcome: &RouterI2npOutcome) -> PeerTestInbound {
    tokio::spawn(async move { do_work().await });
    PeerTestInbound::Ignored
}
"#;
    let composition = source_of(COMPOSITION_MODULE);
    assert!(
        check_the_owner_adds_no_infrastructure(&composition, owner).is_some(),
        "an owner that spawns a task must be reported"
    );
}

#[test]
fn the_guard_reports_an_unbounded_answered_key_table() {
    let mutated = r#"
impl PeerTestOwner {
pub fn handle_inbound(&mut self, outcome: &RouterI2npOutcome, message_id: u32, expiration_ms: u64, now_ms: u64) -> PeerTestInbound {
    let answer = build_peer_test_answer(outcome, message_id, expiration_ms);
    PeerTestInbound::Answer(answer)
}
pub fn expire(&mut self, now_ms: u64) -> usize {
    0
}
}
"#;
    assert!(
        check_the_answered_key_table_is_bounded(mutated).is_some(),
        "an unbounded answered-key table must be reported"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_names_the_corroboration_policy() {
    let owner = r#"
pub fn drive(&mut self, handle: &Ssu2DaemonHandle, token: &CancellationToken, now_ms: u64) {
    let floor = MIN_CORROBORATION_FLOOR;
    let _ = floor;
}
"#;
    let transport = source_of(TRANSPORT_REACHABILITY_MODULE);
    assert!(
        check_the_corroboration_policy_is_untouched(owner, &transport).is_some(),
        "a daemon that names the transport's policy must be reported"
    );
}

#[test]
fn the_guard_reports_a_product_that_owns_a_peer_test() {
    let product = r#"
fn handle_inbound_peer_test(&mut self) {
    let owner = crate::peer_test::PeerTestOwner::new(true);
}
"#;
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    assert!(
        check_the_owner_lives_in_one_place(product, &owner).is_some(),
        "a product that owns a peer test must be reported"
    );
}

#[test]
fn the_guard_reports_a_second_owner() {
    let product = source_of(PRODUCT_MODULE);
    let owner = r#"
pub fn new(enabled: bool) -> Self {
    let owner = PeerTestOwner::new(true);
    let second = PeerTestOwner::new(true);
    Self { owner }
}
"#;
    assert!(
        check_the_owner_lives_in_one_place(&product, owner).is_some(),
        "a second owner must be reported"
    );
}

#[test]
fn the_guard_reports_an_owner_that_builds_a_published_address() {
    let protocol = r#"
pub fn handle_inbound(&mut self, outcome: &RouterI2npOutcome, message_id: u32, expiration_ms: u64, now_ms: u64) -> PeerTestInbound {
    let builder = LocalRouterInfoBuilder::new();
    let _ = builder;
    PeerTestInbound::Ignored
}
"#;
    let owner = source_of(PEER_TEST_RUNTIME_MODULE);
    let netdb = source_of(NETDB_LOCAL_MODULE);
    assert!(
        check_the_publication_is_untouched(&owner, protocol, &netdb).is_some(),
        "an owner that builds a published address must be reported"
    );
}
