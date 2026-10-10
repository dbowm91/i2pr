# Plan 444 status: blocked — no enforceable configured bandwidth budget

Closure token: `blocked-no-enforced-shared-bandwidth-budget`

Plan: `plans/implementation/floodfill/444-configured-shared-bandwidth-class-and-java-selection-control.md`

## Source audit completed

The exact-pinned Java I2P 2.13.0 source checkout was verified at
`9134f808337b401e8e53c73734c81fab04280c9d`. `Router.getBandwidthClass()`
uses the smaller configured inbound/outbound rate in integer KB/s, multiplies
by `getSharePercentage()`, and truncates to integer KB/s. The share default is
80%; values above 1 are treated as percentages, while values at or below 1 are
fractions. The class branches are `<12 K`, `<=48 L`, `<=64 M`, `<=128 N`,
`<=256 O`, `<=2000 P`, otherwise `X`. Java emits `L` instead when hidden or
when its maximum tunnel count is below 20. The inspected source is retained in
the pinned local checkout under
`target/interop/m6-java-sources/i2p.i2p-9134f808337b401e8e53c73734c81fab04280c9d/`.

The I2P Network Database documentation describes shared bandwidth as share
percentage multiplied by the lower inbound/outbound limit and gives `O` as
128–256 KB/s. See the [official Network Database documentation](https://www.i2p.net/en/docs/overview/network-database/).

## Stop condition

The current workspace has no operator-configured global ingress/egress rate
budget and no shared byte-rate shaper enforcing one. Existing bounded queues
and transit admission counts constrain memory and concurrency, not throughput.
Adding `O` from a class mapper, measured idle traffic, host link speed, or a
test-only RouterInfo would therefore exceed the enforceable configured share.
Plan 306's Java result remains authoritative: its sole controlled floodfill
was listed and parsed with tier `Unknown`, but Java did not select it. The
previous Plan 306 Java attempt budget is not reused or extended here.

No source capability, configuration, RouterInfo caps, or reference behavior
was changed. No Java selection attempt was made, and no stock reference was
started. This is a planning stop before class publication, not a protocol
failure.

The bounded resume work is to define and implement normalized operator ingress
and egress ceilings plus a share fraction, enforce the same limits at the
transport and transit admission owners, and derive the class from that
reservation with checked arithmetic. Only then can the plan's class boundary
tests and the three pinned Java controls (healthy bootstrap, ineligible
candidate rejection, eligible candidate selection) run. Plan 437 retains
normal floodfill eligibility; Plans 433 and 436 remain prerequisites to that
role.

Plan 444 is blocked. Plan 306 remains stopped, normal `caps=f` remains
unadvertised, and `specs/support.toml` and conformance claims are unchanged.
