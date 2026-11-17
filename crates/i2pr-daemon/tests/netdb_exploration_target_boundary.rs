//! Plan 514's structural guard for the live exploratory request's target.
//!
//! Plan 513 landed the live NetDB discovery spine, but the request it sent
//! was aimed wrongly: the live path picked the **lowest-hash** floodfill
//! advertiser in the flat store and sent the request to it, so every
//! exploration asked the **same** peer. The routing table's closest-first
//! selection -- the mechanism Plan 489 built and Plan 513 gave a reader --
//! was not consulted on that path at all. And `netdb.exploration_enabled`
//! was read by **nothing**, so the exploration ran whenever the routing
//! table was on.
//!
//! The guard asserts eight things about the **production** surface:
//!
//! 1. the routing table answers with its **closest** floodfill;
//! 2. the discovery spine asks the **table** for the peer to ask;
//! 3. the spine never falls back to the flat store's iteration order;
//! 4. the composition root never selects the floodfill itself;
//! 5. a request the table cannot target is **refused and counted**;
//! 6. the operator's switch is **read**;
//! 7. the switch gates the **exploration only** -- the RouterInfo-kind
//!    resolution stays above it, so the routing table keeps its production
//!    reader and Plan 497's M2 is not reopened;
//! 8. the configuration key is **consumed** by the daemon.
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

/// The participation module: the spine and the routing table's consumer.
const PARTICIPATION_MODULE: &str = "crates/i2pr-daemon/src/netdb_participation.rs";
/// The composition root: the daemon's participation owner and its cycle.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";
/// The configuration module: the operator's switch.
const CONFIG_MODULE: &str = "crates/i2pr-daemon/src/config.rs";

/// The counter that records a request the table could not target.
const NO_TARGET_COUNTER: &str = "explorations_refused_no_target";

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

/// Extracts the body of the `impl` block whose **definition** is anchored at
/// `anchor`.
///
/// The module owns several same-named helpers; scoping a rule to the impl
/// block that owns the helper is what makes the rule structural rather than
/// a search for whichever definition happens to mention a token.
fn impl_block(code: &str, anchor: &str) -> Option<String> {
    let start = find_definition(code, anchor)?;
    let (open, close) = brace_balanced_block(code, start + anchor.len())?;
    Some(code[open + 1..close - 1].to_owned())
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1: the routing table answers with its closest floodfill.
///
/// The accessor must exist, and its own body must ask the table for the
/// candidate list. A named accessor that computes its own order is not the
/// table's answer.
fn check_the_table_answers_with_its_closest_floodfill(source: &str) -> Option<String> {
    let Some(body) = item_body(source, "fn closest_floodfill(") else {
        return Some("`fn closest_floodfill` must exist in the production surface".to_owned());
    };
    if !body.contains("candidates(") {
        return Some("`fn closest_floodfill` must ask the table for its candidates".to_owned());
    }
    None
}

/// Rule 2: the discovery spine asks the **table** for the peer to ask.
///
/// The rule is scoped to the spine's own `impl` block, because the module
/// owns three same-named exploration starts.
fn check_the_spine_asks_the_table_for_its_target(source: &str) -> Option<String> {
    let Some(spine) = impl_block(source, "impl NetDbDiscoverySpine") else {
        return Some("`impl NetDbDiscoverySpine` must exist in the production surface".to_owned());
    };
    let Some(body) = item_body(&spine, "fn begin_exploration(") else {
        return Some(
            "`fn begin_exploration` must exist in the spine's production surface".to_owned(),
        );
    };
    if !body.contains("closest_floodfill(") {
        return Some("the spine's exploratory start must ask the table for its target".to_owned());
    }
    None
}

/// Rule 3: the spine never falls back to the flat store's iteration order.
///
/// The flat store's first advertiser is the **lowest-hash** floodfill, so
/// asking it is not a selection. The spine must not name it at all.
fn check_the_spine_never_falls_back_to_the_flat_store_order(source: &str) -> Option<String> {
    let Some(spine) = impl_block(source, "impl NetDbDiscoverySpine") else {
        return Some("`impl NetDbDiscoverySpine` must exist in the production surface".to_owned());
    };
    let Some(body) = item_body(&spine, "fn begin_exploration(") else {
        return Some(
            "`fn begin_exploration` must exist in the spine's production surface".to_owned(),
        );
    };
    if body.contains("floodfill_advertisers") {
        return Some(
            "the spine's exploratory start must not fall back to the flat store's order".to_owned(),
        );
    }
    None
}

/// Rule 4: the composition root never selects the floodfill itself.
///
/// The daemon's cycle must start the exploration and let the **spine** pick
/// the peer. A composition that picks the peer has the old bug back.
fn check_the_composition_never_selects_the_floodfill(source: &str) -> Option<String> {
    let Some(body) = item_body(source, "fn drive_discovery(") else {
        return Some("`fn drive_discovery` must exist in the production surface".to_owned());
    };
    if body.contains("floodfill_advertisers") {
        return Some(
            "the composition root must not select the floodfill from the flat store's order"
                .to_owned(),
        );
    }
    if !body.contains("begin_exploration(") {
        return Some("the composition root must start the exploration".to_owned());
    }
    None
}

/// Rule 5: a request the table cannot target is refused and counted.
///
/// The counter must be **declared** and **moved** in the spine's own
/// exploratory start. A declared counter nobody moves is a decoration.
fn check_the_refusal_is_counted(source: &str) -> Option<String> {
    let Some(counters) = impl_block(source, "pub struct NetDbDiscoveryCounters") else {
        return Some(
            "`pub struct NetDbDiscoveryCounters` must exist in the production surface".to_owned(),
        );
    };
    if !counters.contains(NO_TARGET_COUNTER) {
        return Some(format!("the counters must declare `{NO_TARGET_COUNTER}`"));
    }
    let Some(spine) = impl_block(source, "impl NetDbDiscoverySpine") else {
        return Some("`impl NetDbDiscoverySpine` must exist in the production surface".to_owned());
    };
    let Some(body) = item_body(&spine, "fn begin_exploration(") else {
        return Some(
            "`fn begin_exploration` must exist in the spine's production surface".to_owned(),
        );
    };
    if !body.contains(NO_TARGET_COUNTER) {
        return Some(format!(
            "the spine's exploratory start must count `{NO_TARGET_COUNTER}`"
        ));
    }
    None
}

/// Rule 6: the operator's switch is read.
///
/// `netdb.exploration_enabled` was parsed, defaulted and cross-validated,
/// and read by nothing. The cycle must consult it.
fn check_the_operator_switch_is_read(source: &str) -> Option<String> {
    let Some(body) = item_body(source, "fn drive_discovery(") else {
        return Some("`fn drive_discovery` must exist in the production surface".to_owned());
    };
    if !body.contains("exploration_enabled") {
        return Some(
            "the discovery cycle must consult the operator's exploration switch".to_owned(),
        );
    }
    None
}

/// Rule 7: the switch gates the **exploration only**.
///
/// The RouterInfo-kind resolution must come **before** the switch. If the
/// switch were checked first, the resolution would be gated too, the routing
/// table would lose its production reader, and Plan 497's M2 would reopen.
fn check_the_switch_gates_the_exploration_only(source: &str) -> Option<String> {
    let Some(body) = item_body(source, "fn drive_discovery(") else {
        return Some("`fn drive_discovery` must exist in the production surface".to_owned());
    };
    let resolution = body.find("pending_resolution_target(");
    let switch = body.find("exploration_enabled");
    match (resolution, switch) {
        (Some(resolution), Some(switch)) if resolution < switch => None,
        (Some(_), Some(_)) => {
            Some("the operator's switch must come after the RouterInfo-kind resolution".to_owned())
        }
        (None, Some(_)) => {
            Some("the discovery cycle must keep the RouterInfo-kind resolution".to_owned())
        }
        _ => Some("the discovery cycle must consult the operator's exploration switch".to_owned()),
    }
}

/// Rule 8: the configuration key is consumed by the daemon.
///
/// The key must be read out of the configuration and carried into the
/// participation owner. A key that is parsed and never read is the defect
/// this plan fixes.
fn check_the_configuration_key_is_consumed(source: &str) -> Option<String> {
    let Some(body) = item_body(
        source,
        "fn new(config: &Config, local_hash: RouterHash, now_ms: u64)",
    ) else {
        return Some(
            "the participation owner's constructor must exist in the production surface".to_owned(),
        );
    };
    if !body.contains("netdb.exploration_enabled") {
        return Some("the participation owner must read `netdb.exploration_enabled`".to_owned());
    }
    None
}

// ----------------------------------------------------------------------
// The rules, against the real tree.
// ----------------------------------------------------------------------

#[test]
fn the_table_answers_with_its_closest_floodfill() {
    let participation = code_view(&source_of(PARTICIPATION_MODULE));
    assert_eq!(
        check_the_table_answers_with_its_closest_floodfill(&participation),
        None,
        "the table must answer with its closest floodfill"
    );
}

#[test]
fn the_spine_asks_the_table_for_its_target() {
    let participation = code_view(&source_of(PARTICIPATION_MODULE));
    assert_eq!(
        check_the_spine_asks_the_table_for_its_target(&participation),
        None,
        "the spine must ask the table for the peer to ask"
    );
}

#[test]
fn the_spine_never_falls_back_to_the_flat_store_order() {
    let participation = code_view(&source_of(PARTICIPATION_MODULE));
    assert_eq!(
        check_the_spine_never_falls_back_to_the_flat_store_order(&participation),
        None,
        "the spine must never fall back to the flat store's iteration order"
    );
}

#[test]
fn the_composition_never_selects_the_floodfill() {
    let composition = code_view(&source_of(COMPOSITION_MODULE));
    assert_eq!(
        check_the_composition_never_selects_the_floodfill(&composition),
        None,
        "the composition root must not select the floodfill itself"
    );
}

#[test]
fn the_refusal_is_counted() {
    let participation = code_view(&source_of(PARTICIPATION_MODULE));
    assert_eq!(
        check_the_refusal_is_counted(&participation),
        None,
        "a request the table cannot target must be refused and counted"
    );
}

#[test]
fn the_operator_switch_is_read() {
    let composition = code_view(&source_of(COMPOSITION_MODULE));
    assert_eq!(
        check_the_operator_switch_is_read(&composition),
        None,
        "the operator's switch must be read"
    );
}

#[test]
fn the_switch_gates_the_exploration_only() {
    let composition = code_view(&source_of(COMPOSITION_MODULE));
    assert_eq!(
        check_the_switch_gates_the_exploration_only(&composition),
        None,
        "the switch must gate the exploration only"
    );
}

#[test]
fn the_configuration_key_is_consumed() {
    let composition = code_view(&source_of(COMPOSITION_MODULE));
    assert_eq!(
        check_the_configuration_key_is_consumed(&composition),
        None,
        "the configuration key must be consumed by the daemon"
    );
}

#[test]
fn the_configuration_declares_the_switch() {
    let config = code_view(&source_of(CONFIG_MODULE));
    assert!(
        config.contains("exploration_enabled"),
        "the configuration must declare `exploration_enabled`"
    );
}

// ----------------------------------------------------------------------
// The inverted probes: each rule must report its own violation.
// ----------------------------------------------------------------------

/// The inverted probe for rule 1: an accessor that computes its own order
/// instead of asking the table must be reported.
#[test]
fn the_guard_reports_a_closest_floodfill_that_does_not_ask_the_table() {
    let violating = "\
fn closest_floodfill(&self) -> Option<RouterHash> {
    let mut best: Option<RouterHash> = None;
    best
}
";
    let reason = check_the_table_answers_with_its_closest_floodfill(violating);
    assert!(
        reason.is_some(),
        "an accessor that does not ask the table must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 2: a spine that names the peer it was handed
/// must be reported.
#[test]
fn the_guard_reports_a_spine_that_does_not_ask_the_table() {
    let violating = "\
impl NetDbDiscoverySpine {
    pub fn begin_exploration(&self, floodfill: RouterHash) -> DiscoveryDispatch {
        let _ = floodfill;
        DiscoveryDispatch::Terminal {
            lookup_id: LookupId::new(0, LookupKind::Exploratory, RouterHash::from_bytes([0; 32])),
            final_state: LookupFinalState::Cancelled,
        }
    }
}
";
    let reason = check_the_spine_asks_the_table_for_its_target(violating);
    assert!(
        reason.is_some(),
        "a spine that does not ask the table must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 3: a spine that falls back to the flat
/// store's order must be reported.
#[test]
fn the_guard_reports_a_spine_that_falls_back_to_the_flat_store_order() {
    let violating = "\
impl NetDbDiscoverySpine {
    pub fn begin_exploration(&self, store: &RouterInfoStore) -> DiscoveryDispatch {
        let floodfill = store.floodfill_advertisers().next();
        let _ = floodfill;
        DiscoveryDispatch::Terminal {
            lookup_id: LookupId::new(0, LookupKind::Exploratory, RouterHash::from_bytes([0; 32])),
            final_state: LookupFinalState::Cancelled,
        }
    }
}
";
    let reason = check_the_spine_never_falls_back_to_the_flat_store_order(violating);
    assert!(
        reason.is_some(),
        "a spine that falls back to the flat store's order must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 4: a composition that picks the floodfill
/// itself must be reported.
#[test]
fn the_guard_reports_a_composition_that_selects_the_floodfill() {
    let violating = "\
fn drive_discovery(&self, store: &RouterInfoStore) {
    let Some(floodfill) = store.floodfill_advertisers().next().map(|peer| peer.key()) else {
        return;
    };
    let dispatch = self.discovery.begin_exploration(floodfill);
}
";
    let reason = check_the_composition_never_selects_the_floodfill(violating);
    assert!(
        reason.is_some(),
        "a composition that selects the floodfill must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 5: a counter that is declared but never moved
/// must be reported.
#[test]
fn the_guard_reports_a_refusal_that_is_never_counted() {
    let violating = "\
pub struct NetDbDiscoveryCounters {
    pub explorations_refused_no_target: u64,
}

impl NetDbDiscoverySpine {
    pub fn begin_exploration(&self) -> DiscoveryDispatch {
        DiscoveryDispatch::Terminal {
            lookup_id: LookupId::new(0, LookupKind::Exploratory, RouterHash::from_bytes([0; 32])),
            final_state: LookupFinalState::Cancelled,
        }
    }
}
";
    let reason = check_the_refusal_is_counted(violating);
    assert!(
        reason.is_some(),
        "a refusal nobody counts must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 6: a cycle that never consults the switch
/// must be reported.
#[test]
fn the_guard_reports_a_cycle_that_ignores_the_operator_switch() {
    let violating = "\
fn drive_discovery(&self) {
    let dispatch = self.discovery.begin_exploration();
}
";
    let reason = check_the_operator_switch_is_read(violating);
    assert!(
        reason.is_some(),
        "a cycle that ignores the operator's switch must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 7: a switch checked **before** the
/// resolution must be reported.
#[test]
fn the_guard_reports_a_switch_that_gates_the_resolution_too() {
    let violating = "\
fn drive_discovery(&self) {
    if !self.exploration_enabled {
        return;
    }
    let target = self.discovery.pending_resolution_target(store);
    let _ = target;
}
";
    let reason = check_the_switch_gates_the_exploration_only(violating);
    assert!(
        reason.is_some(),
        "a switch that gates the resolution too must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 8: an owner that never reads the key must be
/// reported.
#[test]
fn the_guard_reports_a_key_that_is_never_consumed() {
    let violating = "\
fn new(config: &Config, local_hash: RouterHash, now_ms: u64) -> Self {
    let _ = config;
    let _ = local_hash;
    let _ = now_ms;
    Self {}
}
";
    let reason = check_the_configuration_key_is_consumed(violating);
    assert!(
        reason.is_some(),
        "a key nobody consumes must be reported: {reason:?}"
    );
}
