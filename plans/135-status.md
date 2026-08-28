# Plan 135 status — Milestone 7 SAM 3.1 planning authority

Status: **`superseded-by-plan140-audit`**.

Registered: **2026-08-27**.

Source product floor: Plan 134, `passed-milestone6-recv-window-ack-ceiling-closure`.

Plan of record:
[`plans/135-m7-sam31-implementation-roadmap.md`](135-m7-sam31-implementation-roadmap.md).

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure

plan_135 = superseded-by-plan140-audit
plan_136 = passed-sam31-protocol-private-destination-foundation
plan_137 = passed-m7-sam31-loopback-server-session-lifecycle
plan_138 = passed-m7-sam31-stream-connect-accept-bridge
plan_139 = passed-m7-sam31-forward-naming-hardening
plan_140 = blocked-independent-client-stream-path-not-ready

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately

milestone7 = local-product-implemented; closure-blocked
sam_baseline = 3.1-stream
router_construction = may-continue
next_executable_plan = SAM stream-bridge and Base64 compatibility correction
```

## Phase 7 execution sequence

The implementation sequence is intentionally narrow and ordered:

1. **Plan 136 — SAM 3.1 protocol and private-destination foundation**
   - create `i2pr-api` at the application-adapter layer;
   - implement bounded SAM line parsing, typed commands/replies, and exact 3.1 negotiation;
   - reconcile the standard SAM/PrivateKeyFile private-destination representation against i2pr's current Ed25519/X25519 destination profile;
   - implement `DEST GENERATE SIGNATURE_TYPE=7` and strict private-destination import/export without adding generic secret-key getters.

2. **Plan 137 — loopback server and session lifecycle**
   - add the supervised loopback TCP service;
   - implement HELLO, DEST GENERATE, and `SESSION CREATE STYLE=STREAM`;
   - establish transactional session/destination ownership and control-socket teardown;
   - enforce client/session/task/buffer limits.

3. **Plan 138 — STREAM CONNECT / ACCEPT bridge**
   - attach real SAM stream sockets to the existing Milestone 6 `StreamingManager` and `StreamingDestinationAdapter` product path;
   - preserve raw-mode, SILENT, retransmission, ACK, close/reset, and bounded backpressure semantics;
   - prove bidirectional arbitrary binary byte transfer through the local destination architecture.

4. **Plan 139 — STREAM FORWARD, naming, and hardening**
   - implement ordinary STREAM FORWARD with the Milestone 7 loopback-target security restriction;
   - implement `NAMING LOOKUP`, including `NAME=ME` and full-Destination normalization, without inventing an address book or system-DNS path;
   - close aggregate resource, cancellation, unsupported-feature, and logging/privacy requirements.

5. **Plan 140 — independent-client interoperability and Milestone 7 closure**
   - exercise at least two independently implemented SAM clients against the real localhost listener;
   - prove cross-client STREAM interoperability, DEST GENERATE/private import, FORWARD, naming, negative-version behavior, and lifecycle/resource closure;
   - rerun Milestone 6 local regressions;
   - update the roadmap, README, protocol-support matrix, architecture, and security-model documentation;
   - close Milestone 7 and hand off to Milestone 8 / SSU2 planning.

## Roadmap amendment authority

The historical Milestone 7 section in `plans/000-mvp-roadmap.md` omits `DEST GENERATE`. Until Plan 140 performs the final top-level roadmap/documentation update, **Plan 135 is the authoritative Milestone 7 amendment** and defines the baseline as SAM 3.1 STREAM including `DEST GENERATE SIGNATURE_TYPE=7`.

Do not interpret that temporary documentation drift as permission to omit DEST GENERATE during implementation.

## Progression policy

Plan 134 already settled the development-environment sequencing issue:

- independent-router destination, Streaming, tunnel, and live-transport interoperability remains explicit external MVP acceptance debt;
- that debt does not block the SAM baseline;
- do not reopen the retired rootless, VM, Emissary live-wire, or public-network validation lanes merely to satisfy historical milestone sequencing;
- do not create another generalized Milestone 6 closure pass unless a **new concrete protocol defect** is demonstrated by Phase 7 work.

SAM validation is deliberately useful in the current environment because the application-facing protocol can be exercised through ordinary localhost TCP sockets with no root, namespaces, VM, Docker, public I2P access, or privileged network setup.

## Handoff instruction

The implementation handoff begins with:

[`plans/136-m7-sam31-protocol-private-destination-foundation.md`](136-m7-sam31-protocol-private-destination-foundation.md)

The implementer should read Plan 135 first, execute Plan 136 only, create `plans/136-status.md` with exact evidence at closure, and proceed sequentially. Later plans must not be pulled forward to make an early implementation patch larger.

Do not register Plan 137 as executable until Plan 136 proves that i2pr's current destination profile round-trips through a standards-compatible SAM private-destination representation.
