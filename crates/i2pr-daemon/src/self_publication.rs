//! Plan 520 — the router's own published SSU2 address.
//!
//! The router publishes its own RouterInfo to the network. Plan 520 gives
//! that record an address a peer can actually **qualify**: the *direct*
//! SSU2 form, carrying the family capability and the transport's MTU, and
//! published **only** when the router's own reachability evidence admits
//! it.
//!
//! The decision is not the daemon's, and this module never re-implements
//! it:
//!
//! - the **transport** decides. `build_publication_snapshot`
//!   (`i2pr-transport-ssu2/src/publication.rs`) renders the direct form
//!   only when the reachability state is `Reachable` and the policy
//!   allows it, and renders the firewalled form otherwise. The runtime
//!   applies that builder to the router's own evidence
//!   (`i2pr_runtime::Ssu2RuntimeService::publication_material`), and the
//!   **firewalled** form carries no host and no port — it is not an
//!   address a peer can dial.
//! - the **netdb** qualifies the form. `is_qualified_ssu2_address`
//!   (`i2pr-netdb/src/floodfill_role.rs`) is the authority on what a
//!   published SSU2 address may look like, and the netdb's own builder
//!   (`LocalRouterInfoBuilder::build_with_ssu2_address`) refuses any
//!   address that rule refuses.
//!
//! What this module owns is the part that is genuinely the daemon's: the
//! operator's opt-in, the hand-off of the transport's address to the
//! netdb's builder, the install, the **withdrawal** when the evidence
//! lapses, and the count of every outcome under its own reason.
//!
//! The builder this module uses carries **no** floodfill permit and
//! therefore cannot produce `caps=f`: the floodfill path keeps its own
//! builder, its own permit and its own reachability proof. A record this
//! module publishes is an ordinary router's record.

use std::sync::atomic::{AtomicU64, Ordering};

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::LocalRouterInfoBuilder;
use i2pr_proto::Date;
use i2pr_runtime::{
    Ssu2PublicationForm, Ssu2PublicationMaterial, Ssu2PublicationUnavailable, Ssu2RuntimeService,
};

/// The maximum encoded size of the router's own published RouterInfo.
const MAX_ENCODED_ROUTER_INFO: usize = i2pr_proto::MAX_COMMON_STRUCTURE_SIZE;

/// What the owner decided about the router's own published record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelfPublicationDecision {
    /// The operator's opt-in is off: the installed record stands exactly
    /// as it is, and the router publishes what it published before.
    Disabled,
    /// Another path owns the installed record's address, so the installed
    /// record stands untouched.
    StoodDown(StandDownReason),
    /// Install this encoded record: it is the router's own record, signed
    /// with the address the transport admitted.
    Install(Vec<u8>),
    /// The record could not be built, so the installed record stands.
    Failed(SelfPublicationFailure),
}

/// Why the owner stood down and left the installed record alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandDownReason {
    /// The installed record is the floodfill path's. Its address is the
    /// floodfill path's, and it is built from the floodfill permit's own
    /// record; this owner never replaces it.
    FloodfillRecord,
    /// The installed record carries more than one address (a
    /// dual-transport record). The second address is not this owner's to
    /// drop.
    MultipleAddresses,
    /// The installed record's single address is not an SSU2 address.
    ForeignAddress,
    /// The installed record could not be read, so it is not replaced.
    UnreadableRecord,
    /// No record is installed, so there is nothing to refresh.
    NoInstalledRecord,
}

/// The three published forms the router can install.
///
/// Plan 526: the runtime's [`Ssu2PublicationForm`] names the **shape** of
/// the address, and the shape has two variants in the direct family. This
/// enum is the daemon's own view of which of the three forms was published,
/// so the owner never inspects the address's `caps` option to infer it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublishedForm {
    /// The **direct** form without the introducer role letter.
    Direct,
    /// The **direct** form **with** the introducer role letter.
    DirectRelayRole,
    /// The **introducer-only** form.
    IntroducerOnly,
}

/// Returns the published form a material represents.
fn published_form(material: &Ssu2PublicationMaterial) -> PublishedForm {
    match material.form {
        Ssu2PublicationForm::Direct => {
            if material.advertises_introducer_role {
                PublishedForm::DirectRelayRole
            } else {
                PublishedForm::Direct
            }
        }
        Ssu2PublicationForm::IntroducerOnly => PublishedForm::IntroducerOnly,
    }
}

/// Why the addressed record could not be built or installed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelfPublicationFailure {
    /// The netdb's own **direct** qualification refused the address the
    /// transport produced. The address is never signed and never
    /// installed.
    UnqualifiedAddress,
    /// The netdb's own **introducer** qualification refused the address.
    ///
    /// Plan 525: the two forms are qualified by two **different** rules,
    /// so the rule that refused is part of the reason.
    UnqualifiedIntroducerAddress,
    /// The netdb's own **role-bearing** qualification refused the
    /// address.
    ///
    /// Plan 526: the three forms are qualified by three **different**
    /// rules, so the rule that refused is part of the reason.
    UnqualifiedRelayRoleAddress,
    /// The record could not be built or encoded.
    Construction,
    /// The runtime refused the install.
    Install(Ssu2PublicationUnavailable),
}

/// The outcome of one refresh of the router's own published record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelfPublicationOutcome {
    /// The opt-in is off: the router publishes exactly what it published
    /// before this plan.
    Disabled,
    /// The transport's qualified direct address was published.
    Published,
    /// No address is published. The reason is the transport's own.
    Withheld(Ssu2PublicationUnavailable),
    /// Another path owns the installed record's address.
    StoodDown(StandDownReason),
    /// The record could not be built or installed; the previous record
    /// stands.
    Failed(SelfPublicationFailure),
}

/// The counts of every outcome, one per reason.
///
/// A refusal is never silent: every reason the owner can refuse under has
/// its own counter, and a counter that never moves is as much a fact as
/// one that does.
#[derive(Debug, Default)]
pub struct SelfPublicationCounters {
    published_direct: AtomicU64,
    published_direct_relay_role: AtomicU64,
    published_introducer_only: AtomicU64,
    withdrawn: AtomicU64,
    prepared: AtomicU64,
    disabled: AtomicU64,
    withheld_closed: AtomicU64,
    withheld_no_bound_socket: AtomicU64,
    withheld_state_unavailable: AtomicU64,
    withheld_reachability_unqualified: AtomicU64,
    withheld_invalid_publication: AtomicU64,
    withheld_no_introducers: AtomicU64,
    stood_down_floodfill: AtomicU64,
    stood_down_multiple_addresses: AtomicU64,
    stood_down_foreign_address: AtomicU64,
    stood_down_unreadable: AtomicU64,
    stood_down_no_installed_record: AtomicU64,
    failed_unqualified_address: AtomicU64,
    failed_unqualified_introducer_address: AtomicU64,
    failed_unqualified_relay_role_address: AtomicU64,
    failed_construction: AtomicU64,
    failed_install: AtomicU64,
}

/// A plain snapshot of the owner's counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SelfPublicationCounterSnapshot {
    /// An address was published, in either form.
    ///
    /// Plan 525: this is the **sum** of the two published forms. It was
    /// previously a count of every successful install, which included the
    /// withdrawal and so did not mean "an address was published".
    pub published: u64,
    /// The **direct** form was published: the address carries the
    /// router's host and port.
    pub published_direct: u64,
    /// The **role-bearing** direct form was published: the address names
    /// the router's host and port **and** advertises the introducer role.
    ///
    /// Plan 526: this is the form the specification's Relay capability
    /// rides, and it is published **only** while the role is live.
    pub published_direct_relay_role: u64,
    /// The **introducer-only** form was published: the address carries no
    /// host and no port, and publishes the validated introducer set.
    pub published_introducer_only: u64,
    /// The address was **withdrawn**: the record carries no address at
    /// all.
    pub withdrawn: u64,
    /// The owner produced an installable record.
    pub prepared: u64,
    /// The opt-in was off.
    pub disabled: u64,
    /// The runtime was closed.
    pub withheld_closed: u64,
    /// No socket was bound.
    pub withheld_no_bound_socket: u64,
    /// The runtime's state was unavailable.
    pub withheld_state_unavailable: u64,
    /// The router's own reachability evidence did not admit the direct
    /// form.
    pub withheld_reachability_unqualified: u64,
    /// The bound endpoint or the generated address was invalid.
    pub withheld_invalid_publication: u64,
    /// No validated introducer was live.
    pub withheld_no_introducers: u64,
    /// The installed record was the floodfill path's.
    pub stood_down_floodfill: u64,
    /// The installed record carried more than one address.
    pub stood_down_multiple_addresses: u64,
    /// The installed record's address was not an SSU2 address.
    pub stood_down_foreign_address: u64,
    /// The installed record could not be read.
    pub stood_down_unreadable: u64,
    /// No record was installed.
    pub stood_down_no_installed_record: u64,
    /// The netdb's **direct** qualification refused the address.
    pub failed_unqualified_address: u64,
    /// The netdb's **introducer** qualification refused the address.
    pub failed_unqualified_introducer_address: u64,
    /// The netdb's **role-bearing** qualification refused the address.
    pub failed_unqualified_relay_role_address: u64,
    /// The record could not be built or encoded.
    pub failed_construction: u64,
    /// The runtime refused the install.
    pub failed_install: u64,
}

impl SelfPublicationCounters {
    fn record_withheld(&self, reason: Ssu2PublicationUnavailable) {
        let counter = match reason {
            Ssu2PublicationUnavailable::Closed => &self.withheld_closed,
            Ssu2PublicationUnavailable::NoBoundSocket => &self.withheld_no_bound_socket,
            Ssu2PublicationUnavailable::StateUnavailable => &self.withheld_state_unavailable,
            Ssu2PublicationUnavailable::ReachabilityUnqualified => {
                &self.withheld_reachability_unqualified
            }
            Ssu2PublicationUnavailable::InvalidPublication => &self.withheld_invalid_publication,
            Ssu2PublicationUnavailable::NoIntroducers => &self.withheld_no_introducers,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn record_stood_down(&self, reason: StandDownReason) {
        let counter = match reason {
            StandDownReason::FloodfillRecord => &self.stood_down_floodfill,
            StandDownReason::MultipleAddresses => &self.stood_down_multiple_addresses,
            StandDownReason::ForeignAddress => &self.stood_down_foreign_address,
            StandDownReason::UnreadableRecord => &self.stood_down_unreadable,
            StandDownReason::NoInstalledRecord => &self.stood_down_no_installed_record,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn record_failure(&self, reason: SelfPublicationFailure) {
        let counter = match reason {
            SelfPublicationFailure::UnqualifiedAddress => &self.failed_unqualified_address,
            SelfPublicationFailure::UnqualifiedIntroducerAddress => {
                &self.failed_unqualified_introducer_address
            }
            SelfPublicationFailure::UnqualifiedRelayRoleAddress => {
                &self.failed_unqualified_relay_role_address
            }
            SelfPublicationFailure::Construction => &self.failed_construction,
            SelfPublicationFailure::Install(_) => &self.failed_install,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns a plain snapshot of every counter.
    pub fn snapshot(&self) -> SelfPublicationCounterSnapshot {
        let published_direct = self.published_direct.load(Ordering::Relaxed);
        let published_direct_relay_role = self.published_direct_relay_role.load(Ordering::Relaxed);
        let published_introducer_only = self.published_introducer_only.load(Ordering::Relaxed);
        SelfPublicationCounterSnapshot {
            published: published_direct + published_direct_relay_role + published_introducer_only,
            published_direct,
            published_direct_relay_role,
            published_introducer_only,
            withdrawn: self.withdrawn.load(Ordering::Relaxed),
            prepared: self.prepared.load(Ordering::Relaxed),
            disabled: self.disabled.load(Ordering::Relaxed),
            withheld_closed: self.withheld_closed.load(Ordering::Relaxed),
            withheld_no_bound_socket: self.withheld_no_bound_socket.load(Ordering::Relaxed),
            withheld_state_unavailable: self.withheld_state_unavailable.load(Ordering::Relaxed),
            withheld_reachability_unqualified: self
                .withheld_reachability_unqualified
                .load(Ordering::Relaxed),
            withheld_invalid_publication: self.withheld_invalid_publication.load(Ordering::Relaxed),
            withheld_no_introducers: self.withheld_no_introducers.load(Ordering::Relaxed),
            stood_down_floodfill: self.stood_down_floodfill.load(Ordering::Relaxed),
            stood_down_multiple_addresses: self
                .stood_down_multiple_addresses
                .load(Ordering::Relaxed),
            stood_down_foreign_address: self.stood_down_foreign_address.load(Ordering::Relaxed),
            stood_down_unreadable: self.stood_down_unreadable.load(Ordering::Relaxed),
            stood_down_no_installed_record: self
                .stood_down_no_installed_record
                .load(Ordering::Relaxed),
            failed_unqualified_address: self.failed_unqualified_address.load(Ordering::Relaxed),
            failed_unqualified_introducer_address: self
                .failed_unqualified_introducer_address
                .load(Ordering::Relaxed),
            failed_unqualified_relay_role_address: self
                .failed_unqualified_relay_role_address
                .load(Ordering::Relaxed),
            failed_construction: self.failed_construction.load(Ordering::Relaxed),
            failed_install: self.failed_install.load(Ordering::Relaxed),
        }
    }
}

/// The daemon's owner of the router's own published address.
///
/// One owner, one decision at a time. The owner is bounded: it holds a
/// boolean and a fixed set of counters, and it starts no task, no timer
/// and no listener of its own.
#[derive(Debug)]
pub struct SelfPublicationOwner {
    enabled: bool,
    counters: SelfPublicationCounters,
}

impl SelfPublicationOwner {
    /// Creates an owner for the operator's opt-in.
    ///
    /// The opt-in is **off** by default, and an owner that is off changes
    /// nothing: the router publishes exactly what it published before this
    /// plan.
    pub const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            counters: SelfPublicationCounters {
                published_direct: AtomicU64::new(0),
                published_direct_relay_role: AtomicU64::new(0),
                published_introducer_only: AtomicU64::new(0),
                withdrawn: AtomicU64::new(0),
                prepared: AtomicU64::new(0),
                disabled: AtomicU64::new(0),
                withheld_closed: AtomicU64::new(0),
                withheld_no_bound_socket: AtomicU64::new(0),
                withheld_state_unavailable: AtomicU64::new(0),
                withheld_reachability_unqualified: AtomicU64::new(0),
                withheld_invalid_publication: AtomicU64::new(0),
                withheld_no_introducers: AtomicU64::new(0),
                stood_down_floodfill: AtomicU64::new(0),
                stood_down_multiple_addresses: AtomicU64::new(0),
                stood_down_foreign_address: AtomicU64::new(0),
                stood_down_unreadable: AtomicU64::new(0),
                stood_down_no_installed_record: AtomicU64::new(0),
                failed_unqualified_address: AtomicU64::new(0),
                failed_unqualified_introducer_address: AtomicU64::new(0),
                failed_unqualified_relay_role_address: AtomicU64::new(0),
                failed_construction: AtomicU64::new(0),
                failed_install: AtomicU64::new(0),
            },
        }
    }

    /// Whether the operator's opt-in is on.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// The owner's counters.
    pub const fn counters(&self) -> &SelfPublicationCounters {
        &self.counters
    }

    /// Decides what the router's own published record should be.
    ///
    /// This is the seam: the installed record and the transport's material
    /// are **inputs**, so the decision is deterministic and does not need a
    /// live socket to be exercised.
    ///
    /// * `installed` is the record the runtime currently holds.
    /// * `material` is the transport's own answer: the address the
    ///   transport admitted, or the reason it admitted none.
    pub fn decide(
        &self,
        bundle: &RouterIdentityBundle,
        installed: Option<&[u8]>,
        material: Result<&Ssu2PublicationMaterial, Ssu2PublicationUnavailable>,
        now_ms: u64,
    ) -> SelfPublicationDecision {
        if !self.enabled {
            self.counters.disabled.fetch_add(1, Ordering::Relaxed);
            return SelfPublicationDecision::Disabled;
        }
        match installed_record_ownership(installed) {
            Owned::Yes => {}
            Owned::No(reason) => {
                self.counters.record_stood_down(reason);
                return SelfPublicationDecision::StoodDown(reason);
            }
        }
        let material = match material {
            Ok(material) => material,
            Err(reason) => {
                self.counters.record_withheld(reason);
                // The withdrawal. The router does not keep publishing an
                // address it can no longer justify, so the record returns
                // to its address-less form.
                return match self.build_record(bundle, None, now_ms) {
                    Ok(encoded) => {
                        self.counters.prepared.fetch_add(1, Ordering::Relaxed);
                        SelfPublicationDecision::Install(encoded)
                    }
                    Err(failure) => {
                        self.counters.record_failure(failure);
                        SelfPublicationDecision::Failed(failure)
                    }
                };
            }
        };
        match self.build_record(bundle, Some(material), now_ms) {
            Ok(encoded) => {
                self.counters.prepared.fetch_add(1, Ordering::Relaxed);
                SelfPublicationDecision::Install(encoded)
            }
            Err(failure) => {
                self.counters.record_failure(failure);
                SelfPublicationDecision::Failed(failure)
            }
        }
    }

    /// Refreshes the router's own published record against the live
    /// runtime.
    ///
    /// The runtime is asked for the material and owns the decision: the
    /// daemon never computes a second answer to the same question. The
    /// owner's part is the install and the count.
    pub fn refresh(
        &self,
        bundle: &RouterIdentityBundle,
        service: &Ssu2RuntimeService,
        now_ms: u64,
    ) -> SelfPublicationOutcome {
        let installed = service.installed_local_router_info();
        // Plan 525: the runtime answers **which form** the router
        // publishes -- the direct one while the reachability evidence
        // admits it, the introducer-only one while it does not -- and the
        // daemon never infers a form from a refusal.
        let material = service.self_publication_material(now_ms);
        self.apply(
            bundle,
            installed.as_deref(),
            material.as_ref().map_err(|reason| *reason),
            service,
            now_ms,
        )
    }

    /// Applies the runtime's answer to the installed record and counts it.
    ///
    /// Plan 526: the owner's **apply** half is separated from its **ask**
    /// half. The production path asks the runtime and applies the answer,
    /// and the separation changes nothing about that path -- it lets the
    /// hand-off, the netdb's rule and the counters be exercised with a
    /// material the caller supplies, which the runtime's own `Reachable`
    /// gate would otherwise put out of reach in a local lane.
    ///
    /// The applied material is still the **only** thing that is installed,
    /// and the install still goes through the runtime's own gate, so a
    /// material the runtime would not have produced is refused there.
    pub fn apply(
        &self,
        bundle: &RouterIdentityBundle,
        installed: Option<&[u8]>,
        material: Result<&Ssu2PublicationMaterial, Ssu2PublicationUnavailable>,
        service: &Ssu2RuntimeService,
        now_ms: u64,
    ) -> SelfPublicationOutcome {
        if !self.enabled {
            self.counters.disabled.fetch_add(1, Ordering::Relaxed);
            return SelfPublicationOutcome::Disabled;
        }
        let decision = self.decide(bundle, installed, material, now_ms);
        match decision {
            SelfPublicationDecision::Disabled => SelfPublicationOutcome::Disabled,
            SelfPublicationDecision::StoodDown(reason) => SelfPublicationOutcome::StoodDown(reason),
            SelfPublicationDecision::Failed(failure) => SelfPublicationOutcome::Failed(failure),
            SelfPublicationDecision::Install(encoded) => {
                match service.install_local_router_info(encoded, now_ms) {
                    Ok(()) => {
                        // The form the runtime admitted is the form that
                        // was published, and each form is counted under
                        // its own reason. A withdrawal carries no form at
                        // all, and is counted as one.
                        match material.as_ref().map(|material| published_form(material)) {
                            Ok(PublishedForm::Direct) => {
                                self.counters
                                    .published_direct
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            // Plan 526: the role-bearing form is counted
                            // under its own reason, so a reader can tell a
                            // router that advertises the introducer role
                            // from one that does not.
                            Ok(PublishedForm::DirectRelayRole) => {
                                self.counters
                                    .published_direct_relay_role
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            Ok(PublishedForm::IntroducerOnly) => {
                                self.counters
                                    .published_introducer_only
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            Err(_) => {
                                self.counters.withdrawn.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        SelfPublicationOutcome::Published
                    }
                    Err(reason) => {
                        self.counters
                            .record_failure(SelfPublicationFailure::Install(reason));
                        SelfPublicationOutcome::Failed(SelfPublicationFailure::Install(reason))
                    }
                }
            }
        }
    }

    /// Builds the router's own signed record, with the address when one
    /// was admitted and with none when the address was withdrawn.
    fn build_record(
        &self,
        bundle: &RouterIdentityBundle,
        material: Option<&Ssu2PublicationMaterial>,
        now_ms: u64,
    ) -> Result<Vec<u8>, SelfPublicationFailure> {
        let options = i2pr_netdb::controlled_router_options()
            .map_err(|_| SelfPublicationFailure::Construction)?;
        let published = Date::from_millis(now_ms);
        let builder = LocalRouterInfoBuilder::new(bundle);
        let local = match material {
            // The netdb's own qualification is the authority: an address
            // it refuses is never signed. Plan 525: the two forms have
            // two **different** rules, and each form is built by the
            // builder that carries its own rule.
            Some(material) => match published_form(material) {
                PublishedForm::Direct => builder
                    .build_with_ssu2_address(published, options, material.address.clone())
                    .map_err(|error| {
                        SelfPublicationFailure::from_netdb(error, PublishedForm::Direct)
                    })?,
                // Plan 526: the role-bearing direct form has the netdb's
                // **own** third rule and the **own** third builder. The
                // direct builder still refuses it, so the two forms can
                // never be signed by the wrong rule.
                PublishedForm::DirectRelayRole => builder
                    .build_with_ssu2_relay_role_address(
                        published,
                        options,
                        material.address.clone(),
                    )
                    .map_err(|error| {
                        SelfPublicationFailure::from_netdb(error, PublishedForm::DirectRelayRole)
                    })?,
                PublishedForm::IntroducerOnly => builder
                    .build_with_ssu2_introducer_address(
                        published,
                        options,
                        material.address.clone(),
                    )
                    .map_err(|error| {
                        SelfPublicationFailure::from_netdb(error, PublishedForm::IntroducerOnly)
                    })?,
            },
            None => builder
                .build(published, options)
                .map_err(|_| SelfPublicationFailure::Construction)?,
        };
        local
            .encoded(MAX_ENCODED_ROUTER_INFO)
            .map_err(|_| SelfPublicationFailure::Construction)
    }
}

impl SelfPublicationFailure {
    /// Maps the netdb's own refusal onto the reason that names it.
    ///
    /// Plan 525: the direct form and the introducer-only form are
    /// qualified by two **different** rules, so the rule that refused is
    /// part of the reason. Every other refusal is a construction failure.
    fn from_netdb(error: i2pr_netdb::LocalRouterInfoError, form: PublishedForm) -> Self {
        match (error, form) {
            (i2pr_netdb::LocalRouterInfoError::UnqualifiedAddress, PublishedForm::Direct) => {
                Self::UnqualifiedAddress
            }
            (
                i2pr_netdb::LocalRouterInfoError::UnqualifiedAddress,
                PublishedForm::DirectRelayRole,
            ) => Self::UnqualifiedRelayRoleAddress,
            (
                i2pr_netdb::LocalRouterInfoError::UnqualifiedAddress,
                PublishedForm::IntroducerOnly,
            ) => Self::UnqualifiedIntroducerAddress,
            _ => Self::Construction,
        }
    }
}

/// Whether this owner owns the installed record's address.
enum Owned {
    Yes,
    No(StandDownReason),
}

/// Decides whether the installed record's address is this owner's to
/// replace.
///
/// The floodfill path's record carries `caps=f` and its own address; a
/// dual-transport record carries a second transport's address; and a
/// record this owner cannot read is not a record it may replace. Each is
/// a stand-down, counted under its own reason.
fn installed_record_ownership(installed: Option<&[u8]>) -> Owned {
    let Some(installed) = installed else {
        return Owned::No(StandDownReason::NoInstalledRecord);
    };
    let Ok(info) = i2pr_proto::RouterInfo::decode(installed, MAX_ENCODED_ROUTER_INFO) else {
        return Owned::No(StandDownReason::UnreadableRecord);
    };
    if info
        .options()
        .get("caps")
        .is_some_and(|caps| caps.contains('f'))
    {
        return Owned::No(StandDownReason::FloodfillRecord);
    }
    let addresses = info.addresses();
    match addresses.len() {
        0 => Owned::Yes,
        1 => {
            if addresses[0].transport_style() == "SSU2" {
                Owned::Yes
            } else {
                Owned::No(StandDownReason::ForeignAddress)
            }
        }
        _ => Owned::No(StandDownReason::MultipleAddresses),
    }
}
