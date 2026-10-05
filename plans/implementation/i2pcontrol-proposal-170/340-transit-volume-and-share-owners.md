# Plan 340 — Transit volume, transit bandwidth, and tunnel share owners

Status: **passed-transit-volume-owners-with-participation-posture-unchanged**

Closure record:
[`plans/closure/i2pcontrol-proposal-170/340-status.md`](../../closure/i2pcontrol-proposal-170/340-status.md)

## Current implementation progress

Completed and closed on 2026-10-05. All work packages A-E landed; the canonical
RouterInfo gap census is zero. Two defects in this plan's own code were found and
fixed at the source before closure: the window ring stored epochs modulo the ring
size (so reused slots accumulated), and the window's writer and reader used two
different clock bases (so a live window could only read empty). Teeth were
verified at three layers by five inversions. See the closure record for the
requirement matrix, the teeth table, and the two-defect account.

The one claim that did **not** change: transit participation stays disabled, and
the honest product baseline is `0` / `0` / `0.0`.

Classification: capability + source-ownership completion (reopen of Plan 322, Group A).

Hard dependencies: Plan 322 closed blocked; Plan 268 (M11 one-family experimental
qualification) passed with the 2026-10-05 addendum; Plan 339 passed.

Subsystem: `i2pcontrol-proposal-170`.

## Objective

Give Plan 322 Group A's three remaining unavailable canonical RouterInfo selectors truthful
production owners:

| Selector | Proposal meaning | Declared type |
|---|---|---|
| `i2p.router.net.total.transit.bytes` | total transit bytes forwarded since startup | `long` |
| `i2p.router.net.bw.transit.15s` | 15-second average transit bandwidth (bytes/sec) | `long` |
| `i2p.router.net.tunnels.shareratio` | the tunnel share ratio | `double` |

The first two are *"(adopted from i2pd)"* in Proposal 170. The third is **not**, and no
pinned reference implementation provides it; see "Share-ratio definition" below.

## The posture decision this plan does *not* take

`crates/i2pr-daemon/src/transit_compose.rs` and `transit_owner.rs` both document, and
`scripts/check-m11-transit-boundaries.sh` rule 10 enforces, that ordinary product profiles
never construct a transit data-plane owner: production `lib.rs` consults
`controlled_transit_disabled_probe` so the live-owner module has a production caller without
dispatching. A production i2pr router therefore **relays no transit traffic**.

This plan adds the owners those selectors need. It does **not** enable transit
participation, and it does not change any M11, SSU2, RouterInfo, or advertisement behavior.
Enabling participation is a separate product-posture decision that requires M11
re-qualification on the current tree — see "Why participation stays disabled" below.

The consequence is stated up front so no reader mistakes the closure for a capability claim:
**the honest production baseline for all three selectors is `0`, `0`, and `0.0`.** Those are
not placeholders. They are what a router which participates in no tunnels truthfully reports.

## Why participation stays disabled

Plan 268 passed M11 as a **one-family experimental** qualification and Plan 269 kept public
transit disabled, non-advertised, and unclaimed. The 2026-10-05 addendum to
[`268-status.md`](../../closure/transit-tunnels/268-status.md) records that the lane's
zero-production-diff guard was fail open, so the retained evidence is demonstrably bound to
qualification SHA `6ab9dc2d` and **not** to the current tree (151 crate production sources
have changed since). A posture change justified by M11 cannot be justified by evidence that
is not provably attached to the tree being changed. Enabling participation stays out of scope
until a re-qualification run binds the current tree.

## Ownership model

One production object, always installed, published by the composition root next to
`ControlMetrics`:

- **`TransitParticipation::Disabled`** — the enforced product posture. It holds **no
  counters at all**, so it is structurally incapable of reporting a non-zero value. Its
  snapshot reports `participating: false` with zero volume because that is what the variant
  means, not because a default was returned.
- **`TransitParticipation::Enabled(Arc<Mutex<TransitVolumeCounters>>)`** — the controlled
  lane only. The counters are the ones the real `TunnelData` forward path advances, so a
  participating router reports measured volume. A poisoned lock reads as **unavailable**
  (fail closed), never as zero.

`TransitVolumeCounters` is cumulative relay bytes plus a bounded trailing-window ring. It is
advanced in exactly one place: the `Ok(TransitDataOutcome::Forward { .. })` arm of
`TransitBuildService::route_tunnel_data`. The OBEP `Deliver` arm is deliberately **not**
counted: an inbound endpoint decrypting a cell destined for this router is not relaying on
another router's behalf.

Per-cell accounting uses `TUNNEL_DATA_PAYLOAD_SIZE + 4`, matching the pinned i2pd
`TUNNEL_DATA_MSG_SIZE = 1028` (`libi2pd/TunnelBase.h:29`) that i2pd adds in
`TransitTunnel::EncryptTunnelMsg` (`libi2pd/TransitTunnel.cpp:43`). As in i2pd, the I2NP
message header is outside the counted body.

## Trailing-window arithmetic

`i2p.router.net.bw.transit.15s` is a **trailing 15-second mean in bytes/second, computed at
request time** — deliberately not i2pd's 1 Hz timer sample, because adding a timer task is
not justified by a diagnostic selector. The window is a fixed 16-slot ring of
`(epoch_second, bytes)` pairs; a slot is counted only when its recorded epoch is one of the
15 seconds preceding the read. The ring is one slot longer than the window precisely so
every in-window second has a distinct slot, which makes the read side **read-only** — no
lock, no pruning pass, and two reads with no intervening forward agree.

The value is `bytes_in_window / 15`, floored. i2pd instead divides a byte delta between two
ring samples by their real elapsed milliseconds; the two agree for a steady rate and differ
only in the first second after a change of rate. This difference is recorded, not papered
over.

## Share-ratio definition

Proposal 170 says only "returns the tunnel share ratio" and, unlike its neighbours, does
**not** mark it *"(adopted from i2pd)"*. i2pd's handler map has no entry for it, and the
pinned Java I2P freeze provides no implementation either. The arithmetic is therefore
genuinely unspecified, and i2pr must not present a guess as an interoperability fact.

i2pr defines it, boundedly and measurably, as:

```text
tunnels.shareratio = transit bytes forwarded / cumulative bytes sent
```

clamped to `1.0`, with two fail-closed rules:

- **`Disabled` participation reports `0.0`** unconditionally. A router that relays nothing
  shares none of its bandwidth, and that is true regardless of the denominator.
- **`Enabled` participation requires the attested cumulative sent total.** Without an
  authoritative denominator a ratio would be a guess, so an unattested `ControlMetrics` is a
  gap, not a `0.0`. A sent total of `0` yields `0.0` (nothing sent, nothing relayed).

A client that needs the router's *configured* bandwidth-share percentage must read the
RouterInfo `share` option; this selector is the observed participation share and is labelled
as such everywhere it is published.

## Invariants

1. No `Disabled` owner can report non-zero volume; the counters do not exist in that state.
2. Volume is advanced only by a real `Forward` dispatch, never by a `Deliver`, a `Drop`, or a
   rejected build.
3. A poisoned lock, a missing owner, or an unattested sent total fails closed to an
   `InspectionGap`. It never degrades to `0`.
4. Out-of-range or non-finite ratios are rejected or bounded, never projected as an
   unbounded claim.
5. No secret, key, payload, peer identity, endpoint, or address crosses this surface — the
   snapshot is three bounded numbers and one boolean.
6. The window ring is fixed-size. No unbounded container, no new queue, no new task, no new
   timer, no new lock beyond the one shared counter handle the cross-thread handoff needs.
7. Transit participation, M11 state, and every advertisement are unchanged.

## In scope

- New runtime-neutral-shaped daemon module owning the counters, the ring, and the posture.
- Volume accounting in the existing forward path.
- Publication from the composition root; projection of the three selectors; canonical
  RouterInfo dispatch.
- Source-matrix rows, contract census, wire tests, unit tests.

## Out of scope

- Enabling production transit participation, or any M11/SSU2/NetDB behavior change.
- A new configuration surface (no share-percentage option, no transit enable flag).
- New timers, tasks, queues, or channels.
- The Plan 327 outproxy and outbound-secret work, and the Plan 328 conformance gate.
- Advertising transit capability in `specs/support.toml` or anywhere else.

## Work packages

- **A** — `transit_volume.rs`: `TransitVolumeCounters`, `TransitBandwidthWindow`,
  `TransitParticipation`, `TransitVolumeSnapshot`, unit rows.
- **B** — `transit_compose.rs`: hold the shared counter handle, advance it on `Forward`,
  expose it for the controlled lane.
- **C** — `InspectionHandles`: publish/read the posture; project the three selectors with
  per-key gating; wire canonical dispatch.
- **D** — source matrix, contract census, wire tests.
- **E** — records: reference dossier, CONFORMANCE section, this plan's closure, the dated
  Plan 322 correction, registry and roadmap.

## Tests

- Unit: window ring epoch handling (fresh, stale, boundary, wrap, read-only agreement);
  saturation; disabled/enabled snapshot; share-ratio arithmetic including the clamp, the
  zero-sent case, and the missing-denominator gap.
- Wire: the three selectors over a real I2PControl `RouterInfo` request against a disabled
  owner (`0`, `0`, `0.0`); against an enabled owner with recorded volume (non-zero, matching
  the recorded counters); the share ratio failing closed without an attested sent total; and
  failing closed with no transit owner published at all.
- Teeth: every row above must be shown to fail when the fix is disabled — hard-coding the
  projection to zero, and removing the `Forward`-only accounting. Recorded in the closure.

## Verification

The full routine floor from `AGENTS.md`, plus `check-i2pcontrol-acceptance-evidence.sh`,
`check-m11-transit-boundaries.sh`, `check-m11-transit-qualification-evidence.sh`,
`check-runtime-boundaries.sh`, `check-dependency-direction.sh`, and
`check-constrained-host-lane-boundary.sh`.

## Acceptance

1. The three selectors have production owners and the Plan 322 gap census reaches zero.
2. Fail-closed behavior is preserved for every unavailable combination.
3. Transit participation remains disabled and unadvertised, with a production diff that
   touches no M11 or transport behavior.
4. Teeth verified at every layer, with sources restored and an empty diff.
5. Records state the honest `0 / 0 / 0.0` baseline and the share-ratio caveat.

## Stop conditions

Stop and re-audit if the projection would require a new timer, task, queue, unbounded
container, a production transit enable path, or any change to an existing wire field.
