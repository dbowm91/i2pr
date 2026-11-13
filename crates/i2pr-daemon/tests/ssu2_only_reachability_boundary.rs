//! Plan 547's guard — an SSU2-only router can prove its own reachability,
//! and a re-dated dual-transport record is installable.
//!
//! The lane proves the **behaviour**. This guard proves the **shape**, and
//! the shape is what keeps the two gates honest:
//!
//! 1. **one owner**: the peer-test owner lives beside the SSU2 service, the
//!    destination-group product owns none, and the composition root drives
//!    and feeds it from **exactly one** place each — two consumers would
//!    answer the same probe twice and double the counters;
//! 2. **the default posture is unchanged**: `ssu2.peer_test` is still off by
//!    default and is still refused without the SSU2 service, and the owner
//!    is built from that switch **alone** — not from whether a destination
//!    group happens to exist;
//! 3. **the install gate keeps its meaning**: the re-dating path **supplies**
//!    the separately bound NTCP2 endpoint rather than inferring it from the
//!    record, and the runtime's own gate still compares the record's NTCP2
//!    address with that endpoint and still refuses a record that names no
//!    NTCP2 address through the plain form;
//! 4. **the daemon never writes reachability**: the runtime's recorder is
//!    the only writer, and the owner names no address family;
//! 5. **nothing new runs and nothing is advertised**: no listener, no socket,
//!    no task, no timer, no dependency, an unchanged manifest and lock, and
//!    an inventory that advertises nothing;
//! 6. **the predecessors' evidence is byte-identical**.
//!
//! Every rule has an inverted probe that fires it against an **in-memory
//! copy** of the source, and every rule reports a missing item rather than
//! passing it.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const COMPOSITION: &str = "crates/i2pr-daemon/src/lib.rs";
const OWNER: &str = "crates/i2pr-daemon/src/peer_test_runtime.rs";
const PRODUCT: &str = "crates/i2pr-daemon/src/service_product.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const ROUTER_I2NP: &str = "crates/i2pr-daemon/src/router_i2np.rs";
const RUNTIME: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const NETDB_LOCAL: &str = "crates/i2pr-netdb/src/local.rs";
const SUPPORT_TOML: &str = "specs/support.toml";
const MANIFEST: &str = "Cargo.toml";
const LOCK: &str = "Cargo.lock";

const PLAN544_LANE: &str = "crates/i2pr-daemon/tests/router_info_export.rs";
const PLAN544_GUARD: &str = "crates/i2pr-daemon/tests/router_info_export_boundary.rs";
const PLAN545_LANE: &str = "crates/i2pr-daemon/tests/peer_pool_fallback.rs";
const PLAN545_GUARD: &str = "crates/i2pr-daemon/tests/peer_pool_fallback_boundary.rs";
const PLAN546_LANE: &str = "crates/i2pr-daemon/tests/publication_bound_address.rs";
const PLAN546_GUARD: &str = "crates/i2pr-daemon/tests/publication_bound_address_boundary.rs";

/// PLAN544_LANE, byte for byte.
const PLAN544_LANE_SHA256: &str =
    "29900152190e12d387298683b2e50e5158e9514103e3c575885ca2226a11fe5c";
/// PLAN544_GUARD, byte for byte.
const PLAN544_GUARD_SHA256: &str =
    "aa89488d3c73b772d9833f7f4bb11bd098594029aedc3c3c07057ced18490808";
/// PLAN545_LANE, byte for byte.
const PLAN545_LANE_SHA256: &str =
    "98d51bf2d2cc8deddb90de49c6907ec5b3c09f510cdb64463ce4d83391ff5909";
/// PLAN545_GUARD, byte for byte.
const PLAN545_GUARD_SHA256: &str =
    "40e339c0250dc6005d01a69c925db11a4f9e1c30ccc42f62757de0a050ab64c7";
/// PLAN546_LANE, byte for byte.
const PLAN546_LANE_SHA256: &str =
    "9c0b6ccba53bb04924f1009d7c3af22203f49aec889fc487c71a9bdaead127e4";
/// PLAN546_GUARD, byte for byte.
const PLAN546_GUARD_SHA256: &str =
    "5bfd92010404c87ddb2cace6bf4e36ea0b5271e746c5704c561d6dc1d2029a40";

/// The workspace manifest, byte for byte.
const CARGO_TOML_SHA256: &str = "39c44e33add62693eef8866220de17dcfe137437b62732e5044fcaefc4870269";

/// The workspace lock, byte for byte.
const CARGO_LOCK_SHA256: &str = "f0cec0685c4ed460c7d63273cc318f7a683dc2357735d7edaa6dfa1ac243d833";

/// The inventory surface this plan owns.
const PLAN547_SURFACE: &str = "ssu2.ssu2-only-reachability";

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

/// Extracts the body of the `select!` arm whose head is `head`.
fn select_arm_body(code: &str, head: &str) -> Option<String> {
    let start = code.find(head)? + head.len();
    let (open, end) = brace_balanced_block(code, start)?;
    Some(code[open..end].to_owned())
}

/// Extracts one `[[surface]]` block from the inventory.
fn surface_block(support: &str, id: &str) -> Option<String> {
    let needle = format!("id = \"{id}\"");
    let start = support.find(&needle)?;
    let head = support[..start].rfind("[[surface]]")?;
    let tail = &support[start..];
    let end = tail
        .find("[[surface]]")
        .map(|at| start + at)
        .unwrap_or(support.len());
    Some(support[head..end].to_owned())
}

struct Sources {
    composition: String,
    owner: String,
    product: String,
    daemon_config: String,
    router_i2np: String,
    runtime: String,
    netdb_local: String,
    support: String,
    manifest_sha256: String,
    lock_sha256: String,
    plan544_lane_sha256: String,
    plan544_guard_sha256: String,
    plan545_lane_sha256: String,
    plan545_guard_sha256: String,
    plan546_lane_sha256: String,
    plan546_guard_sha256: String,
}

impl Sources {
    fn from_tree() -> Sources {
        Sources {
            composition: source_of(COMPOSITION),
            owner: source_of(OWNER),
            product: source_of(PRODUCT),
            daemon_config: source_of(DAEMON_CONFIG),
            router_i2np: source_of(ROUTER_I2NP),
            runtime: source_of(RUNTIME),
            netdb_local: source_of(NETDB_LOCAL),
            support: source_of(SUPPORT_TOML),
            manifest_sha256: sha256_hex(source_of(MANIFEST).as_bytes()),
            lock_sha256: sha256_hex(source_of(LOCK).as_bytes()),
            plan544_lane_sha256: sha256_hex(source_of(PLAN544_LANE).as_bytes()),
            plan544_guard_sha256: sha256_hex(source_of(PLAN544_GUARD).as_bytes()),
            plan545_lane_sha256: sha256_hex(source_of(PLAN545_LANE).as_bytes()),
            plan545_guard_sha256: sha256_hex(source_of(PLAN545_GUARD).as_bytes()),
            plan546_lane_sha256: sha256_hex(source_of(PLAN546_LANE).as_bytes()),
            plan546_guard_sha256: sha256_hex(source_of(PLAN546_GUARD).as_bytes()),
        }
    }
}

// ---- 1. one owner -------------------------------------------------------

fn rule_the_owner_lives_in_one_place(sources: &Sources) -> Option<String> {
    let composition = code_view(&sources.composition);
    if composition.matches("PeerTestRuntime::new(").count() != 1 {
        return Some("the composition root must construct the owner exactly once".to_owned());
    }
    if composition.matches("peer_test.handle_inbound(").count() != 1 {
        return Some("the peer-test seam must have exactly one production call site".to_owned());
    }
    if composition.matches("peer_test.drive(").count() != 1 {
        return Some("the peer test must be driven from exactly one tick".to_owned());
    }
    // The drive rides the composition root's **existing** tick, so the owner
    // adds no timer of its own.
    let Some(tick) = select_arm_body(&composition, "group_tick.tick() =>") else {
        return Some("the composition root has no group tick arm".to_owned());
    };
    if !tick.contains("peer_test.drive(") {
        return Some(
            "the peer test must be driven on the composition root's existing tick".to_owned(),
        );
    }
    // The product owns no peer test at all: two owners would answer the same
    // probe twice and double the counters.
    let product = code_view(&sources.product);
    for forbidden in [
        "PeerTestOwner",
        "note_confirmed_peer_test",
        "peer_test_owner",
        "drive_peer_test",
        "handle_inbound_peer_test",
    ] {
        if product.contains(forbidden) {
            return Some(format!(
                "the destination-group product must own no peer test: {forbidden}"
            ));
        }
    }
    let owner = code_view(&sources.owner);
    if owner.matches("PeerTestOwner::new(").count() != 1 {
        return Some("the peer-test owner must be constructed exactly once".to_owned());
    }
    // The seam and the drive are the owner's own, and nothing else drives
    // them.
    for signature in ["pub fn handle_inbound(", "pub fn drive("] {
        if item_body(&owner, signature).is_none() {
            return Some(format!("the owner's {signature} is absent"));
        }
    }
    None
}

// ---- 2. the default posture is unchanged --------------------------------

fn rule_the_default_posture_is_unchanged(sources: &Sources) -> Option<String> {
    let config = code_view(&sources.daemon_config);
    let Some(default) = item_body(&config, "fn default_ssu2_peer_test(") else {
        return Some("the peer-test default helper is absent".to_owned());
    };
    if !default.contains("false") {
        return Some("the peer-test default must stay off".to_owned());
    }
    let Some(normalize) = item_body(&config, "fn normalize_ssu2(raw: &RawSsu2Config)") else {
        return Some("the SSU2 normalizer is absent".to_owned());
    };
    if !normalize.contains("raw.peer_test && !raw.enabled") {
        return Some(
            "the peer-test opt-in must still be refused without the SSU2 service".to_owned(),
        );
    }
    // The owner is built from the operator's switch **alone**: the posture in
    // which a destination group happens to exist must not decide whether the
    // router can corroborate its own reachability.
    let composition = code_view(&sources.composition);
    let Some(position) = composition.find("PeerTestRuntime::new(") else {
        return Some("the owner construction is absent".to_owned());
    };
    let end = (position + 160).min(composition.len());
    let construction = &composition[position..end];
    if !construction.contains("ssu2_config.peer_test") {
        return Some("the owner must be built from the operator's switch".to_owned());
    }
    for forbidden in ["service_runtime_active", "has_enabled_groups"] {
        if construction.contains(forbidden) {
            return Some(format!(
                "the owner must not depend on the destination-group posture: {forbidden}"
            ));
        }
    }
    None
}

// ---- 3. the install gate keeps its meaning ------------------------------

fn rule_the_install_gate_keeps_its_meaning(sources: &Sources) -> Option<String> {
    let router = code_view(&sources.router_i2np);
    let Some(refresh) = item_body(&router, "pub fn refresh_local_router_info_record(") else {
        return Some("the daemon's re-dating helper is absent".to_owned());
    };
    // The endpoint is **supplied**: the arm binds the parameter, and the
    // record's own content is only the *predicate* that chooses the form.
    if !refresh.contains("Some(endpoint) if router_info_names_ntcp2(&fresh) =>") {
        return Some(
            "the re-dating path must supply the endpoint and choose the form by the record's content"
                .to_owned(),
        );
    }
    if !refresh.contains("install_scoped_dual_router_info(") {
        return Some("the re-dating path must install through the scoped form".to_owned());
    }
    if !refresh.contains("install_local_router_info(") {
        return Some("the re-dating path must keep the plain form".to_owned());
    }
    // The runtime's own gate is untouched: the plain form still refuses a
    // record that names an NTCP2 address, and the scoped form still
    // compares the record's address with the supplied endpoint.
    let runtime = code_view(&sources.runtime);
    let Some(inner) = item_body(&runtime, "fn install_local_router_info_inner(") else {
        return Some("the runtime's install gate is absent".to_owned());
    };
    for needle in [
        "ntcp2_addresses.is_empty()",
        "Some(endpoint) =>",
        ".get(\"host\")",
        ".get(\"port\")",
        ".get(\"v\") == Some(\"2\")",
    ] {
        if !inner.contains(needle) {
            return Some(format!("the install gate must keep its test: {needle}"));
        }
    }
    let Some(scoped) = item_body(&runtime, "pub fn install_scoped_dual_router_info(") else {
        return Some("the runtime's scoped install is absent".to_owned());
    };
    for needle in [
        "scope.admits(ntcp2_endpoint.ip())",
        "ntcp2_endpoint.port() < 1024",
    ] {
        if !scoped.contains(needle) {
            return Some(format!("the scoped install must keep its test: {needle}"));
        }
    }
    None
}

// ---- 4. the daemon never writes reachability ----------------------------

fn rule_the_daemon_never_writes_reachability(sources: &Sources) -> Option<String> {
    let owner = code_view(&sources.owner);
    for forbidden in [
        "ReachabilitySignal::",
        "ReachabilityTracker",
        "AddressFamily::Ipv4",
        "AddressFamily::Ipv6",
    ] {
        if owner.contains(forbidden) {
            return Some(format!(
                "the daemon must not write reachability of its own: {forbidden}"
            ));
        }
    }
    if owner.matches("note_confirmed_peer_test(").count() != 1 {
        return Some("the runtime's recorder must be called from exactly one place".to_owned());
    }
    // The recorder is the runtime's, and the runtime derives the family.
    let runtime = code_view(&sources.runtime);
    let Some(recorder) = item_body(&runtime, "pub fn note_confirmed_peer_test(") else {
        return Some("the runtime's recorder is absent".to_owned());
    };
    for needle in ["state.active.get(&link_id)", "session.peer_addr.ip()"] {
        if !recorder.contains(needle) {
            return Some(format!(
                "the runtime must keep deriving the family: {needle}"
            ));
        }
    }
    None
}

// ---- 5. nothing new runs and nothing is advertised ----------------------

fn rule_nothing_new_runs_and_nothing_is_advertised(sources: &Sources) -> Option<String> {
    // The **changed items** add no runtime construct. A file-wide ban would
    // be wrong here: the daemon legitimately owns sockets elsewhere.
    let owner = code_view(&sources.owner);
    for signature in [
        "pub fn new(enabled: bool)",
        "pub fn handle_inbound(",
        "pub fn drive(",
    ] {
        let Some(body) = item_body(&owner, signature) else {
            return Some(format!("the owner's {signature} is absent"));
        };
        for forbidden in [
            "TcpListener",
            "UdpSocket::bind",
            "std::thread::spawn",
            "tokio::spawn",
            "sleep(",
        ] {
            if body.contains(forbidden) {
                return Some(format!(
                    "the owner must add no runtime construct: {forbidden}"
                ));
            }
        }
    }
    // The re-dating path adds no runtime construct either.
    let Some(refresh) = item_body(
        &code_view(&sources.router_i2np),
        "pub fn refresh_local_router_info_record(",
    ) else {
        return Some("the daemon's re-dating helper is absent".to_owned());
    };
    for forbidden in ["TcpListener", "UdpSocket::bind", "tokio::spawn"] {
        if refresh.contains(forbidden) {
            return Some(format!(
                "the re-dating path must add no runtime construct: {forbidden}"
            ));
        }
    }
    let local = code_view(&sources.netdb_local);
    if !local.contains("pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.62\";") {
        return Some("the controlled router version must not move".to_owned());
    }
    if sources.support.contains("advertised = true") {
        return Some("the inventory must advertise nothing".to_owned());
    }
    let Some(surface) = surface_block(&sources.support, PLAN547_SURFACE) else {
        return Some(format!(
            "the inventory must carry the {PLAN547_SURFACE} surface"
        ));
    };
    if !surface.contains("advertised = false") {
        return Some(format!(
            "the {PLAN547_SURFACE} surface must not be advertised"
        ));
    }
    if sources.manifest_sha256 != CARGO_TOML_SHA256 {
        return Some("Cargo.toml must stay byte-identical".to_owned());
    }
    if sources.lock_sha256 != CARGO_LOCK_SHA256 {
        return Some("Cargo.lock must stay byte-identical".to_owned());
    }
    None
}

// ---- 6. the predecessors' evidence is untouched --------------------------

fn rule_the_predecessors_evidence_is_untouched(sources: &Sources) -> Option<String> {
    for (label, digest, expected) in [
        (
            "Plan 544's lane",
            sources.plan544_lane_sha256.clone(),
            PLAN544_LANE_SHA256,
        ),
        (
            "Plan 544's guard",
            sources.plan544_guard_sha256.clone(),
            PLAN544_GUARD_SHA256,
        ),
        (
            "Plan 545's lane",
            sources.plan545_lane_sha256.clone(),
            PLAN545_LANE_SHA256,
        ),
        (
            "Plan 545's guard",
            sources.plan545_guard_sha256.clone(),
            PLAN545_GUARD_SHA256,
        ),
        (
            "Plan 546's lane",
            sources.plan546_lane_sha256.clone(),
            PLAN546_LANE_SHA256,
        ),
        (
            "Plan 546's guard",
            sources.plan546_guard_sha256.clone(),
            PLAN546_GUARD_SHA256,
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
fn the_owner_lives_in_one_place() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_owner_lives_in_one_place(&sources), None);
}

#[test]
fn the_default_posture_is_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_default_posture_is_unchanged(&sources), None);
}

#[test]
fn the_install_gate_keeps_its_meaning() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_install_gate_keeps_its_meaning(&sources), None);
}

#[test]
fn the_daemon_never_writes_reachability() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_daemon_never_writes_reachability(&sources), None);
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
fn the_probe_fires_when_the_product_owns_a_peer_test() {
    let mut sources = Sources::from_tree();
    sources.product = sources.product.replace(
        "pub struct ServiceProduct {",
        "pub struct ServiceProduct {\n    peer_test_owner: Option<crate::peer_test::PeerTestOwner>,",
    );
    assert!(
        rule_the_owner_lives_in_one_place(&sources).is_some(),
        "a product that owns a peer test must be reported"
    );
}

#[test]
fn the_probe_fires_when_a_second_consumer_appears() {
    let mut sources = Sources::from_tree();
    sources.composition = sources.composition.replace(
        "peer_test.handle_inbound(&outcome, owner.handle(), &token, now_ms);",
        "peer_test.handle_inbound(&outcome, owner.handle(), &token, now_ms);\n                peer_test.handle_inbound(&outcome, owner.handle(), &token, now_ms);",
    );
    assert!(
        rule_the_owner_lives_in_one_place(&sources).is_some(),
        "a second consumer of the seam must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_owner_depends_on_the_group_posture() {
    let mut sources = Sources::from_tree();
    sources.composition = sources.composition.replace(
        "crate::peer_test_runtime::PeerTestRuntime::new(ssu2_config.peer_test)",
        "crate::peer_test_runtime::PeerTestRuntime::new(ssu2_config.peer_test && has_enabled_groups)",
    );
    assert!(
        rule_the_default_posture_is_unchanged(&sources).is_some(),
        "an owner that depends on the destination-group posture must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_opt_in_defaults_on() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "fn default_ssu2_peer_test() -> bool {\n    false",
        "fn default_ssu2_peer_test() -> bool {\n    true",
    );
    assert!(
        rule_the_default_posture_is_unchanged(&sources).is_some(),
        "an opt-in that defaults on must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_endpoint_is_inferred_from_the_record() {
    let mut sources = Sources::from_tree();
    sources.router_i2np = sources.router_i2np.replace(
        "Some(endpoint) if router_info_names_ntcp2(&fresh)",
        "Some(endpoint) if router_info_names_ntcp2(&fresh) && endpoint_matches(&fresh)",
    );
    assert!(
        rule_the_install_gate_keeps_its_meaning(&sources).is_some(),
        "an endpoint that is not the supplied one must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_plain_install_stops_requiring_no_ntcp2_address() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources
        .runtime
        .replace("None => ntcp2_addresses.is_empty(),", "None => true,");
    assert!(
        rule_the_install_gate_keeps_its_meaning(&sources).is_some(),
        "a plain install that admits an NTCP2 address must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_daemon_writes_reachability() {
    let mut sources = Sources::from_tree();
    sources.owner = sources.owner.replace(
        "let service = handle.service();",
        "let service = handle.service();\n                let _family = AddressFamily::Ipv4;",
    );
    assert!(
        rule_the_daemon_never_writes_reachability(&sources).is_some(),
        "a daemon that names a family must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_owner_spawns_a_task() {
    let mut sources = Sources::from_tree();
    sources.owner = sources.owner.replace(
        "pub fn drive(",
        "pub fn drive(\n        // mutated\n    ) -> () { let _ = (); tokio::spawn(async {}); }\n    pub fn drive_unused(",
    );
    assert!(
        rule_nothing_new_runs_and_nothing_is_advertised(&sources).is_some(),
        "an owner that spawns a task must be reported"
    );
}

#[test]
fn the_probe_fires_when_the_inventory_advertises() {
    let mut sources = Sources::from_tree();
    sources.support = sources
        .support
        .replace("advertised = false", "advertised = true");
    assert!(
        rule_nothing_new_runs_and_nothing_is_advertised(&sources).is_some(),
        "an advertised surface must be reported"
    );
}

#[test]
fn the_probe_fires_when_a_predecessors_lane_moves() {
    let mut sources = Sources::from_tree();
    sources.plan546_lane_sha256 = "0".repeat(64);
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a predecessor's lane that moves must be reported"
    );
}
