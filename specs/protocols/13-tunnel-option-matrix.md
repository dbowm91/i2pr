# Tunnel option matrix (Plan 292 authority)

Status: **Plan 292 implementation authority**. The machine-readable
matrix is `crates/i2pr-i2pcontrol/src/tunnel_matrix.rs`
(`cell_disposition` over the frozen `TUNNEL_OPTIONS` inventory);
this document records the semantics behind each interpretation so
reviewers never have to reverse-engineer intent from code. The
module is authoritative on counts; this text is authoritative on
meaning. They must agree.

Planning authority: **Plan 292**
(`plans/implementation/i2pcontrol-proposal-170/292-tunnel-option-matrix-and-noncrypto-runtime-completion.md`).

Reference contract: Proposal 170 rev 2026-05-20 (see
`docs/provenance/proposal-170-manifest.md`). The project Emissary
Prop 170 fork is the secondary oracle for matrix shape only; i2pr
owners and capabilities differ, so no disposition is copied
blindly. Donor extensions without a pinned reference value carry
explicit i2pr interpretations below.

## Census

336 applicable cells (sum of inventory mask populations):

- `APPLY_CELLS = 227`: a named runtime/persistence owner consumes
  the key for the kind. The owner string names the consuming
  struct/field/sweep, never a storage mirror.
- `NOT_APPLICABLE_CELLS = 37`: the kind has no consuming layer for
  the key even though the Proposal mask group covers it.
- `INCOMPATIBLE_CELLS = 30` (Plan 293 resolution of the former
  `BLOCKED_293_CELLS`): applicable, but the key names a capability
  i2pr explicitly does not provide — dynamic SigType (12 cells),
  encrypted/blinded LeaseSet security and client authorization
  (16 cells), outproxy provider (2 cells). Supplying the key fails
  before allocation with the named limitation; omitting it selects
  ordinary i2pr behavior. Determinations and evidence live in
  `specs/protocols/14-tunnel-deep-option-determinations.md`; Plan
  295 carries the limitations into the final support claim.
- `CORRECTIVE_296_CELLS = 39` (pool backup-quantity/variance,
  multihoming, reply bundling) and `CORRECTIVE_297_CELLS = 3`
  (server `use_ssl` local TLS identity). Rejected keys name the
  limitation or owning plan; nothing is accepted inertly.

Refinement rule (uniform): within an applicable mask group, a kind
without the consuming layer (TCP endpoint, HTTP presentation,
streaming stack, UDP media path) is not-applicable; every other
cell is apply, incompatible, or corrective.

## i2pr interpretations

### Shaping (`tunnel_length`, `tunnel_quantity`, per-direction overrides)

Proposal bounds are enforced at the control boundary and again by
the typed `TunnelShaping` constructor: length 1..=3 (0 rejected;
the pool has no zero-hop remote mode), quantity 1..=6. Symmetric
keys set both directions; per-direction overrides project into
the single-hop-length pool. Differing `inbound_length` /
`outbound_length` values are a `ContradictoryOptions` error: the
pool uses one hop length and the control plane must not silently
drop a requested value. `TunnelShaping::balanced()` is (2, 2, 2)
and reproduces `DestinationConfig::balanced()`. Shaping edits
replace the destination runtime (pools rebuild under the same
identity; active streams follow the replace drain path).

Backup quantity and variance have no pool primitive: they are
`CorrectivePending` owned by Plan 296, never Apply.

### Profile (`profile`, `interactive`)

Only `interactive` is special (donor: PR6
`i2p.streaming.maxWindowSize`): `StreamingConfig::interactive()`
selects 16/16 windows, 32 unacked, 100 ms ACK with bulk timeouts
and ceilings kept. `profile` in {bulk, interactive} ORs with the
`interactive` boolean. Streamr kinds ride datagrams, not
streaming windows: the keys are not-applicable there. Profile
edits replace the destination runtime.

### Idle (`idle_timeout`, `close_on_idle`, `new_dest_on_idle`, `reduce_on_idle`)

Milliseconds deadline, `MIN 1000` / `MAX 86400000` / `DEFAULT
600000`. Flags without a deadline take the default; a deadline
without flags is inert and rejected. The pure `idle_decision`
fires close > rebuild > reduce priority at the exact deadline
with saturating arithmetic. The daemon sweep ticks every 5 s and
applies decisions through stop/restart transactions; the sweep
reads the committed spec each tick, so idle edits are
`MutableInPlace`. Sweep reductions halve quantities (floor 1)
via override; explicit starts clear the override.

### Proxy authentication (`proxy_username`, `proxy_password`)

Both halves required together; plaintext is scrubbed to the
marked verifier (`$i2pr1$` + hex SHA-256 of
`username:realm:password`) in `normalize_definition`, before the
definition mirror or store. Realms are per listener family
(`i2pr-http-proxy`, `i2pr-connect`, `i2pr-socks-proxy`); username
rotation requires the password half. Enforcement: HTTP/CONNECT
answer `407` with a `Basic` challenge naming the realm; SOCKS
selects `05 02` and subnegotiates RFC 1929 (`01 00` / `01 01`,
then close); SOCKS4a carries no authentication and is rejected
without a reply on guarded listeners. Missing-vs-wrong
credentials are indistinguishable at the status line. Credential
edits replace the destination runtime. `proxy_password` is
secret-classified: never stored as plaintext, `[redacted]` in
GET output, absent from errors, logs, and disk (only the marked
verifier is stored).

### Access lists (`access_list`, `white_list`, `black_list`)

Server kinds only (generic, HTTP server, bidirectional; raw TCP
servers have no other inbound gate, IRC server is out of mask).
Entries are canonical Base32 destination hashes (52 chars, with
or without `.b32.i2p`); `.i2p` names and anything else fail
naming the key, never the value. `access_list` unions
`white_list` into allow; `black_list` denies. Deny always wins;
empty allow admits everyone not denied; non-empty allow admits
only members. Enforced pre-SYN in the shared accept path (no
SYN response is queued for denied peers) with an
`access_denied` counter per runtime. Client destinations are
ephemeral per prepare, so allow-lists name stable peers
(persistent servers, SAM destinations); the gate itself is
hash-agnostic. List edits replace the destination runtime.

### Presentation gates (`address_helper`, `jump_list`)

HTTP server kinds only (8, 9); raw TCP servers have no HTTP
layer to gate. Both default open (historical forward-everything
behavior preserved); closing a gate refuses its class with 403
before the local target is touched. The local webserver is
never used as an addressbook/jump oracle unless the operator
opts in.

Wire classes (origin-form targets only; anything else stays
Ordinary and the existing origin-form filter still rejects it):

- Helper: path `/addresshelper` or under `/addresshelper/`, or
  the `i2paddresshelper` query key (case-insensitive).
- Jump: path `/jump` or under `/jump/`, or the `jump` query key
  (case-insensitive).
- Path families win over query keys; helper wins over jump when
  both markers are present; ordinary content never matches.
  Path matching is exact-case (origin servers own path case);
  query keys match ASCII case-insensitively.

Gate edits are `MutableInPlace` (the filter reads the committed
policy per request).

### Unique local source (`unique_local_address`)

Masked server kinds (generic, HTTP server, bidirectional). The
server dials its TCP target from `127.<hash[0]>.<hash[1]>.<hash[2]>`
derived from the SYN peer hash instead of the wildcard source,
so the local target can distinguish callers by source. The bind
stays inside 127/8 (loopback-only invariant holds either way).
Platforms without the derived alias assigned (macOS configures
only 127.0.0.1; Linux binds the whole /8) reject the bind with
`AddrNotAvailable`; only that case falls back to the wildcard
source and is counted per runtime (`unique_local_fallbacks`).
Any other bind failure fails the dial. Flag edits are
`MutableInPlace` (read per connection from the committed spec).

### Streamr redirect (`remote_udp_host`)

Streamr subscriber only. The redirect host pairs with the local
media port: the subscriber binds the local media host with an
ephemeral port and sends media to
`remote_udp_host:local_udp_port` instead of the local target
(replace, never duplicate). Loopback-confined like every
Streamr endpoint. Publisher and non-Streamr kinds reject the
key. Sink edits replace the destination runtime (the loop
captures its sink at supervisor start).

### Targets and multihoming

The Proposal inventory carries no multi-target selection key:
`target_host`/`target_port` (server TCP target) and
`target_destination` (client I2P destination) are singular.
Daemon TOML configuration supports a `targets` list (<= 8,
loopback/`unix:`) with first-target fallback at runtime, but
that is configuration-layer failover, not a control wire key,
and session multihoming (reply-info/target-selection
primitive) is `CorrectivePending` owned by Plan 296. There is
no dead control key to remove and nothing for Plan 292 to
implement here.

## Sensitivity

All Plan 292 keys except `proxy_password` are public: values
echo in GET output and persist in generation files. Only
`proxy_password` is secret-classified (see above). Access-list
entries are destination hashes (public routing metadata), never
redacted; parse failures name the key, never the value.
