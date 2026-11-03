# Plan 516 — the router's publication asks for the network's acknowledgement

**Closure record.** The plan of record is
`plans/implementation/network-participation/516-router-info-publication-confirmation.md`.
This record is the authority on what the plan closed, what it did not, and
on what the evidence is.

## Status

Plan 516 is **closed**. The router's own RouterInfo publication now
**asks** for the network's acknowledgement, and the router **records** it.

Two facts, one gap:

1. **The publication never asked.** `i2pr_netdb::PublicationCoordinator::begin_attempt`
   minted a correlation `reply_token`, recorded it in the bounded token
   map, and documented that a retry "reuses the previously minted token so
   the remote peer can correlate the delivery acknowledgement" — and then
   wrote **`reply_token: 0`** into the `DatabaseStoreMessage` it returned.
   A `DatabaseStore` with a zero reply token asks for **no**
   acknowledgement, so no peer ever answered and
   `correlate_delivery_status` could never match. The `acknowledged`
   counter in `PublicationSnapshot` was structurally always zero.
2. **The acknowledgement already existed — on the other side of the wire.**
   This tree's **own** floodfill service answers a store that carries a
   nonzero reply token, and the `DeliveryStatus`'s message id **is** that
   token. So the router's floodfill service was answering an
   acknowledgement the router's own publication never asked for.

The tree's own live test said so in a comment: the "protocol-derived
acknowledgement" it correlated was **manufactured in-process**.

The reference asks for one. At the pinned commit
(`635b013a612ff47278ef02acf8580a28e10e26c5`, i2pd 2.61.0) it writes the
**real** token (`libi2pd/I2NPProtocol.cpp:272`) and, for a **direct**
publication, a **zero** reply tunnel id meaning a direct reply (`:285`)
with the local hash as the reply gateway (`:287-289`);
`libi2pd/RouterContext.cpp:1321` matches the token and reports
*"Publishing confirmed"* (`:1325`).

**Plan 516 closes the join.** Nothing is promoted: `router.version` stays
`0.9.62`, `caps` and the published RouterInfo stay unchanged, the
inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` stay byte-identical.

## What the plan closed

- The correlator publishes an **explicitly registered record**
  (`PublicationLocalRecord`), so the store message's key, its reply
  gateway and its payload are provably **one** record.
- The store message carries the correlator's **minted** token, in the
  reference's **direct-reply** form.
- The daemon composes **no** store message of its own any more. The
  service product's one-shot publication and the router-level refresh
  share the correlator's builder and its attempt book.
- The **inbound owner** — the SSU2 service's own loop — routes the
  floodfill's answer to the correlator.
- A **delivery** admission and a **network acknowledgement** are counted
  separately. A store the transport accepted is not a record the network
  holds.
- An acknowledgement that matches **no** live attempt is **refused** and
  counted under its own reason.

## What the plan did not close

- The **stock-peer** half. That a pinned stock floodfill answers a
  publication is still blocked on Plan 493's authorized two-host topology.
  No live claim is made and no stock-peer evidence is claimed.
- Plan 515's finding **F1** — the reference's resend burst. The reference
  retries on a *drop* and stops on a *confirmation*, so a faithful burst
  needs this confirmation. The burst is a successor's subject.
- The inbound queue is **capacity one** and the runtime drops an
  inbound message it cannot admit (`inbound_queue_drops`). A confirmation
  that loses that race is not recovered. This is a pre-existing property
  of the tree's inbound path, not something this plan introduced.

## Implementation commits

| Commit | Subject |
| --- | --- |
| `2839172b` | docs(plans): register Plan 516, the publication's confirmation |
| `94d2246f` | docs(plans): decide Plan 516's placement and the correlator's record |
| `a5a99073` | feat(netdb,daemon): the router's publication asks for the acknowledgement (Plan 516) |
| `fe2d2d2c` | docs(specs): record Plan 516's publication acknowledgement |

The closing commit adds this record and the registry and roadmap rows.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| the correlation token reaches the wire | `crates/i2pr-daemon/tests/router_info_publication_confirmation.rs`, `the_publication_carries_the_correlators_minted_token` — the wire token is the correlator's own minted token and is never zero |
| the reply is the reference's **direct** reply | the same lane, `the_store_uses_the_reference_s_direct_reply_form` — a **zero** reply tunnel id and the local hash as the gateway |
| the key, the gateway and the payload are **one** record | the same lane, `the_store_keys_our_own_hash_and_carries_our_own_record` — the store keys our own hash and its payload is the record the publication registered |
| a matching acknowledgement **confirms** the attempt | the same lane, `a_matching_acknowledgement_confirms_the_attempt` |
| an unmatched acknowledgement is **refused** and counted | the same lane, `an_unknown_acknowledgement_is_refused_and_counted` — it confirms nothing, and the refusal is counted under its own reason |
| a **delivery** is not an **acknowledgement** | the same lane, `a_delivery_admission_is_not_an_acknowledgement` and `the_acknowledgement_is_counted_separately_from_the_publication` |
| the attempt book stays **bounded** | the same lane, `the_attempt_book_stays_bounded` |
| the daemon composes **no** store message of its own | `crates/i2pr-daemon/tests/router_info_publication_confirmation_boundary.rs`, `the_daemon_composes_no_store_message` |
| the one-shot goes through the **correlator** | the same guard, `the_one_shot_goes_through_the_correlator` |
| the **inbound owner** joins the publication | the same guard, `the_inbound_owner_joins_the_publication` |
| every rule fires | the same guard's eight **inverted probes**: a token off the wire, a tunnelled reply, a missing gateway, a payload from elsewhere, the daemon's own builder, a one-shot that skips the correlator, an inbound owner that drops the answer, and a confirmation that drops the unmatched case |
| Plan 515's evidence does not regress | Plan 515's own lane (`crates/i2pr-daemon/tests/router_info_publication.rs`, 8) and guard (`.../router_info_publication_boundary.rs`, 15), both green on the closing tree |

## Focused verification

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | clean |
| `cargo check --locked -p i2pr-daemon --all-targets` | clean |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | clean |
| `cargo test --locked -p i2pr-netdb --all-targets` | 434 passed; 0 failed |
| `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` | 132 binaries; 2 744 passed; 0 failed |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation -- --test-threads=1` | 8 passed; 0 failed |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_confirmation_boundary -- --test-threads=1` | 15 passed; 0 failed |
| `cargo test --locked -p i2pr-daemon --test router_info_publication -- --test-threads=1` | 8 passed; 0 failed (Plan 515's lane, unchanged in count) |
| `cargo test --locked -p i2pr-daemon --test router_info_publication_boundary -- --test-threads=1` | 15 passed; 0 failed (Plan 515's guard, byte-identical) |
| `python3 scripts/check-global-plan-number-uniqueness.py` | passed |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 tests, OK |
| `python3 scripts/check-tooling-inventory.py` | passed |

## The routine floor

**139 steps, 139 PASS, 0 FAIL.**

| Step | Result |
| --- | --- |
| `floor-tree-clean` | PASS (0s) |
| `cargo-fmt` | PASS (4s) |
| `cargo-check` | PASS (6s) |
| `cargo-build-siblings` | PASS (1s) |
| `cargo-build-static-fixture` | PASS (0s) |
| `cargo-copy-static-fixture` | PASS (0s) |
| `cargo-rebuild-plain-fixture` | PASS (0s) |
| `cargo-test-workspace` | PASS (1335s) |
| `cargo-clippy` | PASS (14s) |
| `cargo-doc` | PASS (10s) |
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
| `plan495-peer-eligibility-self` | PASS (0s) |
| `plan495-peer-eligibility-profile-drift` | PASS (0s) |
| `plan476-gate-binding` | PASS (1s) |
| `plan476-gate-binding-self` | PASS (0s) |
| `check-managed-app-package-boundary` | PASS (0s) |
| `check-managed-app-package-boundary-self` | PASS (0s) |
| `check-managed-app-policy-boundary` | PASS (0s) |
| `check-managed-app-policy-boundary-self` | PASS (0s) |
| `check-managed-app-secured-sandbox` | PASS (1s) |
| `check-managed-app-secured-sandbox-self` | PASS (0s) |
| `check-service-tunnel-boundaries` | PASS (0s) |
| `check-managed-app-gateway-boundary` | PASS (0s) |
| `check-managed-app-manager-boundary` | PASS (0s) |
| `check-m11-per-epoch-composition` | PASS (0s) |
| `check-service-anonymity-boundaries` | PASS (0s) |
| `check-fixture-manifest` | PASS (0s) |
| `check-ntcp2-vectors` | PASS (0s) |
| `check-ssu2-vectors` | PASS (0s) |
| `check-i2cp-vectors` | PASS (5s) |
| `check-ntcp2-interoperability` | PASS (0s) |
| `check-constrained-host-lane-boundary` | PASS (0s) |
| `check-m11-transit-boundaries` | PASS (2s) |
| `check-m11-transit-qualification-evidence` | PASS (3s) |
| `check-sam-acceptance-evidence` | PASS (0s) |
| `check-ssu2-acceptance-evidence` | PASS (0s) |
| `check-i2cp-acceptance-evidence` | PASS (0s) |
| `check-i2pcontrol-acceptance-evidence` | PASS (0s) |
| `check-els2-type11-transcript-boundary` | PASS (0s) |
| `check-els2-live-lane-evidence` | PASS (2s) |
| `check-encrypted-service-consumer-caller` | PASS (0s) |
| `check-outproxy-request-path` | PASS (0s) |
| `check-outproxy-wire-lane-evidence` | PASS (0s) |
| `check-config-secret-hygiene` | PASS (0s) |
| `check-workflow-validity` | PASS (0s) |
| `check-floodfill-type5-serve` | PASS (1s) |
| `check-service-tunnel-acceptance-evidence` | PASS (0s) |
| `check-exploratory-tunnel-evidence` | PASS (0s) |
| `check-netdb-tunnel-evidence` | PASS (1s) |
| `check-destination-tunnel-evidence` | PASS (0s) |
| `check-streaming-tunnel-evidence` | PASS (5s) |
| `check-m6-mixed-router-acceptance-evidence` | PASS (4s) |
| `check-m12-floodfill-qualification-evidence-self` | PASS (1s) |
| `check-m12-floodfill-boundaries-self` | PASS (1s) |
| `ntcp2-harness-tests` | PASS (0s) |
| `plan488-mesh-lane` | PASS (0s) |
| `plan488-evidence-deterministic` | PASS (0s) |
| `plan489-mesh-lane` | PASS (0s) |
| `plan489-evidence-deterministic` | PASS (0s) |
| `plan490-mesh-lane` | PASS (0s) |
| `plan490-evidence-deterministic` | PASS (0s) |
| `plan490-ssu2-transport-suite` | PASS (7s) |
| `plan490-transport-boundary-guard` | PASS (0s) |
| `plan491-mesh-lane` | PASS (0s) |
| `plan491-evidence-deterministic` | PASS (0s) |
| `plan491-boundary-guard` | PASS (0s) |
| `plan491-proto-suite` | PASS (1s) |
| `plan491-netdb-suite` | PASS (89s) |
| `plan491-client-suite` | PASS (27s) |
| `plan492-posture-suite` | PASS (0s) |
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
| `plan498-bootstrap-resolver-row` | PASS (25s) |
| `plan498-runtime-relay-accessor` | PASS (5s) |
| `plan503-version-eligibility-live` | PASS (1s) |
| `plan503-version-eligibility-unit` | PASS (0s) |
| `plan504-liveness-boundary` | PASS (0s) |
| `plan504-liveness-live` | PASS (0s) |
| `plan504-liveness-unit` | PASS (1s) |
| `plan505-driver-boundary` | PASS (0s) |
| `plan505-driver-live` | PASS (3s) |
| `plan505-driver-unit` | PASS (0s) |
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
| `plan510-tunnelmanager-black-box` | PASS (2s) |
| `plan511-version-ceiling-derivation` | PASS (0s) |
| `plan511-version-ceiling-boundary` | PASS (1s) |
| `plan512-destination-liveness-live` | PASS (0s) |
| `plan512-destination-liveness-boundary` | PASS (0s) |
| `plan489-peer-budget-receipt` | PASS (1s) |
| `plan513-discovery-live` | PASS (2s) |
| `plan513-discovery-boundary` | PASS (0s) |
| `plan513-discovery-unit` | PASS (2s) |
| `plan513-discovery-seam-unit` | PASS (0s) |
| `plan514-exploratory-target` | PASS (8s) |
| `plan514-exploratory-target-boundary` | PASS (0s) |
| `plan516-netdb-publication` | PASS (1s) |
| `plan515-publication` | PASS (1s) |
| `plan515-publication-boundary` | PASS (0s) |
| `plan516-confirmation` | PASS (3s) |
| `plan516-confirmation-boundary` | PASS (0s) |
| `plan514-switch-unit` | PASS (0s) |
| `cargo-deny` | PASS (1s) |

## The negative controls

**13 controls, 13 caught by an assertion.**

| ID | Outcome | What the control proves |
| --- | --- | --- |
| **A** | CAUGHT | caught: the correlation token must reach the wire |
| **B** | CAUGHT | caught: the reply must be the reference's direct reply |
| **C** | CAUGHT | caught: the reply must name a gateway |
| **D** | CAUGHT | caught: the payload must be the registered record's |
| **E** | CAUGHT | caught: the daemon must compose no store message of its own |
| **F** | CAUGHT | caught: the one-shot publication must go through the correlator |
| **G** | CAUGHT | caught: the inbound owner must join the publication |
| **H** | CAUGHT | caught: an unmatched acknowledgement must be refused |
| **I** | CAUGHT | caught: the reply gateway must be our own hash |
| **J** | CAUGHT | caught: the payload must be the record the publication registered |
| **K** | CAUGHT | caught: an acknowledgement that matches no attempt must be refused |
| **L** | CAUGHT | caught: a delivery admission is not an acknowledgement |
| **M** | CAUGHT | caught: the store must key our own hash |

## Invariant review

1. **A publication is never counted as confirmed unless the network
   answered.** `Confirmed` requires a correlated `DeliveryStatus`. A
   transport admission is `Delivered`; the two are separate counters.
   *Held:* the lane's `a_delivery_admission_is_not_an_acknowledgement`.
2. **An unknown token is refused and counted.** A `DeliveryStatus` whose
   message id matches no live attempt never confirms anything and is
   counted under its own reason. *Held:* the lane's
   `an_unknown_acknowledgement_is_refused_and_counted` and the guard's
   rule 7.
3. **The correlator stays bounded.** `MAX_PUBLICATION_ATTEMPTS` is
   unchanged, and the attempt book and the token map keep their ceilings.
   *Held:* the lane's `the_attempt_book_stays_bounded`.
4. **The version gate still holds.** The confirmation path is not a
   second selection: the targets are still the Plan 503 version-gated
   floodfills, selected through the same function.
   *Held:* Plan 515's guard rules 3 and 4, unchanged and green.
5. **No secret crosses the boundary.** The reply token is a correlation
   identifier, not a key. It is never logged with a peer identity and
   never persisted. *Held:* the guard's rules and the lane's assertions.
6. **The publication remains the router's own.** The confirmation needs no
   authority beyond the record the router already installs.
   *Held:* the lane's `the_store_keys_our_own_hash_and_carries_our_own_record`.
7. **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
   published RouterInfo are unchanged; the inventory's advertised count
   stays zero. *Held:* the inventory check and `Cargo.toml`/`Cargo.lock`
   byte-identity.

## Failure, cancellation, restart, and contention review

- A publication cancelled mid-flight is `Cancelled`, never `Confirmed`.
- The correlator's attempt book is **not** persisted. A router that
  restarts loses its outstanding attempts, and a confirmation for a
  previous process is never counted: the token space is per-process, so
  a stale answer matches nothing and is refused and counted.
- Two publications in the same period cannot share a token: the counter is
  monotonic and the token map is keyed by value.
- The correlator is owned by the SSU2 service's loop, so the attempt book
  and the inbound path that correlates into it are the **same** owner. No
  lock, no channel, no cross-process sharing.

## Migration and compatibility review

- **No wire-format change.** The `DatabaseStore` layout is untouched; only
  the values of the existing reply fields change, and they change to the
  form the specification and the reference already use. The codec already
  encoded the direct-reply form — `reply_tunnel_id` is an `Option<u32>`
  and `Some(0)` is the reference's "direct reply" — so no codec change was
  required.
- **A peer that ignores the reply token** simply does not answer. The
  publication is still `Delivered`; it is never `Confirmed`, and it is
  counted as unconfirmed rather than as a failure.
- `Cargo.toml` and `Cargo.lock` stay byte-identical. No new dependency.
- No configuration key: the confirmation is not optional at the protocol
  level.

## Security review

- The reply token is a **correlation identifier**, not a key. It is
  derived from the correlator's bounded counter, never from a secret, and
  it is not persisted.
- The store's reply gateway is the router's **own** public hash, so the
  acknowledgement is addressed to the router and to nobody else.
- The reply tunnel id is **zero**: the acknowledgement comes straight
  back over the direct session and never asks the router to route it
  through a tunnel it does not hold.
- The token is not logged. A confirmation logs the request identifier, and
  an unmatched answer logs the fact that it was unmatched — neither
  carries a peer identity or a payload.
- No **local** condition is written into a peer's profile: a peer that
  does not answer is counted as unconfirmed, and the counter belongs to
  the router.

## Limitations

- The **stock-peer** half is not proved. Nothing here shows that a pinned
  stock floodfill answers a publication; that needs Plan 493's authorized
  two-host topology.
- The inbound queue is capacity one, so a confirmation can be dropped by
  the runtime under inbound pressure. The count of unconfirmed
  publications therefore bounds the confirmation rate from **below**, not
  from above.
- The confirmation is per **attempt**, and the attempt is per **target**.
  A publication that reaches three floodfills produces three attempts and
  up to three acknowledgements; the counters distinguish them.
- The reference's resend burst is not adopted (Plan 515's **F1**).

## Findings by severity

### Medium — open

**F1 — The reference's resend burst is still not adopted.** Plan 515
recorded this and Plan 516 did not close it. The reference asks **one**
floodfill per cycle and then, every `1600` milliseconds, the next closest
ones, up to fifteen (`libi2pd/RouterContext.h:37`, `:38`). This router
asks the closest three in one pass, once per period, and does not re-ask
on failure. Plan 516 removed the *reason* a burst could not be faithful —
the confirmation did not exist — so a successor can now adopt one. The
fan-out bound stays the tree's own and is asserted by the guard.

### Informational

**I1 — The daemon's own store-message builder is gone.** There is now
**one** builder, and it is the correlator's. The daemon's
`router_info_store_message` and its typed error were removed, and the
service product's unit test that asserted the old `reply_token: 0`
contract was removed with them; the subject moved to this plan's lane,
where the new contract is asserted.

**I2 — Plan 515's lane was extended, not rewritten.** The plan's
`Publish` variant carried a `store` field holding the composed
`DatabaseStoreMessage`. Plan 516 moves the composition to the correlator,
so the plan names the **record** instead. Plan 515's lane destructures
that variant in two places, and both were updated from `store: _` to
`record: _`. Nothing else in Plan 515's lane changed, its test count is
unchanged, and Plan 515's **guard** is byte-identical and green.

**I3 — The RouterInfo's key is derived from the router identity, not from
the signed bytes.** `ValidatedRouterInfo::from_router_info` derives the
key from the **router identity**, so the installed record and the
bootstrap's local snapshot share one RouterHash while differing in their
bytes. The publication carries the **installed** bytes — the record that
names the router's address — and the correlator is given that record
explicitly, so the key, the gateway and the payload are one record by
construction.

## Roadmap disposition

The network-participation subsystem's RouterInfo publication is now
**periodic** (Plan 515) and **confirmable** (Plan 516). The remaining
phases — the two-family public qualification (Plan 493) and the floodfill
role with `caps=f` (Plan 494) — are unchanged and remain blocked on
external environments.

## Unblock audit

| Item | State |
| --- | --- |
| Plan 491 F1 | **closed** by Plan 503 |
| Plan 491 F2 | **open** — blocked externally on Plan 493's authorized two-host topology |
| Plan 491 F3 | **closed** by Plan 497 |
| Plan 491 F4 | **conditionally closed** by Plan 510 |
| Plan 491 F5 | **closed** by Plan 511 |
| Plan 497 M2 | the **source-side** half is closed by Plan 513; the stock-peer half stays blocked on Plan 493 |
| Plan 487's stock-peer half | **open** — blocked externally on Plan 493 |
| Plan 515 F1 (the resend burst) | **open** — a successor's subject; Plan 516 removed the reason it could not be faithful |
| Plan 516 F1 (the resend burst) | the same finding, carried forward |
| Plans 433, 435, 436, 437, 438, 439 | **blocked** on external environments |
| Plans 493, 494 | **blocked** on external environments |
| The **local** OBEP consumer | **open** — no local consumer exists |
| NTCP2 **ingress** | **open** — no NTCP2 ingress exists |
