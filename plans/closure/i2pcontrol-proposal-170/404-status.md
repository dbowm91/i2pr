# Plan 404 closure — retained; DH failure did not reproduce

Status: **retained-authorized-dh-failure-not-reproduced-on-following-pinned-run**.

Plan: `plans/implementation/i2pcontrol-proposal-170/404-authorized-dh-consumer-activation-attribution.md`.

## Requirement-to-evidence

| Requirement | Evidence | Result |
|---|---|---|
| Verify typed consumer credential reached the manager | New bounded `consumer-credential-sealed=true` evidence; no secret value is emitted. | Pass |
| Attribute activation state for the DH attempt | Successful run moved from `pre-activation-pending=Some(true)` and no failure to `post-encrypted-target-status=Some(Resolved)`, `post-activation-pending=Some(false)`, `post-activation-failure-present=false`. | Pass |
| Verify complete DH consumer and reverse paths | Run `target/interop/els2-evidence-plan404-dh-activation-stage-20261009b`; all required controls, authority payload, i2pr consumer payload, and reverse i2pd payload passed. Reverse delta had one complete owner-matched authenticated Garlic and one accepted Streaming packet. Evidence SHA `f36ec172f15ebb3993fb71390818e5b6b1e6ee7c63033b2aa29e1d8b0312e37a`; results SHA `d59efca246e315130d8a537ba0faa9ce37002e001e1f77da774ddbe186c9277c`; driver SHA `03c2ae25e31e4981d8afe3c160070cb69313852dd4a7439eef68d6ef650bf24a`. | Pass |
| Explain the earlier failed DH attempt | `target/interop/els2-evidence-plan404-dh-stage-attribution-20261009` had sealed credential present, no encrypted-target status, no deferred failure stage, and one failed connect; i2pd teardown reported a process segfault. The follow-up same-pin run completed end to end. Evidence SHA `413eeeca008fe43c1c208792767d07ffae78bbde0a7541912be4ab526309986a`; results SHA `b8764d24e55570649017030f253d6668c63cb156b0ab80a3d8c8be98a1642bb4`; driver SHA `e3f06627ff5e7df755de22b33f01bf2e6ef2e8a4470955893062a69022448cd0`. | Retained anomaly; exact cause unresolved |

## Commands and outcomes

- `cargo fmt --all --check` — pass.
- ELS2 checker, runner self-test, evidence guard, encrypted-consumer caller guard — pass.
- Focused `cargo check --locked -p i2pr-daemon --test els2_i2pd_external` — pass.
- `cargo test --locked -p i2pr-daemon --test encrypted_service_consumer_wiring -- --test-threads=1` — 18 passed.
- `cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_client_credential -- --test-threads=1` — 10 passed.
- `cargo test --locked -p i2pr-netdb --test els2_client_authorization -- --test-threads=1` — 31 passed.
- Exact-pinned DH failed once, then passed on a separate invocation. Each invocation retained `MAX_ATTEMPTS=1` and its own artifact.
- No production behavior changed. The first attempt remains unexplained and is not erased or counted as success.

## Disposition and unblock audit

Plan 404 is retained because the initial failure's exact cause was not identified; the failure did not reproduce on the subsequent pinned run. Plan 405 is registered to run a final NONE/PSK/DH matrix with the final guarded requester and execute the full routine acceptance floor. No other registered work becomes dependency-ready. The independent Java plan 375 remains blocked; 377 and 378 remain blocked on their existing dependencies. No support or advertisement changes.
