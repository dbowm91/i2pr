# Plan 223 — M6 Java Destination identity / LeaseSet2 crypto-separation corrective

Status: **registered-ready-m6-java-destination-identity-crypto-separation-corrective**

## 1. Bounded objective

Correct the Java-I2P reverse-send `STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION
(17)` boundary identified by Plan 222 **only after proving which exact Java
status-17 branch is being hit**.

The primary hypothesis is now narrow and source-backed:

1. i2pr-generated router-owned Destinations currently advertise X25519/type 4
   in the **Destination identity/key certificate itself**.
2. Java I2P 2.13.0 `OutboundClientMessageOneShotJob.runJob()` rejects any
   target Destination whose `getEncType()` is not
   `EncType.ELGAMAL_2048`, returning status 17 **before the client-NetDB
   lookup or LeaseSet2 key-selection path**.
3. Java's own modern Destination generator intentionally keeps the legacy
   256-byte Destination encryption-public-key slot as ElGamal/type 0 identity
   material even though it is unused for end-to-end encryption.
4. Modern end-to-end ECIES capability is advertised independently in
   LeaseSet2; the existing Java helper already requests
   `i2cp.leaseSetType=3` and `i2cp.leaseSetEncType=4`.
5. i2pr already has most of this distinction conceptually:
   `DestinationIdentity::from_imported()` documents that Java/i2pd
   Destination public-encryption bytes are legacy/unused, while
   `build_signed_lease_set2()` separately emits an X25519/type-4 LS2 key.
   The bug candidate is the router-owned generation path, which currently
   reuses `ROUTER_CRYPTO_KEY_TYPE` and embeds the destination's X25519 static
   public key directly into the Destination.

Plan 223 must therefore prove the early guard first, correct the identity/LS2
crypto-layer separation if and only if that proof holds, and then rerun the
exact Plan-222 destination lane.

This plan is deliberately narrow. It does **not** own a second unrelated
corrective if status 17 survives the identity fix.

## 2. Authority and source facts

### 2.1 Current i2pr authority

Plan 222 closed as:

```text
plan_222 = passed-m6-java-client-netdb-ocmosj-narrowing-corrective
p222_terminal = P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION
implementation_sha = cd334f838d6c80acbeedbcc2ebf6b1ae612a68fa
```

Retained Plan-222 facts:

- helper client DB resolves to an actual client DB;
- production-equivalent routing key / search width is observed;
- selector is nonempty and contains Router B;
- one nonce-correlated reverse helper send returns status 17;
- no matching reverse TunnelData/payload reaches i2pr inside the frozen
  45-second acceptance window;
- J219-B RouterInfo/bootstrap attribution remains refuted.

### 2.2 Exact pinned Java I2P 2.13.0 source

Pin:

```text
9134f808337b401e8e53c73734c81fab04280c9d
```

Load-bearing source locations:

- `router/java/src/net/i2p/router/message/OutboundClientMessageOneShotJob.java`
- `core/java/src/net/i2p/client/impl/I2PClientImpl.java`
- `core/java/src/net/i2p/client/impl/RequestLeaseSetMessageHandler.java`
- `core/java/src/net/i2p/data/LeaseSet2.java`
- `router/java/src/net/i2p/router/client/ClientMessageEventListener.java`
- `router/java/src/net/i2p/router/LeaseSetKeys.java`

Pinned OCMOSJ has **two distinct status-17 mechanisms that must not be
conflated**.

Early Destination guard in `runJob()`:

```java
if (_to.getEncType() != EncType.ELGAMAL_2048) {
    dieFatal(MessageStatusMessage.STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION);
    return;
}
```

Later LeaseSet2 compatibility selection in `getNextLease()`:

```java
LeaseSetKeys ourKeys = getContext().keyManager().getKeys(_from);
if (ourKeys != null)
    supported = ourKeys.getSupportedEncryption();
else
    supported = LeaseSetKeys.SET_ELG;
_encryptionKey = _leaseSet.getEncryptionKey(supported);
if (_encryptionKey == null) {
    if (_leaseSet.getEncryptionKey() != null)
        return MessageStatusMessage.STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION;
    return MessageStatusMessage.STATUS_SEND_FAILURE_BAD_LEASESET;
}
```

Plan 222 proved status 17, but did not distinguish these two code paths.
Plan 223 must.

### 2.3 Java's own Destination / LS2 separation

Pinned `I2PClientImpl.createDestination(...)` does not generate a live
Destination ElGamal keypair anymore. It fills the legacy 256-byte public-key
slot with non-secret random/repeated bytes and writes a random unused legacy
private-key field. The Destination remains compatible with the legacy
ElGamal/type-0 identity shape.

Pinned `RequestLeaseSetMessageHandler` separately interprets
`i2cp.leaseSetEncType`, generates LeaseSet encryption keys, inserts the
public keys into LS2, and sends the matching private keys in
`CreateLeaseSet2Message`.

Pinned `ClientMessageEventListener.handleCreateLeaseSet()` verifies those
LS2 public/private keys and registers the client `LeaseSetKeys`.

The existing test helper already requests:

```text
i2cp.leaseSetType=3
i2cp.leaseSetEncType=4
```

Therefore Java's own architecture is:

```text
Destination identity legacy encryption field  !=  active LS2 encryption key
legacy Destination field: type 0 / 256-byte unused identity material
Standard LeaseSet2 key:    type 4 / X25519 / 32 bytes
```

Official I2P documentation likewise describes `i2cp.leaseSetEncType` as
the LeaseSet encryption type and LS2 key selection as an independent
supported-type negotiation. Proposal 145 notes that the Destination public
encryption field has been unused for normal end-to-end traffic since the
early I2P client protocol era.

### 2.4 Current i2pr conflict

Current router-owned generation in
`crates/i2pr-client/src/identity.rs::DestinationIdentity::from_private_bytes`
constructs:

```text
Destination public encryption key = X25519(static_secret)
Destination key-certificate crypto type = X25519 / type 4
```

using `ROUTER_CRYPTO_KEY_TYPE`.

Current `build_signed_lease_set2()` independently advertises:

```text
LeaseSet2 encryption key = X25519 / type 4 / identity.static_public_bytes()
```

The imported-destination path already documents the reference behavior and
does not require the Destination's legacy public encryption bytes to equal
the LS2 X25519 key.

This is an internal contradiction that Plan 223 must resolve without changing
router identity crypto.

## 3. Non-negotiable invariants

1. **Do not patch Java I2P.**
2. **Do not use reflection or private-field mutation.**
3. **Do not change the Java 2.13.0 pin.**
4. **Do not change the i2pd 2.61.0 pin.**
5. **Do not use public I2P.**
6. **Do not alter RouterInfo / router-identity X25519 behavior.** Router
   identity crypto and Destination identity crypto are separate concerns.
7. **Do not downgrade LS2 encryption.** Standard LS2 must remain X25519/type 4
   for this lane.
8. **Do not add ElGamal end-to-end encryption implementation.** The legacy
   Destination field is identity-format compatibility material, not the
   active destination message encryption key.
9. **Do not change the Plan-222 45-second reverse-payload acceptance window.**
10. **Do not change tunnel length, quantity, floodfill topology, bootstrap,
    SAM path, or NetDB selector behavior.**
11. **Do not derive legacy Destination filler bytes from the X25519 private
    key.** Identity filler is public non-secret material; secret-derived
    filler creates an unnecessary link.
12. **Do not silently invalidate imported Java/i2pd Destinations or SAM
    private-destination round trips.**
13. **Do not rewrite historical Plan-218/220/222 evidence.**
14. Missing diagnostics are `Unknown`, never an inferred pass/fail.
15. If the early Destination guard is not proved, Plan 223 MUST stop before a
    production identity change.

## 4. Scope

### In scope

- exact pre-fix Destination encryption-type observation in Rust and Java;
- exact hash equality for the same target Destination bytes;
- distinction between the early OCMOSJ Destination guard and later LS2
  key-intersection status-17 branch;
- router-owned Destination generation semantics;
- explicit separation between:
  - legacy Destination public-encryption identity material; and
  - X25519 LS2 / ECIES static key material;
- deterministic constructor/API cleanup required by that separation;
- SAM/private-destination compatibility audit and narrowly necessary updates;
- existing unit/integration tests affected by the identity representation;
- Plan-222 destination-only external requalification;
- bounded post-fix key-intersection diagnostics only if status 17 persists.

### Out of scope

- RouterInfo identity conversion;
- NTCP2/SSU2 router static keys;
- Java topology/bootstrap/floodfill changes;
- client-NetDB selector changes;
- SAM bridge pivot (Plan 205);
- tunnel-length or tunnel-count changes;
- public-network testing;
- PQ LS2 expansion;
- multi-key LS2 policy changes;
- unrelated I2CP feature work;
- broad destination persistence redesign;
- M11 work;
- service-tunnel product changes.

## 5. Required terminal classifications

Plan 223 must emit exactly one final classification for the authoritative
external run.

Pre-fix diagnostic classifications:

```text
P223-PREFLIGHT-DESTINATION-ENC-GUARD-CONFIRMED
P223-PREFLIGHT-DESTINATION-ENC-GUARD-NOT-CONFIRMED
P223-PREFLIGHT-OBSERVABILITY-GAP
```

Post-corrective authoritative terminals:

```text
P223-REVERSE-DELIVERY-PASSED
P223-STATUS17-PERSISTS-SOURCE-KEYS-MISSING
P223-STATUS17-PERSISTS-SOURCE-KEYS-NO-X25519
P223-STATUS17-PERSISTS-TARGET-LS2-NO-X25519
P223-STATUS17-PERSISTS-NO-KEY-INTERSECTION
P223-STATUS17-PERSISTS-UNKNOWN
P223-NEXT-BOUNDARY <documented non-17 status>
P223-EVIDENCE-CONTRADICTION
```

The pre-fix proof is a gate, not the final closure terminal.

## 6. Work package A — freeze Plan-222 evidence and add regression guards

Before modifying behavior:

1. retain `P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION`;
2. retain the exact P222 client lookup preflight;
3. retain nonce-tracked `SEND_TRACKED` and ordered status collection;
4. retain the frozen 45-second payload window;
5. retain legacy `SEND`;
6. retain Plan-220/J219-B refutation evidence;
7. add static guards so Plan 223 cannot:
   - change Java/i2pd pins;
   - change `DATAGRAM_WAIT`;
   - replace the P222 selector probe with a raw-hash shortcut;
   - add public-I2P configuration;
   - set LS2 encryption to type 0 merely to suppress status 17.

Acceptance A:

- existing Plan-222 focused tests remain green before the corrective;
- new static checker fails if any forbidden shortcut is introduced.

## 7. Work package B — prove the early Destination guard on exact bytes

Add a bounded, read-only test-only observation path.

Preferred shape: extend the existing Java raw-destination helper control
surface with a command such as:

```text
INSPECT_DEST <destination-base64>
```

returning only public facts:

```text
DEST_INFO
hash_hex=<32-byte hash>
enc_type_code=<numeric>
enc_type_name=<name-or-unknown>
public_key_len=<bounded integer>
sig_type_code=<numeric>
```

The Rust driver must emit the same facts for the exact local
`DestinationIdentity::destination()` bytes before publication.

Required cross-checks:

```text
rust_destination_hash == java_parsed_destination_hash
rust_destination_enc_type == java_parsed_destination_enc_type
rust_destination_public_key_len == java_parsed_public_key_len
```

The pre-fix classifier is:

```text
if exact hashes/types match
and Java parsed target enc type != ELGAMAL_2048/type 0
and the same tracked send produces status 17
then
    P223-PREFLIGHT-DESTINATION-ENC-GUARD-CONFIRMED
else if exact observation proves target enc type == ELGAMAL_2048/type 0
then
    P223-PREFLIGHT-DESTINATION-ENC-GUARD-NOT-CONFIRMED
else
    P223-PREFLIGHT-OBSERVABILITY-GAP
```

Do **not** infer the branch solely from current Rust source. The exact
Destination bytes used by the external lane must be observed by both
implementations.

Stop condition B:

- if guard is not confirmed, do not perform WP D production changes;
  proceed only to WP C diagnostics and close/stall with a new successor plan.

## 8. Work package C — bounded status-17 branch discriminator

This work package is diagnostic-only and is required both as a pre-fix
cross-check and as the post-fix fallback if status 17 persists.

At the same post-bootstrap/pre-send epoch as Plan 222, expose read-only Java
facts for:

### C1. Source helper LeaseSetKeys

For the exact helper/source Destination:

```text
source_hash_match
source_keys_present
source_supported_types = bounded ordered numeric set
source_supports_elgamal
source_supports_x25519
```

Use `RouterContext.keyManager().getKeys(sourceHash)` /
`LeaseSetKeys.getSupportedEncryption()` through public/read-only APIs.

### C2. Target LS2 as Java actually stores/parses it

For the exact i2pr target Destination hash in the helper client DB:

```text
target_ls_present
target_ls_type
target_destination_hash_match
target_destination_enc_type
target_key_count
target_key[0..N].type
target_key[0..N].length
target_has_x25519
```

Bound N to the protocol maximum / existing LS2 key limit.

### C3. Exact Java compatibility intersection

Compute read-only:

```text
selected_key_type = targetLeaseSet.getEncryptionKey(sourceSupportedTypes)
```

or an equivalent test-only same-package/public-API probe that exactly
reproduces the pinned method semantics without mutating Java state.

Record:

```text
selected_key_present
selected_key_type
```

If `source_keys_present=false`, separately record that pinned OCMOSJ would
use `LeaseSetKeys.SET_ELG`.

C diagnostics MUST NOT themselves register keys, install LeaseSets, or alter
the client DB.

## 9. Work package D — separate router-owned Destination identity from LS2 X25519

Execute only after
`P223-PREFLIGHT-DESTINATION-ENC-GUARD-CONFIRMED`.

### D1. Introduce explicit destination identity semantics

Do not reuse `ROUTER_CRYPTO_KEY_TYPE` for router-owned Destination
construction.

Introduce explicit naming for the two independent concepts, for example:

```text
DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE = ElGamal/type 0
DESTINATION_LS2_CRYPTO_TYPE             = X25519/type 4
```

Names may differ, but call sites must make the separation obvious.

### D2. Generated Destination public-key slot

Router-owned generated Destinations must encode the standard legacy identity
shape expected by Java:

- encryption type: ElGamal/type 0;
- public-key slot length: 256 bytes;
- contents: injected-random public non-secret filler/identity material;
- signing type: existing Ed25519/type 7;
- key certificate/padding: canonical for the selected signing type and legacy
  encryption type.

The filler must come from the caller-provided CSPRNG in
`DestinationIdentity::generate()`.

Do not generate or implement a real ElGamal private key for message
encryption. This slot is not the active LS2 key.

### D3. Keep LS2 X25519 independent

`DestinationIdentity` must continue owning its existing X25519 static
private key.

`DestinationIdentity::static_public_bytes()` must continue returning the
public key derived from that X25519 secret.

`build_signed_lease_set2()` must continue emitting exactly one ordinary
X25519/type-4 encryption key using those static public bytes.

Do not source LS2 encryption material from
`destination.public_key().as_bytes()` after this corrective.

### D4. Deterministic reconstruction API

The current `from_private_bytes(signing, static_secret, padding)` does not
carry enough independent public identity material once the legacy Destination
slot is separated from X25519.

Perform a call-site audit before changing it.

Preferred direction:

- introduce an explicit constructor accepting:
  - signing secret;
  - X25519 static secret;
  - legacy Destination public identity bytes;
  - exact padding/certificate material as needed;
- make deterministic tests provide deterministic **public filler** explicitly;
- keep `generate()` responsible for random filler.

Do not derive the legacy public field from:

- X25519 static private bytes;
- Ed25519 signing private bytes;
- another secret-bearing field.

If retaining a compatibility wrapper is materially simpler, it must be
test-only or derive filler from explicit non-secret caller input. Document the
choice.

### D5. Imported destination path

Preserve `DestinationIdentity::from_imported()` behavior unless a narrow
compile/API correction is required.

Imported Java/i2pd Destination bytes are authority for their public identity
shape and must not be rewritten into X25519 Destination key certificates.

### D6. DestinationPublic semantics

Audit `DestinationPublic` and every consumer of:

- `encryption_public_key_type()`;
- `static_public_bytes()`.

The type already accepts legacy ElGamal and X25519 shapes, but comments and
callers must not imply that the legacy Destination public field is the active
LS2 X25519 key.

For client-owned destinations, LS2/decryption-capability matching remains the
actual X25519 enforcement point.

Do not broaden accepted algorithms beyond the existing type-0/type-4 policy
without a separate plan.

## 10. Work package E — SAM/private-destination compatibility audit

Search all callers of:

```text
DestinationIdentity::generate
DestinationIdentity::from_private_bytes
DestinationIdentity::from_imported
DestinationIdentity::destination
DestinationIdentity::static_public_bytes
signing_seed_bytes
static_secret_bytes
DestinationPublic::from_destination
```

Also inspect the SAM private-destination codec/export/import path.

Required outcomes:

1. public Destination round-trip preserves the new legacy identity field;
2. X25519 LS2 secret/public material still round-trips wherever i2pr promises
   that behavior;
3. Java/i2pd imported Destination compatibility remains green;
4. no serialized private format silently changes without:
   - an explicit migration/version decision; or
   - proof the format was test-only/nonpersistent.

If the corrective would require a broad persistent-key format migration,
STOP. Register a dedicated successor rather than hiding it inside Plan 223.

## 11. Work package F — unit and cross-implementation vectors

Add focused tests proving all of the following.

### F1. Generated Destination shape

```text
generated destination encryption type == ElGamal/type 0
generated destination public key length == 256
generated destination signing type == Ed25519/type 7
generated destination public filler != X25519 public key bytes
```

Two generated identities with the same signing/X25519 secrets but distinct
explicit filler must have distinct Destination hashes in deterministic tests.

### F2. LS2 remains ECIES

For a generated identity:

```text
LS2 type == Standard LS2/type 3
LS2 key count == 1
LS2 encryption type == X25519/type 4
LS2 key length == 32
LS2 key bytes == DestinationIdentity.static_public_bytes()
```

### F3. Import compatibility

At least one Java-compatible legacy Destination fixture/vector must parse and
remain accepted without rewriting its public field.

### F4. Java parser agreement

The test-only Java helper must parse the exact generated i2pr Destination as:

```text
EncType.ELGAMAL_2048 / type 0
public key length 256
same destination hash as Rust
```

This is a required external-lane precondition after the fix.

### F5. No router-identity regression

Existing RouterIdentity tests must still prove X25519/type 4 router crypto.
Add a targeted regression if current coverage does not make this distinction
explicit.

## 12. Work package G — post-fix exact-head destination requalification

Commit all implementation and tests before the authoritative external run.

Record:

```bash
git rev-parse HEAD
test -z "$(git status --porcelain=v1)"
```

Run:

```bash
I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

Reuse Plan 222's constraints:

- fresh scratch RouterContexts;
- same exact Java/i2pd pins;
- 45-second reverse-payload window unchanged;
- status-only observation remains independently bounded;
- maximum three exact-head attempts;
- no tuning between attempts;
- infrastructure failure before the driver epoch does not consume a protocol
  classification, but it must be recorded.

Required post-fix evidence order:

```text
1. exact Rust/Java Destination hash match
2. Java target Destination enc type == ElGamal/type 0
3. target public-key slot length == 256
4. local LS2 key == X25519/type 4/32
5. P222 exact client lookup preflight still passes
6. SEND_TRACKED nonce/digest recorded
7. ordered Java status events recorded
8. frozen 45-second TunnelData/payload outcome recorded
9. bounded WP-C intersection diagnostics recorded if and only if status 17 persists
10. exactly one P223 final terminal
```

## 13. Post-fix decision table

### Branch G1 — reverse payload arrives

If the digest-matched Java→i2pr payload arrives inside 45 seconds:

```text
P223-REVERSE-DELIVERY-PASSED
```

Then Plan 223 may close passed and the next plan should be a fresh final Java
second-family qualification/closure pass. Do not fold Streaming requalification
into Plan 223 unless already required by an existing acceptance checker.

### Branch G2 — status 17 persists, source keys absent

If:

```text
target Destination type == 0
source_keys_present == false
status 17 persists
```

emit:

```text
P223-STATUS17-PERSISTS-SOURCE-KEYS-MISSING
```

Do not register/mutate keys inside Plan 223. A successor owns that lifecycle
corrective.

### Branch G3 — source keys present but no X25519

If:

```text
source_keys_present == true
source_supports_x25519 == false
target_has_x25519 == true
status 17 persists
```

emit:

```text
P223-STATUS17-PERSISTS-SOURCE-KEYS-NO-X25519
```

Stop for a successor.

### Branch G4 — target LS2 missing X25519 as Java parses it

If i2pr/Rust asserts type 4 but Java's stored target LS2 has no recognized
X25519 key:

```text
P223-STATUS17-PERSISTS-TARGET-LS2-NO-X25519
```

Stop. A successor owns codec/publication investigation.

### Branch G5 — both sets known but no intersection

Emit:

```text
P223-STATUS17-PERSISTS-NO-KEY-INTERSECTION
```

Persist both bounded type sets. Stop for a successor.

### Branch G6 — status 17 disappears but another terminal appears

Emit:

```text
P223-NEXT-BOUNDARY <status-name-or-code>
```

Plan 223 succeeds at its corrective objective if the exact early guard is
removed and the new boundary is honestly recorded, but it MUST NOT implement a
second unrelated protocol corrective.

### Branch G7 — contradiction

Examples:

- Java reports target type 0 but the same bytes decode as type 4 in Rust;
- target hash differs across parsers;
- Java reports selected compatible X25519 key yet OCMOSJ returns status 17
  from the same correlated message with no earlier status-17 source;
- post-fix LS2 key no longer matches i2pr's X25519 static key.

Emit:

```text
P223-EVIDENCE-CONTRADICTION
```

Stop.

## 14. Failure, cancellation, restart, and contention semantics

- Diagnostic commands are read-only and idempotent.
- No Plan-223 observation command may create a LeaseSet, register keys, or
  mutate NetDB/tunnel state.
- A helper/control connection failure yields Unknown and terminates that run.
- Fresh Java RouterContexts are required after any process-level restart.
- Never reuse P223 facts from a previous external attempt.
- Each attempt records implementation SHA and clean-tree state.
- The classification is derived from one internally consistent run only.
- An infrastructure pre-epoch failure must not be merged with facts from a
  later attempt.
- No retry is permitted after a code/config change without a new implementation
  commit SHA.

## 15. Compatibility and migration constraints

Expected compatibility effect:

- newly generated router-owned i2pr Destinations become structurally aligned
  with Java's long-standing legacy Destination identity convention;
- active destination encryption remains X25519 through LS2;
- existing imported Java/i2pd Destinations remain valid.

Potential compatibility risk:

Changing generated Destination bytes changes destination hashes/addresses for
newly generated identities and may change deterministic test vectors.

Plan 223 MUST inventory whether any currently persisted router-owned
Destination format reconstructs identities from only:

```text
signing secret + X25519 secret + padding
```

If yes, do not silently regenerate a different public identity on load.
Either preserve exact serialized Destination bytes or stop for an explicit
migration plan.

No compatibility claim may be made until this audit is recorded in
`223-status.md`.

## 16. Security review requirements

The closure record must explicitly verify:

- legacy Destination filler contains no secret-derived bytes;
- X25519 private key remains zeroized/non-Debug;
- signing secret ownership unchanged;
- LS2 key still derives from the X25519 private key;
- no private key material appears in diagnostic output;
- no Java helper writes key material to evidence;
- no acceptance path downgrades ECIES to ElGamal;
- no public network participation was added;
- no Java reference code was modified.

## 17. Static evidence checker updates

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh` or a narrowly
named companion checker to reject at least:

1. generated Destination using `ROUTER_CRYPTO_KEY_TYPE` directly;
2. generated Destination key certificate advertising X25519/type 4;
3. LS2 encryption changed away from X25519/type 4;
4. `DATAGRAM_WAIT` changed from 45 seconds;
5. Java/i2pd pins changed;
6. a Plan-223 classifier that treats status 17 as sufficient without the
   Destination-type/source-key/target-key discriminator;
7. source-key absence defaulting to `false` rather than Unknown/explicit
   absent;
8. Java diagnostics that mutate `KeyManager`, client DB, or LeaseSet state.

The checker must verify behavior-shaping source, not only token presence.

## 18. Verification commands

Routine floor:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
```

Boundary/evidence floor:

```bash
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-sam-acceptance-evidence.sh
```

Focused tests:

```bash
cargo test --locked -p i2pr-client -- --test-threads=1
cargo test --locked -p i2pr-api -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p223 -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p222 -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
```

Exact-head external lane:

```bash
test -z "$(git status --porcelain=v1)"
git rev-parse HEAD

I2PR_M6_JAVA_DRIVER=destination \
  bash tests/integration/m6-interop/run-java.sh
```

## 19. Acceptance criteria

Plan 223 closes as passed only when all applicable criteria below are recorded
in `plans/closure/mixed-router-interop/223-status.md`.

1. Pre-fix exact target Destination bytes are parsed by Rust and Java.
2. Rust/Java target Destination hashes match.
3. Pre-fix Java target encryption type is explicitly recorded.
4. Same tracked send reproduces status 17.
5. Early OCMOSJ guard is confirmed before production identity changes, or the
   plan stops without those changes.
6. Generated post-fix Destination uses type 0 / 256-byte legacy public field.
7. Generated post-fix Destination signing type remains Ed25519/type 7.
8. Legacy public filler is independent of the X25519 private key.
9. RouterInfo/router identity remains X25519/type 4.
10. Standard LS2 remains type 3.
11. LS2 encryption remains exactly X25519/type 4/32 bytes.
12. LS2 X25519 public bytes match the local X25519 static secret.
13. Imported Java/i2pd Destination path remains accepted.
14. SAM/private-destination audit is complete.
15. No silent persistent identity migration is introduced.
16. Plan-222 exact selector preflight still passes.
17. Nonce-correlated reverse send still records ordered statuses.
18. Frozen 45-second payload window is unchanged.
19. Status 17 disappears, **or** the bounded C1–C3 discriminator records why it
    persists.
20. Exactly one P223 final terminal is emitted on the authoritative run.
21. No bootstrap/floodfill/SAM/tunnel/topology change is present.
22. No Java source patch/reflection/state mutation is present.
23. Routine floor is green.
24. Focused Plan-222 regressions are green.
25. Implementation commit precedes external evidence and the run starts from a
    clean tree.
26. No more than three exact-head attempts occur.
27. Registry/roadmap/dependent status files are updated truthfully after the
    result.
28. `milestone6_interoperable` remains unclaimed unless a later final
    qualification explicitly closes it.

## 20. Stop conditions

STOP immediately and write the closure status as blocked/stopped if:

- exact pre-fix bytes do not prove the early Destination guard;
- correcting generated Destination shape requires changing Java;
- correcting it requires ElGamal end-to-end encryption;
- correcting it requires changing RouterInfo/router identity crypto;
- a persistent identity migration is required but not already explicitly
  versioned/safe;
- the post-fix status remains 17 and resolving it requires mutating Java
  KeyManager/client-NetDB state;
- evidence requires public I2P;
- any proposed fix changes topology/tunnel/bootstrap behavior.

A stopped Plan 223 should register a narrowly named Plan 224 based on the
first observed post-fix boundary. Do not accumulate another broad diagnostic
campaign.

## 21. Closure evidence template

`223-status.md` must include:

```text
status token
implementation SHA
exact Java/i2pd pins
pre-fix Rust Destination hash/type/length
pre-fix Java Destination hash/type/length
pre-fix P223 classifier
production files changed
post-fix Rust Destination hash/type/length
post-fix Java Destination hash/type/length
post-fix LS2 type/key type/key length/key-match
Plan-222 selector preflight result
tracked nonce + payload digest
ordered Java statuses
45-second TunnelData result
45-second payload result
C1/C2/C3 intersection facts if status 17 persists
P223 final terminal
routine/focused verification results
attempt count + clean-head proof
security review
compatibility/migration review
dependency/unblock audit
next executable plan
```

## 22. Handoff for smaller-model execution

Execute strictly in this order:

```text
A. Freeze P222 behavior and add forbidden-shortcut guards.
B. Add exact Rust/Java target-Destination type observation.
C. Reproduce one pre-fix status-17 run.
D. If and only if the early guard is confirmed:
      separate generated Destination legacy field from LS2 X25519.
E. Audit deterministic constructors + SAM/private-destination compatibility.
F. Add unit/cross-parser tests.
G. Commit implementation.
H. Verify clean tree.
I. Run one exact-head destination-only Java lane.
J. If reverse payload passes, record P223-REVERSE-DELIVERY-PASSED.
K. If status 17 persists, record C1/C2/C3 source/target/intersection facts
   and STOP with the exact P223 status-17 terminal.
L. If a different Java status appears, record P223-NEXT-BOUNDARY and STOP.
M. Run routine/focused floors.
N. Write 223-status.md and perform the dependency/unblock audit.
```

Do not skip B/C and jump directly to the identity change. Do not continue into
a second protocol fix after J/K/L.

## 23. Registration disposition

At registration:

```text
plan_223 = registered-ready-m6-java-destination-identity-crypto-separation-corrective
plan_201 = blocked-pending-plan223-destination-identity-crypto-separation-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan223
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
next_executable_plan = 223-m6-java-destination-identity-crypto-separation-corrective
```

Plan 223 is the only dependency-ready M6 implementation handoff.
