# Plan 102: Milestone 4 RouterInfo, NetDB, bootstrap, and publication authority roadmap

## Status and authority

- Status: **planned; active Milestone 4 parent authority**.
- Date: 2026-08-13.
- Baseline: `abf5c28cf37f5e293516e52dee173e510c63a801` or a clean descendant that preserves the Plan 101 activation boundary.
- Parent roadmap: `plans/000-mvp-roadmap.md`, Milestone 4.
- Protocol dossier: `specs/protocols/04-reseed-netdb.md`.
- Milestone 3 closure authority: Plans 099, 100, and 101.
- First executable child plan: Plan 103.

This document is the authoritative handoff from the completed Milestone 3 development sequence into actual router construction. It intentionally replaces the many historical Milestone 3 "active" blocks as execution guidance. Those historical blocks remain useful audit records, but they do not control work after this plan.

## Executive decision

Milestone 3 is closed **for continuation of router development**, but NTCP2 is not release-qualified.

The retained development result is:

```text
plan_099                    = closed-protocol-defect-localized
plan_100                    = closed-with-recorded-procedural-deviation
plan_101                    = passed-daemon-ntcp2-activation-safety
development_interop         = protocol-defect-localized
exact_wire_stage            = noise_authenticated
ntcp2                       = experimental-non-advertised
normal_daemon_ntcp2         = disabled-and-unenableable
external_netdb_over_ntcp2   = blocked
plan_079                    = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
router_construction         = active
milestone_4                 = active
```

No child of Plan 102 may turn successful NTCP2 interoperability back into a prerequisite for local RouterInfo, local NetDB, persistence, reseed parsing/verification, or transport-neutral NetDB state-machine development.

The next engineering objective is to convert the existing protocol/runtime foundation into a router that owns a validated local network database and can bootstrap safely. The project should spend the next implementation sequence predominantly in Rust product code, not in Python harnesses, CI topology, evidence schemas, or transport-specific test infrastructure.

## Why this sequence is now correct

The repository already has the lower-level pieces Milestone 4 needs:

- `i2pr-proto` has bounded `RouterInfo`, `RouterAddress`, identity, mapping, hash, Lease/LeaseSet, and I2NP message structures.
- `i2pr-crypto` has router identity ownership, RouterInfo signing, and signature verification.
- `i2pr-storage` has versioned/atomic persistence patterns and persistent router identity ownership.
- `i2pr-core` has runtime-neutral lifecycle/resource contracts.
- `i2pr-transport` has transport-neutral peer/link/delivery contracts including `PeerId`, `EncodedI2npMessage`, `DeliveryRequest`, `DeliveryOutcome`, and `TransportManager`.
- `i2pr-runtime` owns Tokio effects and supervision.
- `i2pr-daemon` is now a real composition root with identity load, service graph ownership, supervisor startup, and graceful shutdown.
- `i2pr-transport-ntcp2` and its runtime adapter remain available for experimental/test use, but production activation is intentionally blocked.

The missing layer is stateful router behavior: validation and ownership of RouterInfos, bounded NetDB state, bootstrap ingestion, lookup/publication state machines, and composition of those services.

## Active execution sequence

The Milestone 4 implementation sequence is:

```text
Plan 102  Milestone 4 authority and roadmap
   |
   v
Plan 103  RouterInfo validation + local NetDB foundation
   |
   v
Plan 104  persistent NetDB cache + SU3 reseed trust path
   |
   v
Plan 105  transport-neutral NetDB lookup/store/publication state machines
   |
   v
Plan 106  daemon integration + external transport re-entry checkpoint
   |
   +--> local/bootstrap milestone complete but external transport blocked
   |       OR
   +--> bounded transport qualification/correction at the explicit checkpoint
           |
           v
        live RouterInfo lookup/publication evidence
           |
           v
        Milestone 4 closure
```

The child plans are intentionally sequential for handoff to smaller coding models. Do not execute Plan 104 before Plan 103 closes. Do not execute Plan 105 before Plan 104 closes. Do not enter the Plan 106 live-I2P checkpoint before Plans 103-105 close.

A child plan may identify a compile/test defect owned by an earlier child plan. Fix that defect narrowly before proceeding; do not create another broad roadmap unless the architecture itself is invalidated.

## Child-plan responsibility split

### Plan 103 — local trust boundary

Create `i2pr-netdb` as a runtime-neutral crate. Implement cryptographic and temporal RouterInfo validation, RouterHash derivation/binding, bounded in-memory storage, deterministic replacement/conflict/expiry policy, floodfill-capability extraction as data rather than trust, and construction of the local signed RouterInfo without advertising unqualified transports.

Plan 103 opens no sockets and performs no filesystem I/O.

### Plan 104 — durable bootstrap trust path

Add an untrusted persistent RouterInfo cache and the SU3 reseed verification/ingestion path. Persist canonical signed RouterInfo bytes, revalidate every record at load, and isolate corruption. Implement a bounded SU3 reseed parser/verifier using an explicit reseed-signer trust store, bounded ZIP extraction, filename/hash binding, and a minimal multi-source HTTPS acquisition policy.

The only public-network activity introduced by Plan 104 is clearnet HTTPS reseed acquisition. It must not connect to the I2P network or activate NTCP2.

### Plan 105 — transport-neutral distributed NetDB logic

Implement pure lookup/store/publication state machines around the existing I2NP `DatabaseLookup`, `DatabaseStore`, `DatabaseSearchReply`, and `DeliveryStatus` message models. Implement daily routing-key derivation, bounded floodfill candidate selection, iterative lookup progression, duplicate coalescing, timeouts, cancellation, and store/publication acknowledgement bookkeeping.

Plan 105 emits actions such as "send this I2NP message to this peer"; it does not know whether the peer link is NTCP2, SSU2, a deterministic test link, or a future transport.

### Plan 106 — composition and external checkpoint

Wire local NetDB, cache, reseed, query engine, and local RouterInfo ownership into the daemon/runtime. Establish startup/readiness/shutdown semantics. Then, and only then, evaluate the minimum transport work needed to carry one real RouterInfo lookup/store exchange.

Plan 106 must not silently reactivate NTCP2. The Plan 101 configuration guard remains in force until a deliberate activation decision is reached at the Plan 106 checkpoint.

## Architecture and dependency constraints

The preferred dependency direction for this sequence is:

```text
i2pr-proto -----+
                 +--> i2pr-netdb
 i2pr-crypto ----+

 i2pr-core --------------------+
 i2pr-transport ---------------+--> i2pr-runtime --> i2pr-daemon
 i2pr-netdb -------------------+
 i2pr-storage -----------------+
```

`i2pr-netdb` should remain free of Tokio, sockets, DNS, HTTP clients, and direct filesystem ownership. It may depend on `i2pr-proto` and `i2pr-crypto`; add `i2pr-core` only if a genuinely shared runtime-neutral type is needed. Do not make `i2pr-netdb` depend on `i2pr-runtime`, `i2pr-daemon`, or a transport implementation.

Persistence effects remain owned by `i2pr-storage` and composition/runtime code. Network effects remain owned by `i2pr-runtime`/daemon adapters. Protocol-specific transport crates must not mutate NetDB directly.

Do not create a generic repository/plugin/event-bus framework merely to connect these crates. Use narrow typed APIs.

## Protocol authority for this sequence

Implement against the repository dossier first:

```text
specs/protocols/01-common-identity-crypto.md
specs/protocols/02-i2np.md
specs/protocols/04-reseed-netdb.md
specs/SOURCES.md
specs/CONFORMANCE.md
```

The current official I2P specifications remain the external authority when the dossier and implementation disagree. Important semantics for this sequence include:

- RouterInfo is keyed by SHA-256 of the contained RouterIdentity.
- RouterInfo signatures are verified against the contained signing public key and exact signed bytes.
- Reseed SU3 content type is RESEED and file type is ZIP; the signature covers the SU3 header through the content, and signer trust is content-type scoped.
- Reseed RouterInfo files are top-level entries and their names encode the router hash using the I2P Base64 alphabet.
- The NetDB routing key is the SHA-256 of the 32-byte search key concatenated with UTC `yyyyMMdd`; routing keys are local selection values and are never sent in I2NP messages.
- Floodfill membership is advertised through RouterInfo capability `f`, but capability text is untrusted input and does not make a peer trustworthy.
- RouterInfo lookups are an iterative floodfill operation with bounded continuation through returned candidates.

Do not copy Java/i2pd implementation policy where the protocol leaves room for local policy. Record any compatibility policy chosen by i2pr in code documentation/tests.

## Security and resource policy

Every Plan 102 child must preserve these invariants:

1. Untrusted data crosses one explicit validation boundary before becoming eligible state.
2. Persistence is never a trust boundary; cached data is revalidated on startup.
3. Signed containers do not bypass inner-record validation.
4. Every count, byte length, queue, retry set, outstanding query set, and disk budget is bounded.
5. Time is injected into state-machine/policy code where practical; do not bury wall-clock reads inside pure NetDB logic.
6. Equal-key conflicting records resolve deterministically and fail closed where ordering cannot be established safely.
7. External peer-provided lists are suggestions, not authority. Validate/deduplicate/filter them before changing query state.
8. Default diagnostics expose aggregate counts/reason categories, not raw peer inventories, full RouterInfos, keys, URLs with credentials, or packet contents.
9. Shutdown and cancellation release pending lookup/publication state and runtime-owned tasks.
10. No capability/address is advertised merely because a config flag says it is enabled.

## Scope lock for Milestone 4

The following are in scope:

- RouterInfo validation and local ownership;
- RouterHash binding;
- bounded RouterInfo NetDB state;
- persistence and restart revalidation;
- SU3 reseed client verification and ingestion;
- RouterInfo-oriented NetDB lookup/store/search-reply state machines;
- local RouterInfo publication state;
- daemon/runtime composition;
- a bounded live transport checkpoint sufficient to prove the Milestone 4 external path.

The following remain out of scope unless a child plan explicitly identifies a tiny prerequisite:

- LeaseSet2/EncryptedLeaseSet/MetaLeaseSet semantics beyond preserving existing codecs;
- destination LeaseSet publication;
- exploratory tunnel construction;
- tunnel data plane;
- garlic routing;
- streaming;
- SAM/I2CP;
- SSU2;
- transit tunnels;
- floodfill service role;
- peer profiling/reputation beyond minimal bounded observations needed for client selection;
- family trust/reputation policy;
- generic admin/control APIs;
- public-network long-duration operation.

Milestone 5 owns exploratory tunnels. Milestone 12 owns floodfill service behavior. Do not pull those milestones forward simply because NetDB client semantics reference them.

## NTCP2 and Plan 079 boundary

Plan 079 remains deferred. Plans 103-105 must not execute it and must not extend the Plan 099 workflow.

At the Plan 106 external checkpoint, there are two distinct questions:

1. Can the existing experimental NTCP2 implementation establish an authenticated link and carry the bounded I2NP operation needed for a RouterInfo exchange?
2. Is NTCP2 qualified for normal daemon activation and RouterInfo advertisement?

A positive answer to question 1 does **not** automatically answer question 2.

Before normal-daemon NTCP2 activation, public I2P peer connection, or NTCP2 address advertisement is enabled, Plan 106 must explicitly reconcile the intent of deferred Plan 079 and the current Plan 099/100 evidence. If a new transport correction is required, it must be narrow and defect-driven. Do not pre-create a speculative Plan 107 interop framework.

## Configuration posture

Until Plan 106 explicitly changes it:

```text
transport.ntcp2.enabled = false
explicit enabled=true   = rejected
ntcp2 advertised        = false
public I2P peer dial    = forbidden
reseed HTTPS            = allowed only after Plan 104
local NetDB             = allowed
local RouterInfo        = allowed
persistent NetDB cache  = allowed
```

A local RouterInfo created before transport qualification must contain no NTCP2 address. Zero transport addresses is preferable to publishing a false address.

## Test strategy

Testing should be layered and proportional:

```text
pure unit/property tests
    -> deterministic state-machine tests
    -> storage/reseed fixture tests
    -> daemon local integration tests
    -> one bounded external transport/NetDB checkpoint
```

Do not create a new plan-number-specific Python harness family. Prefer Rust tests for Rust product behavior. Python is acceptable only when it is materially simpler for a stable external fixture/tool boundary and the functionality cannot reasonably live in Rust.

No new CI workflow is required by default. Existing workspace CI should cover product code. External live validation, when Plan 106 reaches it, should reuse the smallest existing mechanism that can answer the exact compatibility question.

## Documentation/status rules

While executing this sequence:

- Update `plans/102...` or the active child status only when the authority graph materially changes.
- Historical Plans 038-101 should not receive broad rewrites.
- `plans/030-milestone-3-closure.md` is an archival Milestone 3 record; its historical "active" blocks do not override Plan 102.
- `plans/099-status.md` remains the authoritative record of the retained NTCP2 development result.
- Update `specs/support.toml`, protocol-support documentation, architecture docs, and README only when implementation support actually changes.

Do not represent locally implemented NetDB logic as live network support before Plan 106 obtains the relevant evidence.

## Milestone 4 terminal states

Plan 106 must finish in one of these explicit states:

### A. `milestone4-passed`

All local/bootstrap requirements pass, a real RouterInfo lookup through a mixed-router path succeeds, local RouterInfo publication is accepted/verified, and any transport activated for that purpose has passed the required activation checkpoint.

### B. `milestone4-local-foundation-complete-external-transport-blocked`

Plans 103-105 and daemon/bootstrap composition are complete, but the current environment or transport qualification prevents authentic external NetDB exchange. This is not Milestone 4 pass, but it preserves completed product implementation and local evidence without reopening general harness engineering.

### C. `milestone4-protocol-defect-localized`

The external checkpoint reaches an authentic transport/I2NP/NetDB stage and exposes a reproducible i2pr-owned protocol defect. Localize it precisely and create only the smallest corrective implementation plan needed to resume Plan 106.

Environment/harness limitations must never be relabeled as protocol failure.

## Plan 102 acceptance criteria

Plan 102 itself is complete as a planning authority when all of the following are true:

1. Plans 103-106 exist and each has explicit scope, dependencies, implementation surfaces, validation, and closure criteria.
2. The sequence preserves Plan 101's NTCP2 activation guard through Plans 103-105.
3. The sequence contains no new generic NTCP2 interoperability framework or CI topology.
4. `i2pr-netdb` has a clear runtime-neutral ownership boundary.
5. Reseed trust, storage trust, and RouterInfo trust boundaries are explicit.
6. The NetDB query layer is transport-neutral and does not require NTCP2 types.
7. The Plan 106 external checkpoint distinguishes "one working link for NetDB" from "normal-daemon transport qualification".
8. Older Milestone 3 planning text is explicitly archival under this authority.
9. The next executable implementation is unambiguously Plan 103.

## Handoff command

The implementation agent should begin with **Plan 103 only**. It should not implement Plan 104+ opportunistically during the same pass. Close and validate each child plan before moving to the next one.
