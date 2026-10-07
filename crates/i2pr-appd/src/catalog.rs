//! Plan 369 §9 — the trusted source of launch authority.
//!
//! # Why this is a trait with exactly one production implementation
//!
//! Launch authority must be *manager-created*: something inside the trusted
//! manager has to decide, on its own evidence, that one application may be
//! launched with one set of capabilities. Plan 369 has no such thing — no
//! package store, no signature check, no grant persistence, no policy engine,
//! and no administrator (Plan 369 "Out of scope"). So the shipped `i2pr-appd`
//! binary owns [`EmptyCatalog`], which yields nothing, and therefore
//! never launches anything.
//!
//! That is not a stub that gets filled in later. It is the whole reason a router
//! running Plan 369 cannot be talked into starting an application: there is no
//! input path from the daemon, from the wire, or from configuration into
//! [`LaunchAuthority`]. The manager protocol has no manager-receivable launch
//! request, so the daemon cannot ask. See [`crate::authority`] for the sealed
//! types themselves.
//!
//! # Where the other implementations come from
//!
//! `tests/` and later plans supply their own. WP5's fixture manager is a
//! **separate binary target** rather than a config field, so a production
//! `i2pr-appd` can never be made to launch an operator-supplied executable.

use crate::authority::LaunchAuthority;

/// A bounded, non-blocking source of launch authorities.
///
/// `next_launch` is called from the manager body, not from a task, so an
/// implementation must not block. It returns `None` when it has nothing left;
/// the manager launches whatever it yields once and then stops, because Plan 369
/// has no autostart or relaunch semantics ("Out of scope: autostart/restart
/// request semantics").
pub trait LaunchCatalog: Send + Sync {
    fn next_launch(&self) -> Option<LaunchAuthority>;
}

/// The only production value: no authority owner, so no launches.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyCatalog;

impl LaunchCatalog for EmptyCatalog {
    fn next_launch(&self) -> Option<LaunchAuthority> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_production_catalog_yields_nothing_and_claims_no_authority() {
        let catalog = EmptyCatalog;
        assert!(catalog.next_launch().is_none());
        assert!(catalog.next_launch().is_none());
    }

    #[test]
    fn a_catalog_is_usable_as_a_trait_object() {
        let catalog: Box<dyn LaunchCatalog> = Box::new(EmptyCatalog);
        assert!(catalog.next_launch().is_none());
    }
}
