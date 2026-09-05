---
name: i2pr-local-dev
description: Work on the local product path of the i2pr Rust I2P router — Milestone 6 destinations/garlic/LeaseSet2/Streaming, Milestone 7 SAM 3.1, and current Milestone 8 SSU2 execution. Plans 155–160 passed the local SSU2 v2 stack; Plan 161 direction A passed against exact-pinned i2pd 2.61.0; Plan 162 passed the external-test lane isolation/routine-CI corrective and Plan 161 resumes.
---

# I2PR Local Development

Use this skill for the local product/SAM/SSU2 execution side of the router.
Historical mixed-router NTCP2 work remains separate acceptance debt.

## Current authority

Milestone 6 local product closure remains Plan 134:

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
```

Current M7/M8 authority:

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
plan_159 = passed-m8-ssu2-path-validation-publication-and-transport-selection
plan_160 = passed-m8-ssu2-peer-test-and-relay-reachability
plan_161 = in-progress-direction-a-proven
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration

milestone8_planning_authority = plan154
milestone8_foundation = passed-via-plan155
milestone8_handshake = passed-via-plan156
milestone8_data_phase = passed-via-plan157
milestone8_udp_runtime = passed-via-plan158
milestone8_path_publication_selection = passed-via-plan159
milestone8_peer_test_relay = passed-via-plan160
milestone8_ssu2_direction_a = passed-via-plan161
milestone8_final_acceptance = not-yet-closed

next_executable_plan = 161
resume_after_plan162 = 161
next_product_layer = milestone8-ssu2-v2
```

Read in order for current SSU2 work:

1. `plans/162-status.md`
2. `plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`
3. `plans/161-status.md`
4. `plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`
5. `plans/160-status.md`
6. `plans/159-status.md`
7. `plans/158-status.md`
8. `plans/157-status.md`
9. `plans/156-status.md`
10. `plans/155-status.md`
11. `plans/154-status.md`

For SAM/local-product history, then read Plan 151, 150, 149 and Plans 146–148
as needed.

Plans 155–160 passed the local SSU2 v2 protocol/runtime/reachability sequence.
Plan 161 has already proven direction A (`i2pr initiator -> i2pd responder`)
over real loopback UDP against exact-pinned i2pd 2.61.0. Plan 162 passed its
narrow corrective: routine CI now ignores the environment-dependent external
test while retaining all-target compilation, and explicit external selection
remains fail-closed. Resume Plan 161 for direction B and the remaining matrix.

SAM stays experimental, loopback-only, disabled by default, and non-advertised.
SSU2 public advertisement/public-network participation and broad router
interoperability remain unclaimed.

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
- Plan 150 external core evidence: pinned i2psam + qualified i2plib SAM surface, exact two-direction 2 MiB transfers, private destinations, SILENT, NAMING, negative matrix, and positive FORWARD;
- Plans 155–160 SSU2 local protocol/runtime/path/peer-test/relay architecture;
- Plan 161 direction-A handshake transcript corrections and regenerated vectors. Independent i2pd comparison exposed those defects; do not revert them to match older i2pr↔i2pr assumptions.

## Why Plan 151 exists

Plan 150's implementation/external-client work is useful, but its final
acceptance ledger overclaimed several deferred cases. The clearest example was
an unconditional `multiple-stream-lifecycle = passed` row referring to a Plan
149 sibling-stream test that did not exist.

Plan 151 made the deferred sibling/backpressure/fault/CLOSE-RESET/FORWARD and
focused M6 regression items executable through the real listener and required
every final `passed` row to derive from a command/test that actually ran.

That pass exposed one narrow M6 robustness defect family, closed by Plan 152
without a wire change: bounded receiver retention/ACK gating, coalesced
duplicate ACK behavior, and sender ECIES ratchet-key trimming.

## Evidence-integrity rule

No required final row may be marked passed merely because another plan/status
says it passed. `tests/integration/sam/run-independent.sh` derives required SAM
rows from executed commands/tests.

Plan 151 added:

```text
scripts/check-sam-acceptance-evidence.sh
```

The checker is enforced in routine Linux CI and the manual SAM external
workflow. Do not weaken it to make CI pass.

The same principle applies to current SSU2 work: Plan 161 final evidence must
come from explicitly executed local/external commands. An external test that
is skipped because no peer exists is **not** an external-interoperability pass.

## Plan 161 independent SSU2 provenance

Retain exact pins:

```text
i2pd
  version: 2.61.0
  repo: PurpleI2P/i2pd
  pin: 635b013a612ff47278ef02acf8580a28e10e26c5
  role: mandatory independent Plan 161 SSU2 reference

Java I2P
  version: 2.13.0
  repo: i2p/i2p.i2p
  pin: 9134f808337b401e8e53c73734c81fab04280c9d
  role: preferred secondary; nonblocking if narrow unprivileged orchestration is disproportionate
```

Do not patch or vendor external routers.

Direction A retained evidence:

```text
i2pr initiator -> i2pd responder
real loopback UDP
tokenless TokenRequest -> Retry -> SessionRequest -> SessionCreated -> SessionConfirmed
mutual authentication
small DatabaseStore i2pr -> i2pd
fragmented DatabaseStore i2pr -> i2pd
DeliveryStatus return for both stores
graceful session/resource teardown
```

Direction B and the remaining Plan 161 token/resource/Java/evidence-workflow
matrix remain open.

## Plan 162 closure rule/result

Current routine CI run `33915994884` on head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c` failed both Ubuntu and macOS
quality jobs because ordinary workspace execution automatically ran:

```text
crates/i2pr-runtime/tests/ssu2_independent.rs
```

without an external i2pd environment. Dependency policy and MSRV passed; the
observed error was `missing required env I2PD_ROUTER_INFO`.

Plan 162 implemented this shape:

```text
ordinary workspace test
  -> external test is compiled/discovered
  -> external test is ignored
  -> ordinary command exits 0

dedicated external invocation
  -> explicitly selects ignored test with --ignored --exact
  -> missing external environment still fails hard
  -> exact-pinned i2pd environment executes the real trajectory
```

Preferred mechanism: a descriptive Rust `#[ignore = "..."]` attribute on only
the environment-dependent external test.

Forbidden fixes:

- missing-env early return/success;
- CI executable-name filtering;
- `|| true`;
- `continue-on-error`;
- fake `I2PD_*` values;
- broad crate/integration-test exclusion;
- production SSU2 changes merely to make CI green.

Plan 162 re-ran direction A after gating and required routine Ubuntu/macOS CI
green on its exact closing commit. The implementation closing commit was
`624e8cce177040674376163160cfbda47e6a60fe`, verified by hosted CI run
`33941941145`; `next_executable_plan = 161` is restored.

## External SAM provenance

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
  role: built/probed, not counted
```

Do not patch or vendor external clients.

## Environment contract

```text
root/sudo                         = no
Linux namespaces                  = no
Docker                            = no
VM/Multipass                      = no
systemd                           = no
public I2P network                = no
localhost TCP                     = yes
localhost UDP                     = yes
exact-pinned external i2pd process= yes, Plan 161 dedicated lane only
routine CI external peer          = no
manual GitHub external lane       = yes
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
bash scripts/check-sam-acceptance-evidence.sh
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

Focused SSU2 floor:

```text
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --lib
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay -- --test-threads=1
bash scripts/check-ssu2-vectors.sh
```

Plan 162 ordinary no-peer regression:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent -- --test-threads=1
# expected after Plan 162 implementation: test ignored, exit 0
```

Plan 162 / Plan 161 explicit external invocation:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

With required environment absent, that explicit command must fail for missing
external configuration. With the exact-pinned i2pd lane provisioned, it must
execute and pass the real direction-A trajectory.

## Coding rules

- No new unbounded channels/queues.
- Runtime/socket ownership stays in daemon/runtime layers.
- The SSU2 central scheduler replaces a handshake's resend deadline with each new arm batch; never min-merge with a stale past value (Plan 158 regression).
- SSU2 path migration must requeue unacked fragments through the bounded loss policy; never just clear sent provenance (Plan 159 regression).
- SSU2 peer-test correlation is by nonce plus role/state, never by source; unsigned out-of-session corroboration never confirms direct reachability (Plan 160).
- SSU2 relay success proves firewalled, never direct; verify HolePunch against nonce-derived connection IDs before touching request state (Plan 160).
- SSU2 path challenges/responses are single-shot minimum-MTU control datagrams; never migrate on source change alone.
- OS CSPRNG for runtime material; deterministic randomness is test-only.
- Never log private destination material, SSU2 static/session keys, tokens, or raw payloads.
- No second private identity copy for SAM bridge ownership.
- Do not weaken M6 Streaming semantics for SAM tests.
- Do not weaken SSU2 authentication/RouterInfo/token/replay semantics for external interop.
- Do not modify SSU2 production wire behavior to repair Plan 162 CI selection.

## Final claim rules

- SAM stays disabled by default, loopback-only, experimental, and non-advertised.
- Milestone 7 final localhost acceptance is closed via Plan 151; Plan 152 is the retained narrow M6 corrective underneath it.
- Plan 153 passed post-M7 docs/CI hygiene.
- Plans 155–160 passed the local SSU2 v2 protocol/runtime/path/reachability sequence.
- Plan 161 direction A against exact-pinned i2pd is passed evidence, but Plan 161 final closure remains open.
- Plan 162 passed the narrow external-test lane/CI corrective; it must not broaden or downgrade direction-A protocol evidence.
- Resume Plan 161 for direction B and final acceptance.
- `milestone6_interoperable = not-yet-claimed` remains unchanged.
- SSU2 public-network participation, broad router interoperability, IPv6 external interop, PQ v3/v4, and SSU1 remain unclaimed/deferred as documented.
- Do not advance `advertised = true` without `specs/CONFORMANCE.md` evidence.

Current handoff: **resume Plan 161 now. Preserve its direction-A interop and
transcript corrections while completing direction B and the remaining final
acceptance matrix.**
