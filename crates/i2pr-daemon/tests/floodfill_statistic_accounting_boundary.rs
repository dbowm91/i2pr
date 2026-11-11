//! Plan 538's guard -- the NetDB's floodfill advertiser count must be
//! **accounted**, not derived by a scan of every record, and the daemon's
//! bootstrap must never hold the router's NetDB lock across a population scan
//! to obtain it.
//!
//! The router's validated `RouterInfo` store already maintains its **encoded
//! byte total** incrementally, on exactly the paths that insert and remove a
//! record. The floodfill advertiser count -- the second number the same struct
//! reports about the same set -- was **derived**: `stats()` walked every record
//! on every call. The daemon's bootstrap reads that statistic while holding
//! the router's NetDB lock, and read it **twice** per state computation,
//! because `compute_state` refreshed the snapshot -- which reads the
//! statistics -- and then read them again.
//!
//! The guard pins the shape of the fix:
//!
//! 1. the store **maintains** a floodfill advertiser count, and `stats()`
//!    reads it -- `stats()`'s body walks **no** record map;
//! 2. `insert` maintains the count on the **new-key** and the **replacement**
//!    paths, and `remove` maintains it on the removal path;
//! 3. the count never goes below zero, and the arithmetic is checked;
//! 4. the daemon's bootstrap computes the store's statistics **once** per
//!    operation, and `compute_state` does not read them a second time;
//! 5. the bootstrap's **public surface** is unchanged -- `snapshot()`,
//!    `diagnostics()`, `store_summary()` and the `BootstrapSnapshot` fields;
//! 6. the store's **selection** surface is unchanged --
//!    `floodfill_advertisers()` and `iter()`;
//! 7. **no** configuration key is added, **no** task, timer, listener or socket
//!    is added, **no** dependency is added, `Cargo.toml` stays
//!    **byte-identical**, and `Cargo.lock` differs from the pre-plan lock by
//!    **exactly** the declared transitive security update -- `rustix 1.1.6`,
//!    and **not** `rustix 1.1.5`;
//! 8. `router.version` stays `0.9.62`, and the inventory's advertised count
//!    stays **zero**;
//! 9. Plan 536's and Plan 537's lanes and guards stay **byte-identical**.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, every rule has a
//! probe that removes the item it names so it **fails closed**, and every rule
//! reports a missing item rather than passing.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const NETDB_STORE: &str = "crates/i2pr-netdb/src/store.rs";
const NETDB_LOCAL: &str = "crates/i2pr-netdb/src/local.rs";
const DAEMON_BOOTSTRAP: &str = "crates/i2pr-daemon/src/bootstrap.rs";
const DAEMON_COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const DAEMON_REQUESTER: &str = "crates/i2pr-daemon/src/relay_requester.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const SUPPORT_TOML: &str = "specs/support.toml";
const WORKSPACE_MANIFEST: &str = "Cargo.toml";
const WORKSPACE_LOCK: &str = "Cargo.lock";
const PLAN536_LANE: &str = "crates/i2pr-daemon/tests/ssu2_relay_target_lock.rs";
const PLAN536_GUARD: &str = "crates/i2pr-daemon/tests/ssu2_relay_target_lock_boundary.rs";
const PLAN537_LANE: &str = "crates/i2pr-daemon/tests/ssu2_relay_requester_lock.rs";
const PLAN537_GUARD: &str = "crates/i2pr-daemon/tests/ssu2_relay_requester_lock_boundary.rs";
const PLAN507_RETENTION_GUARD: &str = "crates/i2pr-daemon/tests/netdb_retention_boundary.rs";

/// Plan 536's lane, byte for byte.
const PLAN536_LANE_SHA256: &str =
    "911b19d357d9eecaab10bcd5026f4098ae195a54c7a44ec389e80a01b72d4d43";

/// Plan 536's guard, byte for byte.
const PLAN536_GUARD_SHA256: &str =
    "ea2ec6f7a6874f896a6c05a7f8c32352fb6320f90b4b35a34fda925087fe594a";

/// Plan 537's lane, byte for byte.
const PLAN537_LANE_SHA256: &str =
    "b6728b985b07a2c58697f508c2ae09e6c710cf8507e45cd4108ed1088fd6df2f";

/// Plan 537's guard, byte for byte.
const PLAN537_GUARD_SHA256: &str =
    "145ec3bfdaa629538c0741fd894f7659ebaa56b4bf23591968556cf4c18d5213";

/// The workspace manifest, byte for byte.
///
/// Plan 538 adds no dependency and no feature, so the manifest and the lock
/// are what the tree already had.
const WORKSPACE_MANIFEST_SHA256: &str =
    "39c44e33add62693eef8866220de17dcfe137437b62732e5044fcaefc4870269";

/// The workspace lock, byte for byte.
/// The workspace lock, byte for byte, after the declared transitive
/// security update `rustix 1.1.5` -> `rustix 1.1.6`.
///
/// The plan of record's section 5.5 declares the update, and the floor's
/// `cargo deny check advisories bans sources` step is what required it:
/// RustSec published RUSTSEC-2026-0486, an **unsound** advisory against
/// `rustix 1.1.5`, after the tree was locked. The manifest is unchanged --
/// no dependency is added -- and the rule below pins the version as well as
/// the digest, so the reason cannot be quietly lost.
const WORKSPACE_LOCK_SHA256: &str =
    "f0cec0685c4ed460c7d63273cc318f7a683dc2357735d7edaa6dfa1ac243d833";

/// Plan 507's retention boundary guard, byte for byte.
///
/// Plan 538 **extends** that guard -- its rule 7 inverted probe anchored on
/// the literal `self.refresh_snapshot();`, which this plan's change to the
/// bootstrap's refresh replaced -- and the extension is declared in the plan
/// of record's section 5.4. The digest is a **tripwire**: a later change to
/// that guard must change this constant **and** say why.
const PLAN507_RETENTION_GUARD_SHA256: &str =
    "5ef83044dcb8036e0ac2ccc8e7e9cb9bf6b7a21127b822b6e634da6170fdd78d";

/// The controlled router version the tree declares.
const CONTROLLED_ROUTER_VERSION: &str = "0.9.62";

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

/// Collapses every run of whitespace into a single space.
fn squeeze(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_space = false;
    for character in source.chars() {
        if character.is_whitespace() {
            in_space = true;
            continue;
        }
        if in_space {
            out.push(' ');
            in_space = false;
        }
        out.push(character);
    }
    if in_space {
        out.push(' ');
    }
    out
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

/// Extracts the whole item whose **definition** is anchored at
/// `signature`: the signature and the body together.
fn item_text(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (_, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[start..close].to_owned())
}

/// The sources the rules read.
///
/// Every rule and every probe works on one of these, so a probe mutates
/// exactly one source and nothing else.
struct Sources {
    netdb_store: String,
    netdb_local: String,
    daemon_bootstrap: String,
    daemon_composition: String,
    daemon_requester: String,
    daemon_config: String,
    support_toml: String,
    workspace_manifest: String,
    workspace_lock: String,
    plan536_lane: String,
    plan536_guard: String,
    plan537_lane: String,
    plan537_guard: String,
    plan507_retention_guard: String,
}

impl Sources {
    /// Reads every source from the real tree.
    fn from_tree() -> Self {
        Self {
            netdb_store: source_of(NETDB_STORE),
            netdb_local: source_of(NETDB_LOCAL),
            daemon_bootstrap: source_of(DAEMON_BOOTSTRAP),
            daemon_composition: source_of(DAEMON_COMPOSITION),
            daemon_requester: source_of(DAEMON_REQUESTER),
            daemon_config: source_of(DAEMON_CONFIG),
            support_toml: source_of(SUPPORT_TOML),
            workspace_manifest: source_of(WORKSPACE_MANIFEST),
            workspace_lock: source_of(WORKSPACE_LOCK),
            plan536_lane: source_of(PLAN536_LANE),
            plan536_guard: source_of(PLAN536_GUARD),
            plan537_lane: source_of(PLAN537_LANE),
            plan537_guard: source_of(PLAN537_GUARD),
            plan507_retention_guard: source_of(PLAN507_RETENTION_GUARD),
        }
    }
}

// ---- 1. the store maintains the count -----------------------------------

fn rule_the_store_maintains_the_floodfill_advertiser_count(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.netdb_store);

    // The store **holds** the count.
    let Some(store) = item_body(&code, "pub struct RouterInfoStore") else {
        return Some("the validated store is absent".to_owned());
    };
    if !store.contains("floodfill_advertiser_count: usize,") {
        return Some("the store must maintain a floodfill advertiser count".to_owned());
    }

    // `stats()` reads it.
    let Some(stats) = item_body(&code, "pub fn stats(&self) -> RouterInfoStoreStats") else {
        return Some("the store's statistics are absent".to_owned());
    };
    if !stats.contains("floodfill_advertiser_count: self.floodfill_advertiser_count,") {
        return Some("the statistics must read the maintained count".to_owned());
    }

    // And `stats()` walks **no** record map. The record map's **length** is
    // O(1) and is not a walk; a walk is a scan of the records themselves.
    for forbidden in [
        ".values()",
        ".values_mut()",
        ".iter()",
        ".iter_mut()",
        ".keys()",
        ".filter(",
        "advertises_floodfill()",
    ] {
        if stats.contains(forbidden) {
            return Some(format!(
                "the statistics must not walk the record map: {forbidden}"
            ));
        }
    }
    None
}

// ---- 2. the mutation paths maintain the count ---------------------------

fn rule_the_insert_and_remove_paths_maintain_the_count(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.netdb_store);

    let Some(insert) = item_body(
        &code,
        "pub fn insert(&mut self, validated: ValidatedRouterInfo) -> InsertOutcome",
    ) else {
        return Some("the store's insert path is absent".to_owned());
    };
    // The **new-key** path accounts the new advertiser ...
    if !insert.contains("if incoming_floodfill") {
        return Some("the insert path must account the new advertiser".to_owned());
    }
    // ... and the **replacement** path accounts the delta, because a
    // replacement may change the capability.
    if !insert.contains("existing_floodfill != incoming_floodfill") {
        return Some("the replacement path must account the change of capability".to_owned());
    }
    // Both paths go through the one adjustment, so the count cannot be
    // maintained two different ways.
    let adjustments = insert.matches("adjust_floodfill_advertiser_count(").count();
    if adjustments != 2 {
        return Some(format!(
            "the insert path must adjust the count on both the new-key and the replacement path: {adjustments}"
        ));
    }

    let Some(remove) = item_body(&code, "pub fn remove(&mut self, key: &RouterHash) -> bool")
    else {
        return Some("the store's removal path is absent".to_owned());
    };
    if !remove.contains("advertises_floodfill()") {
        return Some("the removal path must see the removed record's capability".to_owned());
    }
    if !remove.contains("adjust_floodfill_advertiser_count(false)") {
        return Some("the removal path must release the advertiser".to_owned());
    }
    None
}

// ---- 3. the count never goes below zero ---------------------------------

fn rule_the_count_never_goes_below_zero(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.netdb_store);

    let Some(adjust) = item_body(
        &code,
        "fn adjust_floodfill_advertiser_count(&mut self, add: bool)",
    ) else {
        return Some("the count's adjustment is absent".to_owned());
    };
    // The arithmetic is **checked**, in both directions.
    if !adjust.contains("checked_add(1)") {
        return Some("the count's adjustment must check its addition".to_owned());
    }
    if !adjust.contains("checked_sub(1)") {
        return Some("the count's adjustment must check its subtraction".to_owned());
    }
    // And an impossible adjustment is refused rather than wrapped.
    if !adjust.contains("expect(\"floodfill advertiser count overflow\")") {
        return Some("the count's adjustment must refuse an overflow".to_owned());
    }
    if !adjust.contains("expect(\"floodfill advertiser count underflow\")") {
        return Some("the count's adjustment must refuse an underflow".to_owned());
    }
    // A bare `+=` or `-=` would wrap instead.
    let compact = compact(&adjust);
    if compact.contains("floodfill_advertiser_count+=")
        || compact.contains("floodfill_advertiser_count-=")
    {
        return Some("the count must not be adjusted with a wrapping operator".to_owned());
    }
    None
}

// ---- 4. the bootstrap computes the statistics once -----------------------

fn rule_the_bootstrap_computes_the_statistics_once(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_bootstrap);

    // The refresh **takes** the statistics it refreshes.
    let Some(refresh) = item_text(
        &code,
        "fn refresh_snapshot(&mut self, stats: RouterInfoStoreStats)",
    ) else {
        return Some("the bootstrap's snapshot refresh is absent".to_owned());
    };
    if !squeeze(&refresh).contains("stats: RouterInfoStoreStats") {
        return Some("the snapshot refresh must take the statistics it refreshes".to_owned());
    }
    // And it never reads them itself.
    let Some(refresh_body) = item_body(
        &code,
        "fn refresh_snapshot(&mut self, stats: RouterInfoStoreStats)",
    ) else {
        return Some("the bootstrap's snapshot refresh is absent".to_owned());
    };
    if refresh_body.contains("store.stats()") {
        return Some("the snapshot refresh must not read the statistics itself".to_owned());
    }

    // The state computation reads them **once**.
    let Some(state) = item_body(
        &code,
        "fn compute_state(&mut self, policy: BootstrapPolicy) -> BootstrapState",
    ) else {
        return Some("the bootstrap's state computation is absent".to_owned());
    };
    let reads = state.matches("store.stats()").count();
    if reads != 1 {
        return Some(format!(
            "the state computation must read the statistics once: {reads}"
        ));
    }

    // And **every** call site passes a computed value, so no call site reads
    // the statistics a second time.
    let mut cursor = 0usize;
    let mut calls = 0usize;
    while let Some(found) = code[cursor..].find("self.refresh_snapshot(") {
        let at = cursor + found + "self.refresh_snapshot(".len();
        cursor = at;
        calls += 1;
        let rest = &code[at..];
        if !rest.starts_with("stats)") {
            return Some(
                "every snapshot refresh must be given the statistics the caller computed"
                    .to_owned(),
            );
        }
    }
    if calls < 5 {
        return Some(format!(
            "the bootstrap refreshes its snapshot at every mutation point: {calls}"
        ));
    }
    None
}

// ---- 5. the bootstrap's public surface is unchanged ----------------------

fn rule_the_bootstrap_public_surface_is_unchanged(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_bootstrap);

    for signature in [
        "pub fn snapshot(&self) -> &BootstrapSnapshot",
        "pub fn diagnostics(&self) -> BootstrapSnapshot",
        "pub fn store_summary(store: &RouterInfoStore) -> BootstrapSnapshot",
    ] {
        if item_body(&code, signature).is_none() {
            return Some(format!(
                "the bootstrap's public surface is absent: {signature}"
            ));
        }
    }

    // The snapshot's own fields are unchanged.
    let Some(snapshot) = item_body(&code, "pub struct BootstrapSnapshot") else {
        return Some("the bootstrap's snapshot is absent".to_owned());
    };
    for field in [
        "pub state: BootstrapState,",
        "pub record_count: usize,",
        "pub encoded_bytes: usize,",
        "pub floodfill_advertisers: usize,",
        "pub reseed_attempts: usize,",
        "pub last_reseed_summary: Option<ReseedSummary>,",
    ] {
        if !snapshot.contains(field) {
            return Some(format!(
                "the bootstrap's snapshot must keep the field: {field}"
            ));
        }
    }
    None
}

// ---- 6. the store's selection surface is unchanged ----------------------

fn rule_the_stores_selection_surface_is_unchanged(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.netdb_store);

    let Some(advertisers) = item_body(
        &code,
        "pub fn floodfill_advertisers(&self) -> impl Iterator<Item = &ValidatedRouterInfo>",
    ) else {
        return Some("the store's floodfill selection is absent".to_owned());
    };
    if !advertisers.contains("advertises_floodfill()") {
        return Some(
            "the store's floodfill selection must still filter on the capability".to_owned(),
        );
    }

    let Some(iter) = item_body(
        &code,
        "pub fn iter(&self) -> impl Iterator<Item = (&RouterHash, &ValidatedRouterInfo)>",
    ) else {
        return Some("the store's iterator is absent".to_owned());
    };
    if !iter.contains("self.records.iter()") {
        return Some("the store's iterator must still walk the records".to_owned());
    }
    None
}

// ---- 7. no new surface, and the manifest is unchanged -------------------

fn rule_no_new_surface_and_the_manifest_is_unchanged(sources: &Sources) -> Option<String> {
    // The reason for the lock's declared change is pinned **first**, so a
    // digest alone cannot hide it: the lock must carry the patched `rustix`
    // and must not carry the version the advisory names.
    if !sources
        .workspace_lock
        .contains("name = \"rustix\"\nversion = \"1.1.6\"")
    {
        return Some(
            "the lock must carry the patched rustix 1.1.6 that the floor's cargo deny required"
                .to_owned(),
        );
    }
    if sources
        .workspace_lock
        .contains("name = \"rustix\"\nversion = \"1.1.5\"")
    {
        return Some(
            "the lock must not carry rustix 1.1.5, which RUSTSEC-2026-0486 names".to_owned(),
        );
    }

    // The manifest is byte for byte what it was: no dependency, no feature, no
    // version. The lock differs from the pre-plan lock by **exactly** the
    // transitive security update the plan of record's section 5.5 declares,
    // and the digest below is the tripwire for that declaration.
    for (label, source, expected) in [
        (
            "Cargo.toml",
            &sources.workspace_manifest,
            WORKSPACE_MANIFEST_SHA256,
        ),
        ("Cargo.lock", &sources.workspace_lock, WORKSPACE_LOCK_SHA256),
    ] {
        let digest = sha256_hex(source.as_bytes());
        if digest != expected {
            return Some(format!(
                "{label} must stay byte-identical: {digest} instead of {expected}"
            ));
        }
    }

    // The configuration surface gains no key.
    let config = code_view(&sources.daemon_config);
    let Some(netdb) = item_body(&config, "struct RawNetDbConfig") else {
        return Some("the NetDB configuration surface is absent".to_owned());
    };
    let keys = netdb
        .lines()
        .map(|line| line.trim())
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("//")
                && !line.starts_with("#[")
                && !line.starts_with("///")
                && line.ends_with(',')
                && line.contains(':')
        })
        .count();
    if keys != 12 {
        return Some(format!(
            "the NetDB configuration must keep exactly its twelve keys: {keys}"
        ));
    }
    for key in [
        "enabled: bool,",
        "max_records: u64,",
        "max_encoded_bytes: u64,",
        "min_router_infos: u64,",
        "min_floodfill_advertisers: u64,",
        "routing_table_enabled: bool,",
        "routing_table_max_entries: u64,",
        "routing_table_max_bucket_size: u64,",
        "exploration_enabled: bool,",
        "exploration_max_peers: u64,",
        "exploration_deadline_ms: u64,",
        "participation_maintenance_period_ms: u64,",
    ] {
        if !netdb.contains(key) {
            return Some(format!("the NetDB configuration must keep the key: {key}"));
        }
    }

    // The two changed modules add no infrastructure: they are runtime-neutral
    // and they own no task, timer, listener or socket.
    for (label, source) in [
        ("the validated store", &sources.netdb_store),
        ("the bootstrap", &sources.daemon_bootstrap),
    ] {
        let code = code_view(source);
        for forbidden in [
            "tokio::",
            "UdpSocket",
            "TcpListener",
            "TcpStream",
            "spawn(",
            "sleep(",
            "interval(",
            "std::thread",
            "std::net",
        ] {
            if code.contains(forbidden) {
                return Some(format!("{label} must not add {forbidden}"));
            }
        }
    }
    None
}

// ---- 8. the advertisement is unchanged ----------------------------------

fn rule_the_advertisement_is_unchanged(sources: &Sources) -> Option<String> {
    // The controlled router version is what the tree already declared.
    let local = code_view(&sources.netdb_local);
    let declaration =
        format!("pub const CONTROLLED_ROUTER_VERSION: &str = \"{CONTROLLED_ROUTER_VERSION}\";");
    if !local.contains(&declaration) {
        return Some("the controlled router version is absent".to_owned());
    }
    if local.contains("\"0.9.63\"") {
        return Some("the tree must not declare a newer router version".to_owned());
    }

    // And the inventory advertises nothing.
    let advertised = sources
        .support_toml
        .lines()
        .filter(|line| {
            let line = line.trim();
            line.contains("advertised") && line.contains("= true")
        })
        .count();
    if advertised != 0 {
        return Some(format!(
            "the inventory must advertise nothing: {advertised} advertised surfaces"
        ));
    }
    None
}

// ---- 9. the predecessors' evidence is untouched -------------------------

fn rule_the_predecessors_evidence_is_untouched(sources: &Sources) -> Option<String> {
    for (label, source, expected) in [
        (
            "Plan 536's lane",
            &sources.plan536_lane,
            PLAN536_LANE_SHA256,
        ),
        (
            "Plan 536's guard",
            &sources.plan536_guard,
            PLAN536_GUARD_SHA256,
        ),
        (
            "Plan 537's lane",
            &sources.plan537_lane,
            PLAN537_LANE_SHA256,
        ),
        (
            "Plan 537's guard",
            &sources.plan537_guard,
            PLAN537_GUARD_SHA256,
        ),
    ] {
        let digest = sha256_hex(source.as_bytes());
        if digest != expected {
            return Some(format!(
                "{label} must stay byte-identical: {digest} instead of {expected}"
            ));
        }
    }

    // Plan 537's guard still looks for the strings it looked for ...
    for needle in [
        "pub fn read_published_introducers(",
        "pub fn read_introducer_endpoint(",
        "pub fn build_request(",
        "crate::relay_requester::drive_relay_request(",
        "crate::relay_requester::ValidatedStoreRequesterSource::new(",
        "relay_request_preimage(",
    ] {
        if !sources.plan537_guard.contains(needle) {
            return Some(format!(
                "Plan 537's guard must still look for the string it looked for: {needle}"
            ));
        }
    }

    // ... and the tree still satisfies them, so Plan 537's guard would still
    // pass against it.
    let requester = code_view(&sources.daemon_requester);
    for (signature, needles) in [
        ("pub fn read_published_introducers(", vec!["store.get("]),
        ("pub fn read_introducer_endpoint(", vec!["store.get("]),
        ("pub fn build_request(", vec!["relay_request_preimage("]),
    ] {
        let Some(body) = item_body(&requester, signature) else {
            return Some(format!(
                "Plan 537's guard must still find its item: {signature}"
            ));
        };
        let body = compact(&body);
        for needle in needles {
            if !body.contains(needle) {
                return Some(format!(
                    "Plan 537's guard must still find its string in {signature}: {needle}"
                ));
            }
        }
    }
    let composition = code_view(&sources.daemon_composition);
    if !composition.contains("crate::relay_requester::ValidatedStoreRequesterSource::new(") {
        return Some(
            "Plan 537's guard must still find the production source in the daemon".to_owned(),
        );
    }
    None
}

// ---- 10. the extension of Plan 507's guard is the declared one ---------

fn rule_the_extension_of_plan507s_guard_is_the_declared_one(sources: &Sources) -> Option<String> {
    // The extension is **one anchor string**: the rule 7 inverted probe still
    // injects the local record into the prune path, and it still anchors on
    // the refresh the tree actually makes. The probe's own content is checked
    // first, so a guard that lost the probe fails on that, and not only on
    // the digest.
    for needle in [
        "fn the_local_record_rule_fires_when_the_prune_reaches_the_local_record()",
        "\"self.refresh_snapshot(stats);\"",
        "self.local.take();",
        "fn rule_local_router_info_is_never_a_victim(",
    ] {
        if !sources.plan507_retention_guard.contains(needle) {
            return Some(format!(
                "Plan 507's retention boundary guard must still carry its rule 7 probe: {needle}"
            ));
        }
    }

    // And the extended guard is byte for byte what this plan made it.
    let digest = sha256_hex(sources.plan507_retention_guard.as_bytes());
    if digest != PLAN507_RETENTION_GUARD_SHA256 {
        return Some(format!(
            "Plan 507's retention boundary guard must stay byte-identical to the declared extension: {digest} instead of {PLAN507_RETENTION_GUARD_SHA256}"
        ));
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted.
// ----------------------------------------------------------------------

#[test]
fn the_store_maintains_the_floodfill_advertiser_count() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_store_maintains_the_floodfill_advertiser_count(&sources),
        None
    );
}

#[test]
fn the_insert_and_remove_paths_maintain_the_count() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_insert_and_remove_paths_maintain_the_count(&sources),
        None
    );
}

#[test]
fn the_count_never_goes_below_zero() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_count_never_goes_below_zero(&sources), None);
}

#[test]
fn the_bootstrap_computes_the_statistics_once() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_bootstrap_computes_the_statistics_once(&sources),
        None
    );
}

#[test]
fn the_bootstrap_public_surface_is_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_bootstrap_public_surface_is_unchanged(&sources),
        None
    );
}

#[test]
fn the_stores_selection_surface_is_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_stores_selection_surface_is_unchanged(&sources),
        None
    );
}

#[test]
fn no_new_surface_and_the_manifest_is_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_no_new_surface_and_the_manifest_is_unchanged(&sources),
        None
    );
}

#[test]
fn the_advertisement_is_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_advertisement_is_unchanged(&sources), None);
}

#[test]
fn the_predecessors_evidence_is_untouched() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_predecessors_evidence_is_untouched(&sources), None);
}

#[test]
fn the_extension_of_plan507s_guard_is_the_declared_one() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_extension_of_plan507s_guard_is_the_declared_one(&sources),
        None
    );
}

// ----------------------------------------------------------------------
// The inverted probes: each fires its rule against an in-memory copy.
// ----------------------------------------------------------------------

#[test]
fn the_inverted_probe_fires_when_the_statistics_walk_the_records() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "floodfill_advertiser_count: self.floodfill_advertiser_count,",
        "floodfill_advertiser_count: self.records.values().filter(|record| record.advertises_floodfill()).count(),",
    );
    assert!(
        rule_the_store_maintains_the_floodfill_advertiser_count(&sources).is_some(),
        "a statistics method that walks the records must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_insert_path_stops_accounting() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "        if incoming_floodfill {\n            self.adjust_floodfill_advertiser_count(true);\n        }\n",
        "",
    );
    assert!(
        rule_the_insert_and_remove_paths_maintain_the_count(&sources).is_some(),
        "an insert path that stops accounting must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_adjustment_wraps() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "                .checked_sub(1)\n                .expect(\"floodfill advertiser count underflow\");",
        "                .saturating_sub(1);",
    );
    assert!(
        rule_the_count_never_goes_below_zero(&sources).is_some(),
        "an adjustment that wraps must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_state_computation_reads_twice() {
    let mut sources = Sources::from_tree();
    sources.daemon_bootstrap = sources.daemon_bootstrap.replace(
        "        let stats = self.store.stats();\n        self.refresh_snapshot(stats);",
        "        let stats = self.store.stats();\n        self.refresh_snapshot(stats);\n        let _again = self.store.stats();",
    );
    assert!(
        rule_the_bootstrap_computes_the_statistics_once(&sources).is_some(),
        "a state computation that reads the statistics twice must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_bootstrap_surface_changes() {
    let mut sources = Sources::from_tree();
    sources.daemon_bootstrap = sources
        .daemon_bootstrap
        .replace("pub fn diagnostics(", "pub fn diagnostics_v2(");
    assert!(
        rule_the_bootstrap_public_surface_is_unchanged(&sources).is_some(),
        "a changed bootstrap surface must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_selection_surface_changes() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "pub fn floodfill_advertisers(&self)",
        "pub fn floodfill_advertisers_v2(&self)",
    );
    assert!(
        rule_the_stores_selection_surface_is_unchanged(&sources).is_some(),
        "a changed selection surface must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_manifest_changes() {
    let mut sources = Sources::from_tree();
    sources
        .workspace_manifest
        .push_str("\n# a change to the manifest\n");
    assert!(
        rule_no_new_surface_and_the_manifest_is_unchanged(&sources).is_some(),
        "a changed manifest must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_lock_carries_the_advisory_version() {
    let mut sources = Sources::from_tree();
    sources.workspace_lock = sources.workspace_lock.replace(
        "name = \"rustix\"\nversion = \"1.1.6\"",
        "name = \"rustix\"\nversion = \"1.1.5\"",
    );
    assert!(
        rule_no_new_surface_and_the_manifest_is_unchanged(&sources).is_some(),
        "a lock that carries the version RUSTSEC-2026-0486 names must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_lock_drops_the_patched_version() {
    let mut sources = Sources::from_tree();
    sources.workspace_lock = sources.workspace_lock.replace(
        "name = \"rustix\"\nversion = \"1.1.6\"",
        "name = \"rustix\"\nversion = \"1.1.7\"",
    );
    assert!(
        rule_no_new_surface_and_the_manifest_is_unchanged(&sources).is_some(),
        "a lock that does not carry the patched rustix must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_inventory_advertises() {
    let mut sources = Sources::from_tree();
    sources
        .support_toml
        .push_str("\ndaemon.floodfill-statistic-accounting = { advertised = true }\n");
    assert!(
        rule_the_advertisement_is_unchanged(&sources).is_some(),
        "an advertised surface must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_predecessors_evidence_changes() {
    let mut sources = Sources::from_tree();
    sources
        .plan537_lane
        .push_str("\n// a change to the predecessor's lane\n");
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a changed predecessor lane must fail the rule"
    );
}

// ----------------------------------------------------------------------
// The fail-closed probes: each removes the item its rule names.
// ----------------------------------------------------------------------

#[test]
fn the_guard_fails_closed_when_the_count_field_is_absent() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources
        .netdb_store
        .replace("    floodfill_advertiser_count: usize,\n", "");
    assert!(
        rule_the_store_maintains_the_floodfill_advertiser_count(&sources).is_some(),
        "an absent count field must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_adjustment_is_absent() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "adjust_floodfill_advertiser_count(",
        "adjust_floodfill_advertiser_count_v2(",
    );
    assert!(
        rule_the_insert_and_remove_paths_maintain_the_count(&sources).is_some(),
        "an absent adjustment must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_adjustment_item_is_absent() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "fn adjust_floodfill_advertiser_count(&mut self, add: bool)",
        "fn adjust_floodfill_advertiser_count_v2(&mut self, add: bool)",
    );
    assert!(
        rule_the_count_never_goes_below_zero(&sources).is_some(),
        "an absent adjustment must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_refresh_signature_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_bootstrap = sources.daemon_bootstrap.replace(
        "fn refresh_snapshot(&mut self, stats: RouterInfoStoreStats)",
        "fn refresh_snapshot(&mut self)",
    );
    assert!(
        rule_the_bootstrap_computes_the_statistics_once(&sources).is_some(),
        "a refresh that reads the statistics itself must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_bootstrap_surface_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_bootstrap = sources
        .daemon_bootstrap
        .replace("pub fn store_summary(", "pub fn store_summary_v2(");
    assert!(
        rule_the_bootstrap_public_surface_is_unchanged(&sources).is_some(),
        "an absent bootstrap surface must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_selection_surface_is_absent() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "pub fn iter(&self) -> impl Iterator<Item = (&RouterHash, &ValidatedRouterInfo)>",
        "pub fn iter_v2(&self)",
    );
    assert!(
        rule_the_stores_selection_surface_is_unchanged(&sources).is_some(),
        "an absent selection surface must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_netdb_configuration_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources
        .daemon_config
        .replace("struct RawNetDbConfig", "struct RawNetDbConfigRenamed");
    assert!(
        rule_no_new_surface_and_the_manifest_is_unchanged(&sources).is_some(),
        "an absent configuration surface must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_router_version_is_absent() {
    let mut sources = Sources::from_tree();
    sources.netdb_local = sources.netdb_local.replace(
        "pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.62\";",
        "pub const CONTROLLED_ROUTER_VERSION_V2: &str = \"0.9.62\";",
    );
    assert!(
        rule_the_advertisement_is_unchanged(&sources).is_some(),
        "an absent router version must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_a_predecessors_guard_is_changed() {
    let mut sources = Sources::from_tree();
    sources
        .plan536_guard
        .push_str("\n// a change to the predecessor's guard\n");
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a changed predecessor guard must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_extended_guard_changes() {
    let mut sources = Sources::from_tree();
    sources
        .plan507_retention_guard
        .push_str("\n// a change to the extended guard\n");
    assert!(
        rule_the_extension_of_plan507s_guard_is_the_declared_one(&sources).is_some(),
        "a changed extended guard must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_extended_guard_is_absent() {
    let mut sources = Sources::from_tree();
    sources.plan507_retention_guard = sources
        .plan507_retention_guard
        .replace("self.local.take();", "");
    assert!(
        rule_the_extension_of_plan507s_guard_is_the_declared_one(&sources).is_some(),
        "an absent rule 7 probe must fail the rule"
    );
}
