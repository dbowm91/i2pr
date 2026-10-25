## Status

`passed-live-dsm-version-gate-and-peer-eligibility-receipt`

Plan 503 is closed. The router's **second** API-version obligation — *minimum
floodfill peers will send DSM to* — is now enforced on the **live** paths, and
Plan 491's peer-eligibility receipt is a product the running router actually
produces.

**Plan 491 F1 is discharged.** Its closure record said the receipt log was
not produced by a live product loop. It is now: the live tunnel-build gate and
the live DSM gate each record a receipt into a bounded, live-owned log that the
daemon's inspection plane publishes.

**Plan 491's DSM obligation was not merely unobservable — it was not enforced.**
That is the larger finding, and it is the reason this plan exists. Plan 491's
closure record §11 said: *"The decisions are enforced live — the two
enforcement points refuse at the point of use."* That was true for the
tunnel-build half and **false for the DSM half**. The typed, counted DSM
refusal Plan 491 built (`crates/i2pr-daemon/src/netdb_tunnels.rs:790`
`begin_publication`) had **zero production callers**, because
`NetDbTunnelCoordinator` is never constructed — `crates/i2pr-daemon/src/lib.rs`
states in so many words that Plan 433 owns that composition and is registered
`blocked`. The router's two live `DatabaseStore` sends selected floodfills by
XOR distance alone and never consulted `dsm_target_eligibility`. The router was
publishing its RouterInfo and its LeaseSet2 to floodfills that publish no
version, an uncomparable version, or a version below `0.9.62` — the exact
under-compliance Plan 491 exists to prevent. **Plan 491's closure record is not
rewritten**; the correction lives here and in the registry's row.

**Plan 491 F3 is discharged and verified.** It recorded that
`scripts/check-runtime-boundaries.sh` had no `i2pr-netdb` section. Plan 497's
WP5 added one, and it scans the whole crate directory, so Plan 491's
`version.rs` and `selection.rs` are inside its scope. Plan 503 **verified** that
with negative controls rather than taking it on trust (see the evidence matrix).

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical.

**No stock-peer evidence is claimed.** Plan 491 F2 (no stock-peer evidence for
the floors) needs the authorized two-host topology Plan 493 owns. Plan 491 F4
(the intermittent `i2pcontrol_tunnels` staging failure) belongs to the
I2PControl line. Neither is closed here.

## Commits

| Commit | Subject |
| --- | --- |
| `0f6868c6` | docs(plans): register Plan 503, the live DSM version gate and the live receipt |
| `c8d5d183` | docs(plans): correct Plan 503's receipt-identity claim |
| `3a4a8657` | chore(daemon): declare the version_eligibility module for Plan 503 |
| `0cc48015` | feat(netdb): the shared version-gated floodfill selection |
| `a968d29e` | feat(daemon): the live peer-eligibility receipt owner and its seam |
| `671a09b1` | feat(daemon): the live DSM version gate and the live peer-eligibility receipt |
| `bb6b4335` | docs(plans): the Plan 503 conformance, deep-dives and support count |
| `dd91d1c3` | test(daemon): the Plan 503 boundary rules and the live eligibility lane |
| `d63749eb` | docs(plans): correct Plan 503's WP6 to the guard rules that landed |
| `<closure>` | docs(plans): close Plan 503 and advance the network-participation roadmap |

## What changed

### The shared version-gated floodfill selection — `crates/i2pr-netdb/src/lookup_policy.rs`

Plan 491 built one version-gated selection —
`PublicationCoordinator::nearest_eligible_floodfills` — and it had no
production caller. Plan 503 puts the gate in the crate that owns the ordering,
so the closed enforcement point and the live paths are one implementation:

- `nearest_floodfill_candidates` is the shared ordering core, extracted from
  `select_floodfill_candidates` with the target exclusion moved into the
  caller's `excluded` slice.
- `select_floodfill_candidates` keeps its signature, its set, its order, its
  truncation and its exclusion semantics, and delegates to the core.
- `VersionGatedFloodfillSelection` and `select_version_gated_floodfills` are
  new. The gate applies `dsm_target_eligibility(published_version_of(record))`
  to each nearest candidate and counts every refusal by reason.

**The examined count is the identity the gate can carry exactly.** It counts
only the candidates whose stored record could be read, so every examined
candidate is either an accepted target or a refusal counted by reason, and
`examined == accepted + refused` holds. The unreadable branch is unreachable
today — the candidate list is derived from the same store the records are read
from, and the store is borrowed immutably for the whole selection — but it is
written so the identity cannot drift if that ever changes.

### The publication coordinator's refactor — `crates/i2pr-netdb/src/publication.rs`

`PublicationCoordinator::nearest_eligible_floodfills` now delegates to the
shared selection with the local RouterHash as both the routing key and the
exclusion — the shape the hand-rolled selection always used. Its public
behaviour is unchanged: same targets, same refusal counts, same ordering. The
pre-existing test was not edited and still passes.

### The peer-eligibility decision surface — `crates/i2pr-daemon/src/version_eligibility.rs` (new)

Plan 491's receipt type lived inside the participation driver, and only the
driver's own selection methods produced it — methods with only test callers.
So no running router ever produced a receipt: the shape existed on paper and
nowhere else.

The new module owns the surface: `VersionDecisionKind`,
`VersionEligibilityReceipt`, `version_receipt_digest`, the two constructors,
and `VersionEligibilityOwner` — a bounded log (a `VecDeque` with an explicit
cap, the oldest entry evicted at the cap) behind an internal
`std::sync::Mutex`, so the shared handle is a plain `Arc` and the lock is held
only for the length of a push or a clone, never across an `.await`.

The receipt carries **counts and stable reason labels only**: no peer hash, no
address, no key, no version string, no payload.

### The driver keeps its receipts — `crates/i2pr-daemon/src/netdb_participation.rs`

The moved types are re-exported, so Plan 491's public paths
(`i2pr_daemon::netdb_participation::VersionEligibilityReceipt`) and its lane are
unchanged. The driver's own emission goes through the shared constructor, so
its receipts are byte-identical **by construction** rather than by coincidence:
the Plan 491 evidence artifact regenerates byte-identical after the move, and
an in-crate golden test pins the digests.

### The coordinator's three recording points — `crates/i2pr-daemon/src/destination_tunnels.rs`

`DestinationTunnelCoordinator` gains an optional
`Arc<VersionEligibilityOwner>` and three recording points:

| decision | method | live call site |
| --- | --- | --- |
| tunnel-build gate | `destination_peer_candidates` | `service_product.rs:5494` (the 250 ms group tick) and `service_product.rs:6716` (startup provisioning) |
| DSM gate, LeaseSet2 | `version_gated_candidate_hashes` | `service_product.rs:8409` |
| DSM gate, creator RouterInfo | `version_gated_publication_floodfills` | `service_product.rs:6513` |

A coordinator without an owner records nothing: the controlled helper and the
integration lanes construct one without the owner and are unaffected.

### The two live DSM paths — `crates/i2pr-daemon/src/service_product.rs`

Both live `DatabaseStore` sends now consult the shared gated selection:

- the **creator RouterInfo** publication replaces its hand-rolled XOR sort. The
  routing key (our own router hash) and the own-hash exclusion are unchanged,
  and the three-floodfill bound is unchanged. The candidates are now considered
  within the policy's ceiling and the **eligible** ones are kept, so a
  below-floor floodfill can no longer consume one of the three publication
  slots.
- the **service LeaseSet2** publication replaces `candidate_hashes` with the
  gated equivalent. The routing key, the target exclusion and the
  three-floodfill bound are unchanged.

An empty eligible set fails closed through the paths that already existed: the
creator publication is skipped with a debug transition, and the LeaseSet2
publication releases its reserved intent and returns the existing typed
request-construction failure. A refused floodfill is a **refusal**, not a
delivery failure, and it is counted by reason.

### The observation seam — `crates/i2pr-daemon/src/i2pcontrol_inspection.rs`

`InspectionHandles` gains `version_live`, a `Mutex<Option<Arc<...>>>`, with
`publish_version_eligibility` and `version_eligibility` following the existing
`publish_*` / `*()` pattern exactly. The read is cheap and non-async: the
`Arc` is cloned out from under the slot lock and the lock is released before
the owner is returned. An unpublished slot is a gap, never an empty receipt log.

### The composition — `crates/i2pr-daemon/src/lib.rs`

The owner reaches the coordinator **without a signature change**: the service
product creates it, hands it to the coordinator, keeps it, and exposes it
through `ServiceProduct::version_eligibility`; the composition publishes it
into the inspection seam, so the running router's decisions are observable
through the plane that already publishes the transit posture.

### The controls

`crates/i2pr-daemon/tests/version_floor_boundary.rs` gains the rules that would
have caught Plan 491's gap, and `crates/i2pr-daemon/tests/version_eligibility_live.rs`
is a new out-of-crate lane.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| The shared gated selection refuses a versionless, an uncomparable and a below-floor floodfill, counting each under its own reason | `crates/i2pr-netdb/src/lookup_policy.rs` — the four gate tests; `cargo test --locked -p i2pr-netdb --all-targets` |
| `examined == accepted + refused` on the DSM gate | `crates/i2pr-netdb/src/lookup_policy.rs` `the_examined_count_equals_accepted_plus_refused`; `crates/i2pr-daemon/tests/version_eligibility_live.rs` `the_dsm_gate_satisfies_examined_equals_accepted_plus_refused` |
| The closed enforcement point and the live paths are one implementation, with unchanged behaviour | `crates/i2pr-netdb/src/publication.rs` `the_refactored_publication_selection_matches_the_shared_selection`; the pre-existing publication test, unedited |
| Both live DSM paths consult the gated selection, and neither may fall back to the ungated one | `crates/i2pr-daemon/tests/version_floor_boundary.rs` — the live-publication rules and their inverted probes |
| The live gates record their decisions | `crates/i2pr-daemon/tests/version_floor_boundary.rs` — the recording rules and their inverted probes; `crates/i2pr-daemon/tests/version_eligibility_live.rs` — the receipt assertions |
| The receipt's counts are the decision's own counts | `crates/i2pr-daemon/tests/version_eligibility_live.rs` `the_receipt_counts_are_the_decision_own_counts` |
| The receipt carries no peer identity | `crates/i2pr-daemon/tests/version_floor_boundary.rs` — the receipt-identity rule, retargeted to the new module with its probe unchanged; `crates/i2pr-daemon/tests/version_eligibility_live.rs` `a_receipt_carries_no_peer_identity` |
| The receipt log is bounded | `crates/i2pr-daemon/tests/version_eligibility_live.rs` `the_receipt_log_is_bounded_and_evicts_the_oldest_entry`, `a_receipt_cap_of_zero_records_nothing` |
| A coordinator without an owner records nothing | `crates/i2pr-daemon/tests/version_eligibility_live.rs` `a_coordinator_without_an_owner_records_nothing` |
| The declared version is not edited | `crates/i2pr-daemon/tests/version_floor_boundary.rs` `the_declared_version_is_not_edited`; Plan 285's pinning test |
| The floors are derived, not hand-maintained | `crates/i2pr-daemon/tests/version_floor_boundary.rs` `the_policy_module_derives_the_floors` |
| No configuration key can lower a floor | `crates/i2pr-daemon/tests/version_floor_boundary.rs` `no_configuration_key_can_lower_a_floor` |
| Plan 491 F3: the `i2pr-netdb` boundary section covers the version module | three negative controls on an isolated copy — a clock, an async function and a socket injected into `crates/i2pr-netdb/src/version.rs` are each reported, exit 1 |
| Nothing is advertised | `specs/support.toml` — 95 surfaces, 0 advertised |
| No new dependency | `git diff --exit-code <base> HEAD -- Cargo.toml Cargo.lock` |

## Commands run

### The routine floor — 102 steps, **102 PASS / 0 FAIL**

Run on the closing tree with `CARGO_BUILD_JOBS=4` (a host property: the daemon's
~60 test binaries do not fit when 16 rustc processes run at once). Log:
`/tmp/i2pr-floor-503-run1.log`. The tree was verified clean before and after
the run, so the floor certified the tree the closure describes.

| Step | Result | Time |
| --- | --- | --- |
| `cargo-fmt` | PASS | 4s |
| `cargo-check` | PASS | 6s |
| `cargo-build-siblings` | PASS | 0s |
| `cargo-build-static-fixture` | PASS | 0s |
| `cargo-copy-static-fixture` | PASS | 0s |
| `cargo-rebuild-plain-fixture` | PASS | 0s |
| `cargo-test-workspace` | PASS | 2229s |
| `cargo-clippy` | PASS | 4s |
| `cargo-doc` | PASS | 5s |
| `cargo-test-doc` | PASS | 4s |
| `check-dependency-direction` | PASS | 0s |
| `check-global-plan-number-uniqueness` | PASS | 0s |
| `check-adr-number-uniqueness` | PASS | 0s |
| `check-portable-service-tunnel-api` | PASS | 0s |
| `check-portable-service-tunnel-consumer` | PASS | 7s |
| `planning-tests` | PASS | 14s |
| `check-runtime-boundaries` | PASS | 1s |
| `check-console-boundaries` | PASS | 1s |
| `check-console-browser-security` | PASS | 0s |
| `check-tooling-inventory` | PASS | 0s |
| `check-license-metadata` | PASS | 0s |
| `check-managed-app-private-client-seams` | PASS | 0s |
| `check-router-readiness-contract` | PASS | 0s |
| `check-router-readiness-contract-self` | PASS | 0s |
| `check-core-router-recovery-contract` | PASS | 1s |
| `check-core-router-recovery-contract-self` | PASS | 0s |
| `plan449-ssu2-preflight-self` | PASS | 0s |
| `plan495-peer-eligibility-self` | PASS | 0s |
| `plan495-peer-eligibility-profile-drift` | PASS | 0s |
| `plan476-gate-binding` | PASS | 0s |
| `plan476-gate-binding-self` | PASS | 1s |
| `check-managed-app-package-boundary` | PASS | 0s |
| `check-managed-app-package-boundary-self` | PASS | 0s |
| `check-managed-app-policy-boundary` | PASS | 0s |
| `check-managed-app-policy-boundary-self` | PASS | 0s |
| `check-managed-app-secured-sandbox` | PASS | 0s |
| `check-managed-app-secured-sandbox-self` | PASS | 0s |
| `check-service-tunnel-boundaries` | PASS | 0s |
| `check-managed-app-gateway-boundary` | PASS | 0s |
| `check-managed-app-manager-boundary` | PASS | 0s |
| `check-m11-per-epoch-composition` | PASS | 1s |
| `check-service-anonymity-boundaries` | PASS | 0s |
| `check-fixture-manifest` | PASS | 0s |
| `check-ntcp2-vectors` | PASS | 0s |
| `check-ssu2-vectors` | PASS | 0s |
| `check-i2cp-vectors` | PASS | 5s |
| `check-ntcp2-interoperability` | PASS | 0s |
| `check-constrained-host-lane-boundary` | PASS | 0s |
| `check-m11-transit-boundaries` | PASS | 3s |
| `check-m11-transit-qualification-evidence` | PASS | 1s |
| `check-sam-acceptance-evidence` | PASS | 0s |
| `check-ssu2-acceptance-evidence` | PASS | 0s |
| `check-i2cp-acceptance-evidence` | PASS | 0s |
| `check-i2pcontrol-acceptance-evidence` | PASS | 0s |
| `check-els2-type11-transcript-boundary` | PASS | 0s |
| `check-els2-live-lane-evidence` | PASS | 2s |
| `check-encrypted-service-consumer-caller` | PASS | 0s |
| `check-outproxy-request-path` | PASS | 0s |
| `check-outproxy-wire-lane-evidence` | PASS | 0s |
| `check-config-secret-hygiene` | PASS | 0s |
| `check-workflow-validity` | PASS | 1s |
| `check-floodfill-type5-serve` | PASS | 0s |
| `check-service-tunnel-acceptance-evidence` | PASS | 0s |
| `check-exploratory-tunnel-evidence` | PASS | 0s |
| `check-netdb-tunnel-evidence` | PASS | 0s |
| `check-destination-tunnel-evidence` | PASS | 1s |
| `check-streaming-tunnel-evidence` | PASS | 5s |
| `check-m6-mixed-router-acceptance-evidence` | PASS | 3s |
| `check-m12-floodfill-qualification-evidence-self` | PASS | 0s |
| `check-m12-floodfill-boundaries-self` | PASS | 1s |
| `ntcp2-harness-tests` | PASS | 0s |
| `plan488-mesh-lane` | PASS | 0s |
| `plan488-evidence-deterministic` | PASS | 0s |
| `plan489-mesh-lane` | PASS | 0s |
| `plan489-evidence-deterministic` | PASS | 0s |
| `plan490-mesh-lane` | PASS | 1s |
| `plan490-evidence-deterministic` | PASS | 0s |
| `plan490-ssu2-transport-suite` | PASS | 6s |
| `plan490-transport-boundary-guard` | PASS | 0s |
| `plan491-mesh-lane` | PASS | 1s |
| `plan491-evidence-deterministic` | PASS | 0s |
| `plan491-boundary-guard` | PASS | 0s |
| `plan491-proto-suite` | PASS | 1s |
| `plan491-netdb-suite` | PASS | 95s |
| `plan491-client-suite` | PASS | 29s |
| `plan492-posture-suite` | PASS | 0s |
| `plan492-m11-lane` | PASS | 5s |
| `plan502-transit-composition-lane` | PASS | 0s |
| `plan502-transit-composition-unit` | PASS | 0s |
| `plan502-transit-boundary-self` | PASS | 0s |
| `plan496-private-lan-profile` | PASS | 1s |
| `plan496-ntcp2-activation-gate` | PASS | 0s |
| `plan497-routing-consumer-lane` | PASS | 1s |
| `plan497-netdb-preferred-order` | PASS | 0s |
| `plan497-daemon-participation-unit` | PASS | 2s |
| `plan498-introducer-handoff-lane` | PASS | 2s |
| `plan498-introducer-mesh-lane` | PASS | 0s |
| `plan498-bootstrap-resolver-row` | PASS | 26s |
| `plan498-runtime-relay-accessor` | PASS | 5s |
| `plan503-version-eligibility-live` | PASS | 1s |
| `plan503-version-eligibility-unit` | PASS | 0s |
| `cargo-deny` | PASS | 0s |

The workspace test step took 2229 s and the `i2pr-netdb` suite 95 s; every
other step is a checker or a small lane. **No step was skipped, and no step
was made to pass by relaxing a script, a filter, or an environment
variable.**

### The focused verification, before the floor

| Command | Result |
| --- | --- |
| `cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` | pass (exit 0) |
| `cargo test --locked -p i2pr-daemon --test version_floor_boundary --test version_eligibility_live -- --test-threads=1` | pass — the guard 28/28, the lane 11/11 |
| `cargo test --locked -p i2pr-daemon --test version_eligibility_mesh -- --test-threads=1` | pass — 4/4, and the Plan 491 evidence artifact regenerates **byte-identical** |
| `cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1` | pass — 51/51, unedited |
| `cargo check --locked -p i2pr-daemon --all-targets` | pass |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | pass, no warnings |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-daemon --no-deps` | pass |
| `git diff --exit-code 71f73461 HEAD -- Cargo.toml Cargo.lock` | **byte-identical** |

### The three measured proofs

**1. The gate's refusal counts are the decision's own.** The lane builds a
five-floodfill mesh — one at the floor, one above it, one below it, one that
publishes no version, one that publishes `"0.9.62."` — and asserts the
selection's refusal counters under `unknown-version`, `uncomparable-version`
and `below-floor` separately, the at-floor floodfill's presence in the
target set, and `examined == accepted + refused`.

**2. The receipt carries no peer identity.** The guard asserts the receipt
type's body names no peer hash, no address, no key and no version string
(the rule retargeted to the new module, with its probe unchanged in
substance), and the lane asserts it again over a live receipt and over the
receipt's rendered content.

**3. Plan 491 F3 is discharged, verified rather than assumed.** On an
**isolated copy** of the tree (`/tmp/i2pr-f3-check`, the real tree untouched),
`scripts/check-runtime-boundaries.sh` was run against three injections into
`crates/i2pr-netdb/src/version.rs`:

| Injection | Reported | Exit |
| --- | --- | --- |
| `std::time::SystemTime::now()` | `crates/i2pr-netdb/src/version.rs:250: forbidden wall clock API` | 1 |
| `async fn injected_clock()` | `i2pr-netdb must remain runtime-neutral: no Tokio and no async functions` | 1 |
| `TcpStream::connect` | `i2pr-netdb must not own sockets; the daemon and the transports own listeners` | 1 |

The copy was restored and the baseline re-run green (exit 0). The real tree's
`crates/i2pr-netdb/` was never modified — `git status` on it is empty.

### The false-pass audit

For every "passed" claim, how it could have passed without proving anything:

| Claim | How it could have passed vacuously | Why it did not |
| --- | --- | --- |
| The gate refuses a below-floor peer | the selection could have returned no candidate at all | the lane asserts `examined == 1` for a single-record store, and `examined == 5` for the five-record mesh |
| The at-floor floodfill is eligible | the gate could have refused everything | the lane asserts the at-floor hash **is** in the target set and that the refusal total is zero for it |
| The identity `examined == accepted + refused` | both sides could be zero | the mesh has five records, so `examined == 5` and the identity is over real numbers |
| The receipt carries no identity | the receipt could have been empty | the lane asserts the receipt is present and non-empty before checking its rendering |
| The log is bounded | the owner could have recorded nothing | the lane records cap + 1 receipts and asserts `receipt_count() == cap` |
| The guard's new rules fire | a whole-file token search could be satisfied by a dead helper | the rules are **structural** — a named function's own body, matched with the open parenthesis — and each carries an inverted probe |
| The inverted probes are real negatives | a probe could pass vacuously | each probe was run against a deliberately weakened checker: weakening `check_body_contains` fails 8 probes, weakening `check_body_avoids` fails the ungated-selection probe, making `function_body` return the whole file fails 2 probes, and making the brace matcher string-unaware fails the extractor probe |
| The body extractor finds the right body | it could match a call site or a sibling function | the extractor is anchored on `fn <name>` with an identifier-continuation check, and a dedicated test covers a nested block, a string holding a closing brace, a char literal holding an opening brace, a trailing item and an absent function |
| The Plan 491 evidence is unchanged | the lane could have regenerated a different file | `git diff tests/` is empty — the artifact is byte-identical to the committed one |
| F3's section covers `version.rs` | the section could cover the crate but not that module | it scans `crates/i2pr-netdb/src` as a directory, and three separate injections into `version.rs` are each reported |

## Invariant review

1. **The declared version is not edited.** `CONTROLLED_ROUTER_VERSION` stays
   `"0.9.62"`; Plan 285's pinning test is untouched and passes.
2. **The floors are derived, never transcribed.** The obligation rows in
   `i2pr-proto` stay the only copy of the truth.
3. **Fail closed.** An unknown, uncomparable or below-floor peer is never a
   build peer and never a DSM target; an empty eligible set fails closed with a
   reason; every refusal is counted by reason.
4. **No operator knob.** The floors are specification constants.
5. **No peer identity in a receipt.** Counts and stable labels only.
6. **No advertisement, no wire change, no new option, no new dependency.**
7. **Bounded.** The receipt log has an explicit cap; the candidate ceiling is
   the existing policy's; the refusal counters saturate.
8. **No unbounded channel, no new task, no new listener.**

## Failure review

- A below-floor floodfill is a **refusal**, not an error. The creator
  publication proceeds to the next eligible target; if none remains the
  publication is skipped. The LeaseSet2 publication releases its reserved
  intent and returns the existing typed request-construction failure. No path
  fails the router's startup because of a refusal.
- The gate is a synchronous, bounded operation over an already-bounded store.
  It has nothing to cancel inside it; a cancelled publication leaves the
  coordinator's ledger and the receipt log consistent, because the receipt
  records the *decision*, which was taken before the send.
- Nothing is persisted. The receipts are a live observation log, not a durable
  ledger; a restart starts an empty log and the version evidence is re-derived
  from each stored RouterInfo.
- **Contention.** The owner holds a `std::sync::Mutex` for the length of a push
  or a clone. The lock is taken while the destination coordinator's Tokio mutex
  is held and is never held across an `.await`. The observation path takes only
  the owner's lock and never takes the coordinator's lock, so the order is
  strictly **coordinator → owner**, with no cycle.

## Migration review

None. No wire format changed, no `router.version` changed, no option changed,
no configuration key was added, nothing is persisted.

The **behavioural** consequence is stated plainly: a router that knows only
floodfills publishing no API version, an uncomparable version, or a version
below `0.9.62` will **not** publish its RouterInfo or its LeaseSet2 to them.
That is the intended fail-closed behaviour and it matches both pinned
references, which also refuse a peer whose version they cannot establish or
whose version is below the floor. A publication fixture that publishes no
version must publish one — the fixture is wrong, not the floor.

## Security review

- The receipt log is the only new surface that leaves the crate, and it carries
  **counts and stable reason labels only**. The static guard asserts that the
  receipt type's body names no peer hash, no address, no key and no version
  string, and the lane asserts it again over the live receipt.
- The gate **narrows** what the router talks to: a peer whose version cannot be
  established is no longer a publication target. That is a reduction in
  exposure, not an expansion.
- No new configuration knob, so an operator cannot lower the floor.
- No new dependency, so the supply surface is unchanged.
- The owner is published through the existing inspection seam, which is
  loopback-only and already carries the transit posture. It is not exposed on
  any new port and adds no I2PControl method.

## Documentation and operations

- `specs/CONFORMANCE.md` — a Plan 503 section, and a forward pointer in the
  Plan 491 section recording that one half of its claim was not enforced in
  production until Plan 503.
- `docs/architecture/i2pr-netdb.md` — the shared version-gated selection.
- `docs/architecture/i2pr-daemon.md` — the live gate, the live receipt, the
  observation seam, and the lookup-target finding.
- `specs/support.toml` — one `experimental` surface with `advertised = false`.
- `plans/registry.md`, `plans/subsystems/network-participation-roadmap.md` — the
  Plan 503 rows and the Plan 491 disposition.
- The current-fact surface tallies move from 94/88 to 95/89 in the registry,
  `specs/CONFORMANCE.md` and the daemon deep-dive. The **closed** closure
  records keep their own historical tallies: they record what those plans did,
  and they are not rewritten.

## Limitations

- **No stock-peer evidence.** Plan 503 claims none. The floor, the gate and
  the refusals are proven over a **private, deterministic mesh** built by the
  lane, not against an ordinary router.
- **The lookup-target selection is not gated.** The router still asks a
  below-floor floodfill for a *lookup*. A `DatabaseLookup` is not a DSM and is
  not one of Plan 491's two obligations, so the gate is not extended to it
  here. This is an open finding, not an oversight: extending the floor to the
  lookup path would change which floodfills the router can ask at all, which
  is a product decision beyond this plan.
- **The DSM gate's reach is bounded by what the store knows.** The gate
  examines the nearest floodfills within the policy's ceiling (32). A
  below-floor floodfill inside that window consumes a candidate slot even
  though it is not a target, which is the same bound Plan 491's closed
  enforcement point had. The creator RouterInfo publication previously
  considered **every** known floodfill advertiser; it now considers the 32
  nearest and keeps the eligible ones. The consequence is deliberate and
  fail-closed: a router whose 32 nearest floodfills are all ineligible
  publishes to none of them rather than publishing to one of them.
- **The receipts are not persisted.** A restart empties the log.
- **The peer-profile store is still empty in a live daemon** (Plan 497
  limitation 5). The live gates read the authoritative RouterInfo store, so
  they do not depend on the profile store; the participation driver's own
  receipts still describe its own empty store.
- **Plan 433's composition of `NetDbTunnelCoordinator` is untouched** and
  stays `blocked`. Plan 491's typed `begin_publication` refusal still has no
  production caller; what changed is that the live paths no longer need it,
  because they now carry their own gate.

## Findings by severity

### Medium

- **The lookup-target selection is not gated** — recorded above. It is a real
  gap in the router's API-version hygiene, and it is **not** one of Plan 491's
  two obligations. Closing it needs a plan-of-record, because it changes which
  floodfills the router can ask for a lookup at all.
- **A publication's refusal is not distinguishable from "no floodfill is
  known" in the error class.** Both take the same fail-closed path. The receipt
  distinguishes them (`examined == 0` against `examined == n` with the refusal
  counts), so the information is not lost; only the error *class* does not
  carry it. Widening it would change an existing error contract, so this plan
  left it alone.

### Low

- **The creator RouterInfo publication's skip transition is shared** across
  several reasons (no installed RouterInfo, no eligible floodfill, a store
  composition failure). The message distinguishes the no-eligible-floodfill
  case; the transition name does not. Cosmetic.
- **`VersionDecisionKind::selection_gate` became `pub(crate)`** in the move:
  the driver is a different module from the new owner, so a module-private
  method would not reach it. It stays outside the public API.

### Recorded, not defects

- The `i2pr-daemon` deep-dive's per-module unit-test distribution is stale
  (pre-existing).
- The `i2pr-daemon` deep-dive carries four bare-filename `file:line` citations
  a link checker cannot resolve (the files exist under `crates/i2pr-netdb/src/`)
  (pre-existing).

## Plan 491's F1 and F3 disposition

| Plan 491 finding | Disposition |
| --- | --- |
| **F1 — the receipt log is not produced by a live product loop** | **Discharged.** The live tunnel-build gate and the live DSM gate each record a receipt into a bounded live-owned log published through the daemon's inspection seam. |
| **F2 — no stock-peer evidence** | **Still open.** It needs the authorized two-host topology Plan 493 owns. Not touched here. |
| **F3 — `check-runtime-boundaries.sh` has no `i2pr-netdb` section** | **Already discharged by Plan 497 WP5, and verified here** with three negative controls on an isolated copy: a clock, an async function and a socket injected into `crates/i2pr-netdb/src/version.rs` are each reported with exit 1. The section scans the whole crate directory, so `version.rs` and `selection.rs` are inside its scope. No script was weakened. |
| **F4 — the intermittent `i2pcontrol_tunnels` staging failure** | **Still open**, and it belongs to the I2PControl / service-tunnel line. Not touched here. |

**Plan 491's closure record is not rewritten.** The §11 sentence that claimed
both decisions were enforced live is corrected here and in the registry's Plan
491 row. The precedent is Plan 502's: Plan 492's F2 disposition lives in Plan
502's closure record and in the registry rows, not in a rewritten Plan 492
record.

## Roadmap disposition

The `network-participation` roadmap's Plan 503 row moves to `closed`. The
roadmap's §12 status sentence names Plan 503 as the registered plan that
discharges Plan 491's F1 and the live half of Plan 491's DSM obligation.

**The next candidate on this line** is the **lookup-target gate** (the finding
above): the router still asks a below-floor floodfill for a lookup. It needs a
plan-of-record, because it changes which floodfills the router can ask at all
and therefore interacts with Plan 433's blocked NetDB composition.

Other open source-side candidates, unchanged by this plan: Plan 487 F2 (a SAM
session dies with the control socket's silence) and F3 (no transit-hop failure
detection), Plan 497 limitation 5 (the peer-profile store stays empty in a live
daemon), Plan 490 Medium #3 (the stock-peer introducer lane), and Plan 502's M1
(the creator-correlation retention window).

## Unblock audit

| Item | State after Plan 503 |
| --- | --- |
| Plan 491 F1 (the receipt log is not produced by a live product loop) | **Discharged.** |
| Plan 491 F2 (no stock-peer evidence) | Open — needs Plan 493's authorized two-host topology. |
| Plan 491 F3 (no `i2pr-netdb` boundary section) | **Discharged** by Plan 497 WP5, verified here. |
| Plan 491 F4 (the intermittent `i2pcontrol_tunnels` failure) | Open — the I2PControl line. |
| Plan 433 (the NetDB / destination-tunnel coordinator composition) | Still `blocked` on the external acquisition contract. Untouched. |
| Plan 436 WP3 (integrate the transit owner into the transport / I2NP pump) | Already discharged by Plan 502. Plan 436 stays `blocked`. |
| Plan 437, 438, 439, 493, 494 | Unchanged; all `blocked` on external environments. |
| Plan 492 F1, F3 | Unchanged. |
| The lookup-target gate | **New, open.** Needs a plan-of-record. |
