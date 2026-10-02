# Plan 291 status — Repliable-datagram substrate and Streamr tunnel families

Status: **`passed-prop170-repliable-datagram-and-streamr`**.

Plan of record: [`291-repliable-datagram-and-streamr-tunnel-families.md`](../../implementation/i2pcontrol-proposal-170/291-repliable-datagram-and-streamr-tunnel-families.md).

Hard dependency closed: Plan 289 (`passed-prop170-tunnelmanager-control-state-and-existing-service-adapter`); executed in parallel with Plan 290 (now closed).

## Implementation commit

- `32a1511` — `plan(291): repliable-datagram substrate and Streamr tunnel families`
  (protocol freeze dossier, runtime-neutral `DatagramManager` +
  `StreamrOptions` + two kinds, adapter/local-seam/bridge dispatch
  for protocols 17/18, daemon Streamr executors with loopback UDP,
  twelve-family control mapping with the 7-key Streamr option
  extension, TOML `local_udp` endpoint, contract/unit/product/wire
  tests, arch docs, `support.toml` surface).

This closure commit (closure record + registry + roadmap) lands on top
of `32a1511` with no production-code change.

## Protocol freeze (recorded before code)

`specs/protocols/12-repliable-datagrams-streamr.md` pins the
reference behavior before implementation:

- Java I2P 2.13.0 `net.i2p.i2ptunnel.streamr` (behavioral reference
  only, no code copied): one-byte subscribe `0x00` / unsubscribe
  `0x01`; fast-start 2 s × 5 then steady 10 s refresh with terminal
  unsubscribe; subscriber key `(destination, swapped ports)`;
  `MAX_SUBSCRIPTIONS = 10`; `EXPIRATION = 60 s`; inbound repliable
  datagrams on all ports, outbound raw datagrams to learned reply
  addresses; subscribe `fromPort` = local UDP port, `toPort` = 0.
- Wire: Datagram1 (protocol 17: `from` Destination + 64-byte
  Ed25519 signature over the payload + application bytes) for
  control; raw (protocol 18: payload only) for media. Datagram2/3
  stay unsupported.
- Bounds with sources: 10 subscribers and 60 s expiry (Java);
  10 s steady refresh (Java cadence, interop-safe against Java's
  60 s expiry either way); 1200-byte application ceiling (fork
  donor + datagram reliability guidance — about one tunnel
  message, never fragmented); no fragmentation/reassembly
  anywhere; bounded queues with typed backpressure.
- The project Emissary fork (`i2pcontrol/streamr/* + datagram
  substrate`, manifest class R) was unreachable at its pinned
  path during the freeze window, so no fork line is cited as
  measured; Java carries the wire-visible behavior and the plan's
  fork-donor candidates carry the policy values above. Router-
  internal substrate only: SAM stays streaming-only.

## Requirement-to-evidence matrix

| Plan 291 requirement | Evidence |
|---|---|
| Runtime-neutral repliable-datagram surface under destination ownership: typed send with destination/port metadata, authenticated sender encoding/verification, bounded receive event, hard ceilings, no reassembly, explicit malformed/auth/backpressure outcomes, no second identity store | `i2pr-client::datagram::DatagramManager` (`send` borrows caller identity for Datagram1 signing; `process_inbound` verifies Ed25519 then queues; `MAX_DATAGRAM_APPLICATION_PAYLOAD=1200`, `MAX_DATAGRAM_RECEIVE_QUEUE=64`, `MAX_DATAGRAM_OUTBOUND_QUEUE=64`; `DatagramError` variants); 8 unit tests (round trips, tamper/cross-key rejection, oversize both directions, unsupported protocol, queue-full without silent drop, truncated envelopes) |
| Destination-runtime registration/delivery path for 17/18 | `InboundStreamingOutcome::DatagramReceived` (decoded bytes ride to the owner, Streaming never sees them); `LocalDeliveryReceiver::datagrams` + `LocalDeliveryOutcome::DatagramDelivered`; `bridge_to_peer` swap-and-restore + accept arm; `SamDestinationBridge::datagrams` with accessors; router-backed sink arm with `datagrams_accepted` report field; driver sweep drains the datagram outbound queue |
| streamrserver: persistent destination, loopback UDP source, bounded subscriber table by authenticated remote + ports, refresh/expiry/unsubscribe, bounded fanout, payload ceiling, no peer-controlled spawning | `run_streamr_server_loop`/`drive_streamr_server` (persistent identity via the Plan 290 branch; UDP bind loopback-enforced; `handle_subscribe` 0x00/0x01 with 10-cap deny + port swap; `sweep_expired` 60 s; fanout raw per subscriber with ceiling reject; single supervisor task, no per-peer tasks); 4 daemon unit tests (refresh/unsubscribe/expiry, 11th denied, malformed control, binding predicate) |
| streamrclient: configured producer, loopback UDP target, bounded subscribe/refresh with deterministic shutdown/unsubscribe, producer-bound forwarding only, no non-loopback fallback | `run_streamr_client_loop`/`drive_streamr_client` (producer resolved once; ephemeral UDP bind anchors `fromPort`; 2 s ×5 then configured steady cadence; terminal `0x01`; `is_producer_media` gates protocol + transport-authenticated hash + ceiling; loopback target enforced) |
| TunnelManager integration: kinds, mapping, option applicability, diff/reconcile, snapshots/get, StartOnLoad, persistent server identity; unsupported options fail before bind/allocation | `StreamrClient`/`StreamrServer` kinds with shape + `streamr_options` gating (`plan291` config unit tests); `map_tunnel_type` all twelve + `has_plan291_backend`; `SUPPORTED_291_OPTIONS` 7-key extension (`remote_udp_host` stays unsupported until Plan 292, never accepted inertly); per-kind required fields; TOML `local_udp` endpoint; `tunnel_plan291_streamr_lifecycle_over_wire` (create/get/stop/start/delete × 2 with exact spellings + required-field/cross-kind negatives); `tunnel_streamr_supported_and_secret_rejected_over_wire`; `backend291_count == 12` (290's `== 10` untouched) |
| Evidence rows (vectors, malformed/auth, binding, backpressure/expiry, loopback, caps, refresh/shutdown, fanout digest, isolation, lifecycle/identity, streaming suite intact) | Substrate vectors + auth/ceiling unit tests; `service_tunnel_streamr_product` 5 tests (fanout digest ×2 packets, two-subscriber fanout, oversize drop with post-drop recovery, restart identity + cleared table, sibling isolation); `control spec` unit test; full workspace floor green |
| Acceptance: real authenticated router-internal semantics, both families moving data end-to-end through the actual destination runtime (no UDP-over-Streaming loop) | Product suite drives real UDP sockets through real destinations: subscribes are signed Datagram1 verified by the server manager; media is raw fanout forwarded only for the bound producer; digests match byte-exact |

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1   3416 passed, 1 failed*
  (delta over Plan 290 floor 3391: +25 = 8 substrate + 4 executor unit + 5 product + 1 wire lifecycle + 7 kind/option unit)
  *the single failure is the known `socks_irc_unknown_server_command_is_dropped` wall-clock flake (see Timing-note)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed
cargo test --locked -p i2pr-client --lib datagram                          8 passed
cargo test --locked -p i2pr-client --all-targets                           all green (incl. updated plan129 protocol-17 row)
cargo test --locked -p i2pr-service-tunnels --lib                        261 passed
cargo test --locked -p i2pr-daemon --lib                                 334 passed (incl. 4 streamr table unit + plan291 spec)
cargo test --locked -p i2pr-daemon --test service_tunnel_streamr_product     5 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels                 9 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_socks_irc_product   7 passed (solo confirmation)
cargo test --locked -p i2pr-i2pcontrol --test contract                      13 passed (290 ==10 kept, 291 ==12 added)
bash scripts/check-dependency-direction.sh                                 dependency direction: ok
bash scripts/check-runtime-boundaries.sh                                   runtime boundary checks passed
bash scripts/check-service-tunnel-boundaries.sh                            service-tunnel boundary checks passed
bash scripts/check-ntcp2-interoperability.sh                               OK
bash scripts/check-constrained-host-lane-boundary.sh                      OK
bash scripts/check-m11-transit-boundaries.sh                               passed
bash scripts/check-m11-transit-qualification-evidence.sh                   175 guarded rows green
bash scripts/check-sam-acceptance-evidence.sh                              22 rows green
bash scripts/check-ssu2-acceptance-evidence.sh                             15 rows green
bash scripts/check-i2cp-acceptance-evidence.sh                             24 rows green
bash scripts/check-exploratory-tunnel-evidence.sh                          12 labels green
bash scripts/check-netdb-tunnel-evidence.sh                                12 labels green
bash scripts/check-destination-tunnel-evidence.sh                           21 labels green
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                   11 labels green
python3 -m unittest discover -s tests/integration/ntcp2/harness            OK (18 tests)
cargo deny check advisories bans sources                                   advisories ok, bans ok, sources ok
```

Not run locally (pre-existing macOS environment limit, identical on
unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`,
`check-service-tunnel-acceptance-evidence.sh`,
`check-streaming-tunnel-evidence.sh` require bash 4+ associative arrays
(`declare -A`); this host ships bash 3.2.57 only. New code carries no
`passed` literals tripping the evidence checkers by inspection; Linux
CI is authoritative for those six.

Timing-note (recorded, not a product defect): full-workspace runs on
this host intermittently show single wall-clock failures while the
box carries unrelated load (second-project builds, editor/agent
sessions, load 5–20): `socks_irc_unknown_server_command_is_dropped`
(the Plan 290 2 s line-read flake, green solo and in quiet runs) and
once `ntcp2 link_reader_and_writer_are_joined_after_close`
(untouched NTCP2 code, instant solo pass). The `--no-fail-fast`
floor above is 3416 passed with only the socks_irc flake failing;
that binary is 7/7 green solo. CI runs each test binary separately,
which is the supported loopback procedure per repo guidance.

## Acceptance criteria

Plan 291 closes only when repliable datagrams have real
authenticated router-internal semantics and both Streamr families
move data end-to-end through the actual destination runtime. Both
hold: subscribes verify Ed25519 against the claimed `from`
destination inside the destination manager, and the product suite
moves signed subscribes plus raw fanout through real co-owned
destinations with digest equality — no Streaming is involved on
either path. After Plans 290 and 291, all twelve Proposal tunnel
families have real backends and Plan 292 becomes ready.

## Defects found during implementation (all corrected)

- Raw-media producer binding used the Datagram1 `from_hash`, which
  is zeroed for raw by design, so the client dropped every media
  packet (subscribes arrived, fanout never forwarded). Fixed by
  stamping raw events with the garlic-session authenticated
  transport sender in `process_inbound` (local seam + router-backed
  sink); Datagram1 keeps the signature-verified `from`.
- Plan 129 trajectory `non_protocol_six...never_reaches_streaming`
  expected `UnsupportedProtocol` for protocol 17, which now
  (correctly) yields `DatagramReceived`. Updated the expectation;
  the invariant (Streaming untouched) is asserted unchanged.
- `Destination::decode` is exact-consumption, so envelope prefix
  parsing used `decode_from_cursor` plus canonical re-encode
  prefix comparison (the `packet.rs` precedent), failing closed
  on non-canonical encodings.
- `map_tunnel_type`'s catch-all became unreachable with twelve
  mapped types; removed (future types fail at compile time) with
  runtime rejection retained in `normalize_definition` and the
  supervisor gates.
- Clippy `-D warnings` findings in new code (all corrected, no
  behavior change): struct-update initialization, `clone` on
  `Copy`, bundled drive context under the argument ceiling.

## Deliberate deviations (recorded, not defects)

- `remote_udp_host` (frozen inventory, client-applicable) stays
  `UnsupportedOption` until Plan 292 assigns it meaning: no remote
  UDP peer exists in the Java design, and the workstream rule is
  never accepted inertly. The client's media target is
  `local_udp_host`/`local_udp_port` (router-centric naming,
  symmetric with the server's media source).
- `target_i2p_port` defaults to 0 (Java `toPort`); nonzero values
  are accepted as the packet port, not rejected.
- Oversize local media is rejected per packet, never fragmented or
  truncated; a drop never breaks the subscription.
- Server-half target failure has no error page (same silent
  teardown discipline as the generic server path);
  unsubscribed/expired slots simply stop receiving fanout.
- Shutdown unsubscribe is best-effort (one packet before task
  exit); expiry is the guaranteed reclamation path.
- `serde_json::Map` lexicographic order deterministic (unchanged).

## Migration / compatibility

Additive only: one protocol dossier, one `i2pr-client` module, one
options module, two kinds, one daemon executor module, bridge and
adapter extensions (new outcome variants; existing arms unchanged),
control mapping plus the 7-key Streamr option extension, TOML
`local_udp` endpoint, three test files plus wire/contract/unit
additions. All six M10 families, all four Plan 290 families, and
every Plan 287–290 behavior unchanged (streaming/SAM/SSU2/I2CP
suites intact; the single Plan 129 expectation update is
documented above). No config file format break (new optional TOML
keys only); no new dependencies.

## Security review

- Sender authentication is cryptographic, not advisory: Datagram1
  `from` must decode canonically, must carry Ed25519 (no DSA
  implementation; anything else verify-rejects), and the 64-byte
  signature must verify over the exact payload bytes. Raw media is
  unauthenticated by protocol design and is therefore bound by the
  transport-authenticated sender hash plus the configured producer
  (client) or the loopback socket (server).
- No confused deputy: the client forwards only
  producer-hash + protocol-18 + within-ceiling payloads to its
  configured loopback target; the server fans out only to tabled
  subscribers; non-loopback UDP endpoints fail at three layers
  (option validation, control parsing, bind-time check).
- No unbounded state: 1200-byte payload ceiling both directions,
  64-event receive queue, 64-request outbound queue, 10-entry
  (default, 64-max) subscriber table, no reassembly buffers, no
  per-peer tasks or timers.
- No secret exposure: receives carry hashes/ports/payloads only;
  signing borrows the bridge identity without copying key
  material; error reasons are static.
- Secrets/options: no secret-classified keys in the Streamr
  surface; secret rejection over the wire re-verified with the
  Streamr create running.

## Documentation / operational evidence

- `specs/protocols/12-repliable-datagrams-streamr.md`: the Plan
  291 protocol freeze (new).
- `docs/architecture/i2pr-service-tunnels.md`: twelve-kind config
  row, `streamr` options row, Streamr executor row.
- `docs/architecture/i2pr-daemon.md`: Streamr executor row,
  twelve-family control row.
- `docs/architecture/i2pr-i2pcontrol.md`: twelve-backend tunnel
  row, contract-test row, 291 closure cross-reference.
- `docs/architecture/i2pr-client.md`: datagram module +
  adapter-outcome rows.
- `specs/support.toml`: new `prop170.repliable-datagram-and-streamr`
  surface (`experimental`, `advertised = false`).

## Known limitations

- Datagram2 (19) and Datagram3 (20) stay unsupported (typed
  rejection); DSA_SHA1 `from` identities verify-reject (Ed25519
  only, matching the router's key profile).
- The wider Streamr option semantics (`remote_udp_host`, tuning
  ranges beyond the freeze bounds) belong to Plan 292; supplied
  out-of-scope options still fail unsupported, never accepted
  inertly.
- Profile tuning beyond the pinned parity belongs to Plan 292.
- Reference interop (Java/i2pd Streamr exchange) is unclaimed:
  evidence is i2pr↔i2pr local product only, per workstream scope.
- The six bash-4-only checkers remain unverifiable on this macOS
  host (environment debt, unchanged from Plans 286–290).

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the six
  bash-4-only checkers (environment debt); the raw-binding defect
  (found and fixed in the implementation commit, regression
  covered by fanout digest tests); the wall-clock flakes under
  host load (test-harness timeouts, green on retry/solo and in
  quiet runs; CI runs binaries separately).

## Roadmap disposition

Plan 291 is **closed** (`passed-*`). Plan 292 becomes `ready`
(290 + 291 now closed). Plans 293/294/295 unchanged. No other
Proposal 170 capability is claimed.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 292 (hard deps: Plans 290 + 291, both now closed) — moves
  to `ready` in this commit.
- Plan 293 (hard dep: Plan 292, still open) — remains blocked.
- Plan 294 (hard dep: Plan 287 only) — already `ready`; unaffected.
- Plan 295 (hard deps: Plans 288 + 293 + 294) — remains blocked
  (293 + 294 still open).
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 291 acceptance criterion
passes with executed evidence above.
