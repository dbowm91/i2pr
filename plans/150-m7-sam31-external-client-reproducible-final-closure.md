# Plan 150 — SAM 3.1 reproducible external-client interoperability and final Milestone 7 closure

Status: **next executable after Plan 149 local-product closure**.

Depends on:

- Plan 146 private-destination reference compatibility;
- Plan 149 self-composing localhost SAM product and its documented local raw-path acceptance subset;
- Plan 139 FORWARD/naming implementation;
- Plan 134 Milestone 6 local product closure.

Supersedes for next-action/closure authority:

- Plan 148's blocked external-client attempt and its invalid/stale client-provisioning assumptions.

Plan 148 remains historical evidence showing that two copies of one Rust helper are not independent clients and that external-client evidence cannot be replaced by in-repo transcript helpers.

## 1. Goal

Close Milestone 7 with reproducible evidence from **at least two independently implemented SAM clients** against the real i2pr loopback listener after Plan 149 proves the product self-composes without hidden daemon setup.

This plan is intentionally validation-heavy and implementation-light. If a real external client exposes a narrow SAM 3.1 defect, fix it here. If it exposes another product-composition or M6 architectural defect, stop and write one narrow corrective plan rather than weakening the gate.

Milestone 7 closure means:

```text
SAM 3.1 localhost application interface
 + private destination compatibility
 + STREAM CONNECT/ACCEPT raw bytes
 + FORWARD/naming supported surface
 + bounded lifecycle/resource behavior
 + at least two independent client implementations
```

It does **not** mean:

```text
public I2P participation
router-to-router NTCP2/SSU2 interoperability
mixed-router tunnel interoperability
SAM 3.2 support
```

## 2. Correct the external-client provenance before execution

The Plan 148 client table is not reliable enough to use as-is.

### 2.1 libsam3

The previously recorded `e0da4f4d8d3ca670fef86fd1046dab7c14afc5b7` / `v1.0.0` pin does not resolve in the official `i2p/libsam3` GitHub repository.

Current verified official candidates include:

- official repository: `https://github.com/i2p/libsam3`;
- tag `v0.31.2` -> commit `ea52a3251d60906d67f9a1031a6ed7642753f94f`;
- current official master snapshot observed during planning: `7d6e658798baec31394c5685f9583343cc00900b` (2026-01-14), which includes a destination-key newline validation fix after `v0.31.2`.

Preferred Plan 150 pin:

```text
libsam3 = 7d6e658798baec31394c5685f9583343cc00900b
```

Reason: it is a concrete current official revision and includes the post-release key-validation fix. If execution intentionally chooses `v0.31.2` instead, record the reason and exact commit; do not invent a `v1.0.0` tag.

libsam3 is a C implementation and its repository documents the copy/build model under `src/libsam3` plus examples.

### 2.2 i2psam

Use the official I2P C++ SAM v3.1 implementation as the preferred second client:

```text
repository = https://github.com/i2p/i2psam
pin        = b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
```

That official revision is a SAM v3.1 C++ library whose normal repository build is `make` / `make eepget`.

The exact pin must be re-resolved at execution time only to confirm it still exists; do not silently advance it.

### 2.3 i2plib — supplementary, not mandatory

Retain i2plib as useful third-party Python evidence, but do not make it one of the two mandatory clients unless its runtime is qualified.

Known pin:

```text
repository = https://github.com/l-n-s/i2plib
pin        = 6edf51cd5d21cc745aa7e23cb98c582144884fa8
version    = 0.0.14
```

This is its 2019 final commit. Its high-level async implementation passes the now-removed `loop=` argument to `asyncio.open_connection()`, so modern Python may reject the unmodified high-level API.

Acceptable supplementary uses:

- run it under a deliberately pinned compatible Python version if that environment remains available;
- use its independent `i2plib.sam` message/Base64 parser/encoder surface while a thin harness owns the socket lifecycle, **provided the evidence is labeled exactly that way**.

Do not patch i2plib source to make i2pr pass.

### 2.4 Required independence

The two mandatory clients must be:

- separate repositories/codebases;
- independently implemented from i2pr;
- built without linking any i2pr crate;
- unmodified for i2pr-specific behavior.

`libsam3` + `i2psam` satisfies this minimum even though both are hosted by the I2P organization: they are distinct C and C++ implementations. i2plib is a useful third implementation when practical.

## 3. Execution environment strategy

Do not block closure on the constrained implementation host's inability to fetch arbitrary source at runtime.

Provide two equivalent execution lanes.

### Lane A — preferred reproducible GitHub-hosted Ubuntu workflow

Add a manual workflow, for example:

```text
.github/workflows/sam-interop.yml
```

Trigger:

```yaml
on:
  workflow_dispatch:
```

The workflow should:

1. check out i2pr at the commit being validated;
2. install the pinned Rust toolchain already used by CI;
3. check out/fetch the exact external-client revisions into an ephemeral directory;
4. build only the needed client libraries/runners;
5. build/start i2pr's real SAM listener in the same unprivileged runner;
6. run the Plan 150 client matrix over `127.0.0.1` only;
7. produce a sanitized machine-readable evidence artifact;
8. never commit or upload raw private destination material/application secrets.

No root, sudo, namespace, Docker, VM, systemd, or public I2P access is required.

The external repositories may be checked out into an ephemeral `.interop/` path in the job workspace or `$RUNNER_TEMP`; they must never be committed into the i2pr tree.

### Lane B — local/pre-cached execution

Support the same runners against explicit environment variables or paths:

```text
I2PR_LIBSAM3_SRC=/path/to/pinned/libsam3
I2PR_I2PSAM_SRC=/path/to/pinned/i2psam
I2PR_I2PLIB_SRC=/path/to/pinned/i2plib   # optional
```

or a documented `target/interop/cache/sam/...` structure.

The local lane must verify the source revision before build. A directory with the wrong commit is a hard error, not a silent fallback.

## 4. Keep the harness small

Preferred layout:

```text
tests/integration/sam/
  README.md
  run-independent.sh
  evidence.schema.json or evidence-format.md
  clients/
    libsam3_runner.c
    i2psam_runner.cpp
    i2plib_runner.py          # optional/supplementary
```

The runners are **harness code**, not alternate SAM implementations. They should call the selected client's normal public API as directly as possible.

Rules:

- no large Python orchestration framework;
- no subprocess tree deeper than necessary;
- no vendored third-party source;
- no patches to external clients;
- deterministic temp directories/ports;
- explicit process deadlines;
- always kill/join child processes on failure;
- sanitize logs before artifact upload.

If one external client's public API does not expose a particular optional check, use a small independent raw transcript runner only for that check; do not count that transcript runner as one of the two required independent clients.

## 5. Product-start contract

The Plan 150 harness must start i2pr using the same listener/composition path that Plan 149's black-box test validated.

Forbidden external-lane setup:

```text
SamDestinations::install
build_sam_destination_bridge
install_remote_lease_set2
install_inbound_tunnel_factory
spawn_destination_driver
bridge_to_peer
```

No test executable or helper may call into i2pr private Rust APIs to create the stream trajectory.

The only behavior-driving interface is the SAM TCP listener.

## 6. Required client matrix

At minimum execute both directions:

```text
libsam3 CONNECT  -> i2psam ACCEPT
i2psam CONNECT   -> libsam3 ACCEPT
```

If one library's public API structurally cannot express one direction, document the API limitation and substitute a third independent client for that direction. Do not replace both sides with the same implementation.

Each client must demonstrate:

```text
HELLO VERSION MIN=3.1 MAX=3.1
DEST GENERATE SIGNATURE_TYPE=7 or imported compatible PRIV
SESSION CREATE STYLE=STREAM
STREAM CONNECT and/or STREAM ACCEPT
raw byte transfer
clean close
```

Record exact command/API names used.

## 7. Destination representation matrix

Reconfirm Plan 146 through the external clients without reopening the already-proven Java reference result.

Required:

- at least one mandatory client accepts an i2pr-generated `PRIV`/Destination representation through its normal session API;
- at least one mandatory client imports or derives a destination that i2pr accepts for `SESSION CREATE`;
- public Destination byte equality is recorded where the external API exposes it;
- canonical I2P Base64 `-` / `~` and padding survive unchanged.

Do not persist generated raw `PRIV` strings in evidence.

## 8. STREAM binary matrix

For both cross-client directions, exchange exact payloads containing:

- ASCII;
- LF / CRLF;
- NUL;
- invalid UTF-8;
- every byte value `0x00..0xff`;
- payload beginning with SAM-looking command text;
- payload larger than one negotiated Streaming packet;
- payload larger than the send window;
- simultaneous bidirectional traffic.

At least one mandatory-client trajectory must transfer a multi-megabyte logical payload.

Plan 149 proves the local bounded raw path and terminal cleanup. Plan 150
owns the remaining slow-peer, fault-matrix, sibling-stream, and
external-client compatibility evidence.

## 9. SILENT compatibility

Exercise byte-exact `SILENT=true/false` behavior with external clients when their public API exposes it.

Where a high-level library hardcodes non-silent behavior, use a tiny independent transcript runner for the missing SILENT variant and retain the two mandatory clients for the rest of the matrix.

Required transcript:

```text
CONNECT SILENT=false -> STATUS OK then raw
CONNECT SILENT=true  -> raw first, no success status
ACCEPT SILENT=false  -> STATUS OK, peer Destination line, raw
ACCEPT SILENT=true   -> raw first, no status/peer line
```

The transcript helper is supporting evidence only, not one of the two independent-client counts.

## 10. Multiple streams and lifecycle

Through the real listener and external clients:

- open at least two streams on one session where client APIs permit;
- transfer independent payloads;
- close one while sibling remains usable;
- abrupt stream close;
- graceful stream close;
- close control/session socket with child stream active;
- recreate a session after cleanup;
- repeat create/connect/close enough times to catch leaked registrations/tasks.

Use Plan 149 Rust tests for exact capacity/max+1 ceilings when a client library hides concurrency details.

## 11. STREAM FORWARD final acceptance

Plan 139's forwarding implementation must be revalidated on the Plan 149 self-composed path.

Use at least one mandatory independent client plus a loopback echo target.

Required:

```text
B creates STREAM session
B registers STREAM FORWARD to 127.0.0.1:<ephemeral target>
A CONNECTs to B's public Destination
SAM opens the configured target
```

Non-silent target receives the real authenticated A public Destination metadata specified by i2pr's supported SAM behavior, then raw bytes.

Silent target receives raw bytes first.

Verify:

- exact A -> target payload;
- exact target -> A response;
- second independent stream;
- target refusal;
- target timeout within existing policy;
- control socket close unregisters forwarding;
- ACCEPT/FORWARD mutual exclusion;
- non-loopback targets remain rejected.

No fabricated peer metadata is allowed.

## 12. NAMING final acceptance

Through an independent client or its normal NAMING API:

- full public Destination lookup round-trips exactly;
- `NAME=ME` returns the session destination in valid context;
- malformed full destination fails typed;
- unknown `.i2p` -> `KEY_NOT_FOUND` without DNS;
- locally owned `.b32.i2p` behavior matches documented local-only policy;
- no system DNS/address-book lookup is introduced;
- SAM 3.2 `OPTIONS=true` is not claimed unless separately implemented.

## 13. Negative compatibility matrix

Freeze externally observable rejection behavior for:

```text
HELLO MIN=3.2 MAX=3.3
SESSION CREATE STYLE=DATAGRAM
SESSION CREATE STYLE=RAW
SESSION CREATE STYLE=PRIMARY
STREAM CONNECT ... FROM_PORT=1
STREAM CONNECT ... TO_PORT=1
STREAM FORWARD ... SSL=true
NAMING LOOKUP ... OPTIONS=true
unknown command/action
```

Also cover malformed/duplicate options, overlong lines, malformed I2P Base64, and invalid session IDs.

No unsupported option may be silently accepted merely because an external client sends it.

## 14. Resource/privacy/fault regression floor

Plan 150 does not need to reimplement the deterministic fault injector. It
must rerun the Plan 149 self-composed acceptance suite and record it beside
the external-client results, while closing the carry-forward cases below.

Required local evidence in the same closure run:

- Plan 149 local bounded backpressure and terminal-cleanup evidence;
- slow-reader/slow-writer bounds at the final client boundary;
- loss/duplicate/reorder/ACK-drop/corruption/retransmit-ceiling tests;
- close/reset/sibling lifecycle at the final client boundary;
- default-log privacy assertions;
- parser/session/stream ceilings;
- SAM disabled-by-default and loopback-only policy.

External-client runs must not weaken any of those gates.

## 15. Milestone 6 regression floor

Explicitly rerun the retained Plan 127–134 focused trajectories, especially:

- destination session/routing;
- Streaming wire format;
- Plan 129 full local destination/Streaming product path;
- retransmission/ACK/window corrections;
- Plan 134 receive-window ACK ceiling.

Milestone 7 closure is invalid if SAM glue regresses M6.

## 16. Evidence format

Commit a concise sanitized result, for example:

```text
tests/integration/sam/evidence.md
```

or a generated JSON plus a human-readable summary.

Record:

```text
i2pr commit
runner OS/image
Rust toolchain
workflow run id or local execution identifier
SAM bind policy
libsam3 repository + exact commit + build command
i2psam repository + exact commit + build command
i2plib repository + exact commit/runtime if used
client A -> B result
client B -> A result
private-destination result
binary matrix result
SILENT result
multi-stream/lifecycle result
FORWARD result
NAMING result
negative matrix result
Plan 149 resource/fault/privacy result
Plan 127-134 regression result
known limitations
```

Never record:

- raw private destination strings;
- signing seeds/static private keys;
- application-secret payloads;
- unsanitized external process environment.

## 17. CI/workflow policy

The external-client workflow should be manual or otherwise non-blocking for routine PR CI unless maintainers later decide otherwise.

Recommended:

```text
workflow_dispatch = enabled
ordinary push/PR quality CI = unchanged
external-client fetch/build failures = fail the manual interop run
missing external clients on constrained local host = documented local inability, not a product failure
```

Do not add rootful containers, privileged runners, nested VMs, or network namespaces to this lane.

If GitHub-hosted workflow networking is unavailable, use a pre-cached artifact/source bundle created outside the implementation host and verify exact commit hashes before use. Do not relax the two-client requirement.

## 18. Documentation closure

At successful closure update at least:

```text
README.md
plans/README.md
plans/145-status.md
plans/148-status.md          # historical blocked audit / superseded
plans/149-status.md
plans/150-status.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
specs/support.toml
docs/protocol-support.md
docs/protocols/08-sam.md
docs/architecture/i2pr-daemon.md
docs/architecture/i2pr-client.md
docs/security-model.md
tests/integration/sam/README.md
```

Historical Plan 146–148 records remain audit history.

## 19. Final authority after a successful Plan 150

Only after every acceptance item passes should status become approximately:

```text
plan_146 = passed-sam31-private-destination-reference-requalification
plan_147_raw-driver-implementation = retained-passed
plan_148 = blocked-audit-superseded-by-plan149-150
plan_149 = passed-sam31-self-composing-local-product-corrective
plan_150 = passed-milestone7-sam31-external-client-final-closure

milestone7_local_product = passed
sam31_stream = implemented-and-localhost-client-interoperable
sam31_private_destination = reference-and-client-compatible
sam31_forward = implemented-with-loopback-target-policy
sam_independent_clients = at-least-two-passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = router-interoperability-retained-separately
next_product_layer = Milestone 8 / SSU2 planning
```

Do not write unqualified `milestone7_interoperable = passed`; qualify it as localhost SAM-client interoperability.

## 20. Acceptance criteria

Plan 150 closes only when **all** are true:

1. Plan 149 passed its black-box self-composition and documented local raw-path acceptance subset;
2. the invalid Plan 148 libsam3 pin is removed from live guidance;
3. at least two exact external client revisions are pinned and independently resolvable;
4. neither mandatory external client is patched for i2pr;
5. both mandatory clients negotiate the declared SAM 3.1 baseline with the real listener;
6. destination material passes the required external representation checks;
7. `libsam3 -> i2psam` (or justified equivalent) CONNECT/ACCEPT moves exact bytes;
8. reverse client direction moves exact bytes;
9. binary/multi-packet/multi-megabyte matrices pass;
10. external SILENT evidence plus Plan 149 byte-exact tests pass;
11. multiple streams/lifecycle behavior passes;
12. FORWARD moves real bytes and real peer metadata through the corrected product path;
13. NAMING supported surface passes without DNS/address-book scope expansion;
14. unsupported versions/styles/options fail explicitly;
15. external runs use only the SAM TCP listener to drive product behavior;
16. no private daemon bridge/LeaseSet/tunnel setup is used by the external harness;
17. Plan 149 local resource/privacy gates remain green and the Plan 150 carry-forward fault gates pass;
18. Plan 127–134 regressions remain green;
19. full workspace format/check/test/clippy/doc/boundary gates pass;
20. sanitized evidence records exact commits, build commands, OS, and results;
21. CI/current head is green;
22. `plans/150-status.md` closes Milestone 7 and advances to Milestone 8 only after every item above passes.

## 21. Required validation commands

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
```

Plus:

```text
Plan 146 reference suite
Plan 149 self-composed raw product suite
Plan 149 local self-composed/backpressure/lifecycle suite
Plan 150 carry-forward fault/sibling/lifecycle suites
libsam3 external runner
i2psam external runner
i2plib supplementary runner (if qualified)
FORWARD/NAMING external lane
Plan 127-134 focused regressions
```

Record exact commands and pass counts in `plans/150-status.md`.

## 22. Stop conditions

Stop and write a narrow corrective plan if:

- an external client exposes a protocol mismatch that requires changing M6 Streaming semantics;
- the two mandatory clients disagree on a SAM behavior not clearly resolved by the official spec/reference router;
- client acquisition requires privileged host changes;
- closure would require patching an external client for i2pr;
- product traffic can only succeed by reintroducing private bridge setup into the harness.

## 23. Handoff

Do **not** execute Plan 150 yet.

Execute Plan 149 first. Plan 150 begins only after `plans/149-status.md` records that black-box SAM sessions self-compose the complete local STREAM product and the deferred Plan 147 acceptance matrix is closed.
