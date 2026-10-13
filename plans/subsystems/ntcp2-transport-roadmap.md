# NTCP2 Transport Roadmap

Status: historical M3 closed (protocol defect localized); Plans 410/414/415/416/417/418/419/420/421/422/423/424 stopped before current-pin two-way evidence; recovery Plans 434–435 remain gated.

**Recovery continuation (2026-10-10):** [Plan 434](../implementation/ntcp2-transport/434-ntcp2-authenticated-link-discrepancy-recovery.md) is the bounded Plan 099 authenticated-link discrepancy successor. Plans 410 through 420 stopped at their recorded evidence boundaries. Plan 420 observed the post-validation RouterInfo marker and a session termination marker, but no I2NP block or DeliveryStatus. Plan 421 stopped before wire because its reverse scenario supplied the wrong expected sender identity. Plan 422 corrected that mapping but its reverse attempt failed at SessionConfirmed Part 1. Plan 423 split the existing bounded error categories and spent one reverse-only attempt; Plan 424 added exact operation/kind attribution and spent one reverse-only attempt. The responder's terminal result is a closed SessionConfirmed read, while the pinned helper reported session-not-established; no shared correlation establishes a common cause. Plan 434 remains blocked pending forward plus reverse authenticated I2NP evidence. [Plan 435](../implementation/ntcp2-transport/435-ntcp2-daemon-activation-and-dual-transport.md) is normal-daemon activation plus independent two-family qualification, blocked on Plans 433/434. Neither plan authorizes a premature NTCP2 bind or advertisement; Plan 101 safety remains authoritative until executed qualified successor evidence. Cross-subsystem dependencies: [core-router recovery roadmap](core-router-recovery-roadmap.md). Historical NTCP2 closure records unchanged.

**Corrective successor (2026-10-10):** [Plan 441](../implementation/ntcp2-transport/441-current-pin-single-session-control-and-interop-recovery.md) replaced the repeated single-event Plan 410/414–424 diagnostic loop with a stock-to-stock positive control followed by an isolated single-session i2pr/i2pd control. Its first stock-control run exited 66 on both helpers, but the runner did not retain a terminal reason; it is blocked as unclassified environment evidence, not protocol rejection. [Plan 445](../implementation/ntcp2-transport/445-current-pin-control-runner-attribution-corrective.md) corrected the receipt and spent one fresh stock-control budget. Both pristine helpers reported `control-*-session-not-established`; the positive control therefore remains blocked and no i2pr wire attempt followed. This remains an evidence/source-localization lane feeding Plan 434, not an NTCP2 release qualification or rewrite of old closure outcomes. [Emissary architecture/source comparison](../diagnostics/2026-10-10-emissary-router-qualification-comparison.md) is read-only and uses no copied code. [Cross-subsystem graph](core-router-recovery-roadmap.md). Plan 101 still forbids normal-daemon NTCP2 activation.

**Plan 446 outcome (2026-10-10):** [`446-status.md`](../closure/ntcp2-transport/446-status.md) is `blocked-normal-i2pd-control-fixture-datadir-isolation-not-established`. Source inspection showed that pinned i2pd initializes its data directory before parsing the config file, so config-only `datadir` does not isolate a process. The first bounded launch omitted `--datadir`; no protocol evidence is admissible and no i2pr attempt followed. The record notes that the account's default i2pd directory may have been selected; it was not inspected or cleaned. Any successor must prove unique command-line data directories before launch and establish the egress boundary before attempting protocol traffic. Plans 441/445 and Plan 101 remain unchanged. [Core recovery graph](core-router-recovery-roadmap.md).

**Execution handoff (Plan 452, 2026-10-10):** [Normal-i2pd datadir and stock NTCP2 control](../implementation/ntcp2-transport/452-real-i2pd-isolated-datadir-and-stock-ntcp2-control.md) is **ready** after Plan 446's unsafe config-only data directory attempt. Pinned `daemon/Daemon.cpp` selects the datadir before config parsing; every test process must therefore use a unique **command-line** `--datadir` and its own `--conf`, identity, RouterInfo cache and TCP endpoint. A pair of ordinary routers must be stimulated by a legitimate client/NetDB/tunnel operation; an idle pair with no dial trigger is not a protocol failure. Establish both stock-to-stock authenticated sessions and decoded I2NP before substituting the controlled i2pr adapter. Keep Plan 101 default-off normal NTCP2, no custom reference wrappers, no public network. Current Plans 441/445/446 closures remain immutable.

**NTCP2-FIRST RECOVERY (2026-10-10):** [Plan 455](../implementation/ntcp2-transport/455-initiated-i2np-and-stock-peer-workload-recovery.md) is the P0 correction to Plan 452's **no-dial-trigger**, grounded in pinned i2pd `libi2pd/Transports.cpp::PostMessages/ConnectToPeer`. A stock listener plus a peer RI is insufficient: a legitimate I2NP send, eligible published NTCP2 address, RouterInfo and network ID are required. The corrected CLI `--datadir` isolation remains the baseline. **Run actual i2pr→unmodified i2pd authenticated I2NP immediately**, even if independent stock-to-stock selection still lacks prerequisites; do not gate one on the other. [Plan 456](../implementation/ntcp2-transport/456-guarded-normal-daemon-ntcp2-service-owner.md) is a separate **parallel P0** bounded source/lifecycle implementation for a named loopback-only real `i2pr run` NTCP2 owner using one daemon RouterIdentity, one bandwidth governor and the same NetDB/I2NP dispatcher. [Plan 457](../implementation/ntcp2-transport/457-real-process-ntcp2-i2np-and-ssu2-fallback-acceptance.md) follows executed 455/456 passes with whole-process authenticated I2NP and SSU2 coexistence; the eventual Plan 435 full two-family/public profile still needs its existing external gates. Plan 101 default-off and no-public-transport guard remain mandatory. Plan 458 SAM product work has **lower execution priority** and cannot block transport testing.

**Plan 455 outcome (2026-10-11):** [`455-status.md`](../closure/ntcp2-transport/455-status.md) records Gates A/B passed: i2pr's compressed DatabaseStore was decoded by pristine pinned i2pd, and the corrected four-peer stock SAM control exited 0 after exact-byte exchanges in both directions with SSU2 disabled and NTCP2 active. Plan 455 remains blocked at `blocked-stock-to-i2pr-and-negative-matrix-unproven`; stock-to-i2pr delivery and the bounded negative matrix remain open. The runners are [`plan455_direct_stock_attempt.sh`](../../tests/integration/ntcp2/plan455_direct_stock_attempt.sh) and [`plan455_stock_sam_trigger.py`](../../tests/integration/ntcp2/plan455_stock_sam_trigger.py).

Plan 458 now returns promptly on rejected build submission and correlates exact-attempt invalid replies, including wrapped type-19 `TunnelGateway` replies covered by a deterministic correlation regression, but its latest corrected ordinary-process SAM attempt still timed out at `outbound-build` before a stream. This supplies cross-line product localization, not a transport result; retain Plans 455 and 457 as blocked until stock-to-i2pr delivery and dual-family continuity evidence exist.

**Plans 456–457 outcome (2026-10-11):** [`456-status.md`](../closure/ntcp2-transport/456-status.md) closes Plan 456 at the controlled normal-daemon source/lifecycle scope: real loopback listener, signed identity-bound NTCP2 address/key lifecycle, wrong-network rejection, matching-peer authentication, common daemon dispatch/governor, and restart. The current workspace test run passed 4,821 tests with 39 ignored across 187 suites. Plan 457 is recorded in [`457-status.md`](../closure/ntcp2-transport/457-status.md) as blocked after Plan 455 Gates A/B passed; stock-to-i2pr delivery, correlated reverse traffic, and dual-family failure-continuity evidence remain unavailable. Neither closure promotes public support or closes Plan 435.

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/02-i2np.md` (envelope)
- `specs/protocols/03-ntcp2.md` (NTCP2 transport dossier)
- `specs/SOURCES.md` + `specs/IMPLEMENTATIONS.md` (pinned revisions, entry points)

Related ADRs:

- `docs/adr/0013-ntcp2-data-phase-and-blocks.md`, `0017-rootless-sealed-namespace-interop-evidence.md`, `0021-minimal-java-support-topology.md`, `0022-direct-reference-router-ntcp2-interop-drivers.md`.

## 1. Purpose and ownership boundary

NTCP2 handshake/data-phase state machines, runtime link manager, reference-router interoperability harness (i2pd/Java), rootless/multipass evidence lanes, and the Milestone 3 exit that localized the defect instead of shipping NTCP2.

Historic plans: 030–101 (global i2pr numbers, preserved).

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No production NTCP2 activation; the daemon stays NTCP2-disabled. No new interop lane without a new plan-of-record.

## 4. Current state

Plan 099/100 exit (protocol-defect-localized at noise_authenticated); normal-daemon NTCP2 disabled per Plan 101. Plans 410–420 stopped without authenticated two-way evidence. Plan 420 reached the post-validation RouterInfo marker but observed no I2NP block or DeliveryStatus. Plans 421–424 exercised the current-pin reverse path; Plan 424 identified a closed responder SessionConfirmed read, but the helper outcome remains uncorrelated. Plan 434 remains blocked because authenticated two-way and correlated I2NP evidence are still absent (see the Plan 434 outcome amendment).

## 5. Target architecture

Retained historical subsystem: no new architecture is planned here. Changes require a new plan-of-record
in this subsystem, following `plans/README.md`.

## 6. Dependency graph

```text
030..037 (M3 core) -> 038..066 (harness/candidate iterations) -> 067..099 (probes/correctives) -> 100/101 (exit + daemon hand-off).
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 30 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/030-milestone-3-overview.md` | `plans/closure/ntcp2-transport/030-milestone-3-active-status-amendment-plan-067.md`; `plans/closure/ntcp2-transport/030-milestone-3-closure.md` |
| 31 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/031-m3-transport-contracts-and-crate-boundaries.md` | `plans/closure/ntcp2-transport/031-closure.md` |
| 32 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/032-m3-ntcp2-crypto-transcript-and-vectors.md` | `plans/closure/ntcp2-transport/032-closure.md` |
| 33 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/033-m3-ntcp2-handshake-state-machines.md` | `plans/closure/ntcp2-transport/033-closure.md` |
| 34 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/034-m3-ntcp2-data-phase-and-blocks.md` | `plans/closure/ntcp2-transport/034-closure.md` |
| 35 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/035-m3-runtime-link-manager-and-addresses.md` | `plans/closure/ntcp2-transport/035-closure.md` |
| 36 | see token | no record | — | `plans/closure/ntcp2-transport/036-closure.md`; `plans/closure/ntcp2-transport/036-m3-interoperability-adversarial-validation-closure.md` |
| 37 | see token | no record | — | `plans/closure/ntcp2-transport/037-closure.md`; `plans/closure/ntcp2-transport/037-m3-corrective-integration-closure.md` |
| 38 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/038-ubuntu-reference-router-interoperability-harness.md` | `plans/closure/ntcp2-transport/038-closure.md` |
| 39 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/039-plan-038-corrective-interoperability-roadmap.md` | — |
| 40 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/040-interop-apparatus-corrective-pass.md` | `plans/closure/ntcp2-transport/040-closure.md` |
| 41 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/041-reference-router-private-crosscheck.md` | `plans/closure/ntcp2-transport/041-closure.md` |
| 42 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/042-runtime-owned-ntcp2-wire-driver.md` | `plans/closure/ntcp2-transport/042-status.md` |
| 43 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/043-ubuntu-build-system-interop-gates.md` | — |
| 44 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/044-ntcp2-interop-final-integration-corrective-pass.md` | `plans/closure/ntcp2-transport/044-closure.md` |
| 45 | see token | no record | — | `plans/closure/ntcp2-transport/045-closure-attempt.md`; `plans/closure/ntcp2-transport/045-ntcp2-mixed-router-proof-closure-corrective-pass.md` |
| 46 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/046-rootless-sealed-namespace-evidence-lane.md` | `plans/closure/ntcp2-transport/046-closure.md`; `plans/closure/ntcp2-transport/046-status.md` |
| 47 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/047-cross-host-rootless-lane-expansion.md` | — |
| 48 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/048-multipass-permissive-rootless-evidence-environment.md` | `plans/closure/ntcp2-transport/048-status.md` |
| 49 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/049-multipass-lifecycle-ownership-corrective-pass.md` | `plans/closure/ntcp2-transport/049-closure.md`; `plans/closure/ntcp2-transport/049-status.md` |
| 50 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/050-multipass-cloud-init-recovery-and-guest-probe-pass.md` | `plans/closure/ntcp2-transport/050-closure.md`; `plans/closure/ntcp2-transport/050-status.md` |
| 51 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/051-external-validation-attempt.md`; `plans/implementation/ntcp2-transport/051-external-validation-troubleshooting.md` | — |
| 52 | see token | no record | — | `plans/closure/ntcp2-transport/052-ntcp2-milestone-3-evidence-closure-follow-up.md`; `plans/closure/ntcp2-transport/052-status.md` |
| 53 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/053-plan052-evidence-pipeline-integration-corrective-pass.md` | `plans/closure/ntcp2-transport/053-status.md` |
| 54 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/054-java-startup-and-reference-observation-qualification-pass.md` | `plans/closure/ntcp2-transport/054-status.md` |
| 55 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/055-reference-initiated-ntcp2-trigger-and-topology-qualification-pass.md` | `plans/closure/ntcp2-transport/055-status.md` |
| 56 | see token | no record | — | `plans/closure/ntcp2-transport/056-candidate.md`; `plans/closure/ntcp2-transport/056-closure.md`; `plans/closure/ntcp2-transport/056-ntcp2-milestone-3-two-run-external-evidence-closure-pass.md` |
| 57 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/057-cross-host-milestone-3-external-evidence-run.md` | — |
| 58 | see token | no record | — | `plans/closure/ntcp2-transport/058-plan056-record-and-candidate-integrity-closure-pass.md`; `plans/closure/ntcp2-transport/058-status.md` |
| 59 | see token | no record | — | `plans/closure/ntcp2-transport/059-reference-side-implementation-and-live-qualification-closure-pass.md`; `plans/closure/ntcp2-transport/059-status.md` |
| 60 | see token | no record | — | `plans/closure/ntcp2-transport/060-candidate.md`; `plans/closure/ntcp2-transport/060-closure.md`; `plans/closure/ntcp2-transport/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md` |
| 61 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/061-ntcp2-direct-reference-driver-corrective-roadmap.md` | — |
| 62 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/062-ntcp2-evidence-contract-and-architecture-correction.md` | `plans/closure/ntcp2-transport/062-status.md` |
| 63 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/063-java-i2p-stripped-router-direct-ntcp2-driver.md` | `plans/closure/ntcp2-transport/063-status.md` |
| 64 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/064-i2pd-direct-ntcp2-driver-and-observer-correction.md` | `plans/closure/ntcp2-transport/064-status.md` |
| 65 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/065-ntcp2-canonical-integration-and-live-qualification.md` | `plans/closure/ntcp2-transport/065-status.md` |
| 66 | see token | no record | — | `plans/closure/ntcp2-transport/066-candidate.md`; `plans/closure/ntcp2-transport/066-closure.md`; `plans/closure/ntcp2-transport/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md` |
| 67 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/067-active-sequence-amendment-plan-074.md`; `plans/implementation/ntcp2-transport/067-active-sequence-amendment-plan-081.md`; `plans/implementation/ntcp2-transport/067-active-sequence-amendment-plan-085.md`; `plans/implementation/ntcp2-transport/067-milestone-3-staged-interoperability-corrective-roadmap.md` | — |
| 68 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/068-staged-interop-evidence-and-milestone-3-authority-correction.md` | `plans/closure/ntcp2-transport/068-status.md` |
| 69 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/069-host-compatible-ntcp2-loopback-smoke-lane.md` | `plans/closure/ntcp2-transport/069-status.md` |
| 70 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/070-i2pd-driver-build-and-first-two-way-live-execution.md`; `plans/implementation/ntcp2-transport/070-supersession-amendment-plan-074.md` | — |
| 71 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/071-repeated-i2pd-development-validation-and-negative-controls.md`; `plans/implementation/ntcp2-transport/071-supersession-amendment-plan-079.md` | — |
| 72 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/072-079-gate-amendment-plan-088.md`; `plans/implementation/ntcp2-transport/072-activation-amendment-plan-084.md`; `plans/implementation/ntcp2-transport/072-conditional-emissary-ntcp2-differential-validation.md` | — |
| 73 | see token | no record | — | `plans/closure/ntcp2-transport/073-deferred-java-and-release-qualification-closure.md` |
| 74 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/074-continuation-amendment-plan-081.md`; `plans/implementation/ntcp2-transport/074-milestone-3-real-driver-and-constrained-host-corrective-roadmap.md` | — |
| 75 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/075-plan-069-runner-integrity-and-evidence-correction.md` | `plans/closure/ntcp2-transport/075-status.md` |
| 76 | see token | closed on the local host with real_pinned_i2pd_libraries | `plans/implementation/ntcp2-transport/076-real-pinned-i2pd-library-and-direct-driver-construction.md` | `plans/closure/ntcp2-transport/076-status.md` |
| 77 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/077-constrained-host-ntcp2-execution-lane-provisioning.md` | `plans/closure/ntcp2-transport/077-status.md` |
| 78 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/078-first-real-i2pd-two-way-execution-and-bounded-correction.md` | `plans/closure/ntcp2-transport/078-status.md` |
| 79 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/079-repeated-i2pd-development-validation-and-continuation-decision.md` | — |
| 80 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/080-diagnostic-correction-amendment-plan-081.md`; `plans/implementation/ntcp2-transport/080-multipass-lane-prequalification-for-plan-078.md` | `plans/closure/ntcp2-transport/080-status.md` |
| 81 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/081-milestone-3-pre-protocol-and-minimal-i2pd-corrective-roadmap.md`; `plans/implementation/ntcp2-transport/081-supersession-amendment-plan-085.md` | — |
| 82 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/082-i2pr-state-preparation-and-mixed-runner-contract-correction.md` | `plans/closure/ntcp2-transport/082-status.md` |
| 83 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/083-minimal-i2pr-to-i2pd-ntcp2-wire-probe.md` | `plans/closure/ntcp2-transport/083-084-execution-status-amendment-plan-085.md`; `plans/closure/ntcp2-transport/083-status.md` |
| 84 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/084-i2pd-to-i2pr-reverse-probe-and-development-decision.md` | `plans/closure/ntcp2-transport/084-status.md` |
| 85 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/085-milestone-3-host-loopback-development-execution-roadmap.md` | — |
| 86 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/086-status-authority-and-host-loopback-development-lane.md` | `plans/closure/ntcp2-transport/086-status.md` |
| 87 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/087-first-real-i2pr-to-i2pd-host-loopback-probe.md` | `plans/closure/ntcp2-transport/087-status.md` |
| 88 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/088-reverse-host-loopback-probe-and-development-decision.md` | `plans/closure/ntcp2-transport/088-status.md` |
| 89 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/089-conditional-manual-isolated-i2pd-probe-fallback.md` | — |
| 90 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/090-i2pd-routerinfo-and-plan087-evidence-corrective-pass.md` | — |
| 91 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/091-forward-ntcp2-noise-handshake-corrective-pass.md` | `plans/closure/ntcp2-transport/091-status.md` |
| 92 | see token | no record | — | `plans/closure/ntcp2-transport/092-forward-handshake-evidence-integrity-and-ownership-closure.md`; `plans/closure/ntcp2-transport/092-status.md` |
| 93 | see token | no record | — | `plans/closure/ntcp2-transport/093-plan087-forward-data-phase-and-reference-observer-closure.md` |
| 94 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/094-plan093-completion-and-plan087-to-plan088-handoff.md` | — |
| 95 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/095-ci-host-loopback-live-wire-evidence-lane.md` | — |
| 96 | see token | no record | — | `plans/closure/ntcp2-transport/096-plan095-ci-workflow-correctness-and-pre-dispatch-closure.md` |
| 97 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/097-plan095-artifact-path-and-cleanup-corrective-pass.md` | — |
| 98 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/098-plan095-runner-provenance-boundary-corrective-pass.md` | — |
| 99 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/099-milestone3-interop-exit-and-router-buildout-corrective-plan.md`; `plans/implementation/ntcp2-transport/099-ntcp2-interop-exit-harness-simplification-and-router-build-unblock.md` | `plans/closure/ntcp2-transport/099-status.md` |
| 100 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/100-plan099-exit-gate-cleanup-and-router-handoff.md` | — |
| 101 | archived | historical narrative (no status record) | `plans/implementation/ntcp2-transport/101-daemon-ntcp2-activation-safety-and-router-handoff-correction.md` | — |
| 410 | stopped | `plans/implementation/ntcp2-transport/410-current-pin-ntcp2-loopback-runner.md` | `plans/closure/ntcp2-transport/410-status.md` |
| 414 | stopped | `plans/implementation/ntcp2-transport/414-stock-i2pd-decoded-message-observation.md` | `plans/closure/ntcp2-transport/414-status.md` |
| 415 | stopped | `plans/implementation/ntcp2-transport/415-failure-safe-current-pin-ntcp2-observation.md` | `plans/closure/ntcp2-transport/415-status.md`: failure record and negative controls passed; single forward helper timed out waiting for its control-listening peer; reverse not run |
| 416 | stopped | `plans/implementation/ntcp2-transport/416-current-pin-ntcp2-authentication-stage-diagnosis.md` | `plans/closure/ntcp2-transport/416-status.md`: source trace unresolved; a new plan must add sanitized handshake-stage observation |
| 417 | stopped | `plans/implementation/ntcp2-transport/417-sanitized-stock-ntcp2-handshake-stage-observer.md` | `plans/closure/ntcp2-transport/417-status.md`: stage observer passed; helper log level filtered all debug milestones |
| 418 | stopped | `plans/implementation/ntcp2-transport/418-current-pin-helper-debug-stage-logging.md` | `plans/closure/ntcp2-transport/418-status.md`: SessionConfirmed received before validation, no connected peer or DeliveryStatus |
| 419 | stopped | `plans/implementation/ntcp2-transport/419-session-confirmed-rejection-stage-observation.md` | `plans/closure/ntcp2-transport/419-status.md`: all rejection counters and DeliveryStatus zero; helper timeout recurred |
| 420 | stopped | `plans/implementation/ntcp2-transport/420-post-validation-session-progress-observation.md` | `plans/closure/ntcp2-transport/420-status.md`: RouterInfo accepted marker; no I2NP block or DeliveryStatus |
| 421 | stopped | `plans/implementation/ntcp2-transport/421-current-pin-reverse-initiator-attempt.md` | `plans/closure/ntcp2-transport/421-status.md`: reverse scenario identity mismatch before wire |
| 422 | stopped | `plans/implementation/ntcp2-transport/422-reverse-scenario-identity-correction.md` | `plans/closure/ntcp2-transport/422-status.md`: corrected mapping reached TCP; responder failed at SessionConfirmed Part 1 |
| 423 | stopped | `plans/implementation/ntcp2-transport/423-session-confirmed-part1-error-classification.md` | `plans/closure/ntcp2-transport/423-status-amendment-plan-424-outcome.md`: operation identified as SessionConfirmed read closed; helper cause remains uncorrelated |
| 424 | stopped | `plans/implementation/ntcp2-transport/424-reverse-responder-io-operation-attribution.md` | `plans/closure/ntcp2-transport/424-status.md`: exact responder read and IoErrorKind recorded; one reverse-only attempt spent |
| 446 | blocked | `plans/implementation/ntcp2-transport/446-real-i2pd-daemon-control-and-ntcp2-requalification.md` | `plans/closure/ntcp2-transport/446-status.md`: isolated normal-daemon control not established; no NTCP2/I2NP evidence |

| 455 | blocked | `blocked-stock-to-i2pr-and-negative-matrix-unproven` | `plans/implementation/ntcp2-transport/455-initiated-i2np-and-stock-peer-workload-recovery.md` | `plans/closure/ntcp2-transport/455-status.md`: Gates A/B passed; stock-to-i2pr delivery and the negative matrix remain open |
| 456 | passed at controlled source/lifecycle scope | `passed-controlled-normal-daemon-ntcp2-owner-lifecycle` | `plans/implementation/ntcp2-transport/456-guarded-normal-daemon-ntcp2-service-owner.md` | `plans/closure/ntcp2-transport/456-status.md`: real loopback process owner, signed key/RI lifecycle, matching-peer authentication and restart; external/public qualification excluded |
| 457 | blocked on 455 | `blocked-plan455-stock-to-i2pr-and-dual-family-continuity` | `plans/implementation/ntcp2-transport/457-real-process-ntcp2-i2np-and-ssu2-fallback-acceptance.md` | `plans/closure/ntcp2-transport/457-status.md`: Plan 456 and Plan 455 Gates A/B passed; stock-to-i2pr delivery and dual-family failure continuity remain unproven |
| 459 | passed at controlled loopback scope | `passed-controlled-daemon-initiator-product-lookup-and-correlated-reply` | `plans/implementation/ntcp2-transport/459-live-ntcp2-router-i2np-dispatch-and-reply-owner.md` | `plans/closure/ntcp2-transport/459-status.md`: normal daemon-initiated DatabaseLookup and same-generation correlated DatabaseSearchReply passed |
| 460 | passed | `passed-stock-to-i2pr-without-client-sam-tunnel-plan469-evidence` | `plans/implementation/ntcp2-transport/460-stock-to-i2pr-ntcp2-without-sam-destination.md` | `plans/closure/ntcp2-transport/460-status-amendment-plan469-outcome.md`: source-selected fresh stock NTCP2, normal product acceptance/reply; historical blocker preserved |
| 461 | passed at controlled loopback scope | `passed-dual-family-request-driven-product-delivery-and-failure-continuity` | `plans/implementation/ntcp2-transport/461-dual-transport-normal-process-i2np-continuity.md` | `plans/closure/ntcp2-transport/461-status.md`: fresh outbound NTCP2 request/reply, SSU2 product receipts, both real-socket family-loss directions, and identity/lifecycle/resource fault controls passed; Plan 435 remains separate |
| 463 | blocked | `blocked-plan463-stock-explore-candidate-not-selected` | `plans/implementation/ntcp2-transport/463-controlled-floodfill-stock-trigger-and-live-reply.md` | `plans/closure/ntcp2-transport/463-status.md`: controlled `caps=f` role is implemented; pinned stock entered Explore four times but selected no i2pr target |

| 467 | passed | `passed-normal-daemon-ntcp2-routerinfo-deliverystatus-roundtrip` | `plans/implementation/ntcp2-transport/467-normal-daemon-ntcp2-databasestore-deliverystatus-roundtrip.md` | `plans/closure/ntcp2-transport/467-status.md`: exact-pinned stock response and normal-daemon tokened product round-trip passed |
| 468 | passed | `passed-stock-ri-admission-healthy-private-netdb-and-publish-selection` | `plans/implementation/ntcp2-transport/468-stock-routerinfo-admission-and-private-netdb-topology.md` | `plans/closure/ntcp2-transport/468-status.md`: exact-pin candidate admitted to four healthy private inventories; source selection matched live receipts (2 candidate, 2 other). |
| 469 | passed | `passed-stock-publish-fresh-ntcp2-product-and-correlated-reply` | `plans/implementation/ntcp2-transport/469-stock-routerinfo-publish-trigger-and-fresh-ntcp2-initiator.md` | `plans/closure/ntcp2-transport/469-status.md`: source-selected fresh NTCP2, product effect/reply, and reverse token correlation passed |

| 470 | blocked | `blocked-java-stock-flood-did-not-produce-owned-ntcp2-dial` | `plans/implementation/ntcp2-transport/470-normal-java-i2p-ntcp2-two-way-product-qualification.md` | `plans/closure/ntcp2-transport/470-status.md`: exact Java selector/store/NTCP2 bid preconditions pass; stock flood dispatched, but no Java-owned dial or i2pr product effect |
| 475 | closed | `closed-java-two-role-authenticated-product-and-reply` | `plans/implementation/ntcp2-transport/475-java-comm-system-send-and-sessioncreated-boundaries.md` | `plans/closure/ntcp2-transport/475-status.md`: pinned Java stock StoreJob → authenticated i2pr Store/DeliveryStatus and independent fresh i2pr lookup → Java handling/correlated reply passed; complete floor passed |

### Plans 459–461: actual daemon NTCP2 product dispatch first (updated 2026-10-12)

**Critical source discovery:** `crates/i2pr-daemon/src/lib.rs::register_ntcp2_service` has **two** authenticated link receive loops that call `dispatch_router_i2np_parts(...)` and discard the returned typed outcome. `router_i2np.rs` only decodes/classifies; the live `ServiceProduct::poll_inbound` consumes SSU2 events. Successful Plan 456 NTCP2 handshake and Plan 455 **i2pr→stock** stock-decoded DatabaseStore do not close normal **stock→i2pr** NetDB/tunnel consumption.

1. **[459](../implementation/ntcp2-transport/459-live-ntcp2-router-i2np-dispatch-and-reply-owner.md), passed at controlled loopback scope:** normal-daemon outbound NTCP2 submitted a product DatabaseLookup and correlated the DatabaseSearchReply on the same peer/link generation. The causal fix allows bounded concurrent routes per peer generation. See [459 status](../closure/ntcp2-transport/459-status.md).
2. **[460](../implementation/ntcp2-transport/460-stock-to-i2pr-ntcp2-without-sam-destination.md), passed at controlled loopback scope:** see [460 Plan 469 amendment](../closure/ntcp2-transport/460-status-amendment-plan469-outcome.md); stock selected the admitted `caps=f` floodfill, initiated fresh NTCP2, and received the real product response. Historical blocked status remains preserved.
3. **[461](../implementation/ntcp2-transport/461-dual-transport-normal-process-i2np-continuity.md), passed at controlled loopback scope:** the request-driven pinned fixture proves route miss → fresh outbound NTCP2 → product DatabaseLookup → same-generation correlated reply; the SSU2-only fixture proves product delivery and reply; real-socket tests prove continuity after either family is removed. The closure records stale/expired identity, generation, cancellation, governor, disabled-default, and repeated-restart controls. See [461 status](../closure/ntcp2-transport/461-status.md). Controlled local evidence does **not** replace Plan 435's external two-family/public admission.
4. **[462](../implementation/core-router-recovery/462-normal-sam-tunnel-build-first-divergence-corrective.md), independent P2, blocked:** M6 preflight, tunnel, and NetDB controls pass, but the normal-process SAM build's first missing transition remains unlocalized. Plan 464 authorizes one causal attempt. See [462 status](../closure/core-router-recovery/462-status.md).

**Do not** claim a generic `dispatch_router_i2np_parts` classification is a live NetDB operation, or that an NTCP2 socket bound implies the normal router can route over it. No protocol or peer-support claim changes merely because these plans are registered. Preserve historical 455/456/457 closures and Plan 101 default-off/advertisement guard.

### Plans 467–469 — real request/reply before stock candidate roulette (2026-10-11)

Source-backed continuation of Plan 459 and the historical Plan 463 trigger result, plus a controlled-scope Plan 460 pass recorded by amendment; historical closure records remain preserved:

- [Plan 467](../implementation/ntcp2-transport/467-normal-daemon-ntcp2-databasestore-deliverystatus-roundtrip.md) **passed**: exact stock ACK and normal `i2pr run` product-owned tokened Store/DeliveryStatus passed. Plan 455's token-zero self-store correctly expected no ACK; ACK remains distinct from third-party NetDB acceptance.
- [Plan 468](../implementation/ntcp2-transport/468-stock-routerinfo-admission-and-private-netdb-topology.md) **passed**: exact-pin private NetDB admission; four owners each loaded five floodfills, candidate was admitted, and source-owned normal publish selection matched live observations (two candidate, two other valid floodfills). See [468 status](../closure/ntcp2-transport/468-status.md).
- [Plan 469](../implementation/ntcp2-transport/469-stock-routerinfo-publish-trigger-and-fresh-ntcp2-initiator.md) **passed**: exact-source normal `RouterContext::Publish` selected the admitted i2pr candidate, used fresh NTCP2, completed normal product acceptance/reply, and stock observed confirmation. The live fixture also completed Plan 467's reverse correlation.
- Resume **existing Plan 461** only after meaningful normal NTCP2 peer receipts; no duplicate dual-transport plan. Pinned Java second-family/public Plan 435 and Plan 101 default-disabled/nonadvertised behavior unchanged. i2pr SAM tunnel Plan 462 remains P2.

**Procedure:** follow [the bounded causal correction policy](../README.md#bounded-execution-and-causal-corrective-policy): source preflight → one legitimate send → classify first missing owned event → fix + regression + replay in same active plan. Do not force a new plan for a log parser, reference bootstrap precondition or source-grounded rerun. Keep independent tiers and immutable closure authority.

### Plan 470 — second-family normal Java NTCP2 (2026-10-12)

[Plan 470](../implementation/ntcp2-transport/470-normal-java-i2p-ntcp2-two-way-product-qualification.md) is **blocked after bounded source-owned publication**; see [470 status](../closure/ntcp2-transport/470-status.md). The separate normal Java 2.13.0 NTCP2-only profile, signed-RI admission, live selector, store policy and NTCP2 bid all passed. A real stock `FloodfillNetworkDatabaseFacade.flood` was invoked, but no Java-owned dial, authenticated inbound link, I2NP effect or correlated reply followed. Stop before Noise diagnosis until the Java CommSystem/transport send transition is observed. J2/J3/J4 and the J5 product safety/lifecycle matrix remain open. Plan 444's selector/store criterion and Plan 435's public/two-family gate are unchanged.

**Required evidence:** i2pr→Java authenticated I2NP and real correlated reply, **fresh Java-initiated** NTCP2→i2pr authenticated I2NP and product response, restart/safety-negative matrix and per-role link generation. **No dependency** on client SAM/tunnel products, Plan 444 Java floodfill preference, or an authorized physical LAN. A controlled second-family result can inform existing Plan 435's router protocol matrix, but cannot authorize broad public transport activation or `specs/support.toml`. Historic 455/457/434 outcomes stay preserved.

### Plan 475 — normal Java two-role product qualification (closed 2026-10-13)

[Plan 475](../implementation/ntcp2-transport/475-java-comm-system-send-and-sessioncreated-boundaries.md)
closed at the pinned Java 2.13.0 controlled-loopback scope; see
[475 status](../closure/ntcp2-transport/475-status.md). The normal stock
`FloodfillStoreJob` sent Java's NetDB-owned RouterInfo through NTCP2, the daemon
accepted it through the shared product owner and returned the owed
DeliveryStatus. A separate daemon generation then initiated fresh NTCP2 and
correlated Java's stock DatabaseSearchReply. The read-only diagnostic waits for
Java's source-defined two-minute lookup-throttle initialization; the router
source and pinned artifacts remain unchanged. The fixture's old empty-TCP
listener probe was removed after pinned `EventPumper` source showed that
inbound EOF-before-data blocks the peer IP. This result does not close Plan
444's Java floodfill role matrix, Plan 435's broad/public qualification, or
change Plan 101's default-off/nonadvertised policy. Plan 470's historical
blocked record remains immutable.

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Historical. Retained checkers: `scripts/check-ntcp2-vectors.sh`, `scripts/check-ntcp2-interoperability.sh`, `scripts/check-constrained-host-lane-boundary.sh`.

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- Reference pins (i2pd 2.61.0, Java I2P 2.13.0) are frozen; lane scripts are fail-closed and environment-gated.

## 11. Completion definition

Closed as exit-with-localized-defect. New NTCP2 work needs a new plan-of-record; do not extend the historical lane.

## 12. Milestone status summary

Full row history is §7. Current authority: Plan 099/100 exit (protocol-defect-localized at noise_authenticated); normal-daemon NTCP2 disabled per Plan 101.
