# Plan 052: NTCP2 Milestone 3 evidence closure follow-up

## Status

- Plan type: corrective execution and evidence-closure plan.
- Starting branch: `main`.
- Starting repository head at plan authoring: `37057333e66147c0b72dfec7a7aef78bfcbf3a69`.
- Relevant attempted closure: `plans/045-closure-attempt.md` at
  `6daec13dc99064211aaab8db4b8a7e8c9f541208`.
- Active topology: `rootless-sealed-single-netns` inside the owned Multipass
  guest.
- Pinned references:
  - Java I2P 2.12.0, revision
    `2800040deee9bb376567b671ef2e9c34cf3e30b6`.
  - i2pd 2.60.0, revision
    `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Milestone 3 remains open.
- NTCP2 remains experimental and non-advertised.

## Objective

Produce a reproducible, durable, independently inspectable Milestone 3
mixed-router evidence bundle demonstrating the four required IPv4 directions:

1. `i2pr-to-java-ipv4`;
2. `java-to-i2pr-ipv4`;
3. `i2pr-to-i2pd-ipv4`;
4. `i2pd-to-i2pr-ipv4`.

Each accepted direction must prove all of the following without relying on an
echo assumption:

- the exact i2pr source and launcher binary used;
- the exact pinned reference artifact and installed tree used;
- RouterInfo continuity and digest binding for both peers;
- a completed authenticated NTCP2 handshake observed by both sides;
- the selected bounded data-phase action;
- sender-side frame/I2NP emission;
- receiver-side authenticated frame receipt and successful I2NP decoding, or
  an explicitly narrower result that cannot satisfy Milestone 3;
- rootless sandbox attestation and unchanged parent network state;
- clean process teardown with no surviving router or helper process;
- a complete sanitized evidence record retained in a durable exported bundle.

The plan must also resolve the two current execution blockers:

- intermittent Java I2P startup failure in fresh per-direction state under the
  rootless namespace;
- lack of a valid reference-initiated direct NTCP2 dial seam for Java I2P and
  i2pd.

## Current evidence position

The latest attempt reached real external execution and reported:

| Scenario | Current result | Current interpretation |
| --- | --- | --- |
| `i2pr-to-i2pd-ipv4` | `passed` | Provisional authenticated outbound compatibility result; receiver-side data observation and source provenance still require hardening. |
| `i2pd-to-i2pr-ipv4` | `rejected` | Existing SAM trigger waits for destination tunnel-pool readiness and does not provide a direct transport dial in the isolated two-router topology. |
| `i2pr-to-java-ipv4` | `rejected` | Java router intermittently shuts down during fresh-state initialization before NTCP2 becomes ready. |
| `java-to-i2pr-ipv4` | `rejected` | Same Java startup failure prevents the reference-initiated direction from reaching the transport trigger. |

The latest attempt is useful diagnostic evidence. It is not a Milestone 3
certificate and must not be relabeled as one.

## Corrective findings owned by this plan

### F1. Source provenance is inconsistent

The closure narrative identifies a launching commit ending in
`...2dc1117`, but that full SHA does not resolve. The actual commit beginning
with `1d7f482` is
`1d7f482df7f06b6c88edd4cad012eac45c8ef75c`, while the cited source-transfer
record identifies `868b418ff0b8374b41075ca6489f037eec5f6847`.

No accepted record may contain contradictory source identities. Evidence must
be generated from one exact clean commit and every source-derived field must be
computed from that checked-out tree rather than supplied by narrative text or
an environment override.

### F2. The passing i2pr-to-i2pd record does not yet prove receiver-side data acceptance

The reported counters show i2pr authentication and outbound emission:

- `authenticated = 1`;
- `frames_sent = 1`;
- `i2np_sent = 1`;
- `frames_received = 0`;
- `i2np_received = 0`.

The reference log marker establishes handshake progress, but the current
record does not independently establish that i2pd authenticated, decrypted,
parsed, and accepted the bounded I2NP frame. No-echo behavior is permitted;
missing receiver observation is not.

### F3. Rejected records are not durably retained

The closure narrative contains one full evidence digest and two abbreviated
or cleaned digests. The durable repository-visible or exported bundle does not
contain all four complete sanitized direction records and their linked
attestations.

Rejected, blocked, failed, and cleanup-failed records are evidence and must be
exported. They must never count toward aggregate success.

### F4. Java startup diagnosis is not controlled

The observed `FortunaRandomSource` shutdown is intermittent. Readable
`/dev/urandom` alone neither proves nor disproves an entropy failure. The
current diagnosis confounds:

- namespace placement;
- empty versus initialized router state;
- wrapper versus `runplain.sh` launch;
- single launch versus generation/live restart sequence;
- process-group teardown timing;
- mutable seed and identity file lifecycle.

A controlled matrix is required before changing the accepted evidence lane.

### F5. SAM STREAM creation is not a direct transport trigger

A SAM streaming session may require a ready destination, LeaseSet, inbound and
outbound tunnels, peers, and floodfill publication. That is materially broader
than asking the pinned reference transport stack to dial one imported
RouterInfo over NTCP2.

Using SAM as a transport trigger is acceptable only if source inspection and a
control experiment prove that it deterministically causes the intended direct
reference-to-i2pr NTCP2 connection without requiring unimplemented i2pr
streaming/tunnel functionality. Otherwise it must be replaced.

### F6. Live-debug artifacts and broad diagnostics remain on main

The repository contains temporary top-level helper scripts and broad debug
capture changes introduced while diagnosing the lane. These must be reviewed,
relocated, narrowed, or removed before the authoritative evidence commit.

## Non-goals

This plan does not:

- advertise NTCP2 support;
- close Milestone 3 with fewer than four accepted directional records;
- treat a typed environmental blocker as a protocol pass;
- accept local self-handshakes, testkit simulations, vectors, or loopback-only
  i2pr-to-i2pr runs as external interoperability evidence;
- weaken the Plan 046 rootless namespace, no-escalation, no-public-route, or
  cleanup guarantees;
- permit public I2P network access during execution;
- modify the pinned reference implementation to manufacture a passing result;
- accept a patched Java I2P or i2pd transport binary as equivalent to the
  pinned upstream reference;
- require an application-level echo response from a reference router;
- add streaming, tunnel, or NetDB production features to i2pr solely to make a
  transport test pass;
- use a silently privileged topology fallback;
- accept an unresolved or non-full source commit identifier.

## Mandatory invariants

The implementation agent must preserve these invariants throughout the pass.

### Repository and source invariants

- The evidence source tree is clean: `git status --porcelain=v1` is empty.
- The source commit is a full 40-character SHA returned by
  `git rev-parse HEAD`.
- `git cat-file -e <sha>^{commit}` succeeds in both the host checkout and the
  guest checkout.
- The transferred source archive is generated from the exact recorded commit.
- The guest source manifest, source listing, environment record, launcher
  binary, and every direction record identify the same source commit.
- The authoritative fields are computed by code. Human-authored closure text
  may quote them but may not override them.

### Reference invariants

- Reference version, source revision, artifact SHA-256, installed-tree SHA-256,
  and build recipe identifier remain pinned.
- Reference binaries and libraries used for protocol execution are built from
  the pinned upstream revision without transport behavior patches.
- A test-only trigger helper may link against or invoke the pinned reference
  code only under the direct-trigger rules below. Its source and binary hashes
  must be recorded separately from the reference artifact.
- A trigger helper may request a connection. It may not implement the NTCP2
  handshake on behalf of the reference.

### Sandbox invariants

- All router and trigger processes execute inside the attested rootless sealed
  namespace.
- The namespace has only loopback and the allowlisted synthetic addresses.
- There is no default route, public interface, DNS path, or public-network
  connectivity inside the execution namespace.
- `no_new_privs` is set and verified.
- UID/GID maps remain single-ID rootless mappings.
- Parent network state is unchanged before and after every direction.
- No automatic fallback to `privileged-dual-netns-veth` is permitted.

### Pass/fail invariants

- `known_deviation` is diagnostic metadata only and may never turn a rejection
  into a pass.
- A `passed` direction has an empty `known_deviation` field.
- A pass requires independent observations from both implementations.
- Missing receiver observation is a rejection, not a warning.
- Evidence-finalization failure overrides protocol success.
- Cleanup failure overrides protocol success.
- Aggregate success requires exactly four accepted primary direction records
  and no duplicate, missing, substituted, or unexpected scenario IDs.

## Workstream A: freeze and clean the authoritative baseline

### A1. Establish the evidence candidate commit

Create a dedicated sequence of commits for this plan. Before the first
authoritative run, designate one final evidence candidate commit and stop
changing implementation code during the run.

Required command checks:

```bash
git status --porcelain=v1
git rev-parse --verify HEAD
git cat-file -e "$(git rev-parse HEAD)^{commit}"
git fsck --no-dangling --no-reflogs
```

The run must refuse to start when the tree is dirty unless an explicit
`--diagnostic-dirty-tree` mode is selected. Diagnostic dirty-tree runs must:

- emit `actual_typed_result = blocked`;
- emit `reason_code = blocked-dirty-source-tree`;
- never produce a primary evidence record;
- never be accepted by the aggregate validator.

### A2. Remove or relocate temporary repository-root helpers

Audit these current top-level files and any equivalent untracked debugging
surfaces:

- `check_ri.sh`;
- `rebuild.sh`;
- `wrap.sh`.

For each file, choose exactly one disposition:

1. delete it if it is a one-off shell fragment;
2. move it under `scripts/interop/diagnostics/` with:
   - `set -euo pipefail`;
   - bounded inputs;
   - no `eval`;
   - no embedded absolute guest paths;
   - no secrets or unsanitized RouterInfo contents;
   - a unit/static test;
   - clear diagnostic-only documentation;
3. replace it with a maintained harness command and delete the helper.

No temporary root-level shell script may remain in the evidence candidate
commit.

### A3. Narrow debug capture

Replace unconditional or broad stdout/stderr dumps with an explicit diagnostic
mode:

```text
I2PR_INTEROP_DIAGNOSTICS=off|sanitized|raw-local
```

Rules:

- default is `off` or `sanitized`;
- `raw-local` is never exported and is rejected by the sanitizer if placed
  under an export root;
- accepted evidence records contain bounded, allowlisted event summaries, not
  arbitrary process output;
- rejected records may contain a bounded sanitized failure excerpt;
- a path, IP, key, destination, RouterInfo body, or process command that is not
  explicitly allowlisted must be redacted or represented by a digest.

### A4. Baseline validation

Run and retain the command summary for:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo doc --workspace --no-deps --locked
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

Any unavailable script must be investigated. Do not silently delete a required
check from the sequence.

## Workstream B: make provenance single-source and fail-closed

### B1. Add an evidence run identity record

Introduce one canonical record, for example:

```text
target/interop/evidence/<run-id>/run-identity.json
```

Schema name:

```text
i2pr-interop-run-identity-v1
```

Required fields:

- `schema`;
- `run_id`;
- `created_at`;
- `source_commit`;
- `source_commit_object_sha256` or equivalent immutable commit-object digest;
- `source_tree_sha256`;
- `source_archive_sha256`;
- `source_archive_format`;
- `source_dirty`;
- `host_source_manifest_sha256`;
- `guest_source_manifest_sha256`;
- `guest_source_listing_sha256`;
- `environment_manifest_sha256`;
- `launcher_binary_sha256`;
- `launcher_build_profile`;
- `rustc_version`;
- `cargo_version`;
- `target_triple`;
- `topology_kind`;
- `privilege_model`;
- `reference_lock_sha256`;
- `evidence_schema_revision`.

All hashes are lowercase 64-character hexadecimal SHA-256 strings. The
`source_commit` is a lowercase 40-character Git commit SHA.

### B2. Remove source-commit override ambiguity

Search for all locations that accept or propagate:

- `source_commit`;
- `SOURCE_COMMIT`;
- `I2PR_SOURCE_COMMIT`;
- environment manifest commit fields;
- CLI source commit flags.

The authoritative commit must be computed from the source checkout. An expected
commit may be supplied for verification, but it must be named as an
expectation, for example:

```text
--expected-source-commit <sha>
```

The code must reject when:

- the expected commit differs from `git rev-parse HEAD`;
- the archive manifest differs from the checkout;
- the guest checkout differs from the transferred manifest;
- the launcher binary was built before the final source transfer;
- a short SHA is used;
- the commit cannot be resolved in the guest checkout;
- an evidence record names a commit not present in `run-identity.json`.

Typed reasons:

- `blocked-source-commit-invalid`;
- `blocked-source-commit-unresolvable`;
- `blocked-source-commit-mismatch`;
- `blocked-source-tree-dirty`;
- `blocked-source-tree-hash-mismatch`;
- `blocked-source-archive-hash-mismatch`;
- `blocked-launcher-source-binding-mismatch`;
- `blocked-run-identity-missing`.

### B3. Bind every artifact to the run identity

Every direction record, sandbox attestation, reference build record, trigger
record, cleanup record, and aggregate manifest must carry:

- `run_id`;
- `run_identity_sha256`;
- `source_commit`;
- `launcher_binary_sha256`.

The validator must load the referenced run identity and compare these values.
It must not trust repeated fields without cross-checking them.

### B4. Add provenance tests

Required tests:

- valid exact clean commit passes;
- nonexistent 40-character SHA fails;
- short SHA fails;
- one-nibble commit mismatch fails;
- host and guest source-tree mismatch fails;
- source archive changed after manifest creation fails;
- launcher rebuilt from a different commit fails;
- direction record references another run identity fails;
- human-authored closure text is never used as an input to validation;
- old evidence without a run identity remains diagnostic-only and cannot close
  Milestone 3.

## Workstream C: retain a complete durable evidence bundle

### C1. Define the bundle layout

Use a stable layout such as:

```text
target/interop/evidence/milestone-3/<run-id>/
  run-identity.json
  environment/
    environment.json
    source-transfer.json
    cache-transfer.json
    offline-transition.json
    parent-network-before.sha256
    parent-network-after.sha256
  attestations/
    i2pr-to-java-ipv4.json
    java-to-i2pr-ipv4.json
    i2pr-to-i2pd-ipv4.json
    i2pd-to-i2pr-ipv4.json
  directions/
    i2pr-to-java-ipv4.json
    java-to-i2pr-ipv4.json
    i2pr-to-i2pd-ipv4.json
    i2pd-to-i2pr-ipv4.json
  triggers/
    i2pr-to-java-ipv4.json
    java-to-i2pr-ipv4.json
    i2pr-to-i2pd-ipv4.json
    i2pd-to-i2pr-ipv4.json
  observations/
    i2pr-to-java-ipv4.json
    java-to-i2pr-ipv4.json
    i2pr-to-i2pd-ipv4.json
    i2pd-to-i2pr-ipv4.json
  cleanup/
    i2pr-to-java-ipv4.json
    java-to-i2pr-ipv4.json
    i2pr-to-i2pd-ipv4.json
    i2pd-to-i2pr-ipv4.json
  diagnostics/
    sanitized-summary.json
  manifest.json
  manifest.sha256
```

Names may differ, but all information classes above must be represented.

### C2. Make collection atomic

Do not delete guest evidence as soon as one file is copied.

Required export flow:

1. Write each guest record to a per-run staging directory.
2. `fsync` or otherwise close every record before manifest generation.
3. Generate a guest bundle manifest containing path, size, SHA-256, schema, and
   record type for every exported file.
4. Copy the complete staging directory to a host temporary export directory.
5. Verify every host copy against the guest manifest.
6. Generate the host aggregate manifest.
7. Atomically rename the host temporary directory to the final run directory.
8. Write an export acknowledgement containing the final host manifest digest.
9. Only after acknowledgement may ordinary guest run directories be cleaned.
10. Retain the sanitized guest bundle until the entire gate is complete.

An interrupted export must leave a typed incomplete staging directory and must
not overwrite an older valid bundle.

Typed reasons:

- `failed-evidence-record-write`;
- `failed-evidence-bundle-manifest`;
- `failed-evidence-export-copy`;
- `failed-evidence-export-hash-mismatch`;
- `failed-evidence-export-incomplete`;
- `failed-evidence-export-acknowledgement`.

### C3. Export all terminal outcomes

Every attempted primary direction must produce one sanitized terminal record
with one of:

- `passed`;
- `rejected`;
- `blocked`;
- `failed`;
- `failed_cleanup`.

The record must be retained even when the process fails before handshake.
Fields that cannot be populated must use explicit typed absence rather than a
zero hash that looks valid.

Prefer fields such as:

```json
{
  "router_info": {
    "state": "not-produced",
    "sha256": null
  }
}
```

rather than a 64-zero digest.

### C4. Make record hashes self-consistent

Define one canonical JSON serialization and one rule for the record's own
hash. Recommended model:

- serialize the record without `record_sha256`;
- compute SHA-256 over the canonical bytes;
- add `record_sha256`;
- include the final file SHA-256 separately in `manifest.json`.

Do not call the file hash and logical record hash by the same field name.

### C5. Add bundle validation tests

Required tests:

- exactly four direction files required for the primary gate;
- all terminal outcomes retained;
- missing rejected record fails completeness validation;
- abbreviated digest fails;
- duplicated scenario ID fails;
- unexpected scenario ID fails;
- cross-run file injection fails;
- record hash mismatch fails;
- manifest file hash mismatch fails;
- absent attestation fails a passed record;
- rejected record may have typed missing RouterInfo but may not masquerade as a
  pass;
- interrupted export cannot replace a valid prior bundle;
- aggregate success is false unless all four primary records pass.

## Workstream D: define receiver-side evidence precisely

### D1. Replace ambiguous `observed` values with typed observation levels

Introduce a source-neutral observation schema, for example:

```text
i2pr-ntcp2-direction-observation-v2
```

Required per-side levels:

- `process_started`;
- `listener_ready`;
- `tcp_connected`;
- `ntcp2_authenticated`;
- `frame_emitted`;
- `frame_authenticated_and_decrypted`;
- `i2np_message_decoded`;
- `terminal_clean`.

Each level must include:

- `state = observed|not-observed|not-applicable`;
- `source = typed-status|structured-log|source-derived-log-marker|control-api`;
- `evidence_code`;
- `count` when applicable;
- `first_observed_monotonic_ms` or bounded relative ordering;
- `sanitized_detail`;
- `observer_implementation`.

### D2. Set the Milestone 3 directional predicate

A primary direction passes only when:

1. the initiator reports `ntcp2_authenticated`;
2. the responder reports `ntcp2_authenticated`;
3. the sender reports exactly the expected bounded `frame_emitted` and
   `i2np_message` count;
4. the receiver reports at least one matching
   `frame_authenticated_and_decrypted`;
5. the receiver reports `i2np_message_decoded` for the expected bounded message
   type and size;
6. no unexpected additional non-padding I2NP message is attributed to the test
   action;
7. both processes terminate cleanly;
8. all provenance, sandbox, and cleanup checks pass.

A reference is not required to echo the message. A reference must prove it
received and accepted the message.

### D3. Build a source-derived marker catalog

For each pinned reference:

1. inspect the exact pinned source revision;
2. identify the log/event/counter emitted after:
   - handshake authentication;
   - data-frame AEAD verification;
   - frame block parsing;
   - I2NP message decoding or dispatch;
3. record source file, symbol, revision, and normalized marker in a maintained
   catalog;
4. add a test that rejects marker text not present in the catalog;
5. prefer structured counters or a test-only observation interface over
   unbounded log scraping.

Suggested file:

```text
tests/integration/ntcp2/reference-observation-catalog.toml
```

Each catalog entry should contain:

- reference kind and version;
- revision;
- event name;
- source path;
- symbol or function;
- normalized marker pattern;
- semantic level;
- whether the marker is pre-authentication, post-authentication, post-decrypt,
  or post-I2NP-decode;
- sanitization rule.

Do not infer receiver data acceptance solely from `SessionConfirmed sent` or
`SessionConfirmed received`; those are handshake observations.

### D4. Add message correlation without secrets

The bounded test message must carry a non-secret run correlation value that can
be observed by both sides without exposing keys or full RouterInfo bodies.

Acceptable approaches, in order:

1. an allowlisted DeliveryStatus message ID generated for the run and recorded
   as a keyed or unkeyed digest in evidence;
2. a bounded test nonce contained in a specification-valid field and logged by
   both test surfaces;
3. a strict count-and-time-window correlation when the reference does not
   expose message identifiers.

The correlation value must not be a reusable cryptographic key or router
identity secret.

### D5. Reclassify provisional results honestly

Until receiver-side data acceptance is implemented, use narrower typed reasons
such as:

- `authenticated-handshake-only`;
- `authenticated-handshake-and-frame-emission`;
- `receiver-frame-observation-missing`;
- `receiver-i2np-decode-observation-missing`.

The existing i2pr-to-i2pd attempt may be retained as historical diagnostic
evidence but may not satisfy the new predicate without a new run.

### D6. Add observation tests

Required tests:

- handshake markers alone cannot pass the data phase;
- sender counters alone cannot pass;
- receiver frame decrypt without I2NP decode cannot pass;
- receiver I2NP decode with no authenticated handshake cannot pass;
- an echo is not required;
- duplicate markers do not inflate the bounded message count;
- unrelated background reference messages are not attributed to the test;
- the observation window is bounded and begins only after the target connection
  is identified;
- malformed or unknown reference markers fail closed;
- each reference catalog revision matches `references.lock.toml`.

## Workstream E: isolate and correct Java I2P startup

### E1. Build a standalone Java startup probe

Add a diagnostic command that starts only the pinned Java router under one
specified state and namespace configuration, waits for typed readiness or
terminal failure, stops it, verifies process cleanup, and emits a sanitized
record.

Suggested interface:

```bash
python3 tests/integration/ntcp2/harness/java_startup_probe.py \
  --reference-install <path> \
  --data-dir <path> \
  --launcher runplain|wrapper \
  --namespace outer|rootless \
  --sequence single|generate-live \
  --attempts <n> \
  --output <path>
```

The probe must not run i2pr and must not attempt an NTCP2 peer connection. Its
purpose is to isolate Java startup.

### E2. Run the controlled matrix

Run at least the following matrix:

| Axis | Values |
| --- | --- |
| Namespace | Multipass guest outer namespace; Plan 046 rootless child namespace |
| Data state | Empty; config-only template; uniquely pre-seeded fresh state; previously initialized mutable state for diagnosis only |
| Launcher | `runplain.sh`; supported wrapper launcher |
| Sequence | One clean startup; generation-stop-live restart sequence |
| Attempts | Minimum 10 per cell for cells used to choose the fix |

Do not run all cells concurrently. Concurrency would confound entropy and
process cleanup.

### E3. Record the right diagnostics

For each startup attempt record:

- attempt ID;
- namespace IDs;
- UID/GID map class;
- `no_new_privs` state;
- Java version and command digest;
- data-directory class;
- file names, modes, sizes, and SHA-256 values for allowlisted seed/state files;
- whether `/dev/random`, `/dev/urandom`, and `getrandom()` are accessible;
- process tree before start, at readiness/failure, and after cleanup;
- wrapper PID, Java PID, and child PIDs;
- timestamps for seed initialization, router start, EDH precalc start, shutdown
  request, and terminal exit;
- sanitized exception class and top stack symbols;
- readiness outcome;
- cleanup outcome.

Raw `strace` is diagnostic-only. When needed, use a bounded trace focused on:

```text
getrandom, openat, read, close, clone, clone3, execve, exit, exit_group, kill
```

Do not export raw paths or process environments. Export a sanitized summary.

### E4. Test entropy independently

Inside both namespace placements, run a bounded probe that:

- calls `getrandom()` repeatedly;
- reads `/dev/urandom` repeatedly;
- verifies nonblocking completion;
- records only timing and a digest of the bytes;
- verifies the device major/minor and mount identity;
- repeats before and during Java startup.

This probe must not claim that successful random reads prove Java correctness.
It only removes one variable from the fault tree.

### E5. Define safe state initialization options

Do not copy a mutable Java router directory wholesale into multiple concurrent
or accepted evidence directions.

Allowed candidate fixes:

#### Option 1: unique fresh-state bootstrap

- copy configuration-only files from a read-only template;
- generate a unique `prngseed.rnd` for each attempt from the guest kernel RNG;
- enforce mode `0600` and correct ownership;
- let Java create a unique router identity and key backup;
- wait for all required files to be durably created before export;
- never reuse the seed or identity in another simultaneous direction.

#### Option 2: initialized-state snapshot with explicit identity policy

Use only if source and matrix results show Java requires a completed first-run
initialization sequence.

- initialize one state directory in the same rootless namespace class;
- stop and verify complete Java process teardown;
- classify files as immutable configuration, persistent identity, mutable
  runtime state, or entropy state;
- construct per-direction snapshots with an explicit allowlist;
- generate unique entropy state per snapshot;
- preserve identity only when required for RouterInfo continuity within that
  one direction;
- never share a writable snapshot between directions;
- record the snapshot manifest and source snapshot digest.

#### Option 3: launcher correction

If the wrapper is shown to issue or retain a shutdown path incorrectly, use
`runplain.sh` or correct the harness lifecycle. Do not patch the Java router to
ignore a shutdown random source.

### E6. Verify process lifecycle

The adapter must own the entire Java process group.

Required behavior:

- start in a dedicated process group or cgroup-like process scope available to
  the unprivileged guest;
- distinguish wrapper PID from Java PID;
- readiness belongs to the current Java PID, not a stale log file;
- stop sends the supported graceful request first;
- bounded wait;
- bounded termination escalation inside the namespace;
- verify no matching Java, wrapper, router, or helper process remains;
- delete or rotate stale logs before the next attempt;
- refuse a live phase if the generation phase left a process behind.

### E7. Java acceptance criteria

The selected Java startup method is accepted only when:

- 10 consecutive rootless child-namespace startups succeed from independently
  prepared per-attempt state;
- 10 consecutive generation-stop-live sequences succeed;
- no attempt emits `Random is shut down`;
- no process survives cleanup;
- the generated RouterInfo contains the expected NTCP2 address;
- RouterInfo identity continuity holds within each generation/live direction;
- state is not shared writable across directions;
- the same method succeeds in both Java mixed directions during two complete
  evidence runs.

If the matrix cannot identify a stable method, emit
`blocked-java-reference-startup-unreliable` and do not proceed to Milestone 3
closure.

## Workstream F: create a valid reference-initiated direct NTCP2 trigger

### F1. State the required semantics

A valid trigger must cause the unmodified pinned reference transport stack to
open a TCP connection to the imported i2pr RouterInfo and execute the reference
implementation's NTCP2 initiator handshake.

The trigger must not require i2pr to implement:

- SAM;
- I2CP;
- streaming;
- destination LeaseSets;
- inbound or outbound tunnels;
- floodfill publication;
- application-level service behavior.

Those features are outside the transport closure being tested.

### F2. Investigate direct trigger seams in pinned source

For Java I2P and i2pd separately, inspect the exact pinned revision for:

- an existing command or console action that directly connects to a known
  router peer;
- a test API used by upstream transport integration tests;
- an internal method that queues a direct peer connection using the normal
  transport manager;
- an event or future that reports connection authentication;
- a safe way to invoke the method from a test-only helper without modifying
  transport behavior.

Produce a source-inspection record for each reference containing:

- revision;
- candidate source path and symbol;
- call graph from trigger to NTCP2 transport connection;
- prerequisites;
- whether it uses normal production handshake code;
- whether it depends on NetDB, tunnels, streaming, or floodfills;
- whether it can target the imported RouterInfo deterministically;
- selected or rejected disposition.

Suggested document:

```text
tests/integration/ntcp2/reference-trigger-contracts.md
```

### F3. Preferred implementation: source-pinned test-only direct-connect helper

When the pinned reference exposes a suitable internal connection method, build
a small helper against the pinned reference source or installed libraries.

Rules:

- helper source lives under `tests/integration/ntcp2/reference-drivers/`;
- helper is test-only and cannot be linked into i2pr production crates;
- helper invokes the reference's normal transport manager;
- helper does not construct or encrypt NTCP2 messages itself;
- helper accepts only:
  - reference control endpoint or process locator;
  - target RouterInfo path or target router hash;
  - bounded timeout;
  - run correlation ID;
- helper emits one structured trigger record;
- helper source SHA-256, binary SHA-256, compiler version, and linked reference
  revision are included in run identity/evidence;
- helper and reference run inside the sealed namespace;
- helper cannot connect outside the allowlisted synthetic target address;
- helper has a negative test proving an unknown target cannot be substituted;
- the reference binary/library remains the pinned upstream build.

For Java, an acceptable shape may be a small class compiled against the pinned
router jars that requests a connection through the router's ordinary comm
system. The exact class and method must be selected from source inspection.

For i2pd, an acceptable shape may be a small executable linked against the
pinned i2pd libraries that asks the normal transports manager to connect to the
imported peer. The exact symbol must be selected from source inspection.

Do not invent method names in implementation. Lock the actual source symbol and
revision in the trigger contract.

### F4. Trigger control experiments

Each selected direct trigger must pass these controls:

#### Positive control

- reference imports the exact i2pr RouterInfo;
- helper targets the exact router hash;
- reference opens a TCP connection to the expected synthetic i2pr endpoint;
- both sides observe NTCP2 authentication.

#### Wrong-RouterInfo control

- mutate or substitute the RouterInfo under a diagnostic-only run;
- authentication must fail closed;
- evidence must identify RouterInfo digest mismatch or authentication failure;
- no primary pass record may be written.

#### Wrong-address control

- target a closed allowlisted synthetic port;
- trigger must report a bounded connection failure;
- it must not fall back to another address or the public network.

#### No-trigger control

- run the reference and i2pr responder without invoking the helper;
- no target connection should appear during the bounded window;
- this proves the observed connection is attributable to the trigger.

#### Reference-code control

- prove through process/binary/library digests that the normal pinned reference
  transport implementation performed the handshake;
- the helper must not contain NTCP2 crypto or frame implementation code.

### F5. Fallback investigation: minimal sealed reference support topology

Use this branch only when source inspection proves there is no usable direct
transport seam.

Before implementing it, write an ADR that answers:

- why the direct trigger is unavailable;
- exactly which reference prerequisites require additional routers;
- why the topology still tests NTCP2 transport rather than i2pr streaming or
  tunnel functionality;
- the minimum number and roles of support routers;
- how the target i2pr connection is isolated and attributed;
- how support-router traffic is distinguished from the target direction.

The topology may include pinned support routers inside the same sealed network
namespace, but it must remain completely offline.

Requirements:

- support RouterInfos are generated during preparation and transferred by
  digest;
- no reseed URL or public NetDB access;
- no default route;
- support routers bind only allowlisted synthetic addresses;
- the topology manifest records every router, identity digest, role, endpoint,
  and reference revision;
- target i2pr RouterInfo is imported explicitly;
- target transport connection is identified by router hash and endpoint;
- support routers cannot satisfy the target observation predicate;
- any streaming/tunnel action must be shown not to require production i2pr
  features outside NTCP2;
- the pass predicate remains a direct authenticated NTCP2 connection between
  the selected reference and i2pr.

Potential configurations to source-verify, not assume:

- zero-hop or test-mode tunnel settings;
- one local floodfill/bootstrap router;
- a small multi-router floodfill set when the reference rejects a one-peer
  tunnel pool;
- preloaded RouterInfos and LeaseSets permitted by the reference test stack.

Do not choose a support topology merely because SAM stops timing out. It must
preserve the claimed test semantics.

### F6. Disallowed fallback

Do not:

- connect the guest to the public I2P network;
- use a live reseed server;
- patch the reference to bypass authentication;
- add production streaming/tunnel code to i2pr for this closure;
- use packet injection to impersonate a reference handshake;
- mark a reference-initiated direction passed based only on a SAM session
  creation reply;
- count a support router's connection as the target reference connection.

### F7. Trigger acceptance criteria

A trigger implementation is accepted when:

- source inspection documents the exact production transport path;
- positive, negative, no-trigger, and wrong-target controls pass;
- the reference uses the exact pinned artifact/revision;
- the target TCP flow is reference to i2pr synthetic endpoint;
- both sides authenticate;
- the bounded data phase is observed at the receiver;
- no public-network capability exists;
- the trigger record is included in the durable evidence bundle;
- two consecutive complete evidence runs reproduce the reference-initiated
  directions.

## Workstream G: expose and correct actual i2pr responder defects

Once a valid direct trigger exists, treat any i2pr responder handshake failure
as a protocol or runtime result until proven otherwise.

### G1. Improve responder terminal classification

Split the broad `i2pr-responder-handshake-failed` reason into bounded stages:

- `responder-tcp-accept-missing`;
- `responder-admission-rejected`;
- `responder-message1-decode-failed`;
- `responder-message1-options-invalid`;
- `responder-noise-state-failed`;
- `responder-session-created-write-failed`;
- `responder-session-confirmed-part1-failed`;
- `responder-session-confirmed-part2-failed`;
- `responder-router-identity-verification-failed`;
- `responder-handshake-timeout`;
- `responder-authenticated-link-install-failed`;
- `responder-data-frame-read-failed`;
- `responder-i2np-decode-failed`.

Sanitized errors may include stage and error class, not secret material.

### G2. Preserve runtime ownership and bounds

Any responder correction must preserve:

- pending-handshake admission permits until terminal authentication/failure;
- bounded handshake and link queues;
- cancellation-aware deadlines;
- replay cache ownership;
- link lease accounting;
- reader/writer child ownership;
- no Tokio or socket I/O in `i2pr-transport-ntcp2`;
- no testkit dependency in production crates;
- no daemon activation from `tools/i2pr-interop`.

### G3. Add external-regression fixtures carefully

When external execution finds a wire-format defect:

- reduce it to a sanitized, specification-permitted fixture or vector;
- document whether the fixture comes from Java I2P or i2pd behavior;
- avoid storing full RouterInfos, identities, or session secrets;
- add positive and negative tests;
- update architecture/spec conformance notes;
- do not make the local fixture the external evidence itself.

## Workstream H: harden validation and gate composition

### H1. Separate diagnostic evidence from closure evidence

Add explicit evidence classes:

- `diagnostic`;
- `candidate`;
- `accepted-primary`;
- `accepted-control`.

Rules:

- rejected and blocked records are normally `diagnostic`;
- a passed record is `candidate` until bundle validation succeeds;
- only the aggregate validator may mark it `accepted-primary`;
- control experiments are never primary directions;
- historical Plan 045 attempts remain diagnostic.

### H2. Enforce exact primary catalog

The Milestone 3 aggregate accepts exactly:

```text
i2pr-to-java-ipv4
java-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
i2pd-to-i2pr-ipv4
```

No IPv6 record, self-test, reference crosscheck, support-router record, or
control experiment may substitute for one of these four.

### H3. Strengthen known-deviation handling

- keep an allowlist for diagnostic reason codes;
- do not place success reasons in `known_deviation`;
- require `known_deviation = ""` for passes;
- allow rejected records to identify the typed blocker;
- aggregate validation ignores the allowlist when deciding success: only the
  primary pass predicate matters;
- add a test proving every known deviation still yields aggregate failure.

### H4. Validate parent-state and process cleanup per direction

Do not rely only on an aggregate cleanup check.

Each direction record must bind:

- parent network before digest;
- parent network after digest;
- equality result;
- expected process inventory;
- post-run process inventory;
- surviving PID count;
- surviving listener count;
- run-root cleanup disposition.

A protocol pass followed by a cleanup failure becomes `failed_cleanup`.

### H5. Add an aggregate report

Generate a machine-readable aggregate record containing:

- run identity digest;
- exact primary scenario catalog;
- per-direction record path and SHA-256;
- per-direction actual result;
- per-direction observation predicate result;
- reference artifact digests;
- sandbox attestation digests;
- cleanup results;
- control experiment summary;
- repeat-run linkage;
- aggregate result;
- unresolved blockers;
- claim boundary.

The aggregate result is `passed` only when every required condition is true.

## Workstream I: authoritative execution sequence

### I1. Preparation domain

Preparation may use network access only for declared, pinned dependencies and
reference sources.

Ordered steps:

1. start from the final clean evidence candidate commit;
2. verify the full commit locally;
3. create the source archive and source manifest;
4. transfer the source archive to the owned Multipass guest;
5. verify the guest source commit and tree hash;
6. build Java I2P and i2pd from pinned revisions;
7. verify artifact and installed-tree hashes;
8. build trigger helpers, if selected, and record their source/binary hashes;
9. build `i2pr-interop` from the exact guest source commit;
10. run all static/unit tests;
11. create `run-identity.json`;
12. freeze preparation outputs read-only where practical;
13. transition to offline execution state.

No implementation source change is allowed after step 1. A necessary change
starts a new candidate commit and a new run ID.

### I2. Pre-execution controls

Before the four directions:

1. verify offline namespace capability;
2. verify no default route inside the child namespace;
3. verify parent network digest baseline;
4. verify launcher and reference digests against run identity;
5. run the Java startup acceptance probe;
6. run each selected direct-trigger no-trigger and wrong-target control;
7. verify no stale router/helper process;
8. verify evidence staging directory is empty and unique.

A failed control blocks the primary gate.

### I3. Direction order

Run directions serially to reduce shared-state and entropy confounding:

1. `i2pr-to-java-ipv4`;
2. `java-to-i2pr-ipv4`;
3. `i2pr-to-i2pd-ipv4`;
4. `i2pd-to-i2pr-ipv4`.

For each direction:

1. create unique state directories;
2. create a fresh sandbox attestation;
3. generate/export the initiator RouterInfo;
4. stop and verify generation-phase cleanup when required;
5. import the exact peer RouterInfo;
6. start the responder and wait for typed listener readiness;
7. start the initiator or invoke the direct trigger;
8. wait for both-side authentication observations;
9. execute exactly one bounded data-phase action;
10. wait for sender and receiver data observations;
11. stop processes;
12. verify process and listener cleanup;
13. verify parent network state unchanged;
14. finalize and hash the direction record;
15. retain all sanitized records regardless of outcome.

A rejected direction does not stop evidence export. It may stop later primary
directions only when continuing would contaminate state or violate an
invariant.

### I4. Export and validate

After all directions:

1. generate the guest bundle manifest;
2. export atomically;
3. verify all hashes on the host;
4. run the direction validator;
5. run the aggregate validator;
6. run cleanup verification;
7. generate the aggregate report;
8. retain the complete sanitized bundle.

### I5. Repeatability requirement

A single four-direction pass is not sufficient.

Run at least two complete accepted executions from the same source commit and
reference artifacts:

- Run A: clean guest run directories and prepared cache;
- Run B: new run ID, fresh per-direction state, cache reuse, complete teardown
  between runs.

Recommended third execution:

- Run C: stopped/started guest with the same immutable prepared artifacts and
  new state directories.

All accepted runs must agree on:

- source commit;
- launcher binary digest;
- reference artifact and installed-tree digests;
- trigger helper digests;
- topology kind;
- scenario catalog;
- observation semantics.

Router identities and entropy state may differ where the scenario contract
permits them. Their per-run digests must be recorded.

## Required test inventory

### Provenance tests

- exact clean source passes;
- dirty source fails;
- nonexistent commit fails;
- short commit fails;
- host/guest tree mismatch fails;
- launcher/source mismatch fails;
- cross-run record injection fails.

### Evidence tests

- four terminal records always exported;
- rejected records retained;
- incomplete digest rejected;
- atomic export interruption handled;
- manifest and record hashes verified;
- old diagnostic evidence cannot close Milestone 3.

### Observation tests

- handshake-only cannot pass;
- frame emission without receiver decrypt cannot pass;
- receiver decrypt without I2NP decode cannot pass;
- no echo required;
- background reference traffic excluded;
- source-derived marker catalog locked to revision.

### Java tests

- standalone startup probe unit tests;
- matrix record schema tests;
- stale log/PID rejection;
- unique seed/state preparation;
- generation/live identity continuity;
- 10 consecutive rootless startup stress attempts;
- 10 consecutive generation/live stress attempts.

### Trigger tests

- exact target positive control;
- no-trigger control;
- wrong RouterInfo control;
- wrong address/port control;
- trigger timeout;
- trigger helper/reference revision mismatch;
- helper contains no NTCP2 implementation module;
- target flow attribution.

### Runtime/protocol tests

- outbound i2pr to Java;
- inbound Java to i2pr;
- outbound i2pr to i2pd;
- inbound i2pd to i2pr;
- receiver frame and I2NP observation;
- responder stage-specific failure classification;
- cleanup overrides pass.

### Static boundary tests

- rootless files contain no escalation path;
- no silent privileged fallback;
- trigger helpers cannot enter production dependency graph;
- diagnostic scripts have no `eval` or public network targets;
- evidence sanitizer rejects raw-local diagnostics;
- support topology, if used, has no default/public route.

## Acceptance criteria

Plan 052 is complete only when all criteria below are met.

### Implementation and hygiene

- [ ] Temporary root-level debug helpers are removed or converted into
      maintained diagnostic commands.
- [ ] Debug output is bounded and mode-controlled.
- [ ] All repository boundary checks pass.
- [ ] Full Rust and Python validation passes at the final evidence commit.
- [ ] NTCP2 remains non-advertised until the separate Milestone 3 review.

### Provenance

- [ ] One resolvable 40-character source commit is used.
- [ ] The source tree is clean.
- [ ] Host source, archive, guest source, launcher, and records bind to the same
      run identity.
- [ ] No narrative-only or environment-overridden commit value can be accepted.
- [ ] Reference and trigger artifacts are fully hashed and pinned.

### Java stability

- [ ] Root cause of the intermittent Java shutdown is documented by controlled
      matrix evidence.
- [ ] The selected state/launcher method passes the consecutive startup and
      generation/live stress criteria.
- [ ] No stale process or shared writable state remains.
- [ ] Both Java primary directions can reach NTCP2 execution reliably.

### Reference-initiated trigger

- [ ] Java and i2pd each have a source-documented direct transport trigger, or
      an ADR-approved minimal sealed support topology.
- [ ] Trigger controls prove target attribution and fail closed on wrong input.
- [ ] The normal pinned reference transport stack executes the handshake.
- [ ] The trigger does not require unimplemented i2pr application/tunnel
      behavior.

### Observation

- [ ] Both sides independently observe NTCP2 authentication.
- [ ] Sender emission is observed.
- [ ] Receiver frame authentication/decryption is observed.
- [ ] Receiver I2NP decode is observed.
- [ ] No echo is required.
- [ ] The current provisional i2pr-to-i2pd result is rerun under the strengthened
      predicate.

### Evidence bundle

- [ ] All four terminal records are retained in every run.
- [ ] Each direction has a linked sandbox attestation, trigger record,
      observation record, and cleanup record.
- [ ] The bundle exports atomically and verifies by hash.
- [ ] Exactly four primary direction records are present.
- [ ] Aggregate success is computed, not manually declared.
- [ ] At least two complete runs pass from the same source/reference artifacts.

### Directional closure

- [ ] `i2pr-to-java-ipv4` passes.
- [ ] `java-to-i2pr-ipv4` passes.
- [ ] `i2pr-to-i2pd-ipv4` passes under receiver-side observation.
- [ ] `i2pd-to-i2pr-ipv4` passes using a valid direct trigger.
- [ ] Every direction has clean teardown and unchanged parent network state.

### Claim boundary

- [ ] A new closure document states exactly what was proven.
- [ ] The closure does not claim public-network, performance, DoS-resilience,
      long-duration, IPv6, SSU2, tunnel, streaming, or daemon readiness.
- [ ] Milestone 3 is reviewed separately after evidence validation.
- [ ] Support metadata changes, if any, are made only after that review.

## Failure and blocker semantics

### Blocked outcomes

Use `blocked` when execution cannot begin because a prerequisite is absent or
inconsistent, including:

- source provenance mismatch;
- reference artifact mismatch;
- rootless namespace unavailable;
- Java startup method fails the stability threshold;
- no valid direct trigger or semantics-preserving support topology exists;
- offline execution boundary unavailable.

### Rejected outcomes

Use `rejected` when the scenario executes but fails a protocol/evidence
predicate, including:

- authentication failure;
- receiver frame observation missing;
- receiver I2NP decode missing;
- wrong target connection;
- RouterInfo mismatch;
- unexpected data-phase count.

### Failed outcomes

Use `failed` for harness or unexpected process failures that prevent a reliable
protocol interpretation.

### Failed cleanup

Use `failed_cleanup` whenever:

- a router/helper process survives;
- a listener survives;
- parent network state changes;
- evidence finalization/export fails after protocol execution;
- run-root cleanup violates the contract.

`failed_cleanup` always overrides `passed`.

## Decision points for the implementation agent

### Decision 1: Java state method

Choose the minimum method supported by the controlled matrix:

1. unique fresh-state bootstrap;
2. initialized per-direction snapshot with unique entropy state;
3. launcher/lifecycle correction.

Document rejected methods and their observed failure rates.

### Decision 2: reference trigger

Choose in order:

1. existing supported direct-connect control seam;
2. test-only helper invoking normal pinned reference transport code;
3. ADR-approved minimal sealed support topology;
4. typed blocker when none preserves the transport-only claim.

Do not skip directly to a multi-router topology without source inspection.

### Decision 3: receiver observation

Choose the strongest source-supported reference surface:

1. structured internal test observation;
2. reference counter;
3. source-derived structured log marker;
4. bounded source-derived log correlation.

Handshake-only markers are insufficient.

## Suggested commit sequence

1. `interop: add run identity and strict source provenance validation`
2. `interop: retain complete atomic evidence bundles`
3. `interop: add typed receiver-side observation schema`
4. `interop: add source-derived reference observation catalog`
5. `interop: add controlled Java startup probe and matrix`
6. `interop: stabilize Java per-direction state lifecycle`
7. `interop: add source-pinned Java direct transport trigger`
8. `interop: add source-pinned i2pd direct transport trigger`
9. `interop: classify responder handshake stages`
10. `interop: harden aggregate validation and cleanup precedence`
11. `interop: remove temporary debug helpers and narrow diagnostics`
12. `docs: record Plan 052 evidence execution attempt`
13. `docs: close Plan 052 only after repeatable four-direction bundles`

If the trigger investigation requires an ADR-approved support topology, insert
that ADR and topology implementation before the corresponding trigger commit.

## Handoff execution checklist

### Before coding

- [ ] Read Plans 038, 042, 045, 046, 049, 051, and this plan.
- [ ] Read ADRs 0017 and 0019.
- [ ] Read the three NTCP2 interoperability skills created at current head.
- [ ] Confirm pinned reference revisions.
- [ ] Confirm no Plan 052 file already supersedes this plan.

### During coding

- [ ] Keep changes separated by workstream.
- [ ] Add tests with every schema or behavior change.
- [ ] Do not run authoritative evidence from a dirty tree.
- [ ] Keep diagnostic evidence clearly classified.
- [ ] Do not weaken sandbox or cleanup checks to progress the run.
- [ ] Record source-inspection decisions for both references.

### Before external execution

- [ ] Freeze final evidence candidate commit.
- [ ] Run all local checks.
- [ ] Verify Java startup stability threshold.
- [ ] Verify trigger control experiments.
- [ ] Verify receiver observation predicate using controls.
- [ ] Purge stale run directories and processes without deleting older exported
      evidence.
- [ ] Create a new unique run ID.

### After each run

- [ ] Export all four terminal records.
- [ ] Verify the complete bundle.
- [ ] Preserve rejected/blocked records.
- [ ] Compare parent network state.
- [ ] Verify no processes/listeners survive.
- [ ] Do not edit source before a repeat run under the same candidate commit.

### Before closure

- [ ] Two complete accepted runs exist.
- [ ] Every accepted record resolves to the exact source commit.
- [ ] All file and logical record hashes verify.
- [ ] The aggregate validator passes independently.
- [ ] Closure language matches the evidence and no broader claim is made.

## Expected final deliverables

The completed pass should leave:

1. strict run-identity/provenance implementation;
2. atomic durable evidence-bundle implementation;
3. receiver-side typed observation implementation;
4. reference observation catalog locked to pinned revisions;
5. Java startup diagnostic matrix and selected stable lifecycle;
6. valid Java and i2pd reference-initiated direct transport triggers, or a
   documented typed blocker for any reference where this is impossible;
7. improved i2pr responder failure classification;
8. cleaned repository diagnostics surface;
9. at least two complete sanitized Milestone 3 candidate bundles;
10. a Plan 052 execution/closure record with exact bundle hashes;
11. a separate Milestone 3 review document deciding whether support metadata
    may advance.

## Final closure rule

Plan 052 does not close because one direction once reported `passed`.

It closes only when one exact clean source commit produces at least two complete
sanitized bundles in the rootless sealed Multipass lane, each bundle contains
exactly the four required directions, every direction proves both-side NTCP2
authentication plus sender emission and receiver frame/I2NP acceptance, every
record is bound to the same run identity and pinned references, and every run
finishes with verified evidence export, unchanged parent network state, and no
surviving process.

Anything less remains a typed diagnostic result and keeps Milestone 3 open.
