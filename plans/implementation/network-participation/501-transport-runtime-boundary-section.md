# Plan 501 — The transport crates' runtime-boundary section

| Field | Value |
| --- | --- |
| Plan | 501 |
| Subsystem | `network-participation` |
| State | `registered` (`registered-transport-runtime-boundary-section`) |
| Depends on | Plan 362 (`passed-i2pr-api-runtime-boundary-section`), Plan 366 (`passed-console-boundary-section`), Plan 497 (the `i2pr-netdb` section) |
| Discharges | Plan 490 Low #4 — no `i2pr-transport-ssu2` section in `scripts/check-runtime-boundaries.sh`, and the raw transport greps are fail-open for a grouped `use`; Plan 497 L1 — the `i2pr-api` section prints "8 rules" while carrying seven |
| Closure record | `plans/closure/network-participation/501-status.md` |

## 1. Objective

Give the transport contract crates a **real** runtime-boundary rule.

The transport crates — `i2pr-transport`, `i2pr-transport-ntcp2`,
`i2pr-transport-ssu2` — are declared runtime-neutral: no Tokio, no sockets, no
filesystem I/O, no `async fn` (`AGENTS.md`'s workspace boundaries; `docs/architecture/overview.md`).
They are policed by a **single raw `grep`** in
`scripts/check-runtime-boundaries.sh`, and that grep is **fail-open** and
**over-broad at the same time**:

- **Fail-open.** The pattern is
  `tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|OpenOptions|File::`,
  and the crates legitimately import `std::net::{IpAddr, SocketAddr}` inside a
  **grouped** `use` (`crates/i2pr-transport-ssu2/src/address.rs:21`,
  `crates/i2pr-transport-ntcp2/src/address.rs:9`). A grouped import therefore
  **hides from the grep entirely** — the whole `use std::{…}` block is one
  line, and the grep keys on the literal `std::net`. Running the script's
  exact pattern over the three transport `src` trees returns **zero**
  matches, while a grouped `use std::{net::TcpStream};` would sit right
  beside the legitimate address import and be **missed**.
- **Over-broad.** The same pattern bans `std::net` outright, so a
  *non-grouped* `use std::net::IpAddr;` — which the crates need — would
  **trip** the rule. The rule cannot be satisfied by writing the import in
  the obvious way.

`AGENTS.md`'s **Known checker gaps** section names this exact hole:

> *Open: the transport crates already import `std::net` address values via
> grouped `use` … A transport-scoped plan should settle the socket-keyed rule
> there the way Plan 366 did for the console.*

This plan settles it. The transport crates get a **normalised** section with
a **socket-keyed** rule and an explicit **address-value allow-set**, exactly
as Plan 366 settled the console.

The rule gets **stronger**, not weaker: it also bans `async fn` and
`async_trait`, which the current grep does not — and which `AGENTS.md`
already requires of these crates.

## 2. Why ready

- The script already has the machinery. Plan 362 added `normalise()` — a
  use-tree parser that flattens `use std::{net::{IpAddr, SocketAddr}};` into
  its leaves before the scan, so a grouped import cannot hide. It is already
  used for `i2pr-app-proto`, `i2pr-api`, `i2pr-netdb`, and the console has
  its own section with the socket-keyed rule and the address-value allow-set.
- Plan 366 is the named precedent: it replaced the console's blanket
  `std::net` string ban with a **socket-keyed** ban plus an explicit
  four-type address-value allow-set, because `AGENTS.md`'s exact-`Host`
  rule requires `IpAddr`/`SocketAddr`.
- The transport crates are **clean today**: no `async fn`, no `async_trait`,
  no socket type, no `std::fs`, no `tokio::` in any of the three `src` trees.
  A socket-keyed rule with an address-value allow-set therefore passes on the
  current tree without a single source change.
- The console's section already carries the **positive and negative
  controls** this plan needs: a synthetic violating source must be reported,
  and a source that only *mentions* a forbidden construct in a comment or
  inside a `#[cfg(test)]` block must not be.

## 3. Current implementation evidence

### 3.1 The raw transport grep

`scripts/check-runtime-boundaries.sh:44-48`:

```bash
if grep -REn 'tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|OpenOptions|File::' \
  "$root/crates/i2pr-transport/src" "$root/crates/i2pr-transport-ntcp2/src" "$root/crates/i2pr-transport-ssu2/src" >/dev/null; then
  echo "transport contract crates must not own Tokio, sockets, or filesystem I/O" >&2
  exit 1
fi
```

Running that exact pattern over the three trees returns **0 matches**. The
grouped import that hides it:

```rust
// crates/i2pr-transport-ssu2/src/address.rs:21-25
use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    str::FromStr,
};
```

### 3.2 The normalised scan excludes the transport crates

The script's normalised scan (`normalise()` at `:421-422`, `scan()` at
`:425-438`) covers `i2pr-app-proto`, `i2pr-api` and `i2pr-netdb`, plus the
console's own section. The transport crates are **not** in it — the raw grep
was their only cover.

### 3.3 The console's precedent

`scripts/check-console-boundaries.sh:364-372`:

> `std::net` itself is NOT blanket-banned and NOT blanket-allowed.

```python
ALLOWED_NET_TYPES = ("IpAddr", "SocketAddr", "Ipv4Addr", "Ipv6Addr")
```

and its positive controls include the exact shape that defeats the current
transport grep (`:633`):

```python
"address value in a grouped import": "use std::{fmt, net::{IpAddr, SocketAddr}};\n",
```

### 3.4 A published figure is wrong

`scripts/check-runtime-boundaries.sh:762` prints a **hardcoded**:

```
"i2pr-api runtime-neutrality: ok (8 rules, grouped-import positive controls passed)\n"
```

while `i2pr_api_patterns` (`:462-470`) carries **seven** entries. The
printed figure must be **derived** from the rule set, not hardcoded, so it
cannot drift again. (Plan 497 L1.)

## 4. Invariants that must not regress

1. **The rule gets stronger, never weaker.** The new section must ban
   everything the raw grep banned — `tokio::`, the filesystem constructs and
   the socket types — **and** `async fn` / `async_trait`, which the raw grep
   missed. Nothing is dropped.
2. **No blanket `std::net` ban.** Address *values* stay permitted through an
   explicit allow-set of `IpAddr`, `SocketAddr`, `Ipv4Addr`, `Ipv6Addr`, so
   the crates can import them in the obvious, ungrouped way.
3. **No blanket `std::net` permission either.** A `std::net` import of
   anything outside the allow-set — a socket type, `ToSocketAddrs`, a bare
   `use std::net;`, a glob — is reported.
4. **A grouped import cannot hide.** The scan runs through the script's
   `normalise()` use-tree parser, so `use std::{net::TcpStream};` is
   expanded and reported.
5. **A mention is not an import.** A forbidden construct named in a comment
   or inside a `#[cfg(test)]` block must not be reported.
6. **The rule is enforced, not asserted.** The section must **fail closed**
   on a violating source, and the script must exit non-zero.
7. **The scan is not vacuous.** A section that read no source must fail
   closed rather than reporting a pass.
8. **The script's published figures are derived, not hardcoded.** The rule
   count and the control count come from the rule sets themselves.
9. **No production source change is needed** — the transport crates are
   clean today, and none may be changed to make the rule pass.
10. **No dependency, no advertisement, no promotion.** `Cargo.toml` and
    `Cargo.lock` stay byte-identical; `specs/support.toml` stays at 94
    surfaces with 0 advertised; `router.version` stays `0.9.62`.

## 5. Scope

### 5.1 In scope

- **WP1 — a `transport_patterns` rule set** in the script's normalised
  scan, covering `i2pr-transport`, `i2pr-transport-ntcp2` and
  `i2pr-transport-ssu2`.
- **WP2 — the socket-keyed rule with the address-value allow-set**, applied
  to `std::net` exactly as the console applies it.
- **WP3 — positive and negative controls** for the new section, including
  the grouped-import control that the raw grep cannot catch.
- **WP4 — replacing the raw transport grep** with the new section. The raw
  grep must be **removed**, not merely supplemented: it is both fail-open
  and over-broad, and leaving it would keep the false positive alive.
- **WP5 — deriving the published figures.** The `i2pr-api` line's "8 rules"
  becomes the rule set's real length; the control-count floor rises to
  cover the new controls.
- **WP6 — the documentation.**

### 5.2 Explicitly out of scope

- **Any production source change.** The transport crates are clean. If the
  new rule were to report a violation in them, the rule is wrong, not the
  source.
- **The `i2pr-transport-ntcp2` / `i2pr-transport-ssu2` crates' own
  boundaries.** This plan covers the *transport contract* crates named in
  the raw grep.
- **The `tools/i2pr-interop` crate.** It is outside
  `scripts/check-runtime-boundaries.sh`'s `crates/` globs and is a
  non-production test launcher; a separate plan-of-record would be needed.
- **Weakening or removing any existing rule.** The new section is added; the
  existing `i2pr-api`, `i2pr-netdb`, app-proto, i2pcontrol, service-tunnels
  and console sections are untouched.
- **Any capability promotion, advertisement or version change.**

## 6. Production changes

| File | Change |
| --- | --- |
| `scripts/check-runtime-boundaries.sh` | A `transport_patterns` rule set; the transport crates added to the normalised scan; the socket-keyed `std::net` rule with the address-value allow-set; the new section's positive and negative controls; the raw transport grep **removed**; the published rule and control counts **derived**. |

**No file under `crates/` changes.** That is the point: the rule becomes
enforceable against the transport crates without touching them.

## 7. Work packages

### WP1 — the rule set

A `transport_patterns` dictionary beside `i2pr_api_patterns` and
`netdb_patterns`, covering at least:

- `tokio` — `tokio::`
- `async` — `async\s+fn|async_trait`
- `sockets` — the socket types
- `filesystem` — `std::fs|OpenOptions|File::`
- `std::net` — handled by the socket-keyed rule below, not by a blanket ban

### WP2 — the socket-keyed `std::net` rule

The console's shape, applied to the transport crates: `std::net` is neither
blanket-banned nor blanket-allowed. An import of `IpAddr`, `SocketAddr`,
`Ipv4Addr` or `Ipv6Addr` is permitted; anything else reached through
`std::net` is reported. The allow-set must be **explicit and narrow** — a
new type in the allow-set is a change to the rule, not a convenience.

### WP3 — the controls

- A **positive control**: a synthetic source with a grouped socket import
  (`use std::{net::TcpStream};` and `use std::{fmt, net::{IpAddr, TcpStream}};`)
  must be **reported**.
- A **negative control**: a source that only mentions a forbidden construct
  in a comment or inside a `#[cfg(test)]` block must **not** be reported.
- An **allow-set control**: the legitimate grouped address import
  (`use std::{fmt, net::{IpAddr, SocketAddr}};`) must **not** be reported —
  so the rule cannot be satisfied only by failing everything.

### WP4 — the raw grep goes

The raw transport grep is **removed** and replaced by the new section. If it
were kept, the ungrouped `use std::net::IpAddr;` would still be a false
positive, and the section's value would be halved.

### WP5 — derived figures

- The `i2pr-api` printed rule count comes from `len(i2pr_api_patterns)`.
- The control-count floor rises to cover the new controls.
- Every printed count in the script is derived, so none can drift again.

### WP6 — the documentation

- `docs/architecture/overview.md` and `docs/architecture/tooling.md` — the
  script's coverage.
- `AGENTS.md` — the **Known checker gaps** entry for the transport crates is
  closed. (AGENTS.md is a workspace instruction file, not a plan ledger; the
  entry is a statement about a checker, so removing it is the correct
  end-state.)

## 8. Failure, cancellation, restart, and contention semantics

- **A violating source is found.** The script prints the offending path,
  line and category, and exits non-zero. It does **not** modify anything.
- **A scan that read no source.** Fails closed (the script's existing
  behaviour at `:436-437`), because a scan that read nothing cannot detect
  anything.
- **The controls fail.** The script fails closed; a control that does not
  fire is a broken rule, not a pass.
- **Cancellation, restart.** The script is a one-shot check. Nothing is
  persisted and nothing is owned.

## 9. Compatibility and migration

No migration. No stored format changes. No configuration key is added or
changed. No production source changes. The only behaviour change is that a
**previously undetectable** violation in the transport crates now fails the
script, and that a legitimate **ungrouped** `use std::net::IpAddr;` now
passes where it previously failed.

## 10. Required tests

The section's own controls are the tests. In addition:

- The script exits **0** on the current tree.
- The script exits **non-zero** when a synthetic violating source is
  introduced into a transport crate, and the report names the file and the
  category.
- The script exits **0** when the legitimate address import is written
  ungrouped, proving the allow-set works.
- The script exits **0** when a forbidden construct appears only in a
  comment or a `#[cfg(test)]` block.
- The routine floor in `AGENTS.md` passes in full, including
  `bash scripts/check-runtime-boundaries.sh`.

## 11. Exact verification commands

```text
cargo fmt --all --check
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-tooling-inventory.py
```

and the full routine floor from `AGENTS.md`.

## 12. Documentation updates

- `docs/architecture/overview.md` — the boundary checkers and what they now
  cover.
- `docs/architecture/tooling.md` — the script's coverage and its controls.
- `AGENTS.md` — the Known checker gaps entry is closed.
- `specs/CONFORMANCE.md` — this is a **checker** change, not a protocol
  capability. Nothing is promoted and the support inventory is unchanged;
  the CONFORMANCE change is the forward-pointer from the Plan 499/500
  sections, if the file needs one.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

1. A grouped socket import in a transport crate is **reported** and the
   script exits non-zero.
2. An ungrouped socket import is reported.
3. A `tokio::` import is reported.
4. An `async fn` is reported.
5. A filesystem import is reported.
6. The legitimate address-value imports — grouped **and** ungrouped — are
   **not** reported.
7. A forbidden construct named only in a comment or a `#[cfg(test)]` block
   is **not** reported.
8. The raw transport grep is gone, and the script's printed counts are
   derived.
9. The script exits 0 on the current tree, and the floor passes in full.
10. `specs/support.toml`, `Cargo.toml` and `Cargo.lock` are byte-identical,
    and no file under `crates/` changed.

### 13.2 Stop conditions

Stop and record a finding, rather than working around it, if:

- the socket-keyed rule could only be satisfied by banning `std::net`
  outright again, or by allowing the whole module;
- a compliant transport source would be reported — the rule is then wrong;
- enforcing the rule would require changing a file under `crates/`;
- the raw grep could not be removed without losing a rule it was the only
  cover for — then the new section must cover that rule first, and the
  removal happens only once it does;
- closing the finding would require a capability promotion, an
  advertisement, or a version change.

## 14. Closure evidence required

The closure record must carry: the commits; a requirement-to-evidence
matrix naming each control; the exact commands and their outcomes; an
invariant review against §4, row by row; a failure, migration and security
review; the limitations — including that `tools/i2pr-interop` stays outside
the script's globs and that the rule is a **static** guard, not a runtime
one; findings by severity; the roadmap disposition; and the unblock audit.

## 15. Handoff notes

- `AGENTS.md`'s Known checker gaps entry is closed by this plan. It is a
  statement about a checker's coverage, not a plan ledger, so removing it is
  the correct end-state. **No plan numbers or status tokens are added to
  `AGENTS.md`.**
- Plan 490's closure record is **not** rewritten; the disposition of its
  Low #4 finding is recorded here and in the registry rows.
- Plan 497's closure record is **not** rewritten either; its L1 finding (the
  hardcoded "8 rules") is discharged here.
