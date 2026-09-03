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
- Understand which doc is authoritative for a behavioral claim
- Audit doc-vs-source drift before editing
- Write or update a deep-dive consistent with the rest of the surface

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
    i2pr-<crate>.md          # Per-crate deep-dive (13 crates)
    audit/
      YYYY-MM-DD-doc-audit.md # Subagent doc-vs-source drift audit
  adr/                       # Architecture decision records (0001..0025)
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

1. **Closure records**: `plans/NNN-status.md` (and the
   `passed-*` / `superseded-by-*` / `closed-for-progression-*`
   tokens they declare).
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
6. **Per-plan narratives** (`plans/NNN-name.md`). Historical context;
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
| `i2pr-crypto` | `docs/architecture/i2pr-crypto.md` | Ed25519 / X25519 / AES / ChaCha20-Poly1305 / HMAC / SipHash / HKDF / ECIES. Secret material is zeroized. |
| `i2pr-storage` | `docs/architecture/i2pr-storage.md` | Versioned private-identity persistence; NTCP2 static-key/IV in its own versioned record. |
| `i2pr-core` | `docs/architecture/i2pr-core.md` | Runtime-neutral service contracts, health, cancellation, resource budgets. |
| `i2pr-netdb` | `docs/architecture/i2pr-netdb.md` | RouterInfo validation, bounded local NetDB, SU3 reseed, LS2 store/lookup. |
| `i2pr-netdb-persist` | `docs/architecture/i2pr-netdb-persist.md` | Composition owner for persistent cache + SU3 reseed ingestion. |
| `i2pr-transport` | `docs/architecture/i2pr-transport.md` | Runtime-neutral link/delivery contracts. No Tokio, no I/O, no `async fn`. |
| `i2pr-transport-ntcp2` | `docs/architecture/i2pr-transport-ntcp2.md` | Runtime-neutral Noise handshake, AEAD frames, data-phase blocks. |
| `i2pr-tunnel` | `docs/architecture/i2pr-tunnel.md` | Runtime-neutral exploratory pool, ECIES-X25519 short build, data plane, Plan 117 NetDB composition. |
| `i2pr-runtime` | `docs/architecture/i2pr-runtime.md` | The only production owner of Tokio, sockets, timers, channels, cancellation. |
| `i2pr-daemon` | `docs/architecture/i2pr-daemon.md` | CLI, config, identity lifecycle, Plan 106 NetDB/bootstrap, Plan 117 dispatch. |
| `i2pr-client` | `docs/architecture/i2pr-client.md` | Local destination runtime, ECIES destination Garlic session, destination routing, Streaming core. |
| `i2pr-testkit` | `docs/architecture/i2pr-testkit.md` | Deterministic simulation; no production crate may depend on it. |
| `tools/i2pr-interop/` | `docs/architecture/tooling.md` | Non-production launcher seam; never activates `i2pr-daemon`. |

## ADR index

ADR tokens: `Accepted` is binding; `Rejected` is binding in the
opposite direction; `Superseded` is binding with a replacement ADR
number. ADRs are append-only — superseded ADRs keep their original
text and gain a supersedure marker.

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
```

## Plan-of-record index

The active plan is the highest-numbered `passed-*` plan whose status
record is not `superseded-by-*`. Currently:

- **Milestone 6 authority**: Plan 134
  (`passed-milestone6-recv-window-ack-ceiling-closure`).
- **Milestone 7 SAM 3.1 corrective authority**: Plan 150
  (`passed-m7-sam31-external-client-final-closure`), under the Plan 145
  corrective roadmap. Plan 146
  closed the private-destination sub-claim
  (`passed-m7-sam31-private-destination-reference-requalification`).
  Plan 147 closed the dedicated raw STREAM product path
  (`passed-m7-sam31-dedicated-raw-stream-driver`). Plan 149 closed the
  self-composed local STREAM product
  (`passed-m7-sam31-self-composing-local-product-corrective`); the
  canonical evidence lives at
  `crates/i2pr-daemon/tests/sam_stream_self_composed.rs`. Plan 148
  remains `blocked-audit-historical-superseded` per
  [`plans/148-status.md`](../../plans/148-status.md). Plan 150 passed the
  exact pinned i2psam and qualified i2plib.sam clients through the Plan 149
  self-composed listener for independent-client CONNECT/ACCEPT, SILENT,
  private-destination, FORWARD, NAMING, negative, and lifecycle evidence.
  The official libsam3 snapshot was built/probed but not counted because its
  public key-length API rejects i2pr's compact private destination.
- **Milestone 5**: Plans 107–117 (closed; Plan 117 is
  `closed-for-progression-with-evidence-gap`).
- **Milestone 4**: Plans 102–106 (local-foundation-complete).
- **Milestone 3 interop**: Plans 038–100 historical; current
  result `protocol-defect-localized` at `noise_authenticated`.

When opening a new plan, copy the outline from
`plans/134-m6-recv-window-ack-ceiling-closure.md` and
`plans/134-status.md`. Both files pair a narrative with an explicit
closure record; the closure record carries the status token, the
focused checks, and the test list.

## Static boundary scripts (source of truth)

These eight scripts reject the change on CI. Fix the boundary; do
not weaken the script.

| Script | Catches |
| --- | --- |
| `scripts/check-dependency-direction.sh` | Crate-layer DAG violations. |
| `scripts/check-runtime-boundaries.sh` | `unbounded_channel`, `tokio::*`/`std::net`/`std::fs` in transport, raw `JoinHandle`s, `tokio::spawn` without owner. |
| `scripts/check-fixture-manifest.sh` | I2NP fixture corpus drift. |
| `scripts/check-ntcp2-vectors.sh` | NTCP2 crypto vector corpus drift. |
| `scripts/check-ntcp2-interoperability.sh` | Forbidden artifacts in the synthetic private NTCP2 lane. |
| `scripts/check-rootless-interop-boundary.sh` | Plan 046 rootless lane (no `sudo`/`ip netns`/`nft`/`setcap`/`--privileged`/`--network host`). |
| `scripts/check-multipass-interop-boundary.sh` | Plan 048/049/050/051 Multipass lane (no global `multipass purge`). |
| `scripts/check-constrained-host-lane-boundary.sh` | Plan 077 constrained-host lane order. |
| `scripts/check-plan095-workflow.sh` | Plan 095 manual live-wire workflow artifact paths. |

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
   - Stale scripts in the boundary-script table (the eight scripts
     above; count and names must match `ls scripts/check-*.sh`)
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
template. The script tables in `overview.md` (10 boundary scripts)
and `tooling.md` (10 scripts + 13 crates + 32 deps) are the common
drift points; re-check them after any new script lands.

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

- [`AGENTS.md`](../../AGENTS.md)
- [`README.md`](../../README.md)
- [`GUARDRAILS.md`](../../GUARDRAILS.md)
- [`docs/architecture/overview.md`](overview.md)
- [`docs/architecture/dependency-graph.md`](dependency-graph.md)
- [`docs/architecture/tooling.md`](tooling.md)
- [`docs/architecture/audit/`](audit/) (drift audits)
- [`specs/support.toml`](../../specs/support.toml)
- [`.opencode/skills/i2pr-local-dev/`](../i2pr-local-dev/) (the local product skill)
- [`.opencode/skills/i2pr-ntcp2-interop/`](../i2pr-ntcp2-interop/) (the NTCP2 interop skill)
- [`.opencode/skills/i2pr-rootless-sandbox/`](../i2pr-rootless-sandbox/)
- [`.opencode/skills/i2pr-multipass-recovery/`](../i2pr-multipass-recovery/)
