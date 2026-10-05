# Protocol conformance and evidence policy

## Claim model

`i2pr` must not claim protocol support from code presence alone. A protocol or feature may be marked implemented only when the applicable evidence below exists:

1. strict decode and canonical encode tests;
2. authoritative golden vectors or independently generated cross-implementation vectors;
3. malformed, truncated, oversized and semantically invalid input tests;
4. state-machine success, failure, timeout, cancellation and teardown tests;
5. explicit memory, queue, task, retry and cryptographic-work bounds;
6. replay, duplicate, expiry and clock-skew tests where relevant;
7. mixed-router interoperability against at least two independent implementations for router-to-router protocols;
8. documentation of unsupported and compatibility-only behavior;
9. no advertised RouterInfo, I2NP, API or transport capability beyond the tested subset.

Java I2P and I2P+ share lineage and count as one implementation family for independence. The preferred router-to-router interoperability pair is Java I2P or I2P+ plus i2pd. Emissary/go-i2p should be added where its current implementation is complete enough for the tested surface.


### Evidence tiers for progression versus full conformance

ADR 0026 distinguishes an experimental development gate from the full claim above.

For **experimental development progression**, a non-advertised subsystem may continue
after local conformance requirements and at least one exact-pinned independent
implementation demonstrate the relevant controlled external path. This authorizes later
implementation work only; it does not authorize public exposure, production-readiness
language, broad advertisement, or a full interoperability claim.

For **full router-to-router conformance or broad capability advertisement**, item 7 above
remains mandatory: at least two independent implementation families must interoperate for
the claimed router-to-router surface.

For **client/application protocols** such as Streaming, SAM, I2CP, and service-tunnel
profiles, require independent evidence appropriate to the surface. A second full router
family is not automatically a hard gate merely because the path traverses routers.

Exact-pinned i2pd Plan 193 satisfies M6 experimental mixed-router progression. The Java
full-router lane remains compatibility debt at Plan 247. Full two-family M6 router
conformance remains not claimed.

M11 transit remains retained/unclaimed and `advertised=false`. Plans 255-258 retain their
documented infrastructure/corrections, including Plan 258's externally proven canonical
multicell IBGW emission. Plan 259's transit-endpoint topology inventory is retained but its
receipt-is-OBEP-only conclusion is not current authority: exact-pinned i2pd distinguishes
`TransitTunnelEndpoint(false)` from a creator-owned `InboundTunnel` that sets the incoming
message owner to a destination-pool tunnel before LOCAL garlic dispatch. Plan 260 is
retained-blocked: its seven creator-owned source locks, explicit-peer lock,
fragment-id hardening, tuple validator, and receipt harness all landed with local
rows green, and the dedicated receiver 1-hop `[i2pr]` inbound was exhibited live
(`[A,A]` IBGW accepts plus SAM STATUS OK on a fresh mesh) — but delivery stops
on two exact boundaries: forward-path garlic death at B's endpoint for A-side
senders (zero ingress on counted ids over four multicell-forcing rounds) and
late-run mesh sustainability (establishment 1/3, outbound collapse without
floodfill). Plan 261 is retained-blocked: its B-sender lane work
all landed with local rows green and zero production diff (four
B-side source locks, B SAM plumbing with fail-closed env gate,
`m11-tx-b` sender shape, terminal-signature instrumentation), and
live execution proved the B-sender topology through addressing (B
outbound `[i2pr]` established, B-side LeaseSet resolved from the
floodfill store, 4/4 sends naming counted `[A,A]` ids) — but
delivery stops on one exact boundary: the B3 self-delivery
loopback gap (self-targeted OBEP TUNNEL actions terminate
`NoActiveSession` at the session peer seam, zero ingress, zero
socket receipt). Plan 262 is retained-blocked: its dedicated IBGW
state, exact receive-id ownership, source-neutral seam, and
self-delivery loopback arm all landed with local rows green and
live execution flipped B3 with socket receipt on a healthy mesh
(diag4 `terminal-garlic-self:0/ingress:6/socket:1` with
tuple-bound multicell on `514bf12`), but two same-SHA full-matrix
attempts stop on mesh-sustainability signatures (IBGW-data relay,
SAM timeout). Plan 263 is retained-blocked: its harness-only
sustainability proofs landed with zero production diff and
re-proved receipt (`0/18/1` tuple-bound) + IBGW multicell (max 2)
on `9bd2f39a`, but two same-SHA single-mesh attempts stop on
sustainability signatures (single-cell-only window, receipt
starvation). Plan 264 is retained-blocked: its per-epoch lane +
composition gate landed with zero production diff and proved
the gate (5 epochs 2/2 on `6ab9dc2d`), but emission epochs stop
on window signatures (ibgw-data 1/2, receipt 0/2,
participant-data 1/2, replay 0/2). Plan 265 executed that fixed-budget contract
(24 retained attempts on qualification SHA `4682920e`, zero
production diff, zero i2pr semantic failures) and is
retained-blocked: `ibgw-data` 4/8 and `participant-lifecycle`
5/8 closed, `receipt` 1/8 against a required 2. Plan 266 executed
its ladder contract (8 retained `receipt` attempts on
qualification SHA `a9803ca`, zero production diff, zero i2pr
semantic failures) and is retained-blocked: `receipt` 1/8 against
a required 2 with rung distribution r1x2/r4x2/r6x3/r7x1, every
rung-6 attempt an anchored accepted-id drop population. Plan 267
executed its disposition contract (8 retained `receipt` attempts
on qualification SHA `315fb0d`, zero production diff, zero i2pr
semantic failures) and is retained-blocked: `receipt` 0/8 against
a required 2 with 427 drop rows dominated by accepted-yet-not-
found with delays in minutes. Plan 268 executed its path contract
(8 retained `receipt` attempts on qualification SHA `cc9b40c`,
zero production diff, zero i2pr semantic failures) and closed the
family 3/8 with the install-path divergence falsified (353 paths
all dispatched, zero bypass) with exact-head ordinary CI green, so
ADR 0026's one-family M11 experimental qualification is passed.
Every
attempt remains in the denominator; opportunity is classified
before semantic output; any opportunity-present i2pr
contradiction hard-fails; successful closure requires at least
two qualifying successes per family. This is an M11-specific
evidence composition, not a general relaxation of the
conformance policy. Public transit, RouterInfo capability,
router.version, public-network participation, and broad two-family conformance remain
unauthorized.

### M12 floodfill evidence vocabulary

ADR 0027 defines five M12 evidence tiers: `architecture-frozen`, `local-validated`,
`one-family-experimental`, `two-family-qualified`, and `normal-opt-in-activated`.
They are separate authority transitions: architecture or local tests do not claim
interoperability; one-family evidence permits controlled experimental progression only;
two independent implementation families are required before broad floodfill capability
advertisement; normal activation additionally requires explicit operator opt-in and live
readiness/health. Current state is `architecture-frozen` in progress under Plan 270;
there is no M12 implementation, floodfill serving, or `caps=f` claim. Plan 272 proceeds on Plan 271 plus the Plan 281 type-5-deferred support floor (RouterInfo plus DatabaseStore types 1, 3, and 7); Plan 280 stopped with no acceptable maintained Rust provider for Red25519 (signature type 11), so type-5 EncryptedLeaseSet records remain deferred until a separately reviewed provider plan passes. No tier implies
public-network operation, production readiness, anonymity, or privacy guarantees.

### Proposal 170 / I2PControl support model (Plan 286)

Every Proposal 170 capability is classified independently across seven
dimensions. Parser acceptance or persisted inert options are never runtime
support; full support is claimed only when every applicable cell has a real
source/effect or a protocol-permitted explicit neutral disposition.

1. **wire** — exact JSON-RPC 2.0 envelope, method/selector/action/type/
   option spelling, error-code, and ceiling behavior per the frozen Plan 286
   contract (`crates/i2pr-i2pcontrol`, provenance at
   `docs/provenance/proposal-170-manifest.md`).
2. **source** — the authoritative router owner behind every readable
   selector (identity/publication, transport, NetDB, tunnels, services,
   canonical AddressBook). Missing sources fail explicitly; zero/false/
   empty fabrications are forbidden.
3. **runtime effect** — a real owner consumes every applicable mutation or
   option cell (apply-or-reject; no inert accepted options).
4. **persistence/atomicity** — mutating control state publishes versioned
   recoverable generations with prior-generation fallback; success means
   durable intent and runtime generation agree.
5. **feature isolation** — disabled/default mode allocates no listener,
   managed certificate, token table, background task, or Proposal state,
   and cannot influence ordinary resolution or routing.
6. **security/secret handling** — loopback-by-default TLS with no plaintext
   fallback, API-1 token lifecycle, source-IP throttling, bounded budgets
   on every body/batch/connection/request, and redaction of passwords,
   tokens, destination secrets, and proxy/client-auth material.
7. **evidence** — literal fixtures, malformed/+1 cases, deterministic
   state-machine tests, restart/recovery proofs, and (at Plan 295)
   differential qualification against the pinned fork/Java/i2pd references.

`specs/support.toml` gains Proposal 170 rows only when the first
capability closes with evidence; registration alone claims nothing.

## Source-to-code traceability

Every protocol module should identify:

- the dossier in this directory;
- the official specification path and pinned commit used during implementation;
- relevant proposal numbers;
- the external-standard revision, if any;
- the test-vector origin;
- deliberate deviations or stricter validation;
- any compatibility behavior inferred from implementation evidence.

This may be recorded in module documentation, a nearby `README`, test metadata, or an implementation plan. Avoid scattering unexplained protocol constants through runtime code.

## Decoder policy

Network, disk, reseed and local-API inputs are untrusted. Decoders must:

- enforce a caller-visible maximum before allocation;
- use checked arithmetic for offsets, lengths, counts and time computations;
- distinguish truncation, malformed encoding, unsupported type, semantic invalidity and policy rejection;
- consume exactly the expected input for strict top-level decoding;
- reject duplicate fields or keys where the format requires uniqueness;
- validate canonical ordering where signatures or hashes depend on canonical bytes;
- preserve the signed byte representation when reserialization could change verification semantics;
- avoid recursive structures without explicit depth limits;
- never panic on arbitrary bytes;
- avoid retaining attacker-controlled backing buffers after parsing unless bounded and intentional.

Unknown blocks or options may be ignored only when the specification explicitly defines forward-compatible skipping. The parser must still validate the enclosing length and resource bounds.

## Encoder policy

Encoders must:

- produce deterministic canonical output where the protocol defines canonicalization;
- reject values that cannot be represented without truncation;
- calculate exact encoded length before or during bounded emission;
- avoid implicit platform-width integer conversions;
- emit only capability/version combinations supported by the current runtime;
- keep private key material, session keys and plaintext authentication data out of logs and `Debug` output.

Round-trip tests are necessary but insufficient because two matching bugs may round-trip. Include fixed expected bytes and cross-implementation decoding.

## State-machine policy

Transport, NetDB, tunnel, garlic, streaming and API protocols must use explicit states and legal transitions. Each state machine must define:

- accepted messages/events per state;
- deadlines and retry budgets;
- duplicate and reordered input behavior;
- cancellation points;
- owned resources and cleanup on every terminal path;
- peer-visible errors or silent-drop behavior;
- whether malformed input terminates a message, session, transport link, tunnel build, destination or client connection.

No retry loop may be unbounded. Backoff, peer rotation and global concurrency limits must be tested under deterministic time.

## Cryptographic conformance

Do not implement cryptographic primitives locally. Wrap reviewed libraries with protocol-specific key and nonce types.

Tests must cover:

- official or independently verified positive vectors;
- invalid keys, signatures, tags and authentication data;
- nonce/counter boundary behavior;
- all-zero or low-order X25519 results according to the relevant specification/library contract;
- key-type and encoded-length mismatch;
- domain-separation and network-ID inputs;
- transcript/hash changes from one-bit mutations;
- key erasure or bounded lifetime where library support permits;
- failure without unauthenticated plaintext exposure.

Legacy algorithms required only for reading deployed data must be isolated from new identity generation and ordinary emission policy.

### Red25519 (signature type 11) status

`i2pr-crypto` implements the I2P Red25519 composition — domain-separated `HStar`, daily alpha
derivation, additive re-randomization, randomized signing, and cofactor-aware verification — over
reviewed `curve25519-dalek` arithmetic. Its status is:

- **Verified against the specification's own vectors.** All ten official Red25519 vectors pass;
  the deterministic fields compare byte-for-byte and both signature rows of every vector verify.
- **Byte-compatible with one independent implementation.** `eggstack/emissary@6885a945` reproduces
  the same alpha values, blinded keys, DHT storage keys, and signature bytes.
- **Signature-type 11 is used for one thing only: the encrypted LeaseSet2 type-5 outer
  signature.** It is not advertised, and no other database record uses it. A second, explicitly
  bounded profile now exists for that one use; see below.
- **Known interop limitation, corrected by Plan 346.** `i2pd` and Java I2P sign type 11 with a bare
  SHA-512 transcript that omits the `I2P_Red25519H(x)` domain and the specification's length
  framing, and cannot verify the official vector corpus. Under a strict-only policy an i2pr type-5
  record was therefore unverifiable by both. Plan 346 keeps the strict primitive byte-exact and adds
  the deployed ELS2 transcript as a **separate, bounded profile owned by the type-5 verifier** (ADR
  0032). A type-5 record i2pr publishes is now signed with the transcript those two routers verify,
  and a record either of them publishes is readable by i2pr. Blinding, alpha derivation, and the DHT
  storage key interoperated with both before this change and are unchanged.

Authority: Plans 329–331, the Plan 336 spec-first conformance decision
(`plans/closure/i2pcontrol-proposal-170/336-closure.md`), and Plan 346 / ADR 0032
(`plans/closure/i2pcontrol-proposal-170/346-status.md`). Plan 335's measured-negative boundary is
preserved as history and is superseded for forward execution by Plans 346–347; Plan 346 passes does
not itself claim cross-router interoperability, which is Plan 347's evidence.

#### The two type-11 transcripts (ADR 0032)

- **Strict (Proposal 146) is unchanged and still the only meaning of the generic primitive.**
  `i2pr_crypto::red25519::{sign, verify, sign_with_nonce, verify_blinded}` keep the
  `I2P_Red25519H(x)` domain prefix and the two-byte little-endian message-length framing, and all
  ten official Red25519 vectors still pass byte-for-byte.
- **Deployed (Encrypted LeaseSet2) is a separate composition, not a redefinition.**
  `i2pr_crypto::red25519_deployed` implements `r = SHA-512(T || A || m)`, `c = SHA-512(R || A || m)`,
  `S = r + c·a` over the same reviewed `curve25519-dalek` arithmetic, sharing its point, scalar, and
  equation code rather than duplicating it. Cross-verified against **executed** output from
  `i2p/i2p.i2p@93eef5db…` and `PurpleI2P/i2pd@2c694149…` in both directions.
- **There is no wire discriminator between the two definitions.** A type-5 record names signature
  type 11 and carries 64 bytes; nothing says which transcript produced them.
- **No generic dual-transcript type-11 verifier exists.** The common signature layer
  (`i2pr_crypto::verify_signature`) has no type-11 path at all, so the deployed transcript is
  reachable only from the ELS2 type-5 owner. Enforced statically by
  `scripts/check-els2-type11-transcript-boundary.sh`.
- **Outbound records use the deployed profile; inbound records accept both, only inside the bounded
  verifier.** Acceptance is a typed four-state result (`deployed` / `strict` / `none` /
  `ambiguous`) reported to the type-5 owner, and an `ambiguous` match is **rejected**, not resolved.
  Strict acceptance is retained only so records published under the previous policy, and the
  independent Emissary oracle's output, stay parseable. i2pr does not publish strict records.
- **The signed region is a type.** `i2pr_proto::Els2SignedRegion` can only be built from a decoded
  `EncryptedLeaseSet2` or `EncryptedLeaseSet2OfflineKeys` and has no byte-slice constructor, so no
  application message and no transcript selector can reach the ELS2 signer or verifier.
- **This is a compatibility tradeoff, not equivalent security semantics.** The deployed transcript
  omits the domain separator and length framing. The compensating constraints are the typed signed
  region, the record-length ceiling enforced before hashing, transcript selection that never comes
  from the network or I2PControl, no automatic downgrade or retry outside the ELS2 verifier, and
  randomized signing that still requires a CSPRNG.
- **A deliberate, recorded overlap.** Because the deployed challenge hash `SHA-512(R || A || M)` is
  the plain Ed25519 challenge, a deployed signature is also a valid Ed25519 signature and vice versa.
  What differs between the two type-11 transcripts is the **signing** transcript, not the
  verification equation. The deployed verifier is therefore not a stricter check than type 7 and no
  such claim is made; the protection that holds is that the ELS2 owner dispatches on the record's
  own `sigtype` and refuses a type-5 record declaring a non-11 blinded sigtype before any transcript
  is consulted.

### Encrypted LeaseSet2 (DatabaseStore type 5) status

Plan 332 implements the type-5 record: layer-0 framing, the `credential` and `subcredential`
derivations, both ChaCha20 layer key derivations, the no-client-authorization form, signature and
freshness validation, the bounded store, the per-UTC-day blinding schedule, and the encrypted-service
(`b33`) address codec. Its status is:

- **Structurally first-class.** `DatabaseStoreData::EncryptedLeaseSet` replaces the earlier
  `Deferred` pass-through: a type-5 body is now framed, flag-checked, and signature-preimage
  complete at the wire layer, and a malformed body is a typed error rather than an accepted blob.
- **Layer cryptography agrees with an independent derivation.** A pure-Python re-derivation written
  from the specification text (`tools/generate-els2-independent-fixture.py`, standard library
  SHA-256/HMAC plus an RFC 8439 ChaCha20) reproduces the credential, the subcredential, and the
  complete outer ciphertext byte-for-byte across five cases spanning both unblinded signature types,
  both inner store types, and the block and length boundaries.
- **Adversarial coverage is fail-closed.** Wrong day, wrong lookup secret, wrong `published`
  timestamp, every single-byte ciphertext tamper, flipped signatures, rewritten header fields,
  reserved flag bits in both layer 0 and layer 1, truncated and oversized inputs, unsupported
  signature types, per-client layer-1 flags (refused, not guessed at), and an unknown storage key
  are all rejected.
- **Not accepted on the wire, not advertised, not live-verified.** No `specs/support.toml` entry
  exists, no daemon configuration exposes it, and no publication driver sends or fetches a type-5
  record. `i2pr-client` *builds* the `DatabaseStoreMessage`; the daemon still has nothing that
  publishes it. Per-client authorization is implemented (see below) but remains unreachable from
  any daemon configuration.
- **Known interop limitation, corrected at the policy level by Plan 346; end-to-end proof is still
  Plan 347.** Under the strict-only decision an i2pr-signed type-5 record was unverifiable by
  `i2pd` and Java I2P. Plan 346 makes the ELS2 use of type 11 an explicit bounded compatibility
  profile (ADR 0032): i2pr now publishes the deployed transcript and accepts both. That is a
  **cryptographic-boundary** result, measured against executed Java I2P and i2pd output. It is not
  yet a **live** result: no stock router has published, stored, looked up, decrypted, and used a
  type-5 record end to end in either direction. Plan 347 owns that evidence, and until it passes
  no cross-router ELS2 interoperability is claimed and `advertised` stays `false`.
- **The lookup secret is a discovery control, not a content control.** It changes the daily blinded
  key and therefore the DHT storage key, so a party with the address but not the secret cannot find
  the record. It does not enter the credential or subcredential, so a party that already holds both
  the record bytes and the address can decrypt them. See
  `specs/references/red25519-algorithm-worksheet.md` §14.14.
- **Consumer-side wiring, added by Plan 351 / ADR 0033; resolve-on-demand only, no live claim.**
  Plan 349 built the bounded `EncryptedServiceResolver` with **zero** production callers. Plan 351
  supplies one, and three facts about it are recorded here because each changes what a reader may
  assume:
  - A `.b33` is its **own** destination-reference kind. Its value carries the unblinded signing
    public key, which is *not* a `Destination` hash — the protocol layer says so explicitly, and
    the address lacks the ECIES public key, certificate, and padding a `Destination` encoding needs.
  - The lookup key is the day's **blinded storage key**, supplied verbatim. The wire lookup type is
    unchanged (`LookupKind::LeaseSet2`, code `1`), because a reference client issues that same type
    for an encrypted service; no new wire type is introduced.
  - A fetched record is installed under the **inner record's own destination hash**, gated on that
    record signing with the unblinded public key the `.b33` names. The gate is what makes this
    trustworthy: the hash comes from the record, so it is trusted through a signature against a key
    obtained out of band, not because the record said so.
  - The type-7 relationship is **not** general. Plan 351's publisher emits type 7, where
    `DERIVE_PUBLIC(CONVERT_ED25519_PRIVATE(seed))` reproduces the destination's Ed25519 public key,
    so the address and the inner record agree by construction. A type-11 `.b33` has no such
    relationship — its unblinded key signs only the outer record.
  - Containment (ADR 0033 Gate 1–3): an encrypted remote target is refused on any service that is
    not a `DelayOpen` client, a static alias may not name a `.b33`, the consumer secret arrives
    through the I2PControl definition options (no TOML field, no inline config secret), and a
    failure is recorded on a closed per-service status surface and never propagated into the
    provisioning pass — two of its three production callers tear the product down on any error.
  - **Not claimed:** PSK/DH consumer authorization; daily rollover re-resolution; a cross-router
    result; any Java or i2pd direction of Plan 347. The blinding rotates daily and there is no
    periodic re-resolution, so a resolution computed before a midnight boundary addresses the
    **wrong DHT key**. That is a recorded limitation, not rollover support.

Authority: Plan 332 (`plans/closure/i2pcontrol-proposal-170/332-status.md`), Plan 333, Plan 344,
Plan 346 / ADR 0032 (`plans/closure/i2pcontrol-proposal-170/346-status.md`), and Plan 351 / ADR 0033
(`plans/closure/i2pcontrol-proposal-170/351-status.md`).

### Encrypted LeaseSet2 per-client authorization status

Plan 333 adds both standard authorization modes on top of the Plan 332 foundation. Authorization is
strictly additive: the no-authorization path still delegates to the same general
encrypt/decrypt functions, and the Plan 332 differential fixture remains valid unchanged. Its
status is:

- **Both schemes implemented and both reachable end to end.** `i2pr_netdb::els2_auth` implements
  the PSK and X25519 derivations, the bounded authorization block, constant-time identifier
  matching, and the typed failure vocabulary. `i2pr-client` publishes authorized records and
  resolves them with a credential. Every authorized client of one record reads the same inner
  LeaseSet2, under both schemes.
- **Authorization is not an integrity layer, and is not claimed to be.** Each layer is a raw
  ChaCha20 stream with no tag; the Red25519 signature over the layer-0 region is what detects a
  tamper, and it is verified before any decryption. A flipped ciphertext byte is therefore rejected
  as `InvalidSignature`, not as an authorization failure. See worksheet §14.15 and §14.16–§14.22.
- **Bounded and fail-closed.** The client set is non-empty and capped at 255; capacity 255 is
  exercised and 256 is refused. Duplicate identifiers are rejected at block build rather than
  merged. Entry order carries no meaning: recovery scans every entry and compares in constant time.
  A publication that cannot draw a fresh cookie, salt, or ephemeral key fails rather than
  substituting a predictable value.
- **Refusals are typed and deliberately uninformative.** A missing credential and a wrong credential
  are *different* errors, because only the first is fixable by presenting some authorized key. A
  wrong key and a key that was never configured produce the same error text, so nothing about a
  guess leaks. See worksheet §14.21.
- **Secrets are typed, role-tagged, and non-copyable.** `PskClientKey`, `AuthCookie`, and
  `Els2ClientAuthSecret` are zeroizing, are not `Clone`, and have redacted `Debug`. The four
  authorization roles are distinguished, and persistence is a reversible role-tagged encoding that
  is explicitly not a wire format, a configuration serialization, or a password verifier. At-rest
  encryption remains the storage layer's responsibility and is **not** claimed.
- **An authorized service's address declares the requirement.** `B32_FLAG_REQUIRES_CLIENT_KEY` is
  set at publication and preserved through resolution, so a prospective client learns it needs a
  credential before it fetches anything. A resolver built for such an address refuses the
  unauthenticated path. See worksheet §14.20.
- **Still not accepted on the wire, not advertised, not live-verified.** Nothing changed about
  daemon wiring: no `specs/support.toml` entry, no configuration surface, no publication driver.
  Proposal 170 field mapping is Plan 334.
- **Cross-implementation authorization is unproven.** The plan's evidence list asks for
  publication/decryption against Java I2P and `i2pd` "where supported". No Java I2P build was
  provisioned and the type-11 signature divergence blocks a meaningful live comparison regardless,
  so this row is **not** claimed. The post-freeze Emissary differential covers the Plan 332
  no-authorization construction only; authorization was compared structurally, not byte-for-byte.

Authority: Plan 333 (`plans/closure/i2pcontrol-proposal-170/333-status.md`).

### Proposal 170 LeaseSet mode mapping status (Plan 334)

Plan 334 gives Proposal 170's LeaseSet block a real control-plane surface. Its status is:

- **The normative reading is frozen, and the finding is negative.** Proposal 170 revision
  2026-05-20 lists the ten `EncryptLeaseSet` strings plus `OptionalLookup` and
  `LeaseSetClientAuths` and **defines nothing about them**: no per-mode property table, no wire
  types beyond the option block, no schemas, and no precedence rule between the three parameters.
  The reading i2pr implements — which of the referenced ELS2 specifications owns each parameter,
  the address flags each mode implies, and the two argued dispositions — is frozen in
  `specs/references/proposal-170-encryptleaseset-mode-mapping.md`. The source is pinned by
  bytes, not by URL: SHA-256 `f13ae00b886c5e72131bc5d5b138a371148d1faa6899a119a1dacb65a555e7dc`
  over 19,010 bytes, in `docs/provenance/proposal-170-manifest.md`.
- **All ten values resolve; nine are applied and one is refused.** `encrypted (aes)` is
  *recognized* and then refused by name, with the deprecation cited, so an operator learns they
  supplied a valid Proposal value that i2pr deliberately does not implement rather than being
  told they made a spelling mistake.
- **The ten values reduce to eight protocol behaviors.** `encrypted (psk)` and `encrypted with
  per-user key (psk)` are one behavior under two spellings, because the I2P specifications define
  one pre-shared-key scheme; the per-user distinction is the number of block entries, not the
  string. Which spelling arrived is recorded and reported, not hidden.
- **Mode names are identifiers.** No case folding, trimming, or fuzzy matching: `Disable`,
  `encrypted(psk)`, and `encrypted  (psk)` are all unknown values.
- **No silent no-ops and no silent downgrades.** A secret supplied to a mode that does not consume
  it is an error. A secret a mode requires and did not get is an error, and so is one that is
  present but empty, because an empty value would otherwise publish a service that is less
  protected than the operator asked for.
- **The block is validated as a unit, before any mutation, and re-validated on load.** A merged
  `create`+stored-`edit` candidate and a reloaded stored definition obey the same law as a fresh
  request. A generation written under an older rule fails the load closed instead of starting a
  service in a posture the operator never asked for.
- **No secret reaches any response.** `get` and `rawConfig` report whether each secret is
  configured, the lookup secret's length, and the client count, never a byte. The `EncryptLeaseSet`
  mode itself is public and reads back verbatim.
- **The two i2pr-inventory secret spellings are reconciled to the two Proposal fields.**
  `OptionalLookup` is the single ELS2 lookup secret. `leaseset_blinding_secret` is **refused as a
  duplicate**: the ELS2 specification defines exactly one lookup secret and Proposal 170 spells it
  once, so a second i2pr slot would let an operator believe two secrets are in effect when only one
  can be. This retires an i2pr-invented name rather than aliasing it.
- **`encrypt_lease_set` changes from Boolean to String.** Plan 293 carried the inventory type as
  divergence item 1; Proposal 170's value is an enumeration string, so the inventory type was
  wrong and is corrected.
- **A real type-5 record is built from the service's own identity, with no new stored secret.**
  The blinding identity is the Red25519 conversion of the service's existing Ed25519 signing seed,
  which is what the published address must name for a client to verify the inner LeaseSet2. Six
  rows assert a real type-5 `DatabaseStore` at the day's blinded storage key, carrying the day's
  blinded public key, a non-zero outer salt, and a non-empty outer ciphertext.
- **A control-created server now reaches the publication path (Plan 337, ADR 0031).** The composition
  root builds the **one** `ServiceTunnelManager` in `build_shared_service_manager` and injects the
  same `Arc` into the control state and the destination-group product, so a control-created runtime is
  the same runtime the product delivers and publishes through. The I2PControl service declares
  `depends_on("ssu2-router")` because the graph's order is lexical; the product prepares the manager
  and installs the executable router delivery backend before any control reconcile. A control-created
  server is therefore visible to the publication sweep, which reads the manager's committed
  generation rather than its original configured spec set.
- **A type-5 record is published at the record's own blinded storage key.** `service_publication_store`
  selects the record's own storage key — the day's blinded key for an encrypted service, the
  destination hash otherwise — and the floodfill is chosen for whichever it is. Absent material leaves
  the ordinary publication byte-identical to before.
- **Every server group is persistent, and the ELS2 material is resolved by the manager that wrote the
  record (Plan 338).** `ServiceTunnelSet::destination_groups` sets `persistent` for
  `kind.is_server()`, so a control-created server has a persisted `ServiceDestinationRecord` and a
  stable identity across restarts. Three store paths exist for one concept — `for_group`,
  `for_key_reference`, and the legacy `for_service` — and `ServiceTunnelManager` is the single owner
  of which one a resolved ownership policy uses. A record the publication cannot find would publish
  an address naming an identity the service is **not** using, which is worse than publishing nothing.
- **The posture and the `.b32.i2p` address reach the JSON-RPC wire.** `get` reports
  `lease_set_security` and `encryptedAddress` inside `info`; the address decodes as a real
  `EncryptedServiceAddress` whose flag bits follow the mode. Without this a JSON-RPC client could
  never discover the address it needs to look the service up, and the whole mode mapping would be
  unobservable to a real client.
- **A failed control transaction leaves no runtime behind (Plan 338).** `rollback_state` reconciles the
  shared manager as well as the in-memory mirror, and all five failure paths await it, so no
  transition can leave a service the operator cannot see, stop, or delete.
- **Black-box evidence exists.** Five rows drive a real TLS listener and real JSON-RPC `TunnelManager`
  calls: the create/get/rawConfig round trip, an `edit` that changes the posture and the address, a
  rejected `edit` leaving options, posture, and address byte-identical, a restart restoring a
  secret-bearing definition with the address unchanged, and the refused modes staying refused
  (including `encrypted (aes)` refused *by name*, and non-identifiers not case-folded or trimmed). No
  private bridge, LeaseSet2, driver, pump, or manager API is touched.
- **The client-count ceiling is 24 on this control surface, and that is not a protocol limit.** The
  ELS2 authorization block format permits 65,535 entries and the protocol owner accepts 255; the
  narrower number here is a consequence of carrying the list through one durable option value
  bounded by `MAX_OPTION_VALUE_LEN`, and a test asserts the worst case fits.
- **At-rest protection for the lookup secret and the client list is not claimed.** Both live in the
  durable control definition. That is the same posture as the existing per-service Ed25519 seed
  files: at-rest encryption remains `i2pr-storage`'s responsibility.
- **No live interoperability is claimed.** The cryptographic-boundary differential against Java
  I2P and i2pd is now **executed** and passes in both directions (Plan 346, ADR 0032): a type-5
  record these control-created services publish carries the transcript both routers verify, and a
  record either of them publishes is readable by i2pr. What is still unexecuted is the **live**
  path — stock routers publishing, storing, looking up, decrypting, and using a type-5 record end
  to end — which is Plan 347. A control-created encrypted service is therefore *implemented* but
  still **non-advertised** and not live-qualified, and the same is true of ordinary type 7/3
  publication, whose external lane is unexecuted for unrelated reasons.

Authority: Plan 334 (`plans/closure/i2pcontrol-proposal-170/334-status.md`), with Plan 337
(`337-status.md`, ADR 0031) and Plan 338 (`338-status.md`) for the publication path and transaction
correctness underneath it, and Plan 346 / ADR 0032 (`346-status.md`) for the corrected type-11
transcript profile.

### Per-family network condition codes (Plan 339)

Proposal 170's five per-family selectors (`i2p.router.net.status.v6`,
`.error`, `.error.v6`, `.testing`, `.testing.v6`) are each marked *"(adopted
from i2pd)"*, so the integer vocabulary is i2pd's and is pinned at i2pd
`2c69414` `RouterContext.h:44-72` — `RouterStatus` 0-5, `RouterError` 0-5, and a
0/1 testing flag — rather than chosen by i2pr. Plan 339 implements both
enumerations with bounded decode (an out-of-range value is an error, never
clamped) and derives every value from state i2pr already maintains.

What is claimed, and what is not:

- **The honest baseline is `status=2 (Unknown)`, `error=0 (None)`, `testing=0`.**
  That is a router which has bound IPv4, has never run a peer test, and therefore
  makes no status claim while correctly reporting that it is not testing. A
  fabricated `OK` or a fabricated in-flight test is a fail-closed gap, not a value.
- **A qualified snapshot is evidence about exactly one address family.** An
  IPv4-qualified router reports `Unknown` for the IPv6 rows; a stale or expired
  snapshot supports no claim at all, matching the publication path.
- **Only codes with a real detector are emitted.** `NoDescriptors` (5) follows an
  *attested* empty NetDB and `Offline` (2) follows a family that was configured
  but never bound. `ClockSkew` (1), `SymmetricNAT` (3), and `FullConeNAT` (4) are
  **never emitted** — i2pr owns no clock-skew or NAT-type detector — and
  `Proxy` (3), `Mesh` (4), and `Stan` (5) are never emitted because i2pr has no
  such posture. Their presence in the enumeration is wire completeness, not a
  capability claim.
- **An unattested NetDB is a gap, not `NoDescriptors`.** "No descriptors" is a
  claim about the NetDB, and an absent observation is not evidence of an empty
  one. Gating is per key: the two `error` rows fail closed without an attested
  NetDB while `status.v6` and `testing.v6` still answer.
- **The three transit selectors are owned by Plan 340**, below.

## Transit volume, bandwidth, and share (Plan 340)

The last three canonical Proposal 170 `RouterInfo` selectors —
`i2p.router.net.total.transit.bytes`, `i2p.router.net.bw.transit.15s`, and
`i2p.router.net.tunnels.shareratio` — have bounded production owners. The first
two are *"(adopted from i2pd)"*; the third is not, and no pinned reference
implements it. See
[`specs/references/proposal-170-transit-volume-and-share.md`](references/proposal-170-transit-volume-and-share.md)
for the normative record.

What is claimed, and what is not:

- **The honest product baseline is `0`, `0`, and `0.0`.** Production profiles
  never construct a transit data-plane owner, so a product i2pr router relays
  nothing and says so. The disabled posture holds **no counters at all**, so it
  is structurally incapable of reporting a non-zero volume: the zero is a
  property of the enforced posture, not a measurement substituted for a missing
  one. This advertises the *absence* of transit participation.
- **Transit participation is not enabled and is not advertised.** Enabling it is
  a separate product-posture decision gated by M11 re-qualification; the retained
  M11 evidence is bound to qualification SHA `6ab9dc2d` and not to the current
  tree (see the 2026-10-05 addendum in
  `plans/closure/transit-tunnels/268-status.md`).
- **Volume is counted only where i2pd counts it.** One relayed `TunnelData` cell
  accounts 1028 bytes (`TUNNEL_DATA_PAYLOAD_SIZE + 4`, matching i2pd's
  `TUNNEL_DATA_MSG_SIZE`), counted only on a `Forward` dispatch. An OBEP delivery
  terminating a tunnel addressed to this router is not transit and is not
  counted.
- **The 15-second figure is a trailing-window mean computed at request time**,
  not i2pd's one-hertz timer sample; the two differ only in the first second
  after a change of rate. No timer was added.
- **`tunnels.shareratio` is locally defined and labelled as such.** The Proposal
  leaves its arithmetic unspecified, so i2pr defines it as transit bytes over the
  attested cumulative sent total, clamped to `1.0`. A router that does not
  participate reports `0.0`; a router that does participate **fails closed**
  without an attested denominator rather than publishing a guess. A client
  needing the configured bandwidth-share percentage must read the RouterInfo
  `share` option.
- **An absent owner is a gap, never a zero.** An unpublished posture, an
  unreadable counter, or a missing denominator fails the whole request closed
  with the field and owning plan named, and never returns a partial RouterInfo.

## Outbound proxy secret owner (Plan 341)

Plan 327 was blocked on a credential it could not produce. The inbound
`ProxyCredentials` is **deliberately one-way** — it keeps
`SHA-256(username:realm:password)` and drops the password — which is correct for
a listener that only verifies presented pairs, and useless for an outproxy the
router must *authenticate to*. A restart-safe, non-echoing owner now exists. See
[`specs/references/proposal-170-outbound-secret-owner.md`](references/proposal-170-outbound-secret-owner.md).

What is claimed, and what is not:

- **The owner exists; no outproxy does.** Nothing routes anywhere yet. The
  provider, the HTTP/CONNECT/SOCKS request integration, and the canonical
  `ProxyList` / `UseOutproxyPlugin` / `OutproxyAuth` / `OutproxyType` /
  `SSLProxies` semantics are Plan 342, and no tunnel option reads this store yet.
  **Plan 327 remains blocked** and no outproxy capability is advertised in
  `specs/support.toml`.
- **Stored outbound secrets are router-bound and non-transferable.** The key is
  `HKDF-SHA256(salt = "", ikm = router signing seed, info = "i2pr:outproxy:secret-box:v1")`
  over the identity already persisted and reloaded every start, so a restart
  recovers the credential with no new key file, and a sealed form copied into
  another router's configuration derives a different key and fails to open.
- **Inbound and outbound stored forms are different values.** `$i2pr1$` marks a
  one-way inbound verifier; `$i2pr1o$` marks a sealed outbound secret. Neither is
  ever reinterpreted as the other, and the inbound path is unchanged and still
  one-way.
- **Nothing echoes.** Plaintext lives only in a fixed-size zeroizing buffer with
  no `Debug`, `Display`, or `Clone`; `open` returns that type rather than a
  `String`, so a recovered credential cannot be formatted into a log by an
  ordinary `{:?}`. Errors are fixed strings.
- **Every path fails closed.** Unmarked, oversized, truncated, padded, tampered,
  wrong-router, invalid-UTF-8, and empty results are all errors with no partial
  plaintext, and there is no best-effort path. A fresh CSPRNG nonce is drawn per
  seal, because nonce reuse under a fixed key would leak plaintext pairs.
- **No pinned reference is authority for this construction.** Pinned i2pd
  `2c69414` has no I2P-routed outproxy at all — its outproxy is a clearnet
  upstream defaulting to `127.0.0.1:9050` with no stored password — and the Java
  at-rest scheme was not verified, so no interoperability claim about credential
  storage format is made.

## I2P-routed outproxy provider (Plan 343)

Plan 327 was blocked on a missing outproxy. The provider now exists in both
halves — a runtime-neutral policy layer and a daemon route owner — so a later
plan has something to wire a request path to. See
[`specs/references/proposal-170-outproxy-provider.md`](references/proposal-170-outproxy-provider.md).

What is claimed, and what is not:

- **The provider exists; no request path uses it.** No Proposal 170 option sets
  an outproxy and no HTTP or SOCKS handler consults the provider, so the code is
  exercised only by its own tests. This is infrastructure, **not** a capability:
  the option surface, the request-path integration, and the loopback outproxy
  wire lane are Plan 342, and **Plan 327 remains blocked**. No outproxy
  capability is advertised in `specs/support.toml`.
- **No direct clearnet capability exists, in any path, including failures.**
  There is no fallback branch because there is never a fallback. This is
  enforced three ways: `OutproxyEndpoint::parse` refuses any outproxy entry that
  is not an I2P destination; a clearnet target with no provider is a typed
  refusal rather than a direct route; and rules 9-11 of
  `scripts/check-service-tunnel-boundaries.sh` scan both outproxy files for
  socket, resolver, and plugin spellings, with a positive control so the guard
  cannot silently become vacuous. All three static inversions were shown to fail
  closed.
- **The direct path's `.i2p`-only grammar is not weakened.** The outproxy
  clearnet target is a **separate grammar** that shares no parse result with
  `http::target`; the pre-existing `absolute_form_rejects_clearnet` and
  `authority_form_rejects_clearnet` rows are unchanged and green.
- **An `.i2p` target is never routed through an outproxy.** The bypass is
  decided from the target's own spelling, before any provider is consulted, and
  is re-checked inside the route opener rather than trusted from the caller. The
  `.i2p` spelling is accepted by the outproxy grammar precisely so this decision
  lives in one enforced place instead of in every call site.
- **No resolver and no DNS leak.** The clearnet host is an opaque label that the
  **outproxy** resolves. i2pr performs no name resolution, and IP literals are
  refused outright — an outproxy can reach them, and relaying them would be a
  port-scan primitive.
- **Selection, retry, backoff, connect timeout, and handshake read are all
  bounded**, and every operator input is clamped to a hard ceiling rather than
  rejected, so no configuration can produce an unbounded loop or socket wait.
- **The credential moves through Plan 341's owner and nowhere else.** The stored
  form is opened only while building the header, an armed credential that cannot
  be recovered is an error rather than an unauthenticated request, and a SOCKS
  outproxy refuses an HTTP Basic credential instead of dropping it.
- **`OutproxyType` is a closed vocabulary**, not a provider name and never a
  command, path, or module, so no spelling reaches anything executable.
  `UseOutproxyPlugin` semantics are not implemented here.
- **No pinned reference is authority for this design.** Pinned i2pd `2c69414` has
  no I2P-routed outproxy at all — its outproxy is a clearnet upstream defaulting
  to `127.0.0.1:9050` — and the Java spellings for these fields were not
  verified, so the `OutproxyType` vocabulary and the `SSLProxies` subset rule are
  **i2pr's own locally defined design** and no interoperability claim is made.

## Interoperability matrix

Each milestone should maintain an executable or machine-readable matrix similar to:

| Protocol | Direction/role | Java I2P | i2pd | I2P+ | Emissary/go-i2p | Evidence |
|---|---|---:|---:|---:|---:|---|
| NTCP2 | initiator | pending | pending | family duplicate | optional | test log/vector |
| NTCP2 | responder | pending | pending | family duplicate | optional | test log/vector |
| NetDB lookup | requester | pending | pending | family duplicate | optional | trace/result |
| Tunnel build | creator | pending | pending | family duplicate | optional | testnet artifact |
| Transit tunnel | participant / IBGW / OBEP | retained/deferred | Plan 258 multicell emission retained; Plan 259 endpoint-model conclusion corrected via retained-blocked Plan 260 (creator-owned build exhibited, receipt blocked on forward/sustainability boundaries); Plan 261 B-sender topology proven through counted-id addressing, receipt blocked on the B3 self-delivery loopback boundary; Plan 262 ownership corrected with receipt proven live (diag4 `0/6/1` tuple-bound) but full matrix stopped on sustainability; Plan 263 harness landed with receipt (`0/18/1`) + multicell (max 2) re-proven on `9bd2f39a` but single-mesh two-pass stopped on sustainability; Plan 264 lane + gate landed with 5 epochs 2/2 on `6ab9dc2d` but emission epochs stopped on windows; Plan 265 executed the manifest-v5 fixed budget on `4682920e` with zero semantic contradictions and closed `ibgw-data` (4/8) + `participant-lifecycle` (5/8) while `receipt` reached 1/8; Plan 266 executed the ladder contract on `a9803ca` with zero semantic contradictions and reached `receipt` 1/8 with every rung-6 attempt an anchored accepted-id drop population; Plan 267 executed the disposition contract on `315fb0d` with zero semantic contradictions and reached `receipt` 0/8 with 427 drop rows dominated by accepted-yet-not-found with delays in minutes; Plan 268 executed the path contract on `cc9b40c` with zero semantic contradictions and closed `receipt` 3/8 with the install-path divergence falsified; full receipt-capable M11 qualification complete with exact-head ordinary CI green | family duplicate | optional | Plans 254-264 retained evidence + Plan 265 retained-blocked evidence (24 retained attempts, one qualification SHA, input-side opportunity classification, zero tolerated i2pr semantic contradictions) + Plan 266 retained-blocked evidence (8 retained attempts, ladder rows, zero semantic contradictions) + Plan 267 retained-blocked evidence (8 retained attempts, disposition/timing rows, zero semantic contradictions) + Plan 268 passed evidence (8 retained attempts, install-path rows, 3 completions, composition passed, zero semantic contradictions, exact-head CI green) |
| Streaming | connect/listen | retained/deferred full-router compatibility at Plan 247 | passed bidirectional matrix via Plan 193 | family duplicate | optional | Plan 193 external transcript + Plan 247 retained Java boundary |
| SAM | client-facing server | client tests | client tests | client tests | optional | protocol transcript |
| SSU2 | initiator/responder | pending (secondary debt) | direct IPv4 loopback both directions via Plan 161 lane | family duplicate | optional | `plans/closure/ssu2/161-status.md`, `tests/integration/ssu2/run-independent.sh` |
| Router I2NP preflight | daemon-owned dispatch/delivery | n/a (router-internal) | bidirectional DeliveryStatus control via Plan 184 lane (no tunnel/NetDB/Streaming claim) | n/a | n/a | `plans/closure/mixed-router-interop/184-status.md`, `tests/integration/m6-interop/run-preflight.sh` |
| I2CP | router-facing server | client tests | client tests | client tests | optional | protocol transcript; Plan 164 structural codecs (`crates/i2pr-api/src/i2cp/`), Plan 165 connection/session/option state machines (typed `ConnectionStateMachine`, canonical `SessionConfig` verification with injected clock, bounded option disposition + `DestinationConfig` projection, bounded `SessionRegistry` with reserve/commit/rollback, reconfiguration taxonomy, typed `I2cpAction` vocabulary), Plan 166 client-owned destination + Standard LeaseSet2 bridge (`DestinationOwnership`, `DestinationPublic`, non-`Clone` redacted `InboundDecryptionCapability`, atomic `install_client_lease_set2` with signature + lease ownership + expiry + decryption-key match, typed `LeaseRequest` and `I2cpAction::RequestVariableLeaseSet` sourced from real inbound tunnels), Plan 167 loopback server runtime (`crates/i2pr-daemon/src/i2cp.rs`: `0x2a` preamble, incremental `FrameDecoder`, per-connection `ChildScope`, supervised admission semaphore, bounded per-connection read/write ceilings, atomic `install_client_lease_set2`, single cleanup path; disabled by default, loopback-only), Plan 168 message data plane (`crates/i2pr-api/src/i2cp/data_plane.rs` adds the bounded `I2cpMessageOutcome` vocabulary, `I2cpDataPlaneAction::{EnqueueOutboundPayload, DeliverInboundPayload, ResolveDestinationLookup}`, `PendingStatusTable`, `InboundPayloadQueue` with `WIRE_OVERHEAD_BYTES = 14`, and per-session ceilings `MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION = 64`, `MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION = 128`, `MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION = 64`, `MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION = 64 KiB`, `MAX_DESTINATION_LOOKUP_HORIZON = 10 s`, `MAX_MESSAGE_EXPIRATION_HORIZON = 1 h`; `crates/i2pr-daemon/src/i2cp.rs` projects them into the existing `i2pr_client::DestinationRuntime::enqueue_outbound` seam and drains `MessagePayload` inbound frames through a `tokio::sync::Notify`; cross-session local loopback shortcut routes two active session destinations through the receiving session's inbound queue; `DestLookup` resolves through the local destination registry; `GetBandwidthLimits` returns the config-derived client ceiling; eighteen real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` exercise every Plan 168 §11 case), and Plan 169 self-composed local product and hardening (`crates/i2pr-daemon/src/i2cp.rs` adds `handle_reconfigure_session` + `apply_reconfigure` + `ReconfigurationOutcome`; `I2cpSessionState::last_options` carries the atomic reconfigure baseline; `handle_destroy_session` drains the per-session Plan 168 data-plane bookkeeping synchronously so repeated DestroySession/CreateSession cycles retain zero inbound queue, status correlation, or outbound slot; 31 real-TCP black-box tests across `crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` (5 tests), `crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` (20 tests), and `crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` (6 tests) cover every Plan 169 §4/§5/§6/§7 case; Plan 171 retains that surface and hardens the common per-connection terminal path (`handle_connection` shuts the TCP stream down explicitly before bookkeeping release; the strict wrong-preamble row plus its non-paused companion prove the close with zeroed baselines); SAM router-owned product regressions and the Plan 167 listener regression in `i2cp_loopback.rs` remain green, and Plan 170 independent clients and final closure (exact-pinned Java I2P 2.13.0 `9134f808337b401e8e53c73734c81fab04280c9d` and go-i2cp `b529ee1c10a6011558b4d69fc9436a4afc489eac` exchange digest-matched 25 B/32 KiB payloads both directions through the loopback daemon under `tests/integration/i2cp/run-independent.sh` (9 fail-closed rows) with `scripts/check-i2cp-acceptance-evidence.sh` enforced in routine CI and manual `.github/workflows/i2cp-external.yml`; daemon deltas are `ReplyAndFollowup` RequestVariableLeaseSet after CreateSession, `0.x.y` version negotiation, empty-auth GetDate acceptance, `messageReliability=none` best-effort mapping, and the ElGamal-legacy-slot policy relocation where SessionConfig/`DestinationPublic` accept the legacy slot and X25519 is enforced at `install_client_lease_set2`, fail-closed for legacy slots); no `HostLookup`/`HostReply` resolution, no remote-I2CP/public-network claim, Milestone 9 final acceptance closed via Plan 172 (experimental, loopback-only; Plan 170 wire/data-plane retained-passed, independent LeaseSet2 lifecycle passed-via-plan172 with 24 fail-closed rows) |

Interoperability tests must run only in an authorized private or controlled mixed-router testnet until the milestone plan explicitly permits public-network observation.

### Milestone 10 service-tunnel profile ledger

Bounded M10 application profile (experimental, loopback-only,
disabled by default; Plans 174–180 local product, Plan 182 local
round-trip corrective, Plan 181 independent-application-client
evidence retained, Plan 202 production remote
Destination/Streaming composition, Plan 203 positive remote HTTP
+ IRC application interop):

- generic TCP client/server tunnels with digest-matched byte
  round-trip (small, >=32 KiB multi-segment, reverse, half-close
  EOF propagation, siblings) and restart-stable persistent
  server destinations;
- HTTP/1.1 `.i2p` proxy plus CONNECT with the conservative
  privacy rewrite (User-Agent replaced, Referer/From stripped),
  `.i2p`-only targets, hop-by-hop stripping, smuggling
  rejection, and bounded typed 400/403/502 responses;
- SOCKS5 no-auth DOMAINNAME CONNECT with hostname-at-proxy
  semantics and the default 443-only port policy;
- IRC client profile with the runtime-neutral privacy filter
  (USER/PING/QUIT/PART rewrites, CTCP ACTION allowed, DCC and
  unsupported CTCP dropped, unknown commands dropped);
- IRC server profile with the authenticated peer-Destination
  hostname projection (`<52-char base32>.b32.i2p`) and bounded
  registration interception;
- bounded transactional reconcile with generation/draining
  lifecycle and unified cross-service resource accounting;
- a typed `ServiceDestinationDelivery` capability surface owned
  by the `ServiceTunnelManager` and installed once per daemon
  via `install_router_delivery_handle`; the typed
  `RoutingDecision::LocalCoOwned` / `RemoteRouter` /
  `RemoteUnresolved` enum drives the resolve path; the bounded
  `RemoteDeliveryCounters` surface emits twelve positive
  observations on every counted path; positive Direction A
  external drivers cover the `m10_remote_destination_streaming_composition`
  transport row and the `m10_positive_remote_http_and_irc_application_interop`
  application rows against exact-pinned i2pd 2.61.0.

Explicitly unsupported or deferred (fail-closed, never silently
bridged): clearnet outproxy; SOCKS UDP ASSOCIATE; SOCKS BIND;
SOCKS4; SOCKS username/password auth; transparent proxying;
HTTP/2 or HTTP/3 proxy termination; TLS interception; IRC DCC;
WEBIRC and cloaked-hostname extensions; arbitrary remote admin
exposure; general address-book/subscription management; transit
or floodfill router roles (M11/M12); broad public-network
interoperability beyond exactly demonstrated rows. Remote
independent-I2P HTTP/IRC service interop is now proven on the
dedicated M6 interop lane through Plan 203
(`passed-m10-positive-remote-http-and-irc-application-interop`),
the two `remote-independent-*` rows flip `blocked → passed-on-env`
when the lane provisions the SSU2 endpoint + bind tuple and the
driver emits the `http-remote-application-established` /
`irc-remote-application-established` evidence keys. M10 final acceptance is authoritative through the hosted Plan 215 re-verification of Plan 214. Plan 248 supersedes Plan 204's Java-dependent convergence gate without relabeling the Java lane. See `plans/closure/service-tunnels/204-status.md`,
`plans/closure/service-tunnels/203-status.md`, and `plans/closure/service-tunnels/202-status.md`.

## Fuzzing targets

At minimum, fuzz:

- all top-level common-structure and I2NP decoders;
- RouterInfo, Destination, LeaseSet and signed-container parsing;
- NTCP2 and SSU2 plaintext block parsers after authenticated decryption;
- handshake state transition inputs with deterministic crypto seams where safe;
- tunnel build records and tunnel message fragmentation/reassembly;
- garlic clove and ECIES payload parsing;
- streaming packets and option blocks;
- SAM and I2CP framing, tokenization and option parsing;
- HTTP proxy request-line/header rewriting and SOCKS negotiation.

Fuzz harnesses must have bounded input size and should assert no panic, no excessive allocation, no infinite loop and stable error classification where practical.

## Differential tests

Use differential testing selectively. Valuable comparisons include:

- canonical structure serialization;
- Base64/Base32 and hash derivation;
- signature verification and RouterInfo hashes;
- NTCP2/SSU2 block encoding after supplying identical keys/nonces;
- tunnel build-record crypto;
- streaming packet encoding;
- SAM command parsing and response status.

Do not expose private test keys to public infrastructure or depend on nondeterministic production routers for unit tests. Prefer local fixtures and dedicated test identities.

## Security regression corpus

Every protocol parser should retain minimized fixtures for discovered failures:

- truncation at every field boundary;
- maximum and maximum-plus-one lengths/counts;
- duplicate, unknown and out-of-order fields;
- expired, future-dated and skewed timestamps;
- invalid signatures and authenticated-encryption tags;
- replayed handshakes, packets, I2NP IDs and tunnel records;
- decompression/archive expansion limits for reseed bundles;
- fragmented messages exceeding per-message or per-peer budgets;
- slow-read/slow-write behavior and partial frames;
- cancellation during cryptographic work, persistence and publication.

A production bug is not closed until a fixture or deterministic test prevents recurrence.

## Capability advertisement

Capability publication is a security and interoperability contract. Before changing `router.version`, RouterInfo capabilities, transport addresses/options, LeaseSet type support, SAM version negotiation or I2CP behavior:

1. identify the exact feature implications in the official specifications;
2. verify all implied mandatory behavior is implemented;
3. add mixed-router tests for the changed claim;
4. test downgrade/unsupported peers;
5. update the relevant dossier and protocol-support matrix.

`i2pr` should initially advertise the lowest truthful current feature level compatible with its implemented subset, not mimic another router’s release string.

## Evidence retention

Store stable protocol vectors and minimized malformed fixtures in the repository. Store large captures, generated testnets and sensitive operational logs outside Git history, with scripts and hashes sufficient to reproduce them. Redact live peer identities, IP addresses, destination keys, session keys and potentially identifying timing data before retaining or publishing artifacts.