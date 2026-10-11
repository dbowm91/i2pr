# Plan 444 resume status: blocked — Java selection and store/lookup controls remain open

Closure token: `blocked-java-selection-and-store-lookup-controls-not-run`

Plan: [`444-configured-shared-bandwidth-class-and-java-selection-control.md`](../../implementation/floodfill/444-configured-shared-bandwidth-class-and-java-selection-control.md)

This is the later status for the Plan 454 resume. The original
[`444-status.md`](444-status.md) remains unchanged and records the earlier stop
before an enforced bandwidth budget existed.

## Completed resume scope

- Plan 454 commit `4169a11` supplies a daemon-composed configured byte-rate
  governor, wire-byte charging for SSU2 and the dormant Plan-101-safe NTCP2
  adapter, and an opaque live capacity permit. Plan 454's closure is
  [`454-status.md`](454-status.md).
- The exact Java I2P 2.13.0 source pin remains
  `9134f808337b401e8e53c73734c81fab04280c9d` in both the source checkout and
  the staged cache `source-revision.txt`.
  `Router.getBandwidthClass()` derives its class from the minimum
  configured inbound/outbound KB/s and share percentage. Its thresholds are
  `<12 K`, `<=48 L`, `<=64 M`, `<=128 N`, `<=256 O`, `<=2000 P`, then `X`.
- The Java source has two distinct policies. `FloodfillMonitorJob` admits the
  local router's floodfill role only at N/O/P/X. `FloodfillPeerSelector`
  ranks a remote L-class candidate into its last-resort group; it can still
  return that candidate when the requested result has room. Therefore an L
  peer is not universally rejected by Java's peer selector.
- The daemon's private capacity-bearing floodfill builders now reject a class
  below N before constructing `f`/`fR`. The guard applies to both reachable
  and non-reachable forms and consumes the current permit, role permit, and
  existing address/reachability gates. This changes no normal startup path.
- The Plan 444 implementation requirements now describe the negative control
  as a low-class candidate ranked behind a healthy candidate, rather than
  asserting that Java must categorically reject every L-class peer.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Enforced configured rate and capacity permit | Passed in Plan 454 (`4169a11`); the full enforcement evidence is retained in `454-status.md`. |
| Exact pinned Java class units and thresholds | Source-audited in the verified 2.13.0 checkout; thresholds and local-role admission are summarized above. |
| Floodfill class floor | Passed locally: class N and above are accepted by the private owner builder; truthful L is refused before a class-bearing `f` record is built. |
| Healthy stock Java bootstrap control | Not run. |
| Low-class candidate preference control against a healthy stock candidate | Not run. Pinned source shows L is last-resort, not a strict exclusion. |
| Correctly classed i2pr candidate selected by stock Java | Not run. |
| Real stock Java store/lookup after selection | Not run. |
| Public floodfill/capability behavior | Unchanged; no normal RouterInfo class, `f`, or `fR` advertisement was enabled. |

## Commands and outcomes

Local Linux evidence for this resume:

```text
rtk cargo fmt --all --check
  passed
rtk cargo test --locked -p i2pr-netdb --lib local::tests -- --test-threads=1
  passed (15 tests)
rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd
  passed (sibling binaries already current)
rtk cargo test --locked -p i2pr-daemon --lib floodfill::tests::capacity_routerinfo_rejects_truthful_classes_below_java_floodfill_floor -- --exact --test-threads=1
  passed (1 test)
```

The complete local AGENTS floor passed after the source and planning changes:

```text
rtk cargo fmt --all --check
  passed
rtk cargo check --locked --workspace --all-targets
  passed
rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
  passed
rtk cargo test --locked --workspace --all-targets -- --test-threads=1
  passed (4,797 passed, 39 ignored; 186 suites; 1,053.50 seconds)
rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
  passed
rtk env RUSTDOCFLAGS=-Dwarnings cargo doc --locked --workspace --no-deps
  passed
rtk cargo test --locked --workspace --doc
  passed (29 suites)
```

All 58 remaining AGENTS floor guards passed, including dependency/runtime/
console boundaries, managed-app self-tests, evidence checkers, both M12
floodfill self-tests, the NTCP2 harness tests (18 passed), and `cargo deny`.
The planning suite passed (51 tests), global plan-number uniqueness passed,
and `rtk git diff --check` passed. The new floodfill floor negative test is
included in the workspace test count above.

No Java process was started and no Java store/lookup attempt was made. The
existing `run-java-floodfill.sh` is Plan 279's frozen six-router qualification
lane; it prepares a `caps=f` record and does not implement Plan 444's distinct
candidate-preference controls. Reusing its attempt budget or treating its
matrix as selection evidence would not establish the required controls.

## Migration, security, operations, and findings

No configuration default or wire protocol changed. The shared governor stays
disabled by default, normal-daemon SSU2 stays loopback-only and non-advertised,
normal-daemon NTCP2 stays disabled, and no support or conformance claim changed.
The new floor is in the private daemon owner; no reference source, key, peer
identity, or RouterInfo was patched or retained as evidence. There are no
dependency changes.

Findings: critical — none; high — none; medium — the required pinned Java
candidate-preference and real store/lookup controls have not been implemented
or run; low — the earlier Plan 444 stop remains in its historical closure.

## Unblock audit and disposition

Plan 444 is **blocked**, not passed. Its remaining owner is a Plan-specific,
source-pinned Java fixture with an independent healthy floodfill/bootstrap,
an L-class candidate ranked behind that healthy peer, and an N-or-higher
classed i2pr candidate that stock Java genuinely selects before a real
store/lookup. Evidence must correlate the selected candidate with the
operation; the existing read-only selector probe alone is not enough. Keep
the pinned Java distribution pristine and record only bounded sanitized
evidence.

Plan 437 remains blocked on Plans 433 and 436; this resume does not unlock
normal floodfill role eligibility. Plan 438 remains blocked on Plan 437.
Plans 451 and 452 retain their independent blocked closures. No other registered
work became dependency-ready from this resume. No historical closure, support
inventory, or public capability was promoted.
