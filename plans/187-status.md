# Plan 187 status — M6 remote LeaseSet2 and destination Garlic routing

Status: **`blocked-by-m6-build-reply-interop-gap`** (local rows passed;
remote gate pending Plan 188). Plan 188 already flipped 2/7 rows via
consumed reference replies; the remaining 5/7 destination rows were
blocked on the inbound NetDB reply-path metadata defect that Plan 190
isolates and corrects (see [`plans/190-status.md`](190-status.md);
Plan 190 is now passed locally with the corrected reply path).

Plan of record:
[`plans/187-m6-remote-leaseset2-and-destination-garlic-routing.md`](187-m6-remote-leaseset2-and-destination-garlic-routing.md).

## Current authority

```text
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (local rows passed; remote lane pending exact-pinned i2pd run)
plan_188 = blocked-by-plan190-reply-path-corrective (real outbound/inbound i2pd installs retained-passed)
plan_187 = blocked-by-m6-build-reply-interop-gap (local rows passed; 2/7 flipped via plan188 installs; 5/7 reply-path gap isolated by plan190)
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_183 = registered-m6-mixed-router-streaming-interop-program
m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-pending-plan190-external-run
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 188 (continue external lane with plan190 reply-path correction)
```

## What landed

Strict Plan 187 §2–§9 delivery for the local product; no M6 wire
change. The external lane is fully built and executes fail-closed
against exact-pinned i2pd, stopping at the §11 build-reply gap with
multi-run diagnosis (see §11 below).

```text
crates/i2pr-daemon/src/destination_tunnels.rs (new)
  Daemon-owned destination LeaseSet2/Garlic-over-tunnels
  coordinator (Plan 187 §4–§8), runtime-neutral, no sockets/tasks:
  authoritative bounded RouterInfoStore (ordinary-path reference
  bootstrap, floodfill verification, candidate selection through
  the existing routing-key logic); remote Standard LeaseSet2
  lookup via NetDbSeam::begin_lease_set2_lookup_with_store (never
  the placeholder empty store) + compose_lookup_via_tunnel
  (existing outbound composition, TunnelData/first-hop/floodfill
  proof, direct transport always rejected); ingest through the
  existing LeaseSet2 validators into the authoritative
  LeaseSet2Store (destination binding, signature, expiry, lease
  presence, encryption type, freshness; mismatch/stale/invalid/
  malformed/duplicate bounded); verify_remote_material (counted
  rows reject LocalZeroHop, require usable real inbound leases +
  a real outbound route); bounded local LS2 publication with
  protocol-derived DeliveryStatus correlation and no-resign
  retry; recover_garlic_bytes through the real inbound endpoint
  registry (unknown tunnel ids never allocate state); bounded
  pending/reply-path/publication/retry/deadline surface with
  typed tunnel-loss (never direct fallback) and cancellation.
  MAX_CONCURRENT_LEASE_LOOKUPS=8,
  MAX_RETAINED_LEASE_REPLY_PATHS=8,
  MAX_CONCURRENT_LEASE_PUBLICATIONS=8, MAX_LEASE_LOOKUP_RETRIES=3.
  Evidence surfaces carry counts/hashes/bounds only.

crates/i2pr-daemon/src/netdb_seam.rs
  Additive Plan 187 §5 seam: begin_lease_set2_lookup_with_store +
  advance_lease_set2_after_path_with_store thread the
  authoritative store through the LeaseSet2 state machine (the
  existing no-store wrappers delegate, behavior unchanged);
  ingest_lease_set2_search_reply for bounded LS2 search-reply
  ingest. No wire change.

crates/i2pr-daemon/src/inbound_dispatch.rs
  Additive Plan 187 §7 seam: InboundResponseKind::Garlic (0x0B)
  + InboundDispatchOutcome::GarlicComplete so recovered
  destination Garlic carriers route to the existing client-layer
  ECIES dispatch; every NetDB arm is byte-identical.

crates/i2pr-daemon/src/outbound_lookup.rs
  Additive Plan 187 §6 seam: deliver_outbound_cells wraps
  pre-built client-layer Garlic cells into short-transport
  deliveries addressed to each cell's first hop (no tunnel
  re-composition, no direct route).

crates/i2pr-daemon/src/exploratory_build.rs
  Two narrow additive accessors the Plan 187 data path needs:
  registry()/registry_mut() (scoped borrow for composition and
  inbound dispatch; pool ownership never moves).

crates/i2pr-client/src/routing.rs
  Additive DestinationOutboundRole::from_role (move, never clone;
  preserves the exclusive secret-material ownership invariant).

crates/i2pr-tunnel/src/data_plane_registry.rs
  Additive inbound_receive_ids() (public metadata for reply-path
  selection; no secret material leaves the registry).

crates/i2pr-netdb/src/lib.rs
  Re-export base64::{decode, encode} (the Plan 142 SAM wire
  codec was unreachable outside the crate; the filename codec
  stays untouched).

crates/i2pr-daemon/tests/destination_tunnel_unit.rs (new)
  27 rows: authoritative bootstrap/eligibility, tamper rejection,
  empty-store honest termination, lease send action (key =
  destination-derived router hash), non-floodfill exclusion,
  tunnel-path proof + direct rejection, ingest completion +
  cache, key mismatch, stale expiry, invalid signature, malformed,
  duplicate bounded, search-reply bounded, deadline, cancellation,
  concurrent ceiling (single-active-seam documented, as in Plan
  186), tunnel-loss typed, zero-hop rejected for counted rows,
  real material proof, empty-pool honest error, stale-cache
  refresh, lease-expiry-between-lookup-and-send, publication
  matrix (eligibility/body/ack/mismatch/retry-no-resign/cancel),
  unknown-tunnel garlic rejection.

crates/i2pr-daemon/tests/destination_tunnel_live.rs (new)
  9 two-role rows through real TunnelData cells: lease lookup +
  floodfill proof, inbound LS2 recovery to terminal success +
  cache + routing-layer handoff (install + select), bidirectional
  ECIES/Garlic round-trip through real destination tunnels with
  OBEP lease-target proof + coordinator Garlic recovery +
  byte equality + sibling isolation, tamper-closed (no plaintext,
  no binding), wrong-tunnel/body rejection, local LS2
  publication with protocol ack, typed tunnel-loss/direct
  rejection, liveness green.

crates/i2pr-daemon/tests/destination_tunnel_external.rs (new)
  Single fail-closed driver against exact-pinned i2pd 2.61.0.
  Strict profile, ordinary-path bootstrap, floodfill check,
  daemon session, reference SAM DATAGRAM destination via
  DEST GENERATE + explicit-PRIV session (standard client flow;
  only PUB length recorded, PRIV in-memory only, never logged),
  real one-hop builds both directions with replies routed to
  Installed roles, lease lookup/publication/destination send +
  DATAGRAM RECEIVED equality + inbound Garlic decrypt equality
  + sibling reasoning, direct rejection, liveness. Sanitized
  variant/kind/message-id/session-delta counters only; expired
  cells counted, never fatal. Currently stops at the §11 gate
  after recording every reachable row (see §11).

tests/integration/m6-interop/run-destination.sh (new)
  21 guarded rows: 3 local + 14 external (5 prelim + 3
  reference-log + 7 install-dependent with stop provenance) +
  workspace-gates. i2pd provisioned with notransit=false,
  floodfill=true (Plan 186) plus loopback SAM (Plan 187 only);
  ephemeral i2pd ports, fixed i2pr bind. The raw i2pd log stays
  in scratch (it carries SAM session lines) — only sanitized
  counts reach evidence. Stale evidence hygiene at startup.

scripts/check-destination-tunnel-evidence.sh (new)
  Static evidence-integrity checker: 21 guarded labels through
  record_guarded/m6_row/m6_key_row/ref_row/blocked_row only; a
  literal `record "<label>" passed` line fails the check.
```

## Evidence (Plan 187)

Implementation head: current local commit. All counters below
are command-derived.

Local lane (no external process):

```text
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
# 27 passed
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1
# 9 passed
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
# 7 passed
bash scripts/check-destination-tunnel-evidence.sh
# destination evidence check passed (21 guarded labels)
```

External lane (exact-pinned i2pd, loopback, unmodified):

```text
bash tests/integration/m6-interop/run-destination.sh
# lane fails closed with 7 blocked rows (see below); sanitized evidence:
#   target/interop/m6-destination-evidence/evidence.json
#   target/interop/m6-destination-evidence/evidence.md
#   target/interop/m6-destination-evidence/driver/driver-evidence.tsv
#   target/interop/m6-destination-evidence/reference-facts.tsv
```

Lane rows (21 rows: 11 passed, 7 blocked, 3 local-passed counted above):

```text
local-destination-tunnel-unit      = passed
local-destination-tunnel-live      = passed
local-tunnel-liveness              = passed
external-daemon-strict-profile     = passed
external-reference-verified        = passed
external-reference-floodfill       = passed
external-session-established       = passed
external-sam-destination-created   = passed (dest_len=391, Plan 142 PUB 391/524 lock holds)
external-outbound-tunnel           = blocked (m6-build-reply-interop-gap)
external-inbound-tunnel            = blocked (m6-build-reply-interop-gap)
external-outbound-accepted         = passed (transit-endpoint-created >= 1)
external-inbound-accepted          = passed (transit-gateway-created >= 1)
external-reference-ls2-published   = passed (reference LeaseSet2 updated >= 1)
external-lease-lookup-tunnel       = blocked (m6-build-reply-interop-gap)
external-ls2-publication-tunnel    = blocked (m6-build-reply-interop-gap)
external-destination-outbound      = blocked (m6-build-reply-interop-gap)
external-reference-received        = blocked (m6-build-reply-interop-gap)
external-destination-inbound       = blocked (m6-build-reply-interop-gap)
external-direct-rejected           = passed
external-liveness-first-test       = passed
workspace-gates                    = passed
```

Driver evidence keys (sanitized; no secrets — representative run):

```text
daemon-strict-profile        = true
reference-routerinfo-verified = true
reference-bootstrap-store    = 1
reference-floodfill-capable  = true
i2pr-routerinfo-len           = 651
session-established          = 1
sam-destination-created      = dest_len=391
outbound-build-emitted       = true
inbound-build-emitted        = true
install-pump-summary         = build_reserved=19..30 other=0
                               tunnel_data=0 router_control=0
                               unsupported={11: 6..9, 19: 1}
                               installed_ob=0 installed_ib=0
                               non_install=0 dispatch_error<=12
                               kind_reply=0 kind_other_build=19..30
                               msgid_match=1
session-pump-delta           = datagrams_received=27 i2np_received=26
                               protocol_drops=0 auth_failures=0
                               queue_drops=0
direct-rejected              = true
liveness-first-test          = passed
build-reply-gap-stop         = installed_ob=0 installed_ib=0 kind_reply=0
shutdown-baseline            = true (success path only)
```

Reference (unmodified):

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
profile = notransit = false, floodfill = true, SAM loopback (Plan 187 only)
bind = 127.0.0.1 ephemeral loopback only
advertise = false (i2pr)
```

## Plan 187 §11 stop: the build-reply interop gap

Fired after six independent lane runs plus isolated manual
reproductions. The finding is narrow and fully evidenced:

1. The authenticated SSU2 session establishes every run
   (`session-established = 1`); the session is lossless
   (`datagrams_received=27`, `i2np_received=26`,
   `protocol_drops=0`, `auth_failures=0`, `queue_drops=0`), so
   nothing is lost between the reference and the pump.
2. Both one-hop builds are accepted by the reference every run:
   `transit-endpoint-created >= 1` (our outbound) and
   `transit-gateway-created >= 1` (our inbound) in the
   reference's own structured log, with `Short request record N
   is ours` correlation. Our requests traverse the real session
   (delivery `Accepted`) and our 4-record multirecord shape
   (Plan 110 policy) parses on the reference.
3. The reference emits **zero** build-reply bodies of any known
   type within the bounded 30 s window: `kind_reply=0`, no
   classic/variable reply (`unsupported={11, 19}` only), no
   dispatch error that would indicate a malformed reply.
   Without a consumable reply the existing
   `ShortBuildStateMachine` cannot reach `Established`, so no
   real tunnel material installs and no install-dependent row is
   claimed.
4. What the reference does send instead, every run: ~19–30
   `ShortTunnelBuild` *requests* (it tries to build its own
   tunnels through us, its only peer; correctly unanswered —
   this router is not a transit hop), direct Garlic frames
   (type 11), and one direct TunnelGateway frame
   (`tunnel=38146 inner=11`, routing metadata only). None of
   these is a reply to our builds.
5. Second-order note for the corrective: `route_build_outcome`
   correlates replies by (peer, transport message id), which
   only works because the i2pr responder echoes the request id
   (see `exploratory_build_live.rs`). A reference that assigns
   its own reply ids would need tunnel-based correlation. This
   is unproven either way until a reply is observed; Plan 188
   owns the experiment.
6. Leading hypothesis for Plan 188 (not a claim): our fixed
   4-record multirecord padding may break the reference's reply
   construction even though record-level acceptance works.
   Alternatives: reply addressing (reply tunnel selection),
   reply timing, or a reference short-build-reply limitation in
   this configuration. No production wire change was made to
   chase any of these; that experiment belongs to Plan 188.

Per §11, no authentication was weakened, no unsupported crypto
accepted, no destination layer bypassed, and no install was
synthesized (in particular: creator-known keys were **not**
installed without a consumed reply — the lane stops instead).
The local destination plane (27 unit + 9 live rows, including
the full bidirectional ECIES/Garlic round-trip with sibling
isolation over real TunnelData cells) is unaffected and stays
green. Plan 188 owns the narrow build-reply corrective; the
Plan 187 external rows resume only after it produces installs.

## Reference SAM deviations recorded (no behavior change)

1. i2pd 2.61.0 emits I2P-alphabet destination tokens with RFC
   4648 `=` padding. The driver decodes through the Plan 142
   SAM wire codec (`i2pr-api::sam::base64`, frozen against
   i2pd/Java/i2plib vectors), which accepts this spelling. The
   filename-oriented `i2pr-netdb::base64` codec (`-?` alphabet,
   `~` padding) must never decode wire destinations; an early
   driver revision did and failed closed with `InvalidPadding`.
2. `SESSION STATUS` for a transient DATAGRAM session returns a
   `DESTINATION=` blob that concatenates the 391-byte public
   destination with ~290 further bytes of unknown (possibly
   secret-adjacent) material and no separator. The driver never
   parses it: sessions are created from an explicit DEST
   GENERATE private destination (standard SAM client flow),
   and the blob is never logged or copied. The raw i2pd log is
   never evidence for the same reason (it also logs full
   `DEST REPLY PRIV=` lines); only sanitized counts reach
   evidence, enforced by harness hygiene that deletes stale
   `i2pd.log`/TSV files at startup.
3. Transient Ed25519 destinations are 391 bytes (Plan 142 PUB
   391/524 lock holds: `dest_len=391`).

## Stop conditions

The §11 build-reply stop fired as documented above. No other
Plan 187 §11 stop fired:

- no second Garlic/decrypt/routing stack (all ECIES/Garlic
  flows reuse `i2pr-client` + the Plan 127 seams);
- no SAM/I2CP/service-tunnel shortcut in the counted path;
- no `LocalZeroHop` for counted rows (rejected by
  `verify_remote_material`; the external lane never calls the
  zero-hop seam);
- no direct-transport substitution (always rejected, counted);
- no secret-bearing evidence (counts/hashes/bounds only;
  PRIV in-memory only; raw i2pd log excluded from evidence);
- no Plan 184/185/186/M8/M9/M10 regression (full workspace
  floor green; every static boundary script green).

## Known limitations

- One-hop destination tunnels only; no multi-hop tunnel build.
- The external install gate needs Plan 188; until then §10
  criteria 3/5/6/7 have local (two-role) but not mixed-router
  evidence. No LeaseSet2-over-tunnels or destination-traffic
  interop is claimed.
- Fixed i2pr SSU2 bind (`I2PR_SSU2_DESTINATION_PORT`, default
  44084); i2pd ports are ephemeral per run.
- The concurrent-lookup ceiling documents the same
  single-active-seam reality as Plan 186 (coordinator ceiling
  is an upper bound, not 8 live lookups).
- No Streaming claim; Plan 188 owns build replies first, then
  the Plan 187 external rows resume, then Streaming layers on
  top per the §12 handoff.

## Handoff

Plan 188 is the narrow short-build-reply interop corrective.
It may begin immediately; the Plan 187 local suites plus the
pre-install external rows are its regression floor. Plan 188
passes only when installs complete through consumed reference
replies and the seven blocked rows above flip to passed
without weakening authentication, acceptance correlation, or
evidence hygiene.

```text
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective (local rows passed; remote lane pending)
plan_188 = blocked-by-plan190-reply-path-corrective (real outbound/inbound i2pd installs retained-passed)
plan_187 = blocked-by-m6-build-reply-interop-gap (local rows passed; 2/7 flipped via plan188 installs)
m6_destination_local_product = passed-via-plan187
m6_destination_remote_interop = installs-proven-lookup-pending-plan190-external-run
next_executable_plan = 188 (continue external lane with plan190 reply-path correction)
```
