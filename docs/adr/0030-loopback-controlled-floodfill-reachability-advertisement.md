# ADR 0030: Loopback controlled floodfill reachability advertisement

- Status: Accepted
- Date: 2026-10-03
- Decision owner: repository maintainer (owner-authorized Plan 306)
- Related: ADR 0026, ADR 0027, Plans 279, 303, 306

## Context

Plan 279 stopped at a caps-gated initiation boundary
(`plans/closure/floodfill/279-status.md` §§2–3): the controlled
RouterInfo carries router-level `caps = "f"` only, and stock Java I2P
2.13.0 loads, verifies, and floodfill-lists the record but never
initiates transport to it. Java gates initiation/selection on caps
letters — `R` (reachable) at
`TunnelPeerSelector.allowAsIBGW:301-309`, plus bandwidth-tier
derivation — all of which the Plan 101 posture forbids in controlled
options (`crates/i2pr-netdb/src/local.rs:validate_options`). i2pd is
address-driven and dials the identical publication, so the record is
conformant and dialable; no i2pr wire defect is evidenced.

Plan 306 §3 requires this decision before any lane run: what a
loopback controlled RouterInfo may truthfully advertise so a stock
second family initiates to it.

## Decision

In controlled/loopback qualification scope ONLY, the controlled
floodfill RouterInfo may carry `R` in addition to `f`, iff a
controlled peer-test exchange Confirmed inbound reachability of the
advertised bound address during the same activation.

Concretely:

- `R` is TRUE in lane scope iff `run_controlled_peer_test` returned
  `ControlledPeerTestOutcome::Confirmed` for the exact bound address
  being advertised. `Confirmed` means the reference peer completed
  the SSU2 exchange addressed at our socket and the runtime recorded
  `PeerTestResult{Confirmed}`; that is demonstrated inbound
  acceptance, not inferred reachability.
- Scope is controlled/loopback publication only: a loopback-bound
  address, file-seeded to lane references, reseed disabled, no
  public-network participation. The claim "reachable at this
  address" is true inside the lane because a reference just reached
  it there.
- Bandwidth tiers (`L/M/N/O/P/X`) and every other forbidden letter
  stay forbidden. If Java additionally requires a tier, Plan 306
  STOPS and reports that instead of fabricating one.
- The normal-daemon path is unchanged: `prepare_normal_activation`
  still builds `caps=f`, and it cannot activate on loopback anyway
  (`is_normal_qualified_address` requires a publicly routable
  host). No configuration surface can inject `R`:
  `validate_options` continues to reject `R` in caller-supplied
  options; the `R` is appended only by the permit+proof-gated
  builder.
- Public-network advertisement still requires the standing
  corroboration bar (ADR 0027 §9). This decision authorizes no
  public `R`, no tier, and no claim beyond the lane.

## Why this does not weaken the no-false-advertisement invariant

The invariant forbids advertising what is not demonstrated. The
`R` here is demonstrated twice over: the address is the exact bound
socket the peer test confirmed, and the confirmation came from an
independent reference implementation over real SSU2. The proof
token (`LoopbackReachabilityProof`) can only be minted alongside a
`Confirmed` outcome in the controlled activation owner, and the
boundary script confines the mint site the same way it confines the
advertisement permit. Withdrawal still emits no caps letters at
all, so any eligibility loss withdraws `R` together with `f`.

## Rejected alternatives

### Advertise a bandwidth tier to satisfy Java tier derivation

Rejected. A tier letter would assert measured capacity class without
any measurement behind it — a fabrication, not a demonstration. If
the Java lane proves `R` insufficient, the plan stops.

### Keep `f`-only and change nothing

Rejected for the lane (not for the posture): three frozen-budget
attempts prove Java never initiates to `f`-only, so `f`-only leaves
the second-family gate permanently closed with no further
diagnostic value.

### Inject `R` through configuration or seeded bytes

Rejected. Seeding caps the publication does not carry would
misrepresent it (279-status §9), and config-injected caps would
break the intent-only configuration rule (ADR 0027 §9).

## Review triggers

Revisit through a new accepted ADR if a reference disagrees with
the `fR` record on the wire, Java requires a tier letter, any
public-network advertisement is proposed, the peer-test evidence is
found not to demonstrate inbound acceptance, or the proof mint site
escapes its script-confined owner.

## Review trail

Written under Plan 306 work package 1; reviewed by the repository
maintainer at the Plan 306 closure handoff (acceptance criterion 3:
caps policy reviewed and minimal).
