---
name: i2pr-ntcp2-interop
description: Operate, diagnose, or extend the repository's Plan 038/040/041/043/044/045/052/053/054/055/058/059/062/063/064/065/067/068/074/075/076/077/078/080/081/082/083/084/085/086/087/088/090/091/092/093/094/099/100 host-side Ubuntu 24.04 reference-router NTCP2 interoperability harness (Plan 099/100 closed as protocol-defect-localized; Plan 095 historical). Plan 101 corrects the daemon NTCP2 activation boundary; normal-daemon NTCP2 is disabled. Plan 115 Emissary Q0 construction + native OBEP reply has passed locally with the canonical production I2NP bridge (`ShortBuildI2npBridge` in `crates/i2pr-tunnel/src/bridge.rs`); Q1/Q2/qualified external delivery still pending. Plan 117 closed per Plan 118 as `closed-for-progression-with-evidence-gap` (local Phase G passed; corrected native Emissary reference test rejected the pinned reference's request-prefixed reply during strict i2pr Mapping decoding). The harness includes the Plan 085-088 host-loopback development execution roadmap (historical), the Plan 090 i2pd RouterInfo and pre-TCP classification correction, the Plan 091 i2pd Noise-handshake preconditions, the Plan 092 forward-handshake evidence integrity (superseded by Plan 093), the Plan 093 Plan 087 forward data-phase and reference-observer closure, the Plan 094 Plan 093 completion pass and Plan 087 -> Plan 088 handoff, the Plan 099 Milestone 3 interop exit and router buildout, the Plan 100 one-time exit-gate cleanup and router handoff. Use when an agent is asked to read or reproduce the historical harness surface, run a bounded interop profile, prepare or validate reference routers, add or modify a scenario, or validate evidence. The active development interop lane is closed; NTCP2 remains experimental and non-advertised. Do not activate Plan 072 or build a general Emissary lane unless Plan 088 records `decision = ambiguous-reference-divergence` with one exact wire-stage question. Plan 119 closed as `passed-leaseset2-protocol-foundation`; Plan 120 closed as `passed-destination-lifecycle-and-pools` and lands the first `i2pr-client` destination runtime (local identity, dedicated tunnel pools, signed Standard LeaseSet2, lifecycle, bounded local payloads, registry). The next executable product plan is Plan 121 (ECIES-X25519-AEAD-Ratchet Garlic session layer). External acceptance debt is tracked separately under the Milestone 6 roadmap. The companion skills `i2pr-rootless-sandbox` and `i2pr-multipass-recovery` cover the Plan 046 sealed-namespace lane and the Plan 048/049/050/051 recovery lane.
---

# I2PR NTCP2 Interoperability (host harness, Plans 038/040/041/043/045/055/056/058/059/081/082/083/084)

## Current tunnel-build handoff boundary

Plan 112 is closed as `passed-outbound-pre-delivery-closure`, Plan 113 is
closed as `passed-inbound-reference-reconciliation`, and Plan 114 is
closed as `passed-terminal-routing-chain-correction` for the
runtime-neutral ECIES-X25519 short-build surface. Outbound payloads are
locally conformant, inbound construction is locally reference-compatible
under the explicitly named `reference-compatible-spec-text-discrepancy`
policy, and the terminal routing metadata and intermediate tunnel-id chain
continuity are validated at both the high-level `ShortBuildPath::validate()`
boundary and the public lower-level `prepare_short_build_message()` entry
point: the real request keeps fixed fields + Mapping/padding and exactly
one originator fake carries
`hash16 || fresh X25519 pub32 || random remainder`. The final-spec prose is
not claimed as strictly conformant for that unresolved semantic. This skill
must not present local results as live interoperability. This NTCP2 skill does
not own that tunnel-build implementation and must not activate NTCP2, add a
generic I2NP dispatcher, or use a local/vector/testkit result as
reference-router evidence.

Plan 115 Emissary Q0 construction + native OBEP reply has passed
locally against pinned Emissary revision
`9b43484a21d5a1291c4881cdae62a36c527f8c0f` (emissary-core 0.4.0).
The Q0 test was added as a new `#[tokio::test]` module inside
Emissary's `tunnel/tests/mod.rs`; `i2pr-proto` and `i2pr-tunnel`
were added to Emissary's `[dev-dependencies]` only in the
temporary combined worktree at
`/tmp/opencode/plan115-q0/combined/emissary-core-pkg/`. Plan 115
Branch E (closed-no-bounded-independent-consumer-seam) is
preserved as historical context in `plans/115-status.md`. Q1 (authenticated
transport delivery) and Q2 (reply round-trip to `Established`)
remain pending. The full Plan 115-117 acceptance
(Q0 + Q1 + Q2 + qualified external delivery) is not yet complete.

## Plan 117 terminal disposition (closed for progression with evidence gap)

Plan 117 closed per Plan 118 as
`closed-for-progression-with-evidence-gap`. The local Phase G
production composition remains passed, and the `DataPlaneRegistry`
binds inbound roles to both `TunnelSlot` and local receive
`TunnelId`. `remove_slot(slot)` removes outbound or inbound roles
and all reverse metadata atomically without cloning `LayerKeys`.

The corrected reference attempt used a fresh temporary checkout of
`eepnet/emissary@9b43484a21d5a1291c4881cdae62a36c527f8c0f` and
placed its test inside `emissary-core`'s own `#[cfg(test)]` build.
Only temporary dev-deps and a narrow test-only pre-Garlic reply
observer were used; no permanent Emissary adapter, sockets,
namespaces, or a normal-daemon transport path was added. The
corrected attempt reached native OBEP admission and opened the
reply AEAD with i2pr-derived context, but strict i2pr Mapping
decoding rejected the pinned reference's request-prefixed reply
plaintext. The reference-side defect is localized to the pinned
Emissary revision, and Plan 118 Phase B1 confirmed no usable
upstream correction exists.

The authoritative terminal state is:

```text
plan_117_local_composition           = passed-all-i2pr-production-seam-netdb
plan_117_native_reference            = blocked-reference-defect
plan_117_external_transport          = deferred-host-lane-unavailable
plan_117                             = closed-for-progression-with-evidence-gap
plan_119                             = passed-leaseset2-protocol-foundation
router_construction                  = may-continue
next_router_construction_plan        = Plan 121 (ECIES-X25519-AEAD-Ratchet Garlic session layer)
```

Do not relax the parser or promote parser-only evidence to native
mixed-router NetDB evidence. Publication, lookup, and inbound
return stages remain unproven on this host. The authenticated
transport lane remains separately deferred and NTCP2 stays
experimental/non-advertised. Authority:
`plans/117-status.md`,
`plans/117-handoff.md`,
`plans/118-planning-authority-cleanup-and-plan117-disposition.md`,
`plans/119-status.md`.

Plan 119 closed as `passed-leaseset2-protocol-foundation`. The
ordinary online-signed published Standard LeaseSet2 carrier is
wired into `i2pr-proto` (40-byte `Lease2`, `LeaseSet2Header`,
`LeaseSet2EncryptionKey`, canonical `Mapping` options, signature
domain `0x03 || signed_bytes`) and into `i2pr-netdb`
(`ValidatedLeaseSet2`, `LeaseSet2Store`, `DestinationHash`,
`LookupKind::LeaseSet2`). `DatabaseStoreData::LeaseSet2` replaces
the type-3 `Deferred` payload for the ordinary subset; types 5/7
remain explicitly deferred. EncryptedLeaseSet, MetaLeaseSet,
blinded, offline-signing, leased, and PQ-hybrid variants remain
future work tracked by the Milestone 6 roadmap. Plan 120 closed
as `passed-destination-lifecycle-and-pools` and lands the first
`i2pr-client` destination runtime (local identity, dedicated
tunnel pools, signed Standard LeaseSet2, lifecycle, bounded local
payloads, registry). The next executable plan is **Plan 121**
(ECIES-X25519-AEAD-Ratchet Garlic session layer) under the
Milestone 6 router-construction roadmap in
`plans/118-123-milestone6-router-construction-roadmap.md`.
External acceptance debt (Q1/Q2 authenticated external transport,
live exploratory tunnel pair, live NetDB publication/lookup, live
LS2 publication/lookup) is tracked separately under the same
roadmap's external acceptance debt ledger and does not block the
Milestone 6 product construction.

The inbound creator identity is explicit at the `ShortBuildPath` boundary;
the first remote hop is `InboundGateway`, later remote hops are `Participant`,
and the creator verifies the originator fake after reply processing. The
outbound reply-router identity is also explicit at the `ShortBuildPath`
boundary, the intermediate `hops[i].next_tunnel == hops[i+1].receive_tunnel`
chain invariant is enforced at both the high-level and the public
lower-level builder, and strict outbound/inbound E2E trajectories
deterministically reach `Established`. The evidence note is
`specs/references/short-build-inbound-creator-key.md`, the inbound closure
record is `plans/113-status.md`, the terminal-routing closure record is
`plans/114-status.md`, and the canonical production I2NP bridge closure
record is `plans/115-status.md`. The next short-build activity is an
external-delivery checkpoint on a host where the Plan 046 rootless
sealed-namespace lane or the Plan 048/049 Multipass recovery lane is
runnable; that future lane must consume the byte-correct
count-prefixed STBM payload through the Plan 115
`ShortBuildI2npBridge` and must not re-open the Plan 099-114
surface.

Use this skill from the repository root for the **host-side** Ubuntu 24.04
amd64 Plan 038 reference-router NTCP2 interoperability harness. This skill
intentionally does **not** cover the Plan 046 rootless sealed-namespace lane
or the Plan 048/049/050/051 Multipass recovery lane — load those companion
skills for those lanes.

Read `AGENTS.md`, `plans/038-ubuntu-reference-router-interoperability-harness.md`,
`plans/040-interop-apparatus-corrective-pass.md`,
`plans/041-reference-router-private-crosscheck.md`,
`plans/043-ubuntu-build-system-interop-gates.md`,
`plans/044-ntcp2-interop-final-integration-corrective-pass.md`,
`plans/045-ntcp2-mixed-router-proof-closure-corrective-pass.md`,
`plans/045-closure-attempt.md`, `plans/052-ntcp2-milestone-3-evidence-closure-follow-up.md`,
`plans/053-plan052-evidence-pipeline-integration-corrective-pass.md`,
`plans/054-java-startup-and-reference-observation-qualification-pass.md`,
`plans/055-reference-initiated-ntcp2-trigger-and-topology-qualification-pass.md`,
`plans/056-ntcp2-milestone-3-two-run-external-evidence-closure-pass.md`,
`plans/058-plan056-record-and-candidate-integrity-closure-pass.md`,
`plans/058-status.md`,
`plans/059-reference-side-implementation-and-live-qualification-closure-pass.md`,
`plans/059-status.md`,
`plans/074-milestone-3-real-driver-and-constrained-host-corrective-roadmap.md`,
`plans/075-plan-069-runner-integrity-and-evidence-correction.md`,
`plans/075-status.md`,
`plans/076-real-pinned-i2pd-library-and-direct-driver-construction.md`,
`plans/076-status.md`,
`plans/081-milestone-3-pre-protocol-and-minimal-i2pd-corrective-roadmap.md`,
`plans/082-i2pr-state-preparation-and-mixed-runner-contract-correction.md`,
`plans/083-minimal-i2pr-to-i2pd-ntcp2-wire-probe.md`,
`plans/084-i2pd-to-i2pr-reverse-probe-and-development-decision.md`,
`plans/072-activation-amendment-plan-084.md`,
`tests/integration/ntcp2/README.md`, and the relevant `docs/adr/` records before changing the harness.

The canonical reference identifiers are `java_i2p` and `i2pd`. Locked source
objects: Java I2P `2800040deee9bb376567b671ef2e9c34cf3e30b6` and i2pd
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`. Abbreviated revisions are not
valid cache or evidence inputs.

## Companion skills (load before doing this lane)

- `i2pr-rootless-sandbox` — Plan 046 host-side rootless sealed-namespace lane.
- `i2pr-multipass-recovery` — Plan 048/049/050 Multipass recovery lane
  (atomic lifecycle, cloud-init taxonomy, base verify, four Plan 045 directions,
  sanitized export, selective purge, and the Plan 051 dispatch-gate
  troubleshooting bridge).

If the host emits `blocked_unprivileged_user_namespace` from the Plan 046
probe, do not try to recover inside Plan 038. Hand off to
`i2pr-multipass-recovery`.

## Safety boundary

Treat the harness as experimental infrastructure, not an anonymity or security
tool. **Never** enable `i2pr-daemon`, use public egress, perform DNS / bootstrap
/ reseed, retain identities/keys/RouterInfo/raw logs/packet captures, or turn
a local self-handshake, loopback run, vector, or testkit result into Java I2P
or i2pd interoperability evidence. Keep support rows experimental and
non-advertised unless sanitized evidence satisfies `specs/CONFORMANCE.md`.

Run only on an authorized disposable Ubuntu 24.04 amd64 host. The namespace
and firewall checks are mandatory and fail closed. Do not bypass a host,
privilege, route, cleanup, or evidence validation error.

The exact host contract is Ubuntu 24.04 amd64/x86_64, Bash 4+, UTF-8 locale,
non-interactive `sudo` when not root, Linux namespace/nftables capability,
and ≥4 GiB free under `target/`. Declared package set and locked source,
IzPack, cache, and build-command inputs are authoritative in
`tests/integration/ntcp2/references.lock.toml`.

## Plan 042 runtime and launcher boundary

The NTCP2 wire driver is a runtime-owned composition. `i2pr-runtime` owns
Tokio sockets and tasks, action deadlines, cancellation, replay/admission,
authenticated frame state, bounded queues, and child joins. The
`i2pr-transport-ntcp2` state machines remain runtime-neutral and receive only
complete bounded actions. `tools/i2pr-interop` is a non-production launcher
seam: it validates bounded non-secret scenario input and composes the runtime
driver, but it must **never** activate `i2pr-daemon`.

The launcher status protocol has separate meanings. A completed `listen` emits
listener readiness and then a distinct authenticated terminal result; `dial`
emits one terminal typed result; and `inspect` emits redacted state metadata.
Listener readiness is not authentication.

Plan 042 selects the existing fixed-size DeliveryStatus message (I2NP type 10)
for the first data smoke: 12-byte body, 21-byte NTCP2/SSU2 short transport
encoding, and 24-byte NTCP2 block before frame overhead and padding. A
positive gate requires one authenticated outbound and one authenticated
inbound DeliveryStatus per direction plus orderly cleanup. Reference acceptance
or echo behavior is not yet verified; do not claim interoperability or
substitute padding/TCP readiness for the message exchange.

## Plan 052 evidence closure constraints

Plan 052 is the corrective execution plan for closing Milestone 3. It
supersedes Plan 045 for closure purposes and introduces the following
non-negotiable constraints:

- **Single-source provenance.** Every artifact binds to one exact 40-char
  source commit recorded in `run-identity.json`. Short SHAs, dirty
  trees, archive/manifest mismatches, and non-finalized run identities
  are typed blockers (`tests/integration/ntcp2/harness/run_identity.py`).
- **Tri-state diagnostics.** The prior `I2PR_INTEROP_DUMP_RUN_LOGS`
  switch is replaced by `I2PR_INTEROP_DIAGNOSTICS=off|sanitized|raw-local`.
  `raw-local` is forbidden under any export root
  (`tests/integration/ntcp2/harness/mixed_runner.py:_diagnostics_mode`).
- **Typed observation schema v2.** Per-side observations use
  `i2pr-ntcp2-direction-observation-v2` with bounded levels. A passed
  direction requires both-side `ntcp2_authenticated`, sender
  `frame_emitted`, receiver `frame_authenticated_and_decrypted` AND
  `i2np_message_decoded` (`tests/integration/ntcp2/harness/observation.py`).
- **Atomic evidence bundles.** Each Milestone 3 run produces
  `target/interop/evidence/milestone-3/<run-id>/` with `run-identity.json`,
  an `environment/` block, per-direction `attestations/`, `directions/`,
  `triggers/`, `observations/`, and `cleanup/` records, a
  `diagnostics/sanitized-summary.json`, and a sanitized manifest
  (`tests/integration/ntcp2/harness/evidence_bundle.py`).
- **Java startup probe.** Standalone at
  `tests/integration/ntcp2/harness/java_startup_probe.py`; it isolates
  Java startup from i2pr and NTCP2 and never asserts an interoperability
  result.
- **Reference-trigger contracts.** Source-inspection record at
  `tests/integration/ntcp2/reference-trigger-contracts.md`; until the
  helpers are committed, the two reference-initiated directions remain
  typed blockers.

A Plan 052 evidence bundle closes Milestone 3 only when (a) it contains
exactly the four primary direction records, (b) every record binds to
the same run identity, (c) every record satisfies the v2 observation
predicate, and (d) two complete reproducible runs exist. Anything less
remains a typed diagnostic result.

## Plan 053 integrated diagnostic lane

Plan 053 is the local integration corrective pass. The canonical path uses
`tests/integration/ntcp2/harness/plan052_pipeline.py` to measure one clean
source identity before directions, copy that identity into immutable staging,
write all five artifact classes for every primary direction, and finalize with
`verify_bundle()` before any export. The explicit context arguments are
`--run-id`, `--run-identity`, `--bundle-staging`, and
`--evidence-profile milestone-3-v2`; they must not be inferred from the current
working directory. Blocked or rejected directions still produce complete
records. Their result is `diagnostic-complete-not-certificate`, never a
Milestone 3 pass.

The bundle verifier rejects unknown paths/schemas, traversal, symlinks,
non-regular files, hidden temporary files, duplicate/case-colliding paths,
manifest checksum errors, and mutations after finalization. Export writes the
acknowledgement beside `milestone-3/<run-id>/`, not inside the immutable bundle.
The current lane has no Java/i2pd source-locked receiver markers, so missing
reference data-phase observations remain typed rejections.

Use the focused local seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
bash scripts/check-ntcp2-interoperability.sh
```

Do not present the diagnostic bundle, reference-only controls, or a blocked
rootless/Multipass run as NTCP2 interoperability evidence.

## Plan 054 Java startup and reference-observation qualification

Plan 054 is the active local qualification pass for the Plan 052
directional predicate. It introduces the 16-cell Java startup matrix
(`tests/integration/ntcp2/harness/java_matrix.py`), the frozen
template lifecycle (`scripts/interop/java-prepare-template.py` and
the `seeded-clone` data state), and the machine-readable reference
observation catalog
(`tests/integration/ntcp2/reference-observation-catalog.toml`,
schema `i2pr-reference-observation-catalog-v1`). The Java and i2pd
adapters expose `collect_observation(role, run_id, correlation,
log_cursor, catalog)` and return finalized
`i2pr-ntcp2-direction-observation-v2` records. `mixed_runner.py` no
longer hardcodes `reference-receiver-marker-not-source-locked`; the
Plan 052 predicate now consumes the live observation. The
`plan052_pipeline._build_observation` accepts the live `i2pr_observation`
and `reference_observation` records; the synthetic builder remains as
the typed fallback for blocked and rejected directions.

Do not run a complete external matrix or qualification on a host that
lacks the pinned Java 2.12.0 and i2pd 2.60.0 references. The
Plan 046 host is the negative baseline; the Plan 048/049 Multipass
recovery lane is the canonical external path. A local
`diagnostic-complete-not-certificate` bundle is not Milestone 3
closure. NTCP2 remains experimental and non-advertised.

Use the focused local seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 055 reference-initiated trigger and topology qualification

Plan 055 is the qualification pass for the two reference-initiated
directions (`java-to-i2pr-ipv4` and `i2pd-to-i2pr-ipv4`). It owns:

- The locked trigger record schema `i2pr-reference-trigger-v3` in
  `tests/integration/ntcp2/harness/trigger_record.py`, with the
  bounded `TriggerHelperKind` (`i2pd-direct-helper`,
  `java-direct-helper`, `java-minimal-support-topology`) and
  `TriggerOutcome` enumerations required by Plan 055 A2. The
  helper build provenance, attempt count, correlation nonce,
  bounded monotonic timestamps, and target RouterInfo / public
  NTCP2 static-key digests are all bound into the canonical
  trigger digest.
- The source-inspection record at
  `tests/integration/ntcp2/reference-trigger-contracts.md`. Plan
  055 B5 selected the i2pd direct helper against
  `i2pd::transports::Transports::ConnectToPeer`; Plan 055 C5
  recorded the Java decision
  `java-direct-helper-rejected-global-context-not-isolatable`
  because the pinned Java 2.12.0 outbound path requires a full
  `RouterContext` and NetDB population.
- ADR 0021 (`docs/adr/0021-minimal-java-support-topology.md`)
  authorizes the optional `java-minimal-support-topology` fallback
  when a direct helper is impossible; the ADR must be approved
  before any topology-assisted helper is implemented.
- The Plan 052/053 pipeline
  (`plan052_pipeline.write_direction_artifacts`) binds the
  trigger record digest, correlation nonce, and target RouterInfo
  hash into the direction record (Plan 055 E2). A successful
  trigger outcome may not mask a rejected direction; the
  bounded i2pr responder reason from the Rust launcher is
  preserved (Plan 055 E3).

The Plan 055 helpers and the support topology live in the Plan 046
rootless sealed-namespace lane or the Plan 048/049 Multipass
recovery lane; this host is the negative baseline. The two
reference-initiated directions remain typed blockers until Plan 056
produces two complete reproducible bundles. Plan 056 closed with a
typed host-environment blocker; the canonical two-bundle certificate
verifier, the candidate freeze, and the local diagnostic bundles
are locally generated under the ignored
`target/interop/evidence/plan056/` working directory. Plan 058
retired the Plan 056 candidate, superseded Plan 057, decided
ADR 0021 (Rejected), and split the previous Plan 057 follow-up
into Plan 059 (reference-side implementation) and Plan 060
(fresh candidate + two-run certificate). Plan 059 closes with the
typed blocker `blocked_java_support_topology_rejected` because the
Java support topology is forbidden; the
`java-to-i2pr-ipv4` direction remains a typed blocker for the
pinned Java I2P 2.12.0 revision. Plan 060 cannot start under the
current four-direction contract until the Java reference-initiated
direction is qualified through a future ADR or the four-direction
contract is revised.

The only tracked footprint of the local diagnostic effort is the
bounded local-diagnostic receipt at
`tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`
with `artifact_storage = local-untracked`. The repository does
not track `target/interop/evidence/plan056/`.

## Plan 059 reference-side implementation and live qualification closure pass

Plan 059 implements the i2pd direct helper, the per-reference
observation qualification receipts, and the canonical pipeline
live-mode wiring that Plans 055-057 deferred. ADR 0021 was
Rejected by the Plan 058 record and candidate integrity closure
pass, so Plan 059 closes with the typed blocker
`blocked_java_support_topology_rejected` and the
`java-to-i2pr-ipv4` direction remains blocked for the pinned
Java I2P 2.12.0 revision. Plan 060 cannot start under the
current four-direction contract.

The plan delivered:

- `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
  — the i2pd direct helper source (`i2pd_direct_connect.cpp`),
  the bounded local Python driver (`i2pd_direct_connect.py`),
  the CMake build contract (`CMakeLists.txt`), the source-lock
  record (`source-lock.json`), and the helper README. The C++
  helper links against the pinned i2pd 2.60.0 libraries and
  exercises the documented `Transports::SendMessage` call graph.
- `tests/integration/ntcp2/reference-observation-qualification/`
  — the per-reference qualification receipts and summary. Both
  pinned references carry `qualified = false` for every semantic
  level until the Plan 046 rootless sealed-namespace lane or the
  Plan 048/049 Multipass recovery lane exercises the runtime
  controls. The Java receipt carries the
  `blocked_java_support_topology_rejected` blocker because ADR
  0021 is Rejected.
- `tests/integration/ntcp2/harness/plan059.py` — the Plan 059
  helper module that loads the source-lock record, the
  qualification receipts, and the qualification summary, and
  exposes `i2pd_helper_invocation` for the test matrix.
- `tests/integration/ntcp2/harness/plan052_pipeline.py` — the
  canonical Plan 052/053 pipeline now accepts a `live_mode` flag
  and binds helper, source, catalog, and qualification-receipt
  digests into the direction record. Live mode rejects the
  synthetic fallback for passed reference-initiated directions.
- `tests/integration/ntcp2/harness/test_plan059.py` — the Plan 059
  test matrix (36 cases: i2pd helper, Java support-topology gate,
  receiver observations, Java startup gate, pipeline live mode).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce the
  Plan 059 artifacts, the test matrix coverage, the canonical
  pipeline live-mode enforcement, and the ADR 0021 rejection.

Use the focused local seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan055.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 scripts/interop/plan056_drive_bundles.py --repo-root . \
    --run-a-id <plan056-a-id> --run-b-id <plan056-b-id> \
    --evidence-root target/interop/evidence/plan056
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 060 fresh-candidate and two-run Milestone 3 certificate closure pass

Plan 060 is the execution-only pass that cuts one fresh candidate
after Plan 058 and Plan 059 close, selects exactly one execution
lane (direct-host or guest), runs the four primary IPv4 mixed-router
directions twice on independent mutable state, and produces a
verified Milestone 3 certificate over the two sanitized bundles.

The plan cannot start under the current four-direction contract
until either a future pinned Java revision is adopted or the
closure contract is revised through a new ADR (ADR 0021 is
Rejected by Plan 058). The Plan 046 rootless sealed-namespace
lane returns `blocked_unprivileged_user_namespace` on this host;
the Plan 048/049 Multipass recovery lane is the canonical external
path but cannot complete on this constrained host (per Plan 051).
Plan 060 therefore closes on this host with the typed environment
blocker `blocked_execution_lane_unavailable`; the candidate is
`declared-not-executable`.

The plan delivered:

- `tests/integration/ntcp2/harness/plan060.py` — Plan 060 helper
  module. Exports `plan060_typed_blocker() ->
  "blocked_execution_lane_unavailable"`, `plan060_close_status()
  -> "declared-not-executable"`, `execution_lane_lock(...)` for
  the Plan 058 two-lane contract, `candidate_record_digests()`
  for the bounded digest table, `freeze_readiness_report()` for
  the freeze-readiness checklist,
  `assert_plan060_freeze_invariants()` for the typed blocker
  enforcement, and `plan060_two_bundle_independence(...)` for the
  cross-run independence rules.
- `tests/integration/ntcp2/harness/test_plan060.py` — Plan 060
  test matrix (35 cases across the Plan 060 surface).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 060 artifacts, the Plan 060 test matrix coverage, and
  the candidate/closure marker invariants.
- `plans/060-candidate.md` — Plan 060 candidate record. Status
  `declared-not-executable`. Implements the executed source
  commit, the implementation floor, the bounded digest table,
  the lane lock, the typed blockers, and the schema marker.
- `plans/060-closure.md` — Plan 060 closure record with the
  typed blocker and the close-status.

Use the focused local seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan060.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

A future pinned Java revision that exposes a transport-only direct
seam may trigger an ADR re-issue that supersedes the ADR 0021
rejection and unblocks the `java-to-i2pr-ipv4` direction. Until
Plan 060 produces two passing bundles from a fresh
implementation-floor candidate, NTCP2 stays experimental and
non-advertised and Milestone 3 stays open.

## Plan 067 staged interoperability corrective roadmap

Plan 067 is the **active** Milestone 3 corrective roadmap. Plan 067
supersedes Plan 066 as the active execution authority. Plan 066
remains an immutable historical record of the unavailable
release-qualification lane on the constrained host. Plan 067
corrects the active planning premise that release-grade isolation
and two-run certification are prerequisites for the first external
protocol test, and it corrects the stale active use of the ADR 0021
Java support-topology rejection after ADR 0022 accepted the direct
Java stripped-router driver.

Plan 067 separates NTCP2 interoperability evidence into four bounded
tiers:

- **Level 0 — local conformance.** Deterministic local protocol and
  runtime ownership.
- **Level 1 — external loopback smoke.** Two real processes on the
  host loopback. i2pd is the primary initial validator. Emissary is
  conditional. No rootless namespace, no Multipass, no candidate
  freeze, no two-bundle certificate, no reviewer record, no Java I2P
  required.
- **Level 2 — repeated development interoperability.** Both
  directions against the primary independent validator (pinned i2pd
  2.60.0), three fresh-state repetitions per direction, exact
  message and identity correlation, bounded negative controls.
- **Level 3 — release qualification.** Java I2P 2.12.0 and i2pd
  2.60.0, isolated no-public-egress lane, reproducible
  source/reference provenance, exact authenticated data-phase
  message correlation, independent fresh state, sanitized durable
  evidence. The Plan 066 certificate verifier may be reused at Level
  3.

Java and i2pd remain required for release qualification. NTCP2 stays
experimental and non-advertised.

## Plan 068 staged evidence and Milestone 3 authority correction

Plan 068 implements the staged-evidence and authority correction
that Plan 067 proposes. Plan 068 is the only path that produces the
Plan 030/066 active-status correction, the Plan 066 supersession
notice, the evidence-tier constants and tests, the smoke and
development-validation record schemas and tests, the
static-check-scope simplification for Plans 069-073, and the
documentation propagation.

Plan 068 delivers:

- `docs/adr/0023-staged-ntcp2-interoperability-evidence.md`
  (Accepted). ADR 0023 separates evidence into four bounded tiers
  and forbids lower-tier promotion into release bundles.
- `tests/integration/ntcp2/harness/evidence_tier.py` — the
  evidence-tier constants and tier-separation rules.
- `tests/integration/ntcp2/harness/loopback_smoke_record.py` — the
  Level 1 smoke record schema
  (`i2pr-ntcp2-loopback-smoke-v1`).
- `tests/integration/ntcp2/harness/development_validation.py` — the
  Level 2 development-validation summary schema
  (`i2pr-ntcp2-development-validation-v1`).
- The Plan 068 test matrices
  (`tests/integration/ntcp2/harness/test_evidence_tier.py`,
  `test_loopback_smoke_record.py`,
  `test_development_validation.py`,
  `test_plan068.py`).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce the
  new schema modules, the new test matrices, the ADR 0023
  acceptance marker, and the release-bundle smoke/development
  rejection.

Plan 068 also removes the stale
`blocked_java_support_topology_rejected` interpretation from the
active Java path: ADR 0021 remains Rejected and the Java support
topology remains forbidden, but the ADR 0022 direct Java driver is
the active Java architecture. Java may still be unavailable because
of host/runtime/build defects, but not because ADR 0021 forbids the
already accepted replacement architecture.

The focused closure baseline for Plans 069-073 is the touched-code
test suite plus `cargo fmt --all --check`, `cargo check --workspace
--all-targets`, `cargo test --workspace`,
`scripts/check-dependency-direction.sh`, and
`scripts/check-runtime-boundaries.sh`. Full historical harness
matrices, rootless checks, and Multipass checks remain available for
explicit integration checkpoints but are not required for Level 1
or Level 2 closures.

Use the Plan 068 focused seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_tier.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_development_validation.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan068.py'
```

The Plan 083 focused seam is:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'
bash scripts/check-ntcp2-interoperability.sh
```

## Plan 074 real-driver and constrained-host corrective roadmap (historical)

Plan 074 is historical execution authority. Plan 098 supersedes its active
sequence with **Plan 082 (implemented) → Plan 083 (implemented, execution pending) → Plan 084 (implemented, execution pending) → Plan 085 → Plan 086 → Plan 087 → Plan 090 → Plan 091 → Plan 092 (superseded) → Plan 093 (implementation-landed, closure-incomplete) → Plan 094 (implementation-landed, live-closure-environment-blocked) → Plan 095 (ci-live-wire-lane-corrected, awaiting one authoritative run) → Plan 096 (passed-pre-dispatch-workflow-correction) → Plan 097 (passed-artifact-path-and-cleanup-correction) → Plan 098 (passed-runner-provenance-boundary-correction) → one manual Plan 095 dispatch → Plan 088 → Plan 079 (blocked)**. Plans 075, 076,
077, and 080 are closed prerequisites or historical lane records. The Plan
084 historical `lane-invalidated` closure is reclassified as "runner
implementation completed; required reverse wire execution never occurred" and
the active development decision now lives in `plans/088-status.md`. Plan 094
remains implementation-landed; its live closure environment is blocked on
this host. Plan 095 is the single next executable plan and implements the
GitHub Actions `ubuntu-24.04` host-loopback live-wire evidence lane that
supersedes the local host environment-blocked path that Plan 094 expected
to run. Plan 095 closes Plan 094 without reopening its already-landed
NTCP2 data-phase design. The Plan 092 first
clean committed-head reproduction (`/tmp/opencode/plan092-test-1/forward-record.json`,
`record_sha256 = 696aa1339d3d950f9fec2a2e0b1f5bede2035761a71e167af6ab28b249cc998d`)
is preserved verbatim and superseded by Plan 093 — the i2pd NTCP2 transport
diagnostic is reclassified as data-phase length reader traffic rather than
a handshake-state defect. Plan 088 remains blocked until Plan 095 closes
with a passing instrumented forward record and a passing control forward
record from the same CI evidence pair.

Plan 096 is the active workflow correctness and pre-dispatch closure
pass. The plan delivers the four demonstrated workflow corrections
(explicit i2pr build path, disjoint sanitized evidence, embedded
Python import audit, canonical tracked-source digest), the pre-dispatch
audit script (`scripts/check-plan095-workflow.sh`), and the static
regression matrix (`tests/integration/ntcp2/harness/test_plan096.py`).
Plan 096 is the gating implementation pass before the first
authoritative Plan 095 live run.

Plan 097 is the active narrow corrective pass over the Plan 095
GitHub Actions workflow that closed two workflow defects that
remained after Plan 096: the producer/consumer artifact path
identity mismatch (one canonical absolute `BUILD_OUTPUT` path
used by every producer, verifier, manifest, uploader, and live
consumer) and the disposable run-root cleanup (strict `rm -rf --`
with an exact path guard and an unsuppressed absence assertion).
The Plan 097 regression matrix
(`tests/integration/ntcp2/harness/test_plan097.py`) and the
extended pre-dispatch audit
(`scripts/check-plan095-workflow.sh`) are green locally. Plan 097
is the final pre-dispatch implementation pass before the first
authoritative Plan 095 live run.

Plan 098 is the active runner/provenance boundary corrective pass
over the Plan 095 live runner and wrapper provenance surfaces. The
plan closes the runner/provenance ownership defects that the first
authoritative Plan 095 manual CI dispatch exposed on 2026-08-10.
The authoritative run advanced through the contract, build, and
live runner launch phases but failed closed before any TCP or
NTCP2 wire activity. The runner reconstructed a non-authoritative
`repo_root / target / debug / i2pr-interop` path instead of using
the canonical absolute artifact path supplied by the wrapper; the
runner therefore returned `pre-protocol-preparation-failed` before
launching `i2pr-interop ntcp2 prepare`. That result must **not** be
interpreted as a wire-level NTCP2 failure. Plan 098 corrects the
runner/provenance ownership boundary and the adjacent provenance
defects in a single coherent pass:

- the runner accepts an explicit `i2pr_binary: Path` argument and
  rehashes the supplied file bytes against `i2pr_binary_sha256`
  before any subprocess launch;
- the wrapper threads the exact caller-supplied path to every
  runner and refuses a role/binary mismatch via the new
  `--attempt-kind` flag;
- the i2pr and i2pd build-manifest digests are independently
  measured; the runner no longer aliases a generic manifest
  digest into both artifact classes;
- the Plan 095 final gate validates record digests against the
  actual downloaded artifacts and role-specific manifests.

The Plan 098 regression matrix
(`tests/integration/ntcp2/harness/test_plan098.py`), the extended
pre-dispatch audit (`scripts/check-plan095-workflow.sh`), and the
extended interop boundary check
(`scripts/check-ntcp2-interoperability.sh`) are green locally.

The corrected repository state is:

```text
plan_068_staged_evidence = implemented
plan_069_runner_scaffolding = historical
real_i2pd_driver = implemented
real_i2pd_library_linkage = present
real_reference_process_in_plan069_runner = corrected_by_plan075
real_mixed_router_attempts = 0
plan_085_host_loopback_roadmap = implemented
plan_086_host_loopback_development_lane = planned
plan_087_forward_wire_execution = blocked_pending_plan_095_ci_forward_pass
plan_088_reverse_development_decision = insufficient_evidence
plan_088_active_supersedure = supersedes_plan_084_lane_invalidated
plan_091_status = historical_partial_correction
plan_092_status = superseded_by_plan_093
plan_093_status = implementation_landed_closure_incomplete
plan_094_status = implementation_landed_live_closure_environment_blocked
plan_095_status = active_runner_provenance_corrected_awaiting_authoritative_rerun
plan_096_status = passed_pre_dispatch_workflow_correction
plan_097_status = passed_artifact_path_and_cleanup_correction
plan_098_status = passed_runner_provenance_boundary_correction
plan_079_gate = blocked_pending_plan_088_two_way_passed
plan_072_gate = inactive_pending_plan_088_ambiguity
current_rootless_namespace_lane = unavailable
multipass_lane = qualified
support = experimental
advertised = false
normal_daemon_activation = disabled
```

The constrained-host lane decision and Plan 077 capability probe remain
historical records. Do not treat capability probing or the pre-protocol
Plan 078 stop as protocol evidence.

## Plan 069 host-compatible NTCP2 loopback smoke lane (historical)

> **Reclassification (Plan 074, supersession note).** Plan 069
> implements the Plan 067 Level 1 host-loopback smoke runner and its
> static boundary check; at Plan 074 registration the runner was
> scaffolding/fake-process test coverage only. The runner integrity
> correction landed in Plan 075. Plan 069 remains the historical
> scaffolding snapshot in `plans/069-status.md`.

Plan 069 implements the Plan 067 Level 1 host-loopback smoke lane.
The lane is a non-production composition that exercises a single
two-process NTCP2 direction (one i2pr launcher process, one Plan 064
i2pd direct driver process) on the host loopback, without sudo,
namespaces, Multipass, or any public-network access. The runner is
structurally incapable of producing a Level 3 release bundle or
certificate.

Use the Plan 069 focused seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction i2pr-to-i2pd-ipv4 \
  --reference-driver <path> \
  --reference-build-manifest <path> \
  --reference-source-lock <path> \
  --output <smoke-record.json> \
  --source-commit <40-lowercase-hex>
```

The runner is the lane Plan 075 will exercise for the first real
external i2pr/i2pd direction execution on a host where unprivileged
namespaces are unavailable. Plan 075 owns the runner integrity
correction and the next status update; Plan 069 builds the lane and
proves it fails closed under the focused tests.

## Plan 075 Plan 069 runner integrity and evidence correction

Plan 075 corrects the Plan 069 runner so it is structurally
incapable of producing a mixed-router pass unless it launches one
real i2pr process and one configured real reference process and
consumes authentic structured events from both. The corrected
runner must:

- launch the reference role through the configured reference driver
  via `tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh`,
  not a second `i2pr-interop` process;
- bind every accepted event to a measured reference process binary
  digest, implementation name, run ID, direction, Router Hash pair,
  and exact DeliveryStatus message ID;
- derive milestones only from validated structured events
  (`ntcp2_authenticated`, `frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`),
  never from a TCP loopback probe alone;
- refuse synthetic provenance fallback hashes that fabricate a
  schema-valid digest from a run string;
- fail closed with one of the typed blockers
  `runner-reference-process-not-executed`,
  `runner-reference-events-missing`,
  `runner-synthetic-provenance-rejected`, or
  `runner-protocol-event-unproven` whenever any of the above
  contracts is violated.

Use the Plan 075 focused seam with:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
git diff --check
```

Plan 075 does not build i2pd, run a real mixed-router direction,
add Docker/QEMU/namespaces/CI, change NTCP2 protocol code, or
produce a Level 2 or Level 3 record. Plan 075 closes the runner
integrity surface only. The next active plan is Plan 076; do not
attempt a real mixed-router run until the real i2pd driver and a
qualified Plan 077 execution lane exist.

Plan 081 has since superseded Plan 074 for active execution. Plans 075,
076, 077, and 080 are closed prerequisites; the active sequence is
**Plan 082 (implemented) → Plan 083 (implemented, execution pending) →
Plan 084 (implemented, execution pending) → Plan 085 → Plan 086 → Plan 087
→ Plan 088 → Plan 079 (blocked)**.

## Plan 076 real pinned i2pd driver (closed)

Plan 076 built and linked the exact pinned i2pd 2.60.0 source, removed
terminal listen/dial stubs, proved real symbols and genuine inspect
behavior, and produced measured instrumented/control manifests. The
qualification receipt carries the typed host blocker on the Plan 046
`apparmor_restrict_on` negative baseline.

## Plan 077 constrained-host lane (closed)

Plan 077 documented the constrained-host capability state. Probe existing
rootful Docker (`--network none`), then QEMU TCG (`-nic none`), then the
explicitly reduced inherited-descriptor lane or a manually triggered
remote runner. Do not install privileged services, retry rootless/Multipass
lanes, or claim protocol evidence from a typed no-lane result.

## Plan 081/082 active sequence correction

Plan 080's Multipass lane qualification and Plan 076's real pinned i2pd
driver are valid. Plan 078/080 stopped before TCP, so it is not protocol
rejection evidence. Plan 082 is implemented; the active sequence is
**Plan 082 (implemented) → Plan 083 (implemented, execution pending) →
Plan 084 (implemented, execution pending) → Plan 085 → Plan 086 → Plan 087
→ Plan 088**. Plan 079 is blocked pending the Plan 088 decision, and Plan
072 remains inactive until Plan 088 records
`decision = ambiguous-reference-divergence` with one exact wire-stage question.

Plan 082 uses the test-only `i2pr-interop ntcp2 prepare` and
`i2pr-interop ntcp2 validate-scenario` commands to create and validate i2pr
state before strict Plan 065 scenario rendering. The mixed runner must
bind real RouterInfo/hash values and a frozen
`i2pr-minimal-run-identity-v1` record, assert both peer identities and the
frozen run identity, and must not construct a generated scenario ID or use
empty correlation fields. Preparation and validate-scenario are
pre-protocol diagnostics only. Do not run the i2pd wire probe in the
Plan 082 pass; Plan 083 owns the first minimal `i2pr → i2pd` attempt.

## Plan 083 minimal i2pr-to-i2pd wire probe

Plan 083 lands the canonical development diagnostic for the first real
`i2pr -> i2pd` direction. The probe record schema
`i2pr-minimal-i2pd-probe-v1` lives in
`tests/integration/ntcp2/harness/minimal_i2pd_probe.py`. The schema
enforces the strictly-increasing stage model, the bounded terminal-result
and reason-code sets, the per-process counter skeleton, and the
Plan 082 run-identity contract. The generic `typed-harness-operation-failed`
reason is rejected; preparation and live process counters are separated.
A passed probe record requires the final stage, the four canonical observed
events, clean cleanup, and `reason_code = not_started`. The probe is a
development diagnostic, not a release certificate; it never authorizes
Plan 079 (repeated development validation) or Plan 073 (release
qualification). The probe never imports Plan 056/066 candidate, bundle,
certificate, rootless-topology, or Multipass authority. The focused test
matrices are `test_minimal_i2pd_probe.py` and `test_plan083.py`. The
implementation surface is in place on this host; no real wire attempt has
been executed because the host is the Plan 046 `apparmor_restrict_on`
negative baseline and the Plan 080 Multipass guest cannot complete on
this constrained host.

## Plan 085 Milestone 3 host-loopback development execution roadmap

Plan 085 is the active Milestone 3 host-loopback roadmap. It corrects
the active status authority (Plan 082 implemented and closed; Plans 083
and 084 implementation complete but execution pending) and introduces
exactly one new bounded topology kind, `host-loopback-development`,
that enables literal IPv4 loopback protocol execution on the
constrained host. The topology is development-only; it never satisfies
any release or isolation predicate. Plan 085 is the parent of Plan 086
(status correction + lane enablement), Plan 087 (forward probe), Plan 088
(reverse probe and development decision), and the conditional Plan 089
manual-isolated fallback.

## Plan 086 status authority and host-loopback development lane

Plan 086 enables the `host-loopback-development` lane, accepts literal
`127.0.0.1` only under that topology, adds the
`HostLoopbackDevelopmentPlacement`, and proves a listener-only preflight
without starting a dialer. Plan 086 closes on this host as
`blocked-artifact-or-build-defect` because the canonical
`i2pd_ntcp2_interop_driver_instrumented` binary has not yet been
built on this constrained host; the closure state and the
implementation surface are recorded in `plans/086-status.md`. The
Plan 087 forward direction remains blocked until Plan 086 records
`host-loopback-development-ready` or Plan 089 records
`manual-isolated-fallback-ready`.

## Plan 087 first real i2pr-to-i2pd host-loopback probe

Plan 087 runs the first real `i2pr -> i2pd` forward direction under the
Plan 086 `host-loopback-development` lane. The probe inherits the Plan
083 forward probe record schema (`i2pr-minimal-i2pd-probe-v1`) and
runner orchestration module (`plan083_runner.py`) unchanged. Plan 087
must reach `i2np_delivery_status_decoded` with exact Router Hash and
DeliveryStatus message ID correlation before Plan 088 begins.

The Plan 087 implementation surface landed: the canonical Plan 083
runner now drives the placement-owned concurrent i2pd listener and i2pr
dialer via `HostLoopbackDevelopmentPlacement.popen`, copies the
i2pd-exported RouterInfo into the scenario exchange path with a
verified digest, and threads the missing `reference_tree_sha256` and
`source_inspection_record_sha256` provenance digests through to the
i2pd direct driver invocation. The wrapper
(`scripts/interop/run-minimal-i2pd-host-loopback-probe.py`) accepts
only the two i2pd directions and refuses every release/support profile
flag.

The first instrumented forward attempt on this host reached
`listener_ready` and the i2pr dialer started, then the i2pr dialer
rejected the i2pd RouterInfo with `peer_router_info_invalid` before
any TCP connection — the i2pd direct driver's emitted `router.info`
carries zero `RouterAddress` entries, so `exact_ntcp2_address`
rejects the peer RouterInfo. Plan 090 closes this defect with four
behavior-neutral corrections to the i2pd direct driver
(`tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`):

- `set_bool_option("ntcp2.published", true)` — store the option
  as `bool` to match the `value<bool>()->default_value(true)`
  registration in `libi2pd/Config.cpp` line 330. Storing as `int`
  (the Plan 064 behavior) caused the `boost::any_cast<bool>`
  mismatch that silently failed the address materialization.
- `i2p::config::ParseCmdline(1, fake_argv, ignoreUnknown=true)` +
  `Finalize()` — populate the i2pd option store with declared
  defaults before the driver mutates individual options.
- `set_uint16_option` helper — store `port` and `ntcp2.port`
  as `uint16_t` (was stored as `int`, which throws
  `boost::bad_any_cast`).
- `i2p::transport::transports.SetCheckReserved(false)` — disable
  reserved-range filtering so loopback addresses survive
  `RouterInfo::ReadFromBuffer` deserialization.

The driver also fails closed with
`router-info-endpoint-mismatch` if the authoritative in-memory
RouterInfo does not carry the exact configured NTCP2 endpoint.

Plan 090 also corrects the Plan 083 pre-TCP classification: the
canonical runner now forbids generic `protocol_rejected` /
`reference-events-missing` before `tcp_connected` and serializes
pre-TCP failures as `pre_protocol_rejected` with a bounded
pre-protocol reason allowlist. Plan 083 host-loopback
`validate-scenario` is routed through
`HostLoopbackDevelopmentPlacement.run`.

After the Plan 090 corrections, the first clean committed-head
forward attempt authenticated the i2pd listener and reached
TCP, then the NTCP2 Noise handshake closed the socket with
`Io(ExactIoError { kind: Closed })` before the i2pr initiator
reached `ntcp2_authenticated`. The Plan 090 closure remains
open: the forward direction did not pass. Per the Plan 090
"Forward attempt reaches TCP and fails protocol" branch, the
failed record is preserved and Plan 088 is not allowed to run
until the forward direction passes. The closure record with
the exact live command, recorded digests, and bounded
correction-surfaces contract is in `plans/087-status.md`.

## Plan 090 i2pd RouterInfo and Plan 087 evidence corrective pass

Plan 090 closes the Plan 087 zero-address `router.info` defect
and corrects the Plan 083 pre-TCP classification and placement
ownership. See `docs/adr/0025-plan090-i2pd-driver-routerinfo-correction.md`
for the pinned-source references and the bounded pre-protocol
reason allowlist. Plan 090 lands:

- four driver corrections in
  `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`;
- the Plan 090 lifecycle documentation in
  `tests/integration/ntcp2/reference-drivers/source-verification.md`;
- the Plan 083 pre-TCP classifier, placement-owned scenario
  validation, and pre-protocol reason allowlist in
  `tests/integration/ntcp2/harness/plan083_runner.py`;
- the `tests/integration/ntcp2/harness/test_plan090.py` test
  matrix (14 cases) covering source verification, driver
  binary, control parity, pre-TCP classification, placement
  validation, and record validation;
- the static boundary check
  `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 090 driver corrections, lifecycle documentation,
  and test matrix presence.

The Plan 090 implementation surface travels with the repository
unchanged for any future host where the forward direction
ultimately passes. NTCP2 remains experimental and non-advertised.

## Plan 088 reverse host-loopback probe and development decision

Plan 088 owns the reverse `i2pd -> i2pr` direction and the active
development decision. Plan 088 inherits the Plan 086
`host-loopback-development` lane and reuses the Plan 084 reverse probe
schema (`i2pr-minimal-i2pd-reverse-probe-v1`) and runner orchestration
module (`plan084_runner.py`) unchanged. The Plan 088 development decision
vocabulary is exactly five values: `two-way-development-probe-passed`,
`one-way-passed-reverse-defect`, `ambiguous-reference-divergence`,
`manual-isolated-fallback-required`, `insufficient-evidence`. Only
`two-way-development-probe-passed` may unblock Plan 079; only
`ambiguous-reference-divergence` may activate Plan 072. The historical
`lane-invalidated` and `same-stage-two-way-i2pr-defect` tokens are
forbidden by the static boundary checker.

On this host the recorded decision is `insufficient-evidence` because
the Plan 087 forward direction recorded a pre-TCP rejection owned by the
i2pd direct driver and no real wire run has been retained. The Plan 087
implementation surface is ready for a fresh attempt against a fixed
i2pd driver. The Plan 088 implementation surface travels with the
repository unchanged for any future host where the Plan 086 lane
becomes executable or the Plan 089 manual-isolated fallback becomes
available. See `plans/088-status.md` for the closure record. NTCP2
remains experimental and non-advertised.

## Plan 078 first real two-way run

Plan 078 used the Plan 080-qualified guest and stopped before TCP at the i2pr
pre-protocol RouterInfo stage. Do not reuse a stale or unowned Multipass
instance, or infer protocol evidence from process lifetime or a port probe.
The exact stop result is recorded in `plans/078-status.md`. When a qualified
full-runtime lane exists, run one fresh instrumented
`i2pr-to-i2pd-ipv4` direction and one fresh instrumented
`i2pd-to-i2pr-ipv4` direction, then repeat each with the uninstrumented
control binary. Require exact DeliveryStatus/Router Hash correlation,
structured authentication/frame/post-AEAD/I2NP events, private-network
proof, and clean teardown before writing a Level 1 record. Plan 079 owns later
repetition; no support or advertisement change is implied.

## Companion skills (load before doing this lane)

## Plan 044 mixed-runner composition (host-side executor)

The checkout contains the four directional mixed-scenario definitions under
`tests/integration/ntcp2/mixed-scenarios/`: `i2pr-to-java-ipv4`,
`java-to-i2pr-ipv4`, `i2pr-to-i2pd-ipv4`, and `i2pd-to-i2pr-ipv4`. Each
direction has a unique execution ID, one declared initiator and responder,
and one terminal typed result.

The mixed runner composes `I2prAdapter` with each reference adapter through
a strict launcher scenario renderer. The renderer populates the exact launcher
schema with execution-specific scenario ID, role, address family, synthetic
endpoints, private network ID 99, confined state directory, deadlines,
padding profile, smoke-message profile, and expected-result class.

The data-phase oracle does not rely on an echo assumption. It uses a
protocol-valid trigger supported by both pinned references. Evidence records
carry real counters for authenticated-link count, frames sent/received, I2NP
message aggregates, admission/replay counters, process lifecycle counters,
and cleanup disposition.

Gate archival uses gate-specific staging to prevent cross-gate record
relabeling. The aggregate manifest must include exactly the expected records
for the selected profile; missing, extra, mislabeled, or zero-valued records
fail the gate. **No completed mixed-router i2pr record is present; these are
explicit blockers, not skipped successes.** The single directional record
that landed during the Plan 045 closure attempt is described in
`plans/045-closure-attempt.md` and exists only as a sanitized evidence file
plus its corresponding typed blockers for the other three directions.

## Plan 043 workflow

The semantic gates are ordered and later gates are ineligible when required
inputs are missing or invalid:

```text
contract -> reference-build -> reference-offline-reuse -> environment-smoke
-> reference-crosscheck-ipv4 -> i2pr-handshake-smoke-ipv4 -> full-matrix
-> evidence-validation -> cleanup-verification
```

1. Inspect the lock, scenario definitions, and current workflow status. Do not
   change source revisions, package assumptions, scenario IDs, or the IzPack
   hash without updating the plan and conformance documentation.
2. Run the contract checks without starting routers. Preparation then runs
   `check-host.sh --pre-install`, the declared `setup-host.sh`, and
   `check-host.sh --post-install`.
3. Build exact reference caches with `build-references.sh --force-rebuild`.
   This is the only network-enabled phase and records source/tool/artifact/
   tree hashes. Resolve only through `target/interop/cache/current-cache.json`.
4. Restore the verified cache and run `build-references.sh --offline`.
   Re-hash the complete runtime tree. A cache miss or metadata mismatch is a
   hard failure; never fetch or choose an arbitrary cache.
5. Run `environment-smoke`, then `reference-crosscheck-ipv4`. The latter uses
   separate Java/i2pd namespaces, private network ID 99, staged strict
   RouterInfo validation/import, controlled directions, and dual authenticated
   observations. It is harness control evidence only.
6. Only after reference control passes, build the current launcher and run
   `handshake-smoke`; require four independent i2pr/reference directions,
   authenticated handshake, bounded DeliveryStatus exchange, typed counters,
   sanitized finalization, and clean state. Run `full` only afterward; it adds
   bounded adversarial/resource cases and never unbounded fuzzing.
7. Validate every record and the aggregate manifest with
   `validate-evidence.py` and `check-ntcp2-interoperability.sh`. Empty
   evidence, placeholders, forbidden content, missing scenarios, extra passed
   records, hash mismatches, or incomplete direction coverage fail the gate.
8. Record the clean-host baseline before privileged execution with
   `sudo -E bash scripts/interop/verify-clean-host.sh --record-baseline`.
   Always run `cleanup.sh`, then verify with
   `sudo -E bash scripts/interop/verify-clean-host.sh --verify --baseline
   target/interop/build/clean-host-baseline.json`. Reject residual namespaces,
   veths, child processes, secret-bearing run roots, forbidden retained
   files, or attributable host nftables/routes/forwarding changes. Cleanup
   verification failure overrides protocol success.

The workflow and helper apparatus expose the ordered manual Plan 043 lane,
including clean-host verification and aggregate validation, but **no completed
successful aggregate run is present.** Treat that as an explicit Plan 043
blocker, not a skipped pass. Retain only sanitized typed records and approved
hashes under `target/interop/evidence/`.

## Result interpretation

- `blocked_host_contract` — no router process or protocol claim was made.
- `i2pr-mixed-router-profile-not-wired` — the active scenario ID is not
  allowlisted for the current mixed-router gate.
- Rejected configuration/state, authentication, timeout, cleanup, or
  evidence-validation failures remain typed and visible. **Never** convert
  them to pass or omit them from the closure record.
- An empty evidence directory is not success. Plan 041 reference-pair records
  are harness controls, not i2pr mixed-router evidence.
- For Plan 046 typed host-level blockers (e.g.,
  `blocked_unprivileged_user_namespace`), hand off to
  `i2pr-multipass-recovery`.
- A blocked profile, a reference-only control record, or a typed blocker is
  **never** an i2pr interoperability result. Do not advertise NTCP2 and do
  not close Milestone 3.

## Authoritative command surface (host-side)

Run from the repository root:

```text
# Host + build gates
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline

# Profiles
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

For the Plan 046 rootless sealed-namespace lane (`probe-rootless-sandbox.sh`,
`rootless-enter.sh`, `check-rootless-interop-boundary.sh`), use the
`i2pr-rootless-sandbox` skill.

For the Plan 048/049/050/051 Multipass recovery lane (`run-evidence-lane.sh`,
`create.sh`, `prepare-offline.sh`, `probe.sh`, `snapshot.sh`, `restore.sh`,
`transfer-source.sh`, `transfer-cache.sh`, `verify-base.sh`,
`cloud-init-status.sh`, `verify-clean-host.sh`, `selective-purge.sh`,
`run-matrix.sh`, `run-direction.sh`, `export-evidence.sh`, `dispatch-gate.sh`,
`check-multipass-interop-boundary.sh`), use the `i2pr-multipass-recovery`
skill.

## Files to inspect

- `tests/integration/ntcp2/references.lock.toml` — Ubuntu contract, source
  pins, build commands, exact IzPack SHA-256.
- `tests/integration/ntcp2/scenarios/*.toml` — the eight bounded i2pr/
  reference scenario definitions. IDs synchronized with
  `tests/integration/ntcp2/manifest.toml`.
- `tests/integration/ntcp2/reference-scenarios/` — Plan 041 pair schema and the
  two directional Java I2P / i2pd control scenarios.
- `tests/integration/ntcp2/mixed-scenarios/` — the four Plan 044 directional
  i2pr/reference scenarios.
- `tests/integration/ntcp2/harness/` — Python topology, adapters, process
  bounds, runner, evidence, mixed-runner, launcher renderer, data-phase
  oracle, reference-trigger, rootless supervisor, and multipass code.
- `scripts/interop/` — host setup, builders, isolation, matrix, gate staging,
  aggregate, cleanup.
- `scripts/check-ntcp2-interoperability.sh`,
  `scripts/check-fixture-manifest.sh`, `scripts/check-ntcp2-vectors.sh` —
  static gate checkers.
- `tools/i2pr-interop/` — non-production launcher seam. The current checkout
  composes bounded state preparation, listener/dial, handshake, authenticated
  link, and DeliveryStatus smoke through the Plan 044 mixed-runner. Its
  success is local driver validation only.
- `target/interop/evidence/` — sanitized records only; gate-prefixed files
  live alongside `run-manifest.json`. `target/interop/runs/` is
  secret-bearing and is deleted after every run.

## Development rules

Keep production ownership boundaries intact: runtime owns Tokio tasks and
sockets; transport contracts remain runtime-neutral; the launcher crate under
`tools/i2pr-interop` is a non-production seam and must not activate the
daemon. Add negative-path tests for new configuration, topology, process,
parser, or evidence behavior. Prefer deterministic local checks and never add
raw network fixtures or secrets.

Before handoff, run from the repository root, in this order:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh        # when I2NP fixture bytes change
bash scripts/check-ntcp2-vectors.sh           # when NTCP2 vector bytes change
bash scripts/check-ntcp2-interoperability.sh  # when ntcp2 evidence/manifest change
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
```

Record commands, results, host constraints, and any blocked stop condition
in a closure record; do not report a blocked profile as a passing
interoperability result.

## Plan 062 NTCP2 evidence-contract and architecture correction

Plan 062 is the evidence-contract and architecture correction
pass. The plan does not implement the Java or i2pd drivers and
does not perform an authoritative external interoperability run;
those belong to Plans 063 and 064.

The plan lands:

- `docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md`
  (Accepted) — two-process direct transport drivers for Java
  I2P and i2pd. ADR 0022 supersedes the conclusion of ADR 0021
  (Rejected by Plan 058) without rewriting ADR 0021. The primary
  topology forbids support routers, floodfill, reseed, SAM,
  I2CP, HTTP/I2PControl, and tunnels.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  — the source-locked API surface for the pinned Java I2P
  2.12.0 and i2pd 2.60.0 revisions.
- `tests/integration/ntcp2/harness/reference_trigger_v4.py` — the
  v4 trigger schema (`i2pr-reference-trigger-v4`). Replaces the
  40-hex SHA-1 Router Hash with the 64-hex SHA-256 Router Hash
  and binds the per-run DeliveryStatus `message_id`
  (`1..=0xffffffff`).
- `tests/integration/ntcp2/harness/reference_event.py` — the
  reference-event v1 schema (`i2pr-reference-event-v1`)
  recording per-driver structured events with exact DeliveryStatus
  message ID correlation for data-phase events.
- `tests/integration/ntcp2/harness/observation_v3.py` — the v3
  observation schema (`i2pr-ntcp2-direction-observation-v3`)
  with the mandatory correlation fields
  `delivery_status_message_id`, `peer_router_hash_sha256`,
  `local_router_hash_sha256`, and `source_event_sha256`.
- Retirement of the Plan 060 candidate from all future
  candidate validators and the static boundary checker. The
  Plan 060 candidate record is preserved verbatim; the Plan 060
  closure record carries the explicit "Superseded by Plan 062"
  marker.

The v3 trigger schema and the v2 observation schema remain
readable for historical inspection but cannot contribute to a
new passing bundle. The future candidate implementation floor is
Plan 065 closure or later.

The Plan 062 static boundary checker extension in
`scripts/check-ntcp2-interoperability.sh` fails when:

- the v4 trigger schema, reference-event v1 schema, or v3
  observation schema files are absent;
- ADR 0022 is absent or not Accepted after source verification;
- the Plan 060 candidate is not retired;
- the active code uses `_HEX40` for Router Hash;
- the Plan 062 documentation is absent.

Focused tests:

- `tests/integration/ntcp2/harness/test_reference_trigger_v4.py`
  — v4 trigger schema validator cases (40-hex rejection,
  63/65-hex rejection, uppercase rejection, all-zero provenance
  rejection, message ID bounds, v3 schema rejection, unknown
  fields).
- `tests/integration/ntcp2/harness/test_reference_event.py` —
  reference-event v1 schema cases (data-phase event fields,
  duplicate event sequence rejection, peer Router Hash mismatch
  rejection, terminal-event data-phase field rejection,
  forbidden payload text rejection).
- `tests/integration/ntcp2/harness/test_observation_v3.py` — v3
  observation schema cases (v2 rejection, mandatory correlation
  fields, receiver pass predicate, generic-phrase rejection,
  sender-only and wrong-message fixtures).
- `tests/integration/ntcp2/harness/test_plan062.py` — WP1-WP5
  surface tests covering the source-verification record, ADR
  0022 Accepted status, schema migration, Plan 060 retirement,
  and the Plan 061-066 roadmap chain.

Plan 062 does not advance any support row. NTCP2 remains
experimental and non-advertised; Milestone 3 stays open until
Plan 065 closes with one complete four-direction live diagnostic
bundle and Plan 066 produces a verified Milestone 3 certificate.

## Plan 063 Java I2P stripped-router direct NTCP2 driver

Plan 063 implements the source-locked Java I2P 2.12.0 stripped-router
direct NTCP2 driver. The driver is the test-only counterpart to the
Plan 064 i2pd driver; together they form the source-locked pair
required by the Plan 061 roadmap.

The plan delivered:

- `tests/integration/ntcp2/reference-drivers/java/src/JavaNtcp2InteropDriver.java`
  — the source-locked Java driver. It embeds the upstream
  `net.i2p.router.Router` and `RouterContext`, activates the
  pinned dummy facades, uses the real
  `net.i2p.router.transport.ntcp.NTCPTransport` (no patch, no
  SSU2, no `VMCommSystem`), and submits a real correlated
  `DeliveryStatusMessage` through `OutNetMessagePool`. The driver
  exposes `inspect`, `listen`, and `dial` modes through a strict
  config contract. The receive handler is a `HandlerJobBuilder`
  registered for `DeliveryStatusMessage.MESSAGE_TYPE` (constant
  value 10); the data-phase events (`frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`)
  are emitted from the receive handler invocation, never from a
  generic log phrase.
- `tests/integration/ntcp2/reference-drivers/java/source-lock.json`
  — the source-lock record
  (`i2pr-java-helper-source-lock-v1`) binding the pinned Java
  revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`, the helper
  source path, the build contract, and the locked constraints.
- `tests/integration/ntcp2/reference-drivers/java/classpath-manifest.json`
  — the runtime classpath binding every pinned jar in
  `target/interop/cache/java_i2p/<tree>/lib/` to its purpose. No
  Maven Central dependency may be introduced.
- `tests/integration/ntcp2/reference-drivers/java/build-manifest.schema.json`
  — the build-manifest schema
  (`i2pr-java-helper-build-manifest-v1`) that requires measured
  digests for every pinned artifact, the driver source and
  binary, the classpath manifest, and the JDK versions.
- `tests/integration/ntcp2/reference-drivers/java/build-driver.sh`
  and `run-driver.sh` — the offline build and runtime seams. The
  build script requires the exact pinned source/build cache, uses
  a deterministic sorted source list, uses an explicit classpath,
  and emits no download.
- `tests/integration/ntcp2/harness/java_direct_driver.py` — the
  Python harness adapter that binds every helper invocation into a
  Plan 062 v4 trigger record (`i2pr-reference-trigger-v4`) and
  validates the Plan 063 strict driver config contract. The
  adapter never reaches inside the Java helper state and never
  synthesises a passing record.
- `tests/integration/ntcp2/harness/test_java_direct_driver.py` and
  `test_java_direct_control.py` — the Plan 063 test matrix
  covering the source-verification contract, strict config
  contract, Python harness adapter, structured event contract, and
  the local inspect-mode round-trip where the pinned Java cache is
  available.
- `tests/integration/ntcp2/qualification/java-direct-driver.json` —
  the Plan 063 qualification receipt
  (`i2pr-java-direct-driver-qualification-v1`). On this host the
  receipt records the typed host-environment blocker
  (`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
  qualification remains to be produced in the Plan 046 rootless
  sealed-namespace lane or the Plan 048/049 Multipass recovery
  lane.

The Plan 063 source-verification record addition lives in
`tests/integration/ntcp2/reference-drivers/source-verification.md`
under the Plan 063 topology contract section. The active v4 trigger
schema, v3 observation schema, and reference-event v1 schema
continue to enforce the 64-hex SHA-256 Router Hash contract.

Plan 063 does not wire the Java driver into the canonical primary
`mixed_runner.py`; that wiring belongs to Plan 065. The Plan 063
closure contract is the typed host-environment blocker
`blocked_unprivileged_user_namespace` on the Plan 046 negative
baseline; the canonical external lane is the Plan 048/049
Multipass recovery lane.

### Plan 063 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 064 i2pd direct NTCP2 driver and observer correction

Plan 064 replaces the partial Plan 059 i2pd direct connect helper
with a correctly initialized, dual-mode, source-locked i2pd 2.60.0
NTCP2 interoperability driver. The driver is **test-only** and
never becomes a production dependency of `i2pr-daemon`. Plan 064
explicitly eliminates the eight documented defects of the Plan
059 helper: 64-hex SHA-256 Router Hash, NTCP2-address static-key
binding, source-verified pinned initialization, real
`CreateDeliveryStatusMsg` dispatch, bounded `SendMessage`
asynchronous semantics, sealed-topology reserved-range disable,
exact post-AEAD receive correlation, and measured provenance for
every helper input.

The plan delivered:

- `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`
  — the source-locked C++ driver with strict config validation,
  bounded `inspect` / `listen` / `dial` modes, real
  `CreateDeliveryStatusMsg` submission, and structured
  `i2pr-reference-event-v1` emission.
- `tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h`
  and `interop_observer.cpp` — the compile-time-gated passive
  observer API and sink.
- `tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch`
  — the minimal observer patch that activates the post-AEAD
  receive seam and the successful frame-write send seam.
- `tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json`
  — the source-lock record
  (`i2pr-i2pd-direct-driver-source-lock-v1`) binding the pinned
  i2pd revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`, the
  helper source path, the build contract, the observer patch
  digest, and the locked constraints.
- `tests/integration/ntcp2/reference-drivers/i2pd/build-manifest.schema.json`
  — the build-manifest schema
  (`i2pr-i2pd-direct-driver-build-manifest-v1`) that requires
  measured digests for the i2pd source tree, the observer patch,
  the helper source and binaries, the linked library manifest,
  the CMake version, and the compiler version.
- `tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt`,
  `build-driver.sh`, `run-driver.sh`, and `README.md` — the
  offline build contract, the runtime seam, and the driver
  README.
- `tests/integration/ntcp2/harness/i2pd_direct_driver.py` — the
  Python harness adapter that binds every helper invocation into
  a Plan 062 v4 trigger record (`i2pr-reference-trigger-v4`) and
  validates the Plan 064 strict driver config contract. The
  adapter never reaches inside the C++ helper state and never
  synthesises a passing record.
- `tests/integration/ntcp2/harness/test_i2pd_direct_driver.py`
  and `test_i2pd_direct_control.py` — the Plan 064 test matrices
  covering the source-verification contract, strict config
  contract, Python harness adapter, structured event contract,
  observer compile-time gating, the Plan 059 supersedure, and the
  typed host blocker.
- `tests/integration/ntcp2/qualification/i2pd-direct-driver.json`
  — the Plan 064 qualification receipt
  (`i2pr-i2pd-direct-driver-qualification-v1`). On this host the
  receipt records the typed host-environment blocker
  (`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
  qualification remains to be produced in the Plan 046 rootless
  sealed-namespace lane or the Plan 048/049 Multipass recovery
  lane.

The Plan 064 source-verification record addition lives in
`tests/integration/ntcp2/reference-drivers/source-verification.md`
under the Plan 064 i2pd topology contract section. The Plan 059
helper at `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
is replaced by a fail-closed compatibility stub with the explicit
Plan 064 supersedure marker; the original source-lock record is
preserved verbatim as the bounded historical-reader path.

Plan 064 does not wire the i2pd driver into the canonical primary
`mixed_runner.py`; that wiring belongs to Plan 065.

## Plan 076 real pinned i2pd library and direct driver construction

Plan 076 replaces the Plan 064 terminal-stub helper with a real
source-locked i2pd 2.60.0 test executable that links against the
unmodified pinned i2pd 2.60.0 libraries built from the pinned
CMake project. The Plan 076 implementation surface is mandatory
for the canonical mixed-router lane in Plan 065.

Plan 076 lands:

- `tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt`
  — the driver CMake project. Drives the instrumented and control
  driver binaries against the freshly built pinned i2pd libraries
  via the `I2PD_PATCHED_TREE`, `I2PD_PRISTINE_TREE`, and
  `I2PD_LIB_DIR` cache variables. Defines
  `-DI2PD_PLAN076_LINKED=1` for both binaries; defines
  `-DI2PD_INTEROP_OBSERVER=1` only for the instrumented binary.
  The driver CMake project fails closed when `I2PD_LIB_DIR` is
  not supplied or when no `libi2pd*.a` archives are present.
- `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`
  — the two-stage build script. Builds the pinned i2pd CMake
  project with `WITH_LIBRARY=ON` and `WITH_BINARY=OFF`; applies
  the observer patch with `patch -p1 --fuzz=0`; drives both
  driver binaries; writes the build manifest with measured
  digests under `reference_source_tree_sha256`,
  `i2pd_libraries_sha256`, `linked_library_manifest_sha256`,
  `observer_patch_sha256`, `driver_source_sha256`, and both binary
  digests; sets `linked_i2pd_sources: true` and
  `observer_compile_time_gated: true`.
- `tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`
  — the real C++ driver. Initializes the pinned i2pd context in
  the source-verified order from
  `tests/integration/ntcp2/reference-drivers/source-verification.md`,
  uses the real NTCP2 transport, imports one peer RouterInfo
  directly via `i2p::data::netdb.AddRouterInfo`, and submits a real
  `i2p::CreateDeliveryStatusMsg` through
  `i2p::transport::Transports::SendMessage`. The
  `pinned_libraries_linked()` runtime gate fails closed with exit
  66 when `I2PD_PLAN076_LINKED` is not defined.
- `tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch`
  — the receive observer seam is placed immediately after
  `nextMsg->FromNTCP2()` inside the
  `case eNTCP2BlkI2NPMessage:` block of
  `NTCP2Session::ProcessNextFrame` in `libi2pd/NTCP2.cpp`. The
  send observer seam is placed at the top of
  `NTCP2Session::HandleI2NPMsgsSent`. Both seams are compile-time
  gated by `I2PD_INTEROP_OBSERVER`; the control build uses the
  pristine tree with the patch reverted.
- `tests/integration/ntcp2/reference-drivers/i2pd/source-lock.json`
  — records the `linked_marker_macro`,
  `linked_marker_required`, and `pinned_i2pd_cmake_options`
  fields, plus the measured `libi2pd`, `libi2pdclient`, and
  `libi2pdlang` archive digests.
- `tests/integration/ntcp2/reference-drivers/i2pd/build-manifest.schema.json`
  — requires `i2pd_libraries_sha256`,
  `linked_i2pd_sources: true`, and
  `observer_compile_time_gated: true` on every manifest.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  — the Plan 076 verified call graph section documents every
  symbol, file path, and line number used by the driver against
  the pinned i2pd 2.60.0 tree.
- `tests/integration/ntcp2/qualification/i2pd-direct-driver.json`
  — the qualification receipt carries the measured
  `reference_tree_sha256`, `libi2pd_sha256`, `libi2pdclient_sha256`,
  `libi2pdlang_sha256`, `cmake_lists_sha256`,
  `build_driver_script_sha256`, and the
  `plan_076_local_build_complete: true` invariant.
- `tests/integration/ntcp2/harness/test_i2pd_direct_driver.py` —
  adds six Plan 076 contracts: source-lock linked-marker,
  source-lock measured library digests, build-manifest schema
  library-digest requirement, CMake `I2PD_PLAN076_LINKED` +
  `I2PD_LIB_DIR` requirement, build-driver library-build
  commands, and driver source real i2pd API surface.

Plan 076 explicitly eliminates the six Plan 076 defects (`P1`-`P6`)
from the Plan 064 implementation surface:

- `P1`: helper `CMakeLists.txt` saw i2pd headers but did not
  compile or link the actual pinned i2pd library targets.
- `P2`: `I2PD_PLAN076_LINKED` was not defined by the build.
- `P3`: `run_listen()` and `run_dial()` were terminal rejection
  stubs.
- `P4`: inspect mode did not prove real i2pd initialization or
  RouterInfo production.
- `P5`: build manifests described linked i2pd behaviour the
  current binary did not contain.
- `P6`: a control binary that omits observer calls was not
  sufficient unless both binaries execute the same genuine
  transport path.

The Plan 076 closure boundary does **not** require a mixed-router
pass; the closure is a real binary with verifiable source linkage
and locally testable inspect / control behaviour. On this host
(the Plan 046 `apparmor_restrict_on` negative baseline) the
qualification receipt records the typed host blocker and an
all-zero attempt count. NTCP2 stays experimental and
non-advertised.

### Plan 076 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

### Plan 064 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

Consult [operations.md](references/operations.md) for command routing,
profiles, typed outcomes, and implementation-specific stop conditions.

## Plan 065 NTCP2 canonical integration and live qualification

Plan 065 wires the corrected Java and i2pd direct drivers into the
canonical four-direction mixed-router lane, enforces the exact
DeliveryStatus correlation on the i2pr side, and produces one
complete four-direction live diagnostic bundle from a clean
implementation commit. Plan 065 establishes the implementation
floor from which Plan 066 may cut a candidate.

The Plan 065 implementation ships:

- `tools/i2pr-interop/src/scenario.rs` — the strict scenario
  schema bumped to `i2pr-launcher-scenario-v2` with the per-run
  DeliveryStatus `message_id`, the 64-lowercase-hex expected sender
  and receiver Router Hashes, the `reference_driver_mode` field,
  and the `run_identity_sha256` field. Legacy schema 1 records
  are rejected by the strict parser.
- `tools/i2pr-interop/src/main.rs` — the i2pr sender uses the
  scenario-owned message ID and verifies the round-trip envelope
  message ID and the DeliveryStatus payload message ID before
  frame emission. The i2pr receiver requires the exact envelope
  and payload message ID, rejects duplicates, and emits the
  bounded Plan 065 typed failure categories
  (`SenderDeliveryStatusMessageIdZero`,
  `SenderRouterIdentityMismatch`,
  `SenderDeliveryStatusConstructionFailed`,
  `SenderFrameQueueAmbiguous`, `SenderFrameWriteFailed`,
  `SenderMultiplePrimaryDeliveryStatusEmitted`,
  `SenderCancellationObserved`, `ReceiverFrameReadFailed`,
  `ReceiverFrameAuthenticationFailed`, `ReceiverI2npDecodeFailed`,
  `ReceiverDeliveryStatusMissing`,
  `ReceiverDeliveryStatusIdMismatch`,
  `ReceiverDeliveryStatusDuplicate`,
  `ReceiverPeerIdentityMismatch`,
  `ReceiverDeliveryStatusTimestampInvalid`). The hard-coded
  `0x0420_0001` DeliveryStatus authority is removed.
- `tools/i2pr-interop/src/status.rs` — the status counter carries
  the per-run DeliveryStatus `message_id` and the expected peer
  Router Hash. The typed failure categories are added to the
  bounded `StatusReason` allowlist.
- `tests/integration/ntcp2/harness/launcher_protocol.py` and
  `tests/integration/ntcp2/harness/launcher_renderer.py` — the
  Python strict schema and renderer mirror the Rust schema with
  the same v2 marker and the same required primary fields.
- `tests/integration/ntcp2/harness/mixed_runner.py` — the canonical
  mixed-runner wires the new scenario primary fields through
  `render_and_validate` for both the i2pr initiator and responder
  paths. The `_plan065_primary_fields` helper derives the
  DeliveryStatus `message_id` from the run identity and the
  correlation nonce; the `_reference_driver_mode_for` helper
  returns the source-locked driver mode for a reference kind. The
  runner rejects SAM, HTTP, I2PControl, support-topology, and
  synthetic-fallback helpers for any primary direction.
- `tests/integration/ntcp2/harness/test_plan065.py` — the Plan 065
  test matrix covering scenario v2 acceptance and rejection (zero
  message ID, 40-hex Router Hash, unknown reference driver mode,
  direction-helper mismatch), DeliveryStatus message ID
  derivation uniqueness, status counter contract, reference
  trigger v4 correlation, observation v3 correlation, pass
  predicate exact message ID and Router Hash correlation,
  support-router rejection, Plan 060 candidate retirement, and the
  Plan 066 implementation floor marker.

### Plan 065 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Plan 066 fresh-candidate and authoritative NTCP2 two-run closure pass

Plan 066 is the execution-only pass that cuts one fresh candidate
descended from the Plan 065 implementation floor, selects exactly
one execution lane (direct-host or guest), runs the four primary
IPv4 mixed-router directions twice on independent mutable state,
and produces a verified Milestone 3 certificate over the two
sanitized bundles. The plan-of-record is
`plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md`.

The plan cannot start under the current four-direction contract
until either a future pinned Java revision is adopted or the
closure contract is revised through a new ADR (because ADR 0021 is
Rejected by Plan 058). The Plan 046 rootless sealed-namespace
lane returns `blocked_unprivileged_user_namespace` on this host;
the Plan 048/049 Multipass recovery lane is the canonical external
path but cannot complete on this constrained host (per Plan 051).
Plan 066 therefore closes on this host with the typed environment
blocker `blocked_execution_lane_unavailable`; the candidate is
`declared-not-executable`.

The Plan 066 implementation ships:

- `tests/integration/ntcp2/harness/plan066.py` — the Plan 066
  helper module. Exports `plan066_typed_blocker() ->
  "blocked_execution_lane_unavailable"`, `plan066_close_status()
  -> "declared-not-executable"`, `plan066_execution_lane_lock(...)`
  for the Plan 058/060 two-lane contract,
  `plan066_candidate_record_digests()` for the bounded 23-row
  digest table, `plan066_freeze_readiness_report()` for the
  freeze-readiness checklist,
  `assert_plan066_freeze_invariants()` for the typed blocker
  enforcement, `plan066_directional_record(...)` for the per-
  direction record skeleton, `plan066_two_bundle_independence(...)`
  for the cross-run independence rules, and
  `plan066_finalized_bundle_marker()` for the bundle mutation
  guard.
- `tests/integration/ntcp2/harness/test_plan066.py` — the Plan 066
  test matrix (41 cases covering the 30 enumerated Plan 066
  Phase 12 cases plus the typed-blocker, freeze-readiness,
  helper-contract, and Plan 065 plan-of-record helpers).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 066 artifacts, the Plan 066 test matrix coverage, and
  the candidate/closure marker invariants.
- `plans/066-candidate.md` — the Plan 066 candidate record. Status
  `declared-not-executable`. Implements the executed source
  commit (the Plan 065 implementation floor), the bounded 23-row
  digest table, the lane lock, the typed blockers, and the schema
  marker.
- `plans/066-closure.md` — the Plan 066 closure record with the
  typed blocker and the close-status.

### Plan 066 focused checks

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan066.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Until the Plan 046 rootless sealed-namespace lane or the Plan
048/049 Multipass recovery lane becomes runnable on a host with
the resources Plan 051 required, or until ADR 0021 is superseded
by an ADR that authorizes a different execution path for the
`java-to-i2pr-ipv4` direction, Milestone 3 stays open and NTCP2
stays experimental and non-advertised. The Plan 066 implementation
surface is mandatory regardless of close outcome.

## Plan 077 constrained-host lane provisioning

Plan 077 is closed on the current host with a typed no-full-runtime-lane
result. Use `bash scripts/interop/probe-constrained-host-lanes.sh` before
constrained-host work. The probe is read-only and selects, in order, an
accessible rootful Docker daemon with `--network none`, QEMU TCG with
`-nic none`, inherited descriptors plus `no_new_privs`/seccomp as a
reduced-scope diagnostic, a manual remote workflow, or no lane. It must not
install tools, change host policy, invoke privilege escalation, retry
rootless/Multipass, or start a router.

The strict common execution manifest and sanitized qualification record are
owned by `tests/integration/ntcp2/harness/execution_lane.py`. Run
`bash scripts/check-constrained-host-lane-boundary.sh` and
`python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'`.
On this host Docker is inaccessible, QEMU is absent, and only the reduced
scope is available; Plan 078 remains blocked until a full-runtime
qualification record exists. See `plans/077-status.md` and ADR 0024.

## Plan 102 Milestone 4 RouterInfo/NetDB authority and the Plan 102 amendment (active roadmap)

[Plan 102](../../../../plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
is the active Milestone 4 parent authority that supersedes the
historical Milestone 3 "active" blocks for the purpose of
continuing router development. The retained Plan 099/100/101
NTCP2 result (`protocol-defect-localized` at `noise_authenticated`,
normal-daemon NTCP2 disabled and unenableable) is preserved as
the authoritative NTCP2 development record. The Plan 099/100/101
status blocks below describe that result; the next substantial
product work is now governed by Plan 102 and its child sequence
(Plans 103 → 104 → 105 → 106).

### Plan 102 amendment — exploratory-tunnel dependency

[Plan 102 amendment](../../../../plans/102-amendment-exploratory-tunnel-dependency.md)
corrects an over-optimistic wording in the first Plan 102 draft.
The current I2P `DatabaseLookup` operation uses an outbound
exploratory tunnel and requests the response through an inbound
exploratory tunnel; exploratory tunnels are Milestone 5 scope.
Therefore a standards-conformant live RouterInfo lookup cannot
complete inside the Plan 103–106 implementation sequence merely
by re-entering NTCP2 or another direct router transport.

The authoritative Plan 102 sequence is:

```text
Plan 103  RouterInfo validation + bounded local NetDB
   -> Plan 104  persistent cache + SU3 reseed trust/ingestion
   -> Plan 105  transport-neutral lookup/store/publication state machines
   -> Plan 106  daemon/bootstrap integration
   -> Milestone 5 exploratory tunnel substrate
   -> return to Milestone 4B external acceptance
```

Plan 106 closes the local/bootstrap implementation phase, not
the complete original Milestone 4 exit criteria. After Plan 106
closes, Milestone 4A is
`local-foundation-complete-external-transport-blocked` until
Milestone 5 supplies exploratory inbound/outbound paths and a
router transport is deliberately qualified. A direct
`DatabaseLookup` over NTCP2 is not accepted as a substitute for
the standard exploratory-tunnel path. The next executable
implementation remains **Plan 103** (RouterInfo validation and
local NetDB foundation).

## Plan 099 Milestone 3 interop exit, harness reduction, and router buildout (historical; retained NTCP2 result)

Plan 099 is the corrective and exit plan from the multi-job
CI/provenance expansion that consumed Plans 095–098. It corrects
the central Plan 099 implementation finding — the instrumented
i2pd transport libraries were never actually compiled from the
patched source tree — and it freezes interoperability
architecture growth. The build script now produces two separate
i2pd archive sets (`I2PD_INSTRUMENTED_LIB_DIR` and
`I2PD_PRISTINE_LIB_DIR`), and the pristine control driver uses
native `Transports::SendMessage`, `Transports::IsConnected`, and
`TransportSession::IsEstablished` instead of observer APIs the
control build cannot emit.

Plan 100 closed its exit-readiness defects (D1, D2, D3, D4) and
the Plan 099 single-job CI workflow was dispatched exactly once
from the Plan 100 correction commit. The bounded replacement
runs (allowed by Plan 100 Result branch C) consumed two narrow
direct corrections before the bound forward-instrumented attempt
reached authentic post-TCP protocol evidence. The final
development result is `protocol-defect-localized`:

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

The compact sanitized summary is preserved at
`target/interop/evidence/milestone-3/31521642090/plan099-summary.json`.
The cross-side defect (i2pd listener authenticated but i2pr dialer
recorded `tcp_connected` then `terminal_rejected` with
`reference-events-missing` before observing the i2pd's
authentication event) is recorded as a localized protocol defect;
no further Rust correction is attempted under Plan 100. NTCP2
remains experimental and non-advertised.

The active development interop surface is small and bounded:

- `scripts/interop/run-minimal-i2pd-host-loopback-probe.py` —
  the only allowed entry point to live subprocess execution;
- `tests/integration/ntcp2/harness/plan083_runner.py` and
  `plan084_runner.py` — the canonical forward/reverse runners;
- `tests/integration/ntcp2/harness/preflight_runner.py` —
  the listener-only preflight;
- `tests/integration/ntcp2/harness/i2pd_direct_driver.py`,
  `minimal_i2pd_probe.py`, `minimal_i2pd_reverse_probe.py`,
  `interop_topology.py`, `reference_event.py`,
  `reference_trigger_v4.py`, `execution_lane.py`,
  `plan099_exit_gate.py` — the functional interop modules;
- `tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py`,
  `test_i2pd_direct_driver.py`, `test_i2pd_direct_control.py`,
  `test_execution_lane.py` — the bounded functional tests;
- `tests/integration/ntcp2/reference-drivers/i2pd/build-driver.sh`
  and `CMakeLists.txt` — the i2pd driver build script and the
  split-library link contract;
- `scripts/check-ntcp2-interoperability.sh` — the trimmed
  static boundary check.

Plan 099/Plan 100 forbid adding new `test_planNNN.py` files, new
plan-number-specific Python runners, or new plan-token static
checks. Historical plan documents remain in `plans/` as audit
records but are not executable API contracts.

The active sequence forbids repairing or retrying the Plan 046
rootless lane or the Plan 048/049/050 Multipass recovery lane.
Multipass, Docker, public-network traffic, reseed, SAM, I2CP,
SSU2, and Java I2P are not part of the development interop
surface. A localized NTCP2 defect keeps NTCP2 disabled and
non-advertised but does not block production daemon composition,
RouterInfo publication architecture, NetDB storage/indexing, SU3
reseed parsing, or deterministic local state-machine tests.

Required focused checks for the active sequence:

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

The focused local seam is sufficient for routine development.
The full historical plan-specific Python matrix, rootless checker,
Multipass checker, and release-certificate validator are not
required for Plan 100 closure; they remain available via git
history for forensic archaeology.

The wrapper requires an explicit `--i2pr-binary` path for every
attempted-live path (preflight, forward, reverse). The CI workflow
runs build and execute in a single `ubuntu-24.04` job with no
cross-job binary artifact transfer. Plan 095 is historical. The
active development interop lane is closed; NTCP2 remains
experimental and non-advertised. The next executable
implementation is governed by
[Plan 102](../../../../plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
and its child sequence, with **Plan 103** (RouterInfo validation
and local NetDB foundation) as the immediate next step. A direct
`DatabaseLookup` over NTCP2 is not accepted as a substitute for
the standard exploratory-tunnel path (Milestone 5).
