# Plan 230 — M6 Java reachability-capability/profile-bootstrap corrective and continuation

Status: **registered-ready-m6-java-reachability-capability-profile-bootstrap-corrective**

> This file replaces the earlier Plan-230 profile-population-timeline attribution
> while preserving the global plan number and path. The earlier registration
> treated `profile_count=0` as an open-ended timing/activity question. Exact-pinned
> Java I2P 2.13.0 source review now exposes a narrower deterministic prerequisite
> that must be tested first.

## 1. Objective

Resolve the exact Plan-229 stop:

```text
P229-C-NOT-EXPLORATORY-ELIGIBLE
main_raw_present=true
main_valid_present=true
profile_present=false
profile_count=0
not_failing_count=0
```

without turning Java-router startup behavior into another open-ended diagnostic
campaign.

Plan 230 MUST first determine whether Router C's signed RouterInfo satisfies the
exact-pinned Java `ProfileManagerImpl.heardAbout(...)` profile-creation
predicate. If the controlled loopback topology itself prevents that predicate
from being true, Plan 230 MAY apply only the stock Java configuration correction
authorized below, re-exchange RouterInfos through the already-proven
authenticated I2NP DatabaseStore path, and require profile creation to occur
naturally.

If that gate passes, Plan 230 MUST continue directly through the already-defined
Plan-229 non-zero exploratory and Plan-228 client-build gates. Do not create a
new plan merely because the next Java-internal prerequisite becomes visible.
The purpose of this pass is to get back to the actual Java↔i2pr qualification
lane or produce one genuinely new, protocol-relevant stop.

The intended path is:

```text
correct P229 reachability/profile evidence
        ↓
prove exact heardAbout() creation predicate from the live RouterInfo
        ↓
conditional stock controlled-topology correction, only if predicate is false
        ↓
ordinary authenticated RI exchange creates Router-C profile naturally
        ↓
non-zero exploratory tunnels install
        ↓
client one-hop tunnels build through C
        ↓
resume frozen Plan-201 destination / reverse-delivery qualification
        ↓
either Java second-family closure or one exact new protocol-relevant boundary
```

## 2. Why the original Plan 230 is superseded

Plan 229 correctly proved that Router C was present and valid in Router A's
main NetDB while A's organizer population remained empty. However, two exact
source facts change the interpretation of that evidence.

### 2.1 The P229 `unreachable` signal is not reachability evidence

The current test-only `P229Probe` maps its `unreachable` field to:

```java
ctx.profileOrganizer().isFailing(routerC)
```

In the exact Java I2P 2.13.0 pin
`9134f808337b401e8e53c73734c81fab04280c9d`,
`router/java/src/net/i2p/router/peermanager/ProfileOrganizer.java` defines
`isFailing(Hash)` as deprecated and unconditionally returns `false`.

Therefore historical `unreachable=false` rows from Plan 229 MUST NOT be used as
proof that Router C advertised or had established Java's reachable capability.
The old row may remain for history, but Plan 230 must stop consuming it as a
gate.

### 2.2 Ordinary `heardAbout()` profile creation is capability-gated

The exact-pinned
`router/java/src/net/i2p/router/peermanager/ProfileManagerImpl.java` states
that `heardAbout()` is the main vector for new profile creation. Its
`shouldCreate(caps)` predicate is:

```java
return caps.indexOf('R') >= 0 &&
       (caps.indexOf('f') >= 0 ||
          ((caps.indexOf('L') < 0 ||
            (!_context.netDb().floodfillEnabled() &&
             _context.bandwidthLimiter().getMaxShareBandwidth() < 128*1024)) &&
           caps.indexOf('E') < 0 &&
           caps.indexOf('G') < 0));
```

Plan 229 measured `caps_has_f`, but did not measure `R`, `L`, `E`, or
`G`, and did not evaluate this predicate.

This is especially material in the present fixture:

- Router A is intentionally floodfill.
- Router C is intentionally non-floodfill.
- Java's default outbound bandwidth is 60 KiB/s and default share is 80%.
- `Router.getBandwidthClass()` therefore normally places a default router at
  the `L` boundary.
- For a floodfill Router A, a non-floodfill Router C advertising `L` does not
  satisfy the branch above even if `R` is present.
- The isolated loopback topology also cannot rely on normal public-network peer
  testing to establish reachability. The pinned `UDPTransport` supports the
  stock property `i2np.udp.status=ok`, and `Router.getCapabilities()` emits
  `R` when the communication-system status is OK.

Thus `profile_count=0` may be a deterministic consequence of the synthetic
reference topology, not a timing problem and not an i2pr protocol defect.

## 3. Registration basis

Retain all Plan-229 facts that remain valid:

- A=service, B=publication, C=transit roles were proven live.
- C is non-floodfill; A/B remain floodfill.
- Router A's effective exploratory settings are the stock small-router
  `1/1/1/1` profile.
- `P200-H-publication-path-passed` proves the authenticated RouterInfo
  DatabaseStore bootstrap path.
- C is present and validates in A's main NetDB.
- `ProfileOrganizer.isSelectable(C)=true` is a useful RI-level selector fact,
  but it does not imply a profile exists.
- `profile_count=0` and `not_failing_count=0` are valid observations.
- helper execution, exploratory install proof, P228 continuation, and the
  lookup/reverse lane were not reached.

Supersede only the interpretation that `ProfileOrganizer.isFailing(C)==false`
proved C reachable.

## 4. Scope and invariants

1. Exact Java pin remains
   `9134f808337b401e8e53c73734c81fab04280c9d`; i2pd pin remains unchanged.
2. No i2pr production Rust changes are authorized by the profile-bootstrap
   corrective itself.
3. No Java source patching, reflection, private-field access, VMComm, public
   I2P, or reseed.
4. No direct `ProfileOrganizer.addProfile`,
   `getOrCreateProfile*` invocation from probes, profile-file fabrication,
   tier/score mutation, or profile-map mutation.
5. No direct Java NetDB store/publish calls from launcher/probes. Existing
   authenticated I2NP RouterInfo exchange remains the sole peer-learning path.
6. No direct tunnel installation and no paired-tunnel policy override.
7. Plan-229 A=service/B=publication/C=transit role split remains.
8. Router A's stock small-router exploratory `1/1/1/1` settings remain.
9. Plan-227 client helper remains one-hop, quantity 1, explicit C, zero-hop
   disabled as already frozen.
10. `netDb.alwaysQuery`, Plan-226 distinct-/24 topology, SAM pivot, timeout
    inflation, and client-NetDB RI injection remain forbidden.
11. Historical P229 `unreachable` is non-authoritative. New Plan-230 evidence
    must use RouterInfo capability bits and communication-system status.
12. Raw Java logs stay scratch-only; durable evidence is bounded typed facts.
13. The frozen Plan-223 reverse-payload window remains 45 seconds.
14. M10 product authority through Plan 215 is untouched.
15. Plan 230 is allowed to continue through the existing P229/P228/P201 gates
    after profile bootstrap succeeds; this continuation is deliberate and
    replaces the old one-new-plan-per-Java-gate pattern.

## 5. Work package A — correct the observation contract

Extend the test-only Java probe/launcher and Rust evidence parser with a
Plan-230 capability snapshot. Do not reuse the P229 `unreachable` field.

For Router C as observed by Router A, record at minimum:

```text
main_raw_present
main_valid_present
selectable
banlisted
caps_has_r
caps_has_u
caps_has_f
caps_has_l
caps_has_e
caps_has_g
bandwidth_tier
profile_present
profile_count
not_failing_count
```

For Router A, record the inputs needed to interpret
`ProfileManagerImpl.shouldCreate()`:

```text
local_floodfill_enabled
local_max_share_bandwidth
local_comm_status
```

For Router C, also expose its own communication-system status and the
capabilities of its current self RouterInfo so a stale pre-correction copy can
be distinguished from the newly published RI.

Add a single derived, test-only fact:

```text
heard_about_creation_eligible=true|false
```

It MUST be computed from the exact predicate above and every input used to
derive it MUST also be present independently in the durable row. The static
checker must contain the predicate shape so a future Java-pin change cannot
silently leave a stale classifier.

Required regression locks:

- `ProfileOrganizer.isFailing` may not satisfy any Plan-230 reachability gate.
- `selectable=true` alone may not satisfy profile eligibility.
- `R` alone may not satisfy profile eligibility.
- a non-floodfill `L` peer observed by a floodfill A is ineligible unless the
  exact alternate branch is actually true.
- `E` or `G` on a non-floodfill C makes the ordinary creation predicate false.
- a stale RouterInfo observed before a capability change may not satisfy a
  post-correction gate.

## 6. Work package B — one baseline predicate run

Commit WP A before external execution. Run one fresh disposable A/B/C
destination-only attempt with the current Plan-229 topology and no new
reachability/bandwidth overrides.

The baseline must record one of:

```text
P230-A-PREDICATE-ELIGIBLE
P230-A-PREDICATE-INELIGIBLE reason=<bounded-reason-set>
P230-A-OBSERVABILITY-GAP
```

Allowed bounded reasons are:

```text
missing-r
has-u
low-bandwidth-l-on-floodfill-observer
has-e
has-g
compound
```

`P230-A-PREDICATE-ELIGIBLE` does NOT mean the profile exists; it means the
RouterInfo is eligible for ordinary `heardAbout()` creation. Continue to WP D
without applying the configuration corrective.

`P230-A-PREDICATE-INELIGIBLE` authorizes only the matching WP C correction
below. Do not wait several minutes hoping an impossible predicate will change.

## 7. Work package C — conditional stock controlled-topology correction

This package is conditional. Apply only the properties justified by WP B.

### C1 — reachability correction

If the baseline lacks `R`, has `U`, or otherwise proves that the isolated
loopback router has not established an OK reachability state, set:

```text
i2np.udp.status=ok
```

for the controlled A/B/C RouterContexts.

This is a stock Java transport configuration path in the exact pin:
`UDPTransport.getReachabilityStatus()` maps the literal `ok` to
`Status.OK`, and `Router.getCapabilities()` emits `R` for an OK
communication-system state.

This property is authorized only inside the disposable loopback fixture. The
harness must prove all SSU2 listeners are still loopback-only and public
reseed/network participation remains disabled.

### C2 — transit-role bandwidth-class correction

If C advertises `L` and that is the exact reason floodfill Router A rejects
ordinary profile creation, configure Router C only with stock bandwidth
properties:

```text
i2np.bandwidth.outboundKBytesPerSecond=128
i2np.bandwidth.outboundBurstKBytesPerSecond=128
```

Retain the normal default share percentage. With the pinned bandwidth-class
thresholds this moves C above `L` through real configured bandwidth rather
than lying with `router.forceBandwidthClass`.

`router.forceBandwidthClass` is explicitly forbidden in Plan 230.

Do not change A's stock small-router exploratory settings. Do not make C
floodfill merely to bypass `shouldCreate()`.

### C3 — unanticipated capability exclusions

If `E` or `G` remains the sole blocker after the authorized C1/C2 correction,
stop with:

```text
P230-C-UNEXPECTED-CAPABILITY-EXCLUSION
```

Do not add another tuning knob inside the same implementation SHA.

## 8. Work package D — prove natural profile bootstrap

After any authorized WP C correction, start fresh disposable RouterContexts.
Do not reuse state from the baseline attempt.

Require:

1. live C self-RI reflects the intended corrected capabilities;
2. A receives the corrected C RouterInfo through the existing authenticated
   I2NP DatabaseStore bootstrap;
3. A's stored C RI byte/hash identity corresponds to that post-correction RI;
4. the exact `heard_about_creation_eligible` predicate is true;
5. C appears in `ProfileOrganizer.selectAllPeers()` /
   `getProfileNonblocking(C)` without any probe-side creation call;
6. `not_failing_count > 0`;
7. C remains selectable, non-banned, non-floodfill.

Use a short bounded in-flight processing wait only after criterion 4 is true.
The old multi-minute profile-population exploration is not authorized.

Terminals:

```text
P230-D-PROFILE-BOOTSTRAP-PASSED
P230-D-ELIGIBLE-BUT-NO-PROFILE
P230-D-RI-NOT-UPDATED
P230-D-OBSERVABILITY-GAP
```

If `ELIGIBLE-BUT-NO-PROFILE` occurs, capture whether the authenticated
DatabaseStore handler reached the stock `heardAbout(key, caps)` call using a
bounded sanitized event/counter. Do not call `heardAbout()` manually from a
probe.

## 9. Work package E — continue the already-proven tunnel path

If WP D passes, continue in the same Plan-230 counted run sequence. Do not
register a successor merely to rerun Plan-229 WP D/E.

Re-run the unchanged gates:

1. Router A installs at least one genuine non-zero inbound exploratory tunnel.
2. Router A installs at least one genuine non-zero outbound exploratory tunnel.
3. the selected path contains ordinary eligible remote peer material; no direct
   install or zero-hop-only inference;
4. start the frozen Plan-227 raw helper;
5. prove both inbound and outbound one-hop client pools through Router C;
6. run the retained Plan-228 build-path observations;
7. require the old `NO-PAIRED-TUNNEL` boundary either to disappear or be
   classified as a contradiction with the newly proven non-zero exploratory
   infrastructure.

Existing P227/P228/P229 parser and secret-redaction rules remain authoritative.

Plan-230 continuation terminals at this stage:

```text
P230-E-EXPLORATORY-NOT-INSTALLED direction=inbound|outbound|both
P230-E-CLIENT-NOT-BUILT direction=inbound|outbound|both
P230-E-PAIRED-TUNNEL-CONTRADICTION
P230-E-TUNNEL-CONTINUATION-PASSED
```

A terminal is the earliest missing stage only.

## 10. Work package F — return to the actual Java second-family lane

If WP E passes, resume the frozen Plan-201 destination qualification on the same
controlled topology instead of inventing a new Java-bootstrap plan.

At minimum re-prove:

```text
Router A/B/C authenticated SSU2 topology
real non-zero tunnel infrastructure
target Standard LS2 answerable on Router B
Java public/raw client session established
nonce-correlated tracked send
ordinary target lookup path
reverse Java -> i2pr raw Destination delivery within 45 s
```

If raw bidirectional destination delivery succeeds, continue into the existing
Java Streaming rows already owned by Plan 201. Do not redesign those drivers.

If the first post-bootstrap failure is an i2pr-visible wire/protocol boundary,
stop with a precise Plan-201-class terminal and hand that concrete protocol
failure back to Plan 201. A new plan is justified only at that point.

If all mandatory Plan-201 Java rows pass, Plan 230 may close the outstanding
Java second-family dependency and unblock Plan 204 convergence; do not require
a ceremonial successor solely to repeat the same evidence.

## 11. Attempt discipline

The implementation should normally use two commits at most before counted
execution:

1. observation-contract implementation (WP A);
2. conditional controlled-topology correction (WP C), only if the baseline
   proves it necessary.

Maximum three counted external attempts per implementation SHA. No
between-attempt tuning. Any executable change resets the counted-attempt budget.

A baseline WP-B observation run is allowed before the conditional corrective.
It is evidence for choosing C1/C2, not a closure run.

Fresh RouterContexts are mandatory after a configuration change.

## 12. Required focused tests

Add/adjust unit rows covering at least:

```text
p230_isfailing_never_authoritative_for_reachability
p230_selectable_alone_does_not_imply_creation_eligible
p230_missing_r_is_ineligible
p230_u_without_r_is_ineligible
p230_nonff_l_peer_on_floodfill_observer_is_ineligible
p230_nonff_r_non_l_non_e_non_g_is_eligible
p230_floodfill_peer_with_r_is_eligible
p230_e_is_ineligible_for_nonff_peer
p230_g_is_ineligible_for_nonff_peer
p230_baseline_ineligible_authorizes_only_matching_fixture_correction
p230_reachability_override_is_loopback_fixture_only
p230_bandwidth_correction_applies_to_transit_c_only
p230_force_bandwidth_class_is_forbidden
p230_stale_pre_correction_ri_cannot_pass_post_correction_gate
p230_eligible_without_profile_maps_to_d_boundary
p230_profile_gate_requires_natural_organizer_membership
p230_no_probe_profile_creation_calls
p230_profile_pass_continues_into_exploratory_gate
p230_tunnel_pass_continues_into_frozen_destination_lane
p230_exactly_one_earliest_terminal
p230_secret_or_unrelated_rows_rejected
```

## 13. Static/source invariants

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh` so it
fail-closes on regressions including:

- P230 still using `ProfileOrganizer.isFailing` as reachability authority;
- missing `caps_has_r/l/e/g/u/f` fields;
- missing exact-predicate regression test;
- probe-side `addProfile`, `getOrCreateProfile`, `heardAbout`, direct
  NetDB `store`, or tunnel-install calls;
- `router.forceBandwidthClass`;
- unconditional `i2np.udp.status=ok` outside the Plan-230 controlled-fixture path;
- bandwidth correction applied to A or B rather than C;
- public/reseed/VMComm enablement;
- lookup/reverse claims before the tunnel continuation gate;
- weakening the frozen 45-second reverse window.

## 14. Verification floor

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
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
cargo test --locked -p i2pr-daemon --test java_tunnel_external p230_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p229_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p228_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
javac <all Plan-230 probes + ControlledRouter against exact staged Java jars>
```

If WP F reaches the Java destination/Streaming matrix, also run the existing
Plan-201/M6 final evidence commands rather than inventing replacements.

## 15. Acceptance criteria

Plan 230 closes successfully only when all applicable criteria below hold.

### Observation correction

1. P229 `isFailing()` is no longer treated as reachability authority.
2. Live RouterInfo capability bits and comm-system status are recorded.
3. The exact pinned `heardAbout()` creation predicate is independently
   reconstructable from durable facts.
4. Baseline predicate result is recorded before any new fixture correction.

### Conditional correction

5. `i2np.udp.status=ok` is used only if baseline evidence proves the
   reachability capability is missing/incorrect in the isolated fixture.
6. C's bandwidth is changed only if the exact low-bandwidth branch blocks
   creation.
7. No force-bandwidth-class or profile injection is used.
8. A post-correction signed C RouterInfo is observed through the normal wire
   bootstrap.

### Natural profile bootstrap

9. The exact creation predicate is true.
10. C enters A's organizer population naturally.
11. No probe/harness method creates or promotes the profile.
12. C remains valid/selectable/non-banned/non-floodfill.

### Continuation

13. Genuine non-zero inbound and outbound exploratory tunnels install.
14. Genuine one-hop client tunnels build through C in both directions.
15. The Plan-228 paired-tunnel boundary is either removed or precisely
    contradicted by live evidence.
16. The frozen destination lane is re-entered.
17. No Java-bootstrap successor is created solely to repeat an already
    authorized next gate.

### Closure / handoff

18. If all mandatory Java destination/Streaming rows pass, mark Java
    second-family M6 qualification passed and unblock Plan 204 convergence.
19. If the pass stops, the terminal names the earliest remaining stage and
    distinguishes reference-fixture behavior from an i2pr-visible protocol
    failure.
20. Registry, roadmap, Plan-201/204 dependency state, and closure evidence are
    updated together.
21. Routine and focused verification pass on the closing implementation SHA.

## 16. Stop/decision table

```text
baseline predicate false
    -> apply only matching C1/C2 stock fixture correction

baseline predicate true + profile absent
    -> instrument stock DatabaseStore -> heardAbout reachability only;
       do not tune timing or create profile manually

profile bootstrap passes
    -> continue directly to exploratory/client gates in this plan

exploratory/client gates pass
    -> continue directly to frozen Plan-201 destination lane

first i2pr-visible protocol failure
    -> stop with exact protocol boundary; Plan 201 owns product correction

all Plan-201 mandatory Java rows pass
    -> Java second-family closure; unblock Plan 204

unexpected E/G or non-predicate Java policy boundary
    -> one exact terminal; no speculative tuning
```

## 17. Closure evidence

Update `plans/closure/mixed-router-interop/230-status.md` with:

- implementation SHA(s);
- exact Java/i2pd pins;
- baseline C capability string and all predicate inputs;
- explicit correction authorization reason;
- before/after C RouterInfo capability evidence;
- proof that the corrected RI traversed the normal authenticated bootstrap;
- natural profile membership evidence;
- exploratory/client tunnel snapshots if reached;
- destination/Streaming evidence if reached;
- one exact earliest terminal or full Java-family pass;
- counted attempt ledger;
- routine/focused verification;
- security/compatibility statement;
- Plan-201/204 unblock audit.

## 18. Registration disposition

```text
plan_229 = passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary
plan_230 = registered-ready-m6-java-reachability-capability-profile-bootstrap-corrective

plan_201 = blocked-pending-plan230-reachability-capability-profile-bootstrap-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan230
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 230-m6-java-reachability-capability-profile-bootstrap-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
m10_product_authority = retained-passed-via-plans-213-214-215
```

## 19. Handoff notes for smaller-model execution

Do not begin by adding timers.

Start with the exact predicate. Add the missing capability observations and
prove what Router C actually advertises. The first implementation question is
not "why didn't a profile appear after N seconds?" It is "would the exact
pinned Java code create one for this RouterInfo at all?"

Do not modify the topology until the baseline row answers that question.

If the baseline says missing `R`, add only the stock reachability override.
If it says the floodfill observer rejects C because C is `L`, add only the
documented C bandwidth configuration. Both may be required; the baseline facts
must justify each one independently.

After correction, insist on a newly published/stored C RouterInfo before
checking the profile. A pre-correction RI must not be accepted.

Once the natural profile appears, stop diagnosing profile creation and move
forward immediately through the existing P229/P228/P201 gates. The goal is to
return to interoperability testing, not to continue studying Java internals.
