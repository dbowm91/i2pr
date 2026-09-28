# Current authority amendment — Plan 260 corrective registered

Current status:
**retained-m11-ibgw-receipt-adjudication-endpoint-model-corrective-required-via-plan260**

The historical Plan 259 evidence below is preserved. Its seven-run/nine-chain inventory and its
source analysis of `TransitTunnelEndpoint(false)` remain valid for transit endpoints.

Its global conclusion is narrowed: the record did not model the distinct creator-owned
`InboundTunnel` path in exact-pinned i2pd. In an inbound tunnel, the last remote hop is
configured with `SetNextIdent(local_router)` and endpoint flag cleared; the creator-local
`InboundTunnel::HandleTunnelDataMsg` sets `msg->from` to the pool-owned tunnel; LOCAL garlic
dispatch can therefore reach `msg->from->GetTunnelPool()->ProcessGarlicMessage`.

The one historical A-ending chain did not bind its `next_tunnel` to a receiver-owned local
`InboundTunnel` and destination pool. It does not prove that such a controlled topology cannot
close receipt.

Accordingly:
- the Plan 259 "receipt is OBEP-only" interpretation is not current authority;
- the IBGW receipt row is restored as an open qualification requirement;
- Plan 260 is the corrective authority;
- M11 remains unclaimed/non-advertised;
- M12 remains deferred.

Original Plan 259 record follows unchanged below.

---

# Plan 259 — M11 IBGW receipt-topology adjudication diagnostic: status

**retained-m11-ibgw-receipt-premise-adjudicated-fork2-obep-owns-receipt**

## Disposition

Plan 259 adjudicated the single open boundary from Plan 258 and
takes **Fork 2**: no closing topology exists in-lane, so the
receipt-row premise is revised instead of the lane being forced.

- WP A source-locked the B-endpoint dispatch: a reassembled
  foreign-destined Local garlic at a transit endpoint can only
  reach the reference router-context garlic handler (tag-table
  lookup, then Noise_N decrypt with the router's own key) or the
  pool-owned local-destination path — and the pool path is
  structurally unreachable from transit endpoints. Neither exit
  can emit receipt for a destination-addressed garlic at a router
  that does not own the destination session.
- WP B tabulated every datagram-carrying IBGW chain across all
  seven retained Plan 257/258 executions: 8 of 9 chains terminate
  at B, 1 at A — and receipt is 0 in all seven runs, including
  the A-ending chain.
- Fork 1 (exhibit a closing topology, replan WP D) is refuted:
  the exhibited A-ending chain does not close, and the WP A
  mechanism shows no chain the lane can build could close —
  pool-owned termination is definitionally OBEP, not IBGW.
- WP D does not execute: the constant-`message_id` hardening is
  recorded as deferred (the proven emission path is not touched
  for robustness alone).

No row was weakened. No delivery-type deviation, no lane
forcing, no reference patching, no new dependency. No
capability beyond the evidenced subset is claimed. Hosted
exact-head CI on the closing line is pending at write time and
is recorded in a follow-up commit per the Plan 258 precedent;
the local floor below is fully green.

## Commits

- `bd070a3` — Plan 259 active (registry ready → active).
- Closure commit (this record + registry/roadmap/spec updates;
  SHA recorded at commit time) — no production code change;
  WP A/B/C are reading-only, WP D deferred.
- Follow-up: hosted CI evidence record (Plan 258 precedent).

Baseline: `7f28ffe` (Plan 258 closure line head; production tree
identical to `38c939a` plus the test-only parse-simulation
regression).

## Plan 259 §7 acceptance criteria — requirement-to-evidence matrix

1. **B-endpoint dispatch inventory with source-lock citations** —
   DONE (§"Dispatch inventory" below). Every post-reassembly
   arm is cited against exact-pinned i2pd 2.61.0 @ `635b013a`.
2. **Topology survey with `(receive_id, next_router)` counts** —
   DONE (§"Topology survey" below). Seven retained runs, nine
   datagram-carrying chains, bucketed per run against the
   per-run A/B identity mapping.
3. **Exactly one fork taken with cited evidence; the other
   recorded as refuted** — DONE. Fork 2 taken (§"Fork
   decision"); Fork 1 refuted with ledger citations (the lone
   A-ending chain: `0xf6f56b0a`, 2 delivered, receipt 0) and
   the mechanism argument (no lane-buildable chain can close).
4. **Fork 2 artifacts: premise revision + restated M11 claim,
   no capability beyond it** — DONE (§"Premise revision" and
   §"Restated M11 claim" below).
5. **Full workspace verification passes; exact-head
   Ubuntu/macOS/MSRV/dependency-policy all pass** — LOCAL
   floor green (§"Verification"); hosted CI on the closing
   head pending, recorded in follow-up (same precedent as
   Plan 258's `7f28ffe`).
6. **No critical/high finding open without a named owner; the
   Plan 258 medium msgid finding fixed or re-owned** — DONE.
   No critical/high findings. The msgid medium is re-owned as
   deferred hardening (§"Findings"); one new low residual (sync
   dispatch-line divergence) is recorded with a recommended
   follow-up owner (§"Findings").

## Dispatch inventory (WP A)

Reference: unmodified i2pd 2.61.0 @ `635b013a`,
`target/interop/ssu2-sources/` (read-only, never patched).

Reassembly → dispatch (all four completion sites funnel here):

- `libi2pd/TunnelEndpoint.cpp:135` (single message),
  `:171` (`HandleFollowOnFragment` complete), `:228`
  (`HandleCurrenMessageFollowOnFragment` complete), `:270`
  (out-of-sequence complete) → `HandleNextMessage` (`:311`).
- `HandleNextMessage` logs the reassembly
  (`:319`, the observed `Handle fragment of N bytes, msg type
  11`; type 11 = `eI2NPGarlic`, `libi2pd/I2NPProtocol.h:115`)
  then dispatches on delivery type: Local arm `:323-325`
  calls `i2p::HandleI2NPMessage`; Tunnel/Router arms
  (`:326-337`) forward — unreachable for our injection (and
  for the reference IBGW, which hardcodes Local).
- Our emission provably carries Local: first-fragment flag
  `0x08` (`DeliveryInstruction::Local` + fragmented bit,
  `crates/i2pr-tunnel/src/data.rs:715-720`), asserted by the
  Plan 258 parse-simulation regression
  (`crates/i2pr-tunnel/src/transit.rs`, first flag `0x08`,
  delivery bits `(flag >> 5) & 0x03 == 0`).

Garlic arm (`libi2pd/I2NPProtocol.cpp:476-483`):

- Transit-endpoint traffic arrives with `from == null`:
  `TransitTunnelEndpoint::HandleTunnelDataMsg` builds a fresh
  `newMsg` (`libi2pd/TransitTunnel.cpp:129-137`) and never
  sets `from`; transit tunnels are never pool members
  (`m_Pool` initializes null, `libi2pd/Tunnel.cpp:34`;
  `SetTunnelPool` fires only on pool-owned creation paths,
  never on `CreateTransitTunnel`,
  `libi2pd/TransitTunnel.cpp:178-...`).
- So the dispatcher takes `i2p::context.ProcessGarlicMessage`
  (`:481`), which only `boost::asio::post`s to the service
  (`libi2pd/RouterContext.cpp:1285-1291`).
- `PostGarlicMessage` (`RouterContext.cpp:1293-1311`): router
  tag-table lookup first — a miss returns false with NO log
  (`libi2pd/Garlic.cpp:560-573`); then Noise_N one-time
  decrypt with the ROUTER's key
  (`ECIESX25519AEADRatchetSession.cpp:1381-1406`). Our
  garlics are ECIES-encrypted to rx's destination key on A,
  so at B this can only fail AEAD (`:1398-1403`, warning) —
  and even a success would execute the cloves LOCALLY at B
  via `HandlePayload`, never forward them to A.

The unreachable receipt-capable arm:

- `TunnelPool::ProcessGarlicMessage`
  (`libi2pd/TunnelPool.cpp:477-481`) routes to the pool's
  local destination session (decrypt + deliver) — this is the
  mechanism by which real pool-owned inbound tunnels
  deliver. It requires `msg->from->GetTunnelPool()`
  non-null, i.e. a pool-owned terminating tunnel. Transit
  endpoints can never satisfy it.

Dispatch outcome: **deterministic non-delivery**. No code
path at a transit endpoint routes a destination-addressed
garlic onward to another router or socket. The terminus
router would need to own the destination's garlic session;
transit termination never consults destination sessions.

Retained-log observation (consistent, all runs): in
`m11-plan258-diag-drop2` B completes 52 endpoint messages
(all type 11), including the 3 proven datagram garlics
(1487/1496/1491 bytes at the exact emission seconds,
matching the 3 `nested-multi`/`emitted-multi` emissions,
`failures-total: 0`) — with zero downstream lines of any
kind (no dispatch, no decrypt outcome, no forward). The
same silence holds for every other endpoint completion on
that tunnel.

Residual (low, owned — see §"Findings"): the straight-line
code predicts a synchronous `I2NP: Handling message with
type 11` debug line per Local completion, yet none appears
on the tunnel thread for any of the 52 completions (exactly
one such line exists in the whole log, at startup, from the
gateway path). Candidate explanations (early drop before
the dispatcher vs. logger behavior) were both exhausted
against source without a constructed mechanism; both are
receipt-incapable, so the fork does not depend on resolving
it. Recommended follow-up: a binary-attribution rerun is
NOT authorized by this plan (WP B forbids new execution)
and is left to the next M11 emission-touching plan.

## Topology survey (WP B)

Method: per retained run, `driver/ledger-evidence.tsv`
`ibgw-data/BuildAccepted` rows with `role=ibgw` give the
datagram-carrying chains `(recv, next)`; A/B identity per
run comes from `bootstrap/a-knows-b-ri-exact` (A's NetDB
copy of B's RouterInfo, base64 → hex). Fresh datadirs per
run, so hashes are run-local. Receipt and diagnostic keys
from `driver/driver-evidence.tsv`.

Datagram-carrying IBGW chains (all runs, receipt 0 everywhere):

| run | recv | next | delivered | receipt |
|---|---|---|---|---|
| 257-attempt-1 | 0x206d1d | B | 9 | 0 |
| 257-attempt-2 | 0xc36163fa | B | 13 | 0 |
| 257-infoB | 0x58d4a568 | B | 12 | 0 |
| 257-infoB | 0xf6f56b0a | **A** | 2 | 0 |
| 258-attempt-1 | 0x39f68a12 | B | 18 | 0 |
| 258-h1h2h3 | 0x218f5cbf | B | 3 | 0 |
| 258-h1h2h3 | 0x3337e15c | B | 8 | 0 |
| 258-drop1 | 0xb29420a3 | B | 15 | 0 |
| 258-drop2 | 0x99ae60e1 | B | 19 | 0 |

Totals: 9 chains, 8 B-ending, 1 A-ending, 99
GatewayDelivered emissions, 0 receipts, 0 forward failures
in every run carrying the diagnostic keys. Build-plane
accepted IBGW registrations (`epoch=ibgw`, `role=ibgw`)
likewise chain to both A and B per run (e.g. drop2: 2×
A-ending, 1× B-ending builds) — A-ending topologies occur
in-lane, but only one ever carried datagrams, and it did
not close.

## Fork decision (WP C): Fork 2

Fork 1 is refuted:

- The only exhibited A-ending datagram chain (infoB
  `0xf6f56b0a` → A `8a953c8b…`, 2 `GatewayDelivered`,
  run `gateway-receipt: 0`) does not close. (A's retained
  log for that short diagnostic shows no endpoint
  reassembly at all, so the chain is unexhibited past
  emission — it cannot ground a WP D replan either.)
- The WP A mechanism generalizes beyond the exhibited
  sample: ANY transit terminus (A or B) processes a
  destination-addressed Local garlic through the
  router-context arm (wrong key domain, no destination
  sessions consulted) and can never reach the pool-owned
  delivery arm. A topology the lane cannot build —
  pool-owned termination at the receiver's router — is
  definitionally OBEP, not IBGW.
- Exhibiting a closing topology would therefore require
  lane forcing (all-datagram-to-A replumbing) for a chain
  the mechanism already rules out, or reference patches.
  Both are forbidden by §3/§8. Stop.

Fork 2 is taken: the receipt row premise is revised
(§"Premise revision"), the M11 claim is restated within
the evidenced subset (§"Restated M11 claim"), and WP D
replumbing is not replanned.

## Premise revision (Fork 2 record)

Old premise: an IBGW-injected Local garlic can close
far-side receipt at a transit terminus in-lane.

Revised premise: far-side receipt in this lane is an OBEP
property. The IBGW lane's terminal proven rows are:

- accepted-registration ingress (`GatewayDelivered`;
  99 emissions across 9 chains, 7 runs);
- multicell-bounded emission (3× 2-cell datagram
  emissions, `emitted-max: 2`, zero forward failures —
  Plan 258, `38c939a`);
- byte-exact reference-parse conformance of the emission
  (Plan 258 parse-simulation regression).

Receipt (`gateway-receipt`) belongs to the OBEP direction,
which passes with payload-verified receipts (retained
Plans 250–257; untouched by this plan). The IBGW receipt
row is removed from the lane's closable set with this
record as its stop provenance — not weakened, retired as
unexhibitable after 9 chains and a construction-level
mechanism.

## Restated M11 claim

M11 may claim, within the evidenced subset only:
experimental one-family transit **emission** (IBGW ingress
→ canonical fragmented emission → reference reassembly,
zero forward failures), plus the retained OBEP receipt
direction — all non-advertised (`advertised=false`), no
public transit, no RouterInfo capability, no two-family
conformance. Full transit qualification
(`m11_transit_qualification` as a receipt-capable claim)
remains unclaimed; M12 floodfill planning stays deferred
(see §"Roadmap disposition").

## WP D disposition

Not executed per §4 (Fork 2 only). The constant fragment
`message_id: 1` (Plan 258 medium finding) is re-owned here
as **deferred hardening**: replace with a per-registration
monotonic id (or the nested I2NP msgid via a new
`i2pr-proto` accessor) with a distinct-ids regression —
owned by the next M11 emission-touching plan. The proven
emission path is not touched for robustness alone.

## Verification (local truth; CI labeled)

Local floor on the closing line (all green; no production
code changed by this plan — WP A/B/C are reading-only):

- `cargo fmt --all --check` — clean.
- `cargo check --locked --workspace --all-targets` — clean.
- `cargo test --locked --workspace --all-targets --
  --test-threads=1` — 3093 passed, 27 ignored, 106
  suites, 0 failed.
- `cargo clippy --locked --workspace --all-targets
  --all-features -- -D warnings` — no issues.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked
  --workspace --no-deps` — generated clean.
- `cargo test --locked --workspace --doc` — 0 passed
  (16 suites), 0 failed.
- `cargo deny check advisories bans sources` —
  advisories/bans/sources ok.
- All 20 gate scripts green:
  `check-dependency-direction`,
  `check-runtime-boundaries`,
  `check-service-tunnel-boundaries`, `check-fixture-manifest`,
  `check-ntcp2-vectors`, `check-ssu2-vectors`,
  `check-i2cp-vectors`, `check-ntcp2-interoperability`,
  `check-constrained-host-lane-boundary`,
  `check-m11-transit-boundaries`,
  `check-m11-transit-qualification-evidence`,
  `check-sam-acceptance-evidence`,
  `check-ssu2-acceptance-evidence`,
  `check-i2cp-acceptance-evidence`,
  `check-service-tunnel-acceptance-evidence`,
  `check-exploratory-tunnel-evidence`,
  `check-netdb-tunnel-evidence`,
  `check-destination-tunnel-evidence`,
  `check-streaming-tunnel-evidence`,
  `check-m6-mixed-router-acceptance-evidence`.
- NTCP2 harness: 18 tests ok.
- `git diff --check` — clean.
- Plan 258 emission/telemetry/checker rows re-verified
  green as part of the workspace run (no suppression).

Hosted exact-head CI (Ubuntu/macOS/MSRV/policy): GREEN on
the closing line — Actions run `36470821009` on head
`a175a4f` (Quality ubuntu-latest, Quality macos-latest,
MSRV Ubuntu, Dependency policy: all success). The run
covers the closure record plus all planning/spec updates;
the production tree is unchanged since `51bd472` (whose
run `36461901007` was likewise green).
Note: the Plan 258 line's hosted CI (run `36461901007`,
all four jobs green on `51bd472`) already covers the
production tree this plan retains unchanged.

No external execution was performed or required (§6: none
required to close under Fork 2).

## Invariant / failure / migration / security reviews

- Invariants: no production code touched; frozen §3
  invariants (exact pin, unmodified reference,
  loopback-only, fresh datadirs, no gate redefinition, no
  retry tuning, sanitized counts only, no delivery-type
  deviation, no lane forcing) held — WP A/B read pinned
  source and retained evidence only.
- Failure semantics: nothing new can fail at runtime
  (docs + registry only). The adjudication itself is
  fail-closed: Fork 2 retires rather than redefines.
- Migration/compat: none (no wire, config, or API change).
- Security: no secret material handled; no new attack
  surface; retained B-log forensics used only counts,
  sizes, and message-type numbers already present in
  sanitized evidence shapes. Raw reference logs remain
  unmerged (under `target/`, never committed).

## Findings by severity

- MEDIUM (re-owned, was Plan 258 open): constant fragment
  `message_id: 1` across sequential fragmented emissions —
  cross-assembly collision risk under concurrency at a
  shared endpoint. Deferred hardening owned by the next
  M11 emission-touching plan (§"WP D disposition"). Not
  touched here per §4.
- LOW (new, owned): sync dispatch-line divergence — the
  source-locked Local arm predicts a per-completion
  `I2NP: Handling message` debug line that is absent for
  all 52 retained endpoint completions. Both constructed
  explanations are receipt-incapable, so the fork is
  unaffected. Recommended owner: the next M11
  emission-touching plan (binary-attribution rerun; new
  execution, needs its own plan authority).
- No critical/high findings. No silent rows, no weakened
  gates, no tuning.

## Roadmap disposition + unblock audit

- Plan 259 closes retained with the Fork 2 adjudication;
  no WP D replan is registered (refuted, not deferred).
- `m11_transit_qualification`: marked with the restated
  claim (experimental one-family emission + retained OBEP
  receipt, non-advertised) — full receipt-capable transit
  qualification remains unclaimed by explicit record.
- M12 floodfill: planning stays deferred (it requires
  receipt-capable transit, which this record shows the
  IBGW lane cannot exhibit). No M12 plan registered; the
  roadmap notes the dependency.
- Registry audit: no other registered plan lists Plan 259
  (or the IBGW receipt row) as a hard/interface
  dependency; nothing else unblocks. Plan 201/247 Java
  debt and Plan 204 bookkeeping are unaffected.

## Docs / ops

- This record; `plans/registry.md` (259 ready → active →
  retained-adjudicated); `plans/subsystems/
  transit-tunnels-roadmap.md` (§7 row + Fork 2 outcome);
  `specs/support.toml` (`plan_259_status`,
  `m11_transit_tunnels` restated claim);
  `specs/protocols/05-tunnels.md` (Plan 259 paragraph:
  ready → adjudicated Fork 2).
- `specs/CONFORMANCE.md`: no support-state change beyond
  the restated (narrower) claim — the M11 rows stay
  unclaimed there, consistent.
- Operator impact: none (no daemon/config change;
  transit stays non-advertised and disabled by default).

## Handoff

To close: commit this record with the registry/roadmap/
spec updates, push, observe exact-head hosted CI (four
jobs), record the run ID in a follow-up commit (Plan 258
precedent). Then M11 rests at the restated claim until a
future plan re-opens emission work (owning the two
deferred items above) — or the subsystem roadmap accepts
the IBGW receipt retirement permanently.
