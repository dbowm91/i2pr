# Plan 442 — SSU2 product-owner construction and independently addressed evidence lane

Status: **registered / blocked on Plan 440**. New bounded corrective splitting Plan 431's implementation work from the unavailable non-loopback qualification host. Historical Plan 431 `stopped` status remains authoritative; this plan does not claim Plan 431 passed.

## Objective, readiness and classification

Enable engineering on the existing SSU2 runtime/daemon without waiting for LAN reference-host approval. Create a separate qualified **independently addressed** evidence lane for eventual Plan 431 closure; never promote the controlled/local evidence to normal public RouterInfo. Existing Plan 161 stock i2pd loopback SSU2 success establishes only one-family experimental direction A+B.

**Capability:** local guarded normal-daemon SSU2 owner capable of legitimate publish policy once qualified. **Infrastructure:** persistent SSU2 static/intro identity; versioned storage and binding; transport manager / single socket lifecycle; test profiles and portable two-host runner. **Invariant:** default off, non-loopback/public address requires explicit operator opt-in and independently verified readiness before publication; no address spoofing or private address in public RouterInfo; per-IP/global pending session limits. **Polish:** redacted host preflight and troubleshooting. Out: force-open public SSU2, artificial public R, introducer advertising without evidence, relying on namespaces/containers in routine CI.

## Ordered implementation

1. Baseline current `normalize_ssu2`, bind validation, `register_ssu2_service`, `i2pr-runtime::Ssu2RuntimeService`, ephemeral test identity vs daemon router identity; preserve tested loopback path. Audit Emissary's separation of socket-context initialization from signed RouterInfo creation as a **behavioral model**, not code.
2. Implement versioned atomic persistent key storage and restart/mismatch handling, strict profile-based non-loopback socket **preflight** and opt-in configuration, typed publish/withdrawal evidence from Plan 430. Prove offline/loopback-only lifecycle with deterministic tests; activation may compile but remains non-public when owner health/evidence absent.
3. Establish explicit LAN manifest: two separate hosts or otherwise pre-authorized independently addressed infrastructure; fixed target IPs and real interface addresses, stock pristine i2pd 2.61.0 pin, separate router identities/data directories, matching network ID, reserved UDP ports, source filter/host firewall, time synchronization, teardown. Do not auto-create root namespaces, container/VM, systemd or public-I2P topology in routine tests. If hosts not supplied, file `environment-unavailable`, but preserve completed source work as infrastructure.
4. Verify source/dest IP, actual endpoint, signed SSU2 RI values, direct/retry/token/cached-token, small/fragmented I2NP in both directions, restart identity, failover and stale-address withdrawal. Use stock-to-stock baseline if failure occurs at candidate admission. Include test negative controls: forbidden/non-local bind, wrong transport key, false R, disabled profile no socket and spoofed datagram.
5. Reconcile `specs/support.toml` **only** after matching independent proof: one-family LAN progression is not broad public two-family conformance. Plan 431 remains stopped until its exact intended external gate is genuinely satisfied; Plan 433 retains its external-readiness dependency on Plan 431, while controlled Plan 443 may progress independently.

## Failure/contension/restart/compatibility

Reserve transport resources before crypto, bounded RX datagrams/reassembly, generation-specific RouterInfo, atomic key migrations and old-generation withdrawal before a new bind; no inadvertent key rotation or leakage. Disabled defaults unchanged; opt-in config rejected before side effects if invalid. Host artifacts only retain count/phase/port-class and reference revision, not full peer identities. On host failure release all sockets and stop admission.

## Verification

`cargo test --locked -p i2pr-transport-ssu2 --all-targets`; `cargo test --locked -p i2pr-runtime --all-targets`; `cargo test --locked -p i2pr-daemon -- --test-threads=1`; Plan 161 exact `--ignored --exact` invocation with required env; bounded new independently addressed runner only on an **authorized** pair; full `AGENTS.md` floor, `cargo fmt --all --check`, `cargo check --locked --workspace --all-targets`, readiness/static-guard self-tests. Do not assume a `tests/integration/ssu2/run-independent.sh` path exists.

Acceptance for this plan's *local component*: compiled runtime-owned persistent identity, negative profile controls, signed address/withdrawal generation, local sessions, and restart/cleanup. Acceptance for **Plan 431 external** remains real non-loopback independent i2pd sessions and correct advertisement with no false public claim; this must be recorded separately, never silently bundled or waived. Stop on unsafe key migration or broken resource owner; record precise blocker and follow-up. Closure `plans/closure/core-router-recovery/442-status.md`.
