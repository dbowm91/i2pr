//! Daemon-owned floodfill eligibility and advertisement authority vocabulary.

use std::collections::BTreeMap;

use i2pr_proto::RouterAddress;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloodfillRoleState {
    Disabled,
    Eligible,
    Activating,
    Active,
    Draining,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloodfillEligibilitySnapshot {
    pub controlled_qualification_permit: bool,
    pub qualified_ssu2_address: bool,
    pub direct_reachability: bool,
    pub netdb_ready: bool,
    pub storage_ready: bool,
    pub maintenance_ready: bool,
    pub resource_headroom: bool,
    pub clock_sane: bool,
    pub supervision_healthy: bool,
}

impl FloodfillEligibilitySnapshot {
    pub const fn eligible(self) -> bool {
        self.controlled_qualification_permit
            && self.qualified_ssu2_address
            && self.direct_reachability
            && self.netdb_ready
            && self.storage_ready
            && self.maintenance_ready
            && self.resource_headroom
            && self.clock_sane
            && self.supervision_healthy
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloodfillRoleEffect {
    None,
    ReadyToActivate,
    WithdrawAdvertisementAndStopAdmission,
    FailedClosed,
}

#[derive(Debug)]
pub struct FloodfillRoleController {
    state: FloodfillRoleState,
    snapshot: Option<FloodfillEligibilitySnapshot>,
}

impl Default for FloodfillRoleController {
    fn default() -> Self {
        Self::new()
    }
}

impl FloodfillRoleController {
    pub const fn new() -> Self {
        Self {
            state: FloodfillRoleState::Disabled,
            snapshot: None,
        }
    }
    pub const fn state(&self) -> FloodfillRoleState {
        self.state
    }
    pub const fn accepts_new_work(&self) -> bool {
        matches!(self.state, FloodfillRoleState::Active)
    }

    pub fn update(&mut self, snapshot: FloodfillEligibilitySnapshot) -> FloodfillRoleEffect {
        self.snapshot = Some(snapshot);
        if !snapshot.eligible() {
            return match self.state {
                FloodfillRoleState::Active
                | FloodfillRoleState::Activating
                | FloodfillRoleState::Eligible => {
                    self.state = FloodfillRoleState::Draining;
                    FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission
                }
                _ => {
                    self.state = FloodfillRoleState::Disabled;
                    FloodfillRoleEffect::None
                }
            };
        }
        match self.state {
            FloodfillRoleState::Disabled | FloodfillRoleState::Failed => {
                self.state = FloodfillRoleState::Eligible;
                FloodfillRoleEffect::ReadyToActivate
            }
            _ => FloodfillRoleEffect::None,
        }
    }

    pub fn begin_activation(&mut self) -> bool {
        if self.state != FloodfillRoleState::Eligible
            || !self
                .snapshot
                .is_some_and(FloodfillEligibilitySnapshot::eligible)
        {
            return false;
        }
        self.state = FloodfillRoleState::Activating;
        true
    }

    pub fn complete_activation(&mut self) -> Option<FloodfillAdvertisementPermit> {
        if self.state != FloodfillRoleState::Activating
            || !self
                .snapshot
                .is_some_and(FloodfillEligibilitySnapshot::eligible)
        {
            return None;
        }
        self.state = FloodfillRoleState::Active;
        Some(FloodfillAdvertisementPermit { _sealed: () })
    }

    pub fn complete_drain(&mut self) -> bool {
        if self.state != FloodfillRoleState::Draining {
            return false;
        }
        self.state = FloodfillRoleState::Disabled;
        true
    }

    pub fn fail(&mut self) -> FloodfillRoleEffect {
        let had_advertisement = self.state == FloodfillRoleState::Active;
        self.state = FloodfillRoleState::Failed;
        if had_advertisement {
            FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission
        } else {
            FloodfillRoleEffect::FailedClosed
        }
    }

    pub fn advertisement_permit(&self) -> Option<FloodfillAdvertisementPermit> {
        (self.state == FloodfillRoleState::Active
            && self
                .snapshot
                .is_some_and(FloodfillEligibilitySnapshot::eligible))
        .then_some(FloodfillAdvertisementPermit { _sealed: () })
    }
}

/// Opaque proof that the role controller reached Active from a complete eligibility snapshot.
#[derive(Clone)]
pub struct FloodfillAdvertisementPermit {
    _sealed: (),
}

impl std::fmt::Debug for FloodfillAdvertisementPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FloodfillAdvertisementPermit(..)")
    }
}

/// Opaque proof that a controlled peer-test exchange confirmed inbound
/// reachability of the advertised bound address (Plan 306, ADR 0030).
///
/// The token carries no evidence itself: only the controlled activation
/// owner may mint it, and only beside a
/// `ControlledPeerTestOutcome::Confirmed` for the exact bound address
/// being advertised (mint site confined by
/// `scripts/check-m12-floodfill-boundaries.sh`, mirroring the
/// advertisement permit). Unit tests mint it freely to prove the
/// builder's admission shape.
#[derive(Clone)]
pub struct LoopbackReachabilityProof {
    _sealed: (),
}

impl LoopbackReachabilityProof {
    /// Attests a confirmed controlled peer-test exchange for the
    /// advertised bound address. Callers outside the controlled
    /// activation owner must not call this (boundary-script-gated).
    pub fn attest_confirmed_peer_test() -> Self {
        Self { _sealed: () }
    }
}

impl std::fmt::Debug for LoopbackReachabilityProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LoopbackReachabilityProof(..)")
    }
}

/// Validates the final RouterAddress style at the typed advertisement boundary.
pub fn is_qualified_ssu2_address(address: &RouterAddress) -> bool {
    if address.transport_style() != "SSU2" {
        return false;
    }
    let options = address.options();
    let Some(host) = options
        .get("host")
        .and_then(|value| value.parse::<core::net::IpAddr>().ok())
    else {
        return false;
    };
    let Some(port) = options
        .get("port")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port != 0)
    else {
        return false;
    };
    let Some(mtu) = options
        .get("mtu")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|mtu| (1280..=9000).contains(mtu))
    else {
        return false;
    };
    let Some(version) = options.get("v") else {
        return false;
    };
    let Some(caps) = options.get("caps") else {
        return false;
    };
    let (Some(static_key), Some(intro_key)) = (
        options
            .get("s")
            .and_then(|value| crate::base64::decode(value).ok()),
        options
            .get("i")
            .and_then(|value| crate::base64::decode(value).ok()),
    ) else {
        return false;
    };
    if host.is_unspecified()
        || port.to_string() != options.get("port").unwrap_or_default()
        || mtu.to_string() != options.get("mtu").unwrap_or_default()
        || version != "2"
        || static_key.len() != 32
        || static_key.iter().all(|byte| *byte == 0)
        || intro_key.len() != 32
        || intro_key.iter().all(|byte| *byte == 0)
        || caps.is_empty()
        || host.to_string() != options.get("host").unwrap_or_default()
        || match host {
            core::net::IpAddr::V4(_) => caps != "4",
            core::net::IpAddr::V6(_) => caps != "6",
        }
    {
        return false;
    }
    options.entries().iter().all(|entry| {
        matches!(
            entry.key(),
            "host" | "port" | "mtu" | "v" | "caps" | "s" | "i"
        )
    })
}

/// Validates an SSU2 address in the **role-bearing** direct form.
///
/// Plan 526. The specification's Relay capability is advertised with a
/// `"C"` in the address's `caps` option, and the reference publishes it on
/// the **published** form only (`libi2pd/RouterInfo.cpp:1350`-`:1373`),
/// because its peer-side check demands an endpoint for an introducer
/// (`libi2pd/RouterInfo.cpp:1059`).
///
/// The **direct** rule demands the capability string be *exactly* the
/// address's own family, so a router that advertises the role cannot be
/// qualified by it. This is the netdb's **third** rule, beside
/// [`is_qualified_ssu2_address`] and
/// [`is_qualified_ssu2_introducer_address`], and neither of the first two
/// is weakened by it: the direct rule still refuses a role-bearing address
/// and the introducer rule still refuses any role letter at all.
///
/// The capability string is **exact**: the address's own family letter
/// followed by the role letter, and nothing else. `"BC"`, `"4CB"`, `"46C"`
/// and the bare family letters are all refused, so an unearned role cannot
/// reach the wire through a permissive rule.
pub fn is_qualified_ssu2_relay_role_address(address: &RouterAddress) -> bool {
    if address.transport_style() != "SSU2" {
        return false;
    }
    let options = address.options();
    let Some(host) = options
        .get("host")
        .and_then(|value| value.parse::<core::net::IpAddr>().ok())
    else {
        return false;
    };
    let Some(port) = options
        .get("port")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port != 0)
    else {
        return false;
    };
    let Some(mtu) = options
        .get("mtu")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|mtu| (1280..=9000).contains(mtu))
    else {
        return false;
    };
    let Some(version) = options.get("v") else {
        return false;
    };
    let Some(caps) = options.get("caps") else {
        return false;
    };
    let (Some(static_key), Some(intro_key)) = (
        options
            .get("s")
            .and_then(|value| crate::base64::decode(value).ok()),
        options
            .get("i")
            .and_then(|value| crate::base64::decode(value).ok()),
    ) else {
        return false;
    };
    if host.is_unspecified()
        || port.to_string() != options.get("port").unwrap_or_default()
        || mtu.to_string() != options.get("mtu").unwrap_or_default()
        || version != "2"
        || static_key.len() != 32
        || static_key.iter().all(|byte| *byte == 0)
        || intro_key.len() != 32
        || intro_key.iter().all(|byte| *byte == 0)
        || caps.is_empty()
        || host.to_string() != options.get("host").unwrap_or_default()
        // The capability string is exactly the address's own family
        // letter followed by the introducer role letter.
        || match host {
            core::net::IpAddr::V4(_) => caps != "4C",
            core::net::IpAddr::V6(_) => caps != "6C",
        }
    {
        return false;
    }
    options.entries().iter().all(|entry| {
        matches!(
            entry.key(),
            "host" | "port" | "mtu" | "v" | "caps" | "s" | "i"
        )
    })
}

/// Validates an SSU2 address in the specification's introducer-only form.
///
/// Plan 523. The SSU2 specification forbids publishing a host or a port
/// in an address that publishes introducers, and defines the SSU2
/// introducer group as `ih[N]` (the introducer's router hash), `iexp[N]`
/// (its expiry) and `itag[N]` (its relay tag). The SSU 1 introducer
/// options (`ihost`/`iport`/`ikey`) are not part of an SSU2 address and
/// are refused here.
///
/// This is the netdb's **own** rule, beside
/// [`is_qualified_ssu2_address`]. The direct form and the introducer-only
/// form are exclusive, and neither rule is weakened by the other: the
/// direct rule still demands a host and a port, and this one still
/// forbids them.
pub fn is_qualified_ssu2_introducer_address(address: &RouterAddress) -> bool {
    if address.transport_style() != "SSU2" {
        return false;
    }
    let options = address.options();
    // The specification forbids a host or a port in an address that
    // publishes introducers.
    if options.get("host").is_some() || options.get("port").is_some() {
        return false;
    }
    let Some(mtu) = options
        .get("mtu")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|mtu| (1280..=9000).contains(mtu))
    else {
        return false;
    };
    if mtu.to_string() != options.get("mtu").unwrap_or_default() {
        return false;
    }
    if options.get("v") != Some("2") {
        return false;
    }
    // The specification demands the family capability alongside
    // introducers, and nothing else: the role capabilities are a separate
    // decision.
    let Some(caps) = options.get("caps") else {
        return false;
    };
    if caps.is_empty() || !caps.bytes().all(|byte| byte == b'4' || byte == b'6') {
        return false;
    }
    let (Some(static_key), Some(intro_key)) = (
        options
            .get("s")
            .and_then(|value| crate::base64::decode(value).ok()),
        options
            .get("i")
            .and_then(|value| crate::base64::decode(value).ok()),
    ) else {
        return false;
    };
    if static_key.len() != 32
        || static_key.iter().all(|byte| *byte == 0)
        || intro_key.len() != 32
        || intro_key.iter().all(|byte| *byte == 0)
    {
        return false;
    }

    // Every option must be part of the SSU2 vocabulary, and every
    // introducer group must be complete.
    let mut hashes: BTreeMap<usize, bool> = BTreeMap::new();
    let mut expiries: BTreeMap<usize, bool> = BTreeMap::new();
    let mut tags: BTreeMap<usize, bool> = BTreeMap::new();
    for entry in options.entries().iter() {
        match entry.key() {
            "s" | "i" | "v" | "caps" | "mtu" => {}
            key => {
                let Some((index, kind)) = introducer_option(key) else {
                    return false;
                };
                if index >= MAX_SSU2_PUBLISHED_INTRODUCERS {
                    return false;
                }
                let valid = match kind {
                    IntroducerOptionKind::Hash => crate::base64::decode(entry.value())
                        .map(|bytes| bytes.len() == 32 && !bytes.iter().all(|byte| *byte == 0))
                        .unwrap_or(false),
                    IntroducerOptionKind::Expiry => canonical_decimal(entry.value()).is_some(),
                    IntroducerOptionKind::Tag => canonical_decimal(entry.value())
                        .filter(|tag| *tag != 0)
                        .is_some(),
                };
                if !valid {
                    return false;
                }
                let slot = match kind {
                    IntroducerOptionKind::Hash => &mut hashes,
                    IntroducerOptionKind::Expiry => &mut expiries,
                    IntroducerOptionKind::Tag => &mut tags,
                };
                if slot.insert(index, true).is_some() {
                    // A duplicated introducer option.
                    return false;
                }
            }
        }
    }

    // At least one complete group: a hash and a tag. The expiry is
    // optional in the specification's address table.
    hashes.keys().any(|index| tags.contains_key(index))
}

/// The maximum number of introducer groups the specification's address
/// table allows (`[0-2]`).
const MAX_SSU2_PUBLISHED_INTRODUCERS: usize = 3;

/// One indexed introducer option of an SSU2 address.
#[derive(Clone, Copy)]
enum IntroducerOptionKind {
    /// `ih[N]`: the introducer's router hash.
    Hash,
    /// `iexp[N]`: the introducer's expiry.
    Expiry,
    /// `itag[N]`: the introducer's relay tag.
    Tag,
}

/// Classifies one indexed introducer option key.
///
/// The SSU 1 introducer options (`ihostN`/`iportN`/`ikeyN`) do not
/// classify: `ih` only matches a one-digit suffix, so `ihost0` falls
/// through and is refused as an unknown option.
fn introducer_option(key: &str) -> Option<(usize, IntroducerOptionKind)> {
    const KINDS: [(&str, IntroducerOptionKind); 3] = [
        ("ih", IntroducerOptionKind::Hash),
        ("iexp", IntroducerOptionKind::Expiry),
        ("itag", IntroducerOptionKind::Tag),
    ];
    for (prefix, kind) in KINDS {
        if let Some(suffix) = key.strip_prefix(prefix)
            && suffix.len() == 1
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Some((usize::from(suffix.bytes().next()? - b'0'), kind));
        }
    }
    None
}

/// Parses a canonical decimal value: digits only, no leading zeros, no
/// sign, and a bounded width.
fn canonical_decimal(value: &str) -> Option<u64> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if value.len() > 1 && value.starts_with('0') {
        return None;
    }
    value.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn eligible() -> FloodfillEligibilitySnapshot {
        FloodfillEligibilitySnapshot {
            controlled_qualification_permit: true,
            qualified_ssu2_address: true,
            direct_reachability: true,
            netdb_ready: true,
            storage_ready: true,
            maintenance_ready: true,
            resource_headroom: true,
            clock_sane: true,
            supervision_healthy: true,
        }
    }
    #[test]
    fn config_without_controlled_eligibility_never_activates_and_health_loss_withdraws_first() {
        let mut controller = FloodfillRoleController::new();
        let mut snapshot = eligible();
        snapshot.controlled_qualification_permit = false;
        assert_eq!(controller.update(snapshot), FloodfillRoleEffect::None);
        assert!(controller.advertisement_permit().is_none());
        snapshot = eligible();
        assert_eq!(
            controller.update(snapshot),
            FloodfillRoleEffect::ReadyToActivate
        );
        assert!(controller.begin_activation());
        assert!(controller.complete_activation().is_some());
        assert!(controller.accepts_new_work());
        snapshot.resource_headroom = false;
        assert_eq!(
            controller.update(snapshot),
            FloodfillRoleEffect::WithdrawAdvertisementAndStopAdmission
        );
        assert!(!controller.accepts_new_work());
        assert!(controller.complete_drain());
        assert_eq!(controller.state(), FloodfillRoleState::Disabled);
    }
}
