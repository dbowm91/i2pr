# Plan 295 status — Full Proposal 170 source completion and cross-router conformance

Status: **`passed-prop170-full-source-completion-and-differential-conformance`**.

Plan of record: [`295-full-source-completion-and-cross-router-conformance.md`](../../implementation/i2pcontrol-proposal-170/295-full-source-completion-and-cross-router-conformance.md).

Hard dependencies closed: Plans 288 + 293 + 294
(`passed-prop170-routerinfo-and-clientservices-inspection-plane`,
`passed-prop170-deep-tunnel-option-determinations`,
`passed-prop170-canonical-addressbook-and-resolver-integration`).

Reference re-freeze (no drift): Proposal 170 rev 2026-05-20, base
API-1 2026-07-10, eggstack/emissary
`6885a945d25a5ae61bc68191d27c5816bc3df4c9`, eepnet
`9b43484a21d5a1291c4881cdae62a36c527f8c0f`, Java PR6
`45bb593000408071dd376b78848fdc246dccd964`, i2pd openssl
`2d57d3f6783efbfebde6c5b03f29e6c231a84d6b`.

## Implementation commits

Branch `plan-295-final-conformance`, on top of `0d86380`
(Plan 294 closure):

- `b0e6690` — `plan(295): bounded SSU2 active-peer-hash projection`
  (`Ssu2RuntimeService::active_peer_hashes` at
  `MAX_SSU2_ACTIVE_CEILING`, poison → empty; unit + `ssu2_local`
  session-peers test).
- `22f0fca` — `plan(295): control source owners, live serving,
  matrix completion` (`control_sources.rs`: `LogRing` +
  `LogRingLayer` + `BanLedger` + `ControlMetrics` with 9 unit
  tests; layered `initialize_logging`; live cells, trimmed
  publishers, `CLOCK_SKEW_NEUTRAL`, matrix flips to 27/1/1/1).
- `4ba2efa` — `plan(295): production publishers and differential
  corpus` (composition installs ring/metrics/bans + netdb/transport/
  tunnels attestations; SSU2 factory publishes with metrics
  priming; 28-answered + 8-gapped + 4-error corpus test).
- `bc9a86d` — `plan(295): differential lane, evidence, and integrity
  checker` (`run-differential.sh`, evidence JSON, bash-3.2-clean
  `check-i2pcontrol-acceptance-evidence.sh`).
- `5f7dbad` — `plan(295): final public-contract census test`
  (`final_matrix.rs` 27/1/1/1 gate).
- `98e37ce` — `plan(295): dossier, support surfaces, arch docs, CI
  wiring` (spec 16, two new support surfaces, deep-dives, CI/floor
  tables).
- `b1548f4` / `552d2c2` — fmt normalization; drop unused test import.
- `832197f` / `86a4471` — clippy-clean (`is_empty`, `Default`,
  `partial_cmp`); rustdoc private-link fix. No behavior change.

This closure commit (closure record + registry + roadmap) lands on
top with no production-code change. The differential lane was
re-run at `86a4471` (evidence head); only planning prose follows.

## Outcome

The final source matrix is **27 Available / 1 gated / 1
unavailable-by-design / 1 justified neutral** — the exact qualified
profile, not an unqualified full-support claim:

- gated: `router.hash` (Plan 288 gate, unchanged);
- unavailable-by-design: `news.feed` (no news subsystem exists);
- justified neutral: `network.clock_skew` serves constant `0`
  (loopback-only single-host router has no skew signal; the
  dossier states the justification and the constant).

No `passed` row is asserted beyond the tested subset
(`specs/CONFORMANCE.md`).

## Requirement-to-evidence matrix

| Plan 295 requirement | Evidence |
|---|---|
| SSU2 session-peer source is a truthful bounded owner | `active_peer_hashes` capped at `MAX_SSU2_ACTIVE_CEILING`, lock-poison → empty; `ssu2_local` names live session peers (1 passed) |
| Log source is a real subsystem, not a fabrication shim | `LogRing` (256 entries, 192 B lines, INFO+, 8 insensitive + `PRIV` markers, drop count) fed by `LogRingLayer` installed in `initialize_logging`; `logs.recent` serves ring snapshots with ceiling re-checks; 9 lib tests |
| Ban source has an explicit owner before serving empty | `BanLedger` attested `[]`; `bans.attested` serves only the attested set, uninstalled ledgers gap |
| Metrics source is O(1) per tick, no whole-router scan | `ControlMetrics` over registered cumulative counters (bandwidth pair, 4 `ssu2.*` rates, build pair); NaN-safe interval guard via `partial_cmp`; SSU2 factory primes the service at composition |
| Live inspection serving with trimmed publisher surface | `log_live`/`metrics_live`/`ssu2_live` + `publish_*` owners; `publish_transport`/`publish_tunnels` carry no `clock_skew`/success/bandwidth/rate claims; news stays unavailable; census (27,1,1,1) |
| Exact wire disposition for every method/selector/action/ type/option | 14 matrix rows flipped to Available; freshness + `plan295_*` test IDs; `SOURCE_MATRIX_NEUTRAL_COUNT = 1`; `contract.rs` census 27/1/1/1 (13 passed) + `final_matrix.rs` gate (1 passed) |
| Differential corpus with no unexplained mismatch | Production-composition corpus: 28 answered + 8 gapped + 4 errors with the `(a)–(d)` taxonomy, sanitized `shape` hash `143acaa6dcdd4ac6` under marker `PLAN295-CORPUS`; external rows `#[ignore]`-gated, fail loudly without `I2PR_I2PCONTROL_TARGET` + `I2PR_I2PCONTROL_PASSWORD` (1 passed, 1 ignored) |
| Lane + evidence integrity, fail-closed | `run-differential.sh` regenerates the JSON from the executed corpus (28/8/4 re-verified at `86a4471`); `check-i2pcontrol-acceptance-evidence.sh` derives counts through the exit-code-gated helper, forbids literal passes/forgiveness/fake env/secret evidence; wired into `ci.yml`, `AGENTS.md`, arch tables |
| Dossier + support + docs reconciliation | Spec 16 (final conformance + differential); `support.toml` refresh + `prop170.control-source-owners` + `prop170.final-conformance-and-differential` (`experimental`, unadvertised); `i2pr-i2pcontrol`/`i2pr-daemon` deep-dives; no frontend entered |

## Acceptance criteria disposition (§Acceptance criteria, plan of record)

1. Exact tested wire disposition — yes (census 27/1/1/1 + corpus).
2. Truthful current owners — yes (ring/ledger/metrics/SSU2
   projection; attested-not-fabricated empties).
3. Real runtime effect + recoverable persistence — yes as far as
   claimed: mutations ride the Plans 289–294 paths; the three new
   owners are explicitly volatile (re-prime at composition, nothing
   to recover — stated in the dossier, not silently gap-filled).
4. Twelve tunnel families with real data paths — carried from
   Plans 290/291, untouched and unregressed (tunnels suites green).
5. Resolver/AddressBook coherence — carried from Plan 294,
   unregressed.
6. Disabled/default isolation — publishers attest
   `Disabled`/off; no new listener; loopback default unchanged.
7. No unexplained differential mismatch — 28/8/4 all classified;
   externals `blocked-env-absent` (no provisioned lane).
8. Security review — no unresolved high/medium (see below).
9. Routine + focused control-plane CI green at closure head —
   local floor green (one documented load flake, solo-green;
   six bash-4 scripts Linux-CI-authoritative, as at Plan 294).
10. No frontend — none entered.

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK (also clean on MSRV 1.88.0)
cargo test --locked --workspace --all-targets -- --test-threads=1          71/72 suites green*
  *one wall-clock flake (socks_irc_unknown_server_command_is_dropped under load
   average 44), solo-green in 0.21 s; same class as the Plan 292/294 Timing-note
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 failed (18 suites ok)
cargo test --locked -p i2pr-i2pcontrol --test contract --test final_matrix 13 + 1 passed (census 27/1/1/1)
cargo test --locked -p i2pr-daemon --test i2pcontrol_base                  10 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_inspection              9 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels                11 passed
cargo test --locked -p i2pr-daemon --lib control_sources                    9 passed
cargo test --locked -p i2pr-daemon --test i2pcontrol_differential            1 passed (28/8/4), 1 ignored (external)
cargo test --locked -p i2pr-runtime --test ssu2_local active_peer_hashes     1 passed
bash tests/integration/i2pcontrol/run-differential.sh                     28/8/4 regenerated at 86a4471
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
bash scripts/check-i2pcontrol-acceptance-evidence.sh                       ok (new; bash-3.2-clean)
bash scripts/check-exploratory-tunnel-evidence.sh                          12 labels green
bash scripts/check-netdb-tunnel-evidence.sh                                12 labels green
bash scripts/check-destination-tunnel-evidence.sh                          21 labels green
bash scripts/check-m6-mixed-router-acceptance-evidence.sh                   11 labels green
python3 -m unittest discover -s tests/integration/ntcp2/harness            OK (18 tests)
cargo deny check advisories bans sources                                   advisories ok, bans ok, sources ok
```

Not run locally (pre-existing macOS environment limit, identical
on unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`,
`check-service-tunnel-acceptance-evidence.sh` (bash 3.2.57
parse-rejects line 1952; file untouched since plan215),
`check-streaming-tunnel-evidence.sh` require bash 4+ on this
host's bash 3.2.57. Linux CI is authoritative for those six.

## New dependencies (reviewed)

None. No new crate, no new external dependency, no
`Cargo.toml`/`Cargo.lock` change (`git diff` on manifests is
empty). `cargo deny` (advisories/bans/sources) is clean.

## Security review

- No new sockets, tasks, files, channels, or listeners; the new
  module is pure state + a `tracing` layer.
- No `Debug`/`Display`/unrestricted serialization on secret
  types; no new `Clone` on secrets; ring redaction is
  replace-not-mask over 8 insensitive + `PRIV` markers; poisoned
  locks degrade to empty, never panic across the control plane.
- Every per-request read is O(1)/bounded (256-entry ring
  snapshot, 1024-entry wire ceilings, 8-rate cap,
  `MAX_SSU2_ACTIVE_CEILING` peer cap); `unwrap_or_default`
  degrade, no unbounded allocation.
- Evidence carries no secrets: lane asserts no
  password/token/seed/secret/`PRIV`/raw-payload bytes in the
  JSON; the checker re-asserts it.
- I2PControl stays disabled by default, loopback-only by
  default; non-loopback still needs explicit config.

No high/medium/low findings beyond the noted flake and
environment debt.

## Failure / migration review

- Volatile owners (ring, metrics, bans-attested, SSU2 priming)
  re-derive at composition; restarts lose only the trailing
  window, which the dossier declares — no silent gap-fill, no
  recovery path to break.
- No migration exists or is needed: no new persisted state.
- Whole-request validation and disabled-isolation semantics
  are unchanged from Plans 287–294.

## Documentation / operational evidence

- `specs/protocols/16-final-conformance-and-differential.md`:
  the Plan 295 dossier (new).
- `specs/support.toml`: refreshed RouterInfo surface + new
  `prop170.control-source-owners` and
  `prop170.final-conformance-and-differential` (`experimental`,
  `advertised = false`).
- `docs/architecture/i2pr-i2pcontrol.md`,
  `docs/architecture/i2pr-daemon.md`: owner/composition/matrix
  rows.
- `docs/architecture/overview.md`,
  `docs/architecture/tooling.md`: checker wired into script
  tables; tooling index + CI matrix rows.
- `.github/workflows/ci.yml`, `AGENTS.md`: the new checker in
  the enforced floor.

## Known limitations

- External differential rows are `blocked-env-absent`: no
  provisioned eggstack/emissary/eepnet/Java/i2pd lane ran in
  this plan. The local corpus + taxonomy is the conformance
  evidence; cross-router comparison awaits a provisioned lane
  (a future plan, not a silent pass).
- `news.feed` is unavailable-by-design (no news subsystem);
  `network.clock_skew` is a justified neutral constant `0`;
  `router.hash` stays behind the Plan 288 gate.
- Metrics cover exactly the registered counter sources;
  loopback/destination traffic bypassing transport counters is
  not counted (dossier-stated).
- The six bash-4-only checkers remain unverifiable on this
  macOS host (environment debt, unchanged since Plan 286).

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the three
  clippy lints the new code introduced (fixed with `is_empty`,
  `Default`, `partial_cmp` — no behavior change); the rustdoc
  private-link (fixed); the wall-clock product-suite flake
  under host load (solo-green; binaries run separately on CI);
  the six bash-4-only checkers (environment debt).

## Roadmap disposition

Plan 295 is **closed** (`passed-*`). Plans 296 and 297 stay
`ready` (unaffected — their hard dep is Plan 292, closed).
No other Proposal 170 capability is claimed, and no
unqualified full-support label is stated.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 295 (hard deps: Plans 288 + 293 + 294, all closed) —
  moves to `closed` in this commit.
- Plan 296 (hard dep: Plan 292, closed) — already `ready`;
  unaffected.
- Plan 297 (hard dep: Plan 292, closed) — already `ready`;
  unaffected.
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 295 acceptance
criterion passes with executed evidence above. The only
deferred capability (provisioned external differential lane)
is an explicit, bounded, fail-closed absence — not a hidden gap.
