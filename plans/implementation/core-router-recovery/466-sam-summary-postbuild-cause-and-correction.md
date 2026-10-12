# Plan 466 — SAM post-build summary cause and correction

**State:** blocked P2. Closure: [`466-status.md`](../../closure/core-router-recovery/466-status.md). Owner: core-router-recovery / destination-product. The field receipt run regressed earlier to `outbound-build`: three attempts were admitted and emitted, two received correlated authenticated replies and installed, while the third produced no matched reply before the deadline. The first missing transition is after UDP emission and before authenticated reply correlation; stock acceptance versus reply emission remains unproven. No source correction is justified.

## Objective

Record the three existing public summary values without destination or peer identifiers, reproduce the exact failure once, and correct only the failed product invariant. If all three values are valid but the status remains incomplete, fix the branch/reporting defect. Qualify the corrected normal-process SAM path with one bounded exact-pinned run.

## Work packages

1. Add a source-owned SAM summary receipt with `inbound_receive_count`, `lease_count`, and `inbound_owner_registered`; extend the runner to retain only those three values and the attempt-stage counts. Add deterministic tests for all seven false/true predicate combinations and status mapping.
2. Build managed-app siblings, verify the exact i2pd pin, and run the existing four-router lane once. Do not repeat the unchanged setup. Attribute the exact failed predicate.
3. Make one narrow source correction with a deterministic regression. Candidate fixes must repair the identified lifecycle/data invariant; do not weaken the SAM gate or synthesize leases/owners.
4. Run exactly one fresh pinned post-correction qualification. Pass only if SAM SESSION CREATE succeeds, actual nonzero-hop inbound/outbound tunnels remain installed, a signed LS2 has real leases, and both-direction external STREAM payload bytes match exactly with bounded teardown/restart controls. If no correction is justified or the acceptance row remains blocked, stop with the exact first missing condition.

## Invariants

- Keep the four-peer i2pd references source- and binary-pinned and unmodified; use loopback and fresh private datadirs.
- No zero-hop or synthetic reply, fabricated LeaseSet, direct router socket, public network, identity-bearing logs, or support promotion.
- Commit only aggregate sanitized receipt data; raw reference logs and identities remain temporary.

## Verification and closure

Build `i2pr-app-fixture`, `i2pr-apphost`, `i2pr-appd`, and `i2pr-appctl` before focused daemon tests. Run the summary state-machine tests, parser tests, `cargo test --locked -p i2pr-daemon --test run_lifecycle_readiness -- --test-threads=1`, `cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1`, `cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1`, the one diagnostic run, and the one post-fix run only if a source correction is established. Run the full `AGENTS.md` floor for production changes.
