---
name: i2pr-local-dev
description: Work on the local product path of the i2pr Rust I2P router — Milestone 6 destinations/garlic/LeaseSet2/Streaming and Milestone 7 SAM 3.1. Plan 149 closed the self-composed localhost product; Plan 150 retains external-client core evidence; Plan 151 passed the final acceptance/evidence correction (with narrow Plan 152 M6 corrective); Plan 153 passed post-M7 hygiene; Milestone 8 roadmap registered via Plan 154; Plan 155 passed the SSU2 v2 protocol foundation; Plan 156 passed the SSU2 v2 handshake/token/RouterInfo establishment; Plan 157 passed the SSU2 v2 data-phase reliability/fragmentation.
---

# I2PR Local Development

Use this skill for the local product/SAM side of the router. Historical
mixed-router NTCP2 work remains separate acceptance debt.

## Current authority

Milestone 6 local product closure remains Plan 134:

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
```

Milestone 7 current authority:

```text
plan_146_private_destination_reference = passed
plan_147_raw_driver_implementation = retained
plan_149 = passed-self-composing-local-product
plan_150_external_core_evidence = retained-passed
plan_150_final_acceptance = superseded-by-plan151
plan_151 = passed-final-acceptance-evidence-correction
plan_152 = passed-narrow-m6-corrective
plan_153 = passed-post-m7-authority-and-ci-hygiene
sam_independent_clients = at-least-two-passed-via-plan150
milestone7_local_product = passed-via-plan149
milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed
plan_154 = registered-m8-ssu2-v2-roadmap
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation
plan_158 = passed-m8-ssu2-udp-runtime-and-local-session-product
next_executable_plan = 159
milestone8_planning_authority = plan154
milestone8_foundation = passed-via-plan155
milestone8_handshake = passed-via-plan156
milestone8_data_phase = passed-via-plan157
milestone8_udp_runtime = passed-via-plan158
next_product_layer = milestone8-ssu2-v2
```

Read in order:

1. `plans/158-status.md` (SSU2 v2 UDP runtime closure)
2. `plans/157-status.md` (SSU2 v2 data-phase closure)
3. `plans/156-status.md` (SSU2 v2 handshake/token/RouterInfo closure)
4. `plans/155-status.md` (SSU2 v2 foundation closure)
5. `plans/153-status.md` (passed hygiene pass)
6. `plans/152-status.md` (narrow M6 corrective closure)
7. `plans/151-status.md`
8. `plans/151-m7-sam31-final-acceptance-evidence-correction.md`
9. `plans/150-status.md`
10. `plans/149-status.md`
11. Plans 146–148 for retained/historical context.

Plan 153 has passed. Plan 154 is the registered Milestone 8 roadmap
authority; Plan 155 has passed the runtime-neutral SSU2 v2 protocol
foundation (addresses/headers/blocks), Plan 156 has passed the
runtime-neutral SSU2 v2 handshake/token/RouterInfo establishment, and
Plan 157 has passed the runtime-neutral SSU2 v2 data-phase
reliability/fragmentation (no UDP sockets, no runtime), and Plan 158
has passed the localhost-only SSU2 v2 UDP runtime and local session
product (`i2pr-runtime::Ssu2RuntimeService` plus the real-loopback
`ssu2_local` suite). Execute Plans 159–161 in order. SAM stays
experimental, loopback-only, disabled by default, and non-advertised.
SSU2 v2 claims no public advertisement or router-to-router
interoperability yet.

## Retain these working pieces

Do not rebuild them without a concrete defect:

- Plan 137 bounded loopback listener/session lifecycle;
- Plan 142 I2P Base64 correction;
- Plan 146 Java I2P/i2pd private-destination reference compatibility;
- `DestinationIdentity::from_imported` semantics;
- strict SAM parser/resource ceilings and secret hygiene;
- Plan 139 loopback-only FORWARD/NAMING implementation;
- `StreamingManager` and `StreamingDestinationAdapter` as the authoritative stream implementation;
- Plan 129 local destination/ECIES/Garlic/Streaming product path;
- Plan 147 owned raw `TcpStream` handoff, same-read preservation, actual `Established` wait, OS CSPRNG runtime path, byte pump, and supervised ACK/retransmit driver;
- Plan 149 transactional self-composed `SESSION CREATE`, one shared `Arc<DestinationIdentity>`, `SamLocalProductFabric`, local peer LeaseSet2 resolution, automatic destination driver, byte-exact SILENT/peer metadata, and typed delivery counters;
- Plan 150 external core evidence: pinned i2psam + qualified i2plib SAM surface, exact two-direction 2 MiB transfers, private destinations, SILENT, NAMING, negative matrix, and positive FORWARD.

## Why Plan 151 exists

Plan 150's implementation/external-client work is useful, but its final
acceptance ledger overclaimed several deferred cases. The clearest example is
an unconditional `multiple-stream-lifecycle = passed` row that refers to a
Plan 149 sibling-stream test that is not present.

Plan 149 explicitly deferred these items:

- slow-reader / slow-writer boundedness;
- DATA loss and retransmission;
- ACK loss;
- duplicate DATA;
- reordered DATA;
- authenticated/ciphertext corruption;
- retransmission ceiling;
- sibling-stream and broader CLOSE/RESET lifecycle acceptance.

Plan 151 makes those executable through the real listener and requires every
final `passed` row to derive from a command/test that actually ran.

## Plan 151 implementation shape

Prefer a new focused black-box acceptance suite such as:

```text
crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs
```

Keep behavior-driving interactions TCP/SAM-only after listener startup.
Read-only non-secret counters may be inspected for boundedness/resource release.

For deterministic faults, prefer a narrow **pre-start test configuration**
below the SAM socket boundary. Do not add production `i2pr-testkit`
dependencies and do not mutate private product state after listener startup.

Required executable areas:

- sibling streams: two simultaneous streams, close one, prove the other remains usable;
- slow reader and slow writer with explicit reservoir ceilings;
- DATA drop, ACK drop, duplicate, reorder, corruption, retransmit ceiling;
- graceful CLOSE, RESET/abrupt failure, control-session teardown, repeated lifecycle baselines;
- full FORWARD lifecycle/negative matrix;
- explicit focused Plan 127–134 regression commands;
- final Plan 150 external-client rerun and hosted workflow.

If one of these exposes a concrete M6 Streaming defect, stop and write a
narrow M6 corrective plan rather than weakening the Plan 151 expectation.

That stop fired once: Plan 152 (`plans/152-m6-session-streaming-robustness-corrective.md`)
corrects three proven M6 defects with no wire change — D1 unbounded receiver
retention (per-connection delivered-bytes cap + ACK snooze/NO_ACK gating),
D2 duplicate-never-re-ACKs (coalesced immediate ACK), D3 sender ECIES ratchet
retention (seal-side trim + absolute index ceiling). Fixes landed with
manager/ECIES unit tests; do not re-weaken them to make a SAM test convenient.

## Evidence-integrity rule

No required final row may be marked passed merely because another plan/status
says it passed. `tests/integration/sam/run-independent.sh` must derive each
required final row from an executed command/test.

Plan 151 added the static checker:

```text
scripts/check-sam-acceptance-evidence.sh
```

that rejects unconditional pass bookkeeping for required acceptance labels.
The checker is enforced in routine Linux CI and the manual SAM external
workflow; do not weaken it to make CI pass. Do not build a general
orchestration framework.

## External-client provenance

Retain exact pins:

```text
i2psam
  repo: https://github.com/i2p/i2psam
  pin: b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
  role: counted external client

i2plib
  repo: https://github.com/l-n-s/i2plib
  pin: 6edf51cd5d21cc745aa7e23cb98c582144884fa8
  role: counted qualified SAM-surface substitute

libsam3
  repo: https://github.com/i2p/libsam3
  pin: 7d6e658798baec31394c5685f9583343cc00900b
  role: built/probed, not counted because its public API rejects the compact 608-character Ed25519 PRIV
```

Do not patch or vendor external clients.

## Environment contract

```text
root/sudo             = no
Linux namespaces      = no
Docker                = no
VM/Multipass          = no
systemd               = no
public I2P network    = no
live NTCP2/SSU2       = no
localhost TCP         = yes
manual GitHub-hosted external-client lane = yes
```

## Development commands

Routine floor:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo deny check advisories bans sources
```

Focused SAM floor:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
```

Plan 151 resolved and ran the actual focused Plan 127–134 tests from the
current repository and listed them verbatim in its closure record
(retained evidence; do not re-broaden the matrix without a new plan).

Focused SSU2 floor (Plans 155–158):

```text
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
bash scripts/check-ssu2-vectors.sh
```

## Coding rules

- No new unbounded channels/queues.
- Runtime/socket ownership stays in daemon/runtime layers.
- The SSU2 central scheduler replaces a handshake's resend deadline
  with each new arm batch; never min-merge with a stale past value
  (Plan 158 burned the retry budget that way and died with
  `RetriesExhausted`).
- The SSU2 central scheduler replaces a handshake's resend deadline
  with each new arm batch; never min-merge with a stale past value
  (Plan 158 burned the retry budget that way and died with
  `RetriesExhausted`).
- OS CSPRNG for runtime material; deterministic randomness is test-only.
- Never log private destination material or raw payloads.
- No second private identity copy for SAM bridge ownership.
- Do not weaken M6 Streaming window/ACK/retransmit semantics for SAM tests.
- Every concrete defect exposed by Plan 151 gets a regression.

## Final claim rules

- SAM stays disabled by default, loopback-only, experimental, and non-advertised.
- Plan 150's external-client result does not imply router-to-router interoperability.
- Milestone 7 final localhost acceptance is closed via Plan 151; Plan 152 is the retained narrow M6 corrective underneath it.
- Plan 153 (docs/CI hygiene, no `crates/` or `Cargo.lock` changes) has passed.
- Plan 155 passed the Milestone 8 SSU2 v2 protocol foundation
  (runtime-neutral `i2pr-transport-ssu2` addresses/headers/blocks,
  no interop claim).
- Plan 156 passed the Milestone 8 SSU2 v2 handshake/token/RouterInfo
  establishment (Noise XK transcript, header protection,
  TokenRequest/Retry, bounded one-use tokens, RouterInfo binding,
  initiator/responder machines; still no UDP sockets, no data phase,
  no interop claim). PQ-hybrid v3/v4 stays deferred; SSU1 stays
  unsupported.
- Plan 157 passed the Milestone 8 SSU2 v2 data-phase
  reliability/fragmentation (authenticated `Ssu2Session` short-header
  packets with the corrected two-step data KDF, replay window, strict
  bounded ACK scheduling, fresh retransmission with RTT/RTO/congestion
  control, exact fragmentation/reassembly with duplicate suppression,
  termination/rekey/idle handling; still no UDP sockets, no runtime,
  no interop claim).

Current handoff: **execute Plans 158 → 161 in order under Plan 154**.