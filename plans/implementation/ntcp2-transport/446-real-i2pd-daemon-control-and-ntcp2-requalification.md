# Plan 446 — Replace failed NTCP2 stock-library control with real i2pd daemon qualification

Status: **registered / ready**. Date: 2026-10-10. Owning subsystem: NTCP2 transport. Corrective successor to Plan 441 and Plan 445, whose authoritative closures are `plans/closure/ntcp2-transport/{441,445}-status.md`. Roadmaps: `plans/subsystems/ntcp2-transport-roadmap.md`, `plans/subsystems/core-router-recovery-roadmap.md`. Baseline: `work/plans-440-444-emissary-evidence-recovery` after Plan 445; verify working HEAD and registry before execution.

## Objective / why ready

The first stock-to-stock NTCP2 control using two **custom i2pd-library helpers** failed twice. Plan 445 exposed bounded exit reasons `control-dialing-session-not-established` and `control-listener-session-not-established`, but no complete protocol-stage explanation. It is not admissible to blame i2pr or NTCP2 wire code based on these results. Build a **fresh control using unmodified stock i2pd daemon executables and real daemon lifecycles**, not `tools/i2pr-interop/reference/i2pd-current/src/i2pd_current_ntcp2_driver.cpp`. Use naturally available router/NetDB/SAM/tunnel activity to drive peer connects. Reuse pinned source and tools only for reference provenance; treat past helper failures as separate frozen archaeological evidence.

The existing Emissary fork comparison `plans/diagnostics/2026-10-10-emissary-router-qualification-comparison.md` demonstrates why a normal Router→TransportManager→NetDB/tunnel process matters; it is a **read-only architectural reference**, not a dependency or code-copy source. Plan 440 evidence taxonomy passed, so this correction has a stable interface.

## Invariants and scope classification

**Infrastructure:** two pristine stock i2pd processes with independent data directories, identities, real listeners and bounded protocol event evidence; a follow-on controlled i2pr NTCP2 interop invocation after a *passing* control. **Capability:** no public NTCP2 activation; Plan 434 may progress only when authenticated bidirectional I2NP is genuinely demonstrated. **Invariant:** original Plan 101 disables normal-daemon NTCP2, no reference source patch/vendoring, no root/sudo/container/VM/namespace/systemd/public-I2P, no fake Network ID/capability/R, no raw router hashes, keys, endpoint or payload content in persisted evidence, all processes owned/cancellable. **Polish:** one executable help/manifest and concise reason codes. Out: another general-purpose observer framework, synthetic reference-router library wrapper, busy-loop attempts, changing protocol cryptography without a reproducible i2pr-owned defect.

## Bounded implementation packages

1. **Stock source and executable qualification:** Freeze pristine i2pd 2.61.0 revision `635b013a612ff47278ef02acf8580a28e10e26c5`; build or validate the **normal i2pd executable** on the execution host. Record tracked-tree SHA, executable SHA and invocation, using standard supported CLI/config and separate local-only datadirs. Source-inspect official i2pd options for NTCP2 published address, `netid=2`, local/reserved address policy, TCP bind, NetDB imports and a legitimate application-driven outbound connection. Refuse changed source. No custom embedded library driver in the acceptance oracle.
2. **Two-router normal-process fixture:** Start A and B as independently supervised processes with distinct signing/static identities, noncolliding TCP (and any required SAM/I2CP) ports, isolated RouterInfo caches and network-ID-consistent signed records. Disable external reseed/peer leakage and incidental listeners with actual supported configuration. Document how each router learns the other's *verified* RouterInfo without overwriting its identity; seeding known-good signed RouterInfos may bootstrap, but the authenticated session and replies must occur over live TCP. Explicitly allow local test addresses **only in the isolated router profile**, not public I2P. Check that stock peer filters do not discard loopback/private addresses or reject transit/tunnel candidates. Verify session initiation is actually triggered by a normal router task.
3. **Admissibility positive controls:** Execute a bounded outbound connection A→B, then reverse B→A with fresh direction-specific state and one connection per test generation. The observable pass requires both participants showing the same authenticated peer relationship, at least one externally attributable decoded I2NP request and response (or equivalent real router protocol reply), actual authenticated transport delivery and clean shutdown; RouterInfo parsing/TCP connect/SessionConfirmed log alone are insufficient. Use a normal SAM/I2CP client or NetDB/tunnel activity to stimulate the router rather than the legacy C++ test-only `SendMessage` future. An isolated *known-working Emissary* comparison may help diagnose environment, but its result cannot substitute for stock i2pd reference qualification.
4. **Eliminate false positives:** Fault controls must reject wrong network ID, swapped/duplicate identity, stale/bad RouterInfo, reserved-address disabled profile, unbound/mismatched port, absent listener, truncated message and one-sided authentication. Baseline intentionally bad reference config must fail in a distinct typed pre-wire category. Any stock-to-stock failure is `fixture-ineligible` or `stock-control-blocked` until first source-backed cause is established; never label it i2pr protocol failure.
5. **Substitute i2pr only after passing controls:** With the same validated topology, replace first one stock router with i2pr's existing controlled NTCP2 dial/listen surfaces, respecting Plan 101 default-off normal daemon. Prove forward and reverse Noise+data-phase I2NP/DeliveryStatus (small and fragmented) with the *normal i2pd* peer. Do not force a normal-public NTCP2 listener for this controlled test. If source-evidenced i2pr divergence appears, fix the owning protocol/runtime code plus deterministic regression; no speculative crypto mutation.
6. **One frozen evidence contract:** Before executing, freeze finite attempt budgets and per-stage timers; persist only reference digests, profile code, per-session sanitized stage enum/counts, source ownership, direction and failure class. Keep identity and endpoint correlation exclusively in ephemeral process memory; cleanup private logs/directories. On unexpected failure stop this plan with precise source-owned triage, not a new one-line observer plan. Reconcile Plan 441/445 blocked statuses by forward successor links, never by rewriting history.

## Failure, cancellation, concurrency and compatibility

Process group ownership, unique listener port preflight (avoid TOCTOU assumptions), bounded startup/session/I2NP/restart windows; shutdown SIGTERM→bounded SIGKILL only for owned children, release sockets/temp dirs and clear sensitive buffers on every exit. Do not conflate a listener's successful port bind with authenticated peer connection. Preserve locked reference pins, default-off NTCP2 config and all historical test lanes.

## Verification / exact evidence

```sh
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ntcp2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-interop --all-targets
bash scripts/check-ntcp2-vectors.sh
python3 scripts/check-current-pin-ntcp2-runner.py --self-test
python3 scripts/check-core-router-recovery-contract.py --self-test
python3 scripts/check-global-plan-number-uniqueness.py
# new isolated real-i2pd executable control and post-control i2pr driver;
# confirm exact path/CLI after implementation; full AGENTS.md floor
```

**Pass only if** a normal stock-to-stock authenticated session and decoded I2NP round trip passes in both directions, a controlled i2pr↔stock follow-on passes authenticated NTCP2 in both roles with small/fragmented I2NP and cleanup, all negative controls and local checker floor pass, and no normal public support claims are promoted. If the stock-daemon control works but the i2pr integration does not, record precise first divergence and a narrowly owned corrective. If stock control is invalid on loopback, declare **fixture blocked** and propose a separately authorized two-host test rather than lowering the test oracle. The closure `plans/closure/ntcp2-transport/446-status.md` contains actual commands/outcomes, topology and pin manifest, stock control and candidate matrices, CI state, security/compatibility analysis and Plan 434/435 unblock disposition. No closure exists at registration.
