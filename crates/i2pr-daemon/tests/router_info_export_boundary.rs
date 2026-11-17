//! Plan 544's guard -- the router's own RouterInfo export must stay a lab
//! handoff and nothing more.
//!
//! The lane proves the **naming** property behaviourally: the exported file
//! round-trips through the peer's own cache loader, and the same bytes under
//! any other name are refused. The lane is silent about the **shape** of the
//! change, which is the guard's claim:
//!
//! 1. the export is written under `export/`, and never into `netdb/routers/` --
//!    the cache the router loads into its **own** store;
//! 2. the filename is the record's hash, in lowercase hexadecimal;
//! 3. the directory is `0700` and the file `0600`;
//! 4. the helper reads no configuration, opens no socket, spawns nothing and
//!    takes no lock: it is a bounded, synchronous write;
//! 5. the inspection publication it sits beside is unchanged, and the helper
//!    has exactly one production call site;
//! 6. nothing is advertised -- `router.version` stays `0.9.62` and no inventory
//!    surface becomes advertised;
//! 7. `Cargo.toml` and `Cargo.lock` stay byte-identical;
//! 8. the predecessors' lanes and guards stay byte-identical.
//!
//! Every rule has an inverted probe that fires it against an **in-memory copy**
//! of the source, and every rule reports a missing item rather than passing.

#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const DAEMON_LIB: &str = "crates/i2pr-daemon/src/lib.rs";
const NETDB_LOCAL: &str = "crates/i2pr-netdb/src/local.rs";
const SUPPORT_TOML: &str = "specs/support.toml";
const MANIFEST: &str = "Cargo.toml";
const LOCK: &str = "Cargo.lock";
const PLAN515_LANE: &str = "crates/i2pr-daemon/tests/router_info_publication.rs";
const PLAN515_GUARD: &str = "crates/i2pr-daemon/tests/router_info_publication_boundary.rs";
const PLAN542_LANE: &str = "crates/i2pr-daemon/tests/netdb_retention_posture.rs";
const PLAN542_GUARD: &str = "crates/i2pr-daemon/tests/netdb_retention_posture_boundary.rs";
const PLAN543_LANE: &str = "crates/i2pr-daemon/tests/participation_release.rs";
const PLAN543_GUARD: &str = "crates/i2pr-daemon/tests/participation_release_boundary.rs";

/// PLAN515_LANE, byte for byte.
const PLAN515_LANE_SHA256: &str =
    "89a643b962a587950de9b79d2bcd0c3ece3ede570935c63424316234f58953d3";
/// PLAN515_GUARD, byte for byte.
const PLAN515_GUARD_SHA256: &str =
    "245d92476a82ab9453ed6eec467d35c6e1d22a356a205f864ac5b311fdef8888";
/// PLAN542_LANE, byte for byte.
const PLAN542_LANE_SHA256: &str =
    "0a0657a444fd5de046bb7163b6d988bc75ec4f30b63770c5b93248b352a8184e";
/// PLAN542_GUARD, byte for byte.
const PLAN542_GUARD_SHA256: &str =
    "6a610f3f3af2926870ca4d468f8ce45a80d7ba400c27c6e3eb013461d7689800";
/// PLAN543_LANE, byte for byte.
const PLAN543_LANE_SHA256: &str =
    "04cad0198ce4a94c7b0d8629ed7a3c4b26f0f93170a93fd5c82c1327e53be1f0";
/// PLAN543_GUARD, byte for byte.
const PLAN543_GUARD_SHA256: &str =
    "d2e2360ba36c22d573718b98370f48a4499c5c255ac667d6455c42201a195e47";

/// The workspace manifest, byte for byte.
const CARGO_TOML_SHA256: &str = "39c44e33add62693eef8866220de17dcfe137437b62732e5044fcaefc4870269";

/// The workspace lock, byte for byte.
const CARGO_LOCK_SHA256: &str = "f0cec0685c4ed460c7d63273cc318f7a683dc2357735d7edaa6dfa1ac243d833";

/// The helper's definition, as `item_body` anchors it.
const EXPORT_SIGNATURE: &str = "pub fn export_local_router_info(";

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
    daemon_lib: String,
    netdb_local: String,
    support: String,
    manifest_sha256: String,
    lock_sha256: String,
    plan515_lane_sha256: String,
    plan515_guard_sha256: String,
    plan542_lane_sha256: String,
    plan542_guard_sha256: String,
    plan543_lane_sha256: String,
    plan543_guard_sha256: String,
}

impl Sources {
    fn from_tree() -> Sources {
        Sources {
            daemon_lib: source_of(DAEMON_LIB),
            netdb_local: source_of(NETDB_LOCAL),
            support: source_of(SUPPORT_TOML),
            manifest_sha256: sha256_hex(source_of(MANIFEST).as_bytes()),
            lock_sha256: sha256_hex(source_of(LOCK).as_bytes()),
            plan515_lane_sha256: sha256_hex(source_of(PLAN515_LANE).as_bytes()),
            plan515_guard_sha256: sha256_hex(source_of(PLAN515_GUARD).as_bytes()),
            plan542_lane_sha256: sha256_hex(source_of(PLAN542_LANE).as_bytes()),
            plan542_guard_sha256: sha256_hex(source_of(PLAN542_GUARD).as_bytes()),
            plan543_lane_sha256: sha256_hex(source_of(PLAN543_LANE).as_bytes()),
            plan543_guard_sha256: sha256_hex(source_of(PLAN543_GUARD).as_bytes()),
        }
    }

    /// The helper's body, with comments and test-only blocks removed.
    fn export_body(&self) -> Option<String> {
        item_body(&code_view(&self.daemon_lib), EXPORT_SIGNATURE)
    }
}

// ---- 1. the export is written under `export/` ----------------------------

fn rule_the_export_is_written_to_the_export_directory(sources: &Sources) -> Option<String> {
    let Some(body) = sources.export_body() else {
        return Some("the export helper is absent".to_owned());
    };
    if !body.contains("data_dir.join(\"export\")") {
        return Some("the export must be written under export/".to_owned());
    }
    // `netdb/routers/` is the cache the router loads into its **own** store at
    // startup. A record written there would be re-ingested by the router that
    // produced it. The helper must name neither directory, and must not reach
    // for the cache seam at all. The bare word `netdb` is **not** a forbidden
    // token, because the hash derivation legitimately names `i2pr_netdb`; the
    // rule forbids the *path*, which is what the loader reads.
    for forbidden in [
        "netdb/routers",
        "join(\"netdb\")",
        "\"routers\"",
        "ROUTERS_SUBDIR",
        "ByteCache",
        "cache_seam",
    ] {
        if body.contains(forbidden) {
            return Some(format!(
                "the export must never name the router's own load path: {forbidden}"
            ));
        }
    }
    None
}

// ---- 2. the filename is the record's hash, in lowercase hex --------------

fn rule_the_name_is_the_hash_in_lowercase_hex(sources: &Sources) -> Option<String> {
    let Some(body) = sources.export_body() else {
        return Some("the export helper is absent".to_owned());
    };
    if !body.contains("i2pr_netdb::router_hash(router_info.router_identity())") {
        return Some("the name must be derived from the record's own identity".to_owned());
    }
    // The name describes the bytes that were decoded, not a parallel source.
    if !body.contains("i2pr_proto::RouterInfo::decode(encoded") {
        return Some("the name must be derived from the bytes being written".to_owned());
    }
    if !body.contains("String::with_capacity(64)") {
        return Some("a router hash is 64 hexadecimal characters".to_owned());
    }
    if !body.contains("format!(\"{byte:02x}\")") {
        return Some("the name must be lowercase hexadecimal".to_owned());
    }
    for forbidden in ["{byte:02X}", "{byte:X}", "to_uppercase", "to_lowercase"] {
        if body.contains(forbidden) {
            return Some(format!(
                "the name must not change case after formatting: {forbidden}"
            ));
        }
    }
    if !body.contains("directory.join(&hex)") {
        return Some("the file must be named by the derived hash".to_owned());
    }
    // The bytes written are the bytes the router signed: no re-encoding.
    if !body.contains("std::fs::write(&path, encoded)") {
        return Some("the export must write the encoded record unchanged".to_owned());
    }
    None
}

// ---- 3. the export is written private -----------------------------------

fn rule_the_export_is_written_private(sources: &Sources) -> Option<String> {
    let Some(body) = sources.export_body() else {
        return Some("the export helper is absent".to_owned());
    };
    if !body
        .contains("std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))")
    {
        return Some("the export directory must be 0700".to_owned());
    }
    if !body.contains("std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))") {
        return Some("the export file must be 0600".to_owned());
    }
    // A permissive mode anywhere in the helper, or a mode read from the
    // environment, would let the handoff depend on the operator's umask.
    for forbidden in ["0o644", "0o755", "0o666", "0o777", "0o400"] {
        if body.contains(forbidden) {
            return Some(format!("the export must not use mode {forbidden}"));
        }
    }
    None
}

// ---- 4. the export needs no configuration and opens no socket -----------

fn rule_the_export_needs_no_configuration_and_opens_no_socket(sources: &Sources) -> Option<String> {
    let Some(body) = sources.export_body() else {
        return Some("the export helper is absent".to_owned());
    };
    for forbidden in [
        "Config",
        "config.",
        "std::env",
        "env::var",
        "TcpListener",
        "TcpStream",
        "UdpSocket",
        "std::net",
        "bind(",
        "tokio",
        "spawn",
        "async ",
        "await",
        "Mutex",
        "Arc<",
        "std::thread",
        "Command",
    ] {
        if body.contains(forbidden) {
            return Some(format!(
                "the export is a bounded synchronous write, not {forbidden}"
            ));
        }
    }
    // It writes, and it creates the one directory it owns.
    if !body.contains("std::fs::create_dir_all(&directory)") {
        return Some("the export must create only its own directory".to_owned());
    }
    None
}

// ---- 5. the publication path is unchanged, and the call site is one -----

fn rule_the_publication_path_is_unchanged(sources: &Sources) -> Option<String> {
    let code = code_view(&sources.daemon_lib);
    if !code.contains(".publish_local_router_info_b64(") {
        return Some("the inspection publication must stay in place".to_owned());
    }
    // The definition plus exactly one production call site.
    let calls = code.matches("export_local_router_info(").count();
    if calls != 2 {
        return Some(format!(
            "the export must have one definition and one call site: {calls}"
        ));
    }
    None
}

// ---- 6. nothing is advertised -------------------------------------------

fn rule_nothing_is_advertised(sources: &Sources) -> Option<String> {
    let netdb = code_view(&sources.netdb_local);
    if !netdb.contains("pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.62\";") {
        return Some("router.version must stay 0.9.62".to_owned());
    }
    let Some(start) = sources.support.find("id = \"netdb.router-info-export\"") else {
        return Some("the inventory must carry the export surface".to_owned());
    };
    let block_end = sources.support[start..]
        .find("[[surface]]")
        .map(|offset| start + offset)
        .unwrap_or(sources.support.len());
    let block = &sources.support[start..block_end];
    if !block.contains("advertised = false") {
        return Some("the export surface must not be advertised".to_owned());
    }
    if sources.support.contains("advertised = true") {
        return Some("no inventory surface may become advertised".to_owned());
    }
    None
}

// ---- 7. the manifest and the lock are unchanged -------------------------

fn rule_the_manifest_and_the_lock_are_unchanged(sources: &Sources) -> Option<String> {
    if sources.manifest_sha256 != CARGO_TOML_SHA256 {
        return Some("Cargo.toml must stay byte-identical".to_owned());
    }
    if sources.lock_sha256 != CARGO_LOCK_SHA256 {
        return Some("Cargo.lock must stay byte-identical".to_owned());
    }
    None
}

// ---- 8. the predecessors' evidence is untouched -------------------------

fn rule_the_predecessors_evidence_is_untouched(sources: &Sources) -> Option<String> {
    for (label, digest, expected) in [
        (
            "Plan 515's lane",
            sources.plan515_lane_sha256.clone(),
            PLAN515_LANE_SHA256,
        ),
        (
            "Plan 515's guard",
            sources.plan515_guard_sha256.clone(),
            PLAN515_GUARD_SHA256,
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
        (
            "Plan 543's lane",
            sources.plan543_lane_sha256.clone(),
            PLAN543_LANE_SHA256,
        ),
        (
            "Plan 543's guard",
            sources.plan543_guard_sha256.clone(),
            PLAN543_GUARD_SHA256,
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
fn the_export_is_written_to_the_export_directory() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_export_is_written_to_the_export_directory(&sources),
        None
    );
}

#[test]
fn the_name_is_the_hash_in_lowercase_hex() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_name_is_the_hash_in_lowercase_hex(&sources), None);
}

#[test]
fn the_export_is_written_private() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_export_is_written_private(&sources), None);
}

#[test]
fn the_export_needs_no_configuration_and_opens_no_socket() {
    let sources = Sources::from_tree();
    assert_eq!(
        rule_the_export_needs_no_configuration_and_opens_no_socket(&sources),
        None
    );
}

#[test]
fn the_publication_path_is_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_publication_path_is_unchanged(&sources), None);
}

#[test]
fn nothing_is_advertised() {
    let sources = Sources::from_tree();
    assert_eq!(rule_nothing_is_advertised(&sources), None);
}

#[test]
fn the_manifest_and_the_lock_are_unchanged() {
    let sources = Sources::from_tree();
    assert_eq!(rule_the_manifest_and_the_lock_are_unchanged(&sources), None);
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
fn the_inverted_probe_fires_when_the_export_moves_into_the_router_cache() {
    let mut sources = Sources::from_tree();
    sources.daemon_lib = sources.daemon_lib.replace(
        "data_dir.join(\"export\")",
        "data_dir.join(\"netdb\").join(\"routers\")",
    );
    assert!(
        rule_the_export_is_written_to_the_export_directory(&sources).is_some(),
        "an export in the router's own load path must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_name_is_uppercase() {
    let mut sources = Sources::from_tree();
    sources.daemon_lib = sources.daemon_lib.replace("{byte:02x}", "{byte:02X}");
    assert!(
        rule_the_name_is_the_hash_in_lowercase_hex(&sources).is_some(),
        "an uppercase name must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_export_is_world_readable() {
    let mut sources = Sources::from_tree();
    sources.daemon_lib = sources
        .daemon_lib
        .replace("from_mode(0o600)", "from_mode(0o644)");
    assert!(
        rule_the_export_is_written_private(&sources).is_some(),
        "a world-readable export must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_export_reads_configuration() {
    let mut sources = Sources::from_tree();
    sources.daemon_lib = sources.daemon_lib.replace(
        "let directory = data_dir.join(\"export\");",
        "let _ = config.router.data_dir.clone();\n    let directory = data_dir.join(\"export\");",
    );
    assert!(
        rule_the_export_needs_no_configuration_and_opens_no_socket(&sources).is_some(),
        "an export that reads configuration must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_publication_is_removed() {
    let mut sources = Sources::from_tree();
    sources.daemon_lib = sources.daemon_lib.replace(
        ".publish_local_router_info_b64(",
        ".publish_local_router_info_gone(",
    );
    assert!(
        rule_the_publication_path_is_unchanged(&sources).is_some(),
        "a removed inspection publication must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_version_moves() {
    let mut sources = Sources::from_tree();
    sources.netdb_local = sources.netdb_local.replace(
        "CONTROLLED_ROUTER_VERSION: &str = \"0.9.62\"",
        "CONTROLLED_ROUTER_VERSION: &str = \"0.9.63\"",
    );
    assert!(
        rule_nothing_is_advertised(&sources).is_some(),
        "a moved router.version must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_surface_becomes_advertised() {
    let mut sources = Sources::from_tree();
    sources.support = sources
        .support
        .replace("advertised = false", "advertised = true");
    assert!(
        rule_nothing_is_advertised(&sources).is_some(),
        "an advertised surface must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_the_manifest_changes() {
    let mut sources = Sources::from_tree();
    sources.manifest_sha256 = "0".repeat(64);
    assert!(
        rule_the_manifest_and_the_lock_are_unchanged(&sources).is_some(),
        "a changed manifest must fail the rule"
    );
}

#[test]
fn the_inverted_probe_fires_when_a_predecessor_changes() {
    let mut sources = Sources::from_tree();
    sources.plan543_guard_sha256 = "0".repeat(64);
    assert!(
        rule_the_predecessors_evidence_is_untouched(&sources).is_some(),
        "a predecessor's evidence must fail the rule"
    );
}
