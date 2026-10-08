# Plan 379 status — MIT license and portable service-tunnel cleanup

Status: **`passed-mit-license-and-portable-service-tunnel-cleanup-with-named-crates-io-blocker`**.

Plan of record: [`379-mit-license-and-portable-service-tunnel-cleanup.md`](../../implementation/portable-service-tunnels/379-mit-license-and-portable-service-tunnel-cleanup.md).

Repository baseline: `4f4e98f5a0241355af5099c73065088dede1b1de` on `main`.

## Implementation

| Commit | Work package |
| --- | --- |
| `dc773f4f815479d4e56c59690d0dc74e2b58b98a` | Plan 379 registration (pre-existing): MIT `LICENSE`, README license section, `[workspace.package] license = "MIT"` |
| `cc9985c1763b175ea7770e163a7d1343c78b27c5` | WP A — license metadata convergence + `scripts/check-license-metadata.py` + floor/CI/`tooling.md` wiring |
| `46f082c11d40809cec3b83e170c029982fa29280` | WP B — package/distribution audit, named blocker, stale-doc correction, `deny.toml` comment fix |
| `f0fb74a8582d6077a7c5db49d613699688115bad` | WP D — Plan 359 closure record, roadmap/registry reconciliation, `dbowm91/i2pr-sam` named, planning-test repair |
| `03ea111c040e4469f3abf7ab42f7e6b806aefc3b` | WP C — current external-consumer fixture + two-fixture checker + untracked-artifact rule |

WP E (branch disposition) required no commit: it classified and deleted a remote branch.

## What the plan found

The plan asked for an audit rather than an assumption, and the assumption would have
been wrong twice.

**The license was never in Cargo.** The registration commit recorded MIT in `LICENSE`,
`README.md`, and `[workspace.package]`. It recorded it nowhere Cargo could read it: not
one of the 26 member manifests inherited the workspace license, so
`cargo metadata --no-deps` reported `license = null` for **every** package. A
`[workspace.package] license` key that no member inherits is a comment — no compiler,
no test, and no existing checker would ever report its absence.

**MIT was not the only publication gate.** `cargo package -p i2pr-service-tunnels`
fails, and not for a licensing reason:

```text
all dependencies must have a version requirement specified when packaging.
dependency `i2pr-proto` does not specify a version
```

The Plan 359 edge is `i2pr-proto = { path = "../i2pr-proto" }`. That is correct for a
Git consumer — a dependency from the same repository resolves its path inside the
checkout — but `cargo package` must stage a registry-resolvable dependency. The
posture is therefore **`git-consumable`, and `crates.io-blocked-by-publishable-versioned-i2pr-proto`**.

The prerequisite is satisfiable, which is the genuinely useful result of the audit:
`cargo package -p i2pr-proto` stages 34 files and **verifies cleanly today**. The
blocker is a policy choice on `i2pr-proto`, not an unpackageable crate.

## Requirement-to-evidence matrix

| # | Acceptance criterion | Evidence | Result |
| --- | --- | --- | --- |
| 1 | Repository license is MIT and Cargo metadata agrees for production packages | `LICENSE`, `README.md`, `[workspace.package]`, and `license.workspace = true` in all 26 member manifests; `cargo metadata --no-deps` reports `license = "MIT"`, `license_file = null` for every member | Pass |
| 2 | Clean-room/provenance rules remain explicit | `README.md` `## License` retains the clean-room/provenance clause, the ADR 0028 narrow exception, and its exclusions; enforced as guard rule 6 | Pass |
| 3 | Current package distribution posture is named and evidenced | `docs/architecture/i2pr-service-tunnels.md` §"Distribution posture"; `cargo package` output recorded verbatim; the three-step unblock chain recorded with its plan-of-record requirement | Pass |
| 4 | A post-Plan-359 external Git consumer passes against a fixed revision | `bash scripts/check-portable-service-tunnel-consumer.sh` → 11 tests pass at pinned `f0fb74a8`; lockfile resolves `i2pr-service-tunnels` **and** `i2pr-proto` from that one revision | Pass |
| 5 | Plan 359 has a conventional additive closure record | `plans/closure/portable-service-tunnels/359-status.md`, stating that Plan 379 wrote it and did not execute Plan 359 | Pass |
| 6 | Historical Plan 349–351 closures are not rewritten | `git diff main -- plans/closure/portable-service-tunnels/349-status.md 350-status.md 351-status.md` is empty | Pass |
| 7 | Current planning points SAM work to `dbowm91/i2pr-sam` | `specs/references/portable-service-tunnel-sam-adapter-handoff.md`, roadmap §12, registry milestone-authority bullet | Pass |
| 8 | Obsolete branch safely deleted or retained for a specific reason | Deleted after classification — see §Branch disposition | Pass |
| 9 | Generated fixture build artifacts are not tracked on `main` | `git ls-files -- 'tests/**/target/**'` → 0 files; the same pathspec matches 409 files on the deleted branch. Now asserted by the consumer checker | Pass |
| 10 | No runtime/protocol/support behaviour changed | No `.rs` file under `crates/` changed in any Plan 379 commit; `specs/support.toml`, listeners, and defaults untouched | Pass |

## Package-by-package license metadata

`cargo metadata --format-version 1 --no-deps`, run on the closure head:

| Package | License | `publish` |
| --- | --- | --- |
| `i2pr-addressbook`, `i2pr-api`, `i2pr-app-fixture`, `i2pr-app-manager-proto`, `i2pr-app-proto`, `i2pr-appd`, `i2pr-apphost`, `i2pr-client`, `i2pr-core`, `i2pr-crypto`, `i2pr-daemon`, `i2pr-i2pcontrol`, `i2pr-interop`, `i2pr-netdb`, `i2pr-netdb-persist`, `i2pr-proto`, `i2pr-runtime`, `i2pr-service-tunnels`, `i2pr-storage`, `i2pr-su3`, `i2pr-testkit`, `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2`, `i2pr-tunnel` | `MIT` | `false` |
| `i2pr-console` | `MIT` | *unset* |

All 26 report `license_file = null`.

**Intentional exceptions, recorded rather than silently normalized:**

- Every package except `i2pr-console` declares `publish = false`. That is correct:
  none of them is a released library.
- `i2pr-console` leaves `publish` unset. It is pre-existing (Plan 356–358), outside this
  plan's scope, and harmless — Cargo's default is to publish, and publication is a
  manual act nobody has performed. **Plan 379 does not change it.** Flagged in §Findings
  as a low-severity item for a future plan to decide deliberately.
- `i2pr-app-fixture` and `tools/i2pr-interop` are non-production evidence tooling and
  stay `publish = false`.
- `[licenses] allow = []` in `deny.toml` is still empty. Enabling
  `cargo deny check licenses` means reviewing every *transitive* dependency's license —
  a provenance exercise, not a metadata change. Out of scope here; the stale comment
  claiming no license had been selected was corrected.

## External-consumer evidence

| Fixture | Pin | Tests | Meaning |
| --- | --- | --- | --- |
| `tests/portable-service-tunnel-consumer/` | `fa0824970b67b5ee90d5907765c408ccd0a19941` (Plan 350) | 8 pass | The boundary held **before** the `i2pr-proto` edge existed |
| `tests/portable-service-tunnel-consumer-current/` | `f0fb74a8582d6077a7c5db49d613699688115bad` (Plan 379) | 11 pass | The same boundary today, **including** the permitted edge |

Both are standalone Cargo workspaces outside the root workspace, both build through a
temporary `CARGO_TARGET_DIR`, and neither uses a `path` dependency or names a private
module. The historical pin was **not** repointed: it is the comparison baseline, and
Plan 379's own instruction was not to destroy it.

The current fixture's lockfile is the concrete proof that the permitted edge is
reachable from outside:

```toml
[[package]]
name = "i2pr-proto"
version = "0.1.0"
source = "git+https://github.com/dbowm91/i2pr?rev=f0fb74a8...#f0fb74a8..."
```

Three tests are new and exist only because of that edge: a valid b33 parses and exposes
a structured address; a b33-shaped failure is reported as an *encrypted-service* problem
and never as "Base32 label must be exactly 52 characters"; and `StaticAliasTable`
refuses an encrypted-service target. Addresses are embedded as literals, so the fixture
depends on `i2pr-service-tunnels` alone and the parse/validate/expose path is proven on
data it never had to construct.

## Branch disposition (WP E)

`origin/plans/349-351-portable-service-tunnels` was classified before deletion. Deleted
with owner authorization once the classification proved nothing unique remained.

| Evidence | Finding |
| --- | --- |
| `git cherry main origin/plans/349-351-portable-service-tunnels` | 10 of 11 commits patch-equivalent on `main` (`-`) |
| The 11th, `25b95443` | Added 7 lines to `plans/registry.md` describing Plans 349–351 as ready/blocked at registration. Superseded: the roadmap, all three plan documents, all three closure records, and 7 current registry rows exist on `main` |
| `comm -23` of the branch tree vs `main`, excluding `target/` | Exactly **one** unique non-generated file: `docs/adr/0032-portable-service-tunnel-policy-core-and-adapters.md` |
| That file vs main's `docs/adr/0033-portable-service-tunnel-policy-core-and-adapters.md` | **Byte-identical except line 1** (`ADR 0032:` vs `ADR 0033:`) — the double-filed duplicate the `2416c30b` merge dropped, independently re-verified here and already recorded in `plans/global-number-collision-ledger.md` |
| Tracked `target/` artifacts | 409 files on the branch, **0** on `main`; disposable and never merged |
| Action | `git push origin --delete plans/349-351-portable-service-tunnels`, confirmed absent by `git ls-remote`. No local branch existed. |

## Verification

Run from the repository root on the closure head. **All results below are local runs on
macOS with bash 3.2.57 and BSD userland tools; none is a CI run.**

### Plan §11 named commands

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo check --locked --workspace --all-targets` | Passed |
| `cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1` | Passed: **357 passed, 0 failed** |
| `cargo clippy --locked -p i2pr-service-tunnels --all-targets --all-features -- -D warnings` | Passed |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked -p i2pr-service-tunnels --no-deps` | Passed |
| `cargo metadata --format-version 1 --no-deps` | 26 packages, all `license = "MIT"`, `license_file = null` |
| `cargo tree -p i2pr-service-tunnels --edges normal` | `base64ct`, `i2pr-proto`, `sha2`, `subtle`, `thiserror`, `zeroize` — one internal crate, the Plan 359 edge |
| `python3 scripts/check-portable-service-tunnel-api.py` | Passed: 696 declarations |
| `bash scripts/check-service-tunnel-boundaries.sh` | Passed |
| `bash scripts/check-portable-service-tunnel-consumer.sh` | Passed: 8 + 11 tests, both exact pins, no generated artifact tracked |
| `python3 scripts/check-global-plan-number-uniqueness.py` | Passed |
| `python3 scripts/check-adr-number-uniqueness.py` | Passed |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed: 51 tests |

### Plan §10 required tests

| Requirement | Evidence | Result |
| --- | --- | --- |
| Cargo metadata license audit for production crates | `scripts/check-license-metadata.py` rule 5, against resolved `cargo metadata` | Pass |
| Portable-core API snapshot checker | `check-portable-service-tunnel-api.py`, 696 declarations | Pass |
| Service-tunnel dependency/runtime boundary checker and positive controls | `check-service-tunnel-boundaries.sh`; in-script positive controls; Plan 359 re-run: **12/12** forbidden classes rejected, **3/3** controls pass | Pass |
| Current external-consumer conformance lane | `check-portable-service-tunnel-consumer.sh`, current fixture at `f0fb74a8` | Pass |
| Focused `i2pr-service-tunnels` tests | 357 passed | Pass |
| Global plan-number and ADR uniqueness tests | Both checkers plus 51 planning tests | Pass |
| Negative proof a non-`i2pr-proto` workspace dependency remains rejected | Plan 359 mutation re-run; also the consumer checker's 23-name positive control | Pass |
| Proof generated fixture target contents are not tracked on `main` | `git ls-files -- 'tests/**/target/**'` → 0; pathspec proven against the deleted branch's 409 | Pass |

### Full routine floor (`AGENTS.md`)

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo check --locked --workspace --all-targets` | Passed |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost` | Passed (run before any `-p i2pr-daemon` work, per the AGENTS.md rationale) |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed: exit 0, **0 failures**. 4,631 tests enumerated by `cargo test -- --list` (includes ignored) |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | Passed |
| `cargo test --locked --workspace --doc` | Passed |
| `bash scripts/check-dependency-direction.sh` | Passed |
| `python3 scripts/check-global-plan-number-uniqueness.py` | Passed |
| `python3 scripts/check-adr-number-uniqueness.py` | Passed |
| `bash scripts/check-portable-service-tunnel-consumer.sh` | Passed |
| `bash scripts/check-portable-service-tunnel-consumer.sh` (tracked-artifact rule) | Passed |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed: 51 tests |
| `bash scripts/check-runtime-boundaries.sh` | Passed (8 rules, grouped-import positive controls) |
| `bash scripts/check-console-boundaries.sh` | Passed |
| `bash scripts/check-console-browser-security.sh` | Passed |
| `python3 scripts/check-tooling-inventory.py` + `--self-test` | Passed; self-test ok |
| `python3 scripts/check-license-metadata.py` + `--self-test` | Passed; self-test ok (new in Plan 379) |
| `python3 scripts/check-workflow-validity.py` | Passed: 10 workflows valid |
| `python3 scripts/check-license-metadata.py` in `ci.yml` | Wired |
| `python3 scripts/check-managed-app-gateway-boundary.py` | Passed |
| `python3 scripts/check-managed-app-manager-boundary.py` | Passed |
| `python3 scripts/check-managed-app-private-client-seams.py` | Passed |
| `python3 scripts/check-managed-app-process-boundary.py` + `--self-test` | Passed |
| `bash scripts/check-service-tunnel-boundaries.sh` | Passed |
| `bash scripts/check-service-anonymity-boundaries.sh` | Passed |
| `bash scripts/check-m11-per-epoch-composition.sh` | Passed |
| `bash scripts/check-m11-transit-boundaries.sh` | Passed |
| `bash scripts/check-m12-floodfill-boundaries.sh` | Passed |
| `bash scripts/check-outproxy-request-path.sh` | Passed |
| `bash scripts/check-outproxy-wire-lane-evidence.sh` | Passed |
| `bash scripts/check-config-secret-hygiene.sh` | Passed |
| `bash scripts/check-encrypted-service-consumer-caller.sh` | Passed |
| `bash scripts/check-constrained-host-lane-boundary.sh` | Passed |
| `bash scripts/check-ntcp2-interoperability.sh` | Passed |
| `bash scripts/check-floodfill-type5-serve.sh` | Passed |
| `bash scripts/check-exploratory-tunnel-evidence.sh` | Passed |
| `bash scripts/check-netdb-tunnel-evidence.sh` | Passed |
| `bash scripts/check-destination-tunnel-evidence.sh` | Passed |
| `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | Passed (existing non-blocking coverage warnings) |
| `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | Passed; expected negative-fixture diagnostics printed |
| `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | Passed: 18 tests |
| `cargo deny check advisories bans sources` | Passed: `advisories ok, bans ok, sources ok` (existing duplicate-version warnings) |

### Not runnable on this host — reported, not claimed as passing

These are the documented macOS bash-3.2 traps from `AGENTS.md` plus one newly
observed. None is content drift and **none was "fixed" to work around the host**, per
`AGENTS.md`.

| Command | Local result | Why |
| --- | --- | --- |
| `bash scripts/check-fixture-manifest.sh` | exit 2 | `declare: -A: invalid option` — documented bash 3.2 trap |
| `bash scripts/check-ntcp2-vectors.sh` | exit 2 | same |
| `bash scripts/check-ssu2-vectors.sh` | exit 2 | same |
| `bash scripts/check-i2cp-vectors.sh` | exit 2 | same |
| `bash scripts/check-streaming-tunnel-evidence.sh` | exit 2 | same (`declare -A` at `HELPER_USAGE`) |
| `bash scripts/check-service-tunnel-acceptance-evidence.sh` | exit 2 | documented heredoc-inside-`$( )` mis-parse; surfaces as `line 1970: syntax error near unexpected token '('` |
| `bash scripts/check-java-source-lock-gating.sh` | not run | `mapfile` (exit 127) |
| `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | exit 1 | **new observation, see §Findings** — BSD `sed` cannot run `sed -i 's/…/\n…/'` |

## Invariants, compatibility, migration, security

- **M10 Plan 215 remains the service-tunnel product authority.** No product behaviour,
  listener, default, or config key changed.
- **No `.rs` file under `crates/` changed.** All production changes are manifests,
  documentation, and checkers.
- **Plan 359 stays narrow.** `i2pr-proto` remains the only permitted workspace
  dependency of `i2pr-service-tunnels`; 12/12 forbidden classes and 23 fixture-side
  forbidden names are still rejected.
- **No crate was published.** `publish` was not changed on any package, and no
  `cargo publish` was run.
- **No protocol, support-inventory, or advertisement change.** `specs/support.toml` is
  untouched; no anonymity or privacy claim is made or implied.
- **Migration:** none. The MIT selection changes repository licensing, not runtime
  compatibility — no config, wire, storage, or user migration exists.
- **Secrets:** none touched. The new guard reads manifests and documentation only.
- **Security-relevant decisions:** two guards were *added or tightened* (license
  metadata, consumer fixture set + tracked-artifact rule); no guard was weakened.
- **Contention:** none encountered. The Plan 379 branch was based on current `main`
  (`4f4e98f5`) and pushed as its own branch.

## Documentation and operations

- `LICENSE`, `README.md` — MIT selection and the retained clean-room clause (WP A/D).
- `docs/architecture/tooling.md` — new inventory row plus every count Plan 372's
  inventory guard derives (WP A).
- `docs/architecture/i2pr-service-tunnels.md` — §Dependencies corrected (it still
  claimed Plan 350 had left "no internal crate dependency") and new
  §"Distribution posture" (WP B).
- `crates/i2pr-service-tunnels/README.md` — publication blocker restated accurately (WP B).
- `deny.toml` — stale "no license selected" comment corrected (WP B).
- `specs/references/portable-service-tunnel-sam-adapter-handoff.md` — `dbowm91/i2pr-sam`
  named, two fixtures distinguished, corrective path stated. SHA-256 is now
  `58bb7cd87691cd24bf5ed78eafa385e5300a1b6a7376564808bc5e9d9e3da811`; Plan 351's record
  of the previous hash is **left intact** because it was true when written.
- `plans/subsystems/portable-service-tunnels-roadmap.md`, `plans/registry.md` — current
  state reconciled (WP D/E).
- `AGENTS.md`, `.github/workflows/ci.yml` — new guard wired into the floor and CI (WP A).
- Operational impact: none. No runtime, listener, or default is affected.

## Limitations

- The license guard is **repository-local and structural**. It cannot police
  transitive dependency licenses; that remains `cargo deny check licenses`, which is
  still disabled.
- The distribution audit is a **point-in-time** result. Adding a `version` to the
  `i2pr-proto` edge would invalidate it.
- Step 2 of the unblock chain — `version = "0.1"` on the edge — pins a public minimum
  `i2pr-proto` version for external consumers. It needs its own plan of record, not a
  cleanup pass.
- The current consumer fixture is pinned to a **revision**, so it does not automatically
  follow new work. Advancing the pin is a deliberate act with review.
- Both fixtures execute the same eight policy/filter tests. They are duplicated on
  purpose (see WP C's commit message); divergence between the copies is a deliberate
  signal, not a defect.
- Nothing here re-opens Plan 359's guard amendment or changes its status token.

## Findings by severity

- **critical**: none.
- **high**: none.
- **medium**: none.
- **low** (4, all recorded rather than acted on, as fixing them is out of this plan's
  scope):
  1. `check-m12-floodfill-boundaries.sh --self-test` cannot run on macOS. Its
     `--self-test` path uses `sed -i 's/…/\n…/'`, which is GNU-sed-only; BSD `sed`
     rejects it (`extra characters at the end of n command`). The file is
     **byte-identical to `main`** (last touched by `b281d497`, Plan 360–366) and the
     bare run passes, so this is not a Plan 379 regression and not content drift. It is
     **not** the same trap as the bash-3.2 list in `AGENTS.md` — Homebrew bash 5 would
     not fix it, because it is `sed`, not `bash`. It is the only floor script using
     `sed -i`. Needs a plan-of-record if it should be made portable.
  2. `i2pr-console` is the one package with `publish` unset, so Cargo's default
     (publish) applies. Pre-existing since Plan 356–358 and harmless in practice, but it
     is the only member whose release posture is not stated explicitly.
  3. Plan 359's original mutation table listed 9 forbidden dependency classes while its
     pattern matches 12. The table was a subset, not an error.
  4. `tests/planning/test_tooling_inventory.py` mutated a **hardcoded** published
     count, so any legitimate change to that count silently turned the rule-4 mutation
     test into a no-op. Repaired here (see WP D), and verified honest by neutering
     rule 4 in a throwaway copy and confirming the test then fails.

## Unblock audit

Plan 379 was registered with "no hard code dependency", and nothing in this closure
creates one. Audited the registry's blocked/ready work for anything listing Plan 379, or
the portable-service-tunnel line it sits in, as a dependency:

- **Proposal 170/373–378** — the ELS2/conformance continuation line. Does not depend on
  Plan 379 or on the portable line; its own blockers are i2pd/Java live qualification
  and outproxy resilience. No state change.
- **SAM 3.1 (Plan 151 closed authority) and the `sam33_extension_sequence`** — the
  handoff now *names* `dbowm91/i2pr-sam` as the SAM owner, which removes the ambiguity
  Plan 351 left ("a future independent repository") but creates no in-i2pr dependency.
  No state change.
- **Plans 349–351 and 359** — all closed; their records are unchanged.

**No plan moves to `ready` as a result of this closure.** Nothing was blocked on Plan
379. No corrective pass is required: all four low findings are either environment-only
or explicitly out of scope, and none is a product defect.

Roadmap disposition: **closed with a named, evidenced, un-actioned crates.io blocker.**
No crate published, no support or capability claim promoted, M10 untouched.

## Handoff

i2pr-side portability cleanup is complete. `dbowm91/i2pr-sam` is the named downstream
SAM consumer and can pin either fixture revision; the current one
(`f0fb74a8`) is the one that proves the permitted `i2pr-proto` edge works from outside
the workspace.

If that repository discovers a genuinely missing transport-neutral policy seam, file a
new corrective in i2pr with an external failing fixture. Do **not** move SAM ownership
back into this repository. If a crates.io release is ever wanted, the first step is a
plan-of-record for publishing `i2pr-proto` at `0.1.0`; nothing in this plan authorizes
it.