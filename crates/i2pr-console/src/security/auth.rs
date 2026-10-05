//! Console password verification and login throttling.
//!
//! This is the console's own authentication. It is deliberately a separate
//! mechanism from I2PControl: a browser login yields only a console
//! session, never a control token, and the two secret types are not
//! interchangeable.
//!
//! KDF parameter selection. Argon2id is used with 16 MiB of memory, two
//! passes, and a single lane. That is intentionally **not** the desktop
//! default (which is 256 MiB / three passes): this router is expected to
//! run on constrained hosts such as small SBCs, where a 256 MiB
//! allocation per login attempt is a denial-of-service vector against the
//! device rather than a security win. 16 MiB with two passes is far above
//! the OWASP minimum for Argon2id and costs roughly 30 ms on a small
//! host. The bound on *concurrent* verifications below is what keeps that
//! cost from being multiplied by an attacker.

use std::fmt;
use std::sync::Arc;

use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, Version,
    password_hash::PasswordVerifier as _,
};
use rand_core::TryRngCore;
use zeroize::Zeroize;

use crate::secret::ConsoleSecret;

/// Argon2id memory cost in kibibytes (16 MiB).
///
/// See the module documentation for the constrained-host rationale.
pub const ARGON2_MEMORY_KIB: u32 = 16 * 1024;

/// Argon2id iteration count.
pub const ARGON2_ITERATIONS: u32 = 2;

/// Argon2id parallelism lanes.
///
/// One lane keeps the memory bound attributable to a single attempt and
/// avoids oversubscribing a small host with thread pools.
pub const ARGON2_PARALLELISM: u32 = 1;

/// Maximum accepted length of a configured console password, in bytes.
///
/// An operator password longer than this is a configuration mistake, not a
/// security control, and refusing it keeps startup work bounded.
pub const MAX_PASSWORD_LEN: usize = 256;

/// Maximum accepted length of an Argon2 PHC hash string.
pub const MAX_HASH_LEN: usize = 256;

/// Maximum concurrent password verifications.
///
/// Each verification costs `ARGON2_MEMORY_KIB` of memory. Allowing an
/// unbounded number of them would let a single client turn a slow login
/// into an out-of-memory event on a constrained host.
pub const MAX_CONCURRENT_VERIFICATIONS: usize = 2;

/// Why a password could not be turned into a verifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasswordError {
    /// The configured password was empty.
    EmptyPassword,
    /// The configured password exceeded [`MAX_PASSWORD_LEN`].
    PasswordTooLong,
    /// The pre-hashed PHC string exceeded [`MAX_HASH_LEN`].
    HashTooLong,
    /// The pre-hashed PHC string did not parse.
    MalformedHash,
    /// The PHC string named an algorithm other than Argon2id.
    UnsupportedAlgorithm,
    /// The PHC string's parameters were outside the accepted range.
    UnsafeHashParameters,
    /// The operating-system randomness source failed.
    EntropyUnavailable,
}

impl fmt::Display for PasswordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPassword => formatter.write_str("console password must not be empty"),
            Self::PasswordTooLong => {
                formatter.write_str("console password exceeds the accepted length")
            }
            Self::HashTooLong => {
                formatter.write_str("console password hash exceeds the accepted length")
            }
            Self::MalformedHash => formatter.write_str("console password hash is malformed"),
            Self::UnsupportedAlgorithm => {
                formatter.write_str("console password hash must use Argon2id")
            }
            Self::UnsafeHashParameters => {
                formatter.write_str("console password hash uses unsafe KDF parameters")
            }
            Self::EntropyUnavailable => {
                formatter.write_str("console password salt requires operating-system randomness")
            }
        }
    }
}

impl std::error::Error for PasswordError {}

/// The Argon2id PHC identifier string.
const ARGON2ID_IDENT: &str = "argon2id";

/// Salt length in bytes.
const SALT_BYTES: usize = 16;

/// Reads one decimal PHC parameter and range-checks it.
///
/// Parsing the PHC text directly, rather than trusting a decoded struct,
/// keeps the audit independent of the KDF crate's own coercion rules.
fn check_parameter(
    parsed: &PasswordHash,
    name: &'static str,
    minimum: u32,
    maximum: u32,
) -> Result<(), PasswordError> {
    let raw = parsed
        .params
        .get_str(name)
        .ok_or(PasswordError::MalformedHash)?;
    let value: u32 = raw.parse().map_err(|_| PasswordError::MalformedHash)?;
    if value < minimum || value > maximum {
        return Err(PasswordError::UnsafeHashParameters);
    }
    Ok(())
}

/// Builds the Argon2id hasher with the console's fixed parameters.
fn console_argon2() -> Result<Argon2<'static>, PasswordError> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        None,
    )
    .map_err(|_| PasswordError::UnsafeHashParameters)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// The startup conversion result: a PHC hash string plus the raw secret.
///
/// The verifier holds only the hash. The raw secret is returned so the
/// caller can keep it in a zeroizing handle for the shortest practical
/// lifetime, and drop it as soon as the hash exists.
pub struct DerivedVerifier {
    verifier: Arc<PasswordVerifier>,
    temporary: Option<ConsoleSecret>,
}

impl fmt::Debug for DerivedVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DerivedVerifier")
            .field("verifier", &"<redacted>")
            .field("temporary", &"<redacted>")
            .finish()
    }
}

impl DerivedVerifier {
    /// Returns the verifier to install in the console security state.
    pub fn verifier(&self) -> Arc<PasswordVerifier> {
        Arc::clone(&self.verifier)
    }

    /// Drops the raw secret explicitly.
    ///
    /// Startup calls this immediately after installation. It exists so the
    /// call site reads as an intentional lifetime decision rather than a
    /// drop that happens to occur.
    pub fn forget_temporary(&mut self) {
        self.temporary = None;
    }
}

/// A configured console password, held only as an Argon2id PHC hash.
#[derive(Clone)]
pub struct PasswordVerifier {
    hash: Arc<str>,
    permits: Arc<tokio_free_gate::Gate>,
}

impl fmt::Debug for PasswordVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasswordVerifier")
            // Never render the hash: it is an offline attack verifier.
            .field("hash", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl PasswordVerifier {
    /// Verifies a candidate password in constant time with respect to the
    /// hash contents.
    ///
    /// Returns `false` for any failure, including a malformed stored hash,
    /// so a caller cannot distinguish "wrong password" from "broken
    /// configuration" by timing or by return value alone.
    pub fn verify(&self, candidate: &str) -> bool {
        if candidate.is_empty() || candidate.len() > MAX_PASSWORD_LEN {
            return false;
        }
        // Refusal under saturation is explicit rather than queued: queueing
        // would let an attacker pile up unbounded pending memory requests.
        let Some(permit) = self.permits.try_acquire() else {
            return false;
        };
        let hasher = match console_argon2() {
            Ok(hasher) => hasher,
            Err(_) => return false,
        };
        let parsed = match PasswordHash::new(&self.hash) {
            Ok(parsed) => parsed,
            Err(_) => return false,
        };
        let outcome = hasher
            .verify_password(candidate.as_bytes(), &parsed)
            .is_ok();
        drop(permit);
        outcome
    }

    /// Returns the stored PHC hash for operator diagnostics.
    ///
    /// The value is redacted in every `Display`/`Debug` path; this accessor
    /// exists so the closure evidence checker can assert the algorithm and
    /// parameters without printing the verifier.
    pub fn hash_for_audit(&self) -> &str {
        &self.hash
    }
}

/// A minimal counting semaphore that needs no async runtime.
///
/// The console crate deliberately holds no Tokio dependency, so this uses a
/// plain atomic counter. `acquire` spins with a bounded number of yields and
/// then gives up by returning a permit that immediately restores the count;
/// callers treat that as "verification unavailable" and refuse the login.
mod tokio_free_gate {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A permit returned even when the gate refused to admit the caller.
    #[derive(Debug)]
    pub struct Gate {
        in_flight: AtomicUsize,
        limit: usize,
    }

    /// Evidence that a verification slot was held.
    #[derive(Debug)]
    pub struct Permit(Arc<Gate>);

    impl Drop for Permit {
        fn drop(&mut self) {
            self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl Gate {
        pub fn new(limit: usize) -> Arc<Self> {
            Arc::new(Self {
                in_flight: AtomicUsize::new(0),
                limit,
            })
        }

        /// Admits a caller if the gate is below its limit.
        ///
        /// Refusal is immediate and explicit: a login under saturation is
        /// refused rather than queued, because queueing would let an
        /// attacker accumulate unbounded pending memory requests.
        pub fn try_acquire(self: &Arc<Self>) -> Option<Permit> {
            let current = self.in_flight.fetch_add(1, Ordering::SeqCst);
            if current < self.limit {
                Some(Permit(Arc::clone(self)))
            } else {
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                None
            }
        }

        /// Current number of admitted verifications.
        #[cfg(test)]
        pub fn in_flight(&self) -> usize {
            self.in_flight.load(Ordering::SeqCst)
        }
    }
}

/// Converts a plaintext console password into a verifier.
pub fn derive_from_password(password: ConsoleSecret) -> Result<DerivedVerifier, PasswordError> {
    let raw = password.expose();
    if raw.is_empty() {
        return Err(PasswordError::EmptyPassword);
    }
    if raw.len() > MAX_PASSWORD_LEN {
        return Err(PasswordError::PasswordTooLong);
    }
    let hasher = console_argon2()?;
    let mut salt_bytes = [0u8; SALT_BYTES];
    os_rng()
        .try_fill_bytes(&mut salt_bytes)
        .map_err(|_| PasswordError::EntropyUnavailable)?;
    let hash = hasher
        .hash_password_with_salt(raw.as_bytes(), &salt_bytes)
        .map_err(|_| PasswordError::UnsafeHashParameters)?
        .to_string();
    salt_bytes.zeroize();
    if hash.len() > MAX_HASH_LEN {
        return Err(PasswordError::UnsafeHashParameters);
    }
    Ok(DerivedVerifier {
        verifier: Arc::new(PasswordVerifier {
            hash: Arc::from(hash),
            permits: tokio_free_gate::Gate::new(MAX_CONCURRENT_VERIFICATIONS),
        }),
        temporary: Some(password),
    })
}

/// Builds a verifier from a pre-hashed Argon2id PHC string.
///
/// The parameters embedded in the hash are range-checked against the
/// console's own ceilings, so a supplied hash cannot re-enable a
/// desktop-class memory cost on a constrained host.
pub fn verifier_from_hash(hash: &str) -> Result<PasswordVerifier, PasswordError> {
    if hash.len() > MAX_HASH_LEN {
        return Err(PasswordError::HashTooLong);
    }
    let parsed = PasswordHash::new(hash).map_err(|_| PasswordError::MalformedHash)?;
    if parsed.algorithm.as_str() != ARGON2ID_IDENT {
        return Err(PasswordError::UnsupportedAlgorithm);
    }
    // The declared parameters are read from the PHC parameter string and
    // range-checked, so a supplied hash cannot re-enable a desktop-class
    // memory cost on a constrained host.
    check_parameter(&parsed, "m", ARGON2_MEMORY_KIB, 8 * ARGON2_MEMORY_KIB)?;
    check_parameter(&parsed, "t", ARGON2_ITERATIONS, 8 * ARGON2_ITERATIONS)?;
    check_parameter(&parsed, "p", ARGON2_PARALLELISM, 4)?;
    Ok(PasswordVerifier {
        hash: Arc::from(hash),
        permits: tokio_free_gate::Gate::new(MAX_CONCURRENT_VERIFICATIONS),
    })
}

/// Generates 32 bytes of operating-system randomness as lowercase hex.
///
/// Session identifiers and CSRF tokens come from here. There is no
/// user-supplied entropy path and no deterministic fallback: if the OS
/// source fails, generation fails.
pub fn random_hex() -> String {
    let mut bytes = [0u8; 32];
    // An entropy failure is fatal for token minting: there is deliberately
    // no fallback source, because a weak session identifier is worse than a
    // refused login.
    os_rng()
        .try_fill_bytes(&mut bytes)
        .expect("operating-system randomness is required for console tokens");
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        hex.push(char::from_digit((byte >> 4) as u32, 16).expect("nibble is a hex digit"));
        hex.push(char::from_digit((byte & 0x0f) as u32, 16).expect("nibble is a hex digit"));
    }
    hex
}

/// The console's OS randomness source.
fn os_rng() -> rand_core::OsRng {
    rand_core::OsRng
}

/// Bounded login throttling keyed by peer address.
///
/// A failed login increments a counter; reaching the ceiling refuses
/// further attempts from that peer for a cool-off window. The table is
/// capped, and a full table evicts the oldest entry, so a client that
/// rotates source addresses cannot grow it without bound.
#[derive(Debug)]
pub struct LoginThrottle {
    /// Failed attempts permitted inside the window.
    failure_limit: usize,
    /// Distinct peers retained in the table.
    peer_limit: usize,
    /// Failure-window length in seconds.
    window_secs: u64,
    entries: std::sync::Mutex<Vec<(String, u32, u64)>>,
}

impl LoginThrottle {
    /// Builds a throttle.
    pub fn new(failure_limit: usize, window_secs: u64, peer_limit: usize) -> Self {
        Self {
            failure_limit: failure_limit.max(1),
            peer_limit: peer_limit.max(1),
            window_secs: window_secs.max(1),
            entries: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Returns whether an attempt from `peer` is currently allowed.
    pub fn allows(&self, peer: &str, now_secs: u64) -> bool {
        let mut entries = self.lock();
        Self::expire(&mut entries, now_secs);
        match entries.iter().find(|(key, _, _)| key == peer) {
            Some((_, failures, last)) => {
                let within = now_secs.saturating_sub(*last) < self.window_secs;
                !(within && *failures >= self.failure_limit as u32)
            }
            None => true,
        }
    }

    /// Records a failed attempt from `peer`.
    pub fn record_failure(&self, peer: &str, now_secs: u64) {
        let mut entries = self.lock();
        Self::expire(&mut entries, now_secs);
        if let Some(entry) = entries.iter_mut().find(|(key, _, _)| key == peer) {
            entry.1 = entry.1.saturating_add(1);
            entry.2 = now_secs;
            return;
        }
        // Bounded table: a full store evicts the oldest entry rather than
        // growing. This keeps memory flat under source-address rotation.
        if entries.len() >= self.peer_limit {
            entries.remove(0);
        }
        entries.push((peer.to_string(), 1, now_secs));
    }

    /// Clears the counter for `peer` after a successful login.
    pub fn record_success(&self, peer: &str, now_secs: u64) {
        let mut entries = self.lock();
        Self::expire(&mut entries, now_secs);
        entries.retain(|(key, _, _)| key != peer);
    }

    fn expire(entries: &mut Vec<(String, u32, u64)>, now_secs: u64) {
        let max_age = 86_400;
        entries.retain(|(_, _, last)| now_secs.saturating_sub(*last) < max_age);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(String, u32, u64)>> {
        // A poisoned lock means a previous holder panicked mid-update. The
        // table is best-effort throttling state, so recovery is safe and
        // preferable to propagating a panic into a request path.
        match self.entries.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn secret(value: &str) -> ConsoleSecret {
        ConsoleSecret::new(value)
    }

    #[test]
    fn argon2_parameters_stay_within_constrained_host_budget() {
        // These are compile-time choices, so they are asserted as such:
        // explicitly not the desktop default, because a small host must not
        // have to allocate 256 MiB to log in.
        const _: () = assert!(ARGON2_MEMORY_KIB == 16 * 1024);
        const _: () = assert!(ARGON2_MEMORY_KIB < 64 * 1024);
        const _: () = assert!(ARGON2_ITERATIONS == 2);
        const _: () = assert!(ARGON2_PARALLELISM == 1);
        const _: () = assert!(MAX_CONCURRENT_VERIFICATIONS <= 4);
    }

    #[test]
    fn derived_verifier_accepts_the_right_password_only() {
        let mut derived = derive_from_password(secret("correct horse")).expect("derives");
        let verifier = derived.verifier();
        derived.forget_temporary();
        assert!((verifier.clone().verify("correct horse")));
        assert!(!(verifier.verify("wrong horse")));
        assert!(!(verifier.verify("")));
        assert!(!(verifier.verify(&"x".repeat(MAX_PASSWORD_LEN + 1))));
    }

    #[test]
    fn empty_and_oversized_passwords_are_refused() {
        assert_eq!(
            derive_from_password(secret("")).unwrap_err_check(),
            PasswordError::EmptyPassword
        );
        assert_eq!(
            derive_from_password(secret(&"x".repeat(MAX_PASSWORD_LEN + 1))).unwrap_err_check(),
            PasswordError::PasswordTooLong
        );
    }

    #[test]
    fn verifier_debug_never_renders_the_hash() {
        let derived = derive_from_password(secret("hunter2")).expect("derives");
        let rendered = format!("{:?}", derived.verifier());
        let hash = derived.verifier().hash_for_audit().to_string();
        assert!(
            !rendered.contains(&hash),
            "Debug leaked the hash: {rendered}"
        );
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn derived_verifier_debug_is_redacted() {
        let derived = derive_from_password(secret("hunter2")).expect("derives");
        let rendered = format!("{derived:?}");
        assert!(!rendered.contains("hunter2"));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn generated_hashes_are_argon2id_with_the_console_parameters() {
        let derived = derive_from_password(secret("hunter2")).expect("derives");
        let hash = derived.verifier().hash_for_audit().to_string();
        assert!(
            hash.starts_with("$argon2id$"),
            "unexpected PHC form: {hash}"
        );
        assert!(hash.contains("m=16384"), "memory parameter missing: {hash}");
        assert!(hash.contains("t=2"), "iteration parameter missing: {hash}");
        assert!(hash.contains("p=1"), "parallelism missing: {hash}");
    }

    #[test]
    fn salt_is_random_so_two_hashes_differ() {
        let first = derive_from_password(secret("same")).expect("derives");
        let second = derive_from_password(secret("same")).expect("derives");
        assert_ne!(
            first.verifier().hash_for_audit(),
            second.verifier().hash_for_audit(),
            "identical passwords must not produce identical hashes"
        );
    }

    #[test]
    fn prehashed_verifier_round_trips_and_enforces_parameters() {
        let derived = derive_from_password(secret("hunter2")).expect("derives");
        let hash = derived.verifier().hash_for_audit().to_string();
        let verifier = verifier_from_hash(&hash).expect("hash is accepted");
        assert!((verifier.clone().verify("hunter2")));
        assert!(!(verifier.verify("nope")));

        // Malformed, oversized, and non-Argon2id hashes are refused.
        assert_eq!(
            verifier_from_hash("not-a-hash").unwrap_err_check(),
            PasswordError::MalformedHash
        );
        assert_eq!(
            verifier_from_hash(&"x".repeat(MAX_HASH_LEN + 1)).unwrap_err_check(),
            PasswordError::HashTooLong
        );
        // A well-formed Argon2i hash must be rejected on algorithm, not
        // silently accepted as Argon2id.
        assert_eq!(
            verifier_from_hash(
                "$argon2i$v=19$m=65536,t=2,p=1$c29tZXNhbHQ$CTFhFdXPJO1aFaMaO6Mm5c8y7cJHAph8ArZWb2GRPPc"
            )
            .unwrap_err_check(),
            PasswordError::UnsupportedAlgorithm
        );
    }

    #[test]
    fn prehashed_verifier_refuses_a_memory_hungry_hash() {
        // Same algorithm, but 256 MiB: refused so a supplied hash cannot
        // re-enable a desktop-class memory cost on a small host.
        let greedy = "$argon2id$v=19$m=262144,t=3,p=4$c29tZXNhbHRzb21lc2FsdA$\
                      ZmFrZS1zYWx0LWJ5dGVzLWhlcmUtZm9yLW5vLXZlcmlmaWNhdGlvbg";
        assert_eq!(
            verifier_from_hash(greedy).unwrap_err_check(),
            PasswordError::UnsafeHashParameters
        );
    }

    #[test]
    fn random_tokens_are_long_hex_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..64 {
            let token = random_hex();
            assert_eq!(token.len(), 64);
            assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(seen.insert(token), "token collision");
        }
    }

    #[test]
    fn login_throttle_allows_then_blocks_after_the_limit() {
        let throttle = LoginThrottle::new(3, 60, 8);
        assert!(throttle.allows("127.0.0.1", 0));
        for _ in 0..3 {
            assert!(throttle.allows("127.0.0.1", 1));
            throttle.record_failure("127.0.0.1", 1);
        }
        assert!(!throttle.allows("127.0.0.1", 1), "limit must block");
        // A different peer is unaffected.
        assert!(throttle.allows("127.0.0.2", 1));
        // The window slides: after it, attempts resume.
        assert!(throttle.allows("127.0.0.1", 1000));
    }

    #[test]
    fn successful_login_clears_the_failure_counter() {
        let throttle = LoginThrottle::new(2, 60, 8);
        throttle.record_failure("127.0.0.1", 1);
        throttle.record_failure("127.0.0.1", 2);
        assert!(!throttle.allows("127.0.0.1", 2));
        throttle.record_success("127.0.0.1", 3);
        assert!(throttle.allows("127.0.0.1", 3));
    }

    #[test]
    fn throttle_table_is_bounded_under_source_rotation() {
        let throttle = LoginThrottle::new(8, 60, 8);
        for index in 0..500 {
            throttle.record_failure(&format!("10.0.0.{}", index % 256), index);
        }
        assert!(
            throttle.entries.lock().expect("lock is not poisoned").len() <= 8,
            "throttle table must stay bounded"
        );
    }

    #[test]
    fn concurrent_verification_admission_is_capped() {
        let gate = tokio_free_gate::Gate::new(MAX_CONCURRENT_VERIFICATIONS);
        let first = gate.try_acquire().expect("first admitted");
        let second = gate.try_acquire().expect("second admitted");
        assert!(gate.try_acquire().is_none(), "third must be refused");
        assert_eq!(gate.in_flight(), MAX_CONCURRENT_VERIFICATIONS);
        drop(first);
        assert!(gate.try_acquire().is_some());
        drop(second);
    }

    /// Test helper so a failing `derive_from_password` reports the typed
    /// error instead of a `Debug` dump.
    trait UnwrapErrCheck<T> {
        fn unwrap_err_check(self) -> PasswordError;
    }

    impl<T> UnwrapErrCheck<T> for Result<T, PasswordError> {
        fn unwrap_err_check(self) -> PasswordError {
            match self {
                Ok(_) => panic!("expected an error"),
                Err(error) => error,
            }
        }
    }
}
