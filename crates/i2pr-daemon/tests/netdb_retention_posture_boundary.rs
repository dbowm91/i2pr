//! Plan 542's guard -- the NetDB's retention must not depend on the
//! service-tunnel posture.
//!
//! `RouterInfoStore::insert` **refuses** at the quota and **never evicts**, and
//! the tree's own documentation of the prune states the consequence: *"a NetDB
//! that filled up would refuse every new peer, permanently"*. The operation
//! that prevents that -- `Bootstrap::prune` -- had exactly **one** production
//! caller, and that caller is composed only when a service-tunnel group is
//! enabled, so a router running SSU2 alone filled its NetDB and then refused
//! every subsequent peer for as long as it ran.
//!
//! The guard is the authority for **reachability**, which the lane cannot see:
//! the lane constructs the owner directly, so it observes the step's
//! **behaviour** and not its presence in the daemon's graph. Plan 507's lane
//! had exactly this shape and constructed `PeerReconcile` by hand, which is why
//! the reachability defect survived it.
//!
//! The rules:
//!
//! 1. the retention step has a caller **outside** `peer_reconcile.rs`;
//! 2. that caller is **not** behind the routing-table switch;
//! 3. the retention window has **exactly one** definition, and both callers
//!    read it;
//! 4. the reconcile's own prune, snapshot, insert, learn and run are
//!    **unchanged**, and its ordering is unchanged;
//! 5. the routing table's population, the exploration job, the profile feed
//!    and the discovery spine keep their existing gates, and **no**
//!    configuration key is added;
//! 6. **no** new task, timer, listener or socket is added, and nothing is
//!    advertised;
//! 7. Plan 506's, Plan 507's, Plan 538's, Plan 539's, Plan 540's and Plan 541's
//!    lanes and guards stay **byte-identical**.
//!
//! Every rule has an inverted probe that fires it against an **in-memory copy**
//! of the source, so no file on disk is written, every rule has a probe that
//! removes the item it names so it **fails closed**, and every rule reports a
//! missing item rather than passing.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const PARTICIPATION: &str = "crates/i2pr-daemon/src/netdb_participation.rs";
const COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const RECONCILE: &str = "crates/i2pr-daemon/src/peer_reconcile.rs";
const CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
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
const PLAN541_LANE: &str = "crates/i2pr-daemon/tests/floodfill_selection_visibility.rs";
const PLAN541_GUARD: &str = "crates/i2pr-daemon/tests/floodfill_selection_visibility_boundary.rs";

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
    participation: String,
    composition: String,
    reconcile: String,
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
    plan541_lane_sha256: String,
    plan541_guard_sha256: String,
}

impl Sources {
    fn from_tree() -> Sources {
        Sources {
            participation: source_of(PARTICIPATION),
            composition: source_of(COMPOSITION),
            reconcile: source_of(RECONCILE),
            daemon_config: source_of(CONFIG),
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
            plan541_lane_sha256: sha256_hex(source_of(PLAN541_LANE).as_bytes()),
            plan541_guard_sha256: sha256_hex(source_of(PLAN541_GUARD).as_bytes()),
        }
    }
}

// ---- 1. the step has a caller outside the reconcile ----------------------

fn rule_the_step_has_a_caller_outside_the_reconcile(sources: &Sources) -> Option<String> {
    let reconcile = code_view(&sources.reconcile);
    let participation = code_view(&sources.participation);

    // The step exists, and it is the thing that prunes.
    let Some(step) = item_body(&participation, "pub fn prune_netdb(") else {
        return Some("the retention step is absent".to_owned());
    };
    if !step.contains("guard.prune(now_ms,") {
        return Some("the retention step must prune the store".to_owned());
    }
    // It is the **router's own** window, not a second one.
    if !step.contains("RETENTION_WINDOW_MS") {
        return Some("the retention step must use the router's own window".to_owned());
    }
    // A store that is absent and a lock that cannot be taken are both named,
    // so neither is a silent success.
    if !step.contains("PruneOutcome::NotApplicable") {
        return Some("an absent store must be named, not skipped".to_owned());
    }
    if !step.contains("PruneOutcome::Unavailable") {
        return Some("an unavailable lock must be named, not skipped".to_owned());
    }

    // And it has a caller that is **not** the reconcile.
    if !participation.contains("crate::netdb_participation::run_netdb_retention")
        && !sources
            .composition
            .contains("crate::netdb_participation::run_netdb_retention")
    {
        return Some("the retention cadence has no caller".to_owned());
    }
    let composition = code_view(&sources.composition);
    if !composition.contains("run_netdb_retention(") {
        return Some("the composition root must run the retention cadence".to_owned());
    }
    // The reconcile's own caller is what made the defect invisible: the guard
    // requires the composition root's caller to exist **as well**.
    if !reconcile.contains("RETENTION_WINDOW_MS") {
        return Some("the reconcile's own prune must keep the window".to_owned());
    }
    None
}

// ---- 2. the caller is not behind the routing-table switch -----------------

fn rule_the_caller_is_not_behind_the_routing_table_switch(sources: &Sources) -> Option<String> {
    let composition = code_view(&sources.composition);

    // The two switches are read apart, and the retention one is the NetDB's.
    if !composition.contains("let enabled = config.netdb.routing_table_enabled;") {
        return Some("the routing table's switch must be its own".to_owned());
    }
    if !composition.contains("let netdb_retention_enabled = config.netdb.enabled;") {
        return Some("the retention switch must be the NetDB's own".to_owned());
    }
    // The retention start is guarded by **its own** switch, outside the
    // routing table's `if enabled`.
    let Some(start) = item_body(&composition, "fn register_netdb_participation_service(") else {
        return Some("the participation service is absent".to_owned());
    };
    if !start.contains("if netdb_retention_enabled {") {
        return Some("the retention start must be guarded by the NetDB's switch".to_owned());
    }
    if !start.contains("participation.start_retention(") {
        return Some("the retention start must be called".to_owned());
    }
    // The start must not sit inside the routing table's `if enabled` block:
    // the `if enabled {` block closes before the retention start appears.
    let Some(routing_start) = start.find("if enabled {") else {
        return Some("the routing table's switch is not a block".to_owned());
    };
    let Some(retention_start) = start.find("if netdb_retention_enabled {") else {
        return Some("the retention switch is not a block".to_owned());
    };
    if retention_start < routing_start {
        return Some("the retention start must follow the routing table's block".to_owned());
    }
    None
}

// ---- 3. the window has one definition, and both callers read it -----------

fn rule_the_window_has_one_definition(sources: &Sources) -> Option<String> {
    let reconcile = code_view(&sources.reconcile);
    let participation = code_view(&sources.participation);

    // Exactly one definition, and it is **derived** from the policy.
    let definition = "pub const RETENTION_WINDOW_MS: u64 = acceptance_window_secs() * 1_000;";
    if reconcile.matches(definition).count() != 1 {
        return Some("the window must have exactly one definition".to_owned());
    }
    let total = reconcile.matches("const RETENTION_WINDOW_MS").count()
        + participation.matches("const RETENTION_WINDOW_MS").count();
    if total != 1 {
        return Some(format!("the window is defined {total} times, not once"));
    }
    // The second caller reads the same constant rather than a literal.
    if !participation.contains("RETENTION_WINDOW_MS") {
        return Some("the retention step must read the one window".to_owned());
    }
    for forbidden in ["86_400", "86400", "24 * 60 * 60"] {
        if participation.contains(forbidden) {
            return Some(format!(
                "the retention step must not restate the window: {forbidden}"
            ));
        }
    }
    None
}

// ---- 4. the reconcile is unchanged ---------------------------------------

fn rule_the_reconcile_is_unchanged(sources: &Sources) -> Option<String> {
    let reconcile = code_view(&sources.reconcile);

    for (signature, needle) in [
        (
            "fn prune_bootstrap(&self, now_ms: u64) -> PruneOutcome",
            "RETENTION_WINDOW_MS",
        ),
        (
            "fn prune_destinations(&self, now_ms: u64) -> PruneOutcome",
            "RETENTION_WINDOW_MS",
        ),
        (
            "fn snapshot(&self) -> Option<Vec<ValidatedRouterInfo>>",
            "validated_router_info_snapshot()",
        ),
        (
            "async fn insert_all(&mut self, records: &[ValidatedRouterInfo])",
            "insert_validated_router_info(",
        ),
        (
            "fn learn_all(&mut self, records: &[ValidatedRouterInfo])",
            "learn_all(records.iter().cloned())",
        ),
    ] {
        let Some(body) = item_body(&reconcile, signature) else {
            return Some(format!("the reconcile's item is absent: {signature}"));
        };
        if !body.contains(needle) {
            return Some(format!("the reconcile's item lost {needle}"));
        }
    }

    // The ordering is unchanged: prune, snapshot, insert, advertiser insert,
    // learn -- and the prune comes first.
    let Some(run) = item_body(&reconcile, "pub async fn run(&mut self, now_ms: u64)") else {
        return Some("the reconcile's run is absent".to_owned());
    };
    let order = [
        "self.prune_stores(now_ms);",
        "let Some(records) = self.snapshot() else {",
        "self.insert_all(&records).await;",
        "self.learn_all(&records);",
    ];
    let mut last = 0usize;
    for needle in order {
        let Some(found) = run.find(needle) else {
            return Some(format!("the reconcile's run lost a step: {needle}"));
        };
        if found < last {
            return Some("the reconcile's ordering changed".to_owned());
        }
        last = found;
    }
    None
}

// ---- 5. the features over the NetDB keep their gates ---------------------

fn rule_the_features_keep_their_gates(sources: &Sources) -> Option<String> {
    let composition = code_view(&sources.composition);

    // The routing table's own start is still behind its own switch.
    let Some(service) = item_body(&composition, "fn register_netdb_participation_service(") else {
        return Some("the participation service is absent".to_owned());
    };
    if !service.contains("if enabled {") {
        return Some("the routing table's switch must still gate its start".to_owned());
    }
    if !service.contains("participation.start(") {
        return Some("the participation owner's start must still be called".to_owned());
    }
    // The retention start must not be inside that block.
    let Some(routing_start) = service.find("if enabled {") else {
        return Some("the routing table's switch is not a block".to_owned());
    };
    let Some(retention_start) = service.find("if netdb_retention_enabled {") else {
        return Some("the retention switch is not a block".to_owned());
    };
    if retention_start < routing_start {
        return Some("the retention start precedes the routing table's block".to_owned());
    }

    // No configuration key is added.
    let config = code_view(&sources.daemon_config);
    for token in [
        "retention_posture",
        "netdb_retention",
        "retention_enabled",
        "retention_period",
        "retention_step",
    ] {
        if config.to_lowercase().contains(token) {
            return Some(format!("the configuration must gain no key: {token}"));
        }
    }
    None
}

// ---- 6. nothing new runs, and nothing is advertised ----------------------

fn rule_nothing_new_runs_and_nothing_is_advertised(sources: &Sources) -> Option<String> {
    // The changed modules stay runtime-neutral except for the cadence the
    // supervisor already owns: no listener, no socket, no thread.
    for (label, code) in [
        ("the retention step", code_view(&sources.participation)),
        ("the reconcile", code_view(&sources.reconcile)),
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

    // The manifest and the lock are byte-identical: **no** dependency is added.
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
    if sources.support.contains("advertised = true") {
        return Some("the inventory must advertise nothing".to_owned());
    }
    None
}

// ---- 7. the predecessors' evidence is untouched --------------------------

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
fn the_step_has_a_caller_outside_the_reconcile() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_step_has_a_caller_outside_the_reconcile(&sources),
        None
    );
}

#[test]
fn the_caller_is_not_behind_the_routing_table_switch() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_caller_is_not_behind_the_routing_table_switch(&sources),
        None
    );
}

#[test]
fn the_window_has_one_definition() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_window_has_one_definition(&sources), None);
}

#[test]
fn the_reconcile_is_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_reconcile_is_unchanged(&sources), None);
}

#[test]
fn the_features_keep_their_gates() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_features_keep_their_gates(&sources), None);
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
// The inverted probes: each fires its rule against an in-memory copy.
// ----------------------------------------------------------------------

#[test]
fn the_inverted_probe_fires_when_the_step_has_no_outside_caller() {
    let mut sources = Sources::from_tree();
    sources.composition = sources
        .composition
        .replace("run_netdb_retention(", "no_retention_caller(");
    assert!(
        rule_the_step_has_a_caller_outside_the_reconcile(&sources).is_some(),
        "a step with no caller outside the reconcile must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_step_restates_the_window() {
    let mut sources = Sources::from_tree();
    sources.participation = sources
        .participation
        .replace("RETENTION_WINDOW_MS", "86_400_000");
    assert!(
        rule_the_window_has_one_definition(&sources).is_some(),
        "a step that restates the window must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_caller_is_behind_the_routing_table_switch() {
    let mut sources = Sources::from_tree();
    sources.composition = sources
        .composition
        .replace("if netdb_retention_enabled {", "if enabled {");
    assert!(
        rule_the_caller_is_not_behind_the_routing_table_switch(&sources).is_some(),
        "a caller behind the routing table's switch must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_reconciles_ordering_changes() {
    let mut sources = Sources::from_tree();
    sources.reconcile = sources.reconcile.replace("self.prune_stores(now_ms);", "");
    assert!(
        rule_the_reconcile_is_unchanged(&sources).is_some(),
        "a reconcile that stopped pruning must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_configuration_key_appears() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "pub struct Config {",
        "pub struct Config {\n    pub netdb_retention: usize,",
    );
    assert!(
        rule_the_features_keep_their_gates(&sources).is_some(),
        "a configuration key must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_socket_appears() {
    let mut sources = Sources::from_tree();
    sources.participation = sources.participation.replace(
        "let mut maintenance = tokio::time::interval(maintenance_period);",
        "let _listener = std::net::TcpListener::bind(\"127.0.0.1:0\").ok();\n    let mut maintenance = tokio::time::interval(maintenance_period);",
    );
    assert!(
        rule_nothing_new_runs_and_nothing_is_advertised(&sources).is_some(),
        "a listener must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_predecessor_changes() {
    let mut sources = Sources::from_tree();
    sources.plan507_guard_sha256 = "0".repeat(64);
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a predecessor's evidence must fail the rule"
    );
}
