# Plan 366 — `check-console-boundaries.sh` rule 2 is effectively inert

Status: **registered-verified-high-severity-console-boundary-coverage-failure**

Classification: **invariant corrective** (a CI-enforced hard-boundary guard that enforces almost nothing).

Subsystem: `workspace-foundation` (CI tooling), discovered while executing Plan 362.

## The defect, reproduced

`check-console-boundaries.sh` rule 2 ("console source purity") runs one `awk` over every
`crates/i2pr-console/src/**/*.rs` file:

```awk
/#\[cfg\(test\)\]/ { in_tests = 1 }
!in_tests && /axum::serve|std::net|tokio::|TcpListener|TcpStream|UdpSocket|std::fs|fs::|spawn\(|Server::bind/ {
    print FILENAME ":" FNR ":" $0
}
```

`in_tests` is set on the first `#[cfg(test)]` seen and **is never reset per file**. `find` returns
`theme.rs` first, and `theme.rs` has `#[cfg(test)]` at line 1139. So from that line onward `in_tests`
stays `1` for the rest of the run.

**Net effect: the rule scans `theme.rs` lines 1–1139 and nothing else. 12 of the 13 console source
files are never examined at all.**

### Reproduction (executed)

`use std::net::TcpListener;` + `TcpListener::bind("127.0.0.1:0")` injected into
`crates/i2pr-console/src/security/auth.rs`:

```text
$ bash scripts/check-console-boundaries.sh
check-console-boundaries: ok
exit=0
```

The identical violation injected into `theme.rs` (before its `cfg(test)`) **is** caught. Traced awk
output:

```text
FILE: crates/i2pr-console/src/theme.rs
  HIT 1
  cfg(test) at 1140
FILE: crates/i2pr-console/src/html.rs
  cfg(test) at 65          # already suppressed
FILE: crates/i2pr-console/src/color.rs
  cfg(test) at 226         # already suppressed
```

This is the same defect class Plan 362 exists to close — an assertion that reads as enforcing a
boundary while enforcing none of it — except here it is far more severe than a grouped-import
evasion.

## The second, coupled problem: the rule as written is unsatisfiable

Adding the missing per-file reset (`FNR==1 { in_tests = 0 }`) makes the rule actually run, and it
immediately produces **two real hits**:

- `crates/i2pr-console/src/security/authority.rs:20` — `use std::net::{IpAddr, SocketAddr};`
- `crates/i2pr-console/src/security/mod.rs:27` — the same import

Both are legitimate. `AGENTS.md` states the console boundary as:

> requests must match an exact `Host` authority including the port

Matching a port is not possible without `SocketAddr`/`IpAddr`. The rule's blanket `std::net`
string ban is therefore **unsatisfiable for a correct console** — exactly the situation Plan 345
already resolved for `i2pr-app-proto`, whose rule explicitly permits `std::net` *address values*
while forbidding socket/DNS APIs.

## Objective

Rule 2 enforces the console source-purity boundary across **every** file, and is satisfiable by
correct console code.

## The decision this plan makes

Replace the blanket module-string ban with a **socket-keyed** rule plus an explicit, enumerated
value allow-set — Plan 345's precedent:

- **Forbidden** (never permitted): `TcpListener`, `TcpStream`, `UdpSocket`, `UnixListener`,
  `UnixStream`, `Server::bind`, `axum::serve`, `spawn(`, `std::fs`, `fs::`, `tokio::`.
- **Named, permitted address values**: `std::net::IpAddr`, `std::net::SocketAddr`, and the
  `Ipv4Addr` / `Ipv6Addr` forms — because `AGENTS.md` mandates exact `Host` authority matching
  including the port.

## This is not a weakening — the before/after must be proven

It is tempting to read "dropped `std::net` from the rule" as relaxing a hard boundary. It is the
opposite, and the closure record must demonstrate it with executed evidence:

| | Today | After |
|---|---|---|
| Files scanned | 1 of 13 (partially) | **13 of 13, fully** |
| Socket types actually rejected | **0** (nothing after `theme.rs:1139` is read) | every listed type, in every file |
| `std::net` string ban | asserted, unenforced | replaced by a precise socket-type ban plus a 2-type value allow-set |
| Net effect | **enforces nothing** | enforces the boundary AGENTS.md describes |

The honest statement is: the old rule *claimed* more than it delivered, and its claim was also
false for correct code. The new rule enforces more, on more files, and is satisfiable.

## In scope

1. Per-file reset so every console source file is scanned.
2. Socket-keyed rule + enumerated address-value allow-set, per Plan 345.
3. Confirm the two real imports are permitted for the right reason and no other file regresses.
4. **A negative test per forbidden category, injected into files that are currently unchecked** —
   specifically `security/auth.rs`, `security/mod.rs`, and `routes.rs`, which today are invisible to
   the rule. Probing only `theme.rs` would reproduce the false confidence this plan is fixing.
5. A positive control proving a plain `IpAddr`/`SocketAddr` import is still accepted, so the rule
   cannot be satisfied by simply banning everything.

## Out of scope

- **Changing `crates/i2pr-console/**`.** The console code is correct; the rule was wrong.
- **Extending the same fix to the transport crates.** Plan 362 found `i2pr-transport-ntcp2` and four
  ssu2 files already import `std::net` address values via grouped `use`. That is recorded in Plan
  362's closure record as a finding; fixing it belongs to a transport-scoped plan, not here.
- **The console manifest rule 1.** It works and is unaffected.
- **Plan 365's workflow guard** and every other checker.

## Invariants

- **Every console source file is scanned, with no early exit.** A scan that visits zero files, or
  stops early, is a failure.
- **The allow-set is explicit and enumerated**, never a blanket exemption — the same discipline
  Plan 359 applied to `i2pr-proto` and Plan 361 to the ADR duplicates.
- **Forbidden socket types remain forbidden in every file**, including the ones currently unchecked.
- **`i2pr-console` still depends on no `i2pr-*` crate and names no socket type.** The boundary
  `AGENTS.md` describes is unchanged; only the enforcement is repaired.
- No new dependency.

## Required evidence

- `security/auth.rs`, `security/mod.rs`, `routes.rs`, and `theme.rs` each fail when given a
  `TcpListener` violation, with a per-file trace proving all files are now visited.
- A `use std::net::{IpAddr, SocketAddr};` fixture still passes.
- The two real imports pass, and the reason is recorded.
- The guard passes on the real tree with **no** `crates/` change.
- Mutation-tested: reverting the per-file reset, and widening the allow-set, must each be caught.

## Production changes

`scripts/check-console-boundaries.sh` only. **No `crates/` change.**

## Documentation updates

- `AGENTS.md` — Known checker gaps: remove the grouped-import bullet once Plan 362 closes it, record
  the console rule-2 repair, and note that Plan 365 now guards workflow validity.
- `docs/architecture/tooling.md` — the console rule inventory.

## Acceptance criteria

Plan 366 passes only when:

1. **all** console source files are scanned, proven by per-file trace, not by assertion;
2. the previously-invisible files reject a socket violation;
3. the rule is satisfiable by the console's own correct code, with a positive control;
4. every forbidden category is negative-tested **in files that were previously unchecked**;
5. mutation-tested, with recorded transcripts;
6. no `crates/` file changed;
7. exact-head routine CI is green.

## Stop conditions

Stop and record a classified boundary if the socket-keyed rule cannot be made satisfiable by correct
console code without touching `crates/i2pr-console/**`, or if the per-file reset reveals a genuine
console violation that must be fixed in the crate (which is out of this plan's scope and would need
its own corrective).

## Closure evidence required

Commits; requirement-to-evidence matrix; commands with local/CI outcomes labelled truthfully; the
before/after enforcement table with the per-file trace as proof; the negative-test and mutation-test
transcripts; known limitations; findings by severity; roadmap disposition.

**Note on verification batching.** Executed alongside Plans 360, 361, 362, 364, and 365. Targeted
verification runs per plan; the full routine floor runs once on the combined tree, and the closure
record must say so rather than implying a per-plan floor run.