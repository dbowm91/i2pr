//! Plan 105 §6: standards-conformant `DatabaseLookup` construction.
//!
//! The lookup state machine asks the builder for a typed
//! [`DatabaseLookupMessage`] for one query attempt. The on-wire key
//! is the raw RouterHash (not the daily routing key); the lookup
//! kind is the wire-encoded `lookup_type` byte; the `from`/`reply_*`
//! fields come from the supplied exploratory reply path.

use i2pr_proto::{DatabaseLookupMessage, ReplyEncryption};
use thiserror::Error;

use crate::lookup_action::LOOKUP_EXCLUDED_PEER_BUDGET;
use crate::lookup_id::{LookupKind, ReplyPath};
use crate::router_info::RouterHash;

/// Diagnostics describing why a `DatabaseLookup` cannot be built.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DatabaseLookupBuildError {
    /// The reply path was missing. A direct peer link alone is not
    /// equivalent to a complete reply path.
    #[error("database lookup requires an exploratory reply path")]
    ReplyPathMissing,
    /// The excluded peer list exceeded the lookup state-machine
    /// ceiling.
    #[error("excluded peer list {actual} exceeds {maximum}-entry budget")]
    ExcludedPeersTooLarge {
        /// Actual number of excluded peers.
        actual: usize,
        /// Maximum number of excluded peers accepted.
        maximum: usize,
    },
    /// The I2P codec rejected the constructed message.
    #[error("database lookup codec rejected the constructed message: {context}")]
    CodecRejected {
        /// Codec context string.
        context: &'static str,
    },
}

/// Build a standards-conformant `DatabaseLookup` for the supplied
/// target and reply path. Excluded peers are deduplicated by the
/// caller; the builder does not mutate them.
pub fn build_databaselookup(
    target: &RouterHash,
    kind: LookupKind,
    reply_path: Option<&ReplyPath>,
    excluded_peers: &[RouterHash],
) -> Result<DatabaseLookupMessage, DatabaseLookupBuildError> {
    if excluded_peers.len() > LOOKUP_EXCLUDED_PEER_BUDGET {
        return Err(DatabaseLookupBuildError::ExcludedPeersTooLarge {
            actual: excluded_peers.len(),
            maximum: LOOKUP_EXCLUDED_PEER_BUDGET,
        });
    }
    let (delivery_flag, reply_tunnel_id, reply_gateway) = match reply_path {
        Some(path) => (true, Some(path.tunnel_id()), Some(path.gateway())),
        None => return Err(DatabaseLookupBuildError::ReplyPathMissing),
    };
    let from_hash = reply_gateway.expect("delivery_flag implies gateway");
    let message = DatabaseLookupMessage {
        key: i2pr_proto::Hash::from_bytes(*target.as_bytes()),
        from: i2pr_proto::Hash::from_bytes(*from_hash.as_bytes()),
        delivery_flag,
        reply_tunnel_id,
        lookup_type: kind.wire_code(),
        excluded_peers: excluded_peers
            .iter()
            .map(|hash| i2pr_proto::Hash::from_bytes(*hash.as_bytes()))
            .collect(),
        reply_encryption: ReplyEncryption::None,
    };
    // Round-trip through the codec to ensure structural invariants
    // hold before the state machine hands the body to the runtime
    // adapter.
    let body = i2pr_proto::I2npBody::DatabaseLookup(Box::new(DatabaseLookupMessage {
        key: message.key,
        from: message.from,
        delivery_flag: message.delivery_flag,
        reply_tunnel_id: message.reply_tunnel_id,
        lookup_type: message.lookup_type,
        excluded_peers: message.excluded_peers.clone(),
        reply_encryption: match &message.reply_encryption {
            ReplyEncryption::None => ReplyEncryption::None,
            _ => unreachable!("builder only emits None"),
        },
    }));
    body.encode_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
        .map_err(|_| DatabaseLookupBuildError::CodecRejected { context: "encode" })?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_lookups_without_a_reply_path() {
        let target = RouterHash::from_bytes([0x11u8; 32]);
        let error = build_databaselookup(&target, LookupKind::RouterInfo, None, &[]).unwrap_err();
        assert_eq!(error, DatabaseLookupBuildError::ReplyPathMissing);
    }

    #[test]
    fn rejects_oversized_excluded_peer_list() {
        let target = RouterHash::from_bytes([0x22u8; 32]);
        let gateway = RouterHash::from_bytes([0x33u8; 32]);
        let path = ReplyPath::new(gateway, 1).expect("path");
        let excluded = vec![RouterHash::from_bytes([0x77u8; 32]); LOOKUP_EXCLUDED_PEER_BUDGET + 1];
        let error = build_databaselookup(&target, LookupKind::RouterInfo, Some(&path), &excluded)
            .unwrap_err();
        assert!(matches!(
            error,
            DatabaseLookupBuildError::ExcludedPeersTooLarge { .. }
        ));
    }

    #[test]
    fn builds_databaselookup_with_routerinfo_kind() {
        let target = RouterHash::from_bytes([0x44u8; 32]);
        let gateway = RouterHash::from_bytes([0x55u8; 32]);
        let path = ReplyPath::new(gateway, 7).expect("path");
        let excluded = vec![RouterHash::from_bytes([0x66u8; 32])];
        let lookup = build_databaselookup(&target, LookupKind::RouterInfo, Some(&path), &excluded)
            .expect("lookup");
        assert_eq!(lookup.lookup_type, 2);
        assert_eq!(lookup.key, i2pr_proto::Hash::from_bytes(*target.as_bytes()));
        assert_eq!(
            lookup.from,
            i2pr_proto::Hash::from_bytes(*gateway.as_bytes())
        );
        assert!(lookup.delivery_flag);
        assert_eq!(lookup.reply_tunnel_id, Some(7));
        assert_eq!(lookup.excluded_peers.len(), 1);
        assert_eq!(lookup.reply_encryption, ReplyEncryption::None);
    }

    #[test]
    fn built_lookup_round_trips_through_i2np_codec() {
        let target = RouterHash::from_bytes([0x77u8; 32]);
        let gateway = RouterHash::from_bytes([0x88u8; 32]);
        let path = ReplyPath::new(gateway, 42).expect("path");
        let lookup = build_databaselookup(&target, LookupKind::RouterInfo, Some(&path), &[])
            .expect("lookup");
        let body = i2pr_proto::I2npBody::DatabaseLookup(Box::new(DatabaseLookupMessage {
            key: lookup.key,
            from: lookup.from,
            delivery_flag: lookup.delivery_flag,
            reply_tunnel_id: lookup.reply_tunnel_id,
            lookup_type: lookup.lookup_type,
            excluded_peers: lookup.excluded_peers.clone(),
            reply_encryption: ReplyEncryption::None,
        }));
        let encoded = body
            .encode_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode");
        let message = i2pr_proto::I2npMessage::new_standard(
            1,
            i2pr_proto::Date::from_millis(0),
            i2pr_proto::I2npBody::DatabaseLookup(Box::new(DatabaseLookupMessage {
                key: lookup.key,
                from: lookup.from,
                delivery_flag: lookup.delivery_flag,
                reply_tunnel_id: lookup.reply_tunnel_id,
                lookup_type: lookup.lookup_type,
                excluded_peers: lookup.excluded_peers.clone(),
                reply_encryption: ReplyEncryption::None,
            })),
        )
        .expect("message");
        let encoded_with_header = message
            .encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)
            .expect("encode-with-header");
        assert!(!encoded_with_header.is_empty());
        let decoded = i2pr_proto::I2npMessage::decode_standard(
            &encoded_with_header,
            i2pr_proto::MAX_I2NP_PAYLOAD_SIZE,
        )
        .expect("decode");
        let i2pr_proto::I2npBody::DatabaseLookup(round_tripped) = decoded.body() else {
            panic!("unexpected body variant");
        };
        assert_eq!(round_tripped.lookup_type, lookup.lookup_type);
        assert_eq!(round_tripped.key, lookup.key);
        assert_eq!(round_tripped.from, lookup.from);
        assert_eq!(round_tripped.reply_tunnel_id, lookup.reply_tunnel_id);
        let _ = encoded;
    }
}
