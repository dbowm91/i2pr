# Repliable datagrams + Streamr (Plan 291 freeze)

Status: **Protocol freeze for Plan 291**. Java I2P 2.13.0 behavior
is the reference (behavioral only; no GPL code is copied). The
project Emissary Prop 170 fork (`i2pcontrol/streamr/* + datagram
substrate`, manifest class R) is the policy donor where Java is
silent. Numeric choices state their source below; the Plan 291
closure must confirm each one with executed evidence.

Planning authority: **Plan 291**
(`plans/implementation/i2pcontrol-proposal-170/291-repliable-datagram-and-streamr-tunnel-families.md`).

## Reference behavior (Java I2P, `net.i2p.i2ptunnel.streamr`)

- `StreamrConsumer` (client: I2P client, no privkey; UDP side sends
  to a configured host/port) + `Pinger` (subscribe thread).
- `StreamrProducer` (server: persistent privkey; UDP side receives
  on a configured port) + `Subscriber` (subscription table) +
  `MultiSource` (fanout to many sinks).
- Subscribe message: **one byte `0x00`**; unsubscribe: **one byte
  `0x01`**; any other first byte is invalid and dropped.
- Subscribe cadence: first five at **2 s**, then steady **10 s**;
  unsubscribe sent once on close.
- Subscriber table: keyed by **(destination, swapped from/to
  ports)**; **`MAX_SUBSCRIPTIONS = 10`** (11th denied);
  **`EXPIRATION = 60 s`** from the last subscribe; a 60 s sweep
  removes the expired.
- I2P transport: the UDP base (`I2PTunnelUDPServerBase`) **receives
  repliable datagrams on all ports** and **sends raw datagrams**
  (`I2PSinkAnywhere`). Subscribe control is authenticated;
  media fanout is unauthenticated raw to the learned reply
  addresses. This matches the canonical token pattern in the
  [Datagram specification](https://geti2p.net/en/docs/specs/datagrams):
  one authenticated datagram carries identity, subsequent traffic
  is raw.
- Subscribe addressing: `fromPort` = local UDP port, `toPort` = 0;
  replies swap the ports.

## Wire selected for i2pr (Datagram1 + Raw)

- I2CP protocol numbers (frozen inventory, unchanged):
  **Datagram1 = 17** (repliable, authenticated), **Raw = 18**
  (non-repliable, unauthenticated). Datagram2 (19) and Datagram3
  (20) stay unsupported.
- Datagram1 layout (spec): `from` = full Destination (387+ bytes),
  `signature` (64 bytes Ed25519 over the **payload** for non-DSA
  key types), `payload` (here: exactly 1 byte, `0x00`/`0x01`).
  i2pr destinations are Ed25519; DSA_SHA1-hash-then-sign is not
  implemented and verify-rejects.
- Raw layout: application payload only, no `from`, no signature.
- I2CP ports ride the standard client-payload envelope
  (source/destination ports); unknown protocols keep the existing
  typed `UnsupportedProtocol` drop.

## Bounds selected (source in brackets)

- `MAX_STREAMR_SUBSCRIBERS = 10` [Java `MAX_SUBSCRIPTIONS`].
- `STREAMR_SUBSCRIPTION_EXPIRY_MS = 60_000` [Java `EXPIRATION`].
- Subscribe refresh: fast-start 2 s × 5, then every 10 s; one
  unsubscribe (`0x01`) on deterministic shutdown [Java `Pinger`].
- `MAX_STREAMR_APPLICATION_PAYLOAD = 1200` bytes [fork donor
  candidate, confirmed by the datagram reliability guidance:
  well under the 10 KB recommendation, about one tunnel message,
  so media fanout stays in the reliable regime]. Larger local UDP
  reads are truncated... no: oversized local payloads are
  **rejected** (typed, counted), never fragmented or truncated —
  silent truncation would corrupt media framing.
- No fragmentation/reassembly anywhere: one UDP read = at most
  one I2P datagram; one I2P datagram = at most one UDP write.
- Bounded queues everywhere (subscriber table, send queue, receive
  queue) with typed backpressure outcomes; deterministic-clock
  expiry; no peer-controlled task spawning.

## Authentication and binding rules

- A subscribe is accepted only when: protocol is 17, the `from`
  Destination parses under caller-visible bounds, its signing key
  type is supported (Ed25519), the Ed25519 signature verifies over
  the 1-byte payload, the flag byte is `0x00`/`0x01`, and
  destination/port metadata fits the subscriber key.
- Media (protocol 18) carries no sender proof by design; the
  client forwards it **only** when it arrives on the configured
  producer's destination (binding by destination hash), and the
  server accepts media **only** from its loopback UDP socket.
- Router-internal substrate only: SAM stays streaming-only
  (`specs/protocols/08-sam.md` unchanged); no SAM DATAGRAM/RAW
  surface is added.

## Malformed input (all dropped, typed, counted)

Null destination, empty payload, flag byte other than
`0x00`/`0x01`, unparsable `from`, unsupported signing key type,
signature failure, oversized payload, unknown protocol, expired
subscriber refresh, over-cap subscription, non-loopback UDP
peer — every one is a typed rejection, never an exception path
and never a silent accept.

## What this freeze does not decide

Per-type option defaults beyond the frozen inventory keys
(`streamr_subscribe_interval`, `streamr_expiry`,
`streamr_max_subscribers`, `streamr_payload_limit`,
`local_udp_host/port`, `remote_udp_host`) belong to the
implementation within the bounds above; the wider option matrix
belongs to Plan 292.
