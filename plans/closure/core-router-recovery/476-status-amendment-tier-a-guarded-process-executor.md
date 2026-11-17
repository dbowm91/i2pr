# Plan 476 status amendment — Tier A guarded process/session receipt executor

**Status token:** `blocked-authorized-host-pair-unavailable-tier-a-guarded-process-executor-passed`
**Supersedes the status of** [`476-status.md`](476-status.md)
(`blocked-local-process-executor-and-authorized-host-pair-unavailable`) and of
[`476-status-amendment-private-lan-dial.md`](476-status-amendment-private-lan-dial.md),
which restated it. **Both predecessor records are unchanged and remain the
historical account of the earlier attempts.** This amendment is the newest
explicit status and governs where the two disagree.
**Implementation commits:**
- `7968d81f` — *feat(qualification): land the Plan 476 guarded per-host receipt executor*;
- `ad1326f7` — *fix(qualification): close the defects an independent review of Plan 476 found*;
- `7e055e09` — *fix(daemon): collapse the receipt export tick's let-chain for clippy*.
**Disposition:** **Tier A passed** — the guarded per-host process/session receipt
executor, its fail-closed state machine, its simulator matrix and the machine
enforcement of its binding to the Plan 495 gate are implemented and verified.
**Tier B not run** — no authorized two-host pair exists on this host, no live
process was started by the executor's live path, no packet was sent, and no
two-host SSU2 result may be claimed.

## Why this amendment exists

The predecessor record named exactly one executable next step:

> Plan 476 remains open: implement the per-host local process owner and
> independently captured process/session receipt exchange, with fail-closed
> configuration and cancellation before any packet, then test its positive/
> negative controls offline.

That step is now delivered. Plan 476's plan-of-record asks for the closure to be
written in **two explicit evidence tiers**, and this record does that: Tier A
closes, Tier B stays open and is not closable here.

## Two evidence tiers

| Tier | Meaning | State | Evidence |
| --- | --- | --- | --- |
| **A** | executable source and negative-control contract | **Passed** | the executor, the daemon's sanitized local receipt export, the simulator matrix, the binding checker |
| **B** | live two-host independent auth + I2NP + restart/withdrawal | **Not run** | no authorized host pair exists on this host; no live command was run; no packet was sent |

Tier A is not a capability claim. The lane is **infrastructure-complete, not
capability-complete**: the executor exists, is fail-closed, and is source-verified,
and it has never been run against a peer.

## Requirement-to-evidence matrix

| Requirement (Plan 476) | Evidence | Result |
| --- | --- | --- |
| A constrained local `run --live --role i2pr\|i2pd` owner that starts **only** the declared local router | `tests/integration/ssu2/plan476_live_executor.py::_router_argv` builds exactly `<daemon-binary> run --config <config>` or `<i2pd-binary> --conf <config>`; `SubprocessAdapter.spawn` is the only spawn site. The self-test asserts the exact argv (`plan476_live_executor.py:1639`) | Passed |
| Interhost transfer is manual and owner-approved; the executor does not SSH, discover peer addresses, relay over clearnet services, fetch a reseed, or start a daemon on another machine | The executor's only network-facing dependency is the local UDP socket the router itself binds; the two host receipts are joined by the owner running `reconcile` on files the owner carried across. `docs/operations/ssu2-private-lan-qualification.md` §9.2 states the exchange is manual | Passed |
| Reject missing consent, stale signed RI, changed interface/route, changed ports/identity and mismatched trusted peer artifacts **before** any socket or packet | `execute_run` runs, in this order and all before the single spawn site: `validate_authorizations` (`:1110`), the withholding control (`:1124`), the hygiene protocol (`:1132`), the Plan 449 `check_local_host` interface/route probe (`:1150`), the `0700` data-directory check (`:1151`), the pinned-reference `verify_reference` (`:1156`) — and only then `processes.spawn` (`:1162`) | Passed |
| Treat operator revocation as immediate cancellation/cleanup | `SubprocessAdapter` sends `SIGTERM` to the owned process group, waits a bounded grace period, then `SIGKILL`s and reaps (`:259-318`); only processes this executor started are ever signalled | Passed |
| The evidence receipt stays fully sanitized: bounded phase/state/count, never a raw key, a full RouterInfo, an actual private endpoint, an external log or a path | The daemon export writes phase labels and counts only (`crates/i2pr-daemon/src/ssu2_qualification_export.rs::snapshot`); the executor's `_validate_local_receipt` accepts no field beyond phase labels and counts; the host receipt carries digests only | Passed |
| A collector refusal cannot be forgotten | the export's `error` field is sticky for the life of the process: an overflowed epoch stays recorded even after a later successful drain, and the consumer fails closed on it | Passed |
| Pair equality via an owner-exchanged private artifact | `reconcile_receipts` requires two receipts with distinct roles, one `run_epoch`, distinct host/endpoint/RouterInfo digests, and each side's `peer_router_info_digest` equal to the other's `local_router_info_digest` (`plan476_live_executor.py:1155-1166`) | Passed |
| A successful live lane must require independent captures from **both** endpoints and the exact pin | `reconcile_receipts` refuses one receipt (`independent-peer-receipt-required`), a duplicate role (`receipt-role-invalid`), a mismatched epoch (`run-epoch-mismatch`), colliding identities (`router-identities-not-distinct`), and a cross-bound digest mismatch (`peer-router-info-mismatch`) | Passed |
| Clear temporary secrets and reap all child processes | Every spawn path ends in a reap; artifacts are written `0600` through an atomic rename (`_write_private_json`) | Passed |
| A deterministic simulator/preflight matrix that never emits live packets, driven through an in-process fake I/O adapter that cannot connect | `FakeProcessAdapter`, `FakeHostProbe` and `FakeReceiptCollector` expose no socket/subprocess API (asserted by the executor self-test at `:2166-2177`); `tests/integration/ssu2/test_plan476_live_executor.py` imports no `socket`, `subprocess`, `asyncio`, `ssl` or `http` — enforced mechanically by `scripts/check-plan476-gate-binding.py` rule 3 | Passed |
| Exercise phase transitions and per-host receipt reconciliation offline | `build_tiers` (`:1340`) and `reconcile_receipts` (`:1549`) are exercised by the executor's own self-test (57 checks, 6 negative controls) and by 35 unittest cases | Passed |
| `prepare` and `verify` remain non-emitting | `tests/integration/ssu2/plan472_live.py` is unchanged by this work | Passed |
| Plan 495 binding: evaluate the peer-eligibility predicates before any launch | `execute_run` step 2 (`:1117`) calls `gate.validate_profile`/`validate_peer`/`evaluate`/`classify` and refuses on a non-`lane-ready` verdict, before the withholding control, the hygiene step and the spawn | Passed |
| Plan 495 binding: the deliberate-withholding control must classify as `peer-selection-refused` with the cause `intentional-withholding` | `withholding_control_holds` (`:812`) withholds an `explore` predicate and requires exactly that verdict; a control that misbehaves refuses the launch with `withholding-control-failed` (`:1124`) | Passed |
| Plan 495 binding: record the claim-tier ladder, each tier `pass`/`fail`/`not-attempted` with a reason | `build_tiers` (`:1340-1428`) emits all nine tiers with Plan 495's own tier names in the reason; a lower tier's pass is never retracted | Passed |
| Plan 495 binding: classify every outcome with its cause and never report a peer-side refusal as an `i2pr` defect | `i2pr-defect` is never emitted by the executor; a gate refusal keeps the gate's own outcome class and cause; a lane with no attributable tier is `timeout`/`unattributed`. `i2pr-defect` remains a reviewed human judgement, and only `i2pr-implementation` may accompany it | Passed |
| The peer-state hygiene protocol, with a poisoned peer state not consuming the attempt budget | `build_hygiene_record` + `gate.validate_hygiene` (`:1132`); a violation raises `peer-state-poisoned`/`peer-state` and the evidence records `consumes_budget: false` | Passed |
| Live two-host independent auth + I2NP + restart/withdrawal | No authorized host pair exists on this host. No live command was run, no process was started by the executor's live path, and no packet was sent | **Environment blocked; not run** |

## Verification

| Floor step | Outcome | Seconds |
| --- | --- | ---: |
| `cargo-fmt` | PASS | 3 |
| `cargo-check` | PASS | 9 |
| `cargo-build-siblings` | PASS | 0 |
| `cargo-build-static-fixture` | PASS | 1 |
| `cargo-copy-static-fixture` | PASS | 0 |
| `cargo-rebuild-plain-fixture` | PASS | 0 |
| `cargo-test-workspace` | PASS | 1777 |
| `cargo-clippy` | PASS | 1 |
| `cargo-doc` | PASS | 9 |
| `cargo-test-doc` | PASS | 3 |
| `check-dependency-direction` | PASS | 0 |
| `check-global-plan-number-uniqueness` | PASS | 1 |
| `check-adr-number-uniqueness` | PASS | 0 |
| `check-portable-service-tunnel-api` | PASS | 0 |
| `check-portable-service-tunnel-consumer` | PASS | 6 |
| `planning-tests` | PASS | 6 |
| `check-runtime-boundaries` | PASS | 1 |
| `check-console-boundaries` | PASS | 2 |
| `check-console-browser-security` | PASS | 0 |
| `check-tooling-inventory` | PASS | 0 |
| `check-license-metadata` | PASS | 0 |
| `check-managed-app-private-client-seams` | PASS | 0 |
| `check-router-readiness-contract` | PASS | 0 |
| `check-router-readiness-contract-self` | PASS | 0 |
| `check-core-router-recovery-contract` | PASS | 0 |
| `check-core-router-recovery-contract-self` | PASS | 0 |
| `plan449-ssu2-preflight-self` | PASS | 0 |
| `plan495-peer-eligibility-self` | PASS | 0 |
| `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| `plan476-gate-binding` | PASS | 1 |
| `plan476-gate-binding-self` | PASS | 0 |
| `check-managed-app-package-boundary` | PASS | 0 |
| `check-managed-app-package-boundary-self` | PASS | 0 |
| `check-managed-app-policy-boundary` | PASS | 1 |
| `check-managed-app-policy-boundary-self` | PASS | 0 |
| `check-managed-app-secured-sandbox` | PASS | 0 |
| `check-managed-app-secured-sandbox-self` | PASS | 0 |
| `check-service-tunnel-boundaries` | PASS | 0 |
| `check-managed-app-gateway-boundary` | PASS | 0 |
| `check-managed-app-manager-boundary` | PASS | 0 |
| `check-m11-per-epoch-composition` | PASS | 0 |
| `check-service-anonymity-boundaries` | PASS | 0 |
| `check-fixture-manifest` | PASS | 0 |
| `check-ntcp2-vectors` | PASS | 0 |
| `check-ssu2-vectors` | PASS | 0 |
| `check-i2cp-vectors` | PASS | 1 |
| `check-ntcp2-interoperability` | PASS | 0 |
| `check-constrained-host-lane-boundary` | PASS | 0 |
| `check-m11-transit-boundaries` | PASS | 1 |
| `check-m11-transit-qualification-evidence` | PASS | 0 |
| `check-sam-acceptance-evidence` | PASS | 0 |
| `check-ssu2-acceptance-evidence` | PASS | 1 |
| `check-i2cp-acceptance-evidence` | PASS | 0 |
| `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| `check-els2-type11-transcript-boundary` | PASS | 0 |
| `check-els2-live-lane-evidence` | PASS | 1 |
| `check-encrypted-service-consumer-caller` | PASS | 0 |
| `check-outproxy-request-path` | PASS | 1 |
| `check-outproxy-wire-lane-evidence` | PASS | 0 |
| `check-config-secret-hygiene` | PASS | 0 |
| `check-workflow-validity` | PASS | 0 |
| `check-floodfill-type5-serve` | PASS | 0 |
| `check-service-tunnel-acceptance-evidence` | PASS | 0 |
| `check-exploratory-tunnel-evidence` | PASS | 1 |
| `check-netdb-tunnel-evidence` | PASS | 0 |
| `check-destination-tunnel-evidence` | PASS | 0 |
| `check-streaming-tunnel-evidence` | PASS | 5 |
| `check-m6-mixed-router-acceptance-evidence` | PASS | 2 |
| `check-m12-floodfill-qualification-evidence-self` | PASS | 1 |
| `check-m12-floodfill-boundaries-self` | PASS | 0 |
| `ntcp2-harness-tests` | PASS | 0 |
| `plan488-mesh-lane` | PASS | 0 |
| `plan488-evidence-deterministic` | PASS | 0 |
| `plan489-mesh-lane` | PASS | 1 |
| `plan489-evidence-deterministic` | PASS | 0 |
| `plan490-mesh-lane` | PASS | 0 |
| `plan490-evidence-deterministic` | PASS | 0 |
| `plan490-ssu2-transport-suite` | PASS | 6 |
| `plan490-transport-boundary-guard` | PASS | 0 |
| `plan491-mesh-lane` | PASS | 1 |
| `plan491-evidence-deterministic` | PASS | 0 |
| `plan491-boundary-guard` | PASS | 0 |
| `plan491-proto-suite` | PASS | 0 |
| `plan491-netdb-suite` | PASS | 86 |
| `plan491-client-suite` | PASS | 24 |
| `plan492-posture-suite` | PASS | 1 |
| `plan492-m11-lane` | PASS | 4 |
| `cargo-deny` | PASS | 1 |

<!-- 88 steps, 88 PASS, 0 FAIL -->

The floor ran on the implementation tree (`7e055e09`). The closure commit that adds
this record is documentation-only, and the document-level checkers were re-run on
the closing tree afterwards; the cargo results carry over unchanged, on the same
pattern Plan 492 and Plan 495 used.

## Invariant, failure, migration and security review

**Ordering.** The gate, the withholding control, the hygiene protocol and every
pre-spawn input check all precede the only spawn site. The executor's own
negative controls assert this directly: the fake process adapter records
`spawned == []` on a gate refusal, on a hygiene refusal and on a withholding-control
failure. No path exists on which a router process starts before the gate has
passed.

**Refusal is the default.** An unknown, uncomparable, below-floor or refused peer
predicate is refused rather than skipped, and the refusal carries a reason code.
The executor emits exactly one sanitized JSON line and exits non-zero; it never
prints an address, an interface name, a host id, a key, a RouterInfo, a raw log or
a packet.

**No defect laundered.** The executor never emits `i2pr-defect`. A lane that
produced nothing attributable is `timeout` with the cause `unattributed`, and a
peer-side refusal keeps the Plan 495 outcome class the gate produced. This is the
whole point of the plan: a peer that declines to select us must never be recorded
as our defect.

**Receipt sanitization by construction.** The daemon's export cannot carry a key,
a RouterInfo, an address, an endpoint, a payload or a transient message ID: the
collector drops every transient ID inside `collect`, and the export adds only
phase labels and counts. The Rust integration test asserts the exported text
contains none of that vocabulary, and asserts the file is created `0600`.

**One change to the Plan 480 collector's contract.** A drained epoch with no
authenticated session is now **projected** rather than refused. The refusal had
to go: a drained epoch is consumed whether or not it is projected, so refusing
it silently discarded the epoch's other phases — including the one that records
the local bind, which the export needs. The fail-closed property did not
weaken: the projection carries `session_observed: false` and omits every
session phase, so it still cannot claim a session that did not happen, and the
consumer (`validate_local_observed`) refuses a receipt with no non-zero
authenticated-session phase. The predecessor Plan 480 closure record is
unchanged and remains the historical account of the earlier contract.

**A refusal is sticky.** An overflowed or otherwise refused epoch is recorded
in the export and is never cleared by a later successful drain: the consumer
fails closed on a non-null `error`, so a refusal that healed would turn an
untrustworthy epoch into an apparently healthy one.

**Migration.** None. No configuration key was added to an existing schema without
a default, no existing key changed meaning, no on-disk format changed, and no
data was migrated. `specs/support.toml` and the workspace `Cargo.toml` are
byte-identical to the predecessor tree.

**Support inventory.** Unchanged and re-derived mechanically: 94 surfaces, 88
`experimental`, 5 `deferred`, 1 `ready`, 0 advertised.

**Contention and concurrency.** The executor owns exactly one child process per
attempt and reaps it on every path. The daemon's export tick is a bounded
`MissedTickBehavior::Skip` interval in the composition root; it holds no lock
across a write and replaces the document atomically through a temporary file and
a rename.

## Documentation and operational evidence

| Artifact | State |
| --- | --- |
| `docs/operations/ssu2-private-lan-qualification.md` | §4.1 documents the local receipt export; §9 documents the executor, the ordered state machine, the per-host commands, the shared `--epoch`, the manual receipt exchange and the tier-evidence sources. It states that no live two-host result exists yet |
| `specs/CONFORMANCE.md` | the "External qualification gates" binding section names the canonical model, the operator procedure, the Plan 476 executor, the daemon receipt export and its config key, the checker and the frozen pins, and states that the live lane is environment-blocked |
| `AGENTS.md` | the routine floor runs the new binding checker, both plain and `--self-test` |
| `.github/workflows/ci.yml` | a "Check Plan 476 gate binding" step runs the checker, its self-test, the executor's own self-test and the executor's unittest module |
| `docs/architecture/tooling.md` | the new checker has an inventory row, and every published figure was re-derived from the tree |

## Limitations

1. **Tier B was never executed.** The executor's live path has never started a
   router. Its argv, its signal handling and its collection of a real exported
   receipt are source-verified and simulator-verified, and the daemon's export is
   proven end to end by a black-box test that starts the real `i2pr` executable —
   but no two-host run exists.
2. **The live lane's ceiling under the current observer is T3/T5, not T4.** The
   Plan 480 observer deliberately retains no per-request reply-correlation
   material, so "the peer answered a request we originated" cannot be attributed
   to a request. The executor therefore reports T4 as `not-attempted` even when a
   bidirectional authenticated I2NP exchange was observed. Raising the ceiling
   needs a Plan 480 follow-on that changes the observer's contract.
3. **Two hygiene conditions cannot be derived offline.** The peer's static key on
   the live port, and the advertised endpoint matching the bound socket, are
   live-socket properties. The operator supplies them through `--hygiene`, and
   the executor re-validates them through the Plan 495 checker; it cannot verify
   them itself.
4. **The deliberate-withholding control is provable only when the profile
   satisfies exactly one `explore` route.** A profile that satisfies both i2pd
   explore routes makes the control unprovable, and the executor then refuses to
   launch with `withholding-control-failed`. That is fail-closed, but it makes the
   canonical lab profile load-bearing.
5. **The peer-side observation depends on i2pd's own NetDB naming.** The
   `routerInfo-<base32>.dat` expectation was derived from the pinned source
   (`libi2pd/NetDb.cpp:42` and `libi2pd/Base.cpp:261-290`, i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`). A future pin change needs the
   derivation redone.
6. **The Java family is modelled but unexercised.** The Plan 495 gate derives
   Java I2P predicates from the pinned Java 2.13.0 source, but no lane in this
   repository orchestrates a Java I2P router, so the Java route was never run.
7. **The receipt export is an on-disk artifact.** It is `0600` in an
   operator-designated directory and carries no secret, but it is still a file
   the operator must not publish, and its parent directory must be one the
   operator controls.

## Findings by severity

**Critical:** none.

**High:** none. Three high-severity defects were found by an independent
verification pass and fixed before this record was written; see
**the verification pass** below.

**Medium:**

- **M1 — the live lane cannot reach T4.** See limitation 2. The tier ladder
  cannot attribute a reply to a request, so the lane's honest ceiling is T3
  (one-way I2NP) and T5 (a fresh reverse initiation). *Not a false claim* — the
  executor reports T4 as `not-attempted` with the reason — but it bounds what a
  future live run can prove. Closing it needs a Plan 480 follow-on that changes
  the observer contract, and its own plan-of-record.
- **M2 — the deliberate-withholding control depends on the lab profile.** See
  limitation 4. A profile that satisfies two `explore` routes makes the control
  unprovable and the lane unrunnable. The refusal is correct; the fragility is
  in the fixture, and the canonical lab profile should keep exactly one
  `explore` route satisfied.

**Low:**

- **L1 — the collector's reply correlation is epoch-local.** The written
  message IDs are held for one drain interval, so a reply arriving after the
  next 500 ms export drain is never correlated and tier T4 stays
  `not-attempted`. The executor's reason string states this. Closing it means
  holding the written-ID set in the observer across drains — a Plan 480
  contract change with its own plan-of-record.
- **L2 — the daemon's export consent is hard-coded `true`.** The composition
  root constructs the exporter with `consent: true`, because the operator's
  explicit configuration *is* the consent. The collector's "consent is
  re-checked at collection time" guard therefore cannot fire in production. It
  remains as defence in depth for a future revocation path.
- **L3 — the receipts are digest-bound, not signature-bound.** The plan asks
  for a "signed hash digest of ephemeral receipt". The receipts carry SHA-256
  digests, and the `peer_router_info_digest` is the digest of the *signed*
  RouterInfo, but the receipts themselves are not signed. The authenticity
  comes from the owner-exchanged channel, not from a signature.
- **L4 — `plan472_live.py run --live` still refuses with
  `live-executor-unavailable`.** That runner's offline `prepare`/`verify` are
  still useful and are unchanged; its live subcommand remains a fail-closed
  stub because it has no executor wired in. The operator path is the executor,
  and the operator procedure says so. Reconciling the two runners is a small
  separate cleanup.
- **L5 — `docs/architecture/i2pr-daemon.md` still owes seven module rows.**
  The two Plan 476 modules now have rows and the file's counts were
  re-derived, but seven rows for modules added by other plans remain missing.
  The gap is stated in the document rather than papered over.
- **L6 — a failed live run consumes the whole timeout window.** Readiness now
  requires a session-bearing receipt, so a run in which the peer never
  establishes a session waits out the inventory's `timeout_seconds` before it
  refuses. That is the intended fail-closed behaviour, and it makes the lane
  slow on failure.

**Informational:**

- **I1 — the collector's phase vocabulary grew from six to nine.** The two
  direction phases and `request_sent` were added so that tiers T1, T2, T3 and T5
  are observable at all. The Rust and Python vocabularies are now compared
  element by element by `scripts/check-plan476-gate-binding.py`, so the two
  implementations of that wire contract cannot drift apart silently.
- **I2 — the peer-side observation reads i2pd's naming, not i2pd's internals.**
  The executor globs for the expected file name and never reads the file's
  contents. Only the fact is recorded, never the hash or the name.

## The verification pass

An independent verification pass ran against this commit, reading the plan of
record and the closure records as the acceptance criteria. It found three
high-, five medium- and nine low-severity defects. All were fixed before this
record was written, and the fixes are in commit `ad1326f7`.

The three high-severity defects would each have produced a wrong claim:

| Defect | What was wrong | Fix |
| --- | --- | --- |
| **H1** | the reconciled evidence derived its outcome from "any tier passed", so a run in which the peer admitted our RouterInfo but never selected or dialled us was written out as `outcome: pass, cause: null` — the exact failure this plan exists to prevent | the outcome now follows the ladder: tier 0 failing is `peer-admission-refused`, tier 1 or 5 failing is `peer-selection-refused`, and only an attributable tier is a `pass` (`plan476_live_executor.py`, `build_lane_evidence`) |
| **H2** | readiness was "the exported receipt file exists", and the daemon creates that file on its first export tick with only the permit phase — so the live lane terminated the router before any session could happen, and no receipt was produced at all | readiness now requires a session-bearing receipt; file existence is not readiness |
| **H3** | the spawn-to-collect sequence had no `try/finally`, so an exception or an operator interrupt left the router's process group running with its state on disk | the sequence is guarded, the owned child is always terminated and reaped, and SIGINT/SIGTERM cancel the attempt |

The medium defects: the pinned i2pd binary was never verified (a regression
against the predecessor runner, which did verify it); the local interface and
route were never checked; the declared data directory was not required to be
`0700`; the daemon's export cleared a collector refusal on the next successful
drain, so an overflowed epoch could look healthy again; and the exported file's
mode was only set at creation, so a leftover temporary could keep a wider mode.

The low defects were a stale tier-4 reason, an unreachable tier-2 branch, a
tier-0 reason that did not say the check was by file name, the missing
`--i2pd-source` flag in the operator procedure, the missing crate-deep-dive
rows, and the stale registry and roadmap text.

The pass also showed that the new binding checker was name-based and missed
five mutations: a stub gate with the right method names, a socket import in the
executor, a published option the executor does not accept, a commented-out
floor line, and a writer that leaked an endpoint. The checker now resolves
which file the executor's gate came from, scans the writer that actually lands
on disk, requires the gate to be evaluated before the spawn in source order,
forbids the executor from carrying an `i2pr-defect` literal, checks the
published options against the parser, and requires the floor to run it as a
command. Every one of its ten rules now has a negative probe that exercises
the rule function itself.


## Roadmap disposition

**Blocked, with the executor blocker discharged.** `plans/subsystems/core-router-recovery-roadmap.md`'s
Plan 476 row advances from "local process/session receipt executor incomplete" to
"Tier A passed; Tier B environment-blocked". The plan is **not closed**: its own
acceptance includes the live two-host run, and that run cannot happen without an
authorized host pair, which does not exist on this host.

## Unblock audit

Every registered plan that lists Plan 476 as a hard or interface dependency was
checked against the registry and its own plan document.

| Plan | Relationship to 476 | Other hard dependencies | State change |
| --- | --- | --- | --- |
| **472** — authorized two-host SSU2 live qualification runner | 476 is the corrective successor for 472's live-executor half | an authorized host pair | **No state change.** 472's executor blocker is discharged by 476 Tier A; its live proof still needs the environment, so it stays `blocked`. The registry row now says which half is discharged |
| **493** — two-family public participation qualification | names "Plan 472/476/480 own the runner; Plan 449" as its topology blocker | the authorized two-host topology (431, 472, 476, 480) and Plan 435's transport activation | **No state change.** 476 Tier A is not the blocker; the environment and Plan 435 are |
| **480** — source-authenticated SSU2 receipt observer | names 476 as its consumer | — | **No state change.** 480 is closed at its own narrow scope; its continuation pointer now has a consumer |
| **449** — portable SSU2 independent LAN qualification lane | 476 is the executor that carries 449's live column | an authorized host pair | **No state change.** 449 stays `passed-portable-preflight-infrastructure-only`; its live column stays environment-blocked. See [`449-status-amendment-peer-eligibility-gate.md`](449-status-amendment-peer-eligibility-gate.md) |
| **431** — stopped | historical predecessor | — | **No state change.** Remains `stopped` |
| 474, 475, 478, 485, 486 | each states explicitly that it does **not** depend on Plan 476 | — | **No state change** |

**No plan changed state.** Every remaining blocker is an external environment or
an unbuilt executor that this plan does not own. Moving a plan to `ready` here
would be exactly the error this plan exists to prevent: it would let a
peer-side or environment-side condition be read as a completed qualification.

The registry's "Next blocker correctives — Plans 474–476" row for 476 is updated
to name the discharged half and the remaining blocker; the 472 row says which
half of its blocker is discharged; the 449 row names the executor that now
carries its live column.
