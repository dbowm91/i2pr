# Plan 378 — Final Proposal 170 conformance gate: status

Status: **blocked-on-plan-377-sections-1-2-and-5-executed-and-green**

Plan of record:
[`378-final-prop170-conformance-gate.md`](../../implementation/i2pcontrol-proposal-170/378-final-prop170-conformance-gate.md).

Classification: final conformance / support-claim gate.

This plan **did not pass**, and it may not set `full-proposal-conformant`. What
it *could* do independently, it did: the re-freeze executed clean and every
local gate is green. It is blocked on exactly one dependency.

## 1. Re-freeze of the Open Proposal — **MET**

Executed live at execution time, exactly as the plan requires, rather than
inherited from Plan 348's record.

| Item | Value |
|---|---|
| Document | I2P Proposal 170, "I2PControl Expansion" |
| Status | Open |
| Source form | `https://i2p.net/proposals/170-i2pcontrol-expansion.txt`, read-only |
| Retrieved | 2026-10-07 |
| Current length | 19 010 bytes |
| **Current SHA-256** | `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc` |
| Plan-334 pinned SHA-256 | `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc` |
| Plan-348 re-freeze SHA-256 | `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc` |

**Byte-identical across all three.** The same length and the same digest means
the document Plan 320's canonical wire freeze and Plan 334's re-freeze were
derived from has not changed by a single byte. So:

- field-by-field comparison to the Plan-320 canonical wire freeze — **no drift
  possible**;
- the TunnelManager type/option inventory, RouterInfo selectors, AddressBook
  keys, and return shapes — **unchanged by construction**;
- "if the Proposal changed materially, stop and register a contract
  reconciliation plan" — **not triggered**.

**This is not a conformance result.** It removes one possible reason Plan 378
could not pass. It is not one of the reasons it does.

## 2. Canonical local contract gate — **MET locally**

Every Proposal 170 acceptance checker is green, and the transport, API, and
service crates stayed runtime-neutral throughout.

| Checker | Result |
|---|---|
| `check-i2pcontrol-acceptance-evidence.sh` | PASS |
| `check-els2-type11-transcript-boundary.sh` | PASS |
| `check-encrypted-service-consumer-caller.sh` | PASS |
| `check-outproxy-request-path.sh` | PASS |
| `check-outproxy-wire-lane-evidence.sh` | ok (16 required rows) |
| `check-service-tunnel-boundaries.sh` | PASS |
| `check-config-secret-hygiene.sh` | PASS |
| `check-service-anonymity-boundaries.sh` | PASS |
| `check-portable-service-tunnel-api.py` | PASS |
| `check-portable-service-tunnel-consumer.sh` | PASS |
| `check-i2pcontrol-acceptance-evidence.sh` | PASS |
| `python3 -m unittest discover -s tests/planning` | 51 tests, OK |

The ELS2 type-5 and secret-redaction cells are backed by the exported closures
this pass produced: **Plan 350** (floodfill stores and serves type 5 under its
blinded storage key) and **Plan 351** (both ELS2 resolvers have production
callers). Before those, this cell would have been an accepted-but-inert
promotion; it is no longer.

## 5. Security/resource gate — **MET locally**

| Item | Result |
|---|---|
| global plan-number uniqueness | PASS |
| ADR-number uniqueness | PASS |
| dependency direction | PASS |
| runtime boundaries | PASS |
| console boundaries + browser security | PASS |
| workflow validity | PASS |
| tooling inventory | PASS |
| fixture manifest | PASS |
| config-file error redaction + permissions policy (Plan 352) | PASS |
| no generic dual-transcript type-11 verifier; ELS2 profile confined to typed type-5 use | PASS (`check-els2-type11-transcript-boundary.sh`) |
| no direct-clearnet outproxy path | PASS (`check-service-tunnel-boundaries.sh` rules 9–11, plus Plan 376's live lane) |
| `cargo clippy --workspace --all-targets --all-features -D warnings` | PASS |
| `cargo fmt --all --check` | PASS |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | PASS |
| `cargo test --workspace --doc` | PASS |

## 3, 4, 6, 7 — **NOT MET**

- **§3 runtime capability gate** needs "type-5 external qualification imported
  from Plan 377". Plan 377 is blocked, so there is nothing to import. Every other
  item on that list is met, including the Plan 376 outproxy import.
- **§4 external differential gate** needs the Plan 377 ELS2 application evidence
  imported by exact hash. It does not exist. The control-plane differential
  lanes remain green but cannot stand in for it.
- **§6 final support vocabulary** — `full-proposal-conformant` is **NOT SET**.
  Only a passing gate may set it, for the exact revision frozen in §1. Two of its
  preconditions are unmet.
- **§7 CI and evidence integrity** — the local consistency checks pass (registry,
  roadmaps, `support.toml`, CONFORMANCE, and closure status all agree, which
  Plan 373's reconciliation is what made true). **Exact-head CI is not claimed**
  for this pass: Plan 378 changes code in the Plan 376 commit lineage and no
  exact-head hosted run is cited.

## Acceptance criteria

The plan passes only if the Proposal has no unresolved contract drift, Plans 376
and 377 are passed, every applicable surface has a real owner, the required
external lanes execute, the security gates pass, and exact-head CI is green.

| Criterion | Met |
|---|---|
| no unresolved contract drift | **yes** — §1 byte-identical |
| Plan 376 passed | **yes** |
| Plan 377 passed | **no** |
| every applicable surface has a real owner/effect | locally yes; externally no |
| required external lanes execute | **no** — Plans 374/375 blocked |
| security/resource gates pass | **yes** — §5 |
| exact-head CI green | **not claimed** |

**Plan 378 is blocked.** One dependency stands between it and its goal, and that
dependency is itself blocked on the single unwritten thing in this subsystem.

## Findings by severity

- **critical / high: none.** No defect was found by this plan.
- **medium (recorded, not fixed here)**: the external ELS2 lane has now been
  independently blocked twice (Plans 374 and 375) on the same unwritten driver,
  and Plan 377 and Plan 378 are downstream of it. That is four registered plans
  resting on one piece of unbuilt external-integration work. The unblock audit
  surfaces it rather than leaving it to be rediscovered; registering the driver
  as its own plan-of-record is the obvious remedy and is left to the next
  planning pass rather than done from a closure record.
- **low**: `scripts/interop/fetch-ssu2-reference.sh` cannot fetch the Java
  reference at its pin (clones a default branch, then fails its own pin check).
  Carried from Plans 374/375.

## Roadmap disposition and unblock audit

- **Roadmap disposition: blocked** on Plan 377 alone. Everything Plan 378 can do
  without external evidence has been executed.
- **Unblock audit:** no registered plan depends on Plan 378, so there is nothing
  to move. The audit is recorded as empty rather than omitted.

No corrective pass is registered: no defect was found.

## What is explicitly **not** claimed

- **No `full-proposal-conformant` token.** Type 5 stays `advertised = false`.
- No ELS2 interoperability, in either direction, with any reference.
- No outproxy capability surface — one still does not exist in
  `specs/support.toml`, and this plan adds none.
- `specs/support.toml` is unchanged by this plan. Nothing here implies enabled by
  default, remote exposure, a stable API, every historical I2P crypto algorithm,
  frontend completion, or transit participation.