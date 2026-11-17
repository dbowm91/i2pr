//! Plan 524's structural guard for the router's relay-tag request.
//!
//! A router behind a firewall needs an introducer. The pinned i2pd 2.61.0
//! asks for a relay tag under exactly one condition
//! (`libi2pd/SSU2Session.cpp:753`): the router is **firewalled** and the
//! peer **advertises the introducer capability**
//! (`libi2pd/RouterInfo.h:184`).
//!
//! The guard asserts fifteen things about the **production** surface:
//!
//! 1. the opt-in defaults to **off**, and it is refused without the SSU2
//!    service;
//! 2. the capability read comes from the **validated** store, never from
//!    unvalidated bytes;
//! 3. the capability test is the peer's **advertised** `C`, never its
//!    published introducer set;
//! 4. the router asks only while it is **firewalled**;
//! 5. the request rides an **existing** session: the peer is drawn from
//!    the peers with a live session, and no new connection is made;
//! 6. **at most one** request leaves per window;
//! 7. the daemon never writes the block itself: the transport owns the
//!    vocabulary;
//! 8. the owner adds **no** task, no timer, no listener and no socket;
//! 9. the candidate window is **bounded** and **rotates**;
//! 10. a peer is asked **at most once** while its session lives;
//! 11. every refusal is counted under its **own** reason;
//! 12. the **publication** is untouched: the acquisition publishes
//!     nothing and advertises no `C`;
//! 13. the runtime records only a tag that answers **this router's own**
//!     request;
//! 14. the daemon's drain **skips** the tag answer, so the role owner is
//!     not charged with a refusal it did not make;
//! 15. the runtime's outstanding-request set is **bounded** and refuses
//!     rather than evicts.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required **call**, with its open parenthesis. Comments
//! are stripped and `#[cfg(test)]` blocks are excluded, so a doc comment
//! that merely names a construct never satisfies a rule, and a function that
//! is **absent** fails closed.
//!
//! Every rule has an **inverted probe**: a mutation that must be reported. A
//! rule with no probe is a rule nobody has shown to fire. Every probe
//! mutates an **in-memory copy** of the source, so no file on disk is
//! written.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The daemon's owner of this router's relay-tag requests.
const OWNER_MODULE: &str = "crates/i2pr-daemon/src/introducer_request.rs";
/// The daemon's live inbound path and its tick.
const PRODUCT_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";
/// The daemon's configuration surface.
const CONFIG_MODULE: &str = "crates/i2pr-daemon/src/config.rs";
/// The daemon's relay drain.
const DRAIN_MODULE: &str = "crates/i2pr-daemon/src/ssu2_relay_drain.rs";
/// The runtime's own session state and its recorder.
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
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

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1: the opt-in defaults off, and it is refused without the SSU2
/// service.
///
/// The switch is a real opt-in: the default configuration leaves it off, and
/// a configuration that asks for a relay tag without the service it needs
/// is refused at parse time rather than silently inert.
fn check_the_opt_in_defaults_off_and_needs_the_service(config: &str) -> Option<String> {
    let code = code_view(config);
    let Some(default) = item_body(&code, "fn default_ssu2_introducer_request(") else {
        return Some("the relay-tag request default must exist".to_owned());
    };
    if !default.contains("false") {
        return Some("the relay-tag request default must be off".to_owned());
    }
    let Some(normalize) = item_body(&code, "fn normalize_ssu2(raw: &RawSsu2Config)") else {
        return Some("the SSU2 configuration's own normalization must exist".to_owned());
    };
    if !normalize.contains("raw.introducer_request && !raw.enabled") {
        return Some(
            "the relay-tag request opt-in must be refused when the SSU2 service is off".to_owned(),
        );
    }
    if !normalize.contains("ssu2.introducer_request") {
        return Some("the refusal must name the field it refuses".to_owned());
    }
    None
}

/// Rule 2: the capability read comes from the **validated** store.
///
/// The bytes the read consults are the ones the netdb validated on ingest —
/// the signature, the identity hash, the static-key binding and the
/// freshness were all checked — and the address is parsed with the
/// transport's own strict parser.
fn check_the_capability_read_uses_the_validated_store(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(body) = item_body(&code, "pub fn read_capability") else {
        return Some("the capability read must exist".to_owned());
    };
    if !body.contains("store.get(") {
        return Some("the capability read must use the store's validated lookup".to_owned());
    }
    if !body.contains("Ssu2RouterAddress::parse(") {
        return Some("the capability read must use the transport's strict parser".to_owned());
    }
    if body.contains("decode_standard") || body.contains("RouterAddress::decode") {
        return Some("the capability read must not decode unvalidated bytes".to_owned());
    }
    None
}

/// Rule 3: the capability test is the peer's **advertised** `C`.
///
/// The reference's own test is `caps & eSSUIntroducer`
/// (`libi2pd/RouterInfo.h:184`). A peer that *publishes* introducers is a
/// peer that *has* introducers, not a peer that is willing to *be* one.
fn check_the_capability_test_is_the_advertised_one(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(body) = item_body(&code, "pub fn read_capability") else {
        return Some("the capability read must exist".to_owned());
    };
    if !body.contains("supports_relay(") {
        return Some(
            "the capability test must be the peer's advertised introducer capability".to_owned(),
        );
    }
    if body.contains(".introducers()") {
        return Some(
            "the capability test must not be the peer's published introducer set".to_owned(),
        );
    }
    None
}

/// Rule 4: the router asks only while it is **firewalled**.
///
/// The reference's condition is `eRouterStatusFirewalled &&
/// m_Address->IsIntroducer()` (`libi2pd/SSU2Session.cpp:753`). A reachable
/// router needs no introducer, so it does not ask.
fn check_the_router_asks_only_when_firewalled(product: &str, owner: &str) -> Option<String> {
    let product_code = code_view(product);
    let Some(driver) = item_body(&product_code, "async fn drive_introducer_request(") else {
        return Some("the tick driver must exist".to_owned());
    };
    if !driver.contains("ReachabilityState::Firewalled") {
        return Some("the driver must read the router's own firewalled state".to_owned());
    }
    let owner_code = code_view(owner);
    let Some(decide) = item_body(&owner_code, "pub fn decide(") else {
        return Some("the owner's decision must exist".to_owned());
    };
    if !decide.contains("firewalled") {
        return Some("the owner must refuse every window that is not firewalled".to_owned());
    }
    if !decide.contains("requests_refused_not_firewalled") {
        return Some("the refusal must be counted under its own reason".to_owned());
    }
    None
}

/// Rule 5: the request rides an **existing** session.
///
/// The peer is drawn from the peers the runtime already holds a session
/// with, and the driver never dials: a request that needed a new connection
/// would be a new connection the plan does not make.
fn check_the_request_rides_an_existing_session(product: &str) -> Option<String> {
    let code = code_view(product);
    let Some(driver) = item_body(&code, "async fn drive_introducer_request(") else {
        return Some("the tick driver must exist".to_owned());
    };
    if !driver.contains("active_peer_hashes(") {
        return Some("the driver must draw its peers from the live sessions".to_owned());
    }
    if driver.contains("dial_ssu2(") || driver.contains("dial(") {
        return Some("the driver must not make a new connection".to_owned());
    }
    None
}

/// Rule 6: **at most one** request leaves per window.
fn check_at_most_one_request_leaves_per_window(product: &str) -> Option<String> {
    let code = code_view(product);
    let Some(driver) = item_body(&code, "async fn drive_introducer_request(") else {
        return Some("the tick driver must exist".to_owned());
    };
    let calls = driver.matches("queue_relay_tag_request(").count();
    if calls != 1 {
        return Some(format!(
            "the window must queue exactly one relay-tag request, found {calls}"
        ));
    }
    None
}

/// Rule 7: the daemon never writes the block itself.
///
/// The request block is the one the transport already builds. A daemon that
/// encoded its own block would own a piece of the wire vocabulary.
fn check_the_daemon_never_writes_the_block(product: &str, owner: &str) -> Option<String> {
    for (label, source) in [("product", product), ("owner", owner)] {
        let code = code_view(source);
        for forbidden in [
            "RelayTagRequest",
            "RelayTagBlock",
            "Block::RelayTag",
            "encode_blocks",
        ] {
            if code.contains(forbidden) {
                return Some(format!(
                    "the {label} must not write the relay-tag block itself"
                ));
            }
        }
    }
    None
}

/// Rule 8: the owner adds **no** infrastructure.
///
/// The owner is a plain value the product's tick advances: it owns no task,
/// no timer, no listener and no socket.
fn check_the_owner_adds_no_infrastructure(owner: &str) -> Option<String> {
    let code = code_view(owner);
    for forbidden in [
        "tokio::spawn",
        "tokio::time::",
        "TcpListener",
        "std::net",
        "std::fs",
        "std::thread",
    ] {
        if code.contains(forbidden) {
            return Some(format!("the owner must not own {forbidden}"));
        }
    }
    None
}

/// Rule 9: the candidate window is **bounded** and **rotates**.
///
/// The window is a bound, not a target: a router with more peers than the
/// bound examines the rest over the following windows, so one window's work
/// never grows with the router's peer population.
fn check_the_window_is_bounded_and_rotates(product: &str, owner: &str) -> Option<String> {
    let owner_code = code_view(owner);
    let Some(window) = item_body(&owner_code, "pub fn candidate_window(") else {
        return Some("the candidate window must exist".to_owned());
    };
    if !window.contains(".take(limit)") {
        return Some("the candidate window must be bounded".to_owned());
    }
    if !window.contains("self.cursor") {
        return Some("the candidate window must rotate".to_owned());
    }
    let product_code = code_view(product);
    let Some(driver) = item_body(&product_code, "async fn drive_introducer_request(") else {
        return Some("the tick driver must exist".to_owned());
    };
    if !driver.contains("MAX_INTRODUCER_CANDIDATES") {
        return Some("the driver must use the owner's own window bound".to_owned());
    }
    None
}

/// Rule 10: a peer is asked **at most once** while its session lives.
fn check_a_peer_is_asked_at_most_once(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(queued) = item_body(&code, "pub fn note_queued(") else {
        return Some("the queued-request recorder must exist".to_owned());
    };
    if !queued.contains("self.asked.insert(") {
        return Some("a queued request must record the peer as asked".to_owned());
    }
    let Some(window) = item_body(&code, "pub fn candidate_window(") else {
        return Some("the candidate window must exist".to_owned());
    };
    if !window.contains("!self.asked.contains(peer)") {
        return Some("the window must exclude a peer that was already asked".to_owned());
    }
    if !window.contains("self.asked.retain(") {
        return Some("the asked set must be pruned against the live sessions".to_owned());
    }
    None
}

/// Rule 11: every refusal is counted under its **own** reason.
fn check_every_refusal_is_counted_under_its_own_reason(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(counters) = item_body(&code, "pub struct IntroducerRequestCounters") else {
        return Some("the counters must exist".to_owned());
    };
    for field in [
        "capability_refused_no_record",
        "capability_refused_no_ssu2_address",
        "capability_refused_unparsable",
        "capability_refused_not_advertised",
        "requests_refused_disabled",
        "requests_refused_not_firewalled",
        "requests_refused_no_eligible_peer",
        "requests_refused_transport",
    ] {
        if !counters.contains(field) {
            return Some(format!("the counters must carry {field}"));
        }
    }
    let Some(decide) = item_body(&code, "pub fn decide(") else {
        return Some("the owner's decision must exist".to_owned());
    };
    for counter in [
        "requests_refused_disabled",
        "requests_refused_not_firewalled",
        "requests_refused_no_eligible_peer",
    ] {
        if !decide.contains(counter) {
            return Some(format!("the decision must count {counter}"));
        }
    }
    let Some(transport) = item_body(&code, "pub fn note_transport_refusal(") else {
        return Some("the transport-refusal recorder must exist".to_owned());
    };
    if !transport.contains("requests_refused_transport") {
        return Some("a transport refusal must be counted under its own reason".to_owned());
    }
    None
}

/// Rule 12: the **publication** is untouched.
///
/// The acquisition publishes nothing: it builds no introducer-only address,
/// signs nothing, and advertises no `C` of its own. The publication and
/// the advertisement are successors' subjects.
fn check_the_publication_is_untouched(product: &str, owner: &str, netdb: &str) -> Option<String> {
    let product_code = code_view(product);
    let owner_code = code_view(owner);
    for (label, code) in [("product", &product_code), ("owner", &owner_code)] {
        for forbidden in [
            "introducer_publication_material",
            "build_with_ssu2_introducer_address",
            "LocalRouterInfoBuilder",
        ] {
            if code.contains(forbidden) {
                return Some(format!("the {label} must not touch the publication path"));
            }
        }
    }
    if owner_code.contains("\"caps\"") || owner_code.contains("'C'") {
        return Some("the acquisition must not advertise the introducer capability".to_owned());
    }
    let netdb_code = code_view(netdb);
    if !netdb_code.contains("build_with_ssu2_introducer_address") {
        return Some("the netdb's introducer-only builder must still exist".to_owned());
    }
    None
}

/// Rule 13: the runtime records only a tag that answers **this router's
/// own** request.
///
/// A peer may send the block unsolicited, and recording an introducer
/// nobody asked for would put an unauthenticated peer into the router's
/// introducer set.
fn check_the_runtime_records_only_an_asked_peer(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(arm) = item_body(&code, "SessionEvent::RelayTag(tag) =>") else {
        return Some("the runtime's relay-tag arm must exist".to_owned());
    };
    if !arm.contains("outstanding_tag_requests.remove(&peer)") {
        return Some("the recording must be gated on the router's own request".to_owned());
    }
    if !arm.contains("relay_tags_refused_unrequested") {
        return Some("an unsolicited tag must be counted under its own reason".to_owned());
    }
    if !arm.contains("record_authenticated_introducer(") {
        return Some("an asked tag must be recorded as an authenticated introducer".to_owned());
    }
    None
}

/// Rule 14: the daemon's drain **skips** the tag answer.
///
/// The handoff keeps the event — Plan 499's live-mesh evidence reads the
/// minted tag there — but the drain must not hand it to the role owner,
/// which never served it.
fn check_the_drain_skips_the_tag_answer(drain: &str) -> Option<String> {
    let code = code_view(drain);
    let Some(body) = item_body(&code, "pub fn drain_relay_events(") else {
        return Some("the drain must exist".to_owned());
    };
    if !body.contains("Ssu2RelayEvent::Tag(_)") {
        return Some("the drain must recognise the tag answer".to_owned());
    }
    if !body.contains("outcome.tag_answers") {
        return Some("the drain must count the tag answers it saw".to_owned());
    }
    let skip = body.find("Ssu2RelayEvent::Tag(_)")?;
    let role_owner = body.find("owner.on_relay_event(")?;
    if role_owner < skip {
        return Some("the tag answer must be skipped before the role owner sees it".to_owned());
    }
    None
}

/// Rule 15: the runtime's outstanding-request set is **bounded** and
/// refuses rather than evicts.
///
/// The set is a different bound from the introducer table's ceiling, and it
/// refuses: a request beyond the bound never queues and never evicts an
/// older request.
fn check_the_outstanding_set_is_bounded(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(body) = item_body(&code, "pub fn queue_relay_tag_request(") else {
        return Some("the runtime's queueing entry point must exist".to_owned());
    };
    if !body.contains("MAX_OUTSTANDING_TAG_REQUESTS") {
        return Some("the outstanding set must be bounded".to_owned());
    }
    if !body.contains("relay_tag_requests_refused_capacity") {
        return Some("a request beyond the bound must be counted under its own reason".to_owned());
    }
    if !body.contains("outstanding_tag_requests.insert(peer)") {
        return Some("a queued request must join the outstanding set".to_owned());
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted.
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
fn the_capability_read_uses_the_validated_store() {
    let owner = source_of(OWNER_MODULE);
    assert_eq!(
        check_the_capability_read_uses_the_validated_store(&owner),
        None
    );
}

#[test]
fn the_capability_test_is_the_advertised_one() {
    let owner = source_of(OWNER_MODULE);
    assert_eq!(
        check_the_capability_test_is_the_advertised_one(&owner),
        None
    );
}

#[test]
fn the_router_asks_only_when_firewalled() {
    let product = source_of(PRODUCT_MODULE);
    let owner = source_of(OWNER_MODULE);
    assert_eq!(
        check_the_router_asks_only_when_firewalled(&product, &owner),
        None
    );
}

#[test]
fn the_request_rides_an_existing_session() {
    let product = source_of(PRODUCT_MODULE);
    assert_eq!(check_the_request_rides_an_existing_session(&product), None);
}

#[test]
fn at_most_one_request_leaves_per_window() {
    let product = source_of(PRODUCT_MODULE);
    assert_eq!(check_at_most_one_request_leaves_per_window(&product), None);
}

#[test]
fn the_daemon_never_writes_the_block() {
    let product = source_of(PRODUCT_MODULE);
    let owner = source_of(OWNER_MODULE);
    assert_eq!(
        check_the_daemon_never_writes_the_block(&product, &owner),
        None
    );
}

#[test]
fn the_owner_adds_no_infrastructure() {
    let owner = source_of(OWNER_MODULE);
    assert_eq!(check_the_owner_adds_no_infrastructure(&owner), None);
}

#[test]
fn the_window_is_bounded_and_rotates() {
    let product = source_of(PRODUCT_MODULE);
    let owner = source_of(OWNER_MODULE);
    assert_eq!(
        check_the_window_is_bounded_and_rotates(&product, &owner),
        None
    );
}

#[test]
fn a_peer_is_asked_at_most_once() {
    let owner = source_of(OWNER_MODULE);
    assert_eq!(check_a_peer_is_asked_at_most_once(&owner), None);
}

#[test]
fn every_refusal_is_counted_under_its_own_reason() {
    let owner = source_of(OWNER_MODULE);
    assert_eq!(
        check_every_refusal_is_counted_under_its_own_reason(&owner),
        None
    );
}

#[test]
fn the_publication_is_untouched() {
    let product = source_of(PRODUCT_MODULE);
    let owner = source_of(OWNER_MODULE);
    let netdb = source_of(NETDB_LOCAL_MODULE);
    assert_eq!(
        check_the_publication_is_untouched(&product, &owner, &netdb),
        None
    );
}

#[test]
fn the_runtime_records_only_an_asked_peer() {
    let runtime = source_of(RUNTIME_MODULE);
    assert_eq!(check_the_runtime_records_only_an_asked_peer(&runtime), None);
}

#[test]
fn the_drain_skips_the_tag_answer() {
    let drain = source_of(DRAIN_MODULE);
    assert_eq!(check_the_drain_skips_the_tag_answer(&drain), None);
}

#[test]
fn the_outstanding_set_is_bounded() {
    let runtime = source_of(RUNTIME_MODULE);
    assert_eq!(check_the_outstanding_set_is_bounded(&runtime), None);
}

// ----------------------------------------------------------------------
// The negative controls: every rule must fire on a mutation.
// ----------------------------------------------------------------------

/// The guard's own fail-closed property: a rule whose named item is
/// **absent** must be **reported**, not passed.
///
/// The first draft of this guard returned "no defect" when the item a rule
/// names could not be found, which made a rule pass vacuously — and three
/// of the probes below did not fire because of it. Every rule now returns a
/// refusal when its named item is absent, and this probe proves it.
#[test]
fn the_guard_reports_a_missing_named_item() {
    assert!(
        check_the_opt_in_defaults_off_and_needs_the_service("").is_some(),
        "a rule whose named item is absent must be reported"
    );
    assert!(
        check_the_capability_read_uses_the_validated_store("").is_some(),
        "a rule whose named item is absent must be reported"
    );
    assert!(
        check_the_runtime_records_only_an_asked_peer("").is_some(),
        "a rule whose named item is absent must be reported"
    );
    assert!(
        check_the_outstanding_set_is_bounded("").is_some(),
        "a rule whose named item is absent must be reported"
    );
}

#[test]
fn the_guard_reports_an_opt_in_that_defaults_on() {
    let mutated = r#"
fn default_ssu2_introducer_request() -> bool {
    true
}
fn normalize_ssu2(raw: &RawSsu2Config) -> Result<Ssu2Config, ConfigError> {
    if raw.introducer_request && !raw.enabled {
        return Err(ConfigError::Semantic { field: "ssu2.introducer_request", reason: "x" });
    }
    Ok(Ssu2Config {
        publish_bound_address: false, introducer_request: raw.introducer_request })
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
fn default_ssu2_introducer_request() -> bool {
    false
}
fn normalize_ssu2(raw: &RawSsu2Config) -> Result<Ssu2Config, ConfigError> {
    Ok(Ssu2Config {
        publish_bound_address: false, introducer_request: raw.introducer_request })
}
"#;
    assert!(
        check_the_opt_in_defaults_off_and_needs_the_service(mutated).is_some(),
        "an opt-in that runs without the service must be reported"
    );
}

#[test]
fn the_guard_reports_a_read_of_unvalidated_bytes() {
    let mutated = r#"
pub fn read_capability(store: &RouterInfoStore, peer: PeerId) -> IntroducerCapability {
    let bytes = RouterAddress::decode(&raw)?;
    let parsed = Ssu2RouterAddress::parse(&bytes)?;
    if parsed.capabilities().supports_relay() { IntroducerCapability::Advertised } else { IntroducerCapability::NotAdvertised }
}
"#;
    assert!(
        check_the_capability_read_uses_the_validated_store(mutated).is_some(),
        "a read of unvalidated bytes must be reported"
    );
}

#[test]
fn the_guard_reports_a_capability_test_of_the_published_set() {
    let mutated = r#"
pub fn read_capability(store: &RouterInfoStore, peer: PeerId) -> IntroducerCapability {
    let record = store.get(&key)?;
    let parsed = Ssu2RouterAddress::parse(address)?;
    if !parsed.introducers().is_empty() { IntroducerCapability::Advertised } else { IntroducerCapability::NotAdvertised }
}
"#;
    assert!(
        check_the_capability_test_is_the_advertised_one(mutated).is_some(),
        "a capability test of the published set must be reported"
    );
}

#[test]
fn the_guard_reports_a_router_that_asks_while_reachable() {
    let product = r#"
async fn drive_introducer_request(&mut self, now_ms: u64) {
    let firewalled = self.inner.ssu2_handle.service().snapshot().reachability
        == i2pr_transport::ReachabilityState::Reachable;
}
"#;
    let owner = source_of(OWNER_MODULE);
    assert!(
        check_the_router_asks_only_when_firewalled(product, &owner).is_some(),
        "a router that asks while reachable must be reported"
    );
}

#[test]
fn the_guard_reports_a_request_that_opens_a_connection() {
    let product = r#"
async fn drive_introducer_request(&mut self, now_ms: u64) {
    let live = self.inner.ssu2_handle.service().active_peer_hashes();
    self.inner.ssu2_handle.service().dial_ssu2(target, timeout, &token).await;
}
"#;
    assert!(
        check_the_request_rides_an_existing_session(product).is_some(),
        "a request that opens a connection must be reported"
    );
}

#[test]
fn the_guard_reports_two_requests_in_one_window() {
    let product = r#"
async fn drive_introducer_request(&mut self, now_ms: u64) {
    let first = self.inner.ssu2_handle.service().queue_relay_tag_request(peer);
    let second = self.inner.ssu2_handle.service().queue_relay_tag_request(peer);
}
"#;
    assert!(
        check_at_most_one_request_leaves_per_window(product).is_some(),
        "two requests in one window must be reported"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_writes_the_block() {
    let owner = r#"
pub fn queue(&self, peer: PeerId) {
    let block = Block::RelayTagRequest;
    let _ = block;
}
"#;
    let product = source_of(PRODUCT_MODULE);
    assert!(
        check_the_daemon_never_writes_the_block(&product, owner).is_some(),
        "a daemon that writes the block must be reported"
    );
}

#[test]
fn the_guard_reports_an_owner_that_spawns_a_task() {
    let owner = r#"
pub fn decide(&mut self, eligible: &[PeerId], firewalled: bool, now_ms: u64) -> IntroducerRequestDecision {
    tokio::spawn(async move { do_work().await });
    IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NoEligiblePeer)
}
"#;
    assert!(
        check_the_owner_adds_no_infrastructure(owner).is_some(),
        "an owner that spawns a task must be reported"
    );
}

#[test]
fn the_guard_reports_an_unbounded_window() {
    let owner = r#"
impl IntroducerRequestOwner {
pub fn candidate_window(&mut self, live: &[PeerId], limit: usize) -> Vec<PeerId> {
    self.asked.retain(|peer| live.contains(peer));
    live.to_vec()
}
}
"#;
    let product = source_of(PRODUCT_MODULE);
    assert!(
        check_the_window_is_bounded_and_rotates(&product, owner).is_some(),
        "an unbounded window must be reported"
    );
}

#[test]
fn the_guard_reports_a_window_that_does_not_rotate() {
    let owner = r#"
impl IntroducerRequestOwner {
pub fn candidate_window(&mut self, live: &[PeerId], limit: usize) -> Vec<PeerId> {
    self.asked.retain(|peer| live.contains(peer));
    live.to_vec().into_iter().filter(|peer| !self.asked.contains(peer)).take(limit).collect()
}
}
"#;
    let product = source_of(PRODUCT_MODULE);
    assert!(
        check_the_window_is_bounded_and_rotates(&product, owner).is_some(),
        "a window that does not rotate must be reported"
    );
}

#[test]
fn the_guard_reports_a_peer_that_is_asked_again() {
    let owner = r#"
impl IntroducerRequestOwner {
pub fn note_queued(&mut self, peer: PeerId) {
    self.counters.requests_queued += 1;
}
pub fn candidate_window(&mut self, live: &[PeerId], limit: usize) -> Vec<PeerId> {
    self.cursor = None;
    live.to_vec().into_iter().take(limit).collect()
}
}
"#;
    assert!(
        check_a_peer_is_asked_at_most_once(owner).is_some(),
        "a peer that is asked again must be reported"
    );
}

#[test]
fn the_guard_reports_an_uncounted_refusal() {
    let owner = r#"
pub struct IntroducerRequestCounters {
    pub capability_advertised: u64,
}
pub fn decide(&mut self, eligible: &[PeerId], firewalled: bool, now_ms: u64) -> IntroducerRequestDecision {
    if !self.enabled {
        return IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::Disabled);
    }
    IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NoEligiblePeer)
}
pub fn note_transport_refusal(&mut self, now_ms: u64) {
    self.next_at_ms = now_ms;
}
"#;
    assert!(
        check_every_refusal_is_counted_under_its_own_reason(owner).is_some(),
        "an uncounted refusal must be reported"
    );
}

#[test]
fn the_guard_reports_an_acquisition_that_publishes() {
    let owner = r#"
pub fn decide(&mut self, eligible: &[PeerId], firewalled: bool, now_ms: u64) -> IntroducerRequestDecision {
    let material = self.shared.peer_relay.introducer_publication_material();
    let _ = material;
    IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NoEligiblePeer)
}
"#;
    let product = source_of(PRODUCT_MODULE);
    let netdb = source_of(NETDB_LOCAL_MODULE);
    assert!(
        check_the_publication_is_untouched(&product, owner, &netdb).is_some(),
        "an acquisition that publishes must be reported"
    );
}

#[test]
fn the_guard_reports_an_acquisition_that_advertises_the_capability() {
    let owner = r#"
pub fn decide(&mut self, eligible: &[PeerId], firewalled: bool, now_ms: u64) -> IntroducerRequestDecision {
    let options = Mapping::from_entries(vec![("caps".to_owned(), "C".to_owned())]);
    let _ = options;
    IntroducerRequestDecision::Refuse(IntroducerRequestRefusal::NoEligiblePeer)
}
"#;
    let product = source_of(PRODUCT_MODULE);
    let netdb = source_of(NETDB_LOCAL_MODULE);
    assert!(
        check_the_publication_is_untouched(&product, owner, &netdb).is_some(),
        "an acquisition that advertises the capability must be reported"
    );
}

#[test]
fn the_guard_reports_a_recorder_that_accepts_any_tag() {
    let runtime = r#"
SessionEvent::RelayTag(tag) => {
    self.shared.counters.relay_tags_observed.fetch_add(1, Ordering::Relaxed);
    match self.record_authenticated_introducer(&peer, tag, local_endpoint, now_secs) {
        Ok(()) => { self.shared.counters.relay_tags_recorded.fetch_add(1, Ordering::Relaxed); }
        Err(reason) => { self.shared.counters.relay_tags_refused_record.fetch_add(1, Ordering::Relaxed); }
    }
}
"#;
    assert!(
        check_the_runtime_records_only_an_asked_peer(runtime).is_some(),
        "a recorder that accepts any tag must be reported"
    );
}

#[test]
fn the_guard_reports_a_drain_that_hands_the_tag_to_the_role_owner() {
    let drain = r#"
pub fn drain_relay_events(
    source: &mut dyn RelayEventSource,
    owner: &mut Ssu2IntroducerService,
    signers: &dyn IntroducerSignerResolver,
    carriage: &dyn RelayCarriage,
    now_ms: u64,
    now_secs: u64,
) -> RelayDrainOutcome {
    let mut outcome = RelayDrainOutcome::default();
    match owner.on_relay_event(&event, signers, now_ms, now_secs) {
        Ssu2IntroducerOutcome::Refused { .. } => { outcome.refused = outcome.refused.saturating_add(1); }
        _ => {}
    }
    outcome
}
"#;
    assert!(
        check_the_drain_skips_the_tag_answer(drain).is_some(),
        "a drain that hands the tag to the role owner must be reported"
    );
}

#[test]
fn the_guard_reports_an_unbounded_outstanding_set() {
    let runtime = r#"
pub fn queue_relay_tag_request(&self, peer: PeerId) -> Result<(), Ssu2TagRequestError> {
    match self.queue_relay_block_on_state(&mut state, peer, |session| session.queue_relay_tag_request()) {
        Ok(()) => {
            state.outstanding_tag_requests.insert(peer);
            Ok(())
        }
        Err(error) => Err(Ssu2TagRequestError::NotQueued(error)),
    }
}
"#;
    assert!(
        check_the_outstanding_set_is_bounded(runtime).is_some(),
        "an unbounded outstanding set must be reported"
    );
}
