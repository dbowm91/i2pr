---
name: i2pr-architecture
description: Navigate the i2pr Rust I2P router architecture documentation, ADRs, plans, specs, and source-tree ownership. Use when an agent is asked to find the canonical deep-dive for a crate, locate the ADR for a specific decision, understand why a boundary is enforced, follow a plan-of-record, or audit doc-vs-source drift. Also use when asked to update an existing architecture deep-dive or write a new one consistent with the rest of `docs/architecture/`.
---

# I2PR Architecture

The repository's on-disk documentation surface and how to keep it in
sync with the source tree. Architecture deep-dives live under
`docs/architecture/`; the most recent audit of doc-vs-source drift
lives under `docs/architecture/audit/`.

Load this skill whenever an agent needs to:

- Find the deep-dive for a specific crate
- Find the ADR that records a specific decision
- Locate a plan-of-record for a specific milestone
- Register or close out an implementation plan (see `i2pr-planning` for the
  register/implement/close/unblock lifecycle, `registry.md` mechanics, and
  closure-evidence rules)
- Understand which doc is authoritative for a behavioral claim
- Audit doc-vs-source drift before editing
- Write or update a deep-dive consistent with the rest of the surface

For the current M6 Java second-family boundary, use
`plans/closure/mixed-router-interop/236-status.md` and
`docs/architecture/interop-apparatus.md`: Plan 236 source-locks the pinned
Java response path and closes at an observability gap, without claiming
Router-A or i2pr delivery.

## Documentation surface

```text
AGENTS.md                    # Repository guidelines (read first)
README.md                    # Status, build/test/lint, workspace layout
GUARDRAILS.md                # Non-negotiable engineering + security constraints
CONTRIBUTING.md              # Local quality checks, conventions

docs/
  architecture.md            # Top-level architecture narrative (modular monolith, four planes)
  architecture/
    overview.md              # Bird's-eye view, crate graph, crate index, data-flow narrative
    dependency-graph.md      # Per-crate allowlist + ASCII graph (script: check-dependency-direction.sh)
    tooling.md               # Scripts, fixtures, integration lanes, CI, fuzz
    interop-apparatus.md     # NTCP2 interop apparatus (Plan 038–100; historical surface)
    i2pr-<crate>.md          # Per-crate deep-dive (19 crates: 18 production + i2pr-testkit)
    audit/
      YYYY-MM-DD-doc-audit.md # Subagent doc-vs-source drift audit
  adr/                       # Architecture decision records (0000..0031)
  security-model.md          # Memory hygiene, secret-bearing types, codec error policy
  private-testnet.md         # Private testnet operation guidance
  protocol-support.md        # Generated from specs/support.toml

plans/                       # Plan-of-record + closure records (NNN-name.md, NNN-status.md)
specs/
  CONFORMANCE.md             # What counts as evidence
  IMPLEMENTATIONS.md         # Which router each spec claim is borrowed from
  SOURCES.md                 # Pin-locked upstream references
  support.toml               # Machine-readable workspace support inventory
  references/                # Per-protocol provenance notes (ecies-destination-ratchet.md,
                             # streaming-packet-wire.md, elligator2-production-representation.md,
                             # short-build-inbound-creator-key.md, streaming-client-payload-gzip.md)
  protocols/                 # Per-protocol dossiers
```

## Authority hierarchy

When two documents disagree, the closure record and the executable
test win. From highest to lowest authority:

1. **Closure records**: `plans/closure/<subsystem>/NNN-status.md` (and the
   `passed-*` / `superseded-by-*` / `closed-for-progression-*`
   tokens they declare). Registry index: `plans/registry.md`; subsystem
   roadmaps: `plans/subsystems/<subsystem>-roadmap.md`.
2. **Executable tests** (`cargo test -p <crate>`, `python3 -m
   unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'`,
   `bash scripts/check-*.sh`). A passing test is a passing contract.
3. **Static boundary scripts** under `scripts/check-*.sh`. They are
   the source of truth for non-negotiable invariants. Do not weaken
   the script; fix the boundary.
4. **ADR records** (`docs/adr/NNNN-name.md`). The decision token
   (`Accepted` / `Rejected` / `Superseded`) is binding.
5. **Per-crate deep-dives** (`docs/architecture/i2pr-<crate>.md`).
   Authoritative for the current state; may drift in details.
6. **Per-plan narratives** (`plans/implementation/<subsystem>/NNN-name.md`). Historical context;
   not a live contract.
7. **`AGENTS.md`** carries the workspace-wide conventions; it does
   not override a closure record.
8. **README.md** mirrors the closure record for top-level claims; do
   not let it disagree.

When auditing doc-vs-source drift, the most useful single command is:

```text
cargo metadata --format-version 1 --no-deps
rg -n 'pub use' crates/i2pr-<crate>/src/lib.rs
ls crates/i2pr-<crate>/src/
```

These three together reveal the public surface, the public re-exports,
and the actual module layout — the three facts a deep-dive must match.

## Per-crate deep-dive index

Each deep-dive follows the same outline: Purpose, Module layout,
Public surface, Key contracts, Errors, Dependencies, Tests,
Distinctive design choices, Cross-references. Match this outline when
writing or updating a deep-dive.

| Crate | Deep-dive | One-liner |
| --- | --- | --- |
| `i2pr-proto` | `docs/architecture/i2pr-proto.md` | Bounded wire codecs; Standard LeaseSet2; Streaming. No runtime, no I/O. |
| `i2pr-crypto` | `docs/architecture/i2pr-crypto.md` | Ed25519 / X25519 / AES / ChaCha20-Poly1305 / HMAC / SipHash / HKDF / ECIES / Red25519. Secret material is zeroized. |
| `i2pr-su3` | `docs/architecture/i2pr-su3.md` | Bounded SU3 envelope framing and explicit-key RSA-SHA512 verification. Zero workspace deps. |
| `i2pr-storage` | `docs/architecture/i2pr-storage.md` | Versioned private-identity persistence; NTCP2 static-key/IV in its own versioned record. |
| `i2pr-core` | `docs/architecture/i2pr-core.md` | Runtime-neutral service contracts, health, cancellation, resource budgets. |
| `i2pr-netdb` | `docs/architecture/i2pr-netdb.md` | RouterInfo validation, bounded local NetDB, SU3 reseed, LS2/ELS2 store/lookup. |
| `i2pr-netdb-persist` | `docs/architecture/i2pr-netdb-persist.md` | Composition owner for persistent cache + SU3 reseed ingestion + floodfill records. |
| `i2pr-transport` | `docs/architecture/i2pr-transport.md` | Runtime-neutral link/delivery contracts and network-status model. No Tokio, no I/O, no `async fn`. |
| `i2pr-transport-ntcp2` | `docs/architecture/i2pr-transport-ntcp2.md` | Runtime-neutral Noise handshake, AEAD frames, data-phase blocks. |
| `i2pr-transport-ssu2` | `docs/architecture/i2pr-transport-ssu2.md` | Runtime-neutral SSU2 v2 address/header/block foundation (Plan 155) plus Noise XK establishment, tokens, RouterInfo binding (Plan 156) plus authenticated data-phase reliability/fragmentation (Plan 157). No sockets. |
| `i2pr-tunnel` | `docs/architecture/i2pr-tunnel.md` | Runtime-neutral exploratory **and transit** pool, ECIES-X25519 short build, data plane, Plan 117 NetDB composition. |
| `i2pr-runtime` | `docs/architecture/i2pr-runtime.md` | The only production owner of Tokio, sockets, timers, channels, cancellation. |
| `i2pr-daemon` | `docs/architecture/i2pr-daemon.md` | CLI, config, identity lifecycle, NetDB/bootstrap, dispatch, I2PControl + outproxy route owner, floodfill, service-tunnel executors. |
| `i2pr-client` | `docs/architecture/i2pr-client.md` | Local destination runtime (router-owned and client-owned modes), ECIES destination Garlic session, destination routing, Streaming core, LeaseSet2 lifecycle, typed `LeaseRequest`. |
| `i2pr-api` | `docs/architecture/i2pr-api.md` | Runtime-neutral application adapters: SAM 3.1 parsing/session/registry/FORWARD/NAMING plus the M9 I2CP wire/profile/connection/options/data-plane foundation, session registry, typed `I2cpAction`. No sockets, no Tokio. |
| `i2pr-addressbook` | `docs/architecture/i2pr-addressbook.md` | Canonical `.i2p` books, precedence resolver, subscriptions, versioned generations. No I/O. |
| `i2pr-i2pcontrol` | `docs/architecture/i2pr-i2pcontrol.md` | Proposal 170 JSON-RPC 2.0 contract: envelope, auth vocabulary, method/type/selector inventories, tunnel option metadata, wire ceilings. No I/O. |
| `i2pr-service-tunnels` | `docs/architecture/i2pr-service-tunnels.md` | Runtime-neutral M10 service-tunnel config/policy (12 kinds) plus the outproxy provider **policy** and the outbound-secret capability trait. No sockets, no Tokio; the daemon owns all listeners. |
| `i2pr-testkit` | `docs/architecture/i2pr-testkit.md` | Deterministic simulation; no production crate may depend on it. |
| `tools/i2pr-interop/` | `docs/architecture/tooling.md` | Non-production launcher seam; never activates `i2pr-daemon`. |

The table must list **every** workspace member exactly once. Verify with
`ls crates/ | wc -l` (currently 19) after any crate lands.

## ADR index

ADR tokens: `Accepted` is binding; `Rejected` is binding in the
opposite direction; `Superseded` is binding with a replacement ADR
number. ADRs are append-only — superseded ADRs keep their original
text and gain a supersedure marker.

**Known defect (2026-10-05): `docs/adr/` contains two `0030-*` records,
both `Accepted`.** `0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md`
and `0030-loopback-controlled-floodfill-reachability-advertisement.md`.
This makes ADR 0029's "partially superseded by ADR 0030" ambiguous, and
ADR numbers have no uniqueness checker —
`scripts/check-global-plan-number-uniqueness.py` scans `plans/` only.
Cite ADR 0030 by full filename, never by number alone, until a
plan-of-record renumbers one record. Tracked in
[`audit/2026-10-05-doc-audit.md`](../../../docs/architecture/audit/2026-10-05-doc-audit.md).

```text
0000  adr-process.md                                      Process
0001  modular-monolith.md                                 One crate per subsystem
0002  tokio-runtime-boundary.md                           Only i2pr-runtime owns Tokio
0003  bounded-supervised-services.md                      Service graph + restart policy
0004  router-identity-algorithms.md                       Ed25519 + X25519 crypto profile
0005  crypto-dependency-selection.md                      Reviewed third-party crates
0006  private-identity-storage.md                         Atomic create-only router.identity
0007  explicit-identity-first-run.md                      identity generate is the only writer
0008  runtime-supervision-and-cancellation.md             Hierarchical wakeable cancellation
0009  runtime-observability-and-validation.md             Privacy-safe snapshots
0010  transport-contracts-and-crate-boundaries.md        Synchronous transport contracts
0011  ntcp2-crypto-and-static-key-storage.md              Static key in its own record
0012  ntcp2-handshake-state-machines.md                   Strict, bounded, runtime-neutral
0013  ntcp2-data-phase-and-blocks.md                      Data-phase sync, no implicit clone
0014  ntcp2-runtime-link-manager-and-address-policy.md    Listener/dial, no public-network
0015  ubuntu-reference-router-harness.md                  Plan 038 harness host
0016  ubuntu-build-system-interop-gates.md                Plan 043 build gates
0017  rootless-sealed-namespace-interop-evidence.md       Plan 046 rootless
0018  multipass-rootless-interop-environment.md           Plan 048 recovery lane
0019  guest-level-nft-marker-clarification.md            nft markers inside the guest only
0020  plan053-evidence-pipeline-integrity.md              Plan 053 diagnostic lane
0021  minimal-java-support-topology.md                    Rejected by Plan 058
0022  direct-reference-router-ntcp2-interop-drivers.md    Accepted (Plan 062)
0023  staged-ntcp2-interoperability-evidence.md           Four-tier evidence ladder
0024  constrained-host-ntcp2-execution-lanes.md           Plan 077 lane order
0025  plan090-i2pd-driver-routerinfo-correction.md        Plan 090 corrections
0026  staged-interoperability-progression-and-java-debt.md  Separates progression from 2-family conformance
0027  floodfill-role-provenance-and-advertisement.md      Floodfill role provenance
0028  i2pcontrol-proposal-170-control-plane.md            Proposal 170 control plane
0029  anonymity-boundaries-and-profile-convergence.md     Accepted; partially superseded by ADR 0030 (ambiguous — see above)
0030  destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md   COLLIDING NUMBER
0030  loopback-controlled-floodfill-reachability-advertisement.md              COLLIDING NUMBER
0031  one-shared-service-tunnel-manager.md                One shared ServiceTunnelManager
```

## Plan-of-record index

**Do not maintain a plan ledger in this skill.** Plans 216-344 (the M11
transit lane, M12 floodfill, the Proposal 170 / I2PControl + Red25519/ELS2
continuation, the anonymity lane, and the outproxy work) landed after this
section was written, and the chronological narrative it used to carry went
stale silently. `plans/registry.md` is the live index and is maintained with
the code; a copy here can only rot.

Navigate in this order:

1. [`plans/registry.md`](../../../plans/registry.md) — active roadmaps,
   current milestone authorities, ready plans, blockers, recent closures.
2. `plans/subsystems/<subsystem>-roadmap.md` — the coherent workstream and
   its milestone table.
3. `plans/closure/<subsystem>/NNN-status.md` — **authoritative**. The
   status token declared there wins over every other surface, including
   this skill, `registry.md` prose, and `specs/support.toml`.
4. `plans/archive/legacy-flat-registry-2026-09-18.md` — pre-migration
   history for Plans 000-215.

There is no single "active plan": authority is milestone-keyed with parallel
lanes. `registry.md` currently records `active_plan = plan284` (M12
floodfill), with Plan 342 `ready` on the Proposal 170 outproxy option
surface, `next_executable_plan = 249-m11-transit-admission-and-short-build-participant-foundation`
in the Plan 204 amendment, and the anonymity lane running parallel
(ADR 0030, does not gate M12/mainline). Read the registry for the current
value; do not copy a token from here.

Closed-era milestones, for orientation only (all superseded by the registry):

- **Milestone 6 local**: Plan 134 authority; Plan 152 corrective retained.
- **Milestone 7 SAM 3.1**: Plan 151 final acceptance.
- **Milestone 8 SSU2**: Plan 161 + 162 closed, bounded scope.
- **Milestone 9 I2CP**: Plan 172 final acceptance.
- **Milestone 10 service tunnels**: Plan 215 product authority; Plan 204 is
  `retained-convergence-record-superseded-by-plan248-policy-reconciliation`,
  **not** a live blocker.
- **Milestone 5**: Plans 107-117 (Plan 117 `closed-for-progression-with-evidence-gap`).
- **Milestone 4**: Plans 102-106 (local-foundation-complete).
- **Milestone 3 interop**: Plans 038-100 historical; result
  `protocol-defect-localized` at `noise_authenticated`.

Status tokens most often quoted wrongly, with the current authority:

- Plan 194 is **`passed-m6-java-second-family-mixed-router-closure-with-sam-ls2-gap`**
  (`plans/closure/mixed-router-interop/194-status.md`). It is not "retained-partial"
  and it did not stop at a first-run topology blocker; Plan 196 closed that.
- Plan 201 is **`retained-deferred-nonblocking-java-router-compatibility-debt-via-plan248`**.
  M6 Java is nonblocking retained debt per ADR 0026, not a gate.
- Plan 204 is `retained-convergence-record-superseded-by-plan248-policy-reconciliation`.
  Earlier tokens in that append-only file are superseded history.
- Plan 195 is **`blocked-m10-remote-independent-service-pending-plan213-and-plan214`**.
- M10 final acceptance is `closed-via-plan215`.

When opening a new plan, copy the outline from
`plans/closure/destination-streaming/134-m6-recv-window-ack-ceiling-closure.md` and
`plans/closure/destination-streaming/134-status.md`. Both files pair a narrative with an explicit
closure record; the closure record carries the status token, the
focused checks, and the test list. See the `i2pr-planning` skill for the
registry/roadmap/closure mechanics.
## Static boundary scripts (source of truth)

Fix the boundary; never weaken the script. But note that **"in this table" is
not the same as "enforced on CI"** — the real disposition of every checker is in
[`tooling.md`](../../../docs/architecture/tooling.md) and the coverage holes are
listed under "Known enforcement gaps" below. Verify a script exists with
`ls scripts/`; this table has carried rows for deleted files before.

| Script | Catches |
| --- | --- |
| `scripts/check-dependency-direction.sh` | Crate-layer DAG violations. **See gaps: does not police `i2pr-tunnel` or `tools/i2pr-interop`.** |
| `scripts/check-runtime-boundaries.sh` | `unbounded_channel`, `tokio::*`/`std::net`/`std::fs` in transport, raw `JoinHandle`s, `tokio::spawn` without owner. **See gaps: no `i2pr-api` section.** |
| `scripts/check-global-plan-number-uniqueness.py` | Cross-subsystem implementation-plan number ownership. **Run with `python3`, not `bash`** (CI-enforced). |
| `scripts/check-java-source-lock-gating.sh` | Java source-lock tests stay `#[ignore]`-gated. **CI-enforced, not in the AGENTS.md floor.** |
| `scripts/check-fixture-manifest.sh` | I2NP fixture corpus drift. |
| `scripts/check-ntcp2-vectors.sh` | NTCP2 crypto vector corpus drift. |
| `scripts/check-ssu2-vectors.sh` | SSU2 v2 fixture corpus drift (Plans 155–157; CI-enforced). |
| `scripts/check-i2cp-vectors.sh` | Plan 164 I2CP fixture corpus drift (CI-enforced). |
| `scripts/check-ntcp2-interoperability.sh` | Forbidden artifacts in the synthetic private NTCP2 lane. |
| `scripts/check-constrained-host-lane-boundary.sh` | Plan 077 constrained-host lane order. |
| `scripts/check-rootless-interop-boundary.sh` | Plan 046 rootless lane (no `sudo`/`ip netns`/`nft`/`setcap`/`--privileged`/`--network host`). Historical lane; green. |
| `scripts/check-multipass-interop-boundary.sh` | Plan 048/049/050/051 Multipass lane (no global `multipass purge`). Historical lane; green. |
| `scripts/check-sam-acceptance-evidence.sh` | Plan 151 SAM evidence integrity (no synthetic `passed` rows; CI-enforced). |
| `scripts/check-ssu2-acceptance-evidence.sh` | Plan 161 SSU2 evidence integrity (no synthetic `passed` rows; CI-enforced). |
| `scripts/check-i2cp-acceptance-evidence.sh` | Plan 170/172 I2CP evidence integrity (no synthetic `passed` rows; CI-enforced). |
| `scripts/check-i2pcontrol-acceptance-evidence.sh` | Proposal 170 I2PControl evidence integrity (CI-enforced). |
| `scripts/check-service-tunnel-boundaries.sh` | Plan 180 M10 runtime-neutral invariants (no Tokio/sockets in service-tunnels, no Garlic/I2NP, single pump, no unbounded channels, one entry point) **plus Plan 343 rules 9–11: the outproxy policy and route owner may not name a clearnet socket/resolver/TLS client, load a plugin, or spawn a process, with a positive control on `service_tunnels_http.rs`.** |
| `scripts/check-m11-transit-boundaries.sh` | M11 transit runtime-neutral invariants (CI-enforced). |
| `scripts/check-m11-transit-qualification-evidence.sh` | M11 one-family qualification evidence integrity. **Floor, not CI.** |
| `scripts/check-m11-per-epoch-composition.sh` | M11 per-epoch tunnel composition. **Now in the AGENTS.md floor.** |
| `scripts/check-service-anonymity-boundaries.sh` | ADR 0029/0030 anonymity boundary invariants. **Now in the AGENTS.md floor.** |
| `scripts/check-service-tunnel-acceptance-evidence.sh` | Plan 181/213/214/215 service-tunnel evidence integrity: 29 command-derived local rows plus the Plan 213 generic + Plan 214 application remote qualification (§27/§28/§29 invariants); no literal passes (CI-enforced). |
| `scripts/check-exploratory-tunnel-evidence.sh` | Plan 185 exploratory-tunnel evidence integrity: guarded local build/liveness rows flow through exit-code-gated helpers (CI-enforced). |
| `scripts/check-netdb-tunnel-evidence.sh` | Plan 186 NetDB-over-tunnel evidence integrity: guarded lookup/publication rows flow through exit-code-gated helpers (CI-enforced). |
| `scripts/check-destination-tunnel-evidence.sh` | Plan 187/192 destination-tunnel evidence integrity: 21 guarded labels across local and mixed-router harnesses (CI-enforced). |
| `scripts/check-streaming-tunnel-evidence.sh` | Plan 193 Streaming-tunnel evidence integrity: 33 guarded labels across i2pd + Java harnesses (CI-enforced). |
| `scripts/check-m6-mixed-router-acceptance-evidence.sh` | Plan 189 §8 / Plan 194 / Plan 196 / Plan 197 cross-family M6 mixed-router evidence integrity; Plan 196 rejects `i2p.vmCommSystem=true`, obsolete Plan 194 keys, mutation of the verified Java cache, non-loopback reseed URLs, and `|| true`; Plan 197 adds the `pq` parser-tolerance invariants; Plans 201/246/247 add the Branch-G counters and the observation-window/parser invariants (CI-enforced). |
| `scripts/check-m6-final-closure-evidence.sh` | Plan 198 evidence-consuming gate: exact-head provenance, exact family pins, and no blocked/failed/missing mandatory rows. **Manual external-workflow only** — cannot go green without the exact-pinned Java 2.13.0 cache. Do not add to the floor. |
| `scripts/check-m12-floodfill-qualification-evidence.sh` | Plan 279 §9 M12 floodfill qualification evidence integrity: guarded i2pd (10 rows, budget 1) and Java (12 rows, budget 3) matrix rows, exact reference pins, loopback bind, frozen per-lane attempt budgets, no `|| true`; `--self-test` proves the gates (CI-enforced). |
| `scripts/check-m12-floodfill-boundaries.sh` | **Currently exits 1 — broken.** Still enforces the Plan 281 "type 5 deferred" floor that Plans 332/333/334 legitimately superseded. In neither the floor nor CI, so the failure is silent. Do not add to the floor until a plan corrects the script. |
| `scripts/check-streaming-fingerprint-evidence.sh` | Takes a plan argument; not a zero-arg floor candidate. |
| `scripts/check-http-anonymity-evidence.sh` | Requires a Plan 308 evidence manifest that does not exist (Plan 308 blocked). Not a floor candidate. |

**Pruned, not live:** `scripts/check-plan095-workflow.sh` and
`tests/integration/ntcp2/harness/test_plan09{5,6,7,8}.py` were removed by the
Plan 099 harness reduction (`c04da77a`). Do not link them as live commands.

### Known enforcement gaps (2026-10-05)

A green floor is not full coverage. These are real holes, recorded rather than
silently patched, because closing them means changing a script or the DAG
allowlist and that needs a plan-of-record:

- `check-dependency-direction.sh` has 18 `expected`-map keys for 20 workspace
  members. `i2pr-tunnel` and `tools/i2pr-interop` are absent, so the loop never
  inspects them and a new forbidden `i2pr-*` production edge in either would
  pass CI silently. `tools/i2pr-interop` is also outside
  `check-runtime-boundaries.sh`'s `crates/*/Cargo.toml` glob.
- `check-runtime-boundaries.sh` has no `i2pr-api` section, so its "passed"
  result is not evidence for that crate. It also greps `std::net` literally, so
  a grouped `use std::{…}` import evades it.
- ADR numbers have no uniqueness checker and `docs/adr/` has a live `0030`
  collision (see "ADR index" above).

Cross-reference: `AGENTS.md` → "Known checker gaps".

## Doc-vs-source audit pattern

When asked to audit doc-vs-source drift:

1. Read the doc end-to-end (load_file the whole file).
2. Read every source file in the target crate in parallel.
3. Read `crates/<crate>/Cargo.toml` for the dependency allowlist.
4. Compare the doc's claims to source reality, line by line. Note:
   - Wrong numeric constants (`MAX_*`, fixed sizes)
   - Wrong module / variant / function counts
   - Wrong crate dependency names (e.g. `rsa` → `sad-rsa`,
     `curve25519-elligator2` → `elligator2`)
   - Missing public types (cross-check `pub use` in `lib.rs`)
   - Missing modules (cross-check `mod foo;` declarations in `lib.rs`)
   - Missing crate-level deep-dives in `overview.md` (the crate
     index table must link every workspace member)
   - Stale scripts in the boundary-script table (the scripts
     above; names must match `ls scripts/check-*.sh`)
5. Return a structured report:
   STALE / MISSING / INCORRECT BOUNDS / WRONG LINKS / GOOD, each
   citing doc-line vs source-file-line.
6. Apply targeted patches in a single commit, scoped to the
   highest-impact fixes (wrong constants, missing deep-dives,
   wrong script counts, missing crate edges).
7. Record remaining gaps in a `docs/architecture/audit/YYYY-MM-DD-doc-audit.md`
   so future audits can pick them up.

The 2026-08-27 audit
(`docs/architecture/audit/2026-08-27-doc-audit.md`) is the canonical
template. The script tables in `overview.md` and `tooling.md` are the
common drift points; re-check row counts against
`ls scripts/check-*.sh` after any new script lands (a new checker
must appear in both tables, in routine CI, and in the floors).

## Writing or updating a deep-dive

Match the existing outline:

1. **Crate header**: name, path, one-line purpose.
2. **Purpose**: what the crate owns and what it must not own.
3. **Module layout**: a table with module, file, line count,
   responsibility, key public types. Recompute line counts with
   `wc -l crates/<crate>/src/<file>.rs` when editing.
4. **Public surface**: the actual `pub use` re-exports from `lib.rs`.
5. **Key contracts**: typed error enums, bound constants,
   ownership rules.
6. **Dependencies**: from `Cargo.toml` plus the script-level
   `check-dependency-direction.sh` allowlist.
7. **Tests**: which test files live where; deterministic seeds;
   bounded negative paths.
8. **Distinctive design choices**: 5–10 items, each one sentence.
9. **Cross-references**: ADR numbers, plan numbers, related deep-dives.

When the source is correct but the doc is stale, do a surgical patch
and add a closure note to the next audit document. When the doc is
structurally accurate but missing detail, write a follow-up edit in
the next audit document rather than rewriting the doc whole.

## Cross-references

This skill lives at `.opencode/skills/i2pr-architecture/`, so repo-root paths
need `../../../`, and sibling skill paths need `../`.

- [`AGENTS.md`](../../../AGENTS.md)
- [`README.md`](../../../README.md)
- [`GUARDRAILS.md`](../../../GUARDRAILS.md)
- [`CONTRIBUTING.md`](../../../CONTRIBUTING.md)
- [`docs/architecture/overview.md`](../../../docs/architecture/overview.md)
- [`docs/architecture/dependency-graph.md`](../../../docs/architecture/dependency-graph.md)
- [`docs/architecture/tooling.md`](../../../docs/architecture/tooling.md)
- [`docs/architecture/interop-apparatus.md`](../../../docs/architecture/interop-apparatus.md) (closed lane)
- [`docs/architecture/audit/`](../../../docs/architecture/audit/) (drift audits)
- [`specs/support.toml`](../../../specs/support.toml)
- [`specs/CONFORMANCE.md`](../../../specs/CONFORMANCE.md)
- [`plans/registry.md`](../../../plans/registry.md)
- [`i2pr-local-dev`](../i2pr-local-dev/SKILL.md) (the local product skill)
- [`i2pr-planning`](../i2pr-planning/SKILL.md) (registry/roadmap/closure mechanics)
- [`i2pr-ntcp2-interop`](../i2pr-ntcp2-interop/SKILL.md) (historical, archaeology only)
- [`i2pr-rootless-sandbox`](../i2pr-rootless-sandbox/SKILL.md) (historical, archaeology only)
- [`i2pr-multipass-recovery`](../i2pr-multipass-recovery/SKILL.md) (historical, archaeology only)
