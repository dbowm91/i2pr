---
name: i2pr-local-dev
description: Work on the local product path of the i2pr Rust I2P router — Milestone 6 destinations/garlic/LeaseSet2/Streaming and Milestone 7 SAM 3.1. Use for i2pr-client, i2pr-api, destination-side i2pr-tunnel/i2pr-netdb/i2pr-proto/i2pr-crypto, i2pr-daemon SAM code, local trajectory tests, or Milestone 7 corrective execution. Plan 145 is the current SAM corrective authority; Plan 146 has closed as `passed-m7-sam31-private-destination-reference-requalification` and Plan 147 is the next executable. Plan 148 remains blocked on Plan 147.
---

# I2PR Local Development

Use this skill for the local product side of the router. The historical NTCP2 mixed-router development lane is closed and remains separate external acceptance debt.

## Current authority

Milestone 6 local product closure remains **Plan 134**:

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
```

Milestone 7 SAM is **not closed**. The current authority is:

- [`plans/145-status.md`](../../../plans/145-status.md)
- [`plans/145-m7-sam31-remaining-gap-corrective-roadmap.md`](../../../plans/145-m7-sam31-remaining-gap-corrective-roadmap.md)
- [`plans/146-status.md`](../../../plans/146-status.md) — closed
  as `passed-m7-sam31-private-destination-reference-requalification`.

Current execution sequence:

1. [`plans/147-m7-sam31-dedicated-raw-stream-driver-corrective.md`](../../../plans/147-m7-sam31-dedicated-raw-stream-driver-corrective.md) — **next executable**, blocked on Plan 146 (now closed).
2. [`plans/148-m7-sam31-independent-client-final-closure.md`](../../../plans/148-m7-sam31-independent-client-final-closure.md) — blocked on Plan 147.

Do not move to Milestone 8 until Plan 148 passes.

## Corrected Milestone 7 classification

Treat the landed work as follows:

```text
plan_137_loopback_session_lifecycle = passed

plan_142_base64 = passed
plan_146_private_destination_reference_requalification = passed
plan_146_relaxed_from_imported_invariant = passed

plan_143_local_delivery_seam = landed-and-retained
plan_143_full_raw_stream_acceptance = not-passed

plan_144_in_process_streaming_handshake = passed-local-evidence
plan_144_independent_client_closure = not-passed

sam_independent_clients = 0-passed
milestone7_local_product = not-closed
```

Historical Plan 136–144 files remain audit history. When they disagree with Plan 145+ status records, the newest explicit superseding status wins.

## What is already useful and should not be discarded

Retain unless a concrete defect requires change:

- Plan 137 bounded SAM listener/session lifecycle and loopback-only policy;
- Plan 142 I2P Base64 alphabet (`A-Z a-z 0-9 - ~`) and `=` padding behavior;
- Plan 146 bidirectional reference evidence against the pinned Java I2P
  2.12.0 (`2800040deee9bb376567b671ef2e9c34cf3e30b6`) and i2pd 2.60.0
  (`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`) references;
- `DestinationIdentity::from_imported` (Plan 146) — preserves the
  destination's embedded encryption public field verbatim; only checks
  `signing_public == EdDSA(signing_seed)`;
- strict SAM parser/line/resource ceilings;
- non-Clone/zeroizing/redacted secret-bearing SAM private-destination ownership;
- Plan 139 loopback-only FORWARD, ACCEPT/FORWARD exclusion, local naming policy;
- `StreamingManager` as the authoritative byte-stream implementation;
- `StreamingDestinationAdapter` as the Streaming/destination boundary;
- Plan 129 destination-routing / ECIES / Garlic / tunnel product topology;
- Plan 143 `i2pr_client::deliver` local-delivery seam and captured-outbound removal;
- Plan 144 canonical-vs-receiver StreamingManager routing fix and in-process SYN/SYN-response regression.

Do not rebuild these layers merely to satisfy the SAM corrective sequence.

## Known gaps that Plan 145 tracks

### Private destination — Plan 146 (closed)

Plan 142 fixed Base64 correctly but did not prove the `PRIV` binary
layout with actual bidirectional reference execution. Plan 146 closed
the gap: pinned Java I2P 2.12.0 (`2800040deee9bb376567b671ef2e9c34cf3e30b6`)
and i2pd 2.60.0 (`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`) reference
implementations produce the same canonical 455-byte / 608-char form
i2pr emits; both reference directions are exercised in
`crates/i2pr-daemon/tests/sam_plan146_reference.rs`.

Plan 146 also relaxed the reconstruction invariant: standard Java I2P
`PrivateKeyFile` and i2pd `IdentityEx` populate the destination
encryption public field with random bytes for destinations. The
`DestinationIdentity::from_imported` constructor preserves the
destination bytes verbatim and only enforces
`signing_public == EdDSA(signing_seed)`; a mismatch reports
`DestinationIdentityError::ImportSigningKeyMismatch`.



### Dedicated raw STREAM driver — Plan 147

The daemon currently still lacks the product behavior required after successful `STREAM CONNECT` / `STREAM ACCEPT`:

- CONNECT must wait for actual Streaming `Established`, not mark the SAM attachment established after SYN creation;
- production SAM cryptographic paths must not use deterministic seeded RNG;
- the accepted `TcpStream` must be transferred out of command parsing permanently;
- `LineReader` post-command bytes must be preserved as initial raw bytes;
- ACCEPT must complete the real inbound SYN/accept/SYN-response trajectory;
- raw TCP -> `StreamingManager::send_data` must be bounded;
- ordered delivered Streaming bytes -> raw TCP must be bounded;
- delayed ACK / retransmit / timeout advancement needs supervised runtime ownership;
- SILENT, loss/duplicate/reorder, backpressure, close/reset, sibling-stream and cancellation behavior need real-socket tests.

Internal `bridge_to_peer` tests are lower-level regressions, not a substitute for the dedicated TCP raw lane.

### Independent-client closure — Plan 148

Current selected candidates are `i2plib` and `libsam3`. Neither has yet moved application bytes through the real i2pr listener, so `sam_independent_clients = 0-passed`.

Plan 148 must prove cross-client raw bytes, re-run FORWARD/naming over the corrected path, and close resource/privacy/M6 regressions.

`txi2p` is optional; do not make its legacy `ometa` dependency a hard prerequisite.

## Architecture ownership

```text
i2pr-api
  bounded SAM parsing/version/replies/state/registries
  runtime-neutral

i2pr-client
  DestinationIdentity / registry
  ECIES destination session layer
  destination routing
  StreamingManager
  StreamingDestinationAdapter
  i2pr_client::deliver local product seam

i2pr-daemon
  Tokio listener/socket ownership
  deadlines/cancellation/supervision
  SAM session composition
  Plan 147 raw TCP socket driver
```

Forbidden closure shortcuts:

- a second SAM-specific streaming protocol;
- direct application-byte transfer between StreamingManagers;
- fabricated `Established` state;
- captured-outbound/test history as product acceptance;
- unbounded channel/queue/buffer;
- deterministic cryptographic RNG in production SAM paths;
- public-network or NTCP2/SSU2 dependency for Milestone 7.

## Environment contract

The remaining Milestone 7 sequence must work with:

```text
root/sudo             = no
Linux namespaces      = no
Docker                = no
VM/Multipass          = no
systemd               = no
public I2P network    = no
live NTCP2/SSU2       = no
localhost TCP         = yes
reference libraries   = yes
```

The Plan 129 authenticated-router-link-bypassed local seam remains allowed below the destination/tunnel stack.

## Where the local product lives

```text
crates/i2pr-client/
  src/identity.rs
  src/registry.rs
  src/session.rs
  src/routing.rs
  src/dispatch.rs
  src/streaming/
  src/streaming_adapter.rs
  tests/plan127_trajectory.rs
  tests/plan128_*.rs
  tests/plan129_trajectory.rs
  tests/plan130_trajectory.rs
  tests/plan131_trajectory.rs
  tests/plan132_trajectory.rs
  tests/plan133_trajectory.rs

crates/i2pr-api/src/sam/
  base64.rs
  private_destination.rs
  dest_generate.rs
  session_create.rs
  limits.rs
  registry.rs
  line_reader.rs
  server_state.rs
  streams.rs
  forward.rs
  naming.rs

crates/i2pr-daemon/src/sam.rs
crates/i2pr-daemon/src/sam/streams.rs
crates/i2pr-daemon/tests/sam_*.rs

tests/integration/sam/
  independent reference/client provenance and Plan 146/148 evidence
```

## Development commands

Routine pre-handoff floor:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Run additional static boundary scripts whenever their governed files change.

Useful focused seams:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_plan146_reference  -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_forward_naming
```

Plan 147 should add a dedicated raw-socket product test (recommended name `sam_stream_raw_product.rs`) and make it the canonical SAM application-byte lane.

Plan 148 must explicitly re-run focused Plan 127–134 regressions; aggregate workspace counts alone are not enough for final M7 closure.

## Coding rules most relevant to this work

- No `unsafe` in protocol/client/API/service crates.
- Treat every SAM/network byte as hostile: explicit bounds, no trailing garbage, typed errors.
- No `unbounded_channel`.
- Runtime/socket ownership belongs in daemon/runtime composition, not runtime-neutral crates.
- Use OS CSPRNG/reviewed production randomness; deterministic RNG is test-only.
- Secret-bearing values are non-Clone where practical, redacted in `Debug`, zeroized on drop.
- Do not log SAM `PRIV` values or raw application bytes.
- Do not weaken M6 ACK/send-window/receive-window behavior for SAM convenience.
- Add a regression test that reproduces every concrete defect corrected.

## Plan-specific handoff

### Executing Plan 146 (closed)

Read the closure record and the bidirectional test before changing
the private-destination seam:

- `plans/146-status.md`
- `plans/146-m7-sam31-private-destination-reference-requalification.md`
- `crates/i2pr-daemon/tests/sam_plan146_reference.rs`
- `tests/integration/sam/reference/Plan146ReferenceHelper.java`
- `crates/i2pr-api/src/sam/private_destination.rs`
- `crates/i2pr-client/src/identity.rs`

Plan 146 closed both reference directions. The
`DestinationIdentity::from_imported` constructor is the canonical
SAM import path; do not re-introduce
`encryption_public == X25519(static_secret)` enforcement (the
standard Java I2P `PrivateKeyFile` and i2pd `IdentityEx` layouts do
not enforce it for destinations).

### Executing Plan 147

Read the Plan 129–134 closure paths before changing Streaming semantics. The raw driver must extend existing flow control rather than adding large queues above it.

The command->raw transition must transfer socket ownership and any buffered post-command bytes; line parsing must become impossible after transition.

### Executing Plan 148

Use the real loopback listener and independent client APIs. External clients must not import i2pr internals. Commit evidence without private keys or raw secret material.

## When to load other skills

- NTCP2 / historical mixed-router work: `i2pr-ntcp2-interop`.
- Plan 046 rootless sealed namespace: `i2pr-rootless-sandbox`.
- Plan 048–051 Multipass recovery: `i2pr-multipass-recovery`.
- Architecture/ADR navigation: `i2pr-architecture`.

## Final safety / claim rules

- SAM remains disabled by default and loopback-only during this milestone.
- Do not claim SAM independent-client interoperability before Plan 148.
- Do not claim router-to-router interoperability from SAM localhost evidence.
- Do not bump support `advertised = true` without evidence satisfying `specs/CONFORMANCE.md`.
- Do not move to Milestone 8 until Plan 148 status explicitly closes Milestone 7.
