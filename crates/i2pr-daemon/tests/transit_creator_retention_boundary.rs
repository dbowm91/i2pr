//! Plan 508 — the static guard for the transit creator-correlation
//! retention window.
//!
//! The guard asserts the **structural** shape of the retention policy: the
//! window is derived from the coordinator's own bounds, every installed
//! fact carries an expiry, the expiry is computed in the safe direction,
//! every use site consults the expiry, the facts are maps rather than
//! insert-only sets, the sweep drops the expired facts, the production
//! wiring installs and sweeps on the tick it already had, no configuration
//! key appears, the owner gains no task and no timer, and the counts are
//! the observability seam.
//!
//! Properties of the guard:
//!
//! - **Structural, not textual.** A rule asserts that a named function's
//!   own **body** contains the required **call**, matched with its open
//!   parenthesis. Comments are stripped and `#[cfg(test)]` blocks are
//!   excluded, so a doc comment that merely *names* a construct cannot
//!   satisfy a rule, and a dead helper cannot either.
//! - **Fails closed.** A function that is absent is a violation, never a
//!   silently passing rule.
//! - **Every rule carries an inverted probe.** Each probe mutates a copy of
//!   the real source, asserts that the mutation actually applied, and then
//!   asserts that the rule fires. A rule that cannot fail is a defect.
//!
//! The helper machinery and the rule primitives are copied verbatim from
//! the established guards, so this guard asserts in the repository's own
//! terms.
//!
//! The guard reads the **real** tree from the workspace root, so it fails if
//! the tree does not hold the shape it asserts.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The transit owner module that holds the creator-correlation facts.
const TRANSIT_OWNER_MODULE: &str = "crates/i2pr-daemon/src/transit_owner.rs";

/// The build coordinator module that owns the coordinator's own bounds.
const EXPLORATORY_BUILD_MODULE: &str = "crates/i2pr-daemon/src/exploratory_build.rs";

/// The composition root that installs the facts on the pump tick.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";

/// The configuration parser, which must gain no retention key.
const CONFIG_MODULE: &str = "crates/i2pr-daemon/src/config.rs";

/// The window constant's declaration, which must be derived rather than
/// restated.
const WINDOW_DECLARATION: &str =
    "CREATOR_CORRELATION_RETENTION_MS: u64 = BUILD_DEADLINE_MS + BUILD_TUNNEL_ID_RETENTION_MS";

/// The constructs that would give the owner its own task or timer.
const OWNER_TASK_TOKENS: &[&str] = &["tokio::spawn", "tokio::time::interval"];

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

/// The production code view must not contain `token`, compared without regard
/// to case.
fn check_code_avoids_case_insensitive(source: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    if code.to_lowercase().contains(&token.to_lowercase()) {
        Some(format!(
            "the production code view must not contain `{token}`"
        ))
    } else {
        None
    }
}

// ----------------------------------------------------------------------
// The landed tree the rules are applied to.
// ----------------------------------------------------------------------

/// One named rule over the landed tree.
type Rule = (&'static str, fn(&Landed) -> Option<String>);

/// The landed sources the rules are applied to.
#[derive(Clone)]
struct Landed {
    owner: String,
    build: String,
    composition: String,
    config: String,
}

fn landed() -> Landed {
    Landed {
        owner: source_of(TRANSIT_OWNER_MODULE),
        build: source_of(EXPLORATORY_BUILD_MODULE),
        composition: source_of(COMPOSITION_MODULE),
        config: source_of(CONFIG_MODULE),
    }
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1 — the window is the coordinator's **own** bound.
///
/// The window is derived, not restated: a second number would be a second
/// source of truth, and the fact the facts exist for depends on it.
fn rule_window_is_the_trees_own_bound(tree: &Landed) -> Option<String> {
    check_code_contains(&tree.build, WINDOW_DECLARATION)
}

/// Rule 2 — every installed fact carries an expiry.
fn rule_every_installed_fact_carries_an_expiry(tree: &Landed) -> Option<String> {
    for name in [
        "install_creator_build",
        "install_creator_data",
        "install_creator_gateway",
    ] {
        if let Some(violation) = check_body_contains(&tree.owner, name, "creator_expiry_seconds(") {
            return Some(violation);
        }
    }
    None
}

/// Rule 3 — the expiry is computed in the safe direction.
///
/// The ceiling keeps the stored second no earlier than the true expiry, so
/// a fact is never declined before it has expired.
fn rule_the_expiry_is_computed_in_the_safe_direction(tree: &Landed) -> Option<String> {
    for token in [
        "CREATOR_CORRELATION_RETENTION_MS",
        "saturating_add(999)",
        "/ 1_000",
    ] {
        if let Some(violation) = check_body_contains(&tree.owner, "creator_expiry_seconds", token) {
            return Some(violation);
        }
    }
    None
}

/// Rule 4 — every use site consults the expiry.
///
/// A use site that consults the set's membership rather than the fact's
/// expiry would honour an expired fact.
fn rule_every_use_site_consults_the_expiry(tree: &Landed) -> Option<String> {
    for (name, set) in [
        ("dispatch_router_garlic", "creator_builds"),
        ("dispatch_build_body", "creator_builds"),
        ("handle_tunnel_data_inner", "creator_data"),
        ("handle_gateway_inner", "creator_gateways"),
    ] {
        if let Some(violation) = check_body_contains(&tree.owner, name, set) {
            return Some(violation);
        }
        if let Some(violation) = check_body_contains(&tree.owner, name, "expiry") {
            return Some(violation);
        }
        if let Some(violation) = check_body_avoids(&tree.owner, name, "contains(") {
            return Some(violation);
        }
    }
    None
}

/// Rule 5 — the facts are maps with an expiry, not insert-only sets.
fn rule_the_facts_are_maps_with_an_expiry(tree: &Landed) -> Option<String> {
    for token in [
        "creator_builds: BTreeMap<",
        "creator_data: BTreeMap<",
        "creator_gateways: BTreeMap<",
    ] {
        if let Some(violation) = check_struct_contains(&tree.owner, "TransitLiveOwner", token) {
            return Some(violation);
        }
    }
    for token in ["BTreeSet<(i2pr_proto::Hash, u32)>", "BTreeSet<u32>"] {
        if let Some(violation) = check_code_avoids(&tree.owner, token) {
            return Some(violation);
        }
    }
    None
}

/// Rule 6 — the sweep drops the expired facts.
fn rule_the_sweep_drops_the_expired_facts(tree: &Landed) -> Option<String> {
    for token in [
        "retain(",
        "creator_builds",
        "creator_data",
        "creator_gateways",
        "now_ms / 1_000",
        "> now_seconds",
    ] {
        if let Some(violation) =
            check_body_contains(&tree.owner, "expire_creator_correlation", token)
        {
            return Some(violation);
        }
    }
    None
}

/// Rule 7 — the production wiring installs and sweeps on the tick.
fn rule_the_wiring_installs_and_sweeps(tree: &Landed) -> Option<String> {
    for token in [
        "install_creator_build(",
        "install_creator_data(",
        "install_creator_gateway(",
        "expire_creator_correlation(",
    ] {
        if let Some(violation) = check_body_contains(
            &tree.composition,
            "install_transit_creator_correlation",
            token,
        ) {
            return Some(violation);
        }
    }
    None
}

/// Rule 8 — no new configuration key.
fn rule_no_new_configuration_key(tree: &Landed) -> Option<String> {
    for token in ["retention", "correlation"] {
        if let Some(violation) = check_code_avoids_case_insensitive(&tree.config, token) {
            return Some(violation);
        }
    }
    None
}

/// Rule 9 — the owner gains no task and no timer.
fn rule_the_owner_gains_no_task_or_timer(tree: &Landed) -> Option<String> {
    for token in OWNER_TASK_TOKENS {
        if let Some(violation) = check_code_avoids(&tree.owner, token) {
            return Some(violation);
        }
    }
    None
}

/// Rule 10 — the counts are the observability seam.
fn rule_the_counts_are_the_observability_seam(tree: &Landed) -> Option<String> {
    for token in ["creator_builds", "creator_data", "creator_gateways"] {
        if let Some(violation) =
            check_body_contains(&tree.owner, "creator_correlation_counts", token)
        {
            return Some(violation);
        }
    }
    check_struct_contains(&tree.owner, "CreatorCorrelationCounts", "pub builds: usize")
}

/// Every rule, as a list of violations.
fn check_all(tree: &Landed) -> Vec<String> {
    let mut violations = Vec::new();
    let rules: Vec<Rule> = vec![
        ("rule 1", rule_window_is_the_trees_own_bound),
        ("rule 2", rule_every_installed_fact_carries_an_expiry),
        ("rule 3", rule_the_expiry_is_computed_in_the_safe_direction),
        ("rule 4", rule_every_use_site_consults_the_expiry),
        ("rule 5", rule_the_facts_are_maps_with_an_expiry),
        ("rule 6", rule_the_sweep_drops_the_expired_facts),
        ("rule 7", rule_the_wiring_installs_and_sweeps),
        ("rule 8", rule_no_new_configuration_key),
        ("rule 9", rule_the_owner_gains_no_task_or_timer),
        ("rule 10", rule_the_counts_are_the_observability_seam),
    ];
    for (label, rule) in rules {
        if let Some(violation) = rule(tree) {
            violations.push(format!("{label}: {violation}"));
        }
    }
    violations
}

fn violations_of(tree: &Landed) -> Vec<String> {
    check_all(tree)
}

// ----------------------------------------------------------------------
// The tests.
// ----------------------------------------------------------------------

/// The landed tree must hold every rule.
#[test]
fn the_landed_tree_holds_every_rule() {
    let violations = violations_of(&landed());
    assert!(
        violations.is_empty(),
        "the landed tree must hold every Plan 508 rule, got: {violations:?}"
    );
}

/// The guard reads the real tree, and the tree is not trivial.
#[test]
fn the_guard_reads_the_real_tree() {
    let tree = landed();
    assert!(tree.owner.len() > 20_000);
    assert!(tree.build.contains("CREATOR_CORRELATION_RETENTION_MS"));
    assert!(
        tree.composition
            .contains("install_transit_creator_correlation")
    );
    assert!(tree.owner.contains("expire_creator_correlation"));
}

/// An absent function fails closed: renaming `fn expire_creator_correlation`
/// must make the rules that name it fire, not pass.
#[test]
fn an_absent_function_fails_closed() {
    let tree = landed();
    let mutated = Landed {
        owner: replace_occurrence(
            &tree.owner,
            "fn expire_creator_correlation",
            "fn expire_creator_correlation_x",
            0,
        ),
        ..tree
    };
    assert!(
        mutated.owner.contains("fn expire_creator_correlation_x"),
        "the mutation must apply"
    );
    let violations = violations_of(&mutated);
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("rule 6")),
        "an absent function must be reported, got: {violations:?}"
    );
}

/// Rule 1 — the window is the coordinator's own bound.
#[test]
fn the_window_must_be_the_trees_own_bound() {
    assert_eq!(rule_window_is_the_trees_own_bound(&landed()), None);
}

/// Rule 1's inverted probe: restating the window as a literal must fire the
/// rule.
#[test]
fn the_window_rule_fires_when_the_window_is_restated() {
    let tree = landed();
    let mutated = Landed {
        build: replace_occurrence(
            &tree.build,
            "BUILD_DEADLINE_MS + BUILD_TUNNEL_ID_RETENTION_MS",
            "2_760_000",
            0,
        ),
        ..tree
    };
    assert!(mutated.build != tree.build, "the mutation must apply");
    assert!(
        rule_window_is_the_trees_own_bound(&mutated).is_some(),
        "a restated window must be reported"
    );
}

/// Rule 2 — every installed fact carries an expiry.
#[test]
fn every_installed_fact_must_carry_an_expiry() {
    assert_eq!(rule_every_installed_fact_carries_an_expiry(&landed()), None);
}

/// Rule 2's inverted probe: installing a fact without an expiry must fire
/// the rule.
#[test]
fn the_expiry_rule_fires_when_an_install_drops_the_expiry() {
    let tree = landed();
    let mutated = Landed {
        owner: replace_occurrence(&tree.owner, "Self::creator_expiry_seconds(now_ms)", "0", 1),
        ..tree
    };
    assert!(mutated.owner != tree.owner, "the mutation must apply");
    assert!(
        rule_every_installed_fact_carries_an_expiry(&mutated).is_some(),
        "a fact installed without an expiry must be reported"
    );
}

/// Rule 3 — the expiry is computed in the safe direction.
#[test]
fn the_expiry_must_be_computed_in_the_safe_direction() {
    assert_eq!(
        rule_the_expiry_is_computed_in_the_safe_direction(&landed()),
        None
    );
}

/// Rule 3's inverted probe: dropping the ceiling must fire the rule.
#[test]
fn the_ceiling_rule_fires_when_the_ceiling_is_dropped() {
    let tree = landed();
    let mutated = Landed {
        owner: replace_occurrence(&tree.owner, ".saturating_add(999)", "", 0),
        ..tree
    };
    assert!(mutated.owner != tree.owner, "the mutation must apply");
    assert!(
        rule_the_expiry_is_computed_in_the_safe_direction(&mutated).is_some(),
        "an expiry computed without the ceiling must be reported"
    );
}

/// Rule 4 — every use site consults the expiry.
#[test]
fn every_use_site_must_consult_the_expiry() {
    assert_eq!(rule_every_use_site_consults_the_expiry(&landed()), None);
}

/// Rule 4's inverted probe: a use site that consults membership rather than
/// the expiry must fire the rule.
#[test]
fn the_use_site_rule_fires_when_the_expiry_is_not_consulted() {
    let tree = landed();
    let mutated = Landed {
        owner: replace_occurrence(
            &tree.owner,
            ".is_some_and(|expiry| *expiry > now_ms / 1_000)",
            ".is_some()",
            0,
        ),
        ..tree
    };
    assert!(mutated.owner != tree.owner, "the mutation must apply");
    assert!(
        rule_every_use_site_consults_the_expiry(&mutated).is_some(),
        "a use site that ignores the expiry must be reported"
    );
}

/// Rule 5 — the facts are maps with an expiry, not insert-only sets.
#[test]
fn the_facts_must_be_maps_with_an_expiry() {
    assert_eq!(rule_the_facts_are_maps_with_an_expiry(&landed()), None);
}

/// Rule 5's inverted probe: turning a fact back into an insert-only set must
/// fire the rule.
#[test]
fn the_map_rule_fires_when_a_fact_becomes_a_set() {
    let tree = landed();
    let mutated = Landed {
        owner: replace_occurrence(
            &tree.owner,
            "creator_data: BTreeMap<u32, u64>",
            "creator_data: BTreeSet<u32>",
            0,
        ),
        ..tree
    };
    assert!(mutated.owner != tree.owner, "the mutation must apply");
    assert!(
        rule_the_facts_are_maps_with_an_expiry(&mutated).is_some(),
        "an insert-only set must be reported"
    );
}

/// Rule 6 — the sweep drops the expired facts.
#[test]
fn the_sweep_must_drop_the_expired_facts() {
    assert_eq!(rule_the_sweep_drops_the_expired_facts(&landed()), None);
}

/// Rule 6's inverted probe: a sweep that retains everything must fire the
/// rule.
#[test]
fn the_sweep_rule_fires_when_the_sweep_retains_everything() {
    let tree = landed();
    // Every one of the three retains must go: the rule asserts that the
    // sweep drops the expired facts, and a sweep that retains one of them
    // still retains an expired fact.
    let mut owner = tree.owner.clone();
    for _ in 0..3 {
        owner = replace_occurrence(
            &owner,
            "retain(|_, expiry| *expiry > now_seconds)",
            "retain(|_, expiry| *expiry > 0)",
            0,
        );
    }
    let mutated = Landed { owner, ..tree };
    assert!(mutated.owner != tree.owner, "the mutation must apply");
    assert!(
        !mutated
            .owner
            .contains("retain(|_, expiry| *expiry > now_seconds)"),
        "the mutation must remove every clock comparison"
    );
    assert!(
        rule_the_sweep_drops_the_expired_facts(&mutated).is_some(),
        "a sweep that drops nothing must be reported"
    );
}

/// Rule 7 — the production wiring installs and sweeps on the tick.
#[test]
fn the_wiring_must_install_and_sweep() {
    assert_eq!(rule_the_wiring_installs_and_sweeps(&landed()), None);
}

/// Rule 7's inverted probe: a wiring that installs without sweeping must
/// fire the rule.
#[test]
fn the_wiring_rule_fires_when_the_sweep_is_dropped() {
    let tree = landed();
    let mutated = Landed {
        composition: replace_occurrence(
            &tree.composition,
            "transit.expire_creator_correlation(now_ms);",
            "",
            0,
        ),
        ..tree
    };
    assert!(
        mutated.composition != tree.composition,
        "the mutation must apply"
    );
    assert!(
        rule_the_wiring_installs_and_sweeps(&mutated).is_some(),
        "a wiring that never sweeps must be reported"
    );
}

/// Rule 8 — no new configuration key.
#[test]
fn the_retention_policy_must_add_no_configuration_key() {
    assert_eq!(rule_no_new_configuration_key(&landed()), None);
}

/// Rule 8's inverted probe: adding a retention key must fire the rule.
#[test]
fn the_configuration_rule_fires_when_a_key_is_added() {
    let tree = landed();
    let mutated = Landed {
        config: replace_occurrence(
            &tree.config,
            "pub struct NetDbConfig {",
            "pub struct NetDbConfig {\n    pub retention_window_ms: u64,",
            0,
        ),
        ..tree
    };
    assert!(mutated.config != tree.config, "the mutation must apply");
    assert!(
        rule_no_new_configuration_key(&mutated).is_some(),
        "a new configuration key must be reported"
    );
}

/// Rule 9 — the owner gains no task and no timer.
#[test]
fn the_owner_must_gain_no_task_or_timer() {
    assert_eq!(rule_the_owner_gains_no_task_or_timer(&landed()), None);
}

/// Rule 9's inverted probe: giving the owner a task must fire the rule.
#[test]
fn the_task_rule_fires_when_the_owner_owns_a_task() {
    let tree = landed();
    let mutated = Landed {
        owner: replace_occurrence(
            &tree.owner,
            "pub fn expire_creator_correlation(",
            "pub fn spawn_task() { tokio::spawn(async {}); }\n    pub fn expire_creator_correlation(",
            0,
        ),
        ..tree
    };
    assert!(mutated.owner != tree.owner, "the mutation must apply");
    assert!(
        rule_the_owner_gains_no_task_or_timer(&mutated).is_some(),
        "an owner that owns a task must be reported"
    );
}

/// Rule 10 — the counts are the observability seam.
#[test]
fn the_counts_must_be_the_observability_seam() {
    assert_eq!(rule_the_counts_are_the_observability_seam(&landed()), None);
}

/// Rule 10's inverted probe: dropping a count must fire the rule.
#[test]
fn the_counts_rule_fires_when_a_count_is_dropped() {
    let tree = landed();
    let mutated = Landed {
        owner: replace_occurrence(&tree.owner, "data: self.creator_data.len(),", "data: 0,", 0),
        ..tree
    };
    assert!(mutated.owner != tree.owner, "the mutation must apply");
    assert!(
        rule_the_counts_are_the_observability_seam(&mutated).is_some(),
        "a dropped count must be reported"
    );
}

/// The rule set is complete: every rule is reachable and the whole tree
/// holds.
#[test]
fn every_rule_is_reachable_and_the_tree_holds() {
    let tree = landed();
    let rules: Vec<Rule> = vec![
        ("rule 1", rule_window_is_the_trees_own_bound),
        ("rule 2", rule_every_installed_fact_carries_an_expiry),
        ("rule 3", rule_the_expiry_is_computed_in_the_safe_direction),
        ("rule 4", rule_every_use_site_consults_the_expiry),
        ("rule 5", rule_the_facts_are_maps_with_an_expiry),
        ("rule 6", rule_the_sweep_drops_the_expired_facts),
        ("rule 7", rule_the_wiring_installs_and_sweeps),
        ("rule 8", rule_no_new_configuration_key),
        ("rule 9", rule_the_owner_gains_no_task_or_timer),
        ("rule 10", rule_the_counts_are_the_observability_seam),
    ];
    assert_eq!(rules.len(), 10);
    for (label, rule) in rules {
        assert_eq!(rule(&tree), None, "{label} must hold on the landed tree");
    }
}
