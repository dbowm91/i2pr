# Plan 082: i2pr state preparation and mixed-runner contract correction

## Status and dependencies

- Status: planned, next executable plan.
- Parent roadmap: Plan 081.
- Requires: Plans 076 and 080 remain valid as driver/lane prerequisites.
- Blocks: Plans 083, 084, and 079.
- Plan type: pre-protocol launcher and harness correction.

## Objective

Break the circular dependency in the current mixed runner by preparing i2pr identity, NTCP2 static material, and signed RouterInfo before the strict live scenario is rendered.

This plan must produce authentic values for:

```text
i2pr_router_info_sha256
i2pr_router_hash_sha256
i2pd_router_info_sha256
i2pd_router_hash_sha256
run_identity_sha256
delivery_status_message_id
```

It must then render a valid Plan 065 live scenario without weakening the schema or inventing a generation scenario ID.

Plan 082 does not require a mixed-router pass. It ends before protocol qualification.

## Current defects to eliminate

### D1. Empty strict fields

The current mixed runner calls `_plan065_primary_fields()` with empty Router Hashes and an empty run-identity digest. `launcher_renderer.py` correctly rejects these values.

### D2. Fake generation scenario

The reverse-direction path attempts to render `<primary-id>-gen`. The Plan 065 parser correctly rejects that unallowlisted live-scenario ID.

### D3. Preparation hidden behind live listen/dial

The Rust launcher already has the code required to create/load identity, create/load NTCP2 static material, sign RouterInfo, persist `state/router.info`, and verify the exact local NTCP2 address. It is only reachable after a live scenario has already passed strict parsing.

### D4. Generic failure collapse

The runner maps scenario-render, preparation, launcher-start, RouterInfo-export, and local file errors to `typed-harness-operation-failed`.

### D5. Untruthful process counts

Process counters may use a fallback that records `started = 1` even when process creation did not succeed.

### D6. Evidence finalization can hide the primary result

The broad finalization path may replace the original failure with `evidence-finalization-failed`. Plan 082 must make the pre-protocol result independently inspectable.

## Work packages

### WP1. Add a dedicated preparation command

Preferred CLI:

```text
i2pr-interop ntcp2 prepare \
  --state-dir <absolute-or-confined-path> \
  --local-address <synthetic-ip> \
  --local-port <port> \
  --network-id 99 \
  [--deterministic-seed <u64>]
```

An equivalent command shape is acceptable when it remains test-only and bounded.

Implementation guidance:

- add a `Prepare` variant under `Ntcp2Command`;
- define a small preparation input type rather than reusing the full live `Scenario` parser;
- enforce the existing synthetic IPv4/IPv6 ranges and private network ID 99;
- require a confined state path and strict directory/file permissions;
- call the existing local-state preparation logic or extract a shared helper from it;
- preserve the current identity, static-key, RouterInfo-signing, address-binding, and verification semantics;
- do not bind TCP, open a listener, dial, import a peer RouterInfo, or create a runtime service;
- do not add preparation IDs to `REFERENCE_DRIVER_MODE_BY_DIRECTION` or the primary scenario allowlist.

Suggested bounded stdout record:

```json
{
  "schema": "i2pr-interop-state-prepared-v1",
  "result": "prepared",
  "router_hash_sha256": "<64 lowercase hex>",
  "router_info_sha256": "<64 lowercase hex>",
  "ntcp2_address_count": 1
}
```

Allowed rejection reasons should be fixed and redacted, for example:

```text
prepare_input_invalid
prepare_state_path_invalid
prepare_identity_failed
prepare_static_key_failed
prepare_router_info_sign_failed
prepare_router_info_write_failed
prepare_router_info_verify_failed
prepare_endpoint_binding_failed
```

Do not emit filesystem paths, private keys, RouterInfo bytes, or arbitrary error text.

### WP2. Add `I2prAdapter.prepare_state()`

Add a bounded adapter method with an explicit endpoint input.

Required behavior:

1. invoke the preparation command through the selected `ProcessPlacement`;
2. capture and parse exactly one preparation record;
3. require process exit success and `result = prepared`;
4. verify `state/router.info` exists inside the run root;
5. run the existing strict RouterInfo validation against the expected synthetic endpoint;
6. hash the exact RouterInfo bytes;
7. derive or confirm the 64-hex Router Hash;
8. return a typed preparation result object;
9. increment process counters only after the process was successfully created.

Suggested Python result shape:

```python
@dataclass(frozen=True)
class PreparedI2prState:
    router_info_path: Path
    router_info_sha256: str
    router_hash_sha256: str
    ntcp2_address_count: int
```

Do not infer the Router Hash from the RouterInfo file name. Use the preparation command's verified result or a shared exact RouterIdentity-hash parser.

### WP3. Prepare both peer identities before live rendering

Correct order for either direction:

```text
allocate run root and endpoints
prepare i2pr state
prepare i2pd state through the Plan 076 driver
validate both RouterInfos
obtain both Router Hashes
copy peer RouterInfo into each expected exchange/import location
freeze run identity
render live scenario
```

The implementation may use a common helper such as:

```python
@dataclass(frozen=True)
class PreparedPeerPair:
    i2pr: PreparedI2prState
    reference_router_info_path: Path
    reference_router_info_sha256: str
    reference_router_hash_sha256: str
    run_identity_sha256: str
```

Do not create a generalized plugin framework.

### WP4. Freeze a canonical run-identity record

Before any live listener/dial process starts, write one small canonical JSON record using sorted keys and compact separators.

Minimum fields:

```text
schema = i2pr-minimal-run-identity-v1
run_id
source_commit
direction
reference = i2pd
reference_revision
topology_kind
lane_qualification_sha256
i2pr_binary_sha256
i2pd_binary_sha256
i2pr_router_info_sha256
i2pd_router_info_sha256
i2pr_router_hash_sha256
i2pd_router_hash_sha256
delivery_status_message_id
```

Calculate `run_identity_sha256` from the exact bytes after the record is closed. Do not include `run_identity_sha256` inside the bytes being hashed.

The record is diagnostic provenance, not a release certificate.

### WP5. Render the strict live scenario with real values

Populate:

```text
expected_sender_router_hash_sha256 = actual local i2pr Router Hash
expected_receiver_router_hash_sha256 = actual i2pd Router Hash
run_identity_sha256 = frozen record digest
reference_driver_mode = i2pd-direct-driver
delivery_status_message_id = derived nonzero value
```

Retain the strict schema rules:

- no zero or empty digests;
- no self-reference;
- exact direction-to-driver mapping;
- exact primary scenario ID;
- confined paths;
- network ID 99;
- synthetic address range only.

Do not weaken validation to make preparation easier.

### WP6. Remove the `-gen` live-scenario path

Delete or replace all preparation logic that renders `<scenario>-gen`.

The new preparation command is the only i2pr state-generation authority for this lane.

Add a static/focused test proving that no primary path constructs a live scenario ID ending in `-gen`.

### WP7. Introduce precise pre-protocol failure categories

At minimum distinguish:

```text
i2pr-state-preparation-failed
i2pr-preparation-record-invalid
i2pr-router-info-missing
i2pr-router-info-validation-failed
i2pr-router-hash-invalid
reference-state-preparation-failed
reference-router-info-validation-failed
reference-router-hash-invalid
run-identity-freeze-failed
live-scenario-render-failed
listener-process-start-failed
dialer-process-start-failed
```

The sanitized record stores only the fixed reason code.

When `I2PR_INTEROP_DIAGNOSTICS=raw-local`, preserve the exception class and stack trace under the disposable run root. Do not copy raw diagnostics into an export/evidence directory.

### WP8. Correct process accounting

A process counter increments `started` only after the underlying subprocess creation succeeds.

Required tests:

- render failure -> `started = 0` for both peers;
- i2pr preparation start failure -> `i2pr.started = 0`;
- i2pr preparation exits rejected after creation -> preparation process start is recorded separately or explicitly, but live i2pr process remains zero;
- live i2pr start success -> live counter becomes one;
- no `or 1` or equivalent fallback may fabricate a start.

Keep preparation-process counters separate from live protocol process counters when both are recorded.

### WP9. Add a pre-protocol self-check command

Provide a focused command or test helper that stops before peer protocol execution and proves:

```text
both RouterInfos exist and validate
both Router Hashes are nonzero 64-hex and distinct
run identity is frozen and nonzero
strict live scenario parses in Python
strict live scenario parses in Rust or reaches launcher state preparation
no listener/dial peer connection is attempted
```

This may be a Python unit/integration test using the real i2pr preparation binary and a real i2pd inspect/preparation output where the cache exists.

## Suggested files

Expected touched files are narrow:

```text
tools/i2pr-interop/src/main.rs
tools/i2pr-interop/src/scenario.rs only if shared validation extraction is needed
tools/i2pr-interop/src/status.rs only for bounded preparation status types
tests/integration/ntcp2/harness/i2pr.py
tests/integration/ntcp2/harness/mixed_runner.py
tests/integration/ntcp2/harness/launcher_renderer.py only if no behavior weakening occurs
tests/integration/ntcp2/harness/test_i2pr_prepare.py
tests/integration/ntcp2/harness/test_plan082.py
scripts/check-ntcp2-interoperability.sh only for narrow boundary assertions
plans/082-status.md
```

Avoid changes to production daemon, NetDB, tunnel, SAM/I2CP, SSU2, or release code.

## Acceptance criteria

Plan 082 closes only when:

- the test-only preparation command creates a signed, endpoint-bound RouterInfo without opening a socket;
- repeated preparation against the same state preserves identity/static material as expected;
- fresh-state preparation produces a valid nonzero Router Hash and RouterInfo digest;
- `I2prAdapter.prepare_state()` validates and returns authentic values;
- the real Plan 076 i2pd preparation path yields a valid RouterInfo and Router Hash;
- a canonical run identity is frozen before live launch;
- a strict primary scenario renders with real values and parses successfully;
- no `-gen` live scenario remains on the active path;
- broad pre-protocol failure collapse is removed for the listed stages;
- live process counters remain zero when rendering/preparation fails;
- no interoperability pass is claimed;
- `plans/082-status.md` records exact commands, focused tests, and any remaining blocker.

## Validation

Use a focused baseline:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test -p i2pr-interop
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pr_prepare.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan082.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_harness.py'
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Do not run the entire historical qualification matrix unless a shared schema/evidence module is changed.

## Stop rules

Stop and record a typed blocker when:

- state preparation would require activating `i2pr-daemon`;
- the only proposed fix weakens live scenario correlation fields;
- RouterInfo creation cannot be separated from socket execution without changing crypto/protocol semantics;
- the i2pd prepared RouterInfo no longer validates, indicating Plan 076 drift;
- the Plan 080 lane contract is invalid and cannot be refreshed through its existing scripts;
- work expands into NetDB, public-network bootstrap, tunnels, or a new reference implementation.

## Non-goals

Plan 082 does not:

- prove NTCP2 interoperability;
- run Java or Emissary;
- redesign the Plan 076 i2pd driver;
- create a new VM/container lane;
- weaken Plan 065 strictness;
- rework release evidence;
- enable NTCP2 in the daemon;
- add CI.

## Small-model execution guidance

1. Implement the Rust preparation command first.
2. Test it directly with a temporary state directory.
3. Add the Python adapter method and focused tests.
4. Add the canonical run-identity helper.
5. Replace empty values in one direction only.
6. Remove the `-gen` path.
7. Add precise errors and truthful counters.
8. Stop before running i2pd over the wire.

Do not combine Plan 082 and Plan 083 in one implementation pass. Plan 082 must have its own closure record and commit.