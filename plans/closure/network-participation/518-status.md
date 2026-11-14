# Plan 518 — the router's own record is re-dated and re-signed

**Closure record.** The plan of record is
`plans/implementation/network-participation/518-router-info-publication-freshness.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 518 is **closed**. The router's own RouterInfo now carries a **fresh**
publication date: the record is re-dated and re-signed once per publication
cycle, and a record the network would refuse is refused and counted rather
than published.

Three facts, one gap:

1. **The publication was already periodic.** Plan 515 made the router's own
   RouterInfo a refreshed record on the reference's interval.
2. **The publication was already confirmable.** Plan 516 put the correlator's
   minted token on the wire and joined the inbound owner to the correlator.
3. **The publication was already retrying.** Plan 517 made an unacknowledged
   publication follow the next batch, at the next closest floodfills.

What was still missing is that all three re-send the **same bytes** — and
those bytes carry the publication date the router signed at **startup**.

The netdb refuses a remote record older than `DEFAULT_MAX_AGE`, 24 hours
(`crates/i2pr-netdb/src/router_info.rs:20`), mirrored by the runtime's
`LOCAL_ROUTER_INFO_MAX_AGE_SECS`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:130`); the reference's own netdb
expires routers between one and a half and twenty-seven hours
(`libi2pd/NetDb.hpp:49-51`). So after a day of uptime the router keeps
republishing a record the network will not accept, every 39 minutes, with no
counter and no diagnostic. The publication was not slow: it was **dead**.

The reference avoids this by re-dating and re-signing its record before each
publish: `libi2pd/RouterInfo.cpp:1218` (`LocalRouterInfo::CreateBuffer` calls
`RefreshTimestamp()` at `:1164` and re-signs), `libi2pd/RouterContext.cpp:262`
(`UpdateRouterInfo`), `:1125` (`UpdateTimestamp`, at most every
`ROUTER_INFO_UPDATE_INTERVAL` — `30 * 60` seconds,
`libi2pd/RouterContext.h:33`), and `:1474` (`HandlePublishTimer` dates the
record **before** `Publish()`, so one cycle publishes one record).

**Plan 518 closes the freshness.** Nothing is promoted: `router.version` stays
`0.9.62`, `caps` and the published RouterInfo's **content** stay unchanged,
the inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` stay byte-identical.

## What the plan closed

- The record is **re-dated and re-signed**, once per cycle, at the cycle's
  start (decision **D1**).
- The re-encode is the **proto's own**: the signed region is re-encoded from
  the record's own decoded parts, so the addresses, the peers and the options
  cannot drift (decision **D4**).
- The re-sign **verifies itself**: the crypto layer decodes the result, checks
  the signature and asserts that the date round-tripped, before it returns.
- The fresh record is **installed** as well as published, so the RouterInfo
  the router hands out in `SessionConfirmed` carries the fresh date too
  (decision **D3**).
- The publication's **staleness rule**: a record older than the netdb's
  acceptance window, or dated too far in the future, is refused and counted
  under its own reason (`refused_stale_record`) rather than published
  (decisions **D7**, **D9**).
- The staleness rule uses the **netdb's own window**
  (`i2pr_netdb::DEFAULT_MAX_AGE` and `i2pr_netdb::DEFAULT_MAX_FUTURE_SKEW`),
  not a second constant that could drift.
- A record that cannot be re-dated is **refused, never published stale**
  (decision **D6**).
- The cycle publishes **one record**: the cycle's record lives in the owner,
  so the first batch and every rung publish the same bytes.
- The router's **hash does not change**: a RouterInfo's key is derived from
  the router identity rather than from the signed bytes
  (`crates/i2pr-netdb/src/router_info.rs:249`), so a re-dated record is the
  same router to the network (decision **D5**).
- The publication module holds **no key** and reads **no clock**: the
  freshness comes from its caller and the window from the netdb.
- The re-dating **rides the cycle's start**, so no new timer is introduced
  (decision **D8**).

## What the plan did not close

- The **stock-peer** half. That a pinned stock floodfill accepts the re-dated
  record in the live network, needs Plan 493's authorized two-host topology,
  and no live claim is made.
- The reference's **direct-or-tunnelled** choice of the publication's
  transport. This tree publishes over its direct SSU2 session, and its
  non-loopback binds are forbidden, so the tunnelled branch is unreachable
  here.
- The reference's **variance** on the publish interval. Plan 515 decided
  against it and that decision stands.
- The reference's **congestion** update. That is the bandwidth-metering line,
  which the roadmap records as genuinely hard and still open elsewhere.
- The publication's delivery still **shares the SSU2 service's loop** with the
  relay drain and the inbound owner. See the **Medium** finding below.
- Any change to the published RouterInfo's content, and any advertisement.

## Implementation commits

| Commit | Subject |
| --- | --- |
| `56d07264` | docs(plans): register Plan 518, the publication's freshness |
| `03dbd1da` | feat(publication): re-date and re-sign the router's own RouterInfo (Plan 518) |
| `5e924019` | docs(specs): record Plan 518, the publication's freshness |
| `67a7961a` | fix(publication): the cycle's record is the owner's, so a rung cannot change it |
| `b13eb385` | docs(plans): the cycle's one record, and Plan 518's amendment log |

The closing commit adds this record and the registry and roadmap rows.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| a record is **publishable** when fresh and **refused** when stale | `crates/i2pr-daemon/tests/router_info_publication_refresh.rs`, `a_record_is_publishable_when_fresh_and_refused_when_stale` |
| a record dated too far in the future is **refused** | the same lane, `a_record_dated_too_far_in_the_future_is_refused` |
| the netdb's own validator **refuses** the stale record and **accepts** the re-dated one | the same lane, `the_netdb_refuses_the_stale_record_and_accepts_the_re_dated_one` — at the same reading |
| the re-dated record keeps its **content** and the router's **hash** | the same lane, `the_re_dated_record_keeps_the_content_and_the_router_hash` |
| a **peer** survives the re-dating | the same lane, `the_re_dated_record_keeps_a_peer` |
| the re-dated record's signature **verifies**, and the original's does not cover the new date | the same lane, `the_re_dated_record_verifies_and_the_original_does_not_cover_the_new_date` |
| a record that cannot be re-dated is **refused** | the same lane, `a_record_that_cannot_be_re_dated_is_refused` |
| the staleness refusal is **counted** under its own reason | the same lane, `the_stale_refusal_is_counted` |
| the re-dating is **deterministic** | the same lane, `the_re_dating_is_deterministic` |
| the **fan-out** is unchanged | the same lane, `the_publication_fan_out_is_unchanged` |
| the cycle holds **one record** for every rung | the same lane, `the_cycle_holds_one_record_for_every_rung` |
| the re-dating happens **once per cycle**, at the cycle's start, and not per rung | `crates/i2pr-daemon/tests/router_info_publication_refresh_boundary.rs`, `the_re_dating_happens_once_per_cycle` |
| the publication publishes the **re-dated** record | the same guard, `the_publication_uses_the_re_dated_record` |
| the staleness rule uses the **netdb's** window | the same guard, `the_staleness_rule_uses_the_netdb_window` |
| a stale record is **refused**, never published | the same guard, `a_stale_record_is_refused` |
| the re-dating logs **no secret** | the same guard, `the_re_dating_logs_no_secret` |
| the proto's re-encode preserves the **content** | the same guard, `the_proto_preserves_the_content` |
| the crypto's re-sign **verifies** its own result | the same guard, `the_crypto_verifies_its_result` |
| the module stays **runtime-neutral** | the same guard, `the_module_stays_runtime_neutral` |
| the rungs publish the **cycle's** record | the same guard, `the_rungs_publish_the_cycle_record` |
| the cycle's record is the **owner's** | the same guard, `the_cycle_record_is_the_owners` |
| every rule fires | the same guard's ten **inverted probes**, and the negative controls below |
| Plan 515's, Plan 516's and Plan 517's evidence does not regress | Plan 515's lane (`crates/i2pr-daemon/tests/router_info_publication.rs`, 8) and guard (`.../router_info_publication_boundary.rs`, 15), Plan 516's lane (`.../router_info_publication_confirmation.rs`, 8) and guard (`.../router_info_publication_confirmation_boundary.rs`, 15), Plan 517's lane (`.../router_info_publication_retry.rs`, 14) and guard (`.../router_info_publication_retry_boundary.rs`, 17) — all green on the closing tree, and the three guard files byte-identical |

## Focused verification

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo check --locked --workspace --all-targets` | clean |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | clean |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_refresh -- --test-threads=1` | 11 passed; 0 failed |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_refresh_boundary -- --test-threads=1` | 20 passed; 0 failed (10 rules + 10 inverted probes) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_retry -- --test-threads=1` | 14 passed; 0 failed (Plan 517's lane) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_retry_boundary -- --test-threads=1` | 17 passed; 0 failed (Plan 517's guard) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation -- --test-threads=1` | 8 passed; 0 failed (Plan 516's lane) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation_boundary -- --test-threads=1` | 15 passed; 0 failed (Plan 516's guard) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication -- --test-threads=1` | 8 passed; 0 failed (Plan 515's lane) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_boundary -- --test-threads=1` | 15 passed; 0 failed (Plan 515's guard) |
| `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` | 136 binaries; 2806 passed; 0 failed |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 tests, OK |
| `python3 scripts/check-global-plan-number-uniqueness.py` | passed |
| `python3 scripts/check-tooling-inventory.py` | passed |
| `git status --porcelain` | empty — the tree is clean |

Plan 515's, Plan 516's and Plan 517's **guard files** are byte-identical to
their committed state: Plan 518 extends the publication's own guard and
touches none of theirs.

## The routine floor

**143 steps, 143 PASS, 0 FAIL.**

| Step | Result |
| --- | --- |
| `floor-tree-clean` | PASS (0s) |
| `cargo-fmt` | PASS (3s) |
| `cargo-check` | PASS (10s) |
| `cargo-build-siblings` | PASS (0s) |
| `cargo-build-static-fixture` | PASS (0s) |
| `cargo-copy-static-fixture` | PASS (0s) |
| `cargo-rebuild-plain-fixture` | PASS (0s) |
| `cargo-test-workspace` | PASS (1168s) |
| `cargo-clippy` | PASS (15s) |
| `cargo-doc` | PASS (26s) |
| `cargo-test-doc` | PASS (3s) |
| `check-dependency-direction` | PASS (0s) |
| `check-global-plan-number-uniqueness` | PASS (1s) |
| `check-adr-number-uniqueness` | PASS (0s) |
| `check-portable-service-tunnel-api` | PASS (0s) |
| `check-portable-service-tunnel-consumer` | PASS (6s) |
| `planning-tests` | PASS (7s) |
| `check-runtime-boundaries` | PASS (1s) |
| `check-console-boundaries` | PASS (1s) |
| `check-console-browser-security` | PASS (0s) |
| `check-tooling-inventory` | PASS (0s) |
| `check-license-metadata` | PASS (0s) |
| `check-managed-app-private-client-seams` | PASS (0s) |
| `check-router-readiness-contract` | PASS (0s) |
| `check-router-readiness-contract-self` | PASS (0s) |
| `check-core-router-recovery-contract` | PASS (0s) |
| `check-core-router-recovery-contract-self` | PASS (0s) |
| `plan449-ssu2-preflight-self` | PASS (0s) |
| `plan495-peer-eligibility-self` | PASS (1s) |
| `plan495-peer-eligibility-profile-drift` | PASS (0s) |
| `plan476-gate-binding` | PASS (0s) |
| `plan476-gate-binding-self` | PASS (1s) |
| `check-managed-app-package-boundary` | PASS (0s) |
| `check-managed-app-package-boundary-self` | PASS (0s) |
| `check-managed-app-policy-boundary` | PASS (0s) |
| `check-managed-app-policy-boundary-self` | PASS (0s) |
| `check-managed-app-secured-sandbox` | PASS (0s) |
| `check-managed-app-secured-sandbox-self` | PASS (0s) |
| `check-service-tunnel-boundaries` | PASS (0s) |
| `check-managed-app-gateway-boundary` | PASS (0s) |
| `check-managed-app-manager-boundary` | PASS (0s) |
| `check-m11-per-epoch-composition` | PASS (0s) |
| `check-service-anonymity-boundaries` | PASS (0s) |
| `check-fixture-manifest` | PASS (0s) |
| `check-ntcp2-vectors` | PASS (0s) |
| `check-ssu2-vectors` | PASS (0s) |
| `check-i2cp-vectors` | PASS (3s) |
| `check-ntcp2-interoperability` | PASS (0s) |
| `check-constrained-host-lane-boundary` | PASS (0s) |
| `check-m11-transit-boundaries` | PASS (2s) |
| `check-m11-transit-qualification-evidence` | PASS (1s) |
| `check-sam-acceptance-evidence` | PASS (0s) |
| `check-ssu2-acceptance-evidence` | PASS (0s) |
| `check-i2cp-acceptance-evidence` | PASS (0s) |
| `check-i2pcontrol-acceptance-evidence` | PASS (1s) |
| `check-els2-type11-transcript-boundary` | PASS (0s) |
| `check-els2-live-lane-evidence` | PASS (1s) |
| `check-encrypted-service-consumer-caller` | PASS (1s) |
| `check-outproxy-request-path` | PASS (0s) |
| `check-outproxy-wire-lane-evidence` | PASS (0s) |
| `check-config-secret-hygiene` | PASS (0s) |
| `check-workflow-validity` | PASS (0s) |
| `check-floodfill-type5-serve` | PASS (0s) |
| `check-service-tunnel-acceptance-evidence` | PASS (1s) |
| `check-exploratory-tunnel-evidence` | PASS (0s) |
| `check-netdb-tunnel-evidence` | PASS (0s) |
| `check-destination-tunnel-evidence` | PASS (0s) |
| `check-streaming-tunnel-evidence` | PASS (6s) |
| `check-m6-mixed-router-acceptance-evidence` | PASS (4s) |
| `check-m12-floodfill-qualification-evidence-self` | PASS (0s) |
| `check-m12-floodfill-boundaries-self` | PASS (1s) |
| `ntcp2-harness-tests` | PASS (0s) |
| `plan488-mesh-lane` | PASS (1s) |
| `plan488-evidence-deterministic` | PASS (0s) |
| `plan489-mesh-lane` | PASS (0s) |
| `plan489-evidence-deterministic` | PASS (0s) |
| `plan490-mesh-lane` | PASS (0s) |
| `plan490-evidence-deterministic` | PASS (0s) |
| `plan490-ssu2-transport-suite` | PASS (8s) |
| `plan490-transport-boundary-guard` | PASS (0s) |
| `plan491-mesh-lane` | PASS (0s) |
| `plan491-evidence-deterministic` | PASS (0s) |
| `plan491-boundary-guard` | PASS (0s) |
| `plan491-proto-suite` | PASS (1s) |
| `plan491-netdb-suite` | PASS (97s) |
| `plan491-client-suite` | PASS (27s) |
| `plan492-posture-suite` | PASS (1s) |
| `plan492-m11-lane` | PASS (4s) |
| `plan502-transit-composition-lane` | PASS (1s) |
| `plan502-transit-composition-unit` | PASS (0s) |
| `plan502-transit-boundary-self` | PASS (0s) |
| `plan496-private-lan-profile` | PASS (0s) |
| `plan496-ntcp2-activation-gate` | PASS (0s) |
| `plan497-routing-consumer-lane` | PASS (1s) |
| `plan497-netdb-preferred-order` | PASS (1s) |
| `plan497-daemon-participation-unit` | PASS (2s) |
| `plan498-introducer-handoff-lane` | PASS (1s) |
| `plan498-introducer-mesh-lane` | PASS (0s) |
| `plan498-bootstrap-resolver-row` | PASS (26s) |
| `plan498-runtime-relay-accessor` | PASS (6s) |
| `plan503-version-eligibility-live` | PASS (1s) |
| `plan503-version-eligibility-unit` | PASS (0s) |
| `plan504-liveness-boundary` | PASS (1s) |
| `plan504-liveness-live` | PASS (0s) |
| `plan504-liveness-unit` | PASS (0s) |
| `plan505-driver-boundary` | PASS (1s) |
| `plan505-driver-live` | PASS (2s) |
| `plan505-driver-unit` | PASS (1s) |
| `plan506-reconcile-boundary` | PASS (5s) |
| `plan506-reconcile-live` | PASS (1s) |
| `plan507-retention-feasibility` | PASS (1s) |
| `plan507-retention-boundary` | PASS (0s) |
| `plan507-retention-live` | PASS (1s) |
| `plan508-creator-retention-feasibility` | PASS (0s) |
| `plan508-creator-retention-boundary` | PASS (1s) |
| `plan508-creator-retention-live` | PASS (1s) |
| `plan509-lookup-target-decision` | PASS (0s) |
| `plan509-lookup-target-boundary` | PASS (1s) |
| `plan510-managed-staging-load-lane` | PASS (0s) |
| `plan510-managed-reason-boundary` | PASS (1s) |
| `plan510-tunnelmanager-black-box` | PASS (4s) |
| `plan511-version-ceiling-derivation` | PASS (2s) |
| `plan511-version-ceiling-boundary` | PASS (0s) |
| `plan512-destination-liveness-live` | PASS (1s) |
| `plan512-destination-liveness-boundary` | PASS (0s) |
| `plan489-peer-budget-receipt` | PASS (1s) |
| `plan513-discovery-live` | PASS (3s) |
| `plan513-discovery-boundary` | PASS (0s) |
| `plan513-discovery-unit` | PASS (2s) |
| `plan513-discovery-seam-unit` | PASS (0s) |
| `plan514-exploratory-target` | PASS (8s) |
| `plan514-exploratory-target-boundary` | PASS (0s) |
| `plan516-netdb-publication` | PASS (2s) |
| `plan515-publication` | PASS (0s) |
| `plan515-publication-boundary` | PASS (0s) |
| `plan516-confirmation` | PASS (3s) |
| `plan516-confirmation-boundary` | PASS (1s) |
| `plan517-publication-retry` | PASS (3s) |
| `plan517-publication-retry-boundary` | PASS (1s) |
| `plan518-publication-refresh` | PASS (0s) |
| `plan518-publication-refresh-boundary` | PASS (0s) |
| `plan514-switch-unit` | PASS (1s) |
| `cargo-deny` | PASS (1s) |

The floor ran on the closing tree, with `floor-tree-clean` as its first step, so every step below measured the committed code.


## The negative controls

**16 controls, 16 caught by an assertion.** Each control mutates the **real**
tree, runs one detector, and requires the detector to fail with an
**assertion**; a control that breaks the build is reported as a *missed*
control, because a compile failure proves less than a failing assertion.
Every mutation is reverted **byte-for-byte** and the SHA-256 is asserted, and
the restore writes the file's bytes so cargo rebuilds the crate rather than
reusing the artifact the mutation produced (Plan 514's finding **M1**).

| ID | Outcome | Detector | What the control proves |
| --- | --- | --- | --- |
| **A** | CAUGHT | `the_re_dating_happens_once_per_cycle` | the re-dating must happen once per cycle, not twice |
| **B** | CAUGHT | `the_publication_uses_the_re_dated_record` | the publication must publish the re-dated record, not the installed one |
| **C** | CAUGHT | `the_staleness_rule_uses_the_netdb_window` | the staleness rule must use the netdb's own window |
| **D** | CAUGHT | `a_stale_record_is_refused` | the publication must test the record's freshness before it publishes |
| **E** | CAUGHT | `the_re_dating_logs_no_secret` | the re-dating must log a reason only |
| **F** | CAUGHT | `the_proto_preserves_the_content` | the proto's re-encode must preserve the record's content |
| **G** | CAUGHT | `the_crypto_verifies_its_result` | the crypto's re-sign must verify its own result |
| **H** | CAUGHT | `the_module_stays_runtime_neutral` | the staleness rule must never read a clock |
| **I** | CAUGHT | `the_module_stays_runtime_neutral` | the publication module must hold no key |
| **J** | CAUGHT | `a_record_is_publishable_when_fresh_and_refused_when_stale` | a record exactly at the netdb's window must still be publishable |
| **K** | CAUGHT | `a_record_dated_too_far_in_the_future_is_refused` | a record exactly at the netdb's clock skew must still be publishable |
| **L** | CAUGHT | `the_stale_refusal_is_counted` | the staleness refusal must be counted under its own reason |
| **M** | CAUGHT | `a_record_that_cannot_be_re_dated_is_refused` | a record that cannot be re-dated must be refused, not published |
| **N** | CAUGHT | `the_publication_fan_out_is_unchanged` | Plan 518 must not change the publication's fan-out |
| **O** | CAUGHT | `the_rungs_publish_the_cycle_record` | a rung must publish the cycle's record, not a copy read back from the runtime |
| **P** | CAUGHT | `the_cycle_record_is_the_owners` | the rung must read the cycle's record from the owner |

**Two of them were rewritten after the first run.** Controls **J** and **K**
originally widened `PUBLICATION_MAX_RECORD_AGE_MS` and
`PUBLICATION_MAX_FUTURE_SKEW_MS` and expected the lane to fail — and it did
not, because the lane's staleness assertions are written *relative to the
constant under test*, so the lane's own boundary moved with the constant.
They were rewritten as off-by-one mutations of the **comparisons**, which the
lane does detect. See finding **I1**: the lane alone does not pin the window;
the guard's rule 3 is what pins it to the netdb's own constant.

The two controls added for the cycle's record — **O** and **P** — cover the
hole the first implementation left, and each is caught by the guard rule it
targets.


## Invariant review

1. **Nothing is advertised.** `router.version` stays `0.9.62`; `caps` and the
   published RouterInfo's **content** are unchanged; the inventory's advertised
   count stays zero. *Held:* the inventory's own count and the `Cargo.toml` and
   `Cargo.lock` byte-identity.
2. **The router's hash does not change.** A re-dated record is the same
   router; the key is derived from the identity. *Held:* the lane's
   `the_re_dated_record_keeps_the_content_and_the_router_hash`.
3. **The record's content does not change.** Only the publication date and the
   signature may differ from the startup record. *Held:* the lane's
   `the_re_dated_record_keeps_the_content_and_the_router_hash` and
   `the_re_dated_record_keeps_a_peer`, and the guard's
   `the_proto_preserves_the_content`.
4. **The version gate holds.** Every target of every batch is still selected
   through `select_version_gated_floodfills`. *Held:* unchanged from Plan 517,
   whose guard's rule 3 checks the exclusion set reaches the selection.
5. **A stale record is never published.** A record older than the netdb's
   acceptance window is refused and counted, not published. *Held:* the lane's
   `a_record_is_publishable_when_fresh_and_refused_when_stale` and the
   guard's `a_stale_record_is_refused`.
6. **A record that cannot be re-dated is refused, not published stale.**
   *Held:* the lane's `a_record_that_cannot_be_re_dated_is_refused`.
7. **The cycle publishes one record.** Every rung of a cycle publishes the
   same bytes. *Held:* the lane's `the_cycle_holds_one_record_for_every_rung`
   and the guard's `the_rungs_publish_the_cycle_record` and
   `the_cycle_record_is_the_owners`.
8. **No new timer.** The re-dating rides the publication tick that already
   begins the cycle. *Held:* the guard's `the_re_dating_happens_once_per_cycle`.
9. **No secret is logged.** The re-dating logs a reason, never a key. *Held:*
   the guard's `the_re_dating_logs_no_secret`.

## Failure, cancellation, restart, and contention review

- **A record that cannot be re-dated** is refused under its own reason, the
  refusal is counted, and the cycle ends. The router keeps running.
- **A record that is already stale** — older than the netdb's acceptance
  window — is refused under its own reason and counted. It is never published,
  because a peer would refuse it anyway.
- **A record dated too far in the future** is refused the same way: the netdb
  rejects it too.
- **A cancellation** during the re-dating leaves the previously installed
  record in place; the publication is refused and the cycle ends.
- **A restart** re-signs the record from the persisted identity, so the date
  starts again at the new process's clock.
- **Contention** — the re-dating happens before the cycle's first batch, so
  the correlator's registered record is the fresh one for every rung. The
  cycle's record lives in the owner, so a rung cannot change it.
- **The install** is atomic: the runtime replaces the record under its lock,
  and existing sessions keep the RouterInfo they already exchanged. A failed
  install is **not** fatal to the cycle: the fresh record is still published,
  and the next cycle tries again.
- **A poisoned lock** — the publication is refused under its own reason and
  the cycle is halted, rather than left running with nothing to await.
- **A missing local record** — the publication is refused under its own reason
  and the cycle is halted.

## Migration and compatibility review

- **No wire-format change.** The record has the same structure; only its date
  and its signature differ. The publication's store message is unchanged.
- **No configuration key.** The cadence is the publication's own period.
- `Cargo.toml` and `Cargo.lock` are byte-identical. No new dependency.
- The published RouterInfo's **content** is unchanged, so a peer that already
  holds the record holds the same router.
- The cycle's record moved into the owner, so `refresh_router_info` and
  `advance_publication_cycle` no longer take a `local_record` parameter. There
  is one source of truth for the cycle's record rather than two, and a rung
  cannot publish a record other than the cycle's.
- Plan 515's, Plan 516's and Plan 517's closure records are **not** rewritten,
  and their lanes and guards stay green and byte-identical.

## Security review

- **No secret is logged.** The re-dating's diagnostics carry a reason only: no
  key, no signature, and not the record's bytes. The guard's
  `the_re_dating_logs_no_secret` fails the build if a forbidden token appears
  in the helper's body.
- **No new trust boundary.** The router publishes the same record to the same
  peers, and the selection keeps going through the version gate.
- **The cycle's record is bounded.** It is one record, and the runtime's own
  `LOCAL_ROUTER_INFO_MAX_ENCODED_LEN` bounds its size.
- **The publication module holds no key.** It holds bytes, a hash and a
  reading — never a signing key. The guard's `the_module_stays_runtime_neutral`
  fails if the module names `RouterIdentityBundle`, `signing_key` or
  `signing_seed`.
- **The re-sign verifies itself.** The crypto layer decodes the result,
  verifies the signature and asserts the date round-tripped, so a caller can
  never publish a record the crypto layer could not prove.
- **A local condition is never written into a peer's profile.** A refused
  record, a failed re-dating and a stale record are counted by the router's own
  counters only.

## Limitations

- The **stock-peer** half is blocked on Plan 493's authorized two-host
  topology: that a pinned stock floodfill accepts the re-dated record in the
  live network is not proved here and is not claimed.
- The reference's **direct-or-tunnelled** choice is not implemented; the
  tunnelled branch is unreachable in this tree.
- The reference's **variance** on the publish interval is not adopted.
- The reference's **congestion** update is not implemented.
- The re-dating's date advances once per **cycle**, so the record's age is at
  most one publication period — `39 * 60` seconds — where the reference
  re-dates at most every `30 * 60` seconds
  (`libi2pd/RouterContext.h:33`). The divergence is recorded, not imitated.
- The **staleness window** is the netdb's `DEFAULT_MAX_AGE`, 24 hours, and the
  reference's netdb expires routers between one and a half and twenty-seven
  hours. This tree uses one window for both the local refusal and the
  publication's refusal, so a record this router would refuse is refused
  exactly when a peer would refuse it — at the cost of the reference's
  per-router spread.
- The publication's delivery shares the SSU2 service's loop with the relay
  drain and the inbound owner.
- The cycle's record lives in the **owner**, so the owner holds one copy of
  the router's own public record. It is public data, not a secret, and it is
  bounded by the runtime's own encoded-length ceiling.

## Findings by severity

### Medium — open

- **M1 — The publication's delivery shares the SSU2 service's loop.** The
  refresh (Plan 515), the retry (Plan 517) and now the re-dating and its
  install all run on the SSU2 service's own loop, so a publication that has to
  establish a session — up to the dial timeout per target — delays the relay
  drain and the inbound owner that carries the confirmation. The ladder's
  *timing* is unaffected (the deadline is measured from the moment the batch
  went out), but the loop's other work waits. The re-dating itself is a
  sign and an install, so it adds little; the delivery is the expensive part.
  **Not fixed here**: separating the publication onto its own task needs an
  explicit owner for the publication's state, and the confirmation path owns
  the same state today. A successor plan should give the publication its own
  owner and its own task with explicit cancellation.

### Informational

- **I1 — Two of the first controls were vacuous, and that is a property of
  the lane.** The lane's staleness assertions are written **relative to the
  constant under test** — `T0 + PUBLICATION_MAX_RECORD_AGE_MS + 1` — so a
  control that *widened* the constant could not be detected by the lane: the
  lane's own boundary moved with it. The controls were rewritten as
  off-by-one mutations of the **comparisons**, which the lane does detect.
  What this shows is that **the lane alone does not pin the window**; the
  guard's `the_staleness_rule_uses_the_netdb_window` is what pins it to the
  netdb's own constant. Both are in the tree, so the invariant holds — but
  the two tests are not interchangeable, and the lane must not be read as
  pinning the window on its own.
- **I2 — The first implementation kept invariant 7 true only by accident.**
  The cycle's first batch published the fresh record, but every **rung** read
  the record back out of the runtime's installed copy. Those are the same
  bytes only while the install succeeded, and the proof that a re-dated
  record always installs was a reading of the runtime's code rather than a
  test. Had the install failed, a rung would have published the **stale**
  record while the correlator held the **fresh** one, so the acknowledgement
  the correlator matched would have named a record the cycle had stopped
  publishing. The fix is commit `67a7961a`: the cycle's record moved into the
  owner, the two publication entry points read it from there, and the guard
  gained two rules for it. The plan of record's section 16 records the
  amendment.
- **I3 — The daemon deep-dive published a stale integration-test-file
  count.** It said **112**, written at Plan 508's closure, and was already
  stale by sixteen before Plan 518: Plans 509–517 added test files without
  updating it. It is corrected to **130**, the tracked count. This is a
  pre-existing doc-vs-source drift, corrected because the document was
  already being edited; it is **recorded, not silently fixed**.
- **I4 — The plan of record's survey-time line numbers are now marked as
  such.** Section 3 described the gap with the line numbers of the tree at
  survey time, and this plan's own implementation moved them. The section now
  says so and gives the current locations beside them.
- **I5 — The date advances once per cycle.** The reference re-dates at most
  every `30 * 60` seconds; this tree re-dates once per publication period,
  `39 * 60` seconds. The record's age is therefore at most one period, far
  inside the netdb's window.
- **I6 — Plan 517's unblock audit mislabels Plan 491's F1.** Its table reads
  *"Plan 491 **F1** — the publication's resend burst | closed by Plan 517"*.
  Plan 491's **F1** is *"the receipt log is not produced by a live product
  loop"*
  (`plans/closure/network-participation/491-status.md:666`), which Plan 503
  discharged; the **resend burst** was Plan **515's** F1 and Plan **516's**
  F1. **Plan 517's closure record is not rewritten** — the correction lives
  here and in the registry row, the precedent Plan 503 set for Plan 491 and
  Plan 504 set for Plan 185.
- **I7 — This plan's own registration step damaged the live index, and
  the closure repairs it.** Commit `56d07264` inserted Plan 518's registry row
  with a **prefix** anchor -- the first ninety-three characters of Plan 517's
  row -- and replaced that anchor with `anchor + "\n" + row`. That split Plan
  517's row in half and spliced Plan 518's row into the middle of it, so Plan
  517's link, its closure path and its **entire** description fell out of the
  registry, leaving a ninety-three-character stub. **Plan 517's closure record
  is not rewritten** and Plan 517's row is restored **byte-for-byte** from its
  closure commit (`0000a037`) by the closing commit. The rule this earns:
  **a plan's registration must anchor on the whole row, never on a prefix of
  it.** A prefix anchor is not a substring match on a table row -- it is a
  splice point, and the text after it belongs to the row.

## Roadmap disposition

- The network-participation roadmap's row for Plan 518 moves from
  *registered* to **closed**, with the closure record named.
- The registry's Plan 518 row moves from *registered* to **closed**, in the
  house shape every other closed row uses.
- The registry's **Plan 517 row is restored byte-for-byte** from `0000a037`.
  Plan 517's closure record is not rewritten; the row is the live index, and
  the index had been damaged by this plan's own registration step. See finding
  **I7**.
- The roadmap's open item for a **live** stock-peer confirmation stays open:
  it is Plan 493's topology.
- Plan 517's **M1** — the publication's delivery shares the SSU2 service's
  loop — stays open and is owned by a successor plan.

## Unblock audit

| Item | State |
| --- | --- |
| Plan 491 **F1** — the receipt log is not produced by a live product loop | **closed** by Plan 503 |
| Plan 491 **F2** — no stock-peer evidence for the floors | **still blocked** on Plan 493's authorized two-host topology |
| Plan 491 **F3** — `check-runtime-boundaries.sh` has no `i2pr-netdb` section | **closed** by Plan 497's WP5 and verified by Plan 503 |
| Plan 491 **F4** — the intermittent `i2pcontrol_tunnels` staging failure | **conditionally discharged** by Plan 510; the failure itself belongs to the I2PControl / service-tunnel line |
| Plan 491 **F5** — the comparator's four-component ceiling is an unstated assumption | **closed** by Plan 511 |
| Plan 515 **F1** — the reference's resend burst | **closed** by Plan 517 |
| Plan 516 **F1** — the reference's resend burst | **closed** by Plan 517 |
| Plan 497 **M2** — the routing table's live reader | **not reopened**; Plan 518's selection is the router-wide store's, exactly as Plan 515's and Plan 517's were |
| Plan 517's **M1** — the publication's delivery shares the SSU2 service's loop | **still open**, and Plan 518 adds the re-dating to the same loop |
| Plan 351 **F7** — the `i2pcontrol_tunnels` port guess | **still open**, and it belongs to the I2PControl / service-tunnel line |
| Plans 433, 435, 436, 437, 438, 439, 493, 494 | **still blocked** on their external environments |

Nothing in this plan unblocks a plan that was blocked on an external
environment, and nothing in this plan was done under another plan's number.
