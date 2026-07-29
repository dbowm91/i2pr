# Plan 053: Plan 052 evidence-pipeline integration corrective pass

## Status and relationship to prior work

- Plan type: narrow corrective implementation pass.
- Starting branch: `main`.
- Starting repository head at authoring: `e6d776771308aed0bd4b942da5012bafb182f5b9`.
- Depends on:
  - `plans/052-ntcp2-milestone-3-evidence-closure-follow-up.md`;
  - `plans/052-status.md`;
  - Plans 045, 046, 049, 050, and 051.
- This plan owns only the correctness and end-to-end integration of the Plan 052 evidence plumbing.
- This plan does **not** own Java I2P startup qualification, reference-router source instrumentation, direct reference trigger helpers, or the final external two-run certificate.
- Milestone 3 remains open throughout this pass.
- NTCP2 remains experimental and non-advertised throughout this pass.

## Why this plan exists

Plan 052 added useful schema modules, but the canonical execution path does not yet use them. The current rootless path still runs:

```text
rootless-enter.sh
  -> rootless_inner_runner.py
     -> mixed_runner.py
        -> legacy direction JSON
```

It does not create or bind one authoritative `run-identity.json`, does not stage the complete Plan 052 bundle, does not write observation-v2 records, and does not finalize/export the bundle atomically.

The existing `evidence_bundle.py` also has correctness defects that must be fixed before any external run is trusted:

1. `_classify()` resolves each file relative to itself, so record types become `unknown`.
2. `write_json_atomic()` hashes canonical bytes but writes differently serialized bytes.
3. `manifest.sha256` is written but not verified.
4. `export-acknowledgement.json` is written inside the finalized bundle after manifest creation, making the exported directory fail strict re-verification.
5. Unknown record types and unexpected schemas are not rejected strongly enough at finalization.

The purpose of this pass is to make one canonical execution path capable of producing a complete, verifiable Plan 052 bundle even when every direction is rejected or blocked. Protocol success is explicitly not required here.

## Fixed scope

### In scope

- Correct `evidence_bundle.py` and its tests.
- Define one authoritative run-identity creation point.
- Pass the run identity and bundle staging root through every canonical layer.
- Bind every direction record to the run identity.
- Preserve bounded responder-stage reason codes from the Rust launcher through Python evidence.
- Emit observation-v2, trigger, cleanup, and attestation records for all four primary directions.
- Finalize and verify a complete diagnostic bundle.
- Export the bundle atomically without mutating it after finalization.
- Add static checks that prevent regression to legacy unbound evidence for Plan 052 profiles.

### Out of scope

- Claiming any direction passed.
- Running the Java startup matrix.
- Discovering Java or i2pd receiver-side source markers.
- Building direct Java/i2pd trigger helpers.
- Adding floodfill or support routers.
- Advertising NTCP2.
- Changing production daemon behavior unrelated to preserving launcher status reasons.

## Non-negotiable invariants

1. Exactly one source commit is authoritative for a run.
2. The run identity is created once and is immutable after the first direction starts.
3. Passed Plan 052 records may never contain zero-filled provenance.
4. A blocked or rejected direction still produces a complete typed record set.
5. Raw logs never enter a finalized or exported bundle.
6. The exported bundle must pass the same verifier used on staging.
7. No file may be added inside a bundle after `manifest.json` is finalized.
8. Every path in the manifest is bundle-relative, normalized, non-absolute, and contains no `..` component.
9. Symlinks, device nodes, sockets, and FIFOs are forbidden in staging and export trees.
10. A generic historical reason such as `i2pr-responder-handshake-failed` must not overwrite a more specific bounded launcher reason.
11. Cleanup failure takes precedence over protocol success.
12. No implementation step may weaken the existing rootless or Multipass boundary checkers.

## Expected canonical data flow

```text
host dispatch
  1. freeze exact clean source commit
  2. build source archive and source listing
  3. transfer and verify source in owned guest
  4. build launcher and references
  5. create run-identity.json from measured values
  6. create bundle staging tree
  7. execute four directions through rootless-enter.sh
  8. write per-direction records into staging
  9. validate catalog and cross-record bindings
 10. finalize manifest + checksum
 11. verify staging
 12. export atomically
 13. verify exported bundle
 14. write export receipt beside, not inside, the bundle
```

## Workstream A: repair bundle primitives

### A1. Make classification bundle-relative

Change the classifier signature from an implicit-path form to an explicit-root form:

```python
# Required shape; names may differ only for a strong reason.
def classify_bundle_file(staging_root: Path, path: Path) -> tuple[str, str]:
    root = staging_root.resolve(strict=True)
    candidate = path.resolve(strict=True)
    relative = candidate.relative_to(root)
    # reject absolute, traversal, unknown top-level class, and wrong depth
```

Do not retain expressions equivalent to:

```python
path.relative_to(path)
```

Expected classifications include:

```text
run-identity.json                         -> run-identity
environment/environment.json             -> environment
attestations/i2pr-to-java-ipv4.json      -> attestation
directions/i2pr-to-java-ipv4.json        -> direction
triggers/i2pr-to-java-ipv4.json           -> trigger
observations/i2pr-to-java-ipv4.json       -> observation
cleanup/i2pr-to-java-ipv4.json            -> cleanup
diagnostics/sanitized-summary.json        -> diagnostics
```

Unknown paths must fail finalization. They must not be recorded as `unknown`.

### A2. Hash exactly the bytes written

Serialize once, append exactly one newline, hash those bytes, and write those exact bytes:

```python
encoded = (json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")
digest = hashlib.sha256(encoded).hexdigest()
write_exact_bytes_atomically(path, encoded)
return digest
```

Acceptance requires:

```python
returned_digest == sha256(path.read_bytes()).hexdigest()
```

### A3. Verify `manifest.sha256`

`verify_bundle()` must:

1. read `manifest.sha256` as strict ASCII;
2. require exactly `<64 lowercase hex><two spaces>manifest.json\n`;
3. hash the actual `manifest.json` bytes;
4. reject any mismatch before trusting manifest contents.

Do not treat `manifest.sha256` as decorative metadata.

### A4. Keep export acknowledgement outside the immutable bundle

Required layout:

```text
target/interop/evidence/milestone-3/
  <run-id>/
    manifest.json
    manifest.sha256
    ... immutable bundle files ...
  <run-id>.export-ack.json
```

The acknowledgement must include:

- run ID;
- final bundle path relative to the evidence root, not an absolute host path;
- manifest SHA-256;
- export timestamp;
- verifier result;
- exporter version/schema.

`verify_bundle(<run-id>)` must still pass after acknowledgement creation.

### A5. Harden file-tree handling

Before manifest construction and after copy:

- reject symlinks;
- reject non-regular files;
- reject hard links when link count is greater than one, unless a documented platform exception is proven safe;
- reject hidden temporary files;
- reject files outside the allowlisted layout;
- reject case-colliding paths when normalized to lowercase;
- reject duplicate manifest entries;
- reject a manifest entry for `manifest.json` or `manifest.sha256`.

### A6. Validate semantic record type

For JSON classes, compare path class to payload schema/type. Examples:

```text
observations/*.json must declare i2pr-ntcp2-direction-observation-v2
run-identity.json must declare i2pr-interop-run-identity-v1
directions/*.json must declare the locked mixed-router evidence schema
```

A JSON file with a valid hash but wrong semantic schema must fail finalization.

## Workstream B: authoritative run identity

### B1. Select one creation owner

The run identity must be created after all measured artifacts exist but before the first direction executes. The recommended owner is the guest-side Plan 052 dispatcher invoked by `dispatch-gate.sh`.

The dispatcher must measure rather than trust caller-provided values:

- `git rev-parse HEAD` from the verified guest tree;
- dirty-state result from `git status --porcelain=v1 --untracked-files=all` or the repository's source-tree verifier;
- source tree digest;
- source archive digest from transfer record;
- launcher binary digest from the actual executable;
- reference artifact and installed-tree digests from verified cache manifests;
- rustc/cargo versions;
- target triple;
- topology kind;
- privilege model;
- environment manifest digest;
- evidence schema revision.

Do not accept an environment variable as the source of truth for `source_commit`.

### B2. Freeze identity before directions

After `run-identity.json` is written:

- set mode `0600`;
- compute its digest over the exact bytes on disk;
- write the digest into a separate dispatcher-owned variable;
- refuse to overwrite the file;
- refuse to begin a direction if its content no longer hashes to the frozen digest.

Example typed blocker:

```text
run-identity-mutated-after-freeze
```

### B3. Cross-check all direction records

Every direction record must carry:

```text
run_id
source_commit
launcher_binary_sha256
run_identity_sha256
```

Before bundle finalization, cross-check those fields against `run-identity.json`. A mismatch must produce a failed bundle and must never be rewritten into matching values automatically.

### B4. No contradictory legacy fields

The current record also carries `i2pr_commit`. During Plan 053 either:

- migrate to one canonical field and version the schema; or
- require `i2pr_commit == source_commit` exactly.

Do not allow both to disagree.

## Workstream C: wire the canonical execution path

### C1. Add explicit dispatcher arguments

The outer and inner paths must carry explicit, validated paths. Recommended arguments:

```text
--run-identity <absolute guest path under owned run root>
--bundle-staging <absolute guest path under owned run root>
--run-id <safe typed run id>
```

They must be added coherently through:

- `scripts/interop/multipass/dispatch-gate.sh`;
- `scripts/interop/run-direction.sh` if it remains in the path;
- `scripts/interop/rootless-enter.sh`;
- `tests/integration/ntcp2/harness/rootless_inner_runner.py`;
- `tests/integration/ntcp2/harness/mixed_runner.py`.

Do not infer Plan 052 paths from the current working directory.

### C2. Validate path ownership at every trust boundary

At the host-to-guest and outer-to-inner boundaries:

- resolve paths;
- require that they are under the owned run root;
- reject symlinks;
- require appropriate owner and mode;
- reject existing finalized targets;
- reject reuse of a run ID with a different identity digest.

### C3. Make Plan 052 mode explicit

Do not silently apply Plan 052 semantics to historical runs. Add an explicit profile or schema selector, for example:

```text
--evidence-profile milestone-3-v2
```

When selected, legacy unbound records must be rejected. Historical profiles may remain readable but cannot satisfy Plan 052.

## Workstream D: produce complete per-direction artifacts

For every one of the four directions, regardless of result, write exactly one file into each class:

```text
attestations/<direction>.json
directions/<direction>.json
triggers/<direction>.json
observations/<direction>.json
cleanup/<direction>.json
```

### D1. Direction result

The direction record must contain:

- exact direction ID and reference;
- typed result;
- bounded reason code;
- exact provenance binding;
- RouterInfo produced/absent state;
- reference metadata;
- runtime counters;
- observation record digest;
- trigger record digest;
- attestation digest;
- cleanup record digest.

### D2. Observation-v2 wiring

Plan 053 does not need real Java/i2pd receiver markers, but it must wire the schema. For missing markers, emit typed `not-observed` values with a bounded code such as:

```text
reference-receiver-marker-not-source-locked
```

Do not mark missing receiver observations `not-applicable` when that side is the receiver.

The current Plan 045 pass predicate must not be allowed to produce a Plan 052 `passed` record. In Plan 052 mode, use:

```python
passed = (
    both_authenticated(sender, receiver)
    and sender_emitted_data_frame(sender)
    and receiver_passes_data_phase(receiver)
    and cleanup_clean
)
```

### D3. Preserve Rust responder reason codes

Replace the generic Python collapse:

```python
raise MixedRunError("i2pr-responder-handshake-failed")
```

with bounded propagation from the terminal record. Guidance:

```python
reason = terminal.get("reason_code")
if reason not in ALLOWED_RESPONDER_REASONS:
    raise MixedRunError("unknown-i2pr-terminal-reason")
raise MixedRunError(reason)
```

The allowlist must include exactly the reason codes produced by the current launcher status schema. Do not accept arbitrary peer-controlled strings.

### D4. Trigger record

Always write a trigger record, including initiator directions where no external trigger is needed:

```json
{
  "schema": "i2pr-reference-trigger-v2",
  "scenario_id": "i2pr-to-i2pd-ipv4",
  "mode": "not-required-i2pr-initiator",
  "attempted": false,
  "outcome": "not-applicable",
  "reason_code": "i2pr-is-transport-initiator"
}
```

For blocked reference-initiated directions, use `attempted: false` and a typed blocker. Do not synthesize success.

### D5. Cleanup record

Record process IDs only as bounded counts or opaque run-local handles; do not export host paths or raw command lines. Required facts:

- each expected process started count;
- exited count;
- forced termination count;
- residual process check;
- topology cleanup result;
- final result.

## Workstream E: bundle finalization and export

### E1. Finalization order

The required order is:

1. all four direction classes complete;
2. environment block complete;
3. diagnostics sanitizer complete;
4. no live router/helper process remains;
5. validate all semantic records;
6. cross-check run identity;
7. build manifest;
8. write manifest and checksum;
9. verify staging;
10. export atomically;
11. verify exported bundle;
12. write external acknowledgement beside bundle.

### E2. Interrupted-run behavior

An interrupted run must remain under a staging name such as:

```text
.<run-id>.staging
```

It must never be renamed to the final run ID. A later attempt may inspect it, but must not resume it unless the run identity and all ownership records match exactly.

### E3. Diagnostic bundle acceptance

Plan 053's local acceptance bundle may contain four rejected/blocked direction records. It must still be structurally complete and re-verifiable. Its bundle-level result must be:

```text
diagnostic-complete-not-certificate
```

It must not be stored or named as a Milestone 3 certificate.

## Workstream F: tests required before implementation is considered complete

### F1. Bundle unit tests

Add tests for all of the following:

- bundle-relative record classification;
- rejection of `unknown` classes;
- exact returned digest equals file digest;
- malformed `manifest.sha256` rejection;
- manifest checksum mismatch rejection;
- exported bundle re-verifies after acknowledgement creation;
- acknowledgement is outside bundle;
- symlink rejection;
- FIFO/socket/device rejection where test platform permits;
- traversal and absolute path rejection;
- duplicate/case-colliding path rejection;
- semantic schema/path mismatch rejection;
- unexpected hidden file rejection;
- immutable existing target rejection.

### F2. Run identity tests

- dirty tree rejected;
- short/nonexistent commit rejected;
- launcher digest measured from bytes;
- identity mutation after freeze rejected;
- direction with mismatched run ID rejected;
- direction with mismatched source commit rejected;
- historical record cannot satisfy Plan 052 mode;
- `i2pr_commit` and `source_commit` disagreement rejected.

### F3. Runner integration tests

Use fake adapters and a temporary rootless-equivalent test seam. Cover:

- four rejected directions still produce 20 per-direction class files;
- responder stage reason survives from terminal JSON to direction record;
- missing receiver marker results in rejection, not pass;
- cleanup failure overrides otherwise passing observations;
- one missing class prevents finalization;
- interrupted export leaves no final directory;
- exported diagnostic bundle passes `verify_bundle()`.

### F4. Static checks

Extend the existing interoperability checker to fail when Plan 052 profile code:

- writes a legacy unbound direction record;
- calls the old Plan 045 pass predicate;
- writes raw logs under the bundle root;
- adds files after manifest finalization;
- writes an acknowledgement inside the immutable bundle;
- collapses a known responder-stage reason to the generic historical code.

## Suggested implementation sequence for a smaller model

Use small commits. Do not combine unrelated work.

1. `interop: repair Plan 052 bundle hashing and verification`
2. `interop: harden Plan 052 bundle path and file validation`
3. `interop: freeze and cross-check Plan 052 run identity`
4. `interop: carry Plan 052 run context through rootless lane`
5. `interop: write complete Plan 052 per-direction artifact classes`
6. `interop: preserve bounded responder terminal reasons`
7. `interop: finalize and atomically export diagnostic bundles`
8. `tests: close Plan 053 evidence pipeline acceptance matrix`
9. `docs: record Plan 053 implementation status`

After each commit run the narrow tests first. Run the full gates only after the narrow tests pass.

## Smaller-model execution guidance

### Read before editing

At minimum inspect:

- `tests/integration/ntcp2/harness/evidence_bundle.py`;
- `tests/integration/ntcp2/harness/run_identity.py`;
- `tests/integration/ntcp2/harness/observation.py`;
- `tests/integration/ntcp2/harness/evidence.py`;
- `tests/integration/ntcp2/harness/mixed_runner.py`;
- `tests/integration/ntcp2/harness/rootless_inner_runner.py`;
- `scripts/interop/rootless-enter.sh`;
- `scripts/interop/multipass/dispatch-gate.sh`;
- `tools/i2pr-interop/src/status.rs`.

### Do not guess schemas

Use the existing schema constants. When a schema change is unavoidable, update validator, writer, tests, documentation, and checker in the same commit.

### Do not make evidence pass by filling placeholders

Incorrect:

```python
record["run_identity_sha256"] = "0" * 64
record["receiver_decoded"] = True
```

Correct:

```python
record["actual_typed_result"] = "rejected"
record["reason_code"] = "reference-receiver-marker-not-source-locked"
```

### Preserve failures rather than hiding them

If a new validation breaks an old test, determine whether the old fixture is historical or Plan 052. Historical records may remain readable, but Plan 052 records must satisfy the stronger contract.

### Stop conditions

Stop and write a status record instead of improvising when:

- a required source value cannot be measured;
- a path must escape the owned run root;
- a proposed fix requires raw logs in an export bundle;
- the only way to pass is to weaken the observation predicate;
- a new schema would make historical evidence ambiguous;
- the target-host external lane is required to finish a local integration task.

## Required validation commands

Run at least:

```bash
python3 -m unittest tests.integration.ntcp2.harness.test_evidence_bundle
python3 -m unittest tests.integration.ntcp2.harness.test_plan052
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'

cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

Use the repository's canonical variants if command names have moved. Do not silently skip a missing checker.

## Explicit acceptance criteria

Plan 053 is complete only when every item below is true:

1. `write_json_atomic()` returns the SHA-256 of the exact bytes written.
2. Manifest entries have correct non-`unknown` record types.
3. `manifest.sha256` is parsed and verified.
4. A finalized exported bundle passes `verify_bundle()` without exceptions.
5. The export acknowledgement is outside the immutable bundle.
6. Symlinks and non-regular files are rejected.
7. One immutable run identity is created from measured values before directions execute.
8. Every direction record cross-checks against that identity.
9. The canonical rootless path accepts explicit Plan 052 run/bundle context.
10. All four primary directions produce complete typed artifact classes even when blocked.
11. Plan 052 mode cannot pass using the old handshake/one-sided oracle predicate.
12. Missing receiver evidence produces a typed rejection.
13. Bounded responder-stage reasons survive into sanitized evidence.
14. Cleanup failure overrides protocol success.
15. A complete four-direction diagnostic bundle is generated locally with result `diagnostic-complete-not-certificate`.
16. The diagnostic bundle is verified before and after atomic export.
17. All narrow and full validation commands pass.
18. `plans/053-status.md` records exact commits, tests, and known remaining external blockers.
19. No claim is made that Java startup, reference observation markers, direct triggers, or Milestone 3 are closed.

## Handoff output

The implementing agent must leave:

- code and tests satisfying all criteria;
- one locally generated sanitized diagnostic bundle or a checked-in fixture representing it;
- `plans/053-status.md`;
- no raw diagnostic logs;
- no temporary top-level scripts;
- NTCP2 support still marked experimental/non-advertised.
