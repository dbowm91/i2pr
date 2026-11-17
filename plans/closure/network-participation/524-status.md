# Plan 524 — the router asks a peer for a relay tag, and records the introducer

**Closure record.** The plan of record is
`plans/implementation/network-participation/524-the-router-asks-a-peer-for-a-relay-tag.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 524 is **closed**. The router can now tell that a peer is an
**introducer**, ask such a peer for a **relay tag** while it is firewalled,
and record the tag that comes back as an **authenticated introducer** bound to
the peer that issued it.

Classification: **corrective** (the SSU2 introducer composition's acquisition
half). Plan 490 gave the router the other half — answering a peer's Relay
Request as the introducer — and Plan 523 corrected the **form** of the
introducer set. This plan gives the router the **acquisition**: the learning,
the asking and the recording. The **publication** half and the `C`
advertisement are a successor's subjects, and this record says so plainly so
that a successor does not mistake the two.
**Subsystem:** network-participation. **Closure date:** 2026-11-09.

Nothing is promoted: `router.version` stays `0.9.62`, `caps` and the published
RouterInfo's **content** stay unchanged, the router still advertises **no**
introducer capability of its own, the inventory's advertised count stays
**zero**, and `Cargo.toml` and `Cargo.lock` stay byte-identical.

## What the plan closed

The tree held every fact the acquisition needs, and each fact had exactly one
missing production caller. The plan closed four of them.

**The router could not tell that a peer is an introducer.**
`Ssu2Capabilities::supports_relay` — the transport's own test for the
advertised `C` letter — had **no production caller**. The reference answers
the question from the peer's **advertised** capability
(`libi2pd/RouterInfo.h:184` — `caps & eSSUIntroducer`), and the answer now
exists:

- `crates/i2pr-daemon/src/introducer_request.rs:102` — `read_capability` takes
  the peer's **validated** RouterInfo from the authoritative store and parses
  its SSU2 address with the transport's own **strict** parser
  (`Ssu2RouterAddress::parse`). The bytes are the ones the netdb validated on
  ingest — the signature, the identity hash, the static-key binding and the
  freshness were all checked before the record reached the store — and
  unvalidated bytes are never parsed.
- The answer is one of five **distinct** refusals, never a silent absence:
  `Advertised`, `NoStoredRecord`, `NoSsu2Address`, `UnparsableAddress` and
  `NotAdvertised` (`introducer_request.rs:74-87`).
- The test is the peer's **advertised** `C`, never its published introducer
  set: a peer that *publishes* introducers is a peer that *has* introducers,
  not a peer willing to *be* one.

**Nothing queued the request.** `queue_relay_tag_request` existed and had no
caller. The daemon now owns the policy:

- `crates/i2pr-daemon/src/introducer_request.rs:196` — `IntroducerRequestOwner`
  is the bounded owner. It refuses every window while the operator's opt-in is
  off, refuses every window while the router is not firewalled, and draws its
  candidates from the peers with a **live session**. It holds no task, no
  timer, no listener and no socket: the product's existing tick drives it.
- `introducer_request.rs:254` — `candidate_window` draws a **bounded**,
  **rotating** window from the live session peers
  (`MAX_INTRODUCER_CANDIDATES`, `:53`, sixteen). A busy router therefore sees
  every peer it holds a session with over time instead of re-examining the
  same head of the list for ever. The asked set is pruned against the live
  sessions on every window, so it never outgrows the router's own peer
  population, and a peer the router already asked is not asked again while its
  session lives.
- `introducer_request.rs:303` — `decide` applies the reference's condition:
  the router must be **firewalled** and an eligible peer must exist, and it
  asks **at most one** peer per window.
- `crates/i2pr-daemon/src/service_product.rs:4787` —
  `drive_introducer_request` is the tick step. It never dials: the request
  rides a session the router already holds, and the peer is never named
  unless this router holds a session with it.
- `crates/i2pr-daemon/src/config.rs:1455` — `ssu2.introducer_request` is an
  explicit opt-in, **off** by default, and refused without the SSU2 service
  (`config.rs:3507-3512`).

**A tag that arrived was counted and dropped.** The runtime received the
answer as a `SessionEvent::RelayTag` and handed it on; the introducer role
owner then **refused** it as `NotOurRole`, which was correct — a `Tag` is the
answer to *this router's own* request, not an inbound request to serve — but
the effect was that a legitimate event died counted as a refusal, and nothing
learned the peer. The runtime now records it:

- `crates/i2pr-runtime/src/ssu2_runtime.rs:6532` — the `SessionEvent::RelayTag`
  arm records an authenticated introducer **only** for a peer in the
  **bounded** outstanding-request set (`MAX_OUTSTANDING_TAG_REQUESTS`, `:955`,
  sixteen). A tag for a peer the router never asked is **refused** and
  counted: a peer may send the block unsolicited, and recording an introducer
  nobody asked for would put an unauthenticated peer into the router's
  introducer set.
- `crates/i2pr-runtime/src/ssu2_peer_relay.rs:1119` —
  `note_authenticated_introducer` takes the **wall clock** and applies the
  margin to the **record**, so the table's sweep runs against the real clock.
  The previous shape derived the sweep clock from the record's own expiry,
  which made the sweep run in the future and expire live records early; the
  path had no production caller until this plan, so nothing was wrong on the
  wire.
- `crates/i2pr-runtime/src/ssu2_runtime.rs:2495` — `learned_introducers` makes
  the learned set observable. The values are public protocol material: the
  SSU2 introducer tuple is published in the RouterInfo address.

**The role owner was charged with a refusal it did not make.** The daemon's
drain now **skips** the tag answer
(`crates/i2pr-daemon/src/ssu2_relay_drain.rs:187`), counting it under its own
reason (`tag_answers`). The handoff itself is **unchanged** — it is the
runtime's public authenticated-event channel, and Plan 499's live-mesh
evidence reads the minted tag through it
(`crates/i2pr-daemon/tests/ssu2_introducer_live_mesh.rs:480-487`;
`crates/i2pr-runtime/tests/ssu2_local.rs:913-921`).

### The registered plan's corrections

The implementation made two corrections to the registered text's substance,
both recorded in the plan's amendment log.

**§17.2 — the capability read moves to the daemon.** The registered design
put the read in the transport's validated peer record. That is true of an
**inbound** session and false of an **outbound** one: the transport's
`validate_router_info` (`crates/i2pr-transport-ssu2/src/handshake.rs:924`) is
called on exactly one path — the **responder's** SessionConfirmed
(`state_machine.rs:1446`) — because it is the *initiator* that carries a
RouterInfo in its SessionConfirmed, and the initiator's own peer record is
built with an empty RouterInfo (`state_machine.rs:728-732`,
`router_info: Vec::new()`). The request travels outbound, so the registered
design could not have answered the question for the peers it needed to answer
it for. The correction moves the read to the **daemon**, which is the tree's
authority for peer records, and re-uses the same pattern the product already
uses for a peer's public static encryption key. **D2**, **D5** and **D6**
were replaced; the transport is untouched by this plan.

**§17.3 — the `Tag` event stays on the handoff, and the ceiling evicts.** The
registered D8 said the event is consumed before it reaches the handoff. The
wart was real, but the fix was in the wrong place: the handoff is the
runtime's public authenticated-event channel, and consuming the event there
would have broken Plan 499's evidence. The correction keeps the event on the
handoff and moves the fix to the **daemon's drain**. The same amendment
corrected the registered §3.3, §9 and §11 wording about the introducer
table's ceiling: the table **evicts** the oldest record
(`crates/i2pr-transport-ssu2/src/introducer.rs:196-210`), it does not refuse;
this plan does not change the table's behaviour, and this record states the
ceiling as it is.

**§17.4 — the citation drift.** §3 is the plan's **evidence** and is **not**
rewritten. The two halves moved the constructs §3 cites, so §17.4 carries an
**additive pointer table** mapping each registered citation to the site that
now holds the cited construct, and naming the citations that did not move.
The registered text stands exactly as it was written.

## What the plan did not close

- **The publication.** The router still never publishes an introducer set.
  `Ssu2RuntimeService::introducer_publication_material`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:2586`) and the netdb's
  `build_with_ssu2_introducer_address`
  (`crates/i2pr-netdb/src/local.rs:292`) stay uncalled by production after
  this plan. That is a successor's subject, and Plan 523's whole point was
  that the form must be right before it is used.
- **The `C` advertisement.** This router's own published `caps` does not gain
  the introducer letter. The plan **reads** the capability; it does not
  **publish** it. Publishing a role capability is an advertisement with its own
  evidence burden and its own default-off semantics, and it is a separate
  plan.
- **The daemon's `withheld_no_introducers` counter is still permanently
  zero.** `crates/i2pr-daemon/src/self_publication.rs:125`, `:157`, `:186`,
  `:224`, `:270` — the variant is produced only by
  `introducer_publication_material`, which the publication successor will
  call. Until then the counter reads zero, which is honest but uninformative.
- **The stock-peer half.** Blocked on Plan 493's authorized two-host topology.
- **The withdrawal form's `i` option.** The firewalled snapshot publishes `s`
  and `v` (`crates/i2pr-transport-ssu2/src/publication.rs:389-398`) where the
  specification's unpublished SSU2 address carries `s`, `i` and `v`. A
  separate, smaller divergence on a path Plan 520 owns.
- **The public dial scope.** `Ssu2DialTarget::new` remains loopback-only and
  `new_private_lan` remains RFC1918-IPv4-only; that bound is Plan 431's.
- **Plan 433's scope** and **Plan 494's `caps=f`** are untouched.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `a929a233` | the plan of record, its registry row and its roadmap row |
| `98ecb933` | the plan's §17.2 amendment: the capability read moves to the daemon |
| `2f98d662` | the runtime half — the outstanding-request set, the recording, the drain's skip — and the plan's §17.3 amendment |
| `7b538938` | the formatting and the two clippy lints the runtime half left behind |
| `8a4efc95` | the daemon half — the owner, the capability read, the tick driver, the opt-in and the inspection cell |
| `6076ad7c` | the lane and the guard |
| `c2d04a11` | the documentation, the support inventory, the deep-dives and the plan's §17.4 amendment |
| `05c515cd` | the guard's own fail-closed property, and the probe that proves it |
| `dadfcdb1` | the guard's unused helper, which the floor's clippy step caught |

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the proto crate is untouched | `git diff a929a233..dadfcdb1 -- crates/i2pr-proto/` is empty; the daemon writes no wire vocabulary at all, which the guard proves (`the_daemon_never_writes_the_block`) |
| 2 | the introducer role owner is untouched | `crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs`, `the_drain_skips_the_tag_answer`; `crates/i2pr-daemon/src/ssu2_introducer.rs:349` is unchanged |
| 3 | the router's published RouterInfo content does not change | `specs/support.toml`, the `daemon.introducer-request` surface is `advertised = false`; `Cargo.toml` and `Cargo.lock` are byte-identical |
| 4 | a tag is recorded only from an authenticated session, and only in answer to a request this router made | `crates/i2pr-runtime/src/ssu2_runtime.rs:6532`; `crates/i2pr-daemon/tests/ssu2_introducer_request.rs`, `a_tag_for_a_peer_the_router_did_not_ask_yields_no_record`; `crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs`, `the_runtime_records_only_an_asked_peer` |
| 5 | the capability read is authenticated | `crates/i2pr-daemon/src/introducer_request.rs:102`; `crates/i2pr-daemon/tests/ssu2_introducer_request.rs`, `the_capability_read_answers_from_the_validated_record`; `crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs`, `the_capability_read_uses_the_validated_store` |
| 6 | a malformed or absent capability is refused, never defaulted | `crates/i2pr-daemon/src/introducer_request.rs:74-87`; `crates/i2pr-daemon/tests/ssu2_introducer_request.rs`, `the_capability_read_answers_from_the_validated_record` and `every_capability_refusal_is_counted_under_its_own_reason` |
| 7 | the transport and the netdb stay runtime-neutral | `bash scripts/check-runtime-boundaries.sh` — PASS |
| 8 | every refusal is typed and countable | `crates/i2pr-daemon/src/introducer_request.rs:157` (`IntroducerRequestCounters`), `:303` (`decide`), `:338` (`note_transport_refusal`); `crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs`, `every_refusal_is_counted_under_its_own_reason` |
| 9 | exactly one new configuration key, and it is default-off | `crates/i2pr-daemon/src/config.rs:1455`, `:3507`; `crates/i2pr-daemon/tests/ssu2_introducer_request.rs`, `the_opt_in_is_off_by_default` and `the_opt_in_requires_the_ssu2_service` |
| 10 | the reference pins are untouched | no external router or client is patched, vendored or modified; the pins are unchanged |
| 11 | no boundary script is weakened | `git diff a929a233..dadfcdb1 -- scripts/` is empty |
| 12 | the request is bounded | `crates/i2pr-daemon/src/introducer_request.rs:53` (`MAX_INTRODUCER_CANDIDATES`), `:254` (`candidate_window`), `:303` (`decide`); `crates/i2pr-runtime/src/ssu2_runtime.rs:955` (`MAX_OUTSTANDING_TAG_REQUESTS`); `crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs`, `the_window_is_bounded_and_rotates` and `the_outstanding_set_is_bounded` |

## Focused verification

```text
cargo test --locked -p i2pr-daemon --test ssu2_introducer_request -- --test-threads=1
    13 passed; 0 failed
cargo test --locked -p i2pr-daemon --test ssu2_introducer_request_boundary -- --test-threads=1
    34 passed; 0 failed
cargo test --locked -p i2pr-runtime --all-targets
    162 passed; 0 failed
cargo check --locked --workspace --all-targets
    clean
cargo fmt --all --check
    clean
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
    clean
```

## The routine floor

**84 steps, 84 PASS, 0 FAIL.** The reconciliation is
exact: the script declares its steps and the log carries one result for each,
the step names match one for one, and no step failed.

| Step | Result |
| --- | --- |
| `floor-tree-clean` | PASS |
| `fmt` | PASS |
| `check` | PASS |
| `app-siblings` | PASS |
| `app-fixture-static` | PASS |
| `app-fixture-copy` | PASS |
| `app-fixture-rebuild` | PASS |
| `plan522-peer-pool-lane` | PASS |
| `plan522-peer-pool-boundary` | PASS |
| `plan522-peer-pool-unit` | PASS |
| `transport-ssu2-unit` | PASS |
| `runtime-unit` | PASS |
| `netdb-unit` | PASS |
| `plan523-introducer-lane` | PASS |
| `plan523-introducer-boundary` | PASS |
| `plan524-introducer-request-lane` | PASS |
| `plan524-introducer-request-boundary` | PASS |
| `test-workspace` | PASS |
| `clippy` | PASS |
| `doc` | PASS |
| `test-doc` | PASS |
| `dependency-direction` | PASS |
| `plan-number-uniqueness` | PASS |
| `adr-number-uniqueness` | PASS |
| `portable-service-tunnel-api` | PASS |
| `portable-service-tunnel-consumer` | PASS |
| `planning-tests` | PASS |
| `runtime-boundaries` | PASS |
| `console-boundaries` | PASS |
| `console-browser-security` | PASS |
| `tooling-inventory` | PASS |
| `license-metadata` | PASS |
| `managed-app-private-client-seams` | PASS |
| `router-readiness-contract` | PASS |
| `router-readiness-self-test` | PASS |
| `core-router-recovery-contract` | PASS |
| `core-router-recovery-self-test` | PASS |
| `plan449-ssu2-preflight` | PASS |
| `plan495-peer-eligibility` | PASS |
| `plan476-gate-binding` | PASS |
| `plan476-gate-binding-self-test` | PASS |
| `managed-app-process-boundary` | PASS |
| `managed-app-process-boundary-self-test` | PASS |
| `managed-app-package-boundary` | PASS |
| `managed-app-package-boundary-self-test` | PASS |
| `managed-app-policy-boundary` | PASS |
| `managed-app-policy-boundary-self-test` | PASS |
| `managed-app-secured-sandbox` | PASS |
| `managed-app-secured-sandbox-self-test` | PASS |
| `service-tunnel-boundaries` | PASS |
| `managed-app-gateway-boundary` | PASS |
| `managed-app-manager-boundary` | PASS |
| `m11-per-epoch-composition` | PASS |
| `service-anonymity-boundaries` | PASS |
| `fixture-manifest` | PASS |
| `ntcp2-vectors` | PASS |
| `ssu2-vectors` | PASS |
| `i2cp-vectors` | PASS |
| `ntcp2-interoperability` | PASS |
| `constrained-host-lane-boundary` | PASS |
| `m11-transit-boundaries` | PASS |
| `m11-transit-qualification-evidence` | PASS |
| `sam-acceptance-evidence` | PASS |
| `ssu2-acceptance-evidence` | PASS |
| `i2cp-acceptance-evidence` | PASS |
| `i2pcontrol-acceptance-evidence` | PASS |
| `els2-type11-transcript-boundary` | PASS |
| `els2-live-lane-evidence` | PASS |
| `encrypted-service-consumer-caller` | PASS |
| `outproxy-request-path` | PASS |
| `outproxy-wire-lane-evidence` | PASS |
| `config-secret-hygiene` | PASS |
| `workflow-validity` | PASS |
| `floodfill-type5-serve` | PASS |
| `service-tunnel-acceptance-evidence` | PASS |
| `exploratory-tunnel-evidence` | PASS |
| `netdb-tunnel-evidence` | PASS |
| `destination-tunnel-evidence` | PASS |
| `streaming-tunnel-evidence` | PASS |
| `m6-mixed-router-acceptance-evidence` | PASS |
| `m12-floodfill-qualification-evidence` | PASS |
| `m12-floodfill-boundaries` | PASS |
| `ntcp2-harness` | PASS |
| `cargo-deny` | PASS |

## The negative controls

Every guard rule has an **inverted probe** that applies a mutation and
asserts the rule **fires**. The mutations are applied to an **in-memory copy**
of the source — the probe passes a `String` literal or a mutated `String` to
the rule function — so **no file on disk is ever written**: the restoration is
byte-for-byte by construction, not by a cleanup step that could be skipped.

| Probe | Mutation applied to the in-memory copy | Rule it must break |
| --- | --- | --- |
| `the_guard_reports_an_opt_in_that_defaults_on` | `default_ssu2_introducer_request` returns `true` | 01 — the opt-in defaults off |
| `the_guard_reports_an_opt_in_that_needs_no_service` | the semantic check for `raw.introducer_request && !raw.enabled` is deleted | 01 — the opt-in needs the SSU2 service |
| `the_guard_reports_a_read_of_unvalidated_bytes` | the read parses a `RouterAddress` it decoded itself | 02 — the read comes from the validated store |
| `the_guard_reports_a_capability_test_of_the_published_set` | the test becomes `!parsed.introducers().is_empty()` | 03 — the test is the advertised `C` |
| `the_guard_reports_a_router_that_asks_while_reachable` | the driver compares against `ReachabilityState::Reachable` | 04 — the router asks only while firewalled |
| `the_guard_reports_a_request_that_opens_a_connection` | the driver dials a peer | 05 — the request rides an existing session |
| `the_guard_reports_two_requests_in_one_window` | the driver queues two relay-tag requests | 06 — one window asks one peer |
| `the_guard_reports_a_daemon_that_writes_the_block` | the owner names `Block::RelayTagRequest` | 07 — the daemon writes no wire vocabulary |
| `the_guard_reports_an_owner_that_spawns_a_task` | the owner calls `tokio::spawn` | 08 — the owner adds no infrastructure |
| `the_guard_reports_an_unbounded_window` | `candidate_window` returns the whole live list | 09 — the window is bounded |
| `the_guard_reports_a_window_that_does_not_rotate` | the window never consults the cursor | 09 — the window rotates |
| `the_guard_reports_a_peer_that_is_asked_again` | `note_queued` does not record the peer, and the window does not exclude it | 10 — a peer is asked at most once |
| `the_guard_reports_an_uncounted_refusal` | the counters lose their per-reason fields and the decision stops counting | 11 — every refusal is counted under its own reason |
| `the_guard_reports_an_acquisition_that_publishes` | the owner calls `introducer_publication_material` | 12 — the publication is untouched |
| `the_guard_reports_an_acquisition_that_advertises_the_capability` | the owner builds a `caps` option | 12 — the acquisition advertises nothing |
| `the_guard_reports_a_recorder_that_accepts_any_tag` | the `SessionEvent::RelayTag` arm records without consulting the outstanding set | 13 — only an asked tag is recorded |
| `the_guard_reports_a_drain_that_hands_the_tag_to_the_role_owner` | the drain routes the event to the role owner | 14 — the drain skips the tag answer |
| `the_guard_reports_an_unbounded_outstanding_set` | the queueing path drops the ceiling and the capacity counter | 15 — the outstanding set is bounded |
| `the_guard_reports_a_missing_named_item` | the named item is removed from the source entirely, so the rule cannot find it | every rule — an absent named item is a defect, not a pass |

The guard's own fail-closed property is asserted by
`the_guard_reports_a_missing_named_item`: a rule whose named item is
**absent** returns a **refusal**, not a pass. Three of the probes above were
written because the first draft of the guard did **not** have that property —
an absent item made the rule vacuously pass — and the fix is recorded as
finding **L1** below.

## Acceptance criteria

The plan's §14 states ten acceptance criteria. Each is met, and each is met
by evidence rather than by assertion:

| # | Acceptance criterion | Met by |
| --- | --- | --- |
| 1 | the runtime can answer, for a peer it holds a session with, whether that peer advertises the introducer capability, and the answer is the transport's **validated** one | `crates/i2pr-daemon/src/introducer_request.rs:102`; `crates/i2pr-daemon/tests/ssu2_introducer_request.rs`, `the_capability_read_answers_from_the_validated_record`; `crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs`, `the_capability_read_uses_the_validated_store` and `the_capability_test_is_the_advertised_one` |
| 2 | a router with the opt-in on, firewalled, and holding a session with an eligible peer asks exactly one peer per cadence window | `crates/i2pr-daemon/tests/ssu2_introducer_request.rs`, `a_firewalled_router_asks_exactly_one_peer_per_window` and `the_router_asks_an_eligible_peer_and_records_the_introducer`; `crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs`, `at_most_one_request_leaves_per_window` |
| 3 | a tag arriving on an authenticated session is recorded as an authenticated introducer and is observable in the table | `crates/i2pr-runtime/src/ssu2_runtime.rs:6532`, `:2495`; `crates/i2pr-daemon/tests/ssu2_introducer_request.rs`, `the_router_asks_an_eligible_peer_and_records_the_introducer` |
| 4 | a peer that does not advertise the capability, a zero tag, a peer with no usable introduction key and a full table are each refused and counted | `crates/i2pr-daemon/tests/ssu2_introducer_request.rs`, `a_peer_that_advertises_no_capability_is_never_asked`; `crates/i2pr-runtime/src/ssu2_runtime.rs:919` (`Ssu2TagRequestError`), `:955` (`MAX_OUTSTANDING_TAG_REQUESTS`); `crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs`, `the_outstanding_set_is_bounded` |
| 5 | the `Tag` event no longer reaches the role owner's refusal path | `crates/i2pr-daemon/src/ssu2_relay_drain.rs:187`; `crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs`, `the_drain_skips_the_tag_answer` |
| 6 | the lane and the guard are green, every guard rule has an inverted probe that fires it, and no file on disk is written by a probe | 13 lane rows and 34 guard tests, all green; the 19 probes above |
| 7 | the routine floor is green on the closing tree | the floor table below |
| 8 | `specs/support.toml` records the surface as `experimental` and `advertised = false`; the advertised count stays **zero** | `specs/support.toml`, the `daemon.introducer-request` surface; the inventory reads 117 surfaces, 112 experimental, 4 deferred, 1 ready and **0 advertised** |
| 9 | `router.version` stays `0.9.62`; the published RouterInfo's `caps` is unchanged; `Cargo.toml` and `Cargo.lock` are byte-identical | `git diff a929a233..dadfcdb1 -- Cargo.toml Cargo.lock` is empty; the plan adds no version or capability |
| 10 | no boundary script is weakened | `git diff a929a233..dadfcdb1 -- scripts/` is empty |

**No stop condition fired.** Reading the peer's advertised capability did not
require re-parsing unvalidated bytes or weakening the transport's validation;
the request policy needed no new connection, no new task and no new listener;
recording a tag required publishing nothing; and the peer's advertised
capability **is** the reference's own test.

## Invariant review

1. **The proto crate is untouched.** No new I2NP message, no new SSU2 block
   type, no new wire format, no change to `i2pr-proto`. The guard proves the
   daemon names no block at all
   (`the_daemon_never_writes_the_block`), so the transport remains the only
   owner of the block vocabulary.
2. **The introducer role owner is untouched.** This plan changes what the
   *router* does with a `Tag` event it receives; it does not change how the
   role owner *serves* a peer. `crates/i2pr-daemon/src/ssu2_introducer.rs:349`
   still refuses a `Tag` as `NotOurRole`, and that refusal stands — the
   **drain** is what no longer routes the event there.
3. **The router's published RouterInfo content does not change.** The plan
   publishes no introducer set and advertises no `C`; `router.version` stays
   `0.9.62`, the record's `caps` string is unchanged, and the inventory's
   advertised count stays **zero**.
4. **A tag is recorded only from an authenticated session, and only in answer
   to a request this router made.** The runtime's bounded outstanding-request
   set is the gate (`ssu2_runtime.rs:6532`), and the session that carried the
   tag *is* the authentication. The record's provenance is
   `IntroducerProvenance::AuthenticatedDirect`.
5. **The capability read is authenticated.** The peer RouterInfo the plan
   reads is the one the netdb validated on ingest, and the plan re-uses that
   validated record rather than parsing unvalidated bytes.
6. **A malformed or absent capability is refused, never defaulted.** A peer
   whose address carries no `caps`, an unparsable `caps`, or a `caps` without
   the introducer letter is **not** an introducer, and each is its own counted
   refusal.
7. **The transport and the netdb stay runtime-neutral.** No `tokio::*`, no
   `std::net`, no `std::fs` in `i2pr-transport-ssu2` or `i2pr-netdb`.
   `scripts/check-runtime-boundaries.sh` passes. The new daemon module is in
   the daemon crate, which is the tree's runtime owner.
8. **Every refusal is typed and countable.** The capability read has five
   distinct outcomes, the request policy has four, and the runtime's queueing
   has its own typed error (`Ssu2TagRequestError`, `ssu2_runtime.rs:919`).
   Every counter carries a stable reason label and names no peer, no address,
   no key and no tag.
9. **Exactly one new configuration key, and it is default-off.**
   `ssu2.introducer_request`. It is refused without the SSU2 service rather
   than silently inert, so an operator who asks for it without the service
   learns why.
10. **The reference pins are untouched.** i2pd `2.61.0`
    (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
    (`9134f808337b401e8e53c73734c81fab04280c9d`). Neither reference is
    patched, vendored or modified; the plan reads them and cites them.
11. **No boundary script is weakened.** `scripts/` is byte-identical across the
    plan's commits. No `|| true`, no `continue-on-error`, no filename
    filtering, no early-return-success, and no production wire change was made
    to go green.
12. **The request is bounded.** One request per window; a window of at most
    sixteen peers; an outstanding-request set of at most sixteen peers; and
    the asked set pruned against the live sessions on every window.

## Failure, cancellation, restart, and contention review

- **Failure.** A refusal to ask, a refusal to record and a refused tag are
  each counted under their own reason, and none leaves state behind. A
  poisoned state mutex degrades to "no introducer", never to a fabricated one:
  `queue_relay_tag_request` returns `NotQueued(SessionTerminated)` when the
  runtime's state lock is poisoned.
- **Cancellation.** The plan adds no task of its own; the request rides the
  daemon's existing tick and the daemon's existing product loop, and the
  runtime's queueing is synchronous and non-awaiting. There is nothing to
  cancel and nothing to await. The **only** await the plan adds is the
  capability read's store lock, which is taken and released inside one
  statement and never held across the request.
- **Restart.** The introducer table is in-memory and is lost on restart, the
  same as every other session-scoped table in the tree. The plan adds no
  persistence: a tag learned from a session is meaningless after the session
  is gone.
- **Contention.** The request is bounded to one peer per cadence window, so a
  busy router does not ask every peer it knows. The introducer table's own
  ceiling **evicts** the oldest record
  (`crates/i2pr-transport-ssu2/src/introducer.rs:196-210`); that is the
  transport's existing behaviour and this plan does not change it. The
  **outstanding-request** set is a different bound, and it **refuses**.
- **One local condition is never written into a peer's profile.** The plan
  writes nothing into any peer profile, no reachability state and no score.
  The refusals it introduces are the router's own condition — "this record
  does not parse", "this router is not firewalled" — and each is counted in
  the router's own counters.

## Migration and compatibility review

- **No wire change.** The request block is the one the transport already
  builds, and the answer is the one the transport already parses. The proto
  crate and the block vocabulary are untouched.
- **A reading change, not a writing change.** The router reads a peer's
  advertised capability it did not read before. Nothing the router publishes
  changes.
- **The role owner's behaviour is unchanged** for every event it already
  served.
- **No configuration migration.** The one new key is default-off and has no
  meaning to an existing configuration.
- **The learning is a strict increase in what the router can do.** A peer
  that advertises `C` was previously invisible to the router's acquisition
  path; now it is visible, and a peer that does not is still refused exactly
  as before.

## Security review

- **No secret is added and none is removed.** The relay tag is a
  session-scoped token the specification publishes in the clear, and the value
  still refuses a **zero** tag on construction
  (`crates/i2pr-transport-ssu2/src/address.rs:500-502`). The router hash is a
  public identifier, not a secret.
- **No key material is logged.** The plan adds two `tracing::debug` sites and
  neither names a peer, a tag, an address or a key: the refusal site prints
  the **reason label** and the transport's own `reason()`, both of which are
  stable strings.
- **The counters are counts only.** The cell published into the inspection
  plane holds counts and stable reason labels; it names no peer, no address,
  no key and no tag.
- **An unsolicited tag cannot enter the introducer set.** The recording is
  gated on the router's own outstanding-request set, so a peer that sends the
  block unbidden records nothing. This is the plan's central security
  property, and it is proved live by the lane.
- **The record's contact material is this router's own.** The record's
  `endpoint` and `intro_key` are the router's **own** bound endpoint and its
  **own** introduction key — the SSU 1 shape published the *target's* contact
  material, and the SSU2 published form carries neither. A record therefore
  cannot point a peer at a third party.
- **The publication cannot over-claim.** The acquisition builds no address
  and signs nothing, so it cannot claim a role it has no evidence for. The
  `C` advertisement stays a separate plan's subject.
- **No new attack surface.** The plan adds no message type, no block type, no
  listener and no socket; the request rides an authenticated session that
  already exists.

## Limitations

1. **The publication is not done.** The router now *learns* an introducer but
   never *publishes* the set it learned. The completion definition's
   **publishing** half is therefore **not** discharged, and the publication
   successor's subject.
2. **The `C` advertisement is not done.** The router reads the capability and
   does not advertise it.
3. **The stock-peer half is not proved and is not claimed.** The lane drives
   the daemon's owner and the runtime's live service over a **real** SSU2
   session between two daemon-owned services on loopback. It does not drive a
   stock peer, and the stock half needs Plan 493's authorized two-host
   topology.
4. **The rate is the router's own.** One request per window
   (`INTRODUCER_REQUEST_INTERVAL_MS`, sixty seconds), with a shorter retry
   (fifteen seconds) after a window that found no eligible peer. The
   reference asks on session establishment rather than on a cadence, so the
   *shape* differs even though the *condition* is the reference's own.
5. **The firewalled state must be earned.** The router asks only while its
   own reachability state is `Firewalled`, and that state needs corroborated
   evidence. A router that has never been observed is `Unknown` and asks
   nobody — which is the fail-closed behaviour, and also the reason a fresh
   router sends nothing.
6. **The capability is read from the destination store.** The read takes the
   peer's validated record from the router's authoritative store, which Plan
   506's reconcile keeps fed from the bootstrap store. A peer whose record has
   not reached that store is `NoStoredRecord` — refused, and counted.
7. **The daemon's `withheld_no_introducers` counter is still permanently
   zero** until the publication lands.

## Findings by severity

### Medium — none open

### Low — recorded, not fixed

- **L1 — the guard's rules did not fail closed on an absent item.** The first
  draft of the guard returned "no defect" when the item a rule names could
  not be found, which made a rule pass vacuously. Three of the inverted
  probes did not fire because of it. The guard now returns a **refusal** when
  a named item is absent, every rule was re-checked, and
  `the_guard_reports_a_missing_named_item` proves the property. Fixed in
  `6076ad7c` and `05c515cd`; recorded here because the failure mode — a rule
  that cannot fail — is the one a reviewer must be able to see.
- **L2 — the composition's publication half is absent.** The router learns an
  introducer and cannot publish it. A successor's subject.
- **L3 — the `C` advertisement is absent.** The router reads the capability
  and does not advertise its own.
- **L4 — the withdrawal form omits the `i` option.** A separate, smaller
  divergence on a path Plan 520 owns.
- **L5 — the daemon's `withheld_no_introducers` counter is permanently zero**
  until the publication lands.

### Informational

- **I1 — the registered §3's citations drifted.** The drift is recorded
  additively in the plan's §17.4, which carries a pointer table for every
  citation, and §3 itself is not rewritten.
- **I2 — the reference asks on session establishment, this router asks on a
  cadence.** The condition is the reference's own; the trigger differs, and
  the difference is recorded here rather than hidden.
- **I3 — the floor's clippy step caught an unused helper in the guard.** The
  guard was written from an existing template and carried an `impl_body`
  helper it never called; the floor's
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  failed on it with `dead_code`, and the helper was removed (`dadfcdb1`). The
  floor run that found it is recorded here rather than hidden: the first full
  floor on this plan's tree read **83 PASS, 1 FAIL**, and the FAIL was this
  lint.
- **I4 — the plan's §17.2 corrected a defect that would have made the plan
  unimplementable.** The registered design read the capability from the
  transport's validated peer record, which is empty on exactly the outbound
  sessions the request travels on. The correction was made before any
  production code was written against the registered design.

## Roadmap disposition

The roadmap's `network-participation` sheet records **Plan 524** as closed.
The row it carries names the closure.

The consequences that changed:

- the **Transports** row now names the acquisition: the router learns a
  peer's advertised introducer capability, asks one such peer for a relay tag
  while it is firewalled, and records the answer as an authenticated
  introducer;
- the roadmap's **completion definition** (§11) still requires that the router
  "publishes the validated introducer set in its RouterInfo address", and that
  half is **not** discharged: this plan acquires the introducers and does not
  publish them.

The **RouterInfo**, **Peers**, **NetDb**, **Roles**, **I2NP** and **Clients**
rows are unchanged.

The **stock-peer** half of this plan, Plan 520's stock-peer half, Plan 497's
M2, Plan 487's stock-peer half, and Plans 519–522's stock-peer halves all stay
blocked on **Plan 493's** authorized two-host topology.

## Unblock audit

The unblock audit asks, of every blocked item this line carries, whether this
plan changed its status. The answer is **no**, except for the one item this
plan was opened to discharge.

| Blocked item | Before Plan 524 | After Plan 524 |
| --- | --- | --- |
| The **acquisition** — learn a peer's advertised capability, ask for a relay tag, record the answer | missing | **delivered** — the read, the request and the recording, behind a default-off opt-in |
| The **publication** — build the introducer-only address, choose it over the direct form, sign it | missing | missing — untouched; a successor's subject |
| The **`C` advertisement** | missing | missing — untouched; a separate plan's subject |
| Plan 524's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 520's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 521's stock-peer half | blocked on Plan 493, and on the router-level 231 responder | blocked on Plan 493, and on the router-level 231 responder |
| Plan 522's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 497's **M2** / limitation 1 — the stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plan 487's stock-peer half | blocked on Plan 493 | blocked on Plan 493 |
| Plans 519–522's stock-peer halves | blocked on Plan 493 | blocked on Plan 493 |
| Plans 433, 435, 436, 437, 438, 439, 493, 494 | blocked on external environments | blocked on external environments — untouched |
| The **dial scope** — the transport's own loopback/RFC1918 profile | a transport boundary | a transport boundary — untouched, with its own plan-of-record needed |
| The no-**NTCP2**-ingress and no-**OBEP**-consumer gaps | missing | missing — untouched |
| The **`caps=f`** advertisement | Plan 494, blocked on Plan 493 | Plan 494, blocked on Plan 493 |
| The **withdrawal form's `i` option** | missing | missing — untouched; a path Plan 520 owns |

Nothing was unblocked that this plan did not itself open, and nothing was
promoted. Plan 493's two-host topology is the gate on every remaining
stock-peer half.

## Handoff notes

- The next free plan number after this one is **525**.
- The reference pins are unchanged: i2pd `2.61.0`
  (`635b013a612ff47278ef02acf8580a28e10e26c5`), Java I2P `2.13.0`
  (`9134f808337b401e8e53c73734c81fab04280c9d`). Neither reference is patched
  or vendored.
- **The composition is not finished, and the two halves must not be
  confused.** This plan delivered the **acquisition**: the router learns a
  peer's advertised introducer capability, asks such a peer for a relay tag
  while it is firewalled, and records the answer as an authenticated
  introducer. The **publication** — building the introducer-only address from
  the table, choosing it over the direct form, and signing it — is the next
  subject, and its sites are already located:
  `crates/i2pr-runtime/src/ssu2_runtime.rs:2586`
  (`introducer_publication_material`) and
  `crates/i2pr-netdb/src/local.rs:292`
  (`build_with_ssu2_introducer_address`). The **`C` advertisement** — the
  capability this router publishes about **itself** — is a **third** matter
  and is not the publication's either.
- **Plan 523's closure record states that the router still never asks a peer
  for a relay tag, never records an introducer and never publishes an
  introducer set.** This plan discharges the **first two** of those. The
  closed record is **not** rewritten; the correction lives in this record and
  in this plan's registry row.
- The daemon's `withheld_no_introducers` counter (`self_publication.rs:125`,
  `:157`, `:186`, `:224`, `:270`) is permanently zero until the publication
  calls `introducer_publication_material`.
- The **stock-peer** half stays blocked on Plan 493's authorized two-host
  topology.
- The **dial-scope** limit is a transport boundary and needs its own
  plan-of-record; it is not discharged here.
