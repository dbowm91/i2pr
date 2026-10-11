# Plan 454 status: passed — shared enforcement and current capacity permit

Closure token: `passed-shared-bandwidth-enforcement-and-current-capacity-permit`

Plan: `plans/implementation/floodfill/454-shared-bandwidth-enforcement-and-capability-permit.md`

## Completed scope

- Daemon composition creates one process governor and shares its handle with
  the enabled SSU2 owner. The governor has bounded inbound/outbound buckets,
  a bounded per-process and per-peer waiter queue, explicit Control/Client/
  Transit/Floodfill classes, a reserved control burst, capped control streaks,
  and rotating data-class service.
- SSU2 accounts accepted inbound datagram bytes before protocol processing and
  complete outbound datagrams before socket submission. Authenticated NTCP2
  adapter links account the two-byte frame prefix and ciphertext on read, and
  complete framed writes before socket output. The NTCP2 adapter refuses a
  burst that cannot fit one maximum data frame above the control reserve.
  Normal-daemon NTCP2 remains disabled under Plan 101.
- The runtime issues an opaque capacity permit only for a healthy governor and
  a valid nonzero share. It is tied to that exact governor, carries only its
  configured directional rates/share, and becomes unusable after governor
  stop/replacement or poisoned accounting state. The daemon derives the
  Java-compatible class from the configured bottleneck and share, not observed
  utilization.
- Capacity-bearing RouterInfo construction stays in the daemon floodfill
  owner. It rechecks the shaper permit and separately requires the existing
  floodfill role permit and qualified SSU2 address; the reachable form also
  requires the address-bound reachability proof. Underclaim, disabled, stopped,
  stale, and invalid-share cases are covered. No normal startup path uses this
  constructor.
- No `specs/support.toml`, conformance claim, default, normal daemon RouterInfo,
  `caps=f`, or `caps=fR` behavior was promoted. Plan 101 and the two-family
  Plans 437/438 gates remain authoritative.

## Verification evidence

| Command or evidence | Result |
| --- | --- |
| `rtk cargo test --locked -p i2pr-runtime --lib bandwidth::tests -- --test-threads=1` | Passed: 10 governor tests, including control reserve, class rotation/starvation cap, cancellation, and bound checks. |
| `rtk cargo test --locked -p i2pr-runtime --lib ntcp2_link::tests -- --test-threads=1` | Passed: loopback wire accounting/pacing, two-peer asymmetric accounting, cancellation cleanup, and link restart. |
| `rtk cargo test --locked -p i2pr-runtime --lib ntcp2_runtime::tests::shared_shaper_must_fit_a_maximum_data_frame_above_the_control_reserve -- --exact --test-threads=1` | Passed: default-sized 64 KiB burst is rejected for the dormant adapter; 128 KiB burst is accepted. |
| `rtk cargo test --locked -p i2pr-netdb --lib local::tests::capacity_tier_is_appended_only_with_role_and_address_gates -- --test-threads=1` | Passed: class letter is appended only by the explicitly gated floodfill builders; unqualified addresses remain rejected. |
| `rtk cargo test --locked -p i2pr-daemon --lib bandwidth_eligibility::tests -- --test-threads=1` | Passed: disabled, stopped, stale, underclaim, and share validation cases. |
| `rtk cargo test --locked -p i2pr-daemon --lib floodfill::tests::capacity_routerinfo_stays_inside_role_address_and_current_shaper_gates -- --exact --test-threads=1` | Passed: private floodfill-owner construction emits `fN` only with an active role and current shaper; a replacement governor fails closed. |
| `rtk bash scripts/check-m12-floodfill-boundaries.sh --self-test` | Passed: boundary and 11/11 mutation cases. |
| Full `AGENTS.md` floor from repo root | Passed: 4,796 tests passed, 39 ignored across 186 suites; clippy, rustdoc, doc tests, all listed boundary/evidence checks, and cargo deny passed. |
| `rtk git diff --check` | Passed. |

## Disposition and unblock audit

Plan 454 closes its bounded enforcement and capacity-permit scope. It does not
claim that Transit or floodfill roles are active, that the dormant NTCP2
adapter has authenticated external peers, or that a stock Java router selects
the resulting record. Plan 444 may now resume its independent controlled Java
selection controls using the verified configured class. Plans 437 and 438
still own normal floodfill eligibility, role-health withdrawal, opt-in, and
two-family qualification. Plans 433, 434/435, and 451 retain their separate
product/transport gates. No public traffic or reference router was started for
Plan 454.
