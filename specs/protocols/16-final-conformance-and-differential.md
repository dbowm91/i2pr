# 16 — Plan 295 final Proposal 170 conformance and differential record

Status: Plan 295 implementation dossier. Machine-readable sources:
`crates/i2pr-i2pcontrol/src/source_matrix.rs` (36 rows),
`crates/i2pr-i2pcontrol/src/tunnel_matrix.rs` (336 cells),
`crates/i2pr-i2pcontrol/tests/final_matrix.rs` (exact census),
`crates/i2pr-i2pcontrol/tests/contract.rs` (inventory mirror + census).
No prose-only counting: every count below is asserted by
`plan295_final_public_contract_census`.

## 1. Re-freeze verdict: no drift

At Plan 295 execution start the frozen references were re-fetched and
compared to the Plan 286 freeze:

- Proposal 170: index still `Open | created 2026-05-20 |
  updated 2026-05-20`; full-text structure matches (5 methods,
  7 TunnelManager actions, 12 types, 4 AddressBook types, 13
  SetConfig keys, 6 ClientServicesInfo keys). No new
  methods/selectors/semantics.
- Base I2PControl API-1: classic surface intact, still API version 1,
  no Proposal 170 leakage. Auth/version semantics unchanged.
- Java PR 6 (`i2p/i2p.plugins.i2pcontrol`): still open, unmerged, tip
  `45bb593` identical to the frozen pin.
- i2pd `2d57d3f`, emissary pins: unchanged (verified during
  differential fetch; see §5).

No spec-reconciliation plan was needed. The Plan 295 differential
below compares against these exact pins.

## 2. Final inventory census

| Dimension | Count | Disposition |
|---|---|---|
| Methods | 5 | Authenticate, RouterInfo, AddressBook, TunnelManager, ClientServicesInfo |
| RouterInfo selectors | 30 | 27 Available, 1 PublishedGated (`router.hash`, Plan 288), 1 Unavailable (`news.feed`, §3), 1 PermittedNeutral (`network.clock_skew`, §3) |
| ClientServicesInfo keys | 6 | all Available (disabled is truthful config state) |
| AddressBook SetConfig keys | 13 | frozen allowlist (Plan 294; set divergence recorded in §5) |
| TunnelManager actions | 7 | create/edit/get/start/stop/restart/delete |
| Tunnel types | 12 | all with real backends (Plans 289–291; data-path evidence in their closures) |
| Tunnel options | 46 | typed inventory with secret classification |
| Type×option cells | 336 | 227 apply, 37 not-applicable, 30 explicit incompatibility (Plan 293), 39 corrective-296, 3 corrective-297 |

## 3. Source-owner table (RouterInfo)

`wire` = canonical selector; `read` = per-request serving;
`persist` = durability; `isolate` = disabled/default behavior;
`evidence` = proving test.

| wire | type | owner | read | persist | isolate | evidence |
|---|---|---|---|---|---|---|
| router.version | String | daemon build | constant | static | answers | plan288_router_version_is_crate_version |
| router.api_version | Integer | i2pcontrol contract | constant 1 | static | answers | plan288_router_api_version_is_one |
| router.uptime | Integer | i2pcontrol service | monotonic epoch | none | answers | plan288_router_uptime_advances_monotonically |
| router.status | String | i2pcontrol service | `"running"` while dispatch runs | none | answers | plan288_router_status_is_running_while_serving |
| router.network_id | Integer | daemon config | validated config | config file | answers | plan288_router_network_id_matches_config |
| router.hash | String | bootstrap identity | published snapshot | identity file | gaps until published | plan288_router_hash_gated_until_identity_published |
| netdb.known_peers | List | daemon composition | attested `[]` (no learning paths in the default graph) | none | answers | plan295_netdb_known_peers_attested |
| netdb.active_peers | List | daemon composition | attested `[]` | none | answers | plan295_netdb_active_peers_attested |
| netdb.floodfill_mode | String | daemon config | `"disabled"` (no permit ever constructed) | none | answers | plan295_netdb_floodfill_mode_declared |
| transport.ntcp2.active_peers | List | transport guard | attested `[]` (activation guard holds) | none | answers | plan295_ntcp2_peers_empty_under_guard |
| transport.ssu2.active_sessions | List | SSU2 runtime service | live peer hashes, else attested `[]` | none | answers | plan295_ssu2_sessions_live_or_attested |
| transport.reachability | String | daemon config | `"loopback-only"` declaration | none | answers | plan295_reachability_loopback_declaration |
| transport.errors | List | SSU2 runtime service | live non-zero `name=value` counters, else attested `[]` | none | answers | plan295_transport_errors_live_or_attested |
| tunnel.exploratory.count | List | daemon composition | attested `[0]` (no coordinators in graph) | none | answers | plan295_exploratory_count_attested |
| tunnel.client.count | List | daemon composition | attested `[0]` | none | answers | plan295_client_count_attested |
| tunnel.participating.count | List | daemon composition | attested `[0]` (no transit owner) | none | answers | plan295_participating_count_attested |
| tunnel.build_queue | List | daemon composition | attested `[0]` | none | answers | plan295_build_queue_attested |
| tunnel.success_rate | Map | control metrics owner | rolling pair, `(0,0)` with no reporters | none | answers | plan295_success_rate_from_metrics |
| tunnel.bandwidth | Map | control metrics owner | rolling pair from SSU2 counters once observed | none | answers | plan295_bandwidth_from_metrics |
| addressbook.* (6) | Map | canonical AddressBook | committed generation | generation files | gaps until committed | plan294_addressbook_*_available |
| logs.recent | Map | daemon log ring | live redacted snapshot (INFO+, 256 entries, 192 B lines, drop count) | none | answers once published | plan295_logs_recent_from_ring |
| news.feed | String | none served | never: whole-request `-32603` (Plan 295) | n/a | always gaps | plan295_news_feed_unsupported_by_design |
| network.clock_skew | Integer | i2pcontrol contract | neutral constant `0` (declared, never measured) | none | answers | plan295_clock_skew_neutral |
| network.banned_peers | List | explicit ban ledger | attested `[]` (no reporters, no criteria) | none | answers | plan295_banned_peers_attested_empty |
| network.rates | Map | control metrics owner | rolling `ssu2.*` map, `{}` until observed | none | answers | plan295_rates_from_metrics |

Security rules (all rows): public material only; secret-marker log
lines replaced; peer hashes are public identity; whole-request
failure on any unowned row (no partial responses, no fabricated
zero/false/empty).

## 4. Qualified support profile (exact claim)

i2pr implements the Proposal 170 control plane with this exact
profile — **not** an unqualified "full support" label:

- Supported: all 5 methods; 29 of 30 RouterInfo selectors with live
  or attested sources; all 6 ClientServicesInfo keys; AddressBook
  entry/subscription/config control over 4 books with the frozen
  13-key allowlist; TunnelManager 7 actions over 12 types with the
  227 apply cells owned by real backends.
- Unsupported by design (explicit determinations, tested gaps):
  `news.feed` (no authenticated feed exists; never served);
  30 deep tunnel cells (`ExplicitIncompatibility`, spec 14);
  42 corrective cells (Plans 296/297 own them).
- Canonical divergences from Proposal spellings (intentional,
  frozen, tested): lowercase TunnelManager envelope
  (`action/name/type/options`) with snake_case option keys;
  null-valued RouterInfo/Service select forms; unknown keys rejected
  (Proposal-silent, fork-tolerant, Java-ignoring, i2pd-skipping —
  i2pr is strictest); integer `0` for null clock skew; List shapes
  for counts and ban attestation where the Proposal says
  int/Map.
- Coverage boundaries: metrics cover registered transport counters
  only (local loopback/destination traffic bypassing them is not
  counted); attested empties describe the default graph (no NetDB
  learning, no tunnel coordinators, no transit owner); SSU2 rows go
  live only when the SSU2 service registers.

## 5. Differential comparison (static, at pins)

References fetched read-only at pins; sanitized shapes only. Live
external runs are env-gated (`I2PR_I2PCONTROL_TARGET` +
`I2PR_I2PCONTROL_PASSWORD`) and recorded `blocked-env-absent` on
this host. Taxonomy: (a) Proposal text controls, (b) reference
clarification adopted by i2pr, (c) intentional i2pr-stricter
behavior, (d) unresolved ambiguity recorded without code change.

### 5.1 eggstack/emissary fork @ `6885a94` (strongest oracle)

Overlap exists on every surface (`emissary-cli/src/i2pcontrol/`
at pin: `auth.rs`, `rpc.rs`, `router_info*.rs`,
`address_book.rs` + `domain/address_book.rs`, `domain/tunnel.rs` +
`backends/options.rs`, `client_services.rs`,
`stores/generation_store.rs`).

- (b) Agreed and independently implemented: 32-byte hex tokens,
  1-day life, expiry-then-removed, six auth codes with identical
  numerics, batch/notification/id semantics, whole-request failure
  on unowned rows, secret-redaction pattern, atomic generations
  with prior-fallback, StartOnLoad reconciliation, SigType refusal
  (M121 Outcome C verbatim), LeaseSet-disable rejection, no dummy
  provider, Delete-by-presence, `{success,message}` shapes, method
  inventory.
- (a) Proposal controls against i2pr: Capitalized TunnelManager
  field/option spellings (fork uses them; i2pr lowercase is the
  recorded deviation); ten-mode `EncryptLeaseSet` enum type and
  `LeaseSetClientAuths[]` + `OptionalLookup` shapes (spec 14
  already carries these for Plan 295 — confirmed, no new action).
- (c) i2pr stricter: null-valued selects; `i2p.*` disjoint
  rejection; SAM socket-peer omission; closed AddressBook
  vocabulary/allowlist with no value echo.
- (d) Recorded without code change: throttle parameters +
  success-clear; error message strings; the disjoint 13-key
  SetConfig sets (frozen i2pr set stands; reconciliation needs a
  new plan); `sig_type` text-vs-integer latitude;
  `use_outproxy_plugin` type + 2-vs-4-family applicability;
  banned-peers empty-Map (fork) vs empty-List (i2pr) — reconciled
  to Map if a ban source ever exists (future plan owns the shape);
  clock-skew nullable-integer (fork) vs integer-0 (i2pr) — same
  neutral concept, different encoding; `info`/`rawConfig` exact
  key sets; `List`/`All` compat surface (unclaimed either way).

### 5.2 Java I2PControl PR 6 @ `45bb593` (proposal implementation)

Code exists at pin under
`src/java/net/i2p/i2pcontrol/servlets/jsonrpc2handlers/`
(`RouterInfoHandler`, `AddressBookHandler`, `AddressBookFiles`,
`TunnelManagerHandler`, `TunnelRequestParser`,
`AuthenticateHandler`, `SecurityManager`, `JSONRPC2Helper`,
`JSONRPC2ExtendedError`).

- (b) Adopted: `-32001…-32006` numerics with the
  missing/invalid/expired split; expiry-removal; Delete-presence;
  `{success,message}`; 1-day token life; `Name`+`Action`
  required pair.
- (a) Proposal controls: Capitalized spellings; 10-mode
  `EncryptLeaseSet` string inventory; `LeaseSetClientAuths`
  Name/Key objects; 12-type create split.
- (c) i2pr stricter: closed Authenticate/AddressBook
  vocabularies; unknown-selector rejection; secret no-echo with
  redaction (Java echoes full `rawConfig` including passwords at
  pin — recorded as non-normative); ceilings plus throttle;
  explicit-disable rejection.
- (d) Recorded: check order (Java password-before-API-version);
  message strings; unbounded store plus sweeper vs bounded FIFO;
  default-password policy; `All`; result-status-vs-error
  partition (Java returns errors inside success envelopes);
  status vocabulary; destination echo; news/logs/ban/stats/config
  shapes (Java-de-facto, not Proposal text); open SetConfig keys
  with comment stripping; SigType fallback-vs-rejection (Java
  normalizes, i2pr and the fork refuse — Java is the outlier,
  dispositions unchanged).

### 5.3 i2pd @ `2d57d3f` (adopted base subset)

`daemon/I2PControlHandlers.cpp` + `daemon/I2PControl.cpp` at pin.
No AddressBook, no TunnelManager, no Proposal selectors — absence
is not failure.

- (b) Agreed pattern: token memory-only with expiry sweep;
  6-key ClientServicesInfo inventory; per-key select-and-echo.
- (c) i2pr stricter: unknown-selector rejection;
  null-strictness; `running`/crate-version/BOB-false/SAM-no-IPs;
  explicit auth errors; static parse message; no secret logging
  (i2pd logs new password values at pin — non-normative);
  bounded stores plus throttle.
- (d) Recorded: uptime units (ms vs seconds); I2PTunnel
  inventory scope; token encoding/lifetime (base64/1h — i2pd is
  the outlier; fork, Java, and i2pr agree on hex/1d);
  certificate durability; body/buffer ceilings.

### 5.4 Differential verdict

No unexplained contract mismatch remains on overlapping
supported surfaces: every difference is classified above, and
none changes a Plan 293/294 disposition. Live-external
qualification rows stay env-gated with stop provenance
(`blocked-env-absent`); they are not passed, not failed, not
skipped-silently.

## 6. Security requalification (head scope)

The 15-item review was re-applied to the closing head with focus
on the new surface:

- No new auth, token, batch, connection, TLS, filesystem, or task
  paths: the ring, ledger, and metrics are synchronous bounded
  owners; no sockets, no spawns, no files.
- Secret hygiene: ring redaction is marker-tested (8
  case-insensitive markers + case-sensitive `PRIV`);
  `LogLine::wire` and `Debug` can only emit stored (already
  redacted) lines; metrics expose `u64` counters only; ledger
  attests `[]`; SSU2 session entries are public peer hashes via
  the same I2P-base64 encoder as `router.hash`.
- Residual risk (documented limitation): unknown secret formats
  in log messages rely on the workspace no-secret-logging
  invariant; redaction is defense-in-depth.
- Poison/degrade paths: every lock uses `unwrap_or_default` or
  skips the write — inspection never panics, never fabricates.
- New rows are O(1) or bounded-1024 per request; no whole-router
  scans; metrics windows tick on counter reads plus one
  timestamp.
- Adversarial suites re-run green at the closing head:
  `i2pcontrol_base` (10), `i2pcontrol_inspection` (9),
  `i2pcontrol_tunnels` (11), daemon `i2pcontrol*` lib tests
  (72+), plus the new corpus (28/8/4) and redaction/bound unit
  rows. No high or medium finding remains.

## 7. Residuals (narrow, owned)

- R1: live-external differential rows (`blocked-env-absent`):
  require the provisioned lane (exact-pinned emissary/Java/i2pd
  with reachable I2PControl endpoints). A narrow follow-up plan
  may run the `#[ignore]`-gated external corpus there; no
  production wire change is expected.
- R2: `(d)`-classified shape ambiguities (§5) need a
  proposal-reconciliation plan only if i2pr ever claims
  unqualified support or cross-router parity; the qualified
  profile in §4 stands without one.
- R3: ban shape (`[]` List vs Proposal/Fork Map) must be
  reconciled to Map if a ban source ever exists.
- R4: metrics coverage excludes local loopback/destination
  traffic; a future plan may register those counters.

No corrective plan is registered: every Plan 295 acceptance
criterion passes under the qualified profile, and §4 states the
exact claim boundary.
