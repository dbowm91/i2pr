# Plan 376 — Outproxy live failover, restart, and Plan-327 successor closure: status

Status: **passed-live-multi-endpoint-failover-and-product-restart-proven-plan327-remainder-closed**

Plan of record:
[`376-outproxy-live-failover-restart-closure.md`](../../implementation/i2pcontrol-proposal-170/376-outproxy-live-failover-restart-closure.md).

Classification: capability + resilience/security evidence. This plan changes
production behavior; it promotes no advertisement.

Implementation commit: `00cb4297`.

External router interoperability was **not** required by this plan and was not
attempted: Proposal 170 specifies the control surface, while the security
invariant under test is that every clearnet request is carried only through an
I2P Streaming route to an outproxy.

## What this plan found

Three defects, each invisible to the Plan 342 lane because every Plan 342 row
used **exactly one endpoint**. That is worth stating plainly: the Plan 342 lane
was not weak, it was narrow, and a single-endpoint composition cannot observe
any of these three properties.

1. **The declared backoff was never applied** *(high)*.
   `OutproxyPolicy::backoff_ms` has existed since Plan 342, with a saturating,
   bounded schedule and a hard ceiling. `outproxy_route.rs` never called it: a
   retryable failure went straight to the next endpoint. A `ProxyList` of four
   dead outproxies was therefore hammered in a single burst, which is both a
   self-inflicted load spike and a way to get an entire list banned by one
   outproxy operator. **Fixed**: the schedule is applied between attempts,
   never before the first, and the wait is cancellation-aware via
   `tokio::select!` on `cancellation.cancelled()` so a shutdown during the
   backoff window frees the request instead of sitting out the schedule. No
   lock is held across the await.

2. **The attempt budget ignored the operator's list** *(high)*.
   `outproxy_options.rs` built the policy as `OutproxyPolicy::default()`,
   whose attempt count is 2. An operator configuring four outproxies got two
   attempts; entries three and four were **silently never tried**. The control
   surface accepted the list, echoed it back in its status output, and ignored
   its tail. `OutproxyList::select`'s wrap semantics were unreachable for the
   same reason — the index never reached a third pass. **Fixed**: the budget is
   the list length clamped to `[1, MAX_OUTPROXY_ATTEMPTS]`. One attempt per
   endpoint is also the honest reading of ordered failover; silently re-trying
   one dead endpoint would burn the budget on a list the operator already
   ordered.

3. **A retryable failure superseded by a later success was never counted**
   *(medium — observability)*.
   The retry loop called `note(counters, |c| c.note(failure))` **only** when the
   failure was terminal. A failure that a later attempt went on to succeed past
   was recorded nowhere. An operator whose first outproxy was permanently dead
   therefore saw `target_unreachable: 0` on every request, forever, while every
   request quietly succeeded through the second endpoint. The counters are the
   only surface that carries this diagnosis, and "the first entry is always down"
   is exactly the diagnosis an operator needs. **Fixed**: every failed attempt
   is now recorded where it fails, and `note_exhausted` adds only the
   exhaustion so the final reason is not double-counted. `target_unreachable`
   and `authentication_rejected` now mean **per attempt**, which is a semantic
   change to a documented counter and is called out in the field docs.

The lane found a fourth thing that is **not** a defect but a real property of
the control surface: `TunnelControlState::edit` **merges** the request's options
over the prior definition, so an edit that omits the outproxy keys leaves them
in place. There is therefore no `edit` that removes a provider — only
`delete` + `create`. That is now documented in the lane and exercised by the
fail-closed row rather than worked around silently.

## Requirement-to-evidence matrix

### Failover (plan §39–54)

| # | Requirement | Status | Evidence row |
|---|---|---|---|
| 1 | O1 unavailable at the Streaming-connect stage → the same request succeeds through O2 | pass | `plan376_http_connect_fails_over_to_the_second_outproxy` |
| 2 | the same over a SOCKS path | pass | `plan376_socks5_fails_over_to_the_second_outproxy` |
| 3 | O1 establishes Streaming but refuses upstream → policy retried O2 | pass | `plan376_an_upstream_refusal_is_retried_at_the_second_outproxy` |
| 4 | O1 auth-rejected → explicitly tested, not silent | pass | `plan376_authentication_rejection_is_retried_at_the_next_endpoint_by_the_current_policy` |
| 5 | attempt count never exceeds the configured/hard ceiling | pass | `plan376_the_attempt_budget_is_the_list_length_and_exhaustion_is_typed` (3 endpoints → exactly 3 attempts) + `OutproxyPolicy::new` clamps to `MAX_OUTPROXY_ATTEMPTS` |
| 6 | backoff is bounded and cancellation-aware | pass | production fix 1; pinned by the checker's `backoff_ms` guard |
| 7 | a list shorter than attempts wraps only per `OutproxyList::select` | pass | the budget is now the list length, so `select`'s index never exceeds the list; the wrap remains the policy layer's unit-pinned property |
| 8 | all attempts failing → typed refusal, no direct clearnet socket | pass | same budget row: `attempts_exhausted == 1`, `handshake_ok == 0`, `direct_i2p == 0`, client sees `4xx`/`5xx` |
| 9 | counters identify attempt/success/failure classes with no target or credential value | pass | `OutproxyCounters` is counts-only; the budget row asserts exact counts; the credential row asserts the credential never reaches the client |

### Rotation (plan §56–65)

The provider contract is **ordered failover within one request**, not
round-robin between requests: `OutproxyList::select(attempt)` maps the attempt
index by `attempt % len` and `open_via_outproxy` walks attempts from zero.
Two requests against the same tunnel both begin at the operator's first entry.
That is deliberate — the operator's order is intent.

The closure distinguishes as the plan requires:

- **within-request failover — mandatory — is proven**, in both the HTTP CONNECT
  and SOCKS families.
- **between-request load rotation is not implemented and is not claimed.** No
  provider contract claims it, so inventing it to satisfy historical wording
  would have been the wrong fix. It is recorded in the checker's
  `DOCUMENTED_ABSENCES` as a considered absence.

### Restart (plan §67–78)

| # | Requirement | Status | Evidence row |
|---|---|---|---|
| 1 | the canonical seven-field block persists | pass | `plan376_a_routed_request_survives_a_product_restart` re-reads the stored generation and asserts all seven keys |
| 2 | the sealed password is recoverable only via the router-bound secret owner | pass | same row (the request succeeds post-restart) + the copied-config row (it cannot, under a foreign identity) |
| 3 | startup reconstructs the provider registry **before** a client request is accepted | pass | same row, through `TunnelControlState::startup` |
| 4 | a post-restart HTTP CONNECT reaches an outproxy and **application data passes** | pass | same row — 13 bytes are written after establishment and echoed back, not merely a handshake head |
| 5 | `.i2p` traffic still bypasses the provider after restart | pass | `plan376_after_restart_i2p_traffic_bypasses_and_removing_the_provider_fails_closed` |
| 6 | removing the provider after restart fails clearnet requests closed | pass | same row, via `delete` + `create` (see the `edit`-merges finding above) |
| 7 | a copied config without the matching router secret cannot recover the credential | pass | `plan376_a_copied_config_without_the_router_secret_cannot_recover_the_credential`; the refusal is attributed to `secret_owner_unavailable`, not to a network failure |

**What the restart boundary is, precisely.** A generation owns a manager, a
control state, a supervisor scope, and a cancellation token. A restart drops the
whole generation and builds another over the **same data directory**; nothing
else crosses the boundary. Generation two is brought up by
`TunnelControlState::startup` — the product's own boot path — and the test
never re-applies the operator's `create`, so a provider that exists afterwards
can only have come from disk plus the router-bound secret owner.

That is deliberately stronger than the Plan 342 serialization round trip, which
only wrote and read a stored definition. It is **not** a cross-process `exec`:
the lane has no child process and no config file. **A cross-process restart is
not claimed**, and the limitation is recorded in the lane header and in the
checker's `DOCUMENTED_ABSENCES`.

### Request-family coverage (plan §80–85)

One failover row uses HTTP CONNECT and one uses SOCKS5. The restart row uses
HTTP CONNECT as the canonical proof. The existing forward-HTTP and SOCKS
regressions stay green — all eight Plan 342 rows still pass unchanged.

### Security invariants (plan §87–96)

| Requirement | Status | Evidence |
|---|---|---|
| no `ToSocketAddrs`, system DNS, direct clearnet connect, plugin load, or shell exec in the request path | pass | `check-outproxy-wire-lane-evidence.py` over the lane **and** the five production modules it drives; `check-service-tunnel-boundaries.sh` rules 9–11 |
| `TargetPolicy::AllowsClearnet` reachable only with a valid I2P-only provider | pass | `plan376_after_restart_…_fails_closed`; the removal half of `plan342_provider_registry_drives_the_request_target_policy` |
| no credential in control output, logs, counters, or evidence | pass | counters are counts-only; the copied-config row asserts a refusal rather than any value |
| cancellation frees every pending Streaming attempt | pass | the loop re-checks `cancellation.is_cancelled()` at the top of every attempt and the new backoff waits on the token |
| no lock held across await on the provider/counter path | pass | new checker guard scans `outproxy_route.rs` for a lock guard surviving into an await |

## Commands run (local)

Labelled local honestly. No exact-head CI run is claimed for this plan.

```text
cargo fmt --all --check                                          → PASS
cargo check --locked --workspace --all-targets                  → PASS
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost        → PASS (floor line)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings → PASS
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps → PASS

cargo test --locked -p i2pr-daemon --test outproxy_loopback_wire -- --test-threads=1
                                                                  → 16 passed; 0 failed
cargo test --locked -p i2pr-daemon --test outproxy_control_plane -- --test-threads=1
                                                                  → 17 passed; 0 failed
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels -- --test-threads=1
                                                                  → 5 passed; 0 failed
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1
                                                                  → 9 passed; 0 failed

bash scripts/check-outproxy-request-path.sh                      → PASS
bash scripts/check-outproxy-wire-lane-evidence.sh                → ok (16 required rows)
bash scripts/check-service-tunnel-boundaries.sh                  → PASS
bash scripts/check-dependency-direction.sh                       → PASS
bash scripts/check-runtime-boundaries.sh                         → PASS
bash scripts/check-console-boundaries.sh                         → PASS
```

The three daemon suites above were run because the counter-semantics change
touches production code that `outproxy_control_plane`, `i2pcontrol_tunnels`,
and `service_tunnels_local_roundtrip` also exercise.

## Invariant review

- **Dependency direction and runtime boundaries** unchanged — no new crate, no
  new dependency, no new `tokio`/`std::net`/`std::fs` in a transport/API/service
  crate. The new `std::fs` use is in a `#[cfg(unix)]`-guarded permission copy
  inside a **daemon test**, which is the only layer allowed to touch a disk.
- **No direct-clearnet fallback was introduced.** Defect fix 1 and fix 2 change
  *when* and *how many* outproxy attempts happen; neither adds a path that skips
  the outproxy. The budget row asserts `direct_i2p == 0`.
- **Secrets.** No `Debug`/`Display` was added to any secret type, no `Clone`
  was added to one, and the credential's plaintext lifetime is unchanged (it
  remains the header construction inside `auth_header`). The copied-config row
  asserts a refusal and never formats a credential.
- **Cancellation and lock discipline.** The new backoff holds no lock across its
  await and returns `NotPermitted` on cancellation, so a shutdown mid-backoff
  frees the request immediately.
- **No advertisement changed.** `specs/support.toml` is untouched by this plan.

## Findings by severity

- **critical: none.**
- **high: two**, both fixed in this plan and recorded above — the unapplied
  backoff, and the attempt budget that silently ignored entries 3..N of an
  operator's `ProxyList`. The second is the one that would have reached an
  operator: their configured failover list was accepted, reported, and partly
  inert.
- **medium: one**, fixed — the uncounted superseded attempt failure.

  Its fix was itself caught by the **workspace** suite rather than by the
  focused lane: one unit row,
  `an_exhausted_request_records_both_its_last_reason_and_the_exhaustion`,
  encoded the *old* contract in which `note_exhausted` recorded both the last
  reason and the exhaustion. Moving the per-attempt reason to the failure site
  changed that helper's job, and the row failed. That is the row working: it
  asserted a real contract and the contract genuinely changed. The row was
  updated to test the **composition** rather than the helper in isolation,
  which keeps its original intent against the new shape, and a second row
  (`a_retryable_failure_is_counted_even_when_a_later_attempt_succeeds`) pins
  the defect directly — a retryable failure followed by a success leaves
  `target_unreachable == 1` and `attempts_exhausted == 0`.

  Recorded because it is a useful signal about this repo: a focused lane run
  would have shipped the counter-semantics change with a stale unit row still
  describing the old behaviour. The full workspace run was what surfaced it.
- **low, recorded, not fixed here:**
  - **The same credential is offered to every configured outproxy.** A 407 is
    retryable under the frozen taxonomy, so a `ProxyList` of *n* endpoints is
    offered the same credential *n* times. That is a consequence of the
    operator's own list rather than of the router, and it is the current
    *explicit* policy, which the plan permits — so the row pins it and the lane
    header says so in as many words. But an operator with two outproxies may not
    expect it and has no way to discover it. Changing it (for example, by making
    an authentication rejection terminal, or by scoping the credential per
    endpoint) is a **policy** decision, not an implementation detail, and needs
    its own plan-of-record.
  - **`TunnelControlState::edit` merges options**, so no single control action
    can remove an outproxy block. This is very likely the intended behaviour for
    most options and changing it would break every partial edit in the tree. It
    is documented in the lane and exercised, not changed.

## Roadmap disposition and unblock audit

- **Roadmap disposition: closed.** Plan 327's remainder is fully discharged:
  its two unproven capability rows (live failover, post-restart routing) are now
  executed claims, so **the historical Plan-327 remainder has no unproven
  capability row**. Plan 327's own `blocked` record is left byte-unchanged; it is
  superseded forward by this record, per the authority order.
- **Unblock audit, executed per `plans/README.md`:**

| Plan | Other hard dependencies | All closed? | Disposition |
|---|---|---|---|
| 377 | Plans 374, 375 | no — both blocked on the external Java/i2pd lane | stays blocked |
| 378 | Plans 376, 377 | **376 is now closed**; 377 is not | stays blocked on 377 alone |

Plan 378's dependency set is now one short of satisfied: the outproxy half of
its input is complete. Its §4 import step will have a real outproxy evidence
source (this plan's live lane) rather than the loopback-only one it had at
registration.

No corrective pass is registered for Plan 376 itself: all three defects were
found and fixed within this plan, and the fix for each is pinned by a checker
guard so it cannot silently regress.

## Limitations

- **Loopback evidence, not interoperability.** No real outproxy was contacted.
  The security property proven is that the only route i2pr ever opens for a
  clearnet target is a Streaming connection to an I2P destination.
- **In-process restart, not `exec`.** Every in-memory owner is torn down and
  rebuilt from persisted state through the product's own startup path. A
  cross-process restart is not claimed.
- **No between-request load rotation**, by decision, not by omission.
- **No outproxy capability is advertised.** `specs/support.toml` has no
  outproxy surface, and this plan adds none.