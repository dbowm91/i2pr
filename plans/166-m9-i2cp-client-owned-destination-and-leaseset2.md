# Plan 166 — Milestone 9 I2CP client-owned destination and LeaseSet2 bridge

Status: **blocked until Plan 165 passes**.

Depends on the verified SessionConfig/session-option model from Plan 165.

## 1. Goal

Adapt the existing `i2pr-client` destination product so I2CP can operate a destination whose signing key remains owned by the external client, while i2pr owns the tunnels, routing state, and only the private decryption capability required to receive destination traffic.

This is the central M9 architecture pass. Do not implement a second destination runtime.

Expected closure:

```text
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2
next_executable_plan = 167
```

## 2. Preserve two explicit ownership modes

Introduce a narrow typed distinction, following local naming conventions, equivalent to:

```text
DestinationOwnership::RouterOwned
DestinationOwnership::ClientOwned
```

Router-owned mode retains current Plan 120–152/SAM behavior:

- router owns Destination signing + X25519 secret material;
- router signs/rotates its Standard LeaseSet2;
- SAM private-destination import/export semantics remain unchanged.

Client-owned mode:

- stores the public Destination and derived destination ID/hash;
- never stores/requires the Destination signing private key;
- authorization derives from Plan 165's verified SessionConfig;
- uses the existing destination-specific tunnel pool;
- accepts a client-signed Standard LeaseSet2 only after tunnel readiness;
- stores only required inbound decryption private key material after key/public match verification.

Do not make `DestinationIdentity` artificially optional everywhere if a narrower capability object preserves clearer invariants. Prefer capability-oriented factoring over broad `Option<secret>` fields.

## 3. Required refactor boundary

Audit these current surfaces before changing them:

```text
crates/i2pr-client/src/registry.rs
crates/i2pr-client/src/identity.rs
crates/i2pr-client/src/leaseset.rs
crates/i2pr-client/src/pool.rs
crates/i2pr-client/src/routing.rs
crates/i2pr-client/src/session.rs
crates/i2pr-client/src/dispatch.rs
```

Separate the capabilities actually needed by runtime/routing from full router-owned identity.

Likely capabilities:

```text
public Destination / DestinationId
inbound tunnel pool ownership
outbound tunnel pool ownership
LeaseSet publication source/state
inbound decryption key material
outbound remote-session manager
bounded local payload queues
```

Only router-owned LeaseSet construction/signing should require the local destination signing secret.

## 4. Session creation transaction

A client-owned destination reservation starts only from a `VerifiedSessionConfig` emitted by Plan 165.

Transaction:

1. reserve destination registry capacity;
2. reserve duplicate Destination ownership;
3. project validated tunnel options into `DestinationConfig`;
4. construct the destination tunnel runtime without a signing private key;
5. start/drive tunnel building through the same existing pool machinery;
6. keep session in `PendingLeaseSet`/equivalent until required inbound/outbound tunnel readiness exists;
7. emit the information needed for I2CP `RequestVariableLeaseSet`;
8. only become fully usable after valid client LeaseSet2 + decryption material commits;
9. rollback all resources on any failure/cancellation.

Do not return an “active” session merely because the registry reservation succeeded.

## 5. RequestVariableLeaseSet material

Construct the lease request from actual established inbound tunnel material owned by this destination.

Requirements:

- request no more leases than current policy/tunnel availability supports;
- each requested `(gateway, tunnel_id, end_date)` originates from the real destination pool;
- expiration includes the same safety margins used elsewhere;
- deterministic ordering;
- no fake/stub gateways to satisfy a client codec;
- re-request/rotation behavior is bounded and deadline-driven.

Plan 167 will put this on TCP; this pass exposes a typed action/result only.

## 6. CreateLeaseSet2 acceptance

Implement semantic validation for the M9 Standard LeaseSet2 path.

Required order:

1. structurally decode within Plan 164 ceilings;
2. require Standard LeaseSet2 type supported by M9;
3. validate LeaseSet2 signature against its public signing material according to existing `i2pr-netdb`/`i2pr-client` canonical validation;
4. confirm Destination/session binding;
5. confirm every advertised lease is an allowed lease requested from/owned by this destination runtime;
6. reject unknown, expired, excessively long-lived, duplicate, or foreign tunnel leases;
7. require supported encryption public key type (baseline X25519 / type 4);
8. parse the client-provided private decryption key count/type/length strictly;
9. derive/check each public key against the LeaseSet2 encryption public key before storing the secret;
10. stage publication + decryption capability;
11. atomically commit both or neither.

A valid signature with mismatched private decryption material is a hard session error; do not publish first and discover the mismatch later.

## 7. Secret type

Add the smallest secret-bearing wrapper needed for client-owned inbound decryption material.

Requirements:

- non-`Clone` unless there is a reviewed ownership reason;
- manual/redacted `Debug` or no `Debug`;
- zeroize-on-drop through existing workspace practice;
- no equality over secret bytes except a dedicated test-only verification helper if unavoidable;
- no serialization/persistence in M9;
- no raw secret accessor exposed through `i2pr-api`;
- no logging/evidence.

Do not store the SessionConfig signing private key because the client never supplies it.

## 8. ECIES inbound integration

Wire client-owned X25519 decryption material into the existing ECIES destination receive/session path rather than building an I2CP-specific decryptor.

Required proof:

- new-session garlic encrypted to the LeaseSet2 public key decrypts through the same `EciesSessionManager` semantics as router-owned destinations;
- subsequent tag/ratchet traffic behaves identically;
- destination dispatch routes plaintext only after existing authenticated-decryption checks;
- destruction releases decryption and ratchet state.

If current ECIES ownership is inseparable from `DestinationIdentity`, refactor it into a narrow decryption capability shared by both ownership modes. Preserve all Plan 126/127/152 regressions.

## 9. LeaseSet2 publication lifecycle

Client-owned mode must not silently re-sign a LeaseSet2.

When leases near expiry/change:

- request a new LeaseSet2 from the client through a typed event/action;
- retain the previous valid LeaseSet2 only while its leases are still valid and policy permits;
- stop publishing/using expired client LeaseSet2;
- if the client does not refresh by deadline, degrade/stop the destination rather than forging replacement material;
- reconfiguration that changes tunnel leases triggers a new LeaseSet request.

Publication uses the same validated NetDB/local publication seam already used by destinations; no I2CP-specific NetDB store implementation.

## 10. Router-owned/SAM regression boundary

The existing SAM product is a mandatory regression target.

Do not change:

- `SamPrivateDestination` wire/private-key format;
- one shared router-owned `Arc<DestinationIdentity>` composition;
- SAM `DEST GENERATE`/`SESSION CREATE` semantics;
- Plan 149 self-composed product path;
- Plan 151 final acceptance behavior;
- Plan 152 Streaming/ECIES robustness fixes.

If the ownership refactor breaks the SAM path, fix the shared abstraction in this plan; do not fork behavior.

## 11. Tests

Focused client-owned tests must prove:

- valid verified SessionConfig can reserve a client-owned destination without signing private key;
- invalid/unverified config cannot call the constructor;
- duplicate Destination ownership fails before tunnel allocation;
- actual inbound tunnel material produces the lease request;
- correct signed Standard LeaseSet2 + matching X25519 private key commits;
- mismatched private/public key fails atomically;
- foreign/expired/unrequested/duplicate lease fails;
- encrypted/meta/PQ/legacy LeaseSet types fail explicitly;
- cancellation at each transaction stage restores registry/tunnel/secret baselines;
- lease rotation requests client refresh rather than signing locally;
- ECIES inbound decrypt succeeds with client-owned decryption capability;
- secret wrapper redaction/non-Clone expectations are compile/runtime tested where practical;
- router-owned destination tests remain unchanged/green.

Minimum focused commands:

```text
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
bash scripts/check-i2cp-vectors.sh
bash scripts/check-sam-acceptance-evidence.sh
```

plus the Plan 163 workspace floor.

## 12. Documentation

Update:

- `docs/architecture/i2pr-client.md` with the two ownership modes/capabilities;
- `docs/architecture/i2pr-api.md` only for typed I2CP actions, not secret ownership;
- security model with client-provided decryption-key lifecycle;
- I2CP dossier/CONFORMANCE with Standard LeaseSet2/X25519-only M9 scope;
- support ledger/status.

## 13. Acceptance criteria

Plan 166 closes only when:

1. one destination runtime supports explicit router-owned and client-owned modes without duplicate routing/pool stacks;
2. client-owned construction requires verified SessionConfig but not the client's signing private key;
3. lease requests derive from real destination inbound tunnels;
4. Standard LeaseSet2 signature, destination binding, lease ownership, expiry, and encryption type are validated;
5. client decryption private key is proven to match LeaseSet2 public key before commit;
6. publication and decryption-key installation are atomic;
7. client-owned inbound ECIES traffic uses the existing authenticated destination session path;
8. client-owned lease refresh never causes i2pr to synthesize/sign a replacement;
9. cancellation/destruction zeroizes/releases decryption/session state;
10. unsupported LeaseSet/key types fail explicitly;
11. SAM router-owned product/final-acceptance regressions pass unchanged;
12. dependency/runtime/secret boundaries remain green;
13. workspace floor and exact-head CI pass;
14. `plans/166-status.md` records evidence and the ownership invariant.

## 14. Stop conditions

Stop if the only apparent implementation requires storing a client signing private key, duplicating `DestinationRuntime`, weakening LeaseSet2 validation, or bypassing the existing ECIES session manager. Write a narrow architecture corrective instead.

## 15. Handoff

After Plan 166 passes, execute Plan **167**.