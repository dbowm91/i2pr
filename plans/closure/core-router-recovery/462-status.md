# Plan 462 status: blocked — first SAM build divergence remains unlocalized

Closure token: `blocked-sam-build-first-missing-transition-unproven`

Plan: [`462-normal-sam-tunnel-build-first-divergence-corrective.md`](../../implementation/core-router-recovery/462-normal-sam-tunnel-build-first-divergence-corrective.md)

## Outcome

The bounded source comparison confirms the normal process and service-group
provisioning share `provision_all_service_router_material`. The SAM path reaches
it from `process_deferred_activation_request` after the normal `i2pr run` owner
receives activation; the existing M6 service-group lanes exercise separate
external controls and do not provide a matching normal-process SAM build
receipt. The Plan 458 normal-process failure is therefore not localized by
the passing M6 preflight, exploratory-tunnel, or NetDB lanes.

Plan 458's latest retained normal-process receipt stops at an outbound-build
deadline before STREAM setup. Its older attempts alternated between inbound
and outbound build deadlines. Those stage labels show that SAM provisioning
waited for a build install, but do not show whether the earliest missing event
was selected-request admission, SSU2 emission, peer acceptance, response
encoding, authenticated response receipt, exact-attempt matching, or install
commit. The existing source-owned counters cover coordinator outcomes and
post-install registration, not all those per-attempt transitions in the
deferred activation path. No retained receipt identifies a first missing
transition or a source correction.

The fixture generator configures four exact-pinned i2pd 2.61.0 loopback
routers with distinct generated signing families, `floodfill=true`,
`bandwidth=L`, `notransit=false`, and published SSU2 addresses. Plan 458's
retained execution confirms the signed family-aware RouterInfos were seeded
before the normal daemon attempt. This establishes the configured fixture
roles; it does not establish which peer the build selector chose or that stock
accepted a particular build request. No stock code or fixture bytes were
modified.

Because the first causal divergence remains unknown, no source change was
made and the finite post-correction four-peer rerun was not spent. The
required nonzero-hop SAM exchange, bidirectional exact-byte stream, and
causal regression remain unproven. Plan 462 is blocked rather than passed.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Normal SAM and M6 source/owner comparison | Shared `provision_all_service_router_material` path identified; M6 controls do not expose an equivalent normal-process SAM receipt. No causal source divergence identified. |
| Four-peer fixture role and pin review | Runner uses exact i2pd 2.61.0 pin, distinct generated signed family identities, loopback published SSU2, floodfill enabled, `L` bandwidth, and transit enabled. Retained Plan 458 receipt confirms family-aware records were seeded. Actual selected-peer acceptance was not observed. |
| Earliest missing per-attempt transition | Unavailable: retained normal-process evidence ends at inbound/outbound build deadlines and does not identify the earlier event. |
| Specific source correction and deterministic pre/post regression | Not performed; no proven cause to correct. |
| Post-correction bounded pinned rerun | Not run; no source correction was made. |
| Real inbound/outbound nonzero-hop tunnel install, LS2 and external SAM exchange | Not established. |
| M6 controls | Passed locally for preflight, exploratory tunnels, and NetDB; these do not qualify normal-process SAM composition. |

## Commands and outcomes

Commands ran locally on Linux, against the repository tree at this closure.

| Command / inspection | Outcome |
| --- | --- |
| `rtk bash tests/integration/m6-interop/run-preflight.sh` | Passed; Plan 184 local and exact-pinned i2pd preflight rows. |
| `rtk bash tests/integration/m6-interop/run-tunnels.sh` | Passed; Plan 185 exploratory-tunnel lane against exact-pinned i2pd. |
| `rtk bash tests/integration/m6-interop/run-netdb.sh` | Passed; Plan 186 NetDB lane against exact-pinned i2pd. |
| Source trace of `run_plan458_sam_product.py`, `plan458_sam_driver.py`, `ServiceProduct::poll_inbound`, `process_deferred_activation_request`, and `provision_all_service_router_material` | Confirmed deferred activation invokes the shared provisioner and consumes coordinator outcomes while waiting. No first failed transition can be recovered from retained per-attempt evidence. |
| Fixture review of `run_plan458_sam_product.py` and retained Plan 458 closure | Confirmed pin, generated family keys, loopback SSU2, floodfill, bandwidth, transit settings, family-aware RI seeding, and ordinary `i2pr run` driver. No per-build selected-peer/acceptance evidence. |
| `python3 tests/integration/core-router-recovery/run_plan458_sam_product.py` | Not rerun: the exact corrective plan permits another four-peer attempt only after a specific confirmed source/fixture correction. None was found. |

No product source, fixture, protocol, support inventory, or reference source
was changed. No new dependencies were added.

## Security, compatibility, and findings

- Critical/high: none identified by this bounded investigation.
- Medium: ordinary-process SAM tunnel builds still time out without a localized
  first missing event; nonzero-hop external SAM remains unproven.
- No public I2P, reference patch, synthetic tunnel reply, zero-hop path,
  identity-bearing evidence, or capability promotion was used.
- SAM remains loopback-only, disabled by default, experimental, and
  non-advertised.

## Unblock audit and disposition

Plan 462 is independent of NTCP2 Plans 459–461 and did not delay their work.
Plans 459 and 460 remain blocked at their independently recorded live ingress
and stock-send gates; Plan 461 remains blocked on those dependencies. Plan 458
remains blocked and its historic closure was not rewritten. Resume Plan 462
only when a bounded source-owned receipt can distinguish the first missing
build transition, or a specific source/fixture cause is established. Any
instrumentation needed to establish those receipts requires its own bounded
approved handoff because Plan 462 forbids another observer-only pass.

Disposition: **blocked; first causal build divergence and external SAM
capability gate remain open**.
