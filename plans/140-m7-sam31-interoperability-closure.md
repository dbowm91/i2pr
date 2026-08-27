# Plan 140 — SAM 3.1 independent-client interoperability and Milestone 7 closure

Status: **blocked on successful Plan 139 closure**.

Depends on: Plans 135–139.

## 1. Goal

Close Milestone 7 with evidence that the SAM implementation is useful to ordinary independent clients, not merely internally self-consistent.

This is a **localhost application-protocol interoperability gate**, not a mixed-router/public-I2P gate. It must prove:

- independent clients can negotiate with the real i2pr SAM TCP listener;
- destination generation/session import works using standard SAM representations;
- at least two independently implemented SAM clients can create STREAM sessions and move bytes through the real local destination + Streaming product path;
- command/status/error semantics are sufficiently interoperable;
- all session/stream/forward/naming resources remain bounded and clean up correctly;
- documentation/roadmap claims are updated to exactly match evidence.

Do not reopen NTCP2, SSU2, rootless namespaces, VM harnessing, Java-router live-wire delivery, or public network participation as a requirement for this closure.

## 2. Pre-closure audit

Before adding new compatibility work, inspect the source tree produced by Plans 136–139 and answer these questions in the Plan 140 status record:

1. Does every SAM socket begin with the same bounded HELLO path?
2. Is SAM 3.1 the only advertised version?
3. Is the parser detached permanently after STREAM raw-mode transition?
4. Does each session own exactly one destination and one control socket?
5. Can generated `PRIV` be imported with exact Destination equality?
6. Do CONNECT/ACCEPT/FORWARD all traverse the same M6 Streaming/destination product path?
7. Are all queues/byte buffers named and bounded?
8. Is there any direct test-only manager-to-manager shortcut in the claimed product acceptance trajectory?
9. Are private destinations or application payloads observable in default logs?
10. Are unsupported SAM styles/newer features explicit?

If a concrete defect is found, fix it in Plan 140 only if it is narrow and directly required for the M7 contract. If it is architectural or changes M6 protocol behavior, write one narrowly scoped corrective plan rather than broadening this closure indefinitely.

## 3. Independent-client selection

Use current official I2P SAM documentation's known-client list as the discovery source and pin exact revisions/versions before testing.

Preferred SAM 3.1 STREAM-capable candidates include:

- `i2psam` (C++/C wrapper, SAM 3.1);
- `libsam3` (C, SAM 3.1; maintained by the I2P project according to current SAM docs);
- `i2plib` (Python, SAM 3.1);
- `txi2p` (Python/Twisted, SAM 3.1);
- `i2p-rs` (Rust, SAM 3.1);
- other currently maintained clients from the official known-library table if environment/toolchain compatibility is better.

A newer client such as Go `sam3` may be used if it can be configured to negotiate/use only the M7 SAM 3.1 STREAM subset; do not expand server claims merely to satisfy a client defaulting to 3.2/3.3.

Selection requirements:

- at least **two independent implementations**;
- preferably two different languages/codebases;
- neither may import `i2pr-api` internals;
- pin repository commit, package version, or checksum;
- record license/provenance;
- record the exact commands/features actually exercised;
- avoid clients requiring a live I2P router/network beyond the SAM server.

If one candidate is stale/broken for reasons unrelated to i2pr, document the evidence and choose another. Do not modify i2pr to mimic a demonstrable client bug unless reference routers exhibit the same compatibility behavior.

## 4. No heavy harness requirement

Keep interoperability execution lightweight.

Preferred layout:

```text
tests/integration/sam/
  README.md
  fixtures/ or transcripts/
  client_a/ minimal runner or documented invocation
  client_b/ minimal runner or documented invocation
```

The runners may be Rust tests invoking installed/buildable client binaries/libraries, shell scripts, or small language-specific programs. Do not recreate the earlier large Python orchestration architecture.

Requirements:

- localhost only;
- no root/sudo;
- no namespaces;
- no Docker requirement;
- no systemd requirement;
- deterministic temp directories and ephemeral ports;
- clear SKIP only when an optional external client toolchain is unavailable;
- at least one in-repo CI-capable interoperability lane should remain practical if dependencies are reasonable.

The canonical product tests in Rust must continue to pass even when optional external client tooling is unavailable.

## 5. Mandatory independent-client trajectories

Each of the two selected clients must prove as much of this canonical sequence as its public API exposes.

### A. HELLO/version

- connect to i2pr listener;
- negotiate compatible 3.1;
- no unsupported version claim.

### B. destination generation/private import

At least one independent client must consume i2pr `DEST GENERATE SIGNATURE_TYPE=7` output or equivalent utility API and successfully create a session using the returned private destination.

At least one cross-check must prove the public Destination derived/observed by the client exactly equals i2pr's public Destination after import.

If client libraries hide DEST GENERATE, use a tiny independent transcript client for this one utility operation in addition to the two library STREAM tests.

### C. two-session STREAM connection

Create A and B sessions through the SAM listener.

- B listens/accepts;
- A connects using B's public Destination;
- both reach established application sockets;
- no direct i2pr internal APIs are called by the external clients.

### D. bidirectional binary bytes

Transfer payloads containing:

- ASCII text;
- newline sequences;
- NUL bytes;
- non-UTF8 bytes;
- text that looks like SAM commands;
- payload substantially larger than one Streaming packet.

Verify exact byte equality both directions.

### E. multiple streams

Create at least two concurrent or sequential streams on one SAM session according to each client's capabilities. Failure of one must not destroy the sibling/session.

### F. close

Exercise normal client socket close and verify the opposite side receives EOF/closure within the bounded policy, with resource counts returning to baseline.

## 6. Cross-client matrix

Do not test only each client against an i2pr-owned peer helper. Where APIs permit, execute cross-client combinations:

```text
Client A session/dial -> Client B session/accept
Client B session/dial -> Client A session/accept
```

Both communicate through the same i2pr SAM server and local destination product.

If a selected library cannot perform one direction due to API limitation, document it and use the second library or independent transcript client to fill the missing direction.

## 7. STREAM FORWARD interoperability

At least one independent client or independent transcript runner must exercise FORWARD if no selected high-level library exposes it.

Required evidence:

- forward registration receives `STREAM STATUS RESULT=OK` regardless of SILENT setting;
- inbound stream causes i2pr to connect to the configured loopback target;
- non-silent target receives peer public Destination line first;
- silent target receives raw payload first;
- forward control socket close removes registration;
- non-loopback target rejection is documented as i2pr's intentional M7 security policy.

STREAM FORWARD support may be marked `implemented-with-i2pr-loopback-target-policy` rather than unqualified full SAM exposure.

## 8. NAMING interoperability

At minimum verify with an independent transcript or client API:

- full public Destination lookup round-trips;
- `NAME=ME` returns the current session Destination when used in valid session context;
- malformed key -> `INVALID_KEY`;
- unknown human-readable `.i2p` -> `KEY_NOT_FOUND` without system DNS;
- Proposal 167 `OPTIONS=true` is not claimed.

If a client insists on doing human-readable naming before dialing, configure it to use the full public Destination for the main STREAM trajectory rather than adding an address book to i2pr.

## 9. Negative version/feature compatibility matrix

Freeze direct transcript tests for:

```text
HELLO MIN=3.2 MAX=3.3                 -> no supported version
SESSION CREATE STYLE=DATAGRAM         -> explicit unsupported/error
SESSION CREATE STYLE=RAW              -> explicit unsupported/error
SESSION CREATE STYLE=PRIMARY          -> explicit unsupported/error
STREAM CONNECT ... FROM_PORT=1        -> rejected/not-supported under 3.1
STREAM FORWARD ... SSL=true           -> rejected/not-supported
NAMING LOOKUP ... OPTIONS=true        -> rejected/not-supported M7 behavior
unknown command/action                -> deterministic error/close policy
```

Also test malformed command ordering and duplicate options.

Do not claim interoperability by accepting fields and silently ignoring them.

## 10. Resource/lifecycle closure suite

Run the final boundedness suite through real loopback sockets:

### Session lifecycle

- create/destroy one session repeatedly;
- exact global session ceiling;
- duplicate ID/destination races;
- control socket EOF with zero streams;
- control socket EOF with active CONNECT/ACCEPT/FORWARD streams;
- daemon cancellation with active sessions.

### Streams

- exact per-session stream ceiling;
- pending ACCEPT ceiling;
- multiple sibling streams;
- connect timeout;
- local raw socket abrupt close;
- remote reset path;
- forward target refusal/timeout.

### Buffers

- slow reader at exact byte budget;
- writer exceeding send-window/bridge budget;
- transfer resumes after pressure relief;
- aggregate buffered byte accounting returns to baseline after close.

### Parser

- line max and max + 1;
- token/option max and max + 1;
- command without newline until bound;
- malformed UTF-8/control chars according to parser policy;
- malformed quoting/escape;
- repeated invalid Base64/key material.

Prefer explicit counters/test snapshots and task supervisor state over process-RSS-only assertions.

## 11. Regression gate for Milestone 6

Because SAM drives M6 heavily, rerun all existing local destination/Streaming closure tests, especially:

- Plan 127 destination session/routing trajectory;
- Plan 128 wire protocol tests;
- Plan 129 product trajectory;
- Plan 130/131/132/133 corrective tests still retained;
- Plan 134 receive-window ACK-ceiling regression.

SAM closure is invalid if adapter work weakened these.

Do not require mixed-router NTCP2 acceptance as part of this regression gate.

## 12. Documentation closure

Update the following as implementation work lands. These updates are mandatory in Plan 140 if not already correct.

### `plans/000-mvp-roadmap.md`

Amend Milestone 7 scope to explicitly include:

- `DEST GENERATE`;
- SAM 3.1 STREAM baseline;
- explicit unsupported newer styles/features;
- loopback-only security policy.

Clarify the M7 exit meaning:

```text
SAM client interoperability over local i2pr destination/Streaming product = required
external router/mixed-network destination interoperability               = retained MVP debt
```

### `README.md`

Remove stale language that points to Plan 129 as the current frontier/authority. Summarize current state instead of appending another long chronology.

At M7 closure the status should distinguish:

```text
SAM 3.1 STREAM local API      = implemented/validated
M6 local destination/streaming = passed
live NTCP2/SSU2               = not enabled/validated as applicable
mixed-router/public I2P       = unclaimed
next milestone                = M8 SSU2
```

### `docs/protocol-support.md`

Add a compact SAM matrix including at least:

| Surface | M7 status |
|---|---|
| HELLO VERSION 3.1 | supported |
| DEST GENERATE sig type 7 | supported |
| SESSION CREATE STREAM | supported |
| TRANSIENT | supported |
| imported private destination | supported |
| STREAM CONNECT | supported |
| STREAM ACCEPT | supported |
| STREAM FORWARD | supported with documented loopback target policy |
| SILENT | supported |
| NAMING LOOKUP ME/full Destination | supported |
| address-book `.i2p` naming | not implemented |
| DATAGRAM/RAW | not implemented |
| PRIMARY/subsessions | not implemented |
| SAM 3.2/3.3 claims | not advertised |
| AUTH/TLS | not implemented |

### `docs/architecture.md`

Document:

```text
i2pr-api -> i2pr-client
          -> no inverse dependency
SAM parser/state is runtime-neutral where practical
Tokio listener/tasks live at daemon composition
one control socket owns one ordinary session/destination
stream sockets attach by session ID
```

### `docs/security-model.md`

Document:

- unauthenticated SAM therefore loopback-only;
- non-loopback bind rejected;
- forward target loopback restriction;
- private destination handling/redaction;
- raw payload logging prohibition;
- resource ceilings;
- remaining local-process threat assumptions;
- external interoperability debt.

### Plan/status files

Create `plans/140-status.md` with exact evidence and update `plans/135-status.md` to closed/superseded-by-M7 status.

## 13. Top-level roadmap progression

At successful closure, register:

```text
plan_134                    = passed-milestone6-local-product
plan_135                    = phase7-roadmap-completed
plan_136                    = passed-sam31-protocol-private-destination-foundation
plan_137                    = passed-sam31-loopback-session-lifecycle
plan_138                    = passed-sam31-stream-connect-accept-product
plan_139                    = passed-sam31-forward-naming-hardening
plan_140                    = passed-milestone7-sam31-local-interoperability

milestone7_local_product    = passed
sam31_stream                = implemented
sam_independent_clients     = at-least-two-passed
milestone6_interoperable    = not-yet-claimed
external_acceptance_debt    = retained-separately
router_construction         = may-continue
next_product_layer          = Milestone 8 / SSU2 planning
```

Do not use `milestone7_interoperable = passed` without the qualifier that the evidence is SAM-client/local-product interoperability. Router-to-router interoperability remains separate.

## 14. Required validation commands

The closure record should include at least:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Plus focused commands for:

```text
i2pr-api SAM unit/integration tests
Plan 136 private-destination vectors
Plan 137 lifecycle tests
Plan 138 STREAM product trajectories
Plan 139 FORWARD/naming/resource tests
Plan 134 regression trajectory
external client A interoperability
external client B interoperability
```

Record exact commands, versions, pass counts, and any intentionally optional/skipped external lane.

## 15. Evidence artifact

Create one concise machine/human-readable evidence record under `tests/integration/sam/` or `plans/140-evidence.md` containing:

```text
i2pr commit
OS/toolchain
SAM bind policy
client A name/version/commit
client B name/version/commit
commands/features exercised
A->B byte result
B->A byte result
DEST GENERATE/import result
FORWARD result
NAMING result
negative-version result
resource/lifecycle test result
known limitations
```

Do not embed private keys, full generated `PRIV` strings, or application secrets in committed evidence.

## 16. Acceptance criteria

Milestone 7 closes only if every item below is true:

1. Plans 136–139 are individually closed with status records.
2. SAM 3.1 remains the only advertised version.
3. `DEST GENERATE SIGNATURE_TYPE=7` produces standard private/public representations that round-trip through session creation.
4. both TRANSIENT and imported-private STREAM sessions work.
5. at least two independent SAM clients connect to the real i2pr listener.
6. cross-client CONNECT/ACCEPT moves exact bidirectional binary bytes through the M6 product architecture.
7. no claimed product test bypasses `StreamingDestinationAdapter`/destination routing with direct manager wiring.
8. SILENT behavior is byte-correct.
9. multiple streams behave independently and remain bounded.
10. STREAM FORWARD works against a real loopback target with documented security restriction.
11. NAME=ME and full-Destination NAMING LOOKUP work; unsupported naming remains explicit.
12. unsupported styles/versions/options fail explicitly.
13. parser, client, session, stream, accept, forward, task, and buffered-byte ceilings are tested at boundaries.
14. control disconnect and daemon shutdown return all SAM resource accounting to baseline.
15. slow-reader/writer tests prove no unbounded SAM buffering.
16. privacy/log capture contains no private destination or raw payload material.
17. all M6 local regressions remain green, including Plan 134.
18. full workspace format/check/test/clippy/doc/boundary gates pass.
19. README, roadmap, protocol-support, architecture, and security-model docs describe the current state without overstating live-router interoperability.
20. the next roadmap frontier is explicitly Milestone 8 / SSU2, not another speculative M7 closure pass.

## 17. Stop conditions

Do not declare closure if:

- only i2pr-authored test clients work;
- a client succeeds only because server ignores unsupported options;
- raw mode still passes through line parsing;
- private destination encoding is i2pr-specific;
- STREAM bytes bypass the M6 product path;
- slow readers can grow memory without a hard ceiling;
- session control disconnect leaves active destination/stream tasks;
- docs imply public I2P functionality that has not been demonstrated.

If one independent client exposes a narrow compatibility discrepancy, compare against current Java I2P/i2pd/official SAM behavior before deciding whether i2pr or the client is wrong.

## 18. Handoff checklist

```text
[ ] two independent SAM clients are pinned
[ ] each talks to the real i2pr loopback listener
[ ] DEST GENERATE/private import is externally consumable
[ ] cross-client STREAM data is exact both directions
[ ] FORWARD and NAMING baseline evidence is recorded
[ ] unsupported matrix is explicit
[ ] resource/lifecycle closure suite passes
[ ] M6 regression suite passes
[ ] docs are updated and claims are qualified
[ ] plans/140-status.md is committed
[ ] next_product_layer = Milestone 8 / SSU2 planning
```

Successful execution of this plan closes **Milestone 7: SAM baseline**.