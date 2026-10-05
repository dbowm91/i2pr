# Proposal 170 transit volume, bandwidth, and share — Plan 340 normative record

Status: **frozen 2026-10-05** (Plan 340).

This document records the vocabulary, the derivation, and the honest baseline for
the last three canonical Proposal 170 `RouterInfo` selectors that Plan 322 left
without owners. It exists so that a later change cannot turn a posture into an
advertisement, cannot turn a missing denominator into a `0.0`, and cannot quietly
replace the trailing-window rate with a timer that was never justified.

## 1. The pinned source

| Item | Value |
|---|---|
| Document | I2P Proposal 170, "I2PControl Expansion", author Nick2k4 |
| Status | Open |
| Created / last updated | 2026-05-20 / 2026-05-20 |
| Pin | `docs/provenance/proposal-170-manifest.md` |
| SHA-256 | `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc` |

The three selectors, verbatim from the Proposal:

```text
- `i2p.router.net.total.transit.bytes` - returns total transit bytes forwarded
  since startup. *(adopted from i2pd)* Return Type - `long`
- `i2p.router.net.bw.transit.15s` - returns 15-second average transit bandwidth
  (bytes/sec). *(adopted from i2pd)* Return Type - `long`
- `i2p.router.net.tunnels.shareratio` - returns the tunnel share ratio.
  Return Type - `double`
```

`long` and `double` are the Proposal's own words; the wire types are `Integer`
and `Double` in `crates/i2pr-i2pcontrol/src/proposal_wire.rs`.

## 2. What the pinned reference actually does

`i2pd` at the pinned `2c69414` supplies the first two, and nothing for the third:

| Selector | i2pd handler | Source |
|---|---|---|
| `net.total.transit.bytes` | `NetTotalTransitBytes` | `i2p::transport::transports.GetTotalTransitTransmittedBytes()` |
| `net.bw.transit.15s` | `TransitBandwidth15S` | `i2p::transport::transports.GetTransitBandwidth15s()` |
| `net.tunnels.shareratio` | **absent** | not in `m_RouterInfoHandlers` |

`GetTotalTransitTransmittedBytes` is incremented in
`TransitTunnel::EncryptTunnelMsg` (`libi2pd/TransitTunnel.cpp:43`) by
`TUNNEL_DATA_MSG_SIZE`, which `libi2pd/TunnelBase.h:29` defines as `1028`.

Two consequences are load-bearing and are mirrored exactly:

1. **A relayed cell is 1028 accounted bytes.** i2pr uses
   `TUNNEL_DATA_PAYLOAD_SIZE + 4` — the 1024-byte payload plus the I2NP
   tunnel-data size field — and excludes the I2NP message header, because i2pd
   excludes it too.
2. **Only a relay counts.** The increment sits on the participant/IBGW encrypt.
   An OBEP endpoint decrypting a cell addressed to this router never reaches it,
   so a terminating delivery is not transit volume. i2pr counts only the
   `Ok(TransitDataOutcome::Forward { .. })` arm for the same reason.

### The 15-second figure is not the same computation

i2pd keeps a ring of per-second `TrafficSample`s filled by a one-hertz timer and
reports `(sample1.total - sample2.total) * 1000 / delta`, where `delta` is the
real elapsed milliseconds between the newest sample and the sample fifteen
positions back (`libi2pd/Transports.cpp:420-433`).

i2pr computes a **trailing 15-second mean at request time** instead: the bytes
recorded in the fifteen seconds preceding the read, divided by fifteen and
floored. This plan does not add a timer, because a diagnostic selector does not
justify a new periodic task. The two agree at a steady rate and differ only in
the first second after a change of rate. The difference is recorded here rather
than papered over.

## 3. `i2p.router.net.tunnels.shareratio` is genuinely underspecified

Unlike its two neighbours, this selector is **not** marked *"(adopted from
i2pd)"* in the Proposal, i2pd's handler map has no entry for it, and the pinned
Java I2P freeze provides no implementation either. Its arithmetic is not
recoverable from any authoritative source, so i2pr must not present a guess as
an interoperability fact.

i2pr defines it, boundedly and measurably, as the **observed share of outbound
bytes spent relaying other routers' traffic**:

```text
tunnels.shareratio = transit bytes forwarded / cumulative bytes sent
```

clamped to `1.0`, with two fail-closed rules:

- A router that does not participate reports `0.0` unconditionally. That is
  true independently of the denominator, so it is not a gap.
- A router that *does* participate needs an attested cumulative sent total. An
  unattested `ControlMetrics` is a gap, because a ratio without a denominator
  would be a guess. A sent total of `0` yields `0.0`.

A client that needs the router's *configured* bandwidth-share percentage must
read the RouterInfo `share` option. This selector is the observed participation
share and is labelled as such wherever it is published.

## 4. The posture owner, and why the zeros are not a fabrication

Ordinary product profiles never construct a transit data-plane owner. Production
composition consults `crate::transit_owner::controlled_transit_disabled_probe` so
the live-owner module has a production caller without dispatching, and
`scripts/check-m11-transit-boundaries.sh` rule 10 pins that shape. A production
i2pr router therefore relays nothing.

`TransitParticipation` is the owner these selectors read, and it is always
installed by the composition root:

| Variant | Counters | Reported volume |
|---|---|---|
| `Disabled` (the product posture) | **none exist** | `0` / `0` / `0.0` |
| `Enabled(Arc<Mutex<TransitVolumeCounters>>)` (controlled lane only) | real, advanced by the forward path | measured |

The disabled variant holds no counters, so it is **structurally incapable** of
reporting a non-zero value. This is the difference between a posture and a
substituted measurement: there is no code path in which a missing observation
becomes a zero, because a disabled router genuinely has relayed zero bytes.

## 5. One clock, and one accounting site

The window is only meaningful if its writer and reader share a time base. They
did not, at first: the forward path inherited the transit ingress clock while the
request path inherited the I2PControl service's **monotonic** `now_ms` (measured
from service construction, `I2pControlServiceState::now_ms`). The two lived in
different epochs, so a live window could only ever read empty. Both sides now
derive their stamp from `crate::transit_volume::wall_seconds()`, which removes
the class of bug rather than the instance.

Volume is advanced at exactly one site: the `Forward` arm of
`TransitBuildService::route_tunnel_data`. A dropped cell, a replay, a rejected
build, and an OBEP delivery all leave the counters untouched.

## 6. The window's one sharp edge

The ring holds 16 slots for a 15-second window, so every second inside the
window has a distinct slot and the read needs no lock and no pruning pass.

Each bucket stores the **absolute** second, not a modulo-16 residue. This is
load-bearing: two seconds one full ring period apart land on the same slot, and
only an absolute comparison can distinguish "the slot rolled to a new second"
from "the same second again". A residue makes the second record accumulate into
the first, and the window over-counts by every recycled period. That defect was
introduced, caught by `cumulative_total_is_independent_of_the_window` (which
read 2604 instead of 1028), and fixed before closure.

## 7. Honest production baseline

| Selector | Product baseline | Why |
|---|---|---|
| `net.total.transit.bytes` | `0` | the product owns no counters because it participates in no tunnels |
| `net.bw.transit.15s` | `0` | no relayed byte in the trailing window |
| `net.tunnels.shareratio` | `0.0` | no bandwidth is shared for transit |

These are the values a real i2pr router reports today. They are not placeholders,
and publishing them advertises the *absence* of transit participation rather
than a capability.

## 8. What this record does not cover

- **Transit participation is not enabled by this record.** Enabling it is a
  separate product-posture decision requiring M11 re-qualification on the current
  tree; see the 2026-10-05 addendum in
  [`plans/closure/transit-tunnels/268-status.md`](../../plans/closure/transit-tunnels/268-status.md).
- No transit capability is advertised in `specs/support.toml`, `README.md`, or
  anywhere else, and none may be on the strength of this record.
- Loopback and destination traffic are outside the transit counters entirely.
- The `TransitBandwidthSummary` transit *admission* budget in `i2pr-tunnel` is a
  different quantity from these selectors and is not their source.

## 9. Evidence

| Requirement | Evidence |
|---|---|
| 1028-byte relay accounting, forward-only | `transit_compose::tests::transit_volume_advances_only_on_a_forward` |
| Window epoch, boundary, wrap, low-clock, read-only agreement, saturation | `transit_volume::tests` (15 rows) |
| Disabled posture owns no counters | `transit_volume::tests::disabled_participation_reports_no_volume_and_no_counters` |
| Share-ratio arithmetic, clamp, zero-sent, missing denominator | `transit_volume::tests::share_ratio_*` |
| Disabled posture over the wire answers `0` / `0` / `0.0` | `proposal_transit_volume_reflects_the_published_posture_over_wire` |
| A participating router reports measured volume and a measured ratio | `proposal_transit_volume_reports_a_participating_router_measured_volume` |
| A participating router without an attested denominator fails closed | `proposal_transit_share_ratio_requires_an_attested_denominator_over_wire` |
| An absent owner is a gap, never a zero | `proposal_unavailable_sources_fail_closed_over_wire` |
| No canonical Proposal 170 RouterInfo addition remains a gap | `plan322_source_matrix_covers_all_canonical_additions_and_marks_gaps` |

Teeth were verified at three layers by inverting each fix and confirming the
corresponding rows fail: hard-coding the projection to the disabled posture
(1 of 4 wire rows), removing the forward-path accounting (1 unit row), letting
the ratio fall back to `0.0` (1 unit + 1 wire row), reverting the ring to
modulo epochs (8 unit rows), and reverting a transit row to `Unavailable` (the
contract census). All sources were restored from backup and re-verified green.
