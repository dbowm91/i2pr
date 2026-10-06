# Plan 313 closure — i2pd-compatible Streaming profile convergence

Status: **passed-i2pd-streaming-max-payload-profile-converged-8-of-8-matrix-cells-match**

Plan: `plans/implementation/anonymity/313-i2pd-streaming-profile-convergence.md`
Authority: ADR 0030; Plan 312 passed (`plans/closure/anonymity/312-status.md`).
Reference: i2pd 2.61.0, revision `635b013a612ff47278ef02acf8580a28e10e26c5`.

## Result

The one remotely observable Streaming dimension Plan 312 registered as differing is
now converged. i2pr advertises an **1812-byte** maximum payload, byte-identical to
exact-pinned i2pd 2.61.0, in **both** the client and the server role. The re-run of
the exact Plan 312 directional lane reports **8 of 8** matrix cells `match`, against
a baseline in which 2 of 8 differed.

Plan 313 §5.3 is satisfied structurally, not just numerically: the advertised
service profile and the hard decode ceiling are now separate declarations that
**cannot be re-welded silently**. A compile-time invariant fails the build if the
profile ever exceeds the ceiling, and a mutation-proven runtime assertion fails if
the two are re-merged into one value.

The reference lane was executed twice on this host — once as the Plan 313 baseline
before any code change, and once after the §5.3 residual fix — so the converged
value is measured, not inherited.

## Target freeze (WP1) — the pre-change matrix, re-executed here

Plan 312's matrix was re-run at HEAD `2f82c799` **before** any code change, against
the same pinned reference, to confirm the target rather than assume it:

```
role     dimension       i2pr   i2pd   result
client   flags           1193   1193   match
client   from_included   True   True   match
client   max_payload     1730   1812   different
client   payload_length  0      0      match
server   flags           169    169    match
server   from_included   True   True   match
server   max_payload     1730   1812   different
server   payload_length  0      0      match
```

Six of eight cells already matched. The scope was therefore exactly one dimension
in two roles, not a Streaming re-architecture.

## Post-change matrix (WP5) — the exact Plan 312 rerun

```
role     dimension       i2pr   i2pd   result
client   flags           1193   1193   match
client   from_included   True   True   match
client   max_payload     1812   1812   match
client   payload_length  0      0      match
server   flags           169    169    match
server   from_included   True   True   match
server   max_payload     1812   1812   match
server   payload_length  0      0      match
```

## Requirement disposition

| Requirement | Result | Evidence |
|---|---|---|
| Freeze target matrix before code changes (WP1/§5.1) | passed | pre-change matrix above, re-executed on this host |
| Map each observed dimension to the active i2pr value (WP2/§5.2) | passed | `DEFAULT_ADVERTISED_MAX_PAYLOAD` at `crates/i2pr-proto/src/streaming/packet.rs`, consumed symbolically by `i2pr-client/src/streaming/manager.rs` |
| Centralize service-profile values separately from hard safety ceilings (§5.3) | passed | profile `1812` and ceiling `2048` are independent declarations, each with a compile-time invariant; §D2 records the residual this initially missed |
| Minimum change required to match measured behaviour (§5.4) | passed | one wire constant plus the decouple seam; no congestion, ACK, RTO, window, or retransmission behaviour altered |
| Re-run deterministic correctness/loss/reorder/close tests (§5.5) | passed | `i2pr-proto` 182 passed, `i2pr-client` all targets passed, `i2pr-daemon` + `i2pr-runtime` 1951 passed / 0 failed / 35 ignored |
| Re-run the exact Plan 312 directional evidence (§5.6) | passed | post-change matrix above; lane exit 0, checker passed, exact pin verified |
| Classify residual differences (WP6/§5.7) | passed | §Residual disposition below |
| Plan 313 §3 invariants | passed | no source-constant-only tuning (the value is measured on the wire); no Java gate; no randomization; no ceiling/correctness relaxation; unobservable dimensions untouched; one coherent profile |
| **Exact-head routine CI green** | **no CI reachable from this environment** | **UNPROVEN** |

## The change

| Constant | Before | After | Role |
|---|---|---|---|
| `DEFAULT_ADVERTISED_MAX_PAYLOAD` | `1730` | **`1812`** | the qualified **service profile**, put in the SYN `MAX_PACKET_SIZE` option |
| `MAX_STREAMING_PAYLOAD_BYTES` | `= DEFAULT_ADVERTISED_MAX_PAYLOAD as usize` (an alias) | **`= 2048`** | independent **hard decode / byte-budget ceiling** |
| `MAX_STREAMING_PACKET_BYTES` | `3032` (derived) | `3350` (derived) | worst-case full-packet envelope |

Before this plan the ceiling was a **direct alias** of the advertisement, so a later
profile-tuning pass would have dragged the hostile-decode bound along with it. That
coupling is what Plan 313 §5.3 exists to break, and it is broken by value, by
declaration, and by a compile-time invariant.

### Why the ceiling is 2048 and not something else

- It must be at least 1812: a peer advertising the same profile may legitimately send
  a full 1812-byte payload, and we must be able to decode what we advertise.
- **1812 was rejected** — it makes the invariant vacuous by value (`1812 <= 1812`),
  so re-welding would become undetectable, which is exactly what §5.3 forbids.
- **61 440 was rejected** — that is the destination-path I2CP Data body ceiling. It
  exists because the *transport*, not the codec, bounds a packet on that path.
  Reusing it would widen the per-packet hostile-decode surface ~30x for no measured
  benefit; Plan 312 measured one divergence and §5.4 requires the minimum change.
- 2048 is a power-of-two buffer boundary and leaves 236 bytes (13.0%) of headroom for
  minor peer-profile variance.

## Safety arithmetic

**Packet ceiling.** `22 + 64*4 + 1024 + ceiling`: 3032 before, 3350 after. The only
downstream compile-time assertion on this value, in `i2pr-daemon`'s
`streaming_adapter.rs`, requires the I2CP Data body ceiling (61 440) to exceed it —
margin 58 090, holds.

**Tunnel fragment count is unchanged.** From `crates/i2pr-tunnel/src/data.rs`:
`MAX_PLAINTEXT_DATA_BYTES = 1008`, cell body budget `1008 - 4 - 1 = 1003`,
`MAX_FRAGMENT_COUNT = 64`. Worst-case `DeliveryInstruction::Tunnel`:
`fragmented_first_overhead = 43` gives a 960-byte first body and a 996-byte follow-on
budget.

| case | packet | first | remaining | follow-ons | fragments |
|---|---|---|---|---|---|
| before | 3032 | 960 | 2072 | `ceil(2072/996) = 3` | **4** |
| after (ceiling) | 3350 | 960 | 2390 | `ceil(2390/996) = 3` | **4** |
| after (at advertised profile) | 3114 | 960 | 2154 | `ceil(2154/996) = 3` | **4** |

Four of a permitted 64 fragments in every case. Capacity headroom
`960 + 63*996 = 63 708` bytes.

**The decisive evidence that the tunnel already carries 1812.** Plan 193 recorded i2pd
sending 1812-byte Streaming payloads *through i2pr's own tunnel* and being rejected as
`Codec/PayloadOverflow(1812 > 1730)` — a **codec**-layer rejection, raised after the
bytes had traversed the tunnel and been reassembled. The tunnel envelope was never
the limiting factor; the codec bound was. No tunnel change was needed or made.

### Runtime defaults that change, and why each is necessary

| Default | Change | Necessary because |
|---|---|---|
| `StreamingReceiveLimit::default().max_payload_bytes` | 1730 → 2048 | we advertise 1812; a bound below that rejects traffic we invited |
| `StreamingReceiveLimit::default().max_packet_bytes` | 3032 → 3350 | follows the above |
| `StreamingSendLimit::default()` both fields | same | the encoder must emit a full 1812-payload packet (1834 bytes); at 1730 it would refuse |
| `SendWindowConfig::max_window_bytes` | `W*1730` → `W*2048` | at the old value, legitimately negotiated 1812-byte packets exceed `W*1730` and cause spurious byte backpressure |
| `delivered_cap_bytes()` | `R*1730` → `R*2048` | same derivation |

The window and cap are **byte-budget** derivatives of a **packet-count** window.
Congestion control, the packet-count window, and every ACK/RTO/retransmission
constant are untouched, so Plan 313 §3.5 holds. See finding F2 for the recorded
budget change.

`StreamingReceiveLimit::destination_path()` is retained unchanged. It is public API,
Plan-193-pinned, and still strictly wider than the default for a peer that negotiates
a larger per-packet payload than we advertise.

## D1 — the two invariants, and why one of them is not enough

The decoupling is defended twice, and the second defence exists because the first one
has a hole:

1. `const _: () = assert!(profile <= ceiling)` in `i2pr-proto`, mirrored in
   `i2pr-client`. Catches a profile that exceeds the ceiling.
2. A runtime `assert_ne!` in `plan128_wire.rs` plus the profile/ceiling assertions in
   `plan128_trajectory.rs`. Catches the ceiling being re-aliased to the profile.

Defence 1 alone is **vacuous when the ceiling equals the profile**: `1812 <= 1812`
holds under a re-weld, so the crate still compiles. This was observed, not
hypothesised — see the mutation transcript, where mutation 1 compiled cleanly and only
the runtime assertion fired. That is precisely why 2048 was chosen over 1812 and why
the `assert_ne!` is ordered first.

## D2 — a §5.3 residual the first implementation pass missed

The first implementation pass left one place where the profile and the ceiling were
still welded: `crates/i2pr-client/src/streaming/config.rs` defined
`MAX_PACKET_PAYLOAD_BYTES` as an alias of the proto **ceiling**, and
`crates/i2pr-client/src/streaming/connection.rs` seeded a freshly constructed
`StreamingConnection`'s `local_advertised_max_payload` from it. A field whose name
asserts it holds what we advertise was being filled from the hard decode bound.

Because the negotiation is `min(local, remote)`, this let the negotiated per-packet
payload reach **2048 while i2pr advertised 1812 on the wire** — a per-packet
over-promise against any peer advertising more than 1812.

The fix introduced `MAX_ADVERTISED_PACKET_PAYLOAD_BYTES` in `config.rs` (the
definition site where the confusion originated), a mirrored compile-time invariant,
and seeding both constructors from the profile. It also corrected a stale doc claim
on `MAX_STREAMING_PAYLOAD_BYTES_PER_PACKET` that asserted "the initial SYN
advertises the local ceiling" — a sentence that was itself part of how the
conflation persisted.

**Correction to my own framing.** I initially described the defect as breaking
`negotiated <= local_advertised`. That is wrong, and the worker caught it: with both
sides seeded from the ceiling, the internal comparison still held (2048 <= 2048). The
quantity that actually broke was `negotiated <= the advertisement i2pr publishes on
the wire` (2048 > 1812). The test asserts profile **equality** precisely because that
is the quantity that fails.

The fix is internal-consistency only. It has **no wire-visible effect**, proven twice:
the SYN is built from a `manager.rs` function parameter whose callers pass
`DEFAULT_ADVERTISED_MAX_PAYLOAD`, and the manager overwrites the connection field
before negotiation on every production path. The lane still reports 1812 after the
fix, which is the empirical confirmation.

## Residual disposition (WP6 / §5.7)

| Dimension | Disposition |
|---|---|
| `max_payload`, client and server role | **matched** — 1812 == 1812, measured on the wire |
| `flags`, client and server role | **already matched** before this plan (1193 / 169); unchanged |
| `from_included`, client and server role | **already matched** before this plan; unchanged |
| `payload_length`, client and server role | **already matched** before this plan (0); unchanged |
| window / choke | `NotReliablyObservable` — observed as `-` in all four traces; **unchanged**, per §3.5 |
| ACK / NACK | `NotReliablyObservable`; unchanged |
| RTO / retransmission | `NotReliablyObservable`; unchanged |
| loss / reorder | `NotReliablyObservable`; unchanged |
| close / reset | `NotReliablyObservable`; unchanged |
| terminal state | `NotReliablyObservable`; unchanged |
| timing | **not registered**; no timing conclusion drawn |

The `NotReliablyObservable` set is unchanged **by observation, not by choice**. Plan 313
§5.7 classifies it as not requiring action; it would enter scope only via a new
registered scenario and a new source-observability review, which is out of this plan's
scope and is not claimed here.

## Deliberately unchanged test doubles

These `1730` occurrences are deliberate inputs, not stale expectations, and changing
them would destroy what they test. All verified intact in the final tree:

- `crates/i2pr-daemon/src/destination_streaming.rs` — 6 injected mock endpoint values.
- `crates/i2pr-client/tests/plan128_trajectory.rs` — 6 `establish_pair` calls
  including the deliberate asymmetric-negotiation cases (`1730/1400`, `1200/2000`,
  `3000/1500`).
- `crates/i2pr-testkit/src/streaming_fingerprint.rs` — a `#[cfg(test)]` TSV
  round-trip fixture whose number is arbitrary, not a golden.
- `tests/integration/anonymity/test_streaming_fingerprint.py` — a synthetic TSV
  fixture whose assertions cover `flags` rows only.

## Mutation transcripts

**M1 — re-weld the ceiling to the profile** (`MAX_STREAMING_PAYLOAD_BYTES =
DEFAULT_ADVERTISED_MAX_PAYLOAD as usize`):

```
test plan313_service_profile_is_decoupled_from_the_hard_safety_ceiling ... FAILED
panicked at crates/i2pr-proto/tests/plan128_wire.rs:159:5:
assertion `left != right` failed: the hard safety ceiling must stay a separate
declaration from the advertised profile
  left: 1812
 right: 1812
```

The crate **still compiled** — `assert!(1812 <= 1812)` holds. Recorded because it is
the concrete proof that the compile-time invariant alone is insufficient.

**M2 — profile exceeds the ceiling** (`DEFAULT_ADVERTISED_MAX_PAYLOAD = 4096`):

```
error[E0080]: evaluation panicked: advertised Streaming profile must never exceed
the hard safety ceiling
```

**M3 — reintroduce the §5.3 connection-seeding defect** (both constructors seeded
from the ceiling again):

```
test plan313_fresh_connection_advertises_service_profile_not_hard_ceiling ... FAILED
panicked at crates/i2pr-client/tests/plan128_trajectory.rs:611:9:
assertion `left == right` failed
  left: 2048
 right: 1812
```

**M4 — M3 with the pre-negotiation assertions suppressed**, so the negotiation-stage
assertion is the one that fires:

```
panicked at crates/i2pr-client/tests/plan128_trajectory.rs:621:9:
assertion `left == right` failed
  left: 2048
 right: 1812
```

Line 621 is the post-negotiation assertion: against a peer advertising 65535, the
negotiated payload was 2048 while our published advertisement is 1812.

All four mutations were restored and the tree re-verified green afterwards.

## Commands run — all **local**; no CI is available in this environment

| Command | Outcome |
|---|---|
| `bash tests/integration/anonymity/run-plan312-streaming.sh` (baseline, pre-change) | **exit 0**; exact pin verified; 2/8 cells `different` |
| `bash tests/integration/anonymity/run-plan312-streaming.sh` (post-change) | **exit 0**; 8/8 `match` |
| `bash tests/integration/anonymity/run-plan312-streaming.sh` (post-§5.3-fix) | **exit 0**; 8/8 `match`, wire unchanged at 1812 |
| `cargo test --locked -p i2pr-proto --all-targets` | **182 passed, 0 failed** |
| `cargo test --locked -p i2pr-client --all-targets` | **all targets passed**, 0 failed; 19 test binaries |
| `cargo test --locked -p i2pr-daemon -p i2pr-runtime --all-targets -- --test-threads=1` | **1951 passed, 0 failed, 35 ignored**; exit 0 |
| `cargo fmt --all --check` | **clean**, exit 0 |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **clean**, workspace finished with no diagnostics |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | **clean**, no warnings |
| `bash scripts/check-dependency-direction.sh` | **PASS** |
| `bash scripts/check-runtime-boundaries.sh` | **PASS** |
| `bash scripts/check-console-boundaries.sh` | **PASS** |
| `bash scripts/check-console-browser-security.sh` | **PASS** |
| `bash scripts/check-service-tunnel-boundaries.sh` | **PASS** |
| `bash scripts/check-service-anonymity-boundaries.sh` | **PASS** |

`cargo test --locked --workspace --doc` and the full `AGENTS.md` routine floor were
**not run**. The floor carries ~11 000 s of serialized `tokio::time::sleep` across
socket-backed suites; the crate-level suites covering every crate that consumes the
changed constants were run individually instead, and this is stated rather than
implied. The exact command outcomes for the workspace-wide static gates are recorded
in §Known limitations.

## Commits

| Change | Commit |
|---|---|
| Plan 313 implementation (10 files: production, tests, docs, spec) | `1f649fc0` |
| `plans/closure/anonymity/313-status.md` (this record) | committed with the planning update |
| `plans/registry.md`, `plans/subsystems/anonymity-roadmap.md` | committed with this record |

Implementation commit: **`1f649fc0`** — *plan313: converge the Streaming max-payload
profile on pinned i2pd 1812*.

Files changed: `crates/i2pr-proto/src/streaming/packet.rs`,
`crates/i2pr-proto/tests/plan128_wire.rs`,
`crates/i2pr-client/src/streaming/config.rs`,
`crates/i2pr-client/src/streaming/connection.rs`,
`crates/i2pr-client/src/streaming/manager.rs`,
`crates/i2pr-client/tests/plan128_trajectory.rs`,
`docs/architecture/i2pr-client.md`, `docs/architecture/i2pr-proto.md`,
`docs/protocol-support.md`, `specs/references/streaming-packet-wire.md`.

## Findings by severity

- **F1 (medium, fixed in this plan)** — the profile/ceiling separation did not
  propagate into `i2pr-client`'s connection seeding; a field named
  `local_advertised_max_payload` was filled from the hard ceiling, permitting a
  negotiated payload above our own advertisement. Fixed with a named constant, a
  mirrored invariant, and a mutation-proven test. See D2.
- **F2 (low, recorded, not fixed)** — the derived byte budgets
  (`SendWindowConfig::max_window_bytes`, `delivered_cap_bytes()`) grow from
  `W*1730`/`R*1730` to `W*2048`/`R*2048`, a **+18.4%** byte budget, as a direct
  consequence of the decoupled ceiling. Assessed as safe: the window is a
  **packet-count** window and the byte figure is a derived upper bound, so no
  unbounded queue or allocation growth follows, and congestion control is untouched.
  Recorded because it is a real resource-budget change, not because a defect was found.
- **F3 (low, recorded)** — a Plan-193-pinned unit test
  (`destination_path_accepts_reference_sized_payloads`) asserted that an 1812-byte
  payload **fails** under the default receive bound, with the message "default bound
  still rejects above-advertisement payloads". Once 1812 became our own advertised
  profile that assertion is false by construction. It was rewritten to the Plan 313
  post-condition while **preserving both properties it guarded**: the default bound
  still fails closed above the hard ceiling, and `destination_path()` still accepts
  the profile while rejecting anything above the I2CP body ceiling.
- **F4 (low, recorded)** — `MAX_STREAMING_PAYLOAD_BYTES_PER_PACKET` still aliases the
  ceiling (2048) and so is now a loose upper bound on the negotiated payload, which is
  in fact capped at 1812 by the advertisement. Conservative and safe; its doc comment
  was corrected to state the relationship rather than the old, false claim.

## Migration and compatibility

No configuration surface changed and no persisted state is affected. The change is
internal to the Streaming packet constants. A peer that already negotiated with i2pr
at 1730 continues to work: the negotiated value is `min(local, remote)`, so it can
only rise toward the peer's own advertisement, never above it. Streaming is not an
advertised or externally claimed capability, and no `specs/support.toml` entry changes.

## Security and contention

The hard decode ceiling is the security-relevant bound, and it **widened** from 1730
to 2048 — deliberately, and by the smallest defensible margin over the profile, with
the rationale recorded at the declaration. It did not widen to the destination-path
transport bound (61 440), which would have been a ~30x increase in per-packet
hostile-decode surface. The receive bound that actually applies on the destination
path (`StreamingReceiveLimit::destination_path()`) is unchanged. No secret, key, or
identity material is touched. Concurrency is untouched: no task, channel, timer, or
cancellation behaviour changed.

## Documentation

Updated for the new value and, where the two values are no longer equal and no longer
co-located, split into separate rows: `docs/architecture/i2pr-proto.md`,
`docs/architecture/i2pr-client.md`, `docs/protocol-support.md`,
`specs/references/streaming-packet-wire.md`. Source line counts in the architecture
deep-dives were corrected for the changed files rather than left to drift.

The claim boundary is stated at the constant itself: this is a service-profile
advertisement qualified against **one pinned peer** and **one registered scenario**.
It is **not** a claim of general interoperability, of Java I2P equivalence, or of any
anonymity property.

## Known limitations

- **Exact-head routine CI is UNPROVEN.** No CI is reachable from this environment.
  This is the one acceptance criterion with no local substitute.
- **The full `AGENTS.md` routine floor was not run**, and neither was
  `cargo test --locked --workspace --doc`. The floor carries ~183 minutes of
  serialized `tokio::time::sleep` across socket-backed suites. Every crate that
  consumes the changed constants (`i2pr-proto`, `i2pr-client`, `i2pr-daemon`,
  `i2pr-runtime`) was tested individually, and the workspace-wide static gates —
  `cargo fmt --all --check`, workspace clippy under `-D warnings`, workspace rustdoc
  under `-D warnings`, and the six boundary scripts above — were all run and all
  passed. **What is therefore missing is the remaining workspace *test* execution and
  the doctest pass, not a static gate.** No step of the floor is claimed as passing on
  the strength of a targeted run.
- **One registered scenario only.** The convergence is evidenced by
  `clean_handshake_default_port` against one pinned peer. The evidence does not extend
  to any other scenario, to Java I2P, or to any second i2pd build.
- The `NotReliablyObservable` dimensions were not observed, so their convergence status
  is unknown — not "unchanged and correct", but unmeasured. See §Residual disposition.

## Unblock audit

Audited `plans/registry.md` blocked work plus the anonymity roadmap dependency graph
for any plan naming Plan 313 as a hard or interface dependency:

- The **unregistered** integrated anonymity successor (`plans/subsystems/anonymity-roadmap.md`
  diagram: "passing 308 + 313" -> future integrated successor to stopped Plan 301) is
  **not registered as a plan**, so it cannot be moved to `ready` here. It remains gated
  on **Plan 308**, which is independently blocked on a controlled ordinary-HTTP
  topology and three-family captures that do not exist on this host.
- **Plan 308** — independently blocked, unchanged by this plan.
- **Plan 317** — blocked historical record, unaffected.

**Verdict: closing Plan 313 unblocks nothing that is registered.** The only
successor remains gated on Plan 308's external topology. Recorded rather than
silently omitted.

## Roadmap disposition

`plans/subsystems/anonymity-roadmap.md`: the milestone row for 313 moves `ready` ->
`passed`, the dependency diagram edge `-> 313 i2pd Streaming convergence [ready]`
becomes `[passed]`, and the status line is updated. Plan 312's row is **not** modified:
its recorded token and its "max payload differs (i2pr 1730; i2pd 1812)" statement
were true when written and remain true as history. The current value is superseded
forward by this record, in the same shape the registry already uses for Plan 335.

**Closed.** `passed-i2pd-streaming-max-payload-profile-converged-8-of-8-matrix-cells-match`.