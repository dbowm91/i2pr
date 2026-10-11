# Plan 452 — Correct normal-i2pd data-directory isolation and NTCP2 stock control

Status: **registered / ready** (2026-10-10). Owner: ntcp2-transport. Immediate corrective to `plans/closure/ntcp2-transport/446-status.md`, which found that i2pd 2.61.0 initializes its datadir **before** config parsing and omitted `--datadir`; therefore no valid stock-to-stock protocol attempt occurred. Dependency: Plan 440 test/oracle passed, pristine i2pd pinned source/executable available. Independent of Plans 450/451 and LAN 449.

## Objective

Replace unsafe startup inference with one reproducible **ordinary-daemon** stock i2pd↔i2pd control, then—**only after this control passes**—one controlled i2pr↔i2pd NTCP2 forward/reverse interoperability pass. The real stock executables, not custom linked helpers, are the reference oracle. No new handshake observer framework or speculative crypto change.

Classification: **infrastructure** = fixture startup/process ownership and control; **capability** = authenticated NTCP2 plus decoded I2NP only if actually observed; **invariant** = data isolation, default-off normal NTCP2 (Plan 101), no external egress or misadvertisement; **polish** = fixture command documentation.

## Source grounding

Exact pin i2pd `635b013a612ff47278ef02acf8580a28e10e26c5` (2.61.0), source `daemon/Daemon.cpp::Daemon_Singleton::init`. Check actual frozen i2pd supported CLI and config, especially `--datadir`, `--conf`, NTCP2 transport version/options, `reservedrange`/local-address rejection, fixed `netid=2`, external reseed, and any network/UPnP/proxy/discovery options. The I2P NTCP2 spec requires signed RouterInfo address `s`/`i`/`v` and consistent static key/IV for a published endpoint; NTCP2 only transports I2NP, not a generic application pipe. Sources: https://i2p.net/en/docs/specs/ntcp2/ and https://i2p.net/en/docs/overview/network-database/. Compare the pinned Java/i2pd peer-selection behavior only where it affects fixture construction, not as permission to patch a stock router.

## Bounded work

1. Create a **pre-launch admissibility validator**: two disjoint absolute test-owned 0700 root directories, explicit `i2pd --datadir <role-root> --conf <role-config>` (verify real CLI parsing order), separate keys/RouterInfo/caches, no symlinks, fresh configured TCP ports, verified single listener ownership and named network ID, disabled unused listeners/egress. Never rely on config `datadir`. If any child could touch ambient `~/.i2pd` or an existing operator process, refuse launch; never inspect or clean an account's default router directory.
2. Build the known-good topology using *unmodified normal i2pd* in two subprocesses; validate local-address acceptance explicitly (with test-only reserved-address policy). Seed each side's validated, signed RouterInfo without overwriting identities. **Start a real NetDB/tunnel/client operation that intentionally selects the other router** and produces an NTCP2 outbound dial; absence of such a trigger is `no-dial-trigger`, not `handshake-failed`. Do not assume two idle routers will connect spontaneously.
3. Prove A→B and B→A authenticated NTCP2 **in separately initialized direction trials** with both process sides observing a matching session and at least one decoded I2NP request and reply. A TCP bind, SessionConfirmed read, RI acceptance, future resolving, or process-wide status counter alone is not proof. Preserve only allowlisted stage/count receipts and bounded reference digests; use transient in-memory identity correlation.
4. If control passes, use same fixture/address/network-ID policy to replace one stock peer with the existing controlled i2pr NTCP2 adapter (normal daemon remains disabled). Assert authenticated forward/reverse, small and fragmented encrypted I2NP including DeliveryStatus correlation. Only a source-localized deviation may authorize an i2pr wire patch plus deterministic regression test. An alternative Emissary control is optional diagnostic, not substitute for i2pd.
5. Negative controls: changed network ID, static key/IV mismatch, reserved-address denial, missing dial trigger, wrong port, malformed or stale RI, one peer stopped, duplicate data root. First failure must be classified by exact stage/owner. One finite budget per immutable fixture variant; after a **specific configuration correction**, a new bounded run is valid without another instrumentation-only plan.

## Verification, safety and closure

`cargo test --locked -p i2pr-transport-ntcp2 --all-targets`; `cargo test --locked -p i2pr-runtime --all-targets`; `cargo test --locked -p i2pr-interop --all-targets`; `bash scripts/check-ntcp2-vectors.sh`; `python3 scripts/check-current-pin-ntcp2-runner.py --self-test`; explicit isolated stock-daemon lane; full `AGENTS.md` floor for code changes. Fail-closed host-only no-root/no-container/no-VM/no-public-I2P policy; never widen real daemon NTCP2 or persist secrets. Maintain immutable Plans 441/445/446 blocked closures and Plan 101.

Close to `plans/closure/ntcp2-transport/452-status.md` only with **both** proven stock control and actual i2pr↔i2pd forward/reverse or record a clearly bounded residual failed step. If a known-good stock control genuinely cannot use reserved/loopback addresses, stop fixture-attribution and require operator-authorized LAN qualification; do not blame i2pr or start a new stage-observer loop.
