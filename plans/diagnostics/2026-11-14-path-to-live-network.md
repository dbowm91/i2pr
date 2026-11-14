# Path to a live-network i2pr router — status and plan

**Date:** 2026-11-14. **Scope:** what is demonstrated, what is missing, and the
order in which the missing pieces have to be closed before this router can join
the public I2P network.

**Method.** Every claim below is either a measurement on the three-host private
LAN (`192.168.182.20` floodfill, `192.168.182.143`, `192.168.182.8`) or a
source-level fact located by path and symbol. Nothing here is a projection from
"the LAN works" to "the network will work"; where that inference is not
available, the item is listed as missing.

## 1. What is demonstrated

Measured on the reporting builds (`d4647de1`, `703dc659`, `be77cfd4`,
`d7682c84`) over roughly nine hours of cumulative run time:

| Capability | Evidence |
| --- | --- |
| Real SSU2 sessions between independent processes over UDP | established dials and inbound promotions on all three hosts; 243+ I2NP messages emitted on the floodfill host in one run |
| The router's own peer test | `.20` **39/39** probes completed, RTT 3–43 ms; `.8` 38/38; local 38/40; the floodfill answered **38** peers' probes |
| Reachability corroboration reaching `Reachable` | `reachability_state_changed … to=Reachable supporting_classes=3`, then `=4` |
| Path re-validation on a live session (Plan 549) | `path_revalidation_completed path_revalidations=1…6` on the 10-minute cadence, class count 3 → 4, `path_migrations` 0 |
| Floodfill qualification entering the role | `floodfill qualification activated role="Active"` |
| NTCP2 authenticated links | `NTCP2 authenticated inbound/outbound link promoted` with handshake stage receipts |
| The daemon refuses to publish without evidence | `router RouterInfo publication outcome=RefusedNoTarget`, never `Published` |

## 2. What is missing for live operation, in dependency order

Each item is a **blocker**, not a polish task. The order matters: an earlier
item makes a later one measurable.

### G1 — the router cannot look anything up

The production daemon runs **no RouterInfo-kind NetDB lookup**
(`LookupKind::RouterInfo` / `RouterInfoLookup` appear nowhere in
`crates/i2pr-daemon/src/lib.rs`). Peers are learned only from unsolicited
`DatabaseStore` and from whatever the operator seeded by hand. On a live
network that means the routing table can only grow by accident.

*Consequence:* no peer discovery, no floodfill lookups, no lease resolution
beyond what is pushed at us.

### G2 — a publication cannot find a target

`publication_targets_excluding` → `select_version_gated_floodfills` admits only
records that advertise floodfill, and in a network with no known floodfill every
cycle ends `RefusedNoTarget` (measured: 3 cycles per host, 0 published). On the
live network this is **not** a blocker in the same way — real floodfills exist —
but the out-of-band export path (`export_local_router_info`) is written at
startup, before the floodfill gate runs, so a *new* floodfill cannot announce
itself either. Plan 548 owns the isolated-network case; the live case depends on
G1.

### G3 — the floodfill role hovers at the corroboration threshold

`Reachable` needs three classes. A stable session supplies two on its own
(bind, peer test) and Plan 549 added the third (path re-validation), which
measures as four when class 1 is also fresh. But class 1
(`AuthenticatedPeerObservedExternalAddress`) refreshes **only when a peer sends
an SSU2 Address block**, which in ordinary operation happens only during a
handshake. So the count decays back to exactly three, and the measured role
withdrew at boot+30 min — **with the reachability state unchanged**, so the
withdrawal is a readiness decision, not a lapse. The categorical reasons are
now logged (`d2f2a567`) and are the next data point rather than a hypothesis.

**Measured cause, 2026-11-14 04:47–04:53.** `d2f2a567` added the categorical
reasons to the withdrawal line, and the first flap it caught was:

```text
04:47:45  withdrew  reasons=[AddressUnqualified, ReachabilityUnconfirmed]
04:52:15  withdrew  reasons=[AddressUnqualified, ReachabilityUnconfirmed]
04:52:15  Reachable -> Reachable  classes=3  evidence_expires_in_millis=59949
04:53:15  withdrew  reasons=[AddressUnqualified, ReachabilityUnconfirmed]
```

Both reasons are computed from `readiness.material.is_some_and(...)`
(`crates/i2pr-daemon/src/floodfill.rs:1048`), so their **co-occurrence means the
publication material was absent** at the sampling instant — while the
reachability state was `Reachable` with three classes 18 ms later. The material
is built by `publication_material`, which polls expiry and then refuses when
`reachability != Reachable || expiry <= now`. The only variant consistent with
both facts is `ReachabilityUnqualified` at the instant a supporting class
expired *inside* that call: the state dips to two classes, the gate withdraws,
and the next refresh (the re-validation completed 17 ms later) restores it — the
`04:52:15` transition line reports the net result and **cannot see the dip**,
because the dip happens inside the gate rather than on the maintenance pass.

That is why the role flaps on a 30 s–4 min beat and not on the 30-minute TTL: it
is sampling a threshold it sits exactly on. The 59 949 ms remaining in that log
line is the same signal that withdrew the role 60 seconds later.

*Durable fix:* give class 1 a real producer. The reference does it with the
**SSU2 in-band peer test** (block type 13, messages 4–7, Address block in
messages 6/7), which i2pr does not implement — its peer test is an I2NP
`TunnelTest` round trip. That is named in Plan 549 §5.2 as a successor. Running
at four classes instead of three converts every boundary crossing from a
withdrawal into a non-event.

### G4 — the router does not reseed

`reseed.enabled` defaults **false** and `sources` defaults **empty**
(`crates/i2pr-daemon/src/config.rs`). The machinery exists — `Bootstrap`, the
reseed pipeline, SU3 trust-set construction (`build_trust_set`),
`max_sources`, `max_su3_bytes` — but a live router with no sources stays at
`state=reseed-required`, exactly as the LAN logs show
(`reseed_attempts=0`).

*Needed:* operator-configured reseed sources with pinned SU3 trust anchors, and
a decision on whether shipping default sources is acceptable for this project
(that is a trust decision, not a coding task).

### G5 / G6 — nothing is advertised

`specs/support.toml` carries **141 surfaces, 0 advertised**;
`ssu2.ssu2-only-reachability` and `ssu2.path-revalidation` are `experimental`;
NTCP2 is experimental and non-advertised (`AGENTS.md`). `router.version` stays
`0.9.62`, which peers will use to decide whether to talk to us at all.

*Consequence:* even a router that joins the DHT would be selected against, and
advertising capabilities the inventory does not claim is forbidden by this
repo's own rules. Advertisement is therefore a deliberate gate after G1–G4, not
before.

### G7 — public reachability (NAT) is not closed

The LAB hosts are directly reachable on a private LAN. `publish_bound_address`
is the operator's assertion that the host is directly reachable; the peer test
can now **contradict** it, which is what makes the assertion safe — but the
opposite case, a host behind NAT that must learn its external address and use
introducers, needs the SSU2 relay **requester** side wired into the live path.
The introducer side exists; the requester's reachability indication exists as a
recorder (`note_relay_firewalled`) whose producer is a successor's subject.

### G8 — floodfill serving is qualified only in lanes

`scripts/check-floodfill-type5-serve.sh` and the m12 lanes cover the type-5
serve path, but nothing has been measured against a **stock** peer. That is
Plan 493's question and it is blocked on an authorized external peer.

### G9 — there is no hostile-network posture

The LAN has three cooperative peers. A live network has scanners, replay
senders, token harvesters and resource exhaustion. Bounded queues, drop
categories and token accounting exist and are counted, but no one has measured
this router under adversarial load, and `specs/QUALIFICATION-GATES.md` §3
(ban surface) has never been exercised against a real network.

### G10 — external qualification is unavailable, not skipped

16 plans are `blocked`, most on an authorized host pair or a public-network
presence (455, 457–458, 462–466, 470–474, 476, 493–494). This is an environment
constraint, not a code gap, but it caps how far the "functional on the live
network" claim can be evidenced from inside this repository today.

## 3. Recommended sequence

| Step | Work | Why it is next |
| --- | --- | --- |
| 1 | Finish Plan 549: read the withdrawal `reasons`, decide whether the role fix is "more classes" or "readiness input" | The cause is now observable for the first time; guessing again costs 40-minute cycles |
| 2 | **Class 1 producer** — the SSU2 in-band peer test (msgs 4–7, Address block in 6/7) | Stops the router running at exactly the corroboration threshold, and is the reference's own arrangement. This also replaces the I2NP `TunnelTest` peer test with the protocol-correct one |
| 3 | **NetDB RouterInfo lookup** (G1) | Without it the router is not a network participant; it is a listener that learns by accident |
| 4 | **Reseed with pinned trust anchors** (G4) | Gives the router its first real peers; without it G1 has nothing to look up |
| 5 | Publication target from a real floodfill (G2), then T4 | The first end-to-end proof that the network holds our record |
| 6 | NAT/relay requester (G7), then advertisement decisions (G5/G6) | Only once publishing and lookup work is advertising safe or useful |
| 7 | Hostile-load and stock-peer qualification (G8/G9) | Blocked on environment; attempt only with ban-surface care |

Steps 2–4 are the substance. Everything the LAN proved is transport and
evidence machinery; **nothing in the LAN work exercised discovery, because
there was nothing to discover.**

## 4. Constraints that still bind

- No capability, version or RouterInfo claim beyond the tested subset; the
  inventory's advertised count stays zero until a gate is closed with evidence.
- No public-network routine traffic, no root, no namespaces, no containers.
- No weakening of a boundary script to make a step pass — the m12 glob gap found
  on 2026-11-13 is fixed by its own plan, not by widening the exclusion.
- Reference pins are frozen: i2pd `2.61.0` (`635b013a…`), Java I2P `2.13.0`
  (`9134f808…`).

## 5. What is explicitly not claimed

That the router can join the public network; that it can serve as a floodfill;
that its published record is reachable from the Internet; that it survives a
hostile peer; or that any of G1–G9 is closer than the table says.
