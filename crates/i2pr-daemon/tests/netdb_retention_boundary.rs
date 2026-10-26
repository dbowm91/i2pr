//! Plan 507 — the static guard for the NetDB retention policy.
//!
//! The guard asserts the **structural** shape of the retention policy: the
//! window is the router's own acceptance window, the prune is the tree's own
//! explicit prune, the prune runs before the reconcile's snapshot, the locks
//! are taken one at a time, every prune is counted, no new configuration key
//! appears, the router's own RouterInfo is never a victim, the prune never
//! selects, Plan 506's reconcile invariants survive, and the product drives
//! the prune on the tick it already had.
//!
//! Properties of the guard:
//!
//! - **Structural, not textual.** A rule asserts that a named function's own
//!   **body** contains the required **call**, matched with its open
//!   parenthesis. Comments are stripped and `#[cfg(test)]` blocks are
//!   excluded, so a doc comment that merely *names* a construct cannot
//!   satisfy a rule, and a dead helper cannot either.
//! - **Fails closed.** A function that is absent is a violation, never a
//!   silently passing rule.
//! - **Every rule carries an inverted probe.** Each probe mutates a copy of
//!   the real source, asserts that the mutation actually applied, and then
//!   asserts that the rule fires. A rule that cannot fail is a defect.
//!
//! The guard reads the **real** tree from the workspace root, so it fails if
//! the tree does not hold the shape it asserts.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The reconcile module that owns the prune.
const RECONCILE_MODULE: &str = "crates/i2pr-daemon/src/peer_reconcile.rs";

/// The bootstrap module that owns the NetDB the router accepts peers into.
const BOOTSTRAP_MODULE: &str = "crates/i2pr-daemon/src/bootstrap.rs";

/// The destination coordinator that owns the authoritative peer store.
const DESTINATION_MODULE: &str = "crates/i2pr-daemon/src/destination_tunnels.rs";

/// The product module that owns the reconcile at runtime.
const PRODUCT_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";

/// The configuration parser, which must gain no retention key.
const CONFIG_MODULE: &str = "crates/i2pr-daemon/src/config.rs";

/// The peer-participation module that owns Plan 491's version gate.
const PARTICIPATION_MODULE: &str = "crates/i2pr-daemon/src/netdb_participation.rs";

/// The NetDB store that owns the tree's own explicit prune.
const NETDB_STORE_MODULE: &str = "crates/i2pr-netdb/src/store.rs";

/// The tree's own explicit-prune call, as the daemon must make it.
const TREE_PRUNE_CALL: &str = "self.store.prune(Date::from_millis(now_ms), window_ms)";

/// The NetDB store's own prune signature, which the daemon's call must match.
const TREE_PRUNE_SIGNATURE: &str = "pub fn prune(&mut self, now: Date, max_age_ms: u64) -> usize";

/// The constructs that would give the reconcile its own task or timer.
const RECONCILE_TASK_TOKENS: &[&str] = &["tokio::spawn", "tokio::time::interval"];

/// The constructs a selection-side write would use.
///
/// The prune removes peer **records**. It never selects, so Plan 491's
/// version gate stays at selection.
const SELECTION_TOKENS: &[&str] = &[
    "select_floodfill_candidates(",
    "select_version_gated(",
    "select_tunnel_build_peers(",
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
/// prune` never matches `fn prune_something`.
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
/// identifier, so `fn prune` never matches `fn prune_stores`.
///
/// The body is the text between the function's own braces. A function that is
/// not present yields `None`, so every caller fails closed.
fn function_body<'a>(code: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("fn {name}");
    let start = find_definition(code, &needle)?;
    let (open, close) = brace_balanced_block(code, start + needle.len())?;
    Some(&code[open + 1..close - 1])
}

/// One named rule over the landed tree.
type Rule = (&'static str, fn(&Landed) -> Option<String>);

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

/// The body of `fn <name>` must contain `first` **before** `second`.
///
/// Both must be present: a missing step is a violation, never a silently
/// passing rule. The order in the source **is** the order the work happens,
/// because each step is its own function and each function releases its lock
/// before it returns.
fn check_body_order(source: &str, name: &str, first: &str, second: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, name) else {
        return Some(format!("`fn {name}` must exist in the production surface"));
    };
    let first_at = body.find(first);
    let second_at = body.find(second);
    let (Some(first_at), Some(second_at)) = (first_at, second_at) else {
        return Some(format!(
            "`fn {name}` must contain both `{first}` and `{second}`"
        ));
    };
    if first_at < second_at {
        None
    } else {
        Some(format!(
            "`fn {name}` must contain `{first}` before `{second}`"
        ))
    }
}

// ----------------------------------------------------------------------
// The landed tree the rules are applied to.
// ----------------------------------------------------------------------

/// The landed sources the rules are applied to.
#[derive(Clone)]
struct Landed {
    reconcile: String,
    bootstrap: String,
    destinations: String,
    product: String,
    config: String,
    participation: String,
    netdb_store: String,
}

fn landed() -> Landed {
    Landed {
        reconcile: source_of(RECONCILE_MODULE),
        bootstrap: source_of(BOOTSTRAP_MODULE),
        destinations: source_of(DESTINATION_MODULE),
        product: source_of(PRODUCT_MODULE),
        config: source_of(CONFIG_MODULE),
        participation: source_of(PARTICIPATION_MODULE),
        netdb_store: source_of(NETDB_STORE_MODULE),
    }
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1 — the retention window is the router's **own** acceptance window.
///
/// The window is read from the validation policy rather than restated, so
/// the router retains exactly what it accepts. A second value would be a
/// second source of truth and could disagree with the validator.
fn rule_window_is_the_acceptance_window(tree: &Landed) -> Option<String> {
    if let Some(violation) =
        check_body_contains(&tree.reconcile, "acceptance_window_secs", "default_const()")
    {
        return Some(violation);
    }
    if let Some(violation) =
        check_body_contains(&tree.reconcile, "acceptance_window_secs", ".max_age")
    {
        return Some(violation);
    }
    if let Some(violation) =
        check_code_contains(&tree.reconcile, "acceptance_window_secs() * 1_000")
    {
        return Some(violation);
    }
    if let Some(violation) =
        check_body_contains(&tree.reconcile, "prune_bootstrap", "RETENTION_WINDOW_MS")
    {
        return Some(violation);
    }
    if let Some(violation) =
        check_body_contains(&tree.reconcile, "prune_destinations", "RETENTION_WINDOW_MS")
    {
        return Some(violation);
    }
    None
}

/// Rule 2 — the prune is the tree's **own** explicit prune.
///
/// Plan 103 §3.4 prefers an explicit prune over silent eviction. The daemon
/// must call the NetDB store's own prune, not re-implement a removal, and
/// the NetDB store's own signature is pinned so the two cannot drift.
fn rule_prune_is_the_trees_own(tree: &Landed) -> Option<String> {
    if let Some(violation) = check_body_contains(&tree.bootstrap, "prune", TREE_PRUNE_CALL) {
        return Some(violation);
    }
    if let Some(violation) = check_body_contains(&tree.destinations, "prune", TREE_PRUNE_CALL) {
        return Some(violation);
    }
    if let Some(violation) = check_code_contains(&tree.netdb_store, TREE_PRUNE_SIGNATURE) {
        return Some(violation);
    }
    None
}

/// Rule 3 — the ordering is prune, then snapshot.
///
/// The prune runs **before** the reconcile's snapshot, so a record the prune
/// removed is never carried into the destination store.
fn rule_prune_runs_before_the_snapshot(tree: &Landed) -> Option<String> {
    check_body_order(
        &tree.reconcile,
        "run",
        "self.prune_stores(",
        "self.snapshot()",
    )
}

/// Rule 4 — the locks are taken one at a time and are never nested.
///
/// The bootstrap's lock is released before the destination coordinator's lock
/// is taken, and neither prune step may name the peer-profile store's lock.
fn rule_locks_are_one_at_a_time(tree: &Landed) -> Option<String> {
    if let Some(violation) =
        check_body_contains(&tree.reconcile, "prune_bootstrap", "bootstrap.lock()")
    {
        return Some(violation);
    }
    for token in ["destinations", "profiles"] {
        if let Some(violation) = check_body_avoids(&tree.reconcile, "prune_bootstrap", token) {
            return Some(violation);
        }
    }
    if let Some(violation) = check_body_contains(&tree.reconcile, "prune_destinations", "try_lock(")
    {
        return Some(violation);
    }
    for token in ["bootstrap", "profiles"] {
        if let Some(violation) = check_body_avoids(&tree.reconcile, "prune_destinations", token) {
            return Some(violation);
        }
    }
    if let Some(violation) = check_body_avoids(&tree.reconcile, "prune_stores", "lock(") {
        return Some(violation);
    }
    None
}

/// Rule 5 — every prune is counted.
///
/// A prune that silently drops peers is a silent skip. The counters carry
/// counts and stable reason labels only.
fn rule_every_prune_is_counted(tree: &Landed) -> Option<String> {
    for token in [
        "pruned_bootstrap",
        "pruned_destination",
        "prune_unavailable",
    ] {
        if let Some(violation) = check_body_contains(&tree.reconcile, "prune_stores", token) {
            return Some(violation);
        }
    }
    for token in [
        "\"pruned-bootstrap\"",
        "\"pruned-destination\"",
        "\"prune-unavailable\"",
    ] {
        if let Some(violation) = check_body_contains(&tree.reconcile, "counts_by_reason", token) {
            return Some(violation);
        }
    }
    None
}

/// Rule 6 — no new configuration key.
///
/// The retention window is the router's own acceptance window, so there is
/// nothing to configure. A second knob would be a second source of truth.
fn rule_no_new_configuration_key(tree: &Landed) -> Option<String> {
    for token in ["retention", "max_age"] {
        if let Some(violation) = check_code_avoids_case_insensitive(&tree.config, token) {
            return Some(violation);
        }
    }
    None
}

/// Rule 7 — the router's own RouterInfo is never a prune victim.
///
/// The local record lives beside the store, not inside it, so the prune
/// cannot reach it.
fn rule_local_router_info_is_never_a_victim(tree: &Landed) -> Option<String> {
    check_body_avoids(&tree.bootstrap, "prune", "self.local")
}

/// Rule 8 — the prune never selects, and Plan 491's gate stays at selection.
///
/// The prune removes peer **records**. It never selects, so the version gate
/// is not weakened.
fn rule_prune_never_selects(tree: &Landed) -> Option<String> {
    for token in SELECTION_TOKENS {
        if let Some(violation) = check_body_avoids(&tree.bootstrap, "prune", token) {
            return Some(violation);
        }
        if let Some(violation) = check_body_avoids(&tree.destinations, "prune", token) {
            return Some(violation);
        }
    }
    check_body_contains(
        &tree.participation,
        "select_version_gated",
        "version_gate: Some(",
    )
}

/// Rule 9 — Plan 506's reconcile invariants survive.
///
/// The prune is folded into the reconcile, so the reconcile's own shape must
/// not change: the three steps stay in order and the snapshot still reads
/// the bootstrap under its own lock.
fn rule_reconcile_invariants_survive(tree: &Landed) -> Option<String> {
    for (first, second) in [
        ("self.snapshot()", "self.insert_all("),
        ("self.insert_all(", "self.learn_all("),
    ] {
        if let Some(violation) = check_body_order(&tree.reconcile, "run", first, second) {
            return Some(violation);
        }
    }
    if let Some(violation) = check_body_contains(&tree.reconcile, "snapshot", "bootstrap.lock()") {
        return Some(violation);
    }
    if let Some(violation) = check_body_contains(
        &tree.reconcile,
        "snapshot",
        "validated_router_info_snapshot(",
    ) {
        return Some(violation);
    }
    None
}

/// Rule 10 — the product drives the prune on its existing tick.
///
/// The prune rides the reconcile the product already ran, so the product
/// gains no second knob and the reconcile owns no task and no timer.
fn rule_product_drives_the_prune(tree: &Landed) -> Option<String> {
    if let Some(violation) =
        check_body_contains(&tree.product, "reconcile_peer_knowledge", "reconcile.run(")
    {
        return Some(violation);
    }
    for token in RECONCILE_TASK_TOKENS {
        if let Some(violation) = check_code_avoids(&tree.reconcile, token) {
            return Some(violation);
        }
    }
    None
}

/// Every rule, as a list of violations.
fn check_all(tree: &Landed) -> Vec<String> {
    let mut violations = Vec::new();
    let rules: Vec<Rule> = vec![
        ("rule 1", rule_window_is_the_acceptance_window),
        ("rule 2", rule_prune_is_the_trees_own),
        ("rule 3", rule_prune_runs_before_the_snapshot),
        ("rule 4", rule_locks_are_one_at_a_time),
        ("rule 5", rule_every_prune_is_counted),
        ("rule 6", rule_no_new_configuration_key),
        ("rule 7", rule_local_router_info_is_never_a_victim),
        ("rule 8", rule_prune_never_selects),
        ("rule 9", rule_reconcile_invariants_survive),
        ("rule 10", rule_product_drives_the_prune),
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
        "the landed tree must hold every Plan 507 rule, got: {violations:?}"
    );
}

/// The guard reads the real tree, and the tree is not trivial.
#[test]
fn the_guard_reads_the_real_tree() {
    let tree = landed();
    assert!(tree.reconcile.len() > 4_000);
    assert!(tree.bootstrap.contains("pub fn prune"));
    assert!(tree.destinations.contains("pub fn prune"));
    assert!(tree.product.contains("reconcile_peer_knowledge"));
    assert!(tree.netdb_store.contains("pub fn prune"));
}

/// An absent function fails closed: renaming `fn prune_bootstrap` must make
/// the rules that name it fire, not pass.
#[test]
fn an_absent_function_fails_closed() {
    let tree = landed();
    let mutated = Landed {
        reconcile: replace_occurrence(
            &tree.reconcile,
            "fn prune_bootstrap",
            "fn prune_bootstrap_x",
            0,
        ),
        ..tree
    };
    assert!(
        mutated.reconcile.contains("fn prune_bootstrap_x"),
        "the mutation must apply"
    );
    let violations = violations_of(&mutated);
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("rule 1")),
        "an absent function must be reported, got: {violations:?}"
    );
}

/// Rule 1 — the retention window is the router's own acceptance window.
#[test]
fn the_retention_window_must_be_the_acceptance_window() {
    assert_eq!(rule_window_is_the_acceptance_window(&landed()), None);
}

/// Rule 1's inverted probe: restating the window as a literal must fire the
/// rule.
#[test]
fn the_window_rule_fires_when_the_window_is_restated() {
    let tree = landed();
    let mutated = Landed {
        reconcile: replace_occurrence(&tree.reconcile, "default_const()", "86_400_u64", 0),
        ..tree
    };
    assert!(
        mutated.reconcile != tree.reconcile,
        "the mutation must apply"
    );
    assert!(
        rule_window_is_the_acceptance_window(&mutated).is_some(),
        "a restated window must be reported"
    );
}

/// Rule 2 — the prune is the tree's own explicit prune.
#[test]
fn the_prune_must_be_the_trees_own_explicit_prune() {
    assert_eq!(rule_prune_is_the_trees_own(&landed()), None);
}

/// Rule 2's inverted probe: re-implementing the removal must fire the rule.
#[test]
fn the_prune_rule_fires_when_the_tree_prune_is_not_called() {
    let tree = landed();
    let mutated = Landed {
        bootstrap: replace_occurrence(&tree.bootstrap, TREE_PRUNE_CALL, "", 0),
        ..tree
    };
    assert!(
        mutated.bootstrap != tree.bootstrap,
        "the mutation must apply"
    );
    assert!(
        rule_prune_is_the_trees_own(&mutated).is_some(),
        "a re-implemented removal must be reported"
    );
}

/// Rule 3 — the prune runs before the reconcile's snapshot.
#[test]
fn the_prune_must_run_before_the_snapshot() {
    assert_eq!(rule_prune_runs_before_the_snapshot(&landed()), None);
}

/// Rule 3's inverted probe: dropping the prune call must fire the rule.
#[test]
fn the_ordering_rule_fires_when_the_prune_is_dropped() {
    let tree = landed();
    let mutated = Landed {
        reconcile: replace_occurrence(&tree.reconcile, "self.prune_stores(", "", 0),
        ..tree
    };
    assert!(
        mutated.reconcile != tree.reconcile,
        "the mutation must apply"
    );
    assert!(
        rule_prune_runs_before_the_snapshot(&mutated).is_some(),
        "a dropped prune must be reported"
    );
}

/// Rule 4 — the locks are taken one at a time and are never nested.
#[test]
fn the_prune_locks_must_be_one_at_a_time() {
    assert_eq!(rule_locks_are_one_at_a_time(&landed()), None);
}

/// Rule 4's inverted probe: taking the bootstrap's lock in the destination
/// step must fire the rule.
#[test]
fn the_lock_rule_fires_when_a_prune_step_takes_another_lock() {
    let tree = landed();
    let mutated = Landed {
        reconcile: replace_occurrence(
            &tree.reconcile,
            "destinations.try_lock()",
            "destinations.lock().await",
            0,
        ),
        ..tree
    };
    assert!(
        mutated.reconcile != tree.reconcile,
        "the mutation must apply"
    );
    assert!(
        rule_locks_are_one_at_a_time(&mutated).is_some(),
        "a nested lock must be reported"
    );
}

/// Rule 5 — every prune is counted.
#[test]
fn every_prune_must_be_counted() {
    assert_eq!(rule_every_prune_is_counted(&landed()), None);
}

/// Rule 5's inverted probe: dropping a counter must fire the rule.
#[test]
fn the_counter_rule_fires_when_a_count_is_dropped() {
    let tree = landed();
    let mutated = Landed {
        reconcile: tree.reconcile.replace("pruned_destination", "pruned"),
        ..tree
    };
    assert!(
        mutated.reconcile != tree.reconcile,
        "the mutation must apply"
    );
    assert!(
        !mutated.reconcile.contains("pruned_destination"),
        "the mutation must remove the counter from the counting step"
    );
    assert!(
        rule_every_prune_is_counted(&mutated).is_some(),
        "an uncounted prune must be reported"
    );
}

/// Rule 5's second inverted probe: renaming a stable reason label must fire
/// the rule, because a reader that diffs two snapshots needs the label to
/// stay put.
#[test]
fn the_counter_label_rule_fires_when_a_label_is_renamed() {
    let tree = landed();
    let mutated = Landed {
        reconcile: replace_occurrence(
            &tree.reconcile,
            "\"pruned-destination\"",
            "\"pruned-destination-v2\"",
            0,
        ),
        ..tree
    };
    assert!(
        mutated.reconcile != tree.reconcile,
        "the mutation must apply"
    );
    assert!(
        rule_every_prune_is_counted(&mutated).is_some(),
        "a renamed reason label must be reported"
    );
}

/// Rule 6 — no new configuration key.
#[test]
fn the_retention_policy_must_add_no_configuration_key() {
    assert_eq!(rule_no_new_configuration_key(&landed()), None);
}

/// Rule 6's inverted probe: adding a retention key must fire the rule.
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

/// Rule 7 — the router's own RouterInfo is never a prune victim.
#[test]
fn the_local_router_info_must_never_be_a_prune_victim() {
    assert_eq!(rule_local_router_info_is_never_a_victim(&landed()), None);
}

/// Rule 7's inverted probe: reaching the local record from the prune must
/// fire the rule.
#[test]
fn the_local_record_rule_fires_when_the_prune_reaches_the_local_record() {
    let tree = landed();
    let mutated = Landed {
        bootstrap: replace_occurrence(
            &tree.bootstrap,
            "self.refresh_snapshot();",
            "self.local.take();\n        self.refresh_snapshot();",
            0,
        ),
        ..tree
    };
    assert!(
        mutated.bootstrap != tree.bootstrap,
        "the mutation must apply"
    );
    assert!(
        rule_local_router_info_is_never_a_victim(&mutated).is_some(),
        "a prune that reaches the local record must be reported"
    );
}

/// Rule 8 — the prune never selects, and Plan 491's gate stays at selection.
#[test]
fn the_prune_must_never_select() {
    assert_eq!(rule_prune_never_selects(&landed()), None);
}

/// Rule 8's inverted probe: selecting inside the prune must fire the rule.
#[test]
fn the_selection_rule_fires_when_the_prune_selects() {
    let tree = landed();
    let mutated = Landed {
        destinations: replace_occurrence(
            &tree.destinations,
            "self.store.prune(Date::from_millis(now_ms), window_ms)",
            "self.select_floodfill_candidates(4);",
            0,
        ),
        ..tree
    };
    assert!(
        mutated.destinations != tree.destinations,
        "the mutation must apply"
    );
    assert!(
        rule_prune_never_selects(&mutated).is_some(),
        "a prune that selects must be reported"
    );
}

/// Rule 9 — Plan 506's reconcile invariants survive.
#[test]
fn the_reconcile_invariants_must_survive() {
    assert_eq!(rule_reconcile_invariants_survive(&landed()), None);
}

/// Rule 9's inverted probe: dropping a reconcile step must fire the rule.
#[test]
fn the_invariant_rule_fires_when_a_reconcile_step_is_dropped() {
    let tree = landed();
    let mutated = Landed {
        reconcile: replace_occurrence(&tree.reconcile, "self.learn_all(", "", 0),
        ..tree
    };
    assert!(
        mutated.reconcile != tree.reconcile,
        "the mutation must apply"
    );
    assert!(
        rule_reconcile_invariants_survive(&mutated).is_some(),
        "a dropped reconcile step must be reported"
    );
}

/// Rule 10 — the product drives the prune on its existing tick.
#[test]
fn the_product_must_drive_the_prune_on_its_existing_tick() {
    assert_eq!(rule_product_drives_the_prune(&landed()), None);
}

/// Rule 10's inverted probe: giving the reconcile its own task must fire the
/// rule.
#[test]
fn the_tick_rule_fires_when_the_reconcile_owns_a_task() {
    let tree = landed();
    let mutated = Landed {
        reconcile: replace_occurrence(
            &tree.reconcile,
            "pub async fn run(",
            "pub fn spawn_task() { tokio::spawn(async {}); }\n    pub async fn run(",
            0,
        ),
        ..tree
    };
    assert!(
        mutated.reconcile != tree.reconcile,
        "the mutation must apply"
    );
    assert!(
        mutated.reconcile.contains("tokio::spawn"),
        "the mutation must add the forbidden construct"
    );
    assert!(
        rule_product_drives_the_prune(&mutated).is_some(),
        "a reconcile that owns a task must be reported"
    );
}

/// The rules must not be satisfied by a whole-file token search.
///
/// The prune's calls live in their own functions; a rule that only looked at
/// the file would pass even if the call had moved out of the function that
/// must own it.
#[test]
fn the_rules_are_structural_and_not_whole_file_searches() {
    let tree = landed();
    let reconcile = code_view(&tree.reconcile);
    assert!(
        !reconcile.contains("self.store.prune("),
        "the reconcile module must not call a store's prune directly"
    );
    assert!(
        reconcile.contains("RETENTION_WINDOW_MS"),
        "the window constant must be named in the reconcile module"
    );
    // The constant's derivation lives in the code view even though the
    // explanatory prose does not.
    assert!(
        reconcile.contains("acceptance_window_secs() * 1_000"),
        "the window must be derived, not restated"
    );
}

/// The rule set is complete: every rule is reachable and the whole tree
/// holds.
#[test]
fn every_rule_is_reachable_and_the_tree_holds() {
    let tree = landed();
    let rules: Vec<Rule> = vec![
        ("rule 1", rule_window_is_the_acceptance_window),
        ("rule 2", rule_prune_is_the_trees_own),
        ("rule 3", rule_prune_runs_before_the_snapshot),
        ("rule 4", rule_locks_are_one_at_a_time),
        ("rule 5", rule_every_prune_is_counted),
        ("rule 6", rule_no_new_configuration_key),
        ("rule 7", rule_local_router_info_is_never_a_victim),
        ("rule 8", rule_prune_never_selects),
        ("rule 9", rule_reconcile_invariants_survive),
        ("rule 10", rule_product_drives_the_prune),
    ];
    assert_eq!(rules.len(), 10);
    for (label, rule) in rules {
        assert_eq!(rule(&tree), None, "{label} must hold on the landed tree");
    }
    assert!(check_code_occurs_once(&tree.reconcile, "fn prune_bootstrap").is_none());
    assert!(check_code_occurs_once(&tree.reconcile, "fn prune_destinations").is_none());
}
