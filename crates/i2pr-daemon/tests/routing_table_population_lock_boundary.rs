//! Plan 539's guard -- the routing table's population must not hold the
//! router's NetDB lock.
//!
//! The composition root populated the router's **routing table** by walking
//! every floodfill advertiser the NetDB holds -- an O(population) walk --
//! and it did that walk **under the router's NetDB lock**. Worse, the walk's
//! body called the routing table's own `index`, which takes the routing
//! table's own mutex, so the router's NetDB lock **wrapped** the routing
//! table's mutex, for the whole population, on the router's **live**
//! maintenance cadence.
//!
//! The guard is the authority for the **shape**:
//!
//! 1. the population is read through the **source**, and the composition
//!    root's population holds **no** NetDB lock across the indexing;
//! 2. the production source's read **releases** the lock before it returns,
//!    because it yields **owned** records;
//! 3. the population's **absence** is a **typed** refusal, and it is
//!    **counted** under its own reason;
//! 4. the population yields the **records**, and the table's signed-evidence
//!    rule is **untouched**;
//! 5. the routing table's existing surface and the store's selection
//!    surface are **untouched**;
//! 6. **no** configuration key is added, **no** task, timer, listener or
//!    socket is added, **no** advertisement is made, and the population
//!    rides the **existing** tick;
//! 7. Plan 536's, Plan 537's and Plan 538's lanes and guards stay
//!    **byte-identical**.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, so no file on disk is written, every rule has a
//! probe that removes the item it names so it **fails closed**, and every rule
//! reports a missing item rather than passing.
//!
//! The guard cannot observe that the lock is **free** at the moment the
//! indexing runs -- that is a fact about the running program. The lane
//! observes it, with `try_lock` and with the table's own mutex, and the two
//! claims do not overlap.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const DAEMON_COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const DAEMON_PARTICIPATION: &str = "crates/i2pr-daemon/src/netdb_participation.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const NETDB_STORE: &str = "crates/i2pr-netdb/src/store.rs";
const NETDB_BUCKET_SET: &str = "crates/i2pr-netdb/src/bucket_set.rs";
const NETDB_LOCAL: &str = "crates/i2pr-netdb/src/local.rs";
const SUPPORT_TOML: &str = "specs/support.toml";
const PLAN536_LANE: &str = "crates/i2pr-daemon/tests/ssu2_relay_target_lock.rs";
const PLAN536_GUARD: &str = "crates/i2pr-daemon/tests/ssu2_relay_target_lock_boundary.rs";
const PLAN537_LANE: &str = "crates/i2pr-daemon/tests/ssu2_relay_requester_lock.rs";
const PLAN537_GUARD: &str = "crates/i2pr-daemon/tests/ssu2_relay_requester_lock_boundary.rs";
const PLAN538_LANE: &str = "crates/i2pr-daemon/tests/floodfill_statistic_accounting.rs";
const PLAN538_GUARD: &str = "crates/i2pr-daemon/tests/floodfill_statistic_accounting_boundary.rs";

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

/// Plan 538's lane, byte for byte.
const PLAN538_LANE_SHA256: &str =
    "d1ce001d1e7562ca84b33cd9b4b0bca04325565b7cfef2c7504645825c8bc613";

/// Plan 538's guard, byte for byte.
const PLAN538_GUARD_SHA256: &str =
    "3d059e0690a0ab52b5c54619c0b5220eac8f340168187c2541f12f597c518fa4";

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

/// Every rule and every probe works on one of these, so a probe mutates
/// exactly one source and nothing else.
struct Sources {
    daemon_composition: String,
    daemon_participation: String,
    daemon_config: String,
    netdb_store: String,
    netdb_bucket_set: String,
    netdb_local: String,
    support_toml: String,
    plan536_lane: String,
    plan536_guard: String,
    plan537_lane: String,
    plan537_guard: String,
    plan538_lane: String,
    plan538_guard: String,
}

impl Sources {
    /// Reads every source from the real tree.
    fn from_tree() -> Self {
        Self {
            daemon_composition: source_of(DAEMON_COMPOSITION),
            daemon_participation: source_of(DAEMON_PARTICIPATION),
            daemon_config: source_of(DAEMON_CONFIG),
            netdb_store: source_of(NETDB_STORE),
            netdb_bucket_set: source_of(NETDB_BUCKET_SET),
            netdb_local: source_of(NETDB_LOCAL),
            support_toml: source_of(SUPPORT_TOML),
            plan536_lane: source_of(PLAN536_LANE),
            plan536_guard: source_of(PLAN536_GUARD),
            plan537_lane: source_of(PLAN537_LANE),
            plan537_guard: source_of(PLAN537_GUARD),
            plan538_lane: source_of(PLAN538_LANE),
            plan538_guard: source_of(PLAN538_GUARD),
        }
    }
}

// ---- 1. the population is read through the source ------------------------

fn rule_the_population_is_read_through_the_source(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_composition);
    let Some(population_text) = item_text(&code, "fn populate_routing_table") else {
        return Some("the composition root's population is absent".to_owned());
    };

    // The population reads through the **source**.  The signature is in the
    // whole item, not in the body.
    if !compact(&population_text).contains(&compact(
        "source: &dyn crate::netdb_participation::RoutingTablePopulationSource",
    )) {
        return Some("the population must be read through the source".to_owned());
    }
    let Some(population) = item_body(&code, "fn populate_routing_table") else {
        return Some("the composition root's population is absent".to_owned());
    };

    // And the population touches neither the router's NetDB lock nor the
    // store: the read happens inside the source, and the indexing happens
    // outside every lock.
    for forbidden in [
        "bootstrap.lock()",
        ".floodfill_advertisers()",
        "bootstrap.store()",
    ] {
        if population.contains(forbidden) {
            return Some(format!(
                "the population must not touch the NetDB lock or the store: {forbidden}"
            ));
        }
    }

    // The read returns **first**, and the indexing runs afterwards.
    let Some(read) = population.find("source.advertisers()") else {
        return Some("the population must read through the source".to_owned());
    };
    let Some(index) = population.find("self.routing_table.index(") else {
        return Some("the population must index the advertisers".to_owned());
    };
    if read > index {
        return Some("the read must return before the indexing starts".to_owned());
    }
    None
}

// ---- 2. the source releases the lock before it returns -------------------

fn rule_the_source_releases_the_lock_before_it_returns(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_participation);

    // The trait yields **owned** records, so a guard cannot outlive the read.
    let Some(trait_text) = item_text(&code, "pub trait RoutingTablePopulationSource") else {
        return Some("the population source's trait is absent".to_owned());
    };
    if !compact(&trait_text).contains(&compact(
        "fn advertisers(&self) -> Result<Vec<ValidatedRouterInfo>, RoutingPopulationRefusal>;",
    )) {
        return Some(
            "the source must yield owned records, so the guard cannot outlive the read".to_owned(),
        );
    }

    // The production source reads under the router's NetDB lock ...
    let Some(implementation) = item_body(
        &code,
        "impl RoutingTablePopulationSource for ValidatedStorePopulationSource",
    ) else {
        return Some("the production population source is absent".to_owned());
    };
    let squeezed = squeeze(&implementation);
    if !squeezed.contains("let Ok(guard) = bootstrap.lock() else {") {
        return Some("the production source must read under the router's NetDB lock".to_owned());
    }

    // ... and copies the advertisers out, so the guard is gone when the read
    // returns.
    if !squeezed.contains("Ok(guard.store().floodfill_advertisers().cloned().collect())") {
        return Some("the production source must copy the advertisers out".to_owned());
    }
    if implementation.contains("&ValidatedRouterInfo") {
        return Some("the production source must not return a reference into the store".to_owned());
    }
    None
}

// ---- 3. the absence is a typed and counted refusal -----------------------

fn rule_the_absence_is_a_typed_and_counted_refusal(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_participation);

    let Some(refusal) = item_text(&code, "pub enum RoutingPopulationRefusal") else {
        return Some("the population's refusal type is absent".to_owned());
    };
    if !squeeze(&refusal).contains("StoreUnavailable,") {
        return Some("the refusal must name the store being unavailable".to_owned());
    }

    // Both refusal paths count the refusal, under its own reason.
    let Some(implementation) = item_body(
        &code,
        "impl RoutingTablePopulationSource for ValidatedStorePopulationSource",
    ) else {
        return Some("the production population source is absent".to_owned());
    };
    let counts = implementation.matches("self.refusals").count();
    if counts < 2 {
        return Some(format!(
            "both refusal paths must count the refusal: {counts} counter references"
        ));
    }
    if !implementation.contains("fetch_add(1,") {
        return Some("the refusal must be counted".to_owned());
    }

    // And the composition root **consumes** the typed refusal rather than
    // ignoring it.
    let composition = code_view(&sources.daemon_composition);
    let Some(population) = item_body(&composition, "fn populate_routing_table") else {
        return Some("the composition root's population is absent".to_owned());
    };
    if !squeeze(&population).contains("let Ok(advertisers) = source.advertisers() else {") {
        return Some("the population must consume the typed refusal".to_owned());
    }
    None
}

// ---- 4. the table's signed-evidence rule is untouched --------------------

fn rule_the_table_still_refuses_a_non_advertiser(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_participation);

    // The table still indexes with its **own** signed evidence.
    let Some(driver) = item_body(
        &code,
        "pub fn index(&mut self, validated: &ValidatedRouterInfo) -> TableInsertOutcome",
    ) else {
        return Some("the routing table's driver index is absent".to_owned());
    };
    if !squeeze(&driver).contains("self.table.insert_signed(validated, self.current_date())") {
        return Some("the table must still index with its own signed evidence".to_owned());
    }

    // And the netdb crate's own rule still refuses a peer that does not
    // advertise `f`.
    let bucket = code_view(&sources.netdb_bucket_set);
    let Some(insert) = item_body(&bucket, "pub fn insert_signed") else {
        return Some("the bucket set's signed insert is absent".to_owned());
    };
    if !insert.contains("advertises_floodfill()") {
        return Some("the table's signed-evidence rule must not be weakened".to_owned());
    }
    if !insert.contains("RejectedNotFloodfill") {
        return Some("the table must still refuse a peer that does not advertise `f`".to_owned());
    }
    None
}

// ---- 5. the existing surfaces are untouched ------------------------------

fn rule_the_existing_surfaces_are_untouched(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_participation);

    // The routing table's own surface is exactly what the tree already had.
    for signature in [
        "pub fn index(&self, validated: &ValidatedRouterInfo) -> Option<TableInsertOutcome> {",
        "pub fn index_suggested(&self, key: RouterHash, from: &RouterHash",
    ] {
        if !compact(&code).contains(&compact(signature)) {
            return Some(format!(
                "the routing table's surface must be unchanged: {signature}"
            ));
        }
    }
    for counter in [
        "answered",
        "fell_back_disabled",
        "fell_back_no_candidate",
        "fell_back_unavailable",
    ] {
        if !code.contains(counter) {
            return Some(format!("the table's counter must be unchanged: {counter}"));
        }
    }

    // And the store's selection surface is exactly what the tree already had.
    let store = code_view(&sources.netdb_store);
    if !compact(&store).contains(&compact(
        "pub fn floodfill_advertisers(&self) -> impl Iterator<Item = &ValidatedRouterInfo> {",
    )) {
        return Some("the store's selection surface must be unchanged".to_owned());
    }
    None
}

// ---- 6. no new surface, no advertisement, the existing tick -------------

fn rule_no_new_surface_and_the_population_rides_the_existing_tick(
    sources: &Sources,
) -> Option<String> {
    // The NetDB configuration keeps exactly its twelve keys.
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

    // The population's **own** code gains no infrastructure.  The
    // participation module itself owns a maintenance timer that predates
    // this plan, so the rule is scoped to what this plan added: the trait,
    // the store's implementation, the production source, and the
    // composition root's population.
    let participation = code_view(&sources.daemon_participation);
    let mut population_code = String::new();
    for signature in [
        "pub trait RoutingTablePopulationSource",
        "impl RoutingTablePopulationSource for i2pr_netdb::RouterInfoStore",
        "impl RoutingTablePopulationSource for ValidatedStorePopulationSource",
    ] {
        let Some(text) = item_text(&participation, signature) else {
            return Some(format!("the population's own code is absent: {signature}"));
        };
        population_code.push_str(&text);
        population_code.push('\n');
    }
    let composition = code_view(&sources.daemon_composition);
    let Some(population) = item_body(&composition, "fn populate_routing_table") else {
        return Some("the composition root's population is absent".to_owned());
    };
    population_code.push_str(&population);
    for forbidden in [
        "std::net",
        "tokio::",
        "thread::spawn",
        "std::process",
        "std::fs",
    ] {
        if population_code.contains(forbidden) {
            return Some(format!(
                "the population's own code must not gain {forbidden}"
            ));
        }
    }

    // The population rides the **existing** startup and tick: exactly two
    // production call sites, and no new owner.
    let calls = composition.matches("populate_routing_table(&").count();
    if calls != 2 {
        return Some(format!(
            "the population must ride the existing startup and tick: {calls} call sites"
        ));
    }

    // And nothing is advertised.
    let local = code_view(&sources.netdb_local);
    let declaration =
        format!("pub const CONTROLLED_ROUTER_VERSION: &str = \"{CONTROLLED_ROUTER_VERSION}\";");
    if !local.contains(&declaration) {
        return Some("the controlled router version is absent".to_owned());
    }
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

// ---- 7. the predecessors' evidence is untouched -------------------------

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
        (
            "Plan 538's lane",
            &sources.plan538_lane,
            PLAN538_LANE_SHA256,
        ),
        (
            "Plan 538's guard",
            &sources.plan538_guard,
            PLAN538_GUARD_SHA256,
        ),
    ] {
        let digest = sha256_hex(source.as_bytes());
        if digest != expected {
            return Some(format!(
                "{label} must stay byte-identical: {digest} instead of {expected}"
            ));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted.
// ----------------------------------------------------------------------

#[test]
fn the_population_is_read_through_the_source() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_population_is_read_through_the_source(&sources),
        None
    );
}

#[test]
fn the_source_releases_the_lock_before_it_returns() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_source_releases_the_lock_before_it_returns(&sources),
        None
    );
}

#[test]
fn the_absence_is_a_typed_and_counted_refusal() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_absence_is_a_typed_and_counted_refusal(&sources),
        None
    );
}

#[test]
fn the_table_still_refuses_a_non_advertiser() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_table_still_refuses_a_non_advertiser(&sources),
        None
    );
}

#[test]
fn the_existing_surfaces_are_untouched() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_existing_surfaces_are_untouched(&sources), None);
}

#[test]
fn no_new_surface_and_the_population_rides_the_existing_tick() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_no_new_surface_and_the_population_rides_the_existing_tick(&sources),
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
fn the_inverted_probe_fires_when_the_population_reads_the_bootstrap() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "source: &dyn crate::netdb_participation::RoutingTablePopulationSource",
        "bootstrap: Option<&Arc<Mutex<bootstrap::Bootstrap>>>",
    );
    assert!(
        rule_the_population_is_read_through_the_source(&sources).is_some(),
        "a population that reads the bootstrap must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_population_holds_the_netdb_lock() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "let Ok(advertisers) = source.advertisers() else {",
        "let Ok(guard) = bootstrap.lock() else {\n            return;\n        };\n        let Ok(advertisers) = source.advertisers() else {",
    );
    assert!(
        rule_the_population_is_read_through_the_source(&sources).is_some(),
        "a population that takes the NetDB lock must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_source_yields_a_borrow() {
    let mut sources = Sources::from_tree();
    sources.daemon_participation = sources.daemon_participation.replace(
        "fn advertisers(&self) -> Result<Vec<ValidatedRouterInfo>, RoutingPopulationRefusal>;",
        "fn advertisers(&self) -> Result<&ValidatedRouterInfo, RoutingPopulationRefusal>;",
    );
    assert!(
        rule_the_source_releases_the_lock_before_it_returns(&sources).is_some(),
        "a source that yields a borrow must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_refusal_is_not_counted() {
    let mut sources = Sources::from_tree();
    sources.daemon_participation = sources.daemon_participation.replace(
        "self.refusals\n                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);",
        "",
    );
    assert!(
        rule_the_absence_is_a_typed_and_counted_refusal(&sources).is_some(),
        "an uncounted refusal must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_table_rule_is_weakened() {
    let mut sources = Sources::from_tree();
    sources.netdb_bucket_set = sources.netdb_bucket_set.replace(
        "if !validated.advertises_floodfill() {\n            return TableInsertOutcome::RejectedNotFloodfill;\n        }",
        "",
    );
    assert!(
        rule_the_table_still_refuses_a_non_advertiser(&sources).is_some(),
        "a weakened signed-evidence rule must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_table_surface_changes() {
    let mut sources = Sources::from_tree();
    sources.daemon_participation = sources.daemon_participation.replace(
        "pub fn index(&self, validated: &ValidatedRouterInfo) -> Option<TableInsertOutcome> {",
        "pub fn index(&self, validated: &ValidatedRouterInfo) {",
    );
    assert!(
        rule_the_existing_surfaces_are_untouched(&sources).is_some(),
        "a changed table surface must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_configuration_key_is_added() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources
        .daemon_config
        .replace("    enabled: bool,", "    enabled: bool,\n    extra: u64,");
    assert!(
        rule_no_new_surface_and_the_population_rides_the_existing_tick(&sources).is_some(),
        "an added configuration key must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_population_gains_a_new_call_site() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources.daemon_composition.replace(
        "fn populate_routing_table(",
        "fn populate_routing_table_twice(",
    );
    sources.daemon_composition = sources.daemon_composition.replace(
        "participation.populate_routing_table(&population, &[]);",
        "participation.populate_routing_table(&population, &[]);\n                        let _ = participation.populate_routing_table(&population, &[]);",
    );
    assert!(
        rule_no_new_surface_and_the_population_rides_the_existing_tick(&sources).is_some(),
        "a third population call site must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_inventory_advertises() {
    let mut sources = Sources::from_tree();
    sources.support_toml = sources
        .support_toml
        .replace("advertised = false\n", "advertised = true\n");
    assert!(
        rule_no_new_surface_and_the_population_rides_the_existing_tick(&sources).is_some(),
        "an advertised surface must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_predecessors_evidence_changes() {
    let mut sources = Sources::from_tree();
    sources
        .plan538_guard
        .push_str("\n// a change to a predecessor's guard\n");
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a changed predecessor's guard must fail the rule"
    );
}

// ----------------------------------------------------------------------
// The fail-closed probes: each removes the item its rule names.
// ----------------------------------------------------------------------

#[test]
fn the_guard_fails_closed_when_the_population_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_composition = sources
        .daemon_composition
        .replace("fn populate_routing_table(", "fn renamed_population(");
    assert!(
        rule_the_population_is_read_through_the_source(&sources).is_some(),
        "an absent population must fail the rule"
    );
    assert!(
        rule_the_absence_is_a_typed_and_counted_refusal(&sources).is_some(),
        "an absent population must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_trait_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_participation = sources.daemon_participation.replace(
        "pub trait RoutingTablePopulationSource",
        "pub trait RenamedPopulationSource",
    );
    assert!(
        rule_the_source_releases_the_lock_before_it_returns(&sources).is_some(),
        "an absent trait must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_production_source_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_participation = sources.daemon_participation.replace(
        "impl RoutingTablePopulationSource for ValidatedStorePopulationSource",
        "impl RoutingTablePopulationSource for RenamedPopulationSource",
    );
    assert!(
        rule_the_source_releases_the_lock_before_it_returns(&sources).is_some(),
        "an absent production source must fail the rule"
    );
    assert!(
        rule_the_absence_is_a_typed_and_counted_refusal(&sources).is_some(),
        "an absent production source must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_refusal_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_participation = sources.daemon_participation.replace(
        "pub enum RoutingPopulationRefusal",
        "pub enum RenamedRefusal",
    );
    assert!(
        rule_the_absence_is_a_typed_and_counted_refusal(&sources).is_some(),
        "an absent refusal type must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_signed_insert_is_absent() {
    let mut sources = Sources::from_tree();
    sources.netdb_bucket_set = sources
        .netdb_bucket_set
        .replace("pub fn insert_signed", "pub fn renamed_insert_signed");
    assert!(
        rule_the_table_still_refuses_a_non_advertiser(&sources).is_some(),
        "an absent signed insert must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_selection_surface_is_absent() {
    let mut sources = Sources::from_tree();
    sources.netdb_store = sources.netdb_store.replace(
        "pub fn floodfill_advertisers(&self) -> impl Iterator<Item = &ValidatedRouterInfo> {",
        "pub fn renamed_advertisers(&self) -> impl Iterator<Item = &ValidatedRouterInfo> {",
    );
    assert!(
        rule_the_existing_surfaces_are_untouched(&sources).is_some(),
        "an absent selection surface must fail the rule"
    );
}

#[test]
fn the_guard_fails_closed_when_the_netdb_configuration_is_absent() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources
        .daemon_config
        .replace("struct RawNetDbConfig", "struct RenamedNetDbConfig");
    assert!(
        rule_no_new_surface_and_the_population_rides_the_existing_tick(&sources).is_some(),
        "an absent NetDB configuration must fail the rule"
    );
}
