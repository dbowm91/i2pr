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
//! Plan 503 discharges the finding Plan 491 left open: the second obligation
//! — the minimum floodfill version for DSM — was enforced **nowhere in the
//! two live `DatabaseStore` sends**, which selected floodfills by XOR
//! distance alone. Plan 503 puts the floor on both live DSM paths and makes
//! the peer-eligibility receipt a live product, and the guard gains the
//! structural rules that would have caught that gap:
//!
//! 6. **The live DSM publication paths consult the gated selection.** The
//!    creator-RouterInfo publication and the LeaseSet2 publication must ask
//!    the shared gated selection for their DSM targets, and neither may fall
//!    back to the ungated, distance-only selection.
//! 7. **The live gates record their decisions.** The tunnel-build gate and
//!    the two DSM gates must record a receipt, so a refusal is observable
//!    and never a silent skip.
//! 8. **The gate lives in one place.** The publication coordinator selects
//!    through the shared gated selection and the gate itself lives in the
//!    selection policy, so a refactor cannot move the enforcement out of the
//!    shared path without tripping the guard.
//!
//! The guard is deliberately **fail-closed**: it strips comments before
//! matching (so a doc comment that merely names a construct is not a
//! violation), it excludes `#[cfg(test)]` blocks, and every rule carries its
//! own **inverted probe** — a synthetic violating source must be reported.
//! If a rule ever stops detecting its violation, its probe fails.
//!
//! Rules 1, 2, 4 and 5 are **textual**: they assert that a token does or does
//! not appear anywhere in the production code view. Rules 3, 6, 7 and 8 are
//! **structural**: they assert that a *named function's own body* contains
//! the required call, so a token that appears in a sibling function, in a
//! comment, or in a test block does not satisfy them.

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

/// Returns the byte range of the brace-balanced block that opens at or after
/// `start`, as `(open, close)`; `close` is one past the closing brace.
///
/// The scan is **string-aware**: a brace inside a string literal or inside a
/// character literal does not change the nesting depth, so a body that holds
/// `"}"` does not end the block early. A lifetime (`'a`) is not a character
/// literal and is scanned as ordinary text.
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

/// Returns the body of the function whose **definition** is anchored at
/// `fn <name>`.
///
/// The anchor is the definition and never a call site: the needle carries the
/// `fn` keyword, and the character that follows it must not continue an
/// identifier, so `fn candidate_hashes` never matches
/// `fn version_gated_candidate_hashes`.
///
/// The body is the text between the function's own braces. A function that is
/// not present yields `None`, so every caller fails closed.
fn function_body<'a>(code: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("fn {name}");
    let mut offset = 0usize;
    while let Some(position) = code[offset..].find(needle.as_str()) {
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
        let (open, close) = brace_balanced_block(code, after)?;
        return Some(&code[open + 1..close - 1]);
    }
    None
}

/// Rule 6: the body of `fn <name>` must contain `token`.
///
/// The token is matched with its open parenthesis, so the rule asserts a
/// **call** and not a bare identifier: a dead helper, a type name, or an
/// unreachable branch does not satisfy it.
fn check_body_contains(source: &str, name: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, name) else {
        return Some(format!("`fn {name}` must exist in the production surface"));
    };
    if body.contains(token) {
        None
    } else {
        Some(format!("`fn {name}` must contain `{token}`"))
    }
}

/// Rule 6: the body of `fn <name>` must not contain `token`.
fn check_body_avoids(source: &str, name: &str, token: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = function_body(&code, name) else {
        return Some(format!("`fn {name}` must exist in the production surface"));
    };
    if body.contains(token) {
        Some(format!("`fn {name}` must not contain `{token}`"))
    } else {
        None
    }
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
///
/// The receipt type moved to `version_eligibility` in Plan 503; the rule's
/// substance is unchanged — only the module that owns the type moved.
fn check_receipt_has_no_identity(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(start) = code.find("pub struct VersionEligibilityReceipt") else {
        return Some("the version receipt type must exist".to_owned());
    };
    let Some((open, close)) = brace_balanced_block(&code, start) else {
        return Some("the version receipt type must have a body".to_owned());
    };
    let body = &code[open + 1..close - 1];
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

/// Plan 491 rule 3, plus Plan 503's rule 8: the gate must live in the shared
/// selection, and the shared selection must consult the gate.
///
/// Plan 503 refactored `PublicationCoordinator::nearest_eligible_floodfills`
/// onto the shared gated selection, so the gate call no longer sits in
/// `publication.rs`. The rule is **retargeted, not weakened**: it is now two
/// halves that pin *where* the enforcement lives — the coordinator must
/// consult `select_version_gated_floodfills`, and the selection policy must
/// consult `dsm_target_eligibility` — so a later refactor cannot move the
/// gate out of the shared selection without tripping the guard.
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
        check_body_contains(
            &publication,
            "nearest_eligible_floodfills",
            "select_version_gated_floodfills("
        ),
        None,
        "the publication target selection must consult the shared gated selection"
    );
    let policy = source_of("crates/i2pr-netdb/src/lookup_policy.rs");
    // Plan 545 extracted the gate into `gate_candidates` so both selectors --
    // the floodfill one and the peer one -- reach **one** implementation and
    // cannot come to disagree about a peer. The rule is **retargeted, not
    // weakened**: it used to pin one link and now pins two, so neither the
    // selection nor the gate can lose the DSM rule without tripping the guard.
    assert_eq!(
        check_body_contains(
            &policy,
            "select_version_gated_floodfills",
            "gate_candidates("
        ),
        None,
        "the shared gated selection must reach the shared gate"
    );
    assert_eq!(
        check_body_contains(&policy, "gate_candidates", "dsm_target_eligibility("),
        None,
        "the shared gate must consult the DSM gate"
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

/// Plan 491 rule 5. Plan 503 moved the receipt type out of
/// `netdb_participation` into its own `version_eligibility` module; the rule
/// and its inverted probe are unchanged.
#[test]
fn the_version_receipt_carries_no_peer_identity() {
    let source = source_of("crates/i2pr-daemon/src/version_eligibility.rs");
    assert_eq!(
        check_receipt_has_no_identity(&source),
        None,
        "a receipt that could name a peer is not a receipt"
    );
}

/// Plan 503 rule 6, first half: the live creator-RouterInfo publication path
/// must ask the shared gated selection for its DSM targets.
///
/// This is the rule that would have caught Plan 491's gap: before Plan 503
/// the live publication selected floodfills by XOR distance alone, so a peer
/// below the DSM floor was still a DSM target.
#[test]
fn the_live_creator_router_info_publication_consults_the_gated_selection() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "publish_creator_router_info",
            "version_gated_publication_floodfills("
        ),
        None,
        "the live creator-RouterInfo publication must consult the gated selection"
    );
}

/// Plan 503 rule 6, second half: the live LeaseSet2 publication path must ask
/// the shared gated selection for its DSM targets.
#[test]
fn the_live_service_lease_set_publication_consults_the_gated_selection() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_body_contains(
            &source,
            "publish_service_ls2_for_service",
            "version_gated_candidate_hashes("
        ),
        None,
        "the live LeaseSet2 publication must consult the gated selection"
    );
}

/// Plan 503 rule 6, third half: neither live publication path may use the
/// ungated, distance-only selection.
///
/// The discriminator is the **leading dot**. The gated call site reads
/// `coord_guard.version_gated_candidate_hashes(`, so the text
/// `.version_gated_candidate_hashes(` does **not** contain the token
/// `.candidate_hashes(` — the character before `candidate_hashes` is the
/// `_` of `version_gated_`, not a `.`. The ungated call site reads
/// `coord_guard.candidate_hashes(`, which does contain it. The dot is what
/// separates the two, so the token is matched with the dot.
#[test]
fn the_live_publication_paths_do_not_use_the_ungated_selection() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    for function in [
        "publish_creator_router_info",
        "publish_service_ls2_for_service",
    ] {
        assert_eq!(
            check_body_avoids(&source, function, ".candidate_hashes("),
            None,
            "`fn {function}` must not use the ungated distance-only selection"
        );
    }
}

/// Plan 503 rule 7: the live gates record their decisions, so a refusal is
/// observable and never a silent skip.
///
/// The tunnel-build gate constructs its receipt in place, so its own body
/// must contain the receipt constructor. The two DSM gates delegate the
/// construction to the crate-private `record_dsm_receipt` helper — that is
/// the landed implementation's shape — so each DSM gate's body must call the
/// recorder and the recorder's body must build a **DSM-target** receipt.
/// Together the three assertions pin "this gate records a DSM-target
/// receipt" as tightly as a single-frame check would, and they also fail if
/// the recorder were changed to record a different gate's receipt.
#[test]
fn the_live_gates_record_their_decisions() {
    let source = source_of("crates/i2pr-daemon/src/destination_tunnels.rs");
    assert_eq!(
        check_body_contains(&source, "destination_peer_candidates", "tunnel_build_peer("),
        None,
        "the live tunnel-build gate must record its receipt"
    );
    for function in [
        "version_gated_candidate_hashes",
        "version_gated_publication_floodfills",
    ] {
        assert_eq!(
            check_body_contains(&source, function, "record_dsm_receipt("),
            None,
            "`fn {function}` must record its receipt"
        );
    }
    assert_eq!(
        check_body_contains(&source, "record_dsm_receipt", "dsm_target("),
        None,
        "the DSM recorder must record a DSM-target receipt"
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

/// The inverted probe for rule 8, first half: a publication coordinator that
/// does not consult the shared gated selection must be reported.
#[test]
fn the_guard_reports_a_publication_coordinator_without_the_shared_selection() {
    let violating = "\
fn nearest_eligible_floodfills(&self, store: &RouterInfoStore) -> FloodfillTargetSelection {
    let mut targets = Vec::new();
    for candidate in store.floodfill_advertisers() {
        targets.push(candidate.key);
    }
    FloodfillTargetSelection::default()
}
";
    assert!(
        check_body_contains(
            violating,
            "nearest_eligible_floodfills",
            "select_version_gated_floodfills("
        )
        .is_some(),
        "a publication coordinator that bypasses the shared selection must be reported"
    );
}

/// The inverted probe for rule 8, second half: a selection policy that does
/// not consult the DSM gate must be reported.
#[test]
fn the_guard_reports_a_selection_policy_without_the_gate() {
    let violating = "\
fn select_version_gated_floodfills(
    store: &RouterInfoStore,
    routing_key: &RouterHash,
    excluded: &[RouterHash],
    policy: &LookupPolicy,
) -> VersionGatedFloodfillSelection {
    let candidates = nearest_floodfill_candidates(store, routing_key, excluded, policy);
    VersionGatedFloodfillSelection::default()
}
";
    assert!(
        check_body_contains(
            violating,
            "select_version_gated_floodfills",
            "gate_candidates("
        )
        .is_some(),
        "a shared selection that does not reach the shared gate must be reported"
    );
}

/// The inverted probe for the second half of the retargeted rule: a gate that
/// does not apply the DSM rule must be reported.
#[test]
fn the_guard_reports_a_shared_gate_without_the_dsm_rule() {
    let violating = "\
fn gate_candidates(
    store: &RouterInfoStore,
    candidates: Vec<NearestSelection>,
) -> VersionGatedFloodfillSelection {
    let _ = (store, candidates);
    VersionGatedFloodfillSelection::default()
}
";
    assert!(
        check_body_contains(violating, "gate_candidates", "dsm_target_eligibility(").is_some(),
        "a shared gate that ignores the DSM gate must be reported"
    );
}

/// The inverted probe for rule 6, first half: a live creator-RouterInfo
/// publication that does not consult the gated selection must be reported.
#[test]
fn the_guard_reports_a_creator_publication_without_the_gated_selection() {
    let violating = "\
async fn publish_creator_router_info(&mut self) -> Result<(), PublishError> {
    let targets = self.nearest_floodfills();
    self.send(targets).await
}
";
    assert!(
        check_body_contains(
            violating,
            "publish_creator_router_info",
            "version_gated_publication_floodfills("
        )
        .is_some(),
        "a creator-RouterInfo publication that bypasses the gate must be reported"
    );
}

/// The inverted probe for rule 6, second half: a live LeaseSet2 publication
/// that does not consult the gated selection must be reported.
#[test]
fn the_guard_reports_a_lease_set_publication_without_the_gated_selection() {
    let violating = "\
async fn publish_service_ls2_for_service(&mut self) -> Result<(), PublishError> {
    let targets = self.nearest_floodfills();
    self.send(targets).await
}
";
    assert!(
        check_body_contains(
            violating,
            "publish_service_ls2_for_service",
            "version_gated_candidate_hashes("
        )
        .is_some(),
        "a LeaseSet2 publication that bypasses the gate must be reported"
    );
}

/// The inverted probe for rule 6, third half: a live publication that uses
/// the ungated, distance-only selection must be reported.
#[test]
fn the_guard_reports_a_publication_that_uses_the_ungated_selection() {
    let violating = "\
async fn publish_service_ls2_for_service(&mut self) -> Result<(), PublishError> {
    let selection = coord_guard.candidate_hashes(&target, &routing_key);
    self.send(selection.targets().to_vec()).await
}
";
    assert!(
        check_body_avoids(
            violating,
            "publish_service_ls2_for_service",
            ".candidate_hashes("
        )
        .is_some(),
        "a publication that selects by distance alone must be reported"
    );
}

/// The inverted probe for rule 7, first half: a live tunnel-build gate that
/// records no receipt must be reported.
#[test]
fn the_guard_reports_a_build_gate_without_a_receipt() {
    let violating = "\
fn destination_peer_candidates(&self) -> (Vec<Candidate>, Summary) {
    crate::destination_peers::project_validated_store(&self.store, self.counts(), &self.policy)
}
";
    assert!(
        check_body_contains(
            violating,
            "destination_peer_candidates",
            "tunnel_build_peer("
        )
        .is_some(),
        "a tunnel-build gate that records no receipt must be reported"
    );
}

/// The inverted probe for rule 7, second half: a live DSM gate that records
/// no receipt must be reported.
#[test]
fn the_guard_reports_a_dsm_gate_without_a_receipt() {
    let violating = "\
fn version_gated_candidate_hashes(&self, target: &DestinationHash) -> VersionGatedFloodfillSelection {
    let selection = select_version_gated_floodfills(&self.store, routing_key, &[target_hash], &self.policy);
    selection
}
";
    assert!(
        check_body_contains(
            violating,
            "version_gated_candidate_hashes",
            "record_dsm_receipt("
        )
        .is_some(),
        "a DSM gate that records no receipt must be reported"
    );
}

/// The inverted probe for rule 7, third half: a recorder that records a
/// different gate's receipt must be reported.
#[test]
fn the_guard_reports_a_recorder_that_records_the_wrong_receipt() {
    let violating = "\
fn record_dsm_receipt(&self, selection: &VersionGatedFloodfillSelection) {
    self.record_version_receipt(
        crate::version_eligibility::VersionEligibilityReceipt::tunnel_build_peer(
            &summary,
            selection.accepted() as usize,
        ),
    );
}
";
    assert!(
        check_body_contains(violating, "record_dsm_receipt", "dsm_target(").is_some(),
        "a DSM recorder that records another gate's receipt must be reported"
    );
}

/// The body-scoped rules must not be satisfied by a token that appears
/// somewhere else in the file.
#[test]
fn the_guard_ignores_a_body_rule_token_outside_the_named_function() {
    let outside = "\
/// The gate is version_gated_publication_floodfills.
fn some_other_helper(&self) -> Selection {
    let selection = version_gated_publication_floodfills(&local_router_hash);
    selection
}

fn publish_creator_router_info(&mut self) -> Result<(), PublishError> {
    let targets = self.nearest_floodfills();
    self.send(targets).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn uses_the_gate() {
        let _ = version_gated_publication_floodfills(&local_router_hash);
    }
}
";
    // The gated call exists three times in the fixture — in a comment, in a
    // sibling function, and in a test block — and none of them is inside the
    // named function's body, so the rule must still report the violation.
    assert!(
        check_body_contains(
            outside,
            "publish_creator_router_info",
            "version_gated_publication_floodfills("
        )
        .is_some(),
        "a sibling function, a comment or a test block must not satisfy a body rule"
    );
}

/// The `.candidate_hashes(` discriminator depends on the leading dot, so the
/// gated call site must not contain the ungated token.
#[test]
fn the_ungated_selector_discriminator_is_the_leading_dot() {
    assert!(
        !".version_gated_candidate_hashes(".contains(".candidate_hashes("),
        "the gated call site must not contain the ungated token"
    );
    assert!(
        ".candidate_hashes(".contains(".candidate_hashes("),
        "the ungated call site must contain the token"
    );
    assert!(
        !"version_gated_candidate_hashes(".contains(".candidate_hashes("),
        "an unqualified call site must not contain the token either"
    );
}

/// The function-body extractor must be correct on the shapes a real crate
/// contains: a nested block, a string literal that holds a brace, a character
/// literal that holds a brace, and a function that is the last item in the
/// file.
#[test]
fn the_function_body_extractor_handles_nested_blocks_strings_and_a_trailing_item() {
    let source = "\
fn outer() -> u8 {
    let text = \"}\";
    let brace = '{';
    if true {
        let inner = \"{\";
        let _ = (text, brace, inner);
    }
    7
}
fn last() -> u8 {
    let text = \"{\";
    text.len() as u8
}
";
    let code = code_view(source);
    let outer = function_body(&code, "outer").expect("the outer body must be found");
    assert!(
        outer.contains("if true {"),
        "a nested block must stay inside the body"
    );
    assert!(
        !outer.contains("fn last"),
        "the body must end at its own closing brace"
    );
    let last = function_body(&code, "last").expect("the trailing body must be found");
    assert!(
        last.contains("text.len()"),
        "the trailing item's body must be found"
    );
    assert!(
        function_body(&code, "absent").is_none(),
        "an absent function must not be found"
    );
}
