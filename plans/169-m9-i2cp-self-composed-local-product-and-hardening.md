# Plan 169 — Milestone 9 I2CP self-composed local product and hardening

Status: **blocked until Plan 168 passes**.

## 1. Goal

Close the complete local I2CP product before independent-client testing: transactional reconfiguration, adversarial lifecycle/resource behavior, and a black-box self-composed real-TCP trajectory using only I2CP after listener startup.

This is the local acceptance gate for M9. Independent external clients remain Plan 170.

Expected closure:

```text
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
next_executable_plan = 170
```

## 2. Reconfiguration transaction

Implement the Plan 165 reconfiguration model against the real client-owned destination runtime.

Rules:

- parse/verify the complete new SessionConfig first;
- authorization signature/date rules remain enforced;
- classify options using the same projector as CreateSession;
- immutable identity/LeaseSet/key-profile changes fail explicitly;
- immediate policy changes commit atomically;
- tunnel length/quantity changes that require rebuild use staged replacement;
- retain old usable tunnels/LeaseSet only while valid and policy permits;
- request a fresh client LeaseSet2 for replacement inbound leases;
- do not publish replacement leases until the client supplies a valid matching LeaseSet2/key set;
- on rebuild/LeaseSet failure, retain the old usable configuration if safe or return a typed degraded/failure state; never half-apply.

Test reconfigure cancellation at every stage.

## 3. Session destruction semantics

Pin exact behavior for the declared I2CP profile and selected Java/Go references.

Destroy/disconnect must:

- reject new sends immediately;
- cancel pending lookups/status correlations/lease refreshes;
- stop destination tunnel/runtime work;
- withdraw/release client-owned publication state;
- zeroize inbound decryption and ECIES session material;
- release session/destination registry capacity;
- preserve sibling connections/sessions;
- finish within a named shutdown deadline.

Repeated destroy/close must be idempotent from a resource perspective even if the wire reply semantics differ.

## 4. Dedicated final local acceptance suite

Create a narrowly named suite, e.g.:

```text
crates/i2pr-daemon/tests/i2cp_final_acceptance.rs
```

Do not keep inflating `i2cp_loopback.rs` if that makes evidence mapping ambiguous.

Canonical self-composed trajectory:

1. start the daemon/listener using public test composition;
2. connect two raw I2CP test clients over real localhost TCP;
3. each performs protocol byte + GetDate/SetDate;
4. each creates a separately signed client-owned Destination;
5. each receives real lease requests from its destination tunnel pool;
6. each supplies signed Standard LeaseSet2 + matching X25519 decryption key;
7. both sessions become usable;
8. A sends small and near-limit payloads to B;
9. B verifies bytes/metadata then replies small and near-limit to A;
10. exercise status, destination lookup, and bandwidth queries;
11. reconfigure one mutable tunnel policy through full staged transaction;
12. close one session and prove the other remains usable;
13. recreate/destroy sessions repeatedly;
14. close both/control sockets;
15. assert listener/session/destination/tunnel/status/lookup/resource baselines.

After listener startup, the test must not call private destination/LeaseSet/ECIES/dispatch setup methods to cause positive behavior. Only TCP/I2CP inputs drive the product. Sanitized read-only counters are allowed for boundedness/baseline assertions.

## 5. Adversarial protocol matrix

Add executable cases for at least:

- wrong/missing protocol byte;
- oversized frame length before body allocation;
- truncated/stalled frame;
- high-rate zero/small unknown frames;
- message family in invalid connection/session state;
- duplicate CreateSession/destination;
- malformed/noncanonical mapping;
- bad SessionConfig signature;
- stale/future SessionConfig date just outside boundary;
- unsupported signing/encryption/LeaseSet types;
- invalid option min/max/max+1 and numeric overflow;
- foreign/unrequested/expired/duplicate LeaseSet lease;
- mismatched LeaseSet2 X25519 private/public key;
- repeated invalid LeaseSet2 attempts;
- send before session usable;
- payload max/max+1 and decompression expansion ceiling;
- invalid/expired SendMessageExpires;
- status-nonce flood;
- concurrent lookup ceiling;
- slow reader and slow writer;
- abrupt client reset at each lifecycle phase;
- daemon cancellation with active/pending sessions.

Every case must have a bounded deadline and a post-case resource-baseline assertion where applicable.

## 6. Concurrency/resource matrix

Prove exact configured capacities and max+1 for:

```text
TCP connections
sessions/destinations
pending destination commands
pending I2CP outbound frames/bytes
pending application messages/bytes
pending status correlations
pending lookups
pending lease/reconfigure transactions
```

Where a lower router-wide ceiling already exists, tests must prove I2CP cannot exceed it by multiplying per-connection quotas.

At least one test must hold one slow/malicious connection at its ceiling while a sibling connection continues a valid message exchange.

## 7. Repeated lifecycle soak

Add a deterministic bounded repeat test (not an hours-long benchmark) that creates/activates/exchanges/destroys client-owned sessions enough times to catch monotonic registry/task/secret/status retention.

Record before/after counters for non-secret resource classes. Avoid timing assertions tighter than the hosted runner can reliably satisfy.

## 8. Security/secret review

Audit M9 changes for:

- client destination signing key never present in router memory/API;
- decryption key wrapper redaction/zeroization;
- no raw LeaseSet private key or application payload logs;
- errors do not echo opaque client bodies;
- no non-loopback listener path;
- no unbounded channel/map/vector introduced;
- session IDs alone are never authority without connection ownership;
- stale async events cannot target a newly reused session.

Add static/runtime checks where practical; document remaining limitations.

## 9. Regression floor

Explicitly run the key retained products:

```text
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-vectors.sh
```

Do not re-run external SAM/SSU2 workflows merely because local I2CP tests were added unless a shared production surface changed in a way their evidence specifically guards.

Then run the full Plan 163 workspace floor.

## 10. Documentation and support classification

Before closure update:

- root README;
- `plans/README.md` current M9 state;
- `AGENTS.md` / local-dev skill if used for executor authority;
- `docs/architecture/i2pr-api.md`;
- `docs/architecture/i2pr-client.md`;
- `docs/architecture/i2pr-daemon.md`;
- security model;
- I2CP protocol dossier;
- `specs/support.toml` and `specs/CONFORMANCE.md`.

Classification at this point must say **local I2CP product passed, independent-client acceptance pending**. Keep I2CP experimental, disabled by default, loopback-only, and non-advertised.

## 11. Acceptance criteria

Plan 169 closes only when:

1. reconfiguration is all-or-nothing and mutable/immutable option behavior matches Plan 165;
2. tunnel-policy reconfigure retains old usable state until valid replacement LeaseSet2 is ready or fails safely;
3. destroy/disconnect/cancel release all session/destination/tunnel/secret/status/lookup resources;
4. the canonical two-destination real-TCP self-composed trajectory passes without private behavior-driving calls after listener startup;
5. small and near-limit payloads pass bidirectionally;
6. closing one client/session does not disrupt its sibling;
7. the adversarial protocol/security matrix passes with bounded deadlines;
8. every named resource ceiling has capacity/max+1 evidence;
9. slow reader/writer tests prove bounded memory and sibling isolation;
10. repeated lifecycle test shows no monotonic retained resource growth;
11. security audit confirms no client signing-key ownership/logging and decryption secrets are redacted/zeroized;
12. M6/M7/M8 local regressions remain green;
13. support/docs describe local-only M9 state without independent/public overclaim;
14. full workspace floor and routine CI pass on the exact closing commit;
15. `plans/169-status.md` records exact local evidence and states Plan 170 is next.

## 12. Stop conditions

Stop and write a narrow corrective if the self-composed test exposes a destination/ECIES/tunnel protocol defect, if reconfiguration requires destructive replacement before the new state is ready, or if boundedness requires weakening existing router-wide ceilings. Do not weaken final acceptance to continue.

## 13. Handoff

After Plan 169 passes, execute final M9 Plan **170**.