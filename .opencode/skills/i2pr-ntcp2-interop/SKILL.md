---
name: i2pr-ntcp2-interop
description: Operate, diagnose, or extend the repository's historical Plan 038/040/041/043/044/045/046/048/049/050/051/052/053/054/055/058/059/060/062/063/064/065/066/067/068/069/075/076/077/080/081/082/083/084/085/086/087/088/090/091/092/093/094/095/096/097/098/099/100 NTCP2 interoperability harness. The active development interop lane is closed; the retained NTCP2 development result is `protocol-defect-localized` at `noise_authenticated` (Plan 099/100, normal-daemon NTCP2 disabled per Plan 101). Use when an agent is asked to read or reproduce the historical harness surface, run a bounded interop profile on a host where the active sequence is executable, prepare or validate reference routers, add or modify a scenario, or validate evidence. Do not activate NTCP2 in the production daemon; do not extend the historical lane without a new plan-of-record.
---

# I2PR NTCP2 Interop (host harness, Plans 038/040/041/043/045/055/058/059/081/082/083/084)

The host-side Ubuntu 24.04 amd64 Plan 038 reference-router NTCP2
interoperability harness. **The active development interop lane is
closed; NTCP2 remains experimental and non-advertised.** The Plan
099/100 retained result is `protocol-defect-localized` at
`noise_authenticated`. Plan 101 corrects the daemon NTCP2 activation
boundary; the normal daemon does not activate NTCP2.

Read `AGENTS.md`,
[`docs/architecture/interop-apparatus.md`](../../docs/architecture/interop-apparatus.md),
[`docs/architecture/tooling.md`](../../docs/architecture/tooling.md#testsintegrationntcp2--synthetic-interoperability-lane-plan-036),
the relevant `plans/038-..`, `plans/040-..`, `plans/041-..`,
`plans/043-..`, `plans/044-..`, `plans/045-..`, `plans/046-..`,
`plans/048-..`, `plans/049-..`, `plans/050-..`, `plans/051-..`,
`plans/052-..`, `plans/053-..`, `plans/054-..`, `plans/055-..`,
`plans/058-..`, `plans/059-..`, `plans/060-..`, `plans/062-..`,
`plans/063-..`, `plans/064-..`, `plans/065-..`, `plans/066-..`,
`plans/067-..`, `plans/068-..`, `plans/069-..`, `plans/075-..`,
`plans/076-..`, `plans/077-..`, `plans/081-..`, `plans/082-..`,
`plans/083-..`, `plans/084-..`, `plans/085-..`, `plans/086-..`,
`plans/087-..`, `plans/088-..`, `plans/090-..`, `plans/091-..`,
`plans/092-..`, `plans/093-..`, `plans/094-..`, `plans/095-..`,
`plans/096-..`, `plans/097-..`, `plans/098-..`, `plans/099-..`,
`plans/100-..`, `plans/101-..`, `plans/102-..`, `plans/103-..`,
`plans/104-..`, `plans/105-..`, `plans/106-..` and `plans/115-..`,
`plans/117-..`, `plans/118-..` status records, the relevant
`docs/adr/` records, and `tests/integration/ntcp2/README.md` before
changing the harness.

The canonical reference identifiers are `java_i2p` and `i2pd`.
Locked source objects:

- Java I2P `2800040deee9bb376567b671ef2e9c34cf3e30b6`
- i2pd `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`

Abbreviated revisions are not valid cache or evidence inputs.

## Authoritative terminal state (the closed result)

```text
plan_099            = closed-protocol-defect-localized
plan_100            = passed-exit-cleanup-and-handoff
plan_095            = historical-superseded-by-plan099-single-job-lane
plan_087            = historical-development-sequence-superseded-by-plan100
plan_088            = historical-development-sequence-superseded-by-plan100
plan_079            = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
plan_072            = inactive-pending-plan088-ambiguity
ntcp2               = experimental-non-advertised
normal_daemon_activation = disabled
exact_wire_stage    = noise_authenticated
external_netdb_over_ntcp2 = blocked
compact_summary     = target/interop/evidence/milestone-3/31521642090/plan099-summary.json
```

The cross-side defect is recorded as a localized protocol defect;
no further Rust correction is attempted under Plan 100. NTCP2
remains experimental and non-advertised. **Do not promote a typed
blocker, a reference-only control record, a parser-only result, or a
testkit result into interoperability evidence.**

The active development interop surface is intentionally small and
bounded:

- `scripts/interop/run-minimal-i2pd-host-loopback-probe.py` — the
  only allowed entry point to live subprocess execution
- `tests/integration/ntcp2/harness/plan083_runner.py` and
  `plan084_runner.py` — the canonical forward/reverse runners
- `tests/integration/ntcp2/harness/preflight_runner.py` — the
  listener-only preflight
- `tests/integration/ntcp2/harness/i2pd_direct_driver.py`,
  `minimal_i2pd_probe.py`, `minimal_i2pd_reverse_probe.py`,
  `interop_topology.py`, `reference_event.py`,
  `reference_trigger_v4.py`, `execution_lane.py`,
  `plan099_exit_gate.py` — the functional interop modules
- `tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py`,
  `test_i2pd_direct_driver.py`, `test_i2pd_direct_control.py`,
  `test_execution_lane.py` — the bounded functional tests
- `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`
  and `CMakeLists.txt` — the i2pd driver build script and the
  split-library link contract
- `scripts/check-ntcp2-interoperability.sh` — the trimmed static
  boundary check

**Forbid adding** new `test_planNNN.py` files, new plan-number-
specific Python runners, or new plan-token static checks without a
new plan-of-record. Historical plan documents remain in `plans/` as
audit records but are not executable API contracts.

The active sequence **forbids repairing or retrying** the Plan 046
rootless lane or the Plan 048/049/050 Multipass recovery lane.
Multipass, Docker, public-network traffic, reseed, SAM, I2CP, SSU2,
and Java I2P are not part of the development interop surface. A
localized NTCP2 defect keeps NTCP2 disabled and non-advertised but
does not block production daemon composition, RouterInfo publication
architecture, NetDB storage/indexing, SU3 reseed parsing, or
deterministic local state-machine tests.

## Plan 042 runtime and launcher boundary

The NTCP2 wire driver is a runtime-owned composition. `i2pr-runtime`
owns Tokio sockets and tasks, action deadlines, cancellation, replay
admission, authenticated frame state, bounded queues, and child
joins. The `i2pr-transport-ntcp2` state machines remain
runtime-neutral and receive only complete bounded actions.
`tools/i2pr-interop` is a non-production launcher seam: it validates
bounded non-secret scenario input and composes the runtime driver,
but it must **never** activate `i2pr-daemon`.

The launcher status protocol has separate meanings. A completed
`listen` emits listener readiness and then a distinct authenticated
terminal result; `dial` emits one terminal typed result; `inspect`
emits redacted state metadata. **Listener readiness is not
authentication.**

Plan 042 selects the existing fixed-size DeliveryStatus message
(I2NP type 10) for the first data smoke: 12-byte body, 21-byte
NTCP2/SSU2 short transport encoding, 24-byte NTCP2 block before frame
overhead and padding. A positive gate requires one authenticated
outbound and one authenticated inbound DeliveryStatus per direction
plus orderly cleanup. Reference acceptance or echo behavior is not
yet verified; do not claim interoperability or substitute
padding/TCP readiness for the message exchange.

## Companion skills (load before doing this lane)

- `i2pr-rootless-sandbox` — Plan 046 host-side rootless
  sealed-namespace lane.
- `i2pr-multipass-recovery` — Plan 048/049/050 Multipass recovery
  lane (atomic lifecycle, cloud-init taxonomy, base verify, four
  Plan 045 directions, sanitized export, selective purge, and the
  Plan 051 dispatch-gate troubleshooting bridge).

If the host emits `blocked_unprivileged_user_namespace` from the Plan
046 probe, do not try to recover inside Plan 038. Hand off to
`i2pr-multipass-recovery`.

## Safety boundary

Treat the harness as experimental infrastructure, not an anonymity or
security tool. **Never** enable `i2pr-daemon`, use public egress,
perform DNS / bootstrap / reseed, retain identities/keys/RouterInfo/
raw logs/packet captures, or turn a local self-handshake, loopback
run, vector, or testkit result into Java I2P or i2pd
interoperability evidence. Keep support rows experimental and
non-advertised unless sanitized evidence satisfies
`specs/CONFORMANCE.md`.

Run only on an authorized disposable Ubuntu 24.04 amd64 host. The
namespace and firewall checks are mandatory and fail closed. Do not
bypass a host, privilege, route, cleanup, or evidence validation
error.

The exact host contract is Ubuntu 24.04 amd64/x86_64, Bash 4+, UTF-8
locale, non-interactive `sudo` when not root, Linux
namespace/nftables capability, and ≥4 GiB free under `target/`.
Declared package set and locked source, IzPack, cache, and
build-command inputs are authoritative in
`tests/integration/ntcp2/references.lock.toml`.

## Result interpretation

- `blocked_host_contract` — no router process or protocol claim was
  made.
- `i2pr-mixed-router-profile-not-wired` — the active scenario ID is
  not allowlisted for the current mixed-router gate.
- Rejected configuration/state, authentication, timeout, cleanup, or
  evidence-validation failures remain typed and visible. **Never**
  convert them to pass or omit them from the closure record.
- An empty evidence directory is not success. Plan 041
  reference-pair records are harness controls, not i2pr mixed-router
  evidence.
- For Plan 046 typed host-level blockers (e.g.,
  `blocked_unprivileged_user_namespace`), hand off to
  `i2pr-multipass-recovery`.
- A blocked profile, a reference-only control record, a typed
  blocker, or a parser-only result is **never** an i2pr
  interoperability result. Do not advertise NTCP2 and do not close
  Milestone 3.

## Authoritative command surface (host-side)

The authoritative historical surface lives under
`scripts/interop/ubuntu/` and `scripts/interop/`. Run from the
repository root:

```text
# Host + build gates
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline

# Profiles (Plan 043 gate order)
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile full

# One bounded run
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-java-ipv4 --reference java_i2p
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-i2pd-ipv4 --reference i2pd

# Validation and cleanup
bash scripts/interop/validate-evidence.py
python3 scripts/interop/aggregate-evidence.py --profile <profile>
bash scripts/check-ntcp2-interoperability.sh
sudo -E bash scripts/interop/cleanup.sh
sudo -E bash scripts/interop/verify-clean-host.sh --verify \
    --baseline target/interop/build/clean-host-baseline.json
```

The Plan 043 gate order is:

```text
contract -> reference-build -> reference-offline-reuse
-> environment-smoke -> reference-crosscheck-ipv4
-> i2pr-handshake-smoke-ipv4 -> full-matrix -> evidence-validation
-> cleanup-verification
```

Plan 044 expands each primary IPv4 scenario into four independently
attributable directional executions (`i2pr-to-java-ipv4`,
`java-to-i2pr-ipv4`, `i2pr-to-i2pd-ipv4`, `i2pd-to-i2pr-ipv4`) and
renders each launcher scenario through the strict renderer; the
data-phase proof uses a typed non-echo oracle (split send/receive per
direction) rather than an assumed echo. A successful launcher result
is local driver validation only; the reference profile still requires
authenticated data exchange and cleanup, not TCP or listener
readiness alone.

The launcher status meanings are fixed: schema-1
`i2pr-interop-status` records use fixed phase, result, reason-code,
and aggregate counters; `listen` readiness is separate from a later
authenticated terminal result, `dial` has one terminal result, and
`inspect` returns only redacted metadata. Typed state,
authentication, data-phase, timeout, and cleanup failures are
terminal results, never readiness or evidence.

## Plan 087 host-loopback forward probe (closed)

The Plan 086 `host-loopback-development` lane drove one real
`i2pr -> i2pd` forward wire attempt through the canonical Plan 083
runner. The wrapper:

```text
python3 scripts/interop/run-minimal-i2pd-host-loopback-probe.py \
    --direction i2pr-to-i2pd-ipv4 \
    --repo-root <repo> \
    --run-root <fresh-run-root> \
    --run-id <plan082-run-id> \
    --source-commit <40-hex> \
    --output <record.json> \
    --i2pd-driver-binary <i2pd_ntcp2_interop_driver_instrumented> \
    [--preflight] \
    --handshake-timeout-ms 30000
```

`--preflight` runs the bounded concurrent listener/dialer preflight
and exits `0` on a passing preflight, `5` on a blocked preflight, `6`
on a failed forward probe, `2` on invalid inputs. The wrapper
refuses every release/support profile flag and accepts only the two
i2pd directions.

The first instrumented forward attempt on this host reached
`listener_ready` and started the i2pr dialer, then the i2pr dialer
rejected the i2pd RouterInfo with `peer_router_info_invalid` before
any TCP connection (the i2pd direct driver's emitted `router.info`
carried zero `RouterAddress` entries). Plan 090 closed that defect
with four behavior-neutral corrections; after the corrections the
i2pd listener authenticated and the i2pr dialer reached TCP, but
the NTCP2 Noise handshake closed the socket with `Io(ExactIoError
{ kind: Closed })` before the i2pr initiator reached
`ntcp2_authenticated`. The Plan 087 closure record is
`plans/087-status.md`. **The forward direction did not pass.**
Per the Plan 090 "Forward attempt reaches TCP and fails protocol"
branch, the failed record is preserved and Plan 088 is not allowed
to run until the forward direction passes.

The Plan 088 development decision vocabulary is exactly five values:
`two-way-development-probe-passed`, `one-way-passed-reverse-defect`,
`ambiguous-reference-divergence`, `manual-isolated-fallback-required`,
`insufficient-evidence`. Only `two-way-development-probe-passed` may
unblock Plan 079; only `ambiguous-reference-divergence` may activate
Plan 072. The historical `lane-invalidated` and
`same-stage-two-way-i2pr-defect` tokens are forbidden by the static
boundary checker.

On this host the recorded decision is `insufficient-evidence`
because the Plan 087 forward direction recorded a pre-TCP rejection
owned by the i2pd direct driver and no real wire run has been
retained. The Plan 087 implementation surface is ready for a fresh
attempt against a fixed i2pd driver.

## Plan 077 constrained-host lane (closed)

On this host Docker is inaccessible, QEMU is absent, and only the
reduced-scope lane is available; Plan 078 remains blocked until a
full-runtime qualification record exists. See `plans/077-status.md`
and ADR 0024.

```text
bash scripts/interop/probe-constrained-host-lanes.sh
bash scripts/check-constrained-host-lane-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
```

## Plan 099 / 100 closed status (canonical)

The Plan 099/100 closed status is the authoritative terminal state.
The compact sanitized summary is preserved at
`target/interop/evidence/milestone-3/31521642090/plan099-summary.json`.

```text
plan_099 = closed-protocol-defect-localized
plan_100 = passed-exit-cleanup-and-handoff
plan_095 = historical-superseded-by-plan099-single-job-lane
plan_087 = historical-development-sequence-superseded-by-plan100
plan_088 = historical-development-sequence-superseded-by-plan100
plan_079 = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = next
development_interop = protocol-defect-localized
exact_wire_stage = noise_authenticated
external_netdb_over_ntcp2 = blocked
```

The active development interop surface is small and bounded (see
the **Authoritative terminal state** section above). The focused
local seam is sufficient for routine development. The full historical
plan-specific Python matrix, rootless checker, Multipass checker,
and release-certificate validator are not required for Plan 100
closure; they remain available via git history for forensic
archaeology.

## Plan 102 (Milestone 4) — active authority

The active Milestone 4 authority is
[Plan 102](../../plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md).
The retained Plan 099/100/101 NTCP2 result is preserved as the
authoritative NTCP2 development record. The next substantial product
work is governed by Plan 102 and its child sequence (Plans 103 →
104 → 105 → 106).

The
[Plan 102 amendment](../../plans/102-amendment-exploratory-tunnel-dependency.md)
corrects an over-optimistic wording in the first Plan 102 draft: the
current I2P `DatabaseLookup` operation uses an outbound exploratory
tunnel and requests the response through an inbound exploratory
tunnel; exploratory tunnels are Milestone 5 scope. Therefore a
standards-conformant live RouterInfo lookup cannot complete inside
the Plan 103–106 implementation sequence merely by re-entering NTCP2
or another direct router transport.

Plan 106 closes the local/bootstrap implementation phase, not the
complete original Milestone 4 exit criteria. After Plan 106 closes,
Milestone 4A is `local-foundation-complete-external-transport-blocked`
until Milestone 5 supplies exploratory inbound/outbound paths and a
router transport is deliberately qualified. A direct `DatabaseLookup`
over NTCP2 is not accepted as a substitute for the standard
exploratory-tunnel path. The next executable implementation remains
**Plan 103** (RouterInfo validation and local NetDB foundation).

## Files to inspect

- `tests/integration/ntcp2/references.lock.toml` — Ubuntu contract,
  source pins, build commands, exact IzPack SHA-256.
- `tests/integration/ntcp2/scenarios/*.toml` — the bounded i2pr/
  reference scenario definitions. Keep their IDs synchronized with
  `tests/integration/ntcp2/manifest.toml`.
- `tests/integration/ntcp2/reference-scenarios/` — Plan 041 pair
  schema and the two directional Java I2P / i2pd control scenarios.
- `tests/integration/ntcp2/mixed-scenarios/` — the four Plan 044
  directional i2pr/reference scenarios.
- `tests/integration/ntcp2/harness/` — Python topology, adapters,
  process bounds, runner, evidence, mixed-runner, launcher renderer,
  data-phase oracle, reference-trigger, rootless supervisor, and
  multipass code.
- `scripts/interop/` — host setup, builders, isolation, matrix, gate
  staging, aggregate, cleanup.
- `scripts/check-ntcp2-interoperability.sh`,
  `scripts/check-fixture-manifest.sh`,
  `scripts/check-ntcp2-vectors.sh` — static gate checkers.
- `tools/i2pr-interop/` — non-production launcher seam.
- `target/interop/evidence/` — sanitized records only; gate-prefixed
  files live alongside `run-manifest.json`. `target/interop/runs/` is
  secret-bearing and is deleted after every run.

## Development rules

Keep production ownership boundaries intact: runtime owns Tokio tasks
and sockets; transport contracts remain runtime-neutral; the launcher
crate under `tools/i2pr-interop` is a non-production seam and must not
activate the daemon. Add negative-path tests for new configuration,
topology, process, parser, or evidence behavior. Prefer deterministic
local checks and never add raw network fixtures or secrets.

Before handoff, run from the repository root:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh        # when I2NP fixture bytes change
bash scripts/check-ntcp2-vectors.sh           # when NTCP2 vector bytes change
bash scripts/check-ntcp2-interoperability.sh  # when ntcp2 evidence/manifest change
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
```

The focused local seam (sufficient for routine development):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
git diff --check
```

Record commands, results, host constraints, and any blocked stop
condition in a closure record; do not report a blocked profile as a
passing interoperability result.

## Per-plan record index (for historical context)

The closed historical surface lives in `plans/` as audit records,
not as live contracts. When a closure record and a per-plan narrative
disagree, the closure record wins. The complete per-plan authority
table:

| Plan | Status | Role |
| --- | --- | --- |
| 038 | passed-harness-boundary | Ubuntu 24.04 amd64 harness |
| 040 | closed-apparatus | strict apparatus contract |
| 041 | closed-reference-pair | reference-only controls |
| 043 | closed-build-system-gates | build-system gate order |
| 044 | closed-mixed-runner | four-direction composition |
| 045 | closed-mixed-router-proof | directional predicate |
| 046 | blocked-host-negative-baseline | rootless sealed-namespace lane |
| 048 | qualified-recovery-guest | Multipass recovery lane |
| 049 | closed-lifecycle-ownership | atomic lifecycle reservation |
| 050 | closed-cloud-init-taxonomy | cloud-init classification |
| 051 | closed-dispatch-gate-bridge | dispatch-gate troubleshooting |
| 052 | closed-evidence-closure-follow-up | evidence bundle integration |
| 053 | closed-diagnostic-lane | local diagnostic bundle |
| 054 | closed-java-observation-qualification | Java observation catalog |
| 055 | closed-reference-trigger-qualification | reference-initiated triggers |
| 058 | closed-candidate-integrity | candidate retirement |
| 059 | blocked-java-support-topology-rejected | Java support topology rejected |
| 060 | declared-not-executable | two-run certificate |
| 062 | passed-evidence-contract-correction | trigger/observation v3/v4 |
| 063 | blocked-host-environment | Java stripped-router driver |
| 064 | blocked-host-environment | i2pd direct driver |
| 065 | passed-canonical-integration | strict scenario v2 |
| 066 | declared-not-executable | two-run closure |
| 067 | superseded-by-plan068 | staged roadmap |
| 068 | passed-staged-evidence-authority | four-tier evidence ladder |
| 069 | historical-scaffolding | host-loopback runner scaffolding |
| 075 | passed-runner-integrity | reference-process integrity |
| 076 | passed-real-i2pd-driver | real pinned i2pd library + driver |
| 077 | closed-constrained-host-lane | constrained-host lane order |
| 080 | closed-multipass-qualification | Multipass qualification |
| 081 | superseded-by-plan082 | corrective roadmap |
| 082 | passed-i2pr-state-preparation | pre-protocol preparation |
| 083 | implementation-landed-execution-pending | minimal i2pr-to-i2pd probe |
| 084 | implementation-landed-execution-pending | minimal i2pd-to-i2pr reverse probe |
| 085 | passed-host-loopback-roadmap | host-loopback execution roadmap |
| 086 | passed-host-loopback-lane-enablement | host-loopback lane |
| 087 | forward-attempt-failed | forward direction pre-TCP rejected |
| 088 | insufficient-evidence | reverse direction never started |
| 090 | passed-i2pd-routerinfo-correction | i2pd driver corrections |
| 091 | historical-partial-correction | i2pd Noise-handshake preconditions |
| 092 | superseded-by-plan093 | forward-handshake evidence integrity |
| 093 | implementation-landed-closure-incomplete | data-phase closure |
| 094 | implementation-landed-live-closure-blocked | live-closure environment blocked |
| 095 | historical-superseded-by-plan099-single-job-lane | CI live-wire lane |
| 096 | passed-pre-dispatch-workflow-correction | workflow correctness |
| 097 | passed-artifact-path-and-cleanup-correction | artifact path + cleanup |
| 098 | passed-runner-provenance-boundary-correction | runner/provenance boundary |
| 099 | closed-protocol-defect-localized | retained NTCP2 result |
| 100 | passed-exit-cleanup-and-handoff | exit-gate cleanup |
| 101 | passed-daemon-activation-safety | daemon NTCP2 disabled |
| 102 | active-authority | Milestone 4 RouterInfo/NetDB |
| 103 | passed-routerinfo-foundation | local NetDB foundation |
| 104 | passed-persistent-cache-reseed | persistent cache + SU3 |
| 105 | passed-transport-neutral-state-machines | transport-neutral lookup/store/publication |
| 106 | passed-daemon-bootstrap-integration | daemon bootstrap integration |
| 107 | passed-exploratory-tunnel-substrate | exploratory tunnel substrate |
| 108 | passed-live-ecies-short-build | ECIES-X25519 short build |
| 109 | superseded-by-plan111 | short-build record + Noise |
| 110 | superseded-by-plan111 | multi-record STBM/OTBRM |
| 111 | passed-final-local-short-build-conformance | short-build conformance |
| 112 | passed-outbound-pre-delivery-closure | outbound short-build closure |
| 113 | passed-inbound-reference-reconciliation | inbound reconciliation |
| 114 | passed-terminal-routing-chain-correction | terminal routing + chain |
| 115 | passed-emissary-q0-construction-and-obep-reply-only | Q0 + native OBEP reply |
| 116 | passed-final-local-closure | local tunnel data plane |
| 117 | closed-for-progression-with-evidence-gap | exploratory NetDB composition |
| 118 | passed-planning-authority-cleanup | planning authority |
| 119 | passed-leaseset2-protocol-foundation | Standard LeaseSet2 carrier |
| 120 | passed-destination-lifecycle-and-pools | local destination runtime |
| 121 | superseded-by-plan126 | ECIES destination session (old dialect) |
| 122 | passed-corrected-local-destination-routing | destination routing pipeline |
| 123 | superseded-by-final-corrective-closure | streaming core (Plan 125 supersedes) |
| 124 | passed-plan122-corrective-closure | garlic-through-OBEP composition |
| 125 | superseded-by-final-corrective-closure | streaming corrective |
| 126 | passed-ecies-destination-ratchet-corrective-foundation | normative ECIES-X25519-AEAD-Ratchet |
| 127 | passed-destination-session-routing-final-closure | destination-session routing |
| 128 | passed-streaming-wire-protocol-corrective-closure | Streaming wire format |
| 129 | superseded-by-plan130-final-gate | integrated destination+Streaming gate |
| 130 | superseded-by-plan131-final-local-correctness-gate | M6 wire/runtime |
| 131 | superseded-by-plan132-and-plan133-final-gates | M6 local correctness |
| 132 | implementation-landed-evidence-superseded-by-plan133 | M6 transactional send |
| 133 | passed-evidence-authority-superseded-by-plan134 | M6 evidence authority |
| 134 | passed-milestone6-recv-window-ack-ceiling-closure | M6 receive-window ACK ceiling |

## Cross-references

- [`AGENTS.md`](../../AGENTS.md)
- [`README.md`](../../README.md)
- [`docs/architecture/interop-apparatus.md`](../../docs/architecture/interop-apparatus.md)
- [`docs/architecture/tooling.md`](../../docs/architecture/tooling.md)
- [`specs/support.toml`](../../specs/support.toml)
- [`tests/integration/ntcp2/README.md`](../../tests/integration/ntcp2/README.md)
- [`.opencode/skills/i2pr-rootless-sandbox/`](../i2pr-rootless-sandbox/)
- [`.opencode/skills/i2pr-multipass-recovery/`](../i2pr-multipass-recovery/)
- [`.opencode/skills/i2pr-local-dev/`](../i2pr-local-dev/)
- [`.opencode/skills/i2pr-architecture/`](../i2pr-architecture/)
