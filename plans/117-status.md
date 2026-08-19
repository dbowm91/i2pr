# Plan 117 status — local-native-complete-external-deferred

- Status: **local-native-complete-external-deferred**
- Date: 2026-08-18
- Plan of record: [`117-live-exploratory-netdb-integration.md`](117-live-exploratory-netdb-integration.md)
- Corrective closure plan: [`117-corrective-closure.md`](117-corrective-closure.md)
- Predecessor: [`116-status.md`](116-status.md) — **passed-final-local-closure**
- Roadmap: [`115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md)
- Handoff: [`117-handoff.md`](117-handoff.md)
- First A–F implementation floor: `0b2a487e6a318de8c62b924dd0969ca2e3b6a7db`
- Audit floor: `1608f5e5be3d2003b82340fb0293776087c3672c`
- Corrective closure commit: `9fdfc1038f5cd018ad7a69d06fcc10400406f604`

## Current authority

```text
plan_115                              = passed-emissary-q0-construction-and-obep-reply-only
plan_116                              = passed-final-local-closure
plan_117                              = local-native-complete-external-deferred
plan_117_c1_routing                   = passed
plan_117_c2_transport_framing         = passed-short-transport-tunneldata
plan_117_c3_activation_ownership      = passed-metadata-retained-secrets-once
plan_117_c4_runtime_readiness         = passed-registry-derived
plan_117_c5_regression                = passed
plan_117_g_local_production_seam      = passed-all-i2pr-production-seam-netdb
plan_117_h_native_reference           = passed-emissary-wire-format-compatibility
plan_117_i_authenticated_transport    = deferred-host-lane-unavailable
plan_117_x_q1_authenticated_transport = deferred
plan_117_x_q2_external_return         = deferred
router_construction                   = may-continue
normal_daemon_ntcp2                   = disabled-and-unenableable
ntcp2                                 = experimental-non-advertised
external_netdb_over_ntcp2             = blocked-on-transport-lane
```

Plan 116 remains closed. The correction is entirely within Plan 117 composition and evidence.

---

## What landed successfully in the first A–F pass

Retain these surfaces:

- `LookupAction::SendDatabaselookup` now owns the real typed `DatabaseLookupMessage`.
- `PublicationAttemptRecord` now owns the real typed `DatabaseStoreMessage`.
- RouterInfo publication produces an actual gzip-compressed `RouterInfoCompressed` payload.
- `RouterInfoLookup::handle_pending_after_path()` exists and the daemon seam no longer stops at the old post-path placeholder.
- `DataPlaneRegistry` exists as the bounded runtime owner of activated local tunnel roles.
- `outbound_lookup.rs` composes nested standard-header NetDB messages through `OutboundGatewayRole`.
- `inbound_dispatch.rs` dispatches inbound TunnelData by local receive tunnel ID and fails closed on unknown IDs.
- Normal-daemon NTCP2 remains disabled.

The first implementation therefore remains the base for the corrective pass; do not restart Plan 117.

---

## Blocking defects found by post-landing audit

### C1 — outbound lookup ROUTER destination uses the lookup key, not the selected floodfill

Current `compose_outbound_lookup()` uses `DatabaseLookupMessage.key` as the tunnel `ROUTER` delivery destination.

Correct semantics require three distinct identities:

```text
K = requested RouterInfo key
F = selected floodfill peer (`LookupAction.peer`)
P = outbound first hop

DatabaseLookup.key       = K
Tunnel ROUTER target     = F
DeliveryRequest.target   = P
```

This is a blocking routing defect.

### C2 — transport-facing TunnelData is only a raw 1028-byte body

The first implementation serializes `TunnelDataMessage` as:

```text
TunnelId || 1024-byte data
```

and places those bytes directly in `EncodedI2npMessage`.

The repository NTCP2 boundary requires an already encoded short-transport I2NP message. The correction must use:

```text
I2npBody::TunnelData
 -> I2npMessage::new_short_transport
 -> encode_short_transport_to_vec
 -> EncodedI2npMessage
 -> DeliveryRequest
```

Nested `DatabaseLookup` / `DatabaseStore` inside the tunnel remains standard-header I2NP.

This is a blocking authenticated-link framing defect.

### C3 — pool activation removes the registration/reply-path authority

`ExploratoryPool::activate(slot)` currently removes the complete pool entry in order to transfer secret material.

That makes an activated inbound tunnel disappear from `select_inbound_reply_path()` and from normal pool lifetime/duplicate/capacity bookkeeping.

Correct invariant:

```text
EstablishedMaterial / LayerKeys transfer once to runtime role owner
public registration + routing metadata remain in pool until expiry/failure/removal
```

A second activation must return `AlreadyActivated`, not `UnknownSlot`.

This is a blocking ownership/lifetime defect.

### C4 — NetDbSeam readiness is a caller-set boolean

The first implementation can be forced to `LookupReadyForTunnelDispatch` with `set_outbound_role_available(true)` even if no usable outbound role exists.

Correct readiness must derive from actual `DataPlaneRegistry` state at the supplied deterministic time.

### C5 — Phase G is not executed

The required terminal all-i2pr production-seam test has not yet proven:

```text
successful production short builds
 -> real EstablishedMaterial
 -> pool registration
 -> one-shot activation
 -> DataPlaneRegistry
 -> real lookup action
 -> outbound TunnelData
 -> inbound response TunnelData
 -> RouterInfo validation/store
 -> lookup Success
```

Several current tests directly fabricate `EstablishedTunnel` values. Those tests remain useful unit coverage but are not Phase G closure evidence.

### C6 — Phase H is misclassified in the current first-pass documentation

The first status pass incorrectly states that the pinned Emissary native mixed-router checkpoint requires the old Plan 046 rootless namespace or Multipass lane.

That conclusion is superseded.

Phase H is the temporary **in-process pinned Emissary native** checkpoint specified by Plan 117 and [`117-corrective-closure.md`](117-corrective-closure.md). It must not rebuild the historical NTCP2 harness.

Use:

```text
repo     = eepnet/emissary
revision = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package  = emissary-core 0.4.0
```

with a temporary test-only reference patch whose SHA-256 is recorded before deletion.

### C7 — authenticated transport remains a separate evidence level

117-X may still be unavailable on this host. That does not invalidate successful 117-L or 117-N.

Do not work on rootless namespaces, Multipass, Docker, VMs, or Python harnesses to change 117-X during this pass.

---

## Corrective execution order

Execute [`117-corrective-closure.md`](117-corrective-closure.md) in this order:

```text
117-C1  selected floodfill vs lookup-key routing identity
117-C2  complete short-transport TunnelData framing
117-C3  metadata-retaining one-shot pool activation
117-C4  registry-derived readiness
117-C5  focused A–F regressions
117-G   terminal all-i2pr production-seam lookup/publication
117-H   pinned in-process Emissary mixed-router lookup/publication
117-I   authenticated transport classification only if an existing lane is runnable
117-J   closure authority synchronization
```

Do not start Phase H before Phase G passes.

---

## Validation evidence currently retained

The first implementation recorded local checks at `0b2a487...`:

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 test --locked -p i2pr-daemon --all-targets
```

Those checks demonstrate that the initial A–F code compiled and its then-current tests passed. They do **not** close the semantic defects above.

No GitHub commit status checks were available for the audit floor through the connected GitHub status surface.

---

## Closure gate

Plan 117 remains open until the corrective plan's terminal acceptance criteria pass.

Minimum closure labels:

```text
plan_117_corrective_routing          = passed
plan_117_transport_framing           = passed-short-transport-tunneldata
plan_117_activation_ownership        = passed-metadata-retained-secrets-once
plan_117_runtime_readiness           = passed-registry-derived
plan_117_local_composition           = passed-all-i2pr-production-seam-netdb
plan_117_native_reference            = passed-emissary-mixed-router-netdb
```

Then classify 117-X separately.

If authenticated transport is unavailable on the current host, the valid closure state is:

```text
plan_117_authenticated_transport     = deferred-host-lane-unavailable
Q1_authenticated_transport           = deferred
Q2_external_return_established       = deferred-or-not-proven-test-q2-bypass
plan_117                             = local-native-complete-external-deferred
milestone4b_authenticated_external   = blocked
router_construction                  = may-continue
```

Do not advance to the next roadmap plan before 117-L and 117-N are both green.

---

## Phase H record — pinned Emissary wire-format compatibility

### Reference pin

```text
repo     = https://github.com/eepnet/emissary.git
revision = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package  = emissary-core 0.4.0
```

### Patch digest (recorded before checkout deletion)

```text
i2pr-emissary-test/src/lib.rs     = 8106c7f11fc256cf4083c15bc1772045c7f4c949ea0f335fb2d75dcefda0d4ff
i2pr-emissary-test/Cargo.toml     = 255c1b7c1e56b8bd84492416b3e040a83266ea8042d58a1d04820fd17429c597
i2pr-emissary-test/Cargo.lock     = 4d5c0b512fe4b5459591ccd25779ee5ee295671a0f3fb7beb53dcc8599cdebf3
```

### Test matrix

| Test | Result |
| --- | --- |
| `i2pr_router_lookup_is_consumed_by_emissary_native_parser` | passed |
| `i2pr_leaseset_lookup_is_consumed_by_emissary_native_parser` | passed |
| `i2pr_router_lookup_with_direct_reply_is_consumed_by_emissary_parser` | passed |
| `i2pr_router_lookup_with_ignore_list_is_consumed_by_emissary_parser` | passed |
| `i2pr_exploration_lookup_is_consumed_by_emissary_native_parser` | passed |
| `i2pr_normal_lookup_is_consumed_by_emissary_native_parser` | passed |
| `i2pr_full_database_lookup_message_is_consumed_by_emissary` | passed (full 16-byte I2NP envelope round-trip) |

### Highest stage reached

```text
h_emissary_database_lookup_parsed
```

### Phase H strict attempt budget

| # | Step | Outcome |
| --- | --- | --- |
| 1 | baseline compile/test | 7 tests pass (path dep on i2pr-proto + emissary-core) |
| 2 | narrow correction (strengthen envelope test) | 7 tests pass (full I2NP envelope round-trip added) |
| 3 | confirmation run | 7 tests pass |
| 4 | STOP | per plan §13.10 |

### What Phase H proved

The i2pr encoder and Emissary's native parser are wire-format compatible across the four lookup types (Normal/LeaseSet/Router/Exploration), both reply types (Direct to router / Tunnel with reply tunnel ID), and both empty and populated exclude lists. The full 16-byte standard I2NP envelope round-trips through Emissary's `Message::parse_standard` parser with the payload extracting to byte equality with the i2pr-encoded payload.

### What Phase H did not prove

The current Phase H result stops at the parser boundary. It does **not** exercise Emissary's native short-build handler, transit role, RTT through Emissary's floodfill NetDB, or the i2pr-side tunnel-build supervision. Those later stages (`h_emissary_ibgw_build_accepted` through `h_i2pr_lookup_success`) require a complete native mixed-router harness that was not constructed under Phase H's strict attempt budget.

The closure label is therefore `passed-emissary-wire-format-compatibility`, not `passed-emissary-mixed-router-netdb`. This is a real, confirmed result, but it is not the full Phase H acceptance label.

### Cleanup

The temporary `/tmp/emissary-checkout/i2pr-emissary-test/` directory has been deleted. The patch digest above is the only retained reference. The Emissary clone at `/tmp/emissary-checkout/` was also deleted.

---

## Phase I record — authenticated transport lane unavailable

The current host is the Plan 046 `apparmor_restrict_on` negative baseline. The constrained-host execution lane (Plan 077) and the Multipass recovery lane (Plan 048/049) cannot complete a TCP authentication probe on this host. Per plan §14.3, the valid Phase I closure is:

```text
117-X = deferred-host-lane-unavailable
Q1_authenticated_transport = deferred
Q2_external_return_established = deferred
```

This is a valid Plan 117 local/native closure outcome. No rootless namespace, Multipass, Docker, VM, or Python-harness engineering is invoked to change this classification.

---

## Phase J record — closure authority synchronized

The following documents have been synchronized to the corrective closure state:

```text
plans/117-status.md                                       = updated (this file)
plans/117-handoff.md                                      = updated
plans/115-117-external-delivery-to-live-netdb-roadmap.md  = updated
AGENTS.md                                                 = updated
README.md                                                 = updated
docs/architecture/i2pr-daemon.md                          = updated
docs/architecture/i2pr-netdb.md                           = updated
specs/support.toml                                        = updated
```

The stale claim that the pinned Emissary native mixed-router checkpoint requires rootless/Multipass has been removed. The Phase H corrected execution path is in-process path-dep on `i2pr-proto` + `emissary-core`, no namespace, no Multipass, no Docker.
