# Plan 339 — Per-family network status, error, and testing owners

Status: **passed-per-family-network-condition-owners-with-pinned-i2pd-enumeration**

Classification: capability + source-ownership completion (reopen of Plan 322, Group B).

Hard dependencies: Plan 322 closed blocked; Plans 320/321/323/324 passed.

Subsystem: `i2pcontrol-proposal-170`.

## Objective

Give five of Plan 322's eight unavailable canonical RouterInfo selectors truthful
production owners:

| Selector | Proposal meaning | Declared type |
|---|---|---|
| `i2p.router.net.status.v6` | IPv6 network status code | `int` |
| `i2p.router.net.error` | IPv4 network error code | `int` |
| `i2p.router.net.error.v6` | IPv6 network error code | `int` |
| `i2p.router.net.testing` | whether IPv4 network is in testing state (0 or 1) | `int` |
| `i2p.router.net.testing.v6` | whether IPv6 network is in testing state (0 or 1) | `int` |

This plan does **not** touch the three transit selectors
(`i2p.router.net.total.transit.bytes`, `i2p.router.net.bw.transit.15s`,
`i2p.router.net.tunnels.shareratio`). Those remain Plan 322 Group A and are a
production transit-participation posture change, not a snapshot addition.

## Vocabulary authority (derived, not invented)

Proposal 170 rev 2026-05-20 marks **all five** selectors *"(adopted from
i2pd)"*. The enumeration is therefore i2pd's, not i2pr's to choose. The Proposal
text was re-retrieved read-only for this plan and its SHA-256 re-verified in-repo
against the `docs/provenance/proposal-170-manifest.md` pin:

```text
https://i2p.net/proposals/170-i2pcontrol-expansion.txt
19 010 bytes
f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc
```

At i2pd pin `2c694149fa6996eaeb23e378d5f83c9d3232c22f`,
`libi2pd/RouterContext.h:44-72` declares the two enumerations the Proposal
adopts, and `daemon/I2PControlHandlers.cpp:41-46,208-233` projects them into
these exact keys as `(int)`:

```text
RouterStatus: 0 OK, 1 Firewalled, 2 Unknown, 3 Proxy, 4 Mesh, 5 Stan
RouterError:  0 None, 1 ClockSkew, 2 Offline, 3 SymmetricNAT,
              4 FullConeNAT, 5 NoDescriptors
testing:      0 or 1 (bool)
```

i2pd is read here as a **readable ambiguity reference only**. No i2pd source is
reused, vendored, or patched.

## Emission policy (i2pr's own, conservative, recorded)

The *vocabulary* is i2pd's. The *conditions under which i2pr emits a code* are
i2pr's own truthfulness policy, and are recorded here rather than inferred.

Status is derived only from a **fresh** `ReachabilitySnapshot` whose
`family` is the queried family. A snapshot qualified for IPv4 says nothing
about IPv6, so the IPv6 selectors report `Unknown` unless the qualified family
is itself IPv6.

```text
effective state for family F = snapshot.state   if snapshot.family == F and fresh
                              = Unknown         otherwise (incl. no snapshot)

Reachable                      -> 0 OK
Firewalled | Unreachable       -> 1 Firewalled
everything else                -> 2 Unknown
3 Proxy, 4 Mesh, 5 Stan        -> never emitted (no such posture; Proxy would
                                  also contradict the no-direct-clearnet rule)
```

Error codes are emitted only where i2pr owns a real detector:

```text
5 NoDescriptors  -> emitted when the attested NetDB peer snapshot is empty
2 Offline        -> emitted only when the family was configured but its
                    transport is not running (never-configured is not a fault)
0 None           -> otherwise
1 ClockSkew      -> never emitted: no detector; `router.clockskew` is still a
                    neutral constant, so claiming skew would be fabrication
3 SymmetricNAT   -> never emitted: no NAT-type detection
4 FullConeNAT    -> never emitted: no NAT-type detection
```

Testing is `1` only while a determination is genuinely in progress for that
family, and `0` otherwise:

```text
ObservedUnconfirmed | CandidateReachable -> 1
Reachable | Firewalled | Unreachable | Unknown -> 0
```

The honest ordinary-production baseline is therefore
`status=2, error=0, testing=0` — "no claim, and not testing" — with
`error=5` additionally correct on a router that has not reseeded. No fabricated
`OK`, no fabricated `Firewalled`, and no fabricated "testing" for a test that is
not running.

## Why ready now

- The real state already exists and is maintained:
  `i2pr_transport::ReachabilityState` (`Unknown`, `ObservedUnconfirmed`,
  `CandidateReachable`, `Reachable`, `Firewalled`, `Unreachable`) with
  `AddressFamily` and `expires_at`; `Ssu2SocketConfig { ipv4, ipv6 }` bound
  state; `Ssu2Snapshot` bounded failure counters.
- The canonical dispatch seam and its fail-closed gap contract are established
  (`i2pcontrol.rs` RouterInfo dispatch; `InspectionGap`).
- The Proposal enumeration is now pinned to bytes, so the wire vocabulary is
  not a judgement call.
- Evidence is local and loopback-only: no external router, no transit
  participation, no M11 or constrained-host lane interaction.

## Production changes

1. `crates/i2pr-transport/src/network_status.rs` (**new**, runtime-neutral,
   `#![forbid(unsafe_code)]`): `NetworkStatusCode`, `NetworkErrorCode` with
   `as_i64()` and bounded `try_from_i64`, plus three pure derivations
   (`network_status_code`, `network_error_code`, `network_testing_flag`).
   No I/O, no Tokio, no sockets, no `async fn`.
2. `crates/i2pr-runtime/src/ssu2_runtime.rs`: one new bounded public
   accessor returning a privacy-safe per-family condition snapshot derived from
   the existing `state.reachability` and configured/bound socket state. The
   runtime keeps ownership of its own state; the daemon does not reach into
   private fields. No secret, endpoint, key, or peer identity is exposed.
3. `crates/i2pr-daemon/src/i2pcontrol_inspection.rs`:
   `proposal_network_condition_value` composing the runtime per-family
   snapshot with the existing attested NetDB peer snapshot, returning
   `InspectionGap` when no observation exists.
4. `crates/i2pr-daemon/src/i2pcontrol.rs`: route the five keys in the canonical
   RouterInfo dispatch, failing the whole request closed on any gap.
5. `crates/i2pr-i2pcontrol/src/source_matrix.rs`: convert the three gap row
   groups to published-gated rows naming the real owners and new test ids.

## Invariants

- No new unbounded channel, queue, or task. The snapshot is a fixed-size value.
- No new dependency. No capability, version, RouterInfo, SAM, or I2CP
  advertisement change; `advertised` stays false.
- Never fabricate: an unobserved family is `Unknown`, not `OK`.
- Never claim a code whose detector does not exist (ClockSkew, SymmetricNAT,
  FullConeNAT, Proxy, Mesh, Stan).
- The emitted integer is bounded to the i2pd enumeration; a value outside
  `0..=5` is a decode error, never clamped into range.
- Lock poisoning degrades to the gap path, never to a fabricated value.
- Transit participation stays disabled; no M11/constrained-host lane change.

## Tests and evidence

- `i2pr-transport` unit table over the full
  `ReachabilityState x family x freshness` cross product, asserting the exact
  code for every cell, including that a stale or foreign-family snapshot never
  yields `OK` or `Firewalled`.
- `try_from_i64` bounds rows: `0..=5` accepted, `-1` and `6` rejected.
- Runtime accessor: per-family effective state, configured-vs-bound
  distinction, lock-poisoning degradation.
- Daemon wire test over authenticated RouterInfo: the five keys return exact
  integers under a qualified IPv4 snapshot, an IPv6-unbound profile, and an
  empty-NetDB profile.
- Negative row: with no published transport sample the five keys still fail
  the whole request closed, naming the field and Plan 339 — the existing
  fail-closed behavior must not be weakened.
- Teeth verification: inverting the status or error mapping must fail the new
  rows. Restored afterward with an empty diff.
- Update the existing `proposal_unavailable_sources_fail_closed_over_wire` and
  the source-matrix contract test for the three now-published rows.

## Verification

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport network_condition -- --test-threads=1
cargo test --locked -p i2pr-runtime network_condition -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection network_condition -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-i2pcontrol-acceptance-evidence.sh
python3 scripts/check-global-plan-number-uniqueness.py
```

Plus the full `AGENTS.md` routine floor before handoff.

## Acceptance and stop conditions

Closes when all five selectors have a real bounded owner, the enumeration
matches the pinned i2pd definitions exactly, no unobserved condition is
fabricated, the negative fail-closed row still holds, and teeth are verified.

Stop and record as blocked if any of these would be required: a fabricated
value to obtain a green matrix; enabling transit participation; a new
dependency; a new unbounded structure; an advertisement change; weakening an
existing fail-closed guard.

## Closure evidence required

Requirement-to-evidence matrix; exact commands with outcomes labelled local vs
CI; explicit statement of which i2pd codes i2pr never emits and why; a
`specs/references/` record of the vocabulary authority and emission policy; an
unblock audit; roadmap and registry updates.

## Related decision recorded here

Plan 327's `clearnet target with provider succeeds` evidence row will use a
**self-composed in-tree loopback outproxy fixture**, labelled loopback evidence
and not interoperability. Decided 2026-10-05 so it is not re-litigated when
Plan 327 is reopened; Plan 327 itself is untouched by this plan.

## Current implementation progress

- `i2pr_transport::network_status` carries both adopted enumerations
  (`NetworkStatusCode` 0-5, `NetworkErrorCode` 0-5) with bounded `as_i64` and
  `try_from_i64`, plus `FamilyNetworkCondition` and the three pure derivations
  and `effective_reachability`. 13 unit rows cover the enumeration round trip,
  out-of-range rejection, the full `ReachabilityState x family x freshness`
  table, and the never-derived codes. Runtime-neutral, no I/O.
- `Ssu2RuntimeService::network_condition` returns a bounded, privacy-safe
  per-family condition from the service's own reachability tracker and its
  configured-versus-bound socket state. `ServiceSockets` now records the
  *requested* families separately from the *achieved* bound addresses, which is
  what makes "never configured is not a fault" expressible while "configured but
  never bound" is. A poisoned lock returns `None`, never a fabricated condition.
- `proposal_network_condition_value` composes that condition with the attested
  NetDB peer snapshot and is routed from the canonical RouterInfo dispatch.
  Gating is per key: the two `error` rows need an attested NetDB, the status and
  testing rows do not.
- The three source-matrix row groups are now published-gated with the Plan 339
  owner and their own evidence id; the gap census drops from 8 to the 3 transit
  selectors, and the source-matrix contract test asserts that split.
- Normative record:
  [`specs/references/proposal-170-network-status-error-testing.md`](../../../specs/references/proposal-170-network-status-error-testing.md).
- Teeth verified: inverting the status mapping (fabricating `OK`) and removing the
  family match (leaking one family's state onto the other) failed 4 of 13
  transport rows, both runtime rows, and 2 of the 4 wire rows. Source restored
  with an empty diff.
- No new dependency, no capability or advertisement change, no transit
  participation, no new unbounded structure.

