# Plan 294 status — Canonical AddressBook subsystem and Proposal 170 resolver integration

Status: **`passed-prop170-canonical-addressbook-and-resolver-integration`**.

Plan of record: [`294-canonical-addressbook-and-resolver-integration.md`](../../implementation/i2pcontrol-proposal-170/294-canonical-addressbook-and-resolver-integration.md).

Hard dependency closed: Plan 287
(`passed-prop170-secure-base-server`).

## Implementation commits

- `e98e5ef` — `plan(294): i2pr-addressbook canonical owner crate`
  (four books, precedence lookup, typed hostname/destination
  validation, 13-key `SetConfig` domain, bounded subscription
  ingestion, deterministic generations, refresh queue, read-only
  resolver snapshots; 26 unit tests).
- `1ab0c0f` — `plan(294): address-book generation persistence
  adapter` (opaque current/backup file store in `i2pr-storage`;
  3 tests).
- `1670c1c` — `plan(294): daemon address-book owner, config, and
  consumer wiring` (manager with activation rule, `[addressbook]`
  config, SAM naming step, alias-miss resolution, method dispatch,
  six getters, source-matrix flips).
- `b4e3683` — `plan(294): docs, spec dossier, and support surface`
  (deep-dives, indexes, spec 15, support row).
- `d1ff00d` — `plan(294): clippy and rustdoc hardening` (loop/if
  refactors, doc-link fixes; no behavior change).

This closure commit (closure record + registry + roadmap) lands on
top with no production-code change.

## Outcome

One canonical address-book owner drives both ordinary i2pr naming
and Proposal 170 administrative/getter behavior with atomic
persistence and bounded refresh composition. No Proposal-only JSON
database exists: every mutation path commits through the same
`AddressBook` state that SAM, service tunnels, and RouterInfo
getters read.

## Requirement-to-evidence matrix

| Plan 294 requirement | Evidence |
|---|---|
| New runtime-neutral crate owning books, validation, precedence, subscriptions, revisions; no sockets/Tokio/fs/HTTP/UI | `i2pr-addressbook` (10 modules, `forbid(unsafe_code)`, deps `i2pr-proto` + `base64ct`/`serde`/`serde_json`/`thiserror` only); dependency script + arch graph updated |
| Typed hostname + full Destination validation | `.i2p` canonical policy (lowercase, suffix, labels, lengths); I2P-Base64 alphabet/padding rules + exact `Destination::decode` under workspace policy; 391-byte fixture round-trips, malformed/truncated/oversized rejected |
| Four books, precedence, provenance | private/local/router/published + subscription-derived last; same hostname in many books; layer-reveal + shadowing tests |
| Read-only resolver handle; mutation only through control handle | `AddressBookResolver` (`Arc` snapshot, lookup-only) vs `AddressBookControl` (sole mutation path); snapshot-isolation test |
| Atomic persistence with prior-generation fallback (storage adapter) | Opaque current/backup pair, atomic publish-and-rotate, symlink/permission/size gates, missing dir loads absent; 3 storage tests |
| Subscription domain with bounded refresh composition | 16 URLs × 2048 B, HTTP/HTTPS only; strict body ingestion (1 MiB, 8192-line, 1000-entry caps, whole-body failure, last-wins); queue (one active + one coalesced pending); cadence worker service; unavailable-downloader diagnostics to the bounded artifact |
| Thirteen config keys, one disposition + typed parser each | Six confined artifact paths, 1..=720 h cadence, fetch-path-only proxy host/port, inert theme, artifact-only log path/level, 1..=300 s timeout, 1..=1000 entry ceiling; whole-map atomic updates; tightening below live counts fails |
| Proposal method with exact semantics | One mode per request, whole-request validation, mixed-shape failure, Delete-presence deletion, exact `{success, message}` shapes, deterministic missing-entry behavior |
| RouterInfo getters read committed state | Six selectors render books (hostname maps), subscriptions object, thirteen-key config from the published snapshot with ceiling re-checks |
| Resolver integration (SAM/HTTP/SOCKS/IRC/static aliases) | SAM naming consults the owner after ME/Base32 (inactive stays `KeyNotFound`); alias miss consults the owner (aliases win; hits decode through local/remote machinery covering HTTP/SOCKS/IRC executors); product test proves book hostname → co-owned server destination with no LeaseSet lookup |
| Disabled/I2PControl-off isolation + activation rule | Disabled manager never touches the filesystem (empty-dir assertion); corrupt generations fail into sticky-error inactivity (getters gap, resolvers unpublished, legacy naming); standalone dispatch keeps the Plan 294 marker |
| Theme/log cannot affect frontend/global logging | Theme round-trips inertly; log level gates only the artifact; global tracing untouched (no code path exists) |
| No disconnected store | Mutations, getters, and all three lookup consumers share one committed state; contract-parity test pins the frozen inventory key-for-key |

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK (also clean on MSRV 1.88.0)
cargo test --locked --workspace --all-targets -- --test-threads=1          green* (re-sweep fully green)
  *one wall-clock flake in the first sweep (socks-irc product suite under host
   load), solo-green in 2.48 s; same class as the Plan 292 Timing-note
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed
cargo test --locked -p i2pr-addressbook --lib                               27 passed
cargo test --locked -p i2pr-addressbook --test contract                      5 passed
cargo test --locked -p i2pr-storage --lib                                   22 passed (incl. 3 generation-adapter)
cargo test --locked -p i2pr-i2pcontrol --test contract                       13 passed (census 11/16/3)
cargo test --locked -p i2pr-daemon --lib addressbook                        14 passed (9 manager + 2 config section + 2 method dispatch + 1 alias-miss)
cargo test --locked -p i2pr-daemon --lib sam::tests::plan294                  1 passed (SAM naming through the shared handle)
cargo test --locked -p i2pr-daemon --test service_tunnel_addressbook_product  3 passed (co-owned resolve, restart, isolation)
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

## New dependencies (reviewed)

`i2pr-addressbook` adds no new external crates: `i2pr-proto`
(path), `base64ct`, `serde`, `serde_json`, `thiserror` (all
workspace-centralized, narrow features, no `unsafe` exposure).
The `i2pr-i2pcontrol` dev-dependency serves the contract-parity
test only and is excluded from the production graph. `cargo deny`
(advisories/bans/sources) is clean.

## Security review

- No secret-bearing types: destinations are public routing
  material echoed verbatim; errors carry no hostnames,
  destinations, URLs, or values (pinned by construction + tests).
- No `Debug`/`Display` on secrets (none exist); no new `Clone`
  on secrets; no logging of full state.
- Filesystem: fixed filenames under one state dir, 0700/0600
  discipline, symlink/special-file/path-escape rejection at
  storage, import, config, and artifact layers; size ceilings at
  every read.
- Network: no socket, DNS, or HTTP client added; the refresh
  worker sleeps and drains the queue (attempts report
  unavailable); proxy keys cannot create a general proxy.
- Listeners bind loopback by default, unchanged; no new listener.

## Failure / migration review

- Every mutation validates whole before committing; persistence
  failure rolls the in-memory commit back; corrupt generations
  never activate partially (current → backup → import → sticky
  inactive, in that order).
- No migration exists or is needed: i2pr has no pre-existing
  address-book files; first activation imports bounded operator
  artifacts only when present.
- Restart restores state + revision from the persisted
  generation (product-asserted).
- Disabled subsystems never read, write, or consult control
  files (empty-directory assertion).

## Documentation / operational evidence

- `specs/protocols/15-canonical-addressbook.md`: the Plan 294
  dossier (new).
- `specs/support.toml`: new
  `prop170.canonical-addressbook-and-resolver-integration`
  surface (`experimental`, `advertised = false`).
- `docs/architecture/i2pr-addressbook.md`: new per-crate
  deep-dive.
- `docs/architecture/overview.md`: workspace map + crate index
  + dependency sketch rows.
- `docs/architecture/dependency-graph.md`: production edge
  table + boundary rules.
- `docs/architecture/i2pr-storage.md`: generation-adapter
  clause.
- `docs/architecture/i2pr-daemon.md`: manager/dispatch/
  getters/SAM/service-tunnel rows + crate-root module list.
- `docs/architecture/i2pr-i2pcontrol.md`: source-matrix flip
  note.

## Known limitations

- No downloader owner exists: refresh attempts report
  unavailable and subscription bodies arrive only through the
  tested ingestion seam. `proxy_host`/`proxy_port`/
  `lookup_timeout` validate, store, and round-trip for the fetch
  path without a live consumer (explicitly documented).
- The frozen `SetConfig` thirteen differ as a set from the
  reference fork's M096 thirteen; Plan 294 builds against the
  frozen inventory and Plan 295 adjudicates (spec 14
  §Items-carried, spec 15 §Carried).
- Reference interop for naming is unclaimed: evidence is
  i2pr-local product only, per workstream scope.
- The six bash-4-only checkers remain unverifiable on this macOS
  host (environment debt, unchanged since Plan 286).

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the
  trailing-dot SAM gate miss (found by the new SAM test, fixed
  with canonicalization parity, covered); the manually-created
  state-dir permission gate (test authoring, product correctly
  fail-closed); the current-corrupt fallback gap (found by the
  new fallback test, fixed to consult backup before failing);
  the wall-clock product-suite flake under host load
  (solo-green; CI runs binaries separately); the six
  bash-4-only checkers (environment debt).

## Roadmap disposition

Plan 294 is **closed** (`passed-*`). Plans 296 and 297 stay
`ready` (unaffected). Plan 295 becomes `ready` (its hard deps
288 + 293 + 294 are now all closed). No other Proposal 170
capability is claimed.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 294 (hard dep: Plan 287, closed) — moves to `closed`
  in this commit.
- Plan 295 (hard deps: Plans 288 + 293 + 294, all closed) —
  moves to `ready` in this commit.
- Plan 296 (hard dep: Plan 292, closed) — already `ready`;
  unaffected.
- Plan 297 (hard dep: Plan 292, closed) — already `ready`;
  unaffected.
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 294 acceptance
criterion passes with executed evidence above. The only
deferred fetch capability (downloader owner) is an explicit,
bounded, tested absence — not a hidden gap.
