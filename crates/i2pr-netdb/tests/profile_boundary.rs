//! Plan 488/489 static boundary guard for the `i2pr-netdb` crate.
//!
//! Plan 488's first invariant is that the crate stays runtime-neutral: it
//! gains no `tokio::*`, no `std::net`, no `std::fs`, no `async fn`, no
//! task ownership, and no unchecked RouterInfo insertion path.
//!
//! Plan 489 adds a second, sharper invariant to the same guard: the
//! routing table, the exploration job and the optimized lookup plan must
//! not read a clock. Every time input they use — the publication date,
//! the day boundary, the exploration deadline — is supplied by the
//! caller, so the table's contents are reproducible.
//!
//! `scripts/check-runtime-boundaries.sh` has **no `i2pr-netdb` section**,
//! so nothing in the routine floor enforces that invariant today. This
//! guard is the missing executable half, and it is deliberately written to
//! be *fail-closed*: it scans the crate's own production sources, it
//! strips comments before matching (so a doc comment that merely names a
//! forbidden construct is not a violation), and it excludes `#[cfg(test)]`
//! blocks (so the crate's own unit tests may use whatever they need).
//!
//! The guard carries its own **inverted probe**: a synthetic violating
//! source must be reported, and a synthetic source that only *mentions* the
//! forbidden construct in a comment must not. If the scanner ever stops
//! detecting a violation, the inverted probe fails.

use std::fs;
use std::path::{Path, PathBuf};

/// The forbidden constructs, with the reason each one is forbidden.
const FORBIDDEN: &[(&str, &str)] = &[
    ("tokio::", "the crate must stay runtime-neutral"),
    ("std::net", "the crate must not name socket addresses"),
    ("std::fs", "the crate must not touch the filesystem"),
    (
        "async fn",
        "the crate must not define asynchronous functions",
    ),
    (
        "async move",
        "the crate must not define asynchronous blocks",
    ),
    ("JoinHandle", "the crate must not own tasks"),
    (
        "insert_unchecked",
        "the Plan 103 unchecked-insertion prohibition stands",
    ),
    (
        "std::time::Instant",
        "the Plan 489 table and job must not read a monotonic clock",
    ),
    (
        "std::time::SystemTime",
        "the Plan 489 table and job must not read a wall clock",
    ),
];

fn crate_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Removes comments from Rust source text.
///
/// The scanner must not report a doc comment that merely *names* a
/// forbidden construct, so the text is reduced to its code before the
/// forbidden tokens are matched.
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
                if let Some(escaped) = next {
                    out.push(escaped);
                }
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
            // A lifetime (`'a`) is not a character literal. Rust character
            // literals are exactly `'x'` or `'\n'`; anything else is a
            // lifetime and must not swallow the code that follows it.
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
/// The crate's own unit tests may use whatever they need; the invariant is
/// about the production surface.
fn strip_cfg_test_blocks(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    while let Some(position) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..position]);
        let after_attribute = &rest[position + "#[cfg(test)]".len()..];
        // The attribute is followed by the item it applies to; the guarded
        // region is that item's brace block.
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
        // Keep the line numbering of the original file intact.
        for _ in after_attribute[..cursor].matches('\n') {
            out.push('\n');
        }
        rest = &after_attribute[cursor..];
    }
    out.push_str(rest);
    out
}

/// Returns the forbidden constructs a source text contains.
fn violations(source: &str) -> Vec<(String, &'static str, String)> {
    let code = strip_cfg_test_blocks(&strip_comments(source));
    let mut found = Vec::new();
    for (token, reason) in FORBIDDEN {
        let mut offset = 0usize;
        while let Some(position) = code[offset..].find(token) {
            let start = offset + position;
            let line = code[..start].matches('\n').count() + 1;
            found.push(((*token).to_owned(), *reason, line.to_string()));
            offset = start + token.len();
        }
    }
    found
}

#[test]
fn the_crate_sources_carry_no_forbidden_construct() {
    let root = crate_source();
    let mut failures = Vec::new();
    let mut scanned = 0usize;
    let mut entries: Vec<PathBuf> = fs::read_dir(&root)
        .expect("read the crate source directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .collect();
    entries.sort();
    for path in entries {
        scanned += 1;
        let source = fs::read_to_string(&path).expect("read the crate source");
        for (token, reason, line) in violations(&source) {
            failures.push(format!(
                "{}:{} uses {token} ({reason})",
                path.display(),
                line
            ));
        }
    }
    assert!(scanned > 0, "the guard scanned no source file");
    assert!(
        failures.is_empty(),
        "the crate must stay runtime-neutral:\n{}",
        failures.join("\n")
    );
}

/// The Plan 489 modules the guard must cover.
const PLAN_489_MODULES: &[&str] = &["bucket_set.rs", "exploration.rs", "optimized_lookup.rs"];

#[test]
fn the_guard_scans_the_plan_489_modules() {
    // The guard scans the whole `src/` directory, so a new module is
    // covered by construction. This test makes that coverage explicit:
    // if a Plan 489 module were renamed or dropped, the guard would
    // notice instead of silently scanning less.
    let root = crate_source();
    let mut scanned: Vec<String> = fs::read_dir(&root)
        .expect("read the crate source directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .collect();
    scanned.sort();
    for module in PLAN_489_MODULES {
        assert!(
            scanned.iter().any(|name| name == module),
            "the guard must scan {module}; it scanned {scanned:?}"
        );
    }
}

#[test]
fn the_guard_reports_a_clock_read() {
    // The inverted probe for the Plan 489 invariant: a source that
    // reads a clock must be reported, in the same way a source that
    // owns a task is.
    let violating = "fn now() -> std::time::Instant { std::time::Instant::now() }";
    let found = violations(violating);
    assert!(
        found
            .iter()
            .any(|(token, _, _)| token == "std::time::Instant"),
        "a clock read must be reported: {found:?}"
    );
}

#[test]
fn the_guard_reports_a_violating_source() {
    // The inverted probe: a source that really uses a forbidden construct
    // must be reported. If this ever stops failing, the guard has gone
    // blind.
    let violating = "fn oops() { let _ = tokio::spawn(async move { }); }";
    let found = violations(violating);
    assert!(
        found.iter().any(|(token, _, _)| token == "tokio::"),
        "a real violation must be reported: {found:?}"
    );
    assert!(
        found.iter().any(|(token, _, _)| token == "async move"),
        "a real async block must be reported: {found:?}"
    );
}

#[test]
fn the_guard_ignores_comments_and_test_blocks() {
    // A doc comment that merely names the forbidden construct is not a
    // violation, and neither is a `#[cfg(test)]` block.
    let benign = "\
//! The crate must not use tokio:: or std::net or std::fs.
/* insert_unchecked is forbidden. */
/// async fn is forbidden.
fn fine() -> u8 { 7 }

#[cfg(test)]
mod tests {
    use std::fs;
    #[test]
    fn uses_the_filesystem() {
        let _ = std::fs::read_to_string(\"x\");
        let _ = tokio::spawn(async move {});
    }
}
";
    let found = violations(benign);
    assert!(
        found.is_empty(),
        "comments and test blocks must not be reported: {found:?}"
    );
}

#[test]
fn the_guard_reports_a_forbidden_construct_outside_a_test_block() {
    let violating = "\
#[cfg(test)]
mod tests {
    fn inner() {}
}

fn bad() { let _ = std::fs::read_to_string(\"x\"); }
";
    let found = violations(violating);
    assert!(
        found.iter().any(|(token, _, _)| token == "std::fs"),
        "a production filesystem use must be reported: {found:?}"
    );
}
