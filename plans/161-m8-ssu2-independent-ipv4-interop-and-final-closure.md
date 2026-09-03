# Plan 161 — Milestone 8 SSU2 v2 independent IPv4 interoperability and final closure

Status: **registered; final M8 pass after Plan 160**.

Depends on Plans 155–160 all explicitly passed.

## 1. Goal

Close Milestone 8 with reproducible, unprivileged, real-UDP SSU2 v2 interoperability against one mature independent router implementation in both directions, while preserving an evidence contract that cannot overclaim broader router interoperability.

Mandatory independent implementation:

```text
i2pd 2.61.0
exact commit: 635b013a612ff47278ef02acf8580a28e10e26c5
repository: PurpleI2P/i2pd
```

Preferred secondary reference:

```text
Java I2P 2.13.0
exact commit: 9134f808337b401e8e53c73734c81fab04280c9d
repository: i2p/i2p.i2p
```

Java is nonblocking if a narrow standalone/rootless SSU2 invocation is disproportionate. Do not recreate a large VM/router orchestration framework merely to count a second implementation.

## 2. Claim boundary

This plan may establish only the evidence actually exercised:

```text
SSU2 v2 direct authenticated session interoperability
SSU2 v2 bidirectional I2NP message transport
basic Retry/token/session termination interoperability
bounded malformed/spoof handling at the i2pr boundary
```

It does **not** by itself prove:

- public I2P participation;
- NetDB lookup/publication over external peers;
- tunnel construction through external routers;
- destination/Streaming mixed-router interoperability;
- full router interoperability;
- anonymity/security readiness;
- SSU2 PQ v3/v4;
- IPv6 external interoperability unless separately run.

Keep `milestone6_interoperable = not-yet-claimed` unless separate evidence closes it.

## 3. First task: audit the final local floor

Before building external infrastructure, verify current authoritative status files show Plans 155–160 passed and rerun focused local SSU2 gates.

At minimum:

```text
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets   # SSU2-focused filters may supplement
cargo test --locked -p i2pr-transport --all-targets
bash scripts/check-ssu2-vectors.sh
bash scripts/check-runtime-boundaries.sh
```

Also run named final local real-UDP, migration, peer-test and relay suites from Plans 158–160 and record their exact commands in `plans/161-status.md`.

If local evidence is not green, stop; do not debug external clients first.

## 4. External source acquisition policy

Create a narrow acquisition script, preferably:

```text
scripts/interop/fetch-ssu2-reference.sh
```

or similarly named.

Requirements:

- exact pinned i2pd commit only;
- verify checkout `HEAD` equals expected commit before build;
- ephemeral cache/workspace under `target/interop` or runner temp;
- no vendored i2pd source committed to i2pr;
- no patching i2pd to accommodate i2pr;
- record compiler/build command and exact revision;
- fail closed if network source cannot be verified;
- optional Java acquisition uses the exact recorded release commit and remains clearly secondary.

Do not silently float tags/branches.

## 5. Choose the narrowest independent i2pd execution seam

Prefer, in order:

1. a small external C++ SSU2 test driver built from/unmodified against pinned i2pd libraries if the public/internal boundaries make this straightforward and it still sends actual UDP datagrams;
2. a minimally configured ephemeral full i2pd process bound to loopback if that is simpler and more faithful;
3. another clean external driver that invokes i2pd's normal SSU2 session/server code without copying it into i2pr.

The deciding rule is not “smallest LOC at all costs.” It is:

> use the smallest reproducible seam that preserves independent i2pd SSU2 protocol behavior and puts real UDP datagrams between i2pr and i2pd.

Forbidden:

- calling i2pr's SSU2 protocol state machine from the external driver;
- moving handshake/data bytes directly through FFI instead of UDP;
- replacing i2pd SSU2 crypto/codecs with a local transcript helper;
- patching external source to accept i2pr-specific wire behavior;
- requiring root/network namespaces/Docker/VM/systemd.

## 6. Out-of-band identity/RouterInfo exchange

Because this milestone is direct-transport interop rather than NetDB/public-network participation, the harness may exchange sanitized RouterInfos/outbound dial material out-of-band.

Allowed harness setup:

```text
start i2pr SSU2 listener on 127.0.0.1:<ephemeral>
obtain its signed RouterInfo/public SSU2 address
start i2pd SSU2 listener on 127.0.0.1:<ephemeral>
obtain its signed RouterInfo/public SSU2 address
provide each peer's RouterInfo to the initiating side through test configuration
```

Not allowed:

- direct injection of authenticated session keys;
- bypassing RouterInfo signature/static-key validation;
- direct session-registry creation;
- direct I2NP delivery between processes.

Private router keys remain ephemeral and must not be committed/uploaded in artifacts.

## 7. Mandatory direction A: i2pr initiator -> i2pd responder

Prove through real loopback UDP:

1. i2pr parses/validates the pinned i2pd RouterInfo/SSU2 v2 address.
2. establishment follows the protocol's token/Retry behavior exposed by i2pd.
3. SessionRequest/Created/Confirmed completes.
4. i2pr authenticates the i2pd RouterIdentity/static SSU2 key.
5. i2pd authenticates i2pr.
6. both sides remain active beyond handshake long enough for data exchange.
7. send one small and at least one fragmented encoded I2NP test message i2pr -> i2pd.
8. send authenticated I2NP material back i2pd -> i2pr using the narrow driver/process interface.
9. compare exact known test message bytes or cryptographic hashes + lengths at each application/transport boundary.
10. terminate gracefully and verify i2pr resource/task/session baseline.

The external driver may expose received test I2NP bytes to the harness for comparison; that is evidence, not a protocol bypass.

## 8. Mandatory direction B: i2pd initiator -> i2pr responder

Repeat independently with i2pd initiating the SSU2 v2 session.

Required:

- i2pr responder performs token/source validation normally;
- Retry path is exercised when the external implementation naturally sends an unvalidated request/token request;
- authenticated session promotion occurs only after SessionConfirmed/RouterInfo validation;
- bidirectional small + fragmented I2NP data exchange;
- graceful termination/resource baseline.

Do not count a reconnect initiated by i2pr as satisfying the i2pd-initiator direction.

## 9. Token/Retry interoperability matrix

The final lane should record as much of this matrix as the independent implementation's normal API permits:

- no preexisting token -> Retry/token path -> success;
- valid token -> subsequent establishment path, if i2pd exposes/uses it reproducibly;
- expired/invalid token -> i2pr fails/retries according to spec;
- token source binding remains enforced by i2pr.

Only rows actually exercised externally may be labeled external interop. Local Plan 156/158 tests remain valid evidence for cases the external driver cannot force cleanly.

Do not patch i2pd solely to manufacture an otherwise inaccessible token state.

## 10. Independent I2NP test payloads

Use valid encoded I2NP messages already supported by the common protocol layer, with ephemeral/test-only values and deterministic content safe to upload as hashes/lengths.

Test set should include:

- one small message fitting one SSU2 datagram;
- one message large enough to require SSU2 I2NP fragmentation/reassembly;
- multiple messages in both directions to prove the link is not one-shot.

Evidence artifact should record:

```text
message type/category
encoded length
SHA-256 digest
send direction
received digest/length
```

Do not upload arbitrary application/private payloads.

## 11. Malformed/spoof/resource evidence

The external lane is not a fuzzing harness, but it must preserve at least a compact final boundary check:

- random short/oversized datagram -> cheap drop;
- unsupported SSU2 version -> safe rejection;
- spoofed source for a token/path candidate -> no unbounded state;
- one authenticated tag-corruption case if the harness can inject it without patching external source; otherwise retain the local Plan 157 proof and record that the external lane does not claim this row.

Record before/after i2pr resource counters/snapshots for pending/active/reassembly state without logging peer secrets.

## 12. Java I2P secondary lane

Attempt Java I2P 2.13.0 only after i2pd mandatory directions pass.

Proceed if it can be done with a narrow unprivileged configuration on localhost and no major harness architecture.

Useful secondary evidence:

```text
i2pr -> Java responder direct SSU2 v2
Java initiator -> i2pr responder direct SSU2 v2
```

If blocked by packaging/process/router bootstrap complexity unrelated to SSU2 wire behavior:

- record exact blocker;
- retain Java as secondary-reference debt;
- do not downgrade the passing i2pd result;
- do not create another rootless/VM orchestration program inside M8.

## 13. Final evidence ledger

Create a fail-closed external harness, for example:

```text
tests/integration/ssu2/run-independent.sh
scripts/check-ssu2-acceptance-evidence.sh
```

Every required final row must be command-derived. Adopt the Plan 151 pattern deliberately.

Required final rows should include at least:

```text
local-ssu2-real-udp
local-loss-reorder
path-validation
transport-selection
peer-test
relay
external-i2pd-i2pr-to-i2pd
external-i2pd-i2pd-to-i2pr
external-i2np-small
external-i2np-fragmented
token-retry
malformed-cheap-drop
resource-baseline
plan155-160-focused-regressions
workspace-gates
```

A static checker must reject literal unconditional `passed` records for required rows.

Generated evidence must include:

- i2pr commit;
- exact i2pd commit;
- optional Java commit/result;
- OS/runner/toolchain;
- bind policy (`127.0.0.1`, ephemeral ports);
- executed command/test behind each row;
- exit/result status;
- sanitized byte counts/digests/counters;
- explicit known limitations.

No raw private keys/tokens/session secrets/environment dump.

## 14. Hosted workflow

Add a manual workflow:

```text
.github/workflows/ssu2-external.yml
```

Requirements:

- `workflow_dispatch` only initially;
- Ubuntu 24.04 or current pinned/declared supported Ubuntu runner;
- no sudo unless the repo's normal package build absolutely requires a preinstalled tool; prefer runner-provided toolchain and user-local build;
- no Docker/namespaces/VM/systemd;
- fetch exact i2pd source revision;
- build external driver/process;
- run evidence-integrity checker;
- run external SSU2 matrix;
- upload sanitized evidence even on failure;
- bounded job timeout;
- no public I2P network access/participation beyond GitHub source fetch.

The workflow must run on the exact closing revision.

## 15. Full validation floor

Before final closure, run and record:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-ssu2-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
cargo deny check advisories bans sources
```

Plus exact focused Plan 155–160 test commands from their status records.

## 16. Documentation/support closure

On success update:

```text
plans/161-status.md
plans/154-status.md
plans/README.md
README.md
AGENTS.md
.opencode/skills/* relevant local/architecture/SSU2 skill
docs/architecture/i2pr-transport-ssu2.md
docs/architecture/i2pr-runtime.md
specs/protocols/09-ssu2.md
specs/SOURCES.md
specs/IMPLEMENTATIONS.md
specs/support.toml
docs/protocol-support.md via generator if applicable
```

Create a dedicated local-development SSU2 skill only if the repo's skill conventions make it useful; do not create documentation for its own sake.

Support claims must remain precise. A plausible final machine-readable classification:

```text
milestone8_ssu2_v2_local = passed
milestone8_ssu2_v2_interop = passed-via-i2pd-2.61.0
ssu2_direct_ipv4 = passed
ssu2_ipv6_structure = passed
ssu2_ipv6_interop = passed | infrastructure-limited-debt
ssu2_pq_v3_v4 = deferred
ssu1 = not-implemented
ssu2_public_network = not-claimed
milestone6_interoperable = not-yet-claimed
next_product_layer = milestone9-planning
```

Do not set broad `advertised = true` unless `specs/CONFORMANCE.md` and the support ledger define that direct interoperability is sufficient for the specific surface. Default to experimental/non-advertised if uncertain.

## 17. Acceptance criteria

Plan 161 closes only when **all** mandatory items are true:

1. Plans 155–160 are explicitly passed and focused regressions remain green.
2. exact i2pd commit `635b013a612ff47278ef02acf8580a28e10e26c5` is fetched/verified/built unmodified.
3. external traffic crosses real UDP sockets on loopback.
4. no root/namespaces/container/VM/systemd/public-I2P harness dependency is introduced.
5. i2pr initiator -> i2pd responder v2 handshake authenticates both peers.
6. i2pd initiator -> i2pr responder v2 handshake authenticates both peers.
7. bidirectional small I2NP exchange passes across the independent boundary.
8. bidirectional or at least both-side fragmented I2NP handling is proven sufficiently to exercise fragmentation/reassembly independently; exact expected bytes/digests match.
9. token/Retry behavior is externally exercised to the extent normal i2pd behavior exposes it, with remaining cases explicitly local-evidence only.
10. termination returns i2pr session/tasks/resources to baseline in both directions.
11. compact malformed/spoof cheap-drop/resource checks remain green.
12. no external client/source is patched to match i2pr behavior.
13. Java secondary lane is either passed or documented as nonblocking with exact blocker; it cannot silently disappear from the status.
14. final SSU2 evidence ledger contains no unconditional synthetic pass rows.
15. evidence-integrity checker is green and CI-enforced in the SSU2 external workflow.
16. external artifact records exact commit/provenance/commands and contains no secrets.
17. full local/workspace quality floor passes on closing code.
18. routine CI passes on exact closing commit.
19. manual SSU2 external workflow passes on exact closing commit and uploads evidence.
20. IPv4 direct SSU2 local + independent result is classified passed.
21. IPv6 external status is explicit rather than inferred.
22. PQ v3/v4 remains deferred and SSU1 remains unsupported.
23. no public-network/NetDB/tunnel/destination interop claim is inferred.
24. only after 1–23 pass does `plans/161-status.md` set Milestone 8 closed and `next_product_layer = milestone9-planning`.

## 18. Stop conditions

Stop and write one narrow corrective rather than weakening final acceptance if:

- i2pr and exact-pinned i2pd disagree on a v2 wire behavior that current spec/reference review cannot resolve;
- i2pd cannot be made to expose a real direct SSU2 session without public-network participation and no smaller real-UDP driver is possible;
- the independent test exposes a concrete handshake/data/path defect;
- external success depends on disabling peer authentication, RouterInfo binding, replay/token checks, or resource limits;
- the hosted environment itself prevents localhost UDP or ordinary source build.

If the blocker is external harness packaging rather than protocol behavior, investigate a smaller i2pd-linked driver before broadening infrastructure. Do not repeat the historical Milestone 3 harness build-up.

## 19. Handoff

This is the final Milestone 8 gate. Do not start Milestone 9 until its status record contains exact hosted evidence and explicitly closes M8.