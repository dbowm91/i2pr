//! Plan 519's structural guard for the publication's **own task**.
//!
//! The router's own RouterInfo publication used to run inside the SSU2
//! service's `select!` loop. Its delivery awaited a session establishment
//! (`crates/i2pr-daemon/src/service_product.rs:7438` ->
//! `crates/i2pr-daemon/src/router_i2np.rs:2349` ->
//! `crates/i2pr-runtime/src/ssu2_runtime.rs:3480`), and while that await
//! was pending the loop could not drain its relay queue or correlate an
//! inbound `DeliveryStatus`.
//!
//! Neither pinned reference blocks its transport's loop on a handshake: i2pd
//! 2.61.0 posts the send and ignores the returned future
//! (`libi2pd/Transports.cpp:483-496`), and Java I2P 2.13.0 runs the
//! publication as a job on its own pool (`PublishLocalRouterInfoJob`,
//! `JobQueue.java:163`) whose store is handed to a `FloodfillStoreJob` with
//! its own `PUBLISH_TIMEOUT` (`FloodfillNetworkDatabaseFacade.java:174`).
//!
//! The guard asserts seven things about the **production** surface:
//!
//! 1. the transport's service loop **never awaits** the publication -- the
//!    publication's work is confined to the publication's own task;
//! 2. the publication's **state** is not owned by the loop: the loop binds
//!    no publication owner, and the owner is constructed once and handed to
//!    the task;
//! 3. the task is spawned with **explicit ownership and cancellation** --
//!    never on an ownerless task, and it stops when its cancellation fires;
//! 4. the acknowledgement handoff is **bounded**: a bounded channel, a
//!    bounded capacity, and a hand-over that never waits for room;
//! 5. the selection **prefers an established session**, and the targets it
//!    skips for want of one are counted;
//! 6. the publication's **delivery timeout** is bounded and owned by the
//!    publication;
//! 7. the transport's loop carries **no publication timer**.
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

/// The composition root: the SSU2 service's loop and the publication's task.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";
/// The publication module: the schedule, the cycle, the selection and the
/// delivery.
const PUBLICATION_MODULE: &str = "crates/i2pr-daemon/src/router_info_publication.rs";
/// The publication's task module: the bounded acknowledgement handoff.
const TASK_MODULE: &str = "crates/i2pr-daemon/src/router_info_publication_task.rs";

/// The publication's task, in the composition root.
const PUBLICATION_TASK: &str = "async fn run_router_info_publication_task";

/// The ceiling the publication's delivery timeout may not exceed, in
/// milliseconds.
///
/// The reference bounds one store attempt at 90 seconds
/// (`FloodfillNetworkDatabaseFacade.java:174`); the ceiling is deliberately
/// generous, because the rule is that the timeout is **bounded**, not that it
/// is the reference's exact value.
const DELIVERY_TIMEOUT_CEILING_MS: u64 = 300_000;

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

/// Returns the code with one item's own body **removed**.
///
/// The removal is the item's whole byte range, so the remainder is exactly
/// "the module without that item". An item that is not present yields
/// `None`, so every rule that uses it fails closed.
fn code_without_item(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (_, close) = brace_balanced_block(code, start + signature.len())?;
    let mut remainder = String::with_capacity(code.len());
    remainder.push_str(&code[..start]);
    remainder.push_str(&code[close..]);
    Some(remainder)
}

/// Returns the offset of a **call site** for `needle`.
///
/// The definition is skipped, so the returned offset is a call and never the
/// item's own declaration.
fn call_site(code: &str, needle: &str) -> Option<usize> {
    let definition = find_definition(code, needle);
    let mut offset = 0usize;
    while let Some(position) = code[offset..].find(needle) {
        let start = offset + position;
        offset = start + needle.len();
        if Some(start) == definition {
            continue;
        }
        return Some(start);
    }
    None
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1: the transport's service loop never awaits the publication.
///
/// The publication's work is confined to the publication's own task: the
/// refresh, the retry ladder and the acknowledgement correlation all live
/// there, and none of them is named anywhere else in the composition root.
fn check_the_loop_never_awaits_the_publication(composition: &str) -> Option<String> {
    let code = code_view(composition);
    let Some(remainder) = code_without_item(&code, PUBLICATION_TASK) else {
        return Some("the publication's task is absent".to_owned());
    };
    for needle in [
        "crate::router_info_publication::refresh_router_info(",
        "crate::router_info_publication::advance_publication_cycle(",
        "publication_owner.confirm(",
    ] {
        if remainder.contains(needle) {
            return Some(format!(
                "the transport's service loop awaits the publication: {needle}"
            ));
        }
    }
    None
}

/// Rule 2: the publication's state is not owned by the transport's loop.
///
/// The loop binds no publication owner, and the owner is constructed **once**
/// and handed to the task -- there is exactly one owner of the cycle.
fn check_the_publications_state_is_not_owned_by_the_loop(composition: &str) -> Option<String> {
    let code = code_view(composition);
    let Some(remainder) = code_without_item(&code, PUBLICATION_TASK) else {
        return Some("the publication's task is absent".to_owned());
    };
    for needle in ["let mut publication_owner", "let publication_owner"] {
        if remainder.contains(needle) {
            return Some(format!(
                "the transport's service loop binds the publication's state: {needle}"
            ));
        }
    }
    let constructions = code.matches("RouterInfoPublicationOwner::new(").count();
    if constructions != 1 {
        return Some(format!(
            "the publication's owner must be constructed exactly once, found {constructions}"
        ));
    }
    let handovers = remainder
        .matches("run_router_info_publication_task(")
        .count();
    if handovers != 1 {
        return Some(format!(
            "the composition root must hand the publication's task over exactly once, found {handovers}"
        ));
    }
    None
}

/// Rule 3: the publication's task is spawned with explicit ownership and
/// cancellation.
///
/// The task is spawned under the service's own child scope -- never on an
/// ownerless task -- in a moved closure that owns the publication's state,
/// and the task stops when its cancellation fires.
fn check_the_task_is_spawned_with_ownership_and_cancellation(composition: &str) -> Option<String> {
    let code = code_view(composition);
    if code.contains("tokio::spawn(") {
        return Some("the publication must not run on an ownerless task".to_owned());
    }
    let Some(call) = call_site(&code, "run_router_info_publication_task(") else {
        return Some("the composition root never calls the publication's task".to_owned());
    };
    let prefix = &code[..call];
    let Some(spawn) = prefix.rfind(".spawn(") else {
        return Some(
            "the publication's task must be spawned under the service's child scope".to_owned(),
        );
    };
    let closure = &prefix[spawn + ".spawn(".len()..];
    if !closure.contains("move |") {
        return Some(
            "the publication's task must be spawned in a moved closure that owns its state"
                .to_owned(),
        );
    }
    let window = &code[call..(call + 200).min(code.len())];
    if !window.contains("cancellation") {
        return Some(
            "the publication's task must be given the scope's cancellation token".to_owned(),
        );
    }
    let Some(body) = item_body(&code, PUBLICATION_TASK) else {
        return Some("the publication's task is absent".to_owned());
    };
    if !body.contains("cancellation.cancelled()") {
        return Some("the publication's task must stop when it is cancelled".to_owned());
    }
    None
}

/// Rule 4: the acknowledgement handoff is bounded.
///
/// The channel is bounded and its capacity is bounded, and the hand-over
/// never waits for room: the transport's service loop is never blocked by
/// the publication.
fn check_the_acknowledgement_handoff_is_bounded(task: &str) -> Option<String> {
    let code = code_view(task);
    let Some(body) = item_body(&code, "pub fn publication_acknowledgement_channel") else {
        return Some("the publication's acknowledgement handoff is absent".to_owned());
    };
    if body.contains("unbounded_channel") {
        return Some("the acknowledgement handoff must be bounded".to_owned());
    }
    if !body.contains("tokio::sync::mpsc::channel(") {
        return Some("the acknowledgement handoff must use a bounded channel".to_owned());
    }
    let Some(acknowledge) = item_body(&code, "pub fn acknowledge") else {
        return Some("the handoff's acknowledgement path is absent".to_owned());
    };
    if !acknowledge.contains("try_send(") {
        return Some("the handoff must never block the transport's service loop".to_owned());
    }
    if acknowledge.contains(".send(") {
        return Some("the handoff must never wait for room".to_owned());
    }
    None
}

/// Rule 5: the selection prefers an established session.
///
/// The preference is applied to the batch, and the targets it skips for want
/// of an established session are counted under their own reason.
fn check_the_selection_prefers_an_established_session(publication: &str) -> Option<String> {
    let code = code_view(publication);
    let Some(preference) = item_body(&code, "pub fn prefer_established_sessions") else {
        return Some("the publication's established-session preference is absent".to_owned());
    };
    if !preference.contains("established.contains(") {
        return Some("the preference must consult the established peers".to_owned());
    }
    let Some(batch) = item_body(&code, "async fn publish_once_inner") else {
        return Some("the publication's attempt is absent".to_owned());
    };
    if !batch.contains("prefer_established_sessions(") {
        return Some("the publication's batch must apply the preference".to_owned());
    }
    if !batch.contains("record_skipped_no_session(") {
        return Some(
            "the targets skipped for want of an established session must be counted".to_owned(),
        );
    }
    None
}

/// Rule 6: the publication's delivery timeout is bounded and owned by the
/// publication.
///
/// The timeout wraps the delivery and it is the publication's **own**
/// constant, so a hung handshake holds neither the transport's loop nor the
/// publication's task.
fn check_the_delivery_timeout_is_bounded_and_owned(publication: &str) -> Option<String> {
    let code = code_view(publication);
    let Some(position) = find_definition(&code, "pub const PUBLICATION_DELIVERY_TIMEOUT_MS") else {
        return Some("the publication's delivery timeout is absent".to_owned());
    };
    let declaration = code[position..].lines().next().unwrap_or_default();
    // The bound is the constant's **value**, not merely its spelling: a
    // timeout that reads as a literal is a timeout somebody can read.
    let Some((_, value)) = declaration.split_once('=') else {
        return Some("the publication's delivery timeout must be a bounded literal".to_owned());
    };
    let literal = value.trim().trim_end_matches(';').replace('_', "");
    let Ok(bound) = literal.parse::<u64>() else {
        return Some("the publication's delivery timeout must be a bounded literal".to_owned());
    };
    if bound == 0 || bound > DELIVERY_TIMEOUT_CEILING_MS {
        return Some(format!(
            "the publication's delivery timeout must be bounded, found {bound}"
        ));
    }
    let Some(body) = item_body(&code, "pub async fn publish_router_info") else {
        return Some("the publication's delivery is absent".to_owned());
    };
    if !body.contains("tokio::time::timeout(") {
        return Some("the publication's delivery must be bounded by its own timeout".to_owned());
    }
    if !body.contains("PUBLICATION_DELIVERY_TIMEOUT_MS") {
        return Some("the delivery timeout must be the publication's own".to_owned());
    }
    None
}

/// Rule 7: the transport's service loop carries no publication timer.
///
/// The publication's period tick and the ladder's tick belong to the
/// publication's task, so the transport's loop gains no timer.
fn check_the_loop_carries_no_publication_timer(composition: &str) -> Option<String> {
    let code = code_view(composition);
    let Some(remainder) = code_without_item(&code, PUBLICATION_TASK) else {
        return Some("the publication's task is absent".to_owned());
    };
    for needle in ["publication_tick", "publication_ladder_tick"] {
        if remainder.contains(needle) {
            return Some(format!(
                "the transport's service loop carries a publication timer: {needle}"
            ));
        }
    }
    let Some(body) = item_body(&code, PUBLICATION_TASK) else {
        return Some("the publication's task is absent".to_owned());
    };
    if !body.contains("publication_tick") || !body.contains("publication_ladder_tick") {
        return Some("the publication's ticks must live in the publication's task".to_owned());
    }
    None
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

#[test]
fn the_loop_never_awaits_the_publication() {
    let reason = check_the_loop_never_awaits_the_publication(&source_of(COMPOSITION_MODULE));
    assert!(
        reason.is_none(),
        "the transport's loop must never await the publication: {reason:?}"
    );
}

#[test]
fn the_publications_state_is_not_owned_by_the_loop() {
    let reason =
        check_the_publications_state_is_not_owned_by_the_loop(&source_of(COMPOSITION_MODULE));
    assert!(
        reason.is_none(),
        "the publication's state must not be owned by the loop: {reason:?}"
    );
}

#[test]
fn the_task_is_spawned_with_ownership_and_cancellation() {
    let reason =
        check_the_task_is_spawned_with_ownership_and_cancellation(&source_of(COMPOSITION_MODULE));
    assert!(
        reason.is_none(),
        "the task must be owned and cancellable: {reason:?}"
    );
}

#[test]
fn the_acknowledgement_handoff_is_bounded() {
    let reason = check_the_acknowledgement_handoff_is_bounded(&source_of(TASK_MODULE));
    assert!(
        reason.is_none(),
        "the acknowledgement handoff must be bounded: {reason:?}"
    );
}

#[test]
fn the_selection_prefers_an_established_session() {
    let reason = check_the_selection_prefers_an_established_session(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the selection must prefer an established session: {reason:?}"
    );
}

#[test]
fn the_delivery_timeout_is_bounded_and_owned() {
    let reason = check_the_delivery_timeout_is_bounded_and_owned(&source_of(PUBLICATION_MODULE));
    assert!(
        reason.is_none(),
        "the delivery timeout must be bounded and the publication's own: {reason:?}"
    );
}

#[test]
fn the_loop_carries_no_publication_timer() {
    let reason = check_the_loop_carries_no_publication_timer(&source_of(COMPOSITION_MODULE));
    assert!(
        reason.is_none(),
        "the transport's loop must carry no publication timer: {reason:?}"
    );
}

// ----------------------------------------------------------------------
// The inverted probes: each rule must report its own violation.
// ----------------------------------------------------------------------

/// The inverted probe for rule 1: a composition that awaits the publication
/// outside the publication's task must be reported.
#[test]
fn the_guard_reports_a_loop_that_awaits_the_publication() {
    let violating = "\
async fn run_router_info_publication_task() {
    let _ = 1;
}
fn compose() {
    let _ = crate::router_info_publication::refresh_router_info();
}
";
    let reason = check_the_loop_never_awaits_the_publication(violating);
    assert!(
        reason.is_some(),
        "a loop that awaits the publication must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 1, the other side: a composition that
/// correlates the acknowledgement outside the task must be reported.
#[test]
fn the_guard_reports_a_loop_that_correlates_the_acknowledgement() {
    let violating = "\
async fn run_router_info_publication_task() {
    let _ = 1;
}
fn compose() {
    let _ = publication_owner.confirm(1);
}
";
    let reason = check_the_loop_never_awaits_the_publication(violating);
    assert!(
        reason.is_some(),
        "a loop that correlates the acknowledgement must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 2: a composition that binds the publication's
/// owner in the loop must be reported.
#[test]
fn the_guard_reports_a_loop_that_binds_the_publications_owner() {
    let violating = "\
async fn run_router_info_publication_task() {
    let _ = 1;
}
fn compose() {
    let mut publication_owner = build();
    let _ = publication_owner;
    let _ = RouterInfoPublicationOwner::new(hash, period);
}
";
    let reason = check_the_publications_state_is_not_owned_by_the_loop(violating);
    assert!(
        reason.is_some(),
        "a loop that binds the publication's owner must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 2, the other side: a composition that builds
/// the owner twice must be reported.
#[test]
fn the_guard_reports_a_composition_that_builds_the_owner_twice() {
    let violating = "\
async fn run_router_info_publication_task() {
    let _ = 1;
}
fn compose() {
    let _ = RouterInfoPublicationOwner::new(hash, period);
    let _ = RouterInfoPublicationOwner::new(hash, period);
    let _ = run_router_info_publication_task(task, cancellation);
}
";
    let reason = check_the_publications_state_is_not_owned_by_the_loop(violating);
    assert!(
        reason.is_some(),
        "a composition that builds the owner twice must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 3: a composition that spawns the publication
/// on an ownerless task must be reported.
#[test]
fn the_guard_reports_an_ownerless_publication_task() {
    let violating = "\
async fn run_router_info_publication_task() {
    let _ = cancellation.cancelled();
}
fn compose() {
    tokio::spawn(run_router_info_publication_task(task, cancellation));
}
";
    let reason = check_the_task_is_spawned_with_ownership_and_cancellation(violating);
    assert!(
        reason.is_some(),
        "an ownerless publication task must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 3, the other side: a task that never observes
/// its cancellation must be reported.
#[test]
fn the_guard_reports_a_task_that_never_stops() {
    let violating = "\
async fn run_router_info_publication_task() {
    let _ = 1;
}
fn compose() {
    children.spawn(move |cancellation| async move {
        run_router_info_publication_task(task, cancellation).await
    });
}
";
    let reason = check_the_task_is_spawned_with_ownership_and_cancellation(violating);
    assert!(
        reason.is_some(),
        "a task that never observes its cancellation must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 4: an unbounded acknowledgement handoff must
/// be reported.
#[test]
fn the_guard_reports_an_unbounded_acknowledgement_handoff() {
    let violating = "\
pub fn publication_acknowledgement_channel(capacity: usize) -> (Sender<u32>, Receiver<u32>) {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    (sender, receiver)
}
";
    let reason = check_the_acknowledgement_handoff_is_bounded(violating);
    assert!(
        reason.is_some(),
        "an unbounded acknowledgement handoff must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 4, the other side: a hand-over that waits for
/// room must be reported.
#[test]
fn the_guard_reports_a_handoff_that_waits_for_room() {
    let violating = "\
pub fn publication_acknowledgement_channel(capacity: usize) -> (Sender<u32>, Receiver<u32>) {
    let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
    (sender, receiver)
}

pub fn acknowledge(&self, message_id: u32) -> bool {
    let _ = self.sender.send(message_id);
    true
}
";
    let reason = check_the_acknowledgement_handoff_is_bounded(violating);
    assert!(
        reason.is_some(),
        "a hand-over that waits for room must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 5: a selection that never consults the
/// established peers must be reported.
#[test]
fn the_guard_reports_a_selection_that_ignores_the_established_peers() {
    let violating = "\
pub fn prefer_established_sessions(
    targets: &[RouterHash],
    established: &[Hash],
) -> RouterInfoPublicationPreference {
    let _ = established;
    RouterInfoPublicationPreference { targets: Vec::new(), skipped: 0 }
}
";
    let reason = check_the_selection_prefers_an_established_session(violating);
    assert!(
        reason.is_some(),
        "a selection that ignores the established peers must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 5, the other side: a batch that skips the
/// targets without counting them must be reported.
#[test]
fn the_guard_reports_a_batch_that_does_not_count_the_skips() {
    let violating = "\
pub fn prefer_established_sessions(
    targets: &[RouterHash],
    established: &[Hash],
) -> RouterInfoPublicationPreference {
    let _ = established.contains(targets[0].as_hash());
    RouterInfoPublicationPreference { targets: Vec::new(), skipped: 0 }
}

async fn publish_once_inner() -> u8 {
    let _ = prefer_established_sessions(&targets, &established);
    0
}
";
    let reason = check_the_selection_prefers_an_established_session(violating);
    assert!(
        reason.is_some(),
        "a batch that does not count the skipped targets must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 6: a delivery with no timeout of its own must
/// be reported.
#[test]
fn the_guard_reports_a_delivery_with_no_timeout() {
    let violating = "\
pub const PUBLICATION_DELIVERY_TIMEOUT_MS: u64 = 90_000;

pub async fn publish_router_info(coordinator: &mut u8) -> u8 {
    let _ = coordinator;
    0
}
";
    let reason = check_the_delivery_timeout_is_bounded_and_owned(violating);
    assert!(
        reason.is_some(),
        "a delivery with no timeout of its own must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 6, the other side: an unbounded timeout must
/// be reported.
#[test]
fn the_guard_reports_an_unbounded_delivery_timeout() {
    let violating = "\
pub const PUBLICATION_DELIVERY_TIMEOUT_MS: u64 = u64::MAX;

pub async fn publish_router_info(coordinator: &mut u8) -> u8 {
    let _ = tokio::time::timeout(Duration::from_millis(PUBLICATION_DELIVERY_TIMEOUT_MS), async { 1 })
        .await;
    let _ = coordinator;
    0
}
";
    let reason = check_the_delivery_timeout_is_bounded_and_owned(violating);
    assert!(
        reason.is_some(),
        "an unbounded delivery timeout must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 6, the third side: a timeout constant that
/// reads as an unbounded value must be reported.
#[test]
fn the_guard_reports_an_unbounded_delivery_timeout_constant() {
    let violating = "\
pub const PUBLICATION_DELIVERY_TIMEOUT_MS: u64 = 90_000_000;

pub async fn publish_router_info(coordinator: &mut u8) -> u8 {
    let _ = tokio::time::timeout(Duration::from_millis(PUBLICATION_DELIVERY_TIMEOUT_MS), async { 1 })
        .await;
    let _ = coordinator;
    0
}
";
    let reason = check_the_delivery_timeout_is_bounded_and_owned(violating);
    assert!(
        reason.is_some(),
        "an unbounded delivery timeout constant must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 7: a transport's loop that carries a
/// publication timer must be reported.
#[test]
fn the_guard_reports_a_loop_that_carries_a_publication_timer() {
    let violating = "\
async fn run_router_info_publication_task() {
    let _ = 1;
}
fn compose() {
    let mut publication_tick = tokio::time::interval(period);
    let _ = publication_tick.tick();
}
";
    let reason = check_the_loop_carries_no_publication_timer(violating);
    assert!(
        reason.is_some(),
        "a loop that carries a publication timer must be reported: {reason:?}"
    );
}
