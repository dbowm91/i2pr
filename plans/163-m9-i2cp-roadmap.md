# Plan 163 — Milestone 9 I2CP roadmap

Status: **Milestone 9 planning authority; Plan 164 is the next executable pass**.

Registered: **2026-09-07**.

Depends on:

- Plan 161 `passed-m8-ssu2-independent-ipv4-interop-and-final-closure`;
- Plan 162 `passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration`;
- Milestone 7 SAM localhost closure via Plan 151;
- Milestone 6 local destination/LeaseSet2/Streaming correctness via Plan 134 plus Plan 152.

Execution sequence:

```text
Plan 163  M9 I2CP roadmap / planning authority
    ↓
Plan 164  I2CP source refresh, wire/profile foundation
    ↓
Plan 165  connection/session/config/options state machines
    ↓
Plan 166  client-owned destination + LeaseSet2/decryption-key bridge
    ↓
Plan 167  loopback TCP runtime/server composition
    ↓
Plan 168  message send/receive/status/lookup/bandwidth plane
    ↓
Plan 169  reconfigure/lifecycle/boundedness + self-composed local product
    ↓
Plan 170  independent Java/Go clients, evidence, final M9 closure
```

## 1. Milestone objective

Implement a bounded, interoperable I2CP server over the existing destination, LeaseSet2, garlic/routing, and Streaming-capable client substrate without creating a second destination stack.

Milestone 9 closes when independent I2CP client libraries can connect to the real `i2pr-daemon` loopback listener, create client-owned destinations, supply and publish valid Standard LeaseSet2 material, exchange I2CP application messages in both directions, receive protocol-correct status/lookup/bandwidth responses, reconfigure/destroy sessions safely, and leave all client/session resources at baseline.

The M9 claim is deliberately narrow:

```text
i2cp_localhost_client_protocol_interop = required
public_i2p_participation = not_claimed
milestone6_mixed_router_destination_interop = not_claimed
service_tunnels_http_socks_irc = milestone10
```

## 2. Critical ownership decision: I2CP destinations are client-owned

Do **not** implement I2CP by handing the client a SAM-style router-owned `DestinationIdentity`.

In I2CP, the client proves control of the Destination by signing the canonical `SessionConfig`. For modern LeaseSet2 operation the client later supplies a signed LeaseSet2 plus the matching destination **decryption** private key material needed by the router. The destination signing private key remains client-owned.

Therefore Plan 166 must introduce an explicit externally-owned/client-owned destination mode in `i2pr-client` while retaining the existing router-owned mode used by SAM.

Required ownership model:

```text
SAM / router-owned destination
  router owns Destination signing + decryption material
  router constructs/signs local LeaseSet2

I2CP / client-owned destination
  client owns Destination signing key
  SessionConfig signature proves control
  client supplies signed LeaseSet2
  client supplies only required LeaseSet2 decryption private key(s)
  router validates and installs decryption capability
  router never requires/synthesizes the Destination signing private key
```

This is an architectural invariant, not an implementation convenience. Do not duplicate the destination tunnel pool, remote routing, ECIES session, or payload queue implementations merely to accommodate I2CP.

## 3. Layering decision

### 3.1 Protocol/state lives in `i2pr-api`

Extend the existing runtime-neutral application adapter:

```text
crates/i2pr-api/src/i2cp/
```

It owns:

- strict bounded I2CP framing/codecs;
- connection/session state machines;
- typed SessionConfig and option models;
- canonical status/reply encoding;
- no sockets, Tokio, timers, or task ownership.

Do not create a new I2CP crate unless Plan 164 demonstrates a concrete dependency-cycle or ownership problem that cannot be resolved cleanly inside `i2pr-api`.

### 3.2 Destination functionality remains in `i2pr-client`

`i2pr-client` remains the sole destination product layer. M9 may add narrow capability abstractions for client-owned destinations, external LeaseSet2 installation, and inbound decryption material, but must reuse:

- `DestinationConfig` and tunnel pool policy;
- `DestinationRegistry` / runtime lifecycle;
- `DestinationRouting` and remote LeaseSet lookup;
- ECIES-X25519-AEAD-Ratchet destination sessions;
- bounded payload queues;
- existing inbound dispatch.

`i2pr-client` must not depend on `i2pr-api`.

### 3.3 TCP/Tokio ownership remains in `i2pr-daemon`

The daemon owns:

- `[i2cp]` configuration;
- TCP listener and accepted sockets;
- read/write deadlines;
- supervised connection tasks;
- projection of runtime-neutral I2CP actions into `i2pr-client` operations;
- cancellation and transactional cleanup.

Do not add production socket ownership to `i2pr-api` or `i2pr-client`.

## 4. Security/exposure policy

I2CP is an unencrypted client protocol. For M9:

```text
default_enabled = false
default_bind = 127.0.0.1:7654
non_loopback_bind = rejected
TLS = deferred
username_password_auth = deferred
remote_administration = prohibited
```

Do not expose I2CP on `0.0.0.0`, `::`, or a non-loopback address merely to satisfy an external client test. All M9 independent-client evidence must run on localhost TCP.

If later remote I2CP is desired, require a separate security plan covering authenticated encryption/TLS and credential handling.

## 5. Normative source and reference policy

Plan 164 must refresh and pin the I2CP source ledger before implementing codecs.

Starting references to verify at execution time:

```text
Official I2P website/spec repository snapshot:
  i2p/i2p.website @ 26467e4b275e3a58280b9d4e6d4745d58bb8c499
  content/en/docs/specs/i2cp.md
  content/en/docs/specs/i2cp-overview.md

Java I2P 2.13.0 reference/client:
  i2p/i2p.i2p @ 9134f808337b401e8e53c73734c81fab04280c9d

go-i2cp independent client:
  go-i2p/go-i2cp @ b529ee1c10a6011558b4d69fc9436a4afc489eac
```

The official specification is normative. Java and Go code may be inspected to resolve ambiguity and construct independent evidence, but must not be copied into i2pr. External clients used for Plan 170 must be fetched at exact pins and left unmodified.

If any pin becomes unavailable, stop and update the plan/status with an equivalent immutable pin before implementation; do not silently float to branch HEAD.

## 6. Compatibility/profile policy

Do **not** claim blanket “I2CP 0.9.67 support” merely because the current specification is documented through that API version.

The current i2pr destination product intentionally does not implement every historical or newest I2CP feature, including PQ destination encryption, encrypted/meta LeaseSets, legacy ElGamal/DSA modes, or every historical receive mode.

Plan 164 must produce a precise feature/profile table with three classes:

```text
implemented
explicitly unsupported/rejected
defined-but-ignored only where the I2CP specification itself requires ignore semantics
```

At minimum the M9 profile must support the modern Standard LeaseSet2 + Ed25519/X25519 path used by the selected independent clients.

Choose the lowest honest API-version behavior that allows that path. If the wire version field cannot express partial feature support perfectly, document the exact deviation in `specs/CONFORMANCE.md`; never advertise a feature solely because it was introduced before the chosen version.

Proposal 171 (outbound tunnel switching flag) is draft compatibility-watch material, not M9 implementation scope. If encountered, follow the current specification/proposal-required ignore/reject behavior without implementing tunnel switching.

## 7. Required MVP I2CP message surface

The detailed plans determine exact codecs, but the final product must cover the modern MVP path for:

```text
GetDate / SetDate
CreateSession / ReconfigureSession / DestroySession
SessionStatus
RequestVariableLeaseSet / CreateLeaseSet2
SendMessage / SendMessageExpires
MessagePayload
MessageStatus
GetBandwidthLimits / BandwidthLimits
DestLookup / DestReply
Disconnect
```

`HostLookup` / `HostReply` may be included if required by selected-client compatibility after Plan 164 probing; do not add a second address-book resolver. Deprecated `ReceiveMessageBegin/End`, legacy CreateLeaseSet, ReportAbuse, encrypted/meta LeaseSets, PQ keys, and multi-session semantics are not M9 requirements unless a later corrective plan proves one is necessary for the selected MVP clients.

## 8. SessionConfig and option policy

Every supplied option must have an explicit disposition.

Plan 165 must define a table for at least:

- inbound/outbound tunnel length;
- inbound/outbound quantity;
- inbound/outbound backup quantity;
- length variance;
- zero-hop policy;
- message reliability;
- fast receive;
- LeaseSet type/encryption type;
- unknown options.

Use existing `DestinationConfig` hard ceilings. Do not allow I2CP to bypass router resource limits.

Options that map safely onto existing policy are honored transactionally. Options that cannot be supported correctly are rejected with a protocol-correct status or explicitly documented according to spec semantics. Never silently accept a requested privacy/resource setting while doing something materially different.

## 9. LeaseSet2 validation policy

Before a client-owned session becomes usable, Plan 166 must prove all of the following:

- SessionConfig signature validates against the supplied Destination;
- SessionConfig creation time is within the protocol window;
- the supplied Standard LeaseSet2 signature validates;
- leases correspond to inbound tunnels actually owned by that destination runtime;
- LeaseSet2 encryption public key(s) match the supplied private decryption key(s);
- encryption/signing types are supported by the M9 profile;
- the operation commits atomically or leaves no installed secret/publication state;
- decryption material is non-`Clone` where practical, redacted, and zeroized on release;
- no private key or application payload enters logs/evidence.

A signed but structurally unrelated LeaseSet2 is not sufficient.

## 10. Message/data-plane policy

M9 reuses destination routing; I2CP is not a second routing implementation.

Outbound flow:

```text
I2CP SendMessage/Expires
  -> bounded I2CP payload validation
  -> destination/session ownership check
  -> existing destination routing / ECIES / garlic path
  -> MessageStatus derived from real enqueue/delivery outcome semantics
```

Inbound flow:

```text
existing destination inbound dispatch / ECIES
  -> owning client-owned destination
  -> bounded I2CP MessagePayload
  -> owning connection/session only
```

Status codes must not overclaim end-to-end delivery. Clearly distinguish router acceptance/queueing from stronger delivery evidence.

## 11. Environment contract

All M9 implementation and final evidence must remain compatible with the existing constrained environment:

```text
root/sudo               = no
Linux namespaces        = no
Docker                   = no
VM/Multipass             = no
systemd                  = no
public I2P network       = no
localhost TCP            = yes
GitHub-hosted Ubuntu     = yes
external source checkout = yes, exact-pinned
Java runtime             = allowed in external client lane
Go toolchain             = allowed in external client lane
```

Do not resurrect the historical privileged router harness. I2CP is a local client protocol; independent-client testing needs only an i2pr daemon and client libraries on loopback.

## 12. Independent-client policy

Final Plan 170 uses two independent client implementations where practical:

```text
mandatory primary:
  Java I2P 2.13.0 client library
  9134f808337b401e8e53c73734c81fab04280c9d

mandatory secondary target:
  go-i2p/go-i2cp
  b529ee1c10a6011558b4d69fc9436a4afc489eac
```

The desired final matrix is two simultaneously created destinations with cross-client traffic:

```text
Java client -> i2pr I2CP -> Go client
Go client   -> i2pr I2CP -> Java client
```

If the exact-pinned Go library has a concrete upstream defect that prevents a required row, Plan 170 must record the exact failure and determine whether another unmodified independent I2CP client is available. Do not substitute an in-tree raw driver and still call it independent-client evidence.

## 13. Evidence architecture

Use the successful M7/M8 pattern, but keep it narrow:

```text
tests/integration/i2cp/run-independent.sh
scripts/check-i2cp-acceptance-evidence.sh
.github/workflows/i2cp-external.yml
```

Requirements:

- external tests compile in routine CI but do not require external dependencies there;
- external execution is explicit and fail-closed;
- every final `passed` row derives from an executed command and, where relevant, sanitized evidence keys;
- routine Linux CI enforces the static evidence-integrity checker;
- no literal/unconditional pass bookkeeping;
- exact client pins and toolchain versions recorded;
- no destinations' private signing/decryption keys or raw payloads in artifacts.

Do not build a general multi-router orchestration framework.

## 14. Validation floor for every executable pass

Each Plan 164–170 must run its focused suite and the appropriate workspace floor. Before closing any pass:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-sam-acceptance-evidence.sh
cargo deny check advisories bans sources
```

Once Plan 164 introduces an I2CP vector checker, it becomes part of the floor for all later M9 passes. Once Plan 170 introduces the I2CP evidence-integrity checker, routine Linux CI must enforce it.

A pass does not close merely because local tests pass. Record exact closing commit and hosted routine CI where the plan requires it.

## 15. Milestone 9 final acceptance

Plan 170 may close M9 only if all are true:

1. I2CP framing/codecs are strict, bounded, vector-backed, and reject oversized/trailing/malformed input deterministically.
2. Connection version negotiation and session states are explicit and protocol-correct for the declared profile.
3. SessionConfig signature/date/options are validated before destination activation.
4. I2CP client-owned destinations do not require or synthesize the client's signing private key.
5. Standard LeaseSet2 + X25519 decryption material is validated and installed transactionally.
6. Existing router-owned SAM destination behavior remains green.
7. The daemon listener is disabled by default and loopback-only.
8. Session creation, reconfiguration, destruction, disconnect, cancellation, and daemon shutdown release all destination/listener resources.
9. SendMessage/SendMessageExpires use the existing destination routing path.
10. Inbound messages are returned only to the owning I2CP session through bounded queues.
11. MessageStatus semantics do not claim stronger delivery than actually observed.
12. Bandwidth and destination lookup messages return real/defined values rather than invented data.
13. Tunnel/session options are honored, explicitly rejected, or documented unsupported; none are materially ignored silently.
14. Slow readers/writers, oversized input, malformed sequences, duplicate sessions, bad signatures, stale dates, bad LeaseSet2/key pairs, and queue ceilings remain bounded.
15. A self-composed real-TCP local product test exchanges messages bidirectionally between two I2CP destinations without private setup after listener startup.
16. The exact-pinned Java I2P client creates/destroys a session and exchanges application data through i2pr.
17. A second independent client (target: exact-pinned go-i2cp) does the same, or a documented corrective decision replaces it before closure.
18. Cross-client Java->Go and Go->Java message exchange is command-derived and evidence-backed.
19. External sources remain unmodified.
20. The final evidence ledger/checker rejects synthetic success and contains no secrets/raw payloads.
21. Routine CI is green on the exact final implementation/closure head.
22. The manual I2CP external workflow is green on the exact final implementation/closure head.
23. `specs/support.toml`, `specs/CONFORMANCE.md`, protocol dossier, architecture docs, README, and plan authority agree on the exact supported profile.
24. No M6 mixed-router, public-network, service-tunnel, encrypted/meta LeaseSet, PQ, or remote-I2CP claim is inferred from M9.

Expected closing classification:

```text
plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
plan_165 = passed-m9-i2cp-connection-session-and-options
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2
plan_167 = passed-m9-i2cp-loopback-server-runtime
plan_168 = passed-m9-i2cp-message-data-plane
plan_169 = passed-m9-i2cp-self-composed-local-product-and-hardening
plan_170 = passed-m9-i2cp-independent-clients-and-final-closure
milestone9_i2cp_local_product = passed
milestone9_i2cp_independent_clients = passed
milestone9_final_acceptance = closed-via-plan170
next_product_layer = milestone10-planning
```

## 16. Stop conditions

Stop and write a narrow corrective rather than broadening the active pass if:

- implementing I2CP would require duplicating the destination runtime;
- client-owned destination support would expose/store the client's signing private key unnecessarily;
- an independent client proves a wire-semantic mismatch not covered by current tests;
- a requested option cannot be mapped without violating router-wide resource policy;
- non-loopback exposure appears necessary for acceptance;
- a final evidence row cannot be derived from an executed command;
- a required external dependency must be patched to pass.

## 17. Handoff

Plan 163 is planning authority only. Execute Plans **164 through 170 in order**. Plan 164 is the next executable pass.