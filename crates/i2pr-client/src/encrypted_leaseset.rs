//! Plan 332: the client half of encrypted LeaseSet2 — building and signing a
//! no-client-authorization type-5 record, and resolving a blinded base-32
//! address into a decrypted inner LeaseSet2.
//!
//! # Division of labour
//!
//! This module owns no sockets, no timers, and no publication driver. It turns
//! a local destination plus its daily blinded material into a ready-to-send
//! `DatabaseStoreMessage`, and turns a retrieved record plus a b33 address into
//! an ordinary `LeaseSet2` that the existing destination path already
//! understands. Actually moving those messages is the daemon's job, through the
//! same publication path every other LeaseSet2 uses.
//!
//! # Why the two halves are not symmetric
//!
//! Publishing needs the *blinded private* key, which only the owner has.
//! Resolving needs only the *blinded public* key, which anyone with the
//! address can derive. That asymmetry is the whole point of the design, and it
//! is why [`EncryptedLeaseSet2Resolver`] cannot be used to impersonate a
//! service: it can read a record addressed to it, and it cannot produce one.
//!
//! # Blinding secret handling
//!
//! The optional lookup secret is supplied by the caller and validated on the
//! way in. A configured secret is never silently dropped: if it is rejected for
//! length, the call fails rather than continuing with no secret, because a
//! fallback would publish a record under a key the clients are not looking for
//! and would look like a plain lookup miss.

use core::fmt;

use i2pr_crypto::red25519::{BlindingDay, Red25519PrivateScalar, Red25519PublicKey, sign};
use i2pr_netdb::{
    BlindedStorageKey, BlindingIdentity, BlindingSchedule, BlindingScheduleConfig, DecryptedEls2,
    DestinationHash, Els2Error, Els2ValidationError, LeaseSet2ValidationContext,
    LeaseSet2ValidationError, OwnerBlinding, ValidatedEncryptedLeaseSet2, ValidatedLeaseSet2,
    decrypt_no_auth_outer_ciphertext, encrypt_no_auth_outer_ciphertext,
};
use i2pr_proto::{
    B32_BLINDED_SIGTYPE, B32_UNBLINDED_SIGTYPE_ED25519, B32_UNBLINDED_SIGTYPE_RED25519,
    DatabaseStoreData, DatabaseStoreMessage, ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
    EncryptedLeaseSet2, EncryptedServiceAddress, INNER_LEASE_SET2_STORE_TYPE,
    INNER_META_LEASE_SET2_STORE_TYPE, LeaseSet2, MAX_COMMON_STRUCTURE_SIZE,
};
use rand_core::TryCryptoRng;
use thiserror::Error;

/// Errors produced by the encrypted-LeaseSet2 client paths.
#[derive(Debug, Error)]
pub enum EncryptedLeaseSetError {
    /// The supplied address was not a valid encrypted-service base-32 address.
    #[error("encrypted-service address rejected: {0}")]
    Address(i2pr_proto::Base32Error),
    /// The address's unblinded signature type is not one this router blinds for.
    #[error("encrypted-service address unblinded sigtype {code} is neither 7 nor 11")]
    UnsupportedUnblindedSigtype {
        /// The rejected signature type code.
        code: u16,
    },
    /// The address's blinded signature type was not 11.
    #[error("encrypted-service address blinded sigtype {code} is not 11")]
    UnsupportedBlindedSigtype {
        /// The rejected signature type code.
        code: u16,
    },
    /// The address carried no usable public key.
    #[error("encrypted-service address public key is not a valid curve point")]
    InvalidPublicKey,
    /// The address declared a per-client key requirement this floor does not implement.
    #[error("encrypted-service address requires per-client authorization")]
    ClientAuthorizationRequired,
    /// The inner record was not a standard LeaseSet2.
    #[error("decrypted inner record is not a standard LeaseSet2")]
    UnexpectedInnerStoreType,
    /// The decrypted inner LeaseSet2 failed ordinary LeaseSet2 validation.
    #[error("decrypted inner LeaseSet2 failed validation: {0}")]
    InnerValidation(#[from] LeaseSet2ValidationError),
    /// A type-5 validation or layer-crypto step failed.
    #[error(transparent)]
    Els2(#[from] Els2Error),
    /// A type-5 validation step failed.
    #[error(transparent)]
    Els2Validation(#[from] Els2ValidationError),
    /// A type-5 structural record failed its codec.
    #[error(transparent)]
    Codec(#[from] i2pr_proto::CodecError),
    /// The signing random source failed.
    #[error("encrypted LeaseSet2 randomness unavailable")]
    RandomnessUnavailable,
    /// The schedule does not hold the owner's private key material.
    #[error("encrypted LeaseSet2 publication requires an owner blinding schedule")]
    NotAnOwner,
    /// No record was available for the derived storage key.
    #[error("no encrypted LeaseSet2 record for the derived storage key")]
    RecordMissing,
}

/// A resolved encrypted service: the address, the day's blinded key, and the
/// decrypted inner LeaseSet2.
///
/// The three values are returned together on purpose. Returning them as a unit
/// is what makes a mixed result unrepresentable: a caller cannot accidentally
/// use today's storage key with yesterday's record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedEncryptedService {
    address: EncryptedServiceAddress,
    daily_blinding: i2pr_netdb::DailyBlinding,
    inner: LeaseSet2,
}

impl ResolvedEncryptedService {
    /// Returns the address the service was resolved from.
    pub const fn address(&self) -> &EncryptedServiceAddress {
        &self.address
    }

    /// Returns the daily blinded material used for the lookup.
    pub const fn daily_blinding(&self) -> &i2pr_netdb::DailyBlinding {
        &self.daily_blinding
    }

    /// Returns the decrypted inner LeaseSet2, ready for the ordinary
    /// destination path.
    pub const fn inner_lease_set2(&self) -> &LeaseSet2 {
        &self.inner
    }

    /// Consumes the value and returns the inner LeaseSet2.
    pub fn into_inner_lease_set2(self) -> LeaseSet2 {
        self.inner
    }
}

/// Builds and signs the no-client-authorization encrypted LeaseSet2 for a local
/// destination.
///
/// The publisher borrows the daily blinded material from an owner
/// [`BlindingSchedule`]. The blinded private scalar is not cloned and not
/// `Debug`, so it cannot leak through a log line; the record it produces contains
/// only the blinded public key.
#[derive(Debug)]
pub struct EncryptedLeaseSet2Publisher<'a> {
    schedule: &'a BlindingSchedule,
}

impl<'a> EncryptedLeaseSet2Publisher<'a> {
    /// Borrows an owner schedule as a publisher.
    pub fn new(schedule: &'a BlindingSchedule) -> Self {
        Self { schedule }
    }

    /// Returns the encrypted-service address for this destination.
    ///
    /// The address carries the **unblinded** public key plus both signature
    /// types, because a client needs the unblinded key to derive the daily
    /// blinded key. The `requires_blinding_secret` flag is the operator's
    /// declaration; the secret itself is never encoded.
    pub fn address(
        &self,
        requires_blinding_secret: bool,
    ) -> Result<EncryptedServiceAddress, EncryptedLeaseSetError> {
        let identity = self.schedule.identity();
        EncryptedServiceAddress::new(
            identity.unblinded_sigtype().code(),
            B32_BLINDED_SIGTYPE,
            *identity.unblinded_public_key().as_bytes(),
            requires_blinding_secret,
            false,
        )
        .map_err(EncryptedLeaseSetError::Address)
    }

    /// Builds and signs the type-5 record that wraps `inner`.
    ///
    /// `now_seconds` selects the UTC day, so the record is signed with that
    /// day's blinded key and is filed under that day's storage key. `expires` is
    /// the requested absolute expiration, clamped against both the next UTC
    /// midnight and the two-byte offset field — see
    /// [`i2pr_netdb::day_bound_expiry_offset`].
    pub fn build_record<R: TryCryptoRng + ?Sized>(
        &self,
        inner: &LeaseSet2,
        now_seconds: u32,
        expires_seconds: u32,
        rng: &mut R,
    ) -> Result<EncryptedLeaseSet2, EncryptedLeaseSetError> {
        let published_seconds = inner.published_seconds();
        let owner: OwnerBlinding =
            self.schedule
                .owner_blinding(now_seconds)
                .map_err(|error| match error {
                    Els2Error::Blinding(_) => EncryptedLeaseSetError::NotAnOwner,
                    other => EncryptedLeaseSetError::Els2(other),
                })?;
        let daily = *owner.daily();
        let credentials = self
            .schedule
            .identity()
            .credentials_for(daily.blinded_public_key());

        let boundary = i2pr_netdb::next_utc_day_boundary_seconds(published_seconds);
        let expires_offset =
            i2pr_netdb::day_bound_expiry_offset(published_seconds, expires_seconds, boundary);

        let inner_bytes = inner.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)?;
        let mut outer_salt = [0_u8; i2pr_netdb::ELS2_SALT_LENGTH];
        let mut inner_salt = [0_u8; i2pr_netdb::ELS2_SALT_LENGTH];
        fill_random(&mut outer_salt, rng)?;
        fill_random(&mut inner_salt, rng)?;
        let outer_ciphertext = encrypt_no_auth_outer_ciphertext(
            &credentials,
            published_seconds,
            INNER_LEASE_SET2_STORE_TYPE,
            &inner_bytes,
            &outer_salt,
            &inner_salt,
        )?;

        // The signature is the final field, so the record is first built with a
        // placeholder to obtain the exact preimage, then rebuilt with the real
        // signature. Both builds derive the same signed region, so the bytes a
        // verifier sees are exactly the bytes that were signed.
        let probe = EncryptedLeaseSet2::new(
            ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
            daily.blinded_public_key().as_bytes().to_vec(),
            published_seconds,
            expires_offset,
            None,
            outer_ciphertext,
            vec![0_u8; 64],
        )?;
        let signature = sign(owner.blinded_private_key(), probe.signed_bytes(), rng)
            .map_err(|_| EncryptedLeaseSetError::RandomnessUnavailable)?;
        EncryptedLeaseSet2::new(
            ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE,
            daily.blinded_public_key().as_bytes().to_vec(),
            published_seconds,
            expires_offset,
            None,
            probe.outer_ciphertext().to_vec(),
            signature.as_bytes().to_vec(),
        )
        .map_err(EncryptedLeaseSetError::Codec)
    }

    /// Builds the signed record and wraps it in a `DatabaseStore` message
    /// addressed to the day's storage key.
    ///
    /// This is the hand-off point to the ordinary NetDB publication path: the
    /// caller sends the returned message exactly as it would send any other
    /// LeaseSet2 publication.
    pub fn build_database_store<R: TryCryptoRng + ?Sized>(
        &self,
        inner: &LeaseSet2,
        now_seconds: u32,
        expires_seconds: u32,
        rng: &mut R,
    ) -> Result<(BlindedStorageKey, DatabaseStoreMessage), EncryptedLeaseSetError> {
        let record = self.build_record(inner, now_seconds, expires_seconds, rng)?;
        let key = BlindedStorageKey::from_hash(i2pr_crypto::red25519::blinded_storage_key(
            &Red25519PublicKey::decode(record.blinded_public_key())
                .map_err(|_| EncryptedLeaseSetError::InvalidPublicKey)?,
        ));
        let message = DatabaseStoreMessage {
            key: *key.as_hash(),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::EncryptedLeaseSet(Box::new(record)),
        };
        Ok((key, message))
    }
}

fn fill_random<R: TryCryptoRng + ?Sized>(
    buffer: &mut [u8],
    rng: &mut R,
) -> Result<(), EncryptedLeaseSetError> {
    rng.try_fill_bytes(buffer)
        .map_err(|_| EncryptedLeaseSetError::RandomnessUnavailable)
}

/// Resolves an encrypted-service base-32 address into a decrypted LeaseSet2.
///
/// The resolver is lookup-only: it holds a public [`BlindingIdentity`] and
/// therefore cannot sign a record. That is enforced by the type, not by
/// convention.
#[derive(Debug)]
pub struct EncryptedLeaseSet2Resolver {
    /// The resolver owns exactly one identity, held by the schedule.
    ///
    /// Holding a second, separately-constructed identity would be a live hazard:
    /// a schedule built without the lookup secret would derive a different
    /// storage key from the identity that derives the credentials, and the
    /// storage-key gate would silently stop meaning anything. Routing every
    /// derivation through one owner makes that mistake unrepresentable.
    schedule: BlindingSchedule,
    requires_client_key: bool,
}

impl EncryptedLeaseSet2Resolver {
    /// Builds a resolver for one address.
    ///
    /// The address is validated immediately: a wrong-length, non-canonical, or
    /// unsupported address is a construction error rather than a lookup miss,
    /// because the two failure modes mean very different things to a user.
    pub fn new(
        address: &EncryptedServiceAddress,
        secret: Option<&str>,
    ) -> Result<Self, EncryptedLeaseSetError> {
        if address.requires_client_key() {
            return Err(EncryptedLeaseSetError::ClientAuthorizationRequired);
        }
        let unblinded_sigtype = if address.unblinded_sigtype() == B32_UNBLINDED_SIGTYPE_RED25519 {
            i2pr_proto::SigningKeyType::RedDsaSha512Ed25519
        } else if address.unblinded_sigtype() == B32_UNBLINDED_SIGTYPE_ED25519 {
            i2pr_proto::SigningKeyType::EdDsaSha512Ed25519
        } else {
            return Err(EncryptedLeaseSetError::UnsupportedUnblindedSigtype {
                code: address.unblinded_sigtype(),
            });
        };
        if address.blinded_sigtype() != B32_BLINDED_SIGTYPE {
            return Err(EncryptedLeaseSetError::UnsupportedBlindedSigtype {
                code: address.blinded_sigtype(),
            });
        }
        let public_key = Red25519PublicKey::decode(address.public_key())
            .map_err(|_| EncryptedLeaseSetError::InvalidPublicKey)?;
        let identity = BlindingIdentity::new(public_key, unblinded_sigtype, secret)
            .map_err(EncryptedLeaseSetError::Els2)?;
        let requires_client_key = address.requires_client_key();
        Ok(Self {
            schedule: BlindingSchedule::new(identity, BlindingScheduleConfig::default()),
            requires_client_key,
        })
    }

    /// Returns the identity the resolver derives daily material from.
    pub const fn identity(&self) -> &BlindingIdentity {
        self.schedule.identity()
    }

    /// Returns the storage key the resolver looks up for `day`.
    pub fn storage_key(
        &self,
        day: BlindingDay,
    ) -> Result<BlindedStorageKey, EncryptedLeaseSetError> {
        Ok(self.schedule.identity().derive(day)?.storage_key())
    }

    /// Returns the storage key the resolver looks up at `now_seconds`.
    pub fn current_storage_key(
        &mut self,
        now_seconds: u32,
    ) -> Result<BlindedStorageKey, EncryptedLeaseSetError> {
        Ok(self.schedule.current(now_seconds)?.storage_key())
    }

    /// Decrypts a retrieved record into an ordinary LeaseSet2.
    ///
    /// The resolver derives the day's blinded public key from the address, so
    /// the record's own blinded key is checked against it before anything is
    /// decrypted: a record fetched under the wrong day, or one published under a
    /// different lookup secret, fails here as a storage-key mismatch rather than
    /// as a decryption failure.
    ///
    /// # What the lookup secret does and does not protect
    ///
    /// The secret enters `GENERATE_ALPHA`, so it changes the *blinded* key and
    /// therefore the DHT storage key. It does not enter the credential or the
    /// subcredential, which are functions of the unblinded public key and the
    /// blinded public key alone. The consequence is worth stating plainly,
    /// because it is easy to assume the opposite:
    ///
    /// - a party that holds a b33 address but not the secret **cannot find the
    ///   record**, because it derives a different storage key — this is the
    ///   secret's purpose, and it is a discovery control;
    /// - a party that already holds the record bytes and the b33 address **can
    ///   decrypt them**, because the credentials are public-key derivations.
    ///   Per-client authorization (Plan 333) is what raises that bar.
    ///
    /// A floodfill, meanwhile, holds the record but never learns the unblinded
    /// public key, so it cannot derive the subcredential at all.
    pub fn resolve(
        &mut self,
        record: &ValidatedEncryptedLeaseSet2,
        now_seconds: u32,
    ) -> Result<ResolvedEncryptedService, EncryptedLeaseSetError> {
        if self.requires_client_key {
            return Err(EncryptedLeaseSetError::ClientAuthorizationRequired);
        }
        let daily = self.schedule.current(now_seconds)?;
        if record.storage_key() != daily.storage_key() {
            return Err(EncryptedLeaseSetError::Els2Validation(
                Els2ValidationError::StorageKeyMismatch,
            ));
        }
        let credentials = self
            .schedule
            .identity()
            .credentials_for(daily.blinded_public_key());
        let decrypted: DecryptedEls2 = decrypt_no_auth_outer_ciphertext(
            &credentials,
            record.published_seconds(),
            record.record().outer_ciphertext(),
        )?;
        if decrypted.store_type() != INNER_LEASE_SET2_STORE_TYPE {
            return Err(EncryptedLeaseSetError::UnexpectedInnerStoreType);
        }
        let inner = decrypted.into_lease_set2(MAX_COMMON_STRUCTURE_SIZE)?;
        // The inner record gets the same treatment an unencrypted LeaseSet2
        // would: it is validated, not merely parsed. Feeding a decrypted record
        // into the destination path without validation would let whoever wrote
        // the ciphertext choose the routing keys.
        let destination_hash = inner.key_hash()?;
        ValidatedLeaseSet2::from_lease_set2(
            inner.clone(),
            Some(DestinationHash::from_hash(destination_hash)),
            LeaseSet2ValidationContext::new(now_seconds),
        )?;
        Ok(ResolvedEncryptedService {
            address: self.address(),
            daily_blinding: daily,
            inner,
        })
    }

    /// Returns the address this resolver was built for.
    fn address(&self) -> EncryptedServiceAddress {
        let identity = self.schedule.identity();
        EncryptedServiceAddress::new(
            identity.unblinded_sigtype().code(),
            B32_BLINDED_SIGTYPE,
            *identity.unblinded_public_key().as_bytes(),
            false,
            false,
        )
        .expect("an identity built from a validated address re-encodes")
    }
}

impl fmt::Display for ResolvedEncryptedService {
    /// Prints the address and the storage key only.
    ///
    /// The inner LeaseSet2 is never formatted: a `Display` implementation is one
    /// of the easiest ways to get routing keys into a log line.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "encrypted-service address (storage key derived for day {:?})",
            self.daily_blinding.day()
        )
    }
}

/// Converts an Ed25519 signing seed into the unblinded scalar an owner schedule
/// needs.
pub fn owner_scalar_from_seed(seed: &[u8; 32]) -> Red25519PrivateScalar {
    i2pr_netdb::unblinded_scalar_from_ed25519_seed(seed)
}

/// The MetaLeaseSet inner store type, re-exported for callers that publish one.
pub const ELS2_INNER_META_STORE_TYPE: u8 = INNER_META_LEASE_SET2_STORE_TYPE;

#[cfg(test)]
mod tests {}
