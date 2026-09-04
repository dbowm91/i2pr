//! Deterministic NTCP2/SSU2 transport selection and fallback (Plan 159).
//!
//! This module extends the runtime-neutral manager/policy layer so NTCP2
//! and SSU2 candidates are selected through one deterministic policy.
//! The daemon never asks protocol crates directly which transport to
//! use: callers structurally validate candidate RouterAddresses inside
//! the per-transport crates first, then describe the survivors with the
//! small [`TransportCandidate`] descriptor below and call
//! [`select_peer_transport`].
//!
//! Selection order (plan §8):
//!
//! 1. an existing authenticated usable link to the peer is reused;
//! 2. candidates must be structurally validated per transport
//!    (the caller sets [`TransportCandidate::valid`] only after the
//!    owning crate accepted the address);
//! 3. per-transport dial backoff/health excludes backed-off transports;
//! 4. direct/reachable candidates sort ahead of introducer-only ones;
//! 5. configured transport enablement excludes disabled transports;
//! 6. a deterministic tie-break orders whatever remains;
//! 7. peer-wide limits deny new dials through
//!    [`SelectionOutcome::ResourceDenied`].
//!
//! Address-specific failures never poison a whole transport: the caller
//! passes the opaque tags of failed addresses as `failed_tags` and only
//! those exact candidates are excluded. Transports are excluded only by
//! explicit per-transport backoff or disablement.
//!
//! Normative traceability: `plans/159-m8-ssu2-path-validation-`
//! `publication-and-transport-selection.md` §§8–9. No sockets, no Tokio,
//! no async traits; every contract is a concrete struct/enum like the
//! rest of this crate.

use crate::types::{AddressFamily, LinkId, TransportKind};

/// Maximum candidates examined for one peer selection.
///
/// Over-bound input is denied with [`SelectionOutcome::ResourceDenied`],
/// never silently truncated.
pub const MAX_SELECTION_CANDIDATES: usize = 16;

/// One structurally validated dial candidate described without
/// importing protocol-crate address types.
///
/// This crate sits below `i2pr-transport-ntcp2` and
/// `i2pr-transport-ssu2` in the dependency direction, so per-transport
/// parsing stays in those crates. The caller maps each accepted
/// RouterAddress to one descriptor: `transport` names the owner,
/// `family` classifies the literal address, `direct` is true for a
/// direct/reachable endpoint and false for introducer-only material,
/// `tag` is an opaque caller-chosen tie-break value (for example a
/// truncated hash of the address bytes), and `valid` records that the
/// owning crate structurally accepted the address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransportCandidate {
    transport: TransportKind,
    family: AddressFamily,
    direct: bool,
    tag: u64,
    valid: bool,
}

impl TransportCandidate {
    /// Describes one validated candidate address.
    pub const fn new(
        transport: TransportKind,
        family: AddressFamily,
        direct: bool,
        tag: u64,
        valid: bool,
    ) -> Self {
        Self {
            transport,
            family,
            direct,
            tag,
            valid,
        }
    }

    /// Returns the owning transport.
    pub const fn transport(self) -> TransportKind {
        self.transport
    }

    /// Returns the literal address family.
    pub const fn family(self) -> AddressFamily {
        self.family
    }

    /// Returns whether this is a direct endpoint (false = introducer-only).
    pub const fn direct(self) -> bool {
        self.direct
    }

    /// Returns the opaque caller tie-break tag.
    pub const fn tag(self) -> u64 {
        self.tag
    }

    /// Returns whether the owning crate structurally accepted the address.
    pub const fn valid(self) -> bool {
        self.valid
    }
}

/// One existing authenticated usable link to the selection peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExistingLink {
    transport: TransportKind,
    link: LinkId,
}

impl ExistingLink {
    /// Describes one usable authenticated link.
    pub const fn new(transport: TransportKind, link: LinkId) -> Self {
        Self { transport, link }
    }

    /// Returns the link transport.
    pub const fn transport(self) -> TransportKind {
        self.transport
    }

    /// Returns the local link identifier.
    pub const fn link(self) -> LinkId {
        self.link
    }
}

/// Configured transport enablement and deterministic tie-break policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionPolicy {
    ntcp2_enabled: bool,
    ssu2_enabled: bool,
    prefer_ssu2: bool,
    allow_introducer_only: bool,
}

impl SelectionPolicy {
    /// Builds an explicit policy without reading configuration.
    pub const fn new(
        ntcp2_enabled: bool,
        ssu2_enabled: bool,
        prefer_ssu2: bool,
        allow_introducer_only: bool,
    ) -> Self {
        Self {
            ntcp2_enabled,
            ssu2_enabled,
            prefer_ssu2,
            allow_introducer_only,
        }
    }

    /// Returns whether NTCP2 dials are permitted.
    pub const fn ntcp2_enabled(self) -> bool {
        self.ntcp2_enabled
    }

    /// Returns whether SSU2 dials are permitted.
    pub const fn ssu2_enabled(self) -> bool {
        self.ssu2_enabled
    }

    /// Returns whether SSU2 wins transport ties (false prefers NTCP2).
    pub const fn prefer_ssu2(self) -> bool {
        self.prefer_ssu2
    }

    /// Returns whether introducer-only candidates may be dialed.
    pub const fn allow_introducer_only(self) -> bool {
        self.allow_introducer_only
    }

    /// Returns whether a transport may be dialed under this policy.
    pub const fn transport_enabled(self, transport: TransportKind) -> bool {
        match transport {
            TransportKind::Ntcp2 => self.ntcp2_enabled,
            TransportKind::Ssu2 => self.ssu2_enabled,
        }
    }
}

impl Default for SelectionPolicy {
    fn default() -> Self {
        Self {
            ntcp2_enabled: true,
            ssu2_enabled: true,
            prefer_ssu2: true,
            allow_introducer_only: false,
        }
    }
}

/// The deterministic selection decision for one peer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionOutcome {
    /// Reuse an existing authenticated link instead of dialing.
    Reuse {
        /// The retained local link identifier.
        link: LinkId,
        /// The retained link transport.
        transport: TransportKind,
    },
    /// Dial one candidate address.
    Dial {
        /// The selected transport.
        transport: TransportKind,
        /// The opaque caller tag of the selected candidate.
        tag: u64,
    },
    /// Dial the primary; fall back to the secondary on failure.
    ///
    /// Primary and secondary always name different transports.
    DialFallback {
        /// The first transport to attempt.
        primary_transport: TransportKind,
        /// The opaque caller tag of the primary candidate.
        primary_tag: u64,
        /// The fallback transport to attempt after primary failure.
        secondary_transport: TransportKind,
        /// The opaque caller tag of the secondary candidate.
        secondary_tag: u64,
    },
    /// No candidate survived structural/policy filtering.
    NoCompatibleAddress,
    /// Every surviving candidate sits in per-transport backoff.
    BackedOff,
    /// A bound (peer limit or over-bound input) denied a new dial.
    ResourceDenied,
}

/// Selects the transport for one peer deterministically.
///
/// - `existing` lists the peer's authenticated usable links (reuse
///   takes precedence over any redial).
/// - `candidates` lists structurally described dial candidates.
/// - `failed_tags` lists opaque tags of address-specific failures to
///   exclude without poisoning sibling candidates or transports.
/// - `backed_off` lists transports currently in dial backoff.
/// - `peer_link_limit_reached` reports the peer-wide active-link
///   ceiling; new dials are denied while set.
/// - `policy` carries enablement and tie-break inputs.
///
/// Deterministic inputs always produce the same outcome: reuse prefers
/// the policy-preferred transport then the lowest link identifier;
/// dials prefer direct candidates, then the policy-preferred
/// transport, then the lowest tag, then family order.
pub fn select_peer_transport(
    existing: &[ExistingLink],
    candidates: &[TransportCandidate],
    failed_tags: &[u64],
    backed_off: &[TransportKind],
    policy: SelectionPolicy,
    peer_link_limit_reached: bool,
) -> SelectionOutcome {
    if candidates.len() > MAX_SELECTION_CANDIDATES {
        return SelectionOutcome::ResourceDenied;
    }
    if let Some(reuse) = select_reuse(existing, policy) {
        return reuse;
    }
    if peer_link_limit_reached {
        return SelectionOutcome::ResourceDenied;
    }
    let mut eligible: Vec<(TransportCandidate, bool)> = Vec::new();
    let mut saw_backed_off = false;
    for candidate in candidates {
        if !candidate.valid() {
            continue;
        }
        if candidate.family() == AddressFamily::Unknown {
            continue;
        }
        if !policy.transport_enabled(candidate.transport()) {
            continue;
        }
        if failed_tags.contains(&candidate.tag()) {
            continue;
        }
        if !candidate.direct() && !policy.allow_introducer_only() {
            continue;
        }
        if backed_off.contains(&candidate.transport()) {
            saw_backed_off = true;
            continue;
        }
        eligible.push((*candidate, candidate.direct()));
    }
    if eligible.is_empty() {
        if saw_backed_off {
            return SelectionOutcome::BackedOff;
        }
        return SelectionOutcome::NoCompatibleAddress;
    }
    eligible.sort_by(|left, right| {
        // Direct candidates sort ahead of introducer-only ones.
        right
            .1
            .cmp(&left.1)
            .then_with(|| {
                transport_rank(policy, left.0.transport())
                    .cmp(&transport_rank(policy, right.0.transport()))
            })
            .then_with(|| left.0.tag().cmp(&right.0.tag()))
            .then_with(|| left.0.family().cmp(&right.0.family()))
    });
    let primary = eligible[0].0;
    let secondary = eligible
        .iter()
        .map(|(candidate, _)| *candidate)
        .find(|candidate| candidate.transport() != primary.transport());
    match secondary {
        Some(next) => SelectionOutcome::DialFallback {
            primary_transport: primary.transport(),
            primary_tag: primary.tag(),
            secondary_transport: next.transport(),
            secondary_tag: next.tag(),
        },
        None => SelectionOutcome::Dial {
            transport: primary.transport(),
            tag: primary.tag(),
        },
    }
}

fn select_reuse(existing: &[ExistingLink], policy: SelectionPolicy) -> Option<SelectionOutcome> {
    let mut ordered: Vec<ExistingLink> = existing.to_vec();
    if ordered.len() > MAX_SELECTION_CANDIDATES {
        return Some(SelectionOutcome::ResourceDenied);
    }
    if ordered.is_empty() {
        return None;
    }
    ordered.sort_by(|left, right| {
        transport_rank(policy, left.transport())
            .cmp(&transport_rank(policy, right.transport()))
            .then_with(|| left.link().value().cmp(&right.link().value()))
    });
    let winner = ordered[0];
    Some(SelectionOutcome::Reuse {
        link: winner.link(),
        transport: winner.transport(),
    })
}

const fn transport_rank(policy: SelectionPolicy, transport: TransportKind) -> u8 {
    let ssu2_first = policy.prefer_ssu2();
    match (transport, ssu2_first) {
        (TransportKind::Ssu2, true) | (TransportKind::Ntcp2, false) => 0,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(transport: TransportKind, id: u64) -> ExistingLink {
        ExistingLink::new(transport, LinkId::new(id).expect("id"))
    }

    fn candidate(transport: TransportKind, direct: bool, tag: u64) -> TransportCandidate {
        TransportCandidate::new(transport, AddressFamily::Ipv4, direct, tag, true)
    }

    #[test]
    fn active_ssu2_link_is_reused_for_ssu2_capable_peer() {
        let existing = vec![link(TransportKind::Ssu2, 7)];
        let candidates = vec![
            candidate(TransportKind::Ssu2, true, 1),
            candidate(TransportKind::Ntcp2, true, 2),
        ];
        assert_eq!(
            select_peer_transport(
                &existing,
                &candidates,
                &[],
                &[],
                SelectionPolicy::default(),
                false,
            ),
            SelectionOutcome::Reuse {
                link: LinkId::new(7).expect("id"),
                transport: TransportKind::Ssu2,
            }
        );
    }

    #[test]
    fn active_ntcp2_link_is_reused_rather_than_dialing_ssu2() {
        let existing = vec![link(TransportKind::Ntcp2, 3)];
        let candidates = vec![candidate(TransportKind::Ssu2, true, 1)];
        assert_eq!(
            select_peer_transport(
                &existing,
                &candidates,
                &[],
                &[],
                SelectionPolicy::default(),
                false,
            ),
            SelectionOutcome::Reuse {
                link: LinkId::new(3).expect("id"),
                transport: TransportKind::Ntcp2,
            }
        );
    }

    #[test]
    fn reuse_prefers_policy_transport_then_lowest_link_id() {
        let existing = vec![
            link(TransportKind::Ntcp2, 1),
            link(TransportKind::Ssu2, 9),
            link(TransportKind::Ssu2, 2),
        ];
        let outcome =
            select_peer_transport(&existing, &[], &[], &[], SelectionPolicy::default(), false);
        assert_eq!(
            outcome,
            SelectionOutcome::Reuse {
                link: LinkId::new(2).expect("id"),
                transport: TransportKind::Ssu2,
            }
        );
        let flipped = select_peer_transport(
            &existing,
            &[],
            &[],
            &[],
            SelectionPolicy::new(true, true, false, false),
            false,
        );
        assert_eq!(
            flipped,
            SelectionOutcome::Reuse {
                link: LinkId::new(1).expect("id"),
                transport: TransportKind::Ntcp2,
            }
        );
    }

    #[test]
    fn ssu2_backoff_falls_back_to_valid_ntcp2_candidate() {
        let candidates = vec![
            candidate(TransportKind::Ssu2, true, 1),
            candidate(TransportKind::Ntcp2, true, 2),
        ];
        assert_eq!(
            select_peer_transport(
                &[],
                &candidates,
                &[],
                &[TransportKind::Ssu2],
                SelectionPolicy::default(),
                false,
            ),
            SelectionOutcome::Dial {
                transport: TransportKind::Ntcp2,
                tag: 2,
            }
        );
    }

    #[test]
    fn ntcp2_failure_selects_ssu2_where_valid() {
        let candidates = vec![
            candidate(TransportKind::Ntcp2, true, 2),
            candidate(TransportKind::Ssu2, true, 1),
        ];
        // The NTCP2 address failed; only its tag is excluded.
        assert_eq!(
            select_peer_transport(
                &[],
                &candidates,
                &[2],
                &[],
                SelectionPolicy::default(),
                false,
            ),
            SelectionOutcome::Dial {
                transport: TransportKind::Ssu2,
                tag: 1,
            }
        );
    }

    #[test]
    fn address_failure_does_not_poison_sibling_transport_addresses() {
        let candidates = vec![
            candidate(TransportKind::Ssu2, true, 1),
            candidate(TransportKind::Ssu2, true, 5),
            candidate(TransportKind::Ntcp2, true, 2),
        ];
        // Failing one SSU2 address leaves the other SSU2 address plus
        // the NTCP2 fallback, with SSU2 still primary.
        assert_eq!(
            select_peer_transport(
                &[],
                &candidates,
                &[1],
                &[],
                SelectionPolicy::default(),
                false,
            ),
            SelectionOutcome::DialFallback {
                primary_transport: TransportKind::Ssu2,
                primary_tag: 5,
                secondary_transport: TransportKind::Ntcp2,
                secondary_tag: 2,
            }
        );
    }

    #[test]
    fn dual_transport_candidates_produce_ordered_fallback() {
        let candidates = vec![
            candidate(TransportKind::Ntcp2, true, 2),
            candidate(TransportKind::Ssu2, true, 1),
        ];
        assert_eq!(
            select_peer_transport(
                &[],
                &candidates,
                &[],
                &[],
                SelectionPolicy::default(),
                false,
            ),
            SelectionOutcome::DialFallback {
                primary_transport: TransportKind::Ssu2,
                primary_tag: 1,
                secondary_transport: TransportKind::Ntcp2,
                secondary_tag: 2,
            }
        );
    }

    #[test]
    fn single_transport_candidate_dials_without_fallback() {
        let candidates = vec![candidate(TransportKind::Ntcp2, true, 9)];
        assert_eq!(
            select_peer_transport(
                &[],
                &candidates,
                &[],
                &[],
                SelectionPolicy::new(true, false, true, false),
                false,
            ),
            SelectionOutcome::Dial {
                transport: TransportKind::Ntcp2,
                tag: 9,
            }
        );
    }

    #[test]
    fn disabled_transport_is_never_selected() {
        let candidates = vec![candidate(TransportKind::Ssu2, true, 1)];
        assert_eq!(
            select_peer_transport(
                &[],
                &candidates,
                &[],
                &[],
                SelectionPolicy::new(true, false, true, false),
                false,
            ),
            SelectionOutcome::NoCompatibleAddress
        );
    }

    #[test]
    fn all_backed_off_reports_backoff_not_no_address() {
        let candidates = vec![candidate(TransportKind::Ssu2, true, 1)];
        assert_eq!(
            select_peer_transport(
                &[],
                &candidates,
                &[],
                &[TransportKind::Ssu2],
                SelectionPolicy::default(),
                false,
            ),
            SelectionOutcome::BackedOff
        );
    }

    #[test]
    fn peer_limit_denies_new_dials_but_keeps_reuse() {
        let candidates = vec![candidate(TransportKind::Ssu2, true, 1)];
        assert_eq!(
            select_peer_transport(&[], &candidates, &[], &[], SelectionPolicy::default(), true,),
            SelectionOutcome::ResourceDenied
        );
        let existing = vec![link(TransportKind::Ssu2, 4)];
        assert_eq!(
            select_peer_transport(
                &existing,
                &candidates,
                &[],
                &[],
                SelectionPolicy::default(),
                true,
            ),
            SelectionOutcome::Reuse {
                link: LinkId::new(4).expect("id"),
                transport: TransportKind::Ssu2,
            }
        );
    }

    #[test]
    fn invalid_unknown_and_introducer_only_candidates_are_skipped() {
        let invalid =
            TransportCandidate::new(TransportKind::Ssu2, AddressFamily::Ipv4, true, 1, false);
        let unknown =
            TransportCandidate::new(TransportKind::Ssu2, AddressFamily::Unknown, true, 2, true);
        let relay_only = candidate(TransportKind::Ssu2, false, 3);
        let direct = candidate(TransportKind::Ntcp2, true, 4);
        let outcome = select_peer_transport(
            &[],
            &[invalid, unknown, relay_only, direct],
            &[],
            &[],
            SelectionPolicy::default(),
            false,
        );
        assert_eq!(
            outcome,
            SelectionOutcome::Dial {
                transport: TransportKind::Ntcp2,
                tag: 4,
            }
        );
    }

    #[test]
    fn introducer_only_is_dialed_when_policy_allows() {
        let relay_only = candidate(TransportKind::Ssu2, false, 3);
        assert_eq!(
            select_peer_transport(
                &[],
                &[relay_only],
                &[],
                &[],
                SelectionPolicy::new(true, true, true, true),
                false,
            ),
            SelectionOutcome::Dial {
                transport: TransportKind::Ssu2,
                tag: 3,
            }
        );
    }

    #[test]
    fn deterministic_inputs_always_choose_the_same_candidate() {
        let candidates = vec![
            candidate(TransportKind::Ntcp2, true, 8),
            candidate(TransportKind::Ssu2, true, 3),
            candidate(TransportKind::Ssu2, true, 7),
        ];
        let first = select_peer_transport(
            &[],
            &candidates,
            &[],
            &[],
            SelectionPolicy::default(),
            false,
        );
        // Shuffled input order converges to the same decision.
        let shuffled = vec![
            candidate(TransportKind::Ssu2, true, 7),
            candidate(TransportKind::Ntcp2, true, 8),
            candidate(TransportKind::Ssu2, true, 3),
        ];
        let second =
            select_peer_transport(&[], &shuffled, &[], &[], SelectionPolicy::default(), false);
        assert_eq!(first, second);
        assert_eq!(
            first,
            SelectionOutcome::DialFallback {
                primary_transport: TransportKind::Ssu2,
                primary_tag: 3,
                secondary_transport: TransportKind::Ntcp2,
                secondary_tag: 8,
            }
        );
    }

    #[test]
    fn over_bound_input_is_denied_not_truncated() {
        let candidates =
            vec![candidate(TransportKind::Ssu2, true, 1); MAX_SELECTION_CANDIDATES + 1];
        assert_eq!(
            select_peer_transport(
                &[],
                &candidates,
                &[],
                &[],
                SelectionPolicy::default(),
                false,
            ),
            SelectionOutcome::ResourceDenied
        );
    }
}
