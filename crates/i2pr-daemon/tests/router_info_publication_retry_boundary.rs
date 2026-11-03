//! Plan 517's structural guard for the publication's **retry**.
//!
//! Plan 515 made the router's own RouterInfo publication periodic. Plan 516
//! made it confirmable. Plan 517 makes an **unconfirmed** publication
//! **retried**: the reference does not publish once and wait 39 minutes --
//! it publishes, and while the answer has not arrived it asks the next
//! closest floodfill (`libi2pd/RouterContext.cpp:1560`).
//!
//! The guard asserts seven things about the **production** surface:
//!
//! 1. the retry rides the loop's **existing** tick, and **no new timer** is
//!    introduced;
//! 2. the deadline is the reference's `ROUTER_INFO_CONFIRMATION_TIMEOUT`;
//! 3. the cycle's **exclusion set** reaches the selection, so a target is
//!    never asked twice;
//! 4. the cycle's bound is the reference's
//!    `ROUTER_INFO_MAX_PUBLISH_EXCLUDED_FLOODFILLS`;
//! 5. a **confirmation ends the cycle**;
//! 6. the cycle **never reads a clock** -- it takes the reading from its
//!    caller, which is what makes it provable;
//! 7. the fan-out stays **bounded**.
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

/// Rule 1: the retry rides the loop's existing tick, and no new timer is
/// introduced.
///
/// The confirmation deadline is observed on the loop's existing 100
/// millisecond tick (`crates/i2pr-daemon/src/lib.rs`), so the retry must be
/// driven from that tick -- and the retry must not arm a timer of its own.
fn check_the_retry_rides_the_existing_tick(composition: &str) -> Option<String> {
    let code = code_view(composition);
    if !code.contains("relay_tick.tick()") {
        return Some("the composition root does not tick the relay".to_owned());
    }
    if !code.contains("crate::router_info_publication::advance_publication_cycle(") {
        return Some("the composition root does not advance the publication cycle".to_owned());
    }
    if !code.contains("publication_owner.cycle_expired(") {
        return Some("the composition root does not test the cycle's deadline".to_owned());
    }
    // No new timer: no interval may be armed with the reference's
    // confirmation timeout.
    const INTERVAL: &str = "tokio::time::interval(";
    let mut offset = 0usize;
    while let Some(position) = code[offset..].find(INTERVAL) {
        let start = offset + position;
        let after = &code[start + INTERVAL.len()..];
        let end = after
            .find(");")
            .map(|length| length + 2)
            .unwrap_or(after.len());
        if after[..end].contains("PUBLICATION_CONFIRMATION_TIMEOUT_MS") {
            return Some("the retry introduces a timer of its own".to_owned());
        }
        offset = start + INTERVAL.len();
    }
    None
}

/// Rule 2: the deadline is the reference's constant.
fn check_the_deadline_is_the_reference_constant(source: &str) -> Option<String> {
    let code = code_view(source);
    if !code.contains("PUBLICATION_CONFIRMATION_TIMEOUT_MS: u64 = 1_600") {
        return Some(
            "the confirmation timeout is not the reference's 1600 milliseconds".to_owned(),
        );
    }
    let Some(body) = item_body(&code, "pub fn record_batch(") else {
        return Some("the cycle's batch recorder is absent".to_owned());
    };
    if !body.contains("PUBLICATION_CONFIRMATION_TIMEOUT_MS") {
        return Some("the cycle's deadline is not the reference's timeout".to_owned());
    }
    None
}

/// Rule 3: the cycle's exclusion set reaches the selection.
///
/// A target the cycle has already asked must never be asked twice, so the
/// exclusion set has to reach the **selection** -- and the selection has to
/// keep going through the version gate.
fn check_the_exclusion_set_reaches_the_selection(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub fn publication_targets_excluding(") else {
        return Some("the cycle's selection is absent".to_owned());
    };
    if !body.contains("select_version_gated_floodfills(") {
        return Some("the cycle's selection does not go through the version gate".to_owned());
    }
    let Some(batch) = item_body(&code, "pub fn cycle_batch(") else {
        return Some("the cycle's batch builder is absent".to_owned());
    };
    if !batch.contains("publication_targets_excluding(") {
        return Some("the cycle does not exclude the targets it already asked".to_owned());
    }
    if !batch.contains("self.cycle.asked") {
        return Some("the cycle's exclusion set is not the cycle's own".to_owned());
    }
    None
}

/// Rule 4: the cycle's bound is the reference's constant.
fn check_the_bound_is_the_reference_constant(source: &str) -> Option<String> {
    let code = code_view(source);
    if !code.contains("PUBLICATION_CYCLE_TARGETS: usize = 15") {
        return Some("the cycle's bound is not the reference's fifteen".to_owned());
    }
    let Some(body) = item_body(&code, "pub fn cycle_reached_bound(") else {
        return Some("the cycle's bound test is absent".to_owned());
    };
    if !body.contains("PUBLICATION_CYCLE_TARGETS") {
        return Some("the cycle's bound is not the reference's fifteen".to_owned());
    }
    None
}

/// Rule 5: a confirmation ends the cycle.
///
/// The reference clears its exclusion set and schedules the next publish at
/// the full interval (`libi2pd/RouterContext.cpp:1321-1328`).
fn check_a_confirmation_ends_the_cycle(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub fn confirm(") else {
        return Some("the owner's confirmation is absent".to_owned());
    };
    if !body.contains("end_cycle(") {
        return Some("a confirmation does not end the cycle".to_owned());
    }
    None
}

/// Rule 6: the cycle never reads a clock.
///
/// The cycle takes the reading from its caller, so the whole ladder is
/// provable without a wall clock, a socket or a sleep.
fn check_the_cycle_never_reads_a_clock(source: &str) -> Option<String> {
    let code = code_view(source);
    for signature in [
        "pub fn begin_cycle(",
        "pub fn cycle_batch(",
        "pub fn cycle_step(",
        "pub fn record_batch(",
        "pub fn cycle_expired(",
        "pub fn abandon_batch(",
        "pub fn end_cycle(",
        "pub fn halt_cycle(",
        "pub fn cycle_reached_bound(",
        "pub fn record_cycle_exhausted(",
    ] {
        let Some(body) = item_body(&code, signature) else {
            return Some(format!("the cycle's {signature} is absent"));
        };
        for forbidden in ["SystemTime", "Instant", "wall_clock", "now()", "clock()"] {
            if body.contains(forbidden) {
                return Some(format!("the cycle reads a clock: {forbidden}"));
            }
        }
    }
    None
}

/// Rule 7: the fan-out stays bounded.
fn check_the_fan_out_stays_bounded(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub fn cycle_batch(") else {
        return Some("the cycle's batch builder is absent".to_owned());
    };
    if !body.contains("take(PUBLICATION_FLOODFILLS)") {
        return Some("the cycle's fan-out is not bounded".to_owned());
    }
    None
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

#[test]
fn the_retry_rides_the_existing_tick() {
    let reason = check_the_retry_rides_the_existing_tick(&source_of(COMPOSITION_MODULE));
    assert!(
        reason.is_none(),
        "the retry must ride the loop's existing tick: {reason:?}"
    );
}

#[test]
fn the_deadline_is_the_reference_constant() {
    let reason = check_the_deadline_is_the_reference_constant(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the deadline must be the reference's timeout: {reason:?}"
    );
}

#[test]
fn the_exclusion_set_reaches_the_selection() {
    let reason = check_the_exclusion_set_reaches_the_selection(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the exclusion set must reach the selection: {reason:?}"
    );
}

#[test]
fn the_bound_is_the_reference_constant() {
    let reason = check_the_bound_is_the_reference_constant(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the bound must be the reference's fifteen: {reason:?}"
    );
}

#[test]
fn a_confirmation_ends_the_cycle() {
    let reason = check_a_confirmation_ends_the_cycle(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "a confirmation must end the cycle: {reason:?}"
    );
}

#[test]
fn the_cycle_never_reads_a_clock() {
    let reason = check_the_cycle_never_reads_a_clock(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the cycle must not read a clock: {reason:?}"
    );
}

#[test]
fn the_fan_out_stays_bounded() {
    let reason = check_the_fan_out_stays_bounded(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the fan-out must stay bounded: {reason:?}"
    );
}

// ----------------------------------------------------------------------
// The inverted probes: each rule must report its own violation.
// ----------------------------------------------------------------------

/// A minimal cycle module, for the inverted probes.
///
/// Every cycle function is present, so a probe that changes one of them
/// fails for the reason the probe is about and not for a missing function.
fn minimal_cycle_module() -> String {
    "\
pub fn begin_cycle(&mut self) -> bool {
    true
}

pub fn cycle_batch(&self, store: &u8) -> Option<u8> {
    let _ = store;
    None
}

pub fn cycle_step(&self, store: &u8) -> u8 {
    let _ = store;
    0
}

pub fn record_batch(&mut self, now_ms: u64, targets: &[u8]) {
    let _ = now_ms;
    let _ = targets;
}

pub fn cycle_expired(&self, now_ms: u64) -> bool {
    now_ms > 0
}

pub fn abandon_batch(&mut self, now_ms: u64) -> bool {
    let _ = now_ms;
    false
}

pub fn end_cycle(&mut self) {}

pub fn halt_cycle(&mut self) {}

pub fn cycle_reached_bound(&self) -> bool {
    false
}

pub fn record_cycle_exhausted(&mut self) {}
"
    .to_owned()
}

/// The inverted probe for rule 1: a composition that never advances the
/// cycle must be reported.
#[test]
fn the_guard_reports_a_composition_that_never_advances_the_cycle() {
    let violating = "\
fn compose() {
    let _ = relay_tick.tick();
}
";
    let reason = check_the_retry_rides_the_existing_tick(violating);
    assert!(
        reason.is_some(),
        "a composition that never advances the cycle must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 1, the other side: a retry that arms a timer
/// of its own must be reported.
#[test]
fn the_guard_reports_a_retry_that_arms_a_timer_of_its_own() {
    let violating = "\
fn compose() {
    let _ = relay_tick.tick();
    crate::router_info_publication::advance_publication_cycle();
    let _ = publication_owner.cycle_expired(0);
    let _ = tokio::time::interval(Duration::from_millis(
        crate::router_info_publication::PUBLICATION_CONFIRMATION_TIMEOUT_MS,
    ));
}
";
    let reason = check_the_retry_rides_the_existing_tick(violating);
    assert!(
        reason.is_some(),
        "a retry with a timer of its own must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 2: a deadline that is not the reference's
/// timeout must be reported.
#[test]
fn the_guard_reports_a_deadline_that_is_not_the_reference_timeout() {
    let violating = "\
pub const PUBLICATION_CONFIRMATION_TIMEOUT_MS: u64 = 5_000;

pub fn record_batch(&mut self, now_ms: u64, targets: &[u8]) {
    let _ = now_ms;
    let _ = targets;
}
";
    let reason = check_the_deadline_is_the_reference_constant(violating);
    assert!(
        reason.is_some(),
        "a deadline that is not the reference's timeout must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 3: a cycle that selects without its
/// exclusion set must be reported.
#[test]
fn the_guard_reports_a_cycle_that_selects_without_its_exclusion_set() {
    let violating = "\
pub fn publication_targets_excluding(store: &u8) -> u8 {
    let _ = store;
    0
}

pub fn cycle_batch(&self, store: &u8) -> Option<u8> {
    let _ = store;
    None
}
";
    let reason = check_the_exclusion_set_reaches_the_selection(violating);
    assert!(
        reason.is_some(),
        "a cycle without an exclusion set must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 4: a bound that is not the reference's
/// fifteen must be reported.
#[test]
fn the_guard_reports_a_bound_that_is_not_the_reference_fifteen() {
    let violating = "pub const PUBLICATION_CYCLE_TARGETS: usize = 64;\n";
    let reason = check_the_bound_is_the_reference_constant(violating);
    assert!(
        reason.is_some(),
        "a bound that is not the reference's fifteen must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 5: a confirmation that does not end the
/// cycle must be reported.
#[test]
fn the_guard_reports_a_confirmation_that_does_not_end_the_cycle() {
    let violating = "\
pub fn confirm(&mut self, message_id: u32) -> u32 {
    let _ = message_id;
    0
}
";
    let reason = check_a_confirmation_ends_the_cycle(violating);
    assert!(
        reason.is_some(),
        "a confirmation that does not end the cycle must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 6: a cycle that reads a clock must be
/// reported.
#[test]
fn the_guard_reports_a_cycle_that_reads_a_clock() {
    let mut violating = minimal_cycle_module();
    let original = "\
pub fn record_batch(&mut self, now_ms: u64, targets: &[u8]) {
    let _ = now_ms;
    let _ = targets;
}
";
    assert_eq!(
        violating.matches(original).count(),
        1,
        "the probe's anchor must occur exactly once"
    );
    violating = violating.replace(
        original,
        "\
pub fn record_batch(&mut self, now_ms: u64, targets: &[u8]) {
    let _ = SystemTime::now();
    let _ = now_ms;
    let _ = targets;
}
",
    );
    let reason = check_the_cycle_never_reads_a_clock(&violating);
    assert!(
        reason.is_some(),
        "a cycle that reads a clock must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 7: an unbounded fan-out must be reported.
#[test]
fn the_guard_reports_an_unbounded_fan_out() {
    let violating = "\
pub fn cycle_batch(&self, store: &u8) -> Option<u8> {
    let _ = store;
    None
}
";
    let reason = check_the_fan_out_stays_bounded(violating);
    assert!(
        reason.is_some(),
        "an unbounded fan-out must be reported: {reason:?}"
    );
}
