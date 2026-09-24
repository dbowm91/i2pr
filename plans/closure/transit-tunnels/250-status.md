# Plan 250 status — M11 transit foundation semantic and ownership corrective

Status: **passed-m11-transit-foundation-semantic-and-ownership-corrective-infrastructure-only-m11-capability-not-claimed**

Plan of record:
[`plans/implementation/transit-tunnels/250-m11-transit-foundation-semantic-and-ownership-corrective.md`](../../implementation/transit-tunnels/250-m11-transit-foundation-semantic-and-ownership-corrective.md)

Date: 2026-09-24

## Outcome

Plan 250 corrects the runtime-neutral M11 foundation retained from Plan 249. The authenticated
sender is now an explicit `TransitBuildContext::previous_peer`, separate from the local hop
identity. `TransitAdmissionState` tracks bounded global and per-peer pending work through
move-only opaque tokens, without synchronization primitives; accepted registration commits
consume the token and every reject/fatal path releases it. Policy denials return the actual
sealed `BandwidthRejected (30)` record and leave the registry unchanged. Fatal decode,
time, RNG, sealing, and registry failures return `TransitFatalError`.

Accepted reply mappings contain `b` only. Allocation is bounded by the available share and
optional per-tunnel cap; `m` rejects only when it cannot be met, `r` above the cap is clamped,
and requests without `m` or `r` produce an empty reply mapping. Creation time is checked
against bounded future skew and the fixed 600-second wire lifetime; stored expiry is the
decoded creation time plus 600 seconds.

Secret-owning role, registration, registry, admission-state, and token owners are not
`Clone`. Reply sealing borrows `LayerKeys`; the unused `layer_state_seed` argument is gone.
Unknown registry removal returns `UnknownReceiveTunnelId`, and outcome `Debug` redacts the
218-byte sealed record. The new static M11 guard runs in ordinary Linux CI. No daemon/runtime,
config, dependency, listener, wire-advertisement, or public-network changes were made. M11
capability remains unclaimed.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Previous peer comes from authenticated caller provenance, not local hop identity | `sealed_reply_contains_only_allocated_b_and_uses_authenticated_previous_peer` uses distinct hashes and checks the committed registration | Passed |
| Per-peer active accounting separates senders | `active_per_peer_limit_rejects_one_sender_and_keeps_other_eligible` | Passed |
| Accepted wire replies contain `b` only | `m_only_wire_reply_has_b_and_no_request_fields`, `r_only_wire_reply_has_positive_b_and_no_request_fields`, `sealed_reply_contains_only_allocated_b_and_uses_authenticated_previous_peer`, `no_bandwidth_request_has_empty_accepted_mapping` decrypt and decode the emitted envelope | Passed |
| `r` above cap clamps while a satisfiable `m` is accepted | `r_above_local_cap_is_allocated_to_cap_when_minimum_fits`; m+r wire test checks b is at least m | Passed |
| Valid policy denial is sealed code 30 with no active state | `policy_rejection_is_sealed_code_30_and_releases_pending_reservation`; disabled, degraded, global-active-full, duplicate-id, and insufficient-m tests exercise the other admission arms | Passed |
| Future skew and 600-second expiry direction are correct | `request_time_accepts_bounded_future_and_rejects_expired_lifetime`, `timestamp_outside_skew_rejects_before_commit`, `expiry_at_lifetime_removes_registered_entry` | Passed |
| Pending ceilings use real tokens and remain peer-bounded | `pending_reservations_enforce_global_and_per_peer_limits_and_release_on_drop` covers global/per-peer ceilings and independent senders; `active_per_peer_limit_rejects_one_sender_and_keeps_other_eligible` | Passed |
| Pending count returns to baseline on reject, RNG failure, seal failure, and commit | `policy_rejection_is_sealed_code_30_and_releases_pending_reservation`, `rng_failure_rolls_back`, `reply_seal_failure_releases_pending_reservation`, accepted transaction tests | Passed |
| Token release cannot underflow | Pending test submits a second token with the spent id and verifies release is false and counters remain zero | Passed |
| Registry duplicate after reservation releases pending state | `registry_conflict_after_reservation_releases_pending_token` | Passed |
| Secret ownership and removal are safe | `registry_remove_unknown_id_is_typed_error`, `registry_remove_drops_entry_and_returns_secret`, `debug_output_does_not_expose_key_bytes`, `check-m11-transit-boundaries.sh` | Passed |
| No sync/runtime boundary drift | Static M11 guard rejects synchronization primitives and runtime imports; runtime-boundary checks passed | Passed |

## Verification record

```text
cargo fmt --all --check                                      passed
cargo check --locked --workspace --all-targets               passed
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
                                                              345 passed
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                              2926 passed, 26 ignored (103 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                              passed, no issues
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                              passed, 18 files generated
cargo test --locked --workspace --doc                         passed, 16 suites
cargo deny check advisories bans sources                       passed
bash scripts/check-dependency-direction.sh                     passed
bash scripts/check-runtime-boundaries.sh                       passed
bash scripts/check-service-tunnel-boundaries.sh                passed
bash scripts/check-m11-transit-boundaries.sh                  passed
bash scripts/check-java-source-lock-gating.sh                 passed
git diff --check                                               passed
```

The focused transit module has 57 passing tests, included in the final 345 crate tests. The
full workspace run passed all production code and 344 crate tests; the subsequently added
registry-conflict token-release test is included in the final focused crate run. No new
dependency was introduced. Cargo deny reported only the repository's existing duplicate
lockfile entries; advisories, bans, and sources all passed.

## Plan 249 authority update

Plan 249 remains a retained historical infrastructure pass. Its nine corrective findings
are addressed by this Plan 250 closure. The top amendment and superseded handoff in
[`249-status.md`](249-status.md) now point to this record and the corrected API; the original
Plan 249 execution evidence remains unchanged.

## Unblock audit and roadmap disposition

Plan 251 is already closed, and its ordinary CI run is green on its implementation SHA. No
other registered blocked plan lists Plan 250 as its sole dependency. Plan 252 daemon/runtime
composition is the only immediate successor; the roadmap already defines its intended scope,
but it must remain unregistered until ordinary GitHub Actions is green on this Plan 250
implementation SHA. After that gate, Plan 252 can be written and registered as ready. Plan
253 still depends on Plan 252 and remains unregistered. M12 floodfill planning remains
deferred until controlled M11 transit/resource evidence exists.

## Limitations

- This closes only the runtime-neutral infrastructure corrective; it does not claim transit
  capability or external interoperability.
- The bounded token ledger is single-owner state. Plan 252 owns runtime task/composition and
  must serialize access through explicit ownership rather than adding shared synchronization
  here.
- Ordinary CI on the pushed implementation SHA remains the final gate before Plan 252 is
  registered.
