# Plan 275 status — passed bounded direct replication and daily routing-key rollover

Status: **passed-m12-bounded-direct-replication-and-daily-routing-key-rollover**

Implementation commit: recorded in Git history for this closure.

## Requirement-to-evidence

| Requirement | Evidence |
|---|---|
| Replication requires the validated publisher candidate and current `may_replicate` eligibility | `ReplicationPlanner::plan`; `ServerNetDb::may_replicate`; replicas cannot generate candidates in `FloodfillStoreService` |
| Authenticated source peer is excluded without persisting its identity | `ReplicationCandidate::source_peer`, redacted Debug; planner local/source/self exclusions |
| Only bounded, verified, visible floodfill facts are considered | `ReplicationPolicy::{max_candidates,max_work}`; `FloodfillPeerView`; planner tests and `nearest_peers` |
| Current daily routing key ordering and stable hash tie-break | `daily_routing_key`, `xor_distance`, stable `(distance, peer hash)` sort |
| Next-day rollover windows and strict boundary | pinned Java I2P 2.13.0 `FloodfillNetworkDatabaseFacade.java` constants and `flood` condition: 45 minutes for RI, 10 minutes for LeaseSet-family, strictly inside the window; LS requires a lease past midnight. Unit tests cover equality/inside and survival conditions. |
| Direct-only token-zero action shape | `DirectFloodAction` contains peer and `DatabaseStoreMessage`, with no route variant; message carries token zero and no reply tunnel fields |
| Deduplication and bounded action output | current selection plus at most two distinct next-key targets; max work/candidate ceiling applies before filtering |
| Expiry and age policy | `ServerNetDb::may_replicate` denies expired LS-family and RI older than one hour; reply-body construction also requires a currently answer-eligible record |

## Verification

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` — 165 passed.
- `rtk cargo clippy --locked -p i2pr-netdb --all-targets -- -D warnings` — passed.
- `rtk bash scripts/check-dependency-direction.sh` — passed.
- `rtk bash scripts/check-runtime-boundaries.sh` — passed.
- `rtk bash scripts/check-m12-floodfill-boundaries.sh` — passed.
- `rtk git diff --check` — passed.

Pinned source: Java I2P 2.13.0 commit `9134f808337b401e8e53c73734c81fab04280c9d`,
`router/java/src/net/i2p/router/networkdb/kademlia/FloodfillNetworkDatabaseFacade.java`.
ADR 0027 §5 freezes dual-key consideration, but not the numeric windows; this closure records
the numeric constants as pinned-reference behavior. No change to the accepted ADR was needed.

## Security, operational limits, and findings

No sockets, sends, retries, tunnels, persistence, peer-profile scoring, or advertisement are
implemented. The planner emits effects only. Type 5 remains deferred by Plan 281; no dependency
was added. No critical/high findings remain open; none recorded at medium/low.

## Unblock audit and roadmap disposition

Plan 276's hard dependency is satisfied. Move Plan 276 to ready. Plans 277–279 remain blocked in
sequence; Plan 280 remains stopped for the unavailable reviewed Red25519 provider. No other plan
is unblocked by this closure.
