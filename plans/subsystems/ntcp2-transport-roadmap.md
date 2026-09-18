# NTCP2 Transport Roadmap

Status: closed

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

Plan 099/100 exit (protocol-defect-localized at noise_authenticated); normal-daemon NTCP2 disabled per Plan 101.

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
