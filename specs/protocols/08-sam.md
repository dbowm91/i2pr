# Simple Anonymous Messaging (SAM) v3

Status: **required**  
Primary roadmap milestone: **7**  
Role: primary language-neutral application API for the MVP

## Scope

SAM is a socket-oriented client protocol exposing I2P destinations, naming, streams and optionally datagrams/raw messages to non-Java applications. It is an application-to-router protocol, not an I2P network wire protocol.

## Authoritative sources

- [SAM v3 specification](https://i2p.net/en/docs/api/samv3/), pinned in [SOURCES.md](../SOURCES.md), updated 2026-05 and accurate for 0.9.69.
- Streaming, datagram and naming documentation referenced by SAM.
- Java I2P SAM implementation for details where the API document is ambiguous.

The official document identifies SAM 3 and 3.1 as stable and notes that later 3.x features vary across router implementations; it specifically warns that i2pd does not support most 3.2/3.3 features. The `i2pr` MVP should therefore make SAM 3.1 STREAM behavior its baseline and add later features individually with truthful negotiation.

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

### Streaming

- `STREAM CONNECT`;
- `STREAM ACCEPT`;
- `STREAM FORWARD` if included by the milestone plan;
- status replies before transition to raw stream data;
- destination, port and protocol fields supported by the negotiated version;
- clean close/reset propagation between SAM socket and internal streaming connection.

### Naming

- `NAMING LOOKUP` for `ME`, Base64 destinations, Base32 forms and configured local naming sources.
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
- STREAM connect/accept/forward interoperability using at least two independent SAM clients.
- Destination generation/import/export without private-key leakage.
- Naming success, not-found, invalid Base32/Base64 and forbidden clearnet resolution.
- Slow control client, slow stream reader/writer, queue saturation and cancellation.
- Multiple sessions/streams under global and per-client limits.
- Unsupported DATAGRAM/RAW/later-version commands return correct status without side effects.
- Fuzzing of command lines, option maps and state-machine transitions.

## Deferred and compatibility behavior

- SAM v1/v2 and BOB: legacy-reject; do not implement.
- Most SAM 3.2/3.3 features: feature-by-feature `required-later` or deferred after checking i2pd interoperability and application need.
- Remote unauthenticated listener: excluded.
- UDP SAM transport, primary/secondary sessions, advanced forwarding and SSL-specific variants: deferred unless selected by the detailed plan.
- DATAGRAM/RAW: deferred unless needed by concrete MVP clients; keep protocol-specific implementation separate from streaming.

## Open decisions

1. Exact advertised SAM version range for the first release; baseline recommendation is 3.1 STREAM.
2. Session ownership model when control and stream sockets are separate.
3. Destination key import/export representation and filesystem permissions.
4. Naming backend and address-book scope for the MVP.
5. Allowlisted session/tunnel/streaming options and stable error mapping.
6. Whether STREAM FORWARD is necessary for the first SAM checkpoint.
7. Authentication/TLS design for any future non-loopback listener without inventing incompatible SAM extensions.