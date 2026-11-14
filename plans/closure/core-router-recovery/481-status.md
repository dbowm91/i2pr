# Plan 481 status: passed — source-owned build write-completion infrastructure

Closure token: `passed-source-owned-build-egress-completion-infrastructure`

Plan: [`481-source-owned-build-egress-completion-bridge.md`](../../implementation/core-router-recovery/481-source-owned-build-egress-completion-bridge.md)

## Outcome

The daemon build coordinator now receives bounded, process-local completion
receipts from the transport socket writers. SSU2 receipts bind the exact
authenticated link generation and complete only after every distinct original
fragment is accepted by the UDP socket writer. NTCP2 receipts complete after
the authenticated link owner writes the full frame. Nested TunnelData fanout
aggregates all cell receipts and can report complete, partial, rejected, or
still pending disposition without conflating inner-build, transport, and
terminal reply identifiers.

This closes Plan 481's cross-owner infrastructure and real-build consumption
scope. A writer receipt proves local socket submission only. It does not prove
remote receipt, stock acceptance, terminal build processing, a correlated
reply, tunnel installation, or SAM STREAM. Plans 477 and 478 remain active for
those product and reply gates.

## Requirement-to-evidence matrix

| Requirement | Evidence / result |
| --- | --- |
| Source-owned NTCP2 completion | The selected authenticated link generation carries an optional completion sender to its writer task. The ticket resolves only after the full frame write succeeds; cancellation, route close, enqueue rejection, and writer failure settle false. |
| Source-owned SSU2 completion | The runtime retains a bounded ticket keyed by exact link generation and message ID. The socket writer settles distinct original fragment ordinals; partial send, stage rejection, expiry, link close, and capacity exhaustion fail closed. Ordinary messages without a receipt skip the completion-map lookup. |
| Nested fanout aggregation | `BuildDeliverySet` tracks one build's per-cell receipts and produces all-emitted, partial, not-emitted, pending, or unobserved disposition. Previously admitted cells may complete after a sibling rejection; no atomic-send promise is made. |
| Build-owner consumption | `ExploratoryBuildCoordinator` advances and consumes receipts in the build wait paths. Production diagnostics emit direction and disposition category without peer hashes, endpoints, or the receipt correlation map. |
| Deterministic receipt and lifecycle coverage | NTCP2 writer receipt test, SSU2 distinct-fragment and real localhost multi-fragment tests, build fanout aggregation, Plan 481 sanitizer tests, Plan 474 wait receipt tests, and Plan 464 attempt receipt tests all pass. |
| Bounded pinned stock handoff | One exact-pinned i2pd 2.61.0 Plan-458 attempt after the source-consumer correction failed at outbound build. Sanitized receipt: `outbound:all-emitted=1`, `udp_emitted=1`; stock aggregate: 86 ShortTunnelBuild records, 36 transit-create events, 4 peers. No correlated terminal reply or product effect was established. |

## Commands and outcomes

Executed locally on Linux; no hosted-CI result is claimed.

```text
rtk cargo fmt --all --check
rtk cargo check --locked --workspace --all-targets
rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
  passed
rtk cargo test --locked --workspace --all-targets -- --test-threads=1
  passed (4,872 passed, 39 ignored; 187 suites)
rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" rtk cargo doc --locked --workspace --no-deps
rtk cargo test --locked --workspace --doc
  all passed
rtk cargo test --locked -p i2pr-runtime --all-targets -- --test-threads=1
  passed (143 passed, 1 ignored)
rtk cargo test --locked -p i2pr-daemon --lib exploratory_build -- --test-threads=1
  passed (16 passed)
rtk cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1
  passed (15 passed)
rtk cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
  passed (11 passed)
rtk cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1
  passed (6 passed, 1 ignored)
rtk cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
  passed (4 passed)
rtk cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1
  passed (9 passed)
rtk python3 tests/integration/core-router-recovery/test_plan481_writer_disposition.py
rtk python3 tests/integration/core-router-recovery/test_plan474_build_wait_receipt.py
rtk python3 tests/integration/core-router-recovery/test_plan464_attempt_receipt.py
  all passed (2, 2, and 6 tests respectively)
rtk bash scripts/check-runtime-boundaries.sh
rtk bash scripts/check-service-tunnel-boundaries.sh
  both passed
```

The final serial workspace run once encountered a load-sensitive failure in
`plan151_slow_writer_reverse_pressure_recovers`. The exact test passed on
standalone retry (1 passed, 9 filtered out), and the subsequent full serial
workspace run passed. An earlier Clippy pass identified the new SSU2 staging
helper's argument count; grouping trace inputs in `TracedStage` fixed it and
the final full Clippy run passed. No test or guard was weakened.

The complete AGENTS.md routine static/evidence floor passed, including planning
tests, all runtime/console/managed-app boundaries and self-tests, evidence and
vector manifests, workflow validation, and `cargo deny check advisories bans
sources`. The M6 checker printed its existing unbound-label warnings while
passing its 33 guarded labels. `cargo deny` printed existing duplicate-version
warnings for `block-buffer` and `windows-sys`.

Implementation commit: `7416a61` (`fix(core-router-recovery): observe build writer completion`).
No dependency was added. `specs/support.toml`, `specs/CONFORMANCE.md`, and
advertisement posture are unchanged.

## Compatibility, security, and limitations

Tickets are bounded and process-local; raw correlation IDs are not persisted or
added to wire messages. Receipt cleanup covers queue/stage rejection, partial
fragment submission, expiration, cancellation through route teardown, and
writer/link failure. The aggregation is non-atomic because already emitted
cells may continue through the network after another cell fails. Existing
strict authenticated terminal-peer and reply-ID checks are unchanged.

Findings: critical — none; high — none; medium — the pinned stock attempt still
has no correlated terminal reply, retained by Plans 477/478; low — no hosted CI
run for this closure.

## Unblock audit and disposition

Plans 477 and 478 already own active implementation work and consume this
interface; their state remains active because the stock reply and product
acceptance criteria are not closed. Plans 479 and 480 do not list Plan 481 as
their sole hard dependency. No additional registered plan became newly ready.
Disposition: closed for source-owned writer-completion infrastructure only;
no router, tunnel, SAM, interoperability, or anonymity capability is claimed.
