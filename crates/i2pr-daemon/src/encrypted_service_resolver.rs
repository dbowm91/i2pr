//! Plan 349: the encrypted-LeaseSet2 **consumer** lifecycle owner.
//!
//! # What this owns
//!
//! i2pr could publish an encrypted LeaseSet2 long before it could resolve one. The publisher
//! side is real and live (`service_product.rs`); the consumer side was not, which is why
//! `EncryptedLeaseSet2Resolver` had **zero** production callers and every round trip in the
//! repository was in-process.
//!
//! This module is the missing composition:
//!
//! ```text
//! encrypted service address (+ optional lookup secret, optional client credential)
//!   -> daily blinded key -> blinded DHT storage key
//!   -> DatabaseLookup (composed by the caller's existing lookup transport)
//!   -> DatabaseStore reply
//!   -> ValidatedEncryptedLeaseSet2 (bounded type-11 profile, ADR 0032)
//!   -> storage-key gate -> layer decrypt -> inner LeaseSet2
//!   -> handed back for the ordinary destination path
//! ```
//!
//! It owns exactly the things an owner must own and nothing else: in-flight request identity,
//! a bounded in-flight table, a deadline per request, cancellation, and the credential. It
//! does **not** own the lookup transport — [`begin`](EncryptedServiceResolver::begin) hands the caller the target key and the
//! request id, and the caller composes `DatabaseLookup` through its existing path
//! (`netdb_tunnels` / `destination_tunnels`) rather than this module growing a second one.
//!
//! # Why the resolver is held, not rebuilt
//!
//! Each in-flight request holds its own `EncryptedLeaseSet2Resolver`, which holds its own
//! `BlindingSchedule` and therefore its own unblinded private key. That is deliberate: the
//! resolver derives the day's blinded key from the address, and a resolver built from a
//! *different* identity than the credential would derive a different storage key and make the
//! storage-key gate silently meaningless. One identity per request, held by one owner.
//!
//! The resolver is not `Debug`-leaking (it holds a private scalar through the schedule) and is
//! never logged. A `PendingEncryptedResolve` is likewise not `Debug`, so a debug format of
//! the coordinator cannot print a private key or a credential.
//!
//! # Bounds and lifecycle
//!
//! [`MAX_CONCURRENT_ENCRYPTED_RESOLVES`] caps the in-flight table. Every request carries a
//! deadline; [`expire`](EncryptedServiceResolver::expire) releases a request whose deadline has
//! passed; [`cancel`](EncryptedServiceResolver::cancel) releases one
//! on explicit cancellation. A request is removed on success, on failure, on expiry, and on
//! cancellation, and the retained reply path is released with it — the same lease discipline
//! the type-3 lookup owner uses.
//!
//! # What this deliberately does not do
//!
//! It does not decrypt a record it did not validate, it does not accept a record whose key is
//! not the one it asked for, and it does not retry on its own. A caller that wants a retry
//! decides that, because retry policy is a liveness decision and belongs to the owner that
//! has the deadline.

use i2pr_client::{EncryptedLeaseSet2Resolver, EncryptedLeaseSetError, ResolvedEncryptedService};
use i2pr_crypto::X25519PrivateKey;
use i2pr_netdb::{
    AuthClientPublicKey, BlindedStorageKey, Els2AuthScheme, Els2ClientAuth, Els2ValidationContext,
    Els2ValidationError, PskClientKey, ValidatedEncryptedLeaseSet2,
};
use i2pr_proto::{
    DatabaseStoreData, DatabaseStoreMessage, EncryptedServiceAddress, Hash,
    MAX_COMMON_STRUCTURE_SIZE,
};

/// Maximum number of encrypted-service resolutions in flight at once.
///
/// The bound is small and explicit: an encrypted resolve is a two-message round trip that a
/// caller awaits, so a large table would be a queue in disguise. Capacity 1, exact load, and
/// max+1 are all covered by tests.
pub const MAX_CONCURRENT_ENCRYPTED_RESOLVES: usize = 8;

/// Maximum encoded size accepted for a type-5 record, in bytes.
///
/// Bounded before any parsing or hashing, matching the type-5 record ceiling the validator
/// and the floodfill already apply.
const MAX_TYPE5_RECORD_BYTES: usize = i2pr_netdb::MAX_ELS2_RECORD_LENGTH;

/// Stable identifier for one in-flight encrypted-service resolution.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EncryptedResolveId(pub u64);

/// Typed consumer errors. Every variant is a refusal, never a fallback to a weaker check.
///
/// Not `Clone`/`PartialEq`, matching the underlying error types: an authorization failure can
/// carry a client identifier, and duplicating that into a cloneable error would widen how far
/// an identity-bearing value can travel. Callers match on the variant, not on equality.
#[derive(Debug)]
pub enum EncryptedServiceConsumerError {
    /// The in-flight table is full. The caller may retry after another request completes.
    TooManyResolves,
    /// The address, secret, or credential was rejected before any lookup was composed.
    ///
    /// This is a construction error, not a lookup miss: a wrong-length, non-canonical, or
    /// unsupported address means something very different from "not found".
    InvalidAddress(EncryptedLeaseSetError),
    /// The request id is not in flight. Either it already completed, expired, or was
    /// cancelled.
    UnknownRequest,
    /// The reply was not a type-5 record.
    NotEncryptedLeaseSet,
    /// The record was too large to accept.
    RecordTooLarge,
    /// The record failed bounded validation: bad type-11 signature under the closed profile
    /// policy, a storage-key mismatch, or a stale/expired publication.
    RecordRejected(Els2ValidationError),
    /// The reply's key is not the key this request asked for, so it is not this request's
    /// answer. Kept distinct from a validation failure so a misrouted reply is not reported
    /// as a corrupt record.
    KeyMismatch,
    /// A credential could not be copied into the request-owned form.
    CredentialNotCopyable(Els2AuthScheme),
    /// The record validated but the layer-1 authorization refused this client, or no
    /// credential was supplied for an address that requires one. Carries no detail on
    /// purpose: the refusal must not become a side channel for probing which credential a
    /// service accepts.
    NotAuthorized,
    /// The record validated and passed the storage-key gate but could not be decrypted into
    /// an inner LeaseSet2. The rendered message is retained for operator diagnosis; it is a
    /// message about the record, never about a key.
    Undecryptable(String),
    /// The clock value the caller supplied is not usable for daily blinding.
    InvalidDay(EncryptedLeaseSetError),
}

impl core::fmt::Display for EncryptedServiceConsumerError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooManyResolves => formatter.write_str("too many encrypted resolves in flight"),
            Self::InvalidAddress(error) => write!(formatter, "encrypted address rejected: {error}"),
            Self::UnknownRequest => formatter.write_str("encrypted resolve is not in flight"),
            Self::NotEncryptedLeaseSet => {
                formatter.write_str("reply is not an encrypted LeaseSet2 record")
            }
            Self::RecordTooLarge => formatter.write_str("encrypted LeaseSet2 record is too large"),
            Self::RecordRejected(error) => {
                write!(formatter, "encrypted LeaseSet2 record rejected: {error}")
            }
            Self::KeyMismatch => {
                formatter.write_str("reply key does not match the requested blinded storage key")
            }
            Self::CredentialNotCopyable(scheme) => {
                write!(
                    formatter,
                    "client credential ({scheme:?}) could not be retained"
                )
            }
            Self::NotAuthorized => formatter.write_str("encrypted service refused this client"),
            Self::Undecryptable(detail) => {
                write!(
                    formatter,
                    "encrypted LeaseSet2 could not be decrypted: {detail}"
                )
            }
            Self::InvalidDay(error) => {
                write!(formatter, "encrypted resolve day is not usable: {error}")
            }
        }
    }
}

impl std::error::Error for EncryptedServiceConsumerError {}

/// One in-flight encrypted-service resolution.
///
/// Deliberately **not** `Debug`: it transitively holds a `BlindingSchedule` (and therefore an
/// unblinded private scalar) and an optional client credential. A `#[derive(Debug)]` here would
/// be a secret-leak path reachable from any `{:#?}` of the coordinator.
struct PendingEncryptedResolve {
    resolver: EncryptedLeaseSet2Resolver,
    credential: Option<OwnedClientCredential>,
    storage_key: BlindedStorageKey,
    deadline_ms: u64,
}

/// The request-owned copy of a client authorization credential.
///
/// [`Els2ClientAuth`] borrows its key material, so a coordinator that has to hold a request
/// across an `await` boundary — or simply across a call that takes `&mut self` — needs an owned
/// form. This enum is that form, and it is the only place in the consumer path where client key
/// material is stored rather than borrowed.
///
/// `PskClientKey` and `X25519PrivateKey` are both `Zeroizing`, so dropping the request wipes
/// the bytes. The enum itself is deliberately **not** `Debug` and not `Clone`: a derived
/// `Debug` would be a path from any `{:#?}` of the coordinator to a pre-shared key, and a
/// derived `Clone` would duplicate one into memory that is not zeroized.
enum OwnedClientCredential {
    Psk(PskClientKey),
    Dh {
        private: X25519PrivateKey,
        public: AuthClientPublicKey,
    },
}

impl OwnedClientCredential {
    /// Takes an owned copy of a borrowed credential.
    fn from_borrowed(value: &Els2ClientAuth<'_>) -> Result<Self, Els2AuthSchemeError> {
        match value {
            Els2ClientAuth::Psk(key) => Ok(Self::Psk(PskClientKey::from_bytes(
                key.as_bytes().to_owned(),
            ))),
            Els2ClientAuth::Dh { private, public } => Ok(Self::Dh {
                private: X25519PrivateKey::from_bytes(*private.secret_bytes()),
                public: *public,
            }),
        }
    }

    /// Reborrows the owned key material as the API the resolver takes.
    ///
    /// Split out as a named method so the borrow is created at the call site rather than
    /// spanning a `&mut` borrow of the resolver that lives in the same request.
    fn as_borrowed(&self) -> Els2ClientAuth<'_> {
        match self {
            Self::Psk(key) => Els2ClientAuth::Psk(key),
            Self::Dh { private, public } => Els2ClientAuth::Dh {
                private,
                public: *public,
            },
        }
    }

    /// Returns the scheme this credential will present.
    const fn scheme(&self) -> Els2AuthScheme {
        match self {
            Self::Psk(_) => Els2AuthScheme::Psk,
            Self::Dh { .. } => Els2AuthScheme::Dh,
        }
    }
}

/// A credential that could not be copied into owned form.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Els2AuthSchemeError {
    /// The scheme that could not be copied.
    pub scheme: Els2AuthScheme,
}

/// Bounded owner for encrypted-service resolutions.
///
/// Cheap to hold and free of ambient authority: it owns the in-flight table and nothing else.
/// The caller supplies the clock on every call, so there is no hidden time source and no
/// background task.
#[derive(Default)]
pub struct EncryptedServiceResolver {
    pending: std::collections::BTreeMap<EncryptedResolveId, PendingEncryptedResolve>,
    next_id: u64,
}

/// Presence, count, and the scheme of any in-flight credential. Never the key material.
impl core::fmt::Debug for EncryptedServiceResolver {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("EncryptedServiceResolver")
            .field("in_flight", &self.pending.len())
            .field("capacity", &MAX_CONCURRENT_ENCRYPTED_RESOLVES)
            .field(
                "credential_schemes",
                &self
                    .pending
                    .values()
                    .map(|pending| {
                        pending
                            .credential
                            .as_ref()
                            .map(OwnedClientCredential::scheme)
                    })
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl EncryptedServiceResolver {
    /// Returns the number of resolutions in flight.
    pub fn in_flight(&self) -> usize {
        self.pending.len()
    }

    /// Returns whether the in-flight table is full.
    pub fn is_full(&self) -> bool {
        self.pending.len() >= MAX_CONCURRENT_ENCRYPTED_RESOLVES
    }

    /// Begins a resolution for an address that requires no per-client credential.
    ///
    /// Returns the request id and the blinded storage key to look up. The caller composes the
    /// `DatabaseLookup` itself through its existing transport and later hands the
    /// `DatabaseStore` reply to [`Self::ingest_store`].
    pub fn begin(
        &mut self,
        address: &EncryptedServiceAddress,
        secret: Option<&str>,
        now_seconds: u32,
        deadline_ms: u64,
    ) -> Result<(EncryptedResolveId, BlindedStorageKey), EncryptedServiceConsumerError> {
        let resolver = EncryptedLeaseSet2Resolver::new(address, secret)
            .map_err(EncryptedServiceConsumerError::InvalidAddress)?;
        self.begin_with_credential(resolver, None, now_seconds, deadline_ms)
    }

    /// Begins a resolution for an address that declares per-client authorization.
    ///
    /// The credential is the *client's* key material, never the publisher's. A caller that
    /// has no credential for an address that requires one gets a refusal at
    /// [`EncryptedServiceConsumerError::InvalidAddress`] rather than a lookup that is certain
    /// to fail later.
    pub fn begin_authorized(
        &mut self,
        address: &EncryptedServiceAddress,
        secret: Option<&str>,
        credential: Els2ClientAuth<'_>,
        now_seconds: u32,
        deadline_ms: u64,
    ) -> Result<(EncryptedResolveId, BlindedStorageKey), EncryptedServiceConsumerError> {
        let resolver = EncryptedLeaseSet2Resolver::new_authorized(address, secret)
            .map_err(EncryptedServiceConsumerError::InvalidAddress)?;
        let owned = OwnedClientCredential::from_borrowed(&credential)
            .map_err(|error| EncryptedServiceConsumerError::CredentialNotCopyable(error.scheme))?;
        self.begin_with_credential(resolver, Some(owned), now_seconds, deadline_ms)
    }

    fn begin_with_credential(
        &mut self,
        mut resolver: EncryptedLeaseSet2Resolver,
        credential: Option<OwnedClientCredential>,
        now_seconds: u32,
        deadline_ms: u64,
    ) -> Result<(EncryptedResolveId, BlindedStorageKey), EncryptedServiceConsumerError> {
        if self.is_full() {
            return Err(EncryptedServiceConsumerError::TooManyResolves);
        }
        let storage_key = resolver
            .current_storage_key(now_seconds)
            .map_err(EncryptedServiceConsumerError::InvalidDay)?;
        let id = EncryptedResolveId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("encrypted resolve id space");
        self.pending.insert(
            id,
            PendingEncryptedResolve {
                resolver,
                credential,
                storage_key,
                deadline_ms,
            },
        );
        Ok((id, storage_key))
    }

    /// Returns the blinded storage key an in-flight request is waiting for.
    pub fn target_key(
        &self,
        id: EncryptedResolveId,
    ) -> Result<BlindedStorageKey, EncryptedServiceConsumerError> {
        self.pending
            .get(&id)
            .map(|pending| pending.storage_key)
            .ok_or(EncryptedServiceConsumerError::UnknownRequest)
    }

    /// Returns whether an in-flight request has passed its deadline.
    pub fn is_expired(&self, id: EncryptedResolveId, now_ms: u64) -> bool {
        self.pending
            .get(&id)
            .is_some_and(|pending| now_ms >= pending.deadline_ms)
    }

    /// Releases one in-flight request without resolving it.
    ///
    /// The lease is released on every path: success, failure, expiry, and explicit
    /// cancellation all go through removal, so a caller cannot leak a slot by abandoning a
    /// future.
    pub fn cancel(&mut self, id: EncryptedResolveId) -> Result<(), EncryptedServiceConsumerError> {
        self.pending
            .remove(&id)
            .map(|_| ())
            .ok_or(EncryptedServiceConsumerError::UnknownRequest)
    }

    /// Releases every request whose deadline has passed, returning their ids.
    pub fn expire(&mut self, now_ms: u64) -> Vec<EncryptedResolveId> {
        let expired: Vec<EncryptedResolveId> = self
            .pending
            .iter()
            .filter(|(_, pending)| now_ms >= pending.deadline_ms)
            .map(|(id, _)| *id)
            .collect();
        for id in &expired {
            self.pending.remove(id);
        }
        expired
    }

    /// Validates a reply and decrypts it into an ordinary `LeaseSet2`.
    ///
    /// The order is deliberate and fail-closed at every step:
    ///
    /// 1. the request must be in flight — an unsolicited reply is ignored by the caller, not
    ///    resolved here;
    /// 2. the message must carry a type-5 record and be within the size bound, checked
    ///    **before** parsing or hashing;
    /// 3. the reply's key must be the blinded storage key this request asked for, so a
    ///    misrouted or forged reply cannot be attributed to this request;
    /// 4. bounded validation runs, which includes the closed type-11 profile policy of
    ///    ADR 0032 and the storage-key gate;
    /// 5. the layer-1 authorization check runs for an authorized service, so an unauthorized
    ///    client is refused **before** it can use the inner LeaseSet;
    /// 6. only then is the record decrypted.
    ///
    /// The in-flight lease is released on every outcome, including failure, so a bad reply
    /// cannot strand the table.
    pub fn ingest_store(
        &mut self,
        id: EncryptedResolveId,
        message: &DatabaseStoreMessage,
        now_seconds: u32,
    ) -> Result<ResolvedEncryptedService, EncryptedServiceConsumerError> {
        if !self.pending.contains_key(&id) {
            return Err(EncryptedServiceConsumerError::UnknownRequest);
        }
        // Take ownership of the request up front so every exit path releases the lease.
        let mut pending = self
            .pending
            .remove(&id)
            .ok_or(EncryptedServiceConsumerError::UnknownRequest)?;

        let record = match &message.data {
            DatabaseStoreData::EncryptedLeaseSet(record) => record.as_ref(),
            _ => return Err(EncryptedServiceConsumerError::NotEncryptedLeaseSet),
        };
        if record.encode_to_vec(MAX_TYPE5_RECORD_BYTES).is_err() {
            return Err(EncryptedServiceConsumerError::RecordTooLarge);
        }
        if message.key != *pending.storage_key.as_hash() {
            return Err(EncryptedServiceConsumerError::KeyMismatch);
        }

        let validated = ValidatedEncryptedLeaseSet2::validate(
            record.clone(),
            Some(pending.storage_key),
            Els2ValidationContext::new(now_seconds),
        )
        .map_err(EncryptedServiceConsumerError::RecordRejected)?;

        // The credential is taken out of the request first so it does not borrow the request
        // while the resolver inside that same request is being driven mutably.
        let credential = pending.credential.take();
        let borrowed = credential.as_ref().map(OwnedClientCredential::as_borrowed);
        let resolved =
            match borrowed.as_ref() {
                Some(credential) => pending
                    .resolver
                    .resolve_with_auth(&validated, now_seconds, credential)
                    .map_err(|_| EncryptedServiceConsumerError::NotAuthorized)?,
                // No credential was supplied for this request. The resolver already enforces that
                // an address which requires a client key cannot be resolved without one, so
                // reaching here with a requirement unmet is a caller error rather than a bad
                // record; it is reported as an authorization refusal, not as a decode failure.
                None => pending
                    .resolver
                    .resolve(&validated, now_seconds)
                    .map_err(|error| match error {
                        EncryptedLeaseSetError::ClientAuthorizationRequired
                        | EncryptedLeaseSetError::CredentialNotSupplied
                        | EncryptedLeaseSetError::NotAuthorized { .. } => {
                            EncryptedServiceConsumerError::NotAuthorized
                        }
                        _ => EncryptedServiceConsumerError::Undecryptable(error.to_string()),
                    })?,
            };
        // Explicitly drop the owned key material now rather than at end of scope, so the
        // window in which a private key sits in this frame is as short as the API allows.
        drop(credential);
        Ok(resolved)
    }
}

/// The maximum encoded size the coordinator accepts, exposed for tests and evidence.
pub const MAX_ENCRYPTED_RECORD_BYTES: usize = MAX_TYPE5_RECORD_BYTES;

/// Asserts a coordinator is bounded by [`MAX_CONCURRENT_ENCRYPTED_RESOLVES`] for tests.
pub fn in_flight_within_bound(owner: &EncryptedServiceResolver) -> bool {
    owner.in_flight() <= MAX_CONCURRENT_ENCRYPTED_RESOLVES
}

/// Convenience: the lookup target a caller should place in a `DatabaseLookup`.
pub fn lookup_target(key: &BlindedStorageKey) -> Hash {
    *key.as_hash()
}

/// Convenience: a size guard for callers holding a record they have not yet built a message
/// from, so the bound is applied before an allocation grows.
pub fn record_within_bound(message: &DatabaseStoreMessage) -> bool {
    match &message.data {
        DatabaseStoreData::EncryptedLeaseSet(record) => {
            record.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE).is_ok()
        }
        _ => false,
    }
}

/// Plan 351 — binds an unwrapped inner LeaseSet2 to the `.b33` address it must belong to,
/// returning the destination hash the record should be installed under.
///
/// # Why the address cannot supply the hash
///
/// `EncryptedServiceAddress` carries the **unblinded signing public key** plus both signature
/// types. The protocol layer's own documentation says the value is not a `Destination` hash and
/// must never be treated as one. A `Destination` hash is the SHA-256 of a canonical `Destination`
/// encoding, which needs the ECIES public key, the signing key, the certificate, and the padding
/// — and the address publishes only one of those four. So the hash can only come from the inner
/// record, which is signed.
///
/// # Why that is not a trust problem
///
/// Because the inner record supplies the hash, the hash would otherwise be whatever any valid
/// LeaseSet2 claimed to be — and a publisher could hand a consumer a perfectly valid, working
/// record for a *different* destination than the `.b33` names. The two checks below close that.
///
/// ```text
/// b33 signing key  ==  inner Destination.signing_key     ==>  the hash may be trusted
/// b33 unblinded sigtype  ==  inner signing key type       ==>  no declared-type ambiguity
/// ```
///
/// The binding is therefore transitive through a signature: the hash is not trusted because the
/// record says so, it is trusted because the record's signature is checked against a key the
/// operator obtained out of band from the address.
///
/// Both checks are fail-closed and neither has a fallback. Returns the inner destination's hash,
/// or `None` when the record does not belong to this address.
pub fn bind_inner_to_address(
    address: &EncryptedServiceAddress,
    inner: &i2pr_proto::LeaseSet2,
) -> Option<i2pr_netdb::DestinationHash> {
    let destination = inner.header().destination();
    let signing_key = destination.signing_key();
    if signing_key.as_bytes() != address.public_key() {
        return None;
    }
    if signing_key.key_type().code() != address.unblinded_sigtype() {
        return None;
    }
    destination
        .hash()
        .ok()
        .map(i2pr_netdb::DestinationHash::from_hash)
}
