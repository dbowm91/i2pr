//! Plan 515's structural guard for the router's own RouterInfo publication.
//!
//! Until this plan the router published its own RouterInfo **once**, from
//! the service-tunnel provisioning path, and never again. A record that is
//! never refreshed ages out of the network's view, so no peer can find the
//! router however long it has been running -- and a router with no service
//! tunnels never published at all.
//!
//! The guard asserts seven things about the **production** surface:
//!
//! 1. the refresh is driven by the **router's own** tick, not by the
//!    service product;
//! 2. the service product's one-shot publication goes through the
//!    **shared** delivery path;
//! 3. the selection rule is **shared**: the coordinator's publication
//!    selection is the same function the refresh uses;
//! 4. the publication never **bypasses** the version gate, so Plan 503's
//!    API-version floor holds for every publication;
//! 5. the fan-out stays **bounded**;
//! 6. the schedule **never reads a clock** -- it takes `now_ms` from its
//!    caller, which is what makes it provable;
//! 7. the publication stops when it is **cancelled**.
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

/// The publication module: the schedule, the plan and the shared delivery.
const PUBLICATION_MODULE: &str = "crates/i2pr-daemon/src/router_info_publication.rs";
/// The composition root: the router's own refresh tick.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";
/// The service product: the one-shot publication.
const SERVICE_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";
/// The destination coordinator: the publication's selection.
const DESTINATION_MODULE: &str = "crates/i2pr-daemon/src/destination_tunnels.rs";

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

/// Rule 1: the refresh is driven by the router's own tick.
fn check_the_refresh_is_driven_by_the_router_own_tick(source: &str) -> Option<String> {
    let code = code_view(source);
    if !code.contains("publication_tick.tick()") {
        return Some("the composition root does not tick the publication".to_owned());
    }
    if !code.contains("crate::router_info_publication::refresh_router_info(") {
        return Some("the composition root does not call the refresh".to_owned());
    }
    None
}

/// Rule 2: the one-shot publication uses the shared delivery path.
fn check_the_one_shot_publication_uses_the_shared_path(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "async fn publish_creator_router_info(") else {
        return Some("the creator RouterInfo publication is absent".to_owned());
    };
    if !body.contains("crate::router_info_publication::publish_router_info(") {
        return Some("the creator publication does not use the shared delivery".to_owned());
    }
    None
}

/// Rule 3: the refresh and the coordinator select through the **same**
/// gated function.
///
/// The coordinator applies it over its own store and leaves a DSM receipt;
/// the refresh applies it over the router-wide bootstrap's store. The rule
/// is the same function, so Plan 503's API-version floor holds for both.
fn check_the_selection_rule_is_shared(destination: &str, publication: &str) -> Option<String> {
    let destination_code = code_view(destination);
    let publication_code = code_view(publication);
    let Some(coordinator) = item_body(
        &destination_code,
        "pub fn version_gated_publication_floodfills(",
    ) else {
        return Some("the coordinator's publication selection is absent".to_owned());
    };
    if !coordinator.contains("select_version_gated_floodfills(") {
        return Some(
            "the coordinator's publication selection does not go through the version gate"
                .to_owned(),
        );
    }
    let Some(refresh) = item_body(&publication_code, "pub fn publication_targets(") else {
        return Some("the publication's selection is absent".to_owned());
    };
    if !refresh.contains("select_version_gated_floodfills(") {
        return Some("the refresh's selection does not go through the version gate".to_owned());
    }
    None
}

/// Rule 4: the publication never bypasses the version gate.
fn check_the_publication_never_bypasses_the_version_gate(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub fn publication_targets(") else {
        return Some("the publication's selection is absent".to_owned());
    };
    if !body.contains("select_version_gated_floodfills(") {
        return Some("the publication's selection does not go through the version gate".to_owned());
    }
    None
}

/// Rule 5: the fan-out stays bounded.
fn check_the_fan_out_stays_bounded(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub fn plan(") else {
        return Some("the publication's plan is absent".to_owned());
    };
    if !body.contains("take(PUBLICATION_FLOODFILLS)") {
        return Some("the publication's fan-out is not bounded".to_owned());
    }
    None
}

/// Rule 6: the schedule never reads a clock.
fn check_the_schedule_never_reads_a_clock(source: &str) -> Option<String> {
    let code = code_view(source);
    for signature in ["pub fn is_due(", "pub fn record("] {
        let Some(body) = item_body(&code, signature) else {
            return Some(format!("the schedule's {signature} is absent"));
        };
        for forbidden in ["SystemTime", "Instant", "wall_clock", "now()"] {
            if body.contains(forbidden) {
                return Some(format!("the schedule reads a clock: {forbidden}"));
            }
        }
    }
    None
}

/// Rule 7: the publication stops when it is cancelled.
fn check_the_cancellation_is_honoured(source: &str) -> Option<String> {
    let code = code_view(source);
    // The anchor carries no parenthesis: the function is generic, so the
    // literal `publish_router_info(` is not a substring of the source.
    let Some(body) = item_body(&code, "pub async fn publish_router_info") else {
        return Some("the shared publication is absent".to_owned());
    };
    if !body.contains("cancellation.is_cancelled()") {
        return Some("the publication does not stop when it is cancelled".to_owned());
    }
    None
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

#[test]
fn the_refresh_is_driven_by_the_router_own_tick() {
    let reason = check_the_refresh_is_driven_by_the_router_own_tick(&source_of(COMPOSITION_MODULE));
    assert!(
        reason.is_none(),
        "the refresh must be the router's own: {reason:?}"
    );
}

#[test]
fn the_one_shot_publication_uses_the_shared_path() {
    let reason = check_the_one_shot_publication_uses_the_shared_path(&source_of(SERVICE_MODULE));
    assert!(
        reason.is_none(),
        "the one-shot publication must share the path: {reason:?}"
    );
}

#[test]
fn the_selection_rule_is_shared() {
    let reason = check_the_selection_rule_is_shared(
        &source_of(DESTINATION_MODULE),
        &source_of(PUBLICATION_MODULE),
    );
    assert!(
        reason.is_none(),
        "the selection rule must be shared: {reason:?}"
    );
}

#[test]
fn the_publication_never_bypasses_the_version_gate() {
    let reason =
        check_the_publication_never_bypasses_the_version_gate(&source_of(PUBLICATION_MODULE));
    assert!(reason.is_none(), "the version gate must hold: {reason:?}");
}

#[test]
fn the_fan_out_stays_bounded() {
    let reason = check_the_fan_out_stays_bounded(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the fan-out must stay bounded: {reason:?}"
    );
}

#[test]
fn the_schedule_never_reads_a_clock() {
    let reason = check_the_schedule_never_reads_a_clock(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the schedule must not read a clock: {reason:?}"
    );
}

#[test]
fn the_cancellation_is_honoured() {
    let reason = check_the_cancellation_is_honoured(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the cancellation must be honoured: {reason:?}"
    );
}

// ----------------------------------------------------------------------
// The inverted probes: each rule must report its own violation.
// ----------------------------------------------------------------------

/// The inverted probe for rule 1: a composition that never ticks the
/// publication must be reported.
#[test]
fn the_guard_reports_a_composition_that_never_ticks_the_publication() {
    let violating = "\
fn compose() {
    let _ = relay_tick.tick();
}
";
    let reason = check_the_refresh_is_driven_by_the_router_own_tick(violating);
    assert!(
        reason.is_some(),
        "a composition that never ticks the publication must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 2: a one-shot publication that keeps its own
/// delivery must be reported.
#[test]
fn the_guard_reports_a_one_shot_publication_that_keeps_its_own_delivery() {
    let violating = "\
async fn publish_creator_router_info() {
    let _ = ssu2_handle.delivery();
}
";
    let reason = check_the_one_shot_publication_uses_the_shared_path(violating);
    assert!(
        reason.is_some(),
        "a one-shot publication with its own delivery must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 3: a coordinator that computes its own
/// selection must be reported.
#[test]
fn the_guard_reports_a_coordinator_that_computes_its_own_selection() {
    let violating_destination = "\
impl DestinationTunnelCoordinator {
    pub fn version_gated_publication_floodfills(&self) -> VersionGatedFloodfillSelection {
        let _ = &self.store;
        VersionGatedFloodfillSelection::default()
    }
}
";
    let publication = source_of(PUBLICATION_MODULE);
    let reason = check_the_selection_rule_is_shared(violating_destination, &publication);
    assert!(
        reason.is_some(),
        "a coordinator with its own selection must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 3, the other side: a refresh that selects
/// without the gate must be reported.
#[test]
fn the_guard_reports_a_refresh_that_selects_without_the_gate() {
    let destination = source_of(DESTINATION_MODULE);
    let violating_publication = "\
pub fn publication_targets(store: &RouterInfoStore) -> VersionGatedFloodfillSelection {
    let _ = store;
    VersionGatedFloodfillSelection::default()
}
";
    let reason = check_the_selection_rule_is_shared(&destination, violating_publication);
    assert!(
        reason.is_some(),
        "a refresh that selects without the gate must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 4: a selection that skips the version gate
/// must be reported.
#[test]
fn the_guard_reports_a_selection_that_skips_the_version_gate() {
    let violating = "\
pub fn publication_targets(store: &RouterInfoStore) -> VersionGatedFloodfillSelection {
    let _ = store;
    VersionGatedFloodfillSelection::default()
}
";
    let reason = check_the_publication_never_bypasses_the_version_gate(violating);
    assert!(
        reason.is_some(),
        "a selection that skips the version gate must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 5: an unbounded fan-out must be reported.
#[test]
fn the_guard_reports_an_unbounded_fan_out() {
    let violating = "\
pub fn plan(&self, store: &RouterInfoStore) -> RouterInfoPublicationPlan {
    let _ = store;
    RouterInfoPublicationPlan::Refused(RouterInfoPublicationOutcome::RefusedNoTarget)
}
";
    let reason = check_the_fan_out_stays_bounded(violating);
    assert!(
        reason.is_some(),
        "an unbounded fan-out must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 6: a schedule that reads a clock must be
/// reported.
#[test]
fn the_guard_reports_a_schedule_that_reads_a_clock() {
    let violating = "\
pub fn is_due(&self, now_ms: u64) -> bool {
    let _ = std::time::SystemTime::now();
    now_ms > 0
}
";
    let reason = check_the_schedule_never_reads_a_clock(violating);
    assert!(
        reason.is_some(),
        "a schedule that reads a clock must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 7: a publication that ignores its
/// cancellation must be reported.
#[test]
fn the_guard_reports_a_publication_that_ignores_its_cancellation() {
    let violating = "\
pub async fn publish_router_info(targets: &[RouterHash]) -> RouterInfoPublicationOutcome {
    let _ = targets;
    RouterInfoPublicationOutcome::Published {
        targets: 1,
        accepted: 1,
    }
}
";
    let reason = check_the_cancellation_is_honoured(violating);
    assert!(
        reason.is_some(),
        "a publication that ignores its cancellation must be reported: {reason:?}"
    );
}
