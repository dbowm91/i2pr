//! Plan 505 static boundary guard for the exploratory build driver.
//!
//! Plan 505's invariants are not only behavioural; several of them are
//! *structural*, and a behavioural test cannot see a structural drift:
//!
//! 1. **The live composition builds the driver, and only on the operator's
//!    say-so.** The posture is off by default, and the driver exists only
//!    inside the branch that reads it.
//! 2. **The product owns the driver.** The driver is a value the tick
//!    advances, not a task the driver spawns.
//! 3. **The existing tick drives the driver**, beside Plan 504's liveness
//!    driver. No new task, no new timer, no new listener.
//! 4. **The peer is chosen through Plan 491's version-gated selection**, so a
//!    peer below the floor is refused and counted, never silently skipped.
//! 5. **The build is submitted through the one exploratory build path**, so
//!    the first build and the liveness rebuild cannot drift apart.
//! 6. **The outcome mapping is total**, and a *local* condition is never
//!    written into a peer's profile.
//! 7. **The driver never touches a destination tunnel.**
//! 8. **The peer-profile store is fed in production**, from the router's own
//!    validated peers.
//! 9. **Terminal outcomes reach the driver on both paths** — the expiry
//!    sweep and the inbound build reply.
//! 10. **The counters are published** through the inspection seam, and the
//!     seam's reader returns a *gap* rather than a fabricated zero.
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

/// The driver module the guard is about.
const DRIVER_MODULE: &str = "crates/i2pr-daemon/src/exploratory_driver.rs";

/// The coordinator module that owns the outcome enum.
const BUILD_MODULE: &str = "crates/i2pr-daemon/src/exploratory_build.rs";

/// The product module that owns the driver at runtime.
const PRODUCT_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";

/// The composition root.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";

/// The configuration module.
const CONFIG_MODULE: &str = "crates/i2pr-daemon/src/config.rs";

/// The inspection seam.
const INSPECTION_MODULE: &str = "crates/i2pr-daemon/src/i2pcontrol_inspection.rs";

/// The two spellings of the peer-profile store's learn call.
///
/// The landed composition learns a **batch** of validated peers, so it calls
/// `learn_all(`; `learn(` is the single-peer spelling of the same store
/// write. The two are the same operation and not two paths — the batch form
/// records one receipt for the batch instead of one per peer — so the rule
/// accepts either, each matched with its own open parenthesis.
const LEARN_CALLS: &[&str] = &["learn(", "learn_all("];

/// The constructs a destination-scoped driver would name.
///
/// The driver is exploratory-only. A destination build call, a destination
/// request type or a destination identity in the driver module would be the
/// destination path the plan puts out of scope.
const DESTINATION_TOKENS: &[&str] = &[
    "submit_destination",
    "submit_path(",
    "DestinationBuildRequest",
    "destination_id",
];

/// The constructs that would give the driver its own task or its own timer.
const DRIVER_TASK_TOKENS: &[&str] = &["tokio::spawn", "tokio::time::interval"];

/// The ungated selection call on the peer-profile driver.
///
/// The discriminator is the **leading dot**: the version-gated call site reads
/// `profiles.select_tunnel_build_peers(`, and the text `.select(` does not
/// occur inside it — the character before `select` is the `_` of
/// `select_tunnel_build_peers`, not a `.`.
const UNGATED_SELECTION_CALL: &str = ".select(";

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

/// Returns the body of the struct whose **definition** is anchored at
/// `struct <name>`.
///
/// A struct is not a function, so a whole-file token search could not tell
/// which of the two structs owns a field. The body is the text between the
/// struct's own braces; an absent struct yields `None`, so every caller
/// fails closed.
fn struct_body<'a>(code: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("struct {name}");
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

/// Returns the names of the variants of the enum whose **definition** is
/// anchored at `enum <name>`.
///
/// The variants are enumerated from the **source**, never from a hard-coded
/// list: a rule that asserted a count would keep passing when the enum grew
/// a variant the mapping does not handle.
fn enum_variants(code: &str, name: &str) -> Option<Vec<String>> {
    let needle = format!("enum {name}");
    let start = find_definition(code, &needle)?;
    let (open, close) = brace_balanced_block(code, start + needle.len())?;
    let body = &code[open + 1..close - 1];
    let bytes = body.as_bytes();
    let mut cuts: Vec<usize> = Vec::new();
    let mut depth = 0usize;
    let mut cursor = 0usize;
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
            b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => cuts.push(cursor),
            _ => {}
        }
        cursor += 1;
    }
    let mut variants: Vec<String> = Vec::new();
    let mut chunk_start = 0usize;
    for cut in cuts.into_iter().chain(std::iter::once(body.len())) {
        let chunk = body[chunk_start..cut].trim();
        chunk_start = cut.saturating_add(1);
        let name_end = chunk
            .find(|character: char| !(character.is_alphanumeric() || character == '_'))
            .unwrap_or(chunk.len());
        let variant = &chunk[..name_end];
        if variant.is_empty() {
            continue;
        }
        variants.push(variant.to_owned());
    }
    Some(variants)
}

/// Returns the items of the `#[derive(...)]` attribute that belongs to the
/// definition of `struct <name>`.
fn derive_list(code: &str, name: &str) -> Option<Vec<String>> {
    let start = find_definition(code, &format!("struct {name}"))?;
    let before = &code[..start];
    let attribute = before.rfind("#[derive(")?;
    let open = attribute + "#[derive(".len();
    let close = code[open..].find(")]")? + open;
    let list = &code[open..close];
    Some(
        list.split(',')
            .map(|item| item.trim().to_owned())
            .filter(|item| !item.is_empty())
            .collect(),
    )
}

/// Replaces the `nth` (zero-based) occurrence of `needle` in `source`.
///
/// The rule that needs it must be able to weaken **one** of two identical
/// call sites and leave the other wired, so that each half of the rule is
/// proved separately.
fn replace_occurrence(source: &str, needle: &str, replacement: &str, nth: usize) -> String {
    let mut seen = 0usize;
    let mut offset = 0usize;
    while let Some(position) = source[offset..].find(needle) {
        let start = offset + position;
        if seen == nth {
            let mut out = String::with_capacity(source.len());
            out.push_str(&source[..start]);
            out.push_str(replacement);
            out.push_str(&source[start + needle.len()..]);
            return out;
        }
        seen += 1;
        offset = start + needle.len();
    }
    source.to_owned()
}

/// Returns whether the body of `fn <name>` contains `token`.
///
/// The probes use it to show that a mutation removed the call from **one**
/// function and left the other function's call in place, so each half of a
/// two-path rule is proved on its own.
fn body_contains(source: &str, name: &str, token: &str) -> bool {
    let code = code_view(source);
    function_body(&code, name)
        .map(|body| body.contains(token))
        .unwrap_or(false)
}

/// Rule 1, 3, 4, 5, 8, 9, 10 (structural): the body of `fn <name>` must
/// contain `token`.
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

/// The body of `fn <name>` must contain at least one of the supplied call
/// tokens.
///
/// The rule accepts either spelling of the one store-write seam, because the
/// two spellings are the same seam and not a second path.
fn check_body_contains_any(source: &str, name: &str, tokens: &[&str]) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, name) else {
        return Some(format!("`fn {name}` must exist in the production surface"));
    };
    if tokens.iter().any(|token| body.contains(token)) {
        None
    } else {
        Some(format!(
            "`fn {name}` must contain one of {}",
            tokens.join(", ")
        ))
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

/// The production code view must contain `token` exactly once.
///
/// A second occurrence is a second pipeline, which is the private parallel
/// path the rule forbids.
fn check_code_occurs_once(source: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let occurrences = code.matches(token).count();
    if occurrences == 1 {
        None
    } else {
        Some(format!(
            "the production code view must contain `{token}` exactly once, found {occurrences}"
        ))
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

/// The brace-balanced block that opens after `anchor` must contain `token`.
///
/// This is the "inside a branch" rule: the block that opens after the
/// posture's `if` is the branch's own body, so a construction that sits
/// outside the branch cannot satisfy it.
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

/// The brace-balanced block that opens after `anchor` must not contain
/// `token`.
fn check_block_avoids(source: &str, anchor: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let Some(block) = block_after(&code, anchor) else {
        return Some(format!(
            "the block `{anchor}` must exist in the production surface"
        ));
    };
    if block.contains(token) {
        Some(format!("the block `{anchor}` must not contain `{token}`"))
    } else {
        None
    }
}

/// Rule 6: the outcome mapping is total over `BuildCoordinatorOutcome`.
///
/// The variants are enumerated from the coordinator's own source, so a new
/// variant that the mapping does not handle trips the rule without the rule
/// ever restating the enum.
fn check_outcome_mapping_is_total(build_source: &str, driver_source: &str) -> Option<String> {
    let build = code_view(build_source);
    let Some(variants) = enum_variants(&build, "BuildCoordinatorOutcome") else {
        return Some("`BuildCoordinatorOutcome` must exist in the coordinator module".to_owned());
    };
    if variants.is_empty() {
        return Some("`BuildCoordinatorOutcome` must declare at least one variant".to_owned());
    }
    let driver = code_view(driver_source);
    let Some(body) = function_body(&driver, "observation_for_outcome") else {
        return Some("`fn observation_for_outcome` must exist in the driver module".to_owned());
    };
    for variant in &variants {
        let token = format!("BuildCoordinatorOutcome::{variant}");
        if !body.contains(&token) {
            return Some(format!(
                "the outcome mapping must handle `{token}` (the mapping is total)"
            ));
        }
    }
    None
}

/// Rule 6: the recorder must not write a peer profile for a local
/// condition.
///
/// The local arm is the block that opens after the local match, so the rule
/// reads the arm itself: it must return before the store write, and it must
/// not contain the write at all.
fn check_local_condition_writes_no_profile(driver_source: &str) -> Option<String> {
    let driver = code_view(driver_source);
    let Some(body) = function_body(&driver, "record_observation") else {
        return Some("`fn record_observation` must exist in the driver module".to_owned());
    };
    if !body.contains("PeerObservation::Local") {
        return Some("`fn record_observation` must recognise the local condition".to_owned());
    }
    let anchor = "if matches!(observation, PeerObservation::Local)";
    if block_after(&driver, anchor).is_none() {
        return Some(format!(
            "the local condition must be matched before the store write (`{anchor}`)"
        ));
    }
    if let Some(finding) = check_block_contains(driver_source, anchor, "return;") {
        return Some(format!(
            "the local condition must return before any peer-profile write ({finding})"
        ));
    }
    for token in ["observe_", "profiles.with("] {
        if let Some(finding) = check_block_avoids(driver_source, anchor, token) {
            return Some(format!(
                "the local condition must not write the peer profile ({finding})"
            ));
        }
    }
    None
}

/// Rule 1: the operator posture is off by default.
///
/// The raw section derives `Default`, its only field is a `bool` with a serde
/// default, and the normalization carries the raw value through unchanged —
/// so a router that has never set the posture owns no driver at all.
fn check_posture_is_off_by_default(config_source: &str) -> Option<String> {
    let config = code_view(config_source);
    let Some(body) = struct_body(&config, "RawExploratoryConfig") else {
        return Some("`struct RawExploratoryConfig` must exist in the configuration".to_owned());
    };
    if !body.contains("builds: bool") {
        return Some("`struct RawExploratoryConfig` must hold a `builds: bool` field".to_owned());
    }
    if !body.contains("#[serde(default)]") {
        return Some("the raw posture's field must carry `#[serde(default)]`".to_owned());
    }
    let Some(derives) = derive_list(&config, "RawExploratoryConfig") else {
        return Some("`struct RawExploratoryConfig` must derive `Default`".to_owned());
    };
    if !derives.iter().any(|item| item == "Default") {
        return Some("`struct RawExploratoryConfig` must derive `Default`".to_owned());
    }
    let Some(normalized) = struct_body(&config, "ExploratoryConfig") else {
        return Some("`struct ExploratoryConfig` must exist in the configuration".to_owned());
    };
    if !normalized.contains("builds: bool") {
        return Some("`struct ExploratoryConfig` must hold a `builds: bool` field".to_owned());
    }
    let Some(body) = function_body(&config, "normalize_exploratory") else {
        return Some("`fn normalize_exploratory` must exist in the configuration".to_owned());
    };
    if !body.contains("builds: raw.builds") {
        return Some(
            "the normalized posture must carry the raw posture through unchanged".to_owned(),
        );
    }
    None
}

/// Rule 1: the live composition builds the driver inside the posture's
/// branch.
///
/// The posture is read from the configuration and the construction must sit
/// inside the branch that tests it, so a driver that is built unconditionally
/// trips the rule.
fn check_composition_builds_the_driver_gated(composition_source: &str) -> Option<String> {
    let composition = code_view(composition_source);
    let Some(body) = function_body(&composition, "register_ssu2_service") else {
        return Some("`fn register_ssu2_service` must exist in the composition".to_owned());
    };
    if !body.contains("config.exploratory.builds") {
        return Some("the composition must read the posture from the configuration".to_owned());
    }
    if !body.contains("if exploratory_builds") {
        return Some("the composition must branch on the operator posture".to_owned());
    }
    // Plan 534 extended this guard: the driver may now be constructed with
    // the local relay observations beside it. The rule's intent is unchanged
    // -- the driver is still constructed **inside** the posture's branch,
    // and the posture is still the only thing that decides it.
    let constructed = [
        "ExploratoryDriver::new(",
        "ExploratoryDriver::with_relay_sessions(",
    ]
    .iter()
    .any(|token| {
        check_block_contains(composition_source, "if exploratory_builds", token).is_none()
    });
    if !constructed {
        return Some("the driver must be constructed inside the posture's branch".to_owned());
    }
    None
}

/// Rule 11 (Plan 534): a named target passes the same version gate.
///
/// The named target is the peer this router reached through a relay. It is
/// gated by the **same** function the selection gates a drawn candidate by,
/// and its version comes from the peer's **validated** record. A named
/// target below the floor is refused, and the refusal is counted under the
/// driver's existing version-refusal counters, so a named target and a drawn
/// candidate are counted for the same reason.
fn check_the_named_target_passes_the_same_gate(driver_source: &str) -> Option<String> {
    let driver = code_view(driver_source);
    let Some(body) = function_body(&driver, "relay_target_eligibility") else {
        return Some("the named target's gate must exist in the driver module".to_owned());
    };
    if !body.contains("tunnel_peer_eligibility(") {
        return Some("the named target must be gated by the version floor".to_owned());
    }
    if !body.contains("published_version_of(") {
        return Some("the named target's version must come from the validated record".to_owned());
    }
    if !body.contains("peers()") {
        return Some("the named target's record must come from the validated store".to_owned());
    }
    None
}

/// Rule 4: the driver selects through the version-gated selection.
fn check_driver_selects_through_the_gate(driver_source: &str) -> Option<String> {
    let driver = code_view(driver_source);
    let Some(body) = function_body(&driver, "select_peer") else {
        return Some("`fn select_peer` must exist in the driver module".to_owned());
    };
    if !body.contains("select_tunnel_build_peers(") {
        return Some(
            "`fn select_peer` must select through `select_tunnel_build_peers(`".to_owned(),
        );
    }
    if driver.contains(UNGATED_SELECTION_CALL) {
        return Some(format!(
            "the driver must not use the ungated selection `{UNGATED_SELECTION_CALL}`"
        ));
    }
    if let Some(finding) = check_code_occurs_once(driver_source, "select_tunnel_build_peers(") {
        return Some(finding);
    }
    if let Some(finding) = check_body_avoids(driver_source, "drive", ".peers(") {
        return Some(format!(
            "the driver must not choose a peer out of the store's population itself ({finding})"
        ));
    }
    None
}

/// Rule 5: the driver submits through the one exploratory build path.
fn check_driver_submits_through_the_single_path(driver_source: &str) -> Option<String> {
    let driver = code_view(driver_source);
    let Some(body) = function_body(&driver, "drive") else {
        return Some("`fn drive` must exist in the driver module".to_owned());
    };
    if !body.contains("rebuild_exploratory(") {
        return Some("`fn drive` must submit through `rebuild_exploratory(`".to_owned());
    }
    for token in [".submit(", "submit_path("] {
        if driver.contains(token) {
            return Some(format!(
                "the driver must not open a second build path (`{token}`)"
            ));
        }
    }
    if let Some(finding) = check_code_occurs_once(driver_source, "rebuild_exploratory(") {
        return Some(finding);
    }
    None
}

/// Rule 3: the driver adds no task and no timer.
fn check_driver_owns_no_task(driver_source: &str) -> Option<String> {
    let driver = code_view(driver_source);
    for token in DRIVER_TASK_TOKENS {
        if driver.contains(token) {
            return Some(format!(
                "the driver must own no task and no timer (`{token}`)"
            ));
        }
    }
    None
}

/// Rule 7: the driver never touches a destination tunnel.
fn check_driver_never_touches_a_destination(driver_source: &str) -> Option<String> {
    let driver = code_view(driver_source);
    for token in DESTINATION_TOKENS {
        if driver.contains(token) {
            return Some(format!(
                "the driver must never touch a destination tunnel (`{token}`)"
            ));
        }
    }
    None
}

/// Rule 8: the composition learns the router's real peers into the profile
/// store.
fn check_profile_store_is_fed(composition_source: &str) -> Option<String> {
    let composition = code_view(composition_source);
    if let Some(finding) =
        check_body_contains_any(composition_source, "learn_bootstrap_peers", LEARN_CALLS)
    {
        return Some(finding);
    }
    let Some(start) = function_body(&composition, "start") else {
        return Some("`fn start` must exist in the composition".to_owned());
    };
    if !start.contains("learn_bootstrap_peers(") {
        return Some(
            "the composition must call `learn_bootstrap_peers(` when it starts".to_owned(),
        );
    }
    None
}

/// Rule 9: the terminal outcomes reach the driver on **both** paths.
///
/// The expiry sweep and the inbound build reply are two different functions,
/// so each is checked separately: a rule that accepted either would pass
/// when only one path were wired.
fn check_outcomes_reach_the_driver_on_both_paths(product_source: &str) -> Option<String> {
    let product = code_view(product_source);
    for function in ["advance_destination_pools_at", "process_inbound"] {
        let Some(body) = function_body(&product, function) else {
            return Some(format!("`fn {function}` must exist in the product surface"));
        };
        if !body.contains("observe_exploratory_outcome(") {
            return Some(format!(
                "`fn {function}` must report the terminal outcome to the driver"
            ));
        }
    }
    let Some(body) = function_body(&product, "observe_exploratory_outcome") else {
        return Some(
            "`fn observe_exploratory_outcome` must exist in the product surface".to_owned(),
        );
    };
    if !body.contains("driver.observe_outcome(") {
        return Some(
            "`fn observe_exploratory_outcome` must hand the outcome to the driver".to_owned(),
        );
    }
    None
}

/// Rule 10: the counters are published through the inspection seam, and the
/// seam's reader returns a gap rather than a fabricated zero.
fn check_counters_are_published(product_source: &str, inspection_source: &str) -> Option<String> {
    let product = code_view(product_source);
    let Some(body) = function_body(&product, "publish_exploratory_driver_counters") else {
        return Some(
            "`fn publish_exploratory_driver_counters` must exist in the product surface".to_owned(),
        );
    };
    if !body.contains("driver.counters(") {
        return Some("the product must publish the driver's counters".to_owned());
    }
    let inspection = code_view(inspection_source);
    let Some(body) = function_body(&inspection, "publish_exploratory_driver_counters") else {
        return Some(
            "`fn publish_exploratory_driver_counters` must exist in the inspection seam".to_owned(),
        );
    };
    if !body.contains("exploratory_driver_live") {
        return Some("the inspection seam must publish the driver's counter cell".to_owned());
    }
    let Some(body) = function_body(&inspection, "exploratory_driver_counters_snapshot") else {
        return Some(
            "`fn exploratory_driver_counters_snapshot` must exist in the inspection seam"
                .to_owned(),
        );
    };
    if !body.contains("exploratory_driver_counters()?") {
        return Some(
            "the seam's reader must read the published slot, so an unpublished slot is a gap"
                .to_owned(),
        );
    }
    for token in ["unwrap_or_default(", "unwrap_or("] {
        if body.contains(token) {
            return Some(format!(
                "the seam's reader must return a gap, never a fabricated zero (`{token}`)"
            ));
        }
    }
    None
}

/// Rule 1: the live composition builds the driver, and the build is gated on
/// the operator posture.
///
/// The posture is off by default in the configuration, and the driver's
/// construction sits inside the branch that tests it.
#[test]
fn rule_1_the_composition_builds_the_driver_only_on_the_operator_posture() {
    let composition = source_of(COMPOSITION_MODULE);
    let config = source_of(CONFIG_MODULE);
    assert_eq!(
        check_composition_builds_the_driver_gated(&composition),
        None,
        "the live composition must build the driver inside the posture's branch"
    );
    assert_eq!(
        check_posture_is_off_by_default(&config),
        None,
        "the exploratory posture must be off by default"
    );
}

/// Rule 2: the product owns the driver.
///
/// `ProductInner` carries the driver and the counter cell; `ServiceProductSpec`
/// carries the driver the composition root hands over.
#[test]
fn rule_2_the_product_owns_the_driver() {
    let source = source_of(PRODUCT_MODULE);
    for structure in ["ProductInner", "ServiceProductSpec"] {
        assert_eq!(
            check_struct_contains(
                &source,
                structure,
                "exploratory_driver: Option<crate::exploratory_driver::ExploratoryDriver>,",
            ),
            None,
            "`struct {structure}` must carry the exploratory driver"
        );
    }
    assert_eq!(
        check_struct_contains(&source, "ProductInner", "exploratory_driver_counters_cell:",),
        None,
        "`struct ProductInner` must carry the driver's counter cell"
    );
}

/// Rule 3: the live tick drives the driver beside the liveness driver, and
/// the driver owns no task and no timer.
#[test]
fn rule_3_the_live_tick_drives_the_driver_and_the_driver_owns_no_task() {
    let product = source_of(PRODUCT_MODULE);
    let driver = source_of(DRIVER_MODULE);
    assert_eq!(
        check_body_contains(
            &product,
            "advance_destination_pools_at",
            "drive_tunnel_liveness(",
        ),
        None,
        "the live tick must drive the liveness scheduler"
    );
    assert_eq!(
        check_body_contains(
            &product,
            "advance_destination_pools_at",
            "drive_exploratory_builds(",
        ),
        None,
        "the live tick must drive the exploratory driver"
    );
    assert_eq!(
        check_body_contains(&product, "drive_exploratory_builds", "driver.drive("),
        None,
        "the exploratory step must drive the driver"
    );
    assert_eq!(
        check_driver_owns_no_task(&driver),
        None,
        "the driver must add no task and no timer"
    );
}

/// Rule 4: the driver selects through Plan 491's version-gated selection, and
/// never through an ungated path.
#[test]
fn rule_4_the_driver_selects_through_the_version_gated_selection() {
    let driver = source_of(DRIVER_MODULE);
    assert_eq!(
        check_driver_selects_through_the_gate(&driver),
        None,
        "the driver must select through `select_tunnel_build_peers(`"
    );
}

/// Rule 5: the driver submits through the one exploratory build path.
#[test]
fn rule_5_the_driver_submits_through_the_single_build_path() {
    let driver = source_of(DRIVER_MODULE);
    assert_eq!(
        check_driver_submits_through_the_single_path(&driver),
        None,
        "the driver must submit through `rebuild_exploratory(` and open no second path"
    );
}

/// Rule 6: the outcome mapping is total, and a local condition is never
/// written into a peer's profile.
#[test]
fn rule_6_the_outcome_mapping_is_total_and_a_local_condition_is_local() {
    let build = source_of(BUILD_MODULE);
    let driver = source_of(DRIVER_MODULE);
    assert_eq!(
        check_outcome_mapping_is_total(&build, &driver),
        None,
        "the outcome mapping must handle every `BuildCoordinatorOutcome` variant"
    );
    let code = code_view(&driver);
    let body = function_body(&code, "observation_for_outcome").expect("the mapping must exist");
    assert!(
        body.contains("PeerObservation::Local"),
        "the mapping must distinguish a local condition from a peer fault"
    );
    assert_eq!(
        check_local_condition_writes_no_profile(&driver),
        None,
        "a local condition must never write the peer's profile"
    );
}

/// Rule 7: the driver never touches a destination tunnel.
#[test]
fn rule_7_the_driver_never_touches_a_destination_tunnel() {
    let driver = source_of(DRIVER_MODULE);
    assert_eq!(
        check_driver_never_touches_a_destination(&driver),
        None,
        "the driver must never touch a destination tunnel"
    );
}

/// Rule 8: the composition feeds the peer-profile store from the router's own
/// validated peers.
#[test]
fn rule_8_the_peer_profile_store_is_fed_in_production() {
    let composition = source_of(COMPOSITION_MODULE);
    assert_eq!(
        check_profile_store_is_fed(&composition),
        None,
        "the composition must learn the router's real peers into the profile store"
    );
}

/// Rule 9: the terminal outcomes reach the driver on the expiry sweep **and**
/// on the inbound build reply.
#[test]
fn rule_9_the_terminal_outcomes_reach_the_driver_on_both_paths() {
    let product = source_of(PRODUCT_MODULE);
    assert_eq!(
        check_outcomes_reach_the_driver_on_both_paths(&product),
        None,
        "both the expiry sweep and the inbound build reply must report to the driver"
    );
}

/// Rule 10: the counters are published through the inspection seam, and the
/// seam's reader returns a gap rather than a fabricated zero.
#[test]
fn rule_10_the_counters_are_published_and_the_seam_reports_a_gap() {
    let product = source_of(PRODUCT_MODULE);
    let inspection = source_of(INSPECTION_MODULE);
    assert_eq!(
        check_counters_are_published(&product, &inspection),
        None,
        "the counters must be published through the inspection seam"
    );
}

/// Rule 11 (Plan 534): a named target passes the same version gate.
///
/// Plan 534 extended this guard with one rule. The peer this router reached
/// through a relay is the route's **subject** rather than one more
/// candidate, so the rule is that it is gated by the **same** function the
/// selection gates a drawn candidate by, and that its version comes from the
/// peer's validated record.
#[test]
fn rule_11_a_named_target_passes_the_same_version_gate() {
    let driver = source_of(DRIVER_MODULE);
    assert_eq!(
        check_the_named_target_passes_the_same_gate(&driver),
        None,
        "a named target must pass the same version gate"
    );
}

/// The inverted probes for the ten rules.
///
/// Each probe is the smallest edit that would reintroduce the defect the
/// rule exists to catch, applied to a **copy** of the landed source. The
/// `assert_ne!` before each check proves the mutation actually applied, so a
/// probe cannot pass because the edit silently did nothing.
#[test]
fn the_guard_fires_on_a_weakened_copy_of_the_landed_source() {
    let composition = source_of(COMPOSITION_MODULE);
    let config = source_of(CONFIG_MODULE);
    let product = source_of(PRODUCT_MODULE);
    let driver = source_of(DRIVER_MODULE);
    let build = source_of(BUILD_MODULE);
    let inspection = source_of(INSPECTION_MODULE);

    // Rule 1: the construction leaves the posture's branch.
    let weakened = composition.replace("if exploratory_builds {", "if true {");
    assert_ne!(
        weakened, composition,
        "the posture-branch mutation must have applied"
    );
    assert!(
        check_composition_builds_the_driver_gated(&weakened).is_some(),
        "a driver that is built outside the posture's branch must be reported"
    );

    // Rule 1: the posture is switched on by default.
    let weakened = config.replace("builds: raw.builds", "builds: true");
    assert_ne!(
        weakened, config,
        "the posture-default mutation must have applied"
    );
    assert!(
        check_posture_is_off_by_default(&weakened).is_some(),
        "a posture that defaults to on must be reported"
    );

    // Rule 2: the product stops carrying the driver.
    let weakened = replace_occurrence(
        &product,
        "\n    exploratory_driver: Option<crate::exploratory_driver::ExploratoryDriver>,",
        "",
        0,
    );
    assert_ne!(
        weakened, product,
        "the product-owner mutation must have applied"
    );
    assert!(
        check_struct_contains(
            &weakened,
            "ProductInner",
            "exploratory_driver: Option<crate::exploratory_driver::ExploratoryDriver>,",
        )
        .is_some(),
        "a product that does not own the driver must be reported"
    );

    // Rule 3: the tick stops driving the driver.
    let weakened = product.replace("self.drive_exploratory_builds(now_ms);", "let _ = ();");
    assert_ne!(weakened, product, "the tick mutation must have applied");
    assert!(
        check_body_contains(
            &weakened,
            "advance_destination_pools_at",
            "drive_exploratory_builds(",
        )
        .is_some(),
        "a tick that stops driving the driver must be reported"
    );

    // Rule 3: the driver grows its own task.
    let weakened = driver.replace(
        "self.now_ms = now_ms;",
        "self.now_ms = now_ms;\n        let _task = tokio::spawn(async {});",
    );
    assert_ne!(weakened, driver, "the task mutation must have applied");
    assert!(
        check_driver_owns_no_task(&weakened).is_some(),
        "a driver that spawns its own task must be reported"
    );

    // Rule 4: the selection falls back to the ungated path.
    let weakened = driver.replace(
        "profiles.select_tunnel_build_peers(1, seed)",
        "profiles.select(1, seed)",
    );
    assert_ne!(weakened, driver, "the selection mutation must have applied");
    assert!(
        check_driver_selects_through_the_gate(&weakened).is_some(),
        "an ungated selection must be reported"
    );

    // Rule 5: the driver opens a second, private build path.
    let weakened = driver.replace(
        "coordinator.rebuild_exploratory(",
        "coordinator.submit_path(",
    );
    assert_ne!(
        weakened, driver,
        "the build-path mutation must have applied"
    );
    assert!(
        check_driver_submits_through_the_single_path(&weakened).is_some(),
        "a second, private build path must be reported"
    );

    // Rule 6: the mapping drops a variant behind a catch-all arm.
    let weakened = driver.replace(
        "BuildCoordinatorOutcome::TimedOut { .. } => PeerObservation::Unanswered,",
        "_ => PeerObservation::Unanswered,",
    );
    assert_ne!(weakened, driver, "the totality mutation must have applied");
    assert!(
        check_outcome_mapping_is_total(&build, &weakened).is_some(),
        "a mapping that silently drops a variant must be reported"
    );

    // Rule 6: a local condition is blamed on the peer.
    let weakened = driver.replace(
        "| BuildCoordinatorOutcome::CoordinatorRejected { .. } => PeerObservation::Local,",
        "| BuildCoordinatorOutcome::CoordinatorRejected { .. } => PeerObservation::Misbehaving,",
    );
    assert_ne!(
        weakened, driver,
        "the local-disposition mutation must have applied"
    );
    let code = code_view(&weakened);
    let body = function_body(&code, "observation_for_outcome").expect("the mapping must exist");
    assert!(
        !body.contains("PeerObservation::Local"),
        "the local disposition must be gone after the mutation"
    );

    // Rule 6: the recorder writes the local condition into the peer's profile.
    let weakened = driver.replace(
        "self.counters.local_outcomes = self.counters.local_outcomes.saturating_add(1);\n            return;",
        "self.counters.local_outcomes = self.counters.local_outcomes.saturating_add(1);",
    );
    assert_ne!(
        weakened, driver,
        "the local-write mutation must have applied"
    );
    assert!(
        check_local_condition_writes_no_profile(&weakened).is_some(),
        "a local condition that writes the peer's profile must be reported"
    );

    // Rule 7: the driver reaches for a destination build.
    let weakened = driver.replace(
        "let rebuild_peer = ExploratoryRebuildPeer {",
        "let _destination = coordinator.submit_destination_via_outbound_tunnel();\n        let rebuild_peer = ExploratoryRebuildPeer {",
    );
    assert_ne!(
        weakened, driver,
        "the destination mutation must have applied"
    );
    assert!(
        check_driver_never_touches_a_destination(&weakened).is_some(),
        "a driver that reaches for a destination build must be reported"
    );

    // Rule 8: the composition stops learning the router's peers.
    let weakened = composition.replace("profiles.learn_all(records);", "let _ = records;");
    assert_ne!(
        weakened, composition,
        "the learn mutation must have applied"
    );
    assert!(
        check_profile_store_is_fed(&weakened).is_some(),
        "a composition that never learns its peers must be reported"
    );

    // Rule 8: the composition never calls the learner.
    let weakened = composition.replace(
        "self.learn_bootstrap_peers(bootstrap, now_ms);",
        "let _ = (bootstrap, now_ms);",
    );
    assert_ne!(
        weakened, composition,
        "the learn-call mutation must have applied"
    );
    assert!(
        check_profile_store_is_fed(&weakened).is_some(),
        "a composition that never calls `learn_bootstrap_peers(` must be reported"
    );

    // Rule 9: the expiry sweep stops reporting to the driver.
    let weakened = replace_occurrence(
        &product,
        "self.observe_exploratory_outcome(&outcome);",
        "let _ = &outcome;",
        0,
    );
    assert_ne!(
        weakened, product,
        "the expiry-sweep mutation must have applied"
    );
    assert!(
        !body_contains(
            &weakened,
            "advance_destination_pools_at",
            "observe_exploratory_outcome("
        ),
        "the expiry sweep must have lost its observation call"
    );
    assert!(
        body_contains(&weakened, "process_inbound", "observe_exploratory_outcome("),
        "the inbound build reply must stay wired"
    );
    assert!(
        check_outcomes_reach_the_driver_on_both_paths(&weakened).is_some(),
        "an expiry sweep that stops reporting to the driver must be reported"
    );

    // Rule 9: the inbound build reply stops reporting to the driver.
    let weakened = replace_occurrence(
        &product,
        "self.observe_exploratory_outcome(&outcome);",
        "let _ = &outcome;",
        1,
    );
    assert_ne!(
        weakened, product,
        "the inbound-reply mutation must have applied"
    );
    assert!(
        body_contains(
            &weakened,
            "advance_destination_pools_at",
            "observe_exploratory_outcome("
        ),
        "the expiry sweep must stay wired"
    );
    assert!(
        !body_contains(&weakened, "process_inbound", "observe_exploratory_outcome("),
        "the inbound build reply must have lost its observation call"
    );
    assert!(
        check_outcomes_reach_the_driver_on_both_paths(&weakened).is_some(),
        "an inbound build reply that stops reporting to the driver must be reported"
    );

    // Rule 9: the product's hand-off stops reaching the driver.
    let weakened = product.replace("driver.observe_outcome(outcome);", "let _ = outcome;");
    assert_ne!(weakened, product, "the hand-off mutation must have applied");
    assert!(
        check_outcomes_reach_the_driver_on_both_paths(&weakened).is_some(),
        "a product that never hands the outcome to the driver must be reported"
    );

    // Rule 10: the product stops publishing the counters.
    let weakened = product.replace(
        "let counters = driver.counters();",
        "let counters = Default::default();",
    );
    assert_ne!(weakened, product, "the publish mutation must have applied");
    assert!(
        check_counters_are_published(&weakened, &inspection).is_some(),
        "a product that does not publish the counters must be reported"
    );

    // Rule 10: the seam's reader fabricates a zero instead of a gap.
    let weakened = inspection.replace(
        "let cell = self.exploratory_driver_counters()?;",
        "let cell = self.exploratory_driver_counters().unwrap_or_default();",
    );
    assert_ne!(weakened, inspection, "the gap mutation must have applied");
    assert!(
        check_counters_are_published(&product, &weakened).is_some(),
        "a seam that fabricates a zero must be reported"
    );

    // Rule 11: the named target stops passing the gate.
    let weakened = driver.replace(
        "match tunnel_peer_eligibility(version) {",
        "match VersionEligibility::Eligible {",
    );
    assert_ne!(
        weakened, driver,
        "the named-gate mutation must have applied"
    );
    assert!(
        check_the_named_target_passes_the_same_gate(&weakened).is_some(),
        "a named target that skips the version gate must be reported"
    );

    // Rule 11: the named target's gate is absent altogether.
    let weakened = driver.replace("fn relay_target_eligibility(", "fn unused_relay_target(");
    assert_ne!(
        weakened, driver,
        "the missing-gate mutation must have applied"
    );
    assert!(
        check_the_named_target_passes_the_same_gate(&weakened).is_some(),
        "a driver with no named-target gate must fail the rule"
    );
}

/// A construct that appears only in a comment or in a `#[cfg(test)]` block
/// must not be reported.
#[test]
fn the_guard_ignores_comments_and_test_blocks() {
    // Rule 3: a `tokio::spawn` in a comment and in a test block is not the
    // driver spawning a task.
    let commented = "\
fn drive(&mut self, now_ms: u64) {
    // the driver must not call tokio::spawn( here
    self.now_ms = now_ms;
}

#[cfg(test)]
mod tests {
    #[test]
    fn spawns() {
        let _task = tokio::spawn(async {});
    }
}
";
    assert_eq!(
        check_driver_owns_no_task(commented),
        None,
        "a task that appears only in a comment or a test block must not be reported"
    );

    // Rule 4 and rule 5: an ungated selection or a private build path in a
    // comment or a test block is not a second path.
    let commented = "\
fn drive(&mut self, now_ms: u64) {
    // profiles.select(1, seed) and coordinator.submit_path( are forbidden
    self.now_ms = now_ms;
}

#[cfg(test)]
mod tests {
    #[test]
    fn selects() {
        let _ = profiles.select(1, seed);
        let _ = coordinator.submit_path(request);
    }
}
";
    assert_eq!(
        check_code_avoids(commented, UNGATED_SELECTION_CALL),
        None,
        "an ungated selection in a comment or a test block must not be reported"
    );
    assert_eq!(
        check_code_avoids(commented, "submit_path("),
        None,
        "a private build path in a comment or a test block must not be reported"
    );

    // Rule 7: a destination build in a comment or a test block is not the
    // driver touching a destination.
    let commented = "\
fn drive(&mut self, now_ms: u64) {
    // coordinator.submit_destination_via_outbound_tunnel( is forbidden
    self.now_ms = now_ms;
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds() {
        let _ = coordinator.submit_destination_via_outbound_tunnel(request);
    }
}
";
    assert_eq!(
        check_driver_never_touches_a_destination(commented),
        None,
        "a destination build in a comment or a test block must not be reported"
    );

    // Rule 1: a construction named only in a comment does not satisfy the
    // rule.
    let commented = "\
fn register_ssu2_service() {
    // let exploratory_driver = if exploratory_builds {
    let posture = config.exploratory.builds;
    let _ = posture;
}
";
    assert!(
        check_composition_builds_the_driver_gated(commented).is_some(),
        "a driver that is only named in a comment must not satisfy the rule"
    );

    // Rule 9: an observation call named only in a comment does not satisfy
    // the rule.
    let commented = "\
async fn advance_destination_pools_at(&mut self, now_ms: u64) {
    // self.observe_exploratory_outcome(&outcome);
    let _ = now_ms;
}
";
    assert!(
        check_outcomes_reach_the_driver_on_both_paths(commented).is_some(),
        "an observation call that appears only in a comment must not satisfy the rule"
    );
}

/// The rules are **structural**, not textual: a token that lives in a sibling
/// function must not satisfy a rule that names one function's body.
///
/// This is the property a whole-file token search cannot have, and it is the
/// property Plan 185's gap needed: the driver module existed with every token
/// in it, and a whole-file search would have found all of them while the
/// daemon still owned no driver at all.
#[test]
fn a_token_in_a_sibling_function_does_not_satisfy_a_rule() {
    // Rule 1: the driver is constructed by a sibling function.
    let rule_1 = "\
fn helper() {
    let _driver = crate::exploratory_driver::ExploratoryDriver::new(true, profiles, resolver);
}
fn register_ssu2_service() {
    let posture = config.exploratory.builds;
    let exploratory_builds = posture;
    let _ = exploratory_builds;
}
";
    assert!(
        check_composition_builds_the_driver_gated(rule_1).is_some(),
        "a driver constructed by a sibling function must not satisfy rule 1"
    );

    // Rule 3: the tick call sits in a sibling function.
    let rule_3 = "\
fn helper(&mut self, now_ms: u64) {
    self.drive_exploratory_builds(now_ms);
}
async fn advance_destination_pools_at(&mut self, now_ms: u64) {
    self.drive_tunnel_liveness(now_ms).await;
}
";
    assert!(
        check_body_contains(
            rule_3,
            "advance_destination_pools_at",
            "drive_exploratory_builds(",
        )
        .is_some(),
        "a tick call in a sibling function must not satisfy rule 3"
    );

    // Rule 4: the gated selection sits in a sibling function.
    let rule_4 = "\
fn helper(profiles: &mut PeerParticipationDriver, seed: u64) {
    let _selection = profiles.select_tunnel_build_peers(1, seed);
}
fn select_peer(&mut self, now_ms: u64) -> Option<SelectedPeer> {
    None
}
";
    assert!(
        check_driver_selects_through_the_gate(rule_4).is_some(),
        "a gated selection in a sibling function must not satisfy rule 4"
    );

    // Rule 5: the build submission sits in a sibling function.
    let rule_5 = "\
fn helper(coordinator: &mut ExploratoryBuildCoordinator) {
    let _ = coordinator.rebuild_exploratory(direction, peer, None, &delivery, &bridge, &mut rng);
}
fn drive(&mut self, now_ms: u64) -> ExploratoryDriverAction {
    ExploratoryDriverAction::Disabled
}
";
    assert!(
        check_driver_submits_through_the_single_path(rule_5).is_some(),
        "a build submission in a sibling function must not satisfy rule 5"
    );

    // Rule 6: the mapping of a variant sits in a sibling function.
    let rule_6 = "\
fn helper(outcome: &BuildCoordinatorOutcome) -> PeerObservation {
    match outcome {
        BuildCoordinatorOutcome::TimedOut { .. } => PeerObservation::Unanswered,
        _ => PeerObservation::Local,
    }
}
pub fn observation_for_outcome(outcome: &BuildCoordinatorOutcome) -> PeerObservation {
    PeerObservation::Local
}
";
    assert!(
        check_outcome_mapping_is_total(rule_6, rule_6).is_some(),
        "a variant handled in a sibling function must not satisfy rule 6"
    );

    // Rule 8: the learn call sits in a sibling function.
    let rule_8 = "\
fn helper(profiles: &mut PeerParticipationDriver, records: Vec<ValidatedRouterInfo>) {
    profiles.learn(records);
}
fn learn_bootstrap_peers(&self, bootstrap: Option<&Arc<Mutex<bootstrap::Bootstrap>>>, now_ms: u64) {
    let _ = (bootstrap, now_ms);
}
";
    assert!(
        check_profile_store_is_fed(rule_8).is_some(),
        "a learn call in a sibling function must not satisfy rule 8"
    );

    // Rule 9: the observation call sits in a sibling function.
    let rule_9 = "\
fn helper(&mut self, outcome: &BuildCoordinatorOutcome) {
    self.observe_exploratory_outcome(outcome);
}
async fn advance_destination_pools_at(&mut self, now_ms: u64) {
    let _ = now_ms;
}
async fn process_inbound(&mut self) {
    let _ = ();
}
fn observe_exploratory_outcome(&mut self, outcome: &BuildCoordinatorOutcome) {
    driver.observe_outcome(outcome);
}
";
    assert!(
        check_outcomes_reach_the_driver_on_both_paths(rule_9).is_some(),
        "an observation call in a sibling function must not satisfy rule 9"
    );

    // Rule 10: the publish call sits in a sibling function.
    let rule_10 = "\
fn helper(driver: &ExploratoryDriver) {
    let _counters = driver.counters();
}
fn publish_exploratory_driver_counters(&self) {
    let _ = ();
}
";
    assert!(
        check_counters_are_published(rule_10, rule_10).is_some(),
        "a publish call in a sibling function must not satisfy rule 10"
    );
}

/// The function-body extractor must be correct on the shapes a real crate
/// contains: a nested block, a string literal that holds a brace, a character
/// literal that holds a brace, and a function that is the last item in the
/// file.
#[test]
fn the_function_body_extractor_handles_nested_blocks_strings_and_a_trailing_item() {
    let source = "\
fn outer() -> u8 {
    let text = \"}\";
    let brace = '{';
    if true {
        let inner = \"{\";
        let _ = (text, brace, inner);
    }
    7
}
fn last() -> u8 {
    let text = \"{\";
    text.len() as u8
}
";
    let code = code_view(source);
    let outer = function_body(&code, "outer").expect("the outer body must be found");
    assert!(
        outer.contains("if true {"),
        "a nested block must stay inside the body"
    );
    assert!(
        !outer.contains("fn last"),
        "the body must end at its own closing brace"
    );
    let last = function_body(&code, "last").expect("the trailing body must be found");
    assert!(
        last.contains("text.len()"),
        "the trailing item's body must be found"
    );
    assert!(
        function_body(&code, "absent").is_none(),
        "an absent function must not be found"
    );
}

/// The variant enumerator must read the enum's own source, and a variant that
/// the mapping does not handle must be reported.
#[test]
fn the_variant_enumerator_reads_the_enum_source() {
    let build = "\
pub enum BuildCoordinatorOutcome {
    Installed { slot: TunnelSlot },
    TimedOut { direction: BuildDirection },
    DestinationBuildFailed { destination_id: DestinationId },
}
";
    let code = code_view(build);
    assert_eq!(
        enum_variants(&code, "BuildCoordinatorOutcome"),
        Some(vec![
            "Installed".to_owned(),
            "TimedOut".to_owned(),
            "DestinationBuildFailed".to_owned(),
        ]),
        "the variants must be read from the enum's own source"
    );
    assert!(
        enum_variants(&code, "AbsentOutcome").is_none(),
        "an absent enum must not be found"
    );
}
