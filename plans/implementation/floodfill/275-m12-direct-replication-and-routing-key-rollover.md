# Plan 275 — M12 direct replication and daily routing-key rollover

Status at registration:
**registered-m12-replication-blocked-on-plan274**

Classification: capability / resource policy.

Hard dependency: Plan 274 passed.

## 1. Objective

Implement the runtime-neutral flood replication planner for valid newer publishable records:
nearest eligible floodfill selection by daily routing key, direct-only zero-token DatabaseStore
actions, bounded fanout, and current/next-day rollover coverage.

No sockets are opened here.

## 2. Invariants

- Replication only consumes validated ReplicationCandidate values from Plan 273.
- Standard fanout is 3 successfully selected targets unless fewer eligible peers exist.
- All emitted flood stores have reply token zero.
- Flood actions are DirectOnly; no tunnel fallback is representable.
- The local router, source peer where policy requires, and RI self-key target are excluded.
- Zero-token received replicas are never re-flooded.
- Expired LeaseSets and RouterInfos older than one hour are never flooded.
- Current daily routing key determines normal placement.
- Rollover policy uses the ADR-frozen windows (expected reference behavior: RI about 45 minutes,
  LeaseSet-family about 10 minutes before UTC midnight) and remains caller-time deterministic.
- Selection work and candidate overfetch are bounded.
- Peer quality/diversity filters are separate policy from XOR distance and do not mutate codecs.

## 3. Required production changes

- ReplicationPolicy with fanout, candidate ceiling, age/expiry gates, rollover windows.
- FloodfillPeerView containing only validated selection facts needed by policy.
- ReplicationPlanner producing bounded DirectFloodAction values containing peer id, zero-token
  DatabaseStore body, deadline/priority category, and current/next-key provenance.
- Deterministic current/next-day routing-key helpers using existing daily key primitive.
- Selection diagnostics as aggregate counters only.

## 4. Scope / non-goals

No actual direct dial/session send, no retry task, no Java-style peer profile clone, no global
banlist implementation, no persistence, no role advertisement.

## 5. Work packages

A. Freeze current/next-day time boundaries and record-type age tests.

B. Implement eligible-peer filtering and nearest selection with stable ties.

C. Add bounded over-selection so invalid/ineligible peers cannot make the planner scan the whole
NetDB.

D. Build canonical zero-token DatabaseStore flood actions.

E. Add duplicate target/current+next-day deduplication.

F. Add adversarial tests for huge candidate stores, many same-prefix peers, stale records,
midnight boundaries, and no eligible peers.

## 6. Failure / cancellation / restart / contention

Planner is synchronous and stateless except bounded policy input. The daemon later owns send
failure/retry; a failed direct send does not cause tunnel fallback. Plan 277 may attempt bounded
alternate candidates but cannot exceed policy ceilings.

## 7. Compatibility and migration

No existing client selection behavior changes. Reuse daily_routing_key() but do not change its
wire-independent semantics.

## 8. Required tests

- exact current-day XOR ordering;
- UTC midnight -1ms / midnight / +1ms;
- next-day RI and LeaseSet windows;
- dedupe peers selected for both keys;
- fanout 3 and candidate ceiling;
- direct-only type cannot represent tunnel route;
- reply token always zero;
- no re-flood from FloodReplica provenance;
- RI age >1h denied;
- expired LS-family denied;
- local/source/self-key exclusion;
- deterministic tie order and bounded work.

## 9. Exact verification

~~~bash
cargo fmt --all --check
cargo test --locked -p i2pr-netdb --all-targets
cargo clippy --locked -p i2pr-netdb --all-targets -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
~~~

## 10. Documentation

Document replication and rollover as policy, distinguishing normative direct/zero-token/age rules
from reference-derived rollover window constants.

## 11. Acceptance criteria

- Planner emits only bounded direct zero-token actions.
- Placement uses daily routing keys correctly.
- Rollover is deterministic and deduplicated.
- All stale/expired/reflood negative tests pass.
- No runtime I/O or caps=f.
- No critical/high finding remains.

## 12. Stop conditions

Stop if exact reference behavior demonstrates a materially different rollover rule that affects
interoperability, or if selection requires a new peer-profile architecture beyond factual
eligibility inputs. Resolve separately rather than cloning reference scoring code.

## 13. Closure evidence

Record selection vectors, midnight tests, action invariants, bounded-work evidence, commands, and
unblock audit. On pass, move Plan 276 to ready.

## 14. Handoff

Plan 276 adds storage maintenance and restart behavior; it must preserve direct-only planner
semantics unchanged.
