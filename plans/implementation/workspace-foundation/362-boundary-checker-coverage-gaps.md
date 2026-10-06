# Plan 362 — close the runtime/console boundary-checker coverage holes

Status: **registered-verified-two-coverage-gaps-one-evasion-one-absent-crate**

Classification: **invariant** (static guards for CI-enforced boundaries).

Subsystem: `workspace-foundation` (tooling/CI guard ownership). Parallel to Plan 361.

## Two gaps, both verified empirically on 2026-10-06

### Gap 1 — `check-runtime-boundaries.sh` has no `i2pr-api` section

The script's "passed" result is **not evidence for `crates/i2pr-api`**. It has sections for
`i2pr-runtime`, `i2pr-testkit`, `i2pr-i2pcontrol` (Plan 286), and `i2pr-app-proto` (Plan 345).
`i2pr-api` — the runtime-neutral SAM 3.1 + I2CP wire/state crate that AGARDRAILS explicitly places
in the "no Tokio/sockets/`async fn`" boundary class — is simply absent. Its globs cover `crates/`
only, and `tools/i2pr-interop` is outside the script entirely (already recorded; production edges
are policed by `check-dependency-direction.sh`).

### Gap 2 — grouped `use std::{…}` evades both source scans

**Verified, not asserted.** Both scripts grep for *contiguous module-path strings* (`std::net`,
`std::fs`) plus a set of type names (`TcpStream`, `File::`, …). A grouped import writes the path
with a brace, so the module string is broken:

```rust
use std::{fs, net};
fn read(p: &str) -> String { fs::read_to_string(p).unwrap_or_default() }
fn addr() -> net::IpAddr { net::IpAddr::V4(net::Ipv4Addr::LOCALHOST) }
```

Probe results against the **actual** alternations in the scripts:

| Probe | `check-runtime-boundaries.sh` | `check-console-boundaries.sh` |
|---|---|---|
| `use std::{fs, net};` + `fs::read_to_string` + `net::IpAddr` | **NO MATCH — evades** | matched only via bare `fs::` |
| `use std::{net};` + `net::IpAddr` | — | **NO MATCH — evades** |

So the console's `fs::` alternative happens to save the `fs` case, but **`net` alone evades both**.
`net::IpAddr` is explicitly *permitted as a value* by Plan 345 for `i2pr-app-proto`, which is
exactly why a check keyed on type names alone cannot distinguish "an address value" from "a socket";
the honest fix is to detect the **import**, not the usage.

Note the console still has a working *manifest* rule (rule 1 checks `Cargo.toml` dependency names).
That is a different mechanism and does not catch source-level violations.

## Why ready

- **No hard dependency, no interface dependency, no new dependency, no production code change.**
  These are Python/shell guards over a stable source tree.
- Both scripts already exist and already have positive-control harnesses (`check-runtime-boundaries.sh`
  has one at `:133` that proves each forbidden category is detectable). This extends that idea.

## Objective

A grouped `use std::{…}` is detected as reliably as a flat import, and `i2pr-api` is policed by the
same script that polices its boundary peers.

## In scope

1. **Add an `i2pr-api` section to `check-runtime-boundaries.sh`**, matching the rules its sibling
   runtime-neutral crates get: no `tokio::*`, no sockets, no `std::fs`, no `async fn`, no unbounded
   channels, no raw `JoinHandle`/ownerless `spawn`.
2. **Close the grouped-import evasion in both scripts.** Options, in preference order:
   - **(a) Normalise before scanning.** Strip `use … { … }` group braces into separate `use` lines,
     or scan the import list segment-wise, so `std::{fs, net}` yields the same tokens as
     `use std::fs; use std::net;`. Robust and language-shaped.
   - **(b) Add the brace forms as explicit alternatives** (`std::\{[^}]*\bnet\b`). Brittle; nesting
     and multiple groups defeat it.
   - **(c) Parse with a real Rust parser.** Not available without a new dependency — out of scope.

   **(a) is required.** The temptation is (b), because it is a one-line diff that would look like a
   fix in review while leaving `std::{a::{fs, net}}` and `std::{env, process}` working. Choose the
   change that makes the *probe* fail, not the one that makes the diff small.
3. **Extend each script's positive-control harness** with the grouped-import probe, so a future edit
   to the scan is caught by the same mechanism that already exists for this one.
4. **`tools/i2pr-interop` scope decision** — record explicitly whether the runtime script's globs
   should extend there, or state that production edges are covered by
   `check-dependency-direction.sh` and that the omission is deliberate. Do not leave it ambiguous.

## Out of scope

- **Changing `i2pr-api`'s actual code.** This plan only adds coverage. If `i2pr-api` violates a
  rule, that is a finding to report and fix under its own corrective — not something to widen an
  exemption for here.
- **Weakening, deleting, or adding an exemption to any existing assertion.** Every change here adds
  detection. If a new rule fires on existing legitimate code, that is a real finding.
- **The console's manifest rule 1.** It works and is not part of this gap.
- **Plan 359's `i2pr-proto` named exception** in `check-dependency-direction.sh`. Untouched.

## Invariants

- **Every change makes a guard stricter.** No assertion is relaxed, narrowed, or exempted. If a
  change cannot be justified as strictly additive, it does not belong in this plan.
- **Fails closed** on a scan it cannot parse, rather than skipping.
- **Each new assertion is negative-tested** with a probe that fails *for the right reason*.
- **No new dependency** — POSIX shell + Python 3 standard library, matching existing scripts.
- The two scripts stay independently runnable and keep their current terminal success lines.

## Required evidence

- `check-runtime-boundaries.sh` has an `i2pr-api` section; a probe injecting `tokio::spawn(`,
  `TcpListener`, `async fn`, and `std::fs` into a temp `i2pr-api`-shaped fixture **fails** each.
- The grouped-import probe (`use std::{fs, net};`) now **fails** both scripts where it previously
  passed.
- The existing positive-control harness still passes.
- The real `crates/i2pr-api` passes its new section on the current tree.
- Both scripts still pass on the real tree with **no** production change.

## Production changes

`scripts/check-runtime-boundaries.sh`, `scripts/check-console-boundaries.sh`. No `crates/` change, no
`Cargo.toml` change, no dependency change.

## Documentation updates

- `AGENTS.md` — the "Known checker gaps" section: remove the `i2pr-api` and grouped-import bullets
  once closed, and record the new positive-control probes.
- `docs/architecture/tooling.md` — the per-script rule inventory.

## Acceptance criteria

Plan 362 passes only when:

1. `i2pr-api` is policed by `check-runtime-boundaries.sh` with a recorded rule list;
2. the grouped-import probe fails both scripts, and the **negative-test transcripts** are recorded;
3. both scripts' existing positive controls still pass;
4. the real tree passes both scripts with no production change;
5. **no existing assertion was weakened** — stated explicitly, since the whole risk of this plan is
   that "closing a gap" gets done by carving an exception;
6. exact-head routine CI is green.

## Stop conditions

Stop and record a classified boundary if the brace-normalisation approach cannot be made to fail
closed on malformed or nested `use` syntax, or if `i2pr-api`'s real source violates a rule the
sibling crates are held to — in which case the finding is reported and a corrective registered, not
absorbed by relaxing the rule.

## Closure evidence required

Commits; requirement-to-evidence matrix; commands with local/CI outcomes labelled truthfully; the
**negative-test transcripts for every new assertion**; the explicit "nothing was weakened" statement
with the before/after rule count; known limitations; findings by severity; roadmap disposition.

**Note on verification batching.** Executed alongside Plans 360, 361, and 364. Targeted verification
runs per plan; the full routine floor runs once on the combined tree, and the closure record must
say so rather than implying a per-plan floor run.