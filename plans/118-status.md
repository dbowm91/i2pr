# Plan 118 status — closed

- Status: **closed**
- Date: 2026-08-19
- Plan-of-record: [`118-planning-authority-cleanup-and-plan117-disposition.md`](118-planning-authority-cleanup-and-plan117-disposition.md)
- Source floor: `99374cf498227cf8ab1c4ec6ec4216b5d4d2e08e`
- Predecessor: Plan 117 (`native-reference-terminal-pending`)
- Successor: **Plan 119** (LeaseSet2 protocol foundation)

## Authority after Plan 118

```text
plan_118                               = closed
plan_117_local_composition             = passed-all-i2pr-production-seam-netdb
plan_117_native_reference              = blocked-reference-defect
plan_117_external_transport            = deferred-host-lane-unavailable
plan_117                               = closed-for-progression-with-evidence-gap
plan_115_117_roadmap                   = terminal-campaign-record
plan_118_123_milestone6_roadmap        = active-transport-neutral-router-construction
next_router_construction_plan          = Plan 119 (LeaseSet2 protocol foundation)
router_construction                    = may-continue
normal_daemon_ntcp2                    = disabled-and-unenableable
ntcp2                                  = experimental-non-advertised
```

## Plan 118 deliverables (closed)

- planning authority cleanup with one bounded Plan 117 disposition
  (Plan 118 Phase B Outcome 2: `closed-for-progression-with-evidence-gap`);
- separation of external acceptance debt from the product roadmap
  under section 8 of
  [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md);
- synchronized authority across `plans/000-mvp-roadmap.md`,
  `plans/115-117-external-delivery-to-live-netdb-roadmap.md`,
  `plans/117-status.md`, `plans/117-handoff.md`,
  `plans/118-123-milestone6-router-construction-roadmap.md`,
  `README.md`, `AGENTS.md`,
  `docs/architecture/overview.md`,
  `docs/architecture/i2pr-tunnel.md`,
  `docs/architecture/i2pr-netdb.md`,
  `docs/architecture/i2pr-daemon.md`,
  `docs/protocol-support.md`,
  `specs/support.toml`, and the
  `i2pr-ntcp2-interop` skill;
- the planning hygiene rules of Plan 118 §7 documented for
  Plans 119+ (one numbered plan per coherent product slice,
  parser/evidence separation, environment-blocker-as-debt rule,
  reference-as-validation-tool, no i2pr protocol relaxation to
  accept malformed reference output, normal-daemon NTCP2 stays
  disabled/non-advertised).

## Local product floor remains green

The retained implementation floor is:

```text
Plan 115 Emissary Q0 construction + native OBEP reply (closed)
Plan 116 local tunnel data plane (closed)
Plan 117 local production composition (closed)
Plan 117 native reference (blocked-reference-defect; closed for progression)
Plan 118 planning authority cleanup (closed)
```

The required local checks below remain green on the Plan 118
source floor:

```text
cargo fmt --all --check
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-daemon --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

## Next executable plan

```text
Plan 119: LeaseSet2 protocol foundation
```

See [`119-m6-leaseset2-protocol-foundation.md`](119-m6-leaseset2-protocol-foundation.md)
and the Milestone 6 roadmap at
[`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).