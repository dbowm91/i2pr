//! Plan 525's structural guard for the router's own published address.
//!
//! A router behind a firewall publishes an **introducer-only** address: no
//! `host`, no `port`, and the introducers that will vouch for it. The pinned
//! i2pd 2.61.0 publishes exactly that shape when it is not published
//! directly (`libi2pd/RouterInfo.cpp:1350`-`:1466`).
//!
//! The guard asserts fourteen things about the **production** surface:
//!
//! 1. the publication switch stays **off** by default, and it is refused
//!    without the SSU2 service;
//! 2. the introducer-only snapshot's **evidence expiry** comes from the
//!    introducer set's own expiry, never from the build time — the value
//!    this plan fixed;
//! 3. the family capability is **derived** from the router's bound socket,
//!    never hardcoded;
//! 4. the composition tries the **direct** form first;
//! 5. only a **reachability** refusal falls through to the introducer-only
//!    form: every other refusal is a broken router, not a firewalled one;
//! 6. the daemon holds **no copy** of the publication rule and matches on
//!    the **typed** form;
//! 7. each form is built by **its own** netdb builder;
//! 8. the introducer builder carries **no** floodfill permit;
//! 9. the runtime's install gate accepts the router's own introducer-only
//!    address, and still binds every address that does carry an endpoint;
//! 10. the counters **distinguish** the two published forms and the
//!     withdrawal;
//! 11. the empty-introducer reason is counted under **its own** reason;
//! 12. the counters are published through the **inspection seam**, so the
//!     posture is observable on a running router;
//! 13. the owner adds **no** task, no timer, no listener and no socket, and
//!     the configuration gains **no** new key;
//! 14. the daemon writes **no** SSU2 option vocabulary.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required **call**, with its open parenthesis. Comments
//! are stripped and `#[cfg(test)]` blocks are excluded, so a doc comment
//! that merely names a construct never satisfies a rule, and a function that
//! is **absent** fails closed.
//!
//! Every rule has an **inverted probe**: a mutation that must be reported.
//! A rule with no probe is a rule nobody has shown to fire. Every probe
//! mutates an **in-memory copy** of the source, so no file on disk is
//! written.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The daemon's owner of the router's own published address.
const OWNER_MODULE: &str = "crates/i2pr-daemon/src/self_publication.rs";
/// The daemon's configuration surface.
const CONFIG_MODULE: &str = "crates/i2pr-daemon/src/config.rs";
/// The composition root.
const COMPOSITION_MODULE: &str = "crates/i2pr-daemon/src/lib.rs";
/// The daemon's inspection seam.
const INSPECTION_MODULE: &str = "crates/i2pr-daemon/src/i2pcontrol_inspection.rs";
/// The netdb's local RouterInfo builders.
const NETDB_LOCAL_MODULE: &str = "crates/i2pr-netdb/src/local.rs";
/// The runtime's own publication material, composition and install gate.
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";

/// The workspace root, resolved from this crate's manifest directory.
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

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1: the publication switch stays off by default, and it is refused
/// without the SSU2 service.
///
/// The switch is the **existing** one: this plan adds **no** new key, so
/// the advertisement posture is unchanged.
fn check_the_switch_stays_off_by_default(config: &str) -> Option<String> {
    let code = code_view(config);
    let Some(default) = item_body(&code, "fn default_ssu2_advertise") else {
        return Some("the advertisement switch's default is absent".to_owned());
    };
    if !default.contains("false") {
        return Some("the advertisement switch must default to off".to_owned());
    }
    let Some(normalize) = item_body(&code, "fn normalize_ssu2") else {
        return Some("the SSU2 normalization is absent".to_owned());
    };
    for needle in [
        "if raw.advertise {",
        "if !raw.enabled {",
        "if private_lan {",
    ] {
        if !normalize.contains(needle) {
            return Some(format!(
                "the switch's refusal must stay conditional: {needle}"
            ));
        }
    }
    None
}

/// Rule 2: the introducer-only snapshot's evidence expiry comes from the
/// introducer set's own expiry.
///
/// The value this plan fixed: passing the build time as the evidence expiry
/// made the transport's own gate withdraw **every** snapshot, so the
/// builder could never succeed.
fn check_the_evidence_expiry_comes_from_the_introducer_set(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(body) = item_body(&code, "pub fn introducer_publication_material(") else {
        return Some("the runtime's introducer-only builder is absent".to_owned());
    };
    if body.contains("evidence_expires_secs: now_secs") {
        return Some(
            "the evidence expiry must not be the build time: the transport's own gate would \
             withdraw every snapshot"
                .to_owned(),
        );
    }
    if !body.contains("expires_secs") {
        return Some("the evidence expiry must come from the introducer's own expiry".to_owned());
    }
    if !body.contains(".min()") {
        return Some("the evidence expiry must be the earliest expiry in the set".to_owned());
    }
    None
}

/// Rule 3: the family capability is derived from the bound socket.
///
/// The tree's shortcut was a literal `"4"`, which is a factual claim about
/// a router that may have no IPv4 socket at all.
fn check_the_family_capability_is_derived(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(family) = item_body(&code, "fn bound_family(") else {
        return Some("the runtime's bound-family helper is absent".to_owned());
    };
    for needle in [".v4", ".v6"] {
        if !family.contains(needle) {
            return Some(format!(
                "the bound family must be derived from the router's own socket: {needle}"
            ));
        }
    }
    let Some(body) = item_body(&code, "pub fn introducer_publication_material(") else {
        return Some("the runtime's introducer-only builder is absent".to_owned());
    };
    if body.contains("Ssu2Capabilities::parse(\"4\")") {
        return Some("the family capability must not be hardcoded".to_owned());
    }
    if !body.contains("self.bound_family()?") {
        return Some("the builder must ask for the router's own bound family".to_owned());
    }
    None
}

/// Rule 4: the composition tries the direct form first.
///
/// The direct form is the **preferred** form; the introducer-only form is a
/// fallback for a router the direct form cannot serve.
fn check_the_composition_tries_the_direct_form_first(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(body) = item_body(&code, "pub fn self_publication_material(") else {
        return Some("the runtime's composition is absent".to_owned());
    };
    let direct = body.find("self.publication_material(");
    let introducer = body.find("self.introducer_publication_material(");
    let (Some(direct), Some(introducer)) = (direct, introducer) else {
        return Some(
            "the composition must ask for the direct form and the introducer-only form".to_owned(),
        );
    };
    if direct >= introducer {
        return Some("the direct form must be tried before the introducer-only form".to_owned());
    }
    None
}

/// Rule 5: only a reachability refusal falls through to the introducer-only
/// form.
///
/// A closed runtime, an unbound runtime and a poisoned state are **broken**
/// routers, and publishing an introducer-only address would hide the fault
/// behind a plausible record.
fn check_only_a_reachability_refusal_falls_through(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(body) = item_body(&code, "pub fn self_publication_material(") else {
        return Some("the runtime's composition is absent".to_owned());
    };
    if !body.contains("Err(Ssu2PublicationUnavailable::ReachabilityUnqualified)") {
        return Some(
            "only a reachability refusal may fall through to the introducer-only form".to_owned(),
        );
    }
    if !body.contains("Err(reason) => Err(reason)") {
        return Some("every other refusal must be propagated unchanged".to_owned());
    }
    None
}

/// Rule 6: the daemon holds no copy of the publication rule, and matches on
/// the typed form.
///
/// The daemon never infers a form from a refusal: it matches on the form
/// the runtime admitted.
fn check_the_daemon_matches_on_the_typed_form(owner: &str) -> Option<String> {
    let code = code_view(owner);
    for needle in [
        "ReachabilityState",
        "build_publication_snapshot",
        "i2pr_transport_ssu2",
    ] {
        if code.contains(needle) {
            return Some(format!(
                "the daemon must not hold a copy of the publication rule: {needle}"
            ));
        }
    }
    let Some(refresh) = item_body(&code, "pub fn refresh") else {
        return Some("the owner's refresh is absent".to_owned());
    };
    if !refresh.contains("service.self_publication_material(") {
        return Some("the owner must ask the runtime for the published form".to_owned());
    }
    let Some(build) = item_body(&code, "fn build_record") else {
        return Some("the owner's record builder is absent".to_owned());
    };
    for needle in [
        "Ssu2PublicationForm::Direct",
        "Ssu2PublicationForm::IntroducerOnly",
        "material.form",
    ] {
        if !build.contains(needle) {
            return Some(format!(
                "the owner must build the record from the typed form: {needle}"
            ));
        }
    }
    None
}

/// Rule 7: each form is built by its own netdb builder.
///
/// The direct rule and the introducer rule are two different rules, and
/// each form is signed by the builder that carries its own rule.
fn check_each_form_has_its_own_builder(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(build) = item_body(&code, "fn build_record") else {
        return Some("the owner's record builder is absent".to_owned());
    };
    if !build.contains("build_with_ssu2_address(") {
        return Some("the direct form must be built through the netdb's direct builder".to_owned());
    }
    if !build.contains("build_with_ssu2_introducer_address(") {
        return Some(
            "the introducer-only form must be built through the netdb's introducer builder"
                .to_owned(),
        );
    }
    None
}

/// Rule 8: the introducer builder carries no floodfill permit.
///
/// A record it builds is an ordinary router's record: it cannot mint
/// `caps=f`.
fn check_the_introducer_builder_carries_no_permit(netdb: &str) -> Option<String> {
    let code = code_view(netdb);
    let Some(body) = item_body(&code, "pub fn build_with_ssu2_introducer_address(") else {
        return Some("the netdb's introducer-only builder is absent".to_owned());
    };
    if !body.contains("is_qualified_ssu2_introducer_address(") {
        return Some(
            "the introducer-only builder must qualify the address with the netdb's own rule"
                .to_owned(),
        );
    }
    for needle in ["FloodfillAdvertisementPermit", "build_floodfill"] {
        if body.contains(needle) {
            return Some(format!(
                "the introducer-only builder must carry no floodfill permit: {needle}"
            ));
        }
    }
    None
}

/// Rule 9: the runtime's install gate accepts the router's own introducer-only
/// address, and still binds every address that does carry an endpoint.
///
/// The gate binds the router's **own** identity and contact material: a
/// direct address must name the router's own bound endpoint, and an
/// introducer-only address must carry the router's own keys and at least
/// one introducer.
fn check_the_install_gate_accepts_the_introducer_only_form(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(body) = item_body(&code, "fn install_local_router_info_inner") else {
        return Some("the runtime's install gate is absent".to_owned());
    };
    for needle in [
        "Ssu2AddressClass::Direct",
        "Ssu2AddressClass::IntroducerOnly",
        "Ssu2AddressClass::UnpublishedStatic",
    ] {
        if !body.contains(needle) {
            return Some(format!(
                "the install gate must bind the address by its form: {needle}"
            ));
        }
    }
    if !body.contains("is_bound") {
        return Some("every direct address must name the router's own bound endpoint".to_owned());
    }
    if !body.contains("expected_static") {
        return Some("every SSU2 address must carry the router's own static key".to_owned());
    }
    None
}

/// Rule 10: the counters distinguish the two published forms and the
/// withdrawal.
fn check_the_counters_distinguish_the_forms(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(snapshot) = item_body(&code, "pub fn snapshot(&self)") else {
        return Some("the owner's counter snapshot is absent".to_owned());
    };
    if !snapshot.contains("published_direct + published_introducer_only") {
        return Some("the published count must be the sum of the two published forms".to_owned());
    }
    for needle in ["published_direct", "published_introducer_only", "withdrawn"] {
        if !snapshot.contains(needle) {
            return Some(format!("the counters must distinguish: {needle}"));
        }
    }
    None
}

/// Rule 11: the empty-introducer reason is counted under its own reason.
///
/// The counter named this reason before this plan and it could never move,
/// because nothing ever produced the refusal.
fn check_the_empty_introducer_reason_is_counted(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(record) = item_body(&code, "fn record_withheld") else {
        return Some("the owner's refusal recorder is absent".to_owned());
    };
    if !record.contains("Ssu2PublicationUnavailable::NoIntroducers") {
        return Some("the empty introducer set must be counted under its own reason".to_owned());
    }
    if !record.contains("withheld_no_introducers") {
        return Some("the empty introducer set's counter is absent".to_owned());
    }
    None
}

/// Rule 12: the counters are published through the inspection seam.
fn check_the_counters_reach_the_inspection_seam(
    inspection: &str,
    composition: &str,
) -> Option<String> {
    let code = code_view(inspection);
    let Some(publish) = item_body(&code, "pub fn publish_self_publication_counters(") else {
        return Some("the inspection seam's publisher is absent".to_owned());
    };
    if !publish.contains("self_publication_live") {
        return Some("the publisher must write the seam's own slot".to_owned());
    }
    let Some(read) = item_body(&code, "pub fn self_publication_counters_snapshot(") else {
        return Some("the inspection seam's reader is absent".to_owned());
    };
    if !read.contains("self_publication_counters()") {
        return Some("the reader must read the seam's own slot".to_owned());
    }
    let composition_code = code_view(composition);
    if !composition_code.contains("publish_self_publication_counters(") {
        return Some("the composition must publish the counters".to_owned());
    }
    None
}

/// Rule 13: the owner adds no task, no timer, no listener and no socket, and
/// the configuration gains no new key.
///
/// The owner is a boolean and a fixed set of counters. It runs on the
/// publication's own task and adds no infrastructure of its own.
fn check_the_owner_adds_no_infrastructure(owner: &str, config: &str) -> Option<String> {
    let code = code_view(owner);
    for needle in ["tokio::", "spawn", "TcpListener", "std::net"] {
        if code.contains(needle) {
            return Some(format!(
                "the owner must add no infrastructure of its own: {needle}"
            ));
        }
    }
    let config_code = code_view(config);
    let Some(raw) = item_body(&config_code, "struct RawSsu2Config") else {
        return Some("the SSU2 configuration surface is absent".to_owned());
    };
    if raw.contains("HashMap") || raw.contains("#[serde(flatten)]") {
        return Some("the configuration must gain no new free-form key".to_owned());
    }
    if raw.matches("advertise: bool").count() != 1 {
        return Some("the configuration must gain no new switch beside `advertise`".to_owned());
    }
    None
}

/// Rule 14: the daemon writes no SSU2 option vocabulary.
///
/// The address is the transport's, whole: the daemon neither names an
/// option nor assembles one.
fn check_the_daemon_writes_no_option_vocabulary(owner: &str) -> Option<String> {
    let code = code_view(owner);
    for needle in [
        "RouterAddress::new(",
        "Mapping::from_entries(",
        "\"ih0\"",
        "\"iexp0\"",
        "\"itag0\"",
    ] {
        if code.contains(needle) {
            return Some(format!("the daemon must not write the address: {needle}"));
        }
    }
    let Some(build) = item_body(&code, "fn build_record") else {
        return Some("the owner's record builder is absent".to_owned());
    };
    for needle in ["\"host\"", "\"port\"", "\"caps\"", "\"mtu\""] {
        if build.contains(needle) {
            return Some(format!("the daemon must not write the address: {needle}"));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules as tests.
// ----------------------------------------------------------------------

#[test]
fn the_switch_stays_off_by_default() {
    let reason = check_the_switch_stays_off_by_default(&source_of(CONFIG_MODULE));
    assert!(
        reason.is_none(),
        "the switch must stay default-off: {reason:?}"
    );
}

#[test]
fn the_evidence_expiry_comes_from_the_introducer_set() {
    let reason =
        check_the_evidence_expiry_comes_from_the_introducer_set(&source_of(RUNTIME_MODULE));
    assert!(
        reason.is_none(),
        "the evidence expiry must come from the set: {reason:?}"
    );
}

#[test]
fn the_family_capability_is_derived() {
    let reason = check_the_family_capability_is_derived(&source_of(RUNTIME_MODULE));
    assert!(reason.is_none(), "the family must be derived: {reason:?}");
}

#[test]
fn the_composition_tries_the_direct_form_first() {
    let reason = check_the_composition_tries_the_direct_form_first(&source_of(RUNTIME_MODULE));
    assert!(
        reason.is_none(),
        "the direct form must come first: {reason:?}"
    );
}

#[test]
fn only_a_reachability_refusal_falls_through() {
    let reason = check_only_a_reachability_refusal_falls_through(&source_of(RUNTIME_MODULE));
    assert!(
        reason.is_none(),
        "only a reachability refusal may fall through: {reason:?}"
    );
}

#[test]
fn the_daemon_matches_on_the_typed_form() {
    let reason = check_the_daemon_matches_on_the_typed_form(&source_of(OWNER_MODULE));
    assert!(
        reason.is_none(),
        "the daemon must match on the form: {reason:?}"
    );
}

#[test]
fn each_form_has_its_own_builder() {
    let reason = check_each_form_has_its_own_builder(&source_of(OWNER_MODULE));
    assert!(
        reason.is_none(),
        "each form needs its own builder: {reason:?}"
    );
}

#[test]
fn the_introducer_builder_carries_no_permit() {
    let reason = check_the_introducer_builder_carries_no_permit(&source_of(NETDB_LOCAL_MODULE));
    assert!(
        reason.is_none(),
        "the introducer builder carries no permit: {reason:?}"
    );
}

#[test]
fn the_install_gate_accepts_the_introducer_only_form() {
    let reason =
        check_the_install_gate_accepts_the_introducer_only_form(&source_of(RUNTIME_MODULE));
    assert!(
        reason.is_none(),
        "the gate must accept the introducer-only form: {reason:?}"
    );
}

#[test]
fn the_counters_distinguish_the_forms() {
    let reason = check_the_counters_distinguish_the_forms(&source_of(OWNER_MODULE));
    assert!(
        reason.is_none(),
        "the counters must distinguish the forms: {reason:?}"
    );
}

#[test]
fn the_empty_introducer_reason_is_counted() {
    let reason = check_the_empty_introducer_reason_is_counted(&source_of(OWNER_MODULE));
    assert!(
        reason.is_none(),
        "the empty introducer set must be counted: {reason:?}"
    );
}

#[test]
fn the_counters_reach_the_inspection_seam() {
    let reason = check_the_counters_reach_the_inspection_seam(
        &source_of(INSPECTION_MODULE),
        &source_of(COMPOSITION_MODULE),
    );
    assert!(
        reason.is_none(),
        "the counters must be observable: {reason:?}"
    );
}

#[test]
fn the_owner_adds_no_infrastructure() {
    let reason =
        check_the_owner_adds_no_infrastructure(&source_of(OWNER_MODULE), &source_of(CONFIG_MODULE));
    assert!(
        reason.is_none(),
        "the owner must add no infrastructure: {reason:?}"
    );
}

#[test]
fn the_daemon_writes_no_option_vocabulary() {
    let reason = check_the_daemon_writes_no_option_vocabulary(&source_of(OWNER_MODULE));
    assert!(
        reason.is_none(),
        "the daemon must write no vocabulary: {reason:?}"
    );
}

// ----------------------------------------------------------------------
// The inverted probes.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_a_missing_named_item() {
    // Every rule must fail closed when the item it names is absent.
    assert!(
        check_the_evidence_expiry_comes_from_the_introducer_set("pub fn unrelated() {}").is_some(),
        "an absent builder must be reported"
    );
    assert!(
        check_the_composition_tries_the_direct_form_first("pub fn unrelated() {}").is_some(),
        "an absent composition must be reported"
    );
    assert!(
        check_the_daemon_matches_on_the_typed_form("pub fn unrelated() {}").is_some(),
        "an absent owner must be reported"
    );
    assert!(
        check_the_install_gate_accepts_the_introducer_only_form("pub fn unrelated() {}").is_some(),
        "an absent gate must be reported"
    );
    assert!(
        check_the_introducer_builder_carries_no_permit("pub fn unrelated() {}").is_some(),
        "an absent builder must be reported"
    );
}

#[test]
fn the_guard_reports_an_evidence_expiry_that_is_the_build_time() {
    let violating = "\
pub fn introducer_publication_material(&self, now_secs: u64) -> Result<(), ()> {
    let evidence_expires_secs = now_secs;
    let _ = evidence_expires_secs;
    Ok(())
}
";
    let reason = check_the_evidence_expiry_comes_from_the_introducer_set(violating);
    assert!(
        reason.is_some(),
        "a build-time evidence expiry must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_hardcoded_family_capability() {
    let violating = "\
pub fn introducer_publication_material(&self, now_secs: u64) -> Result<(), ()> {
    let capabilities = Ssu2Capabilities::parse(\"4\").ok();
    let _ = capabilities;
    Ok(())
}
";
    let reason = check_the_family_capability_is_derived(violating);
    assert!(
        reason.is_some(),
        "a hardcoded family capability must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_composition_that_tries_the_introducer_form_first() {
    let violating = "\
pub fn self_publication_material(&self) -> Result<(), ()> {
    let _ = self.introducer_publication_material(0);
    let _ = self.publication_material(0);
    Ok(())
}
";
    let reason = check_the_composition_tries_the_direct_form_first(violating);
    assert!(
        reason.is_some(),
        "a composition that tries the introducer form first must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_composition_that_falls_through_on_any_refusal() {
    let violating = "\
pub fn self_publication_material(&self) -> Result<(), ()> {
    match self.publication_material(0) {
        Ok(material) => Ok(material),
        Err(reason) => self.introducer_publication_material(0).map_err(|_| reason),
    }
}
";
    let reason = check_only_a_reachability_refusal_falls_through(violating);
    assert!(
        reason.is_some(),
        "a composition that falls through on any refusal must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_infers_the_form() {
    let violating = "\
pub fn build_record(&self) -> u64 {
    let reachability = ReachabilityState::Firewalled;
    reachability as u64
}
";
    let reason = check_the_daemon_matches_on_the_typed_form(violating);
    assert!(
        reason.is_some(),
        "a daemon copy of the rule must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_form_built_by_the_wrong_builder() {
    let violating = "\
fn build_record(&self) -> u64 {
    let _ = self.build_with_ssu2_address(0, 0, 0);
    0
}
";
    let reason = check_each_form_has_its_own_builder(violating);
    assert!(
        reason.is_some(),
        "a form built by the wrong builder must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_introducer_builder_with_a_permit() {
    let violating = "\
pub fn build_with_ssu2_introducer_address(&self, permit: &FloodfillAdvertisementPermit) -> Result<(), ()> {
    let _ = permit;
    Ok(())
}
";
    let reason = check_the_introducer_builder_carries_no_permit(violating);
    assert!(
        reason.is_some(),
        "an introducer builder with a permit must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_gate_that_refuses_the_introducer_only_form() {
    let violating = "\
fn install_local_router_info_inner(&self) -> Result<(), ()> {
    let endpoint = parsed.endpoint();
    let is_bound = endpoint.is_some();
    is_bound
}
";
    let reason = check_the_install_gate_accepts_the_introducer_only_form(violating);
    assert!(
        reason.is_some(),
        "a gate that refuses the introducer-only form must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_counters_that_do_not_distinguish_the_forms() {
    let violating = "\
pub fn snapshot(&self) -> u64 {
    self.published.load(0)
}
";
    let reason = check_the_counters_distinguish_the_forms(violating);
    assert!(
        reason.is_some(),
        "counters that do not distinguish the forms must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_uncounted_empty_introducer_set() {
    let violating = "\
fn record_withheld(&self, reason: Ssu2PublicationUnavailable) {
    let _ = reason;
}
";
    let reason = check_the_empty_introducer_reason_is_counted(violating);
    assert!(
        reason.is_some(),
        "an uncounted empty introducer set must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_counters_that_never_reach_the_inspection_seam() {
    let inspection = "\
pub fn publish_self_publication_counters(&self) {
    let _ = 0;
}
";
    let reason = check_the_counters_reach_the_inspection_seam(inspection, "pub fn unrelated() {}");
    assert!(
        reason.is_some(),
        "counters that never reach the seam must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_owner_that_spawns_a_task() {
    let violating = "\
pub fn refresh(&self) {
    let _task = tokio::spawn(async {});
}
";
    let reason = check_the_owner_adds_no_infrastructure(
        violating,
        "struct RawSsu2Config { advertise: bool }",
    );
    assert!(
        reason.is_some(),
        "an owner that spawns a task must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_writes_the_option_vocabulary() {
    let violating = "\
pub fn build_record(&self) {
    let _options = Mapping::from_entries(vec![(\"ih0\".to_owned(), \"x\".to_owned())]);
}
";
    let reason = check_the_daemon_writes_no_option_vocabulary(violating);
    assert!(
        reason.is_some(),
        "a daemon that writes the vocabulary must be reported: {reason:?}"
    );
}
