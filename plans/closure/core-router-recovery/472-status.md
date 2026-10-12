# Plan 472 status: blocked — live executor and authorized host pair unavailable

Closure token: `blocked-live-executor-and-authorized-host-pair-unavailable`

Plan: [`472-authorized-two-host-ssu2-live-qualification-runner.md`](../../implementation/core-router-recovery/472-authorized-two-host-ssu2-live-qualification-runner.md)

## Outcome

Plan 472 now has a portable offline runner at
[`plan472_live.py`](../../../tests/integration/ssu2/plan472_live.py) and an
operator guide. `prepare` reuses Plan 449's strict owner-consent, stock-pin,
private-interface, non-emitting route, and data-directory checks. `verify`
uses the existing signed RouterInfo inspector to validate both artifacts,
their declared SSU2 endpoints and network ID 2, fresh publication dates,
bounded clock skew, distinct router identity hashes, and the pinned stock
router version. The inspector and runner emit only fixed result codes and
bounded metadata; endpoint values, identities, artifact paths and key material
are not emitted.

The live product executor is not implemented. `run` requires the separate
`--live` flag and then fails closed with `live-executor-unavailable`; no
router is started and no packet is sent. The repository has no approved
independent host pair or attributable executor that can prove both initiated
roles, authenticated I2NP/DeliveryStatus, durable-identity restart, and
withdrawal. This closure does not represent the preflight or RouterInfo checks
as live qualification.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Strict per-host consent, pinned stock source, local endpoint and route preflight | Implemented by reusing Plan 449; its deterministic self-test passed. No owner inventory was supplied, so no host-specific preflight ran. |
| Signed RouterInfo signature, network ID, endpoint family/port, static and intro keys | Implemented via the existing cryptographic inspector and endpoint comparison. |
| Freshness, bounded pair clock skew and distinct router identities | Implemented; deterministic positive and negative summary/report tests passed. |
| Per-host `prepare`, pair `verify`, and fail-closed `run --live` gate | Implemented. The run command intentionally returns `live-executor-unavailable`; it is not a traffic runner. |
| Bidirectional SSU2 and I2NP/DeliveryStatus against stock i2pd | Blocked: no reviewed executor or owner-approved host pair. No packets sent. |
| Restart, withdrawal, and full negative-control matrix | Not run; requires the live executor and approved pair. |
| Public reachability, NAT, introducer, IPv6, Java-family, or anonymity claim | None. |

## Verification

| Command | Outcome |
| --- | --- |
| `python3 -m py_compile tests/integration/ssu2/plan472_live.py` | Passed. |
| `python3 tests/integration/ssu2/plan472_live.py --self-test` | Passed, 2 positive and 8 negative cases. |
| `python3 scripts/plan449-ssu2-preflight.py --self-test` | Passed, 8 positive/negative cases. |
| `cargo build --locked -p i2pr-interop` | Passed. |
| `cargo test --locked -p i2pr-transport-ssu2 --all-targets` | Passed, 190 tests. |
| `cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1` | Passed, 10 tests. |
| `cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1` | Passed, 6 passed, 1 ignored. |
| `bash scripts/check-ssu2-vectors.sh` | Passed; manifest complete and hashes match. |
| `bash scripts/check-ssu2-acceptance-evidence.sh` | Passed; 15 command-derived rows, no literal pass records. |
| `python3 scripts/check-workflow-validity.py` | Passed; 11 workflow files parse and are structurally valid. |
| `python3 scripts/check-global-plan-number-uniqueness.py` | Passed. |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed, 51 tests. |
| `bash scripts/check-runtime-boundaries.sh` | Passed. |
| `bash scripts/check-dependency-direction.sh` | Passed. |
| Full `AGENTS.md` routine floor | Passed. Workspace tests: 4,851 passed, 39 ignored across 187 suites; Clippy, docs, doc tests, all listed evidence/boundary guards, planning tests, and `cargo deny check advisories bans sources` passed. |

## Security and compatibility

- `prepare` and `verify` never start a process, open a socket, or send a packet.
- `run` is gated by owner-supplied consent and an explicit flag, and remains
  fail-closed until a reviewed executor exists.
- No dependency, router default, advertised capability, support inventory,
  conformance claim, or protocol behavior changed.
- No raw RouterInfo, identity hash, address, source path, or key is stored in
  closure evidence.

Disposition: **blocked before live execution**. Resume only with a reviewed
pair-specific executor and a genuinely owner-authorized private host pair.
