# Plan 148 — SAM 3.1 independent-client interoperability and final Milestone 7 closure

Status: **blocked on successful Plan 147 closure**.

Depends on: Plan 146 reference-compatible private destination; Plan 147 dedicated raw STREAM product driver; Plan 139 FORWARD/naming implementation; Plan 134 M6 local closure.

## 1. Goal

Close Milestone 7 with evidence from independent SAM clients using the real i2pr loopback listener and the real Plan 147 raw STREAM product path.

This is a **localhost SAM application-interface interoperability gate**. It is not a router-to-router/public-I2P interoperability gate.

Milestone 7 may close only if:

- at least two independent SAM implementations negotiate with the real listener;
- generated/imported private destinations are interoperable;
- independent clients create STREAM sessions;
- cross-client CONNECT/ACCEPT moves exact binary bytes in both directions;
- FORWARD and naming behavior are revalidated on the corrected raw path;
- resource, lifecycle, privacy, and M6 regressions pass;
- documentation states exactly what is and is not proven.

## 2. Client selection

Preferred clients based on the existing provenance work:

### Client A — i2plib

Pin the exact revision already recorded in `tests/integration/sam/README.md` unless a newer pinned revision is required for a concrete compatibility reason.

Use its public SAM API/wire helpers only. It may not import i2pr internals.

### Client B — libsam3

Pin the exact revision already recorded in `tests/integration/sam/README.md` unless a concrete update is required.

Build/use its normal library/example API. It may not link against any i2pr crate.

### Optional alternatives

If either selected client is demonstrably broken or environment-blocked for reasons unrelated to i2pr, choose another maintained independent implementation from current SAM documentation, such as:

- i2p-rs;
- i2psam;
- another maintained SAM client that can be constrained to the SAM 3.1 STREAM subset.

Do not make `txi2p` mandatory; its legacy `ometa` dependency is already known to be problematic in the constrained environment.

Record exact version/commit, language, license, build/install command, and the public API exercised.

## 3. Harness policy

Keep the external lane lightweight.

Preferred layout:

```text
tests/integration/sam/
  README.md
  evidence.md or evidence.json
  i2plib_client.py        # minimal runner if needed
  libsam3_client.c        # minimal runner if needed
  run-independent.sh      # optional thin wrapper
```

Rules:

- localhost only;
- no root/sudo;
- no namespaces;
- no Docker requirement;
- no VM requirement;
- no systemd requirement;
- no public I2P network;
- no external router required for the application-byte lane;
- no large Python orchestration framework;
- deterministic temp directories and ephemeral ports;
- client dependencies pinned;
- optional client absence may SKIP CI, but closure evidence must include an actual completed local run for two clients.

The canonical Rust product tests from Plan 147 remain mandatory even if external tooling is unavailable in ordinary CI.

## 4. Independent representation gate

Before STREAM testing, verify Plan 146 evidence against the external clients.

At minimum:

```text
HELLO VERSION MIN=3.1 MAX=3.1
DEST GENERATE SIGNATURE_TYPE=7
SESSION CREATE STYLE=STREAM ID=<id> DESTINATION=<returned PRIV>
```

Required:

- at least one client consumes i2pr-generated `PRIV` without modification;
- at least one reference-generated/imported `PRIV` creates a session on i2pr;
- the public Destination observed by the independent client matches i2pr exactly;
- canonical I2P Base64 `-` / `~` spelling survives both directions.

Do not continue to byte-path closure if destination identity equality fails.

## 5. Cross-client STREAM matrix

Create two independent sessions, A and B, through the real i2pr listener.

Required combinations where client APIs permit:

```text
i2plib CONNECT  -> libsam3 ACCEPT
libsam3 CONNECT -> i2plib ACCEPT
```

If one client does not expose one direction cleanly, use an independent minimal transcript client to fill that direction while retaining both selected clients in the overall evidence.

No external client may call i2pr's internal registry/Streaming APIs.

## 6. Binary byte matrix

For each cross-client direction, transfer exact payloads containing:

- ASCII;
- `\n` and `\r\n`;
- NUL bytes;
- non-UTF8 bytes;
- all byte values `0x00..0xff`;
- bytes beginning with `PING`, `QUIT`, `HELLO VERSION`, and `STREAM CONNECT` text;
- payload larger than one Streaming packet;
- payload larger than the send window, requiring normal ACK/backpressure progress;
- simultaneous bidirectional traffic.

Verify exact byte equality, byte count, and EOF behavior.

At least one trajectory should transfer a multi-megabyte logical payload while the Plan 147 boundedness counters remain within declared ceilings.

## 7. SILENT interoperability

Use independent clients or a tiny independent transcript runner to freeze exact SAM 3.1 behavior.

Required:

### CONNECT SILENT=false

- final pre-raw line is `STREAM STATUS RESULT=OK`;
- first subsequent byte is application raw data.

### CONNECT SILENT=true

- no STREAM STATUS success line;
- first server/client data is raw application data;
- failure closes the socket without a success line.

### ACCEPT SILENT=false

- `STREAM STATUS RESULT=OK` first;
- once inbound peer arrives, authenticated peer public Destination line;
- then raw bytes.

### ACCEPT SILENT=true

- no status line and no peer-Destination line before raw data.

No raw payload may be interpreted as a later SAM command.

## 8. Multiple streams and lifecycle

Through independent clients:

- open at least two streams on one session;
- transfer independent payloads;
- close one stream and keep sibling alive;
- abrupt local close;
- normal graceful close;
- control socket close with active child stream;
- reconnect/recreate session after cleanup;
- repeated create/connect/close cycle with counts returning to baseline.

Exercise exact configured stream/session ceilings with Rust loopback tests if high-level clients do not expose enough concurrency control.

## 9. STREAM FORWARD final acceptance

Plan 139's FORWARD implementation must be re-run through the corrected Plan 147 byte path.

Use at least one independent client or independent transcript runner.

Required:

```text
session B registers STREAM FORWARD to 127.0.0.1:<echo-target>
session A connects to B over real SAM STREAM
SAM connects to the configured forward target
```

Non-silent target receives:

```text
<A public Destination>\n
<raw bytes>
```

Silent target receives raw bytes first.

Required behavior:

- exact A->target payload;
- exact target->A response;
- second independent stream works;
- target connect refusal maps to bounded failure;
- target connect timeout respects existing <=3-second FORWARD policy;
- FORWARD control socket close unregisters it;
- ACCEPT/FORWARD mutual exclusion remains enforced;
- non-loopback target remains rejected as intentional i2pr security policy.

Do not broaden FORWARD exposure to non-loopback during this plan.

## 10. NAMING final acceptance

Verify through independent client API or transcript:

- full public Destination lookup round-trips exactly;
- `NAME=ME` returns the session Destination in valid session context;
- malformed destination -> typed invalid-key result;
- unknown human-readable `.i2p` -> `KEY_NOT_FOUND` without system DNS;
- locally known `.b32.i2p` behavior matches documented local-only policy;
- Proposal 167 `OPTIONS=true` is not claimed unless actually implemented;
- no system resolver/network DNS fallback occurs.

Do not add an address book merely to satisfy a client convenience API; configure the main STREAM trajectory with the full Destination.

## 11. Negative compatibility matrix

Freeze direct transcript tests for unsupported behavior:

```text
HELLO MIN=3.2 MAX=3.3                 -> no supported version
SESSION CREATE STYLE=DATAGRAM         -> explicit unsupported/error
SESSION CREATE STYLE=RAW              -> explicit unsupported/error
SESSION CREATE STYLE=PRIMARY          -> explicit unsupported/error
STREAM CONNECT ... FROM_PORT=1        -> rejected/not-supported for declared 3.1 subset
STREAM CONNECT ... TO_PORT=1          -> rejected/not-supported for declared 3.1 subset
STREAM FORWARD ... SSL=true           -> rejected/not-supported
NAMING LOOKUP ... OPTIONS=true        -> explicit unsupported unless separately implemented
unknown command/action                -> deterministic error/close policy
```

Also test duplicate options, malformed ordering, oversized line, malformed Base64, and invalid session IDs.

Do not claim interoperability by silently ignoring unsupported options.

## 12. Resource/adversarial closure

Run the final resource suite through real loopback sockets.

### Parser/client bounds

- line max / max+1;
- token/option limits;
- command without newline until ceiling;
- malformed control bytes;
- repeated invalid Base64;
- client connection ceiling.

### Session bounds

- exact global session ceiling;
- duplicate ID/destination races;
- session EOF with no streams;
- session EOF with active streams/FORWARD;
- daemon cancellation with active sessions.

### Stream bounds

- exact per-session stream ceiling;
- pending ACCEPT ceiling;
- sibling streams;
- connection timeout;
- abrupt local raw socket close;
- remote RESET;
- forward target refusal/timeout.

### Buffer bounds

- slow reader exact budget;
- slow writer exact budget;
- transfer resumes after pressure relief;
- aggregate buffered byte accounting returns to baseline.

Prefer explicit counters/task snapshots over process RSS-only assertions.

## 13. Privacy/security closure

Capture default logs during:

- DEST GENERATE;
- imported SESSION CREATE;
- CONNECT/ACCEPT;
- raw binary transfer;
- FORWARD;
- failure paths.

Assert logs contain none of:

- private `PRIV` strings;
- signing seeds/private keys;
- raw application payloads;
- full private destination bytes.

Public Destination values may be logged only according to the existing security/logging policy; do not increase observability for convenience.

Reconfirm:

- SAM remains disabled by default;
- non-loopback SAM bind is rejected;
- FORWARD target remains loopback-only;
- no authentication/TLS capability is implied by the M7 baseline.

## 14. M6 regression floor

Explicitly run the retained Milestone 6 closure trajectories, especially:

- Plan 127 destination session/routing trajectory;
- Plan 128 Streaming wire tests;
- Plan 129 integrated destination + Streaming product path;
- Plan 130/131/132/133 retained corrective tests;
- Plan 134 receive-window ACK-ceiling regression.

Milestone 7 closure is invalid if SAM runtime glue weakens M6 flow control, ACK, retransmission, destination routing, or cryptographic behavior.

Mixed-router NTCP2/SSU2 acceptance remains out of scope.

## 15. Evidence artifact

Create one concise committed evidence record, for example:

```text
tests/integration/sam/evidence.md
```

or a small machine-readable JSON/TOML plus README.

Record:

```text
i2pr commit
OS/toolchain
SAM bind address/port policy
Client A name/version/commit/build command
Client B name/version/commit/build command
private-destination reference result
A -> B CONNECT/ACCEPT result
B -> A CONNECT/ACCEPT result
binary byte matrix result
SILENT result
multi-stream result
FORWARD result
NAMING result
negative matrix result
resource/backpressure result
privacy/log result
M6 regression result
known limitations
```

Do not commit private keys, full generated `PRIV` strings, or application secrets.

## 16. Documentation closure

Update at minimum:

```text
README.md
plans/README.md
plans/145-status.md
plans/148-status.md
AGENTS.md
specs/support.toml
docs/protocol-support.md
docs/protocols/08-sam.md
docs/architecture/i2pr-daemon.md
docs/architecture/i2pr-client.md
docs/security-model.md
tests/integration/sam/README.md
```

Historical Plan 142–144 records remain audit history. The newest status must make clear which earlier claims were superseded.

At successful closure, authority should read approximately:

```text
plan_146 = passed-sam31-private-destination-reference-requalification
plan_147 = passed-sam31-dedicated-raw-stream-driver
plan_148 = passed-milestone7-sam31-independent-client-final-closure

milestone7_local_product = passed
sam31_stream = implemented-and-localhost-interoperable
sam_independent_clients = at-least-two-passed
sam31_forward = implemented-with-loopback-target-policy
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = Milestone 8 / SSU2
```

Do not write `milestone7_interoperable = passed` without qualifying that this is SAM-client / localhost product interoperability only.

## 17. Acceptance criteria

Plan 148 closes only when every item is true:

1. Plan 146 passed with bidirectional reference private-destination evidence;
2. Plan 147 passed with dedicated raw socket ownership and bounded byte flow;
3. at least two independent SAM clients reach the real i2pr listener;
4. both clients negotiate only the declared SAM 3.1 baseline;
5. independent client(s) consume i2pr-generated destination material;
6. reference-generated destination material imports into i2pr;
7. cross-client CONNECT/ACCEPT works in both directions where APIs permit;
8. exact bidirectional binary bytes pass through real SAM TCP sockets and the M6 product path;
9. SILENT semantics are byte-exact;
10. multiple streams remain independent;
11. close/reset/control-session lifecycle is bounded;
12. slow-reader/slow-writer tests prove byte ceilings;
13. loss/duplicate/reorder/ACK behavior remains correct through the raw path;
14. FORWARD sends/receives real bytes to a loopback target and retains the security restriction;
15. NAMING supported surface passes without DNS/address-book scope expansion;
16. unsupported versions/styles/options fail explicitly;
17. default logs expose no private destination or raw payload material;
18. SAM remains disabled by default and loopback-only;
19. Plan 127–134 focused M6 regressions remain green;
20. full workspace format/check/test/clippy/doc/boundary gates pass;
21. committed evidence records exact client versions and results without secrets;
22. `plans/148-status.md` closes Milestone 7 and sets Milestone 8 as next only after all above pass.

## 18. Required validation commands

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
Plan 146 reference commands
Plan 147 raw socket product suite
i2plib independent-client runner
libsam3 independent-client runner
FORWARD/naming independent transcript lane
Plan 127–134 focused regressions
```

Record exact commands and pass counts in `plans/148-status.md`.

## 19. Handoff

Do not move to Milestone 8 until Plan 148 passes.

If an independent client exposes a concrete i2pr protocol defect, fix that defect within Plan 148 only if it is narrow and clearly inside SAM 3.1 scope. If it requires architectural change to M6 or Plan 147, stop and write one narrow corrective plan rather than weakening the acceptance gate.