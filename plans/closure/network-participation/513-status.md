# Plan 513 — the live NetDB discovery spine

**Closure record.** The plan of record is
`plans/implementation/network-participation/513-live-netdb-discovery-spine.md`. This record is the authority on what the plan closed,
what it did not, and on what the evidence is.

## Status

Plan 513 is **closed**. The router now runs a **live DHT discovery spine**: an
exploratory lookup that names peers, a RouterInfo-kind lookup that resolves
those names into records the router can ask, and a shared routing table that
is genuinely read in production.

Plan 497's **M2** (*the shared routing-table consumer has no production
reader*) is **discharged**, and so is **limitation 1**'s read half (*the table
is populated and persisted in a live daemon, and no live lookup consults it
yet*).

The **stock-peer** half of both remains external and is claimed nowhere: the
qualification that would exercise a real exploration and a real RouterInfo
lookup against a pinned stock router needs the two-host topology Plan 493
owns, and Plan 493 is blocked on an unavailable environment.

## Commits

| Commit | Subject |
| --- | --- |
| `87815672` | docs(plans): register Plan 513, the live NetDB discovery spine |
| `fead24bd` | docs(specs): record the references' discovery behaviour for Plan 513 |
| `7f4cff12` | feat(daemon): give the router a live NetDB discovery spine |
| `b651d5cb` | docs(daemon): record Plan 513's spine, its guard and the refused-suggestion fix |
| `e8904229` | test(daemon): pin both Plan 513 surfaces in the advertisement guard |
| `bab35979` | fix(daemon): the floor's two failures — a formatting slip and a rustdoc link |

The closing commit is the one that adds this record and the registry and
roadmap rows.

## What changed

### The gap

The router ran **only** one kind of NetDB lookup — `LeaseSet2` — and only
against peers it already knew. The **exploratory** lookup (wire type `3`) and
the **RouterInfo** lookup (wire type `2`) existed in the source and had **no
production caller**. The routing table was populated and persisted in a live
daemon while **no live lookup consulted it**.

That is the core of what *core router functionality* means for a DHT member: a
router that cannot find a peer can only talk to the peers it was handed.

Plan 497 recorded the gap as **M2**, and its limitation 1 recorded the
consequence. Plan 433 owns the **coordinator's** composition and is blocked,
but the blocker here was **plan ownership**, not an environment: the live
lookup path is the coordinator's own lookup path, which never had a reader.

### What the tree already had

More than the plan assumed, and the plan's own note that the tree "already
had" most of the machinery was right:

- the **lookup state machines** — `RouterInfoLookup::start`,
  `start_with_candidates` and, since Plan 497, `start_with_preferred`;
- the **exploratory job** — `ExplorationJob::begin`, with its peer budget, its
  deadline, its outstanding-request table and its reply accounting;
- the **reply-path provider** — `ExploratoryPoolReplyPathProvider`, which
  answers with the pool's oldest valid **inbound** tunnel;
- the **canonical outbound composition** — `compose_outbound_lookup`, which
  builds the cells a request leaves through an `OutboundGatewayRole`;
- the **routing table** — `BucketSet`, its day-keyed rotation, its
  closest-first selection, and the rule that a **suggestion** is accepted only
  from a peer the table holds with signed evidence;
- the **inbound recovery path** — `inbound_dispatch::dispatch_inbound_tunnel_data`
  already recovers a `DatabaseSearchReply` out of a TunnelData cell.

What was missing was the **composition**: nothing called the exploratory start,
nothing called the RouterInfo-kind start, and nothing read the table.

### The derivation — what the reference does, and what was taken from it

The references are recorded in `specs/QUALIFICATION-GATES.md` §12 with the
i2pd 2.61.0 citations at the exact pin.

**i2pd 2.61.0's exploration** (`libi2pd/NetDbRequests.cpp:459-507`): it takes
the pool's own outbound and inbound tunnels (`:462-465`), picks a random
search key (`:472`), asks the closest floodfill (`:479`), and — when it is
**not** connected directly — sends the request **through the tunnels** with
the inbound tunnel as the reply path (`:484-497`, the explore message at
`:496`). Its **direct** branch (`:499-500`) sends a message whose
`exploratory` flag is **false** (`libi2pd/NetDbRequests.cpp:55-65`,
`libi2pd/I2NPProtocol.cpp:158`): i2pd's direct fallback for an exploration is
a **type-2** RouterInfo lookup, not a type-3 exploratory lookup.

**What i2pr takes.** The RouterInfo-kind lookup rides the **outbound**
exploratory tunnel and takes its reply through the **inbound** one, because
Plan 102's closed authority amendment forbids substituting a direct peer link
for the exploratory-tunnel path. The **exploratory** request is delivered
**directly** to the floodfill being asked, as D3 says.

**What i2pr does not take.** i2pd's direct fallback (the type-2 downgrade) is
**not** implemented, and i2pd's exploratory request is **not** sent through the
tunnels. Both differences are divergences from the reference and are recorded
in the limitations below.

**The Java caveat.** The pinned Java artifact (2.13.0) is not on this host and
the local installation's jars carry **no** `router/netdb` package, so there is
**no** Java reading of the exploration or of the RouterInfo request path. That
is recorded in `specs/QUALIFICATION-GATES.md` §12.6 rather than papered over.

### The pieces

**The spine** (`crates/i2pr-daemon/src/netdb_participation.rs`). One bounded
owner of the router's DHT discovery: the exploratory start, the RouterInfo-kind
start, the reply ingestion, the unresolved-peer queue, and the counted
outcomes. It owns **no task, no timer, no listener and no socket**.

**The cycle** (`crates/i2pr-daemon/src/lib.rs`,
`NetDbParticipation::drive_discovery`). Four ordered steps: await a resolution
in flight; await an exploration in flight; resolve a named peer an earlier
exploration found; otherwise start an exploration. The order is D1: an
exploration's answer is peer **hashes**, and a hash the router cannot resolve
is a name, not a peer.

**The two deliveries**
(`crates/i2pr-daemon/src/service_product.rs`). `deliver_discovery_direct` for
the exploratory kind; `deliver_discovery_via_outbound_tunnel` for the
RouterInfo kind. A router that holds no usable outbound exploratory role
**refuses** the RouterInfo-kind request rather than sending it directly, and
the refusal is counted.

**The reply path**
(`crates/i2pr-daemon/src/exploratory_build.rs`). The exploratory pool's oldest
valid **inbound** tunnel, read from the pool's own registrations, so a
destination-role slot can never be handed out as the discovery reply path.

**The record's landing**
(`crates/i2pr-daemon/src/bootstrap.rs`). `Bootstrap::store_mut`, the narrow
mutating accessor the RouterInfo-kind lookup writes the validated record
through, so the named peer becomes a peer the router can ask.

### The lane

`crates/i2pr-daemon/tests/netdb_discovery_live.rs` — **15 tests**, all green,
through the **public** daemon API only. Fixed seeds, an explicit `now_ms`
everywhere, no socket, no environment variable, no `#[ignore]`, no stock peer
and no sleeping.

### The guard

`crates/i2pr-daemon/tests/netdb_discovery_boundary.rs` — **12 structural rules
and 12 inverted probes**, all green. Each rule asserts that a **named
function's own body** contains the required **call**, with its open
parenthesis; comments are stripped, `#[cfg(test)]` blocks are excluded, and an
**absent** function fails closed. Two rules span two modules, because the
invariant does.

## Requirement-to-evidence matrix

| Plan §11 requirement | Where | Asserts | Evidence |
| --- | --- | --- | --- |
| the exploration starts from a production entry point | lane + guard | the gap is closed | `the_exploration_names_the_peer_and_the_transport_seam`; guard rule 1 |
| the exploratory lookup's dispatch names a peer and the transport seam | lane | the request is really sent | `the_exploration_names_the_peer_and_the_transport_seam` |
| a `DatabaseSearchReply` is ingested and its peers indexed as suggestions | lane | the answer is used | `the_explorations_answer_queues_the_named_peer_for_resolution`; `a_named_peer_is_offered_to_the_table_as_a_suggestion_not_as_evidence` |
| a suggestion is never indexed as evidence | lane + guard | Plan 489's rule | `a_named_peer_is_offered_to_the_table_as_a_suggestion_not_as_evidence`; guard rule 10 |
| the RouterInfo-kind lookup takes its reply path from an **inbound** exploratory tunnel | lane + guard | Plan 102's amendment | `the_routerinfo_request_carries_the_inbound_exploratory_tunnels_reply_path`; guard rules 4 and 5 |
| the table's preference **reorders** and never **narrows** the live selection | lane + guard | Plan 497's M1 rule | `the_table_preference_reorders_and_never_narrows_the_selection`; guard rule 6 |
| the version gate survives the table's preference | guard | Plan 491/503's rule | guard rule 7 — both DSM-gated publication paths consult `select_version_gated_floodfills` and **never** consult the routing table |
| every fallback is counted by reason | lane | no silent fallback | `every_fallback_is_counted_by_reason`; guard rule 8 |
| the guard's rules, each with an inverted probe | guard | the shape is pinned | 24 tests, 12 of them inverted probes |

### Two requirements the plan states that the implementation answers on a different basis

**The version gate.** The plan's D4 says every candidate, table-sourced or not,
is still version-gated. Plan 491's DSM gate is scoped **by its own definition**
— `dsm_target_eligibility` — to the peers the router sends a **DSM** to, and
that is the two **publication** paths. Plan 513 adds no DSM and changes
neither publication path. The routing table's preference is consulted only on
the **lookup** paths, which the tree has never version-gated — Plan 503's
record says so explicitly and calls the lookup-target gate an open finding.
So the gate is not bypassed by the preference; the preference never reaches a
gated path. The guard pins exactly that, and the requirement is discharged on
that stated basis rather than by adding a gate the plan did not ask for. The
lookup path's ungated status is **pre-existing** and is recorded as Plan 503's
open finding, not introduced here.

**The exploratory lookup's reply.** The plan's D3 says the answer comes back
**directly**. The implementation sends the exploratory request **directly** and
takes the **reply** through the inbound exploratory tunnel. The reason is
mechanical and was verified against the tree: the daemon's inbound path
already recovers a `DatabaseSearchReply` from a TunnelData cell
(`inbound_dispatch::dispatch_inbound_tunnel_data` →
`InboundDispatchOutcome::DatabaseSearchReplyComplete`), and the reference's own
exploratory request uses the inbound tunnel as its reply path
(`libi2pd/NetDbRequests.cpp:496`). A directly-returning reply would need a
**new** ingress path for an I2NP body addressed to the router's own
RouterInfo — a second delivery path, which D2 forbids and which the plan did
not budget for. The deviation is recorded here, in the limitations, and in the
deep-dive; the plan of record is **not** rewritten.

## Commands run

The full routine floor from `AGENTS.md`, run on the closing tree:

| # | Step | Outcome | Seconds |
| --- | --- | --- | --- |
| 1 | `cargo-fmt` | **PASS** | 4 |
| 2 | `cargo-check` | **PASS** | 21 |
| 3 | `cargo-build-siblings` | **PASS** | 0 |
| 4 | `cargo-build-static-fixture` | **PASS** | 0 |
| 5 | `cargo-copy-static-fixture` | **PASS** | 1 |
| 6 | `cargo-rebuild-plain-fixture` | **PASS** | 0 |
| 7 | `cargo-test-workspace` | **PASS** | 1927 |
| 8 | `cargo-clippy` | **PASS** | 1 |
| 9 | `cargo-doc` | **PASS** | 0 |
| 10 | `cargo-test-doc` | **PASS** | 4 |
| 11 | `check-dependency-direction` | **PASS** | 0 |
| 12 | `check-global-plan-number-uniqueness` | **PASS** | 0 |
| 13 | `check-adr-number-uniqueness` | **PASS** | 0 |
| 14 | `check-portable-service-tunnel-api` | **PASS** | 0 |
| 15 | `check-portable-service-tunnel-consumer` | **PASS** | 6 |
| 16 | `planning-tests` | **PASS** | 6 |
| 17 | `check-runtime-boundaries` | **PASS** | 1 |
| 18 | `check-console-boundaries` | **PASS** | 1 |
| 19 | `check-console-browser-security` | **PASS** | 0 |
| 20 | `check-tooling-inventory` | **PASS** | 0 |
| 21 | `check-license-metadata` | **PASS** | 0 |
| 22 | `check-managed-app-private-client-seams` | **PASS** | 0 |
| 23 | `check-router-readiness-contract` | **PASS** | 0 |
| 24 | `check-router-readiness-contract-self` | **PASS** | 0 |
| 25 | `check-core-router-recovery-contract` | **PASS** | 0 |
| 26 | `check-core-router-recovery-contract-self` | **PASS** | 0 |
| 27 | `plan449-ssu2-preflight-self` | **PASS** | 0 |
| 28 | `plan495-peer-eligibility-self` | **PASS** | 0 |
| 29 | `plan495-peer-eligibility-profile-drift` | **PASS** | 0 |
| 30 | `plan476-gate-binding` | **PASS** | 1 |
| 31 | `plan476-gate-binding-self` | **PASS** | 0 |
| 32 | `check-managed-app-package-boundary` | **PASS** | 0 |
| 33 | `check-managed-app-package-boundary-self` | **PASS** | 0 |
| 34 | `check-managed-app-policy-boundary` | **PASS** | 0 |
| 35 | `check-managed-app-policy-boundary-self` | **PASS** | 1 |
| 36 | `check-managed-app-secured-sandbox` | **PASS** | 0 |
| 37 | `check-managed-app-secured-sandbox-self` | **PASS** | 0 |
| 38 | `check-service-tunnel-boundaries` | **PASS** | 0 |
| 39 | `check-managed-app-gateway-boundary` | **PASS** | 0 |
| 40 | `check-managed-app-manager-boundary` | **PASS** | 0 |
| 41 | `check-m11-per-epoch-composition` | **PASS** | 0 |
| 42 | `check-service-anonymity-boundaries` | **PASS** | 0 |
| 43 | `check-fixture-manifest` | **PASS** | 0 |
| 44 | `check-ntcp2-vectors` | **PASS** | 0 |
| 45 | `check-ssu2-vectors` | **PASS** | 0 |
| 46 | `check-i2cp-vectors` | **PASS** | 0 |
| 47 | `check-ntcp2-interoperability` | **PASS** | 0 |
| 48 | `check-constrained-host-lane-boundary` | **PASS** | 0 |
| 49 | `check-m11-transit-boundaries` | **PASS** | 3 |
| 50 | `check-m11-transit-qualification-evidence` | **PASS** | 1 |
| 51 | `check-sam-acceptance-evidence` | **PASS** | 0 |
| 52 | `check-ssu2-acceptance-evidence` | **PASS** | 0 |
| 53 | `check-i2cp-acceptance-evidence` | **PASS** | 0 |
| 54 | `check-i2pcontrol-acceptance-evidence` | **PASS** | 0 |
| 55 | `check-els2-type11-transcript-boundary` | **PASS** | 0 |
| 56 | `check-els2-live-lane-evidence` | **PASS** | 2 |
| 57 | `check-encrypted-service-consumer-caller` | **PASS** | 1 |
| 58 | `check-outproxy-request-path` | **PASS** | 0 |
| 59 | `check-outproxy-wire-lane-evidence` | **PASS** | 0 |
| 60 | `check-config-secret-hygiene` | **PASS** | 0 |
| 61 | `check-workflow-validity` | **PASS** | 0 |
| 62 | `check-floodfill-type5-serve` | **PASS** | 0 |
| 63 | `check-service-tunnel-acceptance-evidence` | **PASS** | 1 |
| 64 | `check-exploratory-tunnel-evidence` | **PASS** | 0 |
| 65 | `check-netdb-tunnel-evidence` | **PASS** | 0 |
| 66 | `check-destination-tunnel-evidence` | **PASS** | 0 |
| 67 | `check-streaming-tunnel-evidence` | **PASS** | 5 |
| 68 | `check-m6-mixed-router-acceptance-evidence` | **PASS** | 5 |
| 69 | `check-m12-floodfill-qualification-evidence-self` | **PASS** | 0 |
| 70 | `check-m12-floodfill-boundaries-self` | **PASS** | 1 |
| 71 | `ntcp2-harness-tests` | **PASS** | 0 |
| 72 | `plan488-mesh-lane` | **PASS** | 0 |
| 73 | `plan488-evidence-deterministic` | **PASS** | 0 |
| 74 | `plan489-mesh-lane` | **PASS** | 1 |
| 75 | `plan489-evidence-deterministic` | **PASS** | 0 |
| 76 | `plan490-mesh-lane` | **PASS** | 0 |
| 77 | `plan490-evidence-deterministic` | **PASS** | 0 |
| 78 | `plan490-ssu2-transport-suite` | **PASS** | 6 |
| 79 | `plan490-transport-boundary-guard` | **PASS** | 0 |
| 80 | `plan491-mesh-lane` | **PASS** | 1 |
| 81 | `plan491-evidence-deterministic` | **PASS** | 0 |
| 82 | `plan491-boundary-guard` | **PASS** | 0 |
| 83 | `plan491-proto-suite` | **PASS** | 0 |
| 84 | `plan491-netdb-suite` | **PASS** | 87 |
| 85 | `plan491-client-suite` | **PASS** | 24 |
| 86 | `plan492-posture-suite` | **PASS** | 1 |
| 87 | `plan492-m11-lane` | **PASS** | 4 |
| 88 | `plan502-transit-composition-lane` | **PASS** | 0 |
| 89 | `plan502-transit-composition-unit` | **PASS** | 1 |
| 90 | `plan502-transit-boundary-self` | **PASS** | 0 |
| 91 | `plan496-private-lan-profile` | **PASS** | 0 |
| 92 | `plan496-ntcp2-activation-gate` | **PASS** | 0 |
| 93 | `plan497-routing-consumer-lane` | **PASS** | 1 |
| 94 | `plan497-netdb-preferred-order` | **PASS** | 0 |
| 95 | `plan497-daemon-participation-unit` | **PASS** | 2 |
| 96 | `plan498-introducer-handoff-lane` | **PASS** | 1 |
| 97 | `plan498-introducer-mesh-lane` | **PASS** | 1 |
| 98 | `plan498-bootstrap-resolver-row` | **PASS** | 25 |
| 99 | `plan498-runtime-relay-accessor` | **PASS** | 4 |
| 100 | `plan503-version-eligibility-live` | **PASS** | 1 |
| 101 | `plan503-version-eligibility-unit` | **PASS** | 0 |
| 102 | `plan504-liveness-boundary` | **PASS** | 1 |
| 103 | `plan504-liveness-live` | **PASS** | 0 |
| 104 | `plan504-liveness-unit` | **PASS** | 0 |
| 105 | `plan505-driver-boundary` | **PASS** | 1 |
| 106 | `plan505-driver-live` | **PASS** | 2 |
| 107 | `plan505-driver-unit` | **PASS** | 1 |
| 108 | `plan506-reconcile-boundary` | **PASS** | 4 |
| 109 | `plan506-reconcile-live` | **PASS** | 1 |
| 110 | `plan507-retention-feasibility` | **PASS** | 1 |
| 111 | `plan507-retention-boundary` | **PASS** | 1 |
| 112 | `plan507-retention-live` | **PASS** | 0 |
| 113 | `plan508-creator-retention-feasibility` | **PASS** | 1 |
| 114 | `plan508-creator-retention-boundary` | **PASS** | 0 |
| 115 | `plan508-creator-retention-live` | **PASS** | 1 |
| 116 | `plan509-lookup-target-decision` | **PASS** | 0 |
| 117 | `plan509-lookup-target-boundary` | **PASS** | 1 |
| 118 | `plan510-managed-staging-load-lane` | **PASS** | 0 |
| 119 | `plan510-managed-reason-boundary` | **PASS** | 1 |
| 120 | `plan510-tunnelmanager-black-box` | **PASS** | 3 |
| 121 | `plan511-version-ceiling-derivation` | **PASS** | 1 |
| 122 | `plan511-version-ceiling-boundary` | **PASS** | 0 |
| 123 | `plan512-destination-liveness-live` | **PASS** | 0 |
| 124 | `plan512-destination-liveness-boundary` | **PASS** | 0 |
| 125 | `plan489-peer-budget-receipt` | **PASS** | 1 |
| 126 | `plan513-discovery-live` | **PASS** | 2 |
| 127 | `plan513-discovery-boundary` | **PASS** | 0 |
| 128 | `plan513-discovery-unit` | **PASS** | 2 |
| 129 | `plan513-discovery-seam-unit` | **PASS** | 0 |
| 130 | `cargo-deny` | **PASS** | 1 |

**130 steps, 130 PASS, 0 FAIL.**

### Two failures the floor found, and fixed

The floor's **first** run reported **two** failures, both of them this
plan's own, and its results are **not** cited as evidence:

1. **`cargo fmt --all --check`** failed on the guard file the advertisement
   rule was patched into. The patch was written by a script and had not been
   formatted.
2. **`cargo doc`** failed on a doc comment that linked to the consumer's
   private `RoutingTableConsumerState`
   (`rustdoc::private-intra-doc-links`, denied by the floor's
   `RUSTDOCFLAGS="-D warnings"`). The item is private by design, so the
   sentence now names it as plain text instead of linking it.

Both were fixed (`bab35979`), the floor was **re-run in full** on the closing
tree, and the table above is that second run. The first run's numbers are
recorded here rather than cited, so a reader can see that the floor was not
green on the first attempt and why.

### The focused verification

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked -p i2pr-daemon --all-targets` | PASS |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test netdb_discovery_live -- --test-threads=1` | 15 passed, 0 failed |
| `cargo test --locked -p i2pr-daemon --test netdb_discovery_boundary -- --test-threads=1` | 24 passed, 0 failed |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 tests, OK |
| `bash scripts/check-config-secret-hygiene.sh` | PASS |

## The fourteen negative controls, verified on the real tree

| # | The mutation | Caught by | Outcome |
| --- | --- | --- | --- |
| A | the exploratory lookup's wire type becomes 2 | the lane | **CAUGHT** |
| B | the RouterInfo-kind lookup's wire type becomes 3 | the lane | **CAUGHT** |
| C | the RouterInfo-kind request loses its reply path | the lane | **CAUGHT** |
| D | a direct-send fallback appears in the outbound-tunnel delivery | the guard | **CAUGHT** |
| E | the table's preference narrows the selection | the guard | **CAUGHT** |
| F | the table accepts a suggestion from an unvouched source | the lane | **CAUGHT** |
| G | a reply from a peer the exploration never asked is accepted | the lane | **CAUGHT** |
| H | the table's fallback is not counted by reason | the lane | **CAUGHT** |
| I | the unresolved-peer queue loses its bound | the guard | **CAUGHT** |
| J | the discovery surface is advertised | the guard | **CAUGHT** |
| N | the discovery spine's own surface is advertised | the guard | **CAUGHT** |
| K | the resolved record never completes the lookup | the lane | **CAUGHT** |
| L | the exploration is never finished once its reply arrives | the lane | **CAUGHT** |
| M | an unanswered request is never abandoned | the lane | **CAUGHT** |

**14 controls, 14 caught by an assertion, 0 build-refused, 0 vacuous.** Every mutation was reverted byte-for-byte and the file's SHA-256 was asserted after each revert.

Each control mutates the real tree, runs **one** executable check, and
requires that the check **fails on its assertion**. A control the build
refuses is weaker than one that breaks an assertion; a control that changes
the code without changing the observable is **vacuous**. Both are reported,
not absorbed. Every mutation is reverted **byte-for-byte** and the file's
SHA-256 is asserted to be the one recorded before the control ran.

### Two weaknesses found while building the controls, and fixed

**Control J was vacuous on its first two runs.** It flipped the **first**
textual occurrence of `advertised = false` in `specs/support.toml`, which was
the phrase inside the surface's **notes** prose rather than its declaration —
and the second attempt flipped the declaration of a **different** surface,
because the inventory holds 105 of them. The control now scopes the flip to
the surface's own `[[surface]]` block and to the declaration line inside it.
The **guard** was sound throughout: its `block_declares` helper reads the
line's **name**, so a mention inside a `notes` string never satisfies the rule,
and the guard's own inverted probe already proved that.

**The guard was strengthened because of it.** The advertisement rule pinned
only `netdb.live-routerinfo-lookup`. Plan 513 adds a **second** surface,
`daemon.netdb-discovery-spine`, and a rule that pins one surface does not pin
the plan. The rule now reads **both**, and its inverted probe covers both.

The controls were re-run in full against the final tree after both fixes. The
**first** run's results are not cited.

## Invariant review

| Invariant | Holds? | Evidence |
| --- | --- | --- |
| The live LeaseSet2 lookup's behaviour is unchanged | yes | the LeaseSet2 start and its state machine are untouched; `netdb_seam.rs` changed, but only by **adding** the RouterInfo-kind and Exploratory-kind starts and the ingestion helpers beside the existing LeaseSet2 start |
| The RouterInfo-kind request rides the outbound exploratory tunnel | yes | `deliver_discovery_via_outbound_tunnel`; guard rule 5; the refusal is counted when no usable role exists |
| The RouterInfo-kind reply travels the inbound exploratory tunnel | yes | `the_routerinfo_request_carries_the_inbound_exploratory_tunnels_reply_path`; guard rule 4 |
| The table may only **reorder**, never **narrow** | yes | `the_table_preference_reorders_and_never_narrows_the_selection`; guard rule 6; the seam uses `start_with_preferred`, never `start_with_candidates` |
| The version gate is not bypassed | yes | guard rule 7 — the two DSM-gated publication paths never consult the routing table |
| Every fallback is counted by reason | yes | `every_fallback_is_counted_by_reason`; the spine counts `Disabled`, `NoCandidate` and `Unavailable` separately from the table's own counters |
| A suggestion is never evidence | yes | `a_named_peer_is_offered_to_the_table_as_a_suggestion_not_as_evidence`; guard rule 10 |
| One exploration, one resolution, one request at a time | yes | `an_exploration_is_finished_once_its_reply_arrives`; `a_request_the_wire_never_answered_is_abandoned_when_its_deadline_passes` |
| No advertisement | yes | `router.version` stays `0.9.62`; `caps` and the published RouterInfo are unchanged; the inventory's advertised count stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical |

## Failure review

- **A request that cannot be composed** — no reply path, no usable outbound
  exploratory role, or an empty candidate list — **refuses** and is counted.
  It is never downgraded to another delivery path: a RouterInfo-kind request
  is never sent directly as a fallback.
- **A fallback to the flat store's own selection** is counted **by reason**,
  on the table's counters and on the spine's.
- **A reply from a peer the exploration never asked** introduces **nothing**:
  the reply's sender is unauthenticated, so its names are neither indexed nor
  queued.
- **A suggestion the table refuses** is not queued for resolution. This was a
  **defect found by the lane** while it was being written — the refusal was
  counted and then ignored — and it was **fixed**, not absorbed.
- **A request the wire never answered** is abandoned when its deadline passes
  and counted. It is never retried in a loop.
- **A record that fails validation** changes nothing: the lookup engine
  refuses it, the store is not widened, and the peer leaves the unresolved
  queue only when a record **actually** arrives.
- **A poisoned routing table** is reported as `Unavailable` — its own reason,
  distinct from "the table held no candidate".
- **A cancellation** — the daemon shutting down — stops the tick; the spine
  holds no task of its own, so there is nothing to leak.

## Migration review

No configuration key is added. No persisted state changes. The published
RouterInfo, `caps` and `router.version` are untouched. A router built before
this plan and one built after it interoperate: the new lookups are ordinary
I2NP messages.

The routing table's **persisted projection** is unchanged — the plan adds a
reader, not a writer. The `daemon.netdb-discovery-spine` surface is new, and
the `netdb.live-routerinfo-lookup` surface moves from `deferred` to
`experimental` because the lane has proved it. Both stay `advertised = false`.

## Security review

- **No secret is handled, logged or serialized.** The spine's state is peer
  **hashes** and counters; no key material, no payload and no peer identity
  is written into a receipt, a log line or a projection.
- **A reply's sender is unauthenticated**, and the code treats it that way: a
  reply from a peer the exploration never asked introduces nothing, and a
  suggestion is accepted only from a peer the table holds with signed
  evidence.
- **The resolution is target-bound.** The lookup engine refuses a
  `DatabaseStore` that does not name the lookup's own target, so the router's
  peer population is never widened by an unrelated record.
- **The suggestion's source is vetted.** Only a suggestion the table
  **accepted** is queued for resolution, so an unvouched peer cannot make the
  router issue lookups for arbitrary hashes.
- **The queue is bounded** (`MAX_PENDING_RESOLUTIONS`), and the reply's own
  per-response ceiling bounds one reply's contribution.
- **No peer profile is written.** The failure and the suggestion decisions
  name a hash or a count, never a peer's identity, and a local condition is
  never recorded against a peer.
- **The direct delivery is the router's own transport seam.** The peer is
  named by the composition, and a peer with no established session reports
  `NoActiveSession` explicitly; the refusal is counted.

## Documentation and operations

- `specs/QUALIFICATION-GATES.md` §12 — the references' discovery behaviour,
  with the i2pd citations at the exact pin and the Java caveat.
- `specs/CONFORMANCE.md` — the Plan 513 section.
- `specs/support.toml` — `netdb.live-routerinfo-lookup` moves `deferred` →
  `experimental`; `daemon.netdb-discovery-spine` is added. The inventory now
  holds **105 surfaces, 100 experimental, 4 deferred, 1 ready, 0 advertised**.
- `docs/architecture/i2pr-daemon.md` — the Plan 513 section, the module table
  re-corrected for the seven changed files, and **every** line citation into
  those files re-verified against the live source.
- `plans/registry.md` and `plans/subsystems/network-participation-roadmap.md` —
  Plan 513's row, and Plan 497's row carrying M2's discharge.
- `plans/subsystems/core-router-recovery-roadmap.md` — Plan 512's stale
  *"not yet written"* marker, cleared (finding **L3**). No closed closure
  record was rewritten; the marker was in the index.

**The doc drift this plan found and fixed.** The daemon deep-dive's citations
into the seven changed files were **stale**. The first mechanical pass
re-pointed them by a diff map and produced **many wrong** citations; its
results were **discarded**. The second pass resolved each citation by
**exact content match** in the live file and hand-resolved the fourteen whose
old line was a generic one-liner; a final audit read all 54 cited lines back
and confirmed each one. One piece of **prose** was stale as well: it said the
suggestion list was empty because Plan 489's exploration was not composed into
the service graph, which this plan made false.

### Deviations from the plan of record

The plan of record is **not** rewritten. The deviations are recorded here.

1. **The table's preference is consulted in `netdb_participation.rs`, not in
   `service_product.rs`.** The spine owns the discovery state, so the
   preference and the fallback counters live with it; the product's job is
   delivery. The plan's §6 table named `service_product.rs` for this row.
2. **`netdb_tunnels.rs` gained the free function
   `compose_lookup_via_outbound_tunnel`.** The spine must reach the canonical
   composition without constructing the never-composed coordinator. The
   coordinator's `compose_lookup_via_tunnel` now **delegates** to it with
   byte-identical behaviour, so there is one composition and not two. The plan
   did not list this file.
3. **`exploratory_build.rs` and `bootstrap.rs` changed.** The plan did not
   list them: the first gained the two accessors the delivery needs, the
   second the narrow accessor the resolved record is written through.
4. **The exploratory lookup's reply travels the inbound exploratory tunnel**,
   not directly — see the requirement matrix above.
5. **The plan's §11 row "the version gate survives the table's preference" is
   discharged on the stated basis** in the requirement matrix above, not by
   adding a version gate to the lookup path.

## Limitations

1. **No stock-peer evidence.** The mechanism is landed and proven over the
   tree's own deterministic surfaces. The qualification that would exercise a
   real exploration and a real RouterInfo lookup against a pinned stock router
   is **external** and needs the two-host topology Plan 493 owns.
2. **The reference's direct fallback is not implemented.** i2pd 2.61.0, unable
   to reach a floodfill directly, downgrades the exploratory request to a
   **type-2** RouterInfo lookup. `i2pr` refuses the request instead. The
   difference is a divergence from the reference, recorded rather than
   absorbed.
3. **The reference's exploratory request is not sent through the tunnels.**
   i2pd sends it through the exploratory tunnels when it has both roles;
   `i2pr` sends the exploratory request directly. The reference's tunnel
   branch is a reachability optimisation, and `i2pr`'s floodfills are reached
   over its established transport seam.
4. **The lookup path is still not version-gated.** That is Plan 503's open
   finding and this plan does not change it; the gate's scope is recorded
   rather than assumed.
5. **The live inbound path's carriage and the drain** are unchanged; the
   deep-dive's citations for them were re-pointed, and their behaviour was
   not touched.
6. **The discovery cycle is driven at the participation owner's cadence**, not
   at a discovery-specific one. A router whose participation owner is
   `Optional` and degraded therefore does not explore — which is the correct
   degradation, and is stated rather than hidden.

## Findings by severity

### Medium

None.

### Low

**L1 — The exploratory request's reply is tunnelled where the plan said
direct.** Recorded above. The reference's own behaviour supports the
tunnelled reply; the divergence is from the plan's prose, not from the
reference.

**L2 — The plan's §6 production-file table was incomplete.** Three files it
did not list changed, and the row that named `service_product.rs` for the
table's preference landed in `netdb_participation.rs`. Recorded in the
deviations.

**L3 — The live index carried three stale "not yet written" markers.** The
closure records they named all exist. Plan 512's registry row and Plan 512's
core-router-recovery roadmap row still said its closure record was *"not yet
written"*, and Plan 509's network-participation roadmap row said the same of
its own. All three were cleared in this closing commit, and the markers are
verified absent from the three live index files. **No closure record was
rewritten** — the markers were in the index, not in a record. The same commit
also brought Plan 513's own roadmap row to the house convention for a closed
row: the plan-of-record path in the fourth cell, and the closure path, the
implementation commit and the delivery in the fifth, instead of the row's
registration-time problem description carried forward unchanged.

### Recorded, not defects

**R1 — The plan's D3 and the reference disagree on the exploratory request's
delivery.** i2pd sends the exploratory request through the exploratory tunnels
when it has both roles and falls back to a **type-2** lookup otherwise.
`i2pr` sends it directly and never downgrades. The reference's exact
behaviour is recorded in `specs/QUALIFICATION-GATES.md` §12.

**R2 — The tree has no Java reading of the exploration.** The pinned Java
artifact is not on this host and the local jars carry no `router/netdb`
package. Recorded in `specs/QUALIFICATION-GATES.md` §12.6.

**R3 — The daemon deep-dive's stale citations.** Fixed here; the first
mechanical pass's results were discarded rather than cited.

**R4 — The `MAX_ROUTING_SUGGESTIONS` list the plan's owner carried is gone.**
The spine's bounded unresolved-peer queue replaced it, so there is one queue
and not two, and the owner no longer re-indexes suggestions on every tick.

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md` — Plan 513's row moves
from **registered** to **closed**, its last cell is brought to the house
convention for a closed row (the closure path, the implementation commit and
the delivery, instead of the registration-time problem description), and §12
records that Plan 497's **M2** and limitation 1's read half are discharged.

Three stale *"not yet written"* markers were cleared from the live index in the
same commit — Plan 512's two rows and Plan 509's one. The closure records they
named all exist. No closed closure record was rewritten; the drift was in the
index, and it is recorded here as finding **L3**.

`plans/registry.md` — Plan 513's row moves to **closed**. Plan 497's row
carries **M2**'s discharge and limitation 1's read half, in the same row, so
the history is visible where the reader looks for it. **Plan 497's closure
record is not rewritten.**

**Still open, and owned elsewhere.**

- Plan 497's limitation 5's **stock-peer** half — Plan 493's topology.
- Plan 491's **F2** — no stock-peer evidence for the floors; Plan 493's
  topology.
- Plan 512's **stock-peer** half for Plan 487's **F3** — Plan 493's topology.
- The **lookup-target version gate** — Plan 503's open finding.
- Plan 433's coordinator composition — blocked.

## Unblock audit

**What Plan 513 closes unconditionally.**

- The **production reader** for the shared routing table: the live
  RouterInfo-kind lookup's candidate list is reordered by the table's
  preference, and every fallback is counted by reason.
- The **exploratory lookup** and the **RouterInfo-kind lookup** as live
  production paths, with their two distinct deliveries and the
  RouterInfo-kind request's exploratory-tunnel authority honoured.
- The **suggestion rule** with teeth: a refused suggestion is not acted on, an
  unknown peer's reply introduces nothing, and a resolved record is validated
  and lands in the router's NetDB.
- The **bounded, one-at-a-time cycle**, with an unanswered request abandoned
  and counted rather than retried in a loop.
- The structural wiring: twelve guarded rules with twelve inverted probes,
  and fourteen controls applied to the real tree and reverted byte-for-byte.

**What Plan 513 closes only conditionally.**

Plan 497's **M2** and limitation 1's read half, as a *stock-peer* finding. The
original observation was made against the tree, and this plan reproduces the
mechanism over the tree's own deterministic surfaces, not against a stock
peer. Both are therefore discharged **conditionally**, in the same sense Plan
510 discharged Plan 491's **F4** and Plan 512 discharged Plan 487's **F3**.

**The exact future evidence that would close them unconditionally.**

1. A **two-host** run — the topology Plan 493 owns, with one host running
   `i2pr` and the other a pinned stock router — in which a stock router
   answers an exploratory lookup and a RouterInfo lookup.
2. The observation that the **stock router's answer named peers**, that
   `i2pr` resolved one of them into a signed `RouterInfo`, and that the
   record entered `i2pr`'s NetDB.
3. The observation that `i2pr` then **asked** the resolved peer over its
   established session, and that the peer answered.

That evidence is **external** to this repository and to this environment, and
Plan 493 is blocked on an unavailable two-host topology. No part of it is
claimed here.

**The unblock path.** Plan 493's topology becomes available → run the
network-participation lane against a stock router that answers a RouterInfo
lookup → record the resolution and the successful ask in the evidence file →
update this closure record's status line and Plan 497's registry row from
**conditional** to **closed**. The mechanism, the lane and the guard do not
change; only the evidence does.

**Nothing outside `plans/` was updated to carry this plan's state.** No
`AGENTS.md` edit, no `README.md` edit, and no advertisement. The
specifications and the deep-dives were updated because they describe the
tree's **mechanism**, not the plan's state.
