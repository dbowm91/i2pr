//! Plan 520's structural guard for the router's own published address.
//!
//! The router publishes its own RouterInfo to the network. Plan 520 gives
//! that record an address a peer can **qualify**: the direct SSU2 form,
//! carrying the family capability and the transport's MTU, published only
//! while the router's own reachability evidence admits it, and withdrawn
//! when the evidence lapses.
//!
//! The guard asserts twelve things about the **production** surface:
//!
//! 1. the switch is **not** refused at parse time, and it defaults to off;
//! 2. the daemon holds **no copy** of the direct-versus-firewalled rule:
//!    the transport's builder decides and the daemon asks the runtime;
//! 3. the owner never builds an address: the address is the transport's,
//!    whole, with the router's own bound endpoint;
//! 4. the netdb's addressed builder qualifies the address with the netdb's
//!    **own** rule and needs no floodfill permit;
//! 5. the floodfill builders still require the permit;
//! 6. the runtime's material carries the **family** capability, and the
//!    transport writes the `caps` option only when the set is non-empty;
//! 7. the record is installed, and the cycle publishes it **after** the
//!    owner has refreshed it;
//! 8. the re-dating path preserves the address;
//! 9. the expiry withdraws the address;
//! 10. the daemon records **no** reachability signal of its own;
//! 11. the owner starts no task, no timer and no listener, and the
//!     configuration gains no new key;
//! 12. the runtime's install gate binds every SSU2 address the record
//!     carries to the router's own endpoint, and refuses two.
//!
//! The rules are **structural**: they assert that a *named function's own
//! body* contains the required **call**, with its open parenthesis. Comments
//! are stripped and `#[cfg(test)]` blocks are excluded, so a doc comment that
//! merely names a construct never satisfies a rule, and a function that is
//! **absent** fails closed.
//!
//! Every rule has an **inverted probe**: a mutation that must be reported. A
//! rule with no probe is a rule nobody has shown to fire.

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
/// The crypto crate's re-dating helper.
const CRYPTO_MODULE: &str = "crates/i2pr-crypto/src/lib.rs";
/// The runtime's own publication material and install gate.
const RUNTIME_MODULE: &str = "crates/i2pr-runtime/src/ssu2_runtime.rs";
/// The transport's own publication builder.
const TRANSPORT_PUBLICATION_MODULE: &str = "crates/i2pr-transport-ssu2/src/publication.rs";

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

/// Returns the offset of the **first** occurrence of `needle`.
///
/// Some needles name a call that has no declaration in the file under
/// inspection — `self_publication.refresh(` names a method, not a free
/// function — and the definition/call distinction of [`call_site`] would
/// then discard the very occurrence the rule is about.
fn first_offset(code: &str, needle: &str) -> Option<usize> {
    code.find(needle)
}

// ----------------------------------------------------------------------
// The rules.
// ----------------------------------------------------------------------

/// Rule 1: the switch is not refused at parse time, and it defaults to off.
///
/// The refusal that remains is **conditional**: it names the cases the
/// switch cannot serve (the service is not running, the profile is not the
/// controlled loopback one), and it is never the unconditional refusal the
/// milestone once carried.
fn check_the_switch_is_not_refused_at_parse_time(config: &str) -> Option<String> {
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
    if normalize.contains("SSU2 address publication is unavailable in this milestone") {
        return Some("the switch must not be refused unconditionally at parse time".to_owned());
    }
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

/// Rule 2: the daemon holds no copy of the direct-versus-firewalled rule.
///
/// The owner asks the runtime for the material and takes the transport's
/// answer. It never names a reachability state and never names the
/// transport's builder.
fn check_the_daemon_holds_no_copy_of_the_publication_rule(owner: &str) -> Option<String> {
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
    // Plan 525: the runtime now answers **which form** the router
    // publishes, so the owner must ask the runtime for the composed
    // form. Asking for one form alone would publish the direct address
    // and never the introducer-only one.
    if !refresh.contains("service.self_publication_material(") {
        return Some("the owner must ask the runtime for the published form".to_owned());
    }
    None
}

/// Rule 3: the owner never builds an address.
///
/// The address is the transport's, whole: the owner neither constructs a
/// `RouterAddress` nor assembles an address's options, so the published
/// endpoint is the runtime's bound endpoint and never a configured literal.
fn check_the_owner_never_builds_an_address(owner: &str) -> Option<String> {
    let code = code_view(owner);
    for needle in ["RouterAddress::new(", "Mapping::from_entries("] {
        if code.contains(needle) {
            return Some(format!(
                "the owner must not build the address itself: {needle}"
            ));
        }
    }
    None
}

/// Rule 4: the netdb's addressed builder qualifies the address and needs no
/// permit.
///
/// The builder refuses an address the netdb's **own** qualification rule
/// refuses, and it takes no floodfill permit, so it cannot mint `caps=f`.
fn check_the_addressed_builder_qualifies_without_a_permit(netdb: &str) -> Option<String> {
    let code = code_view(netdb);
    let Some(body) = item_body(&code, "pub fn build_with_ssu2_address") else {
        return Some("the netdb's addressed builder is absent".to_owned());
    };
    if !body.contains("is_qualified_ssu2_address(") {
        return Some("the addressed builder must use the netdb's own qualification".to_owned());
    }
    if body.contains("permit") {
        return Some("the addressed builder must take no floodfill permit".to_owned());
    }
    let Some(signature) = find_definition(&code, "pub fn build_with_ssu2_address") else {
        return Some("the netdb's addressed builder is absent".to_owned());
    };
    let declaration = code[signature..].lines().next().unwrap_or_default();
    let following = code[signature..]
        .lines()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ");
    if following.contains("FloodfillAdvertisementPermit") {
        return Some("the addressed builder must take no floodfill permit".to_owned());
    }
    let _ = declaration;
    None
}

/// Rule 5: the floodfill builders still require the permit.
fn check_the_floodfill_builders_still_require_the_permit(netdb: &str) -> Option<String> {
    let code = code_view(netdb);
    let Some(start) = find_definition(&code, "pub fn build_floodfill(") else {
        return Some("the netdb's floodfill builder is absent".to_owned());
    };
    let signature = code[start..].lines().take(8).collect::<Vec<_>>().join(" ");
    if !signature.contains("permit: &FloodfillAdvertisementPermit") {
        return Some("the floodfill builder must still require the permit".to_owned());
    }
    let Some(body) = item_body(&code, "fn build_floodfill_with_caps") else {
        return Some("the netdb's floodfill builder is absent".to_owned());
    };
    if !body.contains("permit") {
        return Some("the floodfill builder must still require the permit".to_owned());
    }
    None
}

/// Rule 6: the runtime's material carries the family capability, and the
/// transport writes the `caps` option only when the set is non-empty.
///
/// An empty capability set would produce an address with **no** `caps`
/// option at all, and the netdb's own qualification refuses that.
fn check_the_family_capability_is_carried(runtime: &str, transport: &str) -> Option<String> {
    let runtime_code = code_view(runtime);
    let Some(material) = item_body(&runtime_code, "pub fn publication_material") else {
        return Some("the runtime's publication material is absent".to_owned());
    };
    if !material.contains("Ssu2Capabilities::parse(") {
        return Some("the runtime's material must carry the family capability".to_owned());
    }
    if !material.contains("\"4\"") || !material.contains("\"6\"") {
        return Some("the family capability must be the family letter".to_owned());
    }
    let transport_code = code_view(transport);
    let Some(snapshot) = item_body(&transport_code, "fn direct_snapshot") else {
        return Some("the transport's direct snapshot is absent".to_owned());
    };
    if !snapshot.contains("if !caps.as_str().is_empty()") {
        return Some(
            "the transport must write the caps option only when the set is non-empty".to_owned(),
        );
    }
    None
}

/// Rule 7: the record is installed, and the cycle publishes it after the
/// owner has refreshed it.
///
/// The owner installs the record, and the publication's cycle re-dates and
/// publishes **the record the owner installed** — so every rung of the cycle
/// publishes the same bytes.
fn check_the_record_is_installed_and_published(composition: &str, owner: &str) -> Option<String> {
    let owner_code = code_view(owner);
    let Some(refresh) = item_body(&owner_code, "pub fn refresh") else {
        return Some("the owner's refresh is absent".to_owned());
    };
    if !refresh.contains("install_local_router_info(") {
        return Some("the owner must install the record it built".to_owned());
    }
    let code = code_view(composition);
    let Some(own) = first_offset(&code, "self_publication.refresh(") else {
        return Some("the composition never refreshes the router's own address".to_owned());
    };
    let Some(dated) = first_offset(&code, "refresh_local_router_info_record(") else {
        return Some("the composition never re-dates the record".to_owned());
    };
    if own > dated {
        return Some(
            "the owner must refresh the record before the cycle re-dates and publishes it"
                .to_owned(),
        );
    }
    None
}

/// Rule 8: the re-dating path preserves the address.
///
/// The re-dating re-signs the **installed** record with a new date; it never
/// rebuilds the record from scratch, so the addresses survive.
fn check_the_redating_preserves_the_address(crypto: &str) -> Option<String> {
    let code = code_view(crypto);
    let Some(body) = item_body(&code, "pub fn resign_router_info_with_published") else {
        return Some("the re-dating helper is absent".to_owned());
    };
    if !body.contains("signed_bytes_with_published(") {
        return Some("the re-dating must re-sign the installed record".to_owned());
    }
    if body.contains("RouterInfo::new(") {
        return Some("the re-dating must not rebuild the record and drop its addresses".to_owned());
    }
    None
}

/// Rule 9: the expiry withdraws the address.
///
/// When the transport admits no address, the owner builds the record with
/// **no** address: the router does not keep publishing an address it can no
/// longer justify.
fn check_the_expiry_withdraws_the_address(owner: &str) -> Option<String> {
    let code = code_view(owner);
    let Some(build) = item_body(&code, "fn build_record") else {
        return Some("the owner's record builder is absent".to_owned());
    };
    if !build.contains("build_with_ssu2_address(") {
        return Some("the addressed form must be built through the netdb's builder".to_owned());
    }
    if !build.contains(".build(published, options)") {
        return Some("the withdrawal must build the record with no address".to_owned());
    }
    let Some(decide) = item_body(&code, "pub fn decide") else {
        return Some("the owner's decision is absent".to_owned());
    };
    if !decide.contains("Err(reason)") {
        return Some("the owner must handle the transport's refusal".to_owned());
    }
    None
}

/// Rule 10: the daemon records no reachability signal of its own.
///
/// The runtime owns the evidence; the daemon owns the opt-in and the
/// install. A signal the daemon does not genuinely hold is never recorded.
fn check_the_daemon_records_no_reachability_signal(
    owner: &str,
    composition: &str,
) -> Option<String> {
    for (name, text) in [("the owner", owner), ("the composition", composition)] {
        let code = code_view(text);
        if code.contains("ReachabilitySignal") {
            return Some(format!(
                "{name} must not record a reachability signal of its own"
            ));
        }
    }
    None
}

/// Rule 11: the owner starts no task, no timer and no listener, and the
/// configuration gains no new key.
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

/// Rule 12: the runtime's install gate binds every SSU2 address the record
/// carries to the router's own endpoint, and refuses two.
///
/// The gate may accept a record with **no** SSU2 address — the transport's
/// endpoint-less form — but every address it does carry must name the
/// router's own bound endpoint with the router's own keys.
fn check_the_install_gate_binds_every_address(runtime: &str) -> Option<String> {
    let code = code_view(runtime);
    let Some(body) = item_body(&code, "fn install_local_router_info_inner") else {
        return Some("the runtime's install gate is absent".to_owned());
    };
    if body.contains("ssu2_count != 1") {
        return Some("the install gate must not require an SSU2 address to be present".to_owned());
    }
    if !body.contains("ssu2_count > 1") {
        return Some("the install gate must refuse two conflicting SSU2 addresses".to_owned());
    }
    if !body.contains(".all(|address| {") {
        return Some("every SSU2 address the record carries must be bound".to_owned());
    }
    if !body.contains("is_bound") {
        return Some("every SSU2 address must name the router's own bound endpoint".to_owned());
    }
    None
}

// ----------------------------------------------------------------------
// The rules as tests.
// ----------------------------------------------------------------------

#[test]
fn the_switch_is_not_refused_at_parse_time() {
    let reason = check_the_switch_is_not_refused_at_parse_time(&source_of(CONFIG_MODULE));
    assert!(
        reason.is_none(),
        "the switch must be a real opt-in: {reason:?}"
    );
}

#[test]
fn the_daemon_holds_no_copy_of_the_publication_rule() {
    let reason = check_the_daemon_holds_no_copy_of_the_publication_rule(&source_of(OWNER_MODULE));
    assert!(reason.is_none(), "the rule must have one owner: {reason:?}");
}

#[test]
fn the_owner_never_builds_an_address() {
    let reason = check_the_owner_never_builds_an_address(&source_of(OWNER_MODULE));
    assert!(
        reason.is_none(),
        "the address must be the transport's: {reason:?}"
    );
}

#[test]
fn the_addressed_builder_qualifies_without_a_permit() {
    let reason =
        check_the_addressed_builder_qualifies_without_a_permit(&source_of(NETDB_LOCAL_MODULE));
    assert!(
        reason.is_none(),
        "the netdb's own rule must qualify it: {reason:?}"
    );
}

#[test]
fn the_floodfill_builders_still_require_the_permit() {
    let reason =
        check_the_floodfill_builders_still_require_the_permit(&source_of(NETDB_LOCAL_MODULE));
    assert!(
        reason.is_none(),
        "caps=f must stay permit-gated: {reason:?}"
    );
}

#[test]
fn the_family_capability_is_carried() {
    let reason = check_the_family_capability_is_carried(
        &source_of(RUNTIME_MODULE),
        &source_of(TRANSPORT_PUBLICATION_MODULE),
    );
    assert!(
        reason.is_none(),
        "the address must carry the family letter: {reason:?}"
    );
}

#[test]
fn the_record_is_installed_and_published() {
    let reason = check_the_record_is_installed_and_published(
        &source_of(COMPOSITION_MODULE),
        &source_of(OWNER_MODULE),
    );
    assert!(
        reason.is_none(),
        "the installed record must be the published one: {reason:?}"
    );
}

#[test]
fn the_redating_preserves_the_address() {
    let reason = check_the_redating_preserves_the_address(&source_of(CRYPTO_MODULE));
    assert!(
        reason.is_none(),
        "the address must survive the re-dating: {reason:?}"
    );
}

#[test]
fn the_expiry_withdraws_the_address() {
    let reason = check_the_expiry_withdraws_the_address(&source_of(OWNER_MODULE));
    assert!(
        reason.is_none(),
        "the lapsed address must be withdrawn: {reason:?}"
    );
}

#[test]
fn the_daemon_records_no_reachability_signal() {
    let reason = check_the_daemon_records_no_reachability_signal(
        &source_of(OWNER_MODULE),
        &source_of(COMPOSITION_MODULE),
    );
    assert!(
        reason.is_none(),
        "the evidence must be the runtime's: {reason:?}"
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
fn the_install_gate_binds_every_address() {
    let reason = check_the_install_gate_binds_every_address(&source_of(RUNTIME_MODULE));
    assert!(
        reason.is_none(),
        "every published address must be bound: {reason:?}"
    );
}

// ----------------------------------------------------------------------
// The inverted probes: each rule must report its own violation.
// ----------------------------------------------------------------------

#[test]
fn the_guard_reports_an_unconditional_parse_refusal() {
    let violating = "\
fn default_ssu2_advertise() -> bool {
    false
}
fn normalize_ssu2(raw: &RawSsu2Config) -> Result<Ssu2Config, ConfigError> {
    if raw.advertise {
        return Err(ConfigError::Semantic {
            field: \"ssu2.advertise\",
            reason: \"SSU2 address publication is unavailable in this milestone\",
        });
    }
    if !raw.enabled {
        return Err(ConfigError::Semantic { field: \"ssu2.enabled\", reason: \"x\" });
    }
    if private_lan {
        return Err(ConfigError::Semantic { field: \"ssu2.advertise\", reason: \"y\" });
    }
    Ok(Ssu2Config { enabled: raw.enabled })
}
";
    let reason = check_the_switch_is_not_refused_at_parse_time(violating);
    assert!(
        reason.is_some(),
        "an unconditional refusal must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_daemon_copy_of_the_publication_rule() {
    let violating = "\
pub fn refresh(&self, bundle: &RouterIdentityBundle, service: &Ssu2RuntimeService, now_ms: u64) -> SelfPublicationOutcome {
    let _ = ReachabilityState::Reachable;
    let _ = service.publication_material(now_ms);
    SelfPublicationOutcome::Disabled
}
";
    let reason = check_the_daemon_holds_no_copy_of_the_publication_rule(violating);
    assert!(
        reason.is_some(),
        "a daemon copy of the rule must be reported: {reason:?}"
    );
    // Plan 525: an owner that asks for one form alone never publishes the
    // introducer-only address, so it must be reported as well.
    let single_form = "\
pub fn refresh(&self, bundle: &RouterIdentityBundle, service: &Ssu2RuntimeService, now_ms: u64) -> SelfPublicationOutcome {
    let _ = service.publication_material(now_ms);
    SelfPublicationOutcome::Disabled
}
";
    let reason = check_the_daemon_holds_no_copy_of_the_publication_rule(single_form);
    assert!(
        reason.is_some(),
        "an owner that asks for one form alone must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_owner_that_builds_its_own_address() {
    let violating = "\
pub fn refresh(&self) -> u64 {
    let _address = RouterAddress::new(10, Date::from_millis(0), \"SSU2\".to_owned(), Mapping::from_entries(vec![]).unwrap());
    0
}
";
    let reason = check_the_owner_never_builds_an_address(violating);
    assert!(
        reason.is_some(),
        "an owner-built address must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_addressed_builder_that_takes_a_permit() {
    let violating = "\
pub fn build_with_ssu2_address(&self, published: Date, options: Mapping, address: RouterAddress, permit: &FloodfillAdvertisementPermit) -> Result<LocalRouterInfo, LocalRouterInfoError> {
    let _ = is_qualified_ssu2_address(&address);
    let _ = permit;
    Ok(LocalRouterInfo { validated: unreachable!() })
}
";
    let reason = check_the_addressed_builder_qualifies_without_a_permit(violating);
    assert!(
        reason.is_some(),
        "a permit-taking addressed builder must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_floodfill_builder_without_a_permit() {
    let violating = "\
pub fn build_floodfill(&self, published: Date, options: Mapping, address: RouterAddress) -> Result<LocalRouterInfo, LocalRouterInfoError> {
    Ok(LocalRouterInfo { validated: unreachable!() })
}
fn build_floodfill_with_caps(&self) -> u8 {
    0
}
";
    let reason = check_the_floodfill_builders_still_require_the_permit(violating);
    assert!(
        reason.is_some(),
        "a permit-free floodfill builder must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_material_without_the_family_capability() {
    let runtime = "\
pub fn publication_material(&self, wall_now_ms: u64) -> Result<Ssu2PublicationMaterial, Ssu2PublicationUnavailable> {
    let _ = Ssu2Capabilities::parse(\"4\");
    Ok(material)
}
";
    let transport = "\
fn direct_snapshot() -> u8 {
    0
}
";
    let reason = check_the_family_capability_is_carried(runtime, transport);
    assert!(
        reason.is_some(),
        "a material without the family letter must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_transport_that_always_writes_caps() {
    let runtime = "\
pub fn publication_material(&self, wall_now_ms: u64) -> Result<Ssu2PublicationMaterial, Ssu2PublicationUnavailable> {
    let capabilities = Ssu2Capabilities::parse(\"4\").map_err(|_| Ssu2PublicationUnavailable::InvalidPublication)?;
    let _ = capabilities;
    Ok(material)
}
";
    let transport = "\
fn direct_snapshot() -> u8 {
    options.push((\"caps\".to_owned(), caps.as_str().to_owned()));
    0
}
";
    let reason = check_the_family_capability_is_carried(runtime, transport);
    assert!(
        reason.is_some(),
        "an unconditional caps option must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_cycle_that_publishes_before_the_owner_refreshes() {
    let composition = "\
fn compose() {
    let local_record = refresh_local_router_info_record(&bundle, &handle, now_ms);
    let outcome = self_publication.refresh(&bundle, handle.service(), now_ms);
}
";
    let owner = "\
pub fn refresh(&self, bundle: &RouterIdentityBundle, service: &Ssu2RuntimeService, now_ms: u64) -> SelfPublicationOutcome {
    service.install_local_router_info(encoded, now_ms);
    SelfPublicationOutcome::Published
}
";
    let reason = check_the_record_is_installed_and_published(composition, owner);
    assert!(
        reason.is_some(),
        "a cycle that publishes a stale record must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_a_redating_that_rebuilds_the_record() {
    let crypto = "\
pub fn resign_router_info_with_published(bundle: &RouterIdentityBundle, encoded: &[u8], published: Date) -> Result<Vec<u8>, CryptoError> {
    let info = RouterInfo::new(identity, published, Vec::new(), Vec::new(), options, signature)?;
    Ok(Vec::new())
}
";
    let reason = check_the_redating_preserves_the_address(crypto);
    assert!(
        reason.is_some(),
        "a re-dating that drops the addresses must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_owner_that_keeps_the_lapsed_address() {
    let violating = "\
pub fn decide(&self, bundle: &RouterIdentityBundle, installed: Option<&[u8]>, material: Result<&Ssu2PublicationMaterial, Ssu2PublicationUnavailable>, now_ms: u64) -> SelfPublicationDecision {
    SelfPublicationDecision::Disabled
}
fn build_record(&self, bundle: &RouterIdentityBundle, address: Option<i2pr_proto::RouterAddress>, now_ms: u64) -> Result<Vec<u8>, SelfPublicationFailure> {
    builder.build_with_ssu2_address(published, options, address.unwrap());
    Ok(Vec::new())
}
";
    let reason = check_the_expiry_withdraws_the_address(violating);
    assert!(
        reason.is_some(),
        "a kept lapsed address must be reported: {reason:?}"
    );
}

/// The probe's fixture names a **tracker** handle rather than the
/// runtime's own `reachability` field. The rule it exercises is unchanged
/// -- it is the *signal* the daemon may never name -- but the M12 floodfill
/// boundary checker scans the crates for corroboration recording outside the
/// runtime's owner, and its own exclusion of the test directories
/// (`--glob '!*/tests/*'`) does not reach a nested `crates/*/tests/` path. A
/// fixture that spelled the runtime's field name would therefore be read as
/// a production recorder. The fixture is a string inside a test file, and
/// the checker itself is left exactly as it is.
#[test]
fn the_guard_reports_a_daemon_that_records_its_own_signal() {
    let owner = "\
pub fn refresh(&self) -> u64 {
    state.tracker.record(ReachabilitySignal::LocalConfiguredBind { family }, now);
    0
}
";
    let reason = check_the_daemon_records_no_reachability_signal(owner, "fn compose() {}");
    assert!(
        reason.is_some(),
        "a daemon-recorded signal must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_owner_that_starts_a_task() {
    let owner = "\
pub fn refresh(&self) -> u64 {
    tokio::spawn(async move {});
    0
}
";
    let config = "\
struct RawSsu2Config {
    advertise: bool,
}
";
    let reason = check_the_owner_adds_no_infrastructure(owner, config);
    assert!(
        reason.is_some(),
        "an owner-started task must be reported: {reason:?}"
    );
}

#[test]
fn the_guard_reports_an_install_gate_that_requires_an_address() {
    let runtime = "\
fn install_local_router_info_inner(&self, encoded: Vec<u8>, wall_now_ms: u64) -> Result<(), Ssu2PublicationUnavailable> {
    if ssu2_count != 1 {
        return Err(Ssu2PublicationUnavailable::InvalidPublication);
    }
    let address_matches = info.addresses().iter().any(|address| {
        let is_bound = true;
        is_bound
    });
    Ok(())
}
";
    let reason = check_the_install_gate_binds_every_address(runtime);
    assert!(
        reason.is_some(),
        "a gate that requires an address must be reported: {reason:?}"
    );
}
