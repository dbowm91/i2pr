# Plan 117 handoff — terminal native-reference correction

- Status: **ready-for-terminal-native-reference-correction**
- Date: 2026-08-18
- Execute now: [`117-terminal-native-reference-correction.md`](117-terminal-native-reference-correction.md)
- Status authority: [`117-status.md`](117-status.md)
- Original plan: [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)
- Prior corrective pass: [`117-corrective-closure.md`](117-corrective-closure.md)
- Predecessor Plan 116: **closed**
- Current implementation floor: `b7e12e09d84089b5459d29aa962d01a963554b29`
- Pinned independent reference: `eepnet/emissary@9b43484a21d5a1291c4881cdae62a36c527f8c0f`, `emissary-core 0.4.0`

## Start here

Do **not** redo Plan 117 C1-C5 or Phase G.

They are retained as passed:

```text
C1 floodfill-vs-key routing                passed
C2 outer TunnelData short transport        passed
C3 activation metadata/secrets split       passed
C4 registry-derived readiness              passed
C5 regressions                              passed
117-G all-i2pr production-seam NetDB       passed
```

The remaining Plan 117 blocker is the independent native reference claim.

The previous Phase H result:

```text
passed-emissary-wire-format-compatibility
highest stage = h_emissary_database_lookup_parsed
```

is valid parser evidence but **not** `117-N` closure.

---

## Research correction: use Emissary's own test build

The prior Phase H test used a separate path-dependency crate. That method did
not compile `emissary-core` with its own `#[cfg(test)]` configuration, so these
important native test seams were unavailable:

```text
tunnel::tests::TestTransitTunnelManager
tunnel::tests::make_router
tunnel::tests::connect_routers
TunnelPoolHandle::create
TunnelMessage
GarlicHandler test exports
```

The pinned source already contains the production/native path needed for Plan
117:

```text
TransitTunnelManager::handle_short_tunnel_build
  -> IBGW / Participant / OBEP admission

SubsystemManager
  -> TunnelData / TunnelGateway routing by tunnel ID
  -> NetDB message routing

OutboundEndpoint
  -> TunnelData decrypt/checksum/fragment parse
  -> nested standard I2NP parse
  -> ROUTER/TUNNEL delivery

NetDb(floodfill=true)
  -> DatabaseStore RouterInfo acceptance
  -> DatabaseLookup handling
  -> native DatabaseStore / DatabaseSearchReply generation

InboundGateway
  -> TunnelGateway -> TunnelData

Participant
  -> TunnelData layer transform / forwarding
```

Therefore **do not switch references** and do not touch the host execution lane.

Correct test layout:

```text
fresh temporary pinned Emissary checkout
 -> add i2pr crates as emissary-core dev-dependencies
 -> add one test under emissary-core/src/tunnel/tests/
 -> compile/test emissary-core itself
```

The critical rule is:

> The Plan 117 interoperability test must belong to `emissary-core`'s own test
> build. Do not use another external `i2pr-emissary-test` crate for native
> closure.

---

## Required execution order

```text
N0  i2pr slot-based registry cleanup
N1  fresh pinned Emissary checkout
N2  in-tree emissary-core test + minimal #[cfg(test)] helpers
N3  native i2pr->Emissary short builds
N4  native i2pr publication -> OBEP -> floodfill
N5  native i2pr lookup -> OBEP -> floodfill
N6  native reply route -> IBGW -> participant -> i2pr endpoint
N7  require i2pr RouterInfo validation/store + lookup Success
N8  require publication read-after-write observation when available
N9  record final binary patch SHA-256 before checkout deletion
N10 confirmation run + authority synchronization
```

Do not reorder the work around parser-only testing.

---

## N0 — one local production cleanup

`ExploratoryPool` reports expiry/failure using `TunnelSlot`. Outbound registry
roles are keyed by slot; inbound roles are keyed by local receive tunnel ID.

Add a bounded reverse mapping so the runtime can execute:

```text
pool.advance_time(...) -> [TunnelSlot]
for slot in expired:
    registry.remove_slot(slot)
```

and the same for failure.

Recommended invariant:

```text
inbound activation binds (TunnelSlot, local_receive_id)
remove_slot(slot) removes either direction
all reverse metadata is removed atomically
no LayerKeys clone is introduced
```

Keep pool and registry dependency direction unchanged; the composition owner
coordinates them.

---

## N1/N2 — corrected native reference placement

Use exactly:

```text
repo      = https://github.com/eepnet/emissary.git
revision  = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package   = emissary-core 0.4.0
```

Temporary changes only:

```text
emissary-core/Cargo.toml              dev deps on required i2pr crates
emissary-core/src/tunnel/tests/mod.rs add test module
emissary-core/src/tunnel/tests/i2pr_plan117_native.rs
optional small #[cfg(test)] TestTransitTunnelManager helper(s)
optional #[cfg(test)] pre-Garlic raw build-reply observer
```

Do not change production protocol checks.

Do not add a permanent i2pr harness.

---

## N3 — native short-build roles

Topology:

```text
outbound:
  i2pr creator -> Emissary OBEP P

inbound:
  Emissary IBGW I -> Emissary Participant Q -> i2pr creator L
```

Each reference hop must be created from its real Emissary router hash and static
X25519 public key.

Drive i2pr production STBM generation into
`TestTransitTunnelManager::handle_short_tunnel_build()`.

Require native admission and live transit-role registration.

### OBEP build-reply boundary

Do not implement general i2pr garlic.

A temporary `#[cfg(test)]` observer may capture the already-transformed,
count-prefixed reply payload immediately before Emissary wraps it in Garlic.
Feed those native reply bytes to i2pr `BuildEvent::BuildReply`.

Record:

```text
Q2_external_return_established = not-proven-test-q2-bypass
```

The native handler must still produce its normal Garlic/TunnelGateway output.

---

## N4 — publication first

Use i2pr's existing production publication path to publish RouterInfo K to
native Emissary floodfill F:

```text
PublicationCoordinator DatabaseStore
 -> compose_outbound_publication
 -> i2pr OutboundGatewayRole
 -> short-transport TunnelData
 -> Emissary SubsystemManager
 -> native Emissary OBEP
 -> native DatabaseStore to F
 -> native Emissary floodfill NetDb
```

Require native acceptance.

Publishing first enables the later lookup to prove read-after-write rather than
relying on a synthetic response.

---

## N5 — lookup through native OBEP and floodfill

Keep identities visibly distinct:

```text
K = requested RouterInfo
F = native Emissary floodfill
P = native Emissary OBEP
I = native Emissary inbound gateway
```

Required semantics:

```text
F != K
P != F
DatabaseLookup.key == K
Tunnel ROUTER destination == F
DeliveryRequest.target == P
DatabaseLookup.from == I
DatabaseLookup.reply_tunnel_id == I receive tunnel
```

Phase G already proves i2pr's candidate selection from its store. The native
checkpoint should not add unrelated RouterInfo-import machinery merely to
re-prove peer selection. It may bind the fixture's known F into the existing
typed lookup action/test builder while keeping the actual DatabaseLookup and
tunnel composition production-generated.

Do not hand-build wire bytes.

Native flow:

```text
i2pr TunnelData
 -> Emissary OBEP decrypt/checksum/Tunnel Message parser
 -> nested standard DatabaseLookup
 -> ROUTER delivery F
 -> F SubsystemManager
 -> F native NetDb
```

Require native floodfill processing, not parser-only acceptance.

---

## N6 — explicit floodfill-response boundary and native inbound return

Native Emissary floodfill replies to a tunnel through its exploratory pool
handle. In the test build, `TunnelPoolHandle::create()` exposes the resulting
`TunnelMessage::TunnelDeliveryViaRoute`.

Observe and require:

```text
router_id == I
tunnel_id == declared i2pr inbound reply tunnel
message == native standard DatabaseStore for K
```

Do not build a separate Emissary creator-side exploratory outbound tunnel merely
to carry this reply. Convert the observed native routing instruction to the
corresponding native TunnelGateway and inject it into I.

Record this explicit boundary:

```text
reference_floodfill_response_routing = native-TunnelDeliveryViaRoute-observed
reference_floodfill_outbound_exploratory_tunnel = bypassed-at-test-routing-boundary
```

Then require production native transit processing:

```text
Emissary IBGW I
 -> TunnelGateway parse
 -> TunnelData construction/layer
 -> Emissary Participant Q
 -> TunnelData transform
 -> final TunnelData to i2pr L
 -> i2pr LocalInboundEndpointRole
 -> DatabaseStore K
 -> RouterInfo validate/store
 -> lookup Success
```

No reference-side TunnelData decryption or re-encryption is permitted.

---

## N7/N8 — terminal native acceptance

Required final native label:

```text
117-N = passed-emissary-mixed-router-netdb
```

At minimum prove:

```text
Emissary OBEP short build         passed
Emissary IBGW short build         passed
Emissary participant short build  passed
Emissary OBEP TunnelData           passed
Emissary floodfill publication     passed
Emissary floodfill lookup          passed
native reply route decision        passed
Emissary IBGW return               passed
Emissary participant return        passed
i2pr DatabaseStore recovery        passed
i2pr RouterInfo validation/store   passed
i2pr lookup Success                passed
```

Preferred publication observation:

```text
publication accepted
 -> later lookup K
 -> native floodfill returns stored RouterInfo K
```

Record `passed-read-after-write` when achieved.

---

## N9 — evidence requirement

The previous file-by-file digests do not satisfy the terminal patch-evidence
requirement.

Before deleting the temporary checkout:

```bash
git -C "$WORK/emissary" diff --binary > "$WORK/emissary-plan117-native.patch"
wc -c "$WORK/emissary-plan117-native.patch"
sha256sum "$WORK/emissary-plan117-native.patch"
```

Record one final patch byte length and SHA-256 in `plans/117-status.md` before
cleanup.

Then delete the temporary checkout.

---

## Fresh attempt budget for corrected method

The parser-only external-crate attempts do not consume this budget because they
never enabled the intended Emissary native test configuration.

```text
1 baseline in-tree compile/test
2 narrow temporary-test API/borrow/lifecycle corrections
1 confirmation after full success
STOP
```

If production native processing exposes one reproducible i2pr protocol defect:

```text
1 localized i2pr correction
1 focused confirmation
STOP
```

Do not switch to Java/i2pd unless the pinned source itself proves unusable.
Current source inspection shows the required native seams exist.

---

## Forbidden scope

```text
Plan 116 reopening
redoing C1-C5
redoing Phase G architecture
normal-daemon NTCP2 activation
SSU2 work
rootless namespaces
Docker
Multipass / VMs
public I2P
Python interop harness
permanent Emissary adapter crate
new Java/i2pd matrix
general garlic implementation
new generic router dispatcher
reference production API changes
```

---

## Final authority on success

If 117-N passes and the host authenticated lane remains unavailable:

```text
plan_117_local_composition           = passed-all-i2pr-production-seam-netdb
plan_117_native_reference            = passed-emissary-mixed-router-netdb
plan_117_authenticated_transport     = deferred-host-lane-unavailable
Q1_authenticated_transport           = deferred
Q2_external_return_established       = deferred
plan_117                             = local-native-complete-external-deferred
milestone4b_authenticated_external   = blocked
router_construction                  = may-continue
next_router_construction_plan        = unblocked
normal_daemon_ntcp2                  = disabled-and-unenableable
ntcp2                                = experimental-non-advertised
```

Do **not** block subsequent router construction on the unavailable authenticated
transport lane once the local + native Plan 117 evidence is green.

Until 117-N passes:

```text
next_router_construction_plan = blocked-on-plan117-native-terminal-pass
```
