# Plan 258 — M11 IBGW data-plane multicell diagnostic corrective: status

**retained-m11-ibgw-multicell-corrective-with-receipt-topology-boundary-required-via-plan259**

## Disposition

Plan 258 delivered its diagnostic outcome and its narrow production
correction, both proven. It does NOT close the lane:

- The retained multicell row now passes against exact-pinned i2pd:
  three datagram-sized ingresses to accepted registrations each
  emit exactly 2 cells with zero forward failures
  (`multicell-max: 2`, `multicell-bounded: true` in diagnostic
  run `diag-plan258-drop2` on `38c939a`).
- The receipt row stops every run after it: the reference endpoint
  reassembles our garlics but never routes them to the receiver
  socket (`gateway-receipt: 0` in all four Plan 258 executions).
  Forensics localize this to lane-topology/reference-dispatch
  dynamics outside any production defect: our emission is
  byte-proven correct against the reference parse rules, and the
  reference IBGW does exactly what we do. Broadening this plan to
  force receipt would require weakening rows or deviating from
  reference wire behavior — both forbidden. The receipt-topology
  question is registered as Plan 259.

No row was weakened. No retry/timeout/redial tuning was applied.
No public transit, advertisement, or capability claim is made. M11
remains unclaimed; M12 remains deferred.

## Commits (implementation head chain)

- `b1258d3` — WP A: gateway failure/nested-size telemetry
  (observability only, no gate change).
- `096e923` — WP B classification + WP C H3-production emission
  correction with regressions and checker guards.
- `0446353` — WP A follow-up: drop-side telemetry (addressed id +
  nested size on drops).
- `38c939a` — WP B follow-up: per-ingress drop labels
  (scope + size class, diagnostic-only).

Baseline: `6dd9073` (Plan 257 closure head).

## Plan 258 §7 acceptance criteria — requirement-to-evidence matrix

1. **Failure telemetry lands and the checker enforces it** —
   DONE. `LiveGatewayOutcome::Delivered` carries `nested_len`;
   `Dropped` carries `tunnel_id` + `nested_len`; the ledger
   `Observation` carries `gateway_failures` (explicit `Some`,
   even zero) + `nested_len`; 13 `gateway-diag-*` diagnostic keys
   (8 delivered-fold + 4 drop-fold + 1 per-ingress label) flow
   through `record_row(Epoch::IbgwData)`; the checker enforces
   every dimension structurally (arm shapes, helper presence,
   production threshold sharing, gate/diagnostic separation).
2. **One non-counted diagnostic classifies H1/H2/H3 with numbers** —
   DONE, then extended by two further non-counted diagnostics
   (drop-side, per-ingress). Numbers in §"Classification" below.
3. **Narrow correction with focused regressions; no gate
   redefinition, no retry tuning, no public behavior change** —
   DONE. One ownership boundary (`i2pr-tunnel` emission path) +
   telemetry-only driver/checker changes. The multicell predicate,
   attempt gate, receipt predicate, and all Plan 257 rows are
   byte-identical.
4. **Retained Plan 257 implementation intact** — DONE. All 261
   lib + 34 live-owner + 46 driver-local rows green on every
   commit; both static checkers green; no suppression added.
5. **Complete external attempt 1 passes every mandatory row** —
   NOT MET. Attempt 1 (`096e923`) stops at the multicell gate
   (pre-fix head could not emit;18 single-cell ingresses only).
6. **Complete external attempt 2 passes on the same SHA** —
   NOT MET (same cause; attempt 2 not launched after the
   attempt-1 forensics redirected to drop-side diagnostics).
7. **Full workspace verification + exact-head CI green** — LOCAL
   floor green (see §"Verification"); hosted CI not yet run on
   the Plan 258 line (owned by the closing plan per 257 precedent).
8. **No known critical/high finding remains** — one MEDIUM
   finding remains open (constant fragment message id, §"Findings");
   the receipt-topology boundary is a HIGH lane finding owned by
   Plan 259, recorded with full stop provenance instead of worked
   around.

Criteria 5–6 cannot be met without the Plan 259 adjudication;
per §8 the plan stops instead of broadening.

## Classification (WP B)

Reference authority throughout: exact-pinned unmodified i2pd
2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`, loopback-only,
fresh datadirs per execution, never merged.

| Execution | Head | ingress | nested-multi | emitted-max | failures | receipt |
|---|---|---|---|---|---|---|
| `diag-plan258-wpb` | `b1258d3` | 11 | 0 | 1 | 0 | 0 |
| attempt 1 | `096e923` | 18 | 0 | 1 | 0 | 0 |
| `diag-plan258-drop1` | `0446353` | 15 delivered / 49 dropped (9 accepted-id, 40 stale-id, 3 nested-multi) | 0 delivered | 1 | 0 | 0 |
| `diag-plan258-drop2` | `38c939a` | 19 delivered (16 single + 3 multi) / 17 dropped (all stale-id) | 3 delivered | 2 | 0 | 0 |

- **H2 (delivery-side) EXCLUDED**: `failures-total: 0` in all four
  executions — every emitted cell was Accepted by the router seam.
- **H3 (production) CONFIRMED and CORRECTED**: the old
  single-cell branch compared nested length against the
  61,440-byte complete-message ceiling but called `build_single`
  (per-cell capacity 976 bytes), so every nested batch in
  (976, 61_440] bytes — including the lane's ~1.5 KB datagram
  batches — failed closed as `MessageTooLarge` and never emitted.
  The failing control fixture (1,500 B nested rejected) reproduced
  it deterministically.
- **H1 (emission-side) EXCLUDED as the blocker**: post-fix,
  reference relays DO deliver datagram-sized batches to accepted
  ids (3 in `diag-plan258-drop2`, each 4 ms after sends 2–4) and
  the corrected path emits 2 cells each.

## Narrow correction (WP C)

`crates/i2pr-tunnel/src/transit.rs::process_tunnel_gateway` routes
every nested batch through the canonical
`fragment_complete_message` + `build_cells` path — the exact shape
of `OutboundEndpointRole::fragment` (small batches yield one
unfragmented record/one cell; large batches yield 2+ cells). The
wrong-threshold fast branch is deleted; the misleading 61,440-byte
duplicate threshold is deleted (the canonical ceiling lives in
`i2pr-tunnel::data`); the size classifier now shares
`MAX_FRAGMENT_BODY_BYTES` (the true emission boundary). The
reference IBGW (`TransitTunnelGateway::SendTunnelDataMsg`) was
verified in pinned source to hardcode `eDeliveryTypeLocal` and
fragment identically — our corrected path mirrors it field for
field (Local delivery, same fragment encoding, same flag bytes).

Regressions (all green, ordinary lanes):

- `i2pr-tunnel`: classifier boundary (976/977), 62 KB → 2+ cells,
  1,500 B → exactly 2 cells, 500 B → exactly 1 cell,
  reference-endpoint parse simulation (layer inversion +
  zero-delimiter sync + record parse + reassembly == nested bytes,
  first flag `0x08`).
- driver (`m11_transit_i2pd_external`, ordinary `#[test]`):
  failure-dimension presence incl. zero case, size classes incl.
  datagram size, gate rejects single-cell-only, background to
  unaccepted ids rejected, diag fold totals, drop arm records
  addressed id + size, drop fold stale/accepted/multi split,
  per-ingress drop labels.
- live-owner: cancelled-gateway drop carries addressed id +
  exact nested length.
- checker: emission path must route through
  `fragment_complete_message`, must not contain `build_single`,
  must not keep the duplicate threshold; 13 diagnostic keys
  enforced; pass predicate must not read diagnostic keys.

## The receipt-topology boundary (why WP D stops)

After the correction, three datagram relays complete the full
production path through our code: ingress → 2-cell emission →
seam Accepted → reference session → reference endpoint
reassembly (B log: `Handle fragment of 1487/1496/1491 bytes, msg
type 11` at the exact emission seconds). B then never routes any
of them to the receiver socket. The same silence holds for every
fragmented garlic at B endpoints across all runs — including
garlics that never transited us (attempt-1 B log: 1483/1489/1495
B garlics via B's own transit tunnels, 0 onward routings).

Our emission is exonerated twice over: (a) the in-tree
reference-endpoint parse simulation passes (byte-exact
reassembly, Local flag); (b) the reference IBGW emits
identically (Local hardcoded, canonical fragmentation).

Structural reason: our IBGW registration's next hop is B, and B
treats arrivals as its tunnel ENDPOINT (`handle msg for endpoint
4124608261`), while the receiver destination lives on A. A
transit endpoint can only complete receipt on its own router;
foreign-destined garlics with Local delivery are never routed
onward here. Closing receipt needs endpoint-on-A, which the
length-2 `[i2pr, B]` receiver tunnel can never provide — a lane
topology premise, not a code defect. Deviating (non-Local
delivery, lane forcing) is forbidden: the reference hardcodes
Local, and we mirror it.

## Disposition of every Plan 257 retained row

- `ibgw-data/multicell-bounded` — now PASSES externally
  (`diag-plan258-drop2`: max 2, bounded true, 0 failures).
  Evidence is diagnostic-labeled, not counted; the counted
  two-pass demonstration is owned by the post-259 WP D replan.
- `ibgw-data/gateway-receipt` — still `0`; owned by Plan 259.
- cardinality / typed bandwidth / far-side / full-drain cancel /
  session close / real restart — still unexecuted externally (the
  driver aborts fail-closed at receipt before those epochs);
  implementation + local rows retained intact, owned by the
  post-259 WP D replan.
- All Plan 257 local/checker/workflow gates — intact and green.

## Verification (local truth; CI labeled)

On `096e923` (full floor): `cargo fmt --check`, `cargo check
--workspace --all-targets`, `cargo test --workspace --all-targets`
(3089 passed, 27 ignored, 0 failed, 106 suites), `cargo clippy
--workspace --all-targets --all-features -- -D warnings` (clean),
`cargo doc` (clean), workspace doc-tests (0), `cargo deny check
advisories bans sources` (ok), all 20 gate scripts (ok),
`python3 -m unittest discover -s tests/integration/ntcp2/harness`
(18 ok), `git diff --check` (clean).

On `38c939a` (head): workspace check + clippy + fmt clean;
`i2pr-tunnel --all-targets` 386 passed; `i2pr-daemon` lib +
`m11_transit_live_owner` + `m11_transit_i2pd_external` 344 passed,
1 ignored (the `#[ignore]`-gated external lane); both M11
checkers green (148 guarded + 43 epoch + 13 diagnostic keys).

Hosted exact-head CI (Ubuntu/macOS/MSRV/policy): NOT YET RUN on
the Plan 258 line — owned by the closing plan (Plan 259 or the
post-259 WP D execution), same as the Plan 257 precedent.

External executions (all fail-closed, sanitized, unmerged; raw
reference logs read for diagnosis only, never as evidence):
`m11-plan258-diag-h1h2h3`, `m11-plan258-attempt-1`,
`m11-plan258-diag-drop1`, `m11-plan258-diag-drop2` under
`target/interop/` with per-run `evidence.json` manifests
(plan 257 schema, explicit `diag-*`/counted attempt ids,
exact `i2pr_commit` recorded).

## Invariant / failure / migration / security reviews

- Invariants preserved: exact pin, unmodified reference,
  loopback-only, fresh datadirs, single-owner/single-decode,
  bounded state (new telemetry fields are `usize`/`Option<usize>`
  on existing structs; no new channels, tasks, or queues),
  no per-cell tasks, no version/capability/advertisement change,
  no public transit.
- Failure semantics: fragment-path errors stay typed
  (`TransitDataFatalError::TunnelMessage`) → owner maps to
  `Ok(None)` → `Dropped` (fail closed, now with dimensions).
  Oversize nested (> 62,708) still fails closed via the
  canonical ceiling. No retry added anywhere.
- Migration/compat: no wire-format change (our fragments were
  already canonical; now they are emitted for all sizes).
  Threshold semantics change is emission-only and matches the
  reference. No config surface change.
- Security: telemetry is counts/sizes/ids only (public routing
  facts already in evidence as receive-id rows). No payload,
  key, digest, or secret retention added. `cargo deny` clean.
  The `#[ignore]`-gated lane contract untouched (ordinary runs:
  46+34+9 new unit rows execute, 1 external ignored).

## Findings by severity

- HIGH (lane, owned by Plan 259): receipt-topology boundary —
  `[i2pr(IBGW) → B(endpoint)]` transit tunnels cannot deliver to
  a receiver on A; B never routes reassembled foreign garlics
  onward here. Full stop provenance in §"The receipt-topology
  boundary". Never worked around.
- MEDIUM (production, known, deferred): constant fragment
  message id (`message_id: 1` for every IBGW emission).
  Sequential lane traffic is unaffected (proven 3/3 clean
  completions), but concurrent fragmented ingresses to one
  registration could cross-assemble at the next hop (availability
  impact only; garbage fails closed at garlic unwrap). Remediation
  pointer: per-registration monotonic fragment id (or thread the
  nested I2NP msgid — needs a new `i2pr-proto` accessor), owned by
  Plan 259's emission work if it touches this path.
- LOW: none open. The 61,440/62,708 duplicate-ceiling trap is
  removed with the defect.

## Roadmap disposition + unblock audit

- Plan 258: `retained-*` (multicell production defect corrected
  and externally proven; receipt/topology boundary recorded with
  stop provenance; corrective required → Plan 259).
- Plan 259 (M11 IBGW receipt-topology adjudication diagnostic):
  REGISTERED `ready` in the same commit (unblock audit: its only
  hard dependency is the Plan 258 diagnosis, now closed; no other
  deps).
- M12 floodfill planning: stays deferred until a two-pass M11
  external closure exists.
- M6 Java / Plan 201-247 debt: unaffected (retained nonblocking).
- No other registry row lists Plan 258 as a dependency; nothing
  else unblocks.

## Docs / ops

- `specs/support.toml`: `plan_258_status` retained token,
  `plan_259_status` ready token, `m11_transit_tunnels` unchanged
  (still unclaimed), `next_executable_plan = 259`.
- `specs/protocols/05-tunnels.md`: authority section notes the
  corrected emission + the receipt-topology boundary + Plan 259.
- `specs/CONFORMANCE.md`: unchanged (no support-state change).
- Operator impact: none (no config, no listener, no wire change
  beyond emitting canonical fragments for all sizes, matching
  the reference).

## Handoff

Plan 259 adjudicates the receipt-topology boundary with three
bounded questions: (1) confirm the B-endpoint dispatch finding
against the pinned source (drop vs silent-forward inventory —
reading only, never patching); (2) survey whether A-ending
transit topologies ever occur in-lane (build-epoch evidence
across runs, no lane forcing); (3) EITHER exhibit a closing
topology (then replan WP D) OR record the receipt-row premise
revision (receipt belongs to the OBEP direction, which passes;
the IBGW lane closes at multicell + emission). Only after Plan
259 may the unblock audit mark `m11_transit_qualification` and
register M12 floodfill planning.
