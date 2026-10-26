//! Plan 506 static boundary guard for the peer-knowledge reconcile.
//!
//! Plan 506's invariants are not only behavioural; several of them are
//! *structural*, and a behavioural test cannot see a structural drift:
//!
//! 1. **The composition hands the product the handles the reconcile needs.**
//!    A reconcile that owns no bootstrap handle has no source, and a
//!    reconcile that owns no peer-profile handle has nowhere to learn.
//! 2. **The product reconciles on its existing tick.** The reconcile is a
//!    step the tick advances, not a task it spawns and not a timer it owns.
//! 3. **The locks are taken in one order and never nested.** The bootstrap's
//!    lock is released before the destination coordinator's lock is taken,
//!    and the destination coordinator's lock is released before the
//!    peer-profile store's lock is taken.
//! 4. **The reconcile writes through the canonical write APIs.** It never
//!    manipulates a store directly and never writes a selection.
//! 5. **The reconcile is cadenced, and the cadence is bounded.**
//! 6. **A capacity refusal is counted, not fatal.** One bad record does not
//!    abort the batch.
//! 7. **The unsolicited store is ingested once.** The duplicate ingest site
//!    is gone, and the surviving sites are in mutually exclusive branches.
//! 8. **The counters are published** through the inspection seam, and the
//!    seam's reader returns a *gap* rather than a fabricated zero.
//! 9. **Plan 491's version gate is still applied at selection**, and the
//!    refresh never writes a selection.
//!
//! The guard is deliberately **fail-closed**: it strips comments before
//! matching (so a doc comment that merely names a construct is not a
//! violation), it excludes `#[cfg(test)]` blocks, an absent function is a
//! violation, and every rule carries its own **inverted probe** — a mutation
//! of a *copy* of the landed source that reintroduces the defect must be
//! reported. Each mutation probe asserts that the mutation applied, so no
//! probe can pass vacuously.
//!
//! The rules are **structural**, not textual: a rule asserts that a named
//! function's *own body* contains the required **call**, matched with its
//! open parenthesis. A whole-file token search would prove nothing — Plan
//! 185's gap was a module that existed, with every name and token present,
//! and was never wired.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The reconcile module the guard is about.
const RECONCILE_MODULE: &str = "crates/i2pr-daemon/src/peer_reconcile.rs";

/// The product module that owns the reconcile at runtime.
const PRODUCT_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";

/// The composition root.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";

/// The inspection seam.
const INSPECTION_MODULE: &str = "crates/i2pr-daemon/src/i2pcontrol_inspection.rs";

/// The peer-profile participation module that owns the version-gated
/// selection.
const PARTICIPATION_MODULE: &str = "crates/i2pr-daemon/src/netdb_participation.rs";

/// The destination-peer module that owns the per-record version gate.
const DESTINATION_PEERS_MODULE: &str = "crates/i2pr-daemon/src/destination_peers.rs";

/// The canonical destination-store write call.
const DESTINATION_WRITE_CALL: &str = "insert_validated_router_info(";

/// The canonical peer-profile-store write call.
const PROFILE_WRITE_CALL: &str = "learn_all(";

/// The canonical bootstrap snapshot call.
const SNAPSHOT_CALL: &str = "validated_router_info_snapshot(";

/// The canonical unsolicited-store ingest call.
const INGEST_CALL: &str = "ingest_authenticated_router_store(";

/// The anchor of the product spec literal in the composition root.
///
/// The anchor carries **no** opening brace: the block helper searches for
/// the first `{` *after* the anchor, so an anchor that ended in `{` would
/// skip the literal's own brace and read the wrong block.
const PRODUCT_SPEC_ANCHOR: &str = "let spec = crate::service_product::ServiceProductSpec";

/// A direct write into a store's own map.
///
/// The token is `.store.insert(` and not `.insert(`: the latter also
/// matches a `BTreeMap` insert, which is a counter table and not a peer
/// store.
const DIRECT_STORE_WRITE_CALL: &str = ".store.insert(";

/// The constructs that would give the reconcile its own task or timer.
const RECONCILE_TASK_TOKENS: &[&str] = &["tokio::spawn", "tokio::time::interval"];

/// The constructs a selection-side write would use.
///
/// The reconcile writes **peer records**, never a selection. A selection
/// written by the reconcile would bypass Plan 491's gate, which lives at
/// selection.
const SELECTION_TOKENS: &[&str] = &[
    "select_tunnel_build_peers(",
    "select_dsm_targets(",
    "select_candidates",
    "select_version_gated(",
];

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

/// Returns the offset of a **definition** whose text is `needle`.
///
/// The character that follows the needle must not continue an identifier, so
/// `struct ProductInner` never matches `struct ProductInnerAux`, and `fn
/// drive` never matches `fn drive_something`.
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
    let start = find_definition(code, &needle)?;
    let (open, close) = brace_balanced_block(code, start + needle.len())?;
    Some(&code[open + 1..close - 1])
}

/// Returns the body of the brace-balanced block that opens after `anchor`.
///
/// The rule that uses it is the "inside a branch" rule: the block that opens
/// after `if <posture>` is the branch's own body, so a construction that
/// sits *outside* the branch cannot satisfy it.
fn block_after<'a>(code: &'a str, anchor: &str) -> Option<&'a str> {
    let position = code.find(anchor)?;
    let (open, close) = brace_balanced_block(code, position + anchor.len())?;
    Some(&code[open + 1..close - 1])
}

/// Returns the body of the struct whose **definition** is anchored at
/// `struct <name>`.
fn struct_body<'a>(code: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("struct {name}");
    let start = find_definition(code, &needle)?;
    let (open, close) = brace_balanced_block(code, start + needle.len())?;
    Some(&code[open + 1..close - 1])
}

/// Replaces the `nth` (zero-based) occurrence of `needle` in `source`.
fn replace_occurrence(source: &str, needle: &str, replacement: &str, nth: usize) -> String {
    let mut seen = 0usize;
    let mut offset = 0usize;
    while let Some(position) = source[offset..].find(needle) {
        let start = offset + position;
        let after = start + needle.len();
        if seen == nth {
            let mut out = String::with_capacity(source.len());
            out.push_str(&source[..start]);
            out.push_str(replacement);
            out.push_str(&source[after..]);
            return out;
        }
        seen += 1;
        offset = after;
    }
    source.to_owned()
}

/// Returns the number of occurrences of `token` in `text`.
fn occurrences(text: &str, token: &str) -> usize {
    text.matches(token).count()
}

// ----------------------------------------------------------------------
// The rule primitives.
// ----------------------------------------------------------------------

/// Rule 1, 2, 4, 5, 6 (structural): the body of `fn <name>` must contain
/// `token`.
///
/// The token is matched with its open parenthesis, so the rule asserts a
/// **call** and not a bare identifier: a dead helper, a type name, or an
/// unreachable branch does not satisfy it.
fn check_body_contains(source: &str, name: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, name) else {
        return Some(format!("`fn {name}` must exist in the production surface"));
    };
    if body.contains(token) {
        None
    } else {
        Some(format!("`fn {name}` must contain `{token}`"))
    }
}

/// The body of `fn <name>` must not contain `token`.
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

/// The production code view must not contain `token`.
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

/// The production code view must contain `token`.
fn check_code_contains(source: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    if code.contains(token) {
        None
    } else {
        Some(format!("the production code view must contain `{token}`"))
    }
}

/// The production code view must contain `token` exactly once.
///
/// A second occurrence is a second pipeline, which is the private parallel
/// path the rule forbids.
fn check_code_occurs_once(source: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let found = occurrences(&code, token);
    if found == 1 {
        None
    } else {
        Some(format!(
            "the production code view must contain `{token}` exactly once, found {found}"
        ))
    }
}

/// The brace-balanced block that opens after `anchor` must contain `token`.
fn check_block_contains(source: &str, anchor: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let Some(block) = block_after(&code, anchor) else {
        return Some(format!(
            "the block `{anchor}` must exist in the production surface"
        ));
    };
    if block.contains(token) {
        None
    } else {
        Some(format!("the block `{anchor}` must contain `{token}`"))
    }
}

/// The body of `struct <name>` must contain `token`.
fn check_struct_contains(source: &str, name: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = struct_body(&code, name) else {
        return Some(format!(
            "`struct {name}` must exist in the production surface"
        ));
    };
    if body.contains(token) {
        None
    } else {
        Some(format!("`struct {name}` must contain `{token}`"))
    }
}

/// Rule 3 (structural): the three lock-taking steps must appear in `fn run`
/// in this order — the snapshot, then the insert, then the learn.
///
/// The order in the source **is** the order the locks are taken, because
/// each step is its own function and each function releases its lock before
/// it returns.
fn check_lock_order(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, "run") else {
        return Some("`fn run` must exist in the production surface".to_owned());
    };
    // Each step must be **present**: a missing step is a violation, never a
    // silently passing rule. The `?` operator would have propagated `None`
    // out of the rule and reported nothing.
    let (Some(snapshot), Some(insert), Some(learn)) = (
        body.find("self.snapshot()"),
        body.find("self.insert_all("),
        body.find("self.learn_all("),
    ) else {
        return Some(
            "`fn run` must take all three locks in the order snapshot, insert, learn".to_owned(),
        );
    };
    if snapshot < insert && insert < learn {
        None
    } else {
        Some("`fn run` must take the locks in the order snapshot, insert, learn".to_owned())
    }
}

/// Rule 3 (structural): the three lock-taking steps must each own **one**
/// lock and must not name the others' locks.
///
/// This is what makes the ordering a *release* ordering and not merely a
/// textual one: `fn snapshot` may not mention the destination coordinator
/// or the peer-profile store, so it cannot hold either of their locks while
/// it holds the bootstrap's.
fn check_lock_isolation(source: &str, function: &str, forbidden: &[&str]) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, function) else {
        return Some(format!(
            "`fn {function}` must exist in the production surface"
        ));
    };
    for token in forbidden {
        if body.contains(token) {
            return Some(format!("`fn {function}` must not mention `{token}`"));
        }
    }
    None
}

/// Rule 7 (structural): the unsolicited store must be ingested **once per
/// branch**.
///
/// The two surviving ingest sites sit in mutually exclusive branches — the
/// `!ssu2_config.enabled` branch and the SSU2-enabled path — so exactly one
/// of them can run for any message. The rule asserts one ingest in the
/// branch and one ingest outside it, and no more.
fn check_ingest_once_per_branch(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, "register_ssu2_service") else {
        return Some("`fn register_ssu2_service` must exist in the production surface".to_owned());
    };
    let total = occurrences(body, INGEST_CALL);
    let Some(branch) = block_after(&code, "if !ssu2_config.enabled") else {
        return Some(
            "the `if !ssu2_config.enabled` branch must exist in the production surface".to_owned(),
        );
    };
    let inside = occurrences(branch, INGEST_CALL);
    let outside = total - inside;
    if total == 2 && inside == 1 && outside == 1 {
        None
    } else {
        Some(format!(
            "the unsolicited store must be ingested once per branch, found {total} in total ({inside} inside the `!ssu2_config.enabled` branch, {outside} outside)"
        ))
    }
}

/// Rule 8 (structural): the seam's reader must return a **gap**, never a
/// fabricated zero.
///
/// The reader returns an `Option`, and the snapshot is taken with the `?`
/// operator on that `Option`, so an unpublished or poisoned slot is a
/// `None` and never a zeroed counter set.
fn check_reader_returns_a_gap(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, "peer_reconcile_counters_snapshot") else {
        return Some(
            "`fn peer_reconcile_counters_snapshot` must exist in the production surface".to_owned(),
        );
    };
    if !body.contains("self.peer_reconcile_counters()?") {
        return Some(
            "`fn peer_reconcile_counters_snapshot` must take the cell through the gap-returning reader"
                .to_owned(),
        );
    }
    for token in ["unwrap_or_default", "unwrap_or(0)", "unwrap_or_default()"] {
        if body.contains(token) {
            return Some(format!(
                "`fn peer_reconcile_counters_snapshot` must not fabricate a zero with `{token}`"
            ));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Every rule, applied to the landed tree.
///
/// A rule returns the violation it found, or `None` when the tree holds.
fn check_all(
    reconcile: &str,
    product: &str,
    composition: &str,
    inspection: &str,
    participation: &str,
    destination_peers: &str,
) -> Vec<String> {
    let mut violations = Vec::new();

    // Rule 1 — the composition hands the product the handles the reconcile
    // needs, and the cadence.
    if let Some(violation) = check_block_contains(
        composition,
        PRODUCT_SPEC_ANCHOR,
        "bootstrap: live_bootstrap",
    ) {
        violations.push(violation);
    }
    if let Some(violation) = check_block_contains(
        composition,
        PRODUCT_SPEC_ANCHOR,
        "peer_profiles: Some(peer_profiles",
    ) {
        violations.push(violation);
    }
    if let Some(violation) =
        check_block_contains(composition, PRODUCT_SPEC_ANCHOR, "reconcile_period_ms")
    {
        violations.push(violation);
    }
    // The cadence is the **existing** participation maintenance period, so
    // the router has one source of truth and no second knob. The rule names
    // the whole statement, not the bare identifier: the participation owner
    // has its own use of the same period, and that use is not the
    // reconcile's cadence.
    if let Some(violation) = check_code_contains(
        composition,
        "let reconcile_period_ms = config.netdb.participation_maintenance_period_ms;",
    ) {
        violations.push(violation);
    }
    // The product owns the reconcile and the composition publishes its
    // counters.
    if let Some(violation) = check_struct_contains(product, "ProductInner", "peer_reconcile:") {
        violations.push(violation);
    }
    if let Some(violation) =
        check_code_contains(composition, "inspection.publish_peer_reconcile_counters(")
    {
        violations.push(violation);
    }
    // One publication path: a second publication would be a second seam.
    if let Some(violation) =
        check_code_occurs_once(composition, "inspection.publish_peer_reconcile_counters(")
    {
        violations.push(violation);
    }

    // Rule 2 — the product reconciles on its existing tick, and the
    // reconcile owns no task and no timer.
    if let Some(violation) = check_body_contains(
        product,
        "advance_destination_pools_at",
        "self.reconcile_peer_knowledge(",
    ) {
        violations.push(violation);
    }
    for token in RECONCILE_TASK_TOKENS {
        if let Some(violation) = check_body_avoids(product, "reconcile_peer_knowledge", token) {
            violations.push(violation);
        }
        if let Some(violation) = check_code_avoids(reconcile, token) {
            violations.push(violation);
        }
    }

    // Rule 3 — the locks are taken in one order and never nested.
    if let Some(violation) = check_lock_order(reconcile) {
        violations.push(violation);
    }
    if let Some(violation) = check_body_contains(reconcile, "snapshot", SNAPSHOT_CALL) {
        violations.push(violation);
    }
    if let Some(violation) = check_body_contains(reconcile, "snapshot", "bootstrap.lock()") {
        violations.push(violation);
    }
    if let Some(violation) = check_body_contains(reconcile, "insert_all", "destinations.lock()") {
        violations.push(violation);
    }
    if let Some(violation) = check_body_contains(reconcile, "learn_all", "profiles.with(") {
        violations.push(violation);
    }
    // Each step owns exactly one lock: no step may name another's.
    if let Some(violation) =
        check_lock_isolation(reconcile, "snapshot", &["destinations", "profiles"])
    {
        violations.push(violation);
    }
    if let Some(violation) =
        check_lock_isolation(reconcile, "insert_all", &["bootstrap", "profiles"])
    {
        violations.push(violation);
    }
    if let Some(violation) =
        check_lock_isolation(reconcile, "learn_all", &["bootstrap", "destinations"])
    {
        violations.push(violation);
    }

    // Rule 4 — the canonical write APIs, no direct store manipulation, no
    // selection-side write.
    if let Some(violation) = check_body_contains(reconcile, "insert_all", DESTINATION_WRITE_CALL) {
        violations.push(violation);
    }
    if let Some(violation) = check_body_contains(reconcile, "learn_all", PROFILE_WRITE_CALL) {
        violations.push(violation);
    }
    if let Some(violation) = check_code_avoids(reconcile, DIRECT_STORE_WRITE_CALL) {
        violations.push(violation);
    }
    for token in SELECTION_TOKENS {
        if let Some(violation) = check_code_avoids(reconcile, token) {
            violations.push(violation);
        }
    }

    // Rule 5 — the reconcile is cadenced and the cadence is bounded.
    if let Some(violation) = check_body_contains(reconcile, "run", "cadence_due(") {
        violations.push(violation);
    }
    if let Some(violation) = check_body_contains(reconcile, "cadence_due", "self.period_ms") {
        violations.push(violation);
    }
    for bound in ["MIN_RECONCILE_PERIOD_MS", "MAX_RECONCILE_PERIOD_MS"] {
        if let Some(violation) = check_code_contains(reconcile, bound) {
            violations.push(violation);
        }
    }
    if let Some(violation) = check_body_contains(reconcile, "new", "MIN_RECONCILE_PERIOD_MS") {
        violations.push(violation);
    }
    if let Some(violation) = check_body_contains(reconcile, "new", "MAX_RECONCILE_PERIOD_MS") {
        violations.push(violation);
    }

    // Rule 6 — a capacity refusal is counted, not fatal.
    //
    // The rule checks the **count** and not merely the token: naming the
    // variant without recording it would leave a capacity refusal invisible.
    if let Some(violation) = check_body_contains(
        reconcile,
        "record_insert",
        "InsertOutcome::CapacityExceeded",
    ) {
        violations.push(violation);
    }
    if let Some(violation) =
        check_body_contains(reconcile, "record_insert", "self.capacity_exceeded")
    {
        violations.push(violation);
    }
    if let Some(violation) = check_body_avoids(reconcile, "insert_all", "break") {
        violations.push(violation);
    }
    if let Some(violation) = check_body_avoids(reconcile, "insert_all", "?") {
        violations.push(violation);
    }

    // Rule 7 — the unsolicited store is ingested once.
    if let Some(violation) = check_ingest_once_per_branch(composition) {
        violations.push(violation);
    }

    // Rule 8 — the counters are published and the reader returns a gap.
    if let Some(violation) = check_body_contains(
        inspection,
        "publish_peer_reconcile_counters",
        "peer_reconcile_live",
    ) {
        violations.push(violation);
    }
    if let Some(violation) = check_reader_returns_a_gap(inspection) {
        violations.push(violation);
    }

    // Rule 9 — Plan 491's gate is still applied at selection, and the
    // refresh never writes a selection.
    if let Some(violation) =
        check_body_contains(participation, "select_version_gated", "version_gate: Some(")
    {
        violations.push(violation);
    }
    if let Some(violation) =
        check_body_contains(destination_peers, "from_validated", "version_refusal(")
    {
        violations.push(violation);
    }

    violations
}

/// The landed sources the rules are applied to.
struct Landed {
    reconcile: String,
    product: String,
    composition: String,
    inspection: String,
    participation: String,
    destination_peers: String,
}

fn landed() -> Landed {
    Landed {
        reconcile: source_of(RECONCILE_MODULE),
        product: source_of(PRODUCT_MODULE),
        composition: source_of(COMPOSITION_MODULE),
        inspection: source_of(INSPECTION_MODULE),
        participation: source_of(PARTICIPATION_MODULE),
        destination_peers: source_of(DESTINATION_PEERS_MODULE),
    }
}

fn violations_of(landed: &Landed) -> Vec<String> {
    check_all(
        &landed.reconcile,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    )
}

// ----------------------------------------------------------------------
// The tests.
// ----------------------------------------------------------------------

#[test]
fn the_landed_tree_holds_every_rule() {
    let landed = landed();
    let violations = violations_of(&landed);
    assert!(
        violations.is_empty(),
        "the Plan 506 boundary rules were violated:\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_composition_must_hand_the_product_the_bootstrap_handle() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.composition,
        "bootstrap: live_bootstrap.clone()",
        "bootstrap: None",
        0,
    );
    assert_ne!(
        mutated, landed.composition,
        "the mutation must apply: the composition must name the bootstrap handle"
    );
    let violations = check_all(
        &landed.reconcile,
        &landed.product,
        &mutated,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("bootstrap: live_bootstrap")),
        "removing the bootstrap handle must be reported, got: {violations:?}"
    );
}

#[test]
fn the_composition_must_hand_the_product_the_peer_profile_handle() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.composition,
        "peer_profiles: Some(peer_profiles.clone())",
        "peer_profiles: None",
        0,
    );
    assert_ne!(
        mutated, landed.composition,
        "the mutation must apply: the composition must name the peer-profile handle"
    );
    let violations = check_all(
        &landed.reconcile,
        &landed.product,
        &mutated,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("peer_profiles: Some(peer_profiles")),
        "removing the peer-profile handle must be reported, got: {violations:?}"
    );
}

#[test]
fn the_cadence_must_be_the_existing_participation_period() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.composition,
        "let reconcile_period_ms = config.netdb.participation_maintenance_period_ms;",
        "let reconcile_period_ms = config.netdb.routing_table_max_entries as u64;",
        0,
    );
    assert_ne!(
        mutated, landed.composition,
        "the mutation must apply: the composition must reuse the participation period"
    );
    let violations = check_all(
        &landed.reconcile,
        &landed.product,
        &mutated,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("participation_maintenance_period_ms")),
        "a second cadence knob must be reported, got: {violations:?}"
    );
}

#[test]
fn the_product_must_reconcile_on_its_existing_tick() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.product,
        "self.reconcile_peer_knowledge(now_ms).await;",
        "",
        0,
    );
    assert_ne!(
        mutated, landed.product,
        "the mutation must apply: the tick must drive the reconcile"
    );
    let violations = check_all(
        &landed.reconcile,
        &mutated,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("self.reconcile_peer_knowledge(")),
        "an unreconciled tick must be reported, got: {violations:?}"
    );
}

#[test]
fn the_reconcile_must_not_own_a_task_or_a_timer() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "pub async fn run(&mut self, now_ms: u64)",
        "pub async fn run(&mut self, now_ms: u64) { let _ = tokio::spawn(async {});",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the reconcile must have a run method"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("tokio::spawn")),
        "a reconcile-owned task must be reported, got: {violations:?}"
    );
}

#[test]
fn the_reconcile_must_not_own_a_timer() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "pub async fn run(&mut self, now_ms: u64)",
        "pub async fn run(&mut self, now_ms: u64) { let _ = tokio::time::interval(std::time::Duration::from_secs(1));",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the reconcile must have a run method"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("tokio::time::interval")),
        "a reconcile-owned timer must be reported, got: {violations:?}"
    );
}

#[test]
fn the_lock_order_must_be_snapshot_then_insert_then_learn() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "self.insert_all(&records).await;",
        "self.learn_all(&records);",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the run method must insert"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("snapshot, insert, learn")),
        "a reordered lock sequence must be reported, got: {violations:?}"
    );
}

#[test]
fn the_snapshot_must_not_hold_another_lock() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "Some(guard.validated_router_info_snapshot())",
        "Some({ let _ = &self.destinations; guard.validated_router_info_snapshot() })",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the snapshot must exist"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("`fn snapshot` must not mention")),
        "a snapshot that names another lock must be reported, got: {violations:?}"
    );
}

#[test]
fn the_insert_must_not_hold_the_bootstrap_lock() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "let mut guard = destinations.lock().await;",
        "let mut guard = destinations.lock().await; let _bootstrap = bootstrap;",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the insert must take the destination lock"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("`fn insert_all` must not mention")),
        "an insert that names the bootstrap lock must be reported, got: {violations:?}"
    );
}

#[test]
fn the_learn_must_not_hold_the_destination_lock() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "let summary = profiles.with(|driver| driver.learn_all(records.iter().cloned()));",
        "let summary = profiles.with(|driver| { let _ = destinations; driver.learn_all(records.iter().cloned()) });",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the learn must use the profile handle"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("`fn learn_all` must not mention")),
        "a learn that names the destination lock must be reported, got: {violations:?}"
    );
}

#[test]
fn the_reconcile_must_write_through_the_canonical_write_apis() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "let outcome = guard.insert_validated_router_info(record.clone());",
        "let outcome = i2pr_netdb::InsertOutcome::Idempotent;",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the insert must call the canonical write API"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains(DESTINATION_WRITE_CALL)),
        "a direct store write must be reported, got: {violations:?}"
    );
}

#[test]
fn the_reconcile_must_learn_through_the_canonical_write_api() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "driver.learn_all(records.iter().cloned())",
        "driver.learn(records.iter().cloned().next().expect(\"one\"))",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the learn must call the canonical write API"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains(PROFILE_WRITE_CALL)),
        "a single-peer learn must be reported, got: {violations:?}"
    );
}

#[test]
fn the_reconcile_must_not_write_a_selection() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "self.learn_all(&records);",
        "self.learn_all(&records); let _ = select_tunnel_build_peers(1, 0);",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the run method must learn"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("select_tunnel_build_peers(")),
        "a selection-side write must be reported, got: {violations:?}"
    );
}

#[test]
fn the_reconcile_must_be_cadenced() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "if !self.cadence_due(now_ms) {",
        "if false {",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the run method must gate on the cadence"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("cadence_due(")),
        "an uncadenced reconcile must be reported, got: {violations:?}"
    );
}

#[test]
fn the_cadence_must_be_bounded() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "if !(MIN_RECONCILE_PERIOD_MS..=MAX_RECONCILE_PERIOD_MS).contains(&period_ms) {",
        "if false {",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the constructor must bound the cadence"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("MAX_RECONCILE_PERIOD_MS")),
        "an unbounded cadence must be reported, got: {violations:?}"
    );
}

#[test]
fn a_capacity_refusal_must_be_counted_not_fatal() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "self.capacity_exceeded = self.capacity_exceeded.saturating_add(1);",
        "let _ = 0;",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the counter must record a capacity refusal"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("self.capacity_exceeded")),
        "an uncounted capacity refusal must be reported, got: {violations:?}"
    );
}

#[test]
fn the_insert_must_not_abort_the_batch_on_a_refusal() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.reconcile,
        "let outcome = guard.insert_validated_router_info(record.clone());",
        "let outcome = match guard.insert_validated_router_info(record.clone()) { i2pr_netdb::InsertOutcome::CapacityExceeded => break, other => other };",
        0,
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the insert must call the canonical write API"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("`fn insert_all` must not contain `break`")),
        "a batch-aborting refusal must be reported, got: {violations:?}"
    );
}

#[test]
fn the_unsolicited_store_must_be_ingested_once() {
    let landed = landed();
    // Re-introduce the duplicate ingest: the same message is written twice
    // in one loop iteration.
    let anchor = "                                product_effect |= served;\n";
    let duplicate = "                                product_effect |= served;\n                                if !served\n                                    && let Some(i2pr_proto::I2npBody::DatabaseStore(store)) =\n                                        message.map(i2pr_proto::I2npMessage::body)\n                                    && let Some(bootstrap) = bootstrap.as_ref()\n                                {\n                                    match bootstrap.lock() {\n                                        Ok(mut bootstrap) => match bootstrap\n                                            .ingest_authenticated_router_store(store, now_ms)\n                                        {\n                                            Ok(_) => {}\n                                            Err(_) => {}\n                                        },\n                                        Err(_) => {}\n                                    }\n                                }\n";
    let mutated = replace_occurrence(&landed.composition, anchor, duplicate, 0);
    assert_ne!(
        mutated, landed.composition,
        "the mutation must apply: the served flag must be recorded"
    );
    let violations = check_all(
        &landed.reconcile,
        &landed.product,
        &mutated,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("ingested once per branch")),
        "a duplicate ingest must be reported, got: {violations:?}"
    );
}

#[test]
fn the_counters_must_be_published_through_the_inspection_seam() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.composition,
        "inspection.publish_peer_reconcile_counters(",
        "inspection.publish_exploratory_driver_counters(",
        0,
    );
    assert_ne!(
        mutated, landed.composition,
        "the mutation must apply: the composition must publish the reconcile counters"
    );
    let violations = check_all(
        &landed.reconcile,
        &landed.product,
        &mutated,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("publish_peer_reconcile_counters(")),
        "an unpublished counter must be reported, got: {violations:?}"
    );
}

#[test]
fn the_seam_reader_must_return_a_gap_not_a_zero() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.inspection,
        "let cell = self.peer_reconcile_counters()?;",
        "let cell = self.peer_reconcile_counters().unwrap_or_else(|| Arc::new(std::sync::Mutex::new(crate::peer_reconcile::PeerReconcileCounters::default())));",
        0,
    );
    assert_ne!(
        mutated, landed.inspection,
        "the mutation must apply: the reader must take the cell"
    );
    let violations = check_all(
        &landed.reconcile,
        &landed.product,
        &landed.composition,
        &mutated,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation
                .contains("must take the cell through the gap-returning reader")),
        "a fabricated zero must be reported, got: {violations:?}"
    );
}

#[test]
fn the_version_gate_must_stay_at_selection() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.participation,
        "version_gate: Some(kind.selection_gate()),",
        "version_gate: None,",
        0,
    );
    assert_ne!(
        mutated, landed.participation,
        "the mutation must apply: the selection must carry the gate"
    );
    let violations = check_all(
        &landed.reconcile,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &mutated,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("version_gate: Some(")),
        "a removed version gate must be reported, got: {violations:?}"
    );
}

#[test]
fn the_per_record_version_gate_must_stay_on_the_destination_path() {
    let landed = landed();
    let mutated = replace_occurrence(
        &landed.destination_peers,
        "if let Some(refusal) = version_refusal(eligibility) {",
        "if let Some(refusal) = None.or_else(|| Option::<CandidateRefusal>::None) {",
        0,
    );
    assert_ne!(
        mutated, landed.destination_peers,
        "the mutation must apply: the per-record gate must be applied"
    );
    let violations = check_all(
        &landed.reconcile,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &mutated,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("version_refusal(")),
        "a removed per-record gate must be reported, got: {violations:?}"
    );
}

// ----------------------------------------------------------------------
// The benign and sibling probes.
// ----------------------------------------------------------------------

#[test]
fn a_construct_in_a_comment_is_not_reported() {
    let landed = landed();
    let mutated = format!(
        "{}\n// tokio::spawn and tokio::time::interval are only ever named here.\n/* select_tunnel_build_peers( is a comment too. */\n",
        landed.reconcile
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations.is_empty(),
        "a comment must not be reported, got: {violations:?}"
    );
}

#[test]
fn a_construct_in_a_test_block_is_not_reported() {
    let landed = landed();
    let mutated = format!(
        "{}\n#[cfg(test)]\nmod extra_tests {{\n    fn helper() {{\n        let _ = tokio::spawn(async {{}});\n        let _ = select_tunnel_build_peers(1, 0);\n    }}\n}}\n",
        landed.reconcile
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations.is_empty(),
        "a test block must not be reported, got: {violations:?}"
    );
}

#[test]
fn a_token_in_a_sibling_function_does_not_satisfy_a_rule() {
    let landed = landed();
    // A call in a *different* function of the reconcile module must not
    // satisfy the rule that the reconcile's `run` calls the cadence gate.
    let mutated = format!(
        "{}\nfn sibling_helper() {{\n    let _ = cadence_due(0);\n}}\n",
        landed.reconcile
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations.is_empty(),
        "a sibling function must not break a rule, got: {violations:?}"
    );
}

#[test]
fn an_absent_function_fails_closed() {
    let landed = landed();
    let mutated = landed.reconcile.replace(
        "fn cadence_due(&self, now_ms: u64) -> bool {",
        "fn cadence_due_renamed(&self, now_ms: u64) -> bool {",
    );
    assert_ne!(
        mutated, landed.reconcile,
        "the mutation must apply: the cadence gate must exist"
    );
    let violations = check_all(
        &mutated,
        &landed.product,
        &landed.composition,
        &landed.inspection,
        &landed.participation,
        &landed.destination_peers,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("`fn cadence_due` must exist")),
        "an absent function must fail closed, got: {violations:?}"
    );
}

#[test]
fn the_guard_reads_the_real_tree() {
    // The guard must not be satisfied by a stub: every module it reads
    // must exist and be non-trivial.
    assert!(source_of(RECONCILE_MODULE).len() > 4_000);
    assert!(source_of(PRODUCT_MODULE).contains("reconcile_peer_knowledge"));
    assert!(source_of(COMPOSITION_MODULE).contains("publish_peer_reconcile_counters"));
    assert!(source_of(INSPECTION_MODULE).contains("peer_reconcile_counters_snapshot"));
}
