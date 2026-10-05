# Plan 351 status — external adapter conformance and SAM handoff

Status: **`passed-portable-service-tunnel-external-adapter-conformance-and-sam-handoff`**.

Plan of record: [`351-external-adapter-conformance-and-sam-handoff-contract.md`](../../implementation/portable-service-tunnels/351-external-adapter-conformance-and-sam-handoff-contract.md).

## Implementation

- `0ea9927730b5962e9f5452a46f416505f4b6fa1a` — added the external Git-pinned consumer, conformance tests/checker, CI/floor wiring, and SAM adapter handoff reference.
- External package/API pin: `i2pr-service-tunnels` at Plan 350 revision `fa0824970b67b5ee90d5907765c408ccd0a19941`.
- The fixture at `tests/portable-service-tunnel-consumer/` has its own Cargo workspace and lockfile, stays outside the root workspace, imports only the public package, and uses a temporary target directory through `scripts/check-portable-service-tunnel-consumer.sh`.
- This closure commit updates this record, the portable roadmap, and `plans/registry.md`.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| True external package/revision use | Fixture manifest/lockfile and `bash scripts/check-portable-service-tunnel-consumer.sh`; resolved tree is pinned to `fa082497` | Pass |
| Generic client/server and validation | `generic_client_and_server_validate_through_public_values` | Pass |
| Dedicated and explicitly shared mixed client/server groups | `explicit_destination_groups_preserve_dedicated_and_shared_identity_domains` | Pass |
| Authenticated peer hash, allow/deny and monotonic rate input | `access_and_rate_policy_use_authenticated_hash_and_adapter_clock` | Pass |
| HTTP, SOCKS5 CONNECT, IRC behavior reused through core API | `http_client_privacy_and_server_filters_are_reused`; `socks_connect_parser_and_irc_privacy_filter_are_publicly_usable` | Pass |
| Malformed/max+1 bounded input | `malformed_and_max_plus_one_inputs_keep_typed_bounded_failures` | Pass |
| Deterministic generation/lifecycle and visible start failure | `generation_diff_is_deterministic_and_adapter_lifecycle_is_explicit`; `failed_adapter_start_is_visible_and_does_not_change_policy_generation` | Pass |
| No private source or daemon/runtime/testkit/router crate dependency | Consumer checker's source guard and resolved `cargo tree --edges normal` check | Pass |
| API declaration baseline unchanged | `python3 scripts/check-portable-service-tunnel-api.py` | Pass: 678 declarations |
| Clean-room SAM handoff, including identity/session mapping, authenticated peer requirements, lifecycle, and byte-flow diagrams | `specs/references/portable-service-tunnel-sam-adapter-handoff.md` | Pass; SHA-256 `a39f4c2f556bdc1274714e4614ec1f204139fa2f1ec2c74cd4546b5f0b19c73b` |
| No SAM implementation or capability/publication promotion | Change review; no `specs/support.toml` change; package remains `publish = false` | Pass |

The conformance matrix is also maintained in `tests/portable-service-tunnel-consumer/CONFORMANCE.md`.

## Verification

Commands ran from the repository root on the Plan 351 implementation head. Results:

| Command | Result |
|---|---|
| `rtk cargo fmt --all --check` | Passed |
| `rtk cargo check --locked --workspace --all-targets` | Passed |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed: 4,056 passed, 35 ignored, 147 suites (545.13s) |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed |
| `RUSTDOCFLAGS='-D warnings' rtk cargo doc --locked --workspace --no-deps` | Passed |
| `rtk cargo test --locked --workspace --doc` | Passed: 19 suites |
| `rtk bash scripts/check-portable-service-tunnel-consumer.sh` | Passed: 8 tests; package resolved to exact Git commit `fa082497` and no forbidden internal dependency appeared |
| `rtk cargo check --locked -p i2pr-service-tunnels --all-targets` | Passed |
| `rtk cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1` | Passed: 339 tests |
| `rtk cargo clippy --locked -p i2pr-service-tunnels --all-targets --all-features -- -D warnings` | Passed |
| `RUSTDOCFLAGS='-D warnings' rtk cargo doc --locked -p i2pr-service-tunnels --no-deps` | Passed |
| `rtk python3 scripts/check-portable-service-tunnel-api.py` | Passed: 678 declarations |
| `rtk bash scripts/check-dependency-direction.sh` | Passed |
| `rtk python3 scripts/check-global-plan-number-uniqueness.py` | Passed |
| `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed: 6 tests |
| `rtk bash scripts/check-runtime-boundaries.sh` | Passed |
| `rtk bash scripts/check-service-tunnel-boundaries.sh` | Passed |
| Remaining repository floor scripts from `AGENTS.md` | Passed, including fixture manifest, vectors, interop/static boundaries, M6–M12 and service-tunnel evidence checkers |
| `rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | Passed: 18 tests |
| `rtk cargo deny check advisories bans sources` | Passed: advisories/bans/sources OK; existing duplicate-version warnings remain |

The M12 floodfill `--self-test` printed expected negative-fixture rejection messages and returned success. M6/Streaming evidence checks also printed their existing non-blocking guarded-label coverage warnings and returned success.

## Security and compatibility disposition

No product runtime behavior or public API declarations changed. The consumer invokes existing public policy/filter functions instead of reimplementing them. Its fake authenticated-peer wrapper distinguishes authenticated Destination hash input from local or claimed identity. The handoff requires adapters to fail closed for peer-dependent policy when authenticated identity is unavailable, preserve explicit linkability groups, and place filters between SAM streams and local applications. The core remains free of SAM, sockets, tasks, and storage ownership.

No dependency was added to the i2pr workspace. The standalone fixture's Cargo lockfile records transitive third-party resolution; the core package is fetched at the exact pushed Plan 350 revision. Publication remains gated on owner-selected licensing. Findings by severity: critical/high/medium/low — none.

## Unblock audit and disposition

Plans 349, 350, and 351 form the complete registered portable-service-tunnel line. No successor plan is required to satisfy this line. A future independent SAM library/tunnel-manager repository may consume the pinned core and handoff; it is outside i2pr and is not implemented or advertised here. M10 Plan 215 remains authoritative for i2pr's service-tunnel product behavior.

Roadmap disposition: **closed**. No SAM implementation, binding, daemon, package publication, or router support claim was added.
