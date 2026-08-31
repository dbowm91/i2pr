# Plan 144 — SAM 3.1 independent-client validation and final Milestone 7 closure

Status: **blocked on successful Plan 143 closure**.

Depends on: Plans 142–143; retain Plan 139 FORWARD/naming implementation subject to revalidation.

## 1. Goal

Close Milestone 7 only after the corrected SAM implementation works with independent SAM clients over the real localhost listener and all earlier Phase 7 claims are reconciled to evidence.

This is the final corrective gate. It must not add another large architecture layer. If a narrow client-compatibility defect is found, fix it here with focused tests. If a new architectural defect is discovered, stop and record it rather than marking the milestone passed.

Required final claims:

```text
SAM 3.1 STREAM local API interoperability = passed
M6 local destination/Streaming product    = still passed
SAM independent clients                   = at least two passed
live NTCP2/SSU2                           = not required / not claimed here
mixed-router/public-I2P interop           = retained external MVP debt
```

## 2. Independent client selection

Use the current official SAM known-library table as the discovery source:

- https://www.i2p.net/en/docs/api/samv3/

As of the current specification, useful STREAM candidates include:

- **libsam3** — C, SAM 3.1, explicitly maintained by the I2P project;
- **i2psam** — C++/C wrapper, SAM 3.1;
- **i2p-rs** — Rust, SAM 3.1;
- **i2plib** — Python, SAM 3.1;
- **i2p-sam** — TypeScript, SAM 3.1;
- another maintained independent implementation if environment compatibility is materially better.

Prefer `libsam3` as one candidate if it builds cleanly in the target Ubuntu environment because it is maintained by the I2P project. Prefer a second client from another codebase/language. Do not make `txi2p` mandatory; Plan 140 already showed its legacy dependency chain is unsuitable for this environment.

### Selection requirements

For each counted client:

- pin exact version/commit;
- record upstream URL and license;
- build/install without root where practical;
- do not patch the client to understand i2pr-specific behavior;
- negotiate/use only behavior compatible with the M7 server claim;
- connect to the real i2pr `127.0.0.1` listener;
- use public APIs or a tiny standalone runner, never i2pr internals.

Two different wrappers around the same underlying client implementation do not count as two independent clients.

## 3. Version and baseline audit before interoperability

Freeze the exact advertised contract before running clients.

### 3.1 Server version

- advertise SAM 3.1 only;
- no 3.2/3.3 version claim;
- incompatible `MIN`/`MAX` ranges -> `NOVERSION`;
- verify current SAM 3.1 behavior where `MIN` and `MAX` may be omitted and add compatibility tests if the server does not already support this correctly.

### 3.2 Version-gated commands/options

Audit the implementation against current specification version annotations.

In particular, the current spec lists PING/PONG and QUIT/STOP/EXIT as SAM 3.2+ optional features. If i2pr accepts them while negotiated as 3.1, choose one explicit policy:

1. reject them under the strict 3.1 baseline; or
2. retain them as documented non-advertised extensions only if reference clients/routers demonstrate this is harmless and it cannot be mistaken for a 3.2 claim.

Do not count 3.2-only commands as M7 SAM 3.1 conformance evidence.

Continue to reject or not advertise:

- `FROM_PORT` / `TO_PORT` on STREAM CONNECT under 3.1;
- `SSL=true` FORWARD;
- AUTH/TLS;
- PRIMARY/subsessions;
- Proposal 167 `OPTIONS=true` support unless deliberately implemented later;
- DATAGRAM/RAW families for this milestone.

## 4. Canonical independent-client matrix

At least two independent clients must execute real STREAM paths.

Name them `Client A` and `Client B` in the evidence record.

### 4.1 HELLO

Each client independently:

```text
connect -> HELLO -> VERSION=3.1
```

No compatibility shim may rewrite the client's command stream inside i2pr tests.

### 4.2 Destination/session material

Required combined evidence:

- `DEST GENERATE SIGNATURE_TYPE=7` from i2pr produces canonical I2P Base64;
- at least one client consumes that `PRIV` and creates a STREAM session;
- at least one client can use a transient or independently generated compatible private destination;
- public Destination equality is proven across the boundary;
- no RFC-4648 `+/` workaround remains.

If a high-level library hides DEST GENERATE, use a tiny independent transcript utility for this sub-check, but it does not replace either of the two counted STREAM clients.

### 4.3 Cross-client STREAM directions

Mandatory if client APIs permit:

```text
Client A session -> CONNECT -> Client B session ACCEPT
Client B session -> CONNECT -> Client A session ACCEPT
```

If one library cannot expose ACCEPT due to its API design, retain it only if a second client can exercise both directions and add another independent STREAM implementation to preserve two-client evidence.

Do not count an i2pr-owned Rust test peer as an independent client.

## 5. Binary byte interoperability

For each cross-client direction transfer exact payloads containing:

- ordinary ASCII;
- `\n` and `\r\n`;
- NUL;
- all byte values 0x00..0xff at least once;
- invalid UTF-8 sequences;
- text beginning with `HELLO VERSION` / `STREAM CONNECT` / `QUIT`;
- payload > one Streaming packet;
- payload large enough to require many bounded chunks.

Assert exact byte equality, not decoded strings.

This is also the proof that the socket permanently left SAM command mode.

## 6. SILENT interoperability

Use an independent client when the API exposes SILENT, otherwise use a standalone non-i2pr transcript runner against the real listener.

Verify byte-for-byte:

- CONNECT default/non-silent emits exactly one final OK status before application bytes;
- CONNECT silent emits no status after the command;
- ACCEPT default/non-silent emits OK, then peer Destination line, then raw data;
- ACCEPT silent emits no status and no peer-Destination line.

Application payload beginning with newline/text resembling SAM commands must remain untouched.

## 7. Multiple streams and lifecycle

Through independent clients where possible:

- two sequential streams on one session;
- two concurrent streams if supported by selected clients;
- close one stream without killing sibling/session;
- remote EOF arrives after normal close;
- abrupt local close releases stream state;
- control session disconnect cancels active streams;
- session can be recreated after cleanup within resource ceilings.

Record any client-library limitation separately from server behavior.

## 8. Revalidate Plan 139 FORWARD against the corrected bridge

The Plan 139 implementation may remain unchanged if it passes these real-byte tests.

Using a real loopback target server and a SAM client/transcript socket:

1. create session B;
2. issue `STREAM FORWARD ID=B PORT=<target>`;
3. create session A with an independent client;
4. A CONNECTs to B;
5. i2pr opens the configured loopback target TCP connection;
6. non-silent target receives authenticated A public Destination line;
7. A -> target raw bytes are exact;
8. target -> A raw bytes are exact;
9. second inbound stream creates a second target connection while registration remains active;
10. forward control socket close prevents future forwarding;
11. active stream cleanup follows the documented policy;
12. `HOST` outside loopback remains rejected;
13. `SILENT=true` target receives application byte 0 first.

If this trajectory cannot pass because FORWARD still uses a separate capture/local-copy path instead of the Plan 143 bridge, refactor FORWARD to call the common raw bridge rather than duplicating it.

## 9. Revalidate naming

Through independent transcript/client API:

- `NAME=ME` in valid session context -> exact session public Destination;
- full I2P Base64 Destination -> canonical same Destination;
- locally known b32 behavior matches documented scope;
- unknown human `.i2p` -> `KEY_NOT_FOUND` without system DNS;
- malformed Destination -> `INVALID_KEY`;
- `OPTIONS=true` remains explicit unsupported behavior unless the version/scope is deliberately changed.

Ensure all returned Destination values now use corrected I2P Base64.

## 10. Resource and adversarial closure

Run the canonical Rust product tests plus real sockets.

### Parser/client

- exact client ceiling and +1;
- line maximum and +1;
- token/option maximum and +1;
- malformed quote/escape/control bytes;
- repeated invalid I2P Base64 does not accumulate memory.

### Sessions

- exact session ceiling and +1;
- duplicate ID;
- duplicate Destination;
- create/destroy loop;
- disconnect with zero streams;
- disconnect with active CONNECT/ACCEPT/FORWARD.

### Streams

- exact stream ceiling;
- exact pending ACCEPT ceiling;
- connect timeout;
- ACCEPT cancellation;
- sibling stream isolation;
- remote reset/close;
- local abrupt socket error.

### Backpressure

Re-run Plan 143 slow-reader/slow-writer tests and record exact configured byte ceilings. Resource counts and buffered bytes must return to baseline.

### Shutdown

Daemon cancellation with every resource category active must terminate within configured bounds with no orphan listener/task/session/stream/forward entry.

## 11. Privacy/security closure

Capture tracing/logging for representative operations and assert it does not contain:

- `PRIV` strings;
- signing private material;
- X25519 secret material;
- arbitrary raw application payload markers;
- full private SESSION CREATE command lines;
- ECIES session tags.

Confirm:

- SAM listener rejects non-loopback bind configuration;
- FORWARD rejects non-loopback targets;
- no system DNS for `.i2p`/b32 resolution;
- `[sam] enabled=false` remains default;
- no authentication claim is made.

## 12. Milestone 6 regression floor

Phase 7 closure is invalid if its runtime changes regress the local destination/Streaming product.

Run at least:

```text
cargo test --locked -p i2pr-client --test plan127_trajectory
cargo test --locked -p i2pr-client --test plan128_trajectory
cargo test --locked -p i2pr-client --test plan129_trajectory
```

and all retained Plan 130–134 focused regressions. Use actual current test target names from the tree rather than inventing commands if names have changed.

Do not add mixed-router NTCP2/SSU2 as an M7 gate.

## 13. Evidence artifact

Replace/extend `tests/integration/sam/README.md` and create a concise evidence file such as:

```text
tests/integration/sam/m7-closure-evidence.md
```

Record:

```text
i2pr commit
OS/toolchain
SAM bind address/policy
Client A name/version/commit/license
Client B name/version/commit/license
build/run commands
HELLO results
DEST GENERATE/private-import result
A -> B STREAM result
B -> A STREAM result
binary payload sizes/hashes
SILENT result
multi-stream result
FORWARD result
NAMING result
negative-version result
resource/backpressure result
M6 regression result
known limitations
```

Never commit full `PRIV` strings or sensitive payloads. Hash throwaway test payloads/artifacts where useful.

## 14. Documentation/authority cleanup

At successful closure update all current-authority documents in the same pass.

### `plans/000-mvp-roadmap.md`

Milestone 7 must explicitly list:

- `DEST GENERATE`;
- SAM 3.1 STREAM baseline;
- loopback-only listener/security policy;
- explicit unsupported newer features;
- local independent-SAM-client interoperability as the M7 exit;
- router-to-router/public-network interoperability as separate MVP debt.

### `plans/README.md`

Register Plan 144 as the Milestone 7 closure authority and add a concise M7 hierarchy showing why Plans 136/138 historical closure labels were superseded.

### `README.md`

State current capability, not plan chronology. At closure:

```text
M6 local destination/Streaming = passed
SAM 3.1 STREAM localhost API    = implemented/validated with independent clients
SAM listener default            = disabled, loopback-only when enabled
mixed-router/public network     = unclaimed
next milestone                  = M8 / SSU2
```

### Protocol/support docs

Update both `docs/protocol-support.md` and any `specs/` mirror with evidence-accurate status.

Correct stale Plan 136 claims about Base64/private-destination provenance.

### Architecture/security docs

Document the final common raw bridge, session/stream ownership, bounds, loopback assumptions, and remaining external debt.

### Status authority

Create `plans/144-status.md`. Mark Plan 141 roadmap completed. Do not rewrite historical status records except to add concise superseding notes when needed.

## 15. Final validation commands

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
cargo deny check advisories bans sources
git diff --check
```

Plus:

- focused Plan 142 encoding/private-destination tests;
- focused Plan 143 STREAM product tests;
- Plan 139 FORWARD/naming tests over corrected bridge;
- Plan 129–134 regression floor;
- independent Client A runner;
- independent Client B runner.

Remote CI must pass on the final closure commit. If macOS/other runner-specific socket failures recur, correct the underlying test/runtime race or explicitly scope platform support; do not repeatedly weaken timeouts until a flaky test happens to pass.

## 16. Acceptance criteria

Milestone 7 closes only when every criterion is true:

1. Plan 142 passed with canonical I2P Base64 and independently proven private-destination compatibility;
2. Plan 143 passed with real same-socket raw CONNECT/ACCEPT through the full M6 local product stack;
3. SAM advertises only 3.1 and version-gated behavior is documented accurately;
4. at least two independent SAM implementations connect to the real listener and are recorded with pinned provenance;
5. cross-client STREAM moves exact bidirectional arbitrary binary bytes;
6. application data that resembles SAM commands remains raw data;
7. both TRANSIENT/imported-private session paths needed by selected clients work;
8. SILENT behavior is byte-exact;
9. multiple stream lifecycle is correct and bounded;
10. FORWARD moves real bytes through the common corrected bridge and remains loopback-target-only;
11. naming returns corrected canonical Destination encodings and never uses system DNS for `.i2p`;
12. unsupported versions/styles/options are explicit and truthful;
13. all parser/client/session/stream/accept/forward/task/buffer ceilings pass exact-boundary tests;
14. slow reader/writer tests prove bounded memory/backpressure;
15. control disconnect and daemon shutdown restore all resource accounting to baseline;
16. privacy tests contain no private destination or raw payload leakage;
17. all M6 local regressions remain green;
18. workspace/boundary/dependency gates pass;
19. final remote CI passes;
20. README/roadmap/protocol-support/architecture/security docs match the evidence;
21. `plans/144-status.md` records `milestone7_local_product = passed` and `sam_independent_clients >= 2`;
22. only after all above does `next_product_layer = Milestone 8 / SSU2 planning` become authoritative.

## 17. Final status shape

Successful closure should register approximately:

```text
plan_134 = passed-milestone6-local-product
plan_141 = completed-m7-corrective-roadmap
plan_142 = passed-sam31-encoding-private-destination-correction
plan_143 = passed-sam31-live-stream-product-bridge
plan_144 = passed-milestone7-sam31-independent-client-local-interoperability

milestone7_local_product = passed
sam31_stream = implemented-and-product-validated
sam_independent_clients = at-least-two-passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = Milestone 8 / SSU2 planning
```

Do not use an unqualified `milestone7_interoperable = passed`; the proven interoperability is SAM-client/local-product interoperability, not router-to-router network interoperability.