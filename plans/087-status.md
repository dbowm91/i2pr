# Plan 087 status: first real i2pr-to-i2pd host-loopback probe

## Status

The Plan 087 instrumented forward direction did **not** pass. The
latest clean committed-head attempts are:

1. The Plan 090 four-correction set landed. The Plan 090 instrumented
   forward attempt authenticated the i2pd listener
   (`listener_ready`) and reached TCP, then recorded a NTCP2
   protocol failure with the i2pd NTCP2 transport reading
   `End of file` on the first handshake body read while the i2pr
   reports a data-phase `receiver_delivery_status_missing` terminal.
2. The Plan 091 four-correction set landed (i2pd direct driver
   `SetNetID`, explicit logger start/stop, `tcp_accepted` wait,
   symmetric `DeliveryStatus` send). The Plan 091 instrumented
   forward attempt authenticated the i2pd listener, the i2pr dialer
   started, the i2pd NTCP2 transport accepted the TCP connection,
   the i2pd log shows `NTCP2: SessionRequest read error: End of
   file`, and the i2pr status log shows `tcp_connected` immediately
   followed by `terminal,, result: rejected,, reason_code:
   receiver_delivery_status_missing`.
3. The Plan 092 privacy-safe observation infrastructure landed and
   reproduced the same forward outcome on the first clean
   committed-head head. The retained log message
   `NTCP2: Receive length read error: End of file` is from
   `NTCP2Session::HandleReceivedLength` in `libi2pd/NTCP2.cpp`, the
   authenticated data-phase length reader, **not** the
   `HandleSessionRequestReceived` handshake path. The Plan 092
   "Branch A — i2pr runtime state-machine defect" classification is
   **superseded** by Plan 093.

```text
status = open-pending-plan095-ci-forward-evidence-pair
forward_digest = non-zero (Plan 091 instrumented retained record)
                  and Plan 092 retained record (diagnostic history only)
corrective_source_commit = <set-by-plan-094-corrective-commit>
plan_086 = host-loopback-development-ready
plan_090 = routerinfo-correction-landed
plan_091 = historical-partial-correction
plan_092 = superseded-by-plan093
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-closure-next-executable
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
```

Plan 095 is the single next executable plan. Plan 095 implements
the GitHub Actions `ubuntu-24.04` host-loopback live-wire
closure lane that supersedes the local host environment-blocked
path that Plan 094 (plan094) expected to run. Plan 094 remains
implementation-landed; its live closure environment is blocked
on this host. Plan 088 may not run before Plan 095 closes with
a passing instrumented forward record and a passing control
forward record bound to the same CI evidence pair.
Plan 079 and Plan 072 remain blocked pending the Plan 088 decision.

## Historical record

Plan 087 also delivered the runner-architecture fix that converted the
Plan 086 blocking `i2pd_direct_driver_invocation` into a
placement-owned concurrent i2pd listener and i2pr dialer
(Plan 086 WP6 / Plan 087 WP2). The committed source SHA used for the
Plan 086/087 digest was `e34b87c2a45fbb318ea1245be473932cfb7e05d7`,
the `host-loopback-development` topology, network ID `99`, a fresh
run root, and the host loopback endpoint
`127.0.0.1:<random_port>` for both peers.

The Plan 087 implementation surface is otherwise unchanged from
Plan 086/083: the wrapper is still a thin parser and dispatcher,
the placement is still the only subprocess owner, the runner still
uses the Plan 082 `prepare` boundary, the Plan 065 strict scenario
validation, and the Plan 064 strict driver config; the runner still
emits one `i2pr-minimal-i2pd-probe-v1` record per direction; and the
record still carries the bounded topology metadata
(`development_only`, `release_qualified = false`,
`isolation_qualified = false`).

## Plan 087 runner defects closed

| Prior defect                                              | Corrective surface                                                                                                         |
|-----------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------|
| `prepare_state_path_invalid` from `mkdir(...)` (`0o755`)   | runner creates `raw/`, `state/`, `i2pd-data/`, `i2pd-output/`, `exchange/` with `mode=0o700`                              |
| `capture_stdout=False` lost the i2pr prepare JSON          | runner passes `capture_stdout=True` to placement `run` and parses `placement.run.stdout` directly                            |
| `source_inspection_record_sha256` defaulted to `0*64`      | runner accepts `source_inspection_record_sha256` (defaulting to `reference_tree_sha256`) and threads it into both i2pd calls  |
| wrapper passed only 4 of the 6 required runner digests    | wrapper now passes `reference_tree_sha256` and `source_inspection_record_sha256` to `execute_real_probe`/`execute_reverse_probe` |
| listener was launched through blocking `subprocess.run`    | runner rewrites Phase 4–6 to `HostLoopbackDevelopmentPlacement.popen()` for i2pd listener and i2pr dialer concurrently  |
| post-inspect RouterInfo not copied to scenario exchange   | runner copies `output_dir/router.info` to `exchange/i2pd-router.info` and verifies the SHA-256 before scenario render     |

## Validation

The Plan 087 implementation surface was exercised by the focused test
suite plus the exact live command above. The focused checks all pass
before the live command:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan086.py'       passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py' passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'       passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'       passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py' passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py' passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'       passed
bash scripts/check-dependency-direction.sh                 ok
bash scripts/check-runtime-boundaries.sh                   ok
bash scripts/check-fixture-manifest.sh                     ok
bash scripts/check-ntcp2-interoperability.sh                ok
bash scripts/check-rootless-interop-boundary.sh            ok
bash scripts/check-multipass-interop-boundary.sh           ok
bash scripts/check-ntcp2-loopback-smoke-boundary.sh        ok
cargo fmt --all --check                                   ok
cargo check --workspace --all-targets                      ok
cargo test --workspace                                    passed
cargo clippy --workspace --all-targets --all-features -- -D warnings  ok
```

## Future plan unblocking

| Plan | Precondition                                              | Status after Plan 095 |
| ---  | ---                                                       | ---                    |
| 095  | Plan 094 implementation landed; CI host-loopback live-wire closure lane | next executable |
| 088  | Plan 095 must record `passed` instrumented and control forward records | remains blocked |
| 079  | Plan 088 must record `two-way-development-probe-passed`     | remains blocked         |
| 072  | Plan 088 must record `ambiguous-reference-divergence` with one exact diagnostic question | remains inactive |

The retained Plan 090 and Plan 091 instrumented forward records are
preserved at `/tmp/opencode/plan091-evidence/forward/forward-record.json`
and earlier run roots under `/tmp/opencode/`. The records are diagnostic
history only; they are not passing forward records. Plan 092 owns the
ownership probe and the forward closure.