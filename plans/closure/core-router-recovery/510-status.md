## Status

**Plan 510 — the managed-tunnel staging path under load, and its
diagnosability — is closed.**

Plan 491 recorded **F4**: an intermittent `i2pcontrol_tunnels`
managed-tunnel staging failure — *"runtime transition failed: manager
configuration rejected"* — that did not reproduce in three isolated runs,
in the `--no-fail-fast` workspace sweep, or in two later floor runs. The
residual risk was named as *"a load-sensitive race in the managed-service
staging path"*, and the failure was undiagnosable because every manager
rejection collapsed into one static label.

**Plan 491's own words named two things, and the plan delivers both.**

- **the race is load-sensitive** — so the path must be exercised *under
  load*, not in isolation; and
- **the failure was undiagnosable** — every manager rejection collapsed
  into one static label, so an occurrence could not be attributed.

What landed:

- **the load-sensitive lane**, `crates/i2pr-daemon/tests/i2pcontrol_tunnels_load.rs`,
  which drives **eight concurrent** managed-tunnel creates against one real
  daemon, each generating a destination identity, building a lease set and
  binding a loopback listener. The load is **real work**, never a sleep;
  every wait is an explicit bounded deadline; the concurrency is bounded.
- **the value-free class**, forty pinned (marker, class) pairs that map the
  daemon's own `InvalidConfig` detail to a short label of letters and
  spaces. A detail that matches no marker keeps today's single generic
  label, so the change can never make a rejection *less* informative than it
  was.
- **the guard**, `crates/i2pr-daemon/tests/managed_tunnel_reason_boundary.rs`,
  which pins the table, the classification's totality, the agreement between
  the pinned table and the production function, the value-freeness of every
  class, the **liveness** of every marker in the daemon's own error
  construction, the `&'static str` signatures, the fixed reason template,
  and the lane's boundedness, concurrency and freedom from sleeps. Every rule
  carries an inverted probe.

**Plan 491's F4 is closed CONDITIONALLY, and the condition is named.** The
original failure did **not** reproduce under the lane. That is not a
shortfall and it is not a pass either: the lane is the plan's contribution
whether or not it reproduces. What the plan *does* deliver unconditionally
is the two things Plan 491 asked for — the path now has a load-sensitive
lane that did not exist before, and the reason now names its class, so the
next engineer does not start where Plan 491 started. The exact future
evidence that would close F4 unconditionally is named under
*Limitations*.

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero** (102 surfaces, 96 experimental, 5 deferred, 1 ready);
`Cargo.toml` and `Cargo.lock` are byte-identical.

**No stock-peer evidence is claimed.** The lane is over the tree's own
deterministic surfaces.

## Commits

| Commit | Subject |
| --- | --- |
| `1dc0328c` | docs(plans): register Plan 510, the managed-tunnel staging path under load |
| `8ef420d7` | feat(daemon): classify the manager rejection, and lane the staging path under load |
| `422d4249` | docs(daemon): record Plan 510's reason classification and load lane |
| `7eaf4624` | test(daemon): keep Plan 510's load lane out of the ephemeral port range |
| `f973bfff` | docs(daemon): re-correct the module-table row for the file Plan 510 changed |
| `<closure>` | docs(plans): close Plan 510 and advance the core-router-recovery roadmap |

## What changed

### The gap

Two separate gaps, both named by Plan 491 and both real.

**The staging path had no load-sensitive lane.** The `i2pcontrol_tunnels`
black-box lane drove the seven lifecycle actions **one at a time**. Nothing
in the tree ever drove two managed-tunnel creates at once against one
shared manager. Plan 491's residual risk was named as a load-sensitive race,
and a load-sensitive race needs a load-sensitive lane to be caught by — the
lane is what a future occurrence would be caught by.

**The rejection was undiagnosable.** `crates/i2pr-daemon/src/i2pcontrol_tunnels.rs`'s
`static_manager_reason` mapped **every** `ServiceTunnelError::InvalidConfig`
to the single label *"manager configuration rejected"*. The detail was
discarded. The daemon's own messages name their class in prose — *"candidate
validation failed"*, *"candidate bind collision"*, *"use_ssl requires a
daemon TLS policy"*, *"Destination group missing"*, *"SAM destination
registry poisoned"*, *"deferred activation"* — but the whole string was
dropped. The runtime-neutral error type the daemon wraps is already well
classified; the daemon's own wrapper was the lossy one.

### The classification

`MANAGER_CONFIG_CLASSES` is a pinned list of forty `(marker, class)` pairs,
and `manager_config_class(detail)` maps a detail to a class.

- The **marker** is a stable phrase that appears *inside* the detail. The
  service id is embedded at the front of most messages — *"canonical
  use_ssl requires a daemon TLS policy"* — so a *prefix* match would have to
  know the id, and a class that depends on the id is not a class. A marker
  is a constant phrase in the daemon's own error construction, so it carries
  no value.
- The **class** is a short label of letters, spaces and hyphens: *"server
  TLS policy missing"*, *"candidate bind collision"*, *"destination registry
  poisoned"*. It never names an id, a socket, a port, a key or a payload.
- The **order** matters: a longer marker that *contains* a shorter one comes
  first, so the more specific class wins. The guard asserts that **no**
  marker is shadowed by an earlier one.
- The mapping is **total**. A detail that matches no marker keeps
  `MANAGER_CONFIG_GENERIC` — today's label, unchanged — so a new rejection
  can never make the operator's view worse, and a new marker can be added
  deliberately with its own guard entry.
- **No value can reach the wire.** `manager_config_class` and
  `static_manager_reason` both return `&'static str`, so the **type system**
  — not a review — forbids a detail from escaping. The reason template is
  fixed: `runtime transition failed: {reason}`, a class plus a prefix.

### The load lane

`crates/i2pr-daemon/tests/i2pcontrol_tunnels_load.rs` is a **separate**
file, so Plan 289's black-box lane stays byte-identical. Its setup
machinery is copied from that lane; the lane adds a new one.

- **Eight concurrent creates.** Each opens its own TLS connection and posts
  its own `create`, so all eight are in flight at once against one real
  daemon and one shared manager. Each owns a distinct name, a distinct
  loopback port and a distinct target destination, so a failure is
  attributable to the staging path rather than to a collision.
- **The load is real work.** Each create generates a destination identity,
  builds a lease set, validates the crypto policy and binds a loopback
  listener. There is no sleep, no simulated delay and no mocked clock
  anywhere in the lane, and the guard enforces that.
- **Every wait is an explicit bounded deadline.** `WIRE_TIMEOUT` is 60
  seconds — wider than Plan 289's 10 seconds, because eight concurrent
  creates each do real work and the deadline must bound the whole lane
  rather than punish one slow create. It is still a deadline, never a sleep.
- **Every create must succeed**, and every staged tunnel is then verified
  present and running, with its own port and its own target. A create that
  reported success but left nothing behind would be a staging defect the
  status string alone would hide.
- **The reject path is exercised, not assumed.** A server kind with `UseSSL`
  and no daemon TLS policy is admitted by the request envelope and by the
  control spec build, and is refused later, while the shared manager stages
  the runtime. That is the exact path F4 failed on. The lane asserts that
  the reason names a class and carries **no value** — no service id, no port,
  no address, no raw detail.
- **A rejection must not poison the path.** A third test refuses one create
  and then creates another on the same daemon, so a rejection that left the
  shared manager unable to stage would be caught.

### The enforcement

`crates/i2pr-daemon/tests/managed_tunnel_reason_boundary.rs` carries **ten
rules** and **seventeen tests**, and every rule has an inverted probe.

| Rule | What it pins |
| --- | --- |
| 1 | the table is non-empty, no marker and no class is listed twice, and no marker is shadowed by an earlier one |
| 2 | the classification is total: an empty detail and an unknown detail both keep the generic label |
| 3 | the pinned table and the production function classify identically |
| 4 | every class carries no value |
| 5 | the manager reason's own arms carry no value either |
| 6 | the manager reason template is fixed |
| 7 | every marker is still live in the daemon's own error construction |
| 8 | the classifier and the reason mapper return `&'static str`, so a detail cannot be borrowed onto the wire |
| 9 | the `InvalidConfig` arm classifies the detail rather than collapsing every detail |
| 10 | the load lane declares a real concurrency bound, drives its creates concurrently, and never sleeps |

Rule 7 is the one that keeps the table honest: a reworded daemon message
would leave a marker matching nothing, and the condition would silently fall
back to the generic label. The guard reports that instead.

### Three things the change deliberately does not do

- **It does not change the staging path's behaviour.** The plan does not
  make staging faster, lazier or more tolerant. It observes it and reports
  it.
- **It does not echo a value.** If a class had to carry one, the class would
  be dropped and the generic label kept. A value is never echoed.
- **It does not change a known class's label.** The reason text is not a
  contract, but it is the operator's view, and changing it silently is worse
  than not shipping it.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| The I2PControl reason **never echoes a value**: no id, no address, no port, no key, no payload, no error detail | the guard's rule 4 (every class), rule 5 (the reason mapper's own arms), rule 6 (the fixed template) and rule 8 (`&'static str`); and the load lane's reject test, which asserts the status carries no service id, no port, no address and no raw detail |
| The **known** classes keep today's behaviour; an **unknown** class keeps the single generic label | the guard's rule 2 (totality) and rule 3 (the table and the production function agree); the guard's own `the_classification_is_total_and_the_generic_label_is_reachable` row |
| The daemon's error type keeps its shape for every caller but the reason | the commit's own diff: the `InvalidConfig` arm's payload is the only change |
| No new configuration key, dependency, task, timer, listener, socket, or production wire change beyond the reason string | the diff; `Cargo.toml` and `Cargo.lock` are byte-identical |
| The declared `router.version` stays `0.9.62`; `caps` and the published RouterInfo are unchanged; the advertised count stays zero | `specs/support.toml` — one `experimental` surface, `advertised = false`; the inventory's advertised count stays zero |
| Every lane the plan adds is **deterministic** | the lane's explicit bounded deadlines and bounded concurrency; the guard's rule 10; thirteen consecutive green runs |
| The load-sensitive lane exists and drives concurrent creates | `crates/i2pr-daemon/tests/i2pcontrol_tunnels_load.rs`, `concurrent_managed_tunnel_creates_all_succeed_under_load` |
| The reject path is exercised rather than assumed | the same file, `a_contradictory_configuration_is_rejected_with_a_value_free_reason` and `the_staging_path_recovers_after_a_rejected_create` |
| The marker list is pinned | the guard's rule 1 |
| The reason strings carry no value | the guard's rules 4, 5, 6 and 8 |
| The existing black-box lane is unchanged | `cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels` — the plan-of-record's own invariant; the lane file is byte-identical to its committed form |
| Plan 491's F4 is closed | this record, **conditionally**; see *Limitations* |

## Commands run

### The routine floor

The closing floor is **121 steps**, run on the closing tree, and it passed
**121 of 121**. The script is `/tmp/i2pr-floor-510-run1.sh`; the log is
`/tmp/i2pr-floor-510-run1.log`. It is Plan 509's 118-step floor plus three
Plan 510 steps (`plan510-managed-staging-load-lane`,
`plan510-managed-reason-boundary` and `plan510-tunnelmanager-black-box`,
the last being the plan-of-record's own unchanged-invariant row).

| # | Step | Result | Seconds |
| --- | --- | --- | --- |
| 1 | `cargo-fmt` | PASS | 3 |
| 2 | `cargo-check` | PASS | 1 |
| 3 | `cargo-build-siblings` | PASS | 0 |
| 4 | `cargo-build-static-fixture` | PASS | 1 |
| 5 | `cargo-copy-static-fixture` | PASS | 0 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 0 |
| 7 | `cargo-test-workspace` | PASS | 1146 |
| 8 | `cargo-clippy` | PASS | 20 |
| 9 | `cargo-doc` | PASS | 17 |
| 10 | `cargo-test-doc` | PASS | 4 |
| 11 | `check-dependency-direction` | PASS | 0 |
| 12 | `check-global-plan-number-uniqueness` | PASS | 0 |
| 13 | `check-adr-number-uniqueness` | PASS | 0 |
| 14 | `check-portable-service-tunnel-api` | PASS | 0 |
| 15 | `check-portable-service-tunnel-consumer` | PASS | 6 |
| 16 | `planning-tests` | PASS | 7 |
| 17 | `check-runtime-boundaries` | PASS | 1 |
| 18 | `check-console-boundaries` | PASS | 1 |
| 19 | `check-console-browser-security` | PASS | 0 |
| 20 | `check-tooling-inventory` | PASS | 0 |
| 21 | `check-license-metadata` | PASS | 0 |
| 22 | `check-managed-app-private-client-seams` | PASS | 0 |
| 23 | `check-router-readiness-contract` | PASS | 0 |
| 24 | `check-router-readiness-contract-self` | PASS | 0 |
| 25 | `check-core-router-recovery-contract` | PASS | 0 |
| 26 | `check-core-router-recovery-contract-self` | PASS | 0 |
| 27 | `plan449-ssu2-preflight-self` | PASS | 0 |
| 28 | `plan495-peer-eligibility-self` | PASS | 0 |
| 29 | `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| 30 | `plan476-gate-binding` | PASS | 1 |
| 31 | `plan476-gate-binding-self` | PASS | 0 |
| 32 | `check-managed-app-package-boundary` | PASS | 0 |
| 33 | `check-managed-app-package-boundary-self` | PASS | 0 |
| 34 | `check-managed-app-policy-boundary` | PASS | 0 |
| 35 | `check-managed-app-policy-boundary-self` | PASS | 0 |
| 36 | `check-managed-app-secured-sandbox` | PASS | 0 |
| 37 | `check-managed-app-secured-sandbox-self` | PASS | 0 |
| 38 | `check-service-tunnel-boundaries` | PASS | 0 |
| 39 | `check-managed-app-gateway-boundary` | PASS | 0 |
| 40 | `check-managed-app-manager-boundary` | PASS | 0 |
| 41 | `check-m11-per-epoch-composition` | PASS | 0 |
| 42 | `check-service-anonymity-boundaries` | PASS | 0 |
| 43 | `check-fixture-manifest` | PASS | 0 |
| 44 | `check-ntcp2-vectors` | PASS | 0 |
| 45 | `check-ssu2-vectors` | PASS | 0 |
| 46 | `check-i2cp-vectors` | PASS | 13 |
| 47 | `check-ntcp2-interoperability` | PASS | 0 |
| 48 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 49 | `check-m11-transit-boundaries` | PASS | 2 |
| 50 | `check-m11-transit-qualification-evidence` | PASS | 1 |
| 51 | `check-sam-acceptance-evidence` | PASS | 0 |
| 52 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 53 | `check-i2cp-acceptance-evidence` | PASS | 0 |
| 54 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 55 | `check-els2-type11-transcript-boundary` | PASS | 0 |
| 56 | `check-els2-live-lane-evidence` | PASS | 2 |
| 57 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 58 | `check-outproxy-request-path` | PASS | 0 |
| 59 | `check-outproxy-wire-lane-evidence` | PASS | 1 |
| 60 | `check-config-secret-hygiene` | PASS | 0 |
| 61 | `check-workflow-validity` | PASS | 0 |
| 62 | `check-floodfill-type5-serve` | PASS | 0 |
| 63 | `check-service-tunnel-acceptance-evidence` | PASS | 1 |
| 64 | `check-exploratory-tunnel-evidence` | PASS | 0 |
| 65 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 66 | `check-destination-tunnel-evidence` | PASS | 0 |
| 67 | `check-streaming-tunnel-evidence` | PASS | 5 |
| 68 | `check-m6-mixed-router-acceptance-evidence` | PASS | 4 |
| 69 | `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| 70 | `check-m12-floodfill-boundaries-self` | PASS | 1 |
| 71 | `ntcp2-harness-tests` | PASS | 0 |
| 72 | `plan488-mesh-lane` | PASS | 1 |
| 73 | `plan488-evidence-deterministic` | PASS | 0 |
| 74 | `plan489-mesh-lane` | PASS | 0 |
| 75 | `plan489-evidence-deterministic` | PASS | 0 |
| 76 | `plan490-mesh-lane` | PASS | 0 |
| 77 | `plan490-evidence-deterministic` | PASS | 0 |
| 78 | `plan490-ssu2-transport-suite` | PASS | 11 |
| 79 | `plan490-transport-boundary-guard` | PASS | 0 |
| 80 | `plan491-mesh-lane` | PASS | 0 |
| 81 | `plan491-evidence-deterministic` | PASS | 0 |
| 82 | `plan491-boundary-guard` | PASS | 1 |
| 83 | `plan491-proto-suite` | PASS | 1 |
| 84 | `plan491-netdb-suite` | PASS | 95 |
| 85 | `plan491-client-suite` | PASS | 29 |
| 86 | `plan492-posture-suite` | PASS | 0 |
| 87 | `plan492-m11-lane` | PASS | 5 |
| 88 | `plan502-transit-composition-lane` | PASS | 0 |
| 89 | `plan502-transit-composition-unit` | PASS | 0 |
| 90 | `plan502-transit-boundary-self` | PASS | 0 |
| 91 | `plan496-private-lan-profile` | PASS | 0 |
| 92 | `plan496-ntcp2-activation-gate` | PASS | 1 |
| 93 | `plan497-routing-consumer-lane` | PASS | 0 |
| 94 | `plan497-netdb-preferred-order` | PASS | 1 |
| 95 | `plan497-daemon-participation-unit` | PASS | 2 |
| 96 | `plan498-introducer-handoff-lane` | PASS | 1 |
| 97 | `plan498-introducer-mesh-lane` | PASS | 1 |
| 98 | `plan498-bootstrap-resolver-row` | PASS | 25 |
| 99 | `plan498-runtime-relay-accessor` | PASS | 13 |
| 100 | `plan503-version-eligibility-live` | PASS | 1 |
| 101 | `plan503-version-eligibility-unit` | PASS | 0 |
| 102 | `plan504-liveness-boundary` | PASS | 1 |
| 103 | `plan504-liveness-live` | PASS | 0 |
| 104 | `plan504-liveness-unit` | PASS | 0 |
| 105 | `plan505-driver-boundary` | PASS | 1 |
| 106 | `plan505-driver-live` | PASS | 2 |
| 107 | `plan505-driver-unit` | PASS | 1 |
| 108 | `plan506-reconcile-boundary` | PASS | 5 |
| 109 | `plan506-reconcile-live` | PASS | 0 |
| 110 | `plan507-retention-feasibility` | PASS | 1 |
| 111 | `plan507-retention-boundary` | PASS | 1 |
| 112 | `plan507-retention-live` | PASS | 0 |
| 113 | `plan508-creator-retention-feasibility` | PASS | 1 |
| 114 | `plan508-creator-retention-boundary` | PASS | 0 |
| 115 | `plan508-creator-retention-live` | PASS | 1 |
| 116 | `plan509-lookup-target-decision` | PASS | 0 |
| 117 | `plan509-lookup-target-boundary` | PASS | 1 |
| 118 | `plan510-managed-staging-load-lane` | PASS | 0 |
| 119 | `plan510-managed-reason-boundary` | PASS | 1 |
| 120 | `plan510-tunnelmanager-black-box` | PASS | 4 |
| 121 | `cargo-deny` | PASS | 1 |

### The focused verification

Every command below was run **locally** on this host and passed. Nothing in
this record is CI evidence.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS (in the floor) |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels_load -- --test-threads=1` | PASS — 3 tests; 26 consecutive green runs, each between 0.55 s and 0.83 s |
| `cargo test --locked -p i2pr-daemon --test managed_tunnel_reason_boundary -- --test-threads=1` | PASS — 17 tests |
| `cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels -- --test-threads=1` | PASS — Plan 289's lane, unchanged |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS |
| `python3 -c "import tomllib; tomllib.load(open('specs/support.toml','rb'))"` | PASS — the inventory parses |

### The seven negative controls, verified on the real tree

Each control mutates the **real** tree, runs the **real** lanes, and
restores the tree **byte-for-byte**. The detector distinguishes an
**assertion failure** from a **build failure**: a compiler error, a
"could not compile" / "aborting" / "linking" message, or an abort signal
counts as a build failure, and the test runner's own
`error: test failed, to rerun pass ...` footer does **not**.

| # | Mutation | Failing tests | Build broken? |
| --- | --- | --- | --- |
| A | the `InvalidConfig` arm collapses every detail again | 5 (1 lane, 4 guard) | no |
| B | a class carries a value (`candidate bind collision on 127.0.0.1`) | 7 (guard) | no |
| C | the generic fallback becomes unreachable | 4 (guard) | no |
| D | a construction message is reworded (`candidate bind collision` → `candidate bind clash`) | 4 (guard) | no |
| E | the load lane carries no load (concurrency 8 → 1) | 4 (guard) | no |
| F | the load lane sleeps | 3 (guard) | no |
| G | a marker's spelling drifts from the construction | 5 (1 lane, 4 guard) | no |

None of the seven is vacuous: every one changed the code and changed an
**observable**, and none broke the build. A control that breaks the build is
weaker than one that breaks an assertion, so the build-failure count is
reported separately and is zero for all seven.

One of the counts deserves a note, because not every failure is an
independent detection. Under controls **A, B, D and E** one or more
**inverted probes** also fail: the probe's own anchor is text the control
just changed, so the probe's asserted mutation no longer applies and the
probe reports *"the mutation must apply"*. That is the probe behaving
correctly — it refuses to pass silently when its anchor is gone — and it is
a *secondary* effect of the control, not a second detection. The primary
detection in every case is `the_landed_tree_holds_every_rule`, together
with `every_rule_is_reachable_and_the_tree_holds` and
`the_guard_ignores_comments_and_test_blocks`, which all fail because the
mutated tree now violates a rule. The controls that change the
**production** behaviour — A and G — break the **load lane** as well as
the guard, and neither of them touches a probe anchor.

Controls A and G are the two that break the **production** behaviour, and
both break the **load lane** as well as the guard: the lane's reject test
asserts that the reason names its class, so a collapsed arm (A) or a drifted
marker (G) is caught by the lane and not only by the guard. That is the
point of having a black-box lane and not only a static guard.

The tree was restored byte-for-byte after every control, and the restore is
**asserted** by the harness (a SHA-256 comparison per file), not assumed.

### Weaknesses found while building the controls and the guard

1. **The control detector's build-failure matcher was too loose on its first
   run.** The first run reported *"build failed: True, 0 failing tests"* for
   all five controls it reached, and the run was **inconclusive**. The
   matcher had matched the test runner's `error: test failed, to rerun pass
   ...` footer, so every control's count was zero and the build-failure
   flag was spurious. It was tightened to accept only a compiler error with
   a code (`error[E0308]:`), the driver's own "could not compile" /
   "aborting" / "linking" messages, or an abort signal — and the controls
   were re-run from scratch. **This is the second time this repository has
   recorded this trap**; the first was Plan 508's.
2. **One control's anchor was not unique.** The sleep control anchored on
   `let token = authenticate(address).await;`, which occurs **three** times
   in the lane. The harness asserts that every anchor is unique, so the
   control failed loudly instead of mutating the wrong site; the anchor was
   retargeted at `let mut handles = Vec::with_capacity(LOAD_CONCURRENCY);`.
3. **The lane's first draft used an invalid destination body.** The
   generator produced a 52-character upper-case Base32 label, but the
   canonical alphabet is lower-case `a-z2-7` **and** the trailing four bits
   of the 52nd character must be zero or the label does not decode. The
   lane's first run failed with *"destination reference malformed"*, which
   is the failure working. The generator was corrected and the lane's
   distinctness moved to the first two characters.
4. **The guard's dead-code warning.** The first build of the guard carried
   an unused `check_body_contains` helper, which `-D warnings` rejects. The
   helper was removed rather than allowed to sit as dead code.

## Invariant review

| Invariant | Held? | Evidence |
| --- | --- | --- |
| The I2PControl reason never echoes a value: no id, no address, no port, no key, no payload, no error detail | yes | the guard's rules 4, 5, 6 and 8; the load lane's reject test |
| The **known** classes keep today's behaviour exactly; an **unknown** class keeps the single generic label | yes | the guard's rules 2 and 3; the guard's own totality row |
| The daemon's error type keeps its shape for every caller but the reason | yes | the commit's diff: the `InvalidConfig` arm's payload is the only change |
| No new configuration key, dependency, task, timer, listener, socket, or production wire change beyond the reason string | yes | the diff; `Cargo.toml` and `Cargo.lock` are byte-identical |
| The declared `router.version` stays `0.9.62`; `caps` and the published RouterInfo are unchanged; the advertised count stays zero | yes | the support inventory; no production file outside the reason mapping is touched |
| Every lane the plan adds is deterministic: it either passes always or fails always, and it fails with a named reason | yes | the lane's explicit bounded deadlines; the guard's rule 10; 26 consecutive green runs at 0.55–0.83 s |
| Plan 289's black-box lane is unchanged | yes | the lane file is byte-identical to its committed form; `plan510-tunnelmanager-black-box` is a floor step |

## Failure review

| Condition | Behaviour |
| --- | --- |
| The manager refuses a configuration whose detail matches no marker | the operator sees *"manager configuration rejected"* — today's label, unchanged. The change can never make a rejection less informative |
| The manager refuses a configuration whose detail matches a marker | the operator sees the class, and never the id, the socket, the port or the underlying error |
| A daemon message is reworded | the guard's rule 7 fires and reports the stale marker, instead of the condition silently falling back to the generic label |
| A class is added with a duplicate label | the guard's rule 1 fires: a class must identify exactly one condition |
| A marker is added that an earlier marker already matches inside | the guard's rule 1 fires: the new marker would never reach its own class |
| The staging path cannot carry concurrent load | the load lane fails, naming the create, its port and its target. This is the lane's whole purpose |
| A rejected create leaves the shared manager unable to stage | the lane's recovery test fails. A rejection that poisoned the path would turn every later create into the same undiagnosable failure Plan 491 recorded |
| The lane's concurrency bound is reduced, or a sleep is introduced | the guard's rule 10 fires: the lane would no longer be a load lane |

## Migration review

None. Nothing is persisted, nothing is renamed, nothing is reinterpreted, and
no configuration key changes meaning. The plan adds no data format, no
protocol message and no wire change beyond the I2PControl error's `reason`
string for a manager rejection — and that string is pinned by no fixture, no
spec, no contract crate and no other test. The **known** classes keep
today's labels; only previously-collapsed rejections gain a label.

## Security review

- **No new attack surface.** The plan adds no task, no timer, no listener,
  no socket, no configuration key and no dependency. The router's exposure
  is unchanged.
- **The change is strictly less informative about the daemon's internals
  than the detail it replaces.** The daemon's `InvalidConfig` detail names
  the service id, sometimes the socket address, and always the underlying
  error. The class names none of them. The operator's view is a *coarser*
  label than the daemon's internal message, which is the direction the
  repository's secret-hygiene rule wants.
- **The type system enforces it.** `manager_config_class` and
  `static_manager_reason` both return `&'static str`, so a detail cannot be
  borrowed onto the wire even by a future edit: the compiler refuses it.
  The guard's rule 8 pins that signature.
- **No secret is logged, printed or serialised.** The plan adds no
  diagnostic, no receipt and no counter. The lane and the guard read
  source files and drive the daemon over loopback; neither retains a
  service id beyond a test's own local variables.
- **The guard reads only the repository's own source.** It opens no socket
  and reaches no network.
- **The lane binds loopback only.** The daemon under test binds
  `127.0.0.1:0`, and every client tunnel binds a loopback listener. No
  non-loopback bind is introduced.

## Documentation and operations

- `docs/architecture/i2pr-daemon.md` — the managed-tunnel staging path's
  reason classification section: the marker/class table, why the match is by
  marker rather than by prefix, why the order matters, the totality of the
  mapping, the `&'static str` guarantee, the load lane, and the guard.
- `specs/CONFORMANCE.md` — the Plan 510 section, stating what is claimed and
  what is not, including the conditional close.
- `specs/support.toml` — one `experimental` surface,
  `service-tunnels.managed-staging-reason-classification`, with
  `advertised = false`. The inventory is now 102 surfaces, 96 experimental, 5
  deferred, 1 ready, and the advertised count stays zero.
- `docs/architecture/i2pr-daemon.md` — the module-table row for
  `src/i2pcontrol_tunnels.rs` re-corrected to **9 483**, its true value
  after this plan's classification landed (Plan 508's convention: a plan
  re-corrects the row for the production file it changes).
- `plans/registry.md` and `plans/subsystems/core-router-recovery-roadmap.md`
  — the Plan 510 rows, closed.

## Limitations

1. **The original failure did not reproduce.** The lane drove eight
   concurrent creates against a real daemon, repeatedly and deterministically,
   and the staging path carried every one. The specific failure Plan 491
   recorded — *"runtime transition failed: manager configuration rejected"* —
   was therefore **not** a defect this plan found and fixed; it was a
   symptom of something the plan did not identify.

   **The condition on the close.** Plan 491's F4 is closed on the
   **conditional** basis that (a) the staging path is now exercised under
   real concurrent load by a lane that did not exist before, and (b) a
   future occurrence now names its class instead of collapsing into one
   label, so it can be attributed without re-deriving the path.

   **The exact future evidence that would close it unconditionally** is: a
   reproduction of the failure — on any host, under any load — together with
   the class the new reason reports. If the class is *"candidate bind
   collision"*, the cause is the staging path's port ownership; if it is
   *"candidate validation rejected"*, the cause is the candidate set's
   structural validation; if it is *"server TLS policy missing"*, the cause
   is the TLS policy. The three are different defects with different fixes,
   and the new reason is what tells them apart. Until such a reproduction
   exists, F4 is closed conditionally and **not** claimed as a fixed defect.
2. **The lane is bounded, and a bound is a limit.** Eight concurrent creates
   is a real load on one shared manager — eight destination identities, eight
   lease sets, eight listeners, eight staging transactions contending on one
   op lock — but it is not an unbounded overload, and the lane says so. The
   concurrency bound is a deliberate property, not an oversight: the lane
   must be deterministic, and an unbounded create count is not.
3. **The lane exercises the daemon's own staging path, not a peer-visible
   one.** No stock-peer evidence is claimed, and none is possible here: the
   managed-tunnel staging path is the router's own composition work.
4. **The classification covers the daemon's `InvalidConfig` arm only.** The
   runtime-neutral error type the daemon wraps is already well classified
   (`ExceedsCeiling`, `ContradictoryOptions`, `DuplicateId`,
   `DuplicateListener`), and the other three arms of `static_manager_reason`
   were already value-free labels. The plan did not extend the
   classification to any other error type, and it did not need to.
5. **A message the daemon never constructs is not classified.** The forty
   markers are the forty phrases the daemon's own source constructs. A
   *future* message with a new class needs a new marker and a guard entry —
   and until it has one, it keeps the generic label, which is the correct
   behaviour.

## Findings by severity

### Medium

- **None.** The plan found no defect of Medium severity or above, and it did
  not close one: the failure Plan 491 recorded was **not** reproduced, and
  the plan does not claim it fixed one.

### Low

- **The load lane's fixed port band overlapped the operating system's
  ephemeral range**, and the controls surfaced it as a 5.4-second wobble in
  a lane that otherwise ran in 0.57 seconds. The band was
  `41_000 + pid % 5_000` — 41 000 to 45 999 — and this host's ephemeral
  range is `32768–60999`, so an unrelated outbound connection on the same
  host can be handed one of the lane's ports. The control's
  `await_port_release` then waits up to two seconds per affected name for
  the port to free before the staging binds. The wait is bounded, so the
  lane still passes — but its *duration* depended on unrelated host traffic,
  which is exactly the non-determinism a load lane must not have.
  **Fixed in this plan** (`7eaf4624`): the band moves to
  `20_000 + pid % 5_000` (20 000–24 999), below the ephemeral range and
  still clear of Plan 289's lane's band. 26 consecutive runs afterwards
  ran between 0.55 s and 0.83 s.
- **The control harness's build-failure detector was too loose on its first
  run**, and the first control run was consequently inconclusive. See
  *Weaknesses* above. Fixed before the controls were re-run.
- **One control's anchor was not unique** and the harness caught it rather
  than mutating the wrong site. See *Weaknesses* above.
- **The lane's first draft used an invalid destination body** — an
  upper-case Base32 label, and a 52nd character whose trailing bits were not
  zero — and the lane's first run failed with *"destination reference
  malformed"*. That is the failure working. Corrected.

### Recorded, not defects

- **Plan 289's own lane has the same ephemeral-range exposure.** Its band is
  `33_000 + pid % 5_000`, which is inside `32768–60999` on this host. The
  plan-of-record for Plan 510 requires that lane to stay unchanged, so the
  exposure is **recorded here for a successor plan** and not fixed under
  this number. It is a test-harness robustness issue, not a product defect:
  the wait is bounded and the lane passes.
- **The daemon deep-dive's module-table line counts are stale for 32 of the
  63 rows that carry one.** Pre-existing drift, recorded and not fixed: it
  is a separate concern with its own plan-of-record, and correcting
  thirty-two unrelated rows here would bury the change. Plan 508
  established the convention that a plan re-corrects the row for the
  production file it changes, and Plan 510 follows it: the
  `src/i2pcontrol_tunnels.rs` row is now **9 483**, its true value
  (`f973bfff`). The drift therefore fell from **33 to 32**, measured by
  comparing every one of the 63 counted rows against its file.
- **Plan 491's closure record is not rewritten.** F4's discharge lives here
  and in the registry row, as the repository's rule requires.
- **The plan of record is not rewritten.** The plan-of-record's §6
  production-change table says *"one file, one function, one table"*, and
  that is what shipped: one production file, one function's arm, one table.
  The lane and the guard are new files, which the plan-of-record's §8
  work packages (WP2, WP3) already provide for.

## Roadmap disposition

The core-router-recovery roadmap's Plan 510 row is **closed** as
`passed-managed-tunnel-staging-under-load`, and its status sentence names
the two halves. Plan 491's **F4** is **discharged conditionally**: the path
now has a load-sensitive lane and a diagnosable reason, and the registry's
row says so, with the condition and the future evidence named.

## Unblock audit

Plan 491's closure record listed F4 as *"recorded, not fixed"* and named the
owner and the reason it needed a plan-of-record. Plan 510 is that
plan-of-record, and F4 is now **discharged conditionally**. No other
registered plan lists Plan 491's F4 as a hard dependency, so no other row
moves.

The plans that remain blocked are blocked on **external environments** and
are unaffected by this plan: Plans 433, 435, 436, 437, 438, 439, 493 and
494. Plan 436's source-side work was discharged by Plan 502.

The source-side candidates that remain on this line are named in the plan of
record's §14.3 and are unaffected: Plan 491 **F2** (no stock-peer evidence,
which needs Plan 493's authorized two-host topology) and **F5** (the
comparator's four-component ceiling, recorded and not fixed),
Plan 487 **F3** (the transit-hop failure detection, which needs its own
plan-of-record) and Plan 497 limitation 5's **read** half (which is bound to
Plan 433's blocked composition).
