# ADR 0014: bounded NTCP2 runtime link and address policy

## Status

Accepted for Plan 035 controlled local/private testing. This decision does not
advertise NTCP2, enable a public listener, publish RouterInfo addresses, or
claim mixed-router interoperability.

## Context

Plans 031–034 deliberately stop at runtime-neutral transport contracts and
consuming NTCP2 handshake/data state machines. Plan 035 needs to place TCP I/O,
deadlines, admission, replay retention, link children, and queue ownership at
the runtime boundary without leaking sockets or Tokio into the protocol crates.
Peer-controlled connection attempts also create socket-exhaustion, slowloris,
replay, duplicate-link, and high-cardinality address-observation risks.

## Decisions

### Ownership and service graph

`i2pr-runtime` is the sole production owner of `TcpListener`, `TcpStream`,
split halves, Tokio tasks/channels/timers, cancellation, and socket/resource
leases. The service graph contains a transport manager, an explicitly enabled
listener, a bounded dialer/backoff owner, a replay-cache owner, and one owned
reader plus writer child per authenticated link. A service may report ready
only after its required listener policy has been applied. Every child is
registered in `ChildScope`; reader/writer failure cancels the sibling and both
are joined before link closure is reported.

### Admission and timing

Admission is immediate grant-or-deny before expensive cryptography. It applies
global pending-handshake and socket limits, per-IP limits, and explicit IPv4
prefix/IPv6 prefix subnet limits. IPv4 uses a `/24` accounting prefix and IPv6
uses a `/64` accounting prefix; the original endpoint is never retained in
default snapshots or events. Connect, handshake, read-idle, write, queue-wait,
drain, and restart intervals are nonzero and capped by validated configuration.
Tests inject paused Tokio time or a manual clock and use only loopback or an
authorized isolated testnet.

### Replay and retry

Replay tokens are fixed-size digests owned by one bounded cache. Entries expire
in deterministic timestamp order; full capacity fails closed rather than
evicting a live token. Dial backoff is bounded, keyed by admitted peer/address
candidates, uses caller-injected jitter, expires records, and never sleeps
inside the transport contract crate.

### Duplicate links

The transport-neutral default rule is deterministic: same-direction candidates
retain the existing link; for simultaneous opposite directions, outbound wins
when the local router reference orders before the remote reference, otherwise
inbound wins. A winning candidate replaces the existing link, while a losing
candidate drains or is rejected according to the runtime's bounded drain
deadline. Link IDs make stale close notifications harmless. This is a local
policy seam pending the mixed-router evidence required by Plan 036; it is not a
claim about Java I2P or i2pd behavior.

### Addresses and observations

NTCP2 address parsing validates literal host/family, port, static public key,
obfuscation IV, version/capability fields, duplicate options, and unknown
option policy without DNS or socket calls. Configured literals and resolved
dial targets are separate types. Address/reachability results are bounded
observations only; they do not infer an external address or mutate RouterInfo,
NetDB, or publication state. Raw endpoint diagnostics require explicit
operator opt-in outside the default event/snapshot path.

### Framing and queue policy

The runtime exposes exact/bounded partial-I/O helpers and typed
cancellation/deadline inputs for the Plan 042 handshake-action driver. The
Plan 035 implementation did not claim a complete wire-level handshake or
authenticated data-phase composition; the current Plan 042 action executor
fills the bounded handshake actions and `AuthenticatedLink` owns receive
framing and frame-level queue leases, while their listener/dial and I2NP
composition remains incomplete.

## Plan 037 corrective amendment

The corrective integration review tightens the ownership seams without
changing the dependency direction. `InboundChunk::into_stream()` now returns
an `AdmittedInboundStream`, so the non-cloneable pending-handshake permit stays
with the accepted socket until the wrapper is consumed or dropped. Service
created links use `ActiveLinkAdmission` and retain an `ActiveLinkPermit` in the
`LinkHandle`; `Ntcp2RuntimeService::start_link()` is the bounded entry point
for that path. Each queued frame owns its item and byte release through drop,
and reader/writer children use the configured cancellation-aware idle/read,
write, and queue-wait deadlines.

Outbound dial admission is consulted before connect and is cleared only by an
explicit `DialAttempt::mark_authenticated()` call. These APIs remain runtime
seams: the Plan 042 action executor now composes bounded handshake actions,
but listener/dial policy, the authenticated data phase, and the synchronous
`TransportManager` remain unwired, and the daemon keeps live activation
disabled. Full composition must precede any mixed-router testnet execution.

## Plan 042 runtime-driver amendment

Plan 042 is the approved composition boundary: the complete NTCP2 action
executor and authenticated data-phase owner belong in `i2pr-runtime`, using
these socket, admission, replay, backoff, deadline, queue, and child-scope
owners. The non-production launcher supplies only validated scenario and
disposable-state capabilities. It must not move socket ownership into
`i2pr-transport-ntcp2` or activate `i2pr-daemon`.

Inbound listener readiness and authenticated readiness are separate status
events. The pending-handshake permit remains attached until authentication or
terminal failure; only an authenticated stream receives active-link ownership.
The current checkout composes listener/dial, handshake-to-data-phase
promotion, and the DeliveryStatus smoke in the non-production launcher. Local
success provides driver validation only; it cannot provide interoperability
evidence. State, authentication, data-phase, timeout, and cleanup failures
remain typed terminal results.

## Consequences

The runtime can exercise controlled TCP lifecycle and cleanup without changing
the pure protocol dependency direction. It adds implementation and test
surface for admission and ownership, but keeps public activation and support
claims disabled. Loopback success and local vectors remain structural evidence
only. Mixed-router work must validate the duplicate rule, padding/coalescing
policy, address publication, and full handshake/data exchange before changing
the support ledger.

## Review triggers

Revisit this ADR if the specification or deployed-router evidence changes
duplicate resolution, IPv6 scope, replay eviction, runtime padding/coalescing,
or whether a listener may be enabled by default. Any request for DNS, NAT
discovery, NetDB mutation, automatic publication, public-network testing, or a
Tokio dependency below `i2pr-runtime` requires a new bounded plan.
