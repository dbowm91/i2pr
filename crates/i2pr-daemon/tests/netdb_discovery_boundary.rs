//! Plan 513 static boundary guard for the live NetDB discovery spine.
//!
//! Plan 513 installs the **live NetDB discovery spine**: the composition
//! root starts an exploration, the seam starts a RouterInfo-kind lookup and
//! an Exploratory-kind lookup, the RouterInfo request rides the **outbound**
//! exploratory tunnel, the reply travels the **inbound** exploratory tunnel,
//! the routing table only ever *reorders* the flat store's selection, the
//! DSM version gate is never bypassed, every fallback is counted by its own
//! reason, the spine is bounded, a suggestion is never indexed as signed
//! evidence, a RouterInfo reply is ingested into the router's own NetDB, and
//! the support surface stays unadvertised.
//!
//! Those invariants are not only behavioural; several of them are
//! *structural*, and a behavioural test cannot see a structural drift.
//! Plan 185's lesson was that a module can exist, with every name and every
//! token present, and still be unreachable from production, so a whole-file
//! token search proves nothing about the *composition*. Every rule below
//! therefore asserts that a **named function's own body** contains the
//! required **call**, matched with its open parenthesis: a dead helper, a
//! type name, an unreachable branch, or a name that only appears in a
//! sibling function does not satisfy a rule.
//!
//! The guard is deliberately **fail-closed**:
//!
//! - comments are stripped before matching, so a doc comment that merely
//!   *names* a construct is not a violation;
//! - `#[cfg(test)]` blocks are excluded, because the crates' own unit tests
//!   may use whatever they need and the invariants are about the production
//!   surface;
//! - an **absent** function is a violation: the rule returns a reason, never
//!   a silent pass;
//! - every rule carries its own **inverted probe** — a hand-written
//!   violating snippet must be reported — so a rule that stops detecting its
//!   own violation fails its own probe.
//!
//! The rules are **structural, not textual**: they assert the *shape* of the
//! composition (which call sits in which body, and what must never sit
//! beside it), not the presence of a string somewhere in a file. Two rules
//! span two modules, because the invariant itself does: the reply path is
//! selected in one owner and consumed in another, and the RouterInfo record
//! is ingested in the seam and reached from the product.
//!
//! This guard owns **one** file and nothing else. Plan 497's NetDB
//! participation-consumer invariants, Plan 504's
//! `tunnel_liveness_boundary.rs`, Plan 507's `netdb_retention_boundary.rs`,
//! and Plan 512's `destination_liveness_boundary.rs` are **not** modified by
//! this file: the sibling guards keep their own rules, and this guard only
//! *reads* the production sources they also read.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The composition root that starts the discovery.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";

/// The NetDB seam that owns the lookup state machines.
const NETDB_SEAM_MODULE: &str = "crates/i2pr-daemon/src/netdb_seam.rs";

/// The exploratory-build owner that owns the discovery reply path.
const EXPLORATORY_BUILD_MODULE: &str = "crates/i2pr-daemon/src/exploratory_build.rs";

/// The product module that owns the runtime composition.
const PRODUCT_MODULE: &str = "crates/i2pr-daemon/src/service_product.rs";

/// The NetDB participation owner that owns the discovery spine.
const PARTICIPATION_MODULE: &str = "crates/i2pr-daemon/src/netdb_participation.rs";

/// The destination-tunnel owner that owns the DSM version gate.
const DESTINATION_TUNNELS_MODULE: &str = "crates/i2pr-daemon/src/destination_tunnels.rs";

/// The machine-readable support inventory.
const SUPPORT_INVENTORY: &str = "specs/support.toml";

/// The support-inventory surface key for the live RouterInfo lookup.
const DISCOVERY_SURFACE_KEY: &str = "netdb.live-routerinfo-lookup";

/// Plan 513's own surface. The inventory carries **two** Plan 513
/// surfaces: the pre-existing `netdb.live-routerinfo-lookup`, which moves
/// from `deferred` to `experimental` when the lane has proved it, and the
/// plan's own `daemon.netdb-discovery-spine`. Both must stay
/// non-advertised, so the rule reads both.
const DISCOVERY_SPINE_SURFACE_KEY: &str = "daemon.netdb-discovery-spine";

/// The bound the spine's pending-resolution queue must respect.
const PENDING_RESOLUTION_BOUND: &str = "MAX_PENDING_RESOLUTIONS";

/// The counted reasons every routing-table fallback is counted by.
const COUNTED_FALLBACK_REASONS: &[&str] = &["Disabled", "NoCandidate", "Unavailable"];

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

/// Returns the offset of a **definition** whose text is `needle`.
///
/// The character that follows the needle must not continue an identifier, so
/// `fn drive` never matches `fn drive_discovery` and `fn candidate_hashes`
/// never matches `fn version_gated_candidate_hashes`.
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
///
/// The scan is **string-aware**: a brace inside a string literal or inside a
/// character literal does not change the nesting depth, so a body that holds
/// `"}"` does not end the block early.
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

/// Extracts the body of the item whose **definition** is anchored at
/// `signature`.
///
/// The anchor is a definition and never a call site: the needle carries the
/// `fn` keyword, and the character that follows it must not continue an
/// identifier. The body is the text between the item's own braces. An item
/// that is not present yields `None`, so every rule that uses it fails
/// closed.
fn item_body(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (open, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[open + 1..close - 1].to_owned())
}

/// Extracts the body of the `impl` block whose **definition** is anchored at
/// `anchor`.
///
/// The spine owns several same-named helpers; scoping a rule to the impl
/// block that owns the helper is what makes the rule structural rather than
/// a search for whichever definition happens to mention a token.
fn impl_block(code: &str, anchor: &str) -> Option<String> {
    let start = find_definition(code, anchor)?;
    let (open, close) = brace_balanced_block(code, start + anchor.len())?;
    Some(code[open + 1..close - 1].to_owned())
}

/// Returns the number of occurrences of `token` that take part in a
/// comparison.
///
/// A bound that is only *declared* is not a bound: the constant must be
/// compared against something, so the growth of the surface it bounds is
/// actually guarded.
fn comparison_uses(code: &str, token: &str) -> usize {
    let characters: Vec<char> = code.chars().collect();
    let needle: Vec<char> = token.chars().collect();
    let mut count = 0usize;
    let mut index = 0usize;
    while index + needle.len() <= characters.len() {
        if characters[index..index + needle.len()] == needle[..] {
            // Walk back over whitespace to the previous non-space character.
            let mut back = index;
            while back > 0 && characters[back - 1].is_whitespace() {
                back -= 1;
            }
            if back > 0 {
                let previous = characters[back - 1];
                if previous == '>' || previous == '<' || previous == '=' || previous == '!' {
                    count += 1;
                }
            }
            index += needle.len();
        } else {
            index += 1;
        }
    }
    count
}

/// Returns whether the block declares the exact key/value pair `key = value`
/// on one of its own lines.
///
/// The check is **line-anchored**: a `notes` string that merely *mentions*
/// `advertised = false` is not the inventory's own answer, and a guard that
/// a sentence could satisfy would prove nothing.
fn block_declares(block: &str, key: &str, value: &str) -> bool {
    block.lines().any(|line| {
        let line = line.trim();
        let Some((name, declared)) = line.split_once('=') else {
            return false;
        };
        name.trim() == key && declared.trim().trim_matches('"') == value
    })
}

/// Extracts the `[[surface]]` block of the support inventory that names
/// `key`.
///
/// The inventory is read as **text**: the guard owns no TOML dependency, and
/// the block it wants is a plain `[[surface]]` table with a single
/// identifier line.
fn surface_block(inventory: &str, key: &str) -> Option<String> {
    let anchors = [format!("id = \"{key}\""), format!("key = \"{key}\"")];
    let position = anchors
        .iter()
        .find_map(|anchor| inventory.find(anchor.as_str()))?;
    let header = inventory[..position].rfind("[[surface]]")?;
    let rest = &inventory[header..];
    let end = rest[1..]
        .find("[[surface]]")
        .map(|index| index + 1)
        .unwrap_or(rest.len());
    Some(rest[..end].to_owned())
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1: the composition root has a production entry point that starts
/// both the exploration and the peer resolution.
///
/// A spine that is never started from production is not live, and a start
/// that only explores never resolves a named peer.
fn check_exploration_has_a_production_entry_point(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "fn drive_discovery") else {
        return Some("`fn drive_discovery` must exist in the production surface".to_owned());
    };
    for call in ["begin_exploration(", "begin_peer_resolution("] {
        if !body.contains(call) {
            return Some(format!(
                "`fn drive_discovery` must contain the `{call}` call in its own body"
            ));
        }
    }
    None
}

/// Rule 2: the RouterInfo-kind lookup is started with the RouterInfo kind.
///
/// A start that files the request under another kind would answer the wrong
/// lookup, so the kind is part of the start, not a later annotation.
fn check_routerinfo_kind_start_is_live(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "fn begin_routerinfo_lookup") else {
        return Some(
            "`fn begin_routerinfo_lookup` must exist in the production surface".to_owned(),
        );
    };
    if body.contains("LookupKind::RouterInfo") {
        None
    } else {
        Some(
            "`fn begin_routerinfo_lookup` must start the lookup with `LookupKind::RouterInfo`"
                .to_owned(),
        )
    }
}

/// Rule 3: the Exploratory-kind lookup is started with the Exploratory kind.
fn check_exploratory_kind_start_is_live(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "fn begin_exploratory_lookup") else {
        return Some(
            "`fn begin_exploratory_lookup` must exist in the production surface".to_owned(),
        );
    };
    if body.contains("LookupKind::Exploratory") {
        None
    } else {
        Some(
            "`fn begin_exploratory_lookup` must start the lookup with `LookupKind::Exploratory`"
                .to_owned(),
        )
    }
}

/// Rule 4: the reply path is the **inbound** exploratory tunnel, and the
/// product takes it from the owner that selects it.
///
/// The selection lives in the exploratory-build owner and the consumption in
/// the product, so the invariant spans two modules.
fn check_reply_path_is_the_inbound_exploratory_tunnel(
    exploratory_build: &str,
    product: &str,
) -> Option<String> {
    let build_code = code_view(exploratory_build);
    let Some(body) = item_body(&build_code, "fn exploratory_reply_path") else {
        return Some("`fn exploratory_reply_path` must exist in the production surface".to_owned());
    };
    if !body.contains("select_inbound_reply_path(") {
        return Some(
            "`fn exploratory_reply_path` must select the pool's inbound tunnel with `select_inbound_reply_path(`"
                .to_owned(),
        );
    }
    let product_code = code_view(product);
    let Some(body) = item_body(&product_code, "fn drive_netdb_discovery") else {
        return Some("`fn drive_netdb_discovery` must exist in the production surface".to_owned());
    };
    if !body.contains("exploratory_reply_path(") {
        return Some(
            "`fn drive_netdb_discovery` must take the reply path from `exploratory_reply_path(`"
                .to_owned(),
        );
    }
    None
}

/// Rule 5: the RouterInfo request rides the **outbound** exploratory tunnel,
/// and never falls back to a direct send.
///
/// Plan 102's closed amendment forbids substituting a direct peer link for
/// the exploratory-tunnel path, so a direct-send fallback may not appear in
/// this function at all.
fn check_routerinfo_request_rides_the_outbound_exploratory_tunnel(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "fn deliver_discovery_via_outbound_tunnel") else {
        return Some(
            "`fn deliver_discovery_via_outbound_tunnel` must exist in the production surface"
                .to_owned(),
        );
    };
    for call in [
        "compose_lookup_via_outbound_tunnel(",
        "exploratory_outbound_role(",
    ] {
        if !body.contains(call) {
            return Some(format!(
                "`fn deliver_discovery_via_outbound_tunnel` must contain the `{call}` call"
            ));
        }
    }
    if body.contains("encode_standard_envelope") {
        return Some(
            "`fn deliver_discovery_via_outbound_tunnel` must not carry a direct-send fallback (`encode_standard_envelope`)"
                .to_owned(),
        );
    }
    None
}

/// Rule 6: the routing table may only **reorder** the flat store's
/// selection, never narrow it.
///
/// The preferred start is a priority prefix; the closed-list start would
/// close the selection and drop a peer the flat store could serve.
fn check_table_may_only_reorder_never_narrow(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "fn begin_routerinfo_lookup") else {
        return Some(
            "`fn begin_routerinfo_lookup` must exist in the production surface".to_owned(),
        );
    };
    if !body.contains("start_with_preferred") {
        return Some(
            "`fn begin_routerinfo_lookup` must use the preferred start (`start_with_preferred`)"
                .to_owned(),
        );
    }
    if body.contains("start_with_candidates") {
        return Some(
            "`fn begin_routerinfo_lookup` must not use the closed-list start (`start_with_candidates`)"
                .to_owned(),
        );
    }
    None
}

/// Rule 7: the DSM version gate is applied at the publication paths, and
/// those paths never consult the routing table.
///
/// A gated publication that read the routing table would take its target
/// from a cache the gate is meant to protect.
fn check_version_gate_is_not_bypassed(source: &str) -> Option<String> {
    let code = code_view(source);
    for signature in [
        "fn version_gated_publication_floodfills",
        "fn version_gated_candidate_hashes",
    ] {
        let Some(body) = item_body(&code, signature) else {
            return Some(format!(
                "`{signature}` must exist in the production surface"
            ));
        };
        if !body.contains("select_version_gated_floodfills") {
            return Some(format!(
                "`{signature}` must select through `select_version_gated_floodfills`"
            ));
        }
        for forbidden in ["preferred_peers", "routing_table"] {
            if body.contains(forbidden) {
                return Some(format!(
                    "`{signature}` must not consult the routing table (`{forbidden}`)"
                ));
            }
        }
    }
    None
}

/// Rule 8: every fallback is counted, by its own reason.
///
/// Plan 497's participation consumer counts every fallback the routing table
/// cannot answer — `Disabled`, `NoCandidate`, and `Unavailable` — and the
/// preference the table hands the lookup must be reached **through** that
/// counting path (`candidates`), so a fallback is never a silent fallback.
///
/// The counted reasons are named where they are *produced*, in the counting
/// path's own body; the landed source names them in `candidates` rather than
/// in `preferred_peers`, so the rule follows the delegation instead of
/// demanding the reason inside the caller's body.
fn check_every_fallback_is_counted(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "fn preferred_peers") else {
        return Some("`fn preferred_peers` must exist in the production surface".to_owned());
    };
    if !body.contains("candidates(") {
        return Some(
            "`fn preferred_peers` must reach the counted path (`candidates(`) instead of reading the table itself"
                .to_owned(),
        );
    }
    if !body.contains("RoutingTableCandidates::Fallback") {
        return Some(
            "`fn preferred_peers` must recognise the fallback case (`RoutingTableCandidates::Fallback`)"
                .to_owned(),
        );
    }
    let Some(counted) = item_body(&code, "fn candidates") else {
        return Some("`fn candidates` must exist in the production surface".to_owned());
    };
    for reason in COUNTED_FALLBACK_REASONS {
        if !counted.contains(reason) {
            return Some(format!(
                "the counted path must count the `{reason}` fallback by its own reason"
            ));
        }
    }
    None
}

/// Rule 9: the discovery spine is bounded.
///
/// A declared constant that nothing compares against is not a bound: the
/// queue's growth must be guarded by the constant in a comparison.
fn check_spine_is_bounded(source: &str) -> Option<String> {
    let code = code_view(source);
    if !code.contains(PENDING_RESOLUTION_BOUND) {
        return Some(format!(
            "the discovery spine must bound its pending resolutions with `{PENDING_RESOLUTION_BOUND}`"
        ));
    }
    if comparison_uses(&code, PENDING_RESOLUTION_BOUND) == 0 {
        return Some(format!(
            "`{PENDING_RESOLUTION_BOUND}` must guard the queue's growth in a comparison"
        ));
    }
    None
}

/// Rule 10: a suggestion is never indexed as signed evidence.
///
/// The spine's own search-reply ingest must offer a peer to the routing
/// table as a *suggestion* (`index_suggested`) and must never index it as
/// evidence (`.index`).
fn check_suggestion_is_never_evidence(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(spine) = impl_block(&code, "impl NetDbDiscoverySpine") else {
        return Some("`impl NetDbDiscoverySpine` must exist in the production surface".to_owned());
    };
    let Some(body) = item_body(&spine, "fn ingest_search_reply") else {
        return Some(
            "`fn ingest_search_reply` must exist in the discovery spine's production surface"
                .to_owned(),
        );
    };
    if !body.contains("index_suggested(") {
        return Some(
            "the spine's `ingest_search_reply` must offer a peer with `index_suggested(`"
                .to_owned(),
        );
    }
    if body.contains(".index(") {
        return Some(
            "the spine's `ingest_search_reply` must never index a suggestion as signed evidence (`.index(`)"
                .to_owned(),
        );
    }
    None
}

/// Rule 11: a RouterInfo record is ingested into the router's own NetDB.
///
/// The ingest happens in the seam, and the product must reach it — so the
/// invariant spans two modules.
fn check_routerinfo_record_is_ingested_into_the_netdb(
    netdb_seam: &str,
    product: &str,
) -> Option<String> {
    let seam_code = code_view(netdb_seam);
    let Some(body) = item_body(&seam_code, "fn ingest_routerinfo_store") else {
        return Some(
            "`fn ingest_routerinfo_store` must exist in the production surface".to_owned(),
        );
    };
    if !body.contains("handle_database_store(") {
        return Some(
            "`fn ingest_routerinfo_store` must ingest the record with `handle_database_store(`"
                .to_owned(),
        );
    }
    let product_code = code_view(product);
    let Some(body) = item_body(&product_code, "fn ingest_discovery_routerinfo_store") else {
        return Some(
            "`fn ingest_discovery_routerinfo_store` must exist in the production surface"
                .to_owned(),
        );
    };
    if !body.contains("ingest_routerinfo_store(") {
        return Some(
            "`fn ingest_discovery_routerinfo_store` must reach the seam with `ingest_routerinfo_store(`"
                .to_owned(),
        );
    }
    None
}

/// Rule 12: the discovery surface is not advertised.
///
/// The support inventory is read as **text**; the guard owns no TOML
/// dependency. The `[[surface]]` block for the live RouterInfo lookup must
/// exist, must not be advertised, and must not claim readiness.
fn check_discovery_surface_is_not_advertised(inventory: &str) -> Option<String> {
    for key in [DISCOVERY_SURFACE_KEY, DISCOVERY_SPINE_SURFACE_KEY] {
        let Some(block) = surface_block(inventory, key) else {
            return Some(format!(
                "the support inventory must carry a `[[surface]]` block for `{key}`"
            ));
        };
        if !block_declares(&block, "advertised", "false") {
            return Some(format!(
                "the `{key}` surface must not be advertised (`advertised = false`)"
            ));
        }
        if block_declares(&block, "status", "ready") {
            return Some(format!("the `{key}` surface must not claim `status = \"ready\"`"));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The positive tests: each rule against the real production source.
// ----------------------------------------------------------------------

#[test]
fn the_exploration_has_a_production_entry_point() {
    let source = source_of(COMPOSITION_MODULE);
    assert_eq!(
        check_exploration_has_a_production_entry_point(&source),
        None,
        "the discovery spine must be started from the composition root"
    );
}

#[test]
fn the_routerinfo_kind_start_is_live() {
    let source = source_of(NETDB_SEAM_MODULE);
    assert_eq!(
        check_routerinfo_kind_start_is_live(&source),
        None,
        "the RouterInfo-kind lookup must be started under its own kind"
    );
}

#[test]
fn the_exploratory_kind_start_is_live() {
    let source = source_of(NETDB_SEAM_MODULE);
    assert_eq!(
        check_exploratory_kind_start_is_live(&source),
        None,
        "the Exploratory-kind lookup must be started under its own kind"
    );
}

#[test]
fn the_reply_path_is_the_inbound_exploratory_tunnel() {
    let exploratory_build = source_of(EXPLORATORY_BUILD_MODULE);
    let product = source_of(PRODUCT_MODULE);
    assert_eq!(
        check_reply_path_is_the_inbound_exploratory_tunnel(&exploratory_build, &product),
        None,
        "the answer must travel the inbound exploratory tunnel"
    );
}

#[test]
fn the_routerinfo_request_rides_the_outbound_exploratory_tunnel() {
    let source = source_of(PRODUCT_MODULE);
    assert_eq!(
        check_routerinfo_request_rides_the_outbound_exploratory_tunnel(&source),
        None,
        "a RouterInfo request must ride the outbound exploratory tunnel, never a direct send"
    );
}

#[test]
fn the_table_may_only_reorder_never_narrow() {
    let source = source_of(NETDB_SEAM_MODULE);
    assert_eq!(
        check_table_may_only_reorder_never_narrow(&source),
        None,
        "the routing table may reorder the flat store's selection, never narrow it"
    );
}

#[test]
fn the_version_gate_is_not_bypassed() {
    let source = source_of(DESTINATION_TUNNELS_MODULE);
    assert_eq!(
        check_version_gate_is_not_bypassed(&source),
        None,
        "the DSM version gate must be applied at the publication paths"
    );
}

#[test]
fn every_fallback_is_counted() {
    let source = source_of(PARTICIPATION_MODULE);
    assert_eq!(
        check_every_fallback_is_counted(&source),
        None,
        "every routing-table fallback must be counted by its own reason"
    );
}

#[test]
fn the_spine_is_bounded() {
    let source = source_of(PARTICIPATION_MODULE);
    assert_eq!(
        check_spine_is_bounded(&source),
        None,
        "the discovery spine's pending-resolution queue must be bounded"
    );
}

#[test]
fn a_suggestion_is_never_evidence() {
    let source = source_of(PARTICIPATION_MODULE);
    assert_eq!(
        check_suggestion_is_never_evidence(&source),
        None,
        "a suggestion must never be indexed as signed evidence"
    );
}

#[test]
fn the_routerinfo_record_is_ingested_into_the_netdb() {
    let netdb_seam = source_of(NETDB_SEAM_MODULE);
    let product = source_of(PRODUCT_MODULE);
    assert_eq!(
        check_routerinfo_record_is_ingested_into_the_netdb(&netdb_seam, &product),
        None,
        "a RouterInfo reply must be ingested into the router's own NetDB"
    );
}

#[test]
fn the_discovery_surface_is_not_advertised() {
    let inventory = source_of(SUPPORT_INVENTORY);
    assert_eq!(
        check_discovery_surface_is_not_advertised(&inventory),
        None,
        "the live RouterInfo lookup must stay unadvertised"
    );
}

// ----------------------------------------------------------------------
// The inverted probes: each rule must report its own violation.
// ----------------------------------------------------------------------

/// The inverted probe for rule 1: a start that names the calls only in a
/// comment starts nothing, and must be reported.
#[test]
fn the_guard_reports_a_discovery_entry_point_that_starts_nothing() {
    let violating = "\
fn drive_discovery(&self, now_ms: u64) {
    // The exploration would be started with `begin_exploration(` here.
    // The peer resolution would use `begin_peer_resolution(` here.
    let _ = now_ms;
}
";
    let reason = check_exploration_has_a_production_entry_point(violating);
    assert!(
        reason.is_some(),
        "a discovery entry point that starts nothing must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 2: a RouterInfo start filed under another
/// kind — including one that only a test block names — must be reported.
#[test]
fn the_guard_reports_a_routerinfo_kind_start_that_is_not_live() {
    let violating = "\
fn begin_routerinfo_lookup(&mut self, request_id: u64) -> LookupAction {
    let lookup_id = LookupId::new(request_id, LookupKind::Exploratory, target);
    lookup_id
}

#[cfg(test)]
mod tests {
    #[test]
    fn kinds() {
        let _ = LookupKind::RouterInfo;
    }
}
";
    let reason = check_routerinfo_kind_start_is_live(violating);
    assert!(
        reason.is_some(),
        "a RouterInfo start under another kind must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 3: an exploratory start filed under the
/// RouterInfo kind must be reported.
#[test]
fn the_guard_reports_an_exploratory_kind_start_that_is_not_live() {
    let violating = "\
fn begin_exploratory_lookup(
    &mut self,
    request_id: u64,
    search_key: RouterHash,
) -> LookupAction {
    let lookup_id = LookupId::new(request_id, LookupKind::RouterInfo, search_key);
    lookup_id
}
";
    let reason = check_exploratory_kind_start_is_live(violating);
    assert!(
        reason.is_some(),
        "an exploratory start under another kind must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 4: a product that never refreshes the
/// reply path from its owner must be reported.
#[test]
fn the_guard_reports_a_reply_path_that_is_not_the_inbound_exploratory_tunnel() {
    let exploratory_build = "\
pub fn exploratory_reply_path(&self, now_seconds: u64) -> Option<i2pr_netdb::ReplyPath> {
    self.pool
        .select_inbound_reply_path(now_seconds)
        .and_then(|inner| inner.ok())
}
";
    let product = "\
fn drive_netdb_discovery(&mut self, now_ms: u64) {
    let Some(spine) = self.inner.discovery.clone() else {
        return;
    };
    let Some(dispatch) = spine.take_dispatch() else {
        return;
    };
    spine.record_delivery(false);
}
";
    let reason = check_reply_path_is_the_inbound_exploratory_tunnel(exploratory_build, product);
    assert!(
        reason.is_some(),
        "a product that never takes the reply path from its owner must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 5: an outbound-tunnel delivery that keeps a
/// direct-send fallback must be reported.
#[test]
fn the_guard_reports_a_direct_send_fallback_in_the_outbound_tunnel_path() {
    let violating = "\
fn deliver_discovery_via_outbound_tunnel(
    &self,
    peer: i2pr_netdb::RouterHash,
    message: &i2pr_proto::DatabaseLookupMessage,
    now_ms: u64,
) -> bool {
    let Some(role) = self.inner.coordinator.exploratory_outbound_role(now_ms) else {
        return false;
    };
    let Ok((dispatch, _proof)) = crate::netdb_tunnels::compose_lookup_via_outbound_tunnel(
        &action,
        role,
        DISCOVERY_MESSAGE_ID,
        now_ms,
        deadline,
        &mut rng,
        now_ms,
    ) else {
        return false;
    };
    let bytes = crate::outbound_lookup::encode_standard_envelope(
        message,
        DISCOVERY_MESSAGE_ID,
        now_ms,
    );
    bytes.is_ok()
}
";
    let reason = check_routerinfo_request_rides_the_outbound_exploratory_tunnel(violating);
    assert!(
        reason.is_some(),
        "a direct-send fallback in the outbound-tunnel path must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 6: a start that closes the selection with
/// the closed-list start must be reported.
#[test]
fn the_guard_reports_a_lookup_start_that_narrows_the_candidates() {
    let violating = "\
fn begin_routerinfo_lookup(
    &mut self,
    store: &RouterInfoStore,
    request_id: u64,
    target: RouterHash,
    routing_key: &RouterHash,
    preferred: Option<&[RouterHash]>,
) -> LookupAction {
    let lookup_id = LookupId::new(request_id, LookupKind::RouterInfo, target);
    let outcome = self.lookup.start_with_preferred(store, lookup_id, routing_key, preferred);
    let _narrowed = self.lookup.start_with_candidates(lookup_id, &[target]);
    outcome
}
";
    let reason = check_table_may_only_reorder_never_narrow(violating);
    assert!(
        reason.is_some(),
        "a closed-list start must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 7: a gated publication that reads the
/// routing table must be reported.
#[test]
fn the_guard_reports_a_version_gate_that_consults_the_routing_table() {
    let violating = "\
fn version_gated_candidate_hashes(
    &self,
    target: &DestinationHash,
    routing_key: &RouterHash,
) -> VersionGatedFloodfillSelection {
    let selection =
        select_version_gated_floodfills(&self.store, routing_key, &[target_hash], &self.policy);
    let _ = self.routing_table.len();
    selection
}

fn version_gated_publication_floodfills(
    &self,
    local_hash: &RouterHash,
) -> VersionGatedFloodfillSelection {
    let selection =
        select_version_gated_floodfills(&self.store, local_hash, &[*local_hash], &self.policy);
    selection
}
";
    let reason = check_version_gate_is_not_bypassed(violating);
    assert!(
        reason.is_some(),
        "a gated publication that consults the routing table must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 8: a preference that reads the table
/// directly, bypassing the counting path, must be reported.
#[test]
fn the_guard_reports_an_uncounted_fallback() {
    let violating = "\
fn preferred_peers(
    &self,
    store: &RouterInfoStore,
    routing_key: &RouterHash,
    excluded: &[RouterHash],
) -> Vec<RouterHash> {
    let mut preferred = Vec::new();
    preferred
}
";
    let reason = check_every_fallback_is_counted(violating);
    assert!(
        reason.is_some(),
        "a preference that bypasses the counting path must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 9: a declared bound that nothing compares
/// against must be reported.
#[test]
fn the_guard_reports_an_unbounded_spine() {
    let violating = "\
pub const MAX_PENDING_RESOLUTIONS: usize = 64;
";
    let reason = check_spine_is_bounded(violating);
    assert!(
        reason.is_some(),
        "a bound that is only declared must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 10: an ingest that indexes a suggestion as
/// signed evidence must be reported.
#[test]
fn the_guard_reports_a_suggestion_indexed_as_evidence() {
    let violating = "\
impl NetDbDiscoverySpine {
    pub fn ingest_search_reply(
        &self,
        reply: &DatabaseSearchReplyMessage,
    ) -> Result<DiscoveryOutcome, DiscoveryError> {
        for candidate in &accepted {
            guard.table.index_suggested(*candidate, &source);
            guard.table.index(validated);
        }
        Ok(DiscoveryOutcome::default())
    }
}
";
    let reason = check_suggestion_is_never_evidence(violating);
    assert!(
        reason.is_some(),
        "a suggestion indexed as signed evidence must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 11: a product that never reaches the seam's
/// ingest must be reported.
#[test]
fn the_guard_reports_a_routerinfo_record_that_is_never_ingested() {
    let netdb_seam = "\
pub fn ingest_routerinfo_store(
    &mut self,
    store: &mut RouterInfoStore,
    store_message: &DatabaseStoreMessage,
) -> Result<i2pr_netdb::ResponseOutcome, NetDbSeamError> {
    let outcome = i2pr_netdb::handle_database_store(
        &mut self.lookup,
        store,
        lookup_id,
        store_message,
        context,
    )?;
    Ok(outcome)
}
";
    let product = "\
fn ingest_discovery_routerinfo_store(
    &self,
    store_message: &i2pr_proto::DatabaseStoreMessage,
    now_ms: u64,
) {
    let _ = (store_message, now_ms);
}
";
    let reason = check_routerinfo_record_is_ingested_into_the_netdb(netdb_seam, product);
    assert!(
        reason.is_some(),
        "a RouterInfo reply that never reaches the seam must be reported: {reason:?}"
    );
}

/// The inverted probe for rule 12: an advertised surface, and a missing
/// surface, must both be reported.
#[test]
fn the_guard_reports_an_advertised_discovery_surface() {
    let advertised = "\
[[surface]]
id = \"netdb.live-routerinfo-lookup\"
status = \"ready\"
notes = \"the prose says `advertised = false` but the table says otherwise\"
advertised = true

[[surface]]
id = \"daemon.netdb-discovery-spine\"
status = \"experimental\"
advertised = false
";
    let reason = check_discovery_surface_is_not_advertised(advertised);
    assert!(
        reason.is_some(),
        "an advertised discovery surface must be reported: {reason:?}"
    );

    let spine_advertised = "\
[[surface]]
id = \"netdb.live-routerinfo-lookup\"
status = \"experimental\"
advertised = false

[[surface]]
id = \"daemon.netdb-discovery-spine\"
status = \"experimental\"
advertised = true
";
    let reason = check_discovery_surface_is_not_advertised(spine_advertised);
    assert!(
        reason.is_some(),
        "an advertised discovery spine must be reported: {reason:?}"
    );

    let prose_only = "\
[[surface]]
id = \"netdb.live-routerinfo-lookup\"
notes = \"the prose says `advertised = false` but the table does not declare it\"

[[surface]]
id = \"daemon.netdb-discovery-spine\"
status = \"experimental\"
advertised = false
";
    let reason = check_discovery_surface_is_not_advertised(prose_only);
    assert!(
        reason.is_some(),
        "a surface whose prose mentions the value but whose table does not declare it must be reported: {reason:?}"
    );

    let absent = "\
[[surface]]
id = \"netdb.routerinfo-validation\"
status = \"experimental\"
advertised = false
";
    let reason = check_discovery_surface_is_not_advertised(absent);
    assert!(
        reason.is_some(),
        "a missing discovery surface must be reported: {reason:?}"
    );
}
