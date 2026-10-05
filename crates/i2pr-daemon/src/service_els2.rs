//! Plan 334: the daemon-owned encrypted-LeaseSet2 publication owner for a
//! control-owned service tunnel.
//!
//! # What this owns
//!
//! It is the seam between the Proposal 170 control surface and the ELS2
//! owners frozen by Plans 332/333. Given a validated
//! [`LeaseSetSecurityPlan`], it produces the real type-5 `DatabaseStore`
//! hand-off and the real `.b33.i2p` service address, using the same
//! `EncryptedLeaseSet2Publisher` / `Els2AuthorizationServerConfig` types a
//! client-side publication uses. There is no second ELS2 implementation here.
//!
//! # Why the identity needs no new stored secret
//!
//! An encrypted service still publishes an ordinary signed LeaseSet2 as the
//! *inner* record; only the outer wrapper is the blinded type-5 record. The
//! blinding identity is therefore not a second key: for an unblinded type-7
//! service it is the Red25519 conversion of the service's own existing
//! Ed25519 signing seed, which is what `unblinded_scalar_from_ed25519_seed`
//! exists for. Deriving it from the same seed is required, not a convenience —
//! the published address must name the same destination the inner LeaseSet2 is
//! signed by, or a client could not verify it.
//!
//! The consequence is that Plan 334 adds **no** new file format and **no** new
//! at-rest secret. The only new at-rest material is the lookup secret and the
//! client list, and both already live in the durable control definition.
//!
//! # Secrets
//!
//! [`ServiceEls2Material`] holds a private scalar inside its
//! [`BlindingSchedule`] and, for an authorized service, the pre-shared keys or
//! client public keys. It is therefore deliberately not `Clone`, and its
//! `Debug` prints presence and counts only. The lookup secret is borrowed from
//! the control definition at construction and is not copied into a second
//! long-lived string here.
//!
//! # What this does not claim
//!
//! A record produced here is signed with i2pr's Red25519 type-5 transcript.
//! Per ADR 0005 and Plan 336, i2pd and Java I2P cannot verify that transcript
//! today, so publishing a type-5 record is a local, self-consistent act and not
//! an interoperability claim. Plan 335 owns the live lane.

use std::collections::BTreeMap;
use std::fmt;

use i2pr_crypto::red25519::derive_public_key;
use i2pr_i2pcontrol::proposal_leaseset_mode::{
    LeaseSetClientAuthScheme, LeaseSetSecurityPlan, decode_encoded_client_auths,
};
use i2pr_netdb::{
    AuthClientPublicKey, BlindedStorageKey, BlindingIdentity, BlindingSchedule,
    BlindingScheduleConfig, Els2AuthorizationServerConfig, Els2Error, PskClientKey,
    unblinded_scalar_from_ed25519_seed,
};
use i2pr_proto::SigningKeyType;
use i2pr_storage::ServiceDestinationRecord;
#[cfg(test)]
use i2pr_storage::ServiceDestinationStore;
use rand_core::TryCryptoRng;
use thiserror::Error;

/// Days of blinding material retained at once.
///
/// Two is the rollover bound frozen by Plan 332: today and tomorrow is enough
/// for a daily refresh, and it is a cache bound rather than a policy, so a
/// clock that jumps backwards simply re-derives.
const ELS2_CACHED_DAYS: usize = 2;

/// Why one service's ELS2 material could not be built.
///
/// Every variant names the service and the layer that refused; none carries
/// secret bytes.
#[derive(Debug, Error)]
pub enum ServiceEls2Error {
    /// The persisted service destination identity could not be read.
    #[error("service {service_id} destination identity is unavailable for LeaseSet encryption")]
    IdentityUnavailable {
        /// Service tunnel identifier.
        service_id: String,
        /// Static reason from the storage layer.
        reason: &'static str,
    },
    /// The plan requires an authorization block but none is configured.
    #[error("service {service_id} LeaseSet mode requires client authorization")]
    AuthorizationMissing {
        /// Service tunnel identifier.
        service_id: String,
    },
    /// The plan forbids an authorization block but one is configured.
    #[error("service {service_id} LeaseSet mode does not use client authorization")]
    AuthorizationUnexpected {
        /// Service tunnel identifier.
        service_id: String,
    },
    /// A client key was not 32 bytes of hex.
    #[error("service {service_id} has a malformed LeaseSet client key")]
    MalformedClientKey {
        /// Service tunnel identifier.
        service_id: String,
    },
    /// The ELS2 layer refused to build the blinding identity or the record.
    #[error("service {service_id} LeaseSet encryption failed: {reason}")]
    Els2 {
        /// Service tunnel identifier.
        service_id: String,
        /// Static ELS2 failure category.
        reason: &'static str,
    },
    /// The published address could not be encoded.
    #[error("service {service_id} LeaseSet address could not be encoded")]
    Address {
        /// Service tunnel identifier.
        service_id: String,
    },
}

/// One service's encrypted-LeaseSet2 publication material.
///
/// Owns a [`BlindingSchedule`] in owner mode, so it holds the unblinded private
/// scalar for this service. Not `Clone`, on purpose: two copies of a service's
/// private identity are a live hazard, and `BlindingSchedule::new_owner` moves
/// the scalar in rather than copying it.
pub struct ServiceEls2Material {
    service_id: String,
    schedule: BlindingSchedule,
    authorization: Option<Els2AuthorizationServerConfig>,
    requires_client_key: bool,
    requires_blinding_secret: bool,
    client_count: usize,
}

impl fmt::Debug for ServiceEls2Material {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Presence and counts only. The schedule, the private scalar, the
        // lookup secret, and every client key stay out of every log line.
        formatter
            .debug_struct("ServiceEls2Material")
            .field("service_id", &self.service_id)
            .field("schedule", &self.schedule)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "<redacted>"),
            )
            .field("client_count", &self.client_count)
            .field("requires_client_key", &self.requires_client_key)
            .field("requires_blinding_secret", &self.requires_blinding_secret)
            .finish()
    }
}

impl ServiceEls2Material {
    /// The service this material publishes for.
    pub fn service_id(&self) -> &str {
        &self.service_id
    }

    /// How many clients the authorization block carries.
    pub fn client_count(&self) -> usize {
        self.client_count
    }

    /// Whether the published address must set `B32_FLAG_REQUIRES_CLIENT_KEY`.
    pub fn requires_client_key(&self) -> bool {
        self.requires_client_key
    }

    /// Whether the published address must set
    /// `B32_FLAG_REQUIRES_BLINDING_SECRET`.
    pub fn requires_blinding_secret(&self) -> bool {
        self.requires_blinding_secret
    }

    /// The encrypted-service `.b32.i2p` address for this service.
    ///
    /// Carries the **unblinded** public key plus both signature types, because
    /// a client needs the unblinded key to derive the daily blinded key. The
    /// two flags are the plan's; the secret and the client keys are not, and
    /// are never encoded.
    pub fn address(&self) -> Result<String, ServiceEls2Error> {
        let publisher =
            i2pr_client::encrypted_leaseset::EncryptedLeaseSet2Publisher::new(&self.schedule);
        let address = if self.requires_client_key {
            publisher.authorized_address(self.requires_blinding_secret)
        } else {
            publisher.address(self.requires_blinding_secret)
        }
        .map_err(|_| ServiceEls2Error::Address {
            service_id: self.service_id.clone(),
        })?;
        address.to_text().map_err(|_| ServiceEls2Error::Address {
            service_id: self.service_id.clone(),
        })
    }

    /// Builds the signed type-5 record and its `DatabaseStore` hand-off.
    ///
    /// `inner` is the service's ordinary signed LeaseSet2; the returned store
    /// is addressed at the day's *blinded* storage key, which is the whole
    /// point of the mode — an ordinary publication is addressed at the
    /// destination hash instead.
    pub fn build_database_store<R: TryCryptoRng + ?Sized>(
        &self,
        inner: &i2pr_proto::LeaseSet2,
        now_seconds: u32,
        expires_seconds: u32,
        rng: &mut R,
    ) -> Result<(BlindedStorageKey, i2pr_proto::DatabaseStoreMessage), ServiceEls2Error> {
        let publisher =
            i2pr_client::encrypted_leaseset::EncryptedLeaseSet2Publisher::new(&self.schedule);
        let built = match self.authorization.as_ref() {
            Some(authorization) => publisher.build_authorized_database_store(
                authorization,
                inner,
                now_seconds,
                expires_seconds,
                rng,
            ),
            None => publisher.build_database_store(inner, now_seconds, expires_seconds, rng),
        };
        built.map_err(|_| ServiceEls2Error::Els2 {
            service_id: self.service_id.clone(),
            reason: "type-5 record construction failed",
        })
    }
}

/// Builds the ELS2 publication material for one control-owned service.
///
/// Returns `Ok(None)` for a plan that does not publish a type-5 record, which
/// is the ordinary case: a caller must therefore not treat "no material" as an
/// error, only as "publish the ordinary LeaseSet2".
///
/// The plan is assumed already validated — `normalize_definition` refuses an
/// illegal block before a definition can be stored — but the authorization
/// pairing is re-checked here anyway, because this is the layer that actually
/// consumes the client list and a mismatch would otherwise publish an
/// unauthenticated record while the operator believes it is authorized.
pub fn build_service_els2_material(
    service_id: &str,
    plan: &LeaseSetSecurityPlan,
    options: &BTreeMap<String, String>,
    record: &ServiceDestinationRecord,
) -> Result<Option<ServiceEls2Material>, ServiceEls2Error> {
    if !plan.publishes_type5() {
        return Ok(None);
    }
    let scalar = unblinded_scalar_from_ed25519_seed(record.signing_seed());
    let public = derive_public_key(&scalar);
    // Unblinded type 7, blinded type 11: the service signs its inner LeaseSet2
    // with the Ed25519 key the base-32 address already names, and the outer
    // type-5 record with the day's blinded Red25519 key.
    let identity = BlindingIdentity::new(
        public,
        SigningKeyType::EdDsaSha512Ed25519,
        plan.lookup_secret_supplied()
            .then(|| options.get("leaseset_password"))
            .flatten()
            .map(String::as_str),
    )
    .map_err(|error| ServiceEls2Error::Els2 {
        service_id: service_id.to_owned(),
        reason: els2_reason(&error),
    })?;
    let schedule = BlindingSchedule::new_owner(
        identity,
        scalar,
        BlindingScheduleConfig::new(ELS2_CACHED_DAYS, true),
    );

    let authorization = match plan.client_auth_scheme() {
        None => {
            if options.contains_key("leaseset_client_auth") {
                return Err(ServiceEls2Error::AuthorizationUnexpected {
                    service_id: service_id.to_owned(),
                });
            }
            None
        }
        Some(scheme) => {
            let encoded = options.get("leaseset_client_auth").ok_or_else(|| {
                ServiceEls2Error::AuthorizationMissing {
                    service_id: service_id.to_owned(),
                }
            })?;
            let entries = decode_encoded_client_auths(encoded).map_err(|_| {
                ServiceEls2Error::MalformedClientKey {
                    service_id: service_id.to_owned(),
                }
            })?;
            Some(build_authorization(service_id, scheme, &entries)?)
        }
    };

    let flags = plan.address_flags();
    Ok(Some(ServiceEls2Material {
        service_id: service_id.to_owned(),
        schedule,
        authorization,
        requires_client_key: flags.requires_client_key(),
        requires_blinding_secret: flags.requires_blinding_secret(),
        client_count: plan.client_count(),
    }))
}

/// Builds the typed authorization owner for the plan's scheme.
///
/// Both schemes take 32-byte keys, so the *bytes* are validated identically;
/// what differs is which owner consumes them. i2pr cannot validate which of the
/// two an operator meant, and does not pretend to: the mode decides, and the
/// protocol owner's own constructors do the type-specific checks.
fn build_authorization(
    service_id: &str,
    scheme: LeaseSetClientAuthScheme,
    entries: &[i2pr_i2pcontrol::LeaseSetClientAuthEntry],
) -> Result<Els2AuthorizationServerConfig, ServiceEls2Error> {
    let mut keys = Vec::with_capacity(entries.len());
    for entry in entries {
        let bytes = decode_hex_32(entry.encoded_key()).ok_or_else(|| {
            ServiceEls2Error::MalformedClientKey {
                service_id: service_id.to_owned(),
            }
        })?;
        keys.push(bytes);
    }
    let malformed = || ServiceEls2Error::Els2 {
        service_id: service_id.to_owned(),
        reason: "client authorization keys were rejected by the protocol owner",
    };
    match scheme {
        LeaseSetClientAuthScheme::PreSharedKey => {
            // Both constructors are infallible wrappers around 32 bytes; the
            // scheme-specific validation lives in the config builder, which
            // is the single place that can reject a block i2pr would publish.
            let psks: Vec<PskClientKey> = keys.into_iter().map(PskClientKey::from_bytes).collect();
            Els2AuthorizationServerConfig::psk(psks).map_err(|_| malformed())
        }
        LeaseSetClientAuthScheme::DiffieHellman => {
            let publics: Vec<AuthClientPublicKey> = keys
                .into_iter()
                .map(AuthClientPublicKey::from_bytes)
                .collect();
            Els2AuthorizationServerConfig::dh(publics).map_err(|_| malformed())
        }
    }
}

/// Decodes exactly 32 lowercase hex characters into 32 bytes.
///
/// The control contract already validated the length and alphabet, so this
/// re-checks rather than trusts: the bytes cross from a `String` into secret
/// material here, and that boundary should not depend on a caller's ordering.
fn decode_hex_32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = [0_u8; 32];
    for (index, pair) in bytes.chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        out[index] = (high << 4) | low;
    }
    Some(out)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// A bounded static label for an ELS2 failure, so the error never carries bytes.
fn els2_reason(error: &Els2Error) -> &'static str {
    match error {
        Els2Error::LookupSecretRejected(_) => "lookup secret rejected",
        Els2Error::UnsupportedUnblindedSigtype { .. } => "unblinded signature type rejected",
        Els2Error::Blinding(_) => "blinding material rejected",
        _ => "blinding identity rejected",
    }
}

/// Loads the persisted service identity and builds ELS2 material from it.
///
/// `Ok(None)` means the plan does not publish a type-5 record, in which case
/// the stored identity is not read at all.
///
/// The record is resolved by the **manager**, not by a path re-derived here.
/// The manager owns "where does this service's identity live" — it wrote the
/// record — so a second copy of that path could read a store the runtime does
/// not use and publish an address naming the wrong identity. `Ok(None)` from
/// the manager means the service's identity is ephemeral, which cannot produce
/// a stable type-5 address and is reported as unavailable.
pub fn load_service_els2_material(
    manager: &crate::service_tunnels::ServiceTunnelManager,
    service_id: &str,
    plan: &LeaseSetSecurityPlan,
    options: &BTreeMap<String, String>,
) -> Result<Option<ServiceEls2Material>, ServiceEls2Error> {
    if !plan.publishes_type5() {
        return Ok(None);
    }
    let record = manager
        .service_identity_record(service_id)
        .map_err(|_| ServiceEls2Error::IdentityUnavailable {
            service_id: service_id.to_owned(),
            reason: "service identity store rejected",
        })?
        .ok_or_else(|| ServiceEls2Error::IdentityUnavailable {
            service_id: service_id.to_owned(),
            reason: "service identity is ephemeral",
        })?;
    build_service_els2_material(service_id, plan, options, &record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_crypto::red25519::blinded_storage_key;
    use i2pr_i2pcontrol::proposal_leaseset_mode::{
        LeaseSetClientAuthEntry, resolve_encrypt_lease_set_mode, resolve_lease_set_security,
    };
    use i2pr_proto::DatabaseStoreData;
    use i2pr_proto::SignatureValue;
    use tempfile::TempDir;

    fn plan_for(mode: &str, secret: Option<&str>, clients: usize) -> LeaseSetSecurityPlan {
        let resolved = resolve_encrypt_lease_set_mode(mode).expect("frozen mode");
        let entries: Vec<LeaseSetClientAuthEntry> = (0..clients)
            .map(|index| {
                LeaseSetClientAuthEntry::new(&format!("c{index}"), &"11".repeat(32))
                    .expect("valid entry")
            })
            .collect();
        resolve_lease_set_security(Some(&resolved), secret, &entries).expect("valid plan")
    }

    /// A real persisted service identity, loaded back through the storage
    /// layer's own path so these rows exercise the stored bytes rather than a
    /// stand-in.
    ///
    /// The seed is drawn from the caller's label only to make each service
    /// distinct within a test; the identity itself is real generated
    /// material, which is what the BlindingIdentity construction consumes.
    fn record() -> (TempDir, ServiceDestinationRecord) {
        let dir = TempDir::new().expect("temp dir");
        let store = ServiceDestinationStore::for_service(dir.path(), "svc").expect("store path");
        store.generate_new(&mut rand_core::OsRng).expect("generate");
        // Load back rather than reusing the generated value: the rows below
        // must exercise the bytes that would actually survive a restart.
        let loaded = store.load().expect("load");
        (dir, loaded)
    }

    /// A manager over a temp dir with **no** committed generation, so no
    /// service spec — and therefore no identity record — can be found. Any
    /// path that consulted the store would fail here.
    fn empty_manager(dir: &std::path::Path) -> crate::service_tunnels::ServiceTunnelManager {
        let config = crate::service_tunnels::ServiceTunnelManagerConfig {
            data_dir: dir.to_path_buf(),
            aggregate_connection_ceiling: 16,
            per_service_connection_ceiling: 4,
            specs: std::sync::Arc::new(i2pr_service_tunnels::ServiceTunnelSet::new()),
            aliases: std::sync::Arc::new(i2pr_service_tunnels::StaticAliasTable::new()),
        };
        crate::service_tunnels::ServiceTunnelManager::new(config).expect("manager builds")
    }

    #[test]
    fn an_ordinary_plan_needs_no_material_and_reads_no_identity() {
        let plan = plan_for("disable", None, 0);
        let options = BTreeMap::new();
        // No identity is consulted: with no committed spec the store cannot
        // be resolved at all, so an ordinary plan must still resolve.
        let dir = TempDir::new().expect("temp dir");
        assert!(
            load_service_els2_material(&empty_manager(dir.path()), "svc", &plan, &options)
                .expect("ordinary plan resolves")
                .is_none()
        );
        let (_dir, record) = record();
        assert!(
            build_service_els2_material("svc", &plan, &options, &record)
                .expect("ordinary plan resolves")
                .is_none()
        );
    }

    #[test]
    fn every_applied_mode_builds_material_from_the_stored_identity() {
        let (_dir, record) = record();
        for (mode, secret, clients, client_key, blinding_secret) in [
            ("blinded", None, 0, false, false),
            ("blinded with lookup password", Some("pw"), 0, false, true),
            ("encrypted (psk)", None, 1, true, false),
            (
                "encrypted with lookup password (psk)",
                Some("pw"),
                2,
                true,
                true,
            ),
            ("encrypted with per-user key (dh)", None, 3, true, false),
            (
                "encrypted with lookup password and per-user key (dh)",
                Some("pw"),
                1,
                true,
                true,
            ),
        ] {
            let plan = plan_for(mode, secret, clients);
            let mut options = BTreeMap::new();
            if let Some(value) = secret {
                options.insert("leaseset_password".to_owned(), value.to_owned());
            }
            if clients > 0 {
                let entries: Vec<LeaseSetClientAuthEntry> = (0..clients)
                    .map(|index| {
                        LeaseSetClientAuthEntry::new(
                            &format!("c{index}"),
                            &format!("{:02x}", index + 1).repeat(32),
                        )
                        .expect("valid entry")
                    })
                    .collect();
                options.insert(
                    "leaseset_client_auth".to_owned(),
                    i2pr_i2pcontrol::encode_lease_set_client_auths(&entries),
                );
            }
            let material = build_service_els2_material("svc", &plan, &options, &record)
                .unwrap_or_else(|error| panic!("{mode} must build: {error:?}"))
                .unwrap_or_else(|| panic!("{mode} must produce material"));
            assert_eq!(material.client_count(), clients, "{mode}");
            assert_eq!(material.requires_client_key(), client_key, "{mode}");
            assert_eq!(
                material.requires_blinding_secret(),
                blinding_secret,
                "{mode}"
            );
            // The address is a real encrypted-service address carrying the
            // unblinded key, both sigtypes, and exactly the plan's flags.
            let address = material
                .address()
                .unwrap_or_else(|error| panic!("{mode}: {error:?}"));
            assert!(address.ends_with(".b32.i2p"), "{mode}: {address}");
            let decoded = i2pr_proto::EncryptedServiceAddress::from_text(&address)
                .unwrap_or_else(|error| panic!("{mode} address must decode: {error:?}"));
            // The wire flag bits come from the protocol owner, not from a local
            // guess, so this row fails if the frozen constants ever move.
            let mut expected = 0_u8;
            if client_key {
                expected |= i2pr_proto::B32_FLAG_REQUIRES_CLIENT_KEY;
            }
            if blinding_secret {
                expected |= i2pr_proto::B32_FLAG_REQUIRES_BLINDING_SECRET;
            }
            assert_eq!(decoded.flags(), expected, "{mode}");
            assert_eq!(decoded.requires_client_key(), client_key, "{mode}");
            assert_eq!(
                decoded.requires_blinding_secret(),
                blinding_secret,
                "{mode}"
            );
        }
    }

    /// A real signed inner LeaseSet2, built the way a service's publication
    /// path builds one: an Ed25519 destination, one lease, and a real
    /// signature over the preimage.
    fn signed_inner_ls2(signer: &RouterIdentityBundle, published: u32) -> i2pr_proto::LeaseSet2 {
        let destination = i2pr_proto::Destination::new(signer.identity().key_and_cert().clone())
            .expect("destination");
        let header = i2pr_proto::LeaseSet2Header::new(
            destination,
            published,
            600,
            i2pr_proto::LeaseSet2Flags::from_raw(0),
        )
        .expect("header");
        let placeholder = SignatureValue::new(i2pr_crypto::ROUTER_SIGNING_KEY_TYPE, vec![0_u8; 64])
            .expect("placeholder");
        let keys = vec![
            i2pr_proto::LeaseSet2EncryptionKey::new(
                i2pr_proto::CryptoKeyType::X25519,
                vec![0x55; 32],
            )
            .expect("key"),
        ];
        let leases = vec![i2pr_proto::Lease2::new(
            i2pr_proto::Hash::from_bytes([0x11; 32]),
            7,
            i2pr_proto::Date32::from_seconds(published + 600),
        )];
        let unsigned = i2pr_proto::LeaseSet2::new(
            header,
            i2pr_proto::Mapping::empty(),
            keys,
            leases,
            placeholder,
        )
        .expect("unsigned ls2");
        let signature = signer
            .signing_key()
            .sign(&unsigned.signature_preimage())
            .expect("sign");
        i2pr_proto::LeaseSet2::new(
            unsigned.header().clone(),
            unsigned.options().clone(),
            unsigned.encryption_keys().to_vec(),
            unsigned.leases().to_vec(),
            signature,
        )
        .expect("signed ls2")
    }

    #[test]
    fn every_type5_mode_produces_a_real_type5_record_at_its_blinded_key() {
        let (_dir, record) = record();
        let signer = RouterIdentityBundle::generate(&mut rand_core::OsRng).expect("signer");
        const NOW: u32 = 1_700_000_000;
        let inner = signed_inner_ls2(&signer, NOW);
        for (mode, secret, clients) in [
            ("blinded", None, 0),
            ("blinded with lookup password", Some("pw"), 0),
            ("encrypted (psk)", None, 1),
            (
                "encrypted with lookup password and per-user key (psk)",
                Some("pw"),
                2,
            ),
            ("encrypted with per-user key (dh)", None, 1),
            (
                "encrypted with lookup password and per-user key (dh)",
                Some("pw"),
                1,
            ),
        ] {
            let plan = plan_for(mode, secret, clients);
            let mut options = BTreeMap::new();
            if let Some(value) = secret {
                options.insert("leaseset_password".to_owned(), value.to_owned());
            }
            if clients > 0 {
                let entries: Vec<LeaseSetClientAuthEntry> = (0..clients)
                    .map(|index| {
                        LeaseSetClientAuthEntry::new(
                            &format!("c{index}"),
                            &format!("{:02x}", index + 1).repeat(32),
                        )
                        .expect("valid entry")
                    })
                    .collect();
                options.insert(
                    "leaseset_client_auth".to_owned(),
                    i2pr_i2pcontrol::encode_lease_set_client_auths(&entries),
                );
            }
            let mut material = build_service_els2_material("svc", &plan, &options, &record)
                .unwrap_or_else(|error| panic!("{mode} must build: {error:?}"))
                .unwrap_or_else(|| panic!("{mode} must produce material"));
            let (key, message) = material
                .build_database_store(&inner, NOW, 600, &mut rand_core::OsRng)
                .unwrap_or_else(|error| panic!("{mode} record must build: {error:?}"));

            // The hand-off is a real encrypted-LeaseSet2 outer record, not a
            // LeaseSet2: this is the type that distinguishes the mode.
            let payload = match message.data {
                DatabaseStoreData::EncryptedLeaseSet(record) => record,
                other => panic!("{mode} produced a non-type-5 payload: {other:?}"),
            };
            // And it is filed at the day's blinded storage key, which is by
            // definition derived from the record's own blinded public key.
            // An ordinary publication is addressed at the destination hash
            // instead, so this is the property that makes the mode real.
            assert_eq!(message.key, *key.as_hash(), "{mode}");
            assert_eq!(
                message.key,
                blinded_storage_key(
                    &i2pr_crypto::red25519::Red25519PublicKey::decode(payload.blinded_public_key())
                        .expect("blinded public key")
                ),
                "{mode}"
            );
            // A real record: outer ciphertext, a salt, and the day's blinded
            // key, none of which an ordinary LeaseSet2 would carry.
            assert!(!payload.outer_ciphertext().is_empty(), "{mode}");
            assert!(
                !payload.outer_salt().iter().all(|byte| *byte == 0),
                "{mode} must use a non-zero outer salt"
            );
            // The blinded key published is the day's key for this service.
            let day = material.schedule.current(NOW).expect("day");
            assert_eq!(
                payload.blinded_public_key(),
                day.blinded_public_key().as_bytes(),
                "{mode}"
            );
        }
    }

    #[test]
    fn a_lookup_secret_changes_the_daily_key_but_not_the_address() {
        // Two services with the same identity and mode but different lookup
        // secrets must derive different daily keys, while the address itself
        // stays the same: it names the unblinded public key, which the secret
        // does not touch. A client holding the wrong secret therefore finds the
        // service but cannot read it, which is the intended property.
        let (_dir, record) = record();
        let mut options = BTreeMap::new();
        options.insert("leaseset_password".to_owned(), "alpha".to_owned());
        let mut one = build_service_els2_material(
            "svc",
            &plan_for("blinded with lookup password", Some("alpha"), 0),
            &options,
            &record,
        )
        .expect("builds")
        .expect("material");
        options.insert("leaseset_password".to_owned(), "beta".to_owned());
        let mut two = build_service_els2_material(
            "svc",
            &plan_for("blinded with lookup password", Some("beta"), 0),
            &options,
            &record,
        )
        .expect("builds")
        .expect("material");
        assert_eq!(
            one.address().expect("address"),
            two.address().expect("address")
        );
        assert_ne!(
            one.schedule
                .current(1_700_000_000)
                .expect("day")
                .blinded_public_key(),
            two.schedule
                .current(1_700_000_000)
                .expect("day")
                .blinded_public_key(),
            "a different lookup secret must derive a different daily key"
        );
        // Without a secret the same identity derives a third, different key,
        // so "no secret" is not another spelling of either value.
        let mut bare = build_service_els2_material(
            "svc",
            &plan_for("blinded", None, 0),
            &BTreeMap::new(),
            &record,
        )
        .expect("builds")
        .expect("material");
        assert_ne!(
            bare.schedule
                .current(1_700_000_000)
                .expect("day")
                .blinded_public_key(),
            one.schedule
                .current(1_700_000_000)
                .expect("day")
                .blinded_public_key(),
            "an absent lookup secret must not collide with a present one"
        );
    }

    #[test]
    fn an_authorization_mismatch_is_refused_rather_than_published_unauthenticated() {
        let (_dir, record) = record();
        // A mode with no authorization block but a client list present.
        let mut options = BTreeMap::new();
        options.insert(
            "leaseset_client_auth".to_owned(),
            format!("c0:{}", "22".repeat(32)),
        );
        let error =
            build_service_els2_material("svc", &plan_for("blinded", None, 0), &options, &record)
                .expect_err("mismatch refused");
        assert!(matches!(
            error,
            ServiceEls2Error::AuthorizationUnexpected { .. }
        ));

        // A mode needing a block but with none configured.
        let error = build_service_els2_material(
            "svc",
            &plan_for("encrypted (psk)", None, 1),
            &BTreeMap::new(),
            &record,
        )
        .expect_err("missing authorization refused");
        assert!(matches!(
            error,
            ServiceEls2Error::AuthorizationMissing { .. }
        ));

        // A malformed stored client key never reaches the protocol owner.
        let mut malformed = BTreeMap::new();
        malformed.insert("leaseset_client_auth".to_owned(), "c0:nothex".to_owned());
        let error = build_service_els2_material(
            "svc",
            &plan_for("encrypted (psk)", None, 1),
            &malformed,
            &record,
        )
        .expect_err("malformed key refused");
        assert!(matches!(error, ServiceEls2Error::MalformedClientKey { .. }));
    }

    #[test]
    fn no_debug_or_error_output_can_carry_key_or_secret_material() {
        let (_dir, record) = record();
        let key = "11".repeat(32);
        let mut options = BTreeMap::new();
        options.insert("leaseset_password".to_owned(), "topsecretlookup".to_owned());
        options.insert("leaseset_client_auth".to_owned(), format!("c0:{key}"));
        let material = build_service_els2_material(
            "svc",
            &plan_for(
                "encrypted with lookup password (psk)",
                Some("topsecretlookup"),
                1,
            ),
            &options,
            &record,
        )
        .expect("builds")
        .expect("material");
        let rendered = format!("{material:?}");
        for needle in [&key, "topsecretlookup", "c0"] {
            assert!(!rendered.contains(needle), "leaked {needle:?}: {rendered}");
        }
        // The address is public by design: it names the unblinded public key,
        // never the lookup secret or any client key.
        let address = material.address().expect("address");
        assert!(!address.contains(&key));
    }

    #[test]
    fn hex_decoding_rejects_everything_the_contract_did_not_allow() {
        assert!(decode_hex_32("").is_none());
        assert!(decode_hex_32(&"11".repeat(31)).is_none());
        assert!(decode_hex_32(&"11".repeat(33)).is_none());
        assert!(decode_hex_32(&"AB".repeat(32)).is_none());
        assert!(decode_hex_32(&"zz".repeat(32)).is_none());
        assert_eq!(decode_hex_32(&"00".repeat(32)), Some([0_u8; 32]));
        assert_eq!(decode_hex_32(&"ff".repeat(32)), Some([0xff_u8; 32]));
        assert_eq!(hex_nibble(b'0'), Some(0));
        assert_eq!(hex_nibble(b'9'), Some(9));
        assert_eq!(hex_nibble(b'a'), Some(10));
        assert_eq!(hex_nibble(b'f'), Some(15));
        assert_eq!(hex_nibble(b'A'), None);
        assert_eq!(hex_nibble(b'z'), None);
    }
}
