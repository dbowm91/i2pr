# Plan 350 status — package API and dependency stabilization

Status: **`passed-portable-service-tunnel-package-api-stabilization-publication-blocked-by-license-selection`**.

Plan of record: [`350-service-tunnel-package-api-and-dependency-stabilization.md`](../../implementation/portable-service-tunnels/350-service-tunnel-package-api-and-dependency-stabilization.md).

## Implementation

- `fa08249` — removed the dead `i2pr-proto` edge; normalized package metadata and crate docs; established the reviewed public declaration snapshot; tightened dependency/boundary checks; wired them into CI and the routine floor; updated architecture references.
- This closure commit updates this status, the portable roadmap, and `plans/registry.md`.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Plan 349 boundary is authoritative | Plans 349 closure, ADR 0033, and portable-core contract v1 | Pass |
| Remove unused internal dependency | `rtk cargo tree -p i2pr-service-tunnels --edges normal`; full `rg` audit; Cargo.lock diff; checker rejects every direct `i2pr-*` dependency | Pass: five third-party production deps only |
| Durable package metadata and docs | crate `Cargo.toml`, `README.md`, root rustdoc, `cargo metadata --no-deps` | Pass |
| Reviewed public API snapshot | `crates/i2pr-service-tunnels/API-SNAPSHOT.txt`; checker positive-control fixture; wired in CI and floor | Pass: 678 public declaration entries |
| Existing native consumers preserved | daemon foundation, adversarial matrix, and local-roundtrip suites | Pass: 7 + 12 + 9 tests |
| No package publication without selected license | metadata reports `license = null`, `license_file = null`, and `publish = []`; `publish = false` remains | Pass: publication accurately gated |
| No runtime/SAM/product change | source and diff review; M10 closure/support inventory unchanged | Pass |

## Package and dependency evidence

Before Plan 350, `cargo tree -p i2pr-service-tunnels --edges normal` showed the path edge to `i2pr-proto`, which itself brought `flate2` and another `sha2`/`zeroize` dependency path. Full-tree source search found no production or test use of `i2pr_proto`; occurrences were confined to the manifest and a stale architecture comment. After removal, the package tree contains only `base64ct`, `sha2`, `subtle`, `thiserror`, and `zeroize`; Cargo.lock no longer lists `i2pr-proto` under this crate.

The package uses version `0.1.0`, edition 2024, MSRV 1.88, the workspace repository URL, a crate README, a durable description, three keywords, and the `network-programming` category. Cargo metadata leaves both license fields empty and resolves `publish = false`. The owner has not selected a repository license, so `cargo package` was intentionally not run: the plan permits package staging/dry-run only when licensing/package metadata is eligible. No license was inferred or added.

The API snapshot records public module declarations, root re-exports, and public item declarations. It catches public declaration growth and removals; every snapshot update still requires human signature, semver, and secret-safety review. It adds no dependency.

## Verification

Commands ran from the repository root on implementation head `fa08249`:

| Command | Result |
|---|---|
| `rtk cargo fmt --all --check` | Passed |
| `rtk cargo check --locked --workspace --all-targets` | Passed |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed: 4,056 passed, 35 ignored, 147 suites |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed |
| `rtk env RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` | Passed |
| `rtk cargo test --locked --workspace --doc` | Passed: 19 suites |
| `rtk cargo tree -p i2pr-service-tunnels --edges normal` | Passed; only the five third-party dependencies listed above |
| `rtk proxy cargo metadata --format-version 1 --no-deps \| python3 -c 'import json,sys; d=json.load(sys.stdin); p=next(p for p in d["packages"] if p["name"] == "i2pr-service-tunnels"); print({k:p[k] for k in ("version","license","license_file","description","repository","categories","keywords","readme","publish","rust_version")}); print("dependencies:", [x["name"] for x in p["dependencies"] if x["kind"] != "dev"])'` | Passed; package metadata and publication gate recorded above |
| `rtk bash scripts/check-dependency-direction.sh` | Passed |
| `rtk python3 scripts/check-portable-service-tunnel-api.py` | Passed: 678 declarations |
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
| `rtk bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | Passed (existing non-blocking coverage warnings) |
| `rtk bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | Passed; expected negative-fixture diagnostics printed |
| `rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | Passed: 18 tests |
| `rtk cargo deny check advisories bans sources` | Passed (existing duplicate-version warnings; advisories, bans, and sources all OK) |

The first full workspace test attempt after the implementation commit failed during rustc writes with `No space left on device` (the worktree's generated `target/` occupied 33.4 GiB). `rtk cargo clean` removed only generated build artifacts, recovering 33.4 GiB; the exact command was rerun and passed with the result above. No source or test failure occurred in the failed attempt.

## Compatibility, security, and operations

No service behavior or public symbol visibility changed. The removed dependency was unused. API names remain on the pre-1.0 line and are now snapshotted for reviewed change control. Secret-handling code and authentication behavior were not changed. Package contents remain non-publishable pending an explicit owner-selected license. No migration is required; no new dependency was added. Findings by severity: critical/high/medium/low — none.

## Unblock audit and disposition

Plan 351 depends on the closed ownership contract (Plan 349) and a frozen public package/API revision (this plan). Both contracts are now stable. Actual public package publication is not a Plan 351 prerequisite; the fixture can pin the Git commit. Plan 351 is moved from blocked to ready. No other plan in this work line remains blocked on Plan 350.

Roadmap disposition: **closed with publication blocked by license selection**. No SAM protocol/session, daemon, FFI, user-visible product, support claim, or package publication was added.
