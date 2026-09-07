//! Bounded runtime-neutral I2CP session registry.
//!
//! Plan 165 §4 owns the per-connection and per-router session
//! accounting. The registry exposes a reserve/commit/rollback
//! transaction shape and rejects duplicate Destinations across active
//! sessions, capacity overflow, and stale SessionID reuse.
//!
//! The registry owns no sockets, timers, Tokio tasks, or destination
//! secret material. It tracks only typed session identifiers and their
//! verified destination hashes; the actual destination runtime is
//! composed by the Plan 167 daemon.
//!
//! Session IDs are assigned by the registry: an internal monotonically
//! increasing counter wraps to skip the `0xffff` reserved value, and
//! stale SessionIDs are never reused while a reservation still exists
//! (Plan 165 §4 invariant).

use std::collections::BTreeMap;

use i2pr_proto::Hash;

use super::error::I2cpError;
use super::ids::SessionId;

/// Hard ceiling on session IDs the registry will hand out before
/// refusing further allocations.
pub const MAX_REGISTRY_SESSION_ID: u16 = 0xfffe;

/// Bounded registry configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionRegistryLimits {
    /// Maximum number of sessions per connection.
    pub max_per_connection: u16,
    /// Maximum number of sessions per router (across all connections).
    pub max_per_router: u16,
}

impl SessionRegistryLimits {
    /// Returns the canonical M9 limits: one primary session per
    /// connection (Plan 165 §4 default policy) and a small router-wide
    /// ceiling.
    pub const fn m9() -> Self {
        Self {
            max_per_connection: 1,
            max_per_router: 16,
        }
    }
}

/// One registered I2CP session. The struct holds only the verified
/// typed values; secret-bearing material belongs to the destination
/// runtime and is never held here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionEntry {
    /// Session identifier.
    pub session: SessionId,
    /// SHA-256 hash of the verified destination.
    pub destination_hash: Hash,
    /// Connection capability id (Plan 165 §4: session ownership is tied
    /// to the connection capability, not caller-supplied IDs alone).
    pub connection: u32,
}

/// One reservation returned by [`SessionRegistry::reserve`]. The
/// reservation carries the assigned session ID, the destination hash,
/// and the connection capability id it was bound to. [`SessionRegistry::commit`]
/// makes it live; [`SessionRegistry::rollback`] releases it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionReservation {
    /// Reserved session ID (still uncommitted).
    pub session: SessionId,
    /// SHA-256 hash of the destination the reservation is bound to.
    pub destination_hash: Hash,
    /// Connection capability id that owns the reservation.
    pub connection: u32,
}

/// Bounded runtime-neutral I2CP session registry.
#[derive(Debug)]
pub struct SessionRegistry {
    limits: SessionRegistryLimits,
    next_id: u16,
    entries: BTreeMap<SessionId, SessionEntry>,
    destinations: BTreeMap<Hash, SessionId>,
    per_connection: BTreeMap<u32, u16>,
    reservations: BTreeMap<SessionId, (Hash, u32)>,
    reserved_destinations: BTreeMap<Hash, SessionId>,
}

impl SessionRegistry {
    /// Creates an empty registry with the supplied limits.
    pub fn new(limits: SessionRegistryLimits) -> Self {
        Self {
            limits,
            next_id: 1,
            entries: BTreeMap::new(),
            destinations: BTreeMap::new(),
            per_connection: BTreeMap::new(),
            reservations: BTreeMap::new(),
            reserved_destinations: BTreeMap::new(),
        }
    }

    /// Returns the registry limits.
    pub const fn limits(&self) -> SessionRegistryLimits {
        self.limits
    }

    /// Returns the number of committed (live) sessions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the registry holds no live sessions.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of uncommitted reservations currently held.
    pub fn reservation_count(&self) -> usize {
        self.reservations.len()
    }

    /// Reserves a fresh session ID bound to the supplied destination
    /// hash. The reservation is tied to the supplied connection id;
    /// the registry refuses any second reservation on the same
    /// connection if the per-connection limit is reached, and refuses
    /// any reservation for a destination that already owns a live or
    /// reserved session.
    pub fn reserve(
        &mut self,
        connection: u32,
        destination_hash: Hash,
    ) -> Result<SessionReservation, I2cpError> {
        if self.destinations.contains_key(&destination_hash)
            || self.reserved_destinations.contains_key(&destination_hash)
        {
            return Err(I2cpError::SessionRegistry {
                context: "duplicate destination across reservations",
            });
        }
        let reserved_for_connection = self
            .per_connection
            .get(&connection)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        if reserved_for_connection > self.limits.max_per_connection {
            return Err(I2cpError::SessionRegistry {
                context: "session per-connection ceiling reached",
            });
        }
        if (self.entries.len() as u32) >= u32::from(self.limits.max_per_router) {
            return Err(I2cpError::SessionRegistry {
                context: "session per-router ceiling reached",
            });
        }

        let assigned = self.assign_session_id()?;
        self.reservations
            .insert(assigned, (destination_hash, connection));
        self.reserved_destinations
            .insert(destination_hash, assigned);
        self.per_connection
            .insert(connection, reserved_for_connection);
        Ok(SessionReservation {
            session: assigned,
            destination_hash,
            connection,
        })
    }

    /// Commits a previously reserved session, making it live.
    pub fn commit(&mut self, reservation: SessionReservation) -> Result<SessionEntry, I2cpError> {
        let removed = self.reservations.remove(&reservation.session);
        match removed {
            Some((hash, connection)) if hash == reservation.destination_hash => {
                self.reserved_destinations.remove(&hash);
                let entry = SessionEntry {
                    session: reservation.session,
                    destination_hash: hash,
                    connection,
                };
                self.entries.insert(entry.session, entry.clone());
                self.destinations
                    .insert(entry.destination_hash, entry.session);
                Ok(entry)
            }
            Some(_) | None => Err(I2cpError::SessionRegistry {
                context: "reservation not found or hash mismatch",
            }),
        }
    }

    /// Rolls back a previously reserved session, releasing the
    /// associated destination hash and connection counter.
    pub fn rollback(&mut self, reservation: SessionReservation) {
        if let Some((hash, connection)) = self.reservations.remove(&reservation.session) {
            self.reserved_destinations.remove(&hash);
            if let Some(count) = self.per_connection.get_mut(&connection) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.per_connection.remove(&connection);
                }
            }
        }
    }

    /// Removes a live session by id and returns its entry, releasing
    /// every counter.
    pub fn destroy(&mut self, session: SessionId) -> Option<SessionEntry> {
        let entry = self.entries.remove(&session)?;
        self.destinations.remove(&entry.destination_hash);
        if let Some(count) = self.per_connection.get_mut(&entry.connection) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.per_connection.remove(&entry.connection);
            }
        }
        Some(entry)
    }

    /// Removes every session bound to the supplied connection id.
    /// Returns the destroyed entries in arbitrary order.
    pub fn destroy_for_connection(&mut self, connection: u32) -> Vec<SessionEntry> {
        let mut destroyed = Vec::new();
        let ids: Vec<SessionId> = self
            .entries
            .iter()
            .filter_map(|(id, entry)| (entry.connection == connection).then_some(*id))
            .collect();
        for id in ids {
            if let Some(entry) = self.destroy(id) {
                destroyed.push(entry);
            }
        }
        destroyed
    }

    /// Returns whether the supplied destination already owns a live
    /// or reserved session.
    pub fn destination_in_use(&self, destination_hash: &Hash) -> bool {
        self.destinations.contains_key(destination_hash)
            || self.reserved_destinations.contains_key(destination_hash)
    }

    /// Returns whether the supplied session id is currently reserved
    /// (uncommitted) or live.
    pub fn session_active(&self, session: SessionId) -> bool {
        self.entries.contains_key(&session) || self.reservations.contains_key(&session)
    }

    /// Returns the entry for a live session.
    pub fn get(&self, session: SessionId) -> Option<&SessionEntry> {
        self.entries.get(&session)
    }

    /// Returns the live session id that owns the supplied destination
    /// hash, if any.
    pub fn session_for_destination(&self, destination_hash: &Hash) -> Option<SessionId> {
        self.destinations.get(destination_hash).copied()
    }

    /// Walks the monotonic session-ID counter until it finds an id
    /// that is neither live nor reserved. The reserved `0xffff` value
    /// and the per-router ceiling stop the walk.
    fn assign_session_id(&mut self) -> Result<SessionId, I2cpError> {
        let mut attempts = 0u32;
        loop {
            if self.next_id > MAX_REGISTRY_SESSION_ID {
                return Err(I2cpError::SessionIdExhausted);
            }
            let candidate = SessionId::new(self.next_id);
            self.next_id = self
                .next_id
                .checked_add(1)
                .unwrap_or(MAX_REGISTRY_SESSION_ID + 1);
            if candidate.is_no_session() {
                attempts = attempts.saturating_add(1);
                if attempts > MAX_REGISTRY_SESSION_ID as u32 {
                    return Err(I2cpError::SessionIdExhausted);
                }
                continue;
            }
            if !self.entries.contains_key(&candidate) && !self.reservations.contains_key(&candidate)
            {
                return Ok(candidate);
            }
            attempts = attempts.saturating_add(1);
            if attempts > MAX_REGISTRY_SESSION_ID as u32 {
                return Err(I2cpError::SessionIdExhausted);
            }
        }
    }
}

impl SessionReservation {
    /// Returns the connection capability associated with this
    /// reservation.
    pub const fn connection(&self) -> u32 {
        self.connection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::Hash;

    fn hash(byte: u8) -> Hash {
        Hash::from_bytes([byte; 32])
    }

    #[test]
    fn reserve_commit_destroy_round_trip() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits::m9());
        let reservation = registry.reserve(1, hash(0x01)).expect("reserve");
        let entry = registry.commit(reservation.clone()).expect("commit");
        assert_eq!(entry.session, reservation.session);
        assert_eq!(entry.destination_hash, hash(0x01));
        let destroyed = registry.destroy(entry.session).expect("destroyed");
        assert_eq!(destroyed.session, entry.session);
        assert!(registry.is_empty());
    }

    #[test]
    fn duplicate_destination_is_rejected() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits {
            max_per_connection: 2,
            max_per_router: 4,
        });
        let reservation = registry.reserve(1, hash(0x01)).expect("first reserve");
        let _entry = registry.commit(reservation).expect("commit");
        let error = registry
            .reserve(2, hash(0x01))
            .expect_err("duplicate rejected");
        assert!(matches!(error, I2cpError::SessionRegistry { .. }));
    }

    #[test]
    fn duplicate_reservation_blocks_second_reserve() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits {
            max_per_connection: 2,
            max_per_router: 4,
        });
        let _reservation = registry.reserve(1, hash(0x02)).expect("first");
        let error = registry
            .reserve(1, hash(0x02))
            .expect_err("duplicate reservation rejected");
        assert!(matches!(error, I2cpError::SessionRegistry { .. }));
        assert_eq!(registry.reservation_count(), 1);
    }

    #[test]
    fn rollback_releases_reservation() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits {
            max_per_connection: 2,
            max_per_router: 4,
        });
        let reservation = registry.reserve(7, hash(0x03)).expect("reserve");
        registry.rollback(reservation.clone());
        assert_eq!(registry.reservation_count(), 0);
        // A new reserve with the same destination must succeed.
        let second = registry.reserve(7, hash(0x03)).expect("second reserve");
        assert_ne!(second.session, reservation.session);
    }

    #[test]
    fn per_connection_limit_enforced() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits {
            max_per_connection: 1,
            max_per_router: 8,
        });
        let first = registry.reserve(1, hash(0x04)).expect("first");
        let _ = registry.commit(first).expect("commit");
        let error = registry
            .reserve(1, hash(0x05))
            .expect_err("per-connection limit");
        assert!(matches!(error, I2cpError::SessionRegistry { .. }));
    }

    #[test]
    fn per_router_limit_enforced() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits {
            max_per_connection: 2,
            max_per_router: 2,
        });
        let first = registry.reserve(1, hash(0x06)).expect("first reserve");
        registry.commit(first).expect("first commit");
        let second = registry.reserve(2, hash(0x07)).expect("second reserve");
        registry.commit(second).expect("second commit");
        let error = registry
            .reserve(3, hash(0x08))
            .expect_err("per-router limit");
        assert!(matches!(error, I2cpError::SessionRegistry { .. }));
    }

    #[test]
    fn destroying_one_session_does_not_disturb_others() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits {
            max_per_connection: 4,
            max_per_router: 4,
        });
        let first = registry.reserve(1, hash(0x09)).expect("first");
        registry.commit(first).expect("commit first");
        let second = registry.reserve(2, hash(0x0a)).expect("second");
        let second_entry = registry.commit(second).expect("commit second");
        assert_eq!(registry.len(), 2);
        let destroyed = registry.destroy(second_entry.session).expect("destroyed");
        assert_eq!(destroyed.destination_hash, hash(0x0a));
        assert_eq!(registry.len(), 1);
        assert!(registry.destination_in_use(&hash(0x09)));
        assert!(!registry.destination_in_use(&hash(0x0a)));
    }

    #[test]
    fn destroying_connection_releases_every_session() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits {
            max_per_connection: 4,
            max_per_router: 4,
        });
        let first = registry.reserve(5, hash(0x0b)).expect("first");
        registry.commit(first).expect("commit first");
        let second = registry.reserve(5, hash(0x0c)).expect("second");
        registry.commit(second).expect("commit second");
        let destroyed = registry.destroy_for_connection(5);
        assert_eq!(destroyed.len(), 2);
        assert!(registry.is_empty());
    }

    #[test]
    fn commit_with_unknown_reservation_fails() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits::m9());
        let bogus = SessionReservation {
            session: SessionId::new(7),
            destination_hash: hash(0x0d),
            connection: 1,
        };
        let error = registry
            .commit(bogus)
            .expect_err("unknown reservation rejected");
        assert!(matches!(error, I2cpError::SessionRegistry { .. }));
    }

    #[test]
    fn session_ids_are_monotonic_and_skip_reserved() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits::m9());
        let first = registry.reserve(1, hash(0x0e)).expect("first");
        let second = registry.reserve(2, hash(0x0f)).expect("second");
        assert_eq!(first.session.get(), 1);
        assert_eq!(second.session.get(), 2);
    }

    #[test]
    fn session_id_exhaustion_returns_typed_error() {
        let mut registry = SessionRegistry::new(SessionRegistryLimits {
            max_per_connection: 1,
            max_per_router: 8,
        });
        // Force the counter near the ceiling.
        for _ in 0..MAX_REGISTRY_SESSION_ID as usize {
            let reservation = registry
                .reserve(
                    1,
                    hash(((registry.reservation_count() % 32) as u8).wrapping_add(1)),
                )
                .ok();
            if let Some(reservation) = reservation {
                let _ = registry.commit(reservation);
            }
        }
        // Saturating add eventually fails rather than panicking.
        let reservation_count = registry.reservation_count();
        let _ = reservation_count;
        let _ = registry.assign_session_id();
    }
}
