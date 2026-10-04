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

- `APPLY_CELLS = 266`: a named runtime/persistence owner consumes
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
- Current override (Plan 324): the historical SigType incompatibility is
  superseded for type 7 only. Canonical TunnelManager accepts EdDSA-SHA512 /
  Ed25519 (type 7) and active Standard LeaseSet2 X25519 (EncType 4) through
  the typed Destination crypto-policy owner. Other signature/encryption
  types remain explicitly unavailable; Red25519 depends on Plan 325.
- `CORRECTIVE_296_CELLS = 0` (Plan 296 closed every residual into a
  named apply owner: pool backup-quantity/variance, multihoming
  target selection, reply bundling) and `CORRECTIVE_297_CELLS = 0`
  (Plan 297 closed the server `use_ssl` residual with an explicit
  local TLS identity/trust policy). Rejected keys name the
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

### Standby (`tunnel_backup_quantity`) and variance (`tunnel_variance`)

Plan 296 owners. `tunnel_backup_quantity` (Proposal bound 0..=3)
holds that many extra tunnels ready beyond each per-direction
quantity target: the pool maximums become the effective targets
(base plus backup) while usability still keys on the base target,
established tunnels past the base count as standby, and standby
promotes automatically when a base tunnel fails or expires (counted
for evidence). Failures still count toward the replacement
threshold even when standby covers them; build budgets are
unchanged. Each direction's quantity plus backup must fit the pool
directional maximum of 8 — oversums fail as `ContradictoryOptions`,
never clamped.

`tunnel_variance` (Proposal bound −2..=+2, a radius: the sign
carries no direction) randomizes each build's hop length around the
configured length, floored and ceiled at the pool hop policy
(1..=8). Randomness is caller-supplied: production passes CSPRNG
bytes and unit paths inject fixed bytes; the sampler never touches
ambient RNG. Backup/variance edits ride the shaping struct, so
they replace the destination runtime like every shaping edit.

### Targets and multihoming

The Proposal inventory carries no multi-target selection key:
`target_host`/`target_port` (server TCP target) and
`target_destination` (client I2P destination) are singular.
Daemon TOML configuration supports a `targets` list (<= 8,
loopback/`unix:`) with first-target fallback at runtime, but
that is configuration-layer failover, not a control wire key.

Plan 296 resolves `multihoming` (server kinds: generic, HTTP
server, bidirectional) as documented target selection — the only
semantic, with no session reply-info flag introduced and no silent
reinterpretation: when set, each inbound connection dials a
round-robin-selected target with sequential failover instead of
the first target only. Every dial attempt shares the connection's
overall connect deadline (refused targets fail over fast; an
unresponsive target consumes the deadline, preserving the existing
timeout semantic); only an all-target failure counts a failed
connect. The flag requires at least two configured targets (fewer
is a `ContradictoryOptions` error, never inert) and loopback-TCP
targets only (Unix targets are rejected with the flag). Unset keeps
first-target failover. Multihoming edits are `MutableInPlace` (the
dial reads the committed flag and target list per connection).

The control surface carries a singular target and no multi-target
wire key exists, so `multihoming=true` through I2PControl always
meets the two-target minimum as a named contradiction; the
dial-selection owner serves multi-target specs built through other
surfaces.

### Reply bundling (`reply_bundling`)
Plan 296 owner, all kinds. When set, the outbound delivery path
may carry multiple same-remote application payloads as multiple
data cloves in one New Session Reply garlic message (at most four
data cloves, bounded by the destination payload ceiling); unset
keeps one payload per garlic message. Only the reply form bundles:
fresh bound New Sessions keep their mandatory LeaseSet2 bundle and
Existing Session traffic keeps its lean single-clove form.

Inbound, every non-LeaseSet2 data clove routes in wire order with
all-or-nothing admission (pre-count cap, uniform-target and queue
capacity pre-checks run before the first push). The outbound sweep
groups consecutive same-remote requests and bundles runs of two or
more when the destination's policy enables it, reporting per-index
delivery with no retry of sealed bundles (re-sending would
duplicate application bytes); refused bundles, disabled policies,
single requests, and mixed remotes all take the single path.
Bundling-flag edits are `MutableInPlace` (the sweep reads the
committed flag per sweep).

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

### Server TLS (`use_ssl`)

Plan 297 owner, server kinds (generic, HTTP server,
bidirectional) with loopback-TCP targets only. When set, the
server negotiates TLS to the configured loopback target before
proxying application bytes, under the daemon's explicit TLS
identity/trust policy (`[service_tunnels.tls]`): the endpoint
identity comes from provisioned PEM (never silent self-signature
without operator consent) and is offered as the client
certificate when the target requests client authentication; the
target's certificate verifies against exact SPKI pins first, then
explicit trust roots, then — only with explicit loopback opt-in —
unverified. Ambient system roots are never consulted. A policy
that verifies nothing is rejected at load, and `use_ssl` dials
without any installed policy fail before connecting. Verification
failure fails the connection (typed, counted per runtime) and
never falls back to plaintext; `use_ssl=false` keeps plaintext
behavior. TLS implies no interception or MITM capability: the
endpoint terminates or originates TLS only on the loopback target
leg it already owns, with no key escrow and no cross-tunnel
identity reuse. Private key material follows the storage
precedent (restricted permissions, redacted wrappers, never in
control output, logs, or errors). Rotation happens via
configuration change and restart; the provisioned identity expiry
and the per-runtime handshake counters surface through the
tunnel's control state. `use_ssl` edits are `MutableInPlace`
(the dial reads the committed flag per connection).

The pinned PR6 reference maps server `UseSSL` onto TLS between
the tunnel endpoint and the local target; client-side `UseSSL`
is outside the frozen inventory and stays unclaimed.

## Sensitivity

All Plan 292 keys except `proxy_password` are public: values
echo in GET output and persist in generation files. Only
`proxy_password` is secret-classified (see above). Access-list
entries are destination hashes (public routing metadata), never
redacted; parse failures name the key, never the value.
