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

The Plan 098 runner/provenance boundary corrective pass
delivered the implementation surface required for any future
authoritative Plan 095 dispatch. The forward, reverse, and
preflight runners now accept the explicit `i2pr_binary` path as
a mandatory first-class argument and refuse an attempted-live
execution when the path is missing or its measured digest does
not match the supplied SHA-256. The wrapper threads the exact
caller-supplied path to every runner, distinguishes the i2pr and
i2pd build-manifest digests, and binds the i2pd driver role to
the exact role-specific build manifest via the new
`--attempt-kind` flag. The Plan 095 final gate now validates the
record claims against the actual downloaded artifacts and
role-specific manifests.

The first authoritative Plan 095 CI dispatch on 2026-08-10 was
a **pre-protocol runner/provenance failure**, not a wire-level
NTCP2 classification. The live runner reached the pre-protocol
state preparation phase, then the runner reconstructed a
non-authoritative `target/debug/i2pr-interop` path instead of
using the canonical absolute artifact path supplied by the
wrapper. No TCP or NTCP2 wire-level conclusion is supported by
that result.

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
plan_095 = active-runner-provenance-corrected-awaiting-authoritative-rerun
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_098 = passed-runner-provenance-boundary-correction
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
```

Plan 095 remains the single next executable plan. Plan 098
corrected the runner/provenance ownership boundary that the
first authoritative dispatch exposed before any TCP or NTCP2
wire activity. Exactly one manual Plan 095 GitHub Actions
dispatch follows the Plan 098 correction commit. Plan 088 may
not run before Plan 095 closes with a passing instrumented
forward record and a passing control forward record bound to the
same CI evidence pair. Plan 079 and Plan 072 remain blocked
pending the Plan 088 decision.

## Plan 094 status reference

Plan 094 is implementation-landed with live closure
environment-blocked on this host. The Plan 094 active
sequence amendment is recorded in
`plans/094-plan093-completion-and-plan087-to-plan088-handoff.md`.
Plan 094 remained the active runner/provenance authority
between the Plan 093 implementation landing and the Plan 095
CI live-wire lane activation; the Plan 098 runner/provenance
boundary correction supersedes the Plan 094 local-host lane as
the authoritative forward-direction close path. Plan 094
remains implementation-landed; its local live closure
environment is blocked on this host.

The lowercase `plan094` token is recorded in the active status
authority block at the top of this document and in the
historical record below; the Plan 094 active sequence
amendment remains the canonical reference for any future
Plan 094 re-activation.

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

| Plan | Precondition                                              | Status after Plan 096 |
| ---  | ---                                                       | ---                    |
| 096  | Plan 095 implementation landed; workflow defects corrected | closed (pre-dispatch audit passed) |
| 095  | Plan 096 pre-dispatch audit passed; one manual dispatch follows | next executable |
| 088  | Plan 095 must record `passed` instrumented and control forward records | remains blocked |
| 079  | Plan 088 must record `two-way-development-probe-passed`     | remains blocked         |
| 072  | Plan 088 must record `ambiguous-reference-divergence` with one exact diagnostic question | remains inactive |

The retained Plan 090 and Plan 091 instrumented forward records are
preserved at `/tmp/opencode/plan091-evidence/forward/forward-record.json`
and earlier run roots under `/tmp/opencode/`. The records are diagnostic
history only; they are not passing forward records. Plan 092 owns the
ownership probe and the forward closure.

Plan 096 closed the four demonstrated Plan 095 workflow execution
defects: explicit i2pr build path, disjoint sanitized evidence,
embedded Python import audit, and canonical tracked-source digest.
The Plan 096 pre-dispatch audit (`scripts/check-plan095-workflow.sh`)
and test matrix (`tests/integration/ntcp2/harness/test_plan096.py`)
are green locally; exactly one manual Plan 095 GitHub Actions
dispatch follows.

Plan 097 closed the two narrow workflow defects that remained
after Plan 096: the producer/consumer artifact path identity
mismatch (`build-i2pr-interop` wrote to a CWD-relative
`output/i2pr-interop` while the manifest and verifier consumed
from `${BUILD_DIR}/output/i2pr-interop`), and the disposable
run-root cleanup that only deleted descendants with a suppressed
absence assertion. Plan 097 introduced one canonical absolute
`BUILD_OUTPUT` path used by every producer, verifier, manifest
generator, artifact uploader, and live consumer; added an exact
path guard before `rm -rf -- "$PLAN095_RUN_ROOT"`; and required
an unsuppressed post-cleanup absence assertion. The Plan 097
regression matrix
(`tests/integration/ntcp2/harness/test_plan097.py`) and the
extended pre-dispatch audit are green locally; exactly one
manual Plan 095 GitHub Actions dispatch follows the Plan 097
correction commit.

## Plan 099 closure amendment (2026-08-11)

Plan 099 is the active corrective and exit plan from the
multi-job CI/provenance expansion. The CI workflow is reduced
from 988 lines (five jobs with cross-job binary artifact
transfer) to 398 lines (one `development-interop` job that
builds and executes in the same fresh workspace). The Plan 098
D1–D4 evidence-integrity corrections are landed in the new
single-job workflow (chained inequality is gone, manifest parity
is enforced, and the wrapper requires `--i2pr-binary` for every
attempted-live path). The Plan 098 chained-inequality bug is
fixed at the gate level. All Plan 052–098 plan-number-specific
Python test and runner files are deleted; unique functional
assertions are migrated into the bounded functional test set
(`test_execution_lane.py`, `test_i2pd_direct_driver.py`,
`test_i2pd_direct_control.py`, `test_minimal_i2pd_probe.py`).
The `scripts/check-plan095-workflow.sh` and
`scripts/check-ntcp2-loopback-smoke-boundary.sh` scripts are
removed. The `scripts/check-ntcp2-interoperability.sh` static
boundary check is trimmed from 1870 to 124 lines and enforces
only durable invariants.

Plan 099 status:

```text
plan_099 = passed-pruning-and-exit
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_098 = passed-runner-provenance-boundary-correction (historical)
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = deferred-to-pre-activation-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = next
```

Plan 095 remains the single next executable plan; exactly one
manual Plan 095 GitHub Actions dispatch follows the Plan 099
correction commit. Plan 088 may not run before Plan 095 closes
with a passing instrumented forward record and a passing control
forward record bound to the same CI evidence pair. Plan 079 and
Plan 072 remain blocked pending the Plan 088 decision.