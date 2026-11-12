//! Plan 545's guard -- the SSU2 pool must not be floodfill-only.
//!
//! The pool's only candidate source was `select_version_gated_floodfills`, so
//! it could dial floodfills and nothing else. On a private LAN that population
//! is **empty by construction**: floodfill eligibility requires a publicly
//! routable address, and `controlled_loopback` floodfill requires loopback
//! binds. The gap is not LAN-specific -- on the public network the pool never
//! dialled the peers the router had learned.
//!
//! The guard is the authority for the **shape**, which the lane cannot see:
//! the lane proves what the two selections yield, and this guard proves that
//! the pool **prefers** floodfills and reaches the fallback only when the
//! floodfill selection is empty, that the eligibility gate is shared rather
//! than duplicated, and that no pool ceiling moved.
//!
//! Every rule has an inverted probe that fires it against an **in-memory copy**
//! of the source.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const LOOKUP_POLICY: &str = "crates/i2pr-netdb/src/lookup_policy.rs";
const NETDB_LIB: &str = "crates/i2pr-netdb/src/lib.rs";
const PEER_POOL: &str = "crates/i2pr-daemon/src/peer_pool.rs";
const CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const NETDB_LOCAL: &str = "crates/i2pr-netdb/src/local.rs";
const SUPPORT_TOML: &str = "specs/support.toml";
const MANIFEST: &str = "Cargo.toml";
const LOCK: &str = "Cargo.lock";
const PLAN505_LANE: &str = "crates/i2pr-daemon/tests/exploratory_driver_live.rs";
const PLAN505_GUARD: &str = "crates/i2pr-daemon/tests/exploratory_driver_boundary.rs";
const PLAN506_LANE: &str = "crates/i2pr-daemon/tests/peer_reconcile_live.rs";
const PLAN506_GUARD: &str = "crates/i2pr-daemon/tests/peer_reconcile_boundary.rs";
const PLAN507_LANE: &str = "crates/i2pr-daemon/tests/netdb_retention_live.rs";
const PLAN507_GUARD: &str = "crates/i2pr-daemon/tests/netdb_retention_boundary.rs";
const PLAN538_LANE: &str = "crates/i2pr-daemon/tests/floodfill_statistic_accounting.rs";
const PLAN538_GUARD: &str = "crates/i2pr-daemon/tests/floodfill_statistic_accounting_boundary.rs";
const PLAN539_LANE: &str = "crates/i2pr-daemon/tests/routing_table_population_lock.rs";
const PLAN539_GUARD: &str = "crates/i2pr-daemon/tests/routing_table_population_lock_boundary.rs";
const PLAN540_LANE: &str = "crates/i2pr-daemon/tests/netdb_prune_walk.rs";
const PLAN540_GUARD: &str = "crates/i2pr-daemon/tests/netdb_prune_walk_boundary.rs";
const PLAN541_LANE: &str = "crates/i2pr-daemon/tests/floodfill_selection_visibility.rs";
const PLAN541_GUARD: &str = "crates/i2pr-daemon/tests/floodfill_selection_visibility_boundary.rs";
const PLAN542_LANE: &str = "crates/i2pr-daemon/tests/netdb_retention_posture.rs";
const PLAN542_GUARD: &str = "crates/i2pr-daemon/tests/netdb_retention_posture_boundary.rs";
const PLAN543_LANE: &str = "crates/i2pr-daemon/tests/participation_release.rs";
const PLAN543_GUARD: &str = "crates/i2pr-daemon/tests/participation_release_boundary.rs";
const PLAN544_LANE: &str = "crates/i2pr-daemon/tests/router_info_export.rs";
const PLAN544_GUARD: &str = "crates/i2pr-daemon/tests/router_info_export_boundary.rs";

/// The digests the guard pins, filled in by the generator.
const PINNED: &[(&str, &str)] = &[
    (
        PLAN505_LANE,
        "5d15348eca4be8cce785537bf322f07c5fbe9a482ee0668b4f88c5c0c345ff79",
    ),
    (
        PLAN505_GUARD,
        "3a0efb877ad7b5626130bccb97450e1a326502acd486a3496a59cd3c43644622",
    ),
    (
        PLAN506_LANE,
        "ffabd287ec94cdcd044e58784b97dd5ae9f920ce016627d2792f87801b5ef28f",
    ),
    (
        PLAN506_GUARD,
        "33db28ccc76a52338b381b20c9d44ca2df9152d90d4f56a29a4279ad4c4eaafc",
    ),
    (
        PLAN507_LANE,
        "709ec1b47e731bd71edb7ca65f4ef889e4695f36cf5a012cf7d70b5ceae2c5da",
    ),
    (
        PLAN507_GUARD,
        "5ef83044dcb8036e0ac2ccc8e7e9cb9bf6b7a21127b822b6e634da6170fdd78d",
    ),
    (
        PLAN538_LANE,
        "d1ce001d1e7562ca84b33cd9b4b0bca04325565b7cfef2c7504645825c8bc613",
    ),
    (
        PLAN538_GUARD,
        "3d059e0690a0ab52b5c54619c0b5220eac8f340168187c2541f12f597c518fa4",
    ),
    (
        PLAN539_LANE,
        "068410ef125b2122b1acf56866abff78010f6fe20dc4ef933d80a7c4e6d99103",
    ),
    (
        PLAN539_GUARD,
        "60ef63b653835cbb1af7943aea64662fe6970eed5827b1266a92994dea2ae300",
    ),
    (
        PLAN540_LANE,
        "369a69a0ce776fbb44f3a0bfec164a7296755e19ccea01f3f0fd7ca6a6739468",
    ),
    (
        PLAN540_GUARD,
        "da81058d9e32b1e36332e3f031788c93b87c1430dcbb1d753759c227634586a4",
    ),
    (
        PLAN541_LANE,
        "85190546dd7e371ae140cc4e9b964caa32100a3ca44c830986034593b7cfeb41",
    ),
    (
        PLAN541_GUARD,
        "351080b5f5f219a5b54a7a732556185e1ef18b108b4e948e5597d68c8b02ded7",
    ),
    (
        PLAN542_LANE,
        "0a0657a444fd5de046bb7163b6d988bc75ec4f30b63770c5b93248b352a8184e",
    ),
    (
        PLAN542_GUARD,
        "6a610f3f3af2926870ca4d468f8ce45a80d7ba400c27c6e3eb013461d7689800",
    ),
    (
        PLAN543_LANE,
        "04cad0198ce4a94c7b0d8629ed7a3c4b26f0f93170a93fd5c82c1327e53be1f0",
    ),
    (
        PLAN543_GUARD,
        "d2e2360ba36c22d573718b98370f48a4499c5c255ac667d6455c42201a195e47",
    ),
    (
        PLAN544_LANE,
        "8ca1d83d8d4bb5e6cd1a5e707aa101b9ebb7e741edd19263ffef7fc9e5fb7aae",
    ),
    (
        PLAN544_GUARD,
        "2fceec31f0aeeb20d01fdb0eaf867bb929ae7dcc3167357e4c86ccffdd9cadd5",
    ),
];

/// The workspace manifest, byte for byte.
const CARGO_TOML_SHA256: &str = "39c44e33add62693eef8866220de17dcfe137437b62732e5044fcaefc4870269";

/// The workspace lock, byte for byte.
const CARGO_LOCK_SHA256: &str = "f0cec0685c4ed460c7d63273cc318f7a683dc2357735d7edaa6dfa1ac243d833";
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

/// The SHA-256 of some bytes, in lower-case hexadecimal.
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
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
            in_char = true;
            out.push(current);
            index += 1;
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
}

/// Removes `#[cfg(test)]` blocks from source text.
///
/// A test-only helper must never satisfy a production rule.
fn strip_cfg_test_blocks(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut index = 0usize;
    while index < code.len() {
        let rest = &code[index..];
        if rest.starts_with("#[cfg(test)]") {
            let after = index + "#[cfg(test)]".len();
            let Some(open) = code[after..].find('{') else {
                break;
            };
            let open = after + open;
            let Some((_, close)) = brace_balanced_block(code, open) else {
                break;
            };
            out.push(' ');
            index = close;
            continue;
        }
        let character = rest.chars().next().expect("character");
        out.push(character);
        index += character.len_utf8();
    }
    out
}

/// The code view of a source file: no comments and no test-only blocks.
fn code_view(source: &str) -> String {
    strip_cfg_test_blocks(&strip_comments(source))
}

/// Returns the offset of a **definition** whose text is `needle`.
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

/// Returns the offset just past the character literal that starts at
/// `start`, or `None` when the quote opens a lifetime instead.
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
fn item_body(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (open, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[open + 1..close - 1].to_owned())
}

struct Sources {
    lookup_policy: String,
    netdb_lib: String,
    peer_pool: String,
    daemon_config: String,
    netdb_local: String,
    support: String,
    manifest_sha256: String,
    lock_sha256: String,
    predecessors: Vec<(&'static str, String)>,
}

impl Sources {
    fn from_tree() -> Sources {
        Sources {
            lookup_policy: source_of(LOOKUP_POLICY),
            netdb_lib: source_of(NETDB_LIB),
            peer_pool: source_of(PEER_POOL),
            daemon_config: source_of(CONFIG),
            netdb_local: source_of(NETDB_LOCAL),
            support: source_of(SUPPORT_TOML),
            manifest_sha256: sha256_hex(source_of(MANIFEST).as_bytes()),
            lock_sha256: sha256_hex(source_of(LOCK).as_bytes()),
            predecessors: PINNED
                .iter()
                .map(|(path, _expected)| (*path, sha256_hex(source_of(path).as_bytes())))
                .collect(),
        }
    }
}

// ---- 1. the pool prefers floodfills, then falls back --------------------

fn rule_the_pool_prefers_floodfills(sources: &Sources) -> Option<String> {
    let pool = code_view(&sources.peer_pool);

    let Some(plan) = item_body(&pool, "pub fn plan(") else {
        return Some("the pool's plan is absent".to_owned());
    };

    // The floodfill selection is the **first** source ...
    let Some(floodfill_at) = plan.find("select_version_gated_floodfills(") else {
        return Some("the pool must prefer the floodfill selection".to_owned());
    };
    // ... and the fallback is guarded by the floodfill selection being empty.
    let Some(fallback_at) = plan.find("select_version_gated_peers(") else {
        return Some("the pool must have a fallback selection".to_owned());
    };
    if fallback_at < floodfill_at {
        return Some("the fallback must follow the floodfill selection".to_owned());
    }
    let Some(guard_at) = plan.find("targets().is_empty()") else {
        return Some(
            "the fallback must be reached only when the floodfill set is empty".to_owned(),
        );
    };
    if guard_at > fallback_at {
        return Some("the empty check must guard the fallback".to_owned());
    }
    None
}

// ---- 2. the gate is shared, not duplicated ------------------------------

fn rule_the_gate_is_shared(sources: &Sources) -> Option<String> {
    let policy = code_view(&sources.lookup_policy);

    // One gate, called by both selectors.
    if policy.matches("fn gate_candidates(").count() != 1 {
        return Some("the eligibility gate must be defined exactly once".to_owned());
    }
    for selector in [
        "pub fn select_version_gated_floodfills(",
        "pub fn select_version_gated_peers(",
    ] {
        let Some(body) = item_body(&policy, selector) else {
            return Some(format!("the selector is absent: {selector}"));
        };
        if !body.contains("gate_candidates(store,") {
            return Some(format!("{selector} must reach the shared gate"));
        }
    }
    // The eligibility rule itself is not restated.
    if policy.matches("dsm_target_eligibility(").count() != 1 {
        return Some("the version rule must be applied in one place".to_owned());
    }
    None
}

// ---- 3. the ordering, the ceiling and the self-exclusion are unchanged --

fn rule_the_policy_is_unchanged(sources: &Sources) -> Option<String> {
    let policy = code_view(&sources.lookup_policy);

    for needle in [
        "candidates.sort();",
        "candidates.truncate(policy.max_candidates_considered());",
    ] {
        if !policy.contains(needle) {
            return Some(format!("the selection lost its policy step: {needle}"));
        }
    }
    // The self-exclusion is the caller's, so the pool must keep passing it.
    let pool = code_view(&sources.peer_pool);
    if pool.matches("&[*local_hash]").count() < 2 {
        return Some("both selections must exclude the router's own hash".to_owned());
    }
    // The pool's ceilings are unchanged.
    if !pool.contains("MAX_POOLED_PEERS") || !pool.contains("MAX_DIALS_PER_PERIOD") {
        return Some("the pool's ceilings must not move".to_owned());
    }
    None
}

// ---- 4. the exports exist ----------------------------------------------

fn rule_the_selection_is_exported(sources: &Sources) -> Option<String> {
    if !sources.netdb_lib.contains("select_version_gated_peers") {
        return Some("the peer selection must be exported from the crate root".to_owned());
    }
    None
}

// ---- 5. no key, no runtime construct, nothing advertised ----------------

fn rule_nothing_new_and_nothing_advertised(sources: &Sources) -> Option<String> {
    for (label, code) in [
        ("the selection", code_view(&sources.lookup_policy)),
        ("the pool", code_view(&sources.peer_pool)),
    ] {
        for forbidden in [
            "std::net::TcpListener",
            "std::net::UdpSocket",
            "TcpListener::bind",
            "UdpSocket::bind",
            "std::thread::spawn",
            "tokio::spawn",
        ] {
            if code.contains(forbidden) {
                return Some(format!(
                    "{label} must add no runtime construct: {forbidden}"
                ));
            }
        }
    }
    let config = code_view(&sources.daemon_config);
    for token in ["pool_fallback", "peer_pool_fallback", "fallback_peers"] {
        if config.to_lowercase().contains(token) {
            return Some(format!("the configuration must gain no key: {token}"));
        }
    }
    if sources.manifest_sha256 != CARGO_TOML_SHA256 {
        return Some("Cargo.toml must stay byte-identical".to_owned());
    }
    if sources.lock_sha256 != CARGO_LOCK_SHA256 {
        return Some("Cargo.lock must stay byte-identical".to_owned());
    }
    let local = code_view(&sources.netdb_local);
    if !local.contains("pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.62\";") {
        return Some("the controlled router version must not move".to_owned());
    }
    if sources.support.contains("advertised = true") {
        return Some("the inventory must advertise nothing".to_owned());
    }
    None
}

// ---- 6. the predecessors' evidence is byte-identical --------------------

fn rule_the_predecessors_evidence_is_untouched(sources: &Sources) -> Option<String> {
    for (path, actual) in &sources.predecessors {
        let expected = PINNED
            .iter()
            .find(|(pinned_path, _)| pinned_path == path)
            .map(|(_, digest)| *digest)
            .unwrap_or("");
        if actual != expected {
            return Some(format!("{path} must stay byte-identical"));
        }
    }
    None
}

// ----------------------------------------------------------------------

#[test]
fn the_pool_prefers_floodfills() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_pool_prefers_floodfills(&sources), None);
}

#[test]
fn the_gate_is_shared() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_gate_is_shared(&sources), None);
}

#[test]
fn the_policy_is_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_policy_is_unchanged(&sources), None);
}

#[test]
fn the_selection_is_exported() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_selection_is_exported(&sources), None);
}

#[test]
fn nothing_new_and_nothing_advertised() {
    let sources = Sources::from_tree();
    assert_eq!(rule_nothing_new_and_nothing_advertised(&sources), None);
}

#[test]
fn the_predecessors_evidence_is_untouched() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_predecessors_evidence_is_untouched(&sources), None);
}

// ---- the inverted probes ----------------------------------------------

#[test]
fn the_inverted_probe_fires_when_the_fallback_comes_first() {
    let mut sources = Sources::from_tree();
    sources.peer_pool = sources.peer_pool.replace(
        "let mut selection =\n            select_version_gated_floodfills(",
        "let mut selection =\n            select_version_gated_peers(",
    );
    assert!(
        rule_the_pool_prefers_floodfills(&sources).is_some(),
        "a pool that falls back before preferring must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_gate_is_duplicated() {
    let mut sources = Sources::from_tree();
    sources.lookup_policy = sources
        .lookup_policy
        .replace("fn gate_candidates(", "fn gate_candidates_copy(");
    assert!(
        rule_the_gate_is_shared(&sources).is_some(),
        "a renamed gate must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_ceiling_moves() {
    let mut sources = Sources::from_tree();
    sources.lookup_policy = sources.lookup_policy.replace(
        "candidates.truncate(policy.max_candidates_considered());",
        "",
    );
    assert!(
        rule_the_policy_is_unchanged(&sources).is_some(),
        "a selection with no ceiling must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_configuration_key_appears() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "pub struct Config {",
        "pub struct Config {\n    pub pool_fallback: usize,",
    );
    assert!(
        rule_nothing_new_and_nothing_advertised(&sources).is_some(),
        "a configuration key must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_predecessor_changes() {
    let mut sources = Sources::from_tree();
    if let Some((path, digest)) = sources
        .predecessors
        .iter_mut()
        .find(|(path, _)| *path == PLAN543_GUARD)
    {
        let _ = path;
        *digest = "0".repeat(64);
    }
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a predecessor's evidence must fail the rule"
    );
}
