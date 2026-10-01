# Plan 277 status — stopped at daemon/runtime integration boundary

Status: `stopped-m12-daemon-runtime-publication-and-reply-adapter-contract-required`

## Implementation evidence

Partial implementation is committed with this status record. It adds a typed role state machine,
permit-gated local `caps=f` composition, bounded coordinator effect admission, authenticated
DatabaseStore/DatabaseLookup dispatch from the existing single I2NP decode, and focused tests.
The controlled profile remains unavailable in normal configuration.

## Requirement disposition

| Requirement | Evidence | Result |
|---|---|---|
| Role readiness / health withdrawal | `crates/i2pr-netdb/src/floodfill_role.rs`; role transition test | Implemented locally |
| Default-off and typed advertisement authority | `crates/i2pr-netdb/src/local.rs`; permit-gated builder test | Implemented locally |
| Bounded effect queue and resource leases | `crates/i2pr-daemon/src/floodfill.rs`; exact-capacity/backpressure test | Implemented locally |
| Authenticated SSU2 DatabaseStore/Lookup decode | `router_i2np::dispatch_router_i2np_with_floodfill_body` and coordinator ingress | Compile-tested; no live role wiring |
| Runtime-owned publication snapshot bridge | M12 dependency-direction guard forbids daemon's direct `i2pr-transport-ssu2` dependency | Not implemented; requires a narrow runtime-owned API |
| Outbound DeliveryStatus/direct reply/direct replication adapter | No controlled adapter is wired into the daemon SSU2 owner | Not implemented |
| Requested tunnel reply with supplied-key ECIES | Coordinator preserves route/protection intent; no existing protocol-correct daemon reply seam is integrated | Not implemented |
| Persistence recovery, lifecycle drain/restart, RouterInfo before/active/withdraw bytes | No live coordinator owner or restart path | Not implemented |
| Plan 278 controlled external qualification | Requires the full Plan 277 local path | Blocked |

## Verification

Local commands and results:

- `cargo check --offline -p i2pr-daemon` — passed (after removing the forbidden direct SSU2 dependency).
- `cargo test --locked -p i2pr-daemon floodfill::tests::normal_default_stays_off_and_queue_backpressure_releases_budgets -- --exact --test-threads=1` — 1 passed.
- `cargo test --locked -p i2pr-netdb floodfill_role::tests -- --test-threads=1` — 1 passed.
- `cargo fmt --all --check` — passed.
- `cargo clippy --locked -p i2pr-daemon -p i2pr-netdb --all-targets -- -D warnings` — passed.
- `bash scripts/check-dependency-direction.sh` — passed.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- `bash scripts/check-m12-floodfill-boundaries.sh` — passed.
- `git diff --check` — passed.
- `cargo test --all-targets` suites required by Plan 277 were not run; the combined verification command was interrupted while waiting without output. Full local daemon-path acceptance is therefore not established.

## Findings, security, and compatibility

No critical/high finding was identified in the partial surface. Medium integration gaps remain as
listed above. The attempted daemon-to-SSU2 direct dependency failed the repository's enforced
dependency-direction guard and was removed. The normal daemon still cannot create a floodfill
permit through configuration. No RouterInfo `caps=f` production path is enabled.

## Unblock audit and disposition

Plan 278 remains blocked because its exact-pinned i2pd test requires the unimplemented local
full-path daemon owner and reply adapter. Plan 279 remains blocked on Plan 278 and must continue to
gate broad normal-daemon advertisement. Plan 269 was already passed; no other registered plan is
made eligible by this partial result. No acceptance-evidence checker or plan-unblock claim is
made. Roadmap disposition: stopped pending a reviewed runtime publication and delivery contract,
then completion of Plan 277's live owner, reply paths, drain/restart behavior, and required tests.
