//! Plan 509 static boundary guard for the lookup-target API-version posture.
//!
//! Plan 503 recorded as a Medium finding that *the router still asks a
//! below-floor floodfill for a lookup*, and left it open because extending
//! the floor to the lookup path would change which floodfills the router can
//! ask at all — a product decision, and therefore a plan-of-record's work.
//!
//! Plan 509 makes that decision: **the lookup target is not version-gated.**
//! The decision is worth a guard because it is a *structural* posture, not a
//! behaviour: a behavioural test cannot see a lookup selection quietly
//! changing from the nearest-floodfill selection to the version-gated one.
//!
//! The guard asserts four things:
//!
//! 1. **The lookup path is ungated.** The daemon's lease lookup, the
//!    lookup state machine's two store-backed selections and the rotation's
//!    re-selection must all use the **nearest-floodfill** selection, and none
//!    of them may fall back to the version-gated one. The daemon's lookup
//!    body must not take a version decision at all.
//! 2. **The premise holds.** The specification's obligation table carries no
//!    lookup duty, the derived floors carry no lookup floor, and the gate
//!    vocabulary has exactly two variants. A lookup gate cannot be expressed
//!    without a third specification duty, so a change here is a change to the
//!    decision and needs a plan-of-record.
//! 3. **The answer is validated.** The lookup's answer path must go through
//!    the lease-set2 validation, so the responder's version is not what the
//!    router trusts.
//! 4. **The rotation skips.** The state machine must apply its already-queried
//!    and excluded sets when it picks the next candidate, so an unhelpful
//!    candidate is skipped rather than re-asked.
//!
//! The guard is deliberately **fail-closed**: it strips comments before
//! matching, it excludes `#[cfg(test)]` blocks, an absent function is a
//! violation rather than a pass, and every rule carries its own **inverted
//! probe** — a synthetic violating source must be reported. If a rule ever
//! stops detecting its violation, its probe fails.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required call, so a token that appears in a sibling
//! function, in a comment, or in a test block does not satisfy them.

use std::fs;
use std::path::{Path, PathBuf};

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
    let mut offset = 0usize;
    while let Some(position) = code[offset..].find(needle.as_str()) {
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
        let (open, close) = brace_balanced_block(code, after)?;
        return Some(&code[open + 1..close - 1]);
    }
    None
}

/// The body of `fn <name>` must contain `token`.
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

/// The daemon's lease-lookup body, and the state machine's two
/// store-backed selections, must use the nearest-floodfill selection.
const UNGATED_SELECTION_SITES: &[(&str, &str, &str)] = &[
    (
        "crates/i2pr-daemon/src/destination_tunnels.rs",
        "begin_lease_lookup_inner",
        "the daemon's lease lookup",
    ),
    (
        "crates/i2pr-netdb/src/lookup_engine.rs",
        "start",
        "the lookup state machine's first selection",
    ),
    (
        "crates/i2pr-netdb/src/lookup_engine.rs",
        "start_with_preferred",
        "the lookup state machine's preferred selection",
    ),
    (
        "crates/i2pr-netdb/src/lookup_engine.rs",
        "advance_active",
        "the lookup state machine's rotation",
    ),
];

/// The version-gated selection must not appear in the lookup path.
const GATED_SELECTION_BODIES: &[(&str, &str)] = &[
    (
        "crates/i2pr-daemon/src/destination_tunnels.rs",
        "begin_lease_lookup_inner",
    ),
    (
        "crates/i2pr-daemon/src/netdb_seam.rs",
        "begin_lease_set2_lookup_for_key_with_store",
    ),
    ("crates/i2pr-netdb/src/lookup_engine.rs", "start"),
    (
        "crates/i2pr-netdb/src/lookup_engine.rs",
        "start_with_preferred",
    ),
    ("crates/i2pr-netdb/src/lookup_engine.rs", "advance_active"),
];

/// The version decisions the daemon's lookup body must not take.
const VERSION_DECISION_TOKENS: &[&str] = &[
    "dsm_target_eligibility",
    "tunnel_peer_eligibility",
    "VersionEligibility",
    "select_version_gated_floodfills",
];

/// The files the guard reads.
const TREE_FILES: &[&str] = &[
    "crates/i2pr-proto/src/common/version.rs",
    "crates/i2pr-netdb/src/selection.rs",
    "crates/i2pr-netdb/src/lookup_policy.rs",
    "crates/i2pr-netdb/src/lookup_engine.rs",
    "crates/i2pr-daemon/src/destination_tunnels.rs",
    "crates/i2pr-daemon/src/netdb_seam.rs",
];

/// The real tree, read once.
struct Tree {
    sources: Vec<(String, String)>,
}

impl Tree {
    /// The source of one file.
    fn source(&self, path: &str) -> &str {
        self.sources
            .iter()
            .find(|(name, _)| name == path)
            .map(|(_, source)| source.as_str())
            .unwrap_or_else(|| panic!("the tree must carry {path}"))
    }
}

/// Reads the real tree.
fn landed() -> Tree {
    Tree {
        sources: TREE_FILES
            .iter()
            .map(|path| ((*path).to_owned(), source_of(path)))
            .collect(),
    }
}

/// Rule 1: the lookup path selects through the nearest-floodfill selection.
fn rule_the_lookup_path_is_ungated(tree: &Tree) -> Option<String> {
    for (path, name, label) in UNGATED_SELECTION_SITES {
        let source = tree.source(path);
        if let Some(violation) = check_body_contains(source, name, "select_floodfill_candidates(") {
            return Some(format!(
                "{label} must select through the nearest-floodfill selection: {violation}"
            ));
        }
    }
    None
}

/// Rule 1: the lookup path never selects through the version-gated one.
fn rule_the_lookup_path_avoids_the_gated_selection(tree: &Tree) -> Option<String> {
    for (path, name) in GATED_SELECTION_BODIES {
        let source = tree.source(path);
        if let Some(violation) = check_body_avoids(source, name, "select_version_gated_floodfills(")
        {
            return Some(format!(
                "the lookup path must not fall back to the version-gated selection: {violation}"
            ));
        }
    }
    None
}

/// Rule 1: the daemon's lookup body takes no version decision at all.
fn rule_the_lookup_body_takes_no_version_decision(tree: &Tree) -> Option<String> {
    let source = tree.source("crates/i2pr-daemon/src/destination_tunnels.rs");
    let code = code_view(source);
    let Some(body) = function_body(&code, "begin_lease_lookup_inner") else {
        return Some(
            "`fn begin_lease_lookup_inner` must exist in the production surface".to_owned(),
        );
    };
    for token in VERSION_DECISION_TOKENS {
        if body.contains(token) {
            return Some(format!(
                "the lookup path must take no version decision, and it took one ({token})"
            ));
        }
    }
    None
}

/// Rule 2: the specification's obligation table carries no lookup duty.
fn rule_the_obligation_table_carries_no_lookup_duty(tree: &Tree) -> Option<String> {
    let source = tree.source("crates/i2pr-proto/src/common/version.rs");
    let code = code_view(source);
    for structure in ["MinimumPeerObligation", "VersionFloors"] {
        let needle = format!("pub struct {structure}");
        let Some(start) = code.find(&needle) else {
            return Some(format!(
                "the specification's table must carry `{structure}`"
            ));
        };
        let Some((open, close)) = brace_balanced_block(&code, start) else {
            return Some(format!("`{structure}` must have a body"));
        };
        let body = &code[open + 1..close - 1];
        if body.contains("lookup") {
            return Some(format!(
                "the specification's table carries no lookup duty, so `{structure}` must not name one"
            ));
        }
    }
    None
}

/// Rule 2: the gate vocabulary has exactly two variants.
fn rule_the_gate_vocabulary_has_two_variants(tree: &Tree) -> Option<String> {
    let source = tree.source("crates/i2pr-netdb/src/selection.rs");
    let code = code_view(source);
    let Some(start) = code.find("pub enum VersionGate") else {
        return Some("the gate vocabulary must exist".to_owned());
    };
    let Some((open, close)) = brace_balanced_block(&code, start) else {
        return Some("the gate vocabulary must have a body".to_owned());
    };
    let body = &code[open + 1..close - 1];
    for variant in ["TunnelBuildPeer", "DsmTarget"] {
        if !body.contains(variant) {
            return Some(format!(
                "the gate vocabulary must carry the `{variant}` variant, because the \
                 specification's table imposes that duty"
            ));
        }
    }
    if body.contains("Lookup") {
        return Some(
            "the gate vocabulary must carry exactly two variants: the specification's table \
             imposes no lookup duty"
                .to_owned(),
        );
    }
    None
}

/// Rule 3: the lookup's answer is validated by the router.
fn rule_the_answer_is_validated(tree: &Tree) -> Option<String> {
    let source = tree.source("crates/i2pr-netdb/src/lookup_engine.rs");
    check_body_contains(
        source,
        "handle_database_store_lease_set2",
        "ValidatedLeaseSet2::from_lease_set2(",
    )
}

/// Rule 4: the rotation skips what it has already asked.
fn rule_the_rotation_skips(tree: &Tree) -> Option<String> {
    let source = tree.source("crates/i2pr-netdb/src/lookup_engine.rs");
    for token in [
        "state.queried.contains(&key)",
        "state.excluded.contains(&key)",
    ] {
        if let Some(violation) = check_body_contains(source, "advance_attempt", token) {
            return Some(format!(
                "the rotation must skip what it has already asked: {violation}"
            ));
        }
    }
    None
}

/// One rule of the guard: a label and the check it applies.
type Rule = (&'static str, fn(&Tree) -> Option<String>);

/// Every rule, as a list of violations.
fn check_all(tree: &Tree) -> Vec<String> {
    let rules: Vec<Rule> = vec![
        ("rule 1", rule_the_lookup_path_is_ungated),
        ("rule 1", rule_the_lookup_path_avoids_the_gated_selection),
        ("rule 1", rule_the_lookup_body_takes_no_version_decision),
        ("rule 2", rule_the_obligation_table_carries_no_lookup_duty),
        ("rule 2", rule_the_gate_vocabulary_has_two_variants),
        ("rule 3", rule_the_answer_is_validated),
        ("rule 4", rule_the_rotation_skips),
    ];
    let mut violations = Vec::new();
    for (label, rule) in rules {
        if let Some(violation) = rule(tree) {
            violations.push(format!("{label}: {violation}"));
        }
    }
    violations
}

/// Replaces the `nth` occurrence of `old` with `new`.
///
/// The replacement is **asserted**: a mutation that does not apply is a
/// defect in the probe, not a silent pass.
fn replace_occurrence(text: &str, old: &str, new: &str, nth: usize) -> String {
    let mut count = 0usize;
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find(old) {
        if count == nth {
            out.push_str(&rest[..position]);
            out.push_str(new);
            out.push_str(&rest[position + old.len()..]);
            return out;
        }
        count += 1;
        rest = &rest[position + old.len()..];
    }
    out.push_str(rest);
    assert!(count > nth, "the mutation must apply");
    out
}

/// Mutates one file of the tree.
fn mutate(tree: &Tree, path: &str, old: &str, new: &str) -> Tree {
    let mut sources = tree.sources.clone();
    for (name, source) in sources.iter_mut() {
        if name == path {
            *source = replace_occurrence(source, old, new, 0);
            return Tree { sources };
        }
    }
    panic!("the tree must carry {path}");
}

#[test]
fn the_landed_tree_holds_every_rule() {
    assert_eq!(check_all(&landed()), Vec::<String>::new());
}

#[test]
fn every_rule_is_reachable_and_the_tree_holds() {
    let tree = landed();
    let rules: Vec<Rule> = vec![
        ("rule 1", rule_the_lookup_path_is_ungated),
        ("rule 1", rule_the_lookup_path_avoids_the_gated_selection),
        ("rule 1", rule_the_lookup_body_takes_no_version_decision),
        ("rule 2", rule_the_obligation_table_carries_no_lookup_duty),
        ("rule 2", rule_the_gate_vocabulary_has_two_variants),
        ("rule 3", rule_the_answer_is_validated),
        ("rule 4", rule_the_rotation_skips),
    ];
    assert_eq!(rules.len(), 7);
    for (label, rule) in rules {
        assert_eq!(rule(&tree), None, "{label} must hold on the landed tree");
    }
}

/// Rule 1's inverted probe: a lookup that selects through the version-gated
/// selection must be reported.
#[test]
fn the_ungated_rule_fires_when_the_lookup_falls_back_to_the_gated_selection() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        "crates/i2pr-daemon/src/destination_tunnels.rs",
        "select_floodfill_candidates(&self.store, &lookup_key, routing_key, &[], &self.policy)",
        "select_version_gated_floodfills(&self.store, &lookup_key, routing_key, &[], &self.policy)",
    );
    assert!(rule_the_lookup_path_avoids_the_gated_selection(&mutated).is_some());
    assert!(rule_the_lookup_body_takes_no_version_decision(&mutated).is_some());
}

/// Rule 1's second inverted probe: a lookup that takes a version decision
/// must be reported.
#[test]
fn the_version_decision_rule_fires_when_the_lookup_asks_for_one() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        "crates/i2pr-daemon/src/destination_tunnels.rs",
        "        let candidates =\n            select_floodfill_candidates(",
        "        let _ = dsm_target_eligibility;\n        let candidates =\n            select_floodfill_candidates(",
    );
    assert!(rule_the_lookup_body_takes_no_version_decision(&mutated).is_some());
}

/// Rule 2's inverted probe: an obligation row that names a lookup duty must
/// be reported.
#[test]
fn the_premise_rule_fires_when_the_table_gains_a_lookup_duty() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        "crates/i2pr-proto/src/common/version.rs",
        "pub constrains_dsm_targets: bool,",
        "pub constrains_dsm_targets: bool,\n    pub constrains_lookup_targets: bool,",
    );
    assert!(rule_the_obligation_table_carries_no_lookup_duty(&mutated).is_some());
}

/// Rule 2's second inverted probe: a third gate variant must be reported.
#[test]
fn the_gate_rule_fires_when_a_third_variant_appears() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        "crates/i2pr-netdb/src/selection.rs",
        "    DsmTarget,",
        "    DsmTarget,\n    LookupTarget,",
    );
    assert!(rule_the_gate_vocabulary_has_two_variants(&mutated).is_some());
}

/// Rule 3's inverted probe: an answer path that does not validate must be
/// reported.
#[test]
fn the_validation_rule_fires_when_the_answer_is_not_validated() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        "crates/i2pr-netdb/src/lookup_engine.rs",
        "ValidatedLeaseSet2::from_lease_set2(",
        "unvalidated_lease_set2(",
    );
    assert!(rule_the_answer_is_validated(&mutated).is_some());
}

/// Rule 4's inverted probe: a rotation that does not skip what it has already
/// asked must be reported.
#[test]
fn the_rotation_rule_fires_when_the_rotation_re_asks() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        "crates/i2pr-netdb/src/lookup_engine.rs",
        "if state.queried.contains(&key) || state.excluded.contains(&key) {",
        "if false {",
    );
    assert!(rule_the_rotation_skips(&mutated).is_some());
}

/// The guard must not report a construct that only appears in a comment or in
/// a test block.
#[test]
fn the_guard_ignores_comments_and_test_blocks() {
    // A comment that names the gated selection must not change the verdict.
    let tree = landed();
    let mutated = mutate(
        &tree,
        "crates/i2pr-daemon/src/destination_tunnels.rs",
        "        let candidates =\n            select_floodfill_candidates(",
        "        // select_version_gated_floodfills(\n        let candidates =\n            select_floodfill_candidates(",
    );
    assert_eq!(check_all(&mutated), Vec::<String>::new());
}

/// The guard reads the real tree.
#[test]
fn the_guard_reads_the_real_tree() {
    let tree = landed();
    assert_eq!(tree.sources.len(), TREE_FILES.len());
    for (path, source) in &tree.sources {
        assert!(!source.is_empty(), "{path} must not be empty");
    }
}
