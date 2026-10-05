# Proposal 170 per-family network condition codes — Plan 339 normative record

Status: **frozen 2026-10-05** (Plan 339).

This document records the vocabulary and the emission policy for the five
canonical Proposal 170 `RouterInfo` selectors that describe per-address-family
network condition. It is written so that a later change cannot quietly invent an
integer, silently widen a claim, or quietly narrow an honest `Unknown` into a
fabricated `OK`.

## 1. The pinned source

| Item | Value |
|---|---|
| Document | I2P Proposal 170, "I2PControl Expansion", author Nick2k4 |
| Status | Open |
| Created / last updated | 2026-05-20 / 2026-05-20 |
| Retrieved | 2026-10-05, read-only |
| Source form | `https://i2p.net/proposals/170-i2pcontrol-expansion.txt` (19 010 bytes) |
| SHA-256 | `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc` |

The bytes were re-retrieved read-only for Plan 339 and the SHA-256 recomputed
locally. It matches the pin in
[`docs/provenance/proposal-170-manifest.md`](../../docs/provenance/proposal-170-manifest.md)
and the value Plan 334 recorded, so the Proposal text is unchanged across all
three reads.

The five selectors, verbatim from the Proposal:

```text
- `i2p.router.net.status.v6` - returns IPv6 network status code. *(adopted from i2pd)* Return Type - `int`
- `i2p.router.net.error` - returns IPv4 network error code. *(adopted from i2pd)* Return Type - `int`
- `i2p.router.net.error.v6` - returns IPv6 network error code. *(adopted from i2pd)* Return Type - `int`
- `i2p.router.net.testing` - returns whether IPv4 network is in testing state (0 or 1). *(adopted from i2pd)* Return Type - `int`
- `i2p.router.net.testing.v6` - returns whether IPv6 network is in testing state (0 or 1). *(adopted from i2pd)* Return Type - `int`
```

Two things follow directly from that text and are not judgement calls:

1. All five are **`int`**, including the two `testing` rows.
2. All five are **"adopted from i2pd"** — the Proposal defers the *meaning* of
   the integers, not merely their type. i2pr therefore does not get to choose an
   enumeration.

## 2. The adopted enumeration

Pinned to i2pd `2c694149fa6996eaeb23e378d5f83c9d3232c22f`, at
`libi2pd/RouterContext.h:44-72`, where `daemon/I2PControlHandlers.cpp:41-46`
registers exactly these five keys and `:208-233` projects the values as `(int)`:

```text
RouterStatus: 0 OK, 1 Firewalled, 2 Unknown, 3 Proxy, 4 Mesh, 5 Stan
RouterError:  0 None, 1 ClockSkew, 2 Offline, 3 SymmetricNAT,
              4 FullConeNAT, 5 NoDescriptors
testing:      0 or 1
```

i2pd is a **readable ambiguity reference only**. Nothing is vendored, patched,
or reused, and no i2pd behavior beyond these two enumerations is claimed.

i2pr implements both enumerations in `i2pr_transport::network_status` with
bounded `as_i64` / `try_from_i64` accessors. A value outside `0..=5` is a decode
error, never clamped into range: a malformed source must not be widened into a
valid claim.

## 3. i2pr's emission policy

The vocabulary is i2pd's. The **conditions under which i2pr emits a code** are
i2pr's own, deliberately conservative, and recorded here.

### 3.1 Status

i2pr's reachability tracker qualifies for exactly **one** address family at a
time. A snapshot qualified for IPv4 is evidence about IPv4 and is silent about
IPv6, so:

```text
effective state for family F = snapshot.state  if snapshot.family == F and not expired
                              = Unknown        otherwise (foreign family, stale, or absent)

Reachable                        -> 0 OK
Firewalled | Unreachable         -> 1 Firewalled
Unknown | ObservedUnconfirmed
  | CandidateReachable           -> 2 Unknown
3 Proxy, 4 Mesh, 5 Stan          -> never emitted
```

`Proxy` is doubly excluded: i2pr has no proxy posture, and reporting one would
contradict the project's no-direct-clearnet rule. `Mesh` and `Stan` describe
deployment shapes i2pr does not implement.

The expired case mirrors the publication path exactly: `expires_at <= now`
supports no claim.

### 3.2 Error

A code is emitted only where i2pr owns a real detector.

| Code | i2pr policy |
|---|---|
| `5 NoDescriptors` | Emitted when the **attested** NetDB peer snapshot is empty. A router-global condition, reported on both family selectors, matching i2pd's per-family `m_Error`/`m_ErrorV6` structure. |
| `2 Offline` | Emitted only when a family was **configured but never bound**, i.e. its transport is not running. A family that was never requested is not a fault. |
| `0 None` | Otherwise. |
| `1 ClockSkew` | **Never emitted.** i2pr has no clock-skew detector; `router.clockskew` is still a neutral constant, so claiming skew would be fabrication. |
| `3 SymmetricNAT` | **Never emitted.** No NAT-type detection. |
| `4 FullConeNat` | **Never emitted.** No NAT-type detection. |

The variants exist so the wire enumeration is complete and a decoded value
round-trips. Their presence is not a capability claim.

### 3.3 Testing

```text
ObservedUnconfirmed | CandidateReachable -> 1
Reachable | Firewalled | Unreachable | Unknown -> 0
```

`1` means a determination is genuinely in progress: evidence exists and
confirmation is pending. A router that has **never observed anything** is not
testing and reports `0`; claiming otherwise would be a fabricated in-flight
test.

## 4. The honest baseline

An ordinary production profile that has bound IPv4 and never run a peer test
reports:

```text
i2p.router.net.status.v6  = 2   (Unknown — no IPv6 claim is supported)
i2p.router.net.error      = 0   (None, with an attested populated NetDB)
i2p.router.net.error.v6   = 0   (None)
i2p.router.net.testing    = 0
i2p.router.net.testing.v6 = 0
```

`error` additionally becomes `5 NoDescriptors` on a router that has not
reseeded, and fails the request closed — naming the field and Plan 339 — when
the NetDB is **unattested**, because "no descriptors" is a claim about the
NetDB and an absent observation is not evidence of an empty one.

## 5. Availability and gating

All five rows are **published-gated** on two real owners:

- the SSU2 runtime's bounded per-family network condition (configured vs bound
  socket state plus its own reachability snapshot), and
- for the two `error` rows only, the attested NetDB peer snapshot.

Gating is per key. With no transport owner registered, all five fail closed; with
a transport owner but no attested NetDB, only the two `error` rows fail closed
and `status.v6` / `testing.v6` still answer. No row infers its value from the
requested selector, and there is no request-time whole-router scan.

## 6. What this does not cover

The three remaining Plan 322 gaps are the **transit** selectors
(`i2p.router.net.total.transit.bytes`, `i2p.router.net.bw.transit.15s`,
`i2p.router.net.tunnels.shareratio`). They are not per-family conditions and are
not addressed here. `TransitBuildService` and its bounded counters already exist;
production profiles never construct the service, so closing them is a
**transit-participation posture change** gated by M11 qualification and the
constrained-host lane. They remain explicit fail-closed rows owned by Plan 322.

## 7. Evidence

| Property | Evidence |
|---|---|
| Enumeration matches the pinned i2pd definitions exactly | `enumeration_matches_the_pinned_i2pd_definitions`, `adopted_enumerations_round_trip_exactly`, `out_of_range_wire_values_are_rejected_not_clamped` |
| A foreign-family or stale snapshot never yields `OK`/`Firewalled` | `effective_reachability_never_inherits_another_family`, `effective_reachability_rejects_stale_and_absent_snapshots`, `status_never_leaves_unknown_without_corroborated_evidence` |
| Undetectable codes are never derived | `proxy_mesh_and_stan_are_never_derived`, `undetectable_error_codes_are_never_derived` |
| A never-observed router is not "testing" | `testing_flag_is_one_only_while_a_determination_is_pending`, `an_untested_router_is_not_reporting_itself_as_testing` |
| Runtime plumbing: configured vs bound per family | `network_condition_reports_configured_and_bound_per_family`, `network_condition_on_an_unstarted_service_makes_no_claim` |
| Exact integers on the authenticated wire | `proposal_per_family_network_condition_over_wire` |
| The error rows read a real owner | `proposal_per_family_error_tracks_the_attested_netdb` |
| Fail-closed behavior preserved, per key | `proposal_per_family_error_fails_closed_without_an_attested_netdb`, `proposal_per_family_condition_fails_closed_without_a_transport_owner` |
| Row ownership, not a gap | `plan322_source_matrix_covers_all_canonical_additions_and_marks_gaps` |

Teeth were verified by inverting the status mapping (fabricating `OK`) and by
removing the family match (leaking one family's state onto the other): 4 of 13
transport rows, both runtime rows, and 2 of the 4 wire rows failed. The source
was restored with an empty diff.
