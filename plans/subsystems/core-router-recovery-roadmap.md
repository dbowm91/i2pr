# Core Router Recovery and Operational Completion Roadmap

Status: registered planning line; no production capability promotion. Owner: i2pr core routing. Registered 2026-10-10, baseline main `8fd5eb10824909ddebbd99c8b3800dac71eae211`.

## 1. Purpose and ownership boundary

Close the gap between i2pr's strong runtime-neutral I2P primitives and an independently bootstrapping, genuinely interoperable ordinary I2P router, **including eventual NTCP2 and optional floodfill**. Plans 430–439 are new work, not reopening historical closure tokens. Preserve canonical authority in `GUARDRAILS.md`, `specs/CONFORMANCE.md`, `specs/support.toml`, the closure records and ADR 0026/0027/0030 (floodfill reachability).

Ownership remains: `i2pr-proto` owns codecs; `i2pr-transport-ssu2`/`i2pr-transport-ntcp2` own protocol state machines; `i2pr-runtime` owns transport sockets, timers, cancellation and bounded queues; `i2pr-netdb` owns validated routing metadata and capability policy; `i2pr-tunnel`/`i2pr-client` own tunnel/destination state; `i2pr-daemon` is the composition root. No new independent routing stack or copied reference-router code.

## 2. Work classification

**Capability:** online seed acquisition, qualified external transport participation, multi-hop destination traffic, NTCP2 dual-transport routing, optional transit and floodfill. **Infrastructure:** durable transport identities, transport selection, typed evidence bridge, reputation/resource scheduling. **Invariant:** no unqualified advertisement or anonymity claim, fail-closed quotas, persistent identity separation, validated source records. **Polish:** operable setup/diagnostics, evidence manifests, coherent docs.

## 3. Non-goals

No user-interface implementation; no SSU1/NTCP1; no mandatory ML-KEM/PQ modes; no new SAM/I2CP stack; no intrusive public-network load tests; no unconditional automatic clearnet reseeding without consent; no broad release/anonymity claim from lab tests. Floodfill is optional for an ordinary router but **is required to close this recovery work line**. NAT mapping and IPv6 full parity may require later independently recorded plans if qualification identifies blocked platform prerequisites; cannot be mislabeled supported.

## 4. Audited baseline and evidence

- `plans/closure/ntcp2-transport/099-status.md`: `protocol-defect-localized`, i2pd side `noise_authenticated`, i2pr side missing matching final event. Plan 101 forbids normal-daemon NTCP2 activation; do not characterize this as missing crypto primitives or a proven crypto failure.
- `plans/closure/ssu2/161-status.md`: i2pd 2.61.0 direct IPv4 loopback interoperability passed; `crates/i2pr-daemon/src/config.rs::normalize_ssu2` still forbids non-loopback binds, advertisement and introducer service.
- `crates/i2pr-daemon/src/bootstrap.rs` and `docs/architecture/i2pr-daemon.md`: signed SU3 parsing, cache and offline reseed; startup does not fetch HTTPS SU3. `reseed.enabled` defaults false.
- `plans/closure/transit-tunnels/268-status.md`: one-family receipt evidence passed; ordinary public transit remains disabled.
- `plans/closure/floodfill/303-status.md`: controlled i2pd one-family full matrix passed. `306-status.md`: Java lists/parses `fR` but does not select it with bandwidth class unknown; normal `caps=f` is forbidden. Proposal-170 Plans 350/351 closed type-5 store/serve/consumer gaps, so do not resurrect the historical type-5 deferral.
- Plan 360 made the daemon start and bind enabled local listeners. A process reaching supervision is not proof of external transport or application readiness.

## 5. Target architecture

```text
consented HTTPS reseed -> verified SU3 -> validated RouterInfo cache
                                                   |
persistent router/transport identity -> qualified SSU2 + NTCP2 -> shared transport manager
                                                   |
                                   live NetDB lookup/publication
                                                   |
                            exploratory + destination tunnel pools
                                                   |
                                 SAM/I2CP -> Streaming -> application
                                                   |
                   optional transit participant | optional floodfill server
```

Capability advertisement is an output of observed healthy owners and completed conformance gates, never a configuration string. All transport/session allocations share resource governance. On health loss, withdraw claims before allowing new admissions. Runtime-neutral actors produce typed intents; daemon adapters perform I/O.

## 6. Dependency graph and parallelism

```text
430 baseline/freeze
 |-> 431 public-capable SSU2 ----------|
 |-> 432 HTTPS reseed -----------------+-> 433 live NetDB/tunnels/destination product
 |-> 434 NTCP2 discrepancy recovery ---|          |-> 436 normal transit opt-in
                                       +-> 435 qualified NTCP2 dual transport
                                                  |
                             433 + 436 -> 437 truthful floodfill bandwidth eligibility
                                       -> 438 Java+i2pd floodfill qualification/activation
                   433 + 435 + 436 + 438 -> 439 complete-router product acceptance
```

431/432/434 were independently executable after 430's common contract and baseline closed. Plan 431 stopped at the unavailable non-loopback topology, Plan 432 passed, and Plan 434 remains blocked: corrective Plan 410 added and built the current-network-ID loopback profile against pristine pinned i2pd, then stopped because stock i2pd exposes no public decoded inbound NTCP2 I2NP observation. No admissible two-way evidence can be produced without changing the reference router, which is prohibited. Plan 435 depends on 433 + 434. Plan 437 depends on 433 + 436. Plan 438 depends on 437. Plan 439 is the final evidence gate. Other active work lines are not overridden. A blocked step must record its blocker and bounded next evidence, not loop indefinitely.

## 7. Milestones

| Plan | Status at registration | Implementation | Required proof |
| --- | --- | --- | --- |
| 430 | passed | `plans/implementation/core-router-recovery/430-executable-baseline-and-public-role-gates.md` | Source inventory, readiness/claim contract, fail-closed negative controls; no capability promotion |
| 410 | stopped | `plans/implementation/ntcp2-transport/410-current-pin-ntcp2-loopback-runner.md` | `plans/closure/ntcp2-transport/410-status.md`: current profile and pristine helper build pass; stock i2pd exposes no decoded inbound NTCP2 I2NP observation |
| 414 | in-progress | `plans/implementation/ntcp2-transport/414-stock-i2pd-decoded-message-observation.md` | Corrective to Plan 410; validate pinned source-to-debug-log proof and single-message attribution before any live attempt |
| 431 | stopped | `plans/implementation/core-router-recovery/431-public-capable-ssu2-runtime-and-routerinfo.md` | `431-status.md`: resume only with authorized independently addressed non-loopback reference topology |
| 432 | passed | `plans/implementation/core-router-recovery/432-verified-https-reseed-and-cold-start.md` | `plans/closure/core-router-recovery/432-status.md`: trusted local HTTPS SU3 cold start, validated cache persistence, and restart reuse |
| 433 | blocked on 431,432 | `plans/implementation/core-router-recovery/433-live-router-netdb-tunnel-client-composition.md` | Independent-router multihop end-to-end application path |
| 434 | blocked | `plans/implementation/ntcp2-transport/434-ntcp2-authenticated-link-discrepancy-recovery.md` | `plans/closure/ntcp2-transport/434-status.md`; Plan 414 evaluates a stock-log path while preserving the no-reference-modification rule |
| 435 | blocked on 433,434 | `plans/implementation/ntcp2-transport/435-ntcp2-daemon-activation-and-dual-transport.md` | Dual-transport operational profile, two-family gate |
| 436 | blocked on 433 | `plans/implementation/transit-tunnels/436-normal-daemon-transit-participation-optin.md` | Opt-in participant forwarding and resource/lifecycle acceptance |
| 437 | blocked on 433,436 | `plans/implementation/floodfill/437-truthful-bandwidth-tier-and-java-selection.md` | Measured class and Java candidate selection |
| 438 | blocked on 437 | `plans/implementation/floodfill/438-two-family-floodfill-and-normal-optin.md` | Two independent families; opt-in/withdrawal gate |
| 439 | blocked on 433,435,436,438 | `plans/implementation/core-router-recovery/439-independent-full-router-acceptance.md` | Cold start through multihop communication, restart/fault bounds |

No `plans/closure/.../NNN-status.md` is created by registration; closure requires executed evidence. The indicated blocked statuses are scheduling constraints, not claims that source is missing.

## 8. Cross-cutting invariants

No network-supplied raw router identity becomes trusted without validation. Do not couple transport public key to destination/client identity or allow correlation between multiple client destinations via service metadata. Preserve non-advertised defaults and explicit operator authorization for changes in public exposure. Enforce per-address and global pending-handshake, verified-link, byte, fragment, NetDB, tunnel, and floodfill budgets. All network activity is cancellable and recoverable; snapshots and logs exclude secret/identity-bearing full values. Retain protocol specifications and clean-room provenance; Emissary is an **architectural reference only** outside the Proposal 170 ADR 0028 exception.

## 9. Verification strategy

Run the full `AGENTS.md` floor and every affected static guard. Tests: deterministic clock/fault tests first; exact-pinned stock i2pd 2.61.0 one-family controlled topology; Java I2P 2.13.0 second-family where required; clean datadirs, pinned references, no reference patches, no unconditional skip. Publish stage-based sanitized JSON: commit SHA, evidence topology, bind/network id, reference digests, commands, observed independent events, which rows ran and failed, resource baselines and terminal classification. Public-network validation, if authorized, is passive and rate limited; *never* send fault injection or flood traffic on the public network.

## 10. Risks and decisions

Principal hazards: stale/false RouterInfo addresses, direct clearnet reseed disclosure, NTCP2 harness false negatives, replay/CPU exhaustion, transport-selection dead ends, stalled exploratory pools, Java floodfill class policy, cache poisoning, and excessive performance claims. 430 freezes public-profile policy; any enduring new policy needing an ADR receives a uniquely numbered ADR before implementation. Fix real product bugs rather than broadening test apparatus. Do not claim external reachability based on `127.0.0.1`.

## 11. Completion definition

The operational core is demonstrated only when a fresh identity and empty cache bootstrap with authorized reseed, build healthy externally usable transport links, publish a truthful RouterInfo, perform network database operations, complete real multihop tunnels and end-to-end client data, recover from peer/router restart, and remain resource-bounded. The extended core additionally proves NTCP2 and SSU2, bounded opt-in transit, and two-family opt-in floodfill. `specs/support.toml` promotion requires matching closure evidence and the two-family protocol claim gate in `specs/CONFORMANCE.md`. An operationally functional router remains distinct from an audited production-anonymity product.

## 12. Current planning disposition

Plan 430 closed as an infrastructure and invariant baseline; it activated no transport or network role and changed no support claim. Plan 431 stopped at the unavailable independent non-loopback qualification topology. Plan 432 passed with opt-in signed HTTPS bootstrap and cache restart evidence; it made no public reachability claim. Plan 410 added and tested a current-network-ID loopback profile and built a helper against pristine current-pin i2pd, then stopped because stock i2pd exposes no decoded inbound NTCP2 I2NP receive observation and reference-router patching is prohibited. Plan 434 remains blocked; it has no authoritative two-way I2NP evidence. Plans 433, 435, 436, 437, 438, and 439 remain blocked on their listed predecessor evidence. Existing closure/status records remain authoritative and unchanged; Plan 430's source-comment discrepancy is carried to Plan 438 for reconciliation before any floodfill promotion.
