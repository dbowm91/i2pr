//! Bounded direct-only DatabaseStore replication planning.

use std::collections::BTreeSet;

use i2pr_proto::{DatabaseStoreMessage, Date, Hash};

use crate::{
    FloodfillTime, ProvenanceEligibility, ReplicationCandidate, RouterHash, ServerNetDb,
    daily_routing_key, xor_distance,
};

const DAY_MS: u64 = 86_400_000;
const RI_ROLLOVER_MS: u64 = 45 * 60 * 1000;
const LS_ROLLOVER_MS: u64 = 10 * 60 * 1000;
const NEXT_KEY_TARGETS: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplicationPolicy {
    pub fanout: usize,
    pub max_candidates: usize,
    pub max_work: usize,
    pub max_record_age_ms: u64,
    pub deadline_ms: u64,
}

impl Default for ReplicationPolicy {
    fn default() -> Self {
        Self {
            fanout: 3,
            max_candidates: 256,
            max_work: 4096,
            max_record_age_ms: 60 * 60 * 1000,
            deadline_ms: 30_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloodfillPeerView {
    pub peer: RouterHash,
    pub floodfill: bool,
    pub hidden: bool,
    pub verified: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingKeyClass {
    Current,
    Next,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectFloodAction {
    pub peer: Hash,
    pub message: DatabaseStoreMessage,
    pub deadline_ms: u64,
    pub routing_key_class: RoutingKeyClass,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReplicationStats {
    pub candidates_examined: usize,
    pub selected: usize,
    pub excluded: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplicationPlan {
    pub actions: Vec<DirectFloodAction>,
    pub stats: ReplicationStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplicationError {
    InvalidPolicy,
}

pub struct ReplicationPlanner {
    policy: ReplicationPolicy,
}

impl ReplicationPlanner {
    pub fn new(policy: ReplicationPolicy) -> Result<Self, ReplicationError> {
        if policy.fanout == 0
            || policy.max_candidates == 0
            || policy.max_work < policy.max_candidates
            || policy.deadline_ms == 0
        {
            return Err(ReplicationError::InvalidPolicy);
        }
        Ok(Self { policy })
    }

    pub fn plan(
        &self,
        db: &ServerNetDb,
        candidate: ReplicationCandidate,
        peers: &[FloodfillPeerView],
        local: Hash,
        time: FloodfillTime,
    ) -> ReplicationPlan {
        let mut stats = ReplicationStats::default();
        if db.may_replicate(
            &candidate.record,
            time.wall_ms,
            self.policy.max_record_age_ms,
        ) != ProvenanceEligibility::Allowed
        {
            return ReplicationPlan {
                actions: Vec::new(),
                stats,
            };
        }
        let Some(message) = db
            .database_store_for_answer(
                candidate.record.record_type(),
                *candidate.record.key(),
                time.wall_ms,
                self.policy.max_record_age_ms,
                64 * 1024,
            )
            .ok()
            .flatten()
        else {
            return ReplicationPlan {
                actions: Vec::new(),
                stats,
            };
        };
        let local = RouterHash::from_hash(local);
        let source = candidate.source_peer.map(RouterHash::from_hash);
        let mut eligible = Vec::new();
        for peer in peers
            .iter()
            .take(self.policy.max_candidates.min(self.policy.max_work))
        {
            stats.candidates_examined += 1;
            if !peer.verified
                || !peer.floodfill
                || peer.hidden
                || peer.peer == local
                || Some(peer.peer) == source
                || (candidate.record.record_type() == 0
                    && *peer.peer.as_hash() == *candidate.record.key())
            {
                stats.excluded += 1;
                continue;
            }
            eligible.push(peer.peer);
        }
        let current_key = RouterHash::from_hash(
            routing_key(&candidate.record, time.wall_ms, false)
                .unwrap_or_else(|| *candidate.record.key()),
        );
        eligible.sort_by_key(|peer| (xor_distance(peer, &current_key), *peer.as_hash()));
        eligible.dedup();
        let mut selected: Vec<(RouterHash, RoutingKeyClass)> = eligible
            .iter()
            .take(self.policy.fanout)
            .copied()
            .map(|p| (p, RoutingKeyClass::Current))
            .collect();
        let remaining_ms = DAY_MS - (time.wall_ms % DAY_MS);
        let lease_survives = candidate.record.record_type() == 0
            || db
                .database_store_for_answer(
                    candidate.record.record_type(),
                    *candidate.record.key(),
                    time.wall_ms.saturating_add(remaining_ms),
                    self.policy.max_record_age_ms,
                    64 * 1024,
                )
                .ok()
                .flatten()
                .is_some();
        if should_cover_next_key(candidate.record.record_type(), remaining_ms, lease_survives) {
            let next_key = RouterHash::from_hash(
                routing_key(&candidate.record, time.wall_ms, true)
                    .unwrap_or_else(|| *candidate.record.key()),
            );
            let mut next = eligible.clone();
            next.sort_by_key(|peer| (xor_distance(peer, &next_key), *peer.as_hash()));
            for peer in next.into_iter().take(NEXT_KEY_TARGETS) {
                if selected.len() >= self.policy.fanout + NEXT_KEY_TARGETS {
                    break;
                }
                if !selected.iter().any(|(chosen, _)| *chosen == peer) {
                    selected.push((peer, RoutingKeyClass::Next));
                }
            }
        }
        let actions = selected
            .into_iter()
            .map(|(peer, routing_key_class)| DirectFloodAction {
                peer: *peer.as_hash(),
                message: message.clone(),
                deadline_ms: time.wall_ms.saturating_add(self.policy.deadline_ms),
                routing_key_class,
            })
            .collect::<Vec<_>>();
        stats.selected = actions.len();
        ReplicationPlan { actions, stats }
    }
}

fn routing_key(record: &crate::RecordId, now_ms: u64, next: bool) -> Option<Hash> {
    let base = now_ms / DAY_MS * DAY_MS;
    let date = Date::from_millis(base.saturating_add(if next { DAY_MS } else { 0 }));
    daily_routing_key(&RouterHash::from_hash(*record.key()), date).ok()
}

fn should_cover_next_key(record_type: u8, remaining_ms: u64, lease_survives: bool) -> bool {
    if record_type == 0 {
        remaining_ms < RI_ROLLOVER_MS
    } else {
        remaining_ms < LS_ROLLOVER_MS && lease_survives
    }
}

pub fn nearest_peers(peers: &[FloodfillPeerView], target: &Hash, limit: usize) -> Vec<RouterHash> {
    let mut seen = BTreeSet::new();
    let mut values: Vec<_> = peers
        .iter()
        .filter(|p| p.verified && p.floodfill && !p.hidden && seen.insert(p.peer))
        .map(|p| p.peer)
        .collect();
    let target = RouterHash::from_hash(*target);
    values.sort_by_key(|peer| (xor_distance(peer, &target), *peer.as_hash()));
    values.truncate(limit);
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(byte: u8, verified: bool, floodfill: bool, hidden: bool) -> FloodfillPeerView {
        FloodfillPeerView {
            peer: RouterHash::from_hash(Hash::from_bytes([byte; 32])),
            verified,
            floodfill,
            hidden,
        }
    }

    #[test]
    fn nearest_selection_filters_unverified_hidden_and_non_floodfill_peers() {
        let target = Hash::from_bytes([0; 32]);
        let peers = [
            peer(4, true, true, false),
            peer(1, true, true, false),
            peer(2, false, true, false),
            peer(3, true, false, false),
            peer(5, true, true, true),
        ];
        let selected = nearest_peers(&peers, &target, 2);
        assert_eq!(
            selected.iter().map(|p| *p.as_hash()).collect::<Vec<_>>(),
            vec![Hash::from_bytes([1; 32]), Hash::from_bytes([4; 32])]
        );
    }

    #[test]
    fn routing_key_rollover_windows_are_strict_and_type_specific() {
        let record = crate::RecordId::new(0, Hash::from_bytes([7; 32]));
        let before_midnight = DAY_MS - 1;
        assert_eq!(DAY_MS - (before_midnight % DAY_MS), 1);
        assert_ne!(
            routing_key(&record, before_midnight, false),
            routing_key(&record, before_midnight, true)
        );
        assert_eq!(DAY_MS - (DAY_MS % DAY_MS), DAY_MS);
        assert_eq!(
            routing_key(&record, DAY_MS, false),
            routing_key(&record, DAY_MS, false)
        );
        assert_eq!(RI_ROLLOVER_MS, 45 * 60 * 1000);
        assert_eq!(LS_ROLLOVER_MS, 10 * 60 * 1000);
        assert!(!should_cover_next_key(0, RI_ROLLOVER_MS, true));
        assert!(should_cover_next_key(0, RI_ROLLOVER_MS - 1, true));
        assert!(!should_cover_next_key(1, LS_ROLLOVER_MS, true));
        assert!(should_cover_next_key(1, LS_ROLLOVER_MS - 1, true));
        assert!(!should_cover_next_key(1, LS_ROLLOVER_MS - 1, false));
    }

    #[test]
    fn direct_action_shape_has_zero_token_and_no_tunnel_route() {
        let message = DatabaseStoreMessage {
            key: Hash::from_bytes([1; 32]),
            reply_token: 0,
            reply_tunnel_id: None,
            reply_gateway: None,
            data: i2pr_proto::DatabaseStoreData::Deferred {
                store_type: i2pr_proto::DatabaseStoreType::RouterInfo,
                payload: i2pr_proto::DeferredPayload::new(vec![1], 1).expect("bounded"),
            },
        };
        let action = DirectFloodAction {
            peer: Hash::from_bytes([2; 32]),
            message,
            deadline_ms: 1,
            routing_key_class: RoutingKeyClass::Current,
        };
        assert_eq!(action.message.reply_token, 0);
        assert_eq!(action.message.reply_tunnel_id, None);
        assert_eq!(action.message.reply_gateway, None);
    }
}
