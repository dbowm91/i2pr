//! Plan 546's guard — the publication interlock is an operator precondition.
//!
//! The lane proves the relaxation's **behaviour**. This guard proves its
//! **shape**, and the shape is what keeps the relaxation from becoming a
//! blanket one:
//!
//! 1. `may_publish_bound_address` is the single authority for "may this bound
//!    socket's address appear in a record", and both publication entry points
//!    call it instead of writing their own loopback condition;
//! 2. the reachability and expiry gates are untouched — the relaxation admits
//!    an address, not an unreachable one;
//! 3. the default is off, in the runtime config, in the daemon's raw config and
//!    in its default helper;
//! 4. the floodfill keeps one authority: the routability arm survives for the
//!    un-opted-in path, and the opt-in is the only bypass;
//! 5. the operator-facing documentation names the NAT precondition, so the key
//!    cannot be set without the sentence that says when not to;
//! 6. nothing new runs, nothing is advertised, the manifest is byte-identical
//!    and the predecessors' evidence is byte-identical.
//!
//! Every rule has an inverted probe that fires it against an **in-memory copy**
//! of the source, and every rule reports a missing item rather than passing.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const RUNTIME: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
const DAEMON_CONFIG: &str = "crates/i2pr-daemon/src/config.rs";
const FLOODFILL: &str = "crates/i2pr-daemon/src/floodfill.rs";
const NETDB_LOCAL: &str = "crates/i2pr-netdb/src/local.rs";
const SUPPORT_TOML: &str = "specs/support.toml";
const MANIFEST: &str = "Cargo.toml";
const LOCK: &str = "Cargo.lock";
const PLAN544_LANE: &str = "crates/i2pr-daemon/tests/router_info_export.rs";
const PLAN544_GUARD: &str = "crates/i2pr-daemon/tests/router_info_export_boundary.rs";
const PLAN545_LANE: &str = "crates/i2pr-daemon/tests/peer_pool_fallback.rs";
const PLAN545_GUARD: &str = "crates/i2pr-daemon/tests/peer_pool_fallback_boundary.rs";

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

/// Extracts the item whose **definition** starts at `signature`, block
/// included.
fn item_text(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (_, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[start..close].to_owned())
}

struct Sources {
    runtime: String,
    daemon_config: String,
    floodfill: String,
    netdb_local: String,
    support: String,
    manifest_sha256: String,
    lock_sha256: String,
    plan544_lane_sha256: String,
    plan544_guard_sha256: String,
    plan545_lane_sha256: String,
    plan545_guard_sha256: String,
}

impl Sources {
    fn from_tree() -> Sources {
        Sources {
            runtime: source_of(RUNTIME),
            daemon_config: source_of(DAEMON_CONFIG),
            floodfill: source_of(FLOODFILL),
            netdb_local: source_of(NETDB_LOCAL),
            support: source_of(SUPPORT_TOML),
            manifest_sha256: sha256_hex(source_of(MANIFEST).as_bytes()),
            lock_sha256: sha256_hex(source_of(LOCK).as_bytes()),
            plan544_lane_sha256: sha256_hex(source_of(PLAN544_LANE).as_bytes()),
            plan544_guard_sha256: sha256_hex(source_of(PLAN544_GUARD).as_bytes()),
            plan545_lane_sha256: sha256_hex(source_of(PLAN545_LANE).as_bytes()),
            plan545_guard_sha256: sha256_hex(source_of(PLAN545_GUARD).as_bytes()),
        }
    }
}

// ---- 1. one authority for "may this address be published" ---------------

fn rule_the_predicate_is_one_authority(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.runtime);

    if code.matches("pub fn may_publish_bound_address(").count() != 1 {
        return Some("the publication predicate must be defined exactly once".to_owned());
    }
    let Some(predicate) = item_body(
        &code,
        "pub fn may_publish_bound_address(bound: std::net::SocketAddr, publish_bound_address: bool)",
    ) else {
        return Some("the publication predicate is absent".to_owned());
    };
    // It must refuse an unbound/unspecified endpoint outright, admit loopback,
    // and otherwise defer to the operator's key.
    for needle in [
        "bound.port() == 0",
        "bound.ip().is_unspecified()",
        "bound.ip().is_loopback()",
        "publish_bound_address",
    ] {
        if !predicate.contains(needle) {
            return Some(format!("the predicate must decide on {needle}"));
        }
    }
    // Both publication entry points call it, and neither restates the loopback
    // condition inline.
    for site in [
        "pub fn publication_material(",
        "pub fn note_explicit_bind_for_controlled_qualification(",
    ] {
        let Some(body) = item_body(&code, site) else {
            return Some(format!("the publication entry point is absent: {site}"));
        };
        if !body.contains("may_publish_bound_address(") {
            return Some(format!("{site} must call the publication predicate"));
        }
        if body.contains(".ip().is_loopback()") {
            return Some(format!(
                "{site} restates the loopback condition instead of deferring"
            ));
        }
    }
    None
}

// ---- 2. the reachability and expiry gates are untouched ----------------

fn rule_the_reachability_gates_are_untouched(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.runtime);
    let Some(body) = item_body(&code, "pub fn publication_material(") else {
        return Some("publication_material is absent".to_owned());
    };
    for needle in [
        "reachability != ReachabilityState::Reachable",
        "expiry <= now",
    ] {
        if !body.contains(needle) {
            return Some(format!("the publication gate must keep its test: {needle}"));
        }
    }
    None
}

// ---- 3. the default is off, in all three places ------------------------

fn rule_the_default_is_off(sources: &Sources) -> Option<String> {
    let runtime = code_view(&sources.runtime);
    if !runtime.contains("pub publish_bound_address: bool,") {
        return Some("the runtime config must carry the field".to_owned());
    }
    if !runtime.contains("derive(Clone, Copy, Debug, Default, Eq, PartialEq)") {
        return Some("the runtime config must keep its derived Default".to_owned());
    }
    let config = code_view(&sources.daemon_config);
    if !config.contains("#[serde(default = \"default_ssu2_publish_bound_address\")]") {
        return Some("the daemon key must default through its own helper".to_owned());
    }
    let Some(helper) = item_body(
        &config,
        "const fn default_ssu2_publish_bound_address() -> bool",
    ) else {
        return Some("the daemon default helper is absent".to_owned());
    };
    if !helper.contains("false") {
        return Some("the daemon default must be false".to_owned());
    }
    if !config.contains("publish_bound_address: raw.publish_bound_address,") {
        return Some("the raw key must be mapped through".to_owned());
    }
    None
}

// ---- 4. the floodfill keeps one authority ------------------------------

fn rule_the_floodfill_keeps_one_authority(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.floodfill);
    let Some(body) = item_body(&code, "fn is_normal_qualified_address(") else {
        return Some("the floodfill qualification is absent".to_owned());
    };
    // The routability arm must survive for the un-opted-in path ...
    if !body.contains("is_publicly_routable_host(host)") {
        return Some("the un-opted-in path must keep its routability arm".to_owned());
    }
    // ... and the opt-in must be the only bypass, guarded by an early return.
    if !body.contains("if bound_address_publishable {") {
        return Some("the opt-in must be the documented bypass".to_owned());
    }
    if !body.contains("is_qualified_ssu2_address(&material.address)") {
        return Some("the address form must still be qualified first".to_owned());
    }
    // The readiness field exists and the evaluation reads it.
    if !code.contains("pub bound_address_publishable: bool,") {
        return Some("the readiness input must carry the opt-in".to_owned());
    }
    let Some(evaluate) = item_body(&code, "pub fn evaluate_normal_eligibility(") else {
        return Some("the evaluation is absent".to_owned());
    };
    if !evaluate.contains("readiness.bound_address_publishable") {
        return Some("the evaluation must read the operator's opt-in".to_owned());
    }
    None
}

// ---- 5. the key's documentation names the precondition -----------------

fn rule_the_key_documents_the_precondition(sources: &Sources) -> Option<String> {
    let config = sources.daemon_config.as_str();
    let Some(documentation) = item_text(config, "pub publish_bound_address: bool,") else {
        return Some("the parsed config field is absent".to_owned());
    };
    // The field's own documentation is what an operator reads.
    let start = config
        .find("Whether this router may publish its own **bound** SSU2 address")
        .or_else(|| config.find("Whether this router may publish its own"));
    let Some(start) = start else {
        return Some("the parsed field must document what it permits".to_owned());
    };
    let tail = &config[start..];
    for needle in ["NAT", "directly reachable"] {
        if !tail[..tail
            .find("pub publish_bound_address: bool,")
            .unwrap_or(tail.len())]
            .contains(needle)
        {
            return Some(format!(
                "the field's documentation must name the precondition: {needle}"
            ));
        }
    }
    let _ = documentation;
    None
}

// ---- 6. nothing new runs, nothing is advertised ------------------------

fn rule_nothing_new_runs_and_nothing_is_advertised(sources: &Sources) -> Option<String> {
    // The **changed items** add no runtime construct. A file-wide ban would be
    // wrong here: the runtime legitimately binds its UDP sockets.
    for (label, source, signature) in [
        (
            "the publication predicate",
            &sources.runtime,
            "pub fn may_publish_bound_address",
        ),
        (
            "publication_material",
            &sources.runtime,
            "pub fn publication_material(",
        ),
        (
            "the floodfill rule",
            &sources.floodfill,
            "fn is_normal_qualified_address(",
        ),
    ] {
        let Some(body) = item_body(&code_view(source), signature) else {
            return Some(format!("{label} is absent"));
        };
        for forbidden in [
            "TcpListener",
            "UdpSocket::bind",
            "std::thread::spawn",
            "tokio::spawn",
        ] {
            if body.contains(forbidden) {
                return Some(format!(
                    "{label} must add no runtime construct: {forbidden}"
                ));
            }
        }
    }
    let local = code_view(&sources.netdb_local);
    if !local.contains("pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.62\";") {
        return Some("the controlled router version must not move".to_owned());
    }
    if sources.support.contains("advertised = true") {
        return Some("the inventory must advertise nothing".to_owned());
    }
    if sources.manifest_sha256 != CARGO_TOML_SHA256 {
        return Some("Cargo.toml must stay byte-identical".to_owned());
    }
    if sources.lock_sha256 != CARGO_LOCK_SHA256 {
        return Some("Cargo.lock must stay byte-identical".to_owned());
    }
    None
}

// ---- 7. the predecessors' evidence is untouched ------------------------

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
fn the_predicate_is_one_authority() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_predicate_is_one_authority(&sources), None);
}

#[test]
fn the_reachability_gates_are_untouched() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_reachability_gates_are_untouched(&sources), None);
}

#[test]
fn the_default_is_off() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_default_is_off(&sources), None);
}

#[test]
fn the_floodfill_keeps_one_authority() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_floodfill_keeps_one_authority(&sources), None);
}

#[test]
fn the_key_documents_the_precondition() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_key_documents_the_precondition(&sources), None);
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
fn the_inverted_probe_fires_when_a_site_restates_the_loopback_condition() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "if !may_publish_bound_address(bound, self.shared.config.publish_bound_address) {\n            return Err(Ssu2PublicationUnavailable::InvalidPublication);\n        }\n        let endpoint",
        "if !bound.ip().is_loopback() {\n            return Err(Ssu2PublicationUnavailable::InvalidPublication);\n        }\n        let endpoint",
    );
    assert!(
        rule_the_predicate_is_one_authority(&sources).is_some(),
        "an inline loopback condition must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_reachability_gate_is_dropped() {
    let mut sources = Sources::from_tree();
    sources.runtime = sources.runtime.replace(
        "if reachability != ReachabilityState::Reachable || expiry <= now {",
        "if false {",
    );
    assert!(
        rule_the_reachability_gates_are_untouched(&sources).is_some(),
        "a dropped reachability gate must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_default_flips() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace(
        "const fn default_ssu2_publish_bound_address() -> bool {\n    false\n}",
        "const fn default_ssu2_publish_bound_address() -> bool {\n    true\n}",
    );
    assert!(
        rule_the_default_is_off(&sources).is_some(),
        "a default that is not false must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_routability_arm_is_removed() {
    let mut sources = Sources::from_tree();
    sources.floodfill = sources
        .floodfill
        .replace("is_publicly_routable_host(host)", "true");
    assert!(
        rule_the_floodfill_keeps_one_authority(&sources).is_some(),
        "a floodfill with no routability arm must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_precondition_is_undocumented() {
    let mut sources = Sources::from_tree();
    sources.daemon_config = sources.daemon_config.replace("NAT", "elsewhere");
    assert!(
        rule_the_key_documents_the_precondition(&sources).is_some(),
        "a key that does not name the NAT precondition must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_task_appears() {
    let mut sources = Sources::from_tree();
    // Plant the task inside the rule the ban actually inspects.
    sources.floodfill = sources.floodfill.replace(
        "    if bound_address_publishable {\n        return true;\n    }",
        "    let _ = tokio::spawn(async {});\n    if bound_address_publishable {\n        return true;\n    }",
    );
    assert!(
        rule_nothing_new_runs_and_nothing_is_advertised(&sources).is_some(),
        "an ownerless task must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_predecessor_changes() {
    let mut sources = Sources::from_tree();
    sources.plan545_guard_sha256 = "0".repeat(64);
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a predecessor's evidence must fail the rule"
    );
}
