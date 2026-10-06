# Plan 362 — close the runtime/console boundary-checker coverage holes: status

Status: **partially-delivered-runtime-half-complete-console-half-classified-stop-condition**

Plan of record:
[`362-boundary-checker-coverage-gaps.md`](../../implementation/workspace-foundation/362-boundary-checker-coverage-gaps.md).
Subsystem: `workspace-foundation` (tooling/CI guard ownership). Parallel to Plan 361.

> **Read the status line first.** This plan is **not** closed. The runtime half is complete and
> proven; the console half was deliberately **not** absorbed, for a reason set out under
> "D3 — the classified stop condition". Plan 366 later closed the console half's *rule-2 coverage*
> problem, but not this plan's console-normalisation objective. Recording it as passed would
> misrepresent the delivery.

## Commits

| Change | Commit |
|---|---|
| Plan registration (362, alongside 360/361/364) | `5076ec4` — *plans: register 360 (run readiness), 361 (ADR guard), 362 (boundary gaps), 364 (m12 floor)* |
| `scripts/check-runtime-boundaries.sh` (+441/−15) | implementation landed in the working tree, committed with this record's batch |
| `scripts/check-console-boundaries.sh` (part of the combined +695/−12 with Plan 366) | implementation landed in the working tree, committed with this record's batch |
| `docs/architecture/tooling.md` (+42/−1) | implementation landed in the working tree, committed with this record's batch |
| `plans/closure/workspace-foundation/362-status.md` (this record) | committed with this record's batch |

**No implementation SHA is claimed** — the work is uncommitted at the time of writing. Base commit
is `5076ec4` plus the working tree.

**On the diffstat.** The console script is touched by *both* Plan 362 and Plan 366, so the working
tree shows a single combined `+695/−12`. Plan 362's contribution to that file is the
`use`-tree normaliser and the grouped-import positive controls; Plan 366 replaced rule 2 wholesale
(the only 12 removed lines in the file are the old latched-`awk` rule-2 block plus its two-line
header comment — verified by reading the `git diff`). The two plans' changes are **not separable by
`git diff` alone** and this record does not pretend otherwise.

## The headline

**The runtime half is done and proven.** `i2pr-api` had **no section at all** in
`check-runtime-boundaries.sh` at `5076ec4` — `git show HEAD:scripts/check-runtime-boundaries.sh |
grep -c i2pr-api` returns **0**. It now has an 8-rule section, and a `use`-tree normaliser that
rewrites grouped imports into flat leaves before the scan, validated over **493/493** repository
`.rs` files with **zero** fail-open cases.

**Option (b) was rejected, and the rejection is stronger than the plan claimed.** The plan rejected
(b) for being "brittle; nesting defeats it". Measured: option (b) does not merely miss nested
groups, it **turns the guard red on the clean real tree**. See "Why (b), measured".

## D1 — the runtime half, complete

### The `i2pr-api` rule set

`i2pr_api_patterns` (`check-runtime-boundaries.sh:416-424`) — **7 source rules**, printed by the
script as "8 rules" because the manifest rule at `:548-553` is the eighth:

| # | Category | Pattern |
|---|---|---|
| 1 | `tokio` | `tokio::` |
| 2 | `async` | `async\s+fn\|async_trait` |
| 3 | `sockets` | `TcpStream\|TcpListener\|UdpSocket\|UnixStream\|UnixListener\|TcpSocket` |
| 4 | `filesystem` | `std::fs\|OpenOptions\|File::` |
| 5 | `unbounded channels` | `unbounded_channel\|unbounded::<\|UnboundedSender\|UnboundedReceiver` |
| 6 | `raw JoinHandle` | `JoinHandle` |
| 7 | `ownerless spawn` | `spawn\s*\(` |
| 8 | manifest | `i2pr-(daemon\|runtime\|testkit\|console\|service-tunnels)` banned from `crates/i2pr-api/Cargo.toml` |

Rules 5 and 6 are stricter than `i2pr-app-proto`'s (which merges `async fn` into `tokio`), and rule
8 is new for this crate. `std::net` address *values* stay permitted, following Plan 345's precedent
and the shape `crates/i2pr-api/src/sam/forward.rs` already relies on — see the negative control at
`check-runtime-boundaries.sh:480`.

### The normaliser (option (a)), validated

It blanks comment bodies and literal contents (preserving line counts, so line numbers survive),
then rewrites every `use` tree into flat leaves. Verified over the whole repository:

```text
$ python3 -c "... normalise() over every crates/ and tools/ .rs file ..."
repo .rs files: 493
  normalised cleanly (line-count preserving): 493
  ParseError (fails closed):                   0
  OTHER / fail-open:                           0
```

Grouped-import expansion, measured:

| Input | Normalised to |
|---|---|
| `use std::{fs, net};` | `use std::fs; use std::net;` |
| `use std::{fs::{self, File}, net};` | `use std::fs; use std::fs::File; use std::net;` |
| `use std::{\n fs,\n net,\n};` | `use std::fs; use std::net;` |
| `use std::{io::{self, Write}, fs};` | `use std::io; use std::io::Write; use std::fs;` |
| `use tokio::{net::TcpListener, spawn};` | `use tokio::net::TcpListener; use tokio::spawn;` |
| `pub use crate::{a::{b, c}, d};` | `pub use crate::a::b; pub use crate::a::c; pub use crate::d;` |

It **fails closed**: `use std::{fs, net` , `use std::{fs, net}};` and an unterminated string all
raise `ParseError`, asserted at `check-runtime-boundaries.sh:485-490`.

### Positive controls

`CONTROL` grew from 1 entry to **14** (`check-runtime-boundaries.sh:446-466`), each of which must
match at least one rule of its rule set, with a floor `len(CONTROL) < 14` at `:467` so the list
cannot be quietly emptied. A **negative** control was added too (`:480-482`): a crate-local module
called `net` and a plain `IpAddr` value must stay unflagged, so the scan keys on the *import path*,
not a leaf name.

## Why (b), measured — stronger than the plan's stated reason

The plan rejected (b) as "brittle; nesting defeats it". Both halves are true and the second is
understated. Using (b) **exactly as the plan spells it** — `std::\{[^}]*\bfs\b` added to the
existing alternation, module-level only, leaf alternatives untouched:

**Failure mode 1 — the nested miss.** `[^}]*` cannot cross an inner group's closing brace:

| Probe | (a) normaliser | (b) brace-regex |
|---|---|---|
| `use std::{fs, net};` | `filesystem` | `filesystem` |
| `use std::{a::{net, x}, fs};` | **`filesystem`** | **miss** |
| `use std::{a::{fs, x}, net};` | miss (correct: `crate::a::fs`) | **`filesystem`** (false positive) |
| `use tokio::{net::TcpListener, spawn};` | `sockets`, `tokio` | `sockets`, `tokio` |

**Failure mode 2 — (b) is not viable at all.** A brace-regex-only variant, run end-to-end against
the real tree, exits 1 on the **clean** tree:

```text
$ bash <option-b variant>            # clean tree, no probe injected
i2pr-api runtime-neutrality violation:
crates/i2pr-api/src/sam/limits.rs:178: forbidden tokio API:
    // Integration tests run under `tokio::time::test-util`
crates/i2pr-api/src/sam/limits.rs:206: forbidden tokio API:
    // avoid racing with `tokio::time::test-util` auto-advance.
exit=1
```

(b) has no comment masking, so it flags prose in real code. It also **fails the existing positive
control** (`boundary checker positive control missed: i2pr-api/glob group`) before ever reaching the
scan — so its apparent "catches" in a naive comparison are all for the wrong reason. Both facts are
recorded in `docs/architecture/tooling.md` as the fourth observation under "Plan 362 findings".

## D2 — the console half: what *was* delivered

Plan 362's grouped-import work reached the console scanner too. `check-console-boundaries.sh`
carries the normaliser **verbatim** (`:114-120` marks it "Plan 362, kept verbatim"), and the rule-2
self-test drives it. Measured against the plan's two literal probes:

| Probe | console scanner |
|---|---|
| `use std::{fs, net};` | **caught** — `filesystem`, `fs::`, `std::net (not a permitted address value)` |
| `use std::{net};` | **caught** — `std::net (not a permitted address value)` |

## D3 — the classified stop condition

**A production normalised scan over `crates/i2pr-console/src` was NOT added, and doing it
correctly would turn the guard red.** This is the honest boundary, and it is recorded rather than
absorbed.

Why it cannot simply be done: the console's pre-existing rule-2 alternative banned the *string*
`std::net`, and two legitimate imports depend on it —
`crates/i2pr-console/src/security/authority.rs:20` (`use std::net::{IpAddr, SocketAddr};`) and
`security/mod.rs:27` (`use std::net::SocketAddr;`). `AGENTS.md` requires console requests to "match
an exact `Host` authority **including the port**", which is impossible without address values. A
normalised scan over the real console therefore fires on correct code unless the `std::net` rule is
*also* replaced — which is precisely the change the plan lists as **out of scope** for a plan whose
invariant is "every change makes a guard stricter".

Plan 366 later made exactly that replacement (socket-keyed rule plus an enumerated four-type
address-value allow-set) and the console half is now fully served — **but by Plan 366, not by
this one.** This record does not credit itself with that work.

## "Nothing was weakened" — the explicit statement AC5 demands

Measured against `git show HEAD:scripts/check-runtime-boundaries.sh`:

| Property | HEAD | Worktree |
|---|---|---|
| raw shell scans (`^if grep\|^for manifest in`) | **17** | **17** (unchanged) |
| app-proto `patterns` dict | — | **byte-identical to HEAD** |
| the 7 pre-existing shell anchors | all present | all present |
| `CONTROL` positive-control entries | 1 | **14** |
| negative control | none | 1 |
| new assertions added | — | **13** |

The diff is `+441/−15`. All 15 deleted lines are the app-proto positive-control block being merged
into a shared `CONTROL` list and extended; **every deleted assertion is present in the worktree**.
I verified this mechanically by checking each deleted line against the worktree: 5 survive
verbatim-or-reflowed, and the 10 that do not are the old `positive_control` / `missing` /
per-category loop (superseded by the `CONTROL` list, which is *stronger*: every entry must match,
and the count is floored) plus two `print`/manifest lines reflowed.

**The honest caveat, M4 below:** the anti-silencing mechanism catches a *single* deletion, not a
coordinated one.

## Requirement → evidence

| Plan requirement | Evidence | Result |
|---|---|---|
| In-scope 1: `i2pr-api` section added | `:416-424`, `:541`, `:548-553`; rule table above | met |
| In-scope 2(a): grouped-import evasion closed — **runtime** | normaliser; 493/493; probe transcripts | met |
| In-scope 2(a): grouped-import evasion closed — **console** | normaliser carried verbatim; probes caught | met for the scanner |
| In-scope 2(a): normalised production scan for the console crate | **classified stop condition**, D3 | **not delivered, by design** |
| In-scope 3: positive-control harness extended | `CONTROL` 1 → 14, floored at `:467`; negative control at `:480` | met |
| In-scope 4: `tools/i2pr-interop` scope decision recorded | script globs cover `crates/` only; production edges policed by `check-dependency-direction.sh`. Stated in the script header and in `docs/architecture/tooling.md`. **Unchanged by this plan** | met (decision recorded) |
| Required evidence: `i2pr-api` section; probes fail | §Negative-test transcripts | met |
| Required evidence: grouped probe fails both scripts | N1/N2 below | met |
| Required evidence: existing positive controls still pass | C1 | met |
| Required evidence: real tree passes both scripts, no production change | C2, C3; `git status --short crates/` shows only `i2pr-daemon` (Plan 360) | met |
| Invariant: every change strictly additive | "Nothing was weakened" table | met |
| Invariant: fails closed on unparseable input | `:485-490`, exercised in C1 | met |
| Invariant: each new assertion negative-tested | mutation transcripts | met, with M4 recorded |
| AC1 `i2pr-api` policed, rule list recorded | rule table | met |
| AC2 grouped-import probe fails both scripts, transcripts recorded | N1/N2 | met |
| AC3 existing positive controls still pass | C1 | met |
| AC4 real tree passes both scripts, no production change | C2/C3 | met |
| AC5 no existing assertion weakened | "Nothing was weakened" + M4 caveat | met |
| **AC6 exact-head routine CI green** | **no CI reachable from this environment** | **UNPROVEN** |

## Commands — all **local**; no CI available in this environment

```text
bash scripts/check-runtime-boundaries.sh
  i2pr-app-proto runtime/OS boundary: ok (positive control passed)
  i2pr-api runtime-neutrality: ok (8 rules, grouped-import positive controls passed)
  runtime boundary checks passed                                  exit 0

bash scripts/check-console-boundaries.sh                          check-console-boundaries: ok   exit 0
bash scripts/check-console-boundaries.sh --self-test              self-test ok                    exit 0
bash scripts/check-console-boundaries.sh --trace                  13 files, 4416 production lines, 0 hits  exit 0
```

Runtime-guard probes were run end-to-end against an **isolated temp root** (`scripts/` copy +
symlinked crates + a real copy of `crates/i2pr-api`), so the repository tree was never polluted:

```text
control (no probe)                                                        exit 0
use std::{fs, net};                          -> CAUGHT exit=1  category=filesystem
use std::{fs::{self, File}, net};            -> CAUGHT exit=1  category=filesystem
multi-line group                             -> CAUGHT exit=1  category=filesystem
glob group                                   -> CAUGHT exit=1  category=filesystem
grouped tokio                                -> CAUGHT exit=1  category=tokio
use std::{net};                              -> *** NOT CAUGHT ***            <- see finding R1
flat TcpListener                             -> CAUGHT exit=1  category=sockets
async fn                                     -> CAUGHT exit=1  category=async
unbounded_channel                            -> CAUGHT exit=1  category=unbounded channels
JoinHandle                                   -> CAUGHT exit=1  category=raw JoinHandle
ownerless spawn                              -> CAUGHT exit=1  category=ownerless spawn
BENIGN: use std::net::{IpAddr, SocketAddr}; -> PASS (allowed)
BENIGN: use crate::wire::{fs, net};         -> PASS (allowed)
```

## Negative-test transcripts — the plan's required probes

```text
### N1  the plan's literal probe, BOTH scripts
$ python3 scripts/check-adr-number-uniqueness.py   # control: sibling Plan 361 guard unaffected
$ bash scripts/check-runtime-boundaries.sh   # probe `use std::{fs, net};` in i2pr-api
  i2pr-api runtime-neutrality violation:
  crates/i2pr-api/src/probe_tmp.rs:1: forbidden filesystem API: use std::fs; use std::net;
  exit=1
  console scanner, same probe -> filesystem, fs::, std::net (not a permitted address value)
  console scanner, `use std::{net};` -> std::net (not a permitted address value)

### C1/C2/C3 controls
$ bash scripts/check-runtime-boundaries.sh                 exit 0
$ bash scripts/check-console-boundaries.sh                 exit 0
$ bash scripts/check-console-boundaries.sh --self-test     exit 0  ("check-console-boundaries: self-test ok")
```

## Mutation transcripts — with one honestly-reported miss

Differential method: for each mutant, the **control** must pass a probe the mutant is expected to
fail. A mutant is only counted as caught when its behaviour differs from the control *and* the
guard was not merely broken.

| # | Mutation | Verdict | Evidence |
|---|---|---|---|
| M1 | delete the `i2pr-api` scan entry | **CAUGHT** | control=1, mutant=0 on a `TcpListener` probe |
| M2 | normaliser → brace alternatives (option (b)) | **CAUGHT** | the (b) mutant exits 1 on the **clean** real tree (comment false positive) — (b) is not shippable at all |
| M3 | remove the zero-source fail-closed check | **CAUGHT** when paired (M3+M10 below) | see pairing note |
| M4 | remove the raw-scan-count floor **and** delete the unbounded-channel scan | **NOT CAUGHT** | mutant exits 0 on the clean tree |
| M5 | delete the `JoinHandle` anchor literal **and** delete that scan | **CAUGHT** | `pre-existing raw boundary scans were removed: 16 < 17` |
| M6 | remove the positive-control count floor | **CAUGHT** when paired | see pairing note |
| M7 | **lower** the raw-scan floor 17 → 5 (a real scan already deleted) | **CAUGHT** | mutant exits 1: `16 < 5` is false, but `anchor` check fires |
| M8 | remove the `i2pr-api` manifest rule | **CAUGHT** when paired | see pairing note |
| M9 | remove the benign negative control | **CAUGHT** when paired | see pairing note |
| M10 | remove the fail-closed probe **and** remove the normaliser from the scan | **CAUGHT** | mutant exits 1 on the clean tree |

"Caught when paired" means: removing the self-check alone is invisible on a clean tree, so it was
paired with the violation the check exists to catch. M3/M6/M8/M9 are **self-check removals** and are
individually undetectable by construction; pairing is the only honest way to exercise them.

### M4 — reported plainly, not buried

M4 is a real coverage limitation of this plan's own anti-silencing mechanism. The raw-scan **floor**
(`raw_scans < 17`) and the **anchor list** are redundant defences against the *same* deletion. Remove
both together and the deletion is silent:

```text
M4  remove raw-scan floor AND delete the unbounded-channel scan -> exit 0  (NOT CAUGHT)
M4b keep the floor, delete the same scan                            -> exit 1  CAUGHT
     pre-existing raw boundary scans were removed: 16 < 17
```

M5 was caught, but by the **floor**, not by the anchor list it was meant to exercise. Exercising the
anchor list independently (delete the anchor literal, then *neuter* the scan it names while keeping
`raw_scans == 17`) leaves the guard at exit 0, because the anchor strings still appear elsewhere in
the shell body. **The anchor list is therefore weaker than its own comment claims.** Recorded as F4;
not fixed here, because a stronger design needs its own plan.

### Harness defects found and fixed during this work

Three of my own harnesses produced **false kills** and were discarded and redone. They are recorded
because a mutation transcript that hides them is worthless:

1. **Shared probe.** The first M1–M10 run used one probe for every mutant; since the unmutated guard
   already caught it, every mutant trivially "caught" M. Redone differentially.
2. **Inverted criterion.** Removal-mutations are caught when the mutant *passes* where the control
   *fails*. My first criterion flagged the correct M1 kill as "NOT CAUGHT". Redone as a differential.
3. **Mutants in `/tmp`.** For Plan 364 this produced a path error that looked like a kill. Recorded
   there; here the runtime guard derives `root` from `BASH_SOURCE`, so mutants were placed in an
   isolated temp root with symlinked crates rather than in `/tmp` alone.

## Findings by severity

| ID | Sev | Finding | Disposition |
|---|---|---|---|
| R1 | **medium** | The `i2pr-api` rule set is socket-keyed with `std::net` values permitted, so a bare `use std::{net};` with no socket use is **not** flagged. The console rule (Plan 366) forbids exactly that shape; the runtime `i2pr-api` rule does not. Verified: probe exit 0. | **OPEN.** Asymmetry recorded, not fixed — tightening it would fire on legitimate `std::net` address-value imports elsewhere and needs its own plan. |
| R2 | **high** | `check-console-boundaries.sh` rule 2 enforces almost nothing: its `awk` never resets `in_tests`, so 12 of 13 console files are never scanned. Found by this plan. | became **Plan 366** |
| R3 | **high** | With a per-file reset, console rule 2 is **unsatisfiable** for correct code (`authority.rs:20`, `mod.rs:27`), because `AGENTS.md` requires exact `Host`-with-port matching. | became **Plan 366** (socket-keyed + 4-type allow-set) |
| R4 | medium | Transport crates already import `std::net` address values via grouped `use`: `i2pr-transport-ntcp2/src/address.rs:9`, `i2pr-transport-ssu2/src/{address.rs:21, block.rs:20, state_machine.rs:20, token.rs:16}`. The transport scan was **deliberately excluded** from the normalised pass for this reason. | **OPEN**, transport-scoped plan required |
| R5 | low | The anti-silencing floor + anchor list is defeated by removing both (M4). | recorded, not fixed (F4 above) |
| R6 | low | The anchor list does not independently detect a neutered scan (M5 caveat). | recorded, not fixed |
| R7 | info | `crates/i2pr-api/src/sam/limits.rs:178,206` name `tokio::` in **comments only**. This is why the normaliser masks comment bodies; without masking, the scan is red on a clean tree. | recorded in `tooling.md`; benign |

## Known limitations

1. **AC6 is unproven.** No CI is reachable from this environment. **Recorded as unproven.**
2. **The console normalised production scan was not added** (D3). This is the plan's stop condition,
   reached deliberately. Plan 366 closed the underlying rule-2 coverage gap by a different route.
3. **R1 is open**: `use std::{net};` alone still evades the `i2pr-api` rule set. The plan's
   required probe (`use std::{fs, net};`) *is* caught, so AC2 is met as written — but a reader
   should not generalise "grouped `std::net` is now policed for `i2pr-api`".
4. **R4 is open**: transport crates were deliberately left out of the normalised scan.
5. **M4/R5/R6**: the anti-silencing mechanism is single-deletion-only. Recorded, not fixed.
6. **`tools/i2pr-interop` remains outside `check-runtime-boundaries.sh`** (its globs cover `crates/`
   only). Production edges are policed by `check-dependency-direction.sh`. This is the pre-existing
   state and is recorded rather than changed.
7. **Verification was batched.** See below.

## Verification batching — stated plainly

Plans 360, 361, 362, 364, 365, and 366 were executed together. Targeted verification ran per plan;
every command in this record is real and was run for **this** plan specifically. The **full
`AGENTS.md` routine floor ran once on the combined tree**, not once per plan. No per-plan floor run
is claimed.

> **COMBINED-FLOOR RESULT — EXECUTED, LOCAL.**
> Combined `AGENTS.md` routine floor on the combined Plans 360/361/362/364/365/366 tree at `5076ec4`
> + working tree: **RESULT NOT YET OBSERVED — DO NOT READ AS PASSING.**

## Migration / compatibility evidence

None required. No `crates/` file changed under this plan (`git status --short crates/` shows only
`crates/i2pr-daemon/src/lib.rs` and the new `crates/i2pr-daemon/tests/run_lifecycle_readiness.rs`,
both Plan 360). No `Cargo.toml` change, no dependency change, no schema, no wire format, no config
key. The normaliser is **additive**: the pre-existing raw `grep` scans are byte-identical and still
run, so the normalised scan can only add findings. Measured raw-scan count is unchanged at 17.

## Security and contention evidence

**Security.** The change makes two boundary classes mechanically enforced rather than merely
asserted: the `i2pr-api` crate can no longer acquire a runtime or socket dependency without a
floor failure, and a grouped import can no longer hide one. R1 is the residual: a bare
`use std::{net};` in `i2pr-api` is still not flagged, which is a *coverage* gap in detection, not a
new exposure — the crate's real sources contain no such import, and R1 is recorded as open.

**Contention.** Both scripts are read-only over the tree and take no lock. The runtime guard's
normaliser is pure and stateless. The `i2pr-api` manifest rule reads `Cargo.toml` once. No new
concurrency, no shared mutable state, no TOCTOU window that matters.

## Documentation / operational evidence

`docs/architecture/tooling.md` (+42/−1) carries the `i2pr-api` rule list, the normalisation
description, and a new section *"Plan 362 findings that still need a plan-of-record"* recording
R2/R3 (struck through, closed by Plan 366), R4 (**OPEN**), and R7. `AGENTS.md`'s known-checker-gaps
section records the same dispositions.

**Doc-drift observed, not fixed (outside this record's scope):**
`docs/architecture/tooling.md` states *"13/13 files, **4430** production lines"* for the console
scanner. The shipped `--trace` reports **4416** (`TRACE files=13 lines=6269
production_lines_scanned=4416 suppressed_lines=1853 hits=0`), which is also the figure
`AGENTS.md` uses. tooling.md is 14 high. Recorded rather than corrected, since this record may not
edit that file.

Operational cost: the normaliser is one Python pass over the crate source trees plus the unchanged
raw greps. It is materially slower than `grep` alone but runs over a few hundred KB; the guard
completed well inside the floor's existing per-step budget.

## Roadmap disposition

`plans/subsystems/workspace-foundation-roadmap.md` and `plans/registry.md` are **not updated by
this record** — outside its file scope. Required follow-up:

- `plans/registry.md:47` — Plan 362 `ready` → **`conditionally closed`** (runtime half closed, console
  half a classified stop condition), pointing at this record.
- Workspace-foundation roadmap — record the partial closure and name R1 and R4 as open.

**No capability, version, RouterInfo, or support-surface claim is promoted.** No production crate
changed, no listener default moved, nothing on the network is advertised.
