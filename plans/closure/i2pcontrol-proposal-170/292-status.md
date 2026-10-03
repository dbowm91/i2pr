# Plan 292 status — Exact tunnel option matrix and non-cryptographic runtime completion

Status: **`passed-prop170-tunnel-option-matrix-and-noncrypto-completion`**.

Plan of record: [`292-tunnel-option-matrix-and-noncrypto-runtime-completion.md`](../../implementation/i2pcontrol-proposal-170/292-tunnel-option-matrix-and-noncrypto-runtime-completion.md).

Hard dependencies closed: Plans 290
(`passed-prop170-composed-tunnel-families`) and 291
(`passed-prop170-repliable-datagram-and-streamr`).

## Implementation commits

- `cc2fac1` — `plan(292): machine-readable option matrix with
  dispositions; register 296/297 residuals` (matrix module with
  336-cell census, residual plans 296/297).
- `d7cebd0` — `plan(292): tunnel length/quantity shaping with pool
  projection` (`TunnelShaping`, control parsing, diff).
- `e377e34` — `plan(292): cargo fmt`.
- `1f7e46a` — `plan(292): interactive streaming profile with
  window selection` (`StreamingConfig::interactive`, bridge
  threading).
- `4e71819` — `plan(292): idle deadline sweep with close/rebuild/reduce
  actions` (`IdlePolicy`, pure decision, runtime watermarks,
  5 s sweeper wiring).
- `8a99b18` — `plan(292): proxy-auth, access lists, presentation
  gates, unique-local dial, streamr sink` (proxy authentication +
  access lists + presentation gates + unique-local dial +
  Streamr sink redirect with control owner arms, diff classes,
  boundary/product/wire evidence, spec 13 dossier, support
  surface, clippy/style hardening).

This closure commit (closure record + registry + roadmap) lands on top
of `8a99b18` with no production-code change.

## Matrix authority (recorded before the remaining code)

`crates/i2pr-i2pcontrol/src/tunnel_matrix.rs` records exactly one
disposition per applicable cell over the frozen `TUNNEL_OPTIONS`
inventory: 336 applicable cells with `APPLY_CELLS = 227`
(named runtime/persistence owners, never storage mirrors),
`NOT_APPLICABLE_CELLS = 37` (uniform refinement rule: kinds
without the consuming TCP/HTTP/streaming/UDP layer),
`BLOCKED_293_CELLS = 30` (dynamic SigType, LeaseSet security,
outproxy provider), `CORRECTIVE_296_CELLS = 39` (pool
backup-quantity/variance, multihoming, reply bundling),
`CORRECTIVE_297_CELLS = 3` (server `use_ssl` local TLS identity).
Rejected keys name their owning plan; nothing is accepted
inertly (the ownerless `other` arm rejects before allocation).

`specs/protocols/13-tunnel-option-matrix.md` is the interpretation
dossier: shaping bounds, interactive windows, idle pairing, proxy
realms and 407/RFC 1929 discipline, access-list union/deny-wins
semantics, helper/jump wire classes with exact-match rules and
priority, unique-local mapping with the counted platform
fallback, sink assembly, and the targets/multihoming boundary
(TOML-only failover, no Proposal wire key, session multihoming
stays 296).

## Requirement-to-evidence matrix

| Plan 292 requirement | Evidence |
|---|---|
| Matrix recomputed for i2pr with cardinality and named blocked identities; no option silently disappears | `tunnel_matrix.rs` + `tests/tunnel_matrix.rs` 6 passed (census 227/37/30/39/3 asserted, in-mask coverage, residual plan identities); contract suite 13 passed |
| Shaping with real pool projection, bounds at both layers, explicit rebuild semantics, no bypass of ceilings | `TunnelShaping` (length 1..=3, quantity 1..=6, single-length agreement or `ContradictoryOptions`); `DestinationConfig::from_service_shaping`; `ReplaceDestination` diff pin; `tunnel_plan292_shaping_lifecycle_over_wire` |
| Profile mapping with real streaming-window effect | `StreamingConfig::interactive()` (16/16, 32 unacked, 100 ms ACK); `streaming_interactive` diff `ReplaceDestination`; `plan292_profile_selects_streaming_windows` |
| Idle reduce/close/new-destination with real ownership, stated stability, running-edit semantics | Pure `idle_decision` (close > rebuild > reduce, exact-deadline fire, saturating); `IdlePolicy` pairing rules; runtime watermarks + 5 s sweeper through stop/restart; `MutableInPlace` diff pin; sweep unit tests |
| Proxy authentication with real listener effect and secret discipline | `ProxyCredentials` (per-realm SHA-256, `$i2pr1$` marked stored form, constant-time verify, redacted `Debug`); 407 + realm challenge on HTTP/CONNECT; `05 02` + RFC 1929 on SOCKS/SocksIrc; SOCKS4a refused without downgrade; both-halves-required; `service_tunnel_proxy_auth_product` 5 passed (no-auth/wrong/correct matrix per family + 4a) |
| Bounded access controls with real pre-SYN effect | `ServerAccessPolicy` (canonical hashes only, union/deny-wins, values never echoed); shared accept-path gate with `access_denied` counter; `service_tunnel_access_product` 3 passed (stranger-allow rejects with counter, stranger-deny admits with echo, parse pin); member-admit + deny-wins unit-pinned; admit path runs under every M10 roundtrip |
| Presentation gates only through I2P-routed policy (helper/jump), UseSSL with explicit identity policy | `HttpServerPolicy` (defaults open, closed gates refuse 403 pre-filter without touching the target); `classify_presentation` exact classes (13 unit pins incl. near-miss and priority); `HttpErrorKind::PresentationRefused`; `Forbidden` outcome; kind gates reject non-HTTP kinds; `service_tunnel_http_server_product` 10 passed (2 new gate tests: 403 + fixture silence + open-class forwarding); `use_ssl` stays 297-corrective |
| Unique-local-address and multihoming/reply-bundling where applicable; client/server bind and target policies | `dial_server_target` (127.hash[0:3] source, `AddrNotAvailable`-only fallback with `unique_local_fallbacks` counter) on the generic + HTTP server dials; `service_tunnel_unique_local_product` 2 passed (platform-adaptive: deterministic distinct source where aliases exist, graceful counted fallback otherwise); control kind gates (IrcServer out of mask); TOML `targets` failover untouched (no wire key); multihoming/reply-bundling stay 296-corrective |
| Streamr local UDP/port/subscription options incl. redirect | `StreamrOptions.remote_sink` (loopback, subscriber-only, replace-never-duplicate, bind on local media host); `remote_udp_host` control arm pairing host with `local_udp_port`; `ReplaceDestination` diff pin; `streamr_sink_redirect_replaces_media_target` product (byte-exact redirect + old-target silence) |
| Apply-or-reject allocation guards; unsupported options fail before listener/destination/task allocation | Ownerless keys still hit the `other` arm; kind-gate `ContradictoryOptions` per family (client/irc/socks/server/streamr negatives in every boundary test); `plan289_unsupported_options_rejected_before_storage` updated for the proxy-halves precedence (still pre-storage, still key-naming) |
| Live runtime snapshot per applied option; restart persistence and rollback; running-edit rebuild/drain semantics | Product suites assert owner effects (407/1929 bytes, 403 + fixture silence, echo/counter, source IP, sink delivery); `tunnel_plan292_options_persist_over_wire` (create ×4 with new keys, GET echo, restart, echo + stable server destination, verifier-stored/plaintext-absent on disk); diff pins (`ReplaceDestination` for shaping/profile/credentials/sink, `MutableInPlace` for idle/dial/presentation) |
| Secret redaction | `proxy_password` scrubbed to the marked verifier in normalize, `[redacted]` in GET, absent from errors/logs/disk (wire + on-disk assertions); access hashes are public routing metadata by design |
| Cross-family not_applicable rejection | Kind-gate negatives for every new key (streamr-only, HTTP-server-only, server-only matrices) |
| No regression of Plans 289–291 lifecycle/data paths | Full workspace floor green (below); M10 roundtrips, Streamr fanout, control lifecycle suites all pass unmodified |

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK (also clean on MSRV 1.88.0)
cargo test --locked --workspace --all-targets -- --test-threads=1          green*
  *two known wall-clock flakes under host load, both solo-green in 0.01–0.16 s
   (see Timing-note); each test binary otherwise fully green
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed
cargo test --locked -p i2pr-service-tunnels --lib                        285 passed
cargo test --locked -p i2pr-daemon --lib                                 346 passed (incl. 11 plan292* + unique-local dial unit)
cargo test --locked -p i2pr-daemon --lib plan292                          11 passed
cargo test --locked -p i2pr-i2pcontrol --test tunnel_matrix                 6 passed
cargo test --locked -p i2pr-i2pcontrol --test contract                     13 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_proxy_auth_product  5 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_access_product      3 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_unique_local_product  2 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_streamr_product     6 passed
cargo test --locked -p i2pr-daemon --test service_tunnel_http_server_product 10 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels                11 passed
bash scripts/check-dependency-direction.sh                                 dependency direction: ok
bash scripts/check-runtime-boundaries.sh                                   runtime boundary checks passed
bash scripts/check-service-tunnel-boundaries.sh                            service-tunnel boundary checks passed
bash scripts/check-ntcp2-interoperability.sh                               OK
bash scripts/check-constrained-host-lane-boundary.sh                       OK
bash scripts/check-m11-transit-boundaries.sh                               passed
bash scripts/check-m11-transit-qualification-evidence.sh                   175 guarded rows green
bash scripts/check-sam-acceptance-evidence.sh                              22 rows green
bash scripts/check-ssu2-acceptance-evidence.sh                             15 rows green
bash scripts/check-i2cp-acceptance-evidence.sh                             24 rows green
bash scripts/check-exploratory-tunnel-evidence.sh                          12 labels green
bash scripts/check-netdb-tunnel-evidence.sh                                12 labels green
bash scripts/check-destination-tunnel-evidence.sh                          21 labels green
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                   11 labels green
python3 -m unittest discover -s tests/integration/ntcp2/harness            OK (18 tests)
cargo deny check advisories bans sources                                   advisories ok, bans ok, sources ok
```

Not run locally (pre-existing macOS environment limit, identical on
unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`,
`check-service-tunnel-acceptance-evidence.sh`,
`check-streaming-tunnel-evidence.sh` require bash 4+ (`declare -A`
or newer quoting this host's bash 3.2.57 rejects at parse time).
Linux CI is authoritative for those six.

Timing-note (recorded, not a product defect): full-workspace runs on
this host intermittently show single wall-clock failures while the
box carries unrelated load:
`ntcp2_runtime::tests::link_reader_and_writer_are_joined_after_close`
(untouched NTCP2 code, instant solo pass) and
`socks_irc_unknown_server_command_is_dropped` (the Plan 290 2 s
line-read flake, green solo and in quiet runs). CI runs each test
binary separately, which is the supported loopback procedure per
repo guidance.

## New dependencies (reviewed)

`i2pr-service-tunnels` gains three workspace-centralized
dependencies, all pure-Rust with no `unsafe` exposure and narrow
default features: `sha2` (verifier hashing), `base64ct` (Basic
decode), `subtle` (constant-time compare). Purpose-bound to proxy
authentication; `cargo deny` (advisories/bans/sources) is clean.

## Security review

- Secrets: `proxy_password` plaintext exists only in the inbound
  control request; it is scrubbed to the marked verifier before
  the definition mirror or store, never logged, never echoed
  (GET shows `[redacted]`), never on disk (wire + on-disk
  assertions); `ProxyCredentials` forbids `Debug` exposure and
  avoids `Clone` on secret-adjacent paths; constant-time verify.
- No `Debug`/`Display`/unrestricted serialization added on secret
  types; no new `Clone` on secrets.
- All network/config/disk bytes stay bounded: IP-literal parsing
  with loopback enforcement, bool/int parsing with ranges,
  Base32-hash-only access entries, bounded query/path
  classification over already-bounded heads.
- Listeners bind loopback by default, unchanged; sink/redirect
  targets are loopback-confined at three layers (control parse,
  spec validate, bind/send-time check).
- The unique-local source bind stays inside 127/8; the platform
  fallback never leaves loopback.
- No outproxy/clearnet path opened; no TLS interception implied.

## Failure / migration review

- Every new key fails before allocation on wrong kinds or bad
  values (`ContradictoryOptions`/`InvalidOption`/`UnsupportedOption`
  with static reasons); the ownerless `other` arm is unchanged.
- Replace-vs-in-place is explicit per field and pinned by diff
  tests; in-place readers use the live committed spec, so
  MutableInPlace edits apply to new connections/requests without
  rebuilds and never to in-flight streams.
- Restart: definitions round-trip through the stored form
  (marked verifiers rebuild without plaintext); server identities
  stay stable across option edits and restarts (wire-asserted).
- Rollback: the existing stage/publish/verify transaction is
  untouched; a failed commit reconciles back as before.

## Documentation / operational evidence

- `specs/protocols/13-tunnel-option-matrix.md`: the Plan 292
  interpretation dossier (new).
- `specs/support.toml`: new
  `prop170.tunnel-option-matrix-and-noncrypto-completion` surface
  (`experimental`, `advertised = false`).
- `docs/architecture/i2pr-service-tunnels.md`: Plan 292 clauses
  on `config`/`generation`/`http`/`streamr` rows; new
  `auth`/`access`/`idle` rows.
- `docs/architecture/i2pr-daemon.md`: Plan 292 clauses on the
  manager, HTTP server, Streamr, HTTP/SOCKS/SOCKS-IRC executor rows.
- `docs/architecture/i2pr-i2pcontrol.md`: `tunnel_matrix` row +
  contract census note.

## Known limitations

- `use_ssl` stays 297-corrective: no TLS identity is configured
  or advertised by this plan.
- Pool backup-quantity/variance, session multihoming, and garlic
  reply bundling stay 296-corrective: supplied keys name the
  plan, never store.
- SigType, LeaseSet security/client-auth, and outproxy provider
  stay 293-blocked.
- Unique-local source distinctness is platform-dependent (Linux
  full 127/8 vs macOS 127.0.0.1-only); the fallback is counted
  and tested on both behaviors.
- Client destinations are ephemeral per prepare, so access
  allow-lists name stable peers; documented in spec 13.
- Reference interop for the new options is unclaimed: evidence
  is i2pr↔i2pr local product only, per workstream scope.
- The six bash-4-only checkers remain unverifiable on this macOS
  host (environment debt, unchanged since Plan 286).

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the six
  bash-4-only checkers (environment debt); the macOS loopback-alias
  limit (found during implementation, designed around with a
  counted fallback, covered on both paths); the ephemeral-client
  implication for allow-list tests (test-matrix design corrected
  before any product change); the restart-with-listener collision
  in the persistence test (test design corrected to
  `start_on_load = false`; product behavior unchanged); the
  wall-clock flakes under host load (harness timeouts, solo-green;
  CI runs binaries separately).

## Roadmap disposition

Plan 292 is **closed** (`passed-*`). Plans 293, 296, and 297
become `ready` (their only hard dep, Plan 292, is now closed).
Plan 294 is already `ready` and unaffected. Plan 295 stays
blocked (293 + 294 still open). No other Proposal 170 capability
is claimed.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 293 (hard dep: Plan 292, now closed) — moves to `ready`
  in this commit.
- Plan 296 (hard dep: Plan 292, now closed) — moves to `ready`
  in this commit.
- Plan 297 (hard dep: Plan 292, now closed) — moves to `ready`
  in this commit.
- Plan 294 (hard dep: Plan 287, closed) — already `ready`;
  unaffected.
- Plan 295 (hard deps: Plans 288 + 293 + 294) — remains blocked
  (293 + 294 still open).
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 292 acceptance criterion
passes with executed evidence above. The only allowed
`blocked_primitive` cells are the Plan 293 classes named in the
plan (§Signature/LeaseSet/provider), and every other non-apply
cell names its corrective plan with a registered owner.
