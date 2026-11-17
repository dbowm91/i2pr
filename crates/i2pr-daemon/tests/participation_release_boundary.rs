//! Plan 543's guard -- the participation ledger must not exhaust the
//! exploratory pool.
//!
//! `ParticipationLedger` records how often each peer has been selected, and the
//! selection **skips** any peer whose counter has reached the cap. The counter
//! **saturates** at the cap, and the ledger's only other release is eviction of
//! the **oldest** entry when it is **full**. The ledger tracks 4 096 peers while
//! the profile store's population is the bounded 256-record prefix, so eviction
//! never fires and a saturated entry would exclude its peer for as long as the
//! router runs.
//!
//! The guard is the authority for the **shape** of the release:
//!
//! 1. the ledger's entry carries an instant, and `record` stamps it;
//! 2. the ledger has a **reclaim**, and the driver calls it on the clock it
//!    already advances;
//! 3. the window is a `const` derived through a `const fn`, and it is **not**
//!    restated anywhere;
//! 4. the counter's saturation and the ledger's entry bound are **unchanged**,
//!    and the skip rule still reads the counter map;
//! 5. the selection's order, its version gate and its tier gate are unchanged,
//!    and **no** configuration key is added;
//! 6. **no** task, timer, listener or socket is added, and nothing is
//!    advertised;
//! 7. the predecessors' lanes and guards stay **byte-identical**.
//!
//! Every rule has an inverted probe that fires it against an **in-memory copy**
//! of the source, and every rule reports a missing item rather than passing.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const SELECTION: &str = "crates/i2pr-netdb/src/selection.rs";
const DRIVER: &str = "crates/i2pr-daemon/src/netdb_participation.rs";
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

/// PLAN505_LANE, byte for byte.
const PLAN505_LANE_SHA256: &str =
    "5d15348eca4be8cce785537bf322f07c5fbe9a482ee0668b4f88c5c0c345ff79";
/// PLAN505_GUARD, byte for byte.
const PLAN505_GUARD_SHA256: &str =
    "3a0efb877ad7b5626130bccb97450e1a326502acd486a3496a59cd3c43644622";
/// PLAN506_LANE, byte for byte.
const PLAN506_LANE_SHA256: &str =
    "ffabd287ec94cdcd044e58784b97dd5ae9f920ce016627d2792f87801b5ef28f";
/// PLAN506_GUARD, byte for byte.
const PLAN506_GUARD_SHA256: &str =
    "33db28ccc76a52338b381b20c9d44ca2df9152d90d4f56a29a4279ad4c4eaafc";
/// PLAN507_LANE, byte for byte.
const PLAN507_LANE_SHA256: &str =
    "709ec1b47e731bd71edb7ca65f4ef889e4695f36cf5a012cf7d70b5ceae2c5da";
/// PLAN507_GUARD, byte for byte.
const PLAN507_GUARD_SHA256: &str =
    "5ef83044dcb8036e0ac2ccc8e7e9cb9bf6b7a21127b822b6e634da6170fdd78d";
/// PLAN538_LANE, byte for byte.
const PLAN538_LANE_SHA256: &str =
    "d1ce001d1e7562ca84b33cd9b4b0bca04325565b7cfef2c7504645825c8bc613";
/// PLAN538_GUARD, byte for byte.
const PLAN538_GUARD_SHA256: &str =
    "3d059e0690a0ab52b5c54619c0b5220eac8f340168187c2541f12f597c518fa4";
/// PLAN539_LANE, byte for byte.
const PLAN539_LANE_SHA256: &str =
    "068410ef125b2122b1acf56866abff78010f6fe20dc4ef933d80a7c4e6d99103";
/// PLAN539_GUARD, byte for byte.
const PLAN539_GUARD_SHA256: &str =
    "60ef63b653835cbb1af7943aea64662fe6970eed5827b1266a92994dea2ae300";
/// PLAN540_LANE, byte for byte.
const PLAN540_LANE_SHA256: &str =
    "369a69a0ce776fbb44f3a0bfec164a7296755e19ccea01f3f0fd7ca6a6739468";
/// PLAN540_GUARD, byte for byte.
const PLAN540_GUARD_SHA256: &str =
    "da81058d9e32b1e36332e3f031788c93b87c1430dcbb1d753759c227634586a4";
/// PLAN541_LANE, byte for byte.
const PLAN541_LANE_SHA256: &str =
    "85190546dd7e371ae140cc4e9b964caa32100a3ca44c830986034593b7cfeb41";
/// PLAN541_GUARD, byte for byte.
const PLAN541_GUARD_SHA256: &str =
    "351080b5f5f219a5b54a7a732556185e1ef18b108b4e948e5597d68c8b02ded7";
/// PLAN542_LANE, byte for byte.
const PLAN542_LANE_SHA256: &str =
    "0a0657a444fd5de046bb7163b6d988bc75ec4f30b63770c5b93248b352a8184e";
/// PLAN542_GUARD, byte for byte.
const PLAN542_GUARD_SHA256: &str =
    "6a610f3f3af2926870ca4d468f8ce45a80d7ba400c27c6e3eb013461d7689800";

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
    selection: String,
    driver: String,
    daemon_config: String,
    netdb_local: String,
    support: String,
    manifest_sha256: String,
    lock_sha256: String,
    plan505_lane_sha256: String,
    plan505_guard_sha256: String,
    plan506_lane_sha256: String,
    plan506_guard_sha256: String,
    plan507_lane_sha256: String,
    plan507_guard_sha256: String,
    plan538_lane_sha256: String,
    plan538_guard_sha256: String,
    plan539_lane_sha256: String,
    plan539_guard_sha256: String,
    plan540_lane_sha256: String,
    plan540_guard_sha256: String,
    plan541_lane_sha256: String,
    plan541_guard_sha256: String,
    plan542_lane_sha256: String,
    plan542_guard_sha256: String,
}

impl Sources {
    fn from_tree() -> Sources {
        Sources {
            selection: source_of(SELECTION),
            driver: source_of(DRIVER),
            daemon_config: source_of(CONFIG),
            netdb_local: source_of(NETDB_LOCAL),
            support: source_of(SUPPORT_TOML),
            manifest_sha256: sha256_hex(source_of(MANIFEST).as_bytes()),
            lock_sha256: sha256_hex(source_of(LOCK).as_bytes()),
            plan505_lane_sha256: sha256_hex(source_of(PLAN505_LANE).as_bytes()),
            plan505_guard_sha256: sha256_hex(source_of(PLAN505_GUARD).as_bytes()),
            plan506_lane_sha256: sha256_hex(source_of(PLAN506_LANE).as_bytes()),
            plan506_guard_sha256: sha256_hex(source_of(PLAN506_GUARD).as_bytes()),
            plan507_lane_sha256: sha256_hex(source_of(PLAN507_LANE).as_bytes()),
            plan507_guard_sha256: sha256_hex(source_of(PLAN507_GUARD).as_bytes()),
            plan538_lane_sha256: sha256_hex(source_of(PLAN538_LANE).as_bytes()),
            plan538_guard_sha256: sha256_hex(source_of(PLAN538_GUARD).as_bytes()),
            plan539_lane_sha256: sha256_hex(source_of(PLAN539_LANE).as_bytes()),
            plan539_guard_sha256: sha256_hex(source_of(PLAN539_GUARD).as_bytes()),
            plan540_lane_sha256: sha256_hex(source_of(PLAN540_LANE).as_bytes()),
            plan540_guard_sha256: sha256_hex(source_of(PLAN540_GUARD).as_bytes()),
            plan541_lane_sha256: sha256_hex(source_of(PLAN541_LANE).as_bytes()),
            plan541_guard_sha256: sha256_hex(source_of(PLAN541_GUARD).as_bytes()),
            plan542_lane_sha256: sha256_hex(source_of(PLAN542_LANE).as_bytes()),
            plan542_guard_sha256: sha256_hex(source_of(PLAN542_GUARD).as_bytes()),
        }
    }
}

// ---- 1. the entry carries an instant, and `record` stamps it --------------

fn rule_the_entry_carries_an_instant(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.selection);

    let Some(ledger) = item_body(&code, "pub struct ParticipationLedger") else {
        return Some("the ledger is absent".to_owned());
    };
    if !ledger.contains("stamps: BTreeMap<RouterHash, u64>,") {
        return Some("the ledger must carry a per-peer instant".to_owned());
    }
    // The construction initialises it.
    if !code.contains("stamps: BTreeMap::new(),") {
        return Some("the ledger's stamps must be initialised".to_owned());
    }
    // `record` takes the instant and stamps the entry.
    let Some(record) = item_body(
        &code,
        "pub fn record(&mut self, key: &RouterHash, now_ms: u64)",
    ) else {
        return Some("the ledger's record must take the caller's instant".to_owned());
    };
    if !record.contains("self.stamps.insert(*key, now_ms);") {
        return Some("record must stamp the entry".to_owned());
    }
    // Eviction releases the stamp with the counter.
    if !record.contains("self.stamps.remove(&oldest);") {
        return Some("eviction must release the stamp with the counter".to_owned());
    }
    None
}

// ---- 2. the ledger reclaims, and the driver calls it ---------------------

fn rule_the_driver_reclaims_on_its_clock(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.selection);

    let Some(reclaim) = item_body(&code, "pub fn reclaim(&mut self, now_ms: u64) -> usize") else {
        return Some("the ledger's reclaim is absent".to_owned());
    };
    if !reclaim.contains("PARTICIPATION_WINDOW_MS") {
        return Some("the reclaim must use the window".to_owned());
    }
    // It releases the counter, the stamp and the eviction order together.
    for needle in [
        "self.counts.remove(key);",
        "self.stamps.remove(key);",
        "self.order.retain(",
    ] {
        if !reclaim.contains(needle) {
            return Some(format!("the reclaim must release {needle}"));
        }
    }

    // The driver calls it on the clock it already advances.
    let driver = code_view(&sources.driver);
    let Some(advance) = item_body(&driver, "pub fn advance_time(&mut self, now_ms: u64)") else {
        return Some("the driver's advance_time is absent".to_owned());
    };
    if !advance.contains("self.ledger.reclaim(now_ms);") {
        return Some("the driver must reclaim on its clock".to_owned());
    }
    // And every recording site passes the driver's clock.
    let stamped = driver
        .matches("self.ledger.record(&candidate.key, self.clock_ms);")
        .count();
    if stamped < 2 {
        return Some(format!(
            "every recording site must stamp the driver's clock: {stamped}"
        ));
    }
    None
}

// ---- 3. the window is derived, and is not restated ----------------------

fn rule_the_window_is_derived_and_not_restated(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.selection);

    if !code
        .contains("pub const PARTICIPATION_WINDOW_MS: u64 = participation_window_secs() * 1_000;")
    {
        return Some("the window must be derived from the seconds helper".to_owned());
    }
    if !code.contains("pub const fn participation_window_secs() -> u64 {") {
        return Some("the window's seconds helper must be a const fn".to_owned());
    }
    // One definition of the window, in the whole tree's two changed modules.
    let total = code.matches("const PARTICIPATION_WINDOW_MS").count()
        + code_view(&sources.driver)
            .matches("const PARTICIPATION_WINDOW_MS")
            .count();
    if total != 1 {
        return Some(format!("the window is defined {total} times, not once"));
    }
    // The driver must not restate the window.
    for forbidden in ["86_400", "86400", "1_800_000", "1800"] {
        if code_view(&sources.driver).contains(forbidden) {
            return Some(format!(
                "the driver must not restate the window: {forbidden}"
            ));
        }
    }
    None
}

// ---- 4. the saturation, the bound and the skip rule are unchanged --------

fn rule_the_bounds_and_the_skip_rule_are_unchanged(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.selection);

    if !code.contains("pub const MAX_PARTICIPATION_TRACKED_PEERS: usize = 4_096;") {
        return Some("the ledger's entry bound must not move".to_owned());
    }
    if !code.contains("pub const DEFAULT_MAX_PARTICIPATION: u32 = 8;") {
        return Some("the participation cap must not move".to_owned());
    }
    // The counter still saturates at the cap.
    let Some(record) = item_body(
        &code,
        "pub fn record(&mut self, key: &RouterHash, now_ms: u64)",
    ) else {
        return Some("the ledger's record is absent".to_owned());
    };
    if !record.contains(".min(self.max_participation)") {
        return Some("the counter must still saturate at the cap".to_owned());
    }
    // The skip rule still reads the counter map, so a released peer reads zero.
    if !code.contains("constraints.participation.get(key).copied().unwrap_or(0)") {
        return Some("the skip rule must still read the counter map".to_owned());
    }
    // The counter view's type is unchanged, so no caller has to move.
    if !code.contains("pub fn counts(&self) -> &BTreeMap<RouterHash, u32> {") {
        return Some("the counter view's type must not change".to_owned());
    }
    None
}

// ---- 5. the selection's other gates are unchanged -----------------------

fn rule_the_selection_keeps_its_gates(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.selection);

    for needle in [
        "constraints.max_participation > 0",
        "constraints.participation.get(key).copied().unwrap_or(0)",
        "if let Some(gate) = constraints.version_gate {",
        "if let Some(min_tier) = constraints.min_tier",
    ] {
        if !code.contains(needle) {
            return Some(format!("the selection lost a gate: {needle}"));
        }
    }

    // No configuration key is added.
    let config = code_view(&sources.daemon_config);
    for token in [
        "participation_window",
        "participation_release",
        "ledger_window",
        "participation_expiry",
    ] {
        if config.to_lowercase().contains(token) {
            return Some(format!("the configuration must gain no key: {token}"));
        }
    }
    None
}

// ---- 6. nothing new runs, and nothing is advertised ---------------------

fn rule_nothing_new_runs_and_nothing_is_advertised(sources: &Sources) -> Option<String> {
    for (label, code) in [
        ("the ledger", code_view(&sources.selection)),
        ("the driver", code_view(&sources.driver)),
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

// ---- 7. the predecessors' evidence is untouched ------------------------

fn rule_the_predecessors_evidence_is_untouched(sources: &Sources) -> Option<String> {
    for (label, digest, expected) in [
        (
            "Plan 505's lane",
            sources.plan505_lane_sha256.clone(),
            PLAN505_LANE_SHA256,
        ),
        (
            "Plan 505's guard",
            sources.plan505_guard_sha256.clone(),
            PLAN505_GUARD_SHA256,
        ),
        (
            "Plan 506's lane",
            sources.plan506_lane_sha256.clone(),
            PLAN506_LANE_SHA256,
        ),
        (
            "Plan 506's guard",
            sources.plan506_guard_sha256.clone(),
            PLAN506_GUARD_SHA256,
        ),
        (
            "Plan 507's lane",
            sources.plan507_lane_sha256.clone(),
            PLAN507_LANE_SHA256,
        ),
        (
            "Plan 507's guard",
            sources.plan507_guard_sha256.clone(),
            PLAN507_GUARD_SHA256,
        ),
        (
            "Plan 538's lane",
            sources.plan538_lane_sha256.clone(),
            PLAN538_LANE_SHA256,
        ),
        (
            "Plan 538's guard",
            sources.plan538_guard_sha256.clone(),
            PLAN538_GUARD_SHA256,
        ),
        (
            "Plan 539's lane",
            sources.plan539_lane_sha256.clone(),
            PLAN539_LANE_SHA256,
        ),
        (
            "Plan 539's guard",
            sources.plan539_guard_sha256.clone(),
            PLAN539_GUARD_SHA256,
        ),
        (
            "Plan 540's lane",
            sources.plan540_lane_sha256.clone(),
            PLAN540_LANE_SHA256,
        ),
        (
            "Plan 540's guard",
            sources.plan540_guard_sha256.clone(),
            PLAN540_GUARD_SHA256,
        ),
        (
            "Plan 541's lane",
            sources.plan541_lane_sha256.clone(),
            PLAN541_LANE_SHA256,
        ),
        (
            "Plan 541's guard",
            sources.plan541_guard_sha256.clone(),
            PLAN541_GUARD_SHA256,
        ),
        (
            "Plan 542's lane",
            sources.plan542_lane_sha256.clone(),
            PLAN542_LANE_SHA256,
        ),
        (
            "Plan 542's guard",
            sources.plan542_guard_sha256.clone(),
            PLAN542_GUARD_SHA256,
        ),
    ] {
        if digest != expected {
            return Some(format!("{label} must stay byte-identical"));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

#[test]
fn the_entry_carries_an_instant() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_entry_carries_an_instant(&sources), None);
}

#[test]
fn the_driver_reclaims_on_its_clock() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_driver_reclaims_on_its_clock(&sources), None);
}

#[test]
fn the_window_is_derived_and_not_restated() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_window_is_derived_and_not_restated(&sources), None);
}

#[test]
fn the_bounds_and_the_skip_rule_are_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_bounds_and_the_skip_rule_are_unchanged(&sources),
        None
    );
}

#[test]
fn the_selection_keeps_its_gates() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_selection_keeps_its_gates(&sources), None);
}

#[test]
fn nothing_new_runs_and_nothing_is_advertised() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_nothing_new_runs_and_nothing_is_advertised(&sources),
        None
    );
}

#[test]
fn the_predecessors_evidence_is_untouched() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_predecessors_evidence_is_untouched(&sources), None);
}

// ----------------------------------------------------------------------
// The inverted probes.
// ----------------------------------------------------------------------

#[test]
fn the_inverted_probe_fires_when_the_entry_has_no_instant() {
    let mut sources = Sources::from_tree();
    sources.selection = sources
        .selection
        .replace("stamps: BTreeMap<RouterHash, u64>,", "");
    assert!(
        rule_the_entry_carries_an_instant(&sources).is_some(),
        "a ledger with no instant must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_driver_does_not_reclaim() {
    let mut sources = Sources::from_tree();
    sources.driver = sources.driver.replace("self.ledger.reclaim(now_ms);", "");
    assert!(
        rule_the_driver_reclaims_on_its_clock(&sources).is_some(),
        "a driver that never reclaims must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_window_is_restated() {
    let mut sources = Sources::from_tree();
    sources.driver = sources.driver.replace(
        "let now_ms = wall_clock_ms();",
        "let now_ms = wall_clock_ms().min(1_800_000);",
    );
    assert!(
        rule_the_window_is_derived_and_not_restated(&sources).is_some(),
        "a restated window must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_cap_moves() {
    let mut sources = Sources::from_tree();
    sources.selection = sources.selection.replace(
        "pub const DEFAULT_MAX_PARTICIPATION: u32 = 8;",
        "pub const DEFAULT_MAX_PARTICIPATION: u32 = 64;",
    );
    assert!(
        rule_the_bounds_and_the_skip_rule_are_unchanged(&sources).is_some(),
        "a moved cap must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_skip_rule_is_replaced() {
    let mut sources = Sources::from_tree();
    sources.selection = sources.selection.replace(
        "constraints.participation.get(key).copied().unwrap_or(0)",
        "0u32",
    );
    assert!(
        rule_the_bounds_and_the_skip_rule_are_unchanged(&sources).is_some(),
        "a replaced skip rule must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_configuration_key_appears() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "pub struct Config {",
        "pub struct Config {\n    pub participation_window: usize,",
    );
    assert!(
        rule_the_selection_keeps_its_gates(&sources).is_some(),
        "a configuration key must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_timer_appears() {
    let mut sources = Sources::from_tree();
    sources.driver = sources.driver.replace(
        "self.ledger.reclaim(now_ms);",
        "self.ledger.reclaim(now_ms);\n        let _task = tokio::spawn(async {});",
    );
    assert!(
        rule_nothing_new_runs_and_nothing_is_advertised(&sources).is_some(),
        "an ownerless task must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_predecessor_changes() {
    let mut sources = Sources::from_tree();
    sources.plan542_guard_sha256 = "0".repeat(64);
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a predecessor's evidence must fail the rule"
    );
}
