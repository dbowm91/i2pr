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
