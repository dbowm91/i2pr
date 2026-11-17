//! Plan 516's structural guard for the publication's **acknowledgement**.
//!
//! Until this plan the router published its own RouterInfo and learned
//! only that the **transport accepted the message**. It never learned that
//! the **network holds** the record. The correlator minted a reply token,
//! recorded it, documented that the peer would correlate the
//! acknowledgement with it -- and then wrote `reply_token: 0` onto the
//! wire, so no peer could ever answer.
//!
//! The guard asserts seven things about the **production** surface:
//!
//! 1. the correlation token **reaches the wire**: the store message the
//!    publication sends carries the correlator's minted token, and not a
//!    zero token -- a zero token asks for no acknowledgement;
//! 2. the reply form is the reference's **direct** reply: a zero reply
//!    tunnel id and our own hash as the reply gateway
//!    (`libi2pd/I2NPProtocol.cpp:285`);
//! 3. the store's key, its reply gateway and its payload come from **one**
//!    record, so a publication can never store a record under the wrong
//!    key;
//! 4. the daemon composes **no** store message of its own: there is one
//!    builder, and it is the correlator's;
//! 5. the service product's one-shot publication goes through the
//!    **correlator**, so the one-shot and the refresh cannot disagree;
//! 6. the **inbound owner** joins the publication: the acknowledgement
//!    reaches the correlator;
//! 7. an acknowledgement that matches **no** live attempt is refused and
//!    counted under its own reason, never silently dropped.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required **call**, with its open parenthesis. Comments
//! are stripped and `#[cfg(test)]` blocks are excluded, so a doc comment that
//! merely names a construct never satisfies a rule, and a function that is
//! **absent** fails closed.
//!
//! Every rule has an **inverted probe**: a mutation of the real source
//! that must be reported. A rule with no probe is a rule nobody has shown
//! to fire.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The correlator: the minted token, the store message, the attempt book.
const NETDB_MODULE: &str = "crates/i2pr-netdb/src/publication.rs";
/// The publication module: the schedule, the plan and the confirmation.
const PUBLICATION_MODULE: &str = "crates/i2pr-daemon/src/router_info_publication.rs";
/// The composition root: the inbound owner and the refresh tick.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";
/// The service product: the one-shot publication.
const SERVICE_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";

/// The repository root.
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

/// Returns the offset of a **definition** whose text is `needle`.
///
/// The character that follows the needle must not continue an identifier, so
/// `fn drive` never matches `fn drive_discovery` and `fn candidate_hashes`
/// never matches `fn version_gated_candidate_hashes`.
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
///
/// The scan is **string-aware**: a brace inside a string literal or inside a
/// character literal does not change the nesting depth, so a body that holds
/// `"}"` does not end the block early.
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

/// Extracts the body of the item whose **definition** is anchored at
/// `signature`.
///
/// The anchor is a definition and never a call site: the needle carries the
/// `fn` keyword, and the character that follows it must not continue an
/// identifier. The body is the text between the item's own braces. An item
/// that is not present yields `None`, so every rule that uses it fails
/// closed.
fn item_body(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (open, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[open + 1..close - 1].to_owned())
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1: the correlation token reaches the wire.
///
/// The token the correlator mints must be the token the store message
/// carries. A zero token asks the peer for no acknowledgement, so the
/// correlator's correlation half would be unreachable.
fn check_the_correlation_token_reaches_the_wire(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub fn begin_attempt(") else {
        return Some("the correlator's attempt builder is absent".to_owned());
    };
    if !body.contains("reply_token,") {
        return Some("the publication's store message does not carry the minted token".to_owned());
    }
    if body.contains("reply_token: 0") {
        return Some("the publication's store message asks for no acknowledgement".to_owned());
    }
    None
}

/// Rule 2: the reply form is the reference's direct reply.
///
/// A zero reply tunnel id means "send the `DeliveryStatus` straight back
/// to me" (`libi2pd/I2NPProtocol.cpp:285`), and the gateway is the local
/// hash (`:287-289`).
fn check_the_reply_form_is_the_direct_reply(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub fn begin_attempt(") else {
        return Some("the correlator's attempt builder is absent".to_owned());
    };
    if !body.contains("reply_tunnel_id: Some(0)") {
        return Some("the publication's reply is not the reference's direct reply".to_owned());
    }
    if !body.contains("reply_gateway: Some(") {
        return Some("the publication's reply names no gateway".to_owned());
    }
    None
}

/// Rule 3: the key, the gateway and the payload come from one record.
fn check_the_key_the_gateway_and_the_payload_are_one_record(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub fn begin_attempt(") else {
        return Some("the correlator's attempt builder is absent".to_owned());
    };
    if !body.contains("local.hash()") {
        return Some(
            "the publication's key and gateway are not the registered record's".to_owned(),
        );
    }
    if !body.contains("local.encoded()") {
        return Some("the publication's payload is not the registered record's".to_owned());
    }
    None
}

/// Rule 4: the daemon composes no store message of its own.
fn check_the_daemon_composes_no_store_message(source: &str) -> Option<String> {
    let code = code_view(source);
    if code.contains("router_info_store_message") {
        return Some("the daemon composes a store message of its own".to_owned());
    }
    None
}

/// Rule 5: the one-shot publication goes through the correlator.
fn check_the_one_shot_goes_through_the_correlator(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "async fn publish_creator_router_info(") else {
        return Some("the service product's one-shot publication is absent".to_owned());
    };
    if !body.contains("begin_attempts(") {
        return Some("the one-shot publication does not begin a tracked attempt".to_owned());
    }
    if !body.contains("publish_router_info(") {
        return Some("the one-shot publication does not use the shared path".to_owned());
    }
    None
}

/// Rule 6: the inbound owner joins the publication.
fn check_the_inbound_owner_joins_the_publication(source: &str) -> Option<String> {
    let code = code_view(source);
    if !code.contains("publication_owner.confirm(") {
        return Some(
            "the inbound owner does not join the publication's acknowledgement".to_owned(),
        );
    }
    None
}

/// Rule 7: an unmatched acknowledgement is refused.
fn check_an_unmatched_acknowledgement_is_refused(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub fn confirm_publication(") else {
        return Some("the publication's confirmation seam is absent".to_owned());
    };
    if !body.contains("correlate_delivery_status(") {
        return Some("the confirmation does not correlate the acknowledgement".to_owned());
    }
    if !body.contains("PublicationConfirmation::Unmatched") {
        return Some("an unmatched acknowledgement is not refused".to_owned());
    }
    None
}

#[test]
fn the_correlation_token_reaches_the_wire() {
    let source = source_of(NETDB_MODULE);
    assert_eq!(check_the_correlation_token_reaches_the_wire(&source), None);
}

#[test]
fn the_reply_form_is_the_direct_reply() {
    let source = source_of(NETDB_MODULE);
    assert_eq!(check_the_reply_form_is_the_direct_reply(&source), None);
}

#[test]
fn the_key_the_gateway_and_the_payload_are_one_record() {
    let source = source_of(NETDB_MODULE);
    assert_eq!(
        check_the_key_the_gateway_and_the_payload_are_one_record(&source),
        None
    );
}

#[test]
fn the_daemon_composes_no_store_message() {
    let source = source_of(PUBLICATION_MODULE);
    assert_eq!(check_the_daemon_composes_no_store_message(&source), None);
}

#[test]
fn the_one_shot_goes_through_the_correlator() {
    let source = source_of(SERVICE_MODULE);
    assert_eq!(
        check_the_one_shot_goes_through_the_correlator(&source),
        None
    );
}

#[test]
fn the_inbound_owner_joins_the_publication() {
    let source = source_of(COMPOSITION_MODULE);
    assert_eq!(check_the_inbound_owner_joins_the_publication(&source), None);
}

#[test]
fn an_unmatched_acknowledgement_is_refused() {
    let source = source_of(PUBLICATION_MODULE);
    assert_eq!(check_an_unmatched_acknowledgement_is_refused(&source), None);
}

#[test]
fn the_guard_reports_a_publication_that_leaves_the_token_off_the_wire() {
    let source = source_of(NETDB_MODULE);
    let mutated = source.replace(
        "            reply_token,\n",
        "            reply_token: 0,\n",
    );
    assert_ne!(mutated, source, "the mutation must apply");
    assert!(
        check_the_correlation_token_reaches_the_wire(&mutated).is_some(),
        "the guard must report a publication that leaves the token off the wire"
    );
}

#[test]
fn the_guard_reports_a_publication_that_answers_through_a_tunnel() {
    let source = source_of(NETDB_MODULE);
    let mutated = source.replace("reply_tunnel_id: Some(0),", "reply_tunnel_id: Some(7),");
    assert_ne!(mutated, source, "the mutation must apply");
    assert!(
        check_the_reply_form_is_the_direct_reply(&mutated).is_some(),
        "the guard must report a publication that does not use the direct reply"
    );
}

#[test]
fn the_guard_reports_a_publication_that_names_no_gateway() {
    let source = source_of(NETDB_MODULE);
    let mutated = source.replace("reply_gateway: Some(local_hash),", "reply_gateway: None,");
    assert_ne!(mutated, source, "the mutation must apply");
    assert!(
        check_the_reply_form_is_the_direct_reply(&mutated).is_some(),
        "the guard must report a publication that names no gateway"
    );
}

#[test]
fn the_guard_reports_a_publication_whose_payload_comes_from_elsewhere() {
    let source = source_of(NETDB_MODULE);
    let mutated = source.replace("local.encoded()", "local.other_bytes()");
    assert_ne!(mutated, source, "the mutation must apply");
    assert!(
        check_the_key_the_gateway_and_the_payload_are_one_record(&mutated).is_some(),
        "the guard must report a publication whose payload comes from elsewhere"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_composes_its_own_store_message() {
    let source = source_of(PUBLICATION_MODULE);
    let mutated = source.replace(
        "pub fn begin_attempts(",
        "pub fn router_info_store_message(",
    );
    assert_ne!(mutated, source, "the mutation must apply");
    assert!(
        check_the_daemon_composes_no_store_message(&mutated).is_some(),
        "the guard must report a daemon that composes its own store message"
    );
}

#[test]
fn the_guard_reports_a_one_shot_that_skips_the_correlator() {
    let source = source_of(SERVICE_MODULE);
    let mutated = source.replace(
        "crate::router_info_publication::begin_attempts(",
        "crate::router_info_publication::begin_attempt_legacy(",
    );
    assert_ne!(mutated, source, "the mutation must apply");
    assert!(
        check_the_one_shot_goes_through_the_correlator(&mutated).is_some(),
        "the guard must report a one-shot publication that skips the correlator"
    );
}

#[test]
fn the_guard_reports_an_inbound_owner_that_drops_the_acknowledgement() {
    let source = source_of(COMPOSITION_MODULE);
    let mutated = source.replace("publication_owner.confirm(", "publication_owner.ignore(");
    assert_ne!(mutated, source, "the mutation must apply");
    assert!(
        check_the_inbound_owner_joins_the_publication(&mutated).is_some(),
        "the guard must report an inbound owner that drops the acknowledgement"
    );
}

#[test]
fn the_guard_reports_a_confirmation_that_drops_the_unmatched_case() {
    let source = source_of(PUBLICATION_MODULE);
    let mutated = source.replace(
        "PublicationConfirmation::Unmatched",
        "PublicationConfirmation::Ignored",
    );
    assert_ne!(mutated, source, "the mutation must apply");
    assert!(
        check_an_unmatched_acknowledgement_is_refused(&mutated).is_some(),
        "the guard must report a confirmation that drops the unmatched case"
    );
}
