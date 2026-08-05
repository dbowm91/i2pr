# Plan 087 status: first real i2pr-to-i2pd host-loopback probe

## Status

The Plan 086 runner architecture was extended to drive a real
`i2pr -> i2pd` forward wire attempt under the
`host-loopback-development` lane. The Plan 090 corrective pass
applied four behavior-neutral corrections in the i2pd direct
driver (see `tests/integration/ntcp2/reference-drivers/source-verification.md`
"Plan 090 verified RouterInfo lifecycle"):

1. `set_bool_option("ntcp2.published", true)` — store the
   option as `bool` (was stored as `int` by the Plan 064
   helper).
2. `i2p::config::ParseCmdline(1, fake_argv, ignoreUnknown=true)`
   + `Finalize()` — populate the i2pd option store with declared
   defaults before the driver mutates individual options.
3. `set_uint16_option` helper — store `port` and `ntcp2.port`
   as `uint16_t` (was stored as `int`).
4. `i2p::transport::transports.SetCheckReserved(false)` —
   disable reserved-range filtering so loopback addresses
   survive `RouterInfo::ReadFromBuffer` deserialization.

The driver also fails closed with
`router-info-endpoint-mismatch` if the authoritative in-memory
RouterInfo does not carry the exact configured NTCP2 endpoint.

After the Plan 090 corrections, the Plan 087 instrumented
forward attempt (`plan090-real-20260805174541-fresh`)
authenticated the i2pd listener (`listener_ready`) and reached
TCP, then recorded a NTCP2 protocol failure. The i2pd direct
driver emitted `process_started`, `router_info_exported`, and
`listener_ready`; the i2pr dialer emitted a terminal status with
`result = authentication_failed` and
`reason_code = handshake_failed` after
`drive_initiator_handshake` returned
`Io(ExactIoError { kind: Closed })`. The Plan 083 pre-TCP
classifier mapped this to `terminal_result =
pre_protocol_rejected` with
`reason_code = pre-protocol-reference-failed` because no
`tcp_connected` event was observed.

The forward direction did not pass: the i2pd listener bound
on the configured port and emitted `listener_ready`, but the
NTCP2 Noise handshake closed the TCP socket before the i2pr
initiator reached `ntcp2_authenticated`. The first-instrumented
Plan 090 attempt is preserved at
`/tmp/opencode/plan090-real-20260805174541-fresh/forward-record.json`
and the i2pr status log at
`/tmp/opencode/plan090-real-20260805174541-fresh/raw/i2pr-status.jsonl`.
The retained record is the first authentic Plan 090 wire
attempt; it is not yet a passing plan closure.

```text
status = open-plan-090-forward-protocol-failure
forward_digest = non-zero (Plan 090 first clean committed-head attempt)
corrective_source_commit = f3ba6e1c4b465d69f3570d3b9323b97a36e09ba9
plan_090 = corrective-corrections-landed-forward-direction-not-passed
plan_088 = blocked_pending_plan_087_pass
plan_079 = blocked_pending_plan_088_two_way_pass
plan_072 = inactive_pending_plan_088_ambiguity
```

The Plan 090 corrections close the i2pd-side zero-address
RouterInfo defect. The remaining NTCP2 protocol failure is a
post-TCP handshake defect whose ownership is not yet
established. The next active step is a follow-up corrective pass
that runs the forward direction against a Plan 090-corrected
driver until a passing `i2np_message_decoded` event is
observed on both sides. The Plan 088 reverse direction
remains blocked pending a Plan 087 pass; Plan 079 and Plan 072
remain blocked pending the Plan 088 decision.

## Delivered implementation surface

The Plan 087 commit:

- `tests/integration/ntcp2/harness/plan083_runner.py` —
  fixes four pre-existing Plan 086 defects that were masking
  the real Plan 087 protocol attempt:
  - `execute_real_probe` now accepts
    `source_inspection_record_sha256` (default `None`,
    falls back to `reference_tree_sha256`) and threads it into
    both the inspect and listener i2pd driver invocations so
    the Plan 062 v4 trigger record no longer fails
    `validate_trigger_record` with
    `trigger-zero-provenance-digest-with-attempted`;
  - `execute_real_probe` passes
    `capture_stdout=True` to the i2pr prepare placement so the
    runner can parse the preparation JSON line from
    `placement.run.stdout` instead of re-reading the log file
    (the Plan 086 default streams stdout only to the log);
  - `execute_real_probe` creates the run-root subdirectories
    (`raw/`, `state/`, `i2pd-data/`, `i2pd-output/`,
    `exchange/`) with `mode=0o700` so the Plan 001 storage
    `IdentityStore::prepare_directory` accepts the path
    (the prior `mkdir(parents=True, exist_ok=True)` left
    `0o755` directories which the storage layer rejected with
    `prepare_state_path_invalid` before any i2pr subprocess
    ran);
  - `execute_real_probe` rewrites Phase 4–6 to use the
    `HostLoopbackDevelopmentPlacement` for the i2pd listener
    and the i2pr dialer via `popen()` instead of the prior
    blocking `i2pd_direct_driver_invocation(..., mode="listen")`
    call (the blocking call held the listener process group
    alive until `handshake_timeout_ms`, preventing the i2pr
    dialer from ever reaching the wire);
  - after inspect, copies the i2pd-exported signed
    `router.info` (`output_dir/router.info`) to the
    scenario-exact `exchange/i2pd-router.info` path and
    verifies the destination SHA-256 matches the source before
    the `validate-scenario` step renders the strict TOML.
- `tests/integration/ntcp2/harness/plan084_runner.py` — same
  `source_inspection_record_sha256` addition and the same
  `mode=0o700` directory creation; the Plan 088 reverse runner
  keeps the wrapper contract for the future reverse pass.
- `scripts/interop/run-minimal-i2pd-host-loopback-probe.py` —
  threads the missing `reference_tree_sha256` and
  `source_inspection_record_sha256` arguments through
  `_run_forward_probe` and `_run_reverse_probe`, so the
  runner receives non-zero provenance digests instead of the
  Plan 086 `0x00 * 64` placeholder; this keeps the Plan 062 v4
  trigger record contract satisfied when the wrapper invokes
  the canonical Plan 083 runner.

The Plan 087 implementation surface is otherwise unchanged
from Plan 086/083: the wrapper is still a thin parser and
dispatcher, the placement is still the only subprocess owner,
the runner still uses the Plan 082 `prepare` boundary, the
Plan 065 strict scenario validation, and the Plan 064 strict
driver config; the runner still emits one
`i2pr-minimal-i2pd-probe-v1` record per direction; and the
record still carries the bounded topology metadata
(`development_only`, `release_qualified = false`,
`isolation_qualified = false`).

## Authenticated instrumented attempt

The Plan 087 instrumented attempt was executed against the
concurrent-preflight commit `32ca767fe1843d6d4abcec7cb6bad26eda3ac0c2`
with the canonical Plan 083 + Plan 064 implementation surface
intact. The committed source SHA used for the digest was
`e34b87c2a45fbb318ea1245be473932cfb7e05d7`, the
`host-loopback-development` topology, network ID `99`, a fresh
run root, and the host loopback endpoint
`127.0.0.1:<random_port>` for both peers. The exact live
command was:

```text
python3 scripts/interop/run-minimal-i2pd-host-loopback-probe.py \
    --direction i2pr-to-i2pd-ipv4 \
    --repo-root "$(pwd)" \
    --run-root /tmp/opencode/p087-real-r9 \
    --run-id plan087-real7-20260805131*** \
    --source-commit e34b87c2a45fbb318ea1245be473932cfb7e05d7 \
    --output /tmp/opencode/p087-real-r9/forward-record.json \
    --i2pd-driver-binary /home/sugarwookie/projects/i2pr/target/interop/i2pd-driver/build/i2pd_ntcp2_interop_driver_instrumented \
    --handshake-timeout-ms 30000
```

The wrapper exited `6` (probe failed to
`passed`). The sanitized
`forward-record.json` carries:

```text
direction                         = i2pr-to-i2pd-ipv4
topology_kind                      = host-loopback-development
highest_stage_reached              = listener_ready
terminal_result                    = protocol_rejected
reason_code                        = reference-events-missing
i2pr_binary_sha256                 = 6eb9133dc128f2f1d9528e0d41ef7c165228dcde96d47aaea92faffcf1714d85
i2pd_binary_sha256                 = d92890746e022101fe6c8e6e0f54929a6c583937ff3148168cab72ebcaf4d7db
i2pr_router_info_sha256             = c9fdc243adcc30035e0a71278999e2ba9bf6567832ce7f38f11f8aa575c8f33e
i2pd_router_info_sha256             = 8fc3b0cdbd2c14bbce40275095a13d1cb15ba237120e5b9ca2baeb56a6a6c3f3
i2pr_router_hash_sha256            = f7588a8c769581626f81bd8be56090e18ad1bdec2084ccd9b6c5a1579e536555
i2pd_router_hash_sha256            = f7d50e69d8603e0bab85376f18a31a2ff6d23865d6a27c1a383aec9883b8aea7
placement_record_sha256            = b14d7f2b15f4beed2eb276a3c13db8ec6e6b52eac93aaf759d13b7c636b30744
lane_qualification_sha256         = <derived from run_id + direction + source_commit>
delivery_status_message_id          = 69206017 (0x04200001)
process_counters                   = i2pd_listener=1/1/0, i2pd_prepare=1/1/0,
                                    i2pr_dialer=1/1/0,  i2pr_prepare=1/1/0
parent_network_state_unchanged      = true
cleanup_result                     = clean
record_sha256                     = <derived by build_record>
```

The `observed_events` list contains exactly two entries:

```text
listener_ready      (source_side=i2pd,  event_sha256 matches the i2pd reference-event v1 file)
terminal_rejected   (source_side=i2pr,  event_sha256=0x00..00, i2pr reason_code=peer_router_info_invalid)
```

The i2pd direct driver's `events.ndjson` carries the
canonical `process_started`, `router_info_exported`,
`listener_ready`, and `terminal_rejected` (with
`detail = listening-handshake-timeout`) markers from the
inspect-mode and concurrent listener-mode invocations.

The `i2pr.log` carries the final i2pr
`{"phase":"terminal","result":"rejected","reason_code":"peer_router_info_invalid", ...}`
status line, with every i2pr counter reported as
`listener_ready=0, authenticated=0, frames_sent=0,
frames_received=0, i2np_sent=0, i2np_received=0,
delivery_status_message_id=0,
expected_peer_router_hash_sha256=""`. The empty
`expected_peer_router_hash_sha256` confirms the rejection
occurred at the strict-scenario validation step (the i2pr
`prepare_peer_state` returned `LauncherError::PeerRouterInfoInvalid`
from `exact_ntcp2_address`, which iterates
`info.addresses()` looking for an entry whose endpoint equals
`SocketAddr::new(expected_ip, expected_port)`).

## Ownership and precise remaining question

The pre-TCP rejection is owned by the i2pd direct driver's
`initialise_i2pd_runtime` (Plan 064 surfaces). The
driver-produced `router.info` files from both the
inspect-mode and the listen-mode invocations decode to:

```text
addresses: 0
options: Mapping([caps=L, netId=2, router.version=0.9.69])
network_id = 99
```

A standalone i2pd 2.60.0 process started against the same
`127.0.0.1:<port>` configuration produces a `router.info`
with at least one NTCP2 address, confirming that the i2pd
library itself is capable of producing addresses and the
defect sits in the bounded driver config path. The i2pd
direct driver's `initialise_i2pd_runtime` calls
`i2p::context.Init()` (which calls
`RouterContext::CreateNewRouter()` →
`NewRouterInfo()`); on the same data directory
`NewRouterInfo()` does call
`routerInfo.AddNTCP2Address(m_NTCP2Keys->staticPublicKey,
m_NTCP2Keys->iv, ntcp2Port, addressCaps)` for the
non-published branch (since `cfg.network_id == 99` and
`cfg.local_address == "127.0.0.1"` and `ntcp2.enabled ==
true`), but the address does not survive
`RouterContext::UpdateRouterInfo()` →
`m_RouterInfo.CreateBuffer(m_Keys)` on subsequent runs that
load an existing `router.info` from the data directory, and
on a fresh run the buffer written by
`write_local_router_info(cfg)` reports zero addresses from
the i2pr decoder. The buffer is verified by the i2pd
signature and the buffer's published date is fresh, but the
address section is empty.

The precise question for a Plan 064/076 corrective pass
becomes: **does the i2pd direct driver's bounded config and
in-process `i2p::context.Init()` path produce a `RouterInfo`
that survives `CreateBuffer` → `CopyBuffer` →
`write_local_router_info` round-trips with the configured
NTCP2 address, and if so, which step strips or fails to
add the address**? The investigation requires reading the
pinned i2pd 2.60.0 source for the `i2p::context.Init()`
vs. `RouterContext::UpdateRouterInfo()` interaction and
producing a deterministic unit test that decodes the i2pd
driver's emitted `router.info` and asserts
`info.addresses().len() > 0`. Once that test exists and
pins the fix, the Plan 087 instrumented attempt above is
expected to advance past
`peer_router_info_invalid` and produce a one-direction
`passed` (or a real first-stage protocol failure) record;
Plan 088 may then begin a fresh reverse-direction attempt.

Plan 087 stays open on this precondition per the bounded
acceptance criteria; the Plan 087 implementation surface
travels with the repository so any future host that can
run a fixed Plan 064/076 driver can resume the forward
attempt without further runner changes.

## Corrected Plan 086 runner defects closed by this commit

| Prior defect                                              | Corrective surface                                                                                                         |
|-----------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------|
| `prepare_state_path_invalid` from `mkdir(...)` (`0o755`)   | runner creates `raw/`, `state/`, `i2pd-data/`, `i2pd-output/`, `exchange/` with `mode=0o700`                              |
| `capture_stdout=False` lost the i2pr prepare JSON          | runner passes `capture_stdout=True` to placement `run` and parses `placement.run.stdout` directly                            |
| `source_inspection_record_sha256` defaulted to `0*64`      | runner accepts `source_inspection_record_sha256` (defaulting to `reference_tree_sha256`) and threads it into both i2pd calls  |
| wrapper passed only 4 of the 6 required runner digests    | wrapper now passes `reference_tree_sha256` and `source_inspection_record_sha256` to `execute_real_probe`/`execute_reverse_probe` |
| listener was launched through blocking `subprocess.run`    | runner rewrites Phase 4–6 to `HostLoopbackDevelopmentPlacement.popen()` for i2pd listener and i2pr dialer concurrently  |
| post-inspect RouterInfo not copied to scenario exchange   | runner copies `output_dir/router.info` to `exchange/i2pd-router.info` and verifies the SHA-256 before scenario render     |

## Validation

The Plan 087 implementation surface was exercised by the
focused test suite plus the exact live command above. The
focused checks all pass before the live command:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan086.py'       39 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py' 14 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'       50 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'       35 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py' 43 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py' 57 passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'       29 passed
bash scripts/check-dependency-direction.sh                 ok
bash scripts/check-runtime-boundaries.sh                   ok
bash scripts/check-fixture-manifest.sh                     ok
bash scripts/check-ntcp2-interoperability.sh                ok
bash scripts/check-rootless-interop-boundary.sh            ok
bash scripts/check-multipass-interop-boundary.sh           ok
bash scripts/check-ntcp2-loopback-smoke-boundary.sh        ok
cargo fmt --all --check                                   ok
cargo check --workspace --all-targets                      ok
cargo test --workspace                                    241 passed, 0 failed
cargo clippy --workspace --all-targets --all-features -- -D warnings  ok
```

The live command exited `6`; the sanitized
`forward-record.json` reaches `listener_ready`, observed
events `listener_ready` and `terminal_rejected`, and the
i2pr dialer rejects with `peer_router_info_invalid` because
the i2pd direct driver's emitted `router.info` carries zero
`RouterAddress` entries.

## Future plan unblocking

| Plan | Precondition                                              | Status after Plan 091  |
| ---  | ---                                                       | ---                    |
| 088  | Plan 087 must record `passed`                              | remains blocked         |
| 079  | Plan 088 must record `two-way-development-probe-passed`     | remains blocked         |
| 072  | Plan 088 must record `ambiguous-reference-divergence`      | remains inactive        |
| Plan 091 ownership pass | i2pr vs i2pd ownership of the wire-level termination must be classified | required next |

Plan 091 landed the i2pd direct driver corrective preconditions
(``SetNetID``, explicit logger start/stop, ``tcp_accepted``
wait, symmetric ``DeliveryStatus`` send). It did not close
Plan 087 as `passed`; the forward direction still terminates
with the i2pd NTCP2 transport reading `End of file` on the
SessionRequest body while the i2pr reports a data-phase
``receiver_delivery_status_missing`` terminal. The diagnostic
surface is in place (per-direction counters, intermediate
phase emissions, file-scoped i2pd log) and the bounded
forward record is retained at
``/tmp/opencode/plan091-evidence/forward/forward-record.json``.
Plan 088 may not run before a follow-up ownership pass
isolates the wire-level termination to one side.
