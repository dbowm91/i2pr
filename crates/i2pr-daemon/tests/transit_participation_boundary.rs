//! Plan 492 static boundary guard for the transit participation posture.
//!
//! Plan 492's invariants are not only behavioural; some of them are
//! *structural*, and a behavioural test cannot see a structural drift:
//!
//! 1. **Opt-in only, and default off.** The posture's switch is one boolean
//!    that defaults to `false`, and the default configuration does not
//!    change. A section that defaulted to `true` would put every router on
//!    the public network's relay set without an operator decision.
//! 2. **The dispatch order is unchanged.** *Creator/service first, transit
//!    second, unknown fail-closed.* The participation consultation must
//!    sit **after** the creator correlation check and **before** the
//!    transit dispatch, so the transit path never takes priority over the
//!    router's own service traffic and a refused request never allocates a
//!    registration.
//! 3. **The share limit is enforced, not advisory.** The governor's permit
//!    is revalidated at the point of use, and the share is charged before
//!    the cell leaves the router.
//! 4. **Revocation is prompt and observable.** A stop is counted and
//!    recorded; a stop that leaves no trace is not a revocation.
//! 5. **Bounded everything.** The receipt log, the admitted-peer record,
//!    and every limit are bounded by named constants.
//! 6. **No secret in receipts.** A receipt carries counts only: no peer
//!    identity, no address, no tunnel id, no key, no payload.
//! 7. **The selection diversity is applied in the ordinary path.** The /16
//!    rule and the per-peer participation cap come from Plan 488's policy,
//!    and a selection that is never recorded would leave the cap
//!    advisory.
//! 8. **The role is not advertised.** The published capability string and
//!    the support inventory still say the transit role is disabled,
//!    non-advertised, and unclaimed.
//!
//! The guard is deliberately **fail-closed**: it strips comments before
//! matching (so a doc comment that merely names a construct is not a
//! violation), it excludes `#[cfg(test)]` blocks, and every rule carries its
//! own **inverted probe** — a synthetic violating source must be reported.
//! If a rule ever stops detecting its violation, its probe fails.

#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The tokens that would put a peer identity into a participation receipt.
const RECEIPT_IDENTITY_TOKENS: &[&str] = &[
    "RouterHash",
    "RouterIdentity",
    "router_hash",
    "peer_hash",
    "Hash",
    "address",
    "tunnel_id",
    "session_key",
    "payload",
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

/// Extracts the body of a named item from a code view.
///
/// The extraction is brace-scoped and fails closed: an item that cannot be
/// located yields an empty body, which every rule then reports.
fn item_body(code: &str, signature: &str) -> Option<String> {
    let start = code.find(signature)?;
    let open = code[start..].find('{')? + start;
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut cursor = open;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(code[open..=cursor].to_owned());
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

/// Rule 1: the posture's switch defaults to off.
fn check_participation_defaults_off(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "struct RawTransitConfig") else {
        return Some("the `[transit]` configuration section must exist".to_owned());
    };
    let Some(switch) = body.find("participation: bool") else {
        return Some("the `[transit]` section must carry a `participation` switch".to_owned());
    };
    let head = &body[..switch];
    let Some(default) = head.rfind("#[serde(default)]") else {
        return Some(
            "the participation switch must carry `#[serde(default)]` so it defaults to off".to_owned(),
        );
    };
    let between = &head[default..];
    if between.matches("#[serde(default").count() == 1 {
        None
    } else {
        Some("the participation switch must be the defaulted field".to_owned())
    }
}

/// Rule 2: the participation consultation sits between the creator
/// correlation check and the transit dispatch.
fn check_dispatch_order(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "fn dispatch_build_body") else {
        return Some("the live owner's short-build dispatch must exist".to_owned());
    };
    let Some(creator) = body.find("creator_builds.contains") else {
        return Some("the creator correlation check must precede the transit path".to_owned());
    };
    let Some(participation) = body.find("refuse_unless_participating") else {
        return Some("the dispatch must consult the participation posture".to_owned());
    };
    let Some(dispatch) = body.find("dispatch_short_build") else {
        return Some("the transit dispatch must exist".to_owned());
    };
    if creator < participation && participation < dispatch {
        None
    } else {
        Some(
            "the order must be creator correlation, then the participation posture, then the transit dispatch"
                .to_owned(),
        )
    }
}

/// Rule 3: the share is charged before the cell leaves the router, and the
/// permit is revalidated at the point of use.
fn check_share_is_enforced(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "fn handle_tunnel_data_inner") else {
        return Some("the live owner's tunnel-data path must exist".to_owned());
    };
    let Some(revalidate) = body.find("participation.revalidate()") else {
        return Some("the point of use must revalidate the governor's permit".to_owned());
    };
    let Some(charge) = body.find("charge_relay") else {
        return Some("the point of use must charge the share".to_owned());
    };
    let Some(deliver) = body.find("deliver_tunnel_data_forward") else {
        return Some("the forward delivery must exist".to_owned());
    };
    if revalidate < charge && charge < deliver {
        None
    } else {
        Some(
            "the share must be revalidated and charged before the cell is forwarded".to_owned(),
        )
    }
}

/// Rule 4: a stop is counted and recorded.
fn check_revocation_is_recorded(source: &str) -> Option<String> {
    let code = code_view(source);
    if code.contains("TransitReceiptKind::Stopped") && code.contains("fn revocation(") {
        None
    } else {
        Some("a revocation that leaves no recorded stop is not a revocation".to_owned())
    }
}

/// Rule 5: every unbounded surface is bounded by a named constant.
fn check_everything_is_bounded(source: &str) -> Option<String> {
    let code = code_view(source);
    for constant in [
        "MAX_TRANSIT_PARTICIPATION_RECEIPTS",
        "MAX_TRANSIT_PARTICIPATION_TRACKED_PEERS",
        "TRANSIT_SHARE_WINDOW_SECONDS",
    ] {
        if !code.contains(constant) {
            return Some(format!("the participation owner must bound {constant}"));
        }
    }
    None
}

/// Rule 6: a receipt carries counts only.
fn check_receipt_has_no_identity(source: &str) -> Option<String> {
    let code = code_view(source);
    let Some(body) = item_body(&code, "pub struct TransitParticipationReceipt") else {
        return Some("the participation receipt type must exist".to_owned());
    };
    for token in RECEIPT_IDENTITY_TOKENS {
        if body.contains(token) {
            return Some(format!("a receipt may not carry a peer identity ({token})"));
        }
    }
    None
}

/// Rule 7: the ordinary hop selection honours Plan 488's diversity rules.
fn check_selection_diversity(source: &str) -> Option<String> {
    let code = code_view(source);
    for token in ["max_per_prefix16", "max_participation", "prefix16"] {
        if !code.contains(token) {
            return Some(format!(
                "the ordinary build-hop selection must honour Plan 488's {token} rule"
            ));
        }
    }
    None
}

/// Rule 8: a selection is recorded, so the participation cap is not
/// advisory.
fn check_selection_is_recorded(source: &str) -> Option<String> {
    let code = code_view(source);
    if code.contains("record_hop_selection") {
        None
    } else {
        Some("a build-hop selection that is never recorded leaves the cap advisory".to_owned())
    }
}

/// Rule 9: the transit role is not advertised.
fn check_role_is_not_advertised(source: &str) -> Option<String> {
    if !source.contains("public transit remains disabled, non-advertised, and unclaimed") {
        return Some("the support inventory must still record the transit role as disabled, non-advertised, and unclaimed".to_owned());
    }
    let Some(surface) = source.find("id = \"daemon.transit-participation-posture\"") else {
        return Some("the participation posture must have a support-inventory surface".to_owned());
    };
    let tail = &source[surface..];
    let end = tail.find("\n[[surface]]").unwrap_or(tail.len());
    let entry = &tail[..end];
    if entry.contains("advertised = false") {
        None
    } else {
        Some("the participation posture must not be advertised".to_owned())
    }
}

#[test]
fn the_posture_switch_defaults_off() {
    let source = source_of("crates/i2pr-daemon/src/config.rs");
    assert_eq!(
        check_participation_defaults_off(&source),
        None,
        "the participation posture must be opt-in and default off"
    );
}

#[test]
fn the_dispatch_order_is_unchanged() {
    let source = source_of("crates/i2pr-daemon/src/transit_owner.rs");
    assert_eq!(
        check_dispatch_order(&source),
        None,
        "creator/service first, transit second, unknown fail-closed"
    );
}

#[test]
fn the_share_is_enforced_at_the_point_of_use() {
    let source = source_of("crates/i2pr-daemon/src/transit_owner.rs");
    assert_eq!(
        check_share_is_enforced(&source),
        None,
        "the share limit must be enforced, not advisory"
    );
}

#[test]
fn a_revocation_is_recorded() {
    let source = source_of("crates/i2pr-daemon/src/transit_participation.rs");
    assert_eq!(
        check_revocation_is_recorded(&source),
        None,
        "revocation must be prompt and observable"
    );
}

#[test]
fn every_participation_surface_is_bounded() {
    let source = source_of("crates/i2pr-daemon/src/transit_participation.rs");
    assert_eq!(
        check_everything_is_bounded(&source),
        None,
        "bounded everything"
    );
}

#[test]
fn the_participation_receipt_carries_no_peer_identity() {
    let source = source_of("crates/i2pr-daemon/src/transit_participation.rs");
    assert_eq!(
        check_receipt_has_no_identity(&source),
        None,
        "a receipt that could name a peer is not a receipt"
    );
}

#[test]
fn the_ordinary_hop_selection_honours_the_diversity_rules() {
    let source = source_of("crates/i2pr-daemon/src/destination_peers.rs");
    assert_eq!(
        check_selection_diversity(&source),
        None,
        "the router's own tunnel hops must satisfy the /16 rule and the participation cap"
    );
}

#[test]
fn a_build_hop_selection_is_recorded() {
    let source = source_of("crates/i2pr-daemon/src/service_product.rs");
    assert_eq!(
        check_selection_is_recorded(&source),
        None,
        "an unrecorded selection leaves the participation cap advisory"
    );
}

#[test]
fn the_transit_role_is_not_advertised() {
    let source = source_of("specs/support.toml");
    assert_eq!(
        check_role_is_not_advertised(&source),
        None,
        "the transit role must stay disabled, non-advertised, and unclaimed"
    );
}

/// The inverted probe for rule 1: a switch that defaults on must be
/// reported.
#[test]
fn the_guard_reports_a_switch_that_defaults_on() {
    let violating = "\
struct RawTransitConfig {
    participation: bool,
}
";
    assert!(
        check_participation_defaults_off(violating).is_some(),
        "a participation switch without a default must be reported"
    );
}

/// The inverted probe for rule 2: a dispatch that consults the posture
/// before the creator check must be reported.
#[test]
fn the_guard_reports_a_reordered_dispatch() {
    let reordered = "\
fn dispatch_build_body(&mut self) {
    if let Some(reason) = self.refuse_unless_participating(&peer) {
        return;
    }
    if self.creator_builds.contains(&key) {
        return;
    }
    let dispatch = gate.dispatch_short_build(body, peer, now_seconds, false, rng);
}
";
    assert!(
        check_dispatch_order(reordered).is_some(),
        "a reordered dispatch must be reported"
    );
}

/// The inverted probe for rule 3: a share that is charged after the
/// forward must be reported.
#[test]
fn the_guard_reports_a_share_charged_after_the_forward() {
    let violating = "\
fn handle_tunnel_data_inner(&mut self) {
    let delivery = service.deliver_tunnel_data_forward(&next_router, &next_cell, message_id, &self.cancellation);
    self.participation.owner_mut().charge_relay(bytes, now_ms / 1000);
}
";
    assert!(
        check_share_is_enforced(violating).is_some(),
        "a share charged after the forward must be reported"
    );
}

/// The inverted probe for rule 4: a revocation that leaves no record must
/// be reported.
#[test]
fn the_guard_reports_an_unrecorded_revocation() {
    let violating = "\
pub fn revoke(&mut self, reason: TransitRevocationReason) {
    self.revoked = Some(reason);
}
";
    assert!(
        check_revocation_is_recorded(violating).is_some(),
        "an unrecorded revocation must be reported"
    );
}

/// The inverted probe for rule 5: an unbounded receipt log must be
/// reported.
#[test]
fn the_guard_reports_an_unbounded_receipt_log() {
    let violating = "\
pub fn receipts(&self) -> Vec<TransitParticipationReceipt> {
    self.receipts.iter().copied().collect()
}
";
    assert!(
        check_everything_is_bounded(violating).is_some(),
        "an unbounded receipt log must be reported"
    );
}

/// The inverted probe for rule 6: a receipt that names a peer must be
/// reported.
#[test]
fn the_guard_reports_a_receipt_carrying_an_identity() {
    let violating = "\
pub struct TransitParticipationReceipt {
    pub kind: TransitReceiptKind,
    pub peer: RouterHash,
    pub bytes_relayed: u64,
}
";
    assert!(
        check_receipt_has_no_identity(violating).is_some(),
        "a receipt carrying an identity must be reported"
    );
}

/// The inverted probe for rule 7: an ordinary selection without the
/// diversity rules must be reported.
#[test]
fn the_guard_reports_a_selection_without_the_diversity_rules() {
    let violating = "\
fn select(candidates: &[DestinationPeerCandidate]) -> Vec<DestinationPeerCandidate> {
    candidates.to_vec()
}
";
    assert!(
        check_selection_diversity(violating).is_some(),
        "a selection without the diversity rules must be reported"
    );
}

/// The inverted probe for rule 8: an unrecorded selection must be
/// reported.
#[test]
fn the_guard_reports_an_unrecorded_selection() {
    let violating = "\
fn build() {
    let selected = select_projected_destination_path_os(&candidates, &summary)?;
}
";
    assert!(
        check_selection_is_recorded(violating).is_some(),
        "an unrecorded selection must be reported"
    );
}

/// The inverted probe for rule 9: an advertised posture must be reported.
#[test]
fn the_guard_reports_an_advertised_posture() {
    let violating = "\
m11_transit_tunnels = \"Plan 268 passed one-family experimental progression; public transit remains disabled, non-advertised, and unclaimed.\"

[[surface]]
id = \"daemon.transit-participation-posture\"
status = \"experimental\"
advertised = true
";
    assert!(
        check_role_is_not_advertised(violating).is_some(),
        "an advertised posture must be reported"
    );
}

/// The guard must not report a construct that only appears in a comment or in
/// a test block.
#[test]
fn the_guard_ignores_comments_and_test_blocks() {
    let commented = "\
//! The posture switch is `participation: bool`.
//! A `#[serde(default)]` annotation is documented here.
struct RawTransitConfig {
    /// The switch.
    #[serde(default)]
    participation: bool,
}
";
    assert_eq!(
        check_participation_defaults_off(commented),
        None,
        "a comment must not change the verdict"
    );

    let in_test = "\
struct RawTransitConfig {
    participation: bool,
}

#[cfg(test)]
mod tests {
    #[test]
    fn uses_the_switch() {
        let _ = participation: bool;
    }
}
";
    assert_eq!(
        check_participation_defaults_off(in_test),
        check_participation_defaults_off("\
struct RawTransitConfig {
    participation: bool,
}
"),
        "a test block must not change the verdict"
    );
}
