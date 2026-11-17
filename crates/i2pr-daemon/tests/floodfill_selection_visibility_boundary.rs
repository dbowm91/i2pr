//! Plan 541's guard -- the router's floodfill selection must see the router's
//! floodfill advertisers.
//!
//! The router's **authoritative** floodfill selection store -- the
//! `DestinationTunnelCoordinator`'s store, which `candidate_hashes` and
//! `version_gated_candidate_hashes` read -- was fed
//! `Bootstrap::validated_router_info_snapshot()`, and that snapshot is the
//! **first 256 records in ascending `RouterHash` order**. The bound belongs
//! to the **group provider**'s RouterInfo supply, and the floodfill
//! advertisers are distributed across the hash space, so the selection store
//! held an arbitrary subset of the NetDB -- typically no advertiser at all.
//!
//! The guard is the authority for the **shape**:
//!
//! 1. the store **maintains** an advertiser index, and the index is updated
//!    on **every** path that inserts or removes a record;
//! 2. the selection walks the **index** and **still filters** on the
//!    capability;
//! 3. the coordinator's seed and the reconcile's feed carry the
//!    **advertisers**;
//! 4. the **group provider's bound** and the three other consumers of the
//!    bounded snapshot are **unchanged**;
//! 5. **no** configuration key is added, **no** dependency is added, **no**
//!    task, timer, listener or socket is added, and **nothing** is
//!    advertised;
//! 6. Plan 506's, Plan 507's, Plan 538's, Plan 539's and Plan 540's lanes
//!    and guards stay **byte-identical**.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, every rule has a
//! probe that removes the item it names so it **fails closed**, and every rule
//! reports a missing item rather than passing.
//!
//! The guard cannot observe that the selection sees **more** peers than it
//! did -- that is a fact about the running program. The lane observes what
//! the selection **does**, and the two claims do not overlap.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const NETDB_STORE: &str = "crates/i2pr-netdb/src/store.rs";
const DAEMON_BOOTSTRAP: &str = "crates/i2pr-daemon/src/bootstrap.rs";
const DAEMON_RECONCILE: &str = "crates/i2pr-daemon/src/peer_reconcile.rs";
const DAEMON_COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const NETDB_LOCAL: &str = "crates/i2pr-netdb/src/local.rs";
const SUPPORT_TOML: &str = "specs/support.toml";
const MANIFEST: &str = "Cargo.toml";
const LOCK: &str = "Cargo.lock";
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
    netdb_store: String,
    daemon_bootstrap: String,
    daemon_reconcile: String,
    daemon_composition: String,
    daemon_config: String,
    netdb_local: String,
    support: String,
    manifest_sha256: String,
    lock_sha256: String,
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
}

impl Sources {
    fn from_tree() -> Sources {
        Sources {
            netdb_store: source_of(NETDB_STORE),
            daemon_bootstrap: source_of(DAEMON_BOOTSTRAP),
            daemon_reconcile: source_of(DAEMON_RECONCILE),
            daemon_composition: source_of(DAEMON_COMPOSITION),
            daemon_config: source_of(DAEMON_CONFIG),
            netdb_local: source_of(NETDB_LOCAL),
            support: source_of(SUPPORT_TOML),
            manifest_sha256: sha256_hex(source_of(MANIFEST).as_bytes()),
            lock_sha256: sha256_hex(source_of(LOCK).as_bytes()),
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
        }
    }
}

// ---- 1. the store maintains an advertiser index -------------------------

fn rule_the_store_maintains_an_advertiser_index(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.netdb_store);

    // The store **holds** the index.
    let Some(store) = item_body(&code, "pub struct RouterInfoStore") else {
        return Some("the validated store is absent".to_owned());
    };
    if !store.contains("advertisers: BTreeSet<RouterHash>,") {
        return Some("the store must maintain an advertiser index".to_owned());
    }
    if !code.contains("use std::collections::{BTreeMap, BTreeSet};") {
        return Some("the store must import the ordered set".to_owned());
    }

    // The note is the one place the index moves.
    let Some(note) = item_body(
        &code,
        "fn note_advertiser(&mut self, key: RouterHash, is_advertiser: bool)",
    ) else {
        return Some("the store's advertiser note is absent".to_owned());
    };
    if !note.contains("self.advertisers.insert(") {
        return Some("the advertiser note must add the advertiser".to_owned());
    }
    if !note.contains("self.advertisers.remove(") {
        return Some("the advertiser note must release the advertiser".to_owned());
    }

    // The **insert** path notes the capability on **both** of its paths: the
    // new-key path and the replacement path.
    let Some(insert) = item_body(
        &code,
        "pub fn insert(&mut self, validated: ValidatedRouterInfo) -> InsertOutcome",
    ) else {
        return Some("the store's insert path is absent".to_owned());
    };
    let notes = insert.matches("self.note_advertiser(").count();
    if notes != 2 {
        return Some(format!(
            "the insert path must note the advertiser on both of its paths: {notes}"
        ));
    }

    // The **removal** path releases the entry with the record.
    let Some(remove) = item_body(&code, "pub fn remove(&mut self, key: &RouterHash) -> bool")
    else {
        return Some("the store's removal path is absent".to_owned());
    };
    if !remove.contains("self.note_advertiser(") {
        return Some("the removal path must release the advertiser's entry".to_owned());
    }

    // And the index is never rebuilt by a scan.
    for path in ["insert", "remove", "note_advertiser"] {
        let Some(body) = item_body(
            &code,
            if path == "insert" {
                "pub fn insert(&mut self, validated: ValidatedRouterInfo) -> InsertOutcome"
            } else if path == "remove" {
                "pub fn remove(&mut self, key: &RouterHash) -> bool"
            } else {
                "fn note_advertiser(&mut self, key: RouterHash, is_advertiser: bool)"
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

// ---- 2. the selection walks the index and still filters ------------------

fn rule_the_selection_walks_the_index_and_still_filters(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.netdb_store);

    let Some(selection) = item_body(
        &code,
        "pub fn floodfill_advertisers(&self) -> impl Iterator<Item = &ValidatedRouterInfo>",
    ) else {
        return Some("the store's floodfill selection is absent".to_owned());
    };

    // The selection reads the **index**.
    if !selection.contains("self.advertisers") {
        return Some("the selection must walk the advertiser index".to_owned());
    }

    // And it still filters on the capability: the rule is the table's, and
    // the index's word never replaces the record's own evidence.
    if !selection.contains("advertises_floodfill()") {
        return Some("the selection must still filter on the capability".to_owned());
    }

    // And it never scans the record map.
    for forbidden in [
        "self.records.values()",
        "self.records.iter()",
        "self.records.keys()",
        ".values_mut()",
        ".keys()",
    ] {
        if selection.contains(forbidden) {
            return Some(format!(
                "the selection must not scan the record map: {forbidden}"
            ));
        }
    }
    None
}

// ---- 3. the coordinator is fed the advertisers ---------------------------

fn rule_the_coordinator_is_fed_the_advertisers(sources: &Sources) -> Option<String> {
    let bootstrap = code_view(&sources.daemon_bootstrap);
    let reconcile = code_view(&sources.daemon_reconcile);
    let composition = code_view(&sources.daemon_composition);

    // The bootstrap publishes the advertisers.
    let Some(snapshot) = item_body(
        &bootstrap,
        "pub fn floodfill_advertiser_snapshot(&self) -> Vec<i2pr_netdb::ValidatedRouterInfo>",
    ) else {
        return Some("the bootstrap's advertiser snapshot is absent".to_owned());
    };
    if !snapshot.contains("self.store.floodfill_advertiser_snapshot()") {
        return Some("the bootstrap must read the store's advertiser snapshot".to_owned());
    }

    // The Destination-groups seed reads the advertisers ...
    if !composition.contains("bootstrap.floodfill_advertiser_snapshot()") {
        return Some("the Destination-groups seed must read the advertisers".to_owned());
    }
    // ... and not the group provider's prefix.
    if composition.contains("Ok(bootstrap) => bootstrap.validated_router_info_snapshot(),") {
        return Some(
            "the Destination-groups seed must not read the group provider's prefix".to_owned(),
        );
    }

    // The reconcile reads the advertisers ...
    let Some(note) = item_body(
        &reconcile,
        "fn advertiser_snapshot(&self) -> Option<Vec<ValidatedRouterInfo>>",
    ) else {
        return Some("the reconcile's advertiser snapshot is absent".to_owned());
    };
    if !note.contains("guard.floodfill_advertiser_snapshot()") {
        return Some("the reconcile must read the advertisers".to_owned());
    }
    // ... and writes them through the canonical write API.
    let Some(insert) = item_body(
        &reconcile,
        "async fn insert_advertisers(&mut self, records: &[ValidatedRouterInfo])",
    ) else {
        return Some("the reconcile's advertiser feed is absent".to_owned());
    };
    if !insert.contains("insert_validated_router_info(") {
        return Some("the advertiser feed must write through the canonical write API".to_owned());
    }
    if !reconcile.contains("self.insert_advertisers(&advertisers).await;") {
        return Some("the reconcile must feed the coordinator the advertisers".to_owned());
    }
    None
}

// ---- 4. the group provider's bound is unchanged --------------------------

fn rule_the_group_providers_bound_is_unchanged(sources: &Sources) -> Option<String> {
    let bootstrap = code_view(&sources.daemon_bootstrap);
    let reconcile = code_view(&sources.daemon_reconcile);
    let composition = code_view(&sources.daemon_composition);

    // The bound is what it was.
    if !bootstrap.contains("pub const MAX_GROUP_PROVIDER_ROUTER_INFOS: usize = 256;") {
        return Some("the group provider's bound must not move".to_owned());
    }

    // And the group's own snapshot still takes the prefix.
    let Some(snapshot) = item_body(
        &bootstrap,
        "pub fn validated_router_info_snapshot(&self) -> Vec<i2pr_netdb::ValidatedRouterInfo>",
    ) else {
        return Some("the group's bounded snapshot is absent".to_owned());
    };
    if !snapshot.contains(".take(MAX_GROUP_PROVIDER_ROUTER_INFOS)") {
        return Some("the group's bounded snapshot must keep its bound".to_owned());
    }

    // And the three other consumers keep it.
    for needle in [
        "owner.validated_router_info_snapshot()",
        "bootstrap.validated_router_info_snapshot()",
    ] {
        if !composition.contains(needle) {
            return Some(format!(
                "the group's other consumers must keep the bounded prefix: {needle}"
            ));
        }
    }
    if !reconcile.contains("Some(guard.validated_router_info_snapshot())") {
        return Some("the reconcile's own snapshot must keep the bounded prefix".to_owned());
    }
    None
}

// ---- 5. no new key, no new dependency, no new infrastructure -------------

fn rule_no_new_key_and_nothing_is_advertised(sources: &Sources) -> Option<String> {
    // The daemon's configuration gains **no** key: the advertiser set is not
    // a budget, so it has no knob.
    let config = code_view(&sources.daemon_config);
    for token in [
        "prune",
        "retention",
        "age_index",
        "prune_budget",
        "advertiser_index",
        "floodfill_selection",
    ] {
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

// ---- 6. the predecessors' evidence is untouched --------------------------

fn rule_the_predecessors_evidence_is_untouched(sources: &Sources) -> Option<String> {
    for (label, digest, expected) in [
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
fn the_store_maintains_an_advertiser_index() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_store_maintains_an_advertiser_index(&sources), None);
}

#[test]
fn the_selection_walks_the_index_and_still_filters() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_selection_walks_the_index_and_still_filters(&sources),
        None
    );
}

#[test]
fn the_coordinator_is_fed_the_advertisers() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_coordinator_is_fed_the_advertisers(&sources), None);
}

#[test]
fn the_group_providers_bound_is_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_group_providers_bound_is_unchanged(&sources), None);
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
        .replace("self.advertisers.insert(", "self.no_index(");
    assert!(
        rule_the_store_maintains_an_advertiser_index(&sources).is_some(),
        "a store that does not maintain the index must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_removal_does_not_release_the_entry() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources
        .netdb_store
        .replace("                self.note_advertiser(*key, false);\n", "");
    assert!(
        rule_the_store_maintains_an_advertiser_index(&sources).is_some(),
        "a removal path that keeps the entry must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_selection_scans_the_record_map() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "        self.advertisers\n            .iter()\n            .filter_map(|key| self.records.get(key))",
        "        self.records\n            .values()\n            .filter(|record| record.advertises_floodfill())",
    );
    assert!(
        rule_the_selection_walks_the_index_and_still_filters(&sources).is_some(),
        "a selection that scans the record map must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_selection_drops_the_filter() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "            .filter(|record| record.advertises_floodfill())",
        "",
    );
    assert!(
        rule_the_selection_walks_the_index_and_still_filters(&sources).is_some(),
        "a selection that drops the capability filter must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_seed_keeps_the_prefix() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "Ok(bootstrap) => bootstrap.floodfill_advertiser_snapshot(),",
        "Ok(bootstrap) => bootstrap.validated_router_info_snapshot(),",
    );
    assert!(
        rule_the_coordinator_is_fed_the_advertisers(&sources).is_some(),
        "a seed that keeps the group provider's prefix must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_configuration_key_appears() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "pub struct Config {",
        "pub struct Config {\n    pub advertiser_index: usize,",
    );
    assert!(
        rule_no_new_key_and_nothing_is_advertised(&sources).is_some(),
        "a configuration key must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_predecessor_changes() {
    let mut sources = Sources::from_tree();
    sources.plan540_guard_sha256 = "0".repeat(64);
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a predecessor's evidence must fail the rule"
    );
}
