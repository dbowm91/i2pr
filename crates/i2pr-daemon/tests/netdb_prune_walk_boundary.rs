//! Plan 540's guard -- the NetDB's prune must not walk the population.
//!
//! The validated store's prune found the stale records by walking **every**
//! retained record, and the daemon's peer reconcile calls it **under the
//! router's NetDB lock** -- the bootstrap's mutex, which *is* the router's
//! NetDB lock. The prune is on the router's live maintenance cadence, so the
//! lock was held across an O(population) walk and every other NetDB reader
//! queued behind it.
//!
//! The store now maintains an **age index** -- the retained records'
//! publication times, ordered -- so the prune walks only the **stale**
//! records and its cost is proportional to what it removes.
//!
//! The guard is the authority for the **shape**:
//!
//! 1. the store **maintains** an age index, and the index is updated on
//!    **every** path that inserts or removes a record;
//! 2. the prune walks the **index** and **not** the record map;
//! 3. the prune's **signature** is unchanged, and the store's other public
//!    surfaces are unchanged;
//! 4. the daemon's prune call site and the reconcile's shape are unchanged;
//! 5. **no** configuration key is added, **no** dependency is added, **no**
//!    task, timer, listener or socket is added, and **nothing** is
//!    advertised;
//! 6. Plan 507's, Plan 538's and Plan 539's lanes and guards stay
//!    **byte-identical**.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, every rule has a
//! probe that removes the item it names so it **fails closed**, and every rule
//! reports a missing item rather than passing.
//!
//! The guard cannot observe that the prune is **cheap** at the moment it
//! runs -- that is a fact about the running program. The lane observes what
//! the prune *does*, and the two claims do not overlap.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const NETDB_STORE: &str = "crates/i2pr-netdb/src/store.rs";
const DAEMON_BOOTSTRAP: &str = "crates/i2pr-daemon/src/bootstrap.rs";
const DAEMON_RECONCILE: &str = "crates/i2pr-daemon/src/peer_reconcile.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const NETDB_LOCAL: &str = "crates/i2pr-netdb/src/local.rs";
const SUPPORT_TOML: &str = "specs/support.toml";
const MANIFEST: &str = "Cargo.toml";
const LOCK: &str = "Cargo.lock";
const PLAN507_LANE: &str = "crates/i2pr-daemon/tests/netdb_retention_live.rs";
const PLAN507_GUARD: &str = "crates/i2pr-daemon/tests/netdb_retention_boundary.rs";
const PLAN538_LANE: &str = "crates/i2pr-daemon/tests/floodfill_statistic_accounting.rs";
const PLAN538_GUARD: &str = "crates/i2pr-daemon/tests/floodfill_statistic_accounting_boundary.rs";
const PLAN539_LANE: &str = "crates/i2pr-daemon/tests/routing_table_population_lock.rs";
const PLAN539_GUARD: &str = "crates/i2pr-daemon/tests/routing_table_population_lock_boundary.rs";

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

/// Removes every whitespace character.
fn compact(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
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
    netdb_store: String,
    daemon_bootstrap: String,
    daemon_reconcile: String,
    daemon_config: String,
    netdb_local: String,
    support: String,
    manifest_sha256: String,
    lock_sha256: String,
    plan507_lane_sha256: String,
    plan507_guard_sha256: String,
    plan538_lane_sha256: String,
    plan538_guard_sha256: String,
    plan539_lane_sha256: String,
    plan539_guard_sha256: String,
}

impl Sources {
    fn from_tree() -> Sources {
        Sources {
            netdb_store: source_of(NETDB_STORE),
            daemon_bootstrap: source_of(DAEMON_BOOTSTRAP),
            daemon_reconcile: source_of(DAEMON_RECONCILE),
            daemon_config: source_of(DAEMON_CONFIG),
            netdb_local: source_of(NETDB_LOCAL),
            support: source_of(SUPPORT_TOML),
            manifest_sha256: sha256_hex(source_of(MANIFEST).as_bytes()),
            lock_sha256: sha256_hex(source_of(LOCK).as_bytes()),
            plan507_lane_sha256: sha256_hex(source_of(PLAN507_LANE).as_bytes()),
            plan507_guard_sha256: sha256_hex(source_of(PLAN507_GUARD).as_bytes()),
            plan538_lane_sha256: sha256_hex(source_of(PLAN538_LANE).as_bytes()),
            plan538_guard_sha256: sha256_hex(source_of(PLAN538_GUARD).as_bytes()),
            plan539_lane_sha256: sha256_hex(source_of(PLAN539_LANE).as_bytes()),
            plan539_guard_sha256: sha256_hex(source_of(PLAN539_GUARD).as_bytes()),
        }
    }
}

// ---- 1. the store maintains an age index ---------------------------------

fn rule_the_store_maintains_an_age_index(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.netdb_store);

    // The store **holds** the index.
    let Some(store) = item_body(&code, "pub struct RouterInfoStore") else {
        return Some("the validated store is absent".to_owned());
    };
    if !store.contains("ages: BTreeSet<(u64, RouterHash)>,") {
        return Some("the store must maintain an age index".to_owned());
    }
    if !code.contains("use std::collections::{BTreeMap, BTreeSet};") {
        return Some("the store must import the ordered set".to_owned());
    }

    // The **insert** path maintains it on **both** of its paths: the new-key
    // path gains the record, and the replacement path swaps the old entry for
    // the new one.
    let Some(insert) = item_body(
        &code,
        "pub fn insert(&mut self, validated: ValidatedRouterInfo) -> InsertOutcome",
    ) else {
        return Some("the store's insert path is absent".to_owned());
    };
    let gained = insert.matches("self.ages.insert(").count();
    if gained != 2 {
        return Some(format!(
            "the insert path must index the new-key and the replacement path: {gained}"
        ));
    }
    if !insert.contains("self.ages.remove(") {
        return Some("the replacement path must release the replaced record's entry".to_owned());
    }

    // The **removal** path releases the entry with the record.
    let Some(remove) = item_body(&code, "pub fn remove(&mut self, key: &RouterHash) -> bool")
    else {
        return Some("the store's removal path is absent".to_owned());
    };
    if !remove.contains("self.ages.remove(") {
        return Some("the removal path must release the record's entry".to_owned());
    }

    // And the index is never rebuilt by a scan: neither path walks the
    // record map.
    for path in ["insert", "remove"] {
        let Some(body) = item_body(
            &code,
            if path == "insert" {
                "pub fn insert(&mut self, validated: ValidatedRouterInfo) -> InsertOutcome"
            } else {
                "pub fn remove(&mut self, key: &RouterHash) -> bool"
            },
        ) else {
            return Some(format!("the store's {path} path is absent"));
        };
        for forbidden in [
            "self.records.iter()",
            "self.records.values()",
            "self.records.keys()",
        ] {
            if body.contains(forbidden) {
                return Some(format!(
                    "the index must be maintained, not rebuilt by a scan: {forbidden}"
                ));
            }
        }
    }
    None
}

// ---- 2. the prune walks the index and not the record map ------------------

fn rule_the_prune_walks_the_index_and_not_the_record_map(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.netdb_store);

    let Some(prune) = item_body(
        &code,
        "pub fn prune(&mut self, now: Date, max_age_ms: u64) -> usize",
    ) else {
        return Some("the store's prune is absent".to_owned());
    };

    // The prune reads the **index**.  The source wraps the chain across
    // lines, so the text is squeezed before it is matched.
    let squeezed = compact(&prune);
    if !squeezed.contains("self.ages") {
        return Some("the prune must walk the age index".to_owned());
    }
    if !squeezed.contains(".range(..)") {
        return Some("the prune must walk the index's ordered prefix".to_owned());
    }

    // And it never walks the record map.
    for forbidden in [
        "self.records",
        ".iter()",
        ".iter_mut()",
        ".values()",
        ".values_mut()",
        ".keys()",
        ".filter(",
        ".filter_map(",
    ] {
        if prune.contains(forbidden) {
            return Some(format!(
                "the prune must not walk the record map: {forbidden}"
            ));
        }
    }
    None
}

// ---- 3. the prune's signature and the store's surfaces are unchanged ------

fn rule_the_prune_signature_and_the_surfaces_are_unchanged(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.netdb_store);

    // The prune's own signature is pinned, because Plan 507 pins it too.
    if !compact(&code).contains(&compact(
        "pub fn prune(&mut self, now: Date, max_age_ms: u64) -> usize",
    )) {
        return Some("the prune's signature must not move".to_owned());
    }

    // And the store's other public surfaces are exactly what they were.
    for signature in [
        "pub fn insert(&mut self, validated: ValidatedRouterInfo) -> InsertOutcome {",
        "pub fn remove(&mut self, key: &RouterHash) -> bool {",
        "pub fn get(&self, key: &RouterHash) -> Option<&ValidatedRouterInfo> {",
        "pub fn contains(&self, key: &RouterHash) -> bool {",
        "pub fn len(&self) -> usize {",
        "pub fn is_empty(&self) -> bool {",
        "pub fn encoded_bytes(&self) -> usize {",
        "pub fn config(&self) -> RouterInfoStoreConfig {",
        "pub fn stats(&self) -> RouterInfoStoreStats {",
        "pub fn iter(&self) -> impl Iterator<Item = (&RouterHash, &ValidatedRouterInfo)> {",
        "pub fn floodfill_advertisers(&self) -> impl Iterator<Item = &ValidatedRouterInfo> {",
    ] {
        if !compact(&code).contains(&compact(signature)) {
            return Some(format!(
                "the store's surface must be unchanged: {signature}"
            ));
        }
    }
    None
}

// ---- 4. the daemon's call site and the reconcile's shape are unchanged ----

fn rule_the_call_site_and_the_reconcile_are_unchanged(sources: &Sources) -> Option<String> {
    let bootstrap = code_view(&sources.daemon_bootstrap);
    let reconcile = code_view(&sources.daemon_reconcile);

    // The bootstrap still calls the store's own prune, exactly as Plan 507
    // pins it.
    let Some(prune) = item_body(
        &bootstrap,
        "pub fn prune(&mut self, now_ms: u64, window_ms: u64) -> usize",
    ) else {
        return Some("the bootstrap's prune is absent".to_owned());
    };
    if !prune.contains("self.store.prune(Date::from_millis(now_ms), window_ms)") {
        return Some("the bootstrap must call the store's own prune".to_owned());
    }

    // The reconcile still prunes the bootstrap under the bootstrap's lock ...
    let Some(prune_bootstrap) = item_body(&reconcile, "fn prune_bootstrap") else {
        return Some("the reconcile's bootstrap prune is absent".to_owned());
    };
    if !prune_bootstrap.contains("bootstrap.lock()") {
        return Some("the reconcile's bootstrap prune must take the bootstrap's lock".to_owned());
    }
    if !prune_bootstrap.contains("guard.prune(now_ms, RETENTION_WINDOW_MS)") {
        return Some("the reconcile's bootstrap prune must call the bootstrap's prune".to_owned());
    }

    // ... and the destination prune still tries its lock.
    let Some(prune_destinations) = item_body(&reconcile, "fn prune_destinations") else {
        return Some("the reconcile's destination prune is absent".to_owned());
    };
    if !prune_destinations.contains("try_lock(") {
        return Some("the reconcile's destination prune must try its lock".to_owned());
    }

    // And the prune still runs before the reconcile's snapshot.
    let Some(run) = item_body(&reconcile, "fn run") else {
        return Some("the reconcile's run is absent".to_owned());
    };
    let Some(prune_at) = run.find("self.prune_stores(") else {
        return Some("the reconcile must prune its stores".to_owned());
    };
    let Some(snapshot_at) = run.find("self.snapshot()") else {
        return Some("the reconcile must snapshot its stores".to_owned());
    };
    if prune_at > snapshot_at {
        return Some("the prune must run before the snapshot".to_owned());
    }
    None
}

// ---- 5. no new key, no new dependency, no new infrastructure -------------

fn rule_no_new_key_and_nothing_is_advertised(sources: &Sources) -> Option<String> {
    // The daemon's configuration gains **no** key: the index is not a budget,
    // so it has no knob.
    let config = code_view(&sources.daemon_config);
    for token in ["prune", "retention", "age_index", "prune_budget"] {
        if config.to_lowercase().contains(token) {
            return Some(format!("the configuration must gain no key: {token}"));
        }
    }

    // The changed module stays runtime-neutral: it owns no socket, no timer
    // and no task.
    let store = code_view(&sources.netdb_store);
    for forbidden in [
        "tokio::",
        "std::net",
        "std::fs",
        "std::thread",
        "JoinHandle",
        "spawn(",
    ] {
        if store.contains(forbidden) {
            return Some(format!("the store must stay runtime-neutral: {forbidden}"));
        }
    }

    // The manifest and the lock are byte-identical: **no** dependency is
    // added.
    if sources.manifest_sha256 != CARGO_TOML_SHA256 {
        return Some("Cargo.toml must stay byte-identical".to_owned());
    }
    if sources.lock_sha256 != CARGO_LOCK_SHA256 {
        return Some("Cargo.lock must stay byte-identical".to_owned());
    }

    // And nothing is advertised.
    let local = code_view(&sources.netdb_local);
    if !local.contains("pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.62\";") {
        return Some("the controlled router version must not move".to_owned());
    }
    let support = &sources.support;
    if support.contains("advertised = true") {
        return Some("the inventory must advertise nothing".to_owned());
    }
    None
}

// ---- 6. the predecessors' evidence is untouched ----------------------------

fn rule_the_predecessors_evidence_is_untouched(sources: &Sources) -> Option<String> {
    for (label, digest, expected) in [
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
fn the_store_maintains_an_age_index() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_store_maintains_an_age_index(&sources), None);
}

#[test]
fn the_prune_walks_the_index_and_not_the_record_map() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_prune_walks_the_index_and_not_the_record_map(&sources),
        None
    );
}

#[test]
fn the_prune_signature_and_the_surfaces_are_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_prune_signature_and_the_surfaces_are_unchanged(&sources),
        None
    );
}

#[test]
fn the_call_site_and_the_reconcile_are_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_call_site_and_the_reconcile_are_unchanged(&sources),
        None
    );
}

#[test]
fn no_new_key_and_nothing_is_advertised() {
    let sources = Sources::from_tree();
    assert_eq!(rule_no_new_key_and_nothing_is_advertised(&sources), None);
}

#[test]
fn the_predecessors_evidence_is_untouched() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_predecessors_evidence_is_untouched(&sources), None);
}

// ----------------------------------------------------------------------
// The inverted probes: each fires its rule against an in-memory copy.
// ----------------------------------------------------------------------

#[test]
fn the_inverted_probe_fires_when_the_index_is_not_maintained() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources
        .netdb_store
        .replace("self.ages.insert(", "self.no_index(");
    assert!(
        rule_the_store_maintains_an_age_index(&sources).is_some(),
        "a store that does not maintain the index must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_removal_does_not_release_the_entry() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "self.ages.remove(&(removed.published().as_millis(), *key));",
        "",
    );
    assert!(
        rule_the_store_maintains_an_age_index(&sources).is_some(),
        "a removal path that keeps the entry must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_prune_walks_the_record_map() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "            .ages
            .range(..)",
        "            .records
            .iter()",
    );
    assert!(
        rule_the_prune_walks_the_index_and_not_the_record_map(&sources).is_some(),
        "a prune that walks the record map must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_prune_signature_moves() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "pub fn prune(&mut self, now: Date, max_age_ms: u64) -> usize {",
        "pub fn prune(&mut self, now: Date, max_age_ms: u64, budget: usize) -> usize {",
    );
    assert!(
        rule_the_prune_signature_and_the_surfaces_are_unchanged(&sources).is_some(),
        "a prune whose signature moves must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_call_site_moves() {
    let mut sources = Sources::from_tree();
    sources.daemon_bootstrap = sources.daemon_bootstrap.replace(
        "self.store.prune(Date::from_millis(now_ms), window_ms)",
        "self.store.prune(Date::from_millis(now_ms), window_ms, 0)",
    );
    assert!(
        rule_the_call_site_and_the_reconcile_are_unchanged(&sources).is_some(),
        "a call site that moves must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_configuration_key_appears() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "pub struct Config {",
        "pub struct Config {\n    pub prune_budget: usize,",
    );
    assert!(
        rule_no_new_key_and_nothing_is_advertised(&sources).is_some(),
        "a configuration key must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_predecessor_changes() {
    let mut sources = Sources::from_tree();
    sources.plan539_guard_sha256 = "0".repeat(64);
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a predecessor's evidence must fail the rule"
    );
}
