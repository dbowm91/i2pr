# Plan 055 status: reference-initiated NTCP2 trigger and topology qualification

Plan 055 is the qualification pass for the two Plan 052 reference-
initiated directions (`java-to-i2pr-ipv4` and `i2pd-to-i2pr-ipv4`).
It introduces the locked machine-readable trigger record schema,
the source-locked helper call graphs, the optional Java support-
topology ADR, and the trigger/direction record binding. This status
record summarises the local state on `main` after Plan 054; the
helper implementations and the external qualification runs remain
unstarted on the canonical Ubuntu lane (see "External qualification
status" below).

## Trigger record schema (Plan 055 Workstream A)

- The locked schema module is
  `tests/integration/ntcp2/harness/trigger_record.py`. It defines
  `TRIGGER_SCHEMA = "i2pr-reference-trigger-v3"`, schema version 3,
  the bounded `TriggerHelperKind` enumeration
  (`i2pd-direct-helper`, `java-direct-helper`,
  `java-minimal-support-topology`), the bounded `TriggerOutcome`
  enumeration (`not-required-i2pr-initiator`, `requested`,
  `connected`, `authenticated`, `rejected-target-router-info`,
  `rejected-target-endpoint`,
  `direct-trigger-not-source-locked`,
  `direct-trigger-api-unavailable`,
  `direct-trigger-callback-timeout`,
  `direct-trigger-helper-failed`,
  `support-topology-not-approved`,
  `support-topology-not-ready`, `cleanup-failed`), and the strict
  validator that rejects malformed target hashes, wrong endpoint
  types, zero helper digests, attempt counts other than the
  declared one-shot contract, unknown helper kinds, and run-identity
  mismatches.
- The schema also carries the Plan 055 A3 helper build provenance
  fields: `helper_compiler`, `helper_pinned_inputs_sha256`,
  `source_inspection_record_sha256`, and a mandatory
  `run_identity_sha256` for Plan 055 E2 binding.
- `build_trigger_record()` constructs a finalized record through
  `finalize_trigger_record()`, which computes the canonical
  `trigger_sha256` digest and re-validates the record under
  `finalized=True`.

## Source-inspected call graphs (Plan 055 Workstream B/C)

The pinned i2pd and Java sources were inspected against the exact
pinned revisions and recorded in
`tests/integration/ntcp2/reference-trigger-contracts.md`:

- **i2pd 2.60.0 (`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`).**
  Plan 055 B5 decision: `i2pd-direct-helper-selected`. The pinned
  source exposes
  `i2pd::transports::Transports::ConnectToPeer` in
  `libi2pd/Transports.cpp` (line 574, declared `private:` in
  `libi2pd/Transports.h:210`) and reaches the helper through the
  public `Transports::SendMessage` → `SendMessages` →
  `PostMessages` path. The call graph depends on a populated
  `i2pd::data::netdb` (RouterInfo inserted via
  `i2pd::data::netdb::AddRouterInfo`) and a started transports
  subsystem, but does not require streaming, tunnel, or floodfill
  infrastructure. Plan 055 B3 selects the embedded test process
  model (helper initializes the transport context inside its own
  process).
- **Java I2P 2.12.0 (`2800040deee9bb376567b671ef2e9c34cf3e30b6`).**
  Plan 055 C5 decision:
  `java-direct-helper-rejected-global-context-not-isolatable`.
  The pinned source exposes
  `NTCPTransport.outboundMessageReady` in
  `router/java/src/net/i2p/router/transport/ntcp/NTCPTransport.java`
  (line 373) but the call graph requires a fully-initialized
  `RouterContext` (constructed through
  `new NTCPTransport(RouterContext, X25519KeyFactory)` at line 153),
  a populated NetDB, and the `_conByIdent.put(ih, con)` precondition
  at line 395. Plan 055 C5 explicitly forbids patching the global
  context, and "could not get it working" is rejected; the
  rejection cites the source symbols above.

## Support-topology ADR (Plan 055 Workstream D)

- ADR 0021
  (`docs/adr/0021-minimal-java-support-topology.md`) is now
  Proposed. It authorizes the optional
  `java-minimal-support-topology` fallback when the Java direct
  helper is impossible. The ADR enumerates the minimum topology
  roles, the no-public-egress boundary, the support-router
  inventory rule, the cleanup contract, and the Plan 055 D5
  control experiments.
- The ADR is gated by Plan 055 D1 — no topology-assisted helper may
  be implemented until the ADR is approved by the repository
  maintainers.

## Pipeline integration (Plan 055 Workstream E)

- The Plan 052/053 pipeline
  (`tests/integration/ntcp2/harness/plan052_pipeline.py`) now
  accepts a Plan 055 `trigger_record` parameter. When supplied, the
  record is validated against `i2pr-reference-trigger-v3`,
  finalized against the bound `run_identity_sha256`, and serialized
  into the bundle trigger slot.
- `finalize_diagnostic_bundle()` cross-checks the on-disk trigger
  digest against the direction record's `trigger_sha256`. A mismatch
  raises `trigger-direction-binding-mismatch:<direction>`.
- `evidence_bundle.SEMANTIC_SCHEMAS` allowlists the new schema as
  the primary trigger schema; legacy `v1`/`v2` trigger records
  remain readable but are never emitted by the Plan 055 pipeline.
- The trigger record preserves the Plan 052 G1 bounded i2pr
  responder reasons. A successful trigger outcome
  (`authenticated`) paired with a rejected direction (e.g.
  `responder-session-confirmed-part2-failed`) is preserved as
  separate records; the trigger does not mask the direction.

## Validation

- `python3 -m unittest discover -s tests/integration/ntcp2/harness
  -p 'test_*.py'` — 372 tests pass (was 346 before Plan 055; +26
  new tests in `test_plan055.py`).
- `python3 -m unittest discover -s tests/integration/ntcp2/harness
  -p 'test_plan055.py'` — 26 trigger schema, builder, direction
  binding, and end-to-end qualification tests pass.
- `bash scripts/check-ntcp2-interoperability.sh` — passes with the
  Plan 055 trigger schema, source-inspection contract, ADR 0021,
  and AGENTS.md wiring checks added.
- `bash scripts/check-dependency-direction.sh`,
  `bash scripts/check-runtime-boundaries.sh`,
  `bash scripts/check-rootless-interop-boundary.sh`,
  `bash scripts/check-multipass-interop-boundary.sh`,
  `bash scripts/check-fixture-manifest.sh`,
  `bash scripts/check-ntcp2-vectors.sh` — pass.
- `cargo fmt --all --check`, `cargo check --workspace --all-targets`,
  `cargo test --workspace`, `cargo clippy --workspace --all-targets
  --all-features -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` —
  pass.

## External qualification status

- The i2pd direct helper source files and the optional Java
  support topology do not yet exist on this host. The Plan 048/049
  Multipass recovery lane is the canonical external path on which
  the helper implementations and the four-direction qualification
  runs must be executed.
- A complete Plan 052 diagnostic bundle still classifies as
  `diagnostic-complete-not-certificate`; both reference-initiated
  directions remain typed blockers because no helper has been
  exercised and no live Plan 052 observation predicate can pass.
- Milestone 3 stays open. Plan 055 does not advance
  `specs/support.toml` or claim NTCP2 advertisement.

## Required handoff artifacts

- `tests/integration/ntcp2/harness/trigger_record.py` — present
  and locked to schema `i2pr-reference-trigger-v3`.
- `tests/integration/ntcp2/reference-trigger-contracts.md` —
  present, source-inspected against the pinned revisions, decision
  records committed.
- `docs/adr/0021-minimal-java-support-topology.md` — present,
  Proposed, awaiting approval before any support topology may be
  built.
- `tests/integration/ntcp2/harness/test_plan055.py` — present,
  schema + builder + binding + end-to-end tests all pass.
- Updated `plan052_pipeline.write_direction_artifacts()` —
  accepts and validates the Plan 055 trigger record; binds the
  trigger digest into the direction record; preserves i2pr
  responder reasons.
- Updated `scripts/check-ntcp2-interoperability.sh` — Plan 055
  trigger schema, contracts, and ADR checks pass.
- This status document.

## Open blockers

- Reference artifacts are not installed on this host, so the i2pd
  direct helper, the Java support topology, and the live Plan 052
  bundle runs remain unstarted. The Plan 048/049 Multipass
  recovery lane is the canonical external path.
- Two complete reproducible Plan 055 bundles are required to close
  Milestone 3. Plan 056 owns those runs.
- NTCP2 remains experimental and non-advertised.

## Acceptance summary

Plan 055 implements every Workstream that can be done without the
external helper builds. The trigger schema, source-inspected call
graphs, support-topology ADR, pipeline integration, and qualification
tests are committed. The remaining two acceptance criteria — the
i2pd helper implementation and the two live Plan 052 qualification
runs — require the external lanes and are deferred to Plan 056.

## Plan 056 follow-on

Plan 056 is closed with a typed host-environment blocker. The
verifier and the candidate freeze are committed; the local
diagnostic bundles were generated under the ignored
`target/interop/evidence/plan056/` working directory and are
intentionally untracked. The only repository-tracked footprint is
the bounded local-diagnostic receipt at
`tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`
with `artifact_storage = local-untracked`.

Plan 058 retired the Plan 056 candidate, superseded Plan 057, and
decided ADR 0021 (Rejected). The follow-up split is now:

- `plans/058-plan056-record-and-candidate-integrity-closure-pass.md`
  — retired the Plan 056 candidate, superseded Plan 057, added
  the candidate record integrity validator and the two-lane
  contract, and recorded the closure.
- `plans/059-reference-side-implementation-and-live-qualification-closure-pass.md`
  — implements the i2pd direct helper and qualifies receiver
  observations. Plan 058 rejected ADR 0021, so the
  `java-to-i2pr-ipv4` direction remains a typed blocker for the
  pinned Java I2P 2.12.0 revision and Plan 059 must close with
  the typed blocker `blocked_java_support_topology_rejected`.
- `plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md`
  — cuts a fresh implementation-floor candidate after Plan 059
  closes and produces the two-run certificate. Until Plan 060
  produces two passing bundles from a fresh candidate, NTCP2
  stays experimental and non-advertised and Milestone 3 stays
  open.

Plan 055 does not advance `specs/support.toml` and does not claim
NTCP2 advertisement.