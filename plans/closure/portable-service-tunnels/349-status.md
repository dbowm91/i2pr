# Plan 349 status — portable service-tunnel boundary and ownership contract

Status: **`passed-portable-service-tunnel-boundary-and-ownership-contract`**.

Plan of record: [`349-portable-service-tunnel-boundary-and-ownership-contract.md`](../../implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md).

## Implementation

- `1eb1a7c` — added ADR 0032, the portable-core contract and module/export inventory, architecture references, and Plan 349 runtime/dependency boundary checks with positive controls.
- This closure commit updates this record, the portable roadmap, and `plans/registry.md`.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Durable ownership and dependency direction | [ADR 0032](../../../docs/adr/0032-portable-service-tunnel-policy-core-and-adapters.md); [portable-core contract](../../../specs/references/portable-service-tunnel-core-v1.md) | Pass |
| Public/internal module classification | Contract § Module and export inventory lists every current public root re-export grouped by module, and classifies every production module | Pass |
| Adapter metadata, authenticated identity, linkability, lifecycle | Contract § Adapter inputs and outputs | Pass |
| Runtime/router ownership prohibited by static checks | `scripts/check-service-tunnel-boundaries.sh`; dependency/runtime boundary checks | Pass |
| Guard detects forbidden inputs | Checker positive controls cover daemon/NetDB/proto dependency spellings and Tokio/socket/filesystem source spellings | Pass |
| Existing native behavior unchanged | Workspace test suite and service-tunnel acceptance evidence checker | Pass |
| No SAM implementation, publication, or support promotion | Diff review; `publish = false`; no `specs/support.toml` change | Pass |

## Verification

Commands ran from the repository root on the implementation head, `1eb1a7c`:

| Command | Result |
|---|---|
| `rtk cargo fmt --all --check` | Passed |
| `rtk cargo check --locked --workspace --all-targets` | Passed |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed: 4,056 passed, 35 ignored, 147 suites |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed |
| `rtk env RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` | Passed |
| `rtk cargo test --locked --workspace --doc` | Passed: 19 suites |
| `rtk bash scripts/check-dependency-direction.sh` | Passed |
| `rtk python3 scripts/check-global-plan-number-uniqueness.py` | Passed |
| `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed: 6 tests |
| `rtk bash scripts/check-runtime-boundaries.sh` | Passed |
| `rtk bash scripts/check-service-tunnel-boundaries.sh` | Passed |
| `rtk bash scripts/check-fixture-manifest.sh` | Passed |
| `rtk bash scripts/check-ntcp2-vectors.sh` | Passed |
| `rtk bash scripts/check-ssu2-vectors.sh` | Passed |
| `rtk bash scripts/check-i2cp-vectors.sh` | Passed: 15 vector tests |
| `rtk bash scripts/check-ntcp2-interoperability.sh` | Passed |
| `rtk bash scripts/check-constrained-host-lane-boundary.sh` | Passed |
| `rtk bash scripts/check-m11-transit-boundaries.sh` | Passed |
| `rtk bash scripts/check-m11-transit-qualification-evidence.sh` | Passed |
| `rtk bash scripts/check-sam-acceptance-evidence.sh` | Passed |
| `rtk bash scripts/check-ssu2-acceptance-evidence.sh` | Passed |
| `rtk bash scripts/check-i2cp-acceptance-evidence.sh` | Passed |
| `rtk bash scripts/check-i2pcontrol-acceptance-evidence.sh` | Passed |
| `rtk bash scripts/check-service-tunnel-acceptance-evidence.sh` | Passed |
| `rtk bash scripts/check-exploratory-tunnel-evidence.sh` | Passed |
| `rtk bash scripts/check-netdb-tunnel-evidence.sh` | Passed |
| `rtk bash scripts/check-destination-tunnel-evidence.sh` | Passed |
| `rtk bash scripts/check-streaming-tunnel-evidence.sh` | Passed (existing non-blocking coverage warnings) |
| `rtk bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | Passed (existing checker output includes retained coverage warnings) |
| `rtk bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | Passed; intentional negative fixtures emitted their expected rejection diagnostics |
| `rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | Passed: 18 tests |
| `rtk cargo deny check advisories bans sources` | Passed (existing duplicate-version warnings; advisories, bans, and sources all OK) |

An additional, non-floor probe `rtk bash scripts/check-m6-final-closure-evidence.sh --self-test` was also tried and failed because the generated `target/interop/m6-mixed-router-evidence/evidence.json` is absent. It is not a Plan 349 checker or a repository-floor command and no Plan 349 claim depends on it.

## Compatibility, security, and operations

No production behavior or API visibility changed. Authenticated peer identity remains transport-provenance input; local names and addresses cannot substitute for it. Destination group sharing stays explicit. The reusable crate retains no runtime, socket, filesystem, DNS, process, NetDB, or transport ownership. The contract explicitly leaves key storage, connections, clocks, cancellation, reconnect, and local target I/O to adapters.

No dependency changed. The `i2pr-proto` source-use proof and any dependency removal are Plan 350 work. No migration is required. No new findings were identified (critical/high/medium/low: none).

## Unblock audit and disposition

`plans/registry.md` and the portable service-tunnels roadmap were audited. Plan 350's only hard dependency was Plan 349; its boundary contract is now stable, and it is moved from blocked to ready. Plan 351 remains blocked on Plan 350. No other plan in this line is unblocked by Plan 349 alone.

Roadmap disposition: **closed**. M10 Plan 215 remains its product authority. This work adds no SAM protocol implementation, user-visible tunnel capability, public package release, license decision, or router support claim.
