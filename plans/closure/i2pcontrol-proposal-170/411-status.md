# Plan 411 — Java SAM session-create diagnostic: status

Status: **passed-java-sam-default-datagram-port-collision-attributed-one-request**.

Plan of record:
[`411-java-sam-session-create-diagnostic.md`](../../implementation/i2pcontrol-proposal-170/411-java-sam-session-create-diagnostic.md).

Classification: infrastructure/diagnostic. This closes the bounded diagnosis
only. It does not pass a Java ELS2 direction or unblock Plan 375's full matrix.

## Result

One fresh stock Java I2P 2.13.0 run at
`9134f808337b401e8e53c73734c81fab04280c9d` reached one DATAGRAM SAM
`SESSION CREATE`. It returned `I2P_ERROR` after 8 ms. An ephemeral preflight
probe found that UDP `127.0.0.1:7655` was already occupied. Exact-pinned source
tracing shows that the DATAGRAM branch binds its helper socket before it
constructs the I2CP session; the bind `IOException` becomes the observed SAM
error. The failure therefore occurred before I2CP session creation.

The actionable test-driver correction is to pass an available loopback
`sam.udp.host` and `sam.udp.port` in `SESSION CREATE` whenever a DATAGRAM helper
is used. No Java or production code was changed.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| exact pinned stock Java and fresh loopback topology | `result.json`, Java pin and topology fields; runner validates the cached checkout | pass |
| exactly one SAM session-create request | `result.json`: `attempts: 1`, stage `session-create`; one request was sent | pass |
| preserve only sanitized output | response SHA-256 retained; raw SAM response, destination material, and router logs stayed in ephemeral scratch | pass |
| determine whether default UDP bind was available | `udp_127_0_0_1_7655_available_before_start: false` | pass |
| attribute the observed response at the exact pin | `source-trace.json`: `SAMBridge` defaults, `SAMv3DatagramServer` bind, handler ordering, caught `IOException`, error reply | pass |
| leave Plan 279 untouched | no Plan 279 runner invocation; Plan 279 attempt budget remains unchanged | pass |

Evidence directory: `tests/integration/els2/evidence/plan411/`.

| Artifact | SHA-256 |
|---|---|
| `manifest.json` | `4c922578732f4f797cbca80e0c2d20de279672135fccb1094c73a29acaae6975` |
| `result.json` | `328431861c350e32720f2d2d4662bed6697233ac0e4c9f539276672a61e1e8ce` |
| `source-trace.json` | `5e037e55fd5c3d8ca1cfa8e6d6aa3a592cf1539d0ff2dc955cf86192f92a6766` |

## Execution and verification

The independent runner was invoked once. The live request and sanitized result
were produced, then the original invocation exited in its source-trace
postprocessor. The tracer was corrected and run offline against the exact
pinned source; the request was not repeated. This limitation is preserved in
the Plan narrative and reference freeze. The final artifact is accepted by the
checker.

Local checks:

| Command | Result |
|---|---|
| `bash tests/integration/els2/run-java-sam-diagnostic.sh` | one live request; stage reached and result captured; invocation later exited in the source-trace postprocessor |
| `python3 scripts/trace-java-sam-source.py` | pass after offline postprocessor correction; exact-pin source facts emitted |
| `python3 scripts/check-java-sam-diagnostic.py tests/integration/els2/evidence/plan411` | pass |
| `python3 scripts/check-java-sam-diagnostic.py --self-test` | pass; 12 invalid mutations rejected |
| `bash -n tests/integration/els2/run-java-sam-diagnostic.sh` | pass |
| `python3 -m py_compile scripts/check-java-sam-diagnostic.py scripts/trace-java-sam-source.py` | pass |
| `python3 scripts/check-tooling-inventory.py` and `--self-test` | pass |
| `python3 scripts/check-global-plan-number-uniqueness.py` | pass |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | pass (51 tests, run before final closure edits; rerun at handoff) |
| `rtk git diff --check` | pass (rerun at handoff) |

The routine production workspace floor was not run: this plan changed only
diagnostic tooling, evidence, and planning documents. No dependency changed.

## Security, compatibility, and findings

- **Critical/high:** none identified.
- **Medium:** no Java ELS2 behavior has passed; Plan 375's independent driver
  and full matrix remain required.
- **Low:** the runner's post-request source-trace step failed on its first
  execution and was corrected offline. The one-request live budget was
  respected, but the runner was not re-executed end to end.
- Java remained unmodified at the frozen pin. No support inventory,
  advertisement, production API, or persisted format changed.
- Raw response/log data and transient destination material were not committed.

## Roadmap disposition and unblock audit

Plan 411 is closed as passed. Plan 412 is registered and active for the Java
no-auth requester direction, with an explicit available loopback UDP port for
any DATAGRAM helper. Plan 375 remains blocked until its Java driver, both
directions, authorization/negative cases, and required lifecycle rows pass.
Plans 377 and 378 remain blocked.

| Plan | Dependency state | Disposition |
|---|---|---|
| 412 | Plan 411 passed; Plan 406 delivered i2pd scope | active and eligible; executes first Java requester direction |
| 375 | Plan 411 diagnosis passed; Java driver/matrix incomplete | blocked on remaining Java ELS2 work |
| 377 | Plan 375 not passed | remains blocked on 375 |
| 378 | Plan 377 not passed | remains blocked on 377 |

No capability promotion follows from this diagnostic.
