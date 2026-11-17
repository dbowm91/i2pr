# Plan 548 — a publication must reach a floodfill in a network that holds no floodfill advertisement

## 1. Objective

Make it possible for a router on an isolated network to **publish its
RouterInfo to a floodfill**, so that a publication's acknowledgement — a
`DeliveryStatus` correlated against the attempt that caused it — can be
observed without a public network.

The plan is registered by Plan 547's closure, which measured the floodfill
qualification gate reaching `role="Active"` on a real LAN host and then found
that **no publication could ever be sent**: the target selection reads the
publisher's own NetDB for a record that advertises floodfill, and in a
three-node isolated network no host holds one.

## 2. Why ready

The blocker is located, source-attributed and reproducible, and its three
legs are each verified:

1. `publish_once_inner` (`crates/i2pr-daemon/src/router_info_publication.rs`)
   selects targets with `publication_targets_excluding` →
   `select_version_gated_floodfills`, so a target must advertise floodfill.
2. In the measured network every record carries `caps="4"` — including the
   record each router hands out about itself.
3. There is no production path by which a peer could learn otherwise: the
   production router runs no RouterInfo-kind lookup, the floodfill's own
   publication is `RefusedNoTarget` for the same reason, and the out-of-band
   export is written at startup, before activation, and never rewritten.

`plans/closure/network-participation/547-status.md` is the evidence.

There is a **second, independent** blocker in the same measurement and this
plan does **not** own it: the floodfill role does not sustain, because only two
of the three corroboration classes the transport requires are refreshed by
traffic a long-lived session produces. Its owner is the `ssu2` roadmap's
reachability-observability work, named in 547's Findings (medium). A publication
into a `Disabled` window would be refused even with a target, so the two must be
measured together — see §14.

## 3. Current implementation evidence

### 3.1 The selection is floodfill-only

```text
// crates/i2pr-daemon/src/router_info_publication.rs
let selection =
    publication_targets_excluding(store, &self.local_hash, &self.cycle.asked, &self.policy);
```

and `publication_targets_excluding` reaches `select_version_gated_floodfills`,
which admits only records advertising the floodfill capability.

### 3.2 Every host advertises `caps="4"`

Measured on 2026-11-13 from the three-host lab (Plan 547's record, Findings):

```text
via-seed-.20  caps=.4;host=192.168.182.20
via-seed-.8   caps=.4;host=192.168.182.8
local-export  caps=.4;host=192.168.182.143
```

### 3.3 The export predates activation

`export_local_router_info` is reached from the NTCP2 shared-record path
(`crates/i2pr-daemon/src/lib.rs:5308`), which runs once at startup. `.20`'s
export is dated `14:53`, and its floodfill activation was `14:54:36`, so the
record the operator can hand to a peer can never carry `caps=f`.

### 3.4 What the plan does not claim

It does not claim that the reference networks behave this way, that a public
network needs this at all (a public network has floodfills that publish), or
that the floodfill's serving behaviour against stock peers is qualified. Plan
493 owns the stock-peer question.

## 4. Invariants

- **The selection's rule is not weakened.** A publisher still publishes to
  floodfills. The plan changes **how a floodfill becomes known**, never what a
  floodfill is.
- **No forged capabilities.** `caps=f` is only ever placed in a record by the
  router that owns the signing key, inside its own process, and only when the
  qualification gate has already decided it. Nothing else may mint it, and no
  test fixture may be promoted to production.
- **No secret leaves its process.** The export path may write a **signed,
  public** RouterInfo. It may not write, derive or copy a private key, a
  signing seed, a static secret or a token.
- **The export stays bounded and private.** `0700` directory, `0600` file,
  lowercase-hex router-hash name, and the same `data_dir` confinement as today.
- **Nothing is advertised.** `specs/support.toml` gains a surface at
  `advertised = false`; `router.version` stays `0.9.62`; `Cargo.toml` and
  `Cargo.lock` stay byte-identical.
- **The predecessors' artifacts are untouched.** Plans 544–547's lanes, guards
  and closure records are not rewritten; corrections live here.

## 5. Scope

### 5.1 In scope

1. A production path that hands out the local router's **current installed**
   RouterInfo — the one the floodfill gate installed — rather than a record
   captured before the gate ran.
2. A decision, recorded as a design decision, between that path and a
   publication-target bootstrap rule. One of the two, not both.
3. The lane that proves a publication reaches a floodfill and is acknowledged.
4. The guard that pins the invariant set above.
5. Documentation, inventory and the full floor.

### 5.2 Out of scope

- The reachability flap (§2). It has its own owner and its own plan.
- Any RouterInfo-kind lookup implementation. If the plan concludes that a
  lookup is required, it **stops** and registers its own successor: a lookup is
  a large surface with its own admission and ban considerations.
- Stock-peer qualification, NAT traversal, public-network admission.

## 6. Required production changes

### 6.1 Export what the gate installed

`export_local_router_info` takes encoded bytes from its caller. The
NTCP2 shared-record path is the only caller, and it runs at startup. The
change is to reach the export from a point where the **installed** record is
the floodfill's, or to re-invoke the existing helper when the floodfill gate
installs a new record — whichever the guard can pin more tightly.

### 6.2 Or: a bootstrap rule for the target selection

If §6.1 is rejected as an operator burden, the alternative is a bounded,
explicitly configured publication target. It must be an operator precondition
in the shape Plan 546 established — never an inference from a peer's behaviour,
and never a relaxed floodfill predicate.

The plan must choose one and record why.

## 7. Design decisions

### 7.1 The capability is the router's own, and the signature proves it

Handing a peer a signed RouterInfo is not a trust transfer: the peer verifies
the signature and the record's own validity. The plan therefore prefers passing
the real record over inventing a capability, and forbids any path that
constructs `caps=f` outside the owning process.

### 7.2 An operator precondition, not a predicate

Plan 546 established the shape: a decision the operator makes explicitly, at
configuration time, is allowed; a decision the router infers from traffic is
not. Any bootstrap rule follows that shape.

### 7.3 The measurement must include the role window

A publication attempted while the floodfill is `Disabled` is refused for a
different reason than one refused for want of a target, and the two must be
distinguishable in the log before the measurement is credited.

## 8. Work packages

| WP | Deliverable |
| --- | --- |
| WP1 | The chosen production path (§6.1 or §6.2), with the invariant set intact |
| WP2 | Retarget and strengthen any guard the change touches; close any fail-open it exposes |
| WP3 | The lane: a publication reaches a floodfill and is acknowledged by a correlated `DeliveryStatus` |
| WP4 | The guard: the rules in §4, with one inverted probe each |
| WP5 | Negative controls: each mutation fires its own rule, tree restored byte-identically |
| WP6 | Documentation, inventory and the full floor |
| WP7 | The measurement: two hosts at minimum, the role window recorded, the acknowledgement counted |

## 9. Failure, cancellation, restart and contention semantics

- A publication that finds no target stays `RefusedNoTarget` and is counted;
  it never blocks the transport loop and never retries blindly.
- A refused or timed-out delivery is abandoned at once, so a cycle moves to its
  next target instead of stalling on one.
- The bootstrap path must add no task, no timer and no listener; it rides an
  existing tick or an existing startup step.
- A missing or stale handed-out record is a counted, typed refusal, never a
  panic and never a silent skip.

## 10. Compatibility and migration

No new configuration key unless §6.2 is chosen, and then it is
`deny_unknown_fields`, defaulted **off**, and refused when the SSU2 service is
off — the shape `ssu2.peer_test` already uses. `router.version`, the inventory's
advertised count and the two manifests are unchanged.

## 11. Required tests

### 11.1 The lane

Black-box over the repository's own types: a floodfill whose record the
publisher holds `caps=f` for is selected, asked, and the acknowledgement is
correlated; a publisher holding `caps="4"` is refused with `RefusedNoTarget` and
the refusal is counted.

### 11.2 The guard

Every rule in §4, each with an inverted probe that fails for its own reason and
a positive control that must not fire.

## 12. Exact verification commands

```text
cargo test --locked -p i2pr-daemon --test <lane> -- --test-threads=1
cargo test --locked -p i2pr-daemon --test <guard> -- --test-threads=1
cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-dependency-direction.sh
```

plus the full 76-step floor before closure.

## 13. Documentation updates

`specs/support.toml` (one surface, `advertised = false`),
`specs/QUALIFICATION-GATES.md` (a new section), `specs/CONFORMANCE.md` (a new
section), `docs/architecture/i2pr-daemon.md` (the publication chapter),
`plans/registry.md` and the network-participation roadmap.

## 14. Acceptance criteria and stop conditions

Accepted when, on a private LAN, a router publishes its RouterInfo to a
floodfill it holds a `caps=f` record for, the attempt is acknowledged by a
`DeliveryStatus` correlated against that attempt, and the audit trail shows the
role window in which it happened — or, if the role was `Disabled` throughout,
the measurement states that the refusal was the role's and not the selection's.

**Stops** if the change would relax the floodfill predicate, mint `caps=f`
outside the owning process, write a secret, or require a RouterInfo-kind
lookup. A lookup is a successor plan, not a work package here.

## 15. Closure evidence required

Commits; the floor table; lane and guard rows; negative controls with the
restoration proof; the requirement-to-evidence matrix; the invariant, failure,
migration and security reviews; limitations, findings, roadmap disposition and
the unblock audit.

## 16. Handoff notes

- The peer test is **not** the blocker any more. It ran 39/39 on the floodfill
  host and is not to be re-litigated here.
- Measure the **role window** at the same time as the publication, or the
  result will be ambiguous between "no target" and "no role".
- A stale handed-out record is indistinguishable from a wrong one by
  inspection; pin the record's own date in the receipt.

## 17. Amendment log

### 17.1 — 2026-11-13, registration

Registered by Plan 547's closure
(`plans/closure/network-participation/547-status.md`), which measured the
floodfill gate reaching `role="Active"` and located the publication-target
bootstrap gap that stopped the acknowledgement. Plan 546 and Plan 547 both stay
registered until their own acceptance is met; this plan owns neither of them.
