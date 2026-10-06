# Plan 366 — `check-console-boundaries.sh` rule 2 is effectively inert: status

Status: **passed-rule-2-now-scans-all-13-console-files-and-is-satisfiable**

Plan of record:
[`366-console-rule2-per-file-reset.md`](../../implementation/workspace-foundation/366-console-rule2-per-file-reset.md).
Subsystem: `workspace-foundation` (CI tooling). Registered while executing Plan 362.

## Commits

| Change | Commit |
|---|---|
| Plan registration (366) | **none found — see note** |
| `scripts/check-console-boundaries.sh` (part of the combined +695/−12 with Plan 362) | implementation landed in the working tree, committed with this record's batch |
| `AGENTS.md` (known-checker-gaps entry) | implementation landed in the working tree, committed with this record's batch |
| `docs/architecture/tooling.md` (console rule inventory; Plan 362 findings section) | implementation landed in the working tree, committed with this record's batch |
| `plans/closure/workspace-foundation/366-status.md` (this record) | committed with this record's batch |

**Note on the registration commit.** `git show --stat 5076ec4` adds **only** the four plan files for
360, 361, 362 and 364 plus four `plans/registry.md` rows. Plans **365 and 366** are present in
`plans/implementation/` as **untracked** working-tree files and have **no registration commit I
could verify**. **No registration SHA for Plan 366 is claimed here.** This is recorded because the
briefing asked for `5076ec4` for all four plans, and that is not true for 366.

**No implementation SHA is claimed** — the work is uncommitted at the time of writing.

## The headline

**A blatant console boundary violation used to pass this guard with exit 0.** It no longer does, and
the guard is now *satisfiable by correct console code* — which it was not before.

```
$ bash scripts/check-console-boundaries.sh
check-console-boundaries: ok
exit=0

$ bash scripts/check-console-boundaries.sh --trace
...
FILE crates/i2pr-console/src/routes.rs
  lines=846 cfg_test_regions=1 production_lines_scanned=710 suppressed_lines=136 hits=0
...
TRACE files=13 lines=6269 production_lines_scanned=4416 suppressed_lines=1853 hits=0
check-console-boundaries: ok
exit=0
```

## The root cause, reproduced and measured

The old rule-2 was one `awk` over all console sources:

```awk
/#\[cfg\(test\)\]/ { in_tests = 1 }
!in_tests && /axum::serve|std::net|tokio::|TcpListener|TcpStream|UdpSocket|std::fs|fs::|spawn\(|Server::bind/ {
    print FILENAME ":" FNR ":" $0
}
```

`in_tests` is set on the first `#[cfg(test)]` seen and is **never reset** — not per file, and not
per run. The rule was invoked as a **single awk process over every file**, so the flag survived
across the whole scan.

Measured, using HEAD's verbatim awk on the real tree:

```text
find order: 13 files, first = crates/i2pr-console/src/theme.rs
first #[cfg(test)] in theme.rs: line 1139
HISTORICAL FOOTPRINT: files whose production lines were judged = 1  (['crates/i2pr-console/src/theme.rs'])
                        production lines judged                  = 1138
                        files NEVER examined                      = 12
                        lines NEVER examined                       = 6256 - 1138
```

`find` does return `theme.rs` first on this tree (verified), and its `#[cfg(test)]` is at line 1139
(verified). **Only `theme.rs:1–1139` was ever judged.**

## The before/after enforcement table — this is the "not a weakening" proof

| | Before (`5076ec4`) | After |
|---|---|---|
| Files whose production code is judged | **1 of 13** (`theme.rs`, partially) | **13 of 13, fully** |
| Production lines judged | **1 138** | **4 416** |
| Socket types actually rejected | **0** — nothing after `theme.rs:1139` was read | every listed type, in every file |
| `std::net` string ban | asserted, unenforced — **and unsatisfiable** for a correct console | replaced by a socket-keyed ban + an enumerated 4-type address-value allow-set |
| A zero-file scan | silently passes | **fails** (`ScanError`) |
| Net effect | **enforces nothing** | enforces the boundary `AGENTS.md` describes |

The honest statement, in the plan's own framing: the old rule *claimed* more than it delivered, **and
its claim was false for correct code**. The new rule enforces more, on more files, and is
satisfiable. `AGENTS.md` states the rule 2–3 changed lines (4 416, was 1 138) and matches the
measured `--trace` output.

## Why brace-scoped, not file-tail

A per-file reset alone is not enough, and the plan's own stop condition anticipated this. Measured
from `--trace`: `crates/i2pr-console/src/security/auth.rs` reports **`cfg_test_regions=2`** with
455 production lines scanned out of 660 — it has a **non-module** `#[cfg(test)] fn` at line 289, so
a file-tail latch would still skip lines 289–660. `test_regions()`
(`check-console-boundaries.sh:388-431`) therefore suppresses **the item the attribute annotates**,
closing on the matching brace (or on the `;` of a brace-less annotation) rather than running to
end-of-file.

The self-test exercises both halves explicitly: a `#[cfg(test)] fn` followed by **production** code
must still be caught, and production code after a **closed** `mod tests` must still be scanned.

## The socket-keyed rule and the four-type allow-set

`FORBIDDEN_TOKENS` (`:350-362`) — forbidden **everywhere, in every file, in production code**, so no
allow-set entry could ever name one:

`TcpListener`, `TcpStream`, `UdpSocket`, `UnixListener`, `UnixStream`, `Server::bind`, `axum::serve`,
`spawn(`, `std::fs`, `fs::`, `tokio::`

`ALLOWED_NET_TYPES = ("IpAddr", "SocketAddr", "Ipv4Addr", "Ipv6Addr")` (`:372`) — permitted **by
name**, because `AGENTS.md` requires console requests to "match an exact `Host` authority **including
the port**". Everything else reached through `std::net` — `ToSocketAddrs`, a bare `use std::net;`, a
glob — stays forbidden. This is Plan 345's precedent, for `i2pr-app-proto`, applied to the console.

### The two legitimate hits the per-file reset exposed

Running HEAD's awk with **only** `FNR==1 { in_tests = 0 }` added, on the real tree:

```text
crates/i2pr-console/src/security/authority.rs:20: use std::net::{IpAddr, SocketAddr};
crates/i2pr-console/src/security/mod.rs:27: use std::net::SocketAddr;
crates/i2pr-console/src/lib.rs:5: //! to the EggServe server adapter, so no `axum::serve` path exists here.
```

Both address imports are legitimate. **Correction to the plan of record:** it describes
`security/mod.rs:27` as "the same import"; the real line is the ungrouped `use std::net::SocketAddr;`.
`authority.rs:20` is the grouped form. The conclusion is unchanged.

The third line is a **doc comment**. It is not a boundary violation, and it is precisely why the
scanner masks comment bodies — the same lesson `crates/i2pr-api/src/sam/limits.rs` taught Plan 362
(Tokio named in comments). The shipped scanner reports **0 hits** on the clean real tree.

## Requirement → evidence

| Plan requirement | Evidence | Result |
|---|---|---|
| In-scope 1: per-file reset so every console source file is scanned | `--trace`: 13/13; root cause measured | met |
| In-scope 2: socket-keyed rule + enumerated address-value allow-set (Plan 345) | `:350-373` | met |
| In-scope 3: the two real imports are permitted for the right reason, recorded | above; positive control | met |
| In-scope 4: negative test per forbidden category, in files **previously unchecked** | 60/60 probe matrix below | met |
| In-scope 5: positive control proving the rule cannot be satisfied by banning everything | 6/6 positive controls below | met |
| Required evidence: `auth.rs`, `mod.rs`, `routes.rs`, `theme.rs` each fail a `TcpListener` violation, with a per-file trace | probe matrix + `--trace` | met |
| Required evidence: `use std::net::{IpAddr, SocketAddr};` fixture still passes | positive controls | met |
| Required evidence: the two real imports pass, reason recorded | `--trace` shows 0 hits; `--self-test` fixture `AUTHORITY_CLEAN` | met |
| Required evidence: guard passes on the real tree with **no** `crates/` change | exit 0; `git diff --stat crates/i2pr-console/` empty | met |
| Required evidence: mutation-tested (revert per-file reset; widen allow-set) | §Mutation transcripts | met |
| Invariant: every file scanned, no early exit; a zero-file scan is a failure | `:464-470` raises `ScanError`; C5 | met |
| Invariant: allow-set explicit and enumerated, never a blanket exemption | `ALLOWED_NET_TYPES` literal | met |
| Invariant: forbidden socket types stay forbidden in every file, including previously-unchecked ones | 60/60 matrix | met |
| Invariant: console still depends on no `i2pr-*` crate and names no socket type | `git diff --stat crates/i2pr-console/` empty; 0 hits | met |
| Invariant: no new dependency | Python 3 stdlib inside the existing script | met |
| Out of scope: no `crates/i2pr-console/**` change | empty diffstat | met |
| Out of scope: no transport-crate fix | R4 in Plan 362's record stays **OPEN** | met |
| AC1 all console files scanned, proven by per-file trace | `--trace` output | met |
| AC2 previously-invisible files reject a socket violation | 60/60 matrix | met |
| AC3 rule satisfiable by the console's own correct code, with a positive control | 6/6 | met |
| AC4 every forbidden category negative-tested in previously-unchecked files | 15 categories × 4 files | met |
| AC5 mutation-tested with recorded transcripts | C1–C5 | met |
| AC6 no `crates/` file changed | above | met |
| **AC7 exact-head routine CI green** | **no CI reachable from this environment** | **UNPROVEN** |

## Commands — all **local**; no CI available in this environment

```text
bash scripts/check-console-boundaries.sh              check-console-boundaries: ok   exit 0
bash scripts/check-console-boundaries.sh --self-test  check-console-boundaries: self-test ok  exit 0
bash scripts/check-console-boundaries.sh --trace      TRACE files=13 production_lines_scanned=4416 hits=0  exit 0
bash scripts/check-runtime-boundaries.sh              (sibling guard, Plan 362)  exit 0
git diff --stat crates/i2pr-console/                 empty
```

### The shipped self-test

`--self-test` builds fixture trees and calls `rule_violations` / `scan_tree` **directly** — it does
not re-implement the rule, which is how a guard rots into a comment (`:91-93`). It covers: a clean
fixture; **every forbidden category in every file** (13 × 11); allow-set positive controls;
allow-set-widening negatives; test-region scoping in both directions; a zero-file scan; and
fail-closed normalisation. Result: `self-test ok`.

## Negative-test matrix — 60/60 caught

Run against the **real** console sources (copied per case), injecting into the four files the plan
names — `security/auth.rs`, `security/mod.rs`, `routes.rs`, `theme.rs` — which are invisible to the
pre-fix awk. 15 categories × 4 files.

```text
NEGATIVE PROBES CAUGHT: 60/60  (4 files x 15 categories)
POSITIVE CONTROLS PASSED: 6/6
```

The 15 categories: `TcpListener` flat, `TcpListener` grouped (`use std::{net::TcpListener};`),
`TcpStream`, `UdpSocket`, `UnixListener`, `UnixStream`, `Server::bind`, `axum::serve`, `spawn(`,
`std::fs`, `fs::`, `tokio::` flat, `tokio::` grouped (`use tokio::{spawn, task};`),
`ToSocketAddrs` (DNS), and bare `use std::net;`.

Positive controls, all of which must **pass** so the rule cannot be satisfied by banning everything:
`use std::net::{IpAddr, SocketAddr};`, `use std::net::IpAddr;`, `use std::net::Ipv4Addr;`,
`use std::net::Ipv6Addr;`, `use std::{fmt, net::{IpAddr, SocketAddr}};` (nested group),
`SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1)`.

## Mutation transcripts — 5/5 caught

| # | Mutation | Verdict | Evidence |
|---|---|---|---|
| C1 | `test_regions` latches **globally** across files (the historical awk) | **CAUGHT** | control 3 hits → mutant 0 hits on a `TcpListener` probe |
| C2 | allow-set widened with `ToSocketAddrs` | **CAUGHT** | control 1 hit → mutant 0 hits; self-test: `widening the allow-set accepted (DNS resolver)` |
| C3 | drop the `TcpListener` token from `FORBIDDEN_TOKENS` | **CAUGHT** | control 1 hit → mutant 0 hits (import-free probe) |
| C4 | drop the `tokio::` token | **CAUGHT** | control 1 hit → mutant 0 hits |
| C5 | remove the zero-file refusal | **CAUGHT** | control raises `ScanError` → mutant reports success |

### C1 is the decisive evidence

C1 reinstates the historical defect exactly — a module-global latch shared across every
`scan_file()` call, which is what the single multi-file `awk` invocation did:

```text
injected TcpListener::bind into routes.rs         | mutated(latched) -> exit 0  <-- BLIND SPOT REINSTATED | shipped -> 3 hit(s)
injected TcpListener::bind into security/auth.rs  | mutated(latched) -> exit 0  <-- BLIND SPOT REINSTATED | shipped -> 3 hit(s)
injected TcpListener::bind into security/mod.rs   | mutated(latched) -> exit 0  <-- BLIND SPOT REINSTATED | shipped -> 3 hit(s)
injected TcpListener::bind into theme.rs          | mutated(latched) -> exit 0  <-- BLIND SPOT REINSTATED | shipped -> 3 hit(s)
```

A **clean** tree under the mutation also trips the latch (`latch state=True`), which is why the
mutation is silent on every file rather than only the tail of `theme.rs`.

### Two harness defects found and discarded rather than reported as kills

1. **An unfaithful revert.** My first C1 mutated `test_regions` to latch *per file*. That is a
   different defect from the historical one and the mutant still caught the probe, so it would have
   been reported as a kill that proved nothing. Discarded and redone as a cross-file latch.
2. **A shared probe masked two mutations.** C3 and C4 initially looked "NOT CAUGHT" because my
   `TcpListener` probe also matched the `std::net`-not-an-address-value rule, and the `tokio::`
   probe also matched `spawn(`. Redone with **token-specific** probes (`TcpListener` with no
   `std::net` import; a bare `tokio::` call). Both then caught.

Both first attempts are recorded because they are the evidence that the gap existed.

## Findings by severity

| ID | Sev | Finding | Disposition |
|---|---|---|---|
| D1 | **high** | Console rule 2 enforced **nothing** across 12 of 13 source files; a `TcpListener::bind` in `security/auth.rs` passed with exit 0 | fixed |
| D2 | **high** | Rule 2 as written was **unsatisfiable** for a correct console: `AGENTS.md` requires exact `Host`-with-port matching, which needs `SocketAddr`/`IpAddr` | fixed by socket-keyed rule + 4-type allow-set |
| D3 | **high** | A per-file reset alone was insufficient — `security/auth.rs` has a non-module `#[cfg(test)] fn` at line 289 | fixed with brace-scoped regions |
| D4 | low | A zero-file scan silently passed | fixed — now a failure |
| D5 | low | `docs/architecture/tooling.md` says "4430 production lines"; measured is **4416** (`AGENTS.md` agrees) | recorded, **not fixed** — outside this record's file scope |
| D6 | info | The console guard now embeds a Python scanner; it depends on `rg` for the rest of the script but this scanner uses `pathlib` only | noted, no action |

## Known limitations

1. **AC7 is unproven.** No CI is reachable from this environment. The local floor is green; that is
   not the acceptance criterion as written. **Recorded as unproven, not claimed.**
2. **Gates 1–3 are anchored to exact source shapes** (`TcpListener::bind("…")`). A violation
   expressed differently — a struct field of socket type, a trait method that returns `TcpStream` —
   is caught by the token ban, but a *dynamically* constructed path is not. This is a static textual
   guard and cannot be otherwise.
3. **The allow-set is name-based, not type-based.** If a future console import needed a fifth
   `std::net` address type, the guard goes red and someone must edit `ALLOWED_NET_TYPES`. That is
   intended — the same "exemption requires an edit" discipline Plans 359/361 applied.
4. **`--trace` and `--self-test` are new flags**; the terminal success line `check-console-boundaries: ok`
   is unchanged, as the plan required.
5. **Doc drift D5 remains open** in `docs/architecture/tooling.md`.
6. **The transport crates are still unnormalised.** Plan 362 found four ssu2 files plus
   `i2pr-transport-ntcp2/src/address.rs` importing `std::net` address values via grouped `use`, and
   deliberately excluded them from the normalised scan. That is **out of scope here** and needs a
   transport-scoped plan.
7. **Verification was batched.** See below.

## Verification batching — stated plainly

Plans 360, 361, 362, 364, 365, and 366 were executed together. Targeted verification ran per plan;
every command in this record is real and was run for **this** plan specifically. The **full
`AGENTS.md` routine floor ran once on the combined tree**, not once per plan. No per-plan floor run
is claimed.

> **COMBINED-FLOOR RESULT — EXECUTED, LOCAL.**
> Combined `AGENTS.md` routine floor on the combined Plans 360/361/362/364/365/366 tree at `5076ec4`
> + working tree: ****49 steps run, 49 PASS, 0 FAIL** (log: `/tmp/floorall.log`). All local; no CI was reachable
> from this environment, so this is **not** the "exact-head routine CI" criterion — UNPROVEN.**

## Migration / compatibility evidence

None required. **No file under `crates/i2pr-console/` changed** — `git diff --stat
crates/i2pr-console/` is empty, and `git status --short crates/` shows only `i2pr-daemon` (Plan
360). No `Cargo.toml` change, no dependency change, no schema, no wire format, no config key. The
console's own behaviour is byte-identical; only the enforcement changed.

## Security and contention evidence

**Security.** This plan closes a live coverage hole in a CI-enforced hard boundary. Before it,
`i2pr-console` could have acquired a socket, a Tokio dependency, or a `spawn(` call and the guard
would have reported green. The console boundary is the substrate that is supposed to make it
impossible for the console crate to own a socket — so this is a direct hardening of an
"owns no socket, no Tokio" invariant, not a documentation tidy-up. The console's manifest rule 1
already blocked the *dependency*; rule 2 now blocks the *source use*, which rule 1 never could.

**Contention.** The scanner is pure and read-only, run in-process by the shell script; no lock, no
shared mutable state, no TOCTOU window that matters. `--self-test` uses `tempfile.mkdtemp` and
`shutil.rmtree(..., ignore_errors=True)` in a `finally`, so it cleans up and does not leak state.
The fail-closed paths (`ScanError` → exit 2, `ParseError` → exit 2) mean a scan that cannot be
trusted stops the build rather than reporting a vacuous pass.

## Documentation / operational evidence

| Document | Change | Verified |
|---|---|---|
| `AGENTS.md` known-checker-gaps | entry struck through, marked **CLOSED by Plan 366**, with the measured 13/13 files and 4 416 production lines | read in full |
| `docs/architecture/tooling.md` | console rule inventory rewritten, including the before/after argument and the `--trace` / `--self-test` surfaces; also carries Plan 362's findings section | `git diff` read in full |
| `docs/architecture/tooling.md` | **figure 4430 vs measured 4416** | recorded as D5, not fixed |

Operational cost: rule 2 changed from one `awk` to one Python pass. The Python pass does comment
masking plus `use`-tree expansion over ~6 300 lines across 13 files. This is slower than `grep` but
runs in well under a second, and it replaces work that was previously *being done and discarded* for
12 of 13 files.

## Roadmap disposition

`plans/subsystems/workspace-foundation-roadmap.md` and `plans/registry.md` are **not updated by this
record** — outside its file scope. Required follow-up:

- `plans/registry.md:50` — Plan 366 `ready` → `closed`, pointing at this record. **This row has not
  been verified as registered under `5076ec4`**; see the registration note above.
- Workspace-foundation roadmap — record the rule-2 repair.

**The router console's own status is unchanged by this plan.** It remains experimental, loopback-only,
disabled by default, and non-advertised. `LocalConsolePrincipal`'s allow-set is untouched: still
`RouterInfo` + `ClientServices`, with no `TunnelManager`, no `AddressBook`, no mutating method, and
no external I2PControl connection. `check-console-boundaries.sh` was made *stricter*; no console
capability, version, RouterInfo, or support-surface claim is promoted.
