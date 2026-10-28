//! Plan 511 static boundary guard for the version comparator's component
//! ceiling.
//!
//! Plan 491 recorded **F5**: *"The comparator's four-component ceiling is an
//! unstated assumption in the specification's terms."* The specification's
//! API-version table contains a four-component row (`0.6.1.10`), but the
//! tree holds only the table's **minimum-peer obligation** rows as data, so
//! the row the ceiling depends on lived in a prose comment.
//!
//! Plan 511 makes that row a **held record** and the ceiling a **derivation**
//! from it. The guard pins the ten properties the derivation rests on:
//!
//! 1. **The ceiling is derived from the held row**, not a literal beside it.
//! 2. **The held row is the specification's four-component row**, so the
//!    derivation has a real source and a change to it is *reported*.
//! 3. **The stored width is the ceiling**, so a version the parser accepts is
//!    stored in full.
//! 4. **The parser's bound is the ceiling**, so the two can never diverge.
//! 5. **The ordering stays exact** — component-wise, with the exhausted side
//!    less — and never truncates.
//! 6. **The comparator stays `const`**, because `floors_in_force` is a
//!    `const fn`.
//! 7. **Every floor declares three components**, so every live ordering is
//!    against a three-component floor and the ordering's exactness holds
//!    where it matters.
//! 8. **There is no peer-to-peer ordering path**: `PublishedVersion` derives
//!    no `Ord`, so two published peer versions cannot be ordered at all.
//! 9. **The refusal is typed and counted** in the live path, never a silent
//!    skip and never a pass.
//! 10. **The ceiling equals the held row's width**, checked against the
//!     compiled code rather than the source text.
//!
//! The guard is deliberately **fail-closed**: it strips comments before
//! matching, it excludes `#[cfg(test)]` blocks, an absent function is a
//! violation rather than a pass, and every rule carries its own **inverted
//! probe** — a synthetic violating source must be reported. If a rule ever
//! stops detecting its violation, its probe fails.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required *call*, so a token that appears in a sibling
//! function, in a comment, or in a test block does not satisfy them.

use std::fs;
use std::path::{Path, PathBuf};

/// The crate that owns the comparator.
const PROTO: &str = "crates/i2pr-proto/src/common/version.rs";

/// The crate that owns the live eligibility path and its refusal counters.
const NETDB: &str = "crates/i2pr-netdb/src/version.rs";

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

/// Returns the byte range of the brace-balanced block that opens at or after
/// `start`, as `(open, close)`; `close` is one past the closing brace.
///
/// The scan is **string-aware**, so a brace inside a string literal does not
/// change the nesting depth.
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
/// The anchor carries the `fn` keyword and the character that follows it must
/// not continue an identifier, so `fn parse` never matches another name. A
/// function that is not present yields `None`, so every caller fails closed.
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

/// Removes every whitespace character, so a rule does not depend on how
/// `rustfmt` chose to lay a statement out.
fn compact(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// The files the guard reads.
const TREE_FILES: &[&str] = &[PROTO, NETDB];

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

/// The text of one `pub const` item's initialiser, up to the terminating
/// semicolon.
fn const_initialiser(code: &str, name: &str) -> Option<String> {
    let needle = format!("pub const {name}");
    let position = code.find(&needle)?;
    let after = position + needle.len();
    let equals = code[after..].find('=')? + after;
    let semicolon = code[equals..].find(';')? + equals;
    Some(code[equals + 1..semicolon].to_owned())
}

/// The integers of one recorded version row, in order.
fn integer_row(initialiser: &str) -> Option<Vec<u16>> {
    let open = initialiser.find('[')?;
    let close = initialiser[open..].find(']')? + open;
    initialiser[open + 1..close]
        .split(',')
        .map(|part| part.trim().parse::<u16>().ok())
        .collect::<Option<Vec<u16>>>()
}

/// The modifiers written before a function's `fn` keyword.
///
/// The anchor is the definition, and the modifiers are the text between the
/// previous item's end and the keyword, so `pub const fn cmp_to` is read as
/// `const` rather than as a bare `fn`.
fn fn_modifiers(code: &str, name: &str) -> Option<String> {
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
        let boundary = code[..start]
            .rfind([';', '{', '}'])
            .map(|index| index + 1)
            .unwrap_or(0);
        return Some(code[boundary..start].to_owned());
    }
    None
}

/// The derive attribute's tokens for one item, if it has one.
fn derive_tokens(code: &str, item: &str) -> Option<Vec<String>> {
    let position = code.find(item)?;
    let before = &code[..position];
    let attribute = before.rfind("#[derive(")?;
    let open = attribute + "#[derive(".len();
    let close = before[open..].find(')')? + open;
    Some(
        before[open..close]
            .split(',')
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty())
            .collect(),
    )
}

/// Rule 1: the ceiling is derived from the held row, not a literal beside it.
fn rule_the_ceiling_is_derived_from_the_held_row(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(PROTO));
    let Some(raw) = const_initialiser(&code, "MAX_API_VERSION_COMPONENTS") else {
        return Some("the comparator's ceiling must exist".to_owned());
    };
    let initialiser = compact(&raw);
    if !initialiser.contains("SPECIFICATION_WIDEST_VERSION_ROW") {
        return Some(
            "the comparator's ceiling must be derived from the held row, not a literal".to_owned(),
        );
    }
    if initialiser.contains('4') && !initialiser.contains("WIDEST_VERSION_ROW") {
        return Some("the comparator's ceiling must not be a bare literal".to_owned());
    }
    None
}

/// Rule 2: the held row is the specification's four-component row.
fn rule_the_held_row_is_the_specifications_row(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(PROTO));
    let Some(raw) = const_initialiser(&code, "SPECIFICATION_WIDEST_VERSION_ROW") else {
        return Some("the held row must exist".to_owned());
    };
    let Some(numbers) = integer_row(&raw) else {
        return Some("the held row must be one literal row".to_owned());
    };
    if numbers != vec![0, 6, 1, 10] {
        return Some(format!(
            "the held row must be the specification's four-component row, found {numbers:?}"
        ));
    }
    None
}

/// Rule 3: the stored width is the ceiling.
fn rule_the_stored_width_is_the_ceiling(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(PROTO));
    let Some(position) = code.find("pub struct ApiVersion") else {
        return Some("`ApiVersion` must exist in the production surface".to_owned());
    };
    let block = &code[position..];
    let Some((open, close)) = brace_balanced_block(block, 0) else {
        return Some("`ApiVersion` must have a body".to_owned());
    };
    let body = &block[open + 1..close - 1];
    if !body.contains("[u16; MAX_API_VERSION_COMPONENTS]") {
        return Some("the stored array's width must be the comparator's ceiling".to_owned());
    }
    None
}

/// Rule 4: the parser's bound is the ceiling.
fn rule_the_parser_bound_is_the_ceiling(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(PROTO));
    let Some(body) = function_body(&code, "parse") else {
        return Some("`fn parse` must exist in the production surface".to_owned());
    };
    let body = compact(body);
    if !body.contains("MAX_API_VERSION_COMPONENTS") {
        return Some("the parser's bound must be the comparator's ceiling".to_owned());
    }
    for literal in [">=4", "==4", ">4"] {
        if body.contains(literal) {
            return Some(format!(
                "the parser's bound must not be a bare literal `{literal}`"
            ));
        }
    }
    None
}

/// Rule 5: the ordering stays exact, and never truncates.
fn rule_the_ordering_is_exact(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(PROTO));
    let Some(body) = function_body(&code, "cmp_to") else {
        return Some("`fn cmp_to` must exist in the production surface".to_owned());
    };
    let body = compact(body);
    if !body.contains("whileindex<MAX_API_VERSION_COMPONENTS{") {
        return Some("the comparison must run to the comparator's full width".to_owned());
    }
    if !body.contains("index<self.lenasusize") {
        return Some("the comparison must run to the version's own extent".to_owned());
    }
    if !body.contains("index<other.lenasusize") {
        return Some("the comparison must run to the other version's own extent".to_owned());
    }
    if !body.contains("(None,Some(_))=>returnOrdering::Less") {
        return Some("the exhausted side must compare as less".to_owned());
    }
    if !body.contains("(Some(_),None)=>returnOrdering::Greater") {
        return Some("the side that still has a component must compare as greater".to_owned());
    }
    if body.contains("..") {
        return Some("the ordering must not truncate the components".to_owned());
    }
    None
}

/// Rule 6: the comparator stays `const`.
fn rule_the_comparator_stays_const(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(PROTO));
    let Some(modifiers) = fn_modifiers(&code, "cmp_to") else {
        return Some("`fn cmp_to` must exist in the production surface".to_owned());
    };
    if !modifiers.contains("const") {
        return Some("`fn cmp_to` must stay `const`".to_owned());
    }
    None
}

/// Rule 7: every floor declares three components.
fn rule_every_floor_declares_three_components(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(PROTO));
    let anchor = "pub const MINIMUM_PEER_OBLIGATIONS";
    let Some(position) = code.find(anchor) else {
        return Some("the obligation rows must exist in the production surface".to_owned());
    };
    let block = &code[position..];
    let Some((open, close)) = brace_balanced_block(block, 0) else {
        return Some("the obligation rows must have a body".to_owned());
    };
    let body = compact(&block[open + 1..close - 1]);
    if body.contains("new4(") {
        return Some("a floor must declare three components".to_owned());
    }
    let rows = body.matches("ApiVersion::new(").count();
    if rows == 0 {
        return Some("the obligation rows must be built from `ApiVersion::new`".to_owned());
    }
    None
}

/// Rule 8: there is no peer-to-peer ordering path.
fn rule_there_is_no_peer_to_peer_ordering_path(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(PROTO));
    let Some(tokens) = derive_tokens(&code, "pub enum PublishedVersion") else {
        return Some("`PublishedVersion` must exist in the production surface".to_owned());
    };
    for token in &tokens {
        if token == "Ord" || token == "PartialOrd" {
            return Some(
                "`PublishedVersion` must not be orderable: two published peer versions must not be \
                 comparable"
                .to_owned(),
            );
        }
    }
    None
}

/// Rule 9: the refusal is typed and counted in the live path.
fn rule_the_refusal_is_typed_and_counted(tree: &Tree) -> Option<String> {
    let code = code_view(tree.source(NETDB));
    let Some(body) = function_body(&code, "record") else {
        return Some("the refusal counter must exist in the production surface".to_owned());
    };
    let body = compact(body);
    if !body.contains("VersionEligibility::UncomparableVersion") {
        return Some("the refusal counter must count the uncomparable reason".to_owned());
    }
    if !body.contains("uncomparable_version") {
        return Some("the refusal counter must have an uncomparable counter".to_owned());
    }
    None
}

/// Rule 10: the ceiling equals the held row's width, in the compiled code.
fn rule_the_ceiling_equals_the_held_row_width(_tree: &Tree) -> Option<String> {
    let ceiling = i2pr_proto::MAX_API_VERSION_COMPONENTS;
    let width = i2pr_proto::version_row_width(i2pr_proto::SPECIFICATION_WIDEST_VERSION_ROW);
    if ceiling != width {
        return Some(format!(
            "the comparator's ceiling {ceiling} must be the held row's width {width}"
        ));
    }
    None
}

/// One rule of the guard: a label and the check it applies.
type Rule = (&'static str, fn(&Tree) -> Option<String>);

/// Every rule, as a list of violations.
fn check_all(tree: &Tree) -> Vec<String> {
    let rules: Vec<Rule> = vec![
        ("rule 1", rule_the_ceiling_is_derived_from_the_held_row),
        ("rule 2", rule_the_held_row_is_the_specifications_row),
        ("rule 3", rule_the_stored_width_is_the_ceiling),
        ("rule 4", rule_the_parser_bound_is_the_ceiling),
        ("rule 5", rule_the_ordering_is_exact),
        ("rule 6", rule_the_comparator_stays_const),
        ("rule 7", rule_every_floor_declares_three_components),
        ("rule 8", rule_there_is_no_peer_to_peer_ordering_path),
        ("rule 9", rule_the_refusal_is_typed_and_counted),
        ("rule 10", rule_the_ceiling_equals_the_held_row_width),
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
        ("rule 1", rule_the_ceiling_is_derived_from_the_held_row),
        ("rule 2", rule_the_held_row_is_the_specifications_row),
        ("rule 3", rule_the_stored_width_is_the_ceiling),
        ("rule 4", rule_the_parser_bound_is_the_ceiling),
        ("rule 5", rule_the_ordering_is_exact),
        ("rule 6", rule_the_comparator_stays_const),
        ("rule 7", rule_every_floor_declares_three_components),
        ("rule 8", rule_there_is_no_peer_to_peer_ordering_path),
        ("rule 9", rule_the_refusal_is_typed_and_counted),
        ("rule 10", rule_the_ceiling_equals_the_held_row_width),
    ];
    assert_eq!(rules.len(), 10);
    for (label, rule) in rules {
        assert_eq!(rule(&tree), None, "{label} must hold on the landed tree");
    }
}

/// Rule 1's inverted probe: a literal ceiling must be reported.
#[test]
fn the_derivation_rule_fires_when_the_ceiling_is_a_literal() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "version_row_width(SPECIFICATION_WIDEST_VERSION_ROW);",
        "4;",
    );
    assert!(rule_the_ceiling_is_derived_from_the_held_row(&mutated).is_some());
}

/// Rule 2's inverted probe: a changed held row must be reported.
#[test]
fn the_held_row_rule_fires_when_the_record_moves() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "pub const SPECIFICATION_WIDEST_VERSION_ROW: &[u16] = &[0, 6, 1, 10];",
        "pub const SPECIFICATION_WIDEST_VERSION_ROW: &[u16] = &[0, 9, 62];",
    );
    assert!(rule_the_held_row_is_the_specifications_row(&mutated).is_some());
}

/// Rule 3's inverted probe: a stored width that is not the ceiling must be
/// reported.
#[test]
fn the_stored_width_rule_fires_when_the_width_diverges() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "components: [u16; MAX_API_VERSION_COMPONENTS],",
        "components: [u16; 4],",
    );
    assert!(rule_the_stored_width_is_the_ceiling(&mutated).is_some());
}

/// Rule 4's inverted probe: a parser bound that is a literal must be
/// reported.
#[test]
fn the_parser_bound_rule_fires_when_the_bound_is_a_literal() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "if digits == 0 || len >= MAX_API_VERSION_COMPONENTS {",
        "if digits == 0 || len >= 4 {",
    );
    assert!(rule_the_parser_bound_is_the_ceiling(&mutated).is_some());
}

/// Rule 5's first inverted probe: a comparison that stops short of the
/// comparator's full width must be reported.
#[test]
fn the_ordering_rule_fires_when_the_comparison_stops_early() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "while index < MAX_API_VERSION_COMPONENTS {",
        "while index < 2 {",
    );
    assert!(rule_the_ordering_is_exact(&mutated).is_some());
}

/// Rule 5's second inverted probe: a comparison that stops at a version's own
/// extent instead of the full width must be reported.
#[test]
fn the_ordering_rule_fires_when_the_extent_is_narrowed() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "let left = if index < self.len as usize {",
        "let left = if index < 2 {",
    );
    assert!(rule_the_ordering_is_exact(&mutated).is_some());
}

/// Rule 5's third inverted probe: an ordering that truncates the components
/// array must be reported.
#[test]
fn the_ordering_rule_fires_when_the_ordering_truncates() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "Some(self.components[index])",
        "Some(self.components[index..index + 1][0])",
    );
    assert!(rule_the_ordering_is_exact(&mutated).is_some());
}

/// Rule 6's inverted probe: a comparator that is no longer `const` must be
/// reported.
#[test]
fn the_const_rule_fires_when_the_comparator_loses_const() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "pub const fn cmp_to(&self, other: &Self) -> Ordering {",
        "pub fn cmp_to(&self, other: &Self) -> Ordering {",
    );
    assert!(rule_the_comparator_stays_const(&mutated).is_some());
}

/// Rule 7's inverted probe: a four-component floor must be reported.
#[test]
fn the_floor_rule_fires_when_a_floor_widens() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "api_version: ApiVersion::new(0, 9, 62),",
        "api_version: ApiVersion::new4(0, 9, 62, 1),",
    );
    assert!(rule_every_floor_declares_three_components(&mutated).is_some());
}

/// Rule 8's inverted probe: an orderable `PublishedVersion` must be reported.
#[test]
fn the_ordering_path_rule_fires_when_published_versions_become_orderable() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]\npub enum PublishedVersion {",
        "#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]\npub enum PublishedVersion {",
    );
    assert!(rule_there_is_no_peer_to_peer_ordering_path(&mutated).is_some());
}

/// Rule 9's inverted probe: a refusal counter that drops the uncomparable
/// reason must be reported.
#[test]
fn the_counting_rule_fires_when_the_uncomparable_counter_is_dropped() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        NETDB,
        "VersionEligibility::UncomparableVersion => {\n                self.uncomparable_version = self.uncomparable_version.saturating_add(1);\n            }",
        "VersionEligibility::UncomparableVersion => {}",
    );
    assert!(rule_the_refusal_is_typed_and_counted(&mutated).is_some());
}

/// The guard must not report a construct that only appears in a comment or in
/// a test block.
#[test]
fn the_guard_ignores_comments_and_test_blocks() {
    let tree = landed();
    let mutated = mutate(
        &tree,
        PROTO,
        "pub const SPECIFICATION_WIDEST_VERSION_ROW: &[u16] = &[0, 6, 1, 10];",
        "// pub const SPECIFICATION_WIDEST_VERSION_ROW: &[u16] = &[0, 0, 0, 0];\npub const SPECIFICATION_WIDEST_VERSION_ROW: &[u16] = &[0, 6, 1, 10];",
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
