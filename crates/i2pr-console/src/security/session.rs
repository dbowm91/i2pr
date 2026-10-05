//! Bounded, revocable, opaque browser sessions.
//!
//! A session identifier is 32 bytes of OS randomness rendered as hex. It is
//! an opaque lookup key and nothing else: it carries no authority, no
//! router capability, and no I2PControl token, so a leaked cookie grants
//! exactly one console session and cannot be replayed against the control
//! plane.
//!
//! Everything about the table is bounded. A live-session ceiling caps
//! cardinality; idle and absolute expiries cap lifetime; insertion evicts
//! deterministically rather than growing; and every drop path — explicit
//! logout, idle expiry, absolute expiry, table pressure — releases the
//! entry.

use std::fmt;
use std::sync::Mutex;
use std::time::Duration;

use super::auth::random_hex;

/// Length of an opaque session identifier in hex characters.
pub const SESSION_ID_HEX_LEN: usize = 64;

/// Length of a CSRF token in hex characters.
pub const CSRF_TOKEN_HEX_LEN: usize = 64;

/// Maximum accepted length of an inbound cookie value.
///
/// Cookie parsing is a hostile input path; anything longer is refused
/// before it reaches the table.
pub const MAX_COOKIE_VALUE_LEN: usize = 256;

/// Default maximum number of concurrent console sessions.
pub const DEFAULT_MAX_SESSIONS: usize = 32;

/// Hard ceiling on concurrent console sessions.
///
/// A configuration may request fewer, never more.
pub const ABSOLUTE_MAX_SESSIONS: usize = 256;

/// Default idle expiry.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Default absolute lifetime.
///
/// The absolute cap exists so a session that keeps being used cannot live
/// forever; the idle cap exists so an unattended console locks itself.
pub const DEFAULT_ABSOLUTE_TIMEOUT: Duration = Duration::from_secs(8 * 60 * 60);

/// Hard ceilings for the two expiry windows.
pub const MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(60 * 60);
pub const MAX_ABSOLUTE_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

/// Whether a session-store operation succeeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionAdmission {
    /// A session was created.
    Created,
    /// The live-session ceiling is reached; no session was created.
    Saturated,
}

/// Why a session lookup failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRejection {
    /// No session carries that identifier.
    Unknown,
    /// The session exceeded its idle window.
    IdleExpired,
    /// The session exceeded its absolute lifetime.
    AbsolutelyExpired,
}

/// Session store limits, validated on construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionLimits {
    /// Maximum concurrent sessions.
    pub max_sessions: usize,
    /// Idle expiry.
    pub idle_timeout: Duration,
    /// Absolute lifetime.
    pub absolute_timeout: Duration,
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            max_sessions: DEFAULT_MAX_SESSIONS,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            absolute_timeout: DEFAULT_ABSOLUTE_TIMEOUT,
        }
    }
}

impl SessionLimits {
    /// Validates a requested configuration against the hard ceilings.
    pub fn validate(
        max_sessions: usize,
        idle_timeout: Duration,
        absolute_timeout: Duration,
    ) -> Option<Self> {
        if max_sessions == 0 || max_sessions > ABSOLUTE_MAX_SESSIONS {
            return None;
        }
        if idle_timeout.is_zero() || idle_timeout > MAX_IDLE_TIMEOUT {
            return None;
        }
        if absolute_timeout.is_zero() || absolute_timeout > MAX_ABSOLUTE_TIMEOUT {
            return None;
        }
        // An absolute lifetime shorter than the idle window would make the
        // idle check dead code and confuse the operator.
        if absolute_timeout < idle_timeout {
            return None;
        }
        Some(Self {
            max_sessions,
            idle_timeout,
            absolute_timeout,
        })
    }
}

/// One live session.
#[derive(Clone, Debug)]
struct SessionEntry {
    csrf_token: String,
    created_at: u64,
    last_seen: u64,
}

/// An opaque session identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionId(String);

impl SessionId {
    /// Returns the identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The CSRF token bound to one session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CsrfToken(String);

impl CsrfToken {
    /// Returns the token text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The bounded in-memory session table.
///
/// Time is passed in explicitly rather than read from a clock so expiry
/// behaviour is deterministic and testable without sleeping.
#[derive(Debug)]
pub struct SessionStore {
    limits: SessionLimits,
    entries: Mutex<Vec<(SessionId, SessionEntry)>>,
}

impl SessionStore {
    /// Builds a store with validated limits.
    pub fn new(limits: SessionLimits) -> Self {
        Self {
            limits,
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Returns the configured limits.
    pub fn limits(&self) -> SessionLimits {
        self.limits
    }

    /// Returns the number of live sessions.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Returns whether the store holds no sessions.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Creates a session, or refuses when the live ceiling is reached.
    ///
    /// Saturation is a predictable failure, not a silent eviction of a live
    /// session and not unbounded growth.
    pub fn create(&self, now_secs: u64) -> Result<(SessionId, CsrfToken), SessionAdmission> {
        let mut entries = self.lock();
        Self::expire(&mut entries, now_secs, self.limits);
        if entries.len() >= self.limits.max_sessions {
            return Err(SessionAdmission::Saturated);
        }
        let id = SessionId(random_hex());
        let csrf = CsrfToken(random_hex());
        entries.push((
            SessionId(id.0.clone()),
            SessionEntry {
                csrf_token: csrf.0.clone(),
                created_at: now_secs,
                last_seen: now_secs,
            },
        ));
        Ok((id, csrf))
    }

    /// Validates a session identifier and refreshes its idle timer.
    ///
    /// An expired or unknown identifier is removed on the way out, so a
    /// revoked session cannot be reused.
    pub fn validate(&self, id: &str, now_secs: u64) -> Result<CsrfToken, SessionRejection> {
        if id.is_empty() || id.len() > MAX_COOKIE_VALUE_LEN {
            return Err(SessionRejection::Unknown);
        }
        let mut entries = self.lock();
        let Some(index) = entries.iter().position(|(key, _)| key.as_str() == id) else {
            return Err(SessionRejection::Unknown);
        };
        let entry = &entries[index].1;
        if now_secs.saturating_sub(entry.created_at) >= self.limits.absolute_timeout.as_secs() {
            entries.remove(index);
            return Err(SessionRejection::AbsolutelyExpired);
        }
        if now_secs.saturating_sub(entry.last_seen) >= self.limits.idle_timeout.as_secs() {
            entries.remove(index);
            return Err(SessionRejection::IdleExpired);
        }
        entries[index].1.last_seen = now_secs;
        Ok(CsrfToken(entries[index].1.csrf_token.clone()))
    }

    /// Compares a submitted CSRF token against the session's own.
    ///
    /// The comparison is not constant time. That is deliberate and safe:
    /// both values are 256-bit random nonces generated by this process, and
    /// the only party able to submit a guesser is the one already holding the
    /// `HttpOnly` session cookie.
    pub fn verify_csrf(&self, id: &str, token: &str, now_secs: u64) -> bool {
        if token.is_empty() || token.len() > MAX_COOKIE_VALUE_LEN {
            return false;
        }
        match self.validate(id, now_secs) {
            Ok(expected) => expected.as_str() == token,
            Err(_) => false,
        }
    }

    /// Revokes one session. Returns whether a session was actually removed.
    pub fn revoke(&self, id: &str) -> bool {
        let mut entries = self.lock();
        let before = entries.len();
        entries.retain(|(key, _)| key.as_str() != id);
        entries.len() != before
    }

    /// Drops every session, releasing all in-memory authentication state.
    ///
    /// Called on console shutdown: a restart must not resurrect a session.
    pub fn clear(&self) {
        self.lock().clear();
    }

    fn expire(entries: &mut Vec<(SessionId, SessionEntry)>, now_secs: u64, limits: SessionLimits) {
        let idle = limits.idle_timeout.as_secs();
        let absolute = limits.absolute_timeout.as_secs();
        entries.retain(|(_, entry)| {
            now_secs.saturating_sub(entry.last_seen) < idle
                && now_secs.saturating_sub(entry.created_at) < absolute
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(SessionId, SessionEntry)>> {
        match self.entries.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(limits: SessionLimits) -> SessionStore {
        SessionStore::new(limits)
    }

    #[test]
    fn created_sessions_are_opaque_random_hex() {
        let store = store(SessionLimits::default());
        let (id, csrf) = store.create(0).expect("created");
        assert_eq!(id.as_str().len(), SESSION_ID_HEX_LEN);
        assert_eq!(csrf.as_str().len(), CSRF_TOKEN_HEX_LEN);
        assert!(id.as_str().bytes().all(|b| b.is_ascii_hexdigit()));
        // Two sessions never share an identifier or a CSRF token.
        let (second, second_csrf) = store.create(0).expect("created");
        assert_ne!(id, second);
        assert_ne!(csrf, second_csrf);
    }

    #[test]
    fn session_id_carries_no_authority() {
        let store = store(SessionLimits::default());
        let (id, csrf) = store.create(0).expect("created");
        // The identifier is exactly random hex: no prefix, no encoded
        // fields, nothing to decode.
        assert_eq!(id.as_str().len(), 64);
        assert!(id.as_str().bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(csrf.as_str().len(), 64);
    }

    #[test]
    fn live_table_is_capped_and_saturation_is_predictable() {
        let limits = SessionLimits::validate(2, DEFAULT_IDLE_TIMEOUT, DEFAULT_ABSOLUTE_TIMEOUT)
            .expect("limits validate");
        let store = store(limits);
        assert_eq!(store.create(0).expect("first").0.as_str().len(), 64);
        assert_eq!(store.create(0).expect("second").0.as_str().len(), 64);
        assert_eq!(store.create(0), Err(SessionAdmission::Saturated));
        assert_eq!(store.len(), 2, "a refused creation must not grow the table");
    }

    #[test]
    fn idle_and_absolute_expiry_both_release_the_entry() {
        let limits = SessionLimits::validate(4, Duration::from_secs(60), Duration::from_secs(600))
            .expect("limits validate");
        let store = store(limits);

        let (idle_id, _) = store.create(0).expect("created");
        assert!(store.validate(idle_id.as_str(), 30).is_ok());
        // The previous validation refreshed the idle timer, so the window
        // runs from 30, not from creation.
        assert_eq!(
            store.validate(idle_id.as_str(), 90),
            Err(SessionRejection::IdleExpired)
        );
        // The expired entry is gone, not merely rejected.
        assert_eq!(store.len(), 0);
        assert_eq!(
            store.validate(idle_id.as_str(), 91),
            Err(SessionRejection::Unknown)
        );

        // An absolute expiry fires even when the session is continuously
        // used, which is the point of a second window.
        let (absolute_id, _) = store.create(1_000).expect("created");
        for step in 0..10 {
            let now = 1_000 + step * 50;
            assert!(store.validate(absolute_id.as_str(), now).is_ok());
        }
        assert_eq!(
            store.validate(absolute_id.as_str(), 1_600),
            Err(SessionRejection::AbsolutelyExpired)
        );
    }

    #[test]
    fn logout_revokes_and_the_identifier_cannot_be_reused() {
        let store = store(SessionLimits::default());
        let (id, _) = store.create(0).expect("created");
        assert!(store.validate(id.as_str(), 1).is_ok());
        assert!(store.revoke(id.as_str()));
        assert_eq!(
            store.validate(id.as_str(), 2),
            Err(SessionRejection::Unknown)
        );
        // Revoking twice is not an error path that resurrects anything.
        assert!(!store.revoke(id.as_str()));
    }

    #[test]
    fn clear_drops_every_session_for_shutdown() {
        let store = store(SessionLimits::default());
        for _ in 0..4 {
            store.create(0).expect("created");
        }
        assert_eq!(store.len(), 4);
        store.clear();
        assert!(store.is_empty());
    }

    #[test]
    fn csrf_must_match_the_bound_session_token() {
        let store = store(SessionLimits::default());
        let (id, csrf) = store.create(0).expect("created");
        assert!(store.verify_csrf(id.as_str(), csrf.as_str(), 1));
        assert!(!store.verify_csrf(id.as_str(), "wrong-token", 1));
        assert!(!store.verify_csrf(id.as_str(), "", 1));
        // Another session's token is not accepted for this session.
        let (other, _) = store.create(0).expect("created");
        assert!(store.verify_csrf(id.as_str(), csrf.as_str(), 1));
        assert!(!store.verify_csrf(other.as_str(), csrf.as_str(), 1));
        // A revoked session cannot pass CSRF even with the right token.
        store.revoke(id.as_str());
        assert!(!store.verify_csrf(id.as_str(), csrf.as_str(), 2));
    }

    #[test]
    fn hostile_cookie_values_are_refused_without_touching_the_table() {
        let store = store(SessionLimits::default());
        let (id, _) = store.create(0).expect("created");
        for hostile in [
            "",
            &"x".repeat(MAX_COOKIE_VALUE_LEN + 1),
            "; admin=1",
            "../../etc/passwd",
        ] {
            assert_eq!(
                store.validate(hostile, 1),
                Err(SessionRejection::Unknown),
                "hostile cookie value accepted: {hostile}"
            );
        }
        assert!(!store.verify_csrf(&"x".repeat(MAX_COOKIE_VALUE_LEN + 1), "t", 1));
        assert_eq!(
            store.len(),
            1,
            "hostile input must not evict a live session"
        );
        assert!(store.validate(id.as_str(), 1).is_ok());
    }

    #[test]
    fn limit_validation_rejects_out_of_range_configurations() {
        assert!(
            SessionLimits::validate(0, DEFAULT_IDLE_TIMEOUT, DEFAULT_ABSOLUTE_TIMEOUT).is_none()
        );
        assert!(
            SessionLimits::validate(
                ABSOLUTE_MAX_SESSIONS + 1,
                DEFAULT_IDLE_TIMEOUT,
                DEFAULT_ABSOLUTE_TIMEOUT
            )
            .is_none()
        );
        assert!(SessionLimits::validate(4, Duration::ZERO, DEFAULT_ABSOLUTE_TIMEOUT).is_none());
        assert!(
            SessionLimits::validate(4, MAX_IDLE_TIMEOUT * 2, DEFAULT_ABSOLUTE_TIMEOUT).is_none()
        );
        assert!(
            SessionLimits::validate(4, DEFAULT_IDLE_TIMEOUT, MAX_ABSOLUTE_TIMEOUT * 2).is_none()
        );
        // An absolute window shorter than the idle window is refused.
        assert!(
            SessionLimits::validate(4, DEFAULT_ABSOLUTE_TIMEOUT, DEFAULT_IDLE_TIMEOUT).is_none()
        );
    }
}
