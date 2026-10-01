# Plan 273 — M12 bounded inbound DatabaseStore service

Status: **passed-m12-bounded-databasestore-service**

Classification: capability.

Hard dependency: Plan 272 passed.

## 1. Objective

Implement the runtime-neutral floodfill DatabaseStore server path: classify inbound provenance,
validate/store through the Plan 272 record surface, enforce abuse controls, decide DeliveryStatus
acknowledgement, and emit a typed replication-eligibility action for valid newer publisher stores.

Replication itself remains Plan 275.

## 2. Architecture

Add FloodfillService (or equivalent) in i2pr-netdb. It accepts:
- decoded DatabaseStoreMessage;
- explicit authenticated inbound context from Plan 271;
- role state (disabled/serving);
- caller-supplied wall/monotonic time;
- bounded policy.

It emits typed effects such as Ack, Stored, ReplicationEligible, Drop, Throttled, or
Unsupported. It does not send sockets/tunnels or spawn tasks.

## 3. Invariants

- Disabled role performs no floodfill serving.
- Validation precedes mutation and replication eligibility.
- Reply-token semantics follow ADR 0027; tunneled/direct provenance is never inferred from message
  fields alone.
- Zero-token flood replicas never request ack and never become re-flood candidates.
- Stale/idempotent/conflicting records do not create duplicate flood work.
- Own RouterInfo/store collision behavior is explicit and does not permit key takeover.
- Per-peer, per-key, global request, byte, decompression, and crypto-work budgets are bounded.
- Ack size/work cannot be amplified by record payload size.
- Invalid input produces typed outcomes without peer-controlled logs or panic.

## 4. Required production changes

- FloodfillStorePolicy with hard ceilings and deterministic windows.
- Bounded per-source/per-key throttling using authenticated source where available and reply-path
  tuple only as secondary data.
- Store handler for all Plan 272 record classes.
- DeliveryStatus action carrying the original nonzero reply token when protocol rules require it.
- ReplicationCandidate action containing only validated record identity/type and immutable encoded
  data/reference; no transport handle.
- Privacy-safe counters for accepted/new/replaced/stale/conflict/invalid/throttled/capacity.

## 5. Scope / non-goals

No DatabaseLookup serving, ECIES reply encryption, peer selection/flood send, persistence format,
daemon composition, config, or caps=f.

## 6. Work packages

A. Define inbound context and action/outcome types.

B. Implement store state machine and mutation ordering.

C. Add throttles and accounting. Use caller-supplied time so windows are deterministic.

D. Wire record classes and unpublished/serve/flood eligibility decisions from Plans 271/272.

E. Add negative/adversarial tests including compressed RI bombs, repeated keys, spoofed reply
fields, cross-namespace store attempts, and quota pressure.

## 7. Failure / cancellation / restart / contention

Synchronous service has no cancellation token. The daemon may drop an emitted effect later; store
state must therefore not assume successful ack/flood delivery. Replication is best-effort typed
work, not part of store transaction atomicity.

On quota exhaustion reject before expensive optional work where possible, while still performing
protocol-required constant/bounded parsing necessary to classify safely.

## 8. Compatibility and migration

No normal-daemon behavior changes because the service is not composed yet. Existing unsolicited
RouterInfo client ingestion remains separate or is migrated through an explicitly non-floodfill
policy adapter.

## 9. Required tests

- each record type valid-newer -> stored + correct ack/flood eligibility;
- zero-token replica -> store only, no ack/re-flood;
- stale/idempotent/conflict -> no flood;
- invalid signature/key -> no store/ack/flood beyond protocol-required response policy;
- tunneled/direct reply-field semantics;
- namespace leakage negative cases;
- per-peer/per-key/global throttle windows;
- count/byte/decompression/crypto budget exhaustion;
- own-RI/self-key handling;
- virtual-time window reset;
- redacted diagnostics.

## 10. Exact verification

~~~bash
cargo fmt --all --check
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo clippy --locked -p i2pr-netdb --all-targets -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh
~~~

## 11. Documentation

Document server store semantics and explicit distinction from client lookup response ingestion.
Support matrix remains non-advertised.

## 12. Acceptance criteria

- All Plan 272 record classes enter through one authoritative store server path.
- Every action is bounded and typed.
- Zero-token no-reflood invariant is statically/test locked.
- Cross-namespace disclosure/flood eligibility is impossible through public APIs.
- No transport/runtime/persistence activation exists.
- caps=f remains impossible.
- No critical/high finding remains.

## 13. Stop conditions

Stop if protocol-required acknowledgement semantics cannot be represented without daemon effects,
if provenance is missing at the dispatcher boundary, or if safe throttling would require
unbounded source state.

## 14. Closure evidence

Requirement-to-test matrix, throttle/budget evidence, API boundary audit, commands, and unblock
audit. On pass, move Plan 274 to ready.

## 15. Handoff

Plan 274 may add lookup serving but may not bypass this store path for DatabaseStore hits/replies.
