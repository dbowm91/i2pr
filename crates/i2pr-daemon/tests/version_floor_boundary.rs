//! Plan 491 static boundary guard for the API-version floors.
//!
//! Plan 491's invariants are not only behavioural; some of them are
//! *structural*, and a behavioural test cannot see a structural drift:
//!
//! 1. **The declared version is not edited.** `CONTROLLED_ROUTER_VERSION`
//!    stays pinned to the implemented I2NP surface. The plan adds compliance
//!    with the declared level, never a change to it.
//! 2. **The floors are derived, not hand-maintained.** The minimum peer
//!    version for tunnel building and the minimum floodfill version for DSM
//!    come from the specification's table and the declared level. A literal
//!    copy of the floor in the policy module would be a second copy of the
//!    truth, and the plan's stop condition forbids it.
//! 3. **Both enforcement points consult the gate.** The tunnel-build peer
//!    selection and the DSM publication target selection must ask the policy
//!    for the eligibility decision; a selection that quietly ignored the
//!    floor would look identical to a compliant one in the receipts.
//! 4. **No operator knob.** The floors are specification constants. A
//!    configuration key that could lower them would let an operator put the
//!    router below the level it declares.
//! 5. **The receipts carry counts, not identities.** A receipt that could
//!    name a peer is not a receipt.
//!
//! The guard is deliberately **fail-closed**: it strips comments before
//! matching (so a doc comment that merely names a construct is not a
//! violation), it excludes `#[cfg(test)]` blocks, and every rule carries its
//! own **inverted probe** — a synthetic violating source must be reported.
//! If a rule ever stops detecting its violation, its probe fails.

use std::fs;
use std::path::{Path, PathBuf};

/// The declaration the plan forbids editing.
const DECLARED_VERSION_LINE: &str = "pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.62\";";

/// The tokens that would make the policy module hold its own copy of the
/// floor.
const HAND_MAINTAINED_FLOOR_TOKENS: &[&str] = &["\"0.9.62\"", "0, 9, 62"];

/// The tokens that would let an operator lower a floor.
const CONFIG_KNOB_TOKENS: &[&str] = &[
    "MIN_PEER_VERSION_FOR_TUNNEL_BUILD",
    "MIN_FLOODFILL_VERSION_FOR_DSM",
    "min_peer_version",
    "min_floodfill_version",
    "version_floor",
];

/// The tokens that would put a peer identity into a receipt.
const RECEIPT_IDENTITY_TOKENS: &[&str] = &[
    "RouterHash",
    "RouterIdentity",
    "router_hash",
    "peer_hash",
    "address",
    "session_key",
];

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

/// Rule 1: the declared version is not edited.
fn check_declared_version(source: &str) -> Option<String> {
    let code = code_view(source);
    if code.contains(DECLARED_VERSION_LINE) {
        None
    } else {
        Some(format!(
            "the controlled declaration must remain `{DECLARED_VERSION_LINE}`"
        ))
    }
}

/// Rule 2: the policy module must not hold its own copy of the floor.
fn check_no_hand_maintained_floor(source: &str) -> Option<String> {
    let code = code_view(source);
    for token in HAND_MAINTAINED_FLOOR_TOKENS {
        if code.contains(token) {
            return Some(format!(
                "the policy module must derive the floor, not restate it ({token})"
            ));
        }
    }
    None
}

/// Rule 3: an enforcement point must consult the gate.
fn check_calls_gate(source: &str, gate: &str) -> Option<String> {
    let code = code_view(source);
    if code.contains(gate) {
        None
    } else {
        Some(format!("the enforcement point must consult `{gate}`"))
    }
}

/// Rule 4: no configuration key may lower a floor.
fn check_no_config_knob(source: &str) -> Option<String> {
    let code = code_view(source);
    for token in CONFIG_KNOB_TOKENS {
        if code.contains(token) {
            return Some(format!(
                "the floors are specification constants, not operator knobs ({token})"
            ));
        }
    }
    None
}

/// Rule 5: a receipt may not carry a peer identity.
fn check_receipt_has_no_identity(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(start) = code.find("pub struct VersionEligibilityReceipt") else {
        return Some("the version receipt type must exist".to_owned());
    };
    let Some(open) = code[start..].find('{') else {
        return Some("the version receipt type must have a body".to_owned());
    };
    let body_start = start + open;
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut cursor = body_start;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    let body = &code[body_start..cursor];
    for token in RECEIPT_IDENTITY_TOKENS {
        if body.contains(token) {
            return Some(format!("a receipt may not carry a peer identity ({token})"));
        }
    }
    None
}

#[test]
fn the_declared_version_is_not_edited() {
    let source = source_of("crates/i2pr-netdb/src/local.rs");
    assert_eq!(
        check_declared_version(&source),
        None,
        "Plan 491 must not edit the declared version"
    );
}

#[test]
fn the_policy_module_derives_the_floors() {
    let source = source_of("crates/i2pr-netdb/src/version.rs");
    assert_eq!(
        check_no_hand_maintained_floor(&source),
        None,
        "the floors must be derived from the specification's table"
    );
}

#[test]
fn the_tunnel_build_enforcement_point_consults_the_gate() {
    let source = source_of("crates/i2pr-daemon/src/destination_peers.rs");
    assert_eq!(
        check_calls_gate(&source, "tunnel_peer_eligibility"),
        None,
        "the tunnel-build selection must consult the tunnel-build gate"
    );
}

#[test]
fn the_dsm_enforcement_points_consult_the_gate() {
    let daemon = source_of("crates/i2pr-daemon/src/netdb_tunnels.rs");
    assert_eq!(
        check_calls_gate(&daemon, "dsm_target_eligibility"),
        None,
        "the DSM publication path must consult the DSM gate"
    );
    let publication = source_of("crates/i2pr-netdb/src/publication.rs");
    assert_eq!(
        check_calls_gate(&publication, "dsm_target_eligibility"),
        None,
        "the publication target selection must consult the DSM gate"
    );
}

#[test]
fn the_selection_policy_honours_the_version_gate() {
    let source = source_of("crates/i2pr-netdb/src/selection.rs");
    assert_eq!(
        check_calls_gate(&source, "version_gate"),
        None,
        "the selection policy must honour the version gate"
    );
}

#[test]
fn no_configuration_key_can_lower_a_floor() {
    let source = source_of("crates/i2pr-daemon/src/config.rs");
    assert_eq!(
        check_no_config_knob(&source),
        None,
        "the floors must not be operator configuration"
    );
}

#[test]
fn the_version_receipt_carries_no_peer_identity() {
    let source = source_of("crates/i2pr-daemon/src/netdb_participation.rs");
    assert_eq!(
        check_receipt_has_no_identity(&source),
        None,
        "a receipt that could name a peer is not a receipt"
    );
}

/// The inverted probe for rule 1: a declaration that was edited must be
/// reported.
#[test]
fn the_guard_reports_an_edited_declaration() {
    let violating = "pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.70\";";
    assert!(
        check_declared_version(violating).is_some(),
        "an edited declaration must be reported"
    );
}

/// The inverted probe for rule 2: a policy module that restates the floor
/// must be reported.
#[test]
fn the_guard_reports_a_hand_maintained_floor() {
    let as_string = "pub const MIN_PEER_VERSION_FOR_TUNNEL_BUILD: &str = \"0.9.62\";";
    assert!(
        check_no_hand_maintained_floor(as_string).is_some(),
        "a restated floor must be reported"
    );
    let as_number = "pub const FLOOR: ApiVersion = ApiVersion::new(0, 9, 62);";
    assert!(
        check_no_hand_maintained_floor(as_number).is_some(),
        "a restated floor must be reported"
    );
}

/// The inverted probe for rule 3: an enforcement point that ignores the gate
/// must be reported.
#[test]
fn the_guard_reports_an_enforcement_point_without_the_gate() {
    let violating = "fn project(store: &RouterInfoStore) -> Vec<Candidate> { Vec::new() }";
    assert!(
        check_calls_gate(violating, "tunnel_peer_eligibility").is_some(),
        "an enforcement point that ignores the gate must be reported"
    );
}

/// The inverted probe for rule 4: a configuration key that lowers a floor
/// must be reported.
#[test]
fn the_guard_reports_a_configuration_knob() {
    let violating = "pub min_peer_version: Option<String>,";
    assert!(
        check_no_config_knob(violating).is_some(),
        "a configuration knob must be reported"
    );
}

/// The inverted probe for rule 5: a receipt that names a peer must be
/// reported.
#[test]
fn the_guard_reports_a_receipt_carrying_an_identity() {
    let violating = "\
pub struct VersionEligibilityReceipt {
    pub kind: VersionDecisionKind,
    pub peer: RouterHash,
    pub refusals: BTreeMap<&'static str, u32>,
}
";
    assert!(
        check_receipt_has_no_identity(violating).is_some(),
        "a receipt carrying an identity must be reported"
    );
}

/// The guard must not report a construct that only appears in a comment or in
/// a test block.
#[test]
fn the_guard_ignores_comments_and_test_blocks() {
    // Rule 1: the declaration is present in the code; the comment that names
    // another version must not change the verdict.
    let declared = "\
//! A router may declare `pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.70\";`.
pub const CONTROLLED_ROUTER_VERSION: &str = \"0.9.62\";
";
    assert_eq!(
        check_declared_version(declared),
        None,
        "a comment must not change the verdict"
    );

    // Rule 2: the floor literal appears in a comment and in a test block
    // only, so the policy module does not hold its own copy of it.
    let restated = "\
//! The floor is \"0.9.62\".
/// The floor is \"0.9.62\" and the gate is tunnel_peer_eligibility.
fn fine() -> u8 { 7 }

#[cfg(test)]
mod tests {
    const FLOOR: &str = \"0.9.62\";
    #[test]
    fn uses_the_floor() {
        let _ = tunnel_peer_eligibility(FLOOR);
    }
}
";
    assert_eq!(
        check_no_hand_maintained_floor(restated),
        None,
        "a comment or a test block must not be reported"
    );

    // Rule 3: the gate is consulted in production code, so it is found even
    // though a comment names it as well.
    let gated = "\
/// The gate is tunnel_peer_eligibility.
fn project(record: &ValidatedRouterInfo) -> Option<Candidate> {
    let eligibility = tunnel_peer_eligibility(published_version_of(record));
    eligibility.is_eligible().then_some(Candidate::default())
}
";
    assert_eq!(
        check_calls_gate(gated, "tunnel_peer_eligibility"),
        None,
        "the production gate call must be found"
    );
}
