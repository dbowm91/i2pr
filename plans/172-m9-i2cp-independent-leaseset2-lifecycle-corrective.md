# Plan 172 — Milestone 9 independent LeaseSet2 lifecycle corrective

Status: **active corrective; Milestone 9 final acceptance is reopened until this plan passes**.

This plan is intentionally narrow. It does not rewrite I2CP, destination routing, or the Plan 170 external lane. It corrects one acceptance gap: Plan 170 proved independent Java/go-i2cp wire compatibility and bidirectional application traffic, but it did not execute the complete independent client-owned LeaseSet2 lifecycle required by Plan 170 §5/§16.

## 1. Goal

Restore the original Milestone 9 acceptance contract with protocol-correct evidence:

```text
CreateSession
  -> router has a real usable inbound destination tunnel
  -> RequestVariableLeaseSet with >= 1 real Lease
  -> unmodified client constructs/signs Standard LeaseSet2
  -> CreateLeaseSet2 carries the LS2 + X25519 decryption key material
  -> i2pr validates and installs both atomically
  -> session becomes usable
  -> independent client application traffic passes
```

The required final result is **not** public-I2P participation. It is a bounded localhost I2CP product using a legitimate local zero-hop tunnel profile so the LeaseSet2 lifecycle can complete without root, namespaces, VMs, containers, systemd, a public I2P network, or an external router.

Expected closure:

```text
plan_170_external_wire_data_plane = retained-passed
plan_172 = passed-m9-i2cp-independent-leaseset2-lifecycle-corrective
milestone9_i2cp_independent_clients = passed-via-plan170-and-plan172
milestone9_i2cp_independent_leaseset2 = passed-via-plan172
milestone9_final_acceptance = closed-via-plan172
next_product_layer = milestone10-planning
```

Until those criteria pass:

```text
milestone9_final_acceptance = reopened-by-plan172
next_executable_plan = 172
next_product_layer = milestone9-i2cp-corrective
```

## 2. Trigger / why Plan 170 is not sufficient by itself

Plan 170's plan-of-record required both counted independent clients to create modern client-owned Standard LeaseSet2/X25519 sessions through normal public client APIs. The Java trajectory specifically required the normal I2CP client API to respond to the router's LeaseSet request and reach usable state.

The landed Plan 170 evidence is still valuable and must be retained, but its status records two deviations:

1. the counted Java driver bypasses `I2PSession.connect()` and drives `net.i2p.data.i2cp` wire primitives directly;
2. external sessions do not install a LeaseSet2.

The current Java driver also implements its own TCP/frame loop and sets `i2cp.dontPublishLeaseSet=true`. The latter is valid for an unpublished local client but **does not mean "skip LeaseSet creation"**. In normal I2CP, an unpublished LeaseSet is still created/installed so replies can work; publication is the part that is suppressed.

Do not delete or relabel the Plan 170 wire/data-plane rows. They prove independent codec/message compatibility and digest-matched bidirectional payload transfer. Plan 172 adds the missing lifecycle proof.

## 3. Normative/reference research basis

Implement clean-room behavior from specifications and observed public reference behavior; do not copy reference implementation code.

### 3.1 Official I2CP specification

Current source:

- https://geti2p.net/en/docs/protocol/i2cp
- specification index identifies the I2CP document as updated 2025-07 and accurate for I2P 0.9.67.

The required initialization ordering is explicit:

```text
protocol byte 0x2a
GetDate / SetDate
CreateSession
await RequestLeaseSet from router (inbound tunnels built)
CreateLeaseSet / CreateLeaseSet2 from client
client may now initiate or receive connections
```

Relevant options include:

```text
inbound.allowZeroHop   default true
outbound.allowZeroHop  default true
i2cp.dontPublishLeaseSet  client-oriented publication policy
```

A zero-hop tunnel is a normal I2P tunnel form where gateway and endpoint are the same router. It is not a zero-lease LeaseSet.

### 3.2 Exact-pinned Java I2P reference

Retain the Plan 170 pin:

```text
repository = i2p/i2p.i2p
version    = 2.13.0
commit     = 9134f808337b401e8e53c73734c81fab04280c9d
```

Reference observations on that exact revision:

- `core/java/src/net/i2p/client/impl/I2PSessionImpl.java`
  - `connect()` waits until the session has a non-null LeaseSet;
  - the wait may extend to five minutes before `No tunnels built...` failure;
  - `setLeaseSet()` releases the wait.
- `core/java/src/net/i2p/client/impl/RequestLeaseSetMessageHandler.java`
  - handles router LeaseSet requests;
  - constructs Standard LeaseSet2 when supported;
  - fills it from the leases supplied by the router;
  - signs it and sends CreateLeaseSet/CreateLeaseSet2 through normal client behavior;
  - calls `session.setLeaseSet(...)` only after that path completes.
- `router/java/src/net/i2p/router/tunnel/pool/TunnelPool.java`
  - explicitly supports zero-hop fallback tunnels when policy allows zero-hop;
  - zero-hop is represented as a local fallback instead of running a remote tunnel build.

### 3.3 Exact-pinned go-i2cp reference

Retain the Plan 170 pin:

```text
repository = go-i2p/go-i2cp
commit     = b529ee1c10a6011558b4d69fc9436a4afc489eac
```

Reference observations:

- `CreateSessionSync()` returns after `SessionStatus(Created)` and cancels its temporary ProcessIO context. Do not mistake this convenience return for completed LeaseSet establishment.
- the client state machine then expects `RequestVariableLeaseSet` and records `SessionStateAwaitingLeaseSet`;
- the normal message processor handles RequestVariableLeaseSet, constructs/signs a Standard LeaseSet2, generates/retains X25519 lease-set key material, and sends CreateLeaseSet2.

The counted Go trajectory must keep a public `ProcessIO` path alive long enough for this normal handler to run; do not patch the library and do not manually encode CreateLeaseSet2 in the external driver.

## 4. Current i2pr diagnosis

### 4.1 Plan 166 already has the correct LS2 transaction

`i2pr-client` already provides the security-critical part:

```text
DestinationRuntime::new_client_owned(...)
LeaseSetLifecycle::take_client_refresh_request(...)
DestinationRuntime::install_client_lease_set2(...)
InboundDecryptionCapability
```

`install_external` / `install_client_lease_set2` already require:

- client-owned destination binding;
- valid Standard LeaseSet2 signature;
- non-expired LeaseSet2;
- non-empty leases;
- each lease owned by the destination's real inbound tunnel pool;
- no duplicates;
- supported X25519 encryption key;
- supplied decryption capability matching the LeaseSet2 public key;
- atomic commit of LeaseSet2 + decryption capability.

Do **not** weaken these checks to accept the current empty request.

### 4.2 Current daemon workaround is the defect

The Plan 170 daemon path currently emits immediately after CreateSession:

```text
SessionStatus(Created)
RequestVariableLeaseSet { leases: Vec::new() }
```

That request is not sourced from `DestinationRuntime::take_client_refresh_request()` / a usable inbound pool. It exists only to avoid waiting for tunnels.

Remove this as counted product behavior. A zero-lease request must never satisfy the independent-LS2 evidence row.

### 4.3 Current tunnel model does not represent zero-hop

The current remote tunnel substrate intentionally requires one or more remote hops:

- `ExploratoryPoolConfig::MIN_HOPS = 1`;
- `EstablishedTunnel::new()` rejects an empty hop list;
- `TunnelRegistration::new()` rejects an empty hop list;
- `DestinationTunnelPool` accepts only real one-shot `EstablishedMaterial` and derives Lease metadata from those registrations;
- Plan 165 currently rejects `inbound.allowZeroHop=true` / `outbound.allowZeroHop=true` because the router policy had no zero-hop support.

Those constraints are correct for **remote established tunnels**. Do not delete them globally.

The missing primitive is an explicit local zero-hop tunnel kind.

## 5. Architecture decision: explicit local zero-hop kind

Implement zero-hop as a first-class local path, not as fake remote `EstablishedMaterial`.

Required conceptual split:

```text
Tunnel path
  RemoteEstablished
    - >= 1 remote hop
    - per-hop LayerKeys
    - current EstablishedTunnel / EstablishedMaterial invariants

  LocalZeroHop
    - gateway == endpoint == this router
    - no remote peer vector
    - no remote hop LayerKeys
    - non-zero local tunnel id
    - bounded lifetime / expiry
    - explicit direction
```

Names may differ, but the distinction must be typed. A boolean hidden in an otherwise remote material type is not preferred if it leaves impossible fields/sentinels.

### Non-negotiable invariants

- `EstablishedTunnel::new()` continues rejecting an empty remote-hop list.
- `ExploratoryPoolConfig::MIN_HOPS` remains 1 for remote exploratory builds.
- no all-zero router hash is used as a Lease gateway;
- no `u32::MAX`/zero tunnel-id sentinel is used as a usable zero-hop Lease tunnel id;
- zero-hop entries contain no fabricated layer keys;
- zero-hop entries are never offered to the remote participant/IBGW/OBEP crypto data plane;
- remote one-plus-hop behavior and vectors remain unchanged.

## 6. Local zero-hop destination pool requirements

The zero-hop path exists to make the localhost client-owned destination product protocol-complete. It must be production code under the destination/tunnel boundary, not a test-only fixture.

Likely files:

```text
crates/i2pr-tunnel/src/config.rs
crates/i2pr-tunnel/src/pool.rs
crates/i2pr-tunnel/src/established.rs (only if a common enum/wrapper belongs here)
crates/i2pr-tunnel/src/lib.rs
crates/i2pr-client/src/config.rs
crates/i2pr-client/src/pool.rs
crates/i2pr-client/src/registry.rs
```

Required behavior:

### 6.1 Inbound zero-hop

Register one bounded local inbound tunnel with:

```text
direction = inbound
gateway router hash = actual local router identity hash
gateway receive tunnel id = fresh non-zero local receive id
local endpoint = same router
created_at / expiry = normal bounded tunnel lifetime
```

`DestinationTunnelPool::inbound_lease_sources(now)` must return this as a normal `InboundLeaseSource` while usable. The resulting Lease2 is therefore non-empty and points at the local router hash/tunnel id.

### 6.2 Outbound zero-hop

Represent a local outbound path so `DestinationTunnelPool::is_usable()` has an honest outbound route. It must not pretend there is a first remote peer. Local cross-destination routing may continue to use the existing Plan 168 local shortcut; the zero-hop entry is the destination tunnel product's local route, not a fabricated transport peer.

### 6.3 Ownership and expiry

Require tests for:

- one local zero-hop inbound + outbound makes the destination pool usable;
- inbound lease source has the exact local router hash and registered tunnel id;
- capacity/max+1 remains bounded;
- duplicate tunnel id is rejected/idempotent according to existing pool policy;
- expiry removes the lease source;
- release/destroy returns zero-hop entries to baseline;
- zero-hop material cannot be activated as remote `EstablishedMaterial`;
- a remote tunnel registration still rejects zero hops.

## 7. Router identity / local context

A valid zero-hop Lease gateway must be the router that owns the local path.

Do not generate an arbitrary per-session gateway hash inside `i2pr-client`.

Provide the I2CP destination composition with a small runtime-neutral local-router context containing at least:

```text
local_router_hash
zero_hop_tunnel_id_allocator or typed allocation seam
```

Production daemon composition should derive the hash from the loaded router identity. The external localhost example/lane may create a fresh ephemeral **router identity** for that process, but the Lease gateway must be that process's actual configured identity hash and remain stable for the session.

Do not expose router private keys to the I2CP session state or evidence.

If wiring the existing daemon bootstrap identity into the I2CP service is straightforward, prefer that. Do not create a second persistent router identity store.

## 8. I2CP option correction

Plan 165 currently rejects zero-hop requests. Correct the I2CP profile narrowly.

Official I2CP permits:

```text
inbound.allowZeroHop=true
outbound.allowZeroHop=true
inbound.length=0
outbound.length=0
```

For this corrective, support the explicit local profile:

```text
inbound.length=0
outbound.length=0
inbound.quantity=1
outbound.quantity=1
inbound.backupQuantity=0 (if present)
outbound.backupQuantity=0 (if present)
inbound.allowZeroHop=true
outbound.allowZeroHop=true
i2cp.dontPublishLeaseSet=true
i2cp.leaseSetType=3
i2cp.leaseSetEncType=4
```

Exact option defaults may follow the external library if it omits equivalent values, but the final projection must be unambiguous.

Recommended typed design:

```text
DestinationTunnelMode::{Remote { length_hops }, LocalZeroHop}
```

or an equivalent explicit mode in `DestinationConfig` / `ProjectedPolicy`.

Do not lower the remote tunnel minimum from one hop just to make `length_hops=0` pass through `BoundedTunnelPoolConfig`.

`allowZeroHop=false` + `length=0` must reject. Mixed inbound/outbound local/remote modes may remain unsupported in M9 if not required; reject explicitly rather than silently rewriting them.

## 9. Correct I2CP session lifecycle

The daemon must no longer equate CreateSession success with a usable client destination.

Add/retain an explicit per-session phase such as:

```text
Reserved
CreatedAwaitingTunnels
AwaitingLeaseSet2
Usable
Stopping
```

Names may differ.

Required order:

1. receive and verify SessionConfig;
2. reserve/construct the client-owned destination;
3. send `SessionStatus(Created)` according to the supported I2CP ordering;
4. establish/register the local zero-hop inbound/outbound destination routes;
5. derive the Lease request from the destination pool using the Plan 166 request seam;
6. require request lease count >= 1;
7. send RequestVariableLeaseSet carrying exactly those real lease sources;
8. receive CreateLeaseSet2 from the client;
9. execute existing Plan 166 signature/lease/key atomic validation;
10. mark the session `Usable` only after install succeeds;
11. allow normal data-plane sends/receives only according to that usable state;
12. destroy/disconnect releases LeaseSet2, decryption capability, zero-hop entries, session reservation, and TCP resources.

If the normal Java high-level API requires SessionStatus before RequestVariableLeaseSet, retain that ordering. The key correction is that the request comes from the real local destination pool rather than an empty vector.

Do not create or sign a client-owned LeaseSet2 in i2pr. The client remains the signing authority.

## 10. `dontPublishLeaseSet` semantics

Correct any documentation/tests that currently imply `i2cp.dontPublishLeaseSet=true` means no LeaseSet2 is necessary.

For the Plan 172 localhost profile:

```text
dontPublishLeaseSet=true
  => install/retain the client-signed LS2 locally
  => do not publish it to public NetDB
  => local/cross-client routing may use it
```

No public NetDB write is required for closure.

## 11. Independent Java trajectory — counted high-level API only

Retain the exact Java pin from Plan 170.

Create a separate counted Java driver rather than deleting the raw diagnostic driver. Suggested path:

```text
tests/integration/i2cp/external/java/i2cp_java_session_driver.java
```

The counted path must use the normal public client API, for example the public `I2PClient` / `I2PSession` construction and `I2PSession.connect()` flow. Exact classes should be verified against the pinned source during implementation.

The counted driver must **not** implement I2CP framing itself.

Static checker should reject in the counted driver at least direct uses equivalent to:

```text
java.net.Socket
writeFrame/readFrame helpers
manual protocol byte writes
I2CPMessageHandler.readMessage for the counted lifecycle
manual CreateLeaseSet2Message construction
```

Using those primitives in the retained Plan 170 diagnostic driver remains allowed, but those rows are not the Plan 172 lifecycle rows.

Session options should request the local zero-hop profile and Standard LS2/X25519 while keeping publication disabled.

Required Java facts:

- `I2PSession.connect()` returns within a bounded deadline;
- router observed RequestVariableLeaseSet with >=1 real lease;
- router observed CreateLeaseSet2 produced by Java's normal handler;
- installed LS2 contains >=1 lease matching the router's zero-hop pool;
- installed LS2 uses supported X25519 encryption key material;
- router installed matching decryption capability;
- session is marked usable only after that install;
- close returns resources to baseline.

## 12. Independent go-i2cp trajectory — public message loop

Retain exact go-i2cp pin.

The counted driver must use public library APIs only. It may work around the library's `CreateSessionSync()` convenience behavior by keeping/restarting a public `ProcessIO` loop; it may not encode RequestVariableLeaseSet/CreateLeaseSet2 itself.

Prefer a trajectory that starts a persistent public ProcessIO loop and uses the normal session creation API, then waits for router-side sanitized lifecycle evidence.

Required Go facts mirror Java:

- connection/version negotiation;
- SessionStatus Created;
- RequestVariableLeaseSet with >=1 real lease consumed by the library;
- library-generated client-signed CreateLeaseSet2 observed by i2pr;
- successful Plan 166 install;
- matching X25519 decryption capability;
- session usable only after install;
- cleanup baseline.

If exact-pinned go-i2cp contains a concrete bug preventing the normal LeaseSet2 handler from completing, capture it precisely. Do not patch the library. A genuine upstream blocker is a Plan 172 stop condition requiring a narrow disposition amendment; do not silently count the Plan 170 raw wire path instead.

## 13. Cross-client application evidence after LS2 install

Only after **both** independent sessions have completed LeaseSet2 installation, run the retained cross-client matrix:

```text
Java -> Go small
Java -> Go large/near-limit
Go -> Java small
Go -> Java large/near-limit
```

Prefer using each library's public session send/receive API in the counted Plan 172 path. If go-i2cp exposes protocol/port metadata and Java's high-level API does not, compare what each public API actually exposes and retain digest equality as mandatory.

No message may be accepted for the Plan 172 cross-client row before both sessions have `lease_set_installed = true` (or equivalent typed state).

The existing Plan 170 raw wire/data-plane rows remain regression evidence and should also stay green.

## 14. Required local negative tests

Add narrowly named tests proving:

1. empty RequestVariableLeaseSet is never emitted for the counted local-zero-hop profile;
2. a client session cannot become usable before CreateLeaseSet2 commits;
3. zero-lease CreateLeaseSet2 is rejected;
4. a lease with the wrong gateway router hash is rejected;
5. a lease with an unowned/wrong tunnel id is rejected;
6. expired lease is rejected;
7. duplicate lease is rejected;
8. mismatched X25519 decryption private key is rejected atomically;
9. invalid LS2 signature is rejected atomically;
10. rejection leaves no installed LS2/decryption capability and does not leak the zero-hop pool;
11. `allowZeroHop=false` with length 0 rejects;
12. remote one-plus-hop tunnel constructors still reject empty hop lists;
13. session destruction releases zero-hop inbound/outbound entries;
14. sibling I2CP session remains usable when one session is destroyed;
15. SAM router-owned destination behavior is unchanged.

## 15. External evidence lane changes

Extend the existing Plan 170 runner/checker rather than introducing another harness stack:

```text
tests/integration/i2cp/run-independent.sh
scripts/check-i2cp-acceptance-evidence.sh
.github/workflows/i2cp-external.yml
```

Add command-derived mandatory rows at minimum:

```text
java-high-level-connect
java-nonempty-lease-request
java-client-generated-leaseset2
java-leaseset2-installed
go-public-session-lifecycle
go-nonempty-lease-request
go-client-generated-leaseset2
go-leaseset2-installed
zero-hop-lease-gateway-owned
zero-hop-lease-tunnel-id-owned
sessions-usable-after-ls2-only
java-to-go-small-after-ls2
go-to-java-small-after-ls2
java-to-go-large-after-ls2
go-to-java-large-after-ls2
external-clean-resource-baseline
```

Retain existing Plan 170 rows for wire/version/status/bandwidth and raw digest evidence where they remain useful.

Every new row must be derived from executed command/result facts. Router-side lifecycle observations must expose only sanitized facts such as counts, hashes/digests, booleans, session ids, tunnel ids where safe, and algorithm identifiers. Never emit private signing/decryption bytes.

## 16. Evidence checker requirements

Extend `check-i2cp-acceptance-evidence.sh` so it rejects:

- a Plan 172 lifecycle row satisfied by the old raw Java driver;
- counted Java lifecycle source containing manual frame/socket protocol implementation;
- literal/unconditional success rows;
- `lease_count = 0` accepted as lifecycle success;
- RequestVariableLeaseSet constructed from a hard-coded empty vector on the counted path;
- LS2 install row without decryption-key-match evidence;
- cross-client-after-LS2 row if either lifecycle row is absent;
- public/non-loopback I2CP bind;
- public I2P/NetDB requirement;
- external library patching/vendoring;
- private-key/raw-payload evidence fields;
- a skipped/unavailable independent client returning success.

The checker remains routine-Linux CI policy; the full expensive external lane stays manual unless it proves cheap enough to change later.

## 17. Resource and security review

Plan 172 must explicitly audit:

- zero-hop entries are bounded by existing destination pool ceilings;
- tunnel ids are fresh/nonzero and cannot alias an active local route;
- local router hash is public metadata only;
- no new cloneable signing/decryption secret type;
- client signing private key remains client-owned;
- X25519 decryption capability remains non-Clone/redacted/zeroized and is installed only after LS2 validation;
- disconnect during any of `CreatedAwaitingTunnels`, `AwaitingLeaseSet2`, or `Usable` releases all resources;
- no unbounded task/channel/timer is introduced;
- I2CP remains loopback-only and disabled by default;
- zero-hop support cannot be accidentally advertised as public-router tunnel interoperability.

## 18. Validation commands

At minimum run:

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

Focused suites must include:

```text
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
```

Add a narrowly named Plan 172 local lifecycle test rather than overloading Plan 169 files.

Then run the complete external matrix at least twice on the same implementation revision when practical:

```text
bash tests/integration/i2cp/run-independent.sh
bash tests/integration/i2cp/run-independent.sh
```

## 19. Exact hosted closure

Before re-closing M9 require:

- routine CI green on exact final implementation/closure head:
  - Ubuntu quality;
  - macOS quality;
  - MSRV;
  - dependency policy;
- manual `i2cp-external.yml` green on the same exact head;
- preferably two complete external passes on the same implementation revision;
- uploaded sanitized evidence artifact with lifecycle rows;
- final `plans/172-status.md` containing exact SHA, run ids, external pins, evidence artifact id/digest, and commands.

A docs-only status commit after an implementation head must itself receive routine CI; if the external runner is content-identical and the status-only difference is documented, follow the exact-head discipline established in Plans 161/170 and dispatch the external lane on the actual final tree before claiming exact-head closure.

## 20. Documentation / authority updates on closure

On implementation start, authority must state M9 is reopened by Plan 172.

On pass, update coherently:

```text
plans/172-status.md
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

Retain the historical Plan 170 record rather than rewriting history. The corrected final claim should be:

```text
I2CP modern localhost MVP profile:
  independent Java + go-i2cp public-client lifecycle
  real non-empty local zero-hop Lease request
  client-signed Standard LeaseSet2 + X25519 decryption capability installed
  session usability gated on LS2 installation
  bidirectional independent-client application traffic after installation
```

Explicitly retain as unclaimed:

- public I2P participation;
- NetDB publication of the Plan 172 local LS2;
- non-zero-hop external destination tunnel interoperability;
- Milestone 6 mixed-router destination/Streaming/tunnel interop;
- remote/non-loopback I2CP;
- encrypted/meta LeaseSets;
- PQ destination encryption;
- service tunnels/HTTP/SOCKS/IRC (Milestone 10).

## 21. Final acceptance criteria

Plan 172 closes and M9 may be re-closed only when all are true:

1. Plan 170's original independent-client criterion is explicitly restored rather than weakened.
2. Zero-hop is represented as a typed local tunnel form, not empty remote `EstablishedMaterial`.
3. Existing remote tunnel minimum-hop and per-hop crypto invariants remain unchanged.
4. The I2CP local-zero-hop option profile is explicit and bounded.
5. The Lease request is derived from the destination's actual local zero-hop inbound pool.
6. Every counted RequestVariableLeaseSet contains at least one lease.
7. Each lease gateway equals the actual local router hash and each tunnel id is owned by that pool.
8. Java 2.13.0 exact pin is unmodified and counted through high-level public `I2PSession.connect()`-style API, not manual wire framing.
9. Java's normal library handler produces the Standard LeaseSet2/CreateLeaseSet2 response.
10. go-i2cp exact pin is unmodified and its public ProcessIO/session machinery produces CreateLeaseSet2.
11. Both client-generated Standard LeaseSet2 records pass existing Plan 166 signature, ownership, expiry, X25519, and decryption-key matching checks.
12. Both sessions become usable only after the LS2 transaction commits.
13. Java->Go and Go->Java small + larger payloads pass after both LS2 installs, with digest equality.
14. Destroy/disconnect returns connections, sessions, destinations, zero-hop routes, LS2/decryption capability, and pending state to baseline.
15. Empty/foreign/expired/duplicate/key-mismatch/signature-invalid LS2 negatives fail atomically.
16. SAM/M6/M8 retained regressions remain green.
17. Plan 170 raw independent wire/data-plane evidence remains green or is explicitly diagnosed if the external driver split changes its invocation.
18. Evidence checker fails closed on raw-driver substitution, zero-lease success, missing lifecycle rows, pin mismatch, client patching, or private evidence.
19. No root/sudo, namespaces, Docker, VM, systemd, external router, or public I2P network is required.
20. I2CP remains disabled by default and loopback-only.
21. Full workspace floor passes.
22. Exact-head routine CI passes Ubuntu/macOS/MSRV/dependency policy.
23. Exact-head manual external I2CP workflow passes.
24. `plans/172-status.md` records command-derived proof and exact hosted evidence.
25. Authority advances to Milestone 10 planning only after the above pass.

## 22. Stop conditions

Stop and do not re-close M9 if any of these occurs:

- zero-hop can only be implemented by permitting empty remote `EstablishedMaterial` or fake LayerKeys;
- a lease gateway/tunnel id is synthetic and not owned by the local router/pool;
- Java `I2PSession.connect()` still cannot return after a valid non-empty RequestVariableLeaseSet/CreateLeaseSet2 exchange;
- go-i2cp must be patched to send CreateLeaseSet2;
- i2pr must accept a zero-lease LS2 to make a client proceed;
- Plan 166 LS2 signature/lease/key checks must be weakened;
- the only remaining method requires joining public I2P or privileged/containerized networking;
- cross-client messages only work before LS2 by using a private injection seam;
- exact-head routine or external CI is red.

If the zero-hop primitive exposes a broader tunnel-model defect, write a separate narrow tunnel corrective and keep Plan 172 blocked. Do not absorb an unbounded tunnel rewrite into this plan.

## 23. Handoff

Execute Plan 172 before Milestone 10 planning.

The implementation priority is:

```text
A. explicit local zero-hop tunnel representation
B. destination pool + I2CP option projection
C. non-empty real Lease request
D. existing Plan 166 CreateLeaseSet2 atomic install
E. high-level Java + public go-i2cp lifecycle drivers
F. post-LS2 cross-client traffic + cleanup
G. fail-closed hosted evidence + authority re-closure
```

Do not discard Plan 170's existing external evidence; it becomes retained lower-layer evidence underneath this stricter final gate.