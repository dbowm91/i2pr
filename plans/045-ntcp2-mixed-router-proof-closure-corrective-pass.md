# Plan 045: NTCP2 mixed-router proof closure corrective pass

## Objective

Correct the remaining defects in the Plan 044 mixed-router implementation and produce the first trustworthy, independently attributable NTCP2 interoperability proof between i2pr and the pinned Java I2P and i2pd references.

Plan 044 established the intended component boundaries, but the current composition can still generate RouterInfo from one identity and start another identity, validates a responder scenario through Python that the Rust launcher rejects, describes reference triggers and data-phase observations that are not implemented, and can classify a direction as passed without dual authentication or a completed data-phase oracle. Its evidence and gate contracts also remain inconsistent with the four directional scenarios.

Plan 045 must close those defects before another expensive Ubuntu lane is attempted.

The completed phase must establish, on an authorized Ubuntu 24.04 amd64 host and without contacting the public I2P network, that:

- the exact persisted identity and NTCP2 static key that produced a RouterInfo are used by the live router process;
- Python and Rust consume the same versioned launcher scenario schema;
- each reference-side trigger or observation mechanism is source-verified and actually executed;
- the launcher supports a protocol-valid directional data-phase proof rather than requiring an unspecified DeliveryStatus echo;
- both sides independently observe authentication for each direction;
- the required sender and receiver observations are true before a direction may pass;
- all retained hashes and counters are real rather than zero-filled or placeholder values;
- the handshake-smoke gate accepts exactly the four mixed-router directional records;
- the full profile no longer routes impossible legacy scenarios through the environment-smoke runner;
- evidence finalization, cleanup, and aggregate validation remain independent fail-closed requirements.

Plan 045 does not advertise NTCP2 support and does not close Milestone 3 by itself. Milestone 3 remains open until a separate evidence review compares successful retained records against `plans/000-mvp-roadmap.md` and `specs/CONFORMANCE.md`.

## Starting repository state

This plan starts from main commit:

```text
9f89fe4136a5ea5623a94fb19d7d7dddea7508cd
```

Relevant implementation files include:

- `plans/044-ntcp2-interop-final-integration-corrective-pass.md`
- `plans/044-closure.md`
- `tests/integration/ntcp2/harness/mixed_runner.py`
- `tests/integration/ntcp2/harness/launcher_renderer.py`
- `tests/integration/ntcp2/harness/launcher_protocol.py`
- `tests/integration/ntcp2/harness/data_oracle.py`
- `tests/integration/ntcp2/harness/reference_trigger.py`
- `tests/integration/ntcp2/harness/i2pr.py`
- `tests/integration/ntcp2/harness/java_i2p.py`
- `tests/integration/ntcp2/harness/i2pd.py`
- `tools/i2pr-interop/src/main.rs`
- `tools/i2pr-interop/src/scenario.rs`
- `scripts/interop/run-matrix.sh`
- `scripts/interop/run-gate.sh`
- `tests/integration/ntcp2/harness/build_gate.py`
- `.github/workflows/ntcp2-interop-ubuntu.yml`

The current closure record must be treated as a local implementation status note, not proof that Plan 044 completed its external interoperability objective.

## Controlling documents

The implementing agent must read and preserve the boundaries in:

- `plans/000-mvp-roadmap.md`
- `plans/030-milestone-3-overview.md`
- `plans/039-plan-038-corrective-interoperability-roadmap.md`
- `plans/040-interop-apparatus-corrective-pass.md`
- `plans/041-reference-router-private-crosscheck.md`
- `plans/042-runtime-owned-ntcp2-wire-driver.md`
- `plans/043-ubuntu-build-system-interop-gates.md`
- `plans/044-ntcp2-interop-final-integration-corrective-pass.md`
- `plans/044-closure.md`
- `docs/adr/0015-ubuntu-reference-router-harness.md`
- `docs/adr/0016-ubuntu-build-system-interop-gates.md`
- `docs/architecture/interop-apparatus.md`
- `docs/private-testnet.md`
- `docs/security-model.md`
- `specs/CONFORMANCE.md`
- `tests/integration/ntcp2/README.md`
- `tests/integration/ntcp2/evidence/README.md`
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md`

Where documentation and executable behavior disagree, fail closed and correct both in the same commit.

## Non-negotiable boundaries

1. Do not activate `i2pr-daemon`.
2. Do not reseed, bootstrap, publish RouterInfo to a public NetDB, enable public discovery, enable SSU2, create tunnels, accept transit traffic, or contact the public I2P network.
3. Keep all execution inside disposable namespaces joined only by synthetic documentation-range addresses.
4. Preparation may access locked source and dependency locations; scenario execution must use verified offline caches.
5. Keep Tokio, sockets, deadlines, cancellation, replay, admission, queues, and authenticated-link ownership in `i2pr-runtime`.
6. Keep `i2pr-transport-ntcp2` runtime-neutral.
7. Use `tools/i2pr-interop` only as a non-production composition root.
8. Do not retain raw RouterInfo, identities, NTCP2 static keys, I2NP bodies, frame plaintext, transcripts, endpoint text, raw logs, packet captures, or private paths in evidence.
9. Listener readiness, TCP connection success, a generic NTCP2 log phrase, byte counts, padding, termination, or local loopback tests are not sufficient interoperability evidence.
10. One side's authentication observation cannot substitute for the other side's observation.
11. A data-phase oracle description is not an executed data-phase oracle.
12. Protocol success never overrides cleanup failure.
13. Evidence-write failure is not cleanup failure unless cleanup independently failed.
14. Reference-control evidence remains distinct from i2pr mixed-router evidence.
15. Do not weaken validation or silently default unknown identifiers merely to make the lane progress.

## Defects this plan must close

### Defect 1: RouterInfo identity does not match the live process

The current i2pr-initiated flow generates a reference RouterInfo under `ref-gen`, stops that router, then starts a new router under `ref`. Those run roots contain different identity and transport-key state.

The current reference-initiated flow generates i2pr state under `i2pr-gen`, stops that launcher, then starts a different state directory under `i2pr`.

Both flows violate the central NTCP2 requirement that the live peer own the private identity and static key advertised by the RouterInfo used in the handshake.

### Defect 2: The i2pr RouterInfo export path is wrong and validation is unconditional

The Rust launcher writes RouterInfo to `<state_dir>/router.info`. The current helper searches `i2pr-gen/exchange/router.info`, skips import when absent, and still marks the i2pr RouterInfo as validated.

### Defect 3: Python and Rust disagree on absent optional scenario fields

The Python renderer emits empty-string and zero sentinels for responder peer fields. Python normalizes them to `None`, while the Rust deserializer receives present values and rejects the address or port before role validation.

### Defect 4: Reference triggers are typed placeholders

`reference_trigger.py` currently returns `observed=False` and pending descriptions. The mixed runner selects a trigger but does not invoke it.

### Defect 5: Data-phase oracles are typed placeholders

Every `observe()` implementation currently returns false sender and receiver observations. The mixed runner ignores those values and can still set the direction result to passed.

The proposed SAM and HTTP control mechanisms are also disabled in the checked-in reference configurations, and their ability to inject or observe raw I2NP traffic has not been proven against the pinned source revisions.

### Defect 6: The Rust launcher still requires a DeliveryStatus response

The current launcher sends a DeliveryStatus and then requires an inbound DeliveryStatus before emitting `passed`. NTCP2 does not define a generic DeliveryStatus echo, so this remains an invalid universal oracle for stock reference routers.

### Defect 7: Mixed success classification is incomplete

The runner currently:

- calls an adapter method that does not exist (`observed_phrase` on the adapter rather than the process or typed adapter API);
- does not require the reference-side authentication observation to be true;
- does not require sender and receiver oracle observations to be true;
- does not invoke the selected trigger;
- does not verify scenario IDs in launcher status records at the runner boundary;
- can allow an uncontrolled `AttributeError` traceback rather than a typed terminal result.

### Defect 8: Passed mixed evidence contains a zero configuration hash

The mixed record always emits a zero-filled `configuration_sha256`. The validator correctly rejects zero hashes for passed records.

The existing schema also does not retain the dual RouterInfo validation, dual authentication observations, trigger result, or data-phase sender/receiver observations needed to audit the mixed proof.

### Defect 9: Gate allowlists and profile execution disagree

`run-matrix.sh` sends four directional IDs to `mixed_runner.py`, but `run-gate.sh` still allows the older two handshake IDs.

The full profile still invokes legacy scenarios through `runner.py`, which intentionally blocks non-environment-smoke profiles. This makes the full gate impossible before router behavior is considered.

### Defect 10: Unknown reference inference fails open

The shell `reference_for()` helper defaults unknown names to i2pd. An unknown scenario must be rejected, not silently assigned to a reference.

## Deliverable 1: Reopen the Plan 044 status accurately

Update `plans/044-closure.md` or add a corrective status addendum stating:

- Plan 044 component scaffolding and local tests landed;
- the mixed proof remained incomplete;
- identity continuity, schema parity, triggers, data-phase observations, evidence completeness, and gate reconciliation were found incomplete;
- no authenticated mixed-router result exists yet;
- Plan 045 owns the remaining closure work.

Do not erase the historical local validation record. Correct the status interpretation explicitly.

## Deliverable 2: Introduce one live router-instance lifecycle per side

Refactor adapters and `mixed_runner.py` so each side has exactly one owned instance directory for the entire directional run.

Recommended directory layout:

```text
target/interop/runs/<run-id>/
  i2pr/
    state/
    exchange/
    raw/
    scenario.toml
    status.jsonl
  reference/
    reference-runtime/
    reference-data/
    config/
    exchange/
    raw/
```

For each side, the same adapter object and state root must be used for:

1. state preparation;
2. identity generation or load;
3. RouterInfo production;
4. RouterInfo validation;
5. peer RouterInfo import;
6. live process start;
7. authentication observation;
8. data-phase observation;
9. bounded stop;
10. cleanup accounting.

Do not create `*-gen` and live directories with independent state.

### Adapter lifecycle contract

Give every adapter explicit methods with stable semantics, for example:

```text
prepare()
prepare_identity()
router_info_path()
export_router_info()
import_peer_router_info(path)
start()
wait_ready()
authenticated_observation()
configuration_digest()
identity_public_digest()
stop()
counters()
```

The exact names may differ, but preparation and live execution must share state.

For references that can only create RouterInfo after process startup:

- start the same instance;
- wait for RouterInfo production;
- stop it cleanly without deleting its state;
- import the peer RouterInfo into the same state root;
- restart the same instance;
- verify the identity and static-public-key digests did not change.

A controlled restart of the same persisted state is acceptable. Starting a new identity is not.

### Identity-continuity assertions

Before and after every restart, compute sanitized public digests for:

- router identity hash;
- NTCP2 static public key;
- obfuscation IV;
- encoded RouterInfo bytes.

These digests may be retained; private material may not.

Reject the run if any public digest changes unexpectedly.

Add tests proving:

- a same-state restart preserves all public digests;
- changing the state root changes identity and is detected;
- a RouterInfo from one root cannot be used to classify another root as validated;
- the live process path and exported RouterInfo path are descendants of the same owned instance root.

## Deliverable 3: Correct i2pr RouterInfo generation and export

The i2pr launcher state contract must expose the actual RouterInfo generated at:

```text
<run-root>/state/router.info
```

Implement one of these designs:

1. Add a non-networking `ntcp2 prepare` subcommand that creates or loads state and writes the signed RouterInfo without binding a socket.
2. Retain `listen` preparation but use the same state directory for the later live listener and provide a typed `prepared` or `listener_ready` status.

The non-networking prepare command is preferred because it avoids starting a listener solely to materialize state.

Whichever design is chosen:

- return a typed status containing only public counters and reason codes;
- verify the RouterInfo through both the Rust decoder/signature verifier and the strict Python validation boundary;
- verify the advertised endpoint matches the later listener endpoint;
- copy it only into the reference adapter's confined import path;
- never claim `validated-and-bound` unless validation actually completed;
- reject missing, empty, oversized, symlinked, multiply matched, or escaped paths.

## Deliverable 4: Establish one versioned scenario schema shared by Rust and Python

Remove empty-string and zero sentinels for absent optional fields.

Preferred schema behavior:

- initiator scenarios contain `peer_address`, `peer_port`, and `peer_router_info`;
- responder scenarios omit all three fields;
- `deterministic_seed` is omitted when not selected;
- unknown fields remain rejected;
- role-specific required and forbidden fields remain enforced.

Update:

- `tools/i2pr-interop/src/scenario.rs`
- `tests/integration/ntcp2/harness/launcher_protocol.py`
- `tests/integration/ntcp2/harness/launcher_renderer.py`
- scenario fixtures and tests.

### Cross-language contract fixtures

Add checked-in non-secret fixtures for:

- valid IPv4 initiator;
- valid IPv4 responder;
- valid IPv6 initiator;
- valid IPv6 responder;
- missing initiator peer;
- responder with peer fields;
- partial peer endpoint;
- absolute and parent-traversal paths;
- invalid network ID;
- unknown field;
- unsupported padding profile;
- invalid deadline;
- duplicate endpoint.

Run every fixture through both parsers and require identical accept/reject disposition and normalized role/family/endpoint/path values.

Do not allow Python-only parser tests to stand in for the Rust parser.

## Deliverable 5: Perform a pinned-source oracle and trigger capability audit

Before implementing any control mechanism, inspect the exact pinned Java I2P and i2pd source revisions.

Produce a short checked-in design record under `docs/architecture/` or `tests/integration/ntcp2/references/` that answers, for each reference:

- what mechanism can deterministically initiate a connection to one imported peer;
- what mechanism can prove the reference authenticated the NTCP2 session;
- what mechanism can inject or naturally generate a bounded I2NP message onto that authenticated session;
- what mechanism can prove the reference accepted and parsed an inbound I2NP message;
- whether the mechanism exists in the stock pinned build;
- whether it requires enabling a namespace-local control listener;
- whether it requires a minimal observability-only source patch;
- exact source files, symbols, configuration keys, and expected typed observations.

Do not assume:

- SAM can inject arbitrary raw I2NP messages;
- I2CP can inject arbitrary transport-level messages;
- an i2pd JSON-RPC `ConnectPeer` or message-injection method exists;
- a generic log phrase is authoritative;
- automatic dialing occurs merely because one RouterInfo is imported.

### Acceptable mechanism hierarchy

Use the first viable option for each capability:

1. Stock pinned implementation with an existing structured local control/status API.
2. Stock pinned implementation with a deterministic client action that naturally causes the required router transport behavior.
3. Exact pinned source plus a minimal test-only instrumentation patch that exposes counters or injection at the router/transport boundary without altering handshake, encryption, framing, routing, or admission behavior.

If instrumentation is required:

- retain an unmodified stock reference build for the Java-to-i2pd control crosscheck;
- apply the patch only to a separate mixed-test build;
- record the exact upstream revision and patch SHA-256;
- record the patched-tree digest and resulting binary digest;
- review the patch to ensure it changes observability or test injection only;
- bind any control listener only to namespace loopback;
- firewall it from the peer veth and host;
- disable it by default outside the mixed harness;
- never upload the patched source tree or raw control traffic.

Stop the phase if no mechanism can produce trustworthy sender and receiver observations without modifying protocol behavior.

## Deliverable 6: Replace the launcher echo requirement with explicit data-phase modes

Extend the versioned launcher scenario with an explicit data-phase mode selected from a narrow enum.

Recommended modes:

```text
send-one-delivery-status
receive-one-delivery-status
send-then-receive
handshake-only-negative
```

The exact enum may change after the pinned-source audit, but it must express directional intent.

### Send mode

For i2pr sending to a reference:

- complete authentication;
- encode one bounded selected I2NP message;
- queue and write exactly one authenticated NTCP2 frame containing that message;
- emit typed local counters for frame and I2NP send completion;
- remain alive long enough for the reference-side oracle to observe acceptance;
- do not require an unsolicited response.

### Receive mode

For a reference sending to i2pr:

- complete authentication;
- wait within a bounded deadline for one authenticated frame;
- parse the selected I2NP message;
- emit typed frame and I2NP receive counters;
- reject padding-only, termination-only, malformed, oversized, or wrong-type input.

### Send-then-receive mode

Use only if the pinned-source audit establishes a protocol-valid request/response behavior. Do not use it merely to preserve the current implementation shape.

### Runtime ownership

Keep socket and deadline execution in `i2pr-runtime`. The launcher may select the mode and translate typed outcomes, but it must not reimplement runtime I/O loops outside the runtime crate.

Add tests for:

- successful send-only local peer;
- successful receive-only local peer;
- wrong message type;
- malformed frame;
- read timeout;
- write timeout;
- cancellation before and after authentication;
- queue admission failure;
- cleanup after every terminal path.

## Deliverable 7: Implement real reference triggers

Replace pending trigger objects with executable implementations selected from the source audit.

Each trigger must:

- receive the adapter, namespace, peer RouterInfo hash or public endpoint, and bounded timeout explicitly;
- execute only after the intended responder reports readiness;
- return a typed result with `attempted`, `observed`, `kind`, and fixed reason code;
- reject unsupported or disabled control surfaces before starting the mixed run;
- never expose raw control responses in retained evidence;
- never be considered successful solely because a command returned zero.

The mixed runner must call the trigger when the reference is the initiator.

If automatic dialing is used instead:

- prove it deterministically on the pinned build;
- record the exact authoritative state transition;
- do not issue the fallback trigger unless auto-dial fails within a shorter bounded interval;
- record whether auto-dial or explicit trigger caused the attempt.

Add tests proving the runner invokes the trigger exactly once when required and never when i2pr is the initiator.

## Deliverable 8: Implement real data-phase observations

Replace `*-pending` oracle values with executed observations.

For every direction, the oracle must produce:

```text
sender_observed = true
receiver_observed = true
```

before the scenario may pass.

The two observations must be independent:

- sender observation: the sending implementation completed authenticated-frame transmission of the selected I2NP message;
- receiver observation: the receiving implementation accepted, authenticated, decrypted, framed, and parsed the selected I2NP message.

Do not treat these as receiver proof:

- TCP bytes received;
- connection remains open;
- generic log text;
- frame count without authenticated parsing;
- queued-but-not-written local data;
- an echo generated by the test harness itself.

Return only fixed observation codes and numeric counters to the evidence boundary.

The mixed runner must reject:

- unsupported oracle;
- unprobed oracle;
- sender false;
- receiver false;
- mismatch between launcher mode and oracle kind;
- counter values inconsistent with the observation;
- oracle success after authentication or process failure.

## Deliverable 9: Correct mixed-runner success and error semantics

Refactor `mixed_runner.py` around one explicit directional state machine.

Recommended phases:

```text
validated
host_checked
cache_verified
topology_ready
local_state_prepared
peer_state_prepared
router_infos_validated
peer_imported
responder_ready
initiator_triggered
authenticated_both_sides
data_sender_observed
data_receiver_observed
processes_stopped
topology_destroyed
evidence_finalized
```

A phase may advance only after its invariant holds.

### Required pass predicate

A positive direction may be `passed` only when all are true:

- exact reference cache metadata validated;
- live identity continuity validated for both sides;
- both RouterInfos validated and bound where applicable;
- responder readiness observed;
- initiator attempt explicitly observed;
- i2pr authentication observation true;
- reference authentication observation true;
- sender observation true;
- receiver observation true;
- required counters nonzero and internally consistent;
- both processes stopped or joined within bounds;
- topology and owned host state removed;
- evidence record finalized successfully.

### Typed adapter interfaces

Use `authenticated_observation()` or another typed adapter method. Do not reach through adapters to arbitrary process log methods in the runner.

Unknown methods, malformed adapter returns, or unexpected exceptions must become a fixed typed harness failure without exposing a traceback or raw exception string.

Do not catch and discard programmer errors so broadly that tests cannot detect them. Use a narrow internal exception boundary and a final CLI boundary that emits one sanitized terminal record.

### Cleanup and evidence separation

If protocol execution passed but evidence writing failed:

- result must be `rejected` or a dedicated `evidence_failed` class allowed by the schema;
- cleanup remains `clean` when cleanup actually succeeded;
- do not rewrite the condition as `failed_cleanup`.

If cleanup failed, cleanup failure remains terminal regardless of protocol result.

## Deliverable 10: Add a dedicated mixed-router evidence schema

Do not continue overloading schema 1 if it cannot represent the required proof cleanly.

Introduce a versioned mixed-router schema, preferably schema 3, containing only sanitized fields.

Recommended fields:

```text
schema
scenario_id
date_utc
i2pr_commit
reference
reference_version
reference_revision
reference_build_kind
reference_instrumentation_patch_sha256
reference_artifact_sha256
reference_installed_tree_sha256
i2pr_configuration_sha256
reference_configuration_sha256
namespace_topology_sha256
i2pr_router_identity_sha256
reference_router_identity_sha256
i2pr_router_info_sha256
reference_router_info_sha256
direction
address_family
trigger_kind
trigger_observed
authenticated_observations
data_phase_mode
data_phase_oracle
data_phase_observations
resource_counters
process_counters
expected
actual_typed_result
cleanup_result
evidence_sha256
known_deviation
reproduction
```

The final field set may be adjusted, but it must retain enough information to audit the pass predicate.

### Hash requirements

For passed records, require nonzero SHA-256 values for:

- reference artifact;
- reference installed tree;
- i2pr configuration;
- reference configuration;
- topology;
- both public router identities;
- both RouterInfos;
- instrumentation patch when `reference_build_kind = instrumented`.

`configuration_sha256` must be computed from canonical rendered configuration bytes, not a placeholder or hash of another digest string.

### Observation requirements

For passed records require:

```text
authenticated_observations.i2pr = authenticated
authenticated_observations.reference = authenticated
data_phase_observations.sender = observed
data_phase_observations.receiver = observed
trigger_observed = true
```

For i2pr-initiated runs, `trigger_kind` may identify the launcher dial action and must still be typed.

### Sanitation

Continue rejecting:

- raw endpoints;
- home/root paths;
- RouterInfo contents;
- I2NP contents;
- private keys;
- static private keys;
- long encoded blobs;
- logs and packet captures;
- free-form exception or control-response strings.

Add positive and mutation tests for every required field and pass predicate.

## Deliverable 11: Reconcile gate catalogs from one source of truth

Remove duplicated scenario allowlists from shell regexes and Python aggregate tables.

Create one machine-readable gate catalog, for example:

```text
tests/integration/ntcp2/gates.toml
```

It must define:

- gate name;
- scenario IDs;
- runner type;
- allowed result classes;
- required predecessor gates;
- whether IPv6 skip is allowed;
- whether the gate may produce schema 1, 2, or 3 evidence.

Have these consumers load or validate against that catalog:

- `scripts/interop/run-matrix.sh`
- `scripts/interop/run-gate.sh`
- `tests/integration/ntcp2/harness/build_gate.py`
- `scripts/interop/aggregate-evidence.py`
- `scripts/interop/validate-scenarios.py`
- workflow static contract checks.

Shell may call a small Python catalog query utility rather than duplicating regular expressions.

### Required handshake-smoke catalog

The handshake-smoke gate must contain exactly:

```text
i2pr-to-java-ipv4
java-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
i2pd-to-i2pr-ipv4
```

### Full-profile disposition

The current legacy full scenarios cannot continue to run through `runner.py` when that runner supports environment smoke only.

Choose one honest disposition:

1. Migrate every full scenario to an executable directional mixed-runner scenario with real assertions.
2. Remove `full` from workflow choices and mark it `blocked_full_matrix_not_implemented` until a later plan.

For Plan 045, option 2 is acceptable and preferred unless all negative/resource scenarios are genuinely implemented. Do not preserve an impossible green path.

The Plan 045 closure criterion is the four-direction handshake-smoke proof, not the entire adversarial matrix.

### Unknown identifiers

`reference_for()` must reject an unknown scenario rather than defaulting to i2pd.

Prefer reading the canonical reference directly from the gate/scenario catalog.

## Deliverable 12: Strengthen gate archival and aggregate validation

Retain the per-gate staging design, but update it for schema 3 and canonical gate catalogs.

Required behavior:

- validate every staged record before moving it;
- ensure its scenario belongs to the active gate;
- ensure its evidence schema is allowed for that gate;
- refuse duplicate scenario records;
- refuse filename collisions;
- preserve earlier gate records byte-for-byte;
- reject missing expected scenarios;
- reject unexpected extra scenarios;
- reject passing aggregate disposition when any required observation is false;
- clean staging on all exits;
- keep cleanup verification independent.

Pass record paths to Python utilities as arguments. Do not interpolate file paths into inline Python source.

## Deliverable 13: Add end-to-end simulated composition tests

Component unit tests are not enough. Add process-level simulated tests using fake reference and launcher processes that exercise the real runner sequencing without privileges.

Required simulated cases:

- all four positive directions;
- identity changes between preparation and live start;
- responder scenario rejected by Rust parser;
- reference trigger unavailable;
- trigger returns unobserved;
- i2pr authenticates but reference does not;
- reference authenticates but i2pr does not;
- sender observed but receiver not observed;
- receiver observed but sender not observed;
- zero configuration digest;
- malformed launcher terminal status;
- duplicate terminal status;
- scenario-ID mismatch;
- adapter method failure;
- evidence write failure with clean cleanup;
- process stop timeout;
- topology cleanup failure;
- gate staging with all four mixed IDs;
- unknown scenario/reference rejection.

These tests must assert the exact typed result and cleanup classification.

## Deliverable 14: Local validation ladder

Before any privileged Ubuntu run, all of these must pass:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
python3 scripts/interop/validate-scenarios.py
python3 scripts/interop/validate-build-contract.py
python3 scripts/interop/validate-evidence.py
bash -n scripts/check-ntcp2-interoperability.sh scripts/interop/*.sh scripts/interop/lib/*.sh scripts/interop/ubuntu/*.sh
git diff --check
```

Add a deterministic command that renders all four launcher scenarios and runs each through the real Rust parser without opening sockets.

Add a deterministic command that prints the gate catalog expansion and confirms the handshake-smoke set is exactly four directional IDs.

## Deliverable 15: Authorized Ubuntu execution sequence

Run only on an authorized disposable Ubuntu 24.04 amd64 environment with noninteractive sudo.

Execute in this order:

```text
1. static/local validation
2. pre-install host contract
3. reset generated lane state
4. host setup
5. post-install host contract
6. clean-host baseline
7. pinned reference builds
8. cache manifest verification
9. offline cache reuse
10. Java environment smoke
11. i2pd environment smoke
12. Java-to-i2pd reference control
13. i2pd-to-Java reference control
14. i2pr-to-Java mixed direction
15. Java-to-i2pr mixed direction
16. i2pr-to-i2pd mixed direction
17. i2pd-to-i2pr mixed direction
18. evidence validation
19. handshake-smoke aggregate generation
20. cleanup
21. clean-host verification
```

Stop immediately when either reference-control direction fails. Do not introduce i2pr as the variable under test without a passing control.

Stop after any mixed-direction failure and retain only sanitized failed evidence when explicitly permitted by the schema.

Do not run the full profile during Plan 045 unless it was completely migrated and locally validated under Deliverable 11.

## Deliverable 16: Repeatability requirements

After the first all-green handshake-smoke run, repeat:

1. a second run using the same verified offline caches;
2. a run from a fresh checkout using transferred cache artifacts and verified manifests;
3. a post-reboot run on the same class of disposable host.

Require the same four directional dispositions and no residual host state.

Exact timestamps and ephemeral run IDs may differ. Reference revisions, artifact hashes, configuration hashes, scenario IDs, trigger kinds, oracle kinds, and expected observation classes must remain stable.

## Expected files to modify

Likely implementation surfaces include:

- `plans/044-closure.md`
- `tools/i2pr-interop/src/main.rs`
- `tools/i2pr-interop/src/scenario.rs`
- `tools/i2pr-interop/src/status.rs`
- `crates/i2pr-runtime/src/ntcp2.rs` or adjacent runtime-owned link code
- `tests/integration/ntcp2/harness/mixed_runner.py`
- `tests/integration/ntcp2/harness/launcher_renderer.py`
- `tests/integration/ntcp2/harness/launcher_protocol.py`
- `tests/integration/ntcp2/harness/data_oracle.py`
- `tests/integration/ntcp2/harness/reference_trigger.py`
- `tests/integration/ntcp2/harness/i2pr.py`
- `tests/integration/ntcp2/harness/java_i2p.py`
- `tests/integration/ntcp2/harness/i2pd.py`
- `tests/integration/ntcp2/harness/evidence.py`
- `tests/integration/ntcp2/harness/build_gate.py`
- `tests/integration/ntcp2/harness/test_harness.py`
- `tests/integration/ntcp2/gates.toml`
- `scripts/interop/run-matrix.sh`
- `scripts/interop/run-gate.sh`
- `scripts/interop/aggregate-evidence.py`
- `scripts/interop/validate-evidence.py`
- `scripts/interop/validate-scenarios.py`
- `scripts/interop/validate-build-contract.py`
- `.github/workflows/ntcp2-interop-ubuntu.yml`
- `tests/integration/ntcp2/README.md`
- `tests/integration/ntcp2/evidence/README.md`
- `docs/architecture/interop-apparatus.md`
- `docs/protocol-support.md`
- `specs/CONFORMANCE.md`

Do not make unrelated daemon, NetDB, tunnel, SAM, I2CP, SSU2, or service-layer changes.

## Recommended implementation sequence

### Phase A: Correct status and schemas

- amend Plan 044 status;
- remove optional-field sentinels;
- add cross-language fixtures;
- define the mixed evidence schema;
- define the gate catalog.

### Phase B: Correct identity lifecycle

- refactor adapters into prepare/start/restart operations on one state root;
- add i2pr prepare/export support;
- add public identity-continuity digests;
- correct RouterInfo paths and strict validation.

### Phase C: Audit and implement trigger/oracle mechanisms

- inspect pinned source;
- write the capability record;
- choose stock or instrumentation-backed mechanisms;
- implement real triggers and observations;
- update private reference configurations only as required and only namespace-locally.

### Phase D: Correct launcher data modes

- add explicit directional data-phase modes;
- implement send-only and receive-only runtime paths;
- add typed status counters and local tests.

### Phase E: Rebuild the mixed runner pass predicate

- use one instance per side;
- invoke triggers;
- require dual authentication;
- require sender and receiver observations;
- populate real hashes and counters;
- separate evidence failure from cleanup failure.

### Phase F: Reconcile gates and workflow

- source all allowlists from the gate catalog;
- fix handshake-smoke archival;
- reject unknown references;
- disable or migrate the full profile;
- update aggregate validation.

### Phase G: External proof

- run environment smoke;
- run both reference controls;
- run four mixed directions;
- validate and aggregate evidence;
- repeat from offline caches and clean environments.

## Commit strategy

Use small reviewable commits. Recommended sequence:

1. `docs: reopen Plan 044 mixed proof status`
2. `interop: unify Rust and Python launcher scenario schema`
3. `interop: add canonical gate and mixed evidence schemas`
4. `interop: preserve router identity across mixed-run lifecycle`
5. `interop: add i2pr state preparation and RouterInfo export`
6. `interop: document pinned reference trigger and oracle capabilities`
7. `runtime: add directional NTCP2 data-phase modes`
8. `interop: implement reference triggers and data observations`
9. `interop: enforce mixed-run proof predicate and real evidence`
10. `interop: reconcile gate execution and archival`
11. `tests: add mixed composition and failure-path coverage`
12. `docs: record Plan 045 local validation status`
13. external-run corrections only when directly supported by observed typed failures.

Do not combine protocol/runtime changes, reference instrumentation, shell orchestration, and documentation into one opaque commit.

## Stop conditions

Stop and report a typed blocker rather than weakening requirements when:

- the live process cannot reuse the state that produced its RouterInfo;
- Python and Rust cannot agree on one exact scenario schema;
- the pinned reference source does not expose a trustworthy trigger or observation path;
- a proposed reference patch changes transport behavior rather than observability/test injection;
- the reference control cannot authenticate in the private network;
- the reference ignores or rewrites safety-critical configuration;
- either side lacks an authoritative authentication observation;
- sender or receiver data observation remains pending or false;
- a passed record would contain a zero or placeholder digest;
- cleanup leaves a namespace, interface, process, key, identity, RouterInfo, raw log, or private run root;
- the host contract is not exact Ubuntu 24.04 amd64;
- execution would require public I2P connectivity.

## Plan 045 acceptance criteria

Plan 045 is complete only when all of the following are true:

### Local implementation

- Plan 044 status is corrected.
- One persisted instance root is used per live router side.
- Public identity/static-key/RouterInfo continuity is verified across any restart.
- i2pr RouterInfo is exported from the actual state path and strictly validated.
- Python and Rust accept and reject the same scenario fixtures.
- Responder scenarios omit peer fields and start successfully in local parser/execution tests.
- Reference triggers are implemented and invoked.
- Data-phase observations are implemented and no pending placeholder can pass.
- Launcher directional data modes no longer require an unspecified echo.
- Mixed pass requires dual authentication and dual sender/receiver observations.
- Mixed evidence contains real nonzero configuration and public identity hashes.
- Evidence-write failure and cleanup failure remain distinct.
- Gate allowlists come from one canonical catalog.
- Handshake-smoke contains exactly four mixed directional scenarios.
- Unknown scenarios and references fail closed.
- The full profile is either genuinely migrated or removed/blocked honestly.
- All local validation commands pass.

### External proof

- Java environment smoke passes.
- i2pd environment smoke passes.
- Java-to-i2pd control passes.
- i2pd-to-Java control passes.
- i2pr-to-Java passes.
- Java-to-i2pr passes.
- i2pr-to-i2pd passes.
- i2pd-to-i2pr passes.
- Every mixed record contains dual authentication and dual data observations.
- Evidence validation passes.
- Handshake-smoke aggregate validation passes.
- Cleanup and clean-host verification pass.
- The offline-cache repetition passes.
- The fresh-checkout repetition passes.
- The post-reboot repetition passes.

### Documentation

- `plans/045-closure.md` records exact commands, commit IDs, runner contract, gate outcomes, evidence filenames and SHA-256 digests, and remaining blockers.
- `docs/protocol-support.md` remains experimental and non-advertised unless a separate Milestone 3 review changes it.
- No documentation claims full adversarial-matrix completion unless the full profile was actually migrated and executed.

## Handoff note

The implementing agent should not treat the current `data_oracle.py` and `reference_trigger.py` classes as completed mechanisms. They are design placeholders until their methods perform real source-verified operations and return true authoritative observations.

The highest-priority correction is identity continuity. Do not spend time debugging handshake cryptography against Java I2P or i2pd until the live process is proven to own the exact identity, static key, obfuscation IV, and RouterInfo used by its peer.

After identity continuity and schema parity are corrected, prove the reference-only control. Only then introduce i2pr and debug the four directional mixed runs one at a time.
