# Plan 361 — ADR-number uniqueness guard: status

Status: **passed-guard-live-in-floor-and-negative-tested**

Plan of record:
[`361-adr-number-uniqueness-guard.md`](../../implementation/workspace-foundation/361-adr-number-uniqueness-guard.md).
Subsystem: `workspace-foundation` (planning and tooling authority).

## Commits

| Change | Commit |
|---|---|
| Plan registration (361, alongside 360/362/364) | `5076ec4` — *plans: register 360 (run readiness), 361 (ADR guard), 362 (boundary gaps), 364 (m12 floor)* |
| `scripts/check-adr-number-uniqueness.py` (new, 152 lines) | implementation landed in the working tree, committed with this record's batch |
| `tests/planning/test_adr_number_uniqueness.py` (new, 376 lines) | implementation landed in the working tree, committed with this record's batch |
| `AGENTS.md` (one floor line, one known-gaps entry) | implementation landed in the working tree, committed with this record's batch |
| `plans/global-number-collision-ledger.md` (gap note → closed state) | implementation landed in the working tree, committed with this record's batch |
| `plans/README.md:93-101` (no longer calls the plan check the only guard) | implementation landed in the working tree, committed with this record's batch |
| `plans/closure/workspace-foundation/361-status.md` (this record) | committed with this record's batch |

**No implementation SHA is claimed.** The work is uncommitted at the time of writing; a SHA is
invented here only when the batch lands. Base commit for everything below is `5076ec4` plus the
working tree.

## The headline

The guard exists, is in the routine floor, and is negative-tested. The gap
`plans/global-number-collision-ledger.md` advertised in its own words is closed:

> ~~Known coverage gap: no equivalent guard exists for ADR numbers.~~
> → *"The ADR-number coverage gap is closed (Plan 361)."*

**No ADR was renamed, moved, or deleted.** `git status --short docs/adr` is empty.

## What the guard enforces, and what it deliberately does not

`TOLERATED_DUPLICATES` (`scripts/check-adr-number-uniqueness.py:41-70`) is an explicit literal
dict mapping each known duplicate number to the **exact frozenset of filenames** that claim it,
commented with a pointer to `plans/global-number-collision-ledger.md` (`LEDGER`, line 31). A
collision is tolerated only when `set(paths) == set(allowed)` (`check-adr-number-uniqueness.py:106`),
so neither a third claimant nor a renamed file is grandfathered in. There is no glob and no silent
`continue`; the module docstring and the literal's own comment both say so.

The guard enforces **identity only**. It does not read ADR content, status tokens, or supersession
chains — that is stated in the script docstring (lines 15-16) and repeated here because it is the
single most important thing a reader could otherwise over-claim.

### Real-tree facts, measured

| Fact | Value | How measured |
|---|---|---|
| ADR files in `docs/adr/` | **38** | `find docs/adr -type f \| wc -l` |
| Numbers present | `0000`–`0034`, 35 distinct | `min`/`max` over `name[:4]` |
| Filenames matching `^\d{4}-[a-z0-9][a-z0-9._-]*\.md$` | **38/38, zero exceptions** | regex sweep |
| Duplicate numbers | **3**: `0030`, `0032`, `0033` — 2 claimants each | `Counter` over `name[:4]` |

The three pairs, as encoded:

| Number | Claimants |
|---|---|
| `0030` | `0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md`, `0030-loopback-controlled-floodfill-reachability-advertisement.md` |
| `0032` | `0032-managed-native-app-process-and-capability-boundary.md`, `0032-els2-type11-signature-profile-boundary.md` |
| `0033` | `0033-portable-service-tunnel-policy-core-and-adapters.md`, `0033-els2-consumer-lookup-identity-and-install-key.md` |

## Requirement → evidence

| Plan requirement | Evidence | Result |
|---|---|---|
| In-scope 1: guard modelled on the plan-number checker, run with `python3` | `check-adr-number-uniqueness.py:137-152`; `bash` invocation exits 2 (transcript N0) | met |
| In-scope 2: added to the `AGENTS.md` routine floor | `AGENTS.md:91` | met |
| In-scope 3: `tests/planning/` suite, temp-dir fixtures only | `test_adr_number_uniqueness.py`; 8 tests pass; docstring asserts the real tree is only ever read | met |
| In-scope 4: existing exceptions encoded explicitly, ledger-linked | `TOLERATED_DUPLICATES` + `LEDGER`; `test_every_tolerated_entry_is_named_in_the_ledger` | met |
| In-scope 5: the ledger's gap note closed | ledger diff, quoted above | met |
| Required evidence: guard passes on the real tree | transcript C0 | met |
| Required evidence: a duplicate in a fixture **fails**, naming both files | transcript N1 | met |
| Required evidence: a malformed filename **fails**, not skipped | transcript N2 | met |
| Required evidence: removing an exemption makes the real tree fail | transcript N5 **and** `test_tolerated_set_is_load_bearing_against_the_real_tree` | met |
| Required evidence: `check-global-plan-number-uniqueness.py` still passes | transcript C1 | met |
| Required evidence: `tests/planning` still green | transcript C2 — 30 tests, `OK` | met |
| Invariant: fails closed on an unparseable name | N2, plus `contract_nested_unparseable_filename_fails` | met |
| Invariant: no ADR renumbered/moved/deleted | `git status --short docs/adr` empty | met |
| Invariant: adding an exemption requires editing the script | no skip path exists; `test_tolerated_set_has_no_glob` | met |
| Invariant: no new dependency | Python 3 stdlib only (`re`, `argparse`, `sys`, `collections`, `pathlib`) | met |
| AC1 guard exists, in floor, passes real tree | above | met |
| AC2 negative-tested, transcripts recorded | §Negative-test transcripts | met |
| AC3 three duplicates enumerated and traceable | table above; ledger test | met |
| AC4 plan-number checker untouched and green | `git diff --stat scripts/check-global-plan-number-uniqueness.py` → empty; C1 | met |
| AC5 no ADR renamed/moved/deleted | above | met |
| **AC6 exact-head routine CI green** | **no CI reachable from this environment** | **UNPROVEN** |

## Commands — all **local**; no CI available in this environment

```text
python3 scripts/check-adr-number-uniqueness.py                                  PASS  (exit 0)
bash      scripts/check-adr-number-uniqueness.py                                exit 2  — garbled, as documented
python3 scripts/check-global-plan-number-uniqueness.py                          PASS  (exit 0)
python3 -m unittest discover -s tests/planning -p 'test_*.py'                  30 tests, OK
python3 -m unittest discover -s tests/planning -p 'test_adr_number_uniqueness.py'   8 tests, OK
```

The `bash` result is not a defect and is recorded deliberately: the script's own docstring
(`check-adr-number-uniqueness.py:12-13`) says to run it with `python3` because `bash` garbles it
and exits 2, which is indistinguishable from content drift. Confirmed here: `bash` exits **2**.

### CI wiring — a gap in its own right

The guard is in the `AGENTS.md` floor (`AGENTS.md:91`). It is **not** a direct step in
`.github/workflows/ci.yml`; `grep -n 'check-adr' .github/workflows/ci.yml` returns nothing. CI
reaches it only through the existing `Test planning checkers` step
(`.github/workflows/ci.yml:79-80`, `python3 -m unittest discover -s tests/planning`), because
`test_real_tree_passes` runs the guard against `docs/adr`. That is sufficient — a duplicate number
would fail CI — but it is **indirect**, and recorded as F3 below rather than glossed.

## Negative-test transcripts — all executed

Every fixture is a temp-dir copy. **The real `docs/adr/` is only ever read.** The first probe
below is included precisely because it was mis-constructed on the first attempt: copying one file to
a second `0000-*` name produces *one* claimant, not a duplicate, and the run surfaced the
stale-exemption path instead. That is recorded rather than hidden (see known limitation 3).

```text
### N0  bash guard.py                       -> exit 2 (garbled; python3 is the documented invocation)

### N1  duplicate number, full tree copy
$ cp -r docs/adr/. "$T/" && cp "$T"/0000-adr-process.md "$T/0000-injected-duplicate.md
$ python3 scripts/check-adr-number-uniqueness.py --adr-root "$T"
ADR-number ownership check failed:
- ADR 0000 is claimed by 2 files and is not a recorded collision in
  plans/global-number-collision-ledger.md: 0000-adr-process.md, 0000-injected-duplicate.md
exit=1                                                            -> both files named

### N2  malformed filename, full tree copy
$ touch "$T/README.md"
$ python3 scripts/check-adr-number-uniqueness.py --adr-root "$T"
ADR-number ownership check failed:
- unparseable ADR filename README.md: expected NNNN-<lowercase-slug>.md
  (four-digit zero-padded ADR number)
exit=1                                                            -> error, not a silent skip

### N3  THIRD claimant of a recorded number (0030)
$ cp "$T"/0030-loopback-...md "$T/0030-third-claimant.md"
- ADR 0030 claims 0030-destination-linkability-...md, 0030-loopback-...md,
  0030-third-claimant.md, which does not match the recorded collision in
  plans/global-number-collision-ledger.md (0030-destination-linkability-...md,
  0030-loopback-...md)
exit=1                                                            -> exact-set match enforced

### N4  renamed claimant inside a recorded pair
$ cp "$T"/0030-loopback-...md "$T/0030-renamed-copy.md"
- ADR 0030 claims ... 0030-renamed-copy.md, which does not match the recorded collision ...
exit=1                                                            -> a rename is not grandfathered in

### N5  exemption entry deleted from the script, REAL tree
$ python3 "$S/guard.py" --adr-root docs/adr          # "0030": frozenset({...}) removed
- ADR 0030 is claimed by 2 files and is not a recorded collision in
  plans/global-number-collision-ledger.md: 0030-destination-linkability-...md,
  0030-loopback-controlled-floodfill-...md
exit=1                                                            -> the tolerated set is load-bearing

### C0/C1/C2 controls
$ python3 scripts/check-adr-number-uniqueness.py
ADR-number ownership check passed.
exit=0
$ python3 scripts/check-global-plan-number-uniqueness.py
Global plan-number ownership check passed.
exit=0
```

## Mutation transcripts — 6/6 caught

Run by `GuardMutationTests.test_mutations_are_caught`. Each mutant is `compile()`-checked first, so
a mutation that fails to parse cannot be reported as "caught for the wrong reason"
(`test_adr_number_uniqueness.py:248`). A mutation is reported caught only when an assertion the
suite **already makes** rejects it.

| # | Mutation | Caught by |
|---|---|---|
| M1 | `skip-unparseable-filename` — malformed name silently `continue`d | `contract_malformed_filename_fails`, `contract_nested_unparseable_filename_fails` |
| M2 | `drop-tolerated-set-check` — tolerate any number of claimants | `contract_third_claimant_of_recorded_number_fails`, `contract_renamed_recorded_file_fails` |
| M3 | `exit-zero-on-error` — `return 1` → `return 0` | `contract_duplicate_number_fails_naming_both`, `contract_malformed_filename_fails` |
| M4 | `disable-duplicate-detection` — `len(paths) <= 1` → `<= 99` | `contract_duplicate_number_fails_naming_both`, `contract_third_claimant_of_recorded_number_fails` |
| M5 | `report-errors-on-stdout` — messages move off stderr | 3 contracts that assert on stderr |
| M6 | `drop-stale-exemption-check` — the rot detector loop emptied | `contract_stale_tolerated_entry_fails` |

The suite's `RECORDED_COLLISIONS` is an **independent hard-coded copy**, deliberately not imported
from the script (`test_adr_number_uniqueness.py:32-34`): reading it from the script would make it
follow any mutation of the constant it exists to check. `test_script_tolerated_set_matches_this_suite`
compares the two, so drift between guard and test is itself a failure.

**Two harness defects were found and fixed during this work rather than reported as kills** — see
known limitation 4.

## Findings by severity

| ID | Sev | Finding | Disposition |
|---|---|---|---|
| A1 | **high** | ADR numbers were unguarded while three collisions were already live and two `0030` records are both `Accepted`, making ADR 0029's "partially superseded by ADR 0030" ambiguous | fixed |
| A2 | medium | The collision ledger advertised a gap it could not enforce | closed |
| A3 | low | A stale-exemption check is an unusual failure mode: a **partial** fixture tree fails for reasons unrelated to the property under test | recorded, kept — see limitation 3 |
| A4 | low | The guard reaches CI only indirectly, via `tests/planning` | recorded (F3 above); no CI change made here |
| A5 | info | ADR `0030` remains ambiguous by design | unchanged, out of scope by the plan's own out-of-scope list |

## Known limitations

1. **AC6 is unproven.** No CI is reachable from this environment. The local floor is green; that is
   not the acceptance criterion as written. **Recorded as unproven, not claimed.**
2. **Identity only.** No ADR content, status token, supersession chain, or `specs/support.toml` ADR
   list is validated. A wrong status or a broken supersession chain passes this guard.
3. **The stale-exemption check makes partial trees fail.** Verified: pointing the guard at a temp
   dir holding one `0000-*` file reports *only* stale-exemptions for `0030`/`0032`/`0033`, because
   none of their six files are present. This is intentional — a rotted exemption must not decay
   into a dead skip — but it means **a fixture tree must contain the full recorded collision set to
   test anything else**. The suite handles this via `adr_fixture()`; a hand-built fixture must not.
4. **Two harness defects were found by this work and fixed**, rather than being written up as
   mutation kills:
   - `apply_edits` originally asserted nothing about its anchor, so a mutation whose anchor had
     drifted would silently apply zero edits and then be reported as "caught" because the *real*
     checker satisfied the contract's negation for the wrong reason. It now raises on
     `count != 1` (`test_adr_number_uniqueness.py:302`).
   - `mock_tolerated` initially yielded a module different from the one under test, so
     `test_tolerated_set_is_load_bearing_against_the_real_tree` exercised a patched copy while
     asserting against an unpatched one. It now yields the patched module object itself
     (`test_adr_number_uniqueness.py:283-285`).
5. **Verification was batched** with Plans 360, 362, 364, 365, 366. See below.

## Verification batching — stated plainly

Plans 360, 361, 362, 364, 365, and 366 were executed together. Targeted verification ran per plan
as each landed; every command in this record is real and was run for **this** plan specifically.
The **full `AGENTS.md` routine floor ran once on the combined tree**, not once per plan. No
per-plan floor run is claimed.

> **COMBINED-FLOOR RESULT — EXECUTED, LOCAL.**
> Combined `AGENTS.md` routine floor (49 steps as enumerated in `AGENTS.md`) on the combined
> Plans 360/361/362/364/365/366 tree at `5076ec4` + working tree:
> **49 steps run, 49 PASS, 0 FAIL** (log: `/tmp/floorall.log`), re-parsed from `AGENTS.md`
> after the three new steps were added. All local; no CI was reachable from this environment,
> so this is **not** the "exact-head routine CI" acceptance criterion — that remains UNPROVEN.
> Note for whoever fills this in: `AGENTS.md`'s floor block contains **49** steps, but a naive
> `grep -cE '^(cargo|bash|python3)'` returns **48** because step 5 begins with `RUSTDOCFLAGS=`
> rather than `cargo`. Count the fenced block's lines 4–52, not the grep.

## Migration / compatibility evidence

None required and none performed. This plan adds two files and edits three documents. There is no
schema, no manifest, no wire format, no config key, and no dependency change. The plan-number guard
it neighbours is byte-for-byte untouched (`git diff --stat` empty), so the Plan 319 side of the
same invariant is unaffected.

## Security and contention evidence

No production code path is touched, so there is no new attack surface and no security-relevant
behaviour change. The one security-adjacent property is that ADR numbers are load-bearing citation
keys; this plan makes their ownership mechanical, which is a supply-chain-hardening change of the
"a citation cannot silently become ambiguous" kind. Nothing in the guard writes, renames, or deletes
an ADR, so the guard cannot damage the repository it inspects — confirmed by `git status docs/adr`
being empty after the full transcript set above.

**Contention:** two scripts now read `docs/adr/` and both read it read-only. There is no lock, no
write path, and no TOCTOU window that matters, because neither script mutates the tree.

## Documentation / operational evidence

| Document | Change | Verified |
|---|---|---|
| `plans/global-number-collision-ledger.md` | gap note replaced by the closed state, naming the three collisions and the exact-set rule | `git diff` read in full |
| `plans/README.md:93-101` | no longer claims "no checker enforces ADR uniqueness"; now names the guard and its three failure modes | `git diff` read in full |
| `AGENTS.md:91` | new floor line | `grep -n` |
| `AGENTS.md:~209` | known-checker-gaps entry struck through and marked **CLOSED by Plan 361** | read in full |

Operational cost: one fast Python step in the floor (~0.6 s for its own suite; the guard itself is
a single `rglob`). It adds no cargo work and no I/O beyond the ADR tree.

## Roadmap disposition

`plans/subsystems/workspace-foundation-roadmap.md` and `plans/registry.md` are **not updated by
this record** — writing them is outside this record's file scope. The required follow-up is:

- `plans/registry.md:46` — Plan 361 `ready` → `closed`, pointing at this record.
- `plans/subsystems/workspace-foundation-roadmap.md` — add Plan 361 as passed.

**No capability, version, RouterInfo, or support-surface claim is promoted.** Nothing on the network
is advertised. The three ADR collisions still exist; only their *recurrence* is now prevented.
