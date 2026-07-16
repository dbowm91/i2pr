# Aggregate Milestone 3 closure: NTCP2 and transport-neutral link management

Date: 2026-07-15

Status: **blocked; implementation phases complete for their bounded local
scope, milestone acceptance criteria not met**.

This aggregate record preserves the evidence boundary from Plans 031–036. It
does not convert local vectors, self-handshakes, loopback TCP, or deterministic
simulation into mixed-router support.

## Implementation sequence and changed surfaces

The implementation sequence is recorded by the prior closure records:

1. Plan 031: runtime-neutral transport contracts and crate boundaries;
2. Plan 032: NTCP2 cryptographic composition, transcript, and vectors;
3. Plan 033: bounded handshake codecs and consuming initiator/responder states;
4. Plan 034: authenticated data frames, blocks, counters, and partial-I/O
   testkit ownership;
5. Plan 035: runtime-owned sockets, admission, replay, backoff, duplicate
   policy seams, and joined link children; and
6. Plan 036: controlled interoperability manifest, evidence preflight, local
   adversarial campaign, and closure records.

The current change is confined to validation tests, scripts, CI checks, and
documentation. It adds no production dependency and no public API.

## Final crate/dependency graph

```text
i2pr-proto <- i2pr-crypto <- i2pr-storage
      ^             ^
      |             +-------------------+
i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon
      ^             ^       ^
      +-------------+-------+---- i2pr-transport-ntcp2

i2pr-testkit -> test/simulation evidence only; no production crate depends on it
```

`i2pr-runtime` remains the only production owner of Tokio tasks, sockets,
timers, channels, cancellation, and joined child lifecycles.

## Public API and ownership inventory

| Area | Public surface/evidence | Owner and limitation |
| --- | --- | --- |
| Transport contracts | `i2pr-transport` link IDs, delivery, admission, lifecycle, duplicate, observation, and snapshot types | runtime-neutral; no sockets or payload logging |
| NTCP2 protocol | address, crypto, handshake/state, frame, and block modules in `i2pr-transport-ntcp2` | consuming owners; Tokio/filesystem/NetDB-free |
| Runtime | `Ntcp2RuntimeService`, listener/dial, admission, replay/backoff, exact I/O, `LinkHandle` | runtime owns all async resources; controlled local sockets only |
| Testkit | manual clock/scheduler, deterministic seeds, stream driver, redacted replay records | test-only; no public-network path |
| Daemon | explicit identity/config commands; live `run` remains disabled | no RouterInfo publication or capability advertisement |

### Cryptographic dependency and secret-owner table

| Material | Dependency/owner | Lifetime rule |
| --- | --- | --- |
| Ed25519/X25519 identity material | `i2pr-crypto`, `i2pr-storage` | injected/test or OS randomness; zeroizing private owners; atomic create-only storage |
| NTCP2 static X25519 key and IV | versioned `TransportStaticKeyStore` | separate from router identity; strict bounded record; never derived from identity |
| Transcript/shared/cipher/split state | `i2pr-transport-ntcp2` | consuming transitions; no generic provider; terminal on failure |
| Directional data state | NTCP2 transmit/receive owners | separate key/length owners; counters advance once; forbidden nonce never emitted |
| Runtime sockets/tasks/queues/replay | `i2pr-runtime` | bounded, cancellable, joined/drained; no raw values in default diagnostics |

## Protocol constants and policy bounds

| Surface | Final bounded policy |
| --- | --- |
| Session request/created padding | 64..=65,535 bytes; local maxima 880/848 clear padding |
| Session confirmed | 48-byte encrypted static part; bounded part two; total <= 65,535 |
| Clock/replay | ±60 seconds local compatibility policy; replay retention 120 seconds; fail closed |
| NTCP2 frame | clear ciphertext 16..=65,535; plaintext <= 65,519; AEAD before blocks |
| Blocks | bounded block count/unknown bytes; strict singleton/order/trailing rules; bounded RouterInfo/I2NP/options/padding |
| Counters | terminal at exhaustion; no periodic rekey invented; fresh handshake required |
| Admission | global, exact-IP, IPv4 `/24`, IPv6 `/64`; bounded active links/queues/bytes |
| Runtime | bounded connect/handshake/read/write/queue/drain deadlines; owned reader/writer joins |
| Integration | synthetic private network, loopback/private namespace, disabled reseed/bootstrap, disposable secrets |

## State and service diagrams

```text
initiator: RouterInfo -> timestamp -> padding -> SessionRequest
         -> SessionCreated -> replay/timestamp -> SessionConfirmed -> authenticated

responder: SessionRequest -> replay/timestamp/padding -> SessionCreated
         -> SessionConfirmed -> authenticated

runtime: listener/dial -> admission -> owned link reader + writer
       -> bounded queue/replay/duplicate policy -> close -> join/drain
```

The pure state machines emit bounded typed actions. The runtime adapter owns
the effects. Plan 036's required composition and mixed-router execution remain
the blocker.

## Interoperability and adversarial matrix

The required targets are pinned in
`tests/integration/ntcp2/manifest.toml`: Java I2P 2.12.0 (`2800040`) and i2pd
2.60.0 (`f618e41`). Each must be run as i2pr initiator and reference initiator
in IPv4 and supported IPv6, followed by authenticated I2NP exchange,
padding/skew/replay/identity/network failures, malformed/slow/resource cases,
and duplicate-link races. No such run was available for this closure. The
matrix is therefore **blocked**, not passed or skipped-success.

Local evidence covers fixed parser/state mutation, tag/length/block/order
rejection, deterministic replay/skew/admission/backoff/duplicate policy,
loopback child joining, bounded partial I/O, and the committed testkit matrix
for seeds `0..=255`. It does not prove reference-router behavior.

## Support metadata and operator boundary

Every NTCP2 surface remains `experimental` and `advertised = false` in
`specs/support.toml`, with matching wording in `docs/protocol-support.md`.
The operator decision is to keep live daemon activation disabled. No reseed,
NetDB mutation, RouterInfo publication, tunnel behavior, SSU2, client API, or
public-network operation was introduced.

### Plan 046 closure on this checkout

The Plan 046 rootless sealed-namespace lane is closed with a typed
host-level blocker. The host's kernel activates
`kernel.apparmor_restrict_unprivileged_userns=1`, which confines every
unprivileged user namespace to a restrictive AppArmor policy and
prevents `unshare -U -r --map-root-user` from writing `/proc/self/uid_map`.
The ordinary invoking user has no `CAP_MAC_ADMIN` and no other lever to
lift that policy, and Plan 046 forbids `sudo`. The probe writes the
canonical typed blocker `blocked_unprivileged_user_namespace` to the
attestation path; the on-host evidence directory
`target/interop/evidence/handshake-smoke-rootless--host-blocked/`
carries that blocker plus a kernel/sysctl/capability snapshot and the
identical `ssh i2ptest@localhost` result. Plan 046 does not advance
Milestone 3 and does not advertise NTCP2 support. Cross-host recovery is
recorded in `plans/047-cross-host-rootless-lane-expansion.md`.

## Exact validation evidence

The final handoff ran the commands below. All listed local commands passed;
the only cargo-deny output is the existing duplicate `rand_core` warning. The
unavailable external lane is recorded explicitly rather than represented as a
pass.

```text
cargo fmt --all --check
cargo check --workspace
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p i2pr-runtime --all-targets
cargo test -p i2pr-transport --all-targets
cargo test -p i2pr-transport-ntcp2 --all-targets
cargo test -p i2pr-testkit --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
cargo deny check advisories bans sources
cargo +1.85.0 check --workspace --all-targets
cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets
CARGO_NET_OFFLINE=true bash scripts/fuzz-smoke.sh
git diff --check
```

Local result summary: workspace check passed; workspace tests passed with 200
tests across 26 suites; focused runtime/transport/NTCP2/testkit lanes passed
with 36/17/32/25 tests respectively; Clippy and rustdoc passed; dependency,
runtime-boundary, fixture, vector, and interoperability preflight checks
passed; cargo-deny advisories/bans/sources passed with the existing duplicate
`rand_core` warning; Rust 1.85 all-target check and nightly fuzz-workspace
check passed. Fuzz smoke completed 22 targets × 32 runs, and the critical
handshake/blocks/frames/transcript campaigns completed 1,000 runs each at
fixed seed `36` after disabling LeakSanitizer for the managed ptrace runner.

The authorized Java I2P/i2pd commands, duplicate-race repetitions against
those references, sanitized artifact hashes, and external run IDs are
**absent** because the required driver/testnet was unavailable. The exact
local results are the handoff evidence for this blocked closure.

## Milestone 4 gate

Milestone 4 is **not ready**. It may be planned only after the Plan 036
blocker is resolved with reproducible authenticated NTCP2 links to both
required implementations in both directions, bounded I2NP exchange,
duplicate-link stability, adversarial/resource cleanup, sanitized evidence,
and exact CI/manual run identifiers. Transport observations must remain inputs
to any later NetDB/publication policy rather than bypassing the boundary.
