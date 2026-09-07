# Plan 170 — Milestone 9 I2CP independent clients and final closure

Status: **blocked until Plan 169 passes**.

This is the final M9 acceptance/evidence gate.

## 1. Goal

Prove that the real `i2pr-daemon` I2CP localhost server is usable by unmodified independent client implementations, harden a fail-closed command-derived evidence lane, and close Milestone 9 only on exact hosted evidence.

Expected closure:

```text
plan_170 = passed-m9-i2cp-independent-clients-and-final-closure
milestone9_i2cp_local_product = passed-via-plan169
milestone9_i2cp_independent_clients = passed-via-plan170
milestone9_final_acceptance = closed-via-plan170
next_product_layer = milestone10-planning
```

## 2. Environment contract

The final lane must remain simple and unprivileged:

```text
root/sudo            = no
Linux namespaces     = no
Docker               = no
VM/Multipass         = no
systemd              = no
public I2P network   = no
I2CP bind            = loopback only
external clients     = exact-pinned, unmodified
GitHub Ubuntu        = supported manual lane
```

Do not run a Java/i2pd router, public testnet, or transport harness merely to test I2CP. The independent artifact is the **client library** talking to i2pr's local I2CP TCP endpoint.

## 3. Exact independent client pins

Mandatory primary:

```text
Java I2P 2.13.0
repository: i2p/i2p.i2p
commit: 9134f808337b401e8e53c73734c81fab04280c9d
role: official/mature Java I2CP client API
```

Mandatory secondary target:

```text
go-i2cp
repository: go-i2p/go-i2cp
commit: b529ee1c10a6011558b4d69fc9436a4afc489eac
role: independent non-Java I2CP client
```

Fetch into ephemeral/cache directories, verify exact commit, build/use without source modification.

If exact-pinned go-i2cp has a concrete blocker:

1. capture the exact compile/protocol failure;
2. determine whether the blocker is i2pr, client usage, or upstream client implementation;
3. fix i2pr if it is our defect;
4. if it is a genuine upstream defect, locate another unmodified independent I2CP client or write a narrow Plan 170 corrective decision;
5. do not replace it with an in-tree raw test and count that as independent evidence.

Final closure should have two counted independent client implementations unless a later explicitly registered corrective changes that criterion with evidence.

## 4. External driver shape

Prefer tiny client programs that import the pinned libraries through their public APIs rather than parsing I2CP themselves.

Create under:

```text
tests/integration/i2cp/external/
```

or an equivalent clearly non-production path:

- one Java client driver;
- one Go client driver.

Drivers may create destinations/options/payloads and record sanitized result facts. They must not contain alternate I2CP protocol implementations beyond what is necessary to call the external library.

No external client private signing/decryption material is written to evidence.

## 5. Mandatory independent trajectories

### 5.1 Java session

The pinned Java client must, through public I2CP APIs:

- connect to `127.0.0.1:<ephemeral-or-7654>`;
- complete GetDate/SetDate/version behavior;
- create an Ed25519/X25519 client-owned destination/session;
- respond to LeaseSet request with Standard LeaseSet2/decryption material through normal library behavior;
- reach active/usable state;
- perform at least one destination lookup/bandwidth query if public API exposes it;
- destroy/close cleanly.

### 5.2 Go session

The exact-pinned Go client must complete the equivalent modern Standard LeaseSet2/X25519 session lifecycle through its public API.

### 5.3 Cross-client application messages

Run both sessions simultaneously through one i2pr daemon:

```text
Java -> i2pr I2CP/destination layer -> Go
Go   -> i2pr I2CP/destination layer -> Java
```

For each direction require:

- one small payload;
- one larger/near-M9-limit payload that remains practical for both libraries;
- byte-for-byte payload digest match;
- protocol/source/destination-port metadata match where exposed;
- expected MessageStatus/nonce result according to the declared M9 reliability profile;
- no private in-tree delivery injection.

The clients may coordinate expected hashes/tokens through the test harness, not through router internals.

## 6. Protocol/options negative evidence

At least one independent client trajectory must exercise a protocol-correct failure for an unsupported/invalid session/tunnel option where its API permits constructing one.

If public APIs sanitize invalid options before send, retain command-derived local Plan 169 evidence and record that external injection is not exposed by the client API; do not patch the client merely to force malformed traffic.

Required final ledger explicitly distinguishes:

```text
external-independent
local-product
local-malformed-only
```

## 7. Resource/lifecycle external evidence

External lane must assert sanitized baseline facts after client shutdown:

- active I2CP connections = 0;
- active I2CP sessions = 0;
- client-owned destination reservations = baseline;
- pending status/lookups/lease transactions = 0;
- no leaked listener child tasks according to the available sanitized snapshot.

Do not expose peer destination private material in snapshots.

## 8. Fail-closed acceptance runner

Create:

```text
tests/integration/i2cp/run-independent.sh
```

Responsibilities:

1. verify exact repository head/provenance context;
2. fetch/verify exact external pins or use a verified immutable cache;
3. build external clients without patching;
4. build/start one i2pr daemon with I2CP enabled on loopback only;
5. run required local prerequisite tests/checkers;
6. run Java and Go lifecycle/cross-client trajectories;
7. collect only sanitized command/result facts;
8. stop clients/router and verify cleanup;
9. emit deterministic `evidence.json` and human-readable `evidence.md`;
10. exit nonzero if any mandatory row lacks executed proof.

Avoid broad Python orchestration. Shell plus small external-language drivers is sufficient.

Use bounded startup/readiness/deadline loops. No indefinite sleeps.

## 9. Evidence rows

At minimum derive separate rows for:

```text
i2cp-wire-vectors
session-config-signature-option-local
client-owned-leaseset2-local
i2cp-real-tcp-local-product
i2cp-adversarial-boundedness-local
java-connect-version
java-session-leaseset2
java-cleanup
go-connect-version
go-session-leaseset2
go-cleanup
java-to-go-small
go-to-java-small
java-to-go-large
go-to-java-large
message-status-semantics
bandwidth-or-destlookup
option-disposition
external-clean-resource-baseline
profile-and-unsupported-ledger
```

Exact labels may be normalized during implementation, but every final criterion must map to an executed source of truth.

No row may be assigned `passed` merely because a prior plan/status says it passed.

## 10. Static evidence-integrity checker

Add:

```text
scripts/check-i2cp-acceptance-evidence.sh
```

It must statically reject at least:

- unconditional/literal success for mandatory rows;
- missing command-exit gating;
- missing exact Java/Go pins;
- external-client patch/application commands;
- a runner that skips external clients when unavailable but returns success;
- public/non-loopback I2CP bind;
- evidence files containing known private-key/raw-payload field names;
- missing local-product prerequisite rows;
- external test execution that silently succeeds when required environment/provenance is absent.

Wire the checker into routine Linux CI after it exists. Do not duplicate its logic in YAML.

## 11. Manual hosted workflow

Create:

```text
.github/workflows/i2cp-external.yml
```

Use `workflow_dispatch` initially.

Workflow requirements:

- Ubuntu hosted runner;
- exact Rust toolchain/current MSRV policy as appropriate;
- Java/Go toolchains pinned or version-recorded;
- checkout exact requested commit;
- run I2CP vector + evidence-integrity checkers before external work;
- invoke `tests/integration/i2cp/run-independent.sh`;
- upload sanitized evidence artifact on success/failure where safe;
- fail closed on pin/build/readiness/client/test/evidence failure;
- no secrets required.

Do not add this expensive external lane to every routine push unless later data shows it is cheap/stable enough.

## 12. External driver gating in routine CI

If any Rust integration test requires provisioned Java/Go/i2pr external state, follow the Plan 162 pattern:

- compile it in routine all-target checks;
- mark external execution explicitly ignored/gated;
- routine CI sees ignored, not synthetic pass;
- dedicated external runner selects it explicitly;
- explicit selection without required environment fails closed.

Do not teach routine CI to filter test executables by filename.

## 13. Repeatability

Before final closure, run the complete external matrix at least twice on the same implementation revision if runtime cost is reasonable. A single flaky green after repeated unexplained failures is not sufficient.

Any bounded retry added to handle host scheduling must:

- retry a complete independent attempt with fresh IDs/material;
- have a small named maximum;
- record which attempt succeeded;
- still fail after the final attempt;
- never convert a protocol/authentication failure into retry-until-green without diagnosis.

## 14. Final support/docs update

After external evidence passes, update in one coherent closure pass:

```text
plans/170-status.md
plans/README.md
README.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
specs/support.toml
specs/CONFORMANCE.md
specs/protocols/10-i2cp-service-tunnels.md
docs/architecture/i2pr-api.md
docs/architecture/i2pr-client.md
docs/architecture/i2pr-daemon.md
docs/architecture/tooling.md
```

Exact final claim:

```text
I2CP modern MVP profile over localhost TCP:
  client-owned Standard LeaseSet2 + Ed25519/X25519
  bounded session/options/message/status/lookup/bandwidth behavior
  independent Java + second client interoperability
```

Explicitly retain as unclaimed/deferred:

- non-loopback/remote I2CP and TLS/auth;
- full historical I2CP message/version compliance;
- encrypted/meta LeaseSets;
- PQ destination encryption;
- service tunnels/HTTP/SOCKS/IRC;
- public I2P participation;
- Milestone 6 mixed-router destination/Streaming/tunnel interop.

## 15. Full validation floor

On the final implementation revision run at least:

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
bash scripts/check-i2cp-vectors.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
cargo deny check advisories bans sources
```

Plus focused Plan 169 I2CP final acceptance and retained SAM/M6 regressions.

## 16. Final acceptance criteria

Plan 170/M9 closes only when:

1. Plans 164–169 have explicit passed status records.
2. Java source is exact pin `9134f808337b401e8e53c73734c81fab04280c9d`, unmodified.
3. The counted second client is exact-pinned/unmodified (target go-i2cp `b529ee1c10a6011558b4d69fc9436a4afc489eac`).
4. Both independent clients complete I2CP connection/version negotiation.
5. Both create modern client-owned Standard LeaseSet2/X25519 sessions through normal public client APIs.
6. Java->Go small and larger payloads pass through i2pr with digest equality.
7. Go->Java small and larger payloads pass likewise.
8. MessageStatus behavior matches the documented reliability strength.
9. At least one bandwidth/destination lookup public behavior is independently exercised where APIs permit.
10. Invalid/unsupported option behavior has external evidence where possible and explicit local-only classification otherwise.
11. External shutdown returns sanitized resource baselines to zero/baseline.
12. No external client source is patched/vendored.
13. No public I2P network, root, namespaces, container, VM, or systemd is required.
14. I2CP remains disabled by default and loopback-only.
15. `run-independent.sh` fails closed on missing pins/build/client/results.
16. Every mandatory evidence row is command-derived; no synthetic `passed` bookkeeping exists.
17. Evidence artifacts contain no private keys/raw application payloads.
18. `check-i2cp-acceptance-evidence.sh` passes and is enforced in routine Linux CI.
19. Routine CI is green on the exact final implementation/closure head.
20. Manual `i2cp-external.yml` is green on the exact final implementation/closure head.
21. Preferably two consecutive complete external runs pass on the same implementation head, or any variance is explicitly diagnosed before closure.
22. Support/CONFORMANCE/docs all state the same precise M9 feature profile.
23. Unsupported historical/new I2CP features remain explicit rather than implied supported by version number.
24. No service-tunnel, public-network, M6 mixed-router, encrypted/meta LS, PQ, or remote-I2CP claim is introduced.
25. `plans/170-status.md` records exact closing SHA, workflow run IDs, external pins, commands, artifact identifiers/digests, and final classification.

## 17. Stop conditions

Do not close M9 if:

- only the in-tree raw client passes;
- only one independent client can create a usable session and the second-client criterion has not been explicitly corrected by a registered plan;
- a client must be patched;
- cross-client traffic uses a private injection/setup seam after I2CP session activation;
- evidence is reconstructed from logs without command/result provenance;
- routine or external exact-head CI is red;
- the only way forward is remote/public I2CP exposure.

Write a narrow corrective plan for the concrete blocker.

## 18. Handoff

When all criteria pass, close Milestone 9 and advance planning authority to **Milestone 10: generic service tunnels, HTTP, SOCKS5, and IRC**. Do not implement M10 within this plan.