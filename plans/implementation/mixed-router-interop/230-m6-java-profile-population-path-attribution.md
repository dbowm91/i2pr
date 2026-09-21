# Plan 230 — M6 Java profile-population-path attribution

Status: **registered-ready-m6-java-profile-population-path-attribution**

## 1. Objective

Close the exact Plan-229 boundary:

```text
P229-C-NOT-EXPLORATORY-ELIGIBLE (profile_present=false, profile_count=0)
```

by attributing, through ordinary stock-Java mechanisms only, how (and
whether) the controlled A/B/C topology populates Router A's
profile-organizer tier population for its peers — the precondition
pinned `ExploratoryPeerSelector` (via `selectNotFailingPeers`) selects
from before it can build the Plan-229 non-zero exploratory tunnels.

This is a bounded diagnostic attribution. It MAY add read-only
harness diagnostics that observe profile-creation paths (heard-about
events, comm-established profiling, tunnel-participation profiling,
reorganization timing). It MUST NOT create profiles, promote tiers,
mutate scores, fabricate profile files, store NetDB entries, install
tunnels, patch Java, use reflection/VMComm, enable
`netDb.alwaysQuery`, change the Plan-229 roles, change exploratory or
client tunnel settings, or run the lookup/reverse-delivery lane.

The intended progression is:

```text
ordinary authenticated RouterInfo bootstrap (retained, P200-passed)
        ↓
observe which ordinary stock activity first places A/B/C peers
into A's organizer tier population (or prove none does)
        ↓
either the transit-peer gate passes on a documented stock path
        ↓
Plan-229 WP D/E rerun unchanged (non-zero exploratory + client build)
        ↓
or the first new stock-Java boundary is recorded with exact terminal
```

Plan 230 stops before exploratory polling and helper execution. If the
tier population becomes non-empty through a documented stock path, a
successor reruns the unchanged Plan-229 WP D/E gates; Plan 230 itself
claims no tunnels and no interop.

## 2. Registration basis

Plan 229 closed with:

```text
P229-C-NOT-EXPLORATORY-ELIGIBLE
```

on all four counted attempts (SHAs `00dc368` / `10d1015`).

Retained facts:

- A=service, B=publication, C=transit roles proven live; C proven
  non-floodfill (`caps_has_f=false`, `role_ok=1`);
- Router A effective exploratory settings match the stock small-router
  profile (`1/1/1/1`);
- `P200-H-publication-path-passed` on every attempt (ordinary
  authenticated DatabaseStore bootstrap works);
- Router C present and valid in A's main NetDB;
- `ProfileOrganizer.isSelectable(C)=true` (pinned bytecode: RI-fact
  based, tier-map independent);
- `profile_present=false`, `profile_count=0`, `not_failing_count=0`
  after a bounded 60 s in-flight wait on three of four attempts;
- helper never started; no exploratory snapshot; no P228 trace; lookup
  lane never entered.

The problem is upstream of i2pr wire/protocol behavior and upstream of
tunnel building: the tier population is empty.

## 3. Why ready

Hard dependency Plan 229 is closed with the exact stop terminal and a
bytecode-verified mechanism (`isSelectable` vs `selectAllPeers`
measure different populations). Interface dependencies are stable: the
P229 probe/launcher/harness surface (`P229-TRANSIT-PEER` with
`profile_count` / `not_failing_count`) is the observation contract this
plan extends. No ADR is required: no architecture changes, only
read-only harness diagnostics.

## 4. Invariants

1. Java pin unchanged (`9134f80…`); i2pd pin unchanged.
2. No i2pr production Rust changes.
3. No Java source patch; no reflection/private-state mutation.
4. No `ProfileOrganizer.addProfile`, no `getOrCreateProfile*` from any
   probe, no tier promotion, no score mutation, no profile-file
   fabrication, no profile-map manipulation.
5. No direct Java NetDB store/publish from launcher/probes.
6. Existing ordinary authenticated I2NP RouterInfo DatabaseStore
   bootstrap retained as the sole peer-learning mechanism.
7. Plan-229 roles and A-only small-router exploratory profile
   unchanged (byte/semantic equivalent; static guard enforced).
8. No exploratory `explicitPeers`; no quantity/backup/timeout/paired
   policy change.
9. No direct tunnel installation; no VMComm; no `netDb.alwaysQuery`;
   no public I2P/reseed; baseline loopback topology only.
10. Plan-227 raw helper profile unchanged; helper starts only if the
    transit gate passes on the documented stock path (otherwise the
    run stops before helper execution, as in Plan 229).
11. No Streaming-helper execution or change; helper five-minute ceiling
    and tunnel timeouts unchanged.
12. Frozen reverse-delivery 45-second window untouched and not entered.
13. Raw Java logs scratch-only; durable evidence bounded typed facts.
14. No M6 Java interop claim from profile presence alone.

## 5. Scope

In scope: read-only observation of the organizer tier population over
time (bounded polls of `profile_count` / `not_failing_count` /
per-peer membership), correlation with ordinary stock events
(comm-session establishment, DatabaseStore handling, exploratory pool
activity, reorganization), and the earliest-stage terminal taxonomy
below.

Explicitly out of scope: any profile/NetDB/tunnel mutation;
exploratory polling for helper start (owned by a WP D/E rerun
successor); helper execution unless the gate passes; lookup, tracked
send, and reverse-delivery qualification; SAM pivot (Plan 205 stays
retained).

## 6. Work packages

- **A — tier-population timeline.** Bounded read-only polls of the
  P229 transit-peer snapshot from Router-A startup through bootstrap
  and several minutes after, recording when (if ever) each peer enters
  the tier population. No helper start; no build triggering.
- **B — stock-path correlation.** Correlate first tier entry (if any)
  with observable ordinary stock activity using only existing
  sanitized log scopes plus bounded counts (comm establishment,
  DatabaseStore receipt, exploratory pool scheduling). No new DEBUG
  scopes carrying key/tag material; any new scope needs the same
  whitelist-sanitizer treatment as Plans 224/228.
- **C — terminal taxonomy.** Exactly one terminal per counted run:

```text
P230-PROFILE-POPULATED (tier population non-empty via the documented stock path)
P230-PROFILE-ABSENT (bounded budget exhausted, tier population still empty)
P230-OBSERVABILITY-GAP
```

`P230-PROFILE-POPULATED` records the populating stock path and hands
the unchanged Plan-229 WP D/E rerun to a successor. It does not start
the helper itself.

## 7. Failure / cancellation / restart semantics

Fail-closed: any diagnostic unreachability maps to
`P230-OBSERVABILITY-GAP`, never to a populated claim. A run may stop
early once the first authoritative terminal is proven. Every spawned
router/helper context is disposable; cancellation kills the process
group; no state crosses attempts.

## 8. Compatibility and migration

None: test-harness-only diagnostics. No production, fixture, or
reference-pin change.

## 9. Required tests

Focused unit rows covering at least: tier-empty maps to absent;
tier-nonempty-with-C maps to populated only with the documented path
echo; tier-nonempty-without-C is insufficient; isSelectable-alone
cannot satisfy the gate (regression lock of the Plan-229 finding);
secret/unrelated lines rejected; exactly one terminal; no
lookup/reverse payload after any terminal.

## 10. Verification floor

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
cargo test --locked -p i2pr-daemon --test java_tunnel_external p230_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p229_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
```

## 11. Acceptance criteria

Plan 230 closes only if: pins frozen; no production changes; Plan-229
roles/settings/profile-path prohibitions retained (static guard);
bounded tier-population timeline recorded from live read-only
snapshots; the populating stock path (if any) documented with exact
counts; exactly one terminal per counted run; no helper-start claim
without the gate; no lookup/reverse claim; focused/routine
verification passes; closure record, registry, roadmap, and unblock
audit land.

## 12. Stop conditions and successor rules

On `P230-PROFILE-POPULATED`, register a narrow successor that reruns
the unchanged Plan-229 WP D/E gates (exploratory poll + helper +
build-path continuation). On `P230-PROFILE-ABSENT`, register at most
one successor tied to the exact absent stage. On
`P230-OBSERVABILITY-GAP`, fix observability first.

## 13. Closure evidence

Update `plans/closure/mixed-router-interop/230-status.md` with:
implementation SHA(s); pins; role/settings retention proof; bounded
tier-population timeline; stock-path correlation counts; exact
terminal; counted attempt history; local verification; exact-head CI;
security/compatibility notes; unblock audit.

## 14. Registration disposition

```text
plan_229 = passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary
plan_230 = registered-ready-m6-java-profile-population-path-attribution

plan_201 = blocked-pending-plan230-profile-population-path-attribution-after-plan229-not-eligible
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan230
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 230-m6-java-profile-population-path-attribution
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

## 15. Handoff notes

Start from the Plan-229 closing head. The P229 probe already exposes
`profile_count` / `not_failing_count`; extend timeline polling around
it rather than inventing a parallel snapshot. Keep the counted lane
destination-only. Maximum three counted attempts per implementation
SHA; commit implementation before counted execution; clean-head rule
applies.
