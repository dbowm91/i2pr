# Plan 410 status: stopped — stock i2pd inbound frame observation unavailable

Closure token: `stopped-current-pin-stock-i2pd-inbound-frame-observation-unavailable`.

Plan: `plans/implementation/ntcp2-transport/410-current-pin-ntcp2-loopback-runner.md`.

## Scope and disposition

Plan 410 added an explicit current-network-ID loopback profile to the
non-production `i2pr-interop` launcher, threaded the selected network ID into
RouterInfo and both NTCP2 handshake state machines, and built a separate
direct helper against pristine pinned i2pd 2.61.0 libraries.
The helper build does not define an observer macro or patch the reference
source. The existing network-ID-99 scenarios remain the default and retain
their prior behavior.

The plan stops before any wire attempt. The acceptance gate requires
authoritative proof that i2pd decoded the initiating DeliveryStatus. The
current stock API provides an established-session result and a send future,
but no public callback for an inbound decoded NTCP2 I2NP frame:
`NTCP2Session::ProcessNextFrame` is private, and
`I2NPMessagesHandler` exposes no observation-registration surface. A reply
sent immediately after session establishment would not prove the preceding
DeliveryStatus was received. Patching a reference router is prohibited by
`AGENTS.md`, so a live attempt would not produce admissible evidence.

This is a harness observability stop, not a protocol failure or success. No
NTCP2 wire attempt was made, no identity-bearing state or raw logs were
retained, and no production daemon behavior or support claim changed.

## Pin and build evidence

- i2pd version and revision: `2.61.0`,
  `635b013a612ff47278ef02acf8580a28e10e26c5`.
- Canonical tracked-source tree digest:
  `dbffcb2960766cf07cc87a5a56377472122ae577832f2ff5dfaa51611eb82f98`.
- Direct helper source SHA-256:
  `a49cc76b505bdaedd376d60b3064fb06e0fd2c390988b48e426b76c0a47a0044`.
- Driver CMake SHA-256:
  `0c42c55316835d520976447bc516c268c387a9f966f3619ff17d5629e22c9aa5`.
- Build script SHA-256:
  `1964a1765dc1899a01b4741fbaac020af7199de837ef005074a26a3f07847b95`.
- Built helper SHA-256:
  `f9e3fb46d16be37bce37dcc7faf6505e9c929dd558bc90c3fce7005d71ddae7d`.
- The CMake build linked the three pristine static i2pd archives. It completed
  successfully; emitted warnings are unused-parameter warnings in upstream
  headers. The helper's `--help` path exits without initializing i2pd.

## Requirement matrix

| Requirement | Result |
| --- | --- |
| Explicit topology permits network ID 2 only on IPv4 loopback | Implemented and unit-tested; negative cases reject ID 2 under other profiles and non-loopback addresses under this profile |
| Preserve synthetic network ID 99 | Existing scenario tests pass; the prepared local-state test asserts `netId=99` |
| Sign network ID 2 in RouterInfo and use it in initiator/responder handshake state | Implemented; current-profile state test verifies signed `netId=2` and the exact loopback endpoint |
| Build stock i2pd helper without a reference-source patch | Built successfully from the exact current pin with observer instrumentation disabled |
| Bounded bidirectional wire attempts and I2NP receive evidence | Not run; stock i2pd has no public decoded-inbound-frame observation surface |
| Sanitized runner evidence and runner `--self-test` | Not produced; no live runner was implemented because it could not meet the authoritative receive-evidence requirement |

## Commands and outcomes

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-interop --all-targets` — passed, 32 tests.
- `rtk bash tools/i2pr-interop/reference/i2pd-current/build.sh --i2pd-source-dir target/interop/ssu2-sources/i2pd-635b013a612ff47278ef02acf8580a28e10e26c5 --output-dir target/interop/plan410-i2pd` — passed; built against the exact pinned source without modifying tracked reference files.
- `rtk target/interop/plan410-i2pd/i2pd-current-ntcp2-driver --help` — passed; no reference runtime started.
- `rtk python3 scripts/check-current-pin-ntcp2-runner.py` and `--self-test` — passed, including mutations for network ID, topology, observer instrumentation, and identity-hash evidence.
- `rtk bash scripts/check-ntcp2-vectors.sh` — passed.
- `rtk bash scripts/check-ntcp2-interoperability.sh` — passed as a historical-boundary check only; not wire evidence.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `rtk python3 scripts/check-tooling-inventory.py` — passed after documenting the new checker.
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, 51 tests.
- `rtk git diff --check` — passed.
- Full workspace floor — not run; no production crate changed, and Plan 410 stopped before wire execution.

## Readiness and follow-up

Plan 434 remains blocked. Plan 410 resolves the network-ID-99-only launcher
limitation but does not supply authoritative two-way I2NP evidence. No other
Plan 430–439 dependency is unblocked: Plan 431 remains stopped on the
unavailable independent non-loopback qualification topology; Plans 433 and
435–439 remain gated by their recorded predecessors. Normal-daemon NTCP2 stays
disabled and non-advertised under Plan 101.
