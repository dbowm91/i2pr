# Plan 290 status — Composed Proposal 170 tunnel-family parity

Status: **`passed-prop170-composed-tunnel-families`**.

Plan of record: [`290-composed-tunnel-family-parity.md`](../../implementation/i2pcontrol-proposal-170/290-composed-tunnel-family-parity.md).

Hard dependency closed: Plan 289 (`passed-prop170-tunnelmanager-control-state-and-existing-service-adapter`).

## Implementation commit

- `303ce8b` — `plan(290): composed Proposal 170 tunnel families (connectclient/socksirc/httpserver/httpbidirserver)`
  (four composed backends under the one M10 `ServiceTunnelManager`,
  runtime-neutral `connect.rs`/`socks4a.rs`/`http/server.rs` plus
  `MethodNotAllowed`, shared SOCKS version-peek negotiator, SOCKS→IRC
  handoff, filtered HTTP server/bidir executors, ten-family control
  mapping with per-kind option applicability, contract/unit/product/wire
  tests, arch docs, `support.toml` surface).

This closure commit (closure record + registry + roadmap) lands on top
of `303ce8b` with no production-code change.

## Requirement-to-evidence matrix

| Plan 290 requirement | Evidence |
|---|---|
| `connectclient` as its own kind with own option applicability and lifecycle identity over shared primitives | `ServiceTunnelKind::ConnectClient` (`connect-client` spelling); `ConnectClientOptions` (allowed-port set, 443 default); `connect_options` mandatory-for/gated-to the kind; `run_connect_only_connection`/`run_connect_client_loop` (CONNECT-only admission, 405/403 discipline, shared pump relay); `service_tunnel_connect_client_product` 7 tests (roundtrip digest, 405, 403, clearnet/unknown/malformed, siblings) |
| `socksirc` composes SOCKS negotiation with the IRC filter, no raw bypass | `ServiceTunnelKind::SocksIrc` (`socks-irc`); `run_socks_irc_loop` (shared version-peek negotiator, target selection, `run_irc_filtered_loop` with `initial_inbound` handoff); `service_tunnel_socks_irc_product` 7 tests (register+ACTION roundtrip, DCC drop, raw bypass impossible, unknown server command dropped, 4a parity, per-version port reply, siblings) |
| SOCKS4a/SOCKS5 pinned parity, literal-IP/direct-clearnet fail-closed | `socks5/socks4a.rs` (version `0x04`, CONNECT only, `0.0.0.x` marker, bounded USERID 255, retained 520, grant/refuse 8-byte replies, plain IPv4 fail-closed); shared `negotiate_socks_destination` version peek; `socks_irc_socks4a_negotiation_parity` + per-version port-reply tests |
| `httpserver` over the persistent server destination with a runtime-neutral server privacy contract, no client-parser conflation | `ServiceTunnelKind::HttpServer` (`http-server`, `is_server`, no listener/destination); `http/server.rs` (`filter_server_request/response`: origin-form only, required `Host` replaced with the loopback target, `Transfer-Encoding` rejection, hop-by-hop + identifying strip, forced close, no peer-identity injection); `run_http_server_connection` (slowloris-bounded head, paced multi-segment body, filtered response admit, remainder relay); `service_tunnel_http_server_product` 8 tests (GET roundtrip, absolute-form/chunked/no-Host/overlong-head 400s, 4096 B POST digest, restart identity, siblings) |
| `httpbidirserver` composes both halves under one generation and one persistent identity, no second destination, no outproxy | `ServiceTunnelKind::HttpBidirServer` (`http-bidir-server`; listener+target required, dedicated, no remote destination; `http_options` mandatory); `run_http_bidir_loop` (loopback proxy listener + Streaming SYN poll, one supervisor task/generation/identity, `run_http_connection` + `run_http_server_connection` reuse); `service_tunnel_http_bidir_product` 6 tests (server-half filter roundtrip, client-half alias proxy with origin-form rewrite, clearnet 403, restart identity, siblings, target-failure slot release) |
| Kind integration: enum, validation, diff, snapshots, dispatch, startup/reconcile | `config.rs` kind parse/spelling/`is_server`/validate (incl. `http_bidir_server_carries_both_halves`, `connect_client_requires_options`, `socks_irc_requires_both_option_sets` unit tests); `generation.rs` touch points; `ServiceRuntime` dispatch flags + `spec_is_server`; `run_service_loop` routing; control `map_tunnel_type` ten backends + `has_plan290_backend` + `build_control_spec` per-kind required fields |
| No bypass of permits/ownership/generation/target validation/delivery handle/secret policy | Aggregate + per-service permits on every accept path (incl. bidir both halves); Dedicated destination ownership (bidir single identity proven by restart tests); Plan 289 generation/drain machinery untouched and exercised by the new wire lifecycle test; loopback-only targets/listeners; `DeliverySweepCounters` clean (unknown_peer/missing_factory/delivery_failed zero in product runs); no secret-classified keys in new options |
| Shared policy extraction, no second routing stack | SOCKS negotiator shared between SOCKS5 and SOCKS-IRc; IRC filter shared between IRC client and SOCKS-IRc; HTTP client connection shared between HTTP client and bidir client half; HTTP server relay shared between HTTP server and bidir server half; delivery driver/bridge/streaming stack reused unchanged |
| Per-family evidence: config mapping, lifecycle, data path, malformed/slow/oversized, isolation, ceilings, cancellation/half-close/target failure, generation, identity, no clearnet, filters | Connect/SOCKS-IRc/HTTP-server/bidir product suites (28 tests) + `tunnel_plan290_family_lifecycle_over_wire` (create/get/stop/start/delete × 4 kinds with exact Proposal spellings + required-field negatives) + `backend290_count`/`plan290_family_mapping_ten_backends` + target-failure slot-release test + restart-identity tests + sibling-isolation tests |
| Existing M10 suite unchanged | Full workspace floor green with no M10 behavior change (additive only; see Migration) |

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK
cargo test --locked --workspace --all-targets -- --test-threads=1          3391 passed, 0 failed
  (delta over Plan 289 floor 3330: +61 across the four family suites, wire lifecycle, contract count, and unit vectors)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed
cargo test --locked -p i2pr-daemon --test service_tunnel_connect_client_product    7 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_socks_irc_product         7 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_http_server_product       8 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_http_bidir_product        6 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels                       8 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product              15 passed (client-half shared path intact)
cargo test --locked -p i2pr-daemon --lib                                      329 passed
cargo test --locked -p i2pr-i2pcontrol --test contract                             13 passed (incl. backend290_count == 10)
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
(`declare -A`); this host ships bash 3.2.57 only. None of their areas
are touched by this plan beyond the service-tunnel executor surface
(covered by the workspace floor and the service-tunnel boundary
checker above); Linux CI is authoritative for those six.

Timing-note (recorded, not a product defect): three full-workspace
runs each showed a single failure in
`socks_irc_unknown_server_command_is_dropped`
(`read_line_bounded` 2 s wall-clock test timeout, never a product
assertion) while the host carried load averages 11–20 from unrelated
processes (a second-project `rustc` build, editor/agent sessions).
The same binary passes solo repeatedly (3/5 solo runs plus every
isolated re-run green; failing runs take ~4.5 s vs ~2.5 s passing,
exactly one fired 2 s timeout). The final full-floor run above is
3391 passed / 0 failed. CI runs each test binary separately, which is
the supported loopback procedure per repo guidance.

## Acceptance criteria

Plan 290 closes when connectclient, socksirc, httpserver, and
httpbidirserver are real bounded backends under the existing
`ServiceTunnelManager`, the ordinary socks profile has the required
pinned protocol parity, and all lifecycle actions from Plan 289 work
on them. All hold with executed evidence above. Ten of the twelve
Proposal tunnel families have real backends; the two Streamr families
remain exclusively Plan 291 scope.

## Defects found during implementation (all corrected)

- Destructive-drain batch drop in `read_streaming_head`
  (`service_tunnels_http_server.rs`): the head scan returned from
  inside the per-unit drain loop after the first unit, silently
  discarding same-batch body units the manager had already removed.
  Multi-segment POST bodies stalled at exactly one segment (1664 of
  4096 bytes) while the peer correctly counted everything delivered
  (`last_ack=3`, `unacked=0`, three `Delivered` decisions, empty
  pending). Fixed by accumulating the whole drain batch before
  scanning for the terminator; siblings audited (IRC loop
  accumulates, pump writes every unit, bidir/connect reuse the fixed
  or pump paths, socket-sourced handoffs are re-readable). Regression
  covered by `http_server_post_body_is_paced_with_digest`.
- Same file: the documented slowloris head deadline was unenforced
  (`started` unused). Fixed with the `HEAD_READ_DEADLINE` check
  matching `forward_body_to_target` and the module contract.
- Clippy `-D warnings` findings in new branch code (all corrected,
  no behavior change): collapsible `if let` chains, `single_match`
  to `if let`, lazy-continuation doc indent, unused binding; plus
  `rustfmt` on the new test/executor files.

## Deliberate deviations (recorded, not defects)

- `HttpBidirServer` reports client-side in `is_server()` because it
  always carries a loopback listener; its server half is visible
  through the dedicated destination identity, and the daemon builds
  both a listener and a server target for it.
- `forward_body_to_target` drops bytes beyond `content_length`
  within a drain batch (close-delimited, no pipelining; the relay
  terminates on post-request bytes by design).
- Server-half target failure tears down silently (BAD gateway
  outcome, failure counter, slot release) mirroring the generic
  server path; no synthetic error page is injected toward I2P.
- `serde_json::Map` lexicographic order deterministic (unchanged
  from Plan 289).

## Migration / compatibility

Additive only: three new executor modules
(`service_tunnels_http_server.rs`,
`service_tunnels_http_bidir.rs`,
`service_tunnels_socks_irc.rs`), three runtime-neutral modules
(`connect.rs`, `socks5/socks4a.rs`, `http/server.rs`), four product
test files, one wire lifecycle test, kind/option/control extensions.
All six M10 families and every Plan 287–289 behavior unchanged
(http/socks5/irc suites intact). No config file format change; new
kinds parse from the same spec schema.

## Security review

- No secret in new surface: `ConnectClientOptions` (ports),
  `Socks5ClientOptions`, `IrcClientOptions`, `HttpClientOptions`
  carry no secret-classified keys; error/reason strings static or
  bounded; no `Debug` on secret types added.
- No second runtime/routing: all four families ride the one
  `ServiceTunnelManager`, delivery driver, bridge, and Streaming
  stack; permits, Dedicated ownership, generation/drain, and
  loopback-only validation apply identically.
- No clearnet escape: CONNECT/socks/http-proxy authorities resolve
  I2P-only (IP literals, clearnet, mixed-suffix, userinfo,
  non-HTTP schemes rejected with typed 403/400/502); server halves
  never dial out (persistent destination, loopback target only);
  `httpbidirserver` has no outproxy capability by construction.
- No peer-identity injection: the server filter replaces `Host`
  with the loopback target and strips identifying/hop-by-hop
  material; `Via`/`Server` stripped both directions.

## Documentation / operational evidence

- `docs/architecture/i2pr-service-tunnels.md`: `config` ten-kind
  row, `connect`/`socks4a`/`http/server` rows, four daemon
  executor rows.
- `docs/architecture/i2pr-daemon.md`: `service_tunnels.rs`
  dispatch row, per-executor rows, `i2pcontrol_tunnels.rs`
  ten-family row.
- `docs/architecture/i2pr-i2pcontrol.md`: twelve-type ten-backend
  row, contract-test row, 290 closure cross-reference.
- `specs/support.toml`: new `prop170.composed-tunnel-families`
  surface (`experimental`, `advertised = false`).

## Known limitations

- Only 10 of 12 Proposal types have backends; `streamrclient`
  and `streamrserver` fail explicitly as unsupported until
  Plan 291.
- The wider option matrix (per-type applicability beyond the
  7-option subset plus the Plan 290 option sets) belongs to
  Plans 292–293; supplied out-of-subset options still fail
  unsupported, never accepted inertly.
- Profile tuning beyond the pinned parity belongs to Plan 292.
- The six bash-4-only checkers remain unverifiable on this macOS
  host (environment debt, unchanged from Plans 286–289).

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the six
  bash-4-only checkers (environment debt); the drain batch-drop
  (found and fixed in the implementation commit, regression
  covered); the timing-sensitive socks_irc line-read under host
  load (test-harness wall clock, green on retry and in the final
  floor run; CI runs binaries separately).

## Roadmap disposition

Plan 290 is **closed** (`passed-*`). Plan 291 stays `ready`
(parallel branch, unaffected). Plan 292 stays blocked on 290 + 291
(290 now closed; 291 still open). Plans 293/295 unchanged. No
other Proposal 170 capability is claimed.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 291 (hard dep: Plan 289, now closed; parallel with 290) —
  already `ready`; unaffected.
- Plan 292 (hard deps: Plans 290 + 291; 290 now closed, 291 still
  open) — remains blocked.
- Plan 293 (hard dep: Plan 292) — remains blocked.
- Plan 294 (hard dep: Plan 287 only) — already `ready`; unaffected.
- Plan 295 (hard deps: Plans 288 + 293 + 294) — remains blocked
  (293 + 294 still open).
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 290 acceptance criterion
passes with executed evidence above.
