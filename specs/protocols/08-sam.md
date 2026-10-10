# Simple Anonymous Messaging (SAM) v3

Status: **required**  
Primary roadmap milestone: **7**  
Role: primary language-neutral application API for the MVP

## Scope

SAM is a socket-oriented client protocol exposing I2P destinations, naming, streams and optionally datagrams/raw messages to non-Java applications. It is an application-to-router protocol, not an I2P network wire protocol.

The i2pr SAM 3.1 localhost application layer is closed by Plan 151. The
real loopback listener passes exact pinned i2psam and qualified unmodified
i2plib.sam clients for STREAM, private destinations, FORWARD, NAMING, and
negative behavior. The official libsam3 snapshot is built/probed but is not
counted because its public key-length contract rejects i2pr's canonical
compact `PRIV`. This evidence does not claim router-to-router interoperability.
Plan 368 closed the qualified local SAM 3.3 PRIMARY/subsession profile; its
version range is negotiated locally and makes no public or remote-router
capability claim. See `plans/closure/sam/368-status.md`.

## Authoritative sources

- [SAM v3 specification](https://i2p.net/en/docs/api/samv3/), pinned in [SOURCES.md](../SOURCES.md), updated 2026-07 and accurate for 0.9.70.
- Streaming, datagram and naming documentation referenced by SAM.
- Java I2P SAM implementation for details where the API document is ambiguous.

The official document identifies SAM 3 and 3.1 as stable and notes that later 3.x features vary across router implementations; it specifically warns that i2pd does not support most 3.2/3.3 features. Plan 368 uses the current official specification and pinned Java I2P as normative. i2pd is diagnostic only and cannot narrow the selected profile.

## Qualified local SAM 3.3 profile

Plan 368 is the implementation and closure authority for SAM 3.3
PRIMARY/subsessions.
Its normative sources are the current official SAM V3 document and the pinned
Java I2P 2.13.0 implementation. The official document reports SAM 3.3 and adds
DATAGRAM2 (I2CP 19), DATAGRAM3 (I2CP 20), and Proposal 167 LeaseSet option
retrieval in April 2025 without changing the negotiated version. Plan 368
includes those additions, the SAM 3.3 per-datagram send options
`SEND_TAGS`/`TAG_THRESHOLD`/`EXPIRES`/`SEND_LEASESET`, and the selected
PRIMARY/subsession profile. i2pd 2.61.0 is diagnostic only; its unsupported
features cannot narrow this profile. Plan 368 passed its qualification matrix
and repository verification floor. The local profile remains experimental,
loopback-only, disabled by default, and non-advertised for public/remote use.

The official specification marks the per-datagram send controls optional. The
current loopback UDP adapter explicitly rejects packets containing them before
enqueue and emits a payload-free diagnostic because the canonical destination
send owner cannot honor those overrides. Proposal 167 `OPTIONS=true` is
implemented for locally resolvable LeaseSet2 records; unavailable records
return `LEASESET_NOT_FOUND`. This partial command behavior does not promote the
negotiated or advertised SAM version.

The router's DatagramManager now owns bounded protocol 17–20 send/receive
framing, including Proposal 163 recipient-bound Datagram2 signatures, offline
delegation expiry checks, replay rejection, bounded mapping options, and
Datagram3's explicitly unauthenticated source hash. The managed-app manager
protocol has a separate `sam_datagram` stream for typed send/receive operations;
the existing private `sam` stream remains exact SAM octets and carries no host
UDP endpoint authority. The current checkpoint records Java 2.13.0
child-style qualification, DATAGRAM1/2/3 receive and same-PRIMARY STREAM
round-trip evidence, plus an ordinary combined STREAM/DATAGRAM/RAW
shared-Destination fixture.

## Required MVP command surface

### Version negotiation

- `HELLO VERSION` with strict minimum/maximum parsing.
- Select the highest mutually supported version from the implemented set.
- Return explicit unsupported-version results without accepting later commands.
- Keep negotiated capabilities per connection; do not infer them only from command spelling.

### Sessions and destinations

- `SESSION CREATE` for STREAM sessions.
- Named session IDs scoped and validated according to the selected SAM version.
- `DESTINATION GENERATE` or current equivalent behavior required by SAM 3.1.
- Persistent/private destination material handling through explicit safe configuration or returned keys.
- Session options translated through a reviewed allowlist into destination/tunnel/streaming configuration.
- Session destruction on control-connection loss where required, with bounded cleanup.
- `PUB` and `PRIV` text uses the I2P Base64 alphabet
  (`A-Z a-z 0-9 - ~`, `=` padding) — not RFC 4648. This is the
  spelling every Java I2P / i2pd / independent Python SAM client
  reference implementation emits; see
  `specs/references/sam31-private-destination.md` for the
  corroborating evidence.

### Streaming

- `STREAM CONNECT`;
- `STREAM ACCEPT`;
- `STREAM FORWARD` as a loopback-only, experimental local bridge under
  Plans 139/150; it does not expose a general TCP pivot;
- status replies before transition to raw stream data;
- destination, port and protocol fields supported by the negotiated version;
- clean close/reset propagation between SAM socket and internal streaming connection.

### Naming

- `NAMING LOOKUP` for session-scoped `ME`, canonical full Base64
  Destinations, and locally-known Base32/hash forms. Unknown names return
  bounded failure without system DNS or a second address book.
- A valid `NAMING REPLY` echoes the requested `NAME`; Java I2P's SAM client
  uses it to correlate outstanding lookups.
- Strict distinction between syntactically valid cryptographic addresses and human-readable names.
- No implicit clearnet DNS resolution through the SAM endpoint.

Datagram and RAW sessions are required only if explicitly included in the Milestone 7 plan. Their absence must be negotiated/rejected correctly rather than partially accepted.

## Parser and connection model

SAM is line-oriented during command/status phases and may transition a socket to raw payload forwarding. Implement:

- maximum line length, token count, key length and value length;
- ASCII command/keyword handling and exact whitespace/quoting rules from the negotiated version;
- duplicate-option policy;
- known versus unknown option behavior;
- command sequencing by connection/session state;
- deadlines for greeting, command completion, accept/connect and idle control sockets;
- explicit transition boundaries so command bytes cannot be confused with stream payload;
- backpressure in both directions after transition to streaming data.

Do not use shell-style tokenization or URL query parsing unless it exactly matches SAM grammar.

## Security and exposure policy

- Bind to loopback only by default.
- Remote exposure requires explicit configuration, authentication design and TLS/reverse-proxy guidance; it is not part of baseline MVP support.
- Apply global, per-client, per-IP, per-session, per-destination and pending-operation limits.
- Do not log private destination keys, complete destination strings, payloads or authentication material.
- Restrict destination/tunnel options to safe validated fields; clients must not inject arbitrary router configuration.
- Cancel destinations, pending lookups and streams when the owning session/connection terminates according to SAM semantics.
- Prevent session-ID collision or cross-client access.
- Return bounded errors without reflecting large attacker-controlled values.

## Implementation references

- Java I2P: `apps/sam/java/src` plus streaming and naming adapters.
- I2P+: corresponding SAM package; inspect parser hardening and operational defaults.
- i2pd: SAM server under `libi2pd_client` and daemon configuration.
- Emissary/go-i2p: current SAM bridge/companion integration; verify version and command coverage from source/tests.

Use independent SAM client libraries as client-side test drivers, but treat their behavior as interoperability evidence rather than authority. The official spec’s library table notes that listed libraries are not necessarily reviewed or maintained.

## Required tests

- HELLO negotiation across supported, overlapping and disjoint version ranges.
- Command parsing with maximum lines/tokens/options, duplicates, invalid quoting and partial reads.
- Illegal command sequences and commands after raw-data transition.
- Session-ID collision, reuse, disconnect and cleanup.
- STREAM connect/accept/forward interoperability using the two independently
  implemented Plan 150 clients, with the supporting transcript kept separate
  from the independent-client count.
- Destination generation/import/export without private-key leakage.
- Naming success, not-found, invalid Base32/Base64 and forbidden clearnet resolution.
- Slow control client, slow stream reader/writer, queue saturation and cancellation.
- Multiple sessions/streams under global and per-client limits.
- Unsupported DATAGRAM/RAW/later-version commands return correct status without side effects.
- Fuzzing of command lines, option maps and state-machine transitions.

## Plan 368 scope and compatibility behavior

- SAM v1/v2 and BOB: legacy-reject; do not implement.
- Plan 368 includes the current official SAM V3 additions that affect its selected command surface: I2CP ports/protocols, PRIMARY/subsessions, Datagram2/3, per-datagram send controls (`SEND_TAGS`, `TAG_THRESHOLD`, `EXPIRES`, `SEND_LEASESET`), and `NAMING LOOKUP OPTIONS=true` LeaseSet option retrieval.
- Optional authentication/TLS, SSL STREAM FORWARD, optional QUIT/STOP/EXIT utility commands, and unrelated tunnel/I2CP configuration options remain outside Plan 368. They must be explicitly rejected or follow existing behavior; no 3.3 negotiation implies these features.
- Remote unauthenticated listener: excluded.
- Primary/subsessions and UDP SAM datagram transport are selected by Plan 368. The UDP bridge is loopback-only and ordinary-SAM only; the private managed-app operation remains a typed manager-protocol path with no host endpoint authority.
- `MASTER` remains accepted as the documented alias of `PRIMARY`.

## Open decisions

1. Plan 368's acceptance and evidence matrix is closed. SAM 3.3 remains an experimental local profile and is not advertised for public or remote-router use.
2. Session ownership model when control and stream sockets are separate.
3. Destination key import/export representation and filesystem permissions.
4. Naming backend and address-book scope for the MVP.
5. Allowlisted session/tunnel/streaming options and stable error mapping.
6. Plan 139's STREAM FORWARD policy is intentionally loopback-only and
   control-socket-owned; Plan 150 validates its real-byte path through the
   local listener without broadening the exposure policy.
7. Authentication/TLS design for any future non-loopback listener without inventing incompatible SAM extensions.
