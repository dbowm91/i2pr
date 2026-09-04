# Repository Guidelines

`i2pr` is an experimental Rust I2P router. **Not production-ready.** Do not
use it for anonymity, privacy, censorship resistance, or any security-sensitive
workload. NTCP2 remains experimental and non-advertised; the production daemon
does not activate NTCP2. SSU2 v2 has a localhost-only UDP runtime (Plan 158);
no public advertisement, publication, or router-to-router interoperability is
claimed yet.

## Read first

Always read these before changing code or answering questions about state:

1. `README.md` — current product/plan status.
2. `GUARDRAILS.md` — non-negotiable engineering/security/interoperability constraints.
3. `CONTRIBUTING.md` — local quality/runtime/test conventions.
4. [`plans/README.md`](plans/README.md) and the newest relevant status files.
5. `specs/support.toml` plus `specs/CONFORMANCE.md` for support/evidence claims.

Current Milestone 7 authority:

```text
Plan 146 = passed private-destination reference evidence
Plan 147 = raw-driver implementation retained
Plan 149 = passed self-composing localhost SAM product
Plan 150 = external-client core evidence retained-passed
Plan 150 final acceptance = superseded-by-plan151
Plan 151 = passed final acceptance evidence correction
Plan 152 = passed narrow M6 streaming corrective
Plan 153 = passed post-M7 authority/CI hygiene
Milestone 7 SAM localhost = closed (experimental, loopback-only)
Milestone 8 roadmap = Plan 154 registered, Plan 153 passed
Plan 155 = passed SSU2 v2 protocol foundation (no handshake/sockets)
Plan 156 = passed SSU2 v2 handshake/token/RouterInfo establishment (no sockets)
Plan 157 = passed SSU2 v2 data-phase reliability/fragmentation (no sockets)
Plan 158 = passed SSU2 v2 UDP runtime and local session product (localhost only)
next executable plan = 159
next product layer = milestone8-ssu2-v2
```

Read in this order for SAM work:

1. [`plans/151-status.md`](plans/151-status.md)
2. [`plans/151-m7-sam31-final-acceptance-evidence-correction.md`](plans/151-m7-sam31-final-acceptance-evidence-correction.md)
3. [`plans/150-status.md`](plans/150-status.md) — retained external-client core evidence, not final closure
4. [`plans/149-status.md`](plans/149-status.md) — passed product-composition authority
5. Plans 146–148 for historical/reference context.

Do **not** trust prose that disagrees with executable tests/scripts. The newest
explicit superseding status wins when historical records conflict.

## Workspace layout

- `i2pr-proto` — bounded wire codecs, typed errors, no I/O.
- `i2pr-crypto` — protocol cryptographic wrappers.
- `i2pr-storage` — identity/key persistence.
- `i2pr-core` — shared runtime-neutral contracts.
- `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2` — runtime-neutral transport/link codecs.
- `i2pr-netdb`, `i2pr-netdb-persist` — RouterInfo/LeaseSet2 validation and local storage.
- `i2pr-runtime` — production owner of Tokio, sockets, timers, channels, cancellation.
- `i2pr-daemon` — CLI/composition root and SAM runtime/socket ownership.
- `i2pr-tunnel` — runtime-neutral exploratory/tunnel substrate.
- `i2pr-client` — destination lifecycle, LeaseSet2, ECIES session/routing, Streaming.
- `i2pr-api` — runtime-neutral SAM 3.1 parsing/state/registry/FORWARD/NAMING.
- `i2pr-testkit` — deterministic simulation/fault fixtures; no production crate may depend on it.
- `tools/i2pr-interop` — non-production test launcher.

Architecture details live under `docs/architecture/`; ADRs live under
`docs/adr/`.

## Current SAM architecture

Retain the working Plan 149 product structure unless Plan 151 exposes a
concrete defect:

- `SESSION CREATE` transactionally builds the supported localhost product;
- one `Arc<DestinationIdentity>` allocation is shared by destination runtime and SAM bridge;
- `SamLocalProductFabric` creates the localhost LeaseSet2/outbound/inbound-delivery material with OS CSPRNG;
- local peer LeaseSet2 is resolved/validated through the SAM-owned directory;
- one supervised per-destination runtime driver is started automatically;
- raw CONNECT/ACCEPT permanently transfers socket ownership out of the line parser;
- same-read command+raw bytes are preserved;
- `SILENT` behavior and non-silent ACCEPT peer Destination metadata are byte-exact;
- `DeliverySweepCounters` surface typed bounded delivery failure accounting.

The canonical product-composition test is
`crates/i2pr-daemon/tests/sam_stream_self_composed.rs`. After listener startup,
it drives behavior only through TCP/SAM and must not invoke private bridge,
LeaseSet2, tunnel-factory, driver, delivery, or byte-moving setup APIs.

## Plan 151 scope (retained)

Plan 151 was an acceptance/evidence correction, not a SAM rewrite. It added
executable proof for the items Plan 150 claimed but did not fully run:

- no synthetic/unconditional `passed` evidence rows;
- two simultaneous sibling streams and close-one/keep-one isolation;
- slow-reader and slow-writer boundedness;
- deterministic DATA-drop, ACK-drop, duplicate, reorder, corruption, and retransmission-ceiling behavior beneath real SAM sockets;
- CLOSE/RESET/control-session cleanup;
- complete FORWARD lifecycle/negative matrix;
- explicit focused Plan 127–134 regression commands;
- current-head routine CI plus manual external-client workflow.

If a new test exposes an M6 Streaming protocol defect, stop Plan 151 and write a
narrow protocol corrective plan rather than weakening the test. That stop
fired once as Plan 152 (passed narrow M6 corrective, no wire change).

## Plan 153 scope (closed)

Plan 153 was documentation and CI hygiene only: it normalized the
authoritative `plans/152-status.md`, removed stale Plan 151/152 prose,
added the Plan 152 closure pointer to the support ledger, and enforced
the Plan 151 SAM evidence-integrity checker in routine Linux CI and
the manual SAM external workflow. No `crates/` or `Cargo.lock` changes
were made.

## Hard boundaries

These remain non-negotiable and are CI-enforced where applicable:

- preserve workspace dependency direction; no production crate depends on `i2pr-testkit`;
- no unbounded channels/queues introduced for SAM convenience;
- Tokio/socket/timer/task ownership stays in runtime/daemon layers;
- every spawned task has explicit ownership/cancellation;
- SAM remains loopback-only and disabled by default;
- no root/sudo, privileged container, network namespace, VM, systemd, or public-I2P requirement for Plan 151;
- no production NTCP2/SSU2 activation for SAM evidence;
- no external-client patching or vendoring;
- no private SAM `PRIV`, signing seed, static secret, or raw application payload in logs/evidence;
- do not make `DestinationIdentity: Clone` or reconstruct a second private identity for the bridge.

Static boundary scripts are the source of truth. Fix violations; do not weaken
the scripts.

## Build/test floor

Run from repository root before handoff:

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

Focused SAM seams currently include:

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

Plan 151 added its narrowly named final-acceptance suite
(`crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs`) rather than bloating
the existing self-composed file. The Plan 151 evidence-integrity checker
(`scripts/check-sam-acceptance-evidence.sh`) is enforced in routine Linux CI
and the manual SAM external workflow; do not weaken it to make CI pass.

Focused SSU2 seams currently include:

```text
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
bash scripts/check-ssu2-vectors.sh
```

Plan 155 added the SSU2 fixture corpus (`tests/fixtures/ssu2/`) and its
checker (`scripts/check-ssu2-vectors.sh`), enforced in routine Linux CI;
do not weaken it to make CI pass.

## Testing conventions

- Prefer paused Tokio/manual clocks for deterministic runtime tests where compatible with socket behavior.
- Runtime-owned socket tests use loopback only; no DNS/public traffic.
- Use explicit bounded deadlines, not indefinite waits.
- Queue/resource tests cover exact capacity/max+1 and verify release after failure/closure.
- Secret-bearing values stay redacted/zeroized/non-Clone where practical.
- Channel/socket closure is a lifecycle event, not blindly retried.
- A required evidence result must be derived from an executed command/test. Never mark an acceptance row passed merely because a historical plan says it passed.

## External SAM evidence

Provenance retained from Plan 150:

```text
Java I2P Plan146 reference:
  2800040deee9bb376567b671ef2e9c34cf3e30b6

i2pd Plan146 reference:
  f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e

i2psam counted Plan150 client:
  b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac

i2plib counted substitute SAM surface:
  6edf51cd5d21cc745aa7e23cb98c582144884fa8

libsam3 built/probed but not counted:
  7d6e658798baec31394c5685f9583343cc00900b
```

The manual `.github/workflows/sam-external.yml` lane is unprivileged and
localhost-only. Plan 151 reran it on the exact closing head after all new
acceptance tests were integrated; Plan 153 reruns routine CI plus the manual
SAM external workflow on its exact closing head to prove the newly enforced
checker composes with the existing lane.

## Coding conventions

- No unsafe in protocol/client/API/service crates unless separately reviewed.
- Treat all SAM/network bytes as hostile and bounded.
- Use typed errors; do not swallow codec/protocol results.
- Runtime cryptography/local-product ephemeral material uses OS CSPRNG.
- New dependencies require explicit review.
- Avoid global mutable state/service locators; pass narrow capabilities.
- Do not modify M6 wire semantics to make a SAM test convenient.

## Protocol claims

- Milestone 6 local product is closed via Plan 134; router interoperability is not claimed.
- Plan 149 closed the self-composing localhost SAM product.
- Plan 150 retains at-least-two independent-client core evidence, but its final acceptance label is superseded.
- Plan 151 is the current final Milestone 7 acceptance authority.
- Plan 152 is the passed narrow M6 robustness corrective retained underneath Plan 151.
- Plan 153 is the passed docs/CI hygiene pass; Milestone 8 implementation begins at Plan 155.
- Plan 155 is the passed SSU2 v2 protocol foundation (runtime-neutral addresses/headers/blocks; no handshake, no sockets, no interop claim).
- Plan 156 is the passed SSU2 v2 establishment protocol (Noise XK transcript, header protection, TokenRequest/Retry, bounded one-use tokens, RouterInfo binding, initiator/responder machines; still no sockets, no data phase, no interop claim).
- Plan 157 is the passed SSU2 v2 data phase (authenticated short-header packets with the corrected two-step data KDF, packet-number/replay window, strict bounded ACK scheduling, fresh retransmission with conservative RTT/RTO/congestion control, exact I2NP fragmentation/reassembly with duplicate suppression, termination/rekey/idle handling; still no sockets, no runtime, no interop claim).
- Plan 158 is the passed SSU2 v2 UDP runtime and local session product (localhost only).
- Milestone 8 roadmap is registered via Plan 154; Plan 159 is the next executable plan after Plan 158 closure.
- SAM stays experimental, loopback-only, disabled by default, and non-advertised.
- No localhost SAM evidence implies router-to-router NTCP2/SSU2 or public I2P interoperability.
- Do not advance `advertised = true` without `specs/CONFORMANCE.md` evidence.

## OpenCode skills

Use `i2pr-local-dev` for Plan 151/SAM/local-product work and
`i2pr-architecture` for architecture/ADR/plan navigation. Historical NTCP2,
rootless, and Multipass skills remain separate lanes.

## Commits and handoff

Use focused commits. Do not change git config, skip hooks, force-push, or amend
someone else's commit. Closure records must include exact commands/results and
current-head workflow evidence.

Current handoff: **Plan 151 passed, Plan 152 passed, Plan 153 passed,
Plan 155 passed, Plan 156 passed, Plan 157 passed, Plan 158 passed;
next executable plan is 159 under the Plan 154 Milestone 8 roadmap.
SAM stays experimental, loopback-only, disabled by default, and non-advertised.
SSU2 v2 has a localhost-only UDP runtime; no public advertisement or
router-to-router interoperability is claimed yet.**