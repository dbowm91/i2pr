# Plan 069 closure — host-compatible NTCP2 loopback smoke lane

> **Supersession note (Plan 074, registered 2026-07-31).** This
> closure record is preserved as a snapshot of the scaffolding and
> fake-process test coverage that the Plan 069 runner implemented at
> the time of the Plan 074 amendment. The runner was structurally
> incapable of producing a mixed-router pass because it selected the
> i2pr launcher for both process handles, did not invoke the supplied
> i2pd binary as the reference process, and could promote protocol
> milestones without consuming real structured reference events. The
> Plan 064 i2pd listen/dial paths were terminal stubs when real
> pinned i2pd libraries were not linked. The Plan 069 lane is **not
> valid mixed-router evidence**; the corrected runner and the real
> i2pd driver are the responsibility of Plan 075 and Plan 076
> respectively. Plan 070 and Plan 071 are no longer active execution
> authority. The historical implementation record below is
> preserved verbatim.

## Implementation commit

The Plan 069 implementation lands in the same commit as this
closure record. The implementation surface is described in
`plans/069-host-compatible-ntcp2-loopback-smoke-lane.md` and the
focused checks below record the run results. No fabricated live
pass is claimed.

## Runner command examples

```text
bash scripts/interop/run-ntcp2-loopback-smoke.sh \
  --direction i2pr-to-i2pd-ipv4 \
  --reference-driver <path> \
  --reference-build-manifest <path> \
  --reference-source-lock <path> \
  --output <smoke-record.json> \
  --source-commit <40-lowercase-hex> \
  --network-audit-mode auto \
  --diagnostics-mode off
```

The runner accepts the two i2pd directions only; the Java and
Emissary directions are explicitly out of scope for the lane. The
diagnostics mode rejects any value other than ``off`` or
``sanitized``; raw payload capture is structurally unsupported.

## Focused tests and results

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
```

- `test_loopback_smoke.py` — 42 tests, all green. Covers the
  strict config parser, the CLI parser, the bounded helpers
  (DeliveryStatus message ID derivation, run ID generation, loopback
  port allocation), the network audit degradation, the runner
  orchestration (listener-before-dialer ordering, no-retry protocol
  failure, bounded address-in-use retry, run-root removal,
  external network destination staging), the record writer
  (passing record validation, cleanup-failure override), the
  independent guarantees (no Plan 056/066 authority, smoke schema
  markers, pinned i2pd reference, raw-diagnostics forbiddance,
  strict shell wrapper), and the failure staging surface.
- `test_loopback_smoke_record.py` — 35 tests, all green. Covers the
  Plan 068 smoke record schema contract (positive fixture,
  required-field enforcement, tier and metadata, timestamps and
  Router Hashes, message-id bounds, passed-record requirements,
  blocked-record path, secret-field rejection, digest verification).
- `test_plan065.py` — 29 tests, all green. Confirms the Plan 065
  strict scenario contract remains the canonical pre-Plan 069
  surface for the i2pr sender/receiver correlation; Plan 069
  consumes its message-id derivation algorithm and renderer
  conventions without introducing a competing schema.
- `bash scripts/check-ntcp2-loopback-smoke-boundary.sh` —
  succeeds. Verifies the runner module, shell entry point, and
  test matrix are committed; verifies the smoke record schema is
  referenced; verifies the runner is free of
  `verify_milestone3_certificate`, `candidate_record`, `plan060`,
  `plan066`, `rootless_topology`, `rootless_supervisor`,
  `multipass`, `evidence_bundle`, `export_acknowledgement`, and
  `raw-local` references; verifies the shell wrapper does not
  invoke `sudo`, `ip netns`, `setcap`, `--privileged`, `--network
  host`, or `/var/run/docker.sock`; verifies the direction
  allowlist is restricted to the two i2pd directions; verifies
  the Plan 069 plan-of-record is committed.

## Closure baseline

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

The closure baseline commands are recorded; the full historical
harness matrices, rootless checks, and Multipass checks remain
available for explicit integration checkpoints. Plan 070 will
execute the Plan 069 lane on the host or guest and may exercise
the full closure baseline as part of its qualification.

## Real reference pass

No real i2pd pass was incidentally executed during the Plan 069
closure. A real reference run is not required to close Plan 069
per the plan-of-record; Plan 070 owns the real execution and the
Milestone 3 status update based on the Level 1 outcomes.

## Next plan

The next plan is **Plan 075** — Plan 069 runner integrity and
evidence correction. Plan 075 owns the runner integrity fixes
required for the Plan 075-079 mixed-router sequence. Plan 070 and
Plan 071 are no longer active execution authority; the active
sequence is **Plan 075 → Plan 076 → Plan 077 → Plan 078 → Plan 079**.
The Plan 046 rootless sealed-namespace lane and the Plan 048/049
Multipass recovery lane remain the canonical external paths.
