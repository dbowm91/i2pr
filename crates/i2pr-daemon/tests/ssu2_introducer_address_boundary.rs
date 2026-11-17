//! Plan 523's structural guard for the SSU2 introducer address form.
//!
//! The SSU2 **introducer set** — the peers that will introduce this router
//! to others — is the only way a firewalled router is reachable at all.
//! This router's implementation of that set was the **SSU 1** form: the
//! specification says of `ihost`/`iport`/`ikey` that those options "are for
//! SSU only and are not used for SSU2", and the SSU2 tuple is `ih[N]` (the
//! introducer's router hash) + `iexp[N]` (its expiry) + `itag[N]` (its
//! relay tag).
//!
//! Two defects followed. The router could not **read** a real peer's
//! introducer set, because the parser refused `ih`/`iexp` as unknown
//! options and that refusal is production-reachable. And the router's own
//! introducer-only address, were it published, would have been
//! **unreadable**, because the publication builder emitted the SSU 1
//! options that the pinned i2pd 2.61.0 does not parse.
//!
//! The guard asserts thirteen things about the **production** surface:
//!
//!  1. the SSU 1 introducer option names are **absent** from the
//!     transport's production source: they are not part of an SSU2 address;
//!  2. the transport's introducer dispatch names **exactly** the
//!     specification's three options;
//!  3. the transport's introducer value carries the **hash**, the **expiry**
//!     and the **tag**, and exposes no endpoint: an SSU2 introducer is
//!     resolved by hash, not dialled;
//!  4. the transport's parser **refuses** an address that carries both a
//!     direct endpoint and introducers;
//!  5. the transport's "direct with introducers" classification is **gone**:
//!     the specification forbids the form it described;
//!  6. the transport's publication builder emits the **specification's**
//!     option names and no SSU 1 one;
//!  7. the transport's introducer table maps the record's **hash** and
//!     **expiry** into the published value;
//!  8. the handshake's "this address claims introducers" marker names the
//!     **specification's** options, so the check can still fire;
//!  9. the netdb's **direct** rule and its **direct** builder are unchanged:
//!     the introducer rule is added beside them, not over them;
//! 10. the netdb's introducer rule **forbids** a host and a port;
//! 11. the netdb's introducer rule requires a **complete** introducer group;
//! 12. the netdb's introducer builder carries **no** floodfill permit;
//! 13. the proto crate carries **no** introducer option vocabulary: the
//!     plan added no wire vocabulary.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required construct. Comments are stripped and
//! `#[cfg(test)]` blocks are excluded, so a doc comment that merely names a
//! construct never satisfies a rule.
//!
//! Every rule **fails closed**. A rule whose named item is absent reports
//! that absence as a failure, because a missing function is a defect and not
//! a pass.
//!
//! Every rule has an **inverted probe**: a mutation that must be reported. A
//! rule with no probe is a rule nobody has shown to fire.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The transport's SSU2 address vocabulary.
const ADDRESS_MODULE: &str = "crates/i2pr-transport-ssu2/src/address.rs";
/// The transport's publication builder.
const PUBLICATION_MODULE: &str = "crates/i2pr-transport-ssu2/src/publication.rs";
/// The transport's validated introducer table.
const INTRODUCER_MODULE: &str = "crates/i2pr-transport-ssu2/src/introducer.rs";
/// The transport's handshake.
const HANDSHAKE_MODULE: &str = "crates/i2pr-transport-ssu2/src/handshake.rs";
/// The netdb's advertisement authority vocabulary.
const ROLE_MODULE: &str = "crates/i2pr-netdb/src/floodfill_role.rs";
/// The netdb's local RouterInfo builder.
const LOCAL_MODULE: &str = "crates/i2pr-netdb/src/local.rs";
/// The proto crate's source root.
const PROTO_ROOT: &str = "crates/i2pr-proto/src";

/// Removes every whitespace character from a code view.
///
/// A rule that names a construct must survive the formatter's line
/// wrapping, so the two texts are compared with their whitespace squeezed
/// out. The comments are already gone, so a squeezed match is a match on
/// code and not on prose.
fn squeezed(code: &str) -> String {
    code.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

/// Extracts the body of the item whose **definition** is anchored at
/// `signature`, or reports the absence as a failure.
fn required_item_body(code: &str, signature: &str) -> Result<String, String> {
    item_body(code, signature).ok_or_else(|| format!("the named item {signature} is absent"))
}

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

/// Reads every Rust source file under `relative`, concatenated.
fn sources_under(relative: &str) -> String {
    let root = workspace_root().join(relative);
    let mut out = String::new();
    let mut stack = vec![root];
    while let Some(directory) = stack.pop() {
        let entries = fs::read_dir(&directory).unwrap_or_else(|error| {
            panic!("read {}: {error}", directory.display());
        });
        for entry in entries {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|extension| extension == "rs") {
                out.push_str(&fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!("read {}: {error}", path.display());
                }));
            }
        }
    }
    out
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

/// Extracts the **whole** item — its signature and its body — whose
/// definition is anchored at `signature`.
///
/// A rule about what an item *accepts* must see the signature, not only
/// the body: a parameter is part of what the item offers its callers.
fn required_item_text(code: &str, signature: &str) -> Result<String, String> {
    let start = find_definition(code, signature)
        .ok_or_else(|| format!("the named item {signature} is absent"))?;
    let (_, close) = brace_balanced_block(code, start + signature.len())
        .ok_or_else(|| format!("the named item {signature} has no body"))?;
    Ok(code[start..close].to_owned())
}

// ---------------------------------------------------------------------------
// Rule 1: the SSU 1 introducer option names are absent from the transport's
// production source.
// ---------------------------------------------------------------------------

fn check_the_ssu1_introducer_options_are_absent(address: &str) -> Result<(), String> {
    let code = code_view(address);
    for forbidden in [
        "\"ihost\"",
        "\"iport\"",
        "\"ikey\"",
        "ihost{",
        "iport{",
        "ikey{",
    ] {
        if code.contains(forbidden) {
            return Err(format!(
                "an SSU2 address carries no SSU 1 introducer option, found {forbidden}"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 2: the transport's introducer dispatch names exactly the
// specification's three options.
// ---------------------------------------------------------------------------

fn check_the_introducer_dispatch_names_the_specification_options(
    address: &str,
) -> Result<(), String> {
    let code = code_view(address);
    let dispatch = required_item_body(&code, "fn parse_introducer_index")?;
    for required in [
        "INTRODUCER_HASH_PREFIX",
        "INTRODUCER_EXPIRY_PREFIX",
        "INTRODUCER_TAG_PREFIX",
    ] {
        if !dispatch.contains(required) {
            return Err(format!(
                "the introducer dispatch must name {required}, the specification's own option"
            ));
        }
    }
    for declaration in [
        "const INTRODUCER_HASH_PREFIX: &str = \"ih\";",
        "const INTRODUCER_EXPIRY_PREFIX: &str = \"iexp\";",
        "const INTRODUCER_TAG_PREFIX: &str = \"itag\";",
    ] {
        if !code.contains(declaration) {
            return Err(format!(
                "the specification's option declaration {declaration} must be present"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 3: the transport's introducer value carries the hash, the expiry and
// the tag, and exposes no endpoint.
// ---------------------------------------------------------------------------

fn check_the_introducer_value_carries_the_specification_tuple(address: &str) -> Result<(), String> {
    let code = code_view(address);
    let value = required_item_body(&code, "pub struct Ssu2Introducer")?;
    for required in ["peer_hash", "expires_secs", "relay_tag"] {
        if !value.contains(required) {
            return Err(format!(
                "the SSU2 introducer value must carry {required}: the specification's tuple"
            ));
        }
    }
    for forbidden in ["endpoint", "intro_key", "Ssu2Endpoint", "IntroKey"] {
        if value.contains(forbidden) {
            return Err(format!(
                "the SSU2 introducer value must not carry {forbidden}: an SSU2 introducer is resolved by hash"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 4: the transport's parser refuses an address that carries both a
// direct endpoint and introducers.
// ---------------------------------------------------------------------------

fn check_the_parser_refuses_an_endpoint_with_introducers(address: &str) -> Result<(), String> {
    let code = code_view(address);
    if !code.contains("HostPortWithIntroducers") {
        return Err(
            "the transport must carry a typed refusal for an endpoint with introducers".to_owned(),
        );
    }
    let parsed = required_item_body(&code, "fn from_parsed(")?;
    if !parsed.contains("HostPortWithIntroducers") {
        return Err(
            "the parser must refuse an address that carries both an endpoint and introducers"
                .to_owned(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 5: the transport's "direct with introducers" classification is gone.
// ---------------------------------------------------------------------------

fn check_the_direct_with_introducers_class_is_gone(address: &str) -> Result<(), String> {
    let code = code_view(address);
    if code.contains("DirectWithIntroducers") {
        return Err(
            "the specification forbids an address that publishes both an endpoint and introducers, so the classification must be gone"
                .to_owned(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 6: the transport's publication builder emits the specification's
// option names and no SSU 1 one.
// ---------------------------------------------------------------------------

fn check_the_publication_builder_emits_the_specification_options(
    publication: &str,
) -> Result<(), String> {
    let code = code_view(publication);
    let snapshot = required_item_body(&code, "fn introducer_snapshot(")?;
    for required in [
        "format!(\"ih{index}\")",
        "format!(\"iexp{index}\")",
        "format!(\"itag{index}\")",
    ] {
        if !snapshot.contains(required) {
            return Err(format!(
                "the publication builder must emit {required}, the specification's option"
            ));
        }
    }
    for forbidden in ["ihost{", "iport{", "ikey{"] {
        if snapshot.contains(forbidden) {
            return Err(format!(
                "the publication builder must not emit the SSU 1 option {forbidden}"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 7: the transport's introducer table maps the record's hash and expiry
// into the published value.
// ---------------------------------------------------------------------------

fn check_the_introducer_table_maps_the_hash_and_expiry(introducer: &str) -> Result<(), String> {
    let code = code_view(introducer);
    let mapping = required_item_body(&code, "pub fn validated_introducers(")?;
    for required in [
        "record.peer_hash",
        "record.expires_secs",
        "record.relay_tag",
    ] {
        if !mapping.contains(required) {
            return Err(format!(
                "the introducer table must map {required} into the published value"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 8: the handshake's introducer marker names the specification's
// options.
// ---------------------------------------------------------------------------

fn check_the_handshake_marker_names_the_specification_options(
    handshake: &str,
) -> Result<(), String> {
    let code = code_view(handshake);
    for required in ["get(\"ih0\")", "get(\"itag0\")"] {
        if !code.contains(required) {
            return Err(format!(
                "the handshake's introducer marker must name {required}, or the check can never fire"
            ));
        }
    }
    if code.contains("get(\"ihost0\")") {
        return Err("the handshake's introducer marker must not name the SSU 1 option".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 9: the netdb's direct rule and its direct builder are unchanged.
// ---------------------------------------------------------------------------

fn check_the_netdb_direct_rule_is_unchanged(role: &str, local: &str) -> Result<(), String> {
    let role_code = code_view(role);
    let direct = required_item_body(&role_code, "pub fn is_qualified_ssu2_address")?;
    let allow_set = "\"host\"|\"port\"|\"mtu\"|\"v\"|\"caps\"|\"s\"|\"i\"";
    if !squeezed(&direct).contains(&squeezed(allow_set)) {
        return Err(
            "the netdb's direct qualification rule must keep its own option allow-set".to_owned(),
        );
    }
    let local_code = code_view(local);
    let builder = required_item_body(&local_code, "pub fn build_with_ssu2_address")?;
    if !builder.contains("is_qualified_ssu2_address") {
        return Err(
            "the netdb's direct builder must keep the direct rule as its authority".to_owned(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 10: the netdb's introducer rule forbids a host and a port.
// ---------------------------------------------------------------------------

fn check_the_netdb_introducer_rule_forbids_an_endpoint(role: &str) -> Result<(), String> {
    let code = code_view(role);
    let rule = required_item_body(&code, "pub fn is_qualified_ssu2_introducer_address")?;
    let squeezed_rule = squeezed(&rule);
    for required in ["get(\"host\").is_some()", "get(\"port\").is_some()"] {
        if !squeezed_rule.contains(&squeezed(required)) {
            return Err(format!(
                "the netdb's introducer rule must forbid {required}: the specification forbids a host or a port alongside introducers"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 11: the netdb's introducer rule requires a complete introducer group.
// ---------------------------------------------------------------------------

fn check_the_netdb_introducer_rule_requires_a_complete_group(role: &str) -> Result<(), String> {
    let code = code_view(role);
    let rule = required_item_body(&code, "pub fn is_qualified_ssu2_introducer_address")?;
    let squeezed_rule = squeezed(&rule);
    for required in ["hashes.keys()", "tags.contains_key(index)"] {
        if !squeezed_rule.contains(&squeezed(required)) {
            return Err(format!(
                "the netdb's introducer rule must require {required}: an address with no complete introducer group is not qualified"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 12: the netdb's introducer builder carries no floodfill permit.
// ---------------------------------------------------------------------------

fn check_the_netdb_introducer_builder_carries_no_permit(local: &str) -> Result<(), String> {
    let code = code_view(local);
    let builder = required_item_text(&code, "pub fn build_with_ssu2_introducer_address")?;
    if builder.contains("FloodfillAdvertisementPermit") || builder.contains("build_floodfill") {
        return Err("the netdb's introducer builder must carry no floodfill permit".to_owned());
    }
    if !builder.contains("is_qualified_ssu2_introducer_address") {
        return Err(
            "the netdb's introducer builder must use the netdb's own introducer rule as its authority"
                .to_owned(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rule 13: the proto crate carries no introducer option vocabulary.
// ---------------------------------------------------------------------------

fn check_the_proto_crate_carries_no_introducer_vocabulary(proto: &str) -> Result<(), String> {
    let code = code_view(proto);
    for forbidden in ["ihost", "iport", "ikey", "itag", "iexp"] {
        if code.contains(forbidden) {
            return Err(format!(
                "the proto crate must carry no introducer option vocabulary, found {forbidden}"
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The rules against the real tree.
// ---------------------------------------------------------------------------

#[test]
fn rule_01_the_ssu1_introducer_options_are_absent() {
    check_the_ssu1_introducer_options_are_absent(&source_of(ADDRESS_MODULE)).expect("rule 1");
}

#[test]
fn rule_02_the_introducer_dispatch_names_the_specification_options() {
    check_the_introducer_dispatch_names_the_specification_options(&source_of(ADDRESS_MODULE))
        .expect("rule 2");
}

#[test]
fn rule_03_the_introducer_value_carries_the_specification_tuple() {
    check_the_introducer_value_carries_the_specification_tuple(&source_of(ADDRESS_MODULE))
        .expect("rule 3");
}

#[test]
fn rule_04_the_parser_refuses_an_endpoint_with_introducers() {
    check_the_parser_refuses_an_endpoint_with_introducers(&source_of(ADDRESS_MODULE))
        .expect("rule 4");
}

#[test]
fn rule_05_the_direct_with_introducers_class_is_gone() {
    check_the_direct_with_introducers_class_is_gone(&source_of(ADDRESS_MODULE)).expect("rule 5");
}

#[test]
fn rule_06_the_publication_builder_emits_the_specification_options() {
    check_the_publication_builder_emits_the_specification_options(&source_of(PUBLICATION_MODULE))
        .expect("rule 6");
}

#[test]
fn rule_07_the_introducer_table_maps_the_hash_and_expiry() {
    check_the_introducer_table_maps_the_hash_and_expiry(&source_of(INTRODUCER_MODULE))
        .expect("rule 7");
}

#[test]
fn rule_08_the_handshake_marker_names_the_specification_options() {
    check_the_handshake_marker_names_the_specification_options(&source_of(HANDSHAKE_MODULE))
        .expect("rule 8");
}

#[test]
fn rule_09_the_netdb_direct_rule_is_unchanged() {
    check_the_netdb_direct_rule_is_unchanged(&source_of(ROLE_MODULE), &source_of(LOCAL_MODULE))
        .expect("rule 9");
}

#[test]
fn rule_10_the_netdb_introducer_rule_forbids_an_endpoint() {
    check_the_netdb_introducer_rule_forbids_an_endpoint(&source_of(ROLE_MODULE)).expect("rule 10");
}

#[test]
fn rule_11_the_netdb_introducer_rule_requires_a_complete_group() {
    check_the_netdb_introducer_rule_requires_a_complete_group(&source_of(ROLE_MODULE))
        .expect("rule 11");
}

#[test]
fn rule_12_the_netdb_introducer_builder_carries_no_permit() {
    check_the_netdb_introducer_builder_carries_no_permit(&source_of(LOCAL_MODULE))
        .expect("rule 12");
}

#[test]
fn rule_13_the_proto_crate_carries_no_introducer_vocabulary() {
    check_the_proto_crate_carries_no_introducer_vocabulary(&sources_under(PROTO_ROOT))
        .expect("rule 13");
}

// ---------------------------------------------------------------------------
// The inverted probes: each mutation must be reported.
// ---------------------------------------------------------------------------

#[test]
fn probe_01_the_ssu1_introducer_options_are_absent_fires() {
    let mutated = source_of(ADDRESS_MODULE).replace(
        "const INTRODUCER_HASH_PREFIX: &str = \"ih\";",
        "const INTRODUCER_HASH_PREFIX: &str = \"ihost\";",
    );
    assert!(
        check_the_ssu1_introducer_options_are_absent(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_02_the_introducer_dispatch_names_the_specification_options_fires() {
    let mutated = source_of(ADDRESS_MODULE).replace("        INTRODUCER_EXPIRY_PREFIX,\n", "");
    assert!(
        check_the_introducer_dispatch_names_the_specification_options(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_03_the_introducer_value_carries_the_specification_tuple_fires() {
    let mutated = source_of(ADDRESS_MODULE).replace(
        "pub struct Ssu2Introducer {\n    peer_hash: [u8; 32],",
        "pub struct Ssu2Introducer {\n    endpoint: Ssu2Endpoint,\n    peer_hash: [u8; 32],",
    );
    assert!(
        check_the_introducer_value_carries_the_specification_tuple(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_04_the_parser_refuses_an_endpoint_with_introducers_fires() {
    let mutated = source_of(ADDRESS_MODULE).replace("HostPortWithIntroducers", "UnusedVariant");
    assert!(
        check_the_parser_refuses_an_endpoint_with_introducers(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_05_the_direct_with_introducers_class_is_gone_fires() {
    let mutated = source_of(ADDRESS_MODULE).replace(
        "    /// No direct endpoint; contact only via published introducers.",
        "    /// A direct endpoint that additionally publishes introducers.\n    DirectWithIntroducers,\n    /// No direct endpoint; contact only via published introducers.",
    );
    assert!(
        check_the_direct_with_introducers_class_is_gone(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_06_the_publication_builder_emits_the_specification_options_fires() {
    let mutated = source_of(PUBLICATION_MODULE)
        .replace("format!(\"ih{index}\")", "format!(\"ihost{index}\")");
    assert!(
        check_the_publication_builder_emits_the_specification_options(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_07_the_introducer_table_maps_the_hash_and_expiry_fires() {
    let mutated = source_of(INTRODUCER_MODULE).replace("record.peer_hash", "record.endpoint");
    assert!(
        check_the_introducer_table_maps_the_hash_and_expiry(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_08_the_handshake_marker_names_the_specification_options_fires() {
    let mutated = source_of(HANDSHAKE_MODULE).replace("get(\"ih0\")", "get(\"ihost0\")");
    assert!(
        check_the_handshake_marker_names_the_specification_options(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_09_the_netdb_direct_rule_is_unchanged_fires() {
    let mutated = source_of(ROLE_MODULE).replace(
        "\"host\" | \"port\" | \"mtu\" | \"v\" | \"caps\" | \"s\" | \"i\"",
        "\"host\" | \"port\" | \"mtu\" | \"v\" | \"caps\" | \"s\"",
    );
    assert!(
        check_the_netdb_direct_rule_is_unchanged(&mutated, &source_of(LOCAL_MODULE)).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_10_the_netdb_introducer_rule_forbids_an_endpoint_fires() {
    let mutated = source_of(ROLE_MODULE).replace(
        "    if options.get(\"host\").is_some() || options.get(\"port\").is_some() {\n        return false;\n    }\n",
        "",
    );
    assert!(
        check_the_netdb_introducer_rule_forbids_an_endpoint(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_11_the_netdb_introducer_rule_requires_a_complete_group_fires() {
    let mutated = source_of(ROLE_MODULE).replace(
        "hashes.keys().any(|index| tags.contains_key(index))",
        "true",
    );
    assert!(
        check_the_netdb_introducer_rule_requires_a_complete_group(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_12_the_netdb_introducer_builder_carries_no_permit_fires() {
    let mutated = source_of(LOCAL_MODULE).replace(
        "    pub fn build_with_ssu2_introducer_address(\n",
        "    pub fn build_with_ssu2_introducer_address(\n        _permit: &FloodfillAdvertisementPermit,\n",
    );
    assert!(
        check_the_netdb_introducer_builder_carries_no_permit(&mutated).is_err(),
        "the probe must be reported"
    );
}

#[test]
fn probe_13_the_proto_crate_carries_no_introducer_vocabulary_fires() {
    let mutated = format!(
        "{}\nconst IH0: &str = \"ihost0\";\n",
        sources_under(PROTO_ROOT)
    );
    assert!(
        check_the_proto_crate_carries_no_introducer_vocabulary(&mutated).is_err(),
        "the probe must be reported"
    );
}

// ---------------------------------------------------------------------------
// The guard's own property: an absent named item is a failure, not a pass.
// ---------------------------------------------------------------------------

#[test]
fn the_guard_reports_a_missing_named_item() {
    assert!(
        required_item_body("fn unrelated() {}", "fn parse_introducer_index(").is_err(),
        "an absent named item must be reported"
    );
    assert!(
        check_the_introducer_dispatch_names_the_specification_options("fn unrelated() {}").is_err(),
        "a rule that names an absent item must fail closed"
    );
}
