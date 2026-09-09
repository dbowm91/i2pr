# Repository Guidelines

`i2pr` is an experimental Rust I2P router. **Not production-ready.** Do not
use it for anonymity, privacy, censorship resistance, or any security-sensitive
workload. NTCP2 remains experimental and non-advertised; the production daemon
does not activate NTCP2. SSU2 v2 has a localhost UDP runtime and Plan 161 has
proven both direct authenticated IPv4 directions against exact-pinned i2pd 2.61.0;
no public advertisement, public-network participation, broad router
interoperability, or Milestone 6 interoperability is claimed.

## Read first

Always read these before changing code or answering questions about state:

1. `README.md` — current product/plan status.
2. `GUARDRAILS.md` — non-negotiable engineering/security/interoperability constraints.
3. `CONTRIBUTING.md` — local quality/runtime/test conventions.
4. [`plans/README.md`](plans/README.md) and the newest relevant status files.
5. `specs/support.toml` plus `specs/CONFORMANCE.md` for support/evidence claims.

Current authority:

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
Milestone 8 roadmap = Plan 154
Plan 155 = passed SSU2 v2 protocol foundation
Plan 156 = passed SSU2 v2 handshake/token/RouterInfo establishment
Plan 157 = passed SSU2 v2 data-phase reliability/fragmentation
Plan 158 = passed SSU2 v2 UDP runtime and local session product
Plan 159 = passed SSU2 v2 path validation/publication/transport selection
Plan 160 = passed SSU2 v2 peer test and relay reachability
Plan 161 = passed M8 SSU2 independent IPv4 interop and final closure
Plan 162 = passed external-test lane isolation / routine-CI corrective
Plan 163 = registered M9 I2CP roadmap
Plan 164 = passed M9 I2CP protocol and wire foundation
Plan 165 = passed M9 I2CP connection/session/options
Plan 166 = passed M9 I2CP client-owned destination + LeaseSet2 bridge
Plan 167 = passed M9 I2CP loopback server runtime
Plan 168 = passed M9 I2CP message data plane
Plan 169 = passed M9 I2CP self-composed local product and hardening
Plan 170 external wire/data-plane = retained-passed
Plan 170 final acceptance = superseded-by-plan172
Plan 171 = passed M9 I2CP invalid-preamble close and CI corrective (retained)
Plan 172 = passed M9 I2CP independent LeaseSet2 lifecycle corrective
Milestone 9 I2CP final acceptance = closed-via-plan172 (experimental, loopback-only)
Plan 173 = registered M10 service-tunnels roadmap
Plan 174 = passed M10 service-tunnel foundation and shared stream runtime
Milestone 10 foundation = passed-via-plan174 (no listener yet)
next_executable_plan = 175
next product layer = milestone10-service-tunnels
```

For current SSU2 interop work, read in this order:

1. [`plans/162-status.md`](plans/162-status.md)
2. [`plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`](plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md)
3. [`plans/161-status.md`](plans/161-status.md)
4. [`plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`](plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md)
5. [`plans/160-status.md`](plans/160-status.md) through [`plans/155-status.md`](plans/155-status.md)
6. [`plans/154-status.md`](plans/154-status.md) for the M8 roadmap authority.

Read in this order for SAM work:

1. [`plans/151-status.md`](plans/151-status.md)
2. [`plans/151-m7-sam31-final-acceptance-evidence-correction.md`](plans/151-m7-sam31-final-acceptance-evidence-correction.md)
3. [`plans/150-status.md`](plans/150-status.md) — retained external-client core evidence, not final closure
4. [`plans/149-status.md`](plans/149-status.md) — passed product-composition authority
5. Plans 146–148 for historical/reference context.

Read in this order for Milestone 9 I2CP work:

1. [`plans/172-status.md`](plans/172-status.md) — closed final acceptance
2. [`plans/170-status.md`](plans/170-status.md)
3. [`plans/170-m9-i2cp-independent-clients-and-final-closure.md`](plans/170-m9-i2cp-independent-clients-and-final-closure.md)
4. [`plans/171-status.md`](plans/171-status.md)
5. [`plans/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md`](plans/171-m9-i2cp-invalid-preamble-close-and-ci-corrective.md)
6. [`plans/169-status.md`](plans/169-status.md)
7. [`plans/169-m9-i2cp-self-composed-local-product-and-hardening.md`](plans/169-m9-i2cp-self-composed-local-product-and-hardening.md)
8. [`plans/168-status.md`](plans/168-status.md)
9. [`plans/168-m9-i2cp-message-data-plane.md`](plans/168-m9-i2cp-message-data-plane.md)
10. [`plans/167-status.md`](plans/167-status.md)
11. [`plans/167-m9-i2cp-loopback-server-runtime.md`](plans/167-m9-i2cp-loopback-server-runtime.md)
12. [`plans/166-status.md`](plans/166-status.md)
13. [`plans/166-m9-i2cp-client-owned-destination-and-leaseset2.md`](plans/166-m9-i2cp-client-owned-destination-and-leaseset2.md)
14. [`plans/165-status.md`](plans/165-status.md)
15. [`plans/165-m9-i2cp-connection-session-and-options.md`](plans/165-m9-i2cp-connection-session-and-options.md)
16. [`plans/164-status.md`](plans/164-status.md)
17. [`plans/164-m9-i2cp-protocol-and-wire-foundation.md`](plans/164-m9-i2cp-protocol-and-wire-foundation.md)
18. [`plans/163-m9-i2cp-roadmap.md`](plans/163-m9-i2cp-roadmap.md) — planning authority
19. Milestone 9 is closed via Plan 172.

Read in this order for Milestone 10 service-tunnel work:

1. [`plans/174-status.md`](plans/174-status.md) — passed foundation
2. [`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md)
3. [`plans/173-status.md`](plans/173-status.md) — roadmap authority
4. [`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md)
5. Do not start Plan 175 until Plan 174 is passed (it is); do not implement later profiles early.

Plan 171 corrective (retained): every terminal pre-session I2CP
rejection terminates TCP explicitly on the common per-connection
path (`handle_connection` calls `stream.shutdown()` before
`teardown_connection` + `drop_connection`; shutdown failure never
blocks cleanup; no frame is written for an invalid first byte).
`wrong_protocol_byte_is_closed` stays strict — timeout is failure —
and proves a 24-iteration rejection trajectory with zeroed
baselines plus a subsequent valid client; the paused test waits
via a bounded yield-pump/`try_read` drain (no virtual-time
timeout, which raced server polling intermittently on macOS)
and the non-paused
`wrong_protocol_byte_is_closed_real_time` companion separates
product-close evidence from paused-clock timer behavior. Never
revert to drop-timing dependence to make a test convenient.

Do **not** trust prose that disagrees with executable tests/scripts. The newest
explicit superseding status wins when historical records conflict.

## Workspace layout

- `i2pr-proto` — bounded wire codecs, typed errors, no I/O.
- `i2pr-crypto` — protocol cryptographic wrappers.
- `i2pr-storage` — identity/key persistence.
- `i2pr-core` — shared runtime-neutral contracts.
- `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2` — runtime-neutral transport/link codecs.
- `i2pr-netdb`, `i2pr-netdb-persist` — RouterInfo/LeaseSet2 validation and local storage.
- `i2pr-runtime` — production owner of Tokio, sockets, timers, channels, cancellation; also contains non-production external transport test drivers under `tests/`.
- `i2pr-daemon` — CLI/composition root and SAM runtime/socket ownership.
- `i2pr-tunnel` — runtime-neutral exploratory/tunnel substrate.
- `i2pr-client` — destination lifecycle, LeaseSet2, ECIES session/routing, Streaming.
- `i2pr-api` — runtime-neutral SAM 3.1 parsing/state/registry/FORWARD/NAMING plus the M9 I2CP wire/profile foundation (no sockets).
- `i2pr-service-tunnels` — runtime-neutral M10 service-tunnel config/policy (no sockets; foundation only, no listener yet).
- `i2pr-testkit` — deterministic simulation/fault fixtures; no production crate may depend on it.
- `tools/i2pr-interop` — non-production test launcher.

Architecture details live under `docs/architecture/`; ADRs live under
`docs/adr/`.

## Current SAM architecture

Retain the working Plan 149 product structure unless a newer executable test
exposes a concrete defect:

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

If a new test exposes an M6 Streaming protocol defect, write a narrow protocol
corrective rather than weakening the test. That stop fired once as Plan 152
(passed narrow M6 corrective, no wire change).

## Plan 153 scope (closed)

Plan 153 was documentation and CI hygiene only: it normalized the
authoritative `plans/152-status.md`, removed stale Plan 151/152 prose,
added the Plan 152 closure pointer to the support ledger, and enforced
the Plan 151 SAM evidence-integrity checker in routine Linux CI and
the manual SAM external workflow. No `crates/` or `Cargo.lock` changes
were made.

## Plan 161 interop evidence (passed, retained)

Plan 161 has proven both independent direct SSU2 v2 directions against
exact-pinned i2pd (see `plans/161-status.md` for the full matrix and
the 24-criterion checklist):

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
direction = i2pr initiator -> i2pd responder
transport = real loopback UDP
```

The passed trajectory includes tokenless Retry establishment, mutual
authentication, one small and one fragmented DatabaseStore from i2pr to i2pd,
DeliveryStatus traffic back over the authenticated session, and graceful
resource cleanup. Independent comparison exposed three real handshake
transcript divergences that were corrected in Plan 161; do not revert them to
make loopback tests match older fixtures.

Direction B, the cached-token/malformed rows, and the final
ledger/checker/workflow lane have passed locally and hosted (see
`plans/161-status.md`); Java I2P is recorded nonblocking secondary
debt. Direction A+B evidence
does not imply public I2P or broad router interoperability. Milestone 8
is closed within this bounded scope; the next layer is milestone9-planning.

## Plan 162 scope (closed)

Plan 162 was a narrow test-lane/CI corrective. Routine CI run
`33915994884` on Plan 161 direction-A head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c` failed both Ubuntu and macOS
quality jobs because ordinary workspace execution ran
`crates/i2pr-runtime/tests/ssu2_independent.rs` without an external i2pd
environment. Dependency policy and MSRV passed.

Required correction:

- keep the Plan 161 external test compiled by all-target checks;
- mark the environment-dependent test explicitly ignored for ordinary libtest execution;
- run it only with explicit `--ignored --exact` in the external lane;
- keep missing external environment a hard failure after explicit selection;
- do not add filename filtering, `|| true`, `continue-on-error`, fake peer values, or broad workspace exclusions;
- re-run direction A against the exact same pinned i2pd after gating;
- require ordinary Ubuntu + macOS CI green on the Plan 162 closing head;
- then return directly to Plan 161. These conditions passed on implementation
  commit `624e8cce177040674376163160cfbda47e6a60fe`, hosted CI run
  `33941941145`.

Do not change SSU2 production source or wire semantics inside Plan 162. If the
explicit external re-run exposes a real protocol defect, stop and create a
separate narrow protocol corrective.

## Hard boundaries

These remain non-negotiable and are CI-enforced where applicable:

- preserve workspace dependency direction; no production crate depends on `i2pr-testkit`;
- no unbounded channels/queues introduced for convenience;
- Tokio/socket/timer/task ownership stays in runtime/daemon layers;
- every spawned task has explicit ownership/cancellation;
- SAM remains loopback-only and disabled by default;
- no root/sudo, privileged container, network namespace, VM, systemd, or public-I2P requirement for current acceptance work;
- no external-client/reference patching or vendoring;
- no private SAM `PRIV`, signing seed, SSU2 static/session secret, token, or raw private application payload in logs/evidence;
- do not make `DestinationIdentity: Clone` or reconstruct a second private identity for the SAM bridge;
- an external interoperability test may be ignored in routine CI only when its dedicated lane explicitly opts in and remains fail-closed if required external configuration is absent.

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
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
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
cargo test --locked -p i2pr-runtime --lib
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay -- --test-threads=1
bash scripts/check-ssu2-vectors.sh
```

During Plan 162, ordinary no-peer invocation of the external driver must be:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent -- --test-threads=1
# expected: external test discovered as ignored; command exits 0
```

The explicit external invocation after Plan 162 gating is:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

Without the required external environment, that explicit command must fail
closed. With exact-pinned i2pd provisioned, it must execute and pass the
full matrix (directions A+B, cached-token, malformed/resource rows).

The full Plan 161 lane (local suites + matrix + gates, 15 command-derived
rows) is:

```text
bash tests/integration/ssu2/run-independent.sh
bash scripts/check-ssu2-acceptance-evidence.sh
```

Plan 155 added the SSU2 fixture corpus (`tests/fixtures/ssu2/`) and its
checker (`scripts/check-ssu2-vectors.sh`), enforced in routine Linux CI;
do not weaken it to make CI pass.

Focused I2CP seams currently include:

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-api --test i2cp_vectors
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
bash scripts/check-i2cp-vectors.sh
```

Focused M10 service-tunnel foundation seams currently include:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --lib destination_streaming
cargo test --locked -p i2pr-daemon --lib config
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation -- --test-threads=1
```

Plan 164 added the I2CP fixture corpus (`tests/fixtures/i2cp/`) and its
checker (`scripts/check-i2cp-vectors.sh`), enforced in routine Linux CI;
do not weaken it to make CI pass. Plan 165 added the connection state
machine, SessionConfig verification, option disposition table, and
session registry under `crates/i2pr-api/src/i2cp/`; no I2CP
listener, destination activation, or client-interoperability claim
exists yet. Plan 166 added the client-owned destination runtime +
Standard LeaseSet2 validation path under `crates/i2pr-client/`
(`DestinationOwnership`, `DestinationPublic`,
`InboundDecryptionCapability`, `install_client_lease_set2`,
`LeaseRequest`, `take_client_refresh_request`) and the
`I2cpAction::RequestVariableLeaseSet` typed action in
`crates/i2pr-api/src/i2cp/actions.rs`; the listener, socket
ownership, and client-interoperability claim remain Plans 167–170.
Plan 167 added the supervised loopback I2CP v0.9.67 server runtime
in `crates/i2pr-daemon/src/i2cp.rs` plus the real-TCP acceptance
test in `crates/i2pr-daemon/tests/i2cp_loopback.rs`. Plan 168 added
the message data plane under `crates/i2pr-api/src/i2cp/data_plane.rs`
(`I2cpMessageOutcome`, `PendingStatusTable`, `InboundPayloadQueue`,
bounded per-session ceilings, `InboundPayloadFrame::WIRE_OVERHEAD_BYTES`)
plus the per-session `I2cpSessionState` in
`crates/i2pr-daemon/src/i2cp.rs` and the eighteen-test black-box
acceptance suite in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs`;
the daemon validates `SendMessage`/`SendMessageExpires`, routes
payloads through the existing `i2pr_client::DestinationRuntime::enqueue_outbound`,
drains `MessagePayload` inbound frames through a `tokio::sync::Notify`,
resolves `DestLookup` against the local destination registry, and
returns the config-derived `BandwidthLimits` reply. Plan 169 added
the reconfigure transaction handler (`handle_reconfigure_session` +
`apply_reconfigure` + `ReconfigurationOutcome`) and the per-session
reconfigure baseline (`I2cpSessionState::last_options`), the
synchronous `handle_destroy_session` data-plane drain, and three
new narrowly named acceptance suites:
`crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` (the canonical
self-composed trajectory plus a bounded repeated-lifecycle soak and
a destroy-one-session/keep-sibling usable proof),
`crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` (the Plan 169
§5 adversarial protocol matrix), and
`crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` (the Plan 169 §6
concurrency/resource matrix). Independent Java/Go client evidence
remains in Plan 170.

## Testing conventions

- Prefer paused Tokio/manual clocks for deterministic runtime tests where compatible with socket behavior.
- Runtime-owned socket tests use loopback only; no DNS/public traffic.
- Use explicit bounded deadlines, not indefinite waits.
- Queue/resource tests cover exact capacity/max+1 and verify release after failure/closure.
- Secret-bearing values stay redacted/zeroized/non-Clone where practical.
- Channel/socket closure is a lifecycle event, not blindly retried.
- A required evidence result must be derived from an executed command/test. Never mark an acceptance row passed merely because a historical plan says it passed.
- External tests that require a separately provisioned process must never silently pass when that process/environment is absent.

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
acceptance tests were integrated; Plan 153 made its evidence checker a
permanent invariant.

## External SSU2 evidence

Current mandatory reference:

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
role = mandatory Plan 161 independent direct SSU2 reference
```

Preferred secondary reference:

```text
Java I2P 2.13.0
commit = 9134f808337b401e8e53c73734c81fab04280c9d
role = preferred secondary; recorded nonblocking narrow-orchestration debt (see plans/161-status.md)
```

Plan 161 directions A+B plus the ledger/checker/workflow lane have
passed locally. Plan 162 corrected how that external-process
test is selected by routine versus dedicated lanes; the corrective is now
closed.

## Coding conventions

- No unsafe in protocol/client/API/service crates unless separately reviewed.
- Treat all SAM/network bytes as hostile and bounded.
- Use typed errors; do not swallow codec/protocol results.
- Runtime cryptography/local-product ephemeral material uses OS CSPRNG.
- New dependencies require explicit review.
- Avoid global mutable state/service locators; pass narrow capabilities.
- Do not modify M6 wire semantics to make a SAM test convenient.
- Do not modify SSU2 wire semantics merely to make a CI lane green.

## Protocol claims

- Milestone 6 local product is closed via Plan 134; router interoperability is not claimed.
- Plan 149 closed the self-composing localhost SAM product.
- Plan 150 retains at-least-two independent-client core evidence, but its final acceptance label is superseded.
- Plan 151 is the current final Milestone 7 acceptance authority.
- Plan 152 is the passed narrow M6 robustness corrective retained underneath Plan 151.
- Plan 153 is the passed docs/CI hygiene pass.
- Plans 155–160 are passed Milestone 8 SSU2 v2 local protocol/runtime/reachability stages.
- Plan 161 is passed: directions A+B (+ cached-token/malformed rows) against exact-pinned i2pd 2.61.0 are proven over real loopback UDP with authenticated bidirectional evidence, and the fail-closed ledger/checker/workflow lane passes locally and hosted (routine CI runs `34050058216`/`34053041778`, external runs `34051298144`/`34053042857`). Milestone 8 is closed within that bounded scope.
- Plan 162 passed the narrow external-test lane/CI corrective.
- Plan 163 registered the Milestone 9 I2CP roadmap (planning authority only).
- Plan 164 passed the M9 I2CP protocol and wire foundation (structural codecs, fixtures, profile; no behavior claim).
- Plan 165 passed the M9 I2CP connection/session/options state machines (typed connection state, SessionConfig signature/date/ceiling verification with injected clock, option disposition table, bounded session registry, reconfiguration taxonomy, typed `I2cpAction` vocabulary; no listener, destination activation, or interoperability claim).
- Plan 166 passed the M9 I2CP client-owned destination + LeaseSet2 bridge: `DestinationOwnership::RouterOwned` / `ClientOwned`, `DestinationPublic`, `InboundDecryptionCapability`, atomic `install_client_lease_set2` (signature + lease ownership + expiry + decryption-key match), typed `LeaseRequest` for refresh from real inbound tunnels, and the `I2cpAction::RequestVariableLeaseSet` action. SAM router-owned product regressions remain green; no listener, socket ownership, or interoperability claim.
- Plan 167 passed the M9 I2CP loopback server runtime in `crates/i2pr-daemon/src/i2cp.rs`: disabled-by-default `[i2cp]` block, supervised Tokio listener, per-connection `ChildScope`, typed `I2cpAction` dispatch, single `teardown_connection` cleanup path on EOF/reset/timeout/cancel, and twelve real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_loopback.rs`. No application-message direction, no lookup, no reconfiguration, no independent-client interop claim.
- Plan 168 passed the M9 I2CP message data plane: bounded per-session `SendMessage`/`SendMessageExpires` validation against the existing `i2pr_client::DestinationRuntime::enqueue_outbound` seam, bounded `MessageStatus` correlation table, `MessagePayload` inbound frames delivered only to the owning session's bounded queue (sibling-isolation guaranteed), cross-session local loopback shortcut, `DestLookup` resolving through the local destination registry, and `GetBandwidthLimits` returning the config-derived client ceiling and the documented neutral router values. Eighteen real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` exercise every Plan 168 §11 case. SAM router-owned product regressions remain green. No reconfiguration, no `HostLookup`/`HostReply` resolution, and no independent-client interop claim.
- Plan 169 passed the M9 I2CP self-composed local product and hardening: the `ReconfigureSession` transaction handler (`handle_reconfigure_session` + `apply_reconfigure` + `ReconfigurationOutcome`) parses/verifies the full new SessionConfig, classifies each diff entry using the Plan 165 `reconfiguration_class` table, and commits the new baseline atomically through `I2cpSessionState::last_options`; immutable and unsupported keys reject the whole transaction without state mutation. `handle_destroy_session` now drains the per-session Plan 168 data-plane bookkeeping synchronously so repeated DestroySession/CreateSession cycles retain zero inbound queue, status correlation, or outbound slot. The Plan 169 acceptance suites are `crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` (5 tests), `crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` (19 tests at Plan 169 close; 20 after the Plan 171 companion), and `crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` (6 tests); every test binds the listener to `127.0.0.1:0` and drives behavior only through TCP/I2CP inputs. SAM router-owned product regressions, the Plan 167 listener regression in `i2cp_loopback.rs`, and the Plan 168 data-plane suite in `i2cp_message_data_plane.rs` remain green. No `HostLookup`/`HostReply` resolution and no independent Java/Go client evidence yet; those belong to Plan 170.
- Plan 171 passed the M9 I2CP invalid-preamble close and CI corrective (retained): the common per-connection terminal path in `crates/i2pr-daemon/src/i2cp.rs` explicitly shuts the TCP stream down before bookkeeping release (no wire change, no independent-client claim). The adversarial matrix is now 20 tests (the strict 24-iteration `wrong_protocol_byte_is_closed` plus its non-paused `wrong_protocol_byte_is_closed_real_time` companion).
- Plan 170 external wire/data-plane evidence is retained-passed: exact-pinned Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`) and go-i2cp (`b529ee1c10a6011558b4d69fc9436a4afc489eac`) exchange digest-matched 25 B/32 KiB payloads in both directions through the loopback daemon (`tests/integration/i2cp/run-independent.sh`, 9 fail-closed rows, `scripts/check-i2cp-acceptance-evidence.sh` in routine CI, manual `.github/workflows/i2cp-external.yml`). Its final-acceptance interpretation is superseded by Plan 172: the counted Java driver bypassed `I2PSession.connect()` and no external session installed a LeaseSet2. Do not claim independent LeaseSet2 lifecycle from Plan 170 rows alone.
- Plan 172 passed the M9 I2CP independent LeaseSet2 lifecycle corrective (see `plans/172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md` and `plans/172-status.md`): explicit local zero-hop tunnel kind, 44-byte Lease-compatible non-empty real `RequestVariableLeaseSet` from the destination pool, existing Plan 166 atomic install (ElGamal-slot skip, unpublished accepted, u8 LS2 key count), high-level Java `I2PSession.connect()` plus public go-i2cp async `ProcessIO` lifecycle proof, post-LS2 digest-matched cross-client traffic both directions, fail-closed 24-row evidence. Milestone 9 final acceptance is closed-via-plan172.
- SAM stays experimental, loopback-only, disabled by default, and non-advertised.
- SSU2 public advertisement/public-network participation is not claimed.
- No Plan 161 direction-A evidence implies Milestone 6 destination/Streaming/tunnel interoperability or broad router interoperability.
- Do not advance `advertised = true` without `specs/CONFORMANCE.md` evidence.

## OpenCode skills

Use `i2pr-local-dev` for current local product/SSU2 execution guidance and
`i2pr-architecture` for architecture/ADR/plan navigation. Historical NTCP2,
rootless, and Multipass skills remain separate lanes.

## Commits and handoff

Use focused commits. Do not change git config, skip hooks, force-push, or amend
someone else's commit. Closure records must include exact commands/results and
current-head workflow evidence.

Current handoff: **Plan 174 passed the M10 service-tunnel
foundation and shared Streaming runtime (runtime-neutral
`i2pr-service-tunnels` crate, strict disabled-by-default
loopback-only `[service_tunnels]` surface, generic bounded
socket<->Streaming pump reused by SAM, no listener yet).
Milestone 9 remains closed via Plan 172. Do not implement generic,
HTTP, SOCKS5, or IRC listeners until Plan 175.**
