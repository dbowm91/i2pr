# Two-host private-LAN SSU2 and NTCP2 measurement — 2026-11-12

**Diagnostics record, not a plan of record.** This records what two `i2pr`
routers did on a real two-host private LAN, so a reader does not have to
re-derive it from a chat log. It claims nothing about a stock router or a public
network.

## Purpose

Plans 544 and 545 close the gap that kept a two-host SSU2 topology from
existing at all: the router could not produce a record its peer's cache loader
would accept, and its outbound SSU2 pool drew only from the floodfill
population, which is empty on a private LAN. Both are closed. This record
measures what the result actually does, in **both** directions, on **both**
transports — and records the one tier that is still unreachable, with the
structural reason rather than a guess.

## Topology

| Role | Host | Address | Transports |
| --- | --- | --- | --- |
| A | x86_64, this host | `192.168.182.143` | SSU2 UDP `19100`, NTCP2 TCP `19101` |
| B | aarch64, Ubuntu 22.04.5 (glibc 2.35) | `192.168.182.20` | the same ports |

Both configs are symmetric: `[network] network_id = 2`, SSU2 and NTCP2 with
`qualification_profile = "private_lan_qualification"` bound to the host's own
RFC1918 address, `ssu2.peer_pool = true`, and `[floodfill] enabled = true`.

Each host's data directory holds the other's RouterInfo under
`netdb/routers/<router-hash>`, produced by Plan 544's export from the peer and
copied across out of band. Each host logs
`bootstrap pipeline completed … record_count=1`, so both hold one peer record.

### Builds

| Host | Build | SHA-256 | Tree |
| --- | --- | --- | --- |
| A | `cargo build -p i2pr-daemon --bin i2pr` (debug) | `ec0d270c22440f9cd1b2d9ed27a0dec9ed47c6559992534a8d54d3b3b9fe0aac` | `26e282d9` |
| B | CI `build-i2pr-arm64.yml` (release) | `47a06527ebb83cc9c4250b908c545dc87b0f91a20d77f52305414ee096178fe3` | `26e282d9` |

Both are the same source tree. The debug/release difference matters for
timings and not for behaviour.

## Measured: an SSU2 session in each direction, initiated by each side

Each host ran with the other **down**, so its first pool tick dials and fails;
the other host was then started, and it dialled the first. This separates who
initiated from who accepted, which a simultaneous start would not.

**B initiated, A accepted** — B's own pool log:

```text
20:46:43.435886  ssu2 peer pool tick planned=1 examined=1 refused=0 population_reads=1 dials_started=1
20:46:43.457230  ssu2 peer pool dial outcome outcome=Established
```

**A initiated, B accepted** — A's own pool log:

```text
20:48:56.484451  ssu2 peer pool tick planned=1 examined=1 refused=0 population_reads=1 dials_started=1
20:48:56.537236  ssu2 peer pool dial outcome outcome=Established
```

Both established within 21 and 53 milliseconds of the tick that planned them.
`examined=1 refused=0` means the one candidate the pool found was the peer, and
the version gate admitted it; `planned=1` means the **fallback** population —
not the floodfill population, which is empty here — supplied it.

This is the first record of an `i2pr`-to-`i2pr` SSU2 session **in both
directions**. Plan 544's registration and Plan 545's closure each record one
direction.

## Measured: an NTCP2 session in each direction

NTCP2 needs no fallback and was already bidirectional; it is recorded here so
the pair's state is stated once.

```text
B: 20:46:56.697  controlled NTCP2 inbound  handshake stage="noise_authenticated" io_result="completed"
B: 20:46:58.557  controlled NTCP2 outbound handshake stage="noise_authenticated" io_result="completed"
A: 20:48:58.461  controlled NTCP2 inbound  handshake stage="noise_authenticated" io_result="completed"
A: 20:49:11.531  controlled NTCP2 outbound handshake stage="noise_authenticated" io_result="completed"
```

An `inbound` line on one host pairs with an `outbound` line on the other at the
same instant, so each direction is corroborated by both ends rather than read
off one log.

## Not measured, and why: the correlated two-way reply (T4)

The two hosts now authenticate and carry a session in both directions on both
transports. They do **not** perform a correlated request/reply, and that is
**not** a configuration mistake on this topology.

The only correlated exchange the router has in production is its own
publication: it sends a `DatabaseStore` carrying its RouterInfo and correlates
the peer's inbound `DeliveryStatus` with the attempt (Plan 516). That path
needs a **floodfill target**, and A reports none:

```text
20:40:53.714  bootstrap pipeline completed state=reseed-required record_count=1 floodfill_advertisers=0
20:40:53.796  normal floodfill opted in; evaluating eligibility requested=true role="Disabled"
20:40:53.892  router RouterInfo publication transition="router_info_publication" outcome=RefusedNoTarget
```

`RefusedNoTarget` is the whole explanation: no floodfill, no publication, no
`DeliveryStatus` to correlate. The role is `Disabled` because floodfill
eligibility requires a publicly routable address
(`crates/i2pr-daemon/src/floodfill.rs`, `is_publicly_routable_host`), and that
function rejects `is_private()`. An RFC1918 address can therefore **never** be a
floodfill, on either host.

The other two request/reply probes the tree carries are equally unavailable
here: `floodfill.send_initiator_lookup_probe` and
`floodfill.send_reply_token_probe` require `qualification_profile =
"controlled_loopback"` (`crates/i2pr-daemon/src/config.rs`), and
`controlled_loopback` requires loopback binds, which an RFC1918 bind is not.

**So T4 is blocked by the topology, not by a defect.** Three ways to unblock it,
each with a different owner:

1. **A third host with a routable address** acting as a floodfill. No code
   change; needs an address this lab does not have.
2. **A lab-scoped floodfill eligibility rule** — a qualification profile under
   which a private address may advertise `f` for a two-host run. That is a
   capability and advertisement decision, so it needs its own plan of record;
   `is_publicly_routable_host` is a boundary this record does not touch.
3. **The stock-router lane (Plan 493)**, where a real floodfill exists.

## Reproducing

```text
# On each host: config as above, then
i2pr check-config --config <cfg>
i2pr identity generate --config <cfg>
i2pr run --config <cfg>

# Export on one host, copy to the other's netdb/routers/ under the same name:
#   <data_dir>/export/<router-hash>  ->  <peer data_dir>/netdb/routers/<router-hash>
# Both directories 0700, files 0600. The name must not change: the loader
# derives its expected key from it.

# Then, to separate initiator from accepter, start one host with the other down,
# wait one pool tick (30 s), start the second host, and read each host's own
# `ssu2 peer pool tick` / `dial outcome` lines.
```

## Boundaries

- **`i2pr` to `i2pr` only.** No stock router was involved. i2pd 2.61.0 and Java
  I2P 2.13.0 admission remains Plan 493's question, and the predicates and the
  ban surface for it are pinned in `specs/QUALIFICATION-GATES.md` §3.
- **No public network.** Both addresses are RFC1918, and the SSU2 publication
  path is loopback-clamped by design, so nothing was published anywhere.
- **No advertisement.** `router.version` stays `0.9.62`; the inventory's
  advertised count stays **zero**.
- The log lines above are `i2pr`'s own, copied verbatim minus colour codes. They
  carry no key, seed or payload, and the only addresses they name are the lab's.

## The three-host mesh

A third host joins the pair: **`raspi8`**, `192.168.182.8` — a Raspberry Pi,
aarch64, Ubuntu 24.04.5, glibc **2.39**, 4 cores, 899 MB, which also serves the
network's DNS through pihole and is therefore configured as a low-grade
**client**: no `[floodfill]` block, `private_lan_qualification` on both
transports, `ssu2.peer_pool = true`, nothing else.

The arm64 artifact built on Ubuntu 22.04 (glibc 2.35) runs there unchanged,
which is the glibc direction that has to hold: a binary requiring 2.35 loads on
a host offering 2.39.

Each node was seeded with the other two's exported records (Plan 544's
`export/`), copied into `netdb/routers/<router-hash>` at `0700`/`0600`. All
three then logged `record_count=2`.

Measured, after a staggered start so that each dial has a definite initiator:

```text
A 23:08:17.788  tick planned=2 examined=2 refused=0 population_reads=1 dials_started=2
A 23:08:20.885  dial outcome outcome=Established
A 23:08:20.926  dial outcome outcome=Established
C 23:09:11.126  tick planned=0 examined=20 refused=0 population_reads=10 dials_started=14
B 23:09:15.033  tick planned=0 examined=10 refused=0 population_reads=5 dials_started=4
```

**The converged state is `planned=0` on all three**, which is the pool saying
every peer already holds a session; NTCP2 reached `noise_authenticated` in both
directions on every pair. Three `i2pr` routers on three machines now hold
sessions with each other on both transports.

C started while the other two were down and therefore logged
`dials_started=14` before it converged. That is the retry path working, not a
defect: `PeerPool::record_dial_outcome` clears the peer's `dialing` flag for
**every** outcome including `Failed` (`crates/i2pr-daemon/src/peer_pool.rs`), so
the next tick re-plans the peer. An earlier reading of the same evidence as "a
failed dial is permanent" was **wrong**, and is retracted here: `planned=0` with
a grown `dials_started` means *already connected*, not *excluded*.

## The floodfill this topology still does not have

All three nodes report `floodfill_advertisers=0` and, with
`[floodfill] enabled = true`, `role="Disabled"`.

Making `192.168.182.20` a floodfill is **not** a configuration change. Three
independent gates in the runtime each refuse a private bind, and all three
would have to move:

1. `Ssu2Service::publication_material` returns
   `InvalidPublication` unless `bound.ip().is_loopback()`
   (`crates/i2pr-runtime/src/ssu2_runtime.rs`), so on an RFC1918 bind there is
   no publication material at all.
2. `floodfill::is_normal_qualified_address` requires the published host to pass
   `is_publicly_routable_host`, which rejects `is_private()`
   (`crates/i2pr-daemon/src/floodfill.rs`).
3. The only other activation path, `activate_controlled`, begins with
   `note_explicit_bind_for_controlled_qualification`, which **also** requires
   every bound socket to be loopback
   (`crates/i2pr-runtime/src/ssu2_runtime.rs`).

So there is no daemon-only route, and the `controlled_loopback` profile cannot
be repointed: its config validation requires the SSU2 and NTCP2 profiles to be
`controlled_loopback`, and those in turn require loopback binds.

Two designs would work, and they differ in what the router would be **claiming**:

- **Asserted reachability.** A lab profile modelled on `controlled_loopback`,
  which sets `direct_reachability: true` and gates only on the daemon's own
  dual-transport record. Smallest change, mirrors an already-reviewed profile,
  and publishes `caps=f` on a private address as an operator assertion.
- **Proven reachability.** Let the runtime produce publication material for a
  private bind and let `is_normal_qualified_address` admit RFC1918, keeping
  `ReachabilityState::Reachable` as a requirement. `note_observed_external_address`
  (Plan 529) can corroborate it honestly, because a peer that dials in reports
  the address at which it sees the router.

Neither is a defect fix: both relax a predicate the runtime documents as
deliberate, and both make the router advertise a capability on a private
address. That is a capability decision, so it needs a plan of record and the
operator's choice of the two.
