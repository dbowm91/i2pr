//! Plan 105 §11: DatabaseStore ingestion outside an active lookup.
//!
//! The handler is the only path the runtime adapter uses to accept a
//! `DatabaseStore` message that arrived without a corresponding
//! active query. It is purely typed — no socket, no tunnel, no
//! timeout. The router role/context decides whether to accept the
//! record; this module never opens unsolicited LeaseSet or floodfill
//! forwarding paths.

use i2pr_proto::{CodecError, DatabaseStoreData, DatabaseStoreMessage, RouterInfo};
use thiserror::Error;

use crate::lookup_action::{MAX_DECOMPRESSED_ROUTER_INFO_BYTES, decompress_router_info};
use crate::router_info::{RouterHash, ValidatedRouterInfo, ValidationContext, router_hash};
use crate::store::{InsertOutcome, RouterInfoStore};

/// Diagnostic failures for the unsolicited `DatabaseStore` handler.
#[derive(Debug, Error)]
pub enum UnsolicitedStoreError {
    /// The handler is configured to reject unsolicited stores in
    /// the current role/context.
    #[error("unsolicited database store is not allowed in this role")]
    NotAllowed,
    /// The `DatabaseStore` body carried a non-RouterInfo payload.
    #[error("unsolicited database store carried an unsupported payload type")]
    UnsupportedPayload,
    /// The compressed RouterInfo payload exceeded the per-record
    /// ceiling or was malformed.
    #[error("router info decompression failed: {0}")]
    Decompression(String),
    /// The decompressed payload could not be parsed as a
    /// `RouterInfo`.
    #[error("router info decode failed: {0}")]
    Decode(#[from] CodecError),
    /// The contained RouterInfo did not match its declared key.
    #[error("router info key mismatch")]
    KeyMismatch,
    /// The `RouterInfo` failed Plan 103 validation.
    #[error("router info validation failed: {0}")]
    Validation(#[from] crate::router_info::RouterInfoValidationError),
}

/// Outcome returned to the runtime adapter after handling a
/// unsolicited `DatabaseStore`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnsolicitedStoreOutcome {
    /// The record was inserted (new, idempotent, or replacement).
    Inserted(InsertOutcome),
    /// The record was rejected as stale.
    Stale,
    /// The record was rejected as a duplicate-conflict.
    Conflict,
    /// The store was at capacity; the record was rejected.
    CapacityExceeded,
}

/// Whether the runtime owner permits the unsolicited
/// `DatabaseStore` ingestion path at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsolicitedStorePolicy {
    /// The runtime owner refuses unsolicited stores in this role.
    Reject,
    /// The runtime owner accepts unsolicited stores that pass the
    /// normal Plan 103 validation pipeline.
    Accept,
}

/// Handle an unsolicited `DatabaseStore` message. Returns the typed
/// outcome the runtime adapter should report back to the upper layer.
pub fn handle_unsolicited_databasestore(
    store: &mut RouterInfoStore,
    message: &DatabaseStoreMessage,
    policy: UnsolicitedStorePolicy,
    context: ValidationContext,
) -> Result<UnsolicitedStoreOutcome, UnsolicitedStoreError> {
    if policy != UnsolicitedStorePolicy::Accept {
        return Err(UnsolicitedStoreError::NotAllowed);
    }
    let compressed = match &message.data {
        DatabaseStoreData::RouterInfoCompressed(payload) => payload.as_bytes().to_vec(),
        DatabaseStoreData::LeaseSet(_)
        | DatabaseStoreData::LeaseSet2(_)
        | DatabaseStoreData::Deferred { .. } => {
            return Err(UnsolicitedStoreError::UnsupportedPayload);
        }
    };
    let decompressed = decompress_router_info(&compressed)
        .map_err(|error| UnsolicitedStoreError::Decompression(format!("{error:?}")))?;
    if decompressed.len() > MAX_DECOMPRESSED_ROUTER_INFO_BYTES {
        return Err(UnsolicitedStoreError::Decompression(format!(
            "decompressed size {} exceeds ceiling",
            decompressed.len()
        )));
    }
    let router_info = RouterInfo::decode(&decompressed, decompressed.len())?;
    let expected = RouterHash::from_hash(message.key);
    let derived = router_hash(router_info.router_identity())?;
    if derived != expected {
        return Err(UnsolicitedStoreError::KeyMismatch);
    }
    let validated = ValidatedRouterInfo::from_router_info(router_info, Some(expected), context)?;
    Ok(match store.insert(validated) {
        InsertOutcome::Inserted => UnsolicitedStoreOutcome::Inserted(InsertOutcome::Inserted),
        InsertOutcome::Idempotent => UnsolicitedStoreOutcome::Inserted(InsertOutcome::Idempotent),
        InsertOutcome::Replaced => UnsolicitedStoreOutcome::Inserted(InsertOutcome::Replaced),
        InsertOutcome::Conflict => UnsolicitedStoreOutcome::Conflict,
        InsertOutcome::StaleReplacement => UnsolicitedStoreOutcome::Stale,
        InsertOutcome::CapacityExceeded => UnsolicitedStoreOutcome::CapacityExceeded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_crypto::RouterIdentityBundle;
    use i2pr_proto::{Date, Mapping};
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    fn bundle(seed: u64) -> RouterIdentityBundle {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
    }

    fn validate(b: &RouterIdentityBundle) -> ValidatedRouterInfo {
        let info = b
            .sign_router_info(
                Date::from_millis(1),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(1)),
        )
        .expect("validate")
    }

    fn compress(payload: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write as _;
        encoder.write_all(payload).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn rejects_in_default_role() {
        let signer = bundle(0x800);
        let validated = validate(&signer);
        let mut store = RouterInfoStore::default();
        let context = ValidationContext::new(Date::from_millis(1));
        let encoded = validated
            .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let compressed = compress(&encoded);
        let compressed_len = compressed.len();
        let message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes(*validated.key().as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
            ),
        };
        let error = handle_unsolicited_databasestore(
            &mut store,
            &message,
            UnsolicitedStorePolicy::Reject,
            context,
        )
        .unwrap_err();
        assert!(matches!(error, UnsolicitedStoreError::NotAllowed));
    }

    #[test]
    fn accepts_valid_routerinfo_in_accept_role() {
        let signer = bundle(0x810);
        let validated = validate(&signer);
        let mut store = RouterInfoStore::default();
        let context = ValidationContext::new(Date::from_millis(1));
        let encoded = validated
            .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let compressed = compress(&encoded);
        let compressed_len = compressed.len();
        let message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes(*validated.key().as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
            ),
        };
        let outcome = handle_unsolicited_databasestore(
            &mut store,
            &message,
            UnsolicitedStorePolicy::Accept,
            context,
        )
        .expect("outcome");
        assert!(matches!(outcome, UnsolicitedStoreOutcome::Inserted(_)));
    }

    #[test]
    fn rejects_key_mismatch() {
        let signer_a = bundle(0x820);
        let signer_b = bundle(0x821);
        let validated = validate(&signer_a);
        let mut store = RouterInfoStore::default();
        let context = ValidationContext::new(Date::from_millis(1));
        let encoded = validated
            .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let compressed = compress(&encoded);
        let compressed_len = compressed.len();
        // The key here is from signer_b, but the payload is from signer_a.
        let bogus_key = crate::router_info::router_hash(signer_b.identity()).unwrap();
        let message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes(*bogus_key.as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
            ),
        };
        let error = handle_unsolicited_databasestore(
            &mut store,
            &message,
            UnsolicitedStorePolicy::Accept,
            context,
        )
        .unwrap_err();
        assert!(matches!(error, UnsolicitedStoreError::KeyMismatch));
    }

    #[test]
    fn rejects_malformed_compressed_payload() {
        let signer = bundle(0x830);
        let validated = validate(&signer);
        let mut store = RouterInfoStore::default();
        let context = ValidationContext::new(Date::from_millis(1));
        let message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes(*validated.key().as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(vec![0x66, 0x66, 0x66], 3).expect("payload"),
            ),
        };
        let error = handle_unsolicited_databasestore(
            &mut store,
            &message,
            UnsolicitedStorePolicy::Accept,
            context,
        )
        .unwrap_err();
        assert!(matches!(error, UnsolicitedStoreError::Decompression(_)));
    }

    #[test]
    fn decompressed_record_still_requires_plan103_validation() {
        // Sign the RouterInfo with a published date that is past
        // the validator's freshness window and then call the
        // handler with `now = published + max_age + 1`; the
        // validator must reject the record as stale, proving that
        // decompression alone does not bypass the Plan 103 path.
        let signer = bundle(0x840);
        let published_ms = 0u64;
        let info = signer
            .sign_router_info(
                Date::from_millis(published_ms),
                Vec::new(),
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        let validated = ValidatedRouterInfo::from_router_info(
            info,
            None,
            ValidationContext::new(Date::from_millis(published_ms)),
        )
        .expect("validate");
        let mut store = RouterInfoStore::default();
        let encoded = validated
            .encoded(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .expect("encode");
        let compressed = compress(&encoded);
        let compressed_len = compressed.len();
        let message = DatabaseStoreMessage {
            key: i2pr_proto::Hash::from_bytes(*validated.key().as_bytes()),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: DatabaseStoreData::RouterInfoCompressed(
                i2pr_proto::DeferredPayload::new(compressed, compressed_len).expect("payload"),
            ),
        };
        let now = crate::router_info::DEFAULT_MAX_AGE.as_millis() as u64 + 1;
        let context = ValidationContext::new(Date::from_millis(now));
        let error = handle_unsolicited_databasestore(
            &mut store,
            &message,
            UnsolicitedStorePolicy::Accept,
            context,
        )
        .unwrap_err();
        assert!(matches!(error, UnsolicitedStoreError::Validation(_)));
    }
}
