# Plan 286 status — parallel authority, provenance, and contract foundation

Status: **`passed-prop170-parallel-authority-provenance-and-contract-foundation`**.

Plan of record: [`286-parallel-authority-provenance-and-contract-foundation.md`](../../implementation/i2pcontrol-proposal-170/286-parallel-authority-provenance-and-contract-foundation.md).

## Implementation commits

- `8738544` — `plan(286): parallel authority provenance and contract foundation`
  (new `i2pr-i2pcontrol` crate, provenance manifest, `CONFORMANCE.md`
  support model, boundary-script + allowlist enforcement, `serde_json`
  workspace dependency).

This closure commit (registry + roadmap + unblock audit) lands on top of
`8738544` with no production-code change.

## Requirement-to-evidence matrix

| Plan 286 requirement | Evidence |
|---|---|
| A. Exact fork provenance manifest | `docs/provenance/proposal-170-manifest.md`: frozen pins (Proposal 170 rev 2026-05-20, base API-1 docs 2026-07-10, eggstack/emissary `6885a94`, eepnet/emissary `9b43484`, Java PR 6 `45bb593`, i2pd `2d57d3f`), upstream-absence statement, per-path R/B/X classification, zero-reuse statement for Plan 286 |
| B. New runtime-neutral crate | `crates/i2pr-i2pcontrol` (`#![forbid(unsafe_code)]`, deps only `serde`, `serde_json`, `thiserror`): `jsonrpc`, `auth`, `methods`, `router_info`, `client_services`, `address_book`, `tunnel`, `tunnel_options`, `limits`, `conformance` modules |
| B. Dependency review for serde/serde_json | Recorded in `crates/i2pr-i2pcontrol/src/lib.rs` docs (purpose, maintainer, transitive impact, license, `unsafe` exposure, feature set, untrusted-input handling); centralized workspace versions; `default-features = false` + `std` for `serde_json` |
| B. Crate independence | `check-dependency-direction.sh`: `i2pr-i2pcontrol` allows zero `i2pr-*` edges; only `i2pr-daemon` gains the new edge |
| C. Exact inventories | 5 methods; 30 RouterInfo selectors w/ return types (6 address-book); 6 ClientServicesInfo keys; 4 book types + 6 fields + 13 SetConfig keys; 7 actions; 12 types; 46 options w/ sensitivity/applicability; wire ceilings; `ContractInventory` |
| C. Unknown literals representable | `UnknownLiteral` vs `CaseMismatch` typed distinction on every parse surface |
| D. Conformance dimensions | `specs/CONFORMANCE.md` §Proposal 170 support model (wire, source, runtime effect, persistence/atomicity, feature isolation, security/secret handling, evidence) |
| D. support.toml | Deferred per plan allowance (no schema row added; registration claims nothing) |
| E. Boundary enforcement | `check-dependency-direction.sh` + `check-runtime-boundaries.sh` + `docs/architecture/dependency-graph.md` updated (contract-crate neutrality, daemon-only adaptation edge, no UI deps) |
| No listener/token/TLS/adapter/persistence/frontend code | `git show 8738544 --stat` contains only the contract crate, manifest, conformance prose, boundary scripts, and dependency-graph doc |

## Tests and guards run (local; macOS host)

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK
cargo test --locked --workspace --all-targets -- --test-threads=1          3243 passed, 0 failed
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                      0 doc tests, 0 failed
cargo test --locked -p i2pr-i2pcontrol --all-targets                       11 passed (contract.rs)
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
python3 -m unittest discover -s tests/integration/ntcp2/harness            18 tests OK
cargo deny check advisories bans sources                                   advisories ok, bans ok, sources ok
```

Not run locally (pre-existing macOS environment limit, identical on
unmodified `main`): `check-fixture-manifest.sh`,
`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`,
`check-i2cp-vectors.sh`, `check-service-tunnel-acceptance-evidence.sh`,
`check-streaming-tunnel-evidence.sh` require bash 4+ associative arrays
(`declare -A`); this host ships bash 3.2.57 only. None of their areas
(fixture bytes, vectors, service-tunnel/streaming evidence) are touched
by this plan; Linux CI is authoritative for those six.

## Migration / compatibility

No wire, config, storage, or API surface existed before this plan; there
is nothing to migrate. The new crate is additive and no existing crate
gains a dependency on it. `Cargo.lock` gains only the `serde_json`
(+ `itoa`/`ryu`) subtree.

## Security review

- Secret-bearing surface: none added (auth module is vocabulary +
  ceilings only; no storage, no clocks, no `Debug` on secrets).
- `tunnel_options::SECRET_OPTIONS` freezes the 4 secret-classified
  options; the contract test proves the secret set is exactly those 4
  and every option applies to ≥ 1 family.
- Untrusted-input posture: all decoders enforce caller-visible ceilings
  before allocation, distinguish truncation/malformed/unknown/case, and
  consume exact envelope shapes; max/max+1 tests cover every ceiling.
- `cargo deny` clean; `forbid(unsafe_code)` on the new crate enforced by
  workspace lints + boundary script.

## Documentation / operational evidence

- `docs/provenance/proposal-170-manifest.md` (pins + classification).
- `specs/CONFORMANCE.md` Proposal 170 support model.
- `docs/architecture/dependency-graph.md` allowlist mirrors the script.
- Crate-level rustdoc with frozen pins + dependency review; `cargo doc`
  warning-free.

## Known limitations

- The frozen inventories are the workstream contract pinned to Proposal
  170 rev 2026-05-20 as interpreted through the plan's explicit
  requirements; literal donor-file blob hashes are recorded at first
  literal reuse (Plan 287 reuses bounded behavior only).
- `specs/support.toml` intentionally unchanged until the first
  capability closes.

## Findings by severity

- Critical: none. High: none. Medium: none. Low: the six bash-4-only
  checkers are unverifiable on this macOS host (environment debt, not a
  plan defect); CI covers them.

## Roadmap disposition

Plan 286 is **closed** (`passed-*`). Plan 287 becomes dependency-ready;
no other Proposal 170 capability may be claimed yet. See the unblock
audit below.

## Unblock audit (required)

Audited `plans/registry.md` blocked work + the Proposal 170 roadmap §6
graph against the just-closed milestone:

- Plan 287 (hard dep: Plan 286) — all hard deps now closed → moves to
  `ready` in this commit.
- Plans 288, 289, 294 (hard dep: Plan 287, still open) — remain
  `blocked on 287`. Not unblocked.
- Plans 290, 291 (hard dep: Plan 289) — remain blocked. Not unblocked.
- Plan 292 (hard deps: Plans 290 + 291) — remains blocked. Not unblocked.
- Plan 293 (hard dep: Plan 292) — remains blocked. Not unblocked.
- Plan 295 (hard deps: Plans 288 + 293 + 294) — remains blocked. Not unblocked.
- M12/mainline plans — unaffected (parallel workstream per ADR 0028).

No corrective pass is required: every Plan 286 acceptance criterion
passes with executed evidence above.
