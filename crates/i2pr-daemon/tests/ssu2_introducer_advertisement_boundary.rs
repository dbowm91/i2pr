//! Plan 526's structural guard for the router's own introducer-role
//! advertisement.
//!
//! The specification's Relay capability is advertised with a `"C"` in the
//! address's `caps` option, and the pinned i2pd 2.61.0 publishes that letter
//! on the **published** form only (`libi2pd/RouterInfo.cpp:1350`-`:1373`),
//! because its peer-side check demands an endpoint for an introducer
//! (`libi2pd/RouterInfo.cpp:1059`).
//!
//! The guard asserts twelve things about the **production** surface:
//!
//! 1. the introducer role's switch stays **off** by default, and it is
//!    refused without the SSU2 service;
//! 2. the composition no longer refuses the role, and the role owner is
//!    constructed **only** behind the explicit opt-in;
//! 3. the runtime composes the role letter **only** while the role is live;
//! 4. the **direct** form is the only form that carries the role letter;
//! 5. the netdb's **third** rule is exact, and the **first** two rules are
//!    untouched;
//! 6. the third builder carries **no** floodfill permit;
//! 7. the runtime's install gate **refuses** a role-bearing record while the
//!    role is not live;
//! 8. the daemon matches on a **typed** form and each of the three forms has
//!    **its own** builder;
//! 9. the counters distinguish the three published forms;
//! 10. the third rule's refusal is counted under **its own** reason;
//! 11. the owner adds **no** task, no timer, no listener and no socket, and
//!     the configuration gains **no** new key;
//! 12. the daemon writes **no** SSU2 option vocabulary.
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
/// The netdb's local RouterInfo builders.
const NETDB_LOCAL_MODULE: &str = "crates/i2pr-netdb/src/local.rs";
/// The netdb's address qualification rules.
const NETDB_ROLE_MODULE: &str = "crates/i2pr-netdb/src/floodfill_role.rs";
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

/// Extracts the item's **whole** text: its signature and its body.
///
/// A rule that is about what an item *takes* -- a permit, an argument --
/// must read the signature too, which the body alone does not carry.
fn item_text(code: &str, signature: &str) -> Option<String> {
    let start = find_definition(code, signature)?;
    let (_, close) = brace_balanced_block(code, start + signature.len())?;
    Some(code[start..close].to_owned())
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1: the introducer role's switch stays off by default, and it is
/// refused without the SSU2 service.
fn check_the_switch_stays_off_by_default(config: &str) -> Option<String> {
    let code = code_view(config);
    let Some(default) = item_body(&code, "fn default_ssu2_introducer_service") else {
        return Some("the introducer role's default is absent".to_owned());
    };
    if !default.contains("false") {
        return Some("the introducer role's switch must default to off".to_owned());
    }
    let Some(normalize) = item_body(&code, "fn normalize_ssu2") else {
        return Some("the SSU2 normalization is absent".to_owned());
    };
    if !normalize.contains("if raw.introducer_service && !raw.enabled {") {
        return Some("the role's opt-in must stay refused without the SSU2 service".to_owned());
    }
    None
}

/// Rule 2: the composition no longer refuses the role, and the role owner is
/// constructed **only** behind the explicit opt-in.
fn check_the_role_is_reachable_behind_its_opt_in(composition: &str) -> Option<String> {
    let code = code_view(composition);
    if code.contains("SSU2 service requires the strict controlled profile") {
        return Some("the composition must not refuse the introducer role outright".to_owned());
    }
    let Some(body) = item_body(
        &code,
        "let mut introducer = if ssu2_config.introducer_service",
    ) else {
        return Some("the composition's role construction is absent".to_owned());
    };
    if !body.contains("Ssu2IntroducerService::new(") {
        return Some("the role owner must be constructed behind the opt-in".to_owned());
    }
    if !body.contains("enabled: true") {
        return Some("the constructed role owner must be enabled".to_owned());
    }
    None
}

/// Rule 3: the runtime composes the role letter **only** while the role is
/// live.
fn check_the_role_letter_is_composed_only_while_the_role_is_live(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(body) = item_body(&code, "fn capabilities_for(") else {
        return Some("the runtime's capability composition is absent".to_owned());
    };
    if !body.contains("self.introducer_service_enabled()") {
        return Some("the role letter must be gated on the role's live state".to_owned());
    }
    if !body.contains("format!(\"{family_letter}C\")") {
        return Some(
            "the role letter must be the family letter followed by the role letter".to_owned(),
        );
    }
    None
}

/// Rule 4: the **direct** form is the only form that carries the role letter.
fn check_only_the_direct_form_carries_the_role_letter(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(direct) = item_body(&code, "pub fn publication_material(") else {
        return Some("the runtime's direct builder is absent".to_owned());
    };
    if !direct.contains("self.capabilities_for(family)") {
        return Some(
            "the direct form must build its capability through the composition".to_owned(),
        );
    }
    let Some(introducer) = item_body(&code, "pub fn introducer_publication_material(") else {
        return Some("the runtime's introducer-only builder is absent".to_owned());
    };
    if introducer.contains("advertises_introducer_role: true") {
        return Some("the introducer-only form must never advertise the role".to_owned());
    }
    if !introducer.contains("advertises_introducer_role: false") {
        return Some("the introducer-only form must say the role is not advertised".to_owned());
    }
    if introducer.contains("capabilities_for(") {
        return Some(
            "the introducer-only form must not use the role-bearing composition".to_owned(),
        );
    }
    None
}

/// Rule 5: the netdb's **third** rule is exact, and the **first** two rules
/// are untouched.
fn check_the_third_rule_is_exact(netdb_role: &str) -> Option<String> {
    let code = code_view(netdb_role);
    let Some(third) = item_body(&code, "pub fn is_qualified_ssu2_relay_role_address") else {
        return Some("the netdb's third qualification rule is absent".to_owned());
    };
    for needle in ["caps != \"4C\"", "caps != \"6C\""] {
        if !third.contains(needle) {
            return Some(format!("the third rule must be exact: {needle}"));
        }
    }
    let Some(first) = item_body(&code, "pub fn is_qualified_ssu2_address") else {
        return Some("the netdb's direct rule is absent".to_owned());
    };
    if first.contains("4C") {
        return Some("the direct rule must stay untouched".to_owned());
    }
    let Some(second) = item_body(&code, "pub fn is_qualified_ssu2_introducer_address") else {
        return Some("the netdb's introducer rule is absent".to_owned());
    };
    if second.contains("4C") {
        return Some("the introducer rule must stay untouched".to_owned());
    }
    None
}

/// Rule 6: the third builder carries **no** floodfill permit.
fn check_the_third_builder_carries_no_permit(netdb_local: &str) -> Option<String> {
    let code = code_view(netdb_local);
    let Some(text) = item_text(&code, "pub fn build_with_ssu2_relay_role_address") else {
        return Some("the netdb's third builder is absent".to_owned());
    };
    if !text.contains("is_qualified_ssu2_relay_role_address(&address)") {
        return Some("the third builder must consult the third rule".to_owned());
    }
    for forbidden in ["FloodfillAdvertisementPermit", "build_floodfill"] {
        if text.contains(forbidden) {
            return Some(format!(
                "the third builder must carry no floodfill permit: {forbidden}"
            ));
        }
    }
    None
}

/// Rule 7: the runtime's install gate **refuses** a role-bearing record
/// while the role is not live.
fn check_the_install_gate_refuses_a_stale_role_letter(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(body) = item_body(&code, "fn install_local_router_info_inner(") else {
        return Some("the runtime's install gate is absent".to_owned());
    };
    if !body.contains("parsed.capabilities().supports_relay()") {
        return Some("the install gate must read the address's advertised role".to_owned());
    }
    if !body.contains("!self.introducer_service_enabled()") {
        return Some(
            "the install gate must refuse a role-bearing record while the role is not live"
                .to_owned(),
        );
    }
    None
}

/// Rule 8: the daemon matches on a **typed** form and each of the three
/// forms has **its own** builder.
fn check_each_form_has_its_own_builder(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(typed) = item_body(&code, "fn published_form") else {
        return Some("the daemon's typed published form is absent".to_owned());
    };
    if !typed.contains("advertises_introducer_role") {
        return Some("the typed form must read the material's role claim".to_owned());
    }
    let Some(build) = item_body(&code, "fn build_record(") else {
        return Some("the daemon's record builder is absent".to_owned());
    };
    for needle in [
        "build_with_ssu2_address(",
        "build_with_ssu2_relay_role_address(",
        "build_with_ssu2_introducer_address(",
    ] {
        if !build.contains(needle) {
            return Some(format!(
                "each form must be built by its own builder: {needle}"
            ));
        }
    }
    None
}

/// Rule 9: the counters distinguish the three published forms.
fn check_the_counters_distinguish_the_forms(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(counters) = item_body(&code, "pub struct SelfPublicationCounters") else {
        return Some("the self-publication counters are absent".to_owned());
    };
    for needle in [
        "published_direct: AtomicU64",
        "published_direct_relay_role: AtomicU64",
        "published_introducer_only: AtomicU64",
    ] {
        if !counters.contains(needle) {
            return Some(format!("the counters must distinguish the forms: {needle}"));
        }
    }
    let Some(snapshot) = item_body(&code, "pub fn snapshot(") else {
        return Some("the counter snapshot is absent".to_owned());
    };
    if !snapshot
        .contains("published_direct + published_direct_relay_role + published_introducer_only")
    {
        return Some("the aggregate must be the sum of the three published forms".to_owned());
    }
    None
}

/// Rule 10: the third rule's refusal is counted under **its own** reason.
fn check_the_third_rule_has_its_own_reason(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(reasons) = item_body(&code, "pub enum SelfPublicationFailure") else {
        return Some("the self-publication failure reasons are absent".to_owned());
    };
    if !reasons.contains("UnqualifiedRelayRoleAddress") {
        return Some("the third rule's refusal must have its own reason".to_owned());
    }
    let Some(mapping) = item_body(&code, "fn from_netdb") else {
        return Some("the netdb refusal mapping is absent".to_owned());
    };
    if !mapping.contains("PublishedForm::DirectRelayRole") {
        return Some("the mapping must name the role-bearing form".to_owned());
    }
    None
}

/// Rule 11: the owner adds **no** infrastructure, and the configuration gains
/// **no** new key.
fn check_the_owner_adds_no_infrastructure(owner: &str, config: &str) -> Option<String> {
    let code = code_view(owner);
    for forbidden in [
        "tokio::spawn",
        "std::thread::spawn",
        "TcpListener",
        "tokio::time::interval",
        "tokio::net::",
    ] {
        if code.contains(forbidden) {
            return Some(format!("the owner must add no infrastructure: {forbidden}"));
        }
    }
    let config_code = code_view(config);
    for forbidden in ["ssu2.advertise_role", "ssu2.introducer_advertise"] {
        if config_code.contains(forbidden) {
            return Some(format!(
                "the configuration must gain no new key: {forbidden}"
            ));
        }
    }
    None
}

/// Rule 12: the daemon writes **no** SSU2 option vocabulary.
fn check_the_daemon_writes_no_option_vocabulary(owner: &str) -> Option<String> {
    let code = code_view(owner);
    for forbidden in ["\"iexp\"", "\"ih\"", "\"itag\"", "\"pq\""] {
        if code.contains(forbidden) {
            return Some(format!(
                "the daemon must write no option vocabulary: {forbidden}"
            ));
        }
    }
    None
}

// ----------------------------------------------------------------------
// The rules, asserted against the real tree.
// ----------------------------------------------------------------------

#[test]
fn the_switch_stays_off_by_default() {
    assert_eq!(
        check_the_switch_stays_off_by_default(&source_of(CONFIG_MODULE)),
        None
    );
}

#[test]
fn the_role_is_reachable_behind_its_opt_in() {
    assert_eq!(
        check_the_role_is_reachable_behind_its_opt_in(&source_of(COMPOSITION_MODULE)),
        None
    );
}

#[test]
fn the_role_letter_is_composed_only_while_the_role_is_live() {
    assert_eq!(
        check_the_role_letter_is_composed_only_while_the_role_is_live(&source_of(RUNTIME_MODULE)),
        None
    );
}

#[test]
fn only_the_direct_form_carries_the_role_letter() {
    assert_eq!(
        check_only_the_direct_form_carries_the_role_letter(&source_of(RUNTIME_MODULE)),
        None
    );
}

#[test]
fn the_third_rule_is_exact() {
    assert_eq!(
        check_the_third_rule_is_exact(&source_of(NETDB_ROLE_MODULE)),
        None
    );
}

#[test]
fn the_third_builder_carries_no_permit() {
    assert_eq!(
        check_the_third_builder_carries_no_permit(&source_of(NETDB_LOCAL_MODULE)),
        None
    );
}

#[test]
fn the_install_gate_refuses_a_stale_role_letter() {
    assert_eq!(
        check_the_install_gate_refuses_a_stale_role_letter(&source_of(RUNTIME_MODULE)),
        None
    );
}

#[test]
fn each_form_has_its_own_builder() {
    assert_eq!(
        check_each_form_has_its_own_builder(&source_of(OWNER_MODULE)),
        None
    );
}

#[test]
fn the_counters_distinguish_the_forms() {
    assert_eq!(
        check_the_counters_distinguish_the_forms(&source_of(OWNER_MODULE)),
        None
    );
}

#[test]
fn the_third_rule_has_its_own_reason() {
    assert_eq!(
        check_the_third_rule_has_its_own_reason(&source_of(OWNER_MODULE)),
        None
    );
}

#[test]
fn the_owner_adds_no_infrastructure() {
    assert_eq!(
        check_the_owner_adds_no_infrastructure(&source_of(OWNER_MODULE), &source_of(CONFIG_MODULE)),
        None
    );
}

#[test]
fn the_daemon_writes_no_option_vocabulary() {
    assert_eq!(
        check_the_daemon_writes_no_option_vocabulary(&source_of(OWNER_MODULE)),
        None
    );
}

// ----------------------------------------------------------------------
// The negative controls.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_a_missing_named_item() {
    // Every rule must fail closed when the item it names is absent.
    assert!(
        check_the_switch_stays_off_by_default("pub fn unrelated() {}").is_some(),
        "an absent default must be reported"
    );
    assert!(
        check_the_role_is_reachable_behind_its_opt_in("pub fn unrelated() {}").is_some(),
        "an absent composition must be reported"
    );
    assert!(
        check_the_role_letter_is_composed_only_while_the_role_is_live("pub fn unrelated() {}")
            .is_some(),
        "an absent composition must be reported"
    );
    assert!(
        check_only_the_direct_form_carries_the_role_letter("pub fn unrelated() {}").is_some(),
        "an absent builder must be reported"
    );
    assert!(
        check_the_third_rule_is_exact("pub fn unrelated() {}").is_some(),
        "an absent rule must be reported"
    );
    assert!(
        check_the_third_builder_carries_no_permit("pub fn unrelated() {}").is_some(),
        "an absent builder must be reported"
    );
    assert!(
        check_the_install_gate_refuses_a_stale_role_letter("pub fn unrelated() {}").is_some(),
        "an absent gate must be reported"
    );
    assert!(
        check_each_form_has_its_own_builder("pub fn unrelated() {}").is_some(),
        "an absent owner must be reported"
    );
    assert!(
        check_the_counters_distinguish_the_forms("pub fn unrelated() {}").is_some(),
        "an absent counter must be reported"
    );
    assert!(
        check_the_third_rule_has_its_own_reason("pub fn unrelated() {}").is_some(),
        "an absent reason must be reported"
    );
}

#[test]
fn the_guard_reports_a_composition_that_refuses_the_role() {
    let violating = "\
fn something() {
    let _ = \"SSU2 service requires the strict controlled profile\";
}
";
    let reason = check_the_role_is_reachable_behind_its_opt_in(violating);
    assert!(
        reason.is_some(),
        "a blanket refusal must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_role_letter_that_is_not_gated() {
    let violating = "\
fn capabilities_for(&self) -> Result<(), ()> {
    let value = format!(\"{family_letter}C\");
    let _ = value;
    Ok(())
}
";
    let reason = check_the_role_letter_is_composed_only_while_the_role_is_live(violating);
    assert!(
        reason.is_some(),
        "an ungated role letter must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_role_letter_on_the_introducer_only_form() {
    let violating = "\
pub fn publication_material(&self) -> Result<(), ()> {
    let _ = self.capabilities_for(family);
    Ok(())
}

pub fn introducer_publication_material(&self) -> Result<(), ()> {
    let _ = self.capabilities_for(family);
    Ok(Ssu2PublicationMaterial { advertises_introducer_role: true });
}
";
    let reason = check_only_the_direct_form_carries_the_role_letter(violating);
    assert!(
        reason.is_some(),
        "a role letter on the introducer-only form must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_third_rule_that_is_not_exact() {
    let violating = "\
pub fn is_qualified_ssu2_relay_role_address(address: &RouterAddress) -> bool {
    let caps = \"4C\";
    let _ = caps;
    true
}

pub fn is_qualified_ssu2_address(address: &RouterAddress) -> bool {
    let caps = \"4\";
    let _ = caps;
    true
}

pub fn is_qualified_ssu2_introducer_address(address: &RouterAddress) -> bool {
    let caps = \"4\";
    let _ = caps;
    true
}
";
    let reason = check_the_third_rule_is_exact(violating);
    assert!(
        reason.is_some(),
        "a permissive third rule must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_third_builder_with_a_floodfill_permit() {
    let violating = "\
pub fn build_with_ssu2_relay_role_address(&self, permit: &FloodfillAdvertisementPermit) -> Result<(), ()> {
    let _ = is_qualified_ssu2_relay_role_address(&address);
    Ok(())
}
";
    let reason = check_the_third_builder_carries_no_permit(violating);
    assert!(
        reason.is_some(),
        "a third builder with a floodfill permit must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_install_gate_that_accepts_a_stale_role_letter() {
    let violating = "\
fn install_local_router_info_inner(&self) -> Result<(), ()> {
    let _ = parsed.capabilities().supports_relay();
    Ok(())
}
";
    let reason = check_the_install_gate_refuses_a_stale_role_letter(violating);
    assert!(
        reason.is_some(),
        "an install gate that accepts a stale role letter must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_form_without_its_own_builder() {
    let violating = "\
fn published_form(material: &Ssu2PublicationMaterial) -> usize {
    let _ = material.advertises_introducer_role;
    0
}

fn build_record(&self) -> Result<(), ()> {
    let _ = build_with_ssu2_address(1);
    let _ = build_with_ssu2_introducer_address(2);
    Ok(())
}
";
    let reason = check_each_form_has_its_own_builder(violating);
    assert!(
        reason.is_some(),
        "a form without its own builder must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_counters_that_do_not_distinguish_the_forms() {
    let violating = "\
pub struct SelfPublicationCounters {
    published_direct: AtomicU64,
    published_introducer_only: AtomicU64,
}

pub fn snapshot(&self) -> u64 {
    let published_direct = 0;
    let published_introducer_only = 0;
    published_direct + published_introducer_only
}
";
    let reason = check_the_counters_distinguish_the_forms(violating);
    assert!(
        reason.is_some(),
        "counters that do not distinguish the forms must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_third_rule_without_its_own_reason() {
    let violating = "\
pub enum SelfPublicationFailure {
    UnqualifiedAddress,
}

fn from_netdb(error: u8, form: u8) -> u8 {
    let _ = form;
    error
}
";
    let reason = check_the_third_rule_has_its_own_reason(violating);
    assert!(
        reason.is_some(),
        "a third rule without its own reason must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_owner_that_adds_infrastructure() {
    let violating = "\
fn something() {
    let _ = tokio::spawn(async {});
}
";
    let reason = check_the_owner_adds_no_infrastructure(violating, "pub fn unrelated() {}");
    assert!(
        reason.is_some(),
        "an owner that adds a task must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_daemon_that_writes_option_vocabulary() {
    let violating = "\
fn something() {
    let _ = \"ih\";
}
";
    let reason = check_the_daemon_writes_no_option_vocabulary(violating);
    assert!(
        reason.is_some(),
        "a daemon that writes option vocabulary must be reported: {reason:?}"
    );
}
