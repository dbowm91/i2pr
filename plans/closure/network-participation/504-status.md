## Status

`passed-live-tunnel-liveness-in-the-product-composition`

Plan 504 is closed. The tunnel-liveness scheduler Plan 185 delivered — a
complete, bounded, unit-tested mechanism — **is now constructed by the
production daemon**, driven on the existing tick, wired to the existing
inbound seam, and **correlated authentically**.

**Plan 185's closure record said the daemon pump already called that seam.
That statement was false of the tree it described.** Plan 185's record is
**not** rewritten; the correction lives here and in the registry's row. The
precedent is Plan 503's: Plan 491's false live-enforcement claim was
corrected in Plan 503's closure record and in the registry's row.

**Plan 487 F3 is advanced, not closed.** Its destination-inbound half is
**not** closed by this plan, and the reason is concrete rather than
rhetorical: the only in-protocol signal for a tunnel the router is the
endpoint of is **tunnel rekeying**, and the tree implements **no rekeying at
all** — no `rekey` symbol in `i2pr-proto`, `i2pr-tunnel` or `i2pr-client`,
and `TunnelDataMessage` has no rekey payload. That is a new I2NP mechanism
and needs its own plan-of-record.

**The measured limitation, stated plainly.** The production daemon installs
**no exploratory tunnels**: `ExploratoryBuildCoordinator::submit` is called
only from the out-of-crate lanes, while the daemon submits only *destination*
builds. So `exploratory_liveness_pairs()` returns empty in production today
and the scheduler tracks **zero pairs**. It is **installed and inert**, not
broken — and the proven-pair gate is exactly what makes that safe: nothing
is torn down. The population the scheduler watches is a **separate gap**, and
it is named below.

**No stock-peer evidence is claimed.** The liveness response half had never
been observed over a wire, with any peer, before this plan; the feasibility
artefact is a **local** one. Whether an ordinary router relays a probe back
to the named inbound gateway is **unproven**, and the proven-pair gate exists
precisely because that question is open.

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical.

## Commits

| Commit | Subject |
| --- | --- |
| `88950fe9` | docs(plans): register Plan 504, the tunnel liveness scheduler in the product composition |
| `78c8d24f` | docs(plans): correct Plan 504 with the liveness feasibility gate |
| `7fbbb1f5` | feat(daemon): the tunnel liveness scheduler in the product composition |
| `d28acd1f` | docs(plans): the Plan 504 conformance, deep-dive and support entry |
| `636f1a57` | test(daemon): the Plan 504 liveness guard and the live liveness lane |
| `<closure>` | docs(plans): close Plan 504 and advance the network-participation roadmap |

## What changed

### The gap

Plan 185 delivered `crates/i2pr-daemon/src/tunnel_liveness.rs`: a central
scheduler with a bounded pair table (8), a bounded pending-test table (16), a
30 s first test, a 60 s repeat, a 60 s response timeout below the
two-minute idle-deletion boundary, a two-consecutive-failure threshold, and
a typed `LivenessAction` the pump acts on.

**It was never constructed by the production daemon.** An exhaustive audit of
every `pub mod` in `crates/i2pr-daemon/src/lib.rs` — in-crate production
references against out-of-crate test references — returned exactly one
module with no production consumer:

```text
TEST-ONLY IN DAEMON: tunnel_liveness (prod=0 tests=11)
```

The live inbound path called the coordinator's dispatcher **directly**, in
four places (`crates/i2pr-daemon/src/service_product.rs:3957`, `:6177`,
`:7227`, `:7450`), and never the existing `route_inbound_with_liveness`
seam. Every inbound `DeliveryStatus` echo was therefore **discarded**:
nothing correlated it, because there was no pending test.

`plans/closure/mixed-router-interop/185-status.md:71-73` states:

> `route_inbound_with_liveness` is the single helper the **daemon pump
> calls** to thread the Plan 184 central dispatcher into both the build
> coordinator and the liveness scheduler.

**That is false of this tree.**

### The feasibility answer, measured

Plan 504's first work package was an **executable artefact** that could stop
the plan, and it drives the **real** composition — it installs a real
outbound and inbound exploratory tunnel through the coordinator's own pool
and registry, composes the probe through the coordinator's real
`OutboundGatewayRole`, relays the resulting cell through the peer's real
tunnel roles exactly as a peer's answer arrives, and hands it to the
daemon's real inbound dispatch.

The answer was **yes** on all three questions — and the third one changed
the design:

1. a probe composed through the real outbound role **does** produce a
   response the real inbound dispatcher classifies as a `DeliveryStatus`;
2. that response **can** be attributed to one specific pending test, using
   the acknowledged-message id;
3. the answer arrives **tunneled**, through the inbound-tunnel path, not
   through the direct path.

Point 3 is why the live correlation is wired at **both** seams. Correlating
only the direct path would have meant the live router never saw its own
answer.

### The correlation is now authenticated

The dispatcher used to keep the *transport* message id and **discard**
`DeliveryStatusMessage::message_id` — the id of the message being
acknowledged, which is the id that identifies the test. The scheduler could
therefore only match "any `DeliveryStatus`, when exactly one test is
pending", as its own doc comment said.

`RouterI2npOutcome::RouterControl` now carries
`acknowledged_message_id: Option<u32>` — `Some` for `DeliveryStatus`, `None`
for the other three control bodies. The transport message id is untouched and
**no decoder is weakened**. `TunnelLivenessScheduler::record_authenticated_response`
credits exactly the probe that carried the id; an id that matches nothing is
counted as an **unauthenticated response** and credited to nothing.
`route_inbound_with_liveness` now uses the authenticated match; the existing
`record_inbound_outcome` fallback is unchanged and still exported for the
lanes that use it.

### The proven-pair gate — the safety property

A pair becomes **proven** only when it has produced an authenticated
response, and `drive()`'s `MarkUnhealthy` branch requires `pair.proven`.
Until the router has seen that peer answer, a missing response is
indistinguishable from *this peer does not answer*, and the router must not
act on a signal it cannot distinguish from its negation. A missing response
for an unproven pair is still counted, and the **withheld teardown** is
counted once per pair — both published, so an operator sees that the liveness
signal is not working rather than inferring it from silence.

### The probe reuses the canonical path

`compose_liveness_probe` calls the existing `compose_outbound_build_deliveries`
— the same `registry.outbound(slot)` role, the same cell composition, the
same `RouterDeliveryService` delivery as the build reply. The only change to
that shared function is that the delivery instruction became a parameter; the
build reply still passes `DeliveryInstruction::Router`. **No private parallel
pipeline.** The probe's payload is a `DeliveryStatusMessage` whose
acknowledged-message id **is** the probe id.

### An identifier-space collision, found and avoided

`DeliveryStatusMessage::message_id` is already the NetDB LS2 publication
reply token (Plan 487) — a monotonic counter from 1. A liveness id from that
space could be claimed by the publication path, so the liveness ids live in
the **high half** of the `u32` range (`LIVENESS_PROBE_MESSAGE_ID_BASE`),
disjoint from the publication token space.

### The exploratory/destination distinction is structural

`exploratory_liveness_pairs` reads **only** the exploratory pool's own
registration lists. A destination-role slot lives in the shared
`DataPlaneRegistry` but never in the pool's lists, so it cannot appear in a
pair. No slot-number test, no heuristic.

### The live wiring

- `crates/i2pr-daemon/src/service_product.rs` — the product owns the
  scheduler beside the coordinator; `drive_tunnel_liveness` runs on the
  **existing** 250 ms product tick with a bounded number of drives per tick
  (`MAX_LIVENESS_DRIVES_PER_TICK`); `sync_liveness_pairs` registers and
  unregisters the pairs; `emit_liveness_probe` and `remove_liveness_pair`
  act on the scheduler's typed actions; the four direct inbound sites route
  through the liveness seam, and the tunneled `DeliveryStatus` arm
  correlates the relayed answer.
- `crates/i2pr-daemon/src/exploratory_build.rs` —
  `exploratory_liveness_pairs`, `emit_liveness_probe`,
  `compose_liveness_probe`, `remove_exploratory_pair`, `rebuild_exploratory`.
- `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` — the liveness counters
  are published, following the existing `publish_*` / `*()` pattern.
- `crates/i2pr-daemon/src/lib.rs` — the composition publishes them.

**No new task, no new timer, no new listener, no new socket, no new
dependency.** The scheduler runs on the tick that already existed.

## Requirement-to-evidence matrix

The guard's rule numbers below are the **landed** numbering in
`crates/i2pr-daemon/tests/tunnel_liveness_boundary.rs`, not the plan's
working numbering. Eight rules are **structural** — they assert that a named
function's *own body* contains the required *call*, matched with its open
parenthesis. Three are **textual**.

| Requirement | Evidence |
| --- | --- |
| The feasibility question is answered by running code | `crates/i2pr-daemon/src/exploratory_build.rs` — the Plan 504 feasibility artefact |
| The live daemon constructs the scheduler | `crates/i2pr-daemon/src/service_product.rs` — `start` and `start_over_existing_daemon`; guard rule 1 (structural) |
| The live tick drives the scheduler | `crates/i2pr-daemon/src/service_product.rs` — `advance_destination_pools_at` → `drive_tunnel_liveness` → `drive()`; guard rule 4 (structural) |
| The live inbound path routes through the liveness seam | `crates/i2pr-daemon/src/service_product.rs` — the four sites in three functions; guard rule 2 (structural) |
| The seam is not bypassed again | `crates/i2pr-daemon/src/service_product.rs` — no `.route_inbound_i2np(` remains; guard rule 3 (textual) |
| The probe is emitted through the canonical path | `crates/i2pr-daemon/src/exploratory_build.rs` — `compose_liveness_probe` → `compose_outbound_build_deliveries`; guard rule 5 (structural) |
| No private parallel cell pipeline | `crates/i2pr-daemon/src/exploratory_build.rs` — exactly one `.forward_cells(` call site; guard rule 6 (textual) |
| The correlation is authenticated | `crates/i2pr-daemon/src/router_i2np.rs` — the acknowledged id; guard rule 9 (structural) |
| The scheduler matches on the acknowledged id | `crates/i2pr-daemon/src/tunnel_liveness.rs` — `record_authenticated_response`; guard rule 10 (structural) |
| The seam hands the id to the scheduler | `crates/i2pr-daemon/src/tunnel_liveness.rs` — `record_authenticated_outcome`; guard rule 11 (structural) |
| The unproven-pair gate exists | `crates/i2pr-daemon/src/tunnel_liveness.rs` — `pair.proven &&`; guard rule 7 (textual) |
| The unproven-pair gate guards the unhealthy verdict | `crates/i2pr-daemon/src/tunnel_liveness.rs` — the gate inside `drive()`; guard rule 8 (structural); the lane's explicit assertion |
| A destination tunnel is never tracked or removed | `crates/i2pr-daemon/src/exploratory_build.rs` — `exploratory_liveness_pairs`; the lane's assertion |
| The counters are observable | `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` — the published counters; the lane's assertion |
| The plan's claims are guarded, each with an inverted probe | `crates/i2pr-daemon/tests/tunnel_liveness_boundary.rs` — 26 tests |
| The live behaviour is proved over the public API | `crates/i2pr-daemon/tests/tunnel_liveness_live.rs` — 9 tests |
| Nothing is advertised | `specs/support.toml` — 96 surfaces, 0 advertised |
| No new dependency | `git diff --exit-code <base> HEAD -- Cargo.toml Cargo.lock` |

## Commands run

### The routine floor

105 steps, run on the closing tree. See the table below.

| # | Step | Result | Seconds |
| --- | --- | --- | --- |
| 1 | `cargo-fmt` | PASS | 4 |
| 2 | `cargo-check` | PASS | 1 |
| 3 | `cargo-build-siblings` | PASS | 0 |
| 4 | `cargo-build-static-fixture` | PASS | 0 |
| 5 | `cargo-copy-static-fixture` | PASS | 0 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 0 |
| 7 | `cargo-test-workspace` | PASS | 1132 |
| 8 | `cargo-clippy` | PASS | 1 |
| 9 | `cargo-doc` | PASS | 7 |
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
| 37 | `check-managed-app-secured-sandbox-self` | PASS | 1 |
| 38 | `check-service-tunnel-boundaries` | PASS | 0 |
| 39 | `check-managed-app-gateway-boundary` | PASS | 0 |
| 40 | `check-managed-app-manager-boundary` | PASS | 0 |
| 41 | `check-m11-per-epoch-composition` | PASS | 0 |
| 42 | `check-service-anonymity-boundaries` | PASS | 0 |
| 43 | `check-fixture-manifest` | PASS | 0 |
| 44 | `check-ntcp2-vectors` | PASS | 0 |
| 45 | `check-ssu2-vectors` | PASS | 0 |
| 46 | `check-i2cp-vectors` | PASS | 0 |
| 47 | `check-ntcp2-interoperability` | PASS | 1 |
| 48 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 49 | `check-m11-transit-boundaries` | PASS | 2 |
| 50 | `check-m11-transit-qualification-evidence` | PASS | 1 |
| 51 | `check-sam-acceptance-evidence` | PASS | 0 |
| 52 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 53 | `check-i2cp-acceptance-evidence` | PASS | 1 |
| 54 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 55 | `check-els2-type11-transcript-boundary` | PASS | 0 |
| 56 | `check-els2-live-lane-evidence` | PASS | 2 |
| 57 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 58 | `check-outproxy-request-path` | PASS | 0 |
| 59 | `check-outproxy-wire-lane-evidence` | PASS | 0 |
| 60 | `check-config-secret-hygiene` | PASS | 0 |
| 61 | `check-workflow-validity` | PASS | 0 |
| 62 | `check-floodfill-type5-serve` | PASS | 1 |
| 63 | `check-service-tunnel-acceptance-evidence` | PASS | 1 |
| 64 | `check-exploratory-tunnel-evidence` | PASS | 0 |
| 65 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 66 | `check-destination-tunnel-evidence` | PASS | 0 |
| 67 | `check-streaming-tunnel-evidence` | PASS | 5 |
| 68 | `check-m6-mixed-router-acceptance-evidence` | PASS | 4 |
| 69 | `check-m12-floodfill-qualification-evidence-self` | PASS | 1 |
| 70 | `check-m12-floodfill-boundaries-self` | PASS | 0 |
| 71 | `ntcp2-harness-tests` | PASS | 1 |
| 72 | `plan488-mesh-lane` | PASS | 0 |
| 73 | `plan488-evidence-deterministic` | PASS | 0 |
| 74 | `plan489-mesh-lane` | PASS | 0 |
| 75 | `plan489-evidence-deterministic` | PASS | 0 |
| 76 | `plan490-mesh-lane` | PASS | 0 |
| 77 | `plan490-evidence-deterministic` | PASS | 0 |
| 78 | `plan490-ssu2-transport-suite` | PASS | 6 |
| 79 | `plan490-transport-boundary-guard` | PASS | 1 |
| 80 | `plan491-mesh-lane` | PASS | 0 |
| 81 | `plan491-evidence-deterministic` | PASS | 0 |
| 82 | `plan491-boundary-guard` | PASS | 0 |
| 83 | `plan491-proto-suite` | PASS | 1 |
| 84 | `plan491-netdb-suite` | PASS | 87 |
| 85 | `plan491-client-suite` | PASS | 24 |
| 86 | `plan492-posture-suite` | PASS | 1 |
| 87 | `plan492-m11-lane` | PASS | 4 |
| 88 | `plan502-transit-composition-lane` | PASS | 0 |
| 89 | `plan502-transit-composition-unit` | PASS | 0 |
| 90 | `plan502-transit-boundary-self` | PASS | 0 |
| 91 | `plan496-private-lan-profile` | PASS | 1 |
| 92 | `plan496-ntcp2-activation-gate` | PASS | 0 |
| 93 | `plan497-routing-consumer-lane` | PASS | 0 |
| 94 | `plan497-netdb-preferred-order` | PASS | 1 |
| 95 | `plan497-daemon-participation-unit` | PASS | 2 |
| 96 | `plan498-introducer-handoff-lane` | PASS | 1 |
| 97 | `plan498-introducer-mesh-lane` | PASS | 0 |
| 98 | `plan498-bootstrap-resolver-row` | PASS | 25 |
| 99 | `plan498-runtime-relay-accessor` | PASS | 5 |
| 100 | `plan503-version-eligibility-live` | PASS | 1 |
| 101 | `plan503-version-eligibility-unit` | PASS | 0 |
| 102 | `plan504-liveness-boundary` | PASS | 0 |
| 103 | `plan504-liveness-live` | PASS | 1 |
| 104 | `plan504-liveness-unit` | PASS | 0 |
| 105 | `cargo-deny` | PASS | 1 |

**105 steps, 105 PASS, 0 FAIL.**


### The focused verification

| Command | Result |
| --- | --- |
| `cargo test --locked -p i2pr-daemon --test tunnel_liveness_boundary -- --test-threads=1` | 26 passed |
| `cargo test --locked -p i2pr-daemon --test tunnel_liveness_live -- --test-threads=1` | 9 passed |
| `cargo test --locked -p i2pr-daemon --test version_floor_boundary -- --test-threads=1` | 28 passed |
| `cargo test --locked -p i2pr-daemon --test version_eligibility_live -- --test-threads=1` | 11 passed |
| `cargo test --locked -p i2pr-daemon --test version_eligibility_mesh -- --test-threads=1` | 4 passed |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1` | 11 passed |
| `cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- --test-threads=1` | 9 passed |
| `cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1` | 51 passed |
| `cargo test --locked -p i2pr-daemon --test netdb_tunnel_live -- --test-threads=1` | 9 passed |
| `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` | exit 0; 108 test binaries; 0 failures |
| `git diff --exit-code <base> HEAD -- Cargo.toml Cargo.lock crates/i2pr-daemon/Cargo.toml` | byte-identical |

## Invariant review

1. **The declared version is not edited** and no floor is moved. This plan
   touches no version policy.
2. **Fail closed and bounded.** The scheduler's pair table (8), pending table
   (16), and failure counters keep their explicit ceilings; a capacity
   refusal is typed and counted, never a silent drop; the drives per tick are
   bounded.
3. **No new task, no new timer, no new listener, no new socket.** The
   scheduler runs on the **existing** pump tick and the **existing**
   coordinator.
4. **No unbounded structure.** Nothing in this plan adds a growing
   collection.
5. **No new dependency.** `Cargo.toml` and `Cargo.lock` stay byte-identical.
6. **No advertisement, no wire change, no new option, no new capability.**
7. **The liveness seam is the only inbound route.** The plan does not add a
   second, parallel inbound path.
8. **A false liveness verdict may not take down a good destination.** The
   verdict removes an **exploratory** pair; a destination tunnel is not
   touched.
9. **A verdict may only be acted on when the signal is distinguishable from
   its opposite.** A pair that has never produced a correlated response is
   never torn down.
10. **The correlation must be authenticated.** A response that cannot be
    attributed to a pending test is not evidence.

## Failure review

- **Failure.** A capacity refusal (`PairCapacityReached`,
  `PendingCapacityReached`) is typed and counted; the pump skips that pair
  and continues. A probe that cannot be emitted is **abandoned**, not
  charged to the peer: the peer never saw it. A peer whose RouterInfo is
  absent has its pair removed but no rebuild requested — the removal is
  correct and the rebuild is impossible, and the condition is logged and
  counted.
- **Cancellation.** The scheduler holds no task and no timer, so there is
  nothing to cancel inside it. A cancelled pump leaves the scheduler's
  tables as they are; the next tick continues from the coordinator's clock.
- **Restart.** Nothing is persisted. The scheduler starts empty and the
  coordinator's tunnels are rebuilt by the normal path.
- **Contention.** The scheduler lives beside the coordinator inside the
  product's existing state; it adds no lock of its own beyond the ones the
  coordinator already takes. The pump's tick and the inbound route take the
  same coordinator borrow the existing code takes, in the same order, so no
  new lock order is introduced.

## Migration review

None. No wire format changed, no `router.version` changed, no option
changed, no configuration key was added, nothing is persisted.

The **behavioural** consequence is stated plainly: a router with an
installed exploratory pair now emits one bounded `DeliveryStatus` probe per
pair per minute, and tears a pair down only after the router has seen that
peer answer and then stops answering twice consecutively.

## Security review

- The probe carries **no peer identity** beyond the delivery instruction the
  tunnel already requires, and no key material: the payload is a
  `DeliveryStatusMessage` whose id is a liveness id from the high half of the
  `u32` range.
- The **acknowledged-message id space is disjoint** from the NetDB LS2
  publication reply token space, so a liveness probe can never be mistaken
  for a publication reply, nor the reverse.
- The gate **narrows** what the router tears down: only an **exploratory**
  pair that has been proven and has then failed twice.
- No new configuration knob; no new listener; no new socket; no new
  dependency; the counters are published through the existing loopback-only
  inspection seam, and they carry counts only.

## Documentation and operations

- `docs/architecture/i2pr-daemon.md` — the live liveness owner, the
  feasibility answer, the authenticated correlation, the proven-pair gate,
  the canonical probe path, the identifier-space collision, the structural
  distinction, and the measured limitation.
- `specs/CONFORMANCE.md` — the Plan 504 section, including the explicit
  statement that no stock-peer evidence of the liveness response exists.
- `specs/support.toml` — one `experimental` surface with `advertised = false`.
- `plans/registry.md`, `plans/subsystems/network-participation-roadmap.md` —
  the Plan 504 rows and the Plan 185 correction.
- The current-fact surface tallies move from 95/89 to 96/90 in the registry,
  `specs/CONFORMANCE.md` and the daemon deep-dive. The **closed** closure
  records keep their own historical tallies.

## Limitations

- **The scheduler tracks zero pairs in production.** The daemon installs no
  exploratory tunnels. The machinery is installed, wired, authenticated and
  safe; the **population** it watches does not exist yet. This is the
  plan's central limitation and it is recorded as a finding, not smoothed
  over.
- **No stock-peer evidence of the liveness response.** The response half had
  never been observed over a wire with any peer, and the feasibility
  artefact is a local one. Whether an ordinary router relays a probe back to
  the named inbound gateway is unproven.
- **The probe's peer material is supplied by the caller.** The coordinator
  cannot retain a peer's static encryption key itself — the secret tunnel
  material moves into the runtime role owner on activation and is zeroized
  there — so the rebuild path resolves the peer from the authoritative
  RouterInfo store.
- **Plan 487 F3's destination-inbound half is not closed.** It needs tunnel
  rekeying, which the tree does not implement at all.
- **The receipts are not persisted.** A restart empties the log.

## Findings by severity

### Medium

- **The liveness scheduler has nothing to watch in production.** The
  daemon builds no exploratory tunnels, so the plan's mechanism is
  installed and inert. The **gap** this closes is real — the scheduler was
  never constructed and the inbound seam was never used — but the
  *observable* capability is bounded by the absence of the exploratory
  build driver, which is a separate plan.
- **The liveness response is unproven against a stock peer.** The
  proven-pair gate contains the risk, but the router cannot yet distinguish
  "the peer does not answer probes" from "the peer is dead".
- **The coordinator's exploratory pool is private, so the
  coordinator→scheduler hand-off cannot be exercised out-of-crate without a
  socket.** `ExploratoryBuildCoordinator::pool` has no public accessor and no
  pool-injection constructor; `submit` needs a live SSU2 session
  (`RouterDeliveryService::deliver_with_ticket` is `pub(crate)`, and
  `Ssu2RuntimeService` has no in-process transport hook). The live lane
  therefore proves the **scheduler's** half end to end against the very
  `i2pr_tunnel::pool::ExploratoryPool` type the coordinator owns, while the
  hand-off itself rests on the structural guard (rules 1 and 2) and on the
  existing socket-based `exploratory_build_live.rs::liveness_scheduler_pairs_both_directions`,
  which passes unchanged. Reported, not papered over.
- **The pending-test ceiling is not independently reachable.**
  `MAX_LIVENESS_PAIRS` (8) is **smaller** than `MAX_PENDING_LIVENESS_TESTS`
  (16), so the pending table can hold at most one test per pair. The pending
  ceiling is therefore a backstop that the pair ceiling already bounds. The
  lane asserts the invariant as it actually holds
  (`pending_tests() <= MAX_PENDING_LIVENESS_TESTS` **and**
  `pending_tests() <= active_pairs()`) and asserts the *enforced* ceiling —
  the pair table's refusal — rather than asserting a bound the design cannot
  reach.

### Low

- **`route_inbound_with_liveness_classified` was added beside
  `route_inbound_with_liveness`.** The live `process_inbound` site already
  holds the decoded outcome, so re-decoding would run a second decoder; the
  classified variant takes the outcome directly. Both are the same seam and
  neither weakens a decoder.
- **`InboundLivenessOutcome`'s field names changed** to mirror
  `InboundRouteOutcome` exactly (`coordinator` → `dispatcher`, `build_outcomes`
  → `coordinator`). No external caller existed.
- **The guard's rule 6 is a proxy, and a tight one.** "No private parallel
  cell pipeline" is detected by counting `.forward_cells(` call sites in
  `exploratory_build.rs`, which is exact today because that is the only
  multi-cell composer on `OutboundGatewayRole`. A *legitimate* second
  composer would trip the rule. That is the intended direction of failure —
  a false positive on a new canonical composer, not a false negative on a
  private one — but it is stated rather than hidden.
- **The guard's coupling to the landed source text.** The
  `the_guard_fires_on_a_weakened_copy_of_the_landed_source` probe copies
  four real source files and applies the smallest edit that would
  reintroduce each defect. A future refactor that reformats those files
  makes the probe fail loudly with "the X mutation must have applied" —
  fail-closed and explicit, but a coupling a reviewer should know about.
- **The guard's seam rule accepts two spellings.**
  `route_inbound_with_liveness_classified(` does **not** contain the token
  `route_inbound_with_liveness(` — the character after the stem is `_`, not
  `(`. A literal single-token rule would have failed against the landed
  code, so rule 2 accepts either spelling, each matched with its own open
  parenthesis. A body calling neither variant is reported, and the bare
  `.route_inbound_i2np(` remains separately forbidden by rule 3.

### Recorded, not defects

- The daemon deep-dive's per-module unit-test distribution is stale
  (pre-existing).
- The daemon deep-dive carries four bare-filename `file:line` citations a
  link checker cannot resolve (pre-existing).

## Plan 185's disposition, and Plan 487 F3

| Item | Disposition |
| --- | --- |
| **Plan 185's "the daemon pump calls `route_inbound_with_liveness`"** | **False of the tree it described, and now corrected.** The scheduler is constructed by the live product, the seam is used, and the correction is carried by this record and the registry's row. Plan 185's closure record is **not** rewritten — the precedent Plan 503 set for Plan 491. |
| **Plan 487 F3 — the destination's inbound pool has no transit-hop failure detection** | **Advanced, not closed.** The creator side now has a live, authenticated liveness mechanism with a proven-pair gate. The **destination-inbound** half needs **tunnel rekeying**, which the tree does not implement at all, and that is a new I2NP mechanism with its own plan-of-record. |
| **Plan 487 F2** | Open — Plan 485's recorded session policy. |
| **Plan 497 limitation 5** | Open — the peer-profile store stays empty, and the routing table's only live reader is Plan 433's blocked coordinator. |
| **Plan 491 F2, F4** | Open — external evidence and the I2PControl line. |
| **Plan 502 M1** | Open — the creator-correlation retention window is protocol-derived. |

## Roadmap disposition

The `network-participation` roadmap's Plan 504 row moves to `closed`, and its
§12 status sentence names Plan 504 as closed.

**The next candidate on this line is the exploratory build driver.** It is
the population the liveness scheduler watches, and it is what makes three
other things live at once: the liveness scheduler's verdicts, the peer
profile store (Plan 497 limitation 5), and the routing table's live reader.
It needs its own plan-of-record, because it changes what the router builds
and therefore what the router puts on the wire.

Other open source-side candidates, unchanged by this plan: Plan 487 F2 and
F3's destination-inbound half, Plan 497 limitation 5, Plan 491 F2 and F4,
Plan 490 Medium #3 (the stock-peer introducer lane), Plan 502 M1, and Plan
503's lookup-target API-version gate.

## Unblock audit

| Item | State after Plan 504 |
| --- | --- |
| Plan 185's liveness seam | **Live.** The scheduler is constructed, driven, and the seam is used. |
| Plan 487 F3 (transit-hop failure detection) | **Advanced.** The creator side has a live, authenticated mechanism; the destination-inbound half needs tunnel rekeying and stays open. |
| Plan 487 F2 | Open — Plan 485's recorded session policy. |
| Plan 491 F1, F3 | Discharged by Plan 503. |
| Plan 491 F2, F4 | Open. |
| Plan 433 (the NetDB / destination-tunnel coordinator composition) | Still `blocked` on the external acquisition contract. Untouched. |
| Plan 436 WP3 | Already discharged by Plan 502. Plan 436 stays `blocked`. |
| Plan 437, 438, 439, 493, 494 | Unchanged; all `blocked` on external environments. |
| The exploratory build driver | **New, open.** Needs a plan-of-record. |
| The stock-peer liveness response | **New, open.** An external-lane question. |
