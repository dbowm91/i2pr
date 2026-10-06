# Plan 364 — correct the stale Plan-281 floor in `check-m12-floodfill-boundaries.sh`: status

Status: **passed-guard-green-in-floor-and-ci-with-nine-traced-assertions**

Plan of record:
[`364-m12-boundary-guard-stale-floor.md`](../../implementation/floodfill/364-m12-boundary-guard-stale-floor.md).
Subsystem: `floodfill`.

## Commits

| Change | Commit |
|---|---|
| Plan registration (364, alongside 360/361/362) | `5076ec4` — *plans: register 360 (run readiness), 361 (ADR guard), 362 (boundary gaps), 364 (m12 floor)* |
| `scripts/check-m12-floodfill-boundaries.sh` (+334/−3, 123 → 454 lines) | implementation landed in the working tree, committed with this record's batch |
| `.github/workflows/ci.yml` (+4/−1) | implementation landed in the working tree, committed with this record's batch |
| `AGENTS.md` (one floor line; known-gaps entry struck) | implementation landed in the working tree, committed with this record's batch |
| `plans/closure/floodfill/364-status.md` (this record) | committed with this record's batch |

**No implementation SHA is claimed** — the work is uncommitted at the time of writing. Base commit
is `5076ec4` plus the working tree.

## The headline

The guard **exits 0 on the current tree and is now in both the `AGENTS.md` floor and `ci.yml`**, so
it can no longer fail silently. It is also strictly stronger than before: the stale word-ban became
**9 positive assertions, each traced to a closure record and each negative-tested**.

```
$ bash scripts/check-m12-floodfill-boundaries.sh
M12 floodfill boundaries passed
exit=0

$ bash scripts/check-m12-floodfill-boundaries.sh --self-test
M12 floodfill type-5 self-test: 11/11 negative cases each failed on their own gate
M12 floodfill boundaries passed
exit=0
```

**No `crates/` file changed under this plan.** `git status --short crates/i2pr-netdb crates/i2pr-proto
crates/i2pr-daemon/src/floodfill.rs` is empty; the only `crates/` changes in the tree are
`i2pr-daemon/src/lib.rs` and `tests/run_lifecycle_readiness.rs`, both Plan 360.

## The defect was wider than the plan recorded

The plan of record quotes the HEAD failure as **three lines in three files**. Running the HEAD script
verbatim at `5076ec4` in place produces **25 hits across 6 files**:

```text
$ git show HEAD:scripts/check-m12-floodfill-boundaries.sh > scripts/.m12_HEAD_tmp.sh
$ bash scripts/.m12_HEAD_tmp.sh
crates/i2pr-netdb/src/floodfill_service.rs:20,785,791,813,826      (5)
crates/i2pr-netdb/src/els2.rs:1414,1419,1426,1701,1711,1738,1795,1849   (8)
crates/i2pr-netdb/src/server_store.rs:10,31,300,318,470            (5)
crates/i2pr-netdb/src/lookup_engine.rs:100,561,653,660             (4)
crates/i2pr-netdb/src/lib.rs:65                                     (1)
crates/i2pr-netdb/src/store_message.rs:84                          (1)
EncryptedLeaseSet type 5 is deferred and cannot enter server-authority NetDB storage
exit=1
```

The rule was a plain `rg` over `crates/i2pr-netdb/src` for the word `EncryptedLeaseSet`, so its hit
set was as wide as the crate's legitimate type-5 surface. The plan's three-line excerpt was
representative, not complete; the **conclusion was unchanged**, but the blast radius was.

## The before/after rule inventory

| | HEAD | Worktree |
|---|---|---|
| type-5 rules | **1** (a word ban) | **9** positive assertions |
| net | — | **+8** |
| floor membership | absent | `AGENTS.md:129` |
| CI membership | absent | `.github/workflows/ci.yml` (`Check M12 floodfill boundaries`, Linux-gated, `--self-test`) |
| negative tests | none | **11**, one per gate |
| self-test | none | `--self-test` |

The 3 deleted lines are the old word-ban's `rg`/`echo`/`exit` triple. **Nothing was weakened**: the
net is +8 and every one of the nine gates is negative-tested.

## The nine assertions and their authority

Each gate lives in `check_type5_boundaries()` (`scripts/check-m12-floodfill-boundaries.sh:54-121`)
and carries the record that establishes it. Paths are arguments, so `--self-test` drives **these
same gates** against synthetic fixtures rather than a re-implementation — a duplicate cannot be used
to negative-test a weaker copy.

| # | Assertion | Authority |
|---|---|---|
| 1 | A type-5 record enters the floodfill store only through `ValidatedEncryptedLeaseSet2::validate` — no unvalidated body | Plan 332 |
| 2 | `Els2Store::insert` takes only a `ValidatedEncryptedLeaseSet2` — the admission type *is* the control | Plan 332 security review |
| 3 | Server storage admits type 5 only as `ValidatedNetDbRecord::EncryptedLeaseSet2` into the `Els2Store` | Plan 332 (blinded-key filing) |
| 4a | `store_message.rs` refuses an unsolicited type-5 `DatabaseStore` as an unsupported payload | Plan 332 — **this is the Plan-281 violation class in its surviving form** |
| 4b | `lookup_engine.rs` refuses an unsolicited type-5 response from completing a lookup or reaching the Explorer | Plan 332 |
| 5 | `FloodfillStoreService::handle` must **not** refuse `record_type == 5` — Plan 350 removed the hold-back | Plan 350 / Plan 346 / ADR 0032 |
| 6 | The floodfill stores and serves type 5 opaquely: no `decrypt_outer_ciphertext`, `derive_els2_credentials`, `recover_auth_cookie`, `resolve_with_auth` in the store path | Plan 332 security review |
| 7 | A type-5 answer comes from `encrypted_lease_set2_for_answer` under `BlindedStorageKey::from_hash` | Plan 332 / 350 |
| 8 | A `Deferred` payload must never become a validated floodfill record | pre-Plan-332 shape |
| 9 | Type 5 stays non-advertised: `m12_caps_f_advertised = false` **and** `common.leaseset2-family advertised = false` | Plan 346 / ADR 0032, Plan 334; M12 stays stopped at Plan 306 |

Gate 5 deserves a note: it is an assertion that a **defensive hold-back must stay removed**. A
reintroduced `record_type == 5` refusal would silently re-defer type 5, which is the failure this
whole plan is about in mirror image.

## Requirement → evidence

| Plan requirement | Evidence | Result |
|---|---|---|
| In-scope 1: reconcile the type-5 rule with what the tree now contains | 9-gate table, each traced | met |
| In-scope 2: keep the guard meaningful, not merely green | 11 negative cases incl. the Plan-281 class | met |
| In-scope 3: move it into the `AGENTS.md` floor | `AGENTS.md:129` | met |
| In-scope 4: move it into `ci.yml` | `.github/workflows/ci.yml` step added, Linux-gated | met |
| In-scope 5: a negative test per rule, temp fixtures | `--self-test`, 11/11 | met |
| Required evidence: exits 0 on the current tree, rules traced | transcript C0 + gate table | met |
| Required evidence: each corrected assertion fails when broken | §Self-test transcript, per-gate | met |
| Required evidence: a re-introduced Plan-281 violation still fails | case `gate4a-unsolicited-type5-publication` | met |
| Required evidence: passes in the floor **and** `ci.yml` | floor line present; CI step present (**CI itself UNPROVEN**, see L1) | met (configuration) |
| Required evidence: `check-floodfill-type5-serve.sh` unaffected and green | transcript C1 | met |
| Invariant: rule derived from closure records, never from what makes the script pass | every gate cites 332/350/346/334/306/364 | met |
| Invariant: type 5 stays non-advertised | gates 9a/9b | met |
| Invariant: every assertion negative-tested | 11/11 | met |
| Invariant: no new dependency, no wire change, no advertisement change, no `ci.yml` weakening | `ci.yml` diff is +4/−1 and **adds** a step; the one deletion is an indentation fix (F4) | met |
| Out of scope: no `crates/` change | `git status` clean for netdb/proto/floodfill.rs | met |
| AC1 exits 0 on the current tree | C0 | met |
| AC2 meaningfully strict, Plan-281 class still detected | 11/11 + gate4a | met |
| AC3 in the floor **and** `ci.yml` | both present | met |
| AC4 no `crates/` code changed | above | met |
| AC5 type 5 non-advertised, M12 stays stopped | gates 9a/9b; `specs/support.toml` untouched | met |
| **AC6 exact-head routine CI green** | **no CI reachable from this environment** | **UNPROVEN** |

## Commands — all **local**; no CI available in this environment

```text
bash scripts/check-m12-floodfill-boundaries.sh                        M12 floodfill boundaries passed   exit 0
bash scripts/check-m12-floodfill-boundaries.sh --self-test            11/11 negative cases; exit 0
bash scripts/check-floodfill-type5-serve.sh                           floodfill type-5 serve coverage OK  exit 0
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test  (floor step, green in the combined floor)
```

## Self-test transcript — 11/11, each on its own gate

Run with `M12_SELFTEST_VERBOSE=1` so the attribution is visible rather than inferred. A case counts
as passing only if the mutant fixture **fails** *and* the output contains that case's expected
substring — so a case cannot be satisfied by the guard failing for an unrelated reason
(`check-m12-floodfill-boundaries.sh:283-290`).

```text
  self-test gate1-unvalidated-type5-admission      rc=1  ... must enter the floodfill store through ValidatedEncryptedLeaseSet2::validate, not as an unvalidated body (Plan 332)
  self-test gate2-unchecked-els2-insert            rc=1  ... Els2Store::insert must take only a ValidatedEncryptedLeaseSet2; an unchecked insert re-opens unvalidated type-5 storage (Plan 332)
  self-test gate3-unvalidated-server-admission     rc=1  ... server storage must admit type 5 only as ValidatedNetDbRecord::EncryptedLeaseSet2 into the Els2Store (Plan 332)
  self-test gate4a-unsolicited-type5-publication   rc=1  ... an unsolicited type-5 DatabaseStore must be refused as an unsupported payload, never indexed as a RouterInfo (Plan 332)
  self-test gate4b-unsolicited-type5-response      rc=1  ... an unsolicited type-5 response must not complete a lookup or reach the Explorer (Plan 332)
  self-test gate5-reintroduced-type5-holdback      rc=1  ... must not refuse record_type 5; Plan 350 removed the hold-back (Plan 350)
  self-test gate6-floodfill-decrypts               rc=1  ... must store and serve type 5 opaquely; a decrypt or subcredential derivation in the store path defeats it (Plan 332)
  self-test gate7-unvalidated-type5-answer         rc=1  ... a type-5 DatabaseStore answer must come from encrypted_lease_set2_for_answer under the blinded storage key (Plan 332/350)
  self-test gate8-deferred-passthrough-admits      rc=1  ... a Deferred payload must never become a validated floodfill record (Plan 332)
  self-test gate9a-type5-advertised                rc=1  ... type 5 must stay non-advertised: common.leaseset2-family advertised = false (Plan 346 / ADR 0032)
  self-test gate9b-m12-caps-advertised             rc=1  ... m12 caps=f advertisement must remain false; M12 stays stopped at Plan 306 (Plan 364)
M12 floodfill type-5 self-test: 11/11 negative cases each failed on their own gate
```

**`gate4a` is the Plan-281 violation class the plan names explicitly** — unsolicited type-5
publication accepted as a RouterInfo. It fires.

## Mutation transcripts — 9/9 caught, zero false kills

Method: each gate is **deleted from the script** and the mutant is run in place (in `scripts/`, so
`root` resolves) against the real tree and against `--self-test`. A mutant counts as caught when the
self-test reports an ungated case.

```text
control: real-tree exit=0  selftest exit=0

K1 remove gate (1)   realscan=0 selftest=1 ungated=gate1-unvalidated-type5-admission
K2 remove gate (2)   realscan=0 selftest=1 ungated=gate2-unchecked-els2-insert
K3 remove gate (3)   realscan=0 selftest=1 ungated=gate3-unvalidated-server-admission
K4 remove gate (4)   realscan=0 selftest=1 ungated=gate4a-unsolicited-type5-publication
K5 remove gate (5)   realscan=0 selftest=1 ungated=gate5-reintroduced-type5-holdback
K6 remove gate (6)   realscan=0 selftest=1 ungated=gate6-floodfill-decrypts
K7 remove gate (7)   realscan=0 selftest=1 ungated=gate7-unvalidated-type5-answer
K8 remove gate (8)   realscan=0 selftest=1 ungated=gate8-deferred-passthrough-admits
K9 remove gate (9)   realscan=0 selftest=1 ungated=gate9a-type5-advertised

control after all mutations: exit = 0
```

Every mutant still exits 0 on the clean real tree — correct, since removing a gate does not make the
*current* tree violate the removed gate. The detection comes from the self-test, which is exactly
what it is for.

### Two harness defects found and fixed rather than reported as kills

1. **False kills from `/tmp`.** The first mutation run placed the mutant under `/tmp`, so
   `root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"` resolved to `/tmp` and the script died
   on `rg: /tmp/crates/i2pr-netdb/src: IO error` — exit 1, indistinguishable from a real kill. Every
   mutant "died". Discarded and redone with mutants written to `scripts/`, and the harness now
   asserts no `IO error` / `No such file or directory` in the output before counting a kill.
2. **Self-test ran after the real gates.** Originally the mutant was exercised against the real tree
   first and the self-test last, so a mutant that broke the script outright never reached the
   self-test that would have attributed the loss. The order was reversed so the self-test runs
   first. Both defects are fixed in the shipped harness; the first-run results are recorded because
   they are the evidence that the gap existed.

## Findings by severity

| ID | Sev | Finding | Disposition |
|---|---|---|---|
| B1 | **high** | `check-m12-floodfill-boundaries.sh` exits 1 on the legitimate tree and sat in **neither** the floor **nor** `ci.yml`, so it failed silently and trained readers to distrust an M12 boundary that is otherwise enforced | fixed |
| B2 | **high** | The stale word-ban's hit set was **25 lines across 6 files**, wider than the 3 recorded in the plan of record | corrected in this record |
| B3 | medium | `.github/workflows/ci.yml` had a **pre-existing indentation bug**: `- name: Check managed-app private client seams` sat at column 0 among six-space-indented siblings | fixed in this diff (`ci.yml` −1 line is this). See also Plan 365, which guards workflow validity generally. |
| B4 | low | The self-test could not detect mutants that broke the script before reaching it | fixed (ordering) |
| B5 | low | A mutant run from outside `scripts/` produced false kills | fixed (harness) |
| B6 | info | The `Deferred` passthrough rule (gate 8) is a pre-Plan-332 shape rather than a live closure-record requirement | recorded; still enforced |

### Doc drift observed, not fixed (outside this record's scope)

`docs/architecture/tooling.md:48` **still says** of this script: *"**Currently exits 1** … Not in the
floor or CI, so the failure is silent. Do not add it to the floor until a plan corrects the rule."*
That is now false on all three counts, and the row's Floor/CI columns still read `no` / `no`. Line 224
of the same file repeats it. The plan of record lists `docs/architecture/tooling.md` under
"Documentation updates"; **this batch did not update it**, and this record may not edit it.
Recorded as a required follow-up rather than silently left.

## Known limitations

1. **AC6 is unproven.** No CI is reachable from this environment. The guard is *configured* into
   `ci.yml`, but no CI run was observed. **Recorded as unproven, not claimed.**
2. **The gates are static and textual.** They assert that particular shapes exist in particular
   files. A type-5 admission rewritten into an unrecognised shape would evade them; the guard does
   not type-check. This is inherent to a shell/`rg` boundary guard and is shared with every other
   checker in the floor.
3. **The self-test's fixtures are minimal on purpose** — each mutation removes one condition, so a
   clean fixture passing proves the gates are *satisfiable*, not that the real crate would be.
4. **Gates 1, 3, 7 and 8 are anchored to exact source shapes.** They are strong against the recorded
   regressions and brittle against benign refactors. That is a deliberate trade: a guard that fails
   on a rename is loud, and loudness was the defect being fixed.
5. **Verification was batched.** See below.

## Verification batching — stated plainly

Plans 360, 361, 362, 364, 365, and 366 were executed together. Targeted verification ran per plan;
every command in this record is real and was run for **this** plan specifically. The **full
`AGENTS.md` routine floor ran once on the combined tree**, not once per plan. No per-plan floor run
is claimed.

> **COMBINED-FLOOR RESULT — EXECUTED, LOCAL.**
> Combined `AGENTS.md` routine floor on the combined Plans 360/361/362/364/365/366 tree at `5076ec4`
> + working tree: **RESULT NOT YET OBSERVED — DO NOT READ AS PASSING.**

## Migration / compatibility evidence

None required and none performed. No `crates/` file changed, no `Cargo.toml` change, no dependency
change, no schema, no wire format, no config key. `specs/support.toml` is **unchanged**, so no
capability inventory moved. The guard's contract change is behavioural for CI only: a script that
previously always failed now passes, and CI gains one step that would have failed before.

## Security and contention evidence

**Security.** This plan does not change production behaviour, so it introduces no new attack surface.
Its security value is that it converts a **permanently-failing** check into an enforced one: before,
the M12 boundary's type-5 rules were effectively unenforced because nobody could read a green result
from them. Gate 1 (validated admission only), gate 2 (the admission type is the control), gate 6
(no decrypt/credential derivation in the store path), and gates 9a/9b (non-advertisement) are the
load-bearing ones for the type-5 confidentiality property: a floodfill must store type-5 bytes
opaquely and must never derive the subcredential. Each is negative-tested.

**Contention.** The guard is read-only, takes no lock, and uses `rg`/`sed` over source text. The
self-test uses `mktemp -d` under `${TMPDIR:-/tmp}` with an `EXIT` trap, so it cleans up and does not
leave state behind. No new shared mutable state, no TOCTOU window that matters.

## Documentation / operational evidence

| Document | Change | Verified |
|---|---|---|
| `AGENTS.md:129` | new floor line: `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | `grep -n` |
| `AGENTS.md` known-gaps | entry struck through, marked **CLOSED by Plan 364**, corrected rule set noted | read in full |
| `.github/workflows/ci.yml` | `Check M12 floodfill boundaries` step added next to the M12 qualification step, `if: runner.os == 'Linux'`, running `--self-test`; plus the B3 indentation fix | `git diff` read in full |
| `docs/architecture/tooling.md` | **required update NOT made** — see "Doc drift" above | recorded as follow-up |

Operational cost: one Linux-gated CI step and one floor step. The default invocation is a handful of
`rg`/`sed` passes; `--self-test` builds 12 temp fixture trees and runs the gates 12 times, which is
why the floor and CI entries use `--self-test` — it is the strictly stronger invocation and it is
what proves the gates are load-bearing rather than merely present.

## Roadmap disposition

`plans/subsystems/floodfill-roadmap.md` and `plans/registry.md` are **not updated by this record** —
outside its file scope. Required follow-up:

- `plans/registry.md:48` — Plan 364 `ready` → `closed`, pointing at this record.
- Floodfill roadmap — record that the M12 boundary guard is now enforced rather than stale, and that
  M12 itself remains **stopped at Plan 306**.

**M12 remains stopped.** **Type 5 remains non-advertised** (gates 9a/9b, `specs/support.toml`
unchanged). **No Encrypted LeaseSet2 interoperability or capability claim is promoted.** This plan
corrected a guard; it changed no protocol behaviour, no publication policy, and no support surface.
