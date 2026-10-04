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
    DestinationHash, Els2AuthError, Els2Error, Els2ValidationError, LeaseSet2ValidationContext,
    LeaseSet2ValidationError, OwnerBlinding, ValidatedEncryptedLeaseSet2, ValidatedLeaseSet2,
    decrypt_no_auth_outer_ciphertext, decrypt_outer_ciphertext, encrypt_no_auth_outer_ciphertext,
    encrypt_outer_ciphertext,
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
    /// The record requires per-client authorization and the caller supplied no
    /// credential at all. Distinct from [`Self::NotAuthorized`] only in that
    /// the caller can fix it by presenting *some* authorized credential.
    #[error(
        "encrypted-service address requires per-client authorization but no client credential was supplied"
    )]
    CredentialNotSupplied,
    /// Per-client authorization rejected the supplied credential, or the
    /// authorization block was malformed or inconsistent.
    ///
    /// The reason is a bounded, non-secret description. It never names a
    /// client, a key, or a cookie, and it deliberately does not distinguish a
    /// missing key from a wrong one: a caller that could tell them apart would
    /// learn how close a guess was, and there is nothing actionable in the
    /// difference.
    #[error("encrypted-service address refused the supplied client credential: {reason}")]
    NotAuthorized {
        /// A bounded, non-secret description of the refusal.
        reason: String,
    },
    /// A client-authorization derivation step failed.
    #[error(transparent)]
    Auth(#[from] Els2AuthError),
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

    /// Builds the address for a service that requires per-client authorization.
    ///
    /// This is the address an authorized service publishes, and it differs from
    /// [`Self::address`] in exactly one bit: `B32_FLAG_REQUIRES_CLIENT_KEY` is
    /// set, which tells a prospective client before it fetches anything that it
    /// will need a credential. A resolver built for this address refuses the
    /// unauthenticated [`EncryptedLeaseSet2Resolver::resolve`] path.
    pub fn authorized_address(
        &self,
        requires_blinding_secret: bool,
    ) -> Result<EncryptedServiceAddress, EncryptedLeaseSetError> {
        let identity = self.schedule.identity();
        EncryptedServiceAddress::new(
            identity.unblinded_sigtype().code(),
            B32_BLINDED_SIGTYPE,
            *identity.unblinded_public_key().as_bytes(),
            requires_blinding_secret,
            true,
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
        Self::build(address, secret, false)
    }

    /// Builds a resolver for an address that declares per-client authorization.
    ///
    /// This is the constructor an authorized service's address needs. The
    /// credential is deliberately *not* taken here: an address says which
    /// services require a key, not which key this particular caller holds, and
    /// the same resolver may be pointed at several services with different
    /// client keys. The credential is supplied per resolution in
    /// [`Self::resolve_with_auth`].
    ///
    /// [`Self::resolve`] on such a resolver fails with
    /// [`EncryptedLeaseSetError::CredentialNotSupplied`] rather than attempting
    /// an unauthenticated decryption.
    pub fn new_authorized(
        address: &EncryptedServiceAddress,
        secret: Option<&str>,
    ) -> Result<Self, EncryptedLeaseSetError> {
        Self::build(address, secret, address.requires_client_key())
    }

    /// Shared constructor: the address is validated immediately, because a
    /// wrong-length, non-canonical, or unsupported address is a construction
    /// error rather than a lookup miss, and the two mean very different things
    /// to a user.
    fn build(
        address: &EncryptedServiceAddress,
        secret: Option<&str>,
        requires_client_key: bool,
    ) -> Result<Self, EncryptedLeaseSetError> {
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
            return Err(EncryptedLeaseSetError::CredentialNotSupplied);
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
    ///
    /// The per-client-authorization flag is carried through, so a resolved
    /// service reports the same address a caller would have looked up rather
    /// than one that silently understates its access control.
    fn address(&self) -> EncryptedServiceAddress {
        let identity = self.schedule.identity();
        EncryptedServiceAddress::new(
            identity.unblinded_sigtype().code(),
            B32_BLINDED_SIGTYPE,
            *identity.unblinded_public_key().as_bytes(),
            false,
            self.requires_client_key,
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

// --- Plan 333: per-client authorization ------------------------------------------------------

use i2pr_crypto::X25519PrivateKey;
use i2pr_netdb::{
    AuthBlock, AuthCookie, Els2AuthScheme, Els2AuthorizationServerConfig, Els2ClientAuth,
    draw_generation_secrets,
};

impl<'a> EncryptedLeaseSet2Publisher<'a> {
    /// Builds and signs a type-5 record that only the configured clients can open.
    ///
    /// The generation secrets — the `authCookie` and, for the Diffie-Hellman
    /// scheme, the ephemeral keypair — are drawn here, per generation, and
    /// dropped when the record is built. Nothing about them is stored, so a
    /// restart cannot reuse a previous generation's cookie.
    ///
    /// The resulting record names the authorized client count in the clear, which
    /// is the privacy property the specification asks for: a passive observer sees
    /// *how many* clients are subscribed without learning which.
    pub fn build_authorized_record<R: rand_core::TryCryptoRng + ?Sized>(
        &self,
        authorization: &Els2AuthorizationServerConfig,
        inner: &i2pr_proto::LeaseSet2,
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

        let (cookie, esk) = draw_generation_secrets(rng).map_err(EncryptedLeaseSetError::Auth)?;
        let block: AuthBlock = authorization
            .build_block(
                &cookie,
                credentials.subcredential(),
                published_seconds,
                Some(&esk),
                rng,
            )
            .map_err(EncryptedLeaseSetError::Auth)?;
        drop(esk);
        let auth_data = block
            .encode_to_vec()
            .map_err(EncryptedLeaseSetError::Auth)?;
        let layer1_flags = block.scheme().to_flags();

        let boundary = i2pr_netdb::next_utc_day_boundary_seconds(published_seconds);
        let expires_offset =
            i2pr_netdb::day_bound_expiry_offset(published_seconds, expires_seconds, boundary);

        let inner_bytes = inner.encode_to_vec(MAX_COMMON_STRUCTURE_SIZE)?;
        let mut outer_salt = [0_u8; i2pr_netdb::ELS2_SALT_LENGTH];
        let mut inner_salt = [0_u8; i2pr_netdb::ELS2_SALT_LENGTH];
        fill_random(&mut outer_salt, rng)?;
        fill_random(&mut inner_salt, rng)?;
        let authorization =
            i2pr_netdb::Layer1Authorization::new(layer1_flags, &auth_data, cookie.as_bytes())?;
        let outer_ciphertext = encrypt_outer_ciphertext(
            &credentials,
            published_seconds,
            INNER_LEASE_SET2_STORE_TYPE,
            &inner_bytes,
            authorization,
            &outer_salt,
            &inner_salt,
        )?;

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

    /// Builds the authorized signed record and its `DatabaseStore` hand-off.
    pub fn build_authorized_database_store<R: rand_core::TryCryptoRng + ?Sized>(
        &self,
        authorization: &Els2AuthorizationServerConfig,
        inner: &i2pr_proto::LeaseSet2,
        now_seconds: u32,
        expires_seconds: u32,
        rng: &mut R,
    ) -> Result<(BlindedStorageKey, DatabaseStoreMessage), EncryptedLeaseSetError> {
        let record =
            self.build_authorized_record(authorization, inner, now_seconds, expires_seconds, rng)?;
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

impl EncryptedLeaseSet2Resolver {
    /// Resolves a record using a per-client authorization credential.
    ///
    /// The credential is checked against the record's own storage key first, and
    /// then against the layer-1 authorization block. An unauthorized client gets
    /// [`EncryptedLeaseSetError::NotAuthorized`] rather than a decryption
    /// failure, because the two are genuinely different situations and collapsing
    /// them would hide a configuration mistake.
    pub fn resolve_with_auth(
        &mut self,
        record: &ValidatedEncryptedLeaseSet2,
        now_seconds: u32,
        credential: &Els2ClientAuth<'_>,
    ) -> Result<ResolvedEncryptedService, EncryptedLeaseSetError> {
        let daily = self.schedule.current(now_seconds)?;
        if record.storage_key() != daily.storage_key() {
            return Err(EncryptedLeaseSetError::Els2Validation(
                i2pr_netdb::Els2ValidationError::StorageKeyMismatch,
            ));
        }
        let credentials = self
            .schedule
            .identity()
            .credentials_for(daily.blinded_public_key());
        let decrypted: DecryptedEls2 = decrypt_outer_ciphertext(
            &credentials,
            record.published_seconds(),
            record.record().outer_ciphertext(),
            Some(credential),
        )
        .map_err(|error| match error {
            Els2Error::Auth(inner) => EncryptedLeaseSetError::NotAuthorized {
                reason: inner.to_string(),
            },
            Els2Error::ClientAuthorizationRequired { .. } => {
                EncryptedLeaseSetError::CredentialNotSupplied
            }
            other => EncryptedLeaseSetError::Els2(other),
        })?;
        if decrypted.store_type() != INNER_LEASE_SET2_STORE_TYPE {
            return Err(EncryptedLeaseSetError::UnexpectedInnerStoreType);
        }
        let inner = decrypted.into_lease_set2(MAX_COMMON_STRUCTURE_SIZE)?;
        let destination_hash = inner.key_hash()?;
        i2pr_netdb::ValidatedLeaseSet2::from_lease_set2(
            inner.clone(),
            Some(i2pr_netdb::DestinationHash::from_hash(destination_hash)),
            i2pr_netdb::LeaseSet2ValidationContext::new(now_seconds),
        )?;
        Ok(ResolvedEncryptedService {
            address: self.address(),
            daily_blinding: daily,
            inner,
        })
    }
}

/// Generates a client Diffie-Hellman keypair for one authorized client.
///
/// The private half is returned to the caller for storage; the public half is
/// what the server configures. Returning them together is deliberate: a client
/// that has the public half without the private half cannot read anything, and a
/// server that is handed the private half would be able to read everything.
pub fn generate_client_dh_keypair<R: rand_core::TryCryptoRng + ?Sized>(
    rng: &mut R,
) -> Result<(X25519PrivateKey, i2pr_netdb::AuthClientPublicKey), Els2AuthError> {
    let private =
        X25519PrivateKey::generate(rng).map_err(|_| Els2AuthError::RandomnessUnavailable)?;
    let public = i2pr_netdb::AuthClientPublicKey::from_bytes(private.public_bytes());
    Ok((private, public))
}

/// Returns the scheme a server configuration publishes.
pub const fn authorization_scheme(authorization: &Els2AuthorizationServerConfig) -> Els2AuthScheme {
    authorization.scheme()
}

/// Convenience alias so a caller can name a server pre-shared key without
/// importing the authorization module directly.
pub type ServerPsk = i2pr_netdb::PskClientKey;

/// Convenience alias for the per-generation cookie type, for a control surface
/// that wants to hold one across a publication.
pub type GenerationAuthCookie = AuthCookie;
