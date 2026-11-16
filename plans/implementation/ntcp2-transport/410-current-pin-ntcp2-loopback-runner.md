# Plan 410 — current-pin NTCP2 loopback runner for Plan 434

Status: **stopped** — `stopped-current-pin-stock-i2pd-receive-observation-unavailable`.
Closure: `plans/closure/ntcp2-transport/410-status.md`.

Subsystem: NTCP2 transport. Corrective successor to Plan 434's
`blocked-current-pinned-i2pd-reverse-initiator-runner-unavailable` record.

## Objective

Provide one bounded, strictly loopback-only successor execution path for the
current i2pd 2.61.0 pin that can exercise network ID 2 and initiate NTCP2 in
both directions with the existing non-production `i2pr-interop` runtime
launcher. This is harness infrastructure only; it does not enable NTCP2 in the
normal daemon or modify the historical Plan 038–100 lane.

## Why ready

- Plan 434 localized its first blocker to the launcher’s synthetic network-ID
  99 restriction and the absence of a current-pin reverse initiator.
- Plan 430 and Plan 432 are closed. Plan 101's normal-daemon NTCP2 prohibition
  remains in force.
- The exact pinned i2pd 2.61.0 binary and source revision are present in the
  local cache at `635b013a612ff47278ef02acf8580a28e10e26c5`.
- `tools/i2pr-interop` already owns a bounded non-production prepare/listen/
  dial interface and emits typed terminal stages.

## Invariants

1. Only stock i2pd `635b013a612ff47278ef02acf8580a28e10e26c5` may be used.
2. Every local and peer endpoint binds or dials `127.0.0.1`; no DNS, public
   egress, reseed, namespace, container, VM, or daemon listener is permitted.
3. Network ID 2 is accepted only by an explicit current-pin loopback topology
   selector. Existing synthetic network-ID-99 scenarios and their behavior
   remain unchanged.
4. The path uses the non-production launcher and must never activate
   `i2pr-daemon` or advertise NTCP2.
5. Freeze `MAX_ATTEMPTS=1` per direction before any wire attempt. Each attempt
   has a unique state/evidence directory and bounded handshake/data deadlines.
6. Evidence contains only pin, stage enum, direction, bounded message counts,
   result code, and file digests. Never retain identity keys, RouterInfo bytes,
   raw logs, packet captures, or payload bytes.
7. Do not edit or execute the frozen Plan 038–100 host/namespace lane as a
   successor runner. Do not weaken its checker.

## Scope

In scope: a topology-tagged network-ID-2 path through the current launcher;
RouterInfo construction and validation for that topology; one current-pin
i2pd initiator driver or equivalent bounded stock operation; stage and I2NP
correlation; runner preflight, cleanup, sanitized evidence, and a checker with
negative controls.

Out of scope: production daemon activation, RouterInfo advertisement, SSU2,
Java qualification, public-network traffic, changes to NTCP2 cryptographic
state machines without an observed divergence, and rewriting Plan 434 history.

## Ordered work packages

1. Verify exact i2pd source/binary revision and inspect its public NTCP2
   listener/initiator entry points at the pin. Confirm the output artifact is
   buildable without changing reference source.
2. Add the explicit network-ID-2 loopback topology path. Enforce the allowed
   `(topology, network_id, address)` combinations in both scenario parsing and
   `prepare`; thread the selected network ID into the signed RouterInfo and
   handshake state. Add unit tests proving ID 99 behavior is preserved and
   ID 2 is rejected under every other topology/address.
3. Build a minimal current-pin i2pd initiator/control surface using only the
   pinned public API. Correlate `tcp_connected`, `noise_authenticated`,
   authenticated-link handoff, frame RX/TX, I2NP receive, and terminal result
   using a local opaque attempt ID.
4. Add a fail-closed runner that verifies the source pin, reserves unique
   loopback ports safely, sets a single attempt per direction, uses fresh
   state, and always joins/stops both processes. Preserve sanitized failure
   evidence on every terminal path.
5. Run one forward and one reverse current-pin attempt. Require authenticated
   NTCP2 and one valid DeliveryStatus I2NP in each direction. If the first
   divergence is protocol-owned, stop and register a narrowly scoped owner
   correction before changing behavior. If the reference API cannot initiate
   the reverse direction without prohibited patching, record that exact stop.
6. Run targeted launcher and evidence-checker tests, the NTCP2 vector and
   boundary checks, then the repository routine floor if and only if a
   production source file changed.

## Verification

- `cargo test --locked -p i2pr-interop --all-targets`
- a new runner `--self-test` that starts no reference process
- checker self-test plus mutation table
- `bash scripts/check-ntcp2-vectors.sh`
- `bash scripts/check-ntcp2-interoperability.sh`
- exact pinned forward and reverse loopback runs, each with `MAX_ATTEMPTS=1`
- `cargo fmt --all --check`, `cargo check --locked --workspace --all-targets`,
  and `cargo test --locked --workspace --all-targets -- --test-threads=1`
  when production source changes; routine floor for any production correction

## Acceptance

Plan 410 passes only when the explicit network-ID-2 loopback profile is
negative-tested, the frozen network-ID-99 path remains green, the exact-pinned
i2pd runner emits sanitized evidence for both authenticated directions and
correlated I2NP delivery, and all owned processes/resources are cleaned up.
Passing Plan 410 unblocks a fresh Plan 434 qualification attempt; it does not
itself close Plan 434 or change normal-daemon NTCP2 status.

## Stop conditions

Stop without production changes if the current pinned reference requires a
source patch, if the reverse role cannot be initiated through a stock API, if
network-ID-2 RouterInfo validation fails before transport, or if bounded stage
evidence cannot distinguish protocol rejection from missing observation.

## Closure evidence

Record exact pin and source/binary hashes, file ownership, topology/profile
allowlist, unit and mutation rows, each bounded live attempt and its sanitized
digest, cleanup/resource outcomes, exact commands, security/compatibility
review, and the Plan 434/435 unblock audit. No capability or support claim is
permitted by this plan.
