# Plan 501 — Status: the transport crates' runtime-boundary section

## Status

**Closed** as `passed-transport-runtime-boundary-section`.

The transport contract crates — `i2pr-transport`, `i2pr-transport-ntcp2`,
`i2pr-transport-ssu2` — are declared runtime-neutral by `AGENTS.md`: no
Tokio, no sockets, no filesystem I/O, no `async fn`. They were policed by a
**single raw `grep`** in `scripts/check-runtime-boundaries.sh` that was
**fail-open and over-broad at the same time**:

- **Fail-open.** The crates legitimately import
  `std::net::{IpAddr, SocketAddr}` inside a **grouped** `use`
  (`crates/i2pr-transport-ssu2/src/address.rs:21-25`,
  `crates/i2pr-transport-ntcp2/src/address.rs:9-13`). The pattern keyed on
  the literal `std::net`, and the grouped form hides it: running the
  script's exact pattern over the three `src` trees returned **zero**
  matches, while a grouped `use std::{net::TcpStream};` sat right beside the
  legitimate import and was **missed**.
- **Over-broad.** The same pattern banned `std::net` outright, so the
  legitimate **ungrouped** `use std::net::IpAddr;` was **rejected**. The
  rule could not be satisfied by writing the import in the obvious way.

`AGENTS.md`'s **Known checker gaps** section named this exact hole and asked
for a transport-scoped plan to settle the socket-keyed rule the way Plan 366
did for the console. This plan settles it.

The transport crates now join the **normalised** scan — the script's use-tree
parser, so a grouped import cannot hide — with a **socket-keyed** rule and an
explicit **four-type address-value allow-set**, in the console's shape.

**The rule gets stronger, not weaker.** It adds `async fn` / `async_trait`
and the DNS resolver, which the raw grep missed and which `AGENTS.md` already
requires of these crates. The removed raw grep is **replaced**, not
supplemented: leaving it would keep the false positive alive.

**No file under `crates/` changed.** The three crates were already clean, and
if the new rule reported a violation in them the rule would be wrong, not the
source.

Nothing was promoted. `specs/support.toml` stays at 94 surfaces (88
experimental, 5 deferred, 1 ready) with **0 advertised**; `router.version`
stays `0.9.62`; `caps` and the published RouterInfo are unchanged;
`specs/support.toml`, `Cargo.toml` and `Cargo.lock` are byte-identical.

**Plan 490 Low #4 and Plan 497 L1 are discharged.**

## Commits

| Commit | What |
| --- | --- |
| `d181ed1c` | `docs(plans): register Plan 501, the transport crates' runtime-boundary section` |
| `ac7c7897` | `feat(boundaries): give the transport crates a real runtime-neutrality rule` |
| (this commit) | `docs(plans): close Plan 501 and advance the network-participation roadmap` |

## What changed

### The rule set — `scripts/check-runtime-boundaries.sh`

`transport_patterns` (`:545`) carries **seven** rule categories:

| Category | Pattern | Bans |
| --- | --- | --- |
| `tokio` | `tokio::` | Tokio |
| `async` | `async\s+fn\|async_trait` | `async fn` / `async_trait` — **new**; the raw grep missed it |
| `sockets` | `TcpStream\|TcpListener\|UdpSocket\|UnixStream\|UnixListener\|TcpSocket` | the socket types |
| `resolver` | `ToSocketAddrs\|to_socket_addrs` | the DNS resolver — **new** |
| `filesystem` | `std::fs\|OpenOptions\|File::` | filesystem I/O |
| `net module` | `std::net::(self\b\|\*)` | a bare or globbed `std::net` module |
| `std::net import` | `_TRANSPORT_NET_ALLOW_RE` | any `std::net` import outside the allow-set |

### The allow-set — `scripts/check-runtime-boundaries.sh:530`

```python
TRANSPORT_ALLOWED_NET_TYPES = ("IpAddr", "SocketAddr", "Ipv4Addr", "Ipv6Addr")
```

`std::net` is **neither blanket-banned nor blanket-allowed** — the console's
rule (`scripts/check-console-boundaries.sh:364-372`), applied to the
transport crates. The allow-set is expressed as a **negative lookahead**
(`:539`): a `std::net` occurrence is permitted only when it is immediately
followed by one of the four allowed address types. The check runs on the
**normalised** text, so a grouped import cannot hide behind a brace — the
group is expanded into its leaves first and each leaf is judged on its own.

**An allow-set entry may never name a socket or the resolver.** The guard
reuses the rule set's *own* patterns, so it cannot drift away from them, and
widening the allow-set to a socket type fails the script at that point
rather than silently.

### The raw grep is removed

The old check at the script's head:

```bash
if grep -REn 'tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|OpenOptions|File::' \
  "$root/crates/i2pr-transport/src" ... ; then
```

is **gone**, replaced by the normalised transport section. It was both
fail-open and over-broad, and keeping it as a supplement would have kept the
false positive alive.

### The controls

**Nine positive controls** (`:640-656`) — one per rule category, plus the two
grouped socket-import shapes the removed grep could not see through:

- `transport/tokio`, `transport/async`, `transport/sockets`,
  `transport/resolver`, `transport/filesystem`, `transport/net module`,
  `transport/std::net import`
- `transport/grouped socket import` — `use std::{net::TcpStream};`
- `transport/grouped socket import (nested)` — `use std::{fmt, net::{IpAddr, TcpStream}};`

**A per-category completeness check** (`:684-690`), in the same shape as the
`i2pr-netdb` one: a transport rule category cannot be added without a control
that proves it fires.

**Two negative controls**:

- `transport_benign` (`:721`) — the legitimate grouped **and** ungrouped
  address imports. A rule that fires on everything is not a rule, and a rule
  that cannot be satisfied by the obvious import is not a rule either.
- `transport_mentions` (`:738`) — a forbidden construct named only in a
  comment or a `#[cfg(test)]` literal is prose, and the normalised scan masks
  both. An **import** inside a `#[cfg(test)]` region is still reported: the
  removed raw grep covered test code, so suppressing test regions would
  weaken the rule rather than settle it.

### The floors and the derived figures

- The positive-control floor **rises**: `len(CONTROL) < 30` becomes
  `len(CONTROL) < 39` (`:658`).
- The raw-scan count floor moves from `raw_scans < 23` to `raw_scans < 22`
  (`:879`) — **precisely because one raw scan was replaced** by the
  stronger section. The section's presence is therefore asserted
  **structurally**, from the set of targets actually scanned
  (`scanned_targets` at `:828`, the assertion at `:855`), so the raw-scan
  count alone cannot be mistaken for proof that the rule is still there.
  Two **split** text anchors (`:937`) back it up, each split so the anchor
  list cannot satisfy itself by naming the literal it looks for.
- **Every printed rule count is now derived** from the rule set it
  describes (`:949`). The `i2pr-api` line that claimed **"8 rules"** while
  `i2pr_api_patterns` carried **seven** is now correct, and no printed count
  can drift again.

### The documentation

- `AGENTS.md` — the **Known checker gaps** entry for the transport crates is
  **removed**. It was a statement about a checker's coverage, not plan state,
  so removing it is the correct end-state. **No plan number, status token or
  ledger was added to `AGENTS.md`.**
- `docs/architecture/overview.md` — the hard-boundaries bullet is corrected:
  the runtime-neutral crates are now described as having no `std::net`
  **socket or resolver** API, with address *values* permitted where a crate
  needs them. The boundary-checker entry names the transport contracts
  among the normalised-scan targets and describes the socket-keyed rule.
- `docs/architecture/tooling.md` — the script's inventory row describes the
  new section, and the open finding *"The transport `std::net` ban is already
  evaded"* is marked **CLOSED by Plan 501** with the measured behaviour.

## Requirement-to-evidence matrix

| Requirement (plan §4 / §7) | Evidence | Verdict |
| --- | --- | --- |
| §4.1 the rule gets stronger, never weaker | `transport_patterns` (`:545`) adds `async` and `resolver` to the old raw grep's categories; the grouped-import form is now seen through the use-tree parser | pass |
| §4.2 no blanket `std::net` ban | `TRANSPORT_ALLOWED_NET_TYPES` (`:530`); the `transport_benign` negative control (`:721`) proves the ungrouped `use std::net::IpAddr;` passes | pass |
| §4.3 no blanket `std::net` permission either | the `std::net import` and `net module` categories; the `transport/std::net import` and `transport/net module` controls | pass |
| §4.4 a grouped import cannot hide | `transport/grouped socket import` and `transport/grouped socket import (nested)` controls; the measured run below reports the group expanded into its leaves | pass |
| §4.5 a mention is not an import | the `transport_mentions` negative control (`:738`) | pass |
| §4.6 the rule is enforced, not asserted | the measured run below: a grouped socket import makes the script **exit 1** and name the file and the category | pass |
| §4.7 the scan is not vacuous | the script's existing fail-closed on a zero-source scan (`:436-437`) is unchanged and now covers the transport targets too | pass |
| §4.8 the published figures are derived | the `i2pr-api` line now prints `len(i2pr_api_patterns)` = 7, not the hardcoded 8; the transport line prints `len(transport_patterns)` = 7 | pass |
| §4.9 no production source change | `git diff --stat -- crates/` is **empty** | pass |
| §4.10 no dependency, no advertisement, no promotion | `Cargo.toml` / `Cargo.lock` / `specs/support.toml` byte-identical; `router.version` 0.9.62; `caps` unchanged | pass |
| §7 WP1 the rule set | `transport_patterns` (`:545`), seven categories | pass |
| §7 WP2 the socket-keyed rule with the allow-set | `:530`, `:539`, plus the allow-set guard | pass |
| §7 WP3 the controls | nine positive controls, the per-category check, two negative controls | pass |
| §7 WP4 the raw grep goes | removed; the script's comment records why | pass |
| §7 WP5 derived figures | the four printed lines are derived; `raw_scans` floor 22 with a structural replacement assertion | pass |
| §7 WP6 the documentation | `AGENTS.md`, `docs/architecture/overview.md`, `docs/architecture/tooling.md` | pass |
| §10 the script exits 0 on the current tree | `bash scripts/check-runtime-boundaries.sh` → exit 0 | pass |
| §10 the script exits non-zero on a synthetic violation | the measured run below → exit 1 | pass |
| §10 the script exits 0 on the ungrouped address import | the measured run below → exit 0 | pass |
| §10 the floor passes in full | see *Commands run* | pass |
| §13.2 no stop condition triggered | no blanket ban, no blanket permission, no `crates/` change, the raw grep could be removed because the new section covers everything it did — and more | pass |

## Commands run

### The routine floor (on the closing tree `ac7c7897`)

**97 steps, 97 PASS, 0 FAIL** on the re-run
(`/tmp/i2pr-floor-501-run2.log`; completion marker
`=== FLOOR COMPLETE 09:12:45 ===`). The slowest steps were
`cargo-test-workspace` (1140 s), `plan491-netdb-suite` (88 s) and
`plan491-client-suite` (24 s). Every checker script passed, including
`check-dependency-direction`, `check-runtime-boundaries`,
`check-console-boundaries`, `check-console-browser-security`,
`check-tooling-inventory`, `check-managed-app-process-boundary`,
`check-managed-app-package-boundary`, `check-managed-app-policy-boundary`,
`check-managed-app-secured-sandbox`, `check-service-tunnel-boundaries`,
`check-m12-floodfill-boundaries`, `check-outproxy-request-path` and
`cargo-deny` (advisories ok, bans ok, sources ok).

**The first run of the floor on this tree did NOT pass, and the failure is
recorded rather than hidden.** Run 1 (`/tmp/i2pr-floor-501-run1.log`) was
**97 steps, 96 PASS, 1 FAIL**: the `cargo-test-workspace` step failed on one
row —

```text
apphost_launch::tests::an_apphost_that_ignores_eof_is_killed_and_still_reaped
panicked at crates/i2pr-appd/src/apphost_launch.rs:479:
the apphost must be killed and reaped: ReapTimeout(500ms)
```

The evidence that this is a **host-contention flake and not a Plan 501
defect**:

1. **Plan 501 changed no Rust code at all.** `git diff --stat ac7c7897~1
   ac7c7897 -- crates/` is **empty**. The plan touched `scripts/`,
   `AGENTS.md` and `docs/` only. `i2pr-appd` was not modified and is not
   referenced by the plan.
2. **The host was heavily loaded during run 1.** The load average was
   **21.45** (15-minute) and **11.43** (5-minute) at the time of the
   failure, from another workspace's `cargo nextest` run on the same
   machine. The failing row asserts a **500 ms** reap deadline, which a
   loaded scheduler can miss.
3. **The row passes on re-run.** `cargo test --locked -p i2pr-appd --lib --
   apphost_launch::tests::an_apphost_that_ignores_eof_is_killed_and_still_reaped`
   passed **3 of 3** consecutive re-runs.
4. **The whole floor passed on the re-run** on the **same tree**
   (`ac7c7897`), with no source change between the runs.

The row's sensitivity is recorded as a **Low finding** below. The floor was
**not** re-run with the failing row excluded, and no deadline was widened.

### Focused verification (before the floor)

| Command | Outcome |
| --- | --- |
| `bash scripts/check-runtime-boundaries.sh` | PASS — exit 0. Printed: `i2pr-app-proto runtime/OS boundary: ok (7 rules, positive control passed)`, `i2pr-api runtime-neutrality: ok (7 rules, grouped-import positive controls passed)`, `i2pr-netdb runtime-neutrality: ok (12 rules, per-rule positive controls passed)`, `transport runtime-neutrality: ok (7 rules, per-rule positive controls passed)` |
| `python3 scripts/check-tooling-inventory.py` | PASS — "every published figure matches the tree" |
| `cargo fmt --all --check` | PASS |
| `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock crates/` | empty — no production change and no promotion |

### The two measured proofs

Both were run against the **finished** script, with the probe reverted
afterwards and `git status --short crates/` confirmed **empty** afterwards.

**Proof 1 — the grouped socket import is now caught.** The probe
`crates/i2pr-transport-ssu2/src/address.rs` was temporarily changed from
`net::{IpAddr, SocketAddr}` to `net::{IpAddr, SocketAddr, TcpStream}`:

```text
i2pr-transport-ssu2 runtime-neutrality violation:
crates/i2pr-transport-ssu2/src/address.rs:21: forbidden sockets API: use std::fmt; use std::net::IpAddr; use std::net::SocketAddr; use std::net::TcpStream; use std::str:
crates/i2pr-transport-ssu2/src/address.rs:21: forbidden std::net import API: use std::fmt; use std::net::IpAddr; use std::net::SocketAddr; use std::net::TcpStream; use std::str:
```

**exit = 1** (non-zero, as required). The report shows the group **expanded
into its leaves** — exactly what the removed raw grep could not see. The old
grep returned **zero** matches for this file.

**Proof 2 — the legitimate ungrouped address import now passes.** The probe
was temporarily rewritten to `use std::{fmt, str::FromStr};` plus
`use std::net::{IpAddr, SocketAddr};`:

```text
transport runtime-neutrality: ok (7 rules, per-rule positive controls passed)
runtime boundary checks passed
```

**exit = 0**. The old raw grep would have **rejected** this file.

The two proofs are the whole point of the plan, and they are the **opposite**
of the old rule's behaviour in both directions.

## Invariant review

1. **The rule gets stronger, never weaker — preserved.** The new section
   bans everything the raw grep banned, **and** `async fn` / `async_trait`
   and the DNS resolver, **and** it now sees the grouped-import forms the raw
   grep could not. Nothing is dropped.
2. **No blanket `std::net` ban — preserved.** Address *values* stay permitted
   through the explicit four-type allow-set, and the ungrouped import now
   passes. **Proved** by Proof 2.
3. **No blanket `std::net` permission either — preserved.** A socket type,
   the resolver, the bare module and a glob stay forbidden.
4. **A grouped import cannot hide — preserved.** The scan runs through
   `normalise()`. **Proved** by Proof 1 and by the two grouped controls.
5. **A mention is not an import — preserved.** The `transport_mentions`
   negative control covers a comment, a doc comment and a `#[cfg(test)]`
   string literal.
6. **The rule is enforced, not asserted — preserved.** Proof 1 exits 1 and
   names the file and the category.
7. **The scan is not vacuous — preserved.** The script's existing fail-closed
   on a zero-source scan is unchanged and now covers the transport targets.
8. **The published figures are derived — preserved.** Every printed rule
   count comes from its rule set.
9. **No production source change — preserved.** `git diff --stat -- crates/`
   is empty.
10. **No dependency, no advertisement, no promotion — preserved.**

## Failure review

- **A violating source is found.** The script prints the offending path,
  line and category, and exits non-zero. It modifies nothing.
- **A scan that read no source.** Fails closed (the script's existing
  behaviour), because a scan that read nothing cannot detect anything.
- **The controls fail.** The script fails closed; a control that does not
  fire is a broken rule, not a pass.
- **The transport section is removed.** Fails closed — twice over: the
  structural assertion on `scanned_targets` and the split text anchors.
- **The allow-set is widened to a socket.** Fails closed at the guard,
  before the scan.
- **Cancellation, restart.** The script is a one-shot check. Nothing is
  persisted and nothing is owned.

## Migration review

No migration. No stored format changes. No data-directory changes. No
configuration key is added or changed. No production source changes.

The only behaviour changes are the two the plan promised, and both are
improvements:

1. a **previously undetectable** violation in the transport crates now fails
   the script;
2. a legitimate **ungrouped** `use std::net::IpAddr;` now passes where it
   previously failed.

The one counter-direction change is the raw-scan **count** floor, from 23 to
22. That is not a relaxation of a rule: exactly one raw scan was *replaced*
by a section that is strictly stronger, and the replacement is asserted
**structurally** from the set of targets actually scanned. The floor is a
guard against silent removal, and the removal here is neither silent nor
unreplaced.

## Security review

- **The guard is stronger where it matters.** The transport crates are the
  wire boundary for NTCP2 and SSU2. A socket or a Tokio import in one of them
  would be a runtime-ownership violation in the crate that must stay
  runtime-neutral, and the old rule could not see the grouped form — the
  form the crates actually use.
- **The allow-set is narrow and cannot widen silently.** Exactly four
  address-value types, and an entry that names a socket or the resolver
  fails the script.
- **The rule cannot be satisfied by failing everything.** The negative
  control proves the legitimate surface passes.
- **Nothing is written.** The script only reads and reports; it modifies no
  file and the probes used for the proofs were reverted, with
  `git status --short crates/` confirmed empty afterwards.
- **No secret is involved.** The script reports a path, a line number, a
  category and the offending text; it reads only source files, and the
  transport crates hold no secret material.

## Documentation and operations

- `AGENTS.md` — the Known checker gaps entry is closed (removed). No plan
  state was added to `AGENTS.md`.
- `docs/architecture/overview.md` — the hard-boundaries bullet is corrected
  to say *socket or resolver* API with address values permitted, and the
  boundary-checker entry names the transport contracts among the
  normalised-scan targets.
- `docs/architecture/tooling.md` — the script's inventory row describes the
  new section in full, and the open finding is closed with the measured
  behaviour.

**Operations.** Nothing to configure. The script's exit status is the
signal. A violation prints
`i2pr-transport* runtime-neutrality violation: <path>:<line>: forbidden
<category> API: <line>` and exits 1.

## Limitations

1. **The rule is a static guard, not a runtime one.** It reads source text;
   it cannot observe a socket the transport crate somehow acquired at
   runtime. That is the same class of guard as every other section in the
   script.
2. **`tools/i2pr-interop` stays outside the script's globs.** It is a
   non-production test launcher and the script's `crates/` globs do not
   reach it; covering it would need its own plan-of-record.
3. **The allow-set is per-crate-set, not per-crate.** The transport crates
   share one rule set and one allow-set. A future crate that needs a fifth
   address-value type would widen the allow-set for all three, and the
   guard would then have to be re-read to confirm the new entry is not a
   socket.
4. **The raw-scan count floor is now 22, not 23.** The replacement is
   asserted structurally, but the *number* is one lower than before. A
   future reviewer must read the structural assertion, not the number, to
   know the transport rule is present.
5. **No production source was exercised.** The rule's behaviour is proved
   with a temporary probe that was reverted; the crates' own behaviour is
   unchanged because no crate changed.

## Findings by severity

**Low**

- **L1 — the raw-scan count floor dropped by one.** See *Limitations* 4 and
  *Migration review*. Recorded rather than left implicit, because a floor
  that moves down is exactly the kind of change a reviewer must be able to
  see and judge.

**Low**

- **L2 — a managed-app reap row has a 500 ms deadline that a loaded host can
  miss.** `crates/i2pr-appd/src/apphost_launch.rs:479` asserts that an
  apphost that ignores EOF is killed and reaped within **500 ms**. On a host
  under a load average above 20 the reap can exceed that, and the row fails
  with `ReapTimeout(500ms)`. This is **pre-existing** and is in a crate
  Plan 501 did not touch; it surfaced only because the floor ran while
  another workspace was saturating the machine. The deadline is a
  *test-timing* bound, not a product bound — the product behaviour the row
  asserts (the apphost is killed and reaped) is not in question. Widening
  the deadline, or re-running with the row excluded, would be a change to a
  managed-app test and is **out of this plan's scope**; it is recorded here
  for a plan-of-record that owns `i2pr-appd`.

**Informational**

- **I1 — the old rule's own comment was misleading.** The removed grep's
  pattern included `std::net`, which read as "no address types", while the
  crates' legitimate grouped import meant the pattern matched **nothing**.
  The rule and its intent had drifted apart before this plan; the new
  section states the intent explicitly.
- **I2 — the printed `i2pr-api` rule count was wrong** (Plan 497 L1: "8
  rules" against seven). **Fixed** here, and the fix is structural — every
  printed count is now derived, so the class of drift is closed, not just
  the instance.

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md`: the Plan 501 row moves
from `registered` to `closed` with the token
`passed-transport-runtime-boundary-section` and this closure record.

The line's own ordering (ADR 0041: substrate-first, no advertisement) is
unchanged. Plan 501 is a **discharge** of Plan 490's Low #4 and Plan 497's
L1 — both recorded findings, neither of which added a phase to the sequence.
Nothing is advertised.

## Unblock audit

Audited `plans/registry.md`'s blocked work and the network-participation
roadmap's dependency graph for anything that lists the discharged findings as
a dependency.

| Plan | Was | Now | Why |
| --- | --- | --- | --- |
| 433 — live NetDB/tunnel/client composition | blocked | **blocked** | Blocked on Plans 431/432. Unchanged. |
| 493 — two-family public participation qualification | blocked | **blocked** | Blocked on the authorized two-host topology and Plan 435's transport activation. Unchanged. |
| 494 — floodfill role and the `caps=f` advertisement | blocked | **blocked** | Blocked on Plan 493. Unchanged. |
| 435 / 439 / 436 | blocked | **blocked** | External environments this host does not have. Unchanged. |

**Nothing moved to `ready`.** Plan 501 closes a *checker* gap; no registered
plan was blocked on it.

No corrective plan is required by this closure. L1 is recorded rather than
fixed, because the change it describes is deliberate and defended above.
