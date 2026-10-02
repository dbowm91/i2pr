# Plan 288 status — Proposal 170 RouterInfo and ClientServicesInfo inspection plane

Status: **`passed-prop170-routerinfo-and-clientservices-inspection-plane`**.

Plan of record: [`288-routerinfo-and-clientservices-inspection-plane.md`](../../implementation/i2pcontrol-proposal-170/288-routerinfo-and-clientservices-inspection-plane.md).

Hard dependency Plan 287 is closed (`passed-prop170-secure-base-i2pcontrol-jsonrpc-auth-tls`).

## Implementation commit

- `2962906` — `plan(288): RouterInfo and ClientServicesInfo inspection plane`
  (source matrix, inspection handles + dispatch, shared live-owner
  publication, `session_ids` snapshot, contract/unit/wire tests,
  `i2pr-i2pcontrol` deep-dive, daemon/API doc updates, `support.toml`
  surfaces, `rustls-pemfile` doc-drift fix).

This closure commit (closure record + registry + roadmap) lands on top
of `2962906` with no production-code change.

## Requirement-to-evidence matrix

| Plan 288 requirement | Evidence |
|---|---|
| Machine-readable source matrix, one row per selector | `crates/i2pr-i2pcontrol/src/source_matrix.rs`: `ROUTER_INFO_SOURCE_MATRIX` (30) + `CLIENT_SERVICES_SOURCE_MATRIX` (6); `matrix_mirrors_inventories()` fails the daemon build path on drift |
| Availability never derived from serializer existence | `SourceAvailability::{Available, PublishedGated, PermittedNeutral, Unavailable}`; census machine-checked: 5/16/9/0 (1 gate in-plan, 15 residual-295; 6 unavailable for 294, 3 for 295) |
| Unavailable fails whole-request, no partial response | `process_router_info`: first gap aborts with `-32603`; wire tests assert `result` absent for 294/295/gated rows, incl. inside batches |
| Empty/zero only after owner reports empty/zero | Published snapshots carry owner-supplied vectors; unpublished slots fail; `publish_netdb(vec![], ...)` proves owner-attested empty answers `[]` |
| Narrow inspection handles, not global context | `InspectionHandles`: static config truth + `publish_*` slots + `Arc`-published live states with read-only snapshot reads only |
| Router identity/publication | `router.version` (own crate version), `api_version` (1), `uptime` (control-plane seconds), `status` (`running`), `network_id` (validated config), `router.hash` (identity-gated 44-char I2P base64 published by `run_daemon` from bootstrap) |
| Transport/network state | `TransportSnapshot` slots + `publish_transport` (ntcp2/ssu2 lists, reachability, errors, clock-skew); no raw mutable maps cross the boundary |
| NetDB | `publish_netdb` (bounded hash lists + floodfill mode); ban facility explicitly absent → `banned_peers` unavailable with the missing-owner reason recorded |
| Tunnel state | `publish_tunnels` (counts as one-element lists per the frozen `List` shape, rolling success/bandwidth maps); TBM/queue internals stay with their owners |
| Logs/news/address-book not fabricated | 6 address-book rows unavailable-294, `logs.recent`/`news.feed` unavailable-295; residual rows enumerated below |
| ClientServicesInfo six selectors | `client_service_result`: I2PTunnel (startup inventory + manager overlay, client/server split), HTTPProxy/SOCKS (first-enabled bind), SAM (config + bounded live ids, no sockets/peer IPs), BOB (constant `false`), I2CP (config + live counts) |
| No private/secret/high-cardinality leakage | `Debug` redacted (presence flags only); SAM ids non-secret labels; `session_ids` capped at 64; publication lists capped at 1024; hash format-validated |
| Presence/select semantics match pinned behavior | Null-valued select form per requested key (verified read-only against pinned i2pd `I2PControlHandlers.cpp`: per-key selection, echoed keys); `i2p.*` base keys structurally disjoint → invalid params (stricter than i2pd skip-and-log, recorded) |
| Bounded collections before allocation | Publish-time +1 rejection (`ListOverBound`, `RatesOverBound`, `StringOverBound`, `MalformedHash`); serialization re-checks; batch/body ceilings unchanged from Plan 287 |
| No mutation from read requests | Wire test proves byte-identical repeat responses; builders take `&self`; locks degrade to unpublished on poisoning |
| No M12/mainline behavior change | Only additive: new module, new test files, optional-service publish calls, `run_daemon` hash publication; full workspace floor green |

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK
cargo test --locked --workspace --all-targets -- --test-threads=1          3304 passed, 0 failed
  (delta over Plan 287 floor: +1 contract census, +19 inspection unit,
   +9 inspection wire, +2 registry session_ids)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed
cargo test --locked -p i2pr-daemon --lib i2pcontrol                       39 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection            9 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_base                 10 passed (unchanged behaviors intact)
cargo test --locked -p i2pr-api --lib sam::registry                      10 passed
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
bash scripts/check-destination-tunnel-evidence.sh                          21 labels green
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                  11 labels green
python3 -m unittest discover -s tests/integration/ntcp2/harness            OK
cargo deny check advisories bans sources                                   advisories ok, bans ok, sources ok
```

Not run locally (pre-existing macOS environment limit, identical on
unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`, `check-service-tunnel-acceptance-evidence.sh`,
`check-streaming-tunnel-evidence.sh` require bash 4+ associative arrays
(`declare -A`); this host ships bash 3.2.57 only. None of their areas
are touched by this plan; Linux CI is authoritative for those six.

## Acceptance criteria

Plan 288 closes as **partial read-only Proposal 170 support** per the
plan's §Acceptance criteria: residual selectors are precisely
enumerated for Plan 295 below, and the whole RouterInfo surface is
explicitly not claimed complete. Conformance passes for every
implemented row; `specs/support.toml` gains two `experimental`,
`advertised = false` surfaces.

## Residual selectors for Plan 295 (precise enumeration)

Publication wiring (owner exists or is expected; mechanism proven,
publisher to be threaded):

- `netdb.known_peers`, `netdb.active_peers`, `netdb.floodfill_mode`
  (owner: netdb; needs a persistent NetDB snapshot publisher).
- `transport.ntcp2.active_peers` (owner: transport; NTCP2 activation
  guard holds, so the first publisher is the transport state itself).
- `transport.ssu2.active_sessions`, `transport.reachability`,
  `transport.errors`, `tunnel.bandwidth`, `network.clock_skew`
  (owners: transport/SSU2 service).
- `tunnel.exploratory.count`, `tunnel.client.count`,
  `tunnel.participating.count`, `tunnel.build_queue`,
  `tunnel.success_rate` (owners: exploratory/destination/transit
  coordinators; success-rate needs the request-independent bounded
  metric owner).
- `network.rates` (owner: bounded rate owner).

Unavailable pending new owners or protocol decisions:

- `network.banned_peers` — i2pr has no ban facility; an authoritative
  empty result requires an explicit ban owner first (else fabrication).
- `logs.recent`, `news.feed` — no safe bounded log owner and no
  authenticated news owner exist.
- `network.clock_skew` neutral disposition is undecided (strict gating
  holds until Plan 295 justifies neutrality).
- All six `addressbook.*` rows belong to Plan 294, not Plan 295.

## Defects found during implementation (all corrected)

- `serde_json::Map` sorts keys (no `preserve_order`): responses emit
  lexicographic order, not canonical matrix order. Determinism holds
  either way; documented and frozen by wire test.
- Test TOML used `[[service_tunnels.tunnels]]` (plural) and an invalid
  b32 label; corrected to the `[[service_tunnels.tunnel]]` shape with
  the `canonical_b32` convention.
- `docs/architecture/dependency-graph.md` still listed `rustls-pemfile`,
  removed before the Plan 287 closure: corrected to `rustls-pki-types`
  in this commit.

## Deliberate deviations (pinned-behavior parity notes)

All verified read-only against pinned i2pd openssl `2d57d3f6`
(`daemon/I2PControl.cpp`, `daemon/I2PControlHandlers.cpp`); no i2pd
code imported or vendored.

- Unknown selector keys → `-32602` (i2pd skips-and-logs; ours is
  stricter by frozen contract).
- Selector values must be null (i2pd ignores values; null-required is
  the canonical select form).
- SAM sessions omit socket peer endpoints (i2pd emits
  `remote_endpoint` IPs; ours is privacy-stricter per plan).
- `router.status` reports `running` (i2pd reports destination
  readiness `1`/`0`; ours reports control-plane liveness).
- `router.version` is the i2pr crate version (never another router's
  release string, per `specs/CONFORMANCE.md`).
- `BOB` is always `enabled: false` (plan-mandated; i2pd reflects its
  BOB channel).

## Migration / compatibility

Additive only: new modules/files, one new `Arc` parameter on three
private register functions, one new public graph constructor (old
signature delegates), one new `session_ids` registry method, two
`support.toml` surfaces. `Authenticate`/batch/TLS/throttle behavior
unchanged; three Plan 287 tests updated to the new select-form success
(empty selection → `{}`).

## Security review

- No new secret handling: hashes/ids/counts/binds are public or
  operator labels; passwords/tokens/destinations/keys never enter
  snapshots; `Debug` prints presence flags only.
- Every publication path has an explicit ceiling with +1 rejection
  tests; poisoning degrades to unpublished, never panics, never
  fabricates.

## Documentation / operational evidence

- New `docs/architecture/i2pr-i2pcontrol.md` deep-dive (the crate had
  none).
- `docs/architecture/i2pr-daemon.md`: `lib.rs` composition row,
  `i2pcontrol.rs` Plan 288 row, new `i2pcontrol_inspection.rs` row.
- `docs/architecture/i2pr-api.md`: registry `session_ids` note.
- `specs/support.toml`: `prop170.routerinfo-inspection-plane` +
  `prop170.clientservicesinfo-inspection-plane` (`experimental`,
  `advertised = false`).

## Known limitations

- Partial support only (see residuals above); the whole RouterInfo
  surface is not claimed complete.
- Uptime is control-plane uptime (service epoch), not router boot time.
- `router.hash` stays gated when bootstrap builds no local RouterInfo.
- Live SAM/I2CP overlays appear only after their factories publish
  (supervisor start); before that, config truth answers.
- The six bash-4-only checkers are unverifiable on this macOS host
  (environment debt, unchanged from prior closures).

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the six bash-4-only
  checkers (environment debt); the `rustls-pemfile` doc drift
  (corrected in the implementation commit).

## Roadmap disposition

Plan 288 is **closed** (`passed-*`). Plans 290/291 stay blocked on
Plan 289. Plan 295 remains blocked on Plans 293 + 294 (288 now
closed). No other Proposal 170 capability is claimed.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 289 (hard dep: Plan 287 only) — already `ready`; unaffected.
- Plan 294 (hard dep: Plan 287 only) — already `ready`; unaffected.
- Plans 290/291 (hard dep: Plan 289, still open) — remain blocked.
- Plan 292 (hard deps: Plans 290 + 291) — remains blocked.
- Plan 293 (hard dep: Plan 292) — remains blocked.
- Plan 295 (hard deps: Plans 288 + 293 + 294) — remains blocked
  (288 now closed; 293 + 294 still open).
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 288 acceptance criterion
passes with executed evidence above. Plan 289 executes next on this
branch.
