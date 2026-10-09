# Plan 388 status — blocked; inbound build completion corrective registered

Status: **blocked-post-start-inbound-builds-stall-before-response-plan-389**.

Plan: `plans/implementation/i2pcontrol-proposal-170/388-post-start-els2-pool-submission-corrective.md`.

## Result

Plan 388 localized and fixed a real scheduling defect. The service pool has
inbound and outbound targets of 2 with a build concurrency of 2. The post-start
replenisher submitted the entire available window outbound-first, allowing
pending outbound attempts to starve the inbound builds required to derive the
server's Standard LS2. The scheduler now interleaves inbound and outbound
submissions, beginning with inbound. A bounded regression covers one slot,
exact capacity, unequal deficits, and maximum-plus-one requested work.

A fresh exact-pinned NONE run confirmed the scheduling change: the server had
two pending inbound builds and zero pending outbound builds. It still had no
registrations or LS2 after the bounded 180-second publication wait. The
coordinator snapshot reported 43 inbound replies routed, 25 unmatched inbound
build replies, 11 installed builds, zero hop rejections, zero invalid replies,
four timeouts, and zero delivery failures. Those coordinator counts are
product-wide; they cannot attribute the replies or installations to the new
server Destination. Therefore the remaining transition is not yet localized
to its response correlation, completion, owner registration, or lease
installation. Plan 389 owns destination-scoped accounting and the corrective.

## Evidence

| Requirement | Evidence | Outcome |
|---|---|---|
| Preserve exact reference pin and one-attempt lookup | `target/interop/els2-evidence-plan388-none-postscheduler2-20261009/evidence.json` | i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1` |
| Three-peer mesh and ordinary authority control | Same artifact; `driver-evidence.tsv` | Passed; the post-start ordinary authority payload returned |
| Commit the reverse server | Same artifact; `reverse-server-create=committed` | Passed |
| Confirm scheduler interleaves the capacity-two window | `crates/i2pr-daemon/src/service_product.rs`; focused test below | Passed: inbound then outbound; bounded output |
| Observe reverse pool after 180 seconds | `driver-evidence.tsv`: `reverse-destination-provisioning` | Failed readiness: 0 registrations, 2 inbound pending, 0 outbound pending, no LS2 |
| Attribute reply outcomes to this Destination | Current coordinator snapshot | Not possible: existing completion/orphan counters are product-wide; Plan 389 owns attribution |
| Reverse NONE payload and PSK/DH matrix | Lane `results.tsv` | Not passed; test stopped at MissingLeaseSet before SAM consumption |

The sanitized package hashes are:

```text
evidence.json         e2d49318bca2a32694ebd1da19309746d6326f8dcec5b0b68b21063fa4836895
results.tsv           0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9
driver-evidence.tsv   0dabaa02946fa00f4b916e839d5691bd6a7e625770f9d66e3eb4dfdc1234af23
```

The lane JSON digest matches the packaged `results.tsv`. The initial attempt
after the fix and one earlier attempt ended with pinned i2pd segmentation
faults before the reverse driver row; neither is counted as a product result.

## Commands and outcomes

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed before daemon tests.
- `rtk cargo test --locked -p i2pr-daemon --lib destination_replenishment_tests -- --test-threads=1` — passed, 1 test.
- `rtk bash tests/integration/els2/run-i2pd-els2.sh --self-test` — passed; no reference process launched.
- `rtk python3 scripts/check-els2-live-lane-evidence.py --self-test` — passed.
- `rtk python3 scripts/check-els2-live-lane-evidence.py --mutation-table` — passed, 16 detected, 0 missed, 2 controls accepted.
- `rtk bash tests/integration/els2/run-i2pd-els2.sh` with evidence directory `target/interop/els2-evidence-plan388-none-postscheduler2-20261009` — ordinary authority payload passed; reverse publication failed at `MissingLeaseSet` after 185.76 seconds.
- Full routine floor, PSK, and DH were not run because the prerequisite NONE row did not pass.
- No CI result is claimed.

## Lifecycle, security, compatibility, and disposition

- Scheduling stays under the existing single product coordinator and respects
  the configured per-Destination/global pending limits. Existing generation
  replacement still cancels old Destination builds through
  `cancel_destination_builds`; no cancellation policy was weakened.
- Snapshot additions are bounded counts and profile quantities only. They
  expose no peer, destination, key, LS2, or payload material.
- No dependency, wire behavior, persistent format, transcript, credential seam,
  Proposal 170 inventory, support metadata, or advertisement changed.
- Type 5 remains non-advertised; no ELS2 capability or interoperability claim
  is made.

## Unblock audit

| Future plan | Can unblock? | Disposition |
|---|---|---|
| 374 i2pd bidirectional qualification | No | Still requires reverse NONE/PSK/DH live payload rows |
| 375 Java qualification | No | Independently blocked on Java pinned-source proof and live matrix |
| 377 ELS2 convergence | No | Requires 374 and 375 |
| 378 fresh full-Proposal gate | No | Requires 377 |
| 386 reverse publication corrective | No | Historical blocker remains downstream of server LS2 readiness |
| 387 server Destination readiness corrective | No | Superseded diagnostically by Plans 388 and 389; no pass claim |
| 389 inbound build completion and owner attribution corrective | Yes | Registered ready; localizes replies/completion/registration for the post-start server |

No ELS2 qualification or convergence plan is unblocked. `full-proposal-conformant`
remains unset and type-5 advertisement remains false.
