# Plan 452 status: blocked — stock NTCP2 control has no dial trigger

Closure token: `blocked-stock-control-no-dial-trigger`

Plan: [`452-real-i2pd-isolated-datadir-and-stock-ntcp2-control.md`](../../implementation/ntcp2-transport/452-real-i2pd-isolated-datadir-and-stock-ntcp2-control.md)

## Outcome

The corrected ordinary-daemon launch was exercised with two unmodified i2pd
2.61.0 processes. Both received disjoint temporary 0700 data roots and explicit
CLI `--datadir <role-root> --conf <role-config>` arguments. Each created its
`router.info` in its assigned root, and each restarted with the other's signed
RouterInfo imported into its own `netDb` and a trusted-router restriction. Both
reopened their configured loopback NTCP2 listeners.

The control did **not** produce a dial trigger: across the bounded 20-second
post-start observation, both logs had zero SessionRequest, authenticated
session, and decoded-I2NP markers. This is `no-dial-trigger`, not a failed
handshake. The Plan 452 stop condition is therefore reached; no i2pr NTCP2
attempt was made. The NTCP2 protocol discrepancy remains untested.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Frozen stock reference | Executed binary reported `i2pd version 2.61.0`; source tree HEAD was the pinned `635b013a612ff47278ef02acf8580a28e10e26c5`. |
| CLI data-directory ordering | Pinned `daemon/Daemon.cpp::Daemon_Singleton::init` calls `ParseCmdline` before reading `datadir` and `DetectDataDir`; config parsing occurs afterward. Pinned `Config.cpp` defines the `datadir` CLI option. |
| Disjoint isolated process roots | Two new `/tmp` role roots were created mode 0700, each process launched with explicit separate `--datadir` and `--conf`; both `router.info` files appeared under the matching CLI root. The roots, configs, and logs were removed after the run. No ambient operator data directory was inspected or modified. |
| Local stock NTCP2 listener | Both fresh processes and both peer-seeded restart processes logged `NTCP2: Start listening v4 TCP port` for their selected loopback ports. Config disabled reseed, UPnP, SSU2, proxies, SAM, I2CP, and I2PControl; the only admitted peer RouterInfo was the other local role. |
| Actual transport dial and authenticated stock control | Not established. In the 20-second seeded/trusted observation each process had 0 SessionRequest lines, 0 authenticated session lines, and 0 decoded-I2NP markers. Classified `no-dial-trigger`; not a transport or handshake failure. |
| i2pr↔i2pd interoperability | Not run because the required stock control did not pass. |
| Existing M6 regression baselines | Prior local run: `run-preflight.sh` passed and `run-netdb.sh` passed. `run-streaming.sh` failed at external i2pd SAM session creation (`SAM session create read`); this SSU2/M6 regression result is separate from the NTCP2 no-dial-trigger outcome and is not interpreted as an NTCP2 result. |

## Commands and outcomes

Executed locally on Linux. The two-process trial used one attempt, loopback
only, a bounded 20-second seeded observation, explicit CLI data roots, no
public I2P traffic, and no modified reference source.

```text
rtk target/interop/cache/ssu2/i2pd/635b013a612ff47278ef02acf8580a28e10e26c5/bin/i2pd --version
  passed: i2pd version 2.61.0
rtk python3 scripts/check-current-pin-ntcp2-runner.py --self-test
  passed
rtk cargo test --locked -p i2pr-transport-ntcp2 --all-targets
  passed (35 tests, 2 suites)
rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1
  passed (120 passed, 1 ignored; 5 suites)
rtk cargo test --locked -p i2pr-interop --all-targets
  passed (32 tests)
rtk bash scripts/check-ntcp2-vectors.sh
  passed
rtk bash tests/integration/m6-interop/run-preflight.sh
  passed (prior local baseline)
rtk bash tests/integration/m6-interop/run-netdb.sh
  passed (prior local baseline)
rtk bash tests/integration/m6-interop/run-streaming.sh
  failed at external SAM session creation; not an NTCP2 observation
```

## Security and limitations

All sockets were loopback. The two i2pd processes had reseed, UPnP, unrelated
listeners, and other transports disabled. Each process received only its own
fresh data root and the other process's validated signed RouterInfo was copied
into that root's pinned HashedStorage layout. No router identity, endpoint, or
raw log was persisted to repository evidence. The scratch roots were deleted.

No custom helper or modified reference was used. No secrets or ambient
`~/.i2pd` data were inspected. The absence of SessionRequest evidence prevents
claims about authentication, decoded I2NP, DeliveryStatus, or i2pr
interoperability.

Findings: critical — none; high — no protocol defect localized; medium — a
stock-owned operation that intentionally triggers an NTCP2 route is still
missing; low — none.

## Unblock audit and disposition

Plan 452 remains **blocked** at stock control's missing normal-process dial
trigger. Plan 434/435 and Plan 101's disabled normal-daemon NTCP2 posture remain
unchanged. No i2pr protocol attempt or wire change is authorized by this
evidence. Plan 451 and Plan 454 are independent and remain executable; Plan 431
and 433 remain gated by their separate SSU2/application evidence. Resuming the
NTCP2 protocol comparison requires a new plan-of-record that supplies a
stock-owned bounded operation and proves the selected peer before spending an
interop attempt.
