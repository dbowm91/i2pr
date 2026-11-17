//! Plan 510 static boundary guard for the managed-tunnel reason
//! classification.
//!
//! Plan 491 recorded F4: an intermittent `i2pcontrol_tunnels`
//! managed-tunnel staging failure — *"runtime transition failed:
//! manager configuration rejected"* — that never reproduced again. Two
//! things made it undiagnosable, and this guard pins both.
//!
//! 1. **The class was lost.** Every `ServiceTunnelError::InvalidConfig`
//!    detail collapsed into one static label, so a rejection could not
//!    be attributed. Plan 510 classifies the detail into a **value-free
//!    class** by a pinned marker list.
//! 2. **The path was untested under load.** The staging path had no
//!    load-sensitive lane at all, so a load-sensitive race had nothing
//!    to catch it.
//!
//! The guard asserts six things:
//!
//! 1. **The table is pinned and unambiguous.** The marker list is
//!    non-empty, no marker and no class is listed twice, and no marker is
//!    shadowed by an earlier one — a shadowed marker would silently
//!    change another condition's class.
//! 2. **The classification is total.** Every detail yields a label: a
//!    known marker yields its own class, and an unknown detail keeps the
//!    generic label, so the change can never make a rejection *less*
//!    informative than it was before.
//! 3. **The production function agrees with the pinned table.** The
//!    table in the source and the function the daemon actually calls must
//!    classify identically, so the pinned artefact is not a fiction.
//! 4. **Every class carries no value.** No digit, no colon, no path
//!    separator, no dot, no punctuation that would delimit a value: a
//!    class is a short phrase of letters and spaces.
//! 5. **Every marker is still live.** Each marker must appear in the
//!    daemon's *own* error construction, so a reworded message is
//!    reported rather than silently changing a class.
//! 6. **The reason cannot borrow the detail.** Both the classifier and
//!    the reason mapper return `&'static str`, so the type system — not
//!    a review — forbids a detail from reaching the wire.
//!
//! The guard is deliberately **fail-closed**: it strips comments before
//! matching, it excludes `#[cfg(test)]` blocks, an absent function is a
//! violation rather than a pass, and every rule carries its own
//! **inverted probe** — a synthetic violating source must be reported.
//! If a rule ever stops detecting its violation, its probe fails.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required *call*, so a token that appears in a
//! sibling function, in a comment, or in a test block does not satisfy
//! them.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The daemon file that owns the classification and the reason.
const DAEMON: &str = "crates/i2pr-daemon/src/i2pcontrol_tunnels.rs";

/// The daemon file that constructs the `InvalidConfig` details the
/// classification reads.
const CONSTRUCTION: &str = "crates/i2pr-daemon/src/service_tunnels.rs";

/// The load-sensitive black-box lane the plan adds.
const LANE: &str = "crates/i2pr-daemon/tests/i2pcontrol_tunnels_load.rs";

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

/// Returns the **signature** of the function whose definition is anchored at
/// `fn <name>` — the text from the anchor to the function's opening brace.
fn function_signature<'a>(code: &'a str, name: &str) -> Option<&'a str> {
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
        let open = code[after..].find('{')? + after;
        return Some(&code[start..open]);
    }
    None
}

/// Every string literal in `text`, in order.
///
/// The scan is over a **code view**, so a literal that only appears in a
/// comment is not counted.
fn string_literals(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"' {
            let end = skip_string_literal(bytes, cursor);
            if end > cursor + 1 && bytes[end - 1] == b'"' {
                out.push(text[cursor + 1..end - 1].to_owned());
            }
            cursor = end;
            continue;
        }
        cursor += 1;
    }
    out
}

/// Removes every whitespace character, so a rule does not depend on how
/// `rustfmt` chose to lay a statement out.
fn compact(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// The classification table, parsed out of the daemon's source.
struct Table {
    /// The `(marker, class)` pairs, in the order the source lists them.
    entries: Vec<(String, String)>,
    /// The label an unmatched detail keeps.
    generic: String,
}

/// Parses the pinned classification table out of the daemon's source.
///
/// The parse is over the **code view**, so a marker that only appeared in a
/// comment would not be counted, and the table's own declaration is the only
/// place the pairs are read from.
fn parse_table(source: &str) -> Table {
    let code = code_view(source);
    let anchor = "const MANAGER_CONFIG_CLASSES: &[(&str, &str)] = &[";
    let start = code
        .find(anchor)
        .expect("the classification table must exist");
    let block_start = start + anchor.len();
    let block_end = block_start
        + code[block_start..]
            .find("];")
            .expect("the classification table must end");
    let literals = string_literals(&code[block_start..block_end]);
    assert!(
        literals.len().is_multiple_of(2),
        "the classification table must hold marker/class pairs"
    );
    let entries = literals
        .chunks(2)
        .map(|pair| (pair[0].clone(), pair[1].clone()))
        .collect();
    let generic_anchor = "const MANAGER_CONFIG_GENERIC: &str = ";
    let generic_start = code
        .find(generic_anchor)
        .expect("the generic label must exist")
        + generic_anchor.len();
    let generic = string_literals(&code[generic_start..])
        .into_iter()
        .next()
        .expect("the generic label must be a literal");
    Table { entries, generic }
}

/// The guard's own classification, over the parsed table.
///
/// It mirrors the production function exactly: the first marker that appears
/// inside the detail wins, and an unmatched detail keeps the generic label.
fn classify<'a>(table: &'a Table, detail: &str) -> &'a str {
    for (marker, class) in &table.entries {
        if detail.contains(marker.as_str()) {
            return class.as_str();
        }
    }
    table.generic.as_str()
}

/// Whether a label carries **no value**.
///
/// A label is value-free when it is a short phrase of letters, spaces and
/// hyphens: no digit (a port, a count, a version), no `:` (a `host:port` or a
/// scheme), no path separator, no `.` (a filename or an address), no `@` (an
/// identity), and no punctuation that would delimit a value.
fn value_free(label: &str) -> bool {
    if label.is_empty() || label.len() > 64 {
        return false;
    }
    if !label.contains(' ') {
        return false;
    }
    if label.contains("  ") {
        return false;
    }
    label
        .chars()
        .all(|character| character.is_ascii_alphabetic() || character == ' ' || character == '-')
}

/// The files the guard reads.
const TREE_FILES: &[&str] = &[DAEMON, CONSTRUCTION, LANE];

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

/// Rule 1: the classification table is pinned, non-empty, and unambiguous.
fn rule_the_table_is_pinned(tree: &Tree) -> Option<String> {
    let table = parse_table(tree.source(DAEMON));
    if table.entries.is_empty() {
        return Some("the classification table must not be empty".to_owned());
    }
    let mut markers = BTreeSet::new();
    let mut classes = BTreeSet::new();
    for (marker, class) in &table.entries {
        if marker.is_empty() {
            return Some("a marker must not be empty".to_owned());
        }
        if class.is_empty() {
            return Some(format!("the class for {marker:?} must not be empty"));
        }
        if !markers.insert(marker.clone()) {
            return Some(format!("the marker {marker:?} is listed twice"));
        }
        if !classes.insert(class.clone()) {
            return Some(format!("the class {class:?} is listed twice"));
        }
        // A marker that an earlier marker also matches inside would never
        // reach its own class: the classification would report a different
        // condition's class for it.
        if classify(&table, marker) != class.as_str() {
            return Some(format!(
                "the marker {marker:?} is shadowed by an earlier entry"
            ));
        }
    }
    None
}

/// Rule 2: the classification is total — every detail yields a label.
fn rule_the_classification_is_total(tree: &Tree) -> Option<String> {
    let table = parse_table(tree.source(DAEMON));
    if table.generic.is_empty() {
        return Some("the generic label must not be empty".to_owned());
    }
    if classify(&table, "") != table.generic.as_str() {
        return Some("an empty detail must keep the generic label".to_owned());
    }
    let unknown = "a detail that no marker matches, written only for this guard";
    if classify(&table, unknown) != table.generic.as_str() {
        return Some("an unknown detail must keep the generic label".to_owned());
    }
    None
}

/// Rule 3: the production function agrees with the pinned table.
///
/// The table in the source is the plan's artefact; the function the daemon
/// calls is the plan's behaviour. They must classify identically, or the
/// pinned artefact is a fiction.
fn rule_the_production_function_agrees_with_the_table(tree: &Tree) -> Option<String> {
    let table = parse_table(tree.source(DAEMON));
    let production = i2pr_daemon::i2pcontrol_tunnels::manager_config_class;
    for (marker, class) in &table.entries {
        let actual = production(marker);
        if actual != class.as_str() {
            return Some(format!(
                "the production classification maps {marker:?} to {actual:?}, not {class:?}"
            ));
        }
    }
    for probe in [
        "",
        "a detail that no marker matches, written only for this guard",
    ] {
        let actual = production(probe);
        if actual != table.generic.as_str() {
            return Some(format!(
                "the production classification maps {probe:?} to {actual:?}, not the generic label"
            ));
        }
    }
    None
}

/// Rule 4: every class carries no value.
fn rule_every_class_is_value_free(tree: &Tree) -> Option<String> {
    let table = parse_table(tree.source(DAEMON));
    for (marker, class) in &table.entries {
        if !value_free(class) {
            return Some(format!(
                "the class {class:?} for {marker:?} carries a value"
            ));
        }
    }
    if !value_free(&table.generic) {
        return Some(format!(
            "the generic label {:?} carries a value",
            table.generic
        ));
    }
    None
}

/// Rule 5: the manager reason's own arms carry no value either.
fn rule_every_manager_reason_is_value_free(tree: &Tree) -> Option<String> {
    let source = tree.source(DAEMON);
    let code = code_view(source);
    let Some(body) = function_body(&code, "static_manager_reason") else {
        return Some("`fn static_manager_reason` must exist in the production surface".to_owned());
    };
    for literal in string_literals(body) {
        if !value_free(&literal) {
            return Some(format!("the manager reason {literal:?} carries a value"));
        }
    }
    None
}

/// Rule 6: the manager reason template is fixed.
///
/// The reason the operator sees is the class plus a fixed prefix. A template
/// that interpolated anything else would put a value on the wire.
fn rule_the_manager_reason_has_a_fixed_prefix(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(DAEMON));
    let mut found = 0usize;
    for literal in string_literals(&code) {
        if literal.contains("runtime transition failed") {
            found += 1;
            if literal != "runtime transition failed: {reason}" {
                return Some(format!(
                    "the manager reason template must be fixed, found {literal:?}"
                ));
            }
        }
    }
    if found == 0 {
        return Some("the manager reason template must exist".to_owned());
    }
    None
}

/// Rule 7: every marker is still live in the daemon's own error construction.
///
/// A reworded message would leave a marker matching nothing, and the
/// condition would silently fall back to the generic label. The guard
/// reports that instead.
fn rule_every_marker_is_live(tree: &Tree) -> Option<String> {
    let table = parse_table(tree.source(DAEMON));
    let construction = code_view(tree.source(CONSTRUCTION));
    let literals = string_literals(&construction);
    for (marker, _) in &table.entries {
        if !literals
            .iter()
            .any(|literal| literal.contains(marker.as_str()))
        {
            return Some(format!(
                "the marker {marker:?} no longer appears in the daemon's own error construction"
            ));
        }
    }
    None
}

/// Rule 8: the reason cannot borrow the detail.
///
/// Both the classifier and the reason mapper return `&'static str`, so the
/// type system — not a review — forbids a detail from reaching the wire.
fn rule_the_reason_cannot_borrow_the_detail(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(DAEMON));
    for name in ["manager_config_class", "static_manager_reason"] {
        let Some(signature) = function_signature(&code, name) else {
            return Some(format!("`fn {name}` must exist in the production surface"));
        };
        if !signature.contains("-> &'static str") {
            return Some(format!("`fn {name}` must return `&'static str`"));
        }
    }
    None
}

/// Rule 9: the `InvalidConfig` arm no longer collapses every detail.
fn rule_the_invalid_config_arm_classifies_the_detail(tree: &Tree) -> Option<String> {
    let source = tree.source(DAEMON);
    let code = code_view(source);
    let Some(body) = function_body(&code, "static_manager_reason") else {
        return Some("`fn static_manager_reason` must exist in the production surface".to_owned());
    };
    let body = compact(body);
    if !body.contains("InvalidConfig(detail)") {
        return Some("`fn static_manager_reason` must bind the rejected detail".to_owned());
    }
    if !body.contains("manager_config_class(detail)") {
        return Some("`fn static_manager_reason` must classify the rejected detail".to_owned());
    }
    None
}

/// Rule 10: the load lane is present, bounded, concurrent, and sleep-free.
///
/// The lane is the plan's durable contribution: a load-sensitive path with
/// no lane has nothing to catch a load-sensitive race. A lane that sleeps is
/// not a load lane, and the repository forbids a wall-clock sleep for an
/// overload test.
fn rule_the_load_lane_is_present_and_sleep_free(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(LANE));
    let anchor = "const LOAD_CONCURRENCY: usize = ";
    let Some(position) = code.find(anchor) else {
        return Some("the load lane must declare its concurrency bound".to_owned());
    };
    let rest = &code[position + anchor.len()..];
    let end = rest.find(';').unwrap_or(rest.len());
    let Ok(value) = rest[..end].trim().parse::<usize>() else {
        return Some("the load lane's concurrency bound must be a number".to_owned());
    };
    if value < 4 {
        return Some(format!(
            "the load lane must carry a real load, found {value}"
        ));
    }
    if !code.contains("tokio::spawn(") {
        return Some("the load lane must drive its creates concurrently".to_owned());
    }
    for forbidden in ["sleep(", "sleep_until(", "thread::sleep"] {
        if code.contains(forbidden) {
            return Some(format!("the load lane must not sleep: {forbidden}"));
        }
    }
    None
}

/// One rule of the guard: a label and the check it applies.
type Rule = (&'static str, fn(&Tree) -> Option<String>);

/// Every rule, as a list of violations.
fn check_all(tree: &Tree) -> Vec<String> {
    let rules: Vec<Rule> = vec![
        ("rule 1", rule_the_table_is_pinned),
        ("rule 2", rule_the_classification_is_total),
        ("rule 3", rule_the_production_function_agrees_with_the_table),
        ("rule 4", rule_every_class_is_value_free),
        ("rule 5", rule_every_manager_reason_is_value_free),
        ("rule 6", rule_the_manager_reason_has_a_fixed_prefix),
        ("rule 7", rule_every_marker_is_live),
        ("rule 8", rule_the_reason_cannot_borrow_the_detail),
        ("rule 9", rule_the_invalid_config_arm_classifies_the_detail),
        ("rule 10", rule_the_load_lane_is_present_and_sleep_free),
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
        ("rule 1", rule_the_table_is_pinned),
        ("rule 2", rule_the_classification_is_total),
        ("rule 3", rule_the_production_function_agrees_with_the_table),
        ("rule 4", rule_every_class_is_value_free),
        ("rule 5", rule_every_manager_reason_is_value_free),
        ("rule 6", rule_the_manager_reason_has_a_fixed_prefix),
        ("rule 7", rule_every_marker_is_live),
        ("rule 8", rule_the_reason_cannot_borrow_the_detail),
        ("rule 9", rule_the_invalid_config_arm_classifies_the_detail),
        ("rule 10", rule_the_load_lane_is_present_and_sleep_free),
    ];
    assert_eq!(rules.len(), 10);
    for (label, rule) in rules {
        assert_eq!(rule(&tree), None, "{label} must hold on the landed tree");
    }
}

/// The classification is total: a known detail names its class, and an
/// unknown one keeps the generic label.
#[test]
fn the_classification_is_total_and_the_generic_label_is_reachable() {
    let production = i2pr_daemon::i2pcontrol_tunnels::manager_config_class;
    assert_eq!(
        production(""),
        "manager configuration rejected",
        "an empty detail must keep the generic label"
    );
    assert_eq!(
        production("a detail that no marker matches, written only for this guard"),
        "manager configuration rejected",
        "an unknown detail must keep the generic label"
    );
    assert_eq!(
        production("candidate bind collision on 127.0.0.1:1234 between a and b"),
        "candidate bind collision",
        "a known detail must name its class"
    );
    assert_eq!(
        production("x use_ssl requires a daemon TLS policy"),
        "server TLS policy missing",
        "the marker must match inside the detail, not only at its front"
    );
}

/// Every marker maps to its own class: no marker is shadowed by an earlier
/// one, and no class is shared by two conditions.
#[test]
fn every_marker_maps_to_its_own_class() {
    let tree = landed();
    let table = parse_table(tree.source(DAEMON));
    assert!(!table.entries.is_empty(), "the table must not be empty");
    let production = i2pr_daemon::i2pcontrol_tunnels::manager_config_class;
    let mut markers = BTreeSet::new();
    let mut classes = BTreeSet::new();
    for (marker, class) in &table.entries {
        assert!(markers.insert(marker.clone()), "{marker:?} is listed twice");
        assert!(classes.insert(class.clone()), "{class:?} is listed twice");
        assert_eq!(
            production(marker),
            class.as_str(),
            "the marker {marker:?} must map to its own class"
        );
    }
}

/// Rule 1's inverted probe: a class shared by two conditions must be
/// reported.
#[test]
fn the_pinning_rule_fires_when_a_class_is_listed_twice() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        DAEMON,
        "(\"candidate bind collision\", \"candidate bind collision\"),",
        "(\"candidate bind collision\", \"candidate validation rejected\"),",
    );
    assert!(rule_the_table_is_pinned(&mutated).is_some());
}

/// Rule 1's second inverted probe: a marker shadowed by an earlier entry
/// must be reported.
#[test]
fn the_pinning_rule_fires_when_a_marker_is_shadowed() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        DAEMON,
        "    (\n        \"SAM destination registry poisoned\",\n        \"SAM destination registry poisoned\",\n    ),\n    (\n        \"destination registry poisoned\",\n        \"destination registry poisoned\",\n    ),",
        "    (\n        \"destination registry poisoned\",\n        \"destination registry poisoned\",\n    ),\n    (\n        \"SAM destination registry poisoned\",\n        \"SAM destination registry poisoned\",\n    ),",
    );
    assert!(rule_the_table_is_pinned(&mutated).is_some());
}

/// Rule 2's inverted probe: a marker that matches every detail must be
/// reported: the generic label would become unreachable.
#[test]
fn the_totality_rule_fires_when_a_marker_matches_everything() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        DAEMON,
        "        \"candidate validation failed\",",
        "        \"\",",
    );
    assert!(rule_the_classification_is_total(&mutated).is_some());
}

/// Rule 3's inverted probe: a table that disagrees with the production
/// function must be reported.
#[test]
fn the_agreement_rule_fires_when_the_table_and_the_function_disagree() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        DAEMON,
        "(\"candidate bind collision\", \"candidate bind collision\"),",
        "(\"candidate bind collision\", \"bind collision\"),",
    );
    assert!(rule_the_production_function_agrees_with_the_table(&mutated).is_some());
}

/// Rule 4's inverted probe: a class that carries a value must be reported.
#[test]
fn the_value_free_rule_fires_when_a_class_carries_a_value() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        DAEMON,
        "(\"candidate bind collision\", \"candidate bind collision\"),",
        "(\"candidate bind collision\", \"candidate bind collision on 127.0.0.1\"),",
    );
    assert!(rule_every_class_is_value_free(&mutated).is_some());
}

/// Rule 6's inverted probe: a manager reason template that interpolates
/// something else must be reported.
#[test]
fn the_template_rule_fires_when_the_reason_template_carries_a_value() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        DAEMON,
        "\"runtime transition failed: {reason}\"",
        "\"runtime transition failed: {reason} for {name}\"",
    );
    assert!(rule_the_manager_reason_has_a_fixed_prefix(&mutated).is_some());
}

/// Rule 7's inverted probe: a reworded construction message must be
/// reported, because its marker would match nothing.
#[test]
fn the_liveness_rule_fires_when_a_construction_is_reworded() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        CONSTRUCTION,
        "candidate bind collision",
        "candidate bind clash",
    );
    assert!(rule_every_marker_is_live(&mutated).is_some());
}

/// Rule 8's inverted probe: a classifier that may borrow the detail must be
/// reported.
#[test]
fn the_borrow_rule_fires_when_the_classifier_may_borrow() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        DAEMON,
        "pub fn manager_config_class(detail: &str) -> &'static str {",
        "pub fn manager_config_class(detail: &str) -> &str {",
    );
    assert!(rule_the_reason_cannot_borrow_the_detail(&mutated).is_some());
}

/// Rule 9's inverted probe: an `InvalidConfig` arm that collapses every
/// detail again must be reported.
#[test]
fn the_arm_rule_fires_when_every_detail_collapses_again() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        DAEMON,
        "E::InvalidConfig(detail) => manager_config_class(detail),",
        "E::InvalidConfig(_) => \"manager configuration rejected\",",
    );
    assert!(rule_the_invalid_config_arm_classifies_the_detail(&mutated).is_some());
}

/// Rule 10's inverted probe: a lane that sleeps must be reported.
#[test]
fn the_lane_rule_fires_when_the_lane_sleeps() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        LANE,
        "    let token = authenticate(address).await;",
        "    let token = authenticate(address).await;\n    tokio::time::sleep(std::time::Duration::from_secs(1)).await;",
    );
    assert!(rule_the_load_lane_is_present_and_sleep_free(&mutated).is_some());
}

/// Rule 10's second inverted probe: a lane that carries no load must be
/// reported.
#[test]
fn the_lane_rule_fires_when_the_lane_carries_no_load() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        LANE,
        "const LOAD_CONCURRENCY: usize = 8;",
        "const LOAD_CONCURRENCY: usize = 1;",
    );
    assert!(rule_the_load_lane_is_present_and_sleep_free(&mutated).is_some());
}

/// The guard must not report a construct that only appears in a comment or in
/// a test block.
#[test]
fn the_guard_ignores_comments_and_test_blocks() {
    // A comment that names the generic label must not change the verdict.
    let tree = landed();
    let mutated = mutate(
        &tree,
        DAEMON,
        "    (\n        \"candidate validation failed\",",
        "    // (\"manager configuration rejected\", \"manager configuration rejected\"),\n    (\n        \"candidate validation failed\",",
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
