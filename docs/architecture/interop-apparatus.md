# Interoperability and evidence apparatus

> ## ⚠️ STATUS: the NTCP2 interoperability apparatus below is HISTORICAL
>
> **The NTCP2 interop development lane is CLOSED.** The retained NTCP2
> development result is **`protocol-defect-localized` at
> `noise_authenticated`** (Plans 099/100). **NTCP2 is disabled in the
> production daemon** by the Plan 101 guard and is experimental and
> non-advertised.
>
> - The Plan 046 **rootless** and Plan 048/049/050/051 **Multipass** lanes
>   are retained **for archaeology only** — they are not runnable
>   acceptance lanes.
> - The `i2pr-ntcp2-interop`, `i2pr-rootless-sandbox`, and
>   `i2pr-multipass-recovery` skills are **historical / read-only**.
> - Nothing in this document authorizes an NTCP2 interoperability claim,
>   a rootless/Multipass repair or retry, or normal-daemon NTCP2
>   activation. Extending the historical lane requires a new
>   plan-of-record.
> - The **active** lanes are the SSU2 / M6 / M9 / M10 / M11 / M12 /
>   anonymity / I2PControl surfaces under `tests/integration/`, driven by
>   `run-*.sh` + `scripts/check-*-evidence.sh` and described in
>   [Live lanes](#live-lanes) below.

The historical Plan 038–100 apparatus documented in
[Historical NTCP2 apparatus](#historical-ntcp2-apparatus-closed) is
preserved for archaeology. It describes what the closed lane was, not
what is runnable today. Current product authority lives in
[`plans/registry.md`](../../plans/registry.md) and
[`plans/README.md`](../../plans/README.md), not in this file.

## Cross-references

| Concern | Authority |
| --- | --- |
| Crate index, data flow | [`overview.md`](overview.md) |
| Scripts, fixtures, lanes, CI | [`tooling.md`](tooling.md) |
| Dependency allowlist | [`dependency-graph.md`](dependency-graph.md) |
| Binding evidence policy | [`../../specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md) |
| Machine-readable support inventory | [`../../specs/support.toml`](../../specs/support.toml) |
| Plan registry / current authority | [`../../plans/registry.md`](../../plans/registry.md) |
| Planning process and closure records | [`../../plans/README.md`](../../plans/README.md) |
| Routine floor, hard boundaries, historical-skill rule | [`../../AGENTS.md`](../../AGENTS.md) |
| M6 Java bounded diagnostic (Plan 236) | [`236-status.md`](../../plans/closure/mixed-router-interop/236-status.md) |
| Harness reduction / pruning audit | [`audit/2026-09-18-skills-docs-hygiene.md`](audit/2026-09-18-skills-docs-hygiene.md) |
| Historical skills/docs hygiene | [`audit/2026-08-27-skills-pass.md`](audit/2026-08-27-skills-pass.md) |
| NTCP2-era doc audits | [`audit/2026-08-27-doc-audit.md`](audit/2026-08-27-doc-audit.md), [`audit/2026-09-18-doc-audit.md`](audit/2026-09-18-doc-audit.md) |

## Evidence classes

The binding policy is [`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md).
A protocol or feature may be marked implemented only when the applicable
evidence exists — strict decode/encode tests, authoritative or
independently generated cross-implementation vectors, malformed /
truncated / oversized / semantically invalid input tests, state-machine
success / failure / timeout / cancellation / teardown tests, explicit
resource bounds, replay and expiry tests where relevant, documentation
of unsupported behavior, and **no advertised RouterInfo, I2NP, API or
transport capability beyond the tested subset**.

Java I2P and I2P+ share lineage and count as **one** implementation
family for independence. The preferred router-to-router pair is Java I2P
(or I2P+) plus i2pd.

### Raw logs are never evidence

> **Raw reference logs are never evidence. Only sanitized counts,
> digests, and typed dispositions reach an evidence file.**

A log capture is an input to a sanitizing step, not an artifact. This
rule is binding across every lane in this document, historical and
active. The historical apparatus enforced it with an explicit upload
allowlist (sanitized JSON records, the sanitized reference-build summary,
and the aggregate manifest only) and required raw run roots to be deleted
before artifact upload. Private router keys, destination secrets, and raw
application payloads stay in ephemeral scratch directories; evidence
records carry digests, lengths, and counters only.

### Evidence tiers

ADR 0026 ([`0026-staged-interoperability-progression-and-java-debt.md`](../adr/0026-staged-interoperability-progression-and-java-debt.md))
separates an **experimental development gate** from **full conformance**:

- For **experimental development progression**, a non-advertised
  subsystem may continue after local conformance plus at least one
  exact-pinned independent implementation demonstrates the relevant
  controlled external path. This authorizes later implementation work
  only — not public exposure, production-readiness language, broad
  advertisement, or a full interoperability claim.
- For **full router-to-router conformance or broad advertisement**, two
  independent implementation families must interoperate for the claimed
  surface.
- For **client/application protocols** (Streaming, SAM, I2CP,
  service-tunnel profiles) independent evidence appropriate to the
  surface is required; a second full router family is not automatically
  a hard gate merely because the path traverses routers.

The NTCP2 lane's own four-tier ladder (ADR 0023) is historical and
closed; see [Evidence ladder](#evidence-ladder-adr-0023-historical).

## Reference pins

These pins are exact-SHA and **must not change without a new plan**. The
i2pd pin is the mandatory reference; the Java I2P pin is secondary.

| Reference | Version | Pin | Role |
| --- | --- | --- | --- |
| i2pd | `2.61.0` | `635b013a612ff47278ef02acf8580a28e10e26c5` | **Mandatory** router reference |
| Java I2P | `2.13.0` | `9134f808337b401e8e53c73734c81fab04280c9d` | **Secondary** full-router reference |
| go-i2cp | — | `b529ee1c10a6011558b4d69fc9436a4afc489eac` | I2CP independent client |
| i2psam | — | `b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac` | Counted SAM client |
| i2plib | — | `6edf51cd5d21cc745aa7e23cb98c582144884fa8` | Counted SAM client |

The SAM client pins live in
[`scripts/interop/fetch-sam-clients.sh`](../../scripts/interop/fetch-sam-clients.sh).
Supporting sources are catalogued in
[`specs/SOURCES.md`](../../specs/SOURCES.md) and
[`specs/IMPLEMENTATIONS.md`](../../specs/IMPLEMENTATIONS.md).

> **Frozen NTCP2-era pins.** The historical NTCP2 harness carries its own
> older lock — Java I2P `2.12.0` at `2800040deee9bb376567b671ef2e9c34cf3e30b6`
> and i2pd `2.60.0` at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` in
> [`tests/integration/ntcp2/references.lock.toml`](../../tests/integration/ntcp2/references.lock.toml).
> Those are **frozen historical** values and are **not** the current pins
> above. See [`tooling.md`](tooling.md), which records the same
> distinction.

## The fail-closed discipline

Environment-gated tests are **`#[ignore]`-gated**. An ordinary
`cargo test` run compiles them and skips them. An explicit run requires
`--ignored --exact`:

```text
cargo test --locked -p i2pr-daemon --test m11_transit_i2pd_external -- --ignored --exact
```

**Missing environment must FAIL, never silently pass.** A missing
reference, endpoint, or pin is a lane failure, not a skip.

Forbidden in lanes, drivers, workflows, and checkers:

- `|| true`, `set +e`-style suppression, or any early-return-success
- `continue-on-error`
- filename filtering that narrows a lane to the passing subset
- fake or synthesized peer environment
- broad exclusions
- production wire changes made to turn a lane green

Reference-revision pins are verified on every run, and every lane
re-hashes its artifacts rather than trusting a recorded digest. Counted
rows must be produced by an executed command or test; a `passed` row
without an executed command behind it is rejected by the
evidence-integrity checkers.

## Live lanes

Each live lane is a `tests/integration/<area>/run-*.sh` driver plus a
`scripts/check-*-evidence.sh` evidence-integrity checker. Routine
checkers run in [CI](../../.github/workflows/ci.yml); external lanes are
manual [`workflow_dispatch`](../../.github/workflows) workflows. All lanes
are loopback-only, unprivileged, and free of root/sudo/namespaces/
containers/VMs/public-I2P.

| Lane | Driver(s) | Evidence checker(s) | Workflow |
| --- | --- | --- | --- |
| **anonymity** | [`run-plan312-streaming.sh`](../../tests/integration/anonymity/run-plan312-streaming.sh) | `check-http-anonymity-evidence.sh`, `check-streaming-fingerprint-evidence.sh` | none (local, evidence-arg) |
| **floodfill** (M12) | [`run-i2pd.sh`](../../tests/integration/floodfill/run-i2pd.sh), [`run-java-floodfill.sh`](../../tests/integration/floodfill/run-java-floodfill.sh) | `check-m12-floodfill-qualification-evidence.sh` (`--self-test`), `check-m12-floodfill-boundaries.sh` | none (manual local) |
| **i2cp** (M9) | [`run-independent.sh`](../../tests/integration/i2cp/run-independent.sh) | `check-i2cp-acceptance-evidence.sh` | [`i2cp-external.yml`](../../.github/workflows/i2cp-external.yml) |
| **i2pcontrol** | [`run-differential.sh`](../../tests/integration/i2pcontrol/run-differential.sh) | `check-i2pcontrol-acceptance-evidence.sh` | none (CI corpus) |
| **m11-transit** | [`run-i2pd.sh`](../../tests/integration/m11-transit/run-i2pd.sh) | `check-m11-transit-qualification-evidence.sh`, `check-m11-transit-boundaries.sh` | [`m11-transit-external.yml`](../../.github/workflows/m11-transit-external.yml) |
| **m6-interop** | [`run-m6-mixed-router.sh`](../../tests/integration/m6-interop/run-m6-mixed-router.sh), [`run-java.sh`](../../tests/integration/m6-interop/run-java.sh), [`run-preflight.sh`](../../tests/integration/m6-interop/run-preflight.sh), [`run-tunnels.sh`](../../tests/integration/m6-interop/run-tunnels.sh), [`run-netdb.sh`](../../tests/integration/m6-interop/run-netdb.sh), [`run-destination.sh`](../../tests/integration/m6-interop/run-destination.sh), [`run-streaming.sh`](../../tests/integration/m6-interop/run-streaming.sh) | `check-m6-mixed-router-acceptance-evidence.sh`, `check-m6-final-closure-evidence.sh`, `check-netdb-tunnel-evidence.sh`, `check-destination-tunnel-evidence.sh`, `check-exploratory-tunnel-evidence.sh`, `check-streaming-tunnel-evidence.sh` | [`m6-mixed-router-external.yml`](../../.github/workflows/m6-mixed-router-external.yml) |
| **sam** (M7) | [`run-independent.sh`](../../tests/integration/sam/run-independent.sh) | `check-sam-acceptance-evidence.sh` | [`sam-external.yml`](../../.github/workflows/sam-external.yml) |
| **service-tunnels** (M10) | [`run-independent.sh`](../../tests/integration/service-tunnels/run-independent.sh), [`run-plan213-generic.sh`](../../tests/integration/service-tunnels/run-plan213-generic.sh), [`run-plan214-applications.sh`](../../tests/integration/service-tunnels/run-plan214-applications.sh), [`test-plan215-tunnels-conf.sh`](../../tests/integration/service-tunnels/test-plan215-tunnels-conf.sh) | `check-service-tunnel-acceptance-evidence.sh`, `check-service-tunnel-boundaries.sh` | [`service-tunnels-external.yml`](../../.github/workflows/service-tunnels-external.yml) |
| **ssu2** (M8) | [`run-independent.sh`](../../tests/integration/ssu2/run-independent.sh) | `check-ssu2-acceptance-evidence.sh` | [`ssu2-external.yml`](../../.github/workflows/ssu2-external.yml) |
| **ntcp2** | *no driver script* — closed lane, see [Historical](#historical-ntcp2-apparatus-closed) | `check-ntcp2-interoperability.sh` (archaeology guard) | three historical workflows below |

`tests/integration/service-tunnels/run-independent.sh` delegates its
remote leg to `run-plan214-applications.sh`.

## Evidence-integrity checkers

Every checker below rejects bookkeeping that would let a claim be marked
`passed` without an executed command or test behind it. They run in
routine CI where listed in [`../../AGENTS.md`](../../AGENTS.md).

| Checker | Guards |
| --- | --- |
| `check-sam-acceptance-evidence.sh` | SAM 3.1 acceptance rows in `sam/run-independent.sh` |
| `check-ssu2-acceptance-evidence.sh` | SSU2 v2 acceptance rows in `ssu2/run-independent.sh` |
| `check-i2cp-acceptance-evidence.sh` | I2CP independent-LeaseSet2 lifecycle rows |
| `check-service-tunnel-acceptance-evidence.sh` | M10 service-tunnel external rows |
| `check-m6-mixed-router-acceptance-evidence.sh` | M6 two-family per-layer acceptance rows |
| `check-m6-final-closure-evidence.sh` | M6 final closure gate; evidence-consuming, run only after a complete manual external workflow |
| `check-netdb-tunnel-evidence.sh` | NetDB lane rows in `m6-interop/run-netdb.sh` |
| `check-destination-tunnel-evidence.sh` | Destination lane rows in `m6-interop/run-destination.sh` |
| `check-exploratory-tunnel-evidence.sh` | Exploratory tunnel lane rows |
| `check-streaming-tunnel-evidence.sh` | M6 i2pd mixed-router Streaming fingerprint invariants |
| `check-streaming-fingerprint-evidence.sh` | Plan 312 anonymity Streaming fingerprint evidence (takes an evidence dir) |
| `check-http-anonymity-evidence.sh` | HTTP anonymity profile evidence (takes an evidence root) |
| `check-m11-transit-qualification-evidence.sh` | M11 production self-reply qualification; adds Plan 257 far-side / full-drain / source-lock rejections |
| `check-m11-transit-boundaries.sh` | M11 composition boundaries |
| `check-m12-floodfill-qualification-evidence.sh` | M12 floodfill matrix rows (`--self-test` mode supported) |
| `check-i2pcontrol-acceptance-evidence.sh` | I2PControl differential counted rows from the executed Rust corpus |
| `check-java-source-lock-gating.sh` | Java source-lock test environment gating and ordinary CI wiring |
| `check-ntcp2-interoperability.sh` | **Historical** NTCP2 harness boundary invariants (archaeology guard, not a live lane) |

Boundary/vector checkers that are not evidence checkers but gate the same
apparatus: `check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`, `check-fixture-manifest.sh`,
`check-constrained-host-lane-boundary.sh`,
`check-rootless-interop-boundary.sh`,
`check-multipass-interop-boundary.sh`, `check-service-tunnel-boundaries.sh`,
`check-m11-per-epoch-composition.sh`, `check-service-anonymity-boundaries.sh`.

## M6 Java bounded diagnostic (Plan 236)

Plan 236 is a **bounded diagnostic, not a Java-family pass**. It
source-locks the exact Java I2P `2.13.0` Streaming response path and
**stops** at `P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP`. It does
**not** infer Router-A or i2pr behavior and does **not** change
production code.

[`scripts/interop/check-m6-java-response-source-lock.sh`](../../scripts/interop/check-m6-java-response-source-lock.sh)
validates this exact path against the pinned checkout:

```text
ConnectionPacketHandler.receivePacket(SYN)
  -> Connection.eventOccurred()
  -> SchedulerReceived.eventOccurred()
  -> Connection.sendPacket(PacketLocal)
  -> PacketQueue.enqueue(PacketLocal)
  -> boolean I2PSession.sendMessage(... SendMessageOptions)
```

The source lock and the sanitized Plan 236 terminal are durable facts.
**A returned `I2PSocket` does not prove response emission**, Router-A
I2CP admission, tunnel dispatch, or i2pr delivery. Java-family M6
remains unclaimed and no production corrective is authorized. The
driver is [`m6-interop/run-java.sh`](../../tests/integration/m6-interop/run-java.sh);
the terminal is recorded in
[`236-status.md`](../../plans/closure/mixed-router-interop/236-status.md).
M6 exact-pinned i2pd progression is recorded as passed; the Java
full-router lane remains compatibility debt.

## Historical NTCP2 apparatus (closed)

> Everything in this section describes a **closed** lane. It is preserved
> for archaeology. Do not run, repair, retry, or extend it without a new
> plan-of-record. NTCP2 stays experimental and non-advertised, and
> normal-daemon NTCP2 is disabled per the Plan 101 guard.

### Harness surface (surviving)

The Plan 099 harness reduction pruned most plan-numbered modules. The
surviving surface under
[`tests/integration/ntcp2/harness/`](../../tests/integration/ntcp2/harness):

```text
execution_lane.py            i2pd_direct_driver.py       reference_trigger_v4.py
interop_topology.py          minimal_i2pd_probe.py       rootless_inner_runner.py
minimal_i2pd_reverse_probe.py plan083_runner.py           rootless_supervisor.py
plan084_runner.py            plan099_exit_gate.py        rootless_topology.py
preflight_runner.py          reference_event.py          topology.py
reference_topology.py
```

plus the focused tests `test_execution_lane.py`,
`test_i2pd_direct_driver.py`, `test_i2pd_direct_control.py`,
`test_minimal_i2pd_probe.py`.

The functional entry points were
[`scripts/interop/run-minimal-i2pd-host-loopback-probe.py`](../../scripts/interop/run-minimal-i2pd-host-loopback-probe.py)
(the only allowed live-subprocess entry point),
[`plan083_runner.py`](../../tests/integration/ntcp2/harness/plan083_runner.py) /
[`plan084_runner.py`](../../tests/integration/ntcp2/harness/plan084_runner.py)
(forward/reverse runners), and
[`preflight_runner.py`](../../tests/integration/ntcp2/harness/preflight_runner.py)
(listener-only preflight). Supporting configuration survives in
[`references.lock.toml`](../../tests/integration/ntcp2/references.lock.toml),
[`mixed-scenarios/`](../../tests/integration/ntcp2/mixed-scenarios),
[`scenarios/`](../../tests/integration/ntcp2/scenarios),
[`reference-scenarios/`](../../tests/integration/ntcp2/reference-scenarios),
[`reference-drivers/`](../../tests/integration/ntcp2/reference-drivers),
[`qualification/`](../../tests/integration/ntcp2/qualification),
[`evidence-receipts/`](../../tests/integration/ntcp2/evidence-receipts),
and [`reference-observation-catalog.toml`](../../tests/integration/ntcp2/reference-observation-catalog.toml).

#### Pruned surfaces

The Plan 099 harness reduction and later hygiene passes removed these
paths. They are cited here as **plain code spans, not links**, so no dead
reference remains:

| Pruned path | Status |
| --- | --- |
| `tests/integration/ntcp2/harness/test_plan095.py` | pruned |
| `tests/integration/ntcp2/harness/test_plan096.py` | pruned |
| `tests/integration/ntcp2/harness/test_plan097.py` | pruned |
| `tests/integration/ntcp2/harness/test_plan098.py` | pruned |
| `scripts/check-plan095-workflow.sh` | pruned (also recorded as pruned in [`tooling.md`](tooling.md)) |

The Plan 095/096/097/098 CI workflow correctness work is retained in
`plans/` as audit records. See
[`audit/2026-09-18-skills-docs-hygiene.md`](audit/2026-09-18-skills-docs-hygiene.md),
which records the harness reduction and the `check-plan095-workflow.sh`
pruning.

### Build contract and topology (historical)

Preparation runs on the supported Ubuntu 24.04 amd64 host and may fetch
only lock-listed sources, the IzPack artifact, and declared packages.
Execution is offline and namespace-isolated; there is no default route,
DNS, forwarding path, or public egress. Secret-bearing state lived only
under `target/interop/runs/<run-id>/`; sanitized records were finalized
under `target/interop/evidence/` after processes and namespaces were gone.
Cache identity hashed the canonical reference, full source object ID,
lock digest, the `ubuntu-24.04-amd64` host contract, and reviewed build
command version; `--offline` could not fetch a missing source.

ADR 0015 ([`0015-ubuntu-reference-router-harness.md`](../adr/0015-ubuntu-reference-router-harness.md))
fixed the reference-router harness boundary (**accepted**, extended by
Plans 041/042/044, amended by Plan 046). ADR 0016
([`0016-ubuntu-build-system-interop-gates.md`](../adr/0016-ubuntu-build-system-interop-gates.md))
fixed the ordered fail-closed build-system promotion gates
(**accepted**, amended by Plan 046): `contract → reference-build →
reference-offline-reuse → environment-smoke → reference-crosscheck-ipv4 →
i2pr-handshake-smoke-ipv4 → full-matrix → evidence-validation →
cleanup-verification`, with `verify-clean-host.sh` residual-state
verification. ADR 0020
([`0020-plan053-evidence-pipeline-integrity.md`](../adr/0020-plan053-evidence-pipeline-integrity.md))
fixed the evidence-pipeline integrity boundary (**Accepted** for Plan 053).

### Evidence ladder (ADR 0023, historical)

ADR 0023
([`0023-staged-ntcp2-interoperability-evidence.md`](../adr/0023-staged-ntcp2-interoperability-evidence.md),
**Accepted**) separated NTCP2 interoperability evidence into bounded
tiers and forbade lower-tier promotion into release bundles:

1. **Level 1 — external loopback smoke** (`evidence_tier = external-loopback-smoke`).
2. **Level 2 — repeated development interoperability** (`evidence_tier = repeated-development-interop`).
3. **Level 2D — conditional Emissary differential validation** (`evidence_tier = conditional-differential`).
4. **Level 3 — release qualification** (`evidence_tier = release-qualification`).

A record declares exactly one tier; a release-bundle validator rejects
any lower tier. ADR 0023 does **not** supersede ADR 0022.

### Direct reference drivers (ADR 0022)

ADR 0022
([`0022-direct-reference-router-ntcp2-interop-drivers.md`](../adr/0022-direct-reference-router-ntcp2-interop-drivers.md),
**Accepted**) chose two-process direct transport drivers: one reference
router plus one i2pr process in a sealed namespace or isolated guest, with
no support router, floodfill, reseed, SAM, I2CP, HTTP/I2PControl, or
tunnel pool in the primary path. It replaced the conclusion of
[`0021-minimal-java-support-topology.md`](../adr/0021-minimal-java-support-topology.md)
(**Rejected** by Plan 058) without rewriting it; the Java support
topology was never implemented. ADR 0025
([`0025-plan090-i2pd-driver-routerinfo-correction.md`](../adr/0025-plan090-i2pd-driver-routerinfo-correction.md),
**Accepted** for Plan 090) corrected the i2pd direct-driver RouterInfo
emission and pre-TCP classification.

### Constrained-host lane order (ADR 0024, historical)

ADR 0024
([`0024-constrained-host-ntcp2-execution-lanes.md`](../adr/0024-constrained-host-ntcp2-execution-lanes.md),
**Accepted** for Plan 077) fixed the fail-closed, ordered
capability-selection list for hosts that cannot use the rootless or
Multipass paths:

1. an already accessible rootful Docker daemon, one `--network none` container;
2. QEMU system emulation with TCG and `-nic none`;
3. inherited connected descriptors with `no_new_privs`/seccomp, explicitly reduced-scope;
4. a manually triggered remote Linux workflow with documented isolation;
5. a typed **no-full-runtime-lane** result.

The inspection-only probe is
[`scripts/interop/probe-constrained-host-lanes.sh`](../../scripts/interop/probe-constrained-host-lanes.sh);
the manifest/qualification record is validated by
[`execution_lane.py`](../../tests/integration/ntcp2/harness/execution_lane.py);
the static guard is
[`check-constrained-host-lane-boundary.sh`](../../scripts/check-constrained-host-lane-boundary.sh).
A tool, workflow, or reduced-scope capability is **not** a qualification.

### Rootless sealed-namespace lane (ADR 0017, historical)

ADR 0017
([`0017-rootless-sealed-namespace-interop-evidence.md`](../adr/0017-rootless-sealed-namespace-interop-evidence.md),
**accepted** for Plan 046) replaced the host-global namespace requirement
with a rootless, process-scoped user/network/mount/PID sandbox
(`rootless-sealed-single-netns`, `unprivileged-userns`) that an ordinary
user could run without sudo, setuid helpers, host-visible namespaces,
host veths, or firewall mutation. The outer entrypoint is
[`scripts/interop/rootless-enter.sh`](../../scripts/interop/rootless-enter.sh);
the inner supervisor is
[`rootless_supervisor.py`](../../tests/integration/ntcp2/harness/rootless_supervisor.py);
the static guard is
[`check-rootless-interop-boundary.sh`](../../scripts/check-rootless-interop-boundary.sh).
On a passed record, `IsolationAttestation` digests bound to the
evidence and parent-network pre/post digests must be byte-equal. Plan 046
closed on this host with the typed blocker
`blocked_unprivileged_user_namespace` (the AppArmor
`kernel.apparmor_restrict_unprivileged_userns=1` baseline).

### Multipass recovery lane (ADR 0018, historical)

ADR 0018
([`0018-multipass-rootless-interop-environment.md`](../adr/0018-multipass-rootless-interop-environment.md),
**Accepted**) adopted a lifecycle-owned permissive rootless environment:
atomic lifecycle reservation, per-run/per-instance locks, explicit
transition states, ownership proof by host/guest token and digest
match, explicit `--adopt-owned`/`--resume-owned`/`--recreate-owned`/
`--destroy-owned`, and a sanitized export. `--inspect` is read-only;
normal execution never silently adopts, recreates, stops, deletes, or
purges. ADR 0019
([`0019-guest-level-nft-marker-clarification.md`](../adr/0019-guest-level-nft-marker-clarification.md),
**accepted** for Plan 051) reconciled the guest-level nft egress-deny
marker. The subtree survives under
[`scripts/interop/multipass/`](../../scripts/interop/multipass); the
static guard is
[`check-multipass-interop-boundary.sh`](../../scripts/check-multipass-interop-boundary.sh).
This lane could not complete on the constrained host (Plan 051).

### Retained NTCP2 result and exit gate

The retained development result is `protocol-defect-localized` at
`noise_authenticated` (Plans 099/100). The exit-gate vocabulary is
exactly three values, implemented in
[`plan099_exit_gate.py`](../../tests/integration/ntcp2/harness/plan099_exit_gate.py)
and covered by the `Plan099ExitGateTests` class in
[`test_minimal_i2pd_probe.py`](../../tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py):

```text
passed                        # all four per-attempt records passed with clean cleanup
protocol-defect-localized     # a direction reached tcp_connected or later, then failed
                              # before the correlated DeliveryStatus pass
environment-or-harness-blocked  # earliest nonpassing path is pre-TCP/build/startup
```

Plan 101 disabled and made unenableable normal-daemon NTCP2. The three
retained historical workflows — [`ntcp2-interop-ubuntu.yml`](../../.github/workflows/ntcp2-interop-ubuntu.yml),
[`ntcp2-interop-rootless.yml`](../../.github/workflows/ntcp2-interop-rootless.yml),
and [`ntcp2-interop-host-loopback-development.yml`](../../.github/workflows/ntcp2-interop-host-loopback-development.yml)
— are `workflow_dispatch`-only with `contents: read`; they are not a
routine acceptance path and are not run for routine work.

Plans 099/100 forbid adding new `test_planNNN.py` files, new
plan-number-specific runners, or new plan-token static checks.
Historical plan documents remain in `plans/` as audit records, not
executable contracts.

## Skills

Skill bundles are canonical under [`.opencode/skills/`](../../.opencode/skills)
(`.agents/skills` is a symlink to the same directory). The three below
are **historical and read-only for archaeology**; routine work uses
`i2pr-local-dev`, `i2pr-architecture`, or `i2pr-planning`.

| Skill | Scope |
| --- | --- |
| `i2pr-ntcp2-interop` | Historical Plan 038–100 NTCP2 harness; read/reproduce the closed harness surface, prepare/validate reference routers, validate evidence. Do not activate NTCP2 in the production daemon or extend the lane without a new plan-of-record. |
| `i2pr-rootless-sandbox` | Historical Plan 046 rootless sealed-namespace sandbox; run the rootless probe, enter the sandbox, validate the typed blocker taxonomy, update the static boundary checker. |
| `i2pr-multipass-recovery` | Historical Plan 048/049/050/051/053 Multipass recovery lane; create/adopt/resume/recreate/destroy a guest, run the evidence lane, classify cloud-init failure, troubleshoot the host-side Plan 046 blocker bridge. |

`AGENTS.md` forbids root/sudo/namespaces/containers/VM/public-I2P for
routine acceptance, and all three of these lanes require them; they are
therefore never routine. See
[`i2pr-local-dev`](../../.opencode/skills/i2pr-local-dev) for the active
lane surface.
