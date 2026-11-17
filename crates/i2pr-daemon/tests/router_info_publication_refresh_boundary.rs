//! Plan 518's structural guard for the publication's **freshness**.
//!
//! Plan 515 made the router's own RouterInfo publication periodic. Plan 516
//! made it confirmable. Plan 517 made it retry. All three re-send the **same
//! bytes** -- and those bytes carry the publication date the router signed
//! at **startup**, so the record ages out of the network's view: the netdb's
//! acceptance window is 24 hours
//! (`crates/i2pr-netdb/src/router_info.rs:20`), and the reference's netdb
//! expires routers between one and a half and twenty-seven hours
//! (`libi2pd/NetDb.hpp:49-51`).
//!
//! The reference re-dates and re-signs its record before each publish
//! (`libi2pd/RouterInfo.cpp:1218`, `libi2pd/RouterContext.cpp:1474`). The
//! guard asserts eight things about the **production** surface:
//!
//! 1. the re-dating happens **once per cycle**, at the cycle's start, and
//!    **not per rung**;
//! 2. the publication path publishes the **re-dated** record, not the
//!    installed startup bytes;
//! 3. the staleness rule uses the **netdb's own** acceptance window, not a
//!    second constant;
//! 4. a **stale** record is refused, never published;
//! 5. the re-dating **never logs** a key, a signature or the record's bytes;
//! 6. the proto's re-encode preserves the record's **content** -- its
//!    addresses, its peers and its options;
//! 7. the crypto's re-sign **verifies** its own result;
//! 8. the publication module stays **runtime-neutral**: it reads no clock
//!    and holds no key.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required **call**, with its open parenthesis. Comments
//! are stripped and `#[cfg(test)]` blocks are excluded, so a doc comment that
//! merely names a construct never satisfies a rule, and a function that is
//! **absent** fails closed.
//!
//! Every rule has an **inverted probe**: a mutation that must be reported.
//! A rule with no probe is a rule nobody has shown to fire.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The composition root: the router's publication tick and relay tick.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";
/// The publication module: the schedule, the cycle and the staleness rule.
const PUBLICATION_MODULE: &str = "crates/i2pr-daemon/src/router_info_publication.rs";
/// The daemon's identity module: the re-dating helper.
const ROUTER_I2NP_MODULE: &str = "crates/i2pr-daemon/src/router_i2np.rs";
/// The proto layer: the RouterInfo's signed region.
const PROTO_MODULE: &str = "crates/i2pr-proto/src/common/router_info.rs";
/// The crypto layer: the re-sign.
const CRYPTO_MODULE: &str = "crates/i2pr-crypto/src/lib.rs";

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

/// Returns the body of a `select!` arm, from the arm's head to its closing
/// brace.
fn select_arm_body(code: &str, head: &str) -> Option<String> {
    let start = code.find(head)? + head.len();
    let (open, end) = brace_balanced_block(code, start)?;
    Some(code[open..end].to_owned())
}

/// Rule 9: the rungs publish the **cycle's** record.
///
/// A rung must not read a copy of the record back out of the runtime: the
/// cycle's record is the owner's, and a rung that re-read the installed copy
/// would publish a different record from the one the correlator registered
/// when the cycle began.
fn check_the_rungs_publish_the_cycle_record(composition: &str) -> Option<String> {
    let code = code_view(composition);
    let Some(relay) = select_arm_body(&code, "relay_tick.tick() =>") else {
        return Some("the composition root has no relay tick arm".to_owned());
    };
    if relay.contains("installed_local_router_info(") {
        return Some(
            "the relay tick reads the installed record: a rung must publish the cycle's record"
                .to_owned(),
        );
    }
    None
}

/// Rule 10: the cycle's record is the **owner's**.
///
/// The rung reads the cycle's record from the owner, so the record is decided
/// once, at the cycle's start, and cannot change under the cycle.
fn check_the_cycle_record_is_the_owners(publication: &str) -> Option<String> {
    let code = code_view(publication);
    let Some(body) = item_body(&code, "pub async fn advance_publication_cycle") else {
        return Some("the publication's rung is absent".to_owned());
    };
    if !body.contains("cycle_record(") {
        return Some("the rung does not read the cycle's record".to_owned());
    }
    None
}

/// Rule 1: the re-dating happens once per cycle, at the cycle's start, and
/// not per rung.
///
/// The reference dates the record in `HandlePublishTimer`
/// (`libi2pd/RouterContext.cpp:1474`), once per cycle, and its resend rungs
/// publish that same record.
fn check_the_re_dating_happens_once_per_cycle(composition: &str) -> Option<String> {
    let code = code_view(composition);
    let call = "crate::router_i2np::refresh_local_router_info_record(";
    let occurrences = code.matches(call).count();
    if occurrences != 1 {
        return Some(format!(
            "the composition root must re-date the record exactly once, found {occurrences}"
        ));
    }
    let Some(publication) = select_arm_body(&code, "publication_tick.tick() =>") else {
        return Some("the composition root has no publication tick arm".to_owned());
    };
    if !publication.contains(call) {
        return Some("the publication tick does not re-date the record".to_owned());
    }
    let Some(relay) = select_arm_body(&code, "relay_tick.tick() =>") else {
        return Some("the composition root has no relay tick arm".to_owned());
    };
    if relay.contains(call) {
        return Some(
            "the relay tick re-dates the record: a rung must publish the cycle's record".to_owned(),
        );
    }
    None
}

/// Rule 2: the publication path publishes the re-dated record.
fn check_the_publication_uses_the_re_dated_record(composition: &str) -> Option<String> {
    let code = code_view(composition);
    let Some(publication) = select_arm_body(&code, "publication_tick.tick() =>") else {
        return Some("the composition root has no publication tick arm".to_owned());
    };
    if publication.contains("installed_local_router_info(") {
        return Some(
            "the publication tick reads the installed record directly: it must publish the re-dated one"
                .to_owned(),
        );
    }
    None
}

/// Rule 3: the staleness rule uses the netdb's own acceptance window.
fn check_the_staleness_rule_uses_the_netdb_window(publication: &str) -> Option<String> {
    let code = code_view(publication);
    if !code.contains("i2pr_netdb::DEFAULT_MAX_AGE") {
        return Some(
            "the staleness rule does not use the netdb's own acceptance window".to_owned(),
        );
    }
    let Some(body) = item_body(&code, "pub fn record_is_publishable") else {
        return Some("the publication's staleness rule is absent".to_owned());
    };
    if !body.contains("PUBLICATION_MAX_RECORD_AGE_MS") {
        return Some("the staleness rule does not use the acceptance window".to_owned());
    }
    None
}

/// Rule 4: a stale record is refused, never published.
fn check_a_stale_record_is_refused(publication: &str) -> Option<String> {
    let code = code_view(publication);
    let Some(body) = item_body(&code, "async fn publish_once") else {
        return Some("the publication's attempt is absent".to_owned());
    };
    if !body.contains("record_is_publishable(") {
        return Some("the publication never tests the record's freshness".to_owned());
    }
    if !body.contains("RefusedStaleRecord") {
        return Some("a stale record is not refused under its own reason".to_owned());
    }
    None
}

/// Rule 5: the re-dating never logs a key, a signature or the record's
/// bytes.
fn check_the_re_dating_logs_no_secret(router_i2np: &str) -> Option<String> {
    let code = code_view(router_i2np);
    let Some(body) = item_body(&code, "pub fn refresh_local_router_info_record") else {
        return Some("the daemon's re-dating helper is absent".to_owned());
    };
    for forbidden in [
        "signing_key",
        "signing_seed",
        "secret",
        "signature",
        "PRIV",
        "%fresh",
        "%installed",
    ] {
        if body.contains(forbidden) {
            return Some(format!(
                "the re-dating logs something it must not: {forbidden}"
            ));
        }
    }
    None
}

/// Rule 6: the proto's re-encode preserves the record's content.
fn check_the_proto_preserves_the_content(proto: &str) -> Option<String> {
    let code = code_view(proto);
    let Some(body) = item_body(&code, "pub fn signed_bytes_with_published") else {
        return Some("the proto's re-encode is absent".to_owned());
    };
    if !body.contains("Self::encode_unsigned(") {
        return Some("the proto's re-encode does not use the shared encoder".to_owned());
    }
    for part in [
        "&self.router_identity",
        "&self.addresses",
        "&self.peers",
        "&self.options",
    ] {
        if !body.contains(part) {
            return Some(format!("the proto's re-encode drops a part: {part}"));
        }
    }
    None
}

/// Rule 7: the crypto's re-sign verifies its own result.
fn check_the_crypto_verifies_its_result(crypto: &str) -> Option<String> {
    let code = code_view(crypto);
    let Some(body) = item_body(&code, "pub fn resign_router_info_with_published") else {
        return Some("the crypto's re-sign is absent".to_owned());
    };
    if !body.contains("signed_bytes_with_published(") {
        return Some("the crypto's re-sign does not use the proto's re-encode".to_owned());
    }
    if !body.contains("verify_router_info(") {
        return Some("the crypto's re-sign does not verify its own result".to_owned());
    }
    None
}

/// Rule 8: the publication module stays runtime-neutral.
fn check_the_module_stays_runtime_neutral(publication: &str) -> Option<String> {
    let code = code_view(publication);
    let Some(body) = item_body(&code, "pub fn record_is_publishable") else {
        return Some("the publication's staleness rule is absent".to_owned());
    };
    for forbidden in ["SystemTime", "Instant", "wall_clock", "now()"] {
        if body.contains(forbidden) {
            return Some(format!("the staleness rule reads a clock: {forbidden}"));
        }
    }
    for forbidden in ["RouterIdentityBundle", "signing_key", "signing_seed"] {
        if code.contains(forbidden) {
            return Some(format!("the publication module holds a key: {forbidden}"));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

#[test]
fn the_re_dating_happens_once_per_cycle() {
    let reason = check_the_re_dating_happens_once_per_cycle(&source_of(COMPOSITION_MODULE));
    assert!(
        reason.is_none(),
        "the re-dating must happen once per cycle: {reason:?}"
    );
}

#[test]
fn the_publication_uses_the_re_dated_record() {
    let reason = check_the_publication_uses_the_re_dated_record(&source_of(COMPOSITION_MODULE));
    assert!(
        reason.is_none(),
        "the publication must publish the re-dated record: {reason:?}"
    );
}

#[test]
fn the_staleness_rule_uses_the_netdb_window() {
    let reason = check_the_staleness_rule_uses_the_netdb_window(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the staleness rule must use the netdb's own window: {reason:?}"
    );
}

#[test]
fn a_stale_record_is_refused() {
    let reason = check_a_stale_record_is_refused(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "a stale record must be refused: {reason:?}"
    );
}

#[test]
fn the_re_dating_logs_no_secret() {
    let reason = check_the_re_dating_logs_no_secret(&source_of(ROUTER_I2NP_MODULE));
    assert!(
        reason.is_none(),
        "the re-dating must not log a secret: {reason:?}"
    );
}

#[test]
fn the_proto_preserves_the_content() {
    let reason = check_the_proto_preserves_the_content(&source_of(PROTO_MODULE));
    assert!(
        reason.is_none(),
        "the proto's re-encode must preserve the content: {reason:?}"
    );
}

#[test]
fn the_crypto_verifies_its_result() {
    let reason = check_the_crypto_verifies_its_result(&source_of(CRYPTO_MODULE));
    assert!(
        reason.is_none(),
        "the crypto's re-sign must verify its result: {reason:?}"
    );
}

#[test]
fn the_module_stays_runtime_neutral() {
    let reason = check_the_module_stays_runtime_neutral(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the publication module must stay runtime-neutral: {reason:?}"
    );
}

#[test]
fn the_rungs_publish_the_cycle_record() {
    let reason = check_the_rungs_publish_the_cycle_record(&source_of(COMPOSITION_MODULE));
    assert!(
        reason.is_none(),
        "the rungs must publish the cycle's record: {reason:?}"
    );
}

#[test]
fn the_cycle_record_is_the_owners() {
    let reason = check_the_cycle_record_is_the_owners(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the cycle's record must be the owner's: {reason:?}"
    );
}

// ----------------------------------------------------------------------
// The inverted probes: each rule must report its own violation.
// ----------------------------------------------------------------------

/// The inverted probe for rule 1: a composition that re-dates the record in
/// the relay arm as well must be reported.
#[test]
fn the_guard_reports_a_composition_that_re_dates_per_rung() {
    let violating = "\
fn compose() {
    let _ = publication_tick.tick() => {
        crate::router_i2np::refresh_local_router_info_record();
    };
    let _ = relay_tick.tick() => {
        crate::router_i2np::refresh_local_router_info_record();
    };
}
";
    let reason = check_the_re_dating_happens_once_per_cycle(violating);
    assert!(
        reason.is_some(),
        "a re-dating per rung must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 2: a publication tick that reads the
/// installed record directly must be reported.
#[test]
fn the_guard_reports_a_publication_that_reads_the_installed_record() {
    let violating = "\
fn compose() {
    let _ = publication_tick.tick() => {
        let _ = owner.handle().service().installed_local_router_info();
    };
}
";
    let reason = check_the_publication_uses_the_re_dated_record(violating);
    assert!(
        reason.is_some(),
        "a publication that reads the installed record must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 9: a relay arm that reads the installed
/// record back out of the runtime must be reported.
#[test]
fn the_guard_reports_a_rung_that_reads_the_installed_record() {
    let violating = "\
fn compose() {
    let _ = relay_tick.tick() => {
        let _ = owner.handle().service().installed_local_router_info();
    };
}
";
    let reason = check_the_rungs_publish_the_cycle_record(violating);
    assert!(
        reason.is_some(),
        "a rung that reads the installed record must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 10: a rung that does not read the cycle's
/// record must be reported.
#[test]
fn the_guard_reports_a_rung_that_does_not_read_the_cycle_record() {
    let violating = "\
pub async fn advance_publication_cycle(owner: &mut Owner) {
    let _ = owner;
}
";
    let reason = check_the_cycle_record_is_the_owners(violating);
    assert!(
        reason.is_some(),
        "a rung that does not read the cycle's record must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 3: a staleness rule with its own window must
/// be reported.
#[test]
fn the_guard_reports_a_staleness_rule_with_its_own_window() {
    let violating = "\
pub const PUBLICATION_MAX_RECORD_AGE_MS: u64 = 86_400_000;

pub fn record_is_publishable(encoded: &[u8], now_ms: u64) -> bool {
    let _ = (encoded, now_ms);
    true
}
";
    let reason = check_the_staleness_rule_uses_the_netdb_window(violating);
    assert!(
        reason.is_some(),
        "a staleness rule with its own window must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 4: a publication that never tests the
/// record's freshness must be reported.
#[test]
fn the_guard_reports_a_publication_that_never_tests_freshness() {
    let violating = "\
async fn publish_once<R: RngCore>(owner: &mut u8) -> u8 {
    let _ = owner;
    0
}
";
    let reason = check_a_stale_record_is_refused(violating);
    assert!(
        reason.is_some(),
        "a publication that never tests freshness must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 5: a re-dating that logs the record's bytes
/// must be reported.
#[test]
fn the_guard_reports_a_re_dating_that_logs_the_record() {
    let violating = "\
pub fn refresh_local_router_info_record(bundle: &u8, handle: &u8, now_ms: u64) -> Option<Vec<u8>> {
    let fresh = Vec::new();
    tracing::warn!(reason = %fresh, \"the record bytes\");
    let _ = (bundle, handle, now_ms);
    None
}
";
    let reason = check_the_re_dating_logs_no_secret(violating);
    assert!(
        reason.is_some(),
        "a re-dating that logs the record must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 6: a proto re-encode that drops the peers
/// must be reported.
#[test]
fn the_guard_reports_a_proto_re_encode_that_drops_a_part() {
    let violating = "\
pub fn signed_bytes_with_published(&self, published: Date) -> Result<Vec<u8>, CodecError> {
    let _ = published;
    Ok(Vec::new())
}
";
    let reason = check_the_proto_preserves_the_content(violating);
    assert!(
        reason.is_some(),
        "a re-encode that drops a part must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 7: a crypto re-sign that does not verify its
/// result must be reported.
#[test]
fn the_guard_reports_a_re_sign_that_does_not_verify() {
    let violating = "\
pub fn resign_router_info_with_published(
    bundle: &u8,
    encoded: &[u8],
    published: Date,
) -> Result<Vec<u8>, CryptoError> {
    let _ = (bundle, encoded, published);
    Ok(Vec::new())
}
";
    let reason = check_the_crypto_verifies_its_result(violating);
    assert!(
        reason.is_some(),
        "a re-sign that does not verify must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 8: a staleness rule that reads a clock must
/// be reported.
#[test]
fn the_guard_reports_a_staleness_rule_that_reads_a_clock() {
    let violating = "\
pub fn record_is_publishable(encoded: &[u8], now_ms: u64) -> bool {
    let _ = (encoded, now_ms);
    let _ = SystemTime::now();
    true
}
";
    let reason = check_the_module_stays_runtime_neutral(violating);
    assert!(
        reason.is_some(),
        "a staleness rule that reads a clock must be reported: {reason:?}"
    );
}
