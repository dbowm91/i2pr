---
name: i2pr-local-dev
description: Work on the local product path of the i2pr Rust I2P router — Milestone 6 destinations/garlic/LeaseSet2/Streaming and Milestone 7 SAM 3.1. Use for i2pr-client, i2pr-api, destination-side i2pr-tunnel/i2pr-netdb/i2pr-proto/i2pr-crypto, i2pr-daemon SAM code, local trajectory tests, or Milestone 7 corrective execution. Plan 145 is the corrective umbrella; Plan 149 is the next executable plan; Plan 150 is blocked on Plan 149.
---

# I2PR Local Development

Use this skill for the local product side of the router. The historical NTCP2 mixed-router development lane is separate external acceptance debt.

## Current authority

Milestone 6 local product closure remains Plan 134:

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
```

Milestone 7 is **not closed**. Read, in order:

1. `plans/145-status.md` — corrective umbrella;
2. `plans/149-status.md` — current executable classification;
3. `plans/149-m7-sam31-self-composing-local-product-corrective.md` — **execute this plan only**;
4. `plans/146-status.md` — closed private-destination reference result;
5. `plans/147-status.md` — retained raw-driver implementation evidence;
6. `plans/148-status.md` — blocked historical audit;
7. `plans/150-m7-sam31-external-client-reproducible-final-closure.md` — blocked until Plan 149 passes.

Current classification:

```text
plan_146_private_destination_reference = passed
plan_147_raw_driver_implementation = retained
plan_147_full_original_acceptance = superseded-by-plan149
plan_148 = blocked-audit-superseded-by-plan149-150
plan_149 = active-next-executable
plan_150 = blocked-on-plan149
sam_independent_clients = 0-passed
milestone7_local_product = not-closed
```

Do not move to Milestone 8 until Plan 150 explicitly closes Milestone 7.

## Retain these closed/useful pieces

Do not rebuild them without a concrete defect:

- Plan 137 bounded loopback SAM listener/session lifecycle;
- Plan 142 I2P Base64 alphabet (`A-Z a-z 0-9 - ~`) and `=` padding;
- Plan 146 Java I2P/i2pd bidirectional private-destination evidence;
- `DestinationIdentity::from_imported` relaxed encryption-public reconstruction invariant;
- strict SAM parser/resource ceilings;
- secret redaction/zeroization/non-Clone ownership;
- Plan 139 loopback-only FORWARD and local NAMING policy;
- `StreamingManager` + `StreamingDestinationAdapter` as authoritative stream implementation;
- Plan 129 destination-routing / ECIES / Garlic / tunnel product topology;
- Plan 143 local-delivery seam;
- Plan 144 canonical-vs-receiver StreamingManager routing fix;
- Plan 147 owned raw `TcpStream` handoff, same-read raw-byte preservation, actual `Established` wait, OS CSPRNG, TCP↔Streaming pump, and supervised ACK/retransmit driver.

## Why Plan 149 exists

The Plan 147 canonical raw STREAM test is not black-box product composition. After SAM `SESSION CREATE`, it manually:

```text
builds/installs SamDestinationBridge
installs peer LeaseSet2 routing
installs deterministic inbound-tunnel factory
spawns the per-destination runtime driver
```

The production `execute_session_create()` path does not currently self-compose those prerequisites. A real external SAM client cannot call those private Rust APIs.

Plan 149 must make successful `SESSION CREATE` mean that the supported localhost STREAM product is ready before `SESSION STATUS RESULT=OK` is sent.

Plan 149 also owns the Plan 147 acceptance items that were incorrectly deferred:

- byte-exact CONNECT/ACCEPT `SILENT=true/false`;
- authenticated non-silent ACCEPT peer Destination metadata;
- multi-megabyte bounded transfer;
- slow-reader/slow-writer bounds;
- loss/duplicate/reorder/ACK-drop/corruption/retransmit-ceiling tests;
- CLOSE/RESET/control-session teardown;
- sibling-stream isolation.

The current raw-transition code writes success even for `SILENT=true`; treat that as a concrete Plan 149 defect.

## Plan 149 ownership rule

Do not solve SAM self-composition by making `DestinationIdentity: Clone` or reconstructing a second private identity.

Maintain:

```text
one logical destination
 -> one private identity allocation / ownership graph
```

If the destination runtime and SAM bridge both need long-lived access, share one allocation through a narrow capability/`Arc` or move the bridge operations under the canonical runtime. Do not expose raw private bytes through public handles.

## Local product seam

Milestone 7 is localhost product evidence, not live tunnel/router interoperability.

The authenticated-router-link-bypassed Plan 129 seam remains allowed, but Plan 149 must install its local delivery capability automatically under daemon ownership. Runtime-created ephemeral material uses OS CSPRNG; deterministic tunnel factories remain test-only.

Missing peer/LeaseSet/tunnel capability must surface a typed bounded failure, not silently drop queued Streaming requests.

## Canonical Plan 149 test

Plan 149 should add a self-composed black-box SAM test, e.g.:

```text
crates/i2pr-daemon/tests/sam_stream_self_composed.rs
```

Allowed after service startup:

```text
TCP HELLO
TCP SESSION CREATE
TCP STREAM ACCEPT/CONNECT
raw bytes
non-secret final counters after shutdown
```

Forbidden after service startup:

```text
build_sam_destination_bridge
SamDestinations::install
install_remote_lease_set2
install_inbound_tunnel_factory
spawn_destination_driver
bridge_to_peer
send_data_segment
deliver_outbound
```

The old `sam_stream_raw_product` test stays as a lower-level implementation regression, not final composition evidence.

## Plan 150 external-client guidance

Do not execute until Plan 149 passes.

Preferred mandatory clients:

```text
libsam3
  repo: https://github.com/i2p/libsam3
  pin: 7d6e658798baec31394c5685f9583343cc00900b
  language: C

i2psam
  repo: https://github.com/i2p/i2psam
  pin: b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
  language: C++
```

The old Plan 148 libsam3 `e0da... / v1.0.0` pin is invalid and must not be restored.

Supplementary:

```text
i2plib
  pin: 6edf51cd5d21cc745aa7e23cb98c582144884fa8
  version: 0.0.14
```

Its 2019 high-level asyncio API uses the removed `loop=` argument, so do not make it mandatory on modern Python unless a compatible runtime is explicitly qualified. Do not patch external client source for i2pr.

Plan 150 may add a manual GitHub-hosted Ubuntu `workflow_dispatch` lane to fetch/build exact external revisions and run localhost SAM. No root/sudo/namespaces/Docker/VM/systemd/public I2P is allowed or needed.

## Architecture ownership

```text
i2pr-api
  bounded SAM parsing/version/replies/state/registries
  runtime-neutral

i2pr-client
  canonical destination identity/runtime/routing/Streaming
  Plan 129 local delivery seam

i2pr-daemon
  Tokio listener/socket ownership
  session composition
  local SAM product fabric
  raw TCP socket owner
  destination driver supervision
```

Forbidden shortcuts:

- a second SAM-specific streaming protocol;
- direct application-byte transfer between StreamingManagers;
- fabricated `Established` state;
- hidden private post-SESSION-CREATE setup in final acceptance;
- unbounded queues/buffers;
- deterministic runtime cryptographic randomness;
- public-network/NTCP2/SSU2 dependency for M7.

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
reference libraries   = yes
manual GitHub-hosted external-client lane = yes (Plan 150)
```

## Development commands

Routine floor:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Focused SAM seams:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product
cargo test --locked -p i2pr-daemon --test sam_forward_naming
```

Plan 149 adds the self-composed black-box lane and explicit fault/backpressure/lifecycle coverage. Plan 150 must rerun focused Plan 127–134 M6 regressions; aggregate workspace counts alone are insufficient.

## Coding rules

- No `unsafe` in protocol/client/API/service crates.
- Treat every SAM/network byte as hostile; explicit bounds/typed errors.
- No `unbounded_channel`.
- Runtime/socket ownership stays in daemon/runtime composition.
- OS CSPRNG in runtime paths; deterministic RNG test-only.
- Secret values redacted/zeroized and not casually Clone-able.
- Never log SAM `PRIV` or raw application payloads.
- Do not weaken M6 ACK/send/receive-window behavior for SAM convenience.
- Add a regression test for every concrete defect corrected.

## When to load other skills

- Historical NTCP2/mixed-router work: `i2pr-ntcp2-interop`.
- Plan 046 rootless namespace: `i2pr-rootless-sandbox`.
- Plan 048–051 Multipass recovery: `i2pr-multipass-recovery`.
- Architecture/ADR navigation: `i2pr-architecture`.

## Final claim rules

- SAM remains disabled by default and loopback-only during Milestone 7.
- Do not claim independent-client interoperability before Plan 150.
- Do not claim router-to-router interoperability from localhost SAM evidence.
- Do not bump `advertised = true` without `specs/CONFORMANCE.md` evidence.
- Do not move to Milestone 8 until Plan 150 status explicitly closes Milestone 7.

Current handoff: **execute Plan 149 only**.
