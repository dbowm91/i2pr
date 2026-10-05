# Doc, skill, and planning hygiene pass — 2026-10-05

Scope: skills, `AGENTS.md`, `README.md`, and the docs that lag their own
declared source. This follows the same pass earlier the same day
(`audit/2026-10-05-doc-audit.md`), which re-audited the 19 per-crate deep dives
and found them accurate as of `2bce42f2`. **That verdict still holds and those
files were not touched here.** This pass found the drift in the *navigation and
skills* layer that sits above the deep dives.

## Method

Three read-only audits in parallel (`explore`), every claim re-verified
directly against the tree before being written down:

1. `i2pr-local-dev` (82 KB, untouched since 2026-09-22) against `crates/` and
   `plans/`.
2. `i2pr-architecture` (44 KB) plus `plans/` and the root docs.
3. The three historical bundles (`i2pr-ntcp2-interop`, `i2pr-rootless-sandbox`,
   `i2pr-multipass-recovery`) for dangling paths and survival value.

Plus a link-resolution sweep over every skill and `specs/support.toml`
self-consistency checks.

## Headline finding: two skills had become status mirrors

`i2pr-local-dev` carried a 342-line `plan_NNN = <token>` ledger ending at Plan
~250. `i2pr-architecture` carried a 376-line chronological plan-of-record index
ending at Plan ~215. Both duplicate `plans/registry.md` and
`plans/closure/`, and both had gone roughly 90 plans stale while those
moved. Under the repo's own authority order (closure records > tests/scripts >
ADRs > prose) both were asserting stale prose over the authority.

Both ledgers are **deleted**, not refreshed, replaced with a navigation pointer
plus an explicitly-labelled orientation block. A refreshed copy is a fresh
guarantee that it rots again. `i2pr-planning` gained a
"Never mirror plan state outside `plans/`" rule so the failure mode is recorded
where plan work is done.

Net: `i2pr-local-dev` 820 → 471 lines, `i2pr-architecture` 644 → 395 lines.

## Factual errors corrected

| Surface | Was | Now (authority) |
|---|---|---|
| `i2pr-local-dev` `plan_201` | `in-progress-branch-c-d-attempt-blocked-on-java-loopback-peer-profile-scoring` (fabricated — 0 occurrences in the closure record) | `retained-deferred-nonblocking-java-router-compatibility-debt-via-plan248` |
| `i2pr-local-dev` `plan_194` | `retained-partial`, stopped at a first-run topology blocker | `passed-m6-java-second-family-mixed-router-closure-with-sam-ls2-gap` (Plans 196/197) |
| `i2pr-local-dev` `plan_204` | `blocked-on-independent-java-m6-branch-and-m10-plan213-plan214-qualification` | `retained-convergence-record-superseded-by-plan248-policy-reconciliation` |
| `i2pr-local-dev` `plan_208` | defined twice with conflicting values | no ambiguity; Plan 208 is `closed` |
| `specs/support.toml:458` `next_executable_plan` | `plan283 (...)` — while line 429 declared `plan_283_status = "passed-..."` | `plan342 (...)`, matching the registry's ready plan |
| `specs/support.toml:455` `m12_type11_red25519` | `type5-deferred; no acceptable reviewed Rust provider; plan280 stopped` | in-repo provider passed (Plan 330/331); type-5 storage/auth implemented (Plans 332/333); i2pr-only, nothing advertised |
| `docs/protocol-support.md` | "authority through Plan 268 … Plan 269 is the sole ready plan" | through Plan 283; Plan 342 ready (its own header named `support.toml` as authority) |
| `AGENTS.md` routine floor | `bash scripts/check-global-plan-number-uniqueness.py` | `python3 …` — see below |
| `AGENTS.md` ADR range | `0000–0025` | `0000–0031` |
| `i2pr-architecture` per-crate index | 17 of 19 crates; zero mention of `i2pr-su3`, `i2pr-addressbook`, `i2pr-i2pcontrol` | all 19 |
| `i2pr-architecture` ADR index | 0000–0025 | 0000–0031, with the 0030 collision recorded |
| `i2pr-architecture` script table | listed `check-plan095-workflow.sh` (deleted in `c04da77a`) | row removed; 14 missing checkers added; known gaps added |

**`bash` on a `.py` script.** `bash scripts/check-global-plan-number-uniqueness.py`
does not run the check. It garbles the Python, shells out to ImageMagick
`import`, and exits 2 — so an agent reading the result would believe the plan
numbering was checked (or that it failed) when it never ran. CI already used
`python3`. Fixed in `AGENTS.md` and called out in `i2pr-local-dev`.

**Crates absent from every index.** `i2pr-addressbook` and `i2pr-i2pcontrol`
appeared **zero times** in `AGENTS.md`, `README.md`, and the architecture
skill, and `i2pr-su3` was missing from `README.md`. The whole Proposal 170
lane was invisible to an agent starting from those files. Added to all three.

## Historical bundles: fixed forward, not deleted

All three carry non-reconstructible knowledge (terminal-state reasoning, the
typed blocker taxonomy, the lifecycle state machine), so pruning would have
traded ~59 KB for real loss. Kept and corrected:

- **Descriptions had no historical signal.** The in-body HISTORICAL banners are
  invisible to description-based skill selection, and two descriptions invited
  exactly the forbidden work `AGENTS.md` bans:
  `i2pr-multipass-recovery` said "create, adopt, resume, recreate, or destroy a
  Multipass guest"; `i2pr-rootless-sandbox` said "enter the sandbox";
  `i2pr-ntcp2-interop` said "add or modify a scenario". All three now open with
  `HISTORICAL, ARCHAEOLOGY-ONLY` and name the prohibition. All three
  `agents/openai.yaml` said "run or extend" and are now rewritten.
- **`references/operations.md` was the worst artifact in the repo for this
  failure mode:** ~130 lines with no historical banner, including
  `sudo bash scripts/interop/ubuntu/setup-host.sh` and
  `sudo -E bash scripts/interop/run-matrix.sh --profile full`. Banner added.
  `references/script-catalog.md` likewise.
- **Eight broken links** in `i2pr-ntcp2-interop/SKILL.md` (`../../` where
  `../../../` is needed) and one broken anchor. All 8 skills now resolve 100%
  of relative markdown links.
- **False "pruned" annotations.** `i2pr-rootless-sandbox` and
  `i2pr-multipass-recovery` claimed four files were "pruned on 2026-08-13".
  Git refutes this twice over: the date is 2026-08-11 (`c04da77a`), and
  `rootless_supervisor.py` / `rootless_inner_runner.py` were **re-added** in
  `8aba042f` and exist today. Only `test_rootless_topology.py` and
  `test_multipass.py` are genuinely gone.
- **A false "known pre-existing issue".** The rootless skill said its checker
  "fails with `rootless-owned file missing` … treat that as a known pre-existing
  issue". Run: it **passes** (exit 0). That claim would have made a future
  agent discount a real regression.
- **Silent no-op commands.** `unittest discover -p 'test_rootless_topology.py'`
  and `-p 'test_multipass.py'` name deleted files: they collect zero tests and
  still exit 0. The multipass skill's "Test surface" section described a suite
  that does not exist, implying verification that never happened. Both now say
  so explicitly.
- **An unrecoverable "authoritative" path.** `plan099-summary.json` under
  `target/` is gitignored, never tracked, and absent on a clean checkout, yet
  two places described it as "preserved at" / authoritative terminal state. Now
  marked absent with a pointer to the closure record.
- **Misleading tense.** "Authoritative terminal state", "The active development
  interop surface", and "sufficient for routine development" all implied a live
  lane and that this seam is what routine work uses (it is not — that is
  `i2pr-local-dev`). Reworded.

## Post-rebase reconciliation (4 upstream commits landed mid-pass)

`git push` was rejected; `origin/main` had advanced past the audit's base by
four commits (`e86f3a05`, `50e98f3c`, `b4e0d6f1`, `d363c94e`). They touched
`plans/**` only, so the rebase was conflict-free and the disjointness is itself
evidence: this pass owned the skills/docs/specs layer, upstream owned planning.

Upstream invalidated three orientation values written minutes earlier, all of
them the kind this pass had just argued should not be duplicated:

- A **16th roadmap** appeared (`managed-native-app-runtime`, Plan 345 ready,
  contract foundation only — no launcher, sandbox, or clearnet brokering).
- **`plan_322` became `passed-canonical-routerinfo-sources-with-the-transit-
  participation-posture-unchanged`** (Plans 339/340 closed its eight selectors;
  the three transit ones stay at zero because enabling transit is a production
  posture change, not a missing snapshot). It had been written as blocked.
- **Plan 346/347/348** reframed the ELS2 transcript. The Plan 335 *measurement*
  stands (i2pd and Java verify each other, both reject i2pr's former strict-only
  form, blinded keys identical) but its *interpretation* is superseded: a
  specification/deployment split, not a reference defect.

All three were corrected. This is a small live demonstration of the failure mode
this pass removed, and the reason the orientation block is labelled "read the
registry for the current value" rather than presented as current: it rotted
within the hour, exactly as the two deleted ledgers had. The lesson recorded in
`i2pr-planning` ("never mirror plan state outside `plans/`") is why this was three
line edits and not a re-audit.

## Unpatched defects recorded rather than fixed

Closing these means changing a script or the DAG allowlist, which needs a
plan-of-record. Documented in `AGENTS.md` → "Known checker gaps" and the
architecture skill rather than silently worked around.

- **`scripts/check-m12-floodfill-boundaries.sh` currently exits 1.** It still
  enforces the Plan 281 "type 5 is deferred" floor, but Plans 332/333/334
  legitimately put `DatabaseStoreData::EncryptedLeaseSet` into NetDB storage. It
  was last touched 2026-10-03; Plan 333 landed 2026-10-04. It is in neither the
  floor nor CI, so the failure has been silent. **Deliberately not added to the
  floor** — that would have turned a silent breakage into a red floor. Not
  weakened either, per "fix code, never weaken scripts".
- **`check-dependency-direction.sh` has 18 `expected`-map keys for 20
  workspace members** (19 crates plus `tools/i2pr-interop`).
  `i2pr-tunnel` and `tools/i2pr-interop` are never inspected, so a new forbidden
  `i2pr-*` production edge in either passes CI silently.
- **`check-runtime-boundaries.sh` has no `i2pr-api` section**, and it greps
  `std::net` literally, so a grouped `use std::{…}` import evades it.
- **`tools/i2pr-interop`** is a workspace member with 0 references in the
  direction script and outside the runtime script's `crates/*/Cargo.toml` glob.
- **ADR numbers are not uniqueness-checked.** `docs/adr/` holds two `0030-*`
  records, both `Accepted`, which makes ADR 0029's "partially superseded by ADR
  0030" ambiguous. `check-global-plan-number-uniqueness.py` scans `plans/` only.
  Not renamed: ADRs are append-only and renumbering is a plan-of-record action.

## Floor additions

Two green, unpoliced boundary checkers were added to the `AGENTS.md` floor and
to the `tooling.md` floor/CI matrix (recomputed from disk rather than hand-edited):

- `check-m11-per-epoch-composition.sh` (M11 per-epoch composition)
- `check-service-anonymity-boundaries.sh` (ADR 0029/0030 anonymity boundaries)

`AGENTS.md` also gained the CI-enforced outproxy boundary as a hard rule: the
policy and route owner may only route through an I2P Streaming connection, with
`check-service-tunnel-boundaries.sh` rules 9–11 as the enforcer — including the
`std::net::IpAddr` carve-out that exists so the target grammar can refuse IP
literals. It is recorded that the policy and route owner exist but there is **no
reachable request path** (`open_via_outproxy` has zero callers) and no direct
clearnet fallback, so this is not a working outproxy.

## Judged accurate, left alone

- All 19 `docs/architecture/i2pr-*.md` deep dives, `overview.md`,
  `dependency-graph.md`, `specs/CONFORMANCE.md`, `plans/README.md`, and
  `plans/registry.md`.
- `i2pr-planning`'s mechanics and every path it cites.
- `tests/integration/ntcp2/references.lock.toml` pins Java 2.12.0 / i2pd 2.60.0
  against the current 2.13.0 / 2.61.0, but both skills already document this as
  frozen-vs-current, so the divergence is intentional and labelled.

## Not done, and why

- **No plan registered or closed out.** This pass changed documentation and
  skills only. Registering a plan is itself a planning act requiring the
  `i2pr-planning` lifecycle, and the user asked for a hygiene pass, not a new
  plan. The recorded defects above are the input to whoever opens one.
- **No checker or DAG change**, per the reasoning above.
- **No code change at all**, so no fixture, vector, or evidence bytes moved and
  the `check-fixture-manifest.sh` / `check-*-vectors.sh` corpora are untouched.

## Verification run

```text
bash    scripts/check-dependency-direction.sh                 OK
bash    scripts/check-runtime-boundaries.sh                    OK
bash    scripts/check-service-tunnel-boundaries.sh             OK
bash    scripts/check-m11-transit-boundaries.sh                OK
bash    scripts/check-m11-per-epoch-composition.sh             OK
bash    scripts/check-service-anonymity-boundaries.sh          OK
python3 scripts/check-global-plan-number-uniqueness.py         OK
python3 -m unittest discover -s tests/planning -p 'test_*.py'  OK
bash    scripts/check-rootless-interop-boundary.sh             OK (claims the skill wrongly called a failure)
bash    scripts/check-multipass-interop-boundary.sh            OK
python3 scripts/check-m12-floodfill-boundaries.sh              FAIL — pre-existing, see above
bash    scripts/check-ntcp2-interoperability.sh                OK
bash    scripts/check-constrained-host-lane-boundary.sh         OK
bash    scripts/check-sam-acceptance-evidence.sh               OK
bash    scripts/check-ssu2-acceptance-evidence.sh              OK
bash    scripts/check-i2cp-acceptance-evidence.sh              OK
bash    scripts/check-i2pcontrol-acceptance-evidence.sh        OK
bash    scripts/check-exploratory-tunnel-evidence.sh           OK
bash    scripts/check-netdb-tunnel-evidence.sh                 OK
bash    scripts/check-destination-tunnel-evidence.sh           OK
bash    scripts/check-m6-mixed-router-acceptance-evidence.sh   OK
bash    scripts/check-m11-transit-qualification-evidence.sh    OK
bash    scripts/check-fixture-manifest.sh                      SKIPPED — needs bash 4+; see below
bash    scripts/check-ntcp2-vectors.sh                         SKIPPED — needs bash 4+
bash    scripts/check-ssu2-vectors.sh                          SKIPPED — needs bash 4+
bash    scripts/check-i2cp-vectors.sh                          SKIPPED — needs bash 4+
bash    scripts/check-streaming-tunnel-evidence.sh             SKIPPED — needs bash 4+
bash    scripts/check-service-tunnel-acceptance-evidence.sh    SKIPPED — needs bash 4+; see below
cargo fmt --all --check                                        OK
cargo check --locked --workspace --all-targets                OK
```

**The six skipped checkers are a host limitation, not a finding.** This macOS
host has only bash 3.2.57 and no bash 4+.

- Five use `declare -A` / `mapfile` and exit 2 with
  `declare: -A: invalid option`.
- `check-service-tunnel-acceptance-evidence.sh` fails differently: line ~1667
  puts a `<<'PY'` heredoc inside a `$( )` command substitution, which bash 3.2
  mis-parses. The reported error surfaces ~287 lines later at line 1954 —
  a red herring that points at a perfectly valid `echo` line.

None of these declare a bash requirement, so a host failure looks exactly like
content drift. The repo documents this class of requirement for
`check-java-source-lock-gating.sh` only. Both mechanisms are now recorded in
`AGENTS.md` under "macOS/bash-3.2 trap". Verified pre-existing: all six fail
identically from the committed blobs at `HEAD`, and this pass modified no script
in `scripts/`.

Nothing in this pass touched a fixture, vector, or evidence byte — the change set
is documentation, skills, and `specs/support.toml` only — so the fixture and
vector corpora are provably unchanged and these checkers are not a gate on this
commit. `cargo test` was still run in full; see the commit message.

Plus a link sweep: 39 relative markdown links across all 6 skills resolve, 0
broken (9 were broken before this pass: 8 in `i2pr-ntcp2-interop`, plus 1
bad link depth in `i2pr-architecture`).
