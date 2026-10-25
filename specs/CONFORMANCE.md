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

### External qualification gates: the first failure owner (Plan 495)

An external lane runs against a **stock peer that has its own admission and
selection state**. That is a second, independent source of failure, and a gate
that cannot tell the two apart is not a gate. The governing model is
[`specs/QUALIFICATION-GATES.md`](QUALIFICATION-GATES.md); the operator
procedure is
[`docs/operations/ssu2-private-lan-qualification.md`](../docs/operations/ssu2-private-lan-qualification.md);
Plan 495 owns the enforcement, which is
[`scripts/plan495-peer-eligibility.py`](../scripts/plan495-peer-eligibility.py)
— an offline checker that opens no socket, starts no process and touches no
network, and whose `--self-test` runs in the repository's routine floor.

Every predicate it evaluates is derived from the **pinned reference's own
source** and carries that source's file and line. The pins are frozen:
i2pd `2.61.0` (`635b013a612ff47278ef02acf8580a28e10e26c5`) and Java I2P
`2.13.0` (`9134f808337b401e8e53c73734c81fab04280c9d`).

Five rules bind every external lane:

1. **The peer's admission and selection are preconditions, not assumptions.**
   Before a process launches, the lane determines from the pinned reference's
   own source whether the peer *could* admit our RouterInfo and *could* select
   us — its ingest gates and its selection filters. A stock peer will not dial
   an arbitrary peer: it may require a specific RouterInfo admission
   (a version above its floor, a present and matching network id, a valid
   signature), a specific eligibility predicate, or a reason to explore us at
   all. A predicate that cannot be determined offline **fails closed**; it is
   never treated as satisfied. This is the concrete form of
   `plans/README.md`'s bounded-execution rule 2 and of the external-router
   handoff rule that *"a randomized peer selector cannot guarantee one named
   candidate unless its actual admission and selection are verified."*
2. **Every outcome names its failure owner.** A lane result is exactly one
   of: `pass`, `i2pr-defect`, `peer-admission-refused`,
   `peer-selection-refused`, `peer-state-poisoned`, `environment-unavailable`,
   `timeout`. Only `i2pr-defect` and `pass` are statements about `i2pr`. A peer
   that admits our RouterInfo and then declines to select us is
   `peer-selection-refused` — a result about that peer's selection state,
   never an `i2pr` defect and never a pass. The claim tiers are recorded
   independently, so a higher tier's failure never retracts a lower tier's
   pass.
3. **An isolated-topology result is not a live-network verdict.** In an
   isolated topology the peer has no alternative peers, so one bad
   interaction is the whole topology, and the peer's own state — profile
   store, selection filters, ban, cooldown — can dominate the outcome. A
   failure there is not evidence of a live-network failure, and a success
   there is not evidence of public reachability. Peer state is a first-class
   cause: a peer-side ban or cooldown does not consume an attempt budget,
   because a cooldown expiring is a material change.
4. **The failure owner is named on two axes.** The outcome class says *where*
   the run ended; a **cause** says *whose artifact* was responsible:
   `i2pr-lab-profile` (the operator-facing lab profile the preflight
   evaluates), `i2pr-implementation` (the daemon's own code path),
   `peer-state`, `intentional-withholding` or `unattributed`. A missing
   `netId` is refused by the peer, but the artifact at fault is ours — it is
   reported as `peer-admission-refused` with the cause `i2pr-lab-profile`,
   never as a peer-side condition and never as a pass. Only
   `i2pr-implementation` may accompany `i2pr-defect`. The
   deliberate-withholding control is the gate's own self-test: with a
   selection predicate deliberately withheld, the verdict is
   `peer-selection-refused` with the cause `intentional-withholding` and the
   peer's own reason — never `i2pr-defect`, never a pass.
5. **A lane's own readout is not a claim.** A per-host receipt is a
   **private local readout**: phase labels and counts only, never a key,
   RouterInfo, address, endpoint, payload, or transient message ID. Its
   `live_qualification` field is always `"not-run"`, because a local
   observation is not a two-host result. Only a reconciled lane evidence
   document can carry the two-host claim, and only after an authorized pair
   has actually run.

A gate that cannot separate these is not evidence, and its result must not be
cited for or against `i2pr`.

Plan 476 supplies the **guarded per-host process/session executor** for the
authorized two-host private-LAN lane,
[`tests/integration/ssu2/plan476_live_executor.py`](../tests/integration/ssu2/plan476_live_executor.py).
Its `run` mode is a strict, ordered, fail-closed state machine — **authorization
→ gate → withholding control → hygiene → spawn → collect → emit** — and it
launches nothing until the Plan 495 gate says `lane-ready`, the
deliberate-withholding control behaves, and the peer-state hygiene record is
clean. Its `reconcile` mode joins two owner-exchanged host receipts into the
`i2pr-plan495-lane-evidence-v1` document; it refuses a mismatched run epoch,
a duplicate role, or a missing peer receipt, and it **never** emits
`i2pr-defect` — that outcome is a reviewed human judgement, and only the
cause `i2pr-implementation` may accompany it. The tier ladder is not
restated here; it is canonical in
[`specs/QUALIFICATION-GATES.md`](QUALIFICATION-GATES.md) §5. In this lane T7
(a tunnel was built) and T8 (external SAM/Streaming acceptance) are out of
scope and are always `not-attempted`.

The daemon side of that lane is the sanitized, count-only local receipt
export,
[`crates/i2pr-daemon/src/ssu2_qualification_export.rs`](../crates/i2pr-daemon/src/ssu2_qualification_export.rs).
It is enabled only by the `[ssu2] qualification_receipt_path` key, which is
**absent by default** and is honoured only with `ssu2.enabled = true` and an
absolute path; the parent directory must already exist and the file is
created mode `0600`. The operator procedure is
[`docs/operations/ssu2-private-lan-qualification.md`](../docs/operations/ssu2-private-lan-qualification.md).
Current plan state is in `plans/registry.md`.

**The live two-host lane is still environment-blocked.** The Plan 476
executor and its offline simulator are complete and source-verified, but no
authorized two-host pair has been executed, so **no live two-host SSU2
result may be claimed**. The lane is infrastructure-complete, not
capability-complete: until an authorized pair runs, this section supports
the *shape* of a result, never a result.

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
preserved as history and is superseded for forward execution by Plan 346; Plan 346 passing does
not itself claim cross-router interoperability. Plan 347 stopped at 0 of 4 directions and is not
the forward owner either — the live rows belong to Plans 374 (i2pd) and 375 (Java), converged by
Plan 377.

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
- **Published, served, and consumed locally; still not advertised and not live-verified against a
  reference.** The wire-layer gaps this bullet used to record are closed: a daemon configuration
  now publishes type-5 records (Plan 334 reclosed after 337/338), the controlled floodfill stores
  and serves type 5 under its blinded storage key (Plan 350), and both ELS2 resolvers have
  production callers reached through a service-tunnel remote target (Plan 351 / ADR 0033). Type 5
  remains `advertised = false`, and the local capability above the crypto boundary is still an
  i2pr-to-i2pr statement.
- **Known interop limitation, corrected at the policy level by Plan 346; live cross-router proof is
  Plans 374/375.** Under the strict-only decision an i2pr-signed type-5 record was unverifiable by
  `i2pd` and Java I2P. Plan 346 makes the ELS2 use of type 11 an explicit bounded compatibility
  profile (ADR 0032 (Proposal 170)): i2pr now publishes the deployed transcript and accepts both.
  That is a **cryptographic-boundary** result, measured against executed Java I2P and i2pd output.
  **Crypto-boundary cross-verification is not interoperability** — it compares signatures over
  controlled inputs and exercises no router, no NetDB, and no tunnel. Real ELS2 interoperability
  requires the whole chain in one row: a real `DatabaseStore` type 5, storage under the blinded
  key, a real blinded-key `DatabaseLookup`, outer deployed-profile verification, authorization and
  decrypt, inner LS2 validation, and a streaming/application payload. No such row exists yet.
  Plan 347 is the stopped historical attempt; Plans 374 (i2pd) and 375 (Java) own the two
  reference-specific directions and Plan 377 converges them. Until Plan 377 passes, no
  cross-router ELS2 interoperability is claimed and `advertised` stays `false`.
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
    result; any Java or i2pd live direction. The blinding rotates daily and there is no
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
- **The ELS2 *consumer* client credential is i2pr-local and reaches the control plane through the
  typed `CustomOptions` seam (Plan 380).** Proposal 170 defines no field for it: the publisher side
  spells the authorized clients as `LeaseSetClientAuths`, and a consumer is expected to already
  hold the matching PSK or DH key out of band. So the credential travels as
  `{"CustomOptions": {"i2pr": {"LeasesetClientCredential": "..."}}}`, and
  `i2pr-i2pcontrol::extension_options` owns that shape. Three consequences, all deliberate:
  the frozen `PROPOSAL_TUNNEL_MANAGER_FIELDS` inventory is unchanged, so this router still claims
  exactly the Proposal options Proposal defines; the **untyped** `CustomOptions` blob form stays
  refused with the historical "no safe typed allowlist" error, because the reason it was refused
  has not changed; and an unrecognised extension name is refused rather than ignored, so a
  mistyped setting cannot leave a service in a posture nobody chose. The value is secret material,
  so it is sealed under the router identity before the definition can reach a generation file, it is
  reported on `get`/`rawConfig` as a boolean and nothing else, and it is not in
  `SPECRET_OPTIONS` — that inventory is the frozen Proposal option set.
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
  to end — which belongs to Plan 374 (i2pd), Plan 375 (Java), and Plan 377 (convergence). A
  control-created encrypted service is therefore *implemented* but
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

- **The honest default baseline is `0`, `0`, and `0.0`.** The default
  production profile constructs no transit data-plane owner, so a default
  product i2pr router relays nothing and says so. The disabled posture
  holds **no counters at all**, so it is structurally incapable of
  reporting a non-zero volume: the zero is a property of the enforced
  posture, not a measurement substituted for a missing one. This
  advertises the *absence* of transit participation. **Plan 502** makes an
  *opted-in* profile construct the owner and report measured counters —
  the default profile is unchanged, and see "The transit data-plane owner
  in the product composition (Plan 502)" below.
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

## NetDb peer profile store (Plan 488)

Plan 488 layered a **peer profile** over the flat Plan 103 validated-RouterInfo
store. The profile is internal state: a capacity tier, a reachability state,
bounded counters, and a bounded score per peer, plus a pure selection policy
that draws diverse candidate sets from it.

What is claimed, and what is not:

- **The claim is a bounded, tiered, honest peer store — not a participation
  capability.** Plan 488 adds no network role. The router still cannot be
  found by a peer, still enters the network only through a reseed, and still
  carries no other router's traffic. The later phases of the
  [network-participation roadmap](../plans/subsystems/network-participation-roadmap.md)
  (Plans 489–494) own those roles, and the advertisement comes last.
- **Nothing is advertised.** The plan changes no `router.version`, no `caps`,
  no published RouterInfo, and no transport address. The capacity tier is
  derived from a peer's *signed* `caps` option, which is self-asserted data;
  it is treated as a selection hint and never as trust. A peer that declares
  no capacity class is `Untracked` — an honest *unknown*, never a downgrade.
- **The failure model distinguishes "never tried" from "did not answer".**
  `ProfileState` keeps `NeverTried`, `Answered`, `AnsweredEmpty`, `Refused`,
  `Misbehaving`, `Unanswered`, and `Unreachable` apart, and the maintenance
  pass demotes an aged failure one tier at a time and then drops it with a
  counted transition. A peer is never silently forgotten.
- **The score is bounded and explainable.** It is the sum of named credits
  and penalties inside `[0, 1000]`, and the ordering rule is a requirement
  rather than an implementation detail: *a peer that answered a lookup
  without holding the record ranks above a peer that did not answer at all.*
- **The selection policy is pure and deterministic.** The same store
  contents and the same constraints produce the same candidate set, byte
  for byte. The /16 diversity rule is never violated to reach the requested
  count: a selection that cannot satisfy the rule returns fewer candidates.
- **Receipts and evidence carry counts and digests only.** A receipt never
  names a peer: no hash, no address, no key material, no session state, no
  payload. The private-mesh lane asserts this against the rendered
  receipts and against the evidence file.
- **A corrupt or oversized persisted projection is discarded with a count**,
  never a startup failure. The projection carries a SHA-256 body digest, so
  truncation or mutation is detected rather than silently applied.
- **The selection policy is not wired into the router's tunnel building.**
  Plan 488 delivers the policy and its inputs; the wiring is the consumer's
  change and belongs to the participation phases that follow. No existing
  tunnel-building behaviour changed.

Evidence: `crates/i2pr-netdb/src/profile.rs`,
`crates/i2pr-netdb/src/profile_store.rs`,
`crates/i2pr-netdb/src/selection.rs`,
`crates/i2pr-netdb/tests/profile_boundary.rs`,
`crates/i2pr-daemon/src/netdb_participation.rs`,
`crates/i2pr-daemon/tests/netdb_participation_mesh.rs`, and the sanitized
lane artifact `tests/integration/network-participation/evidence/plan488/result.json`.
The closure record is `plans/closure/network-participation/488-status.md`.

## NetDb floodfill-indexed routing table (Plan 489)

Plan 489 replaced the flat, undifferentiated peer map with a **bounded,
day-keyed Kademlia routing table over the floodfill set only**, plus a bounded
exploration job and a pure optimized lookup plan that draws its candidates from
that table.

What is claimed, and what is not:

- **The claim is a routing table and a selection policy — not a participation
  capability.** Plan 489 adds no network role. The router is still not found
  by a peer, still enters the network only through a reseed, and still carries
  no other router's traffic. The table and the optimized lookup are **not
  wired into the router's tunnel building**; the wiring belongs to the
  participation phases that follow. The `[netdb]` keys that expose them
  (`routing_table_enabled`, `exploration_enabled`) default to **off**, and the
  exploration cannot be enabled while the routing table is off.
- **Nothing is advertised.** The plan changes no `router.version`, no `caps`,
  no published RouterInfo, and no transport address.
- **Only floodfills are indexed.** A peer whose own signed `RouterInfo` does
  not advertise the `f` capability is refused and counted, however well
  connected it is. A peer named by a *verified* floodfill enters the table with
  weaker "suggested" evidence — a peer that is not itself a verified
  floodfill can never introduce another peer — and is promoted to signed
  evidence the moment its own record is resolved. A peer whose resolved record
  shows it is *not* a floodfill is evicted.
- **The keyspace is date-keyed.** A peer's position is
  `SHA256(router_hash || UTC_yyyyMMdd)`, the same daily key the lookup path
  already uses, and the partition rotates with the UTC day. The persisted
  projection carries the day it was built for, so **a restart never
  resurrects an expired table**.
- **Selection is target-relative and closest-first.** The anchor decides the
  partition and which peer is evicted first; it never decides which peer a
  lookup prefers. The plan splits the table's peers into *queryable* — the
  router holds the peer's `RouterInfo`, so it can be asked — and *pending* —
  the table knows the peer but its `RouterInfo` is unresolved, so it cannot be
  asked yet. A peer without an address is never asked.
- **The exploration job is bounded and cancel-safe.** A peer budget, a
  deadline, and an outstanding-request table with a hard ceiling. A timeout
  releases the outstanding slot but the query still counts against the
  budget; cancelling releases every slot and consumes the budget.
- **The table is runtime-neutral and reproducible.** It reads no clock, opens
  no socket and touches no file; every time input is supplied by the caller.
  The crate-local static guard enforces this, including a clock read as a
  forbidden construct, with its own inverted probe.
- **Receipts and evidence carry counts and digests only.** A receipt never
  names a peer: no hash, no address, no key material, no session state, no
  payload. The private-mesh lane asserts this against the rendered receipts
  and against the evidence file, and it asserts that the peer the lookup asks
  is a member of the routing table — the discriminator that distinguishes a
  table-driven lookup from a hand-supplied peer.
- **A corrupt, oversized or expired persisted projection is discarded with a
  count**, never a startup failure. The projection carries a SHA-256 body
  digest, so truncation or mutation is detected rather than silently applied.

Evidence: `crates/i2pr-netdb/src/bucket_set.rs`,
`crates/i2pr-netdb/src/exploration.rs`,
`crates/i2pr-netdb/src/optimized_lookup.rs`,
`crates/i2pr-netdb/tests/profile_boundary.rs`,
`crates/i2pr-netdb-persist/src/routing_table.rs`,
`crates/i2pr-daemon/src/netdb_participation.rs`,
`crates/i2pr-daemon/tests/netdb_routing_mesh.rs`, and the sanitized lane
artifact `tests/integration/network-participation/evidence/plan489/result.json`.
The closure record is `plans/closure/network-participation/489-status.md`.

## SSU2 introducer (Bob) relay role (Plan 490)

Plan 490 gave the router an **opt-in SSU2 introducer role**: the
specification's NAT-traversal helper. A firewalled peer (Charlie) asks this
router for a relay tag and publishes it; a requester (Alice) that can reach
this router presents that tag to name the target; the router forwards a
Relay Intro to the target and answers Alice with a Relay Response.

What is claimed, and what is not:

- **The claim is a role behind an explicit switch — not a public-network
  participation claim.** `ssu2.introducer_service` is **available** and
  **default off**. When it is off the role does not start, and the
  configuration status says so truthfully: the role is *available but not
  enabled*, not *unavailable in this milestone*. The plan changes no
  `router.version`, no `caps`, no published RouterInfo, and nothing is
  advertised.
- **The relay tag identifies the target, and the target's hash is what the
  requester's signature binds.** The role owner resolves the target through
  the tag before it verifies anything, so a request that names a target this
  router never issued a tag for is refused as an unknown tag.
- **The requester's key comes from a validated RouterInfo, never from the
  wire.** The role owner resolves it through the `IntroducerSignerResolver`
  seam; an unknown requester is refused and counted.
- **Every refusal is typed, counted, and leaves no state behind.** The
  reasons are a closed vocabulary (`disabled`, `not_our_role`,
  `rate_limited`, `unknown_tag`, `expired_tag`, `unknown_requester`,
  `target_unknown`, `invalid_signature`, `stale_timestamp`,
  `duplicate_request`, `budget_exhausted`, `transport_refused`,
  `unknown_correlation`, `sender_mismatch`, `service_disabled`).
- **Admission precedes crypto.** The per-source rate limit is applied
  before the tag resolution and before any signature work, and the
  transport's 3× response budget is enforced before its own crypto.
- **Everything is bounded.** The per-source rate window, the tracked
  introductions (≤ 64), the retained rate sources (≤ 64), the transport's
  relay-tag and request tables, and the runtime's relay handoff queue
  (≤ 16) all have fixed ceilings.
- **The published introducer-only address form follows the specification.**
  An address that publishes introducers omits `host`/`port` and carries the
  `4`/`6` family capability alongside the introducer groups, at most three
  introducers in the deterministic order. The builder adds only the family
  flag implied by the reachability snapshot; it never invents a role
  capability and never publishes `caps=f`.
- **Receipts and evidence carry counts only.** A relay receipt never names
  a peer: no hash, no address, no tag value, no key, no payload. The private
  controlled-mesh lane asserts this against the rendered receipt and
  against the evidence file.
- **The transport stays runtime-neutral and clock-free.** The role and the
  publication builder take every time input from the caller. The crate-local
  static guard enforces this, with inverted probes for a socket use, a clock
  read, and a task owner.

Evidence: `crates/i2pr-transport-ssu2/src/relay.rs`,
`crates/i2pr-transport-ssu2/src/publication.rs`,
`crates/i2pr-transport-ssu2/tests/profile_boundary.rs`,
`crates/i2pr-runtime/src/ssu2_peer_relay.rs`,
`crates/i2pr-runtime/src/ssu2_runtime.rs`,
`crates/i2pr-daemon/src/ssu2_introducer.rs`,
`crates/i2pr-daemon/tests/ssu2_introducer_mesh.rs`, and the sanitized lane
artifact `tests/integration/network-participation/evidence/plan490/result.json`.
The closure record is `plans/closure/network-participation/490-status.md`.

## API-version floors and peer eligibility (Plan 491)

Plan 491 makes the router **honor the minimum-peer obligations that its own
published `router.version` attaches**. The specification's API-version table
attaches two duties to the `0.9.62` row — the level this router declares:

```text
minimum peers will build tunnels through, as of 0.9.68
minimum floodfill peers will send DSM to, as of 0.9.68
```

What is claimed, and what is not:

- **The claim is compliance with a level the router already declares — not a
  new capability.** `CONTROLLED_ROUTER_VERSION` is unchanged at `0.9.62`,
  its Plan 285 pinning test still passes, and the plan changes no
  `caps`, no published RouterInfo, no `router.version`, and nothing is
  advertised. There is **no new participation capability**.
- **The comparator is total and typed.** Every published `router.version`
  string maps to a comparable value or to an explicit `Unknown` (no version
  published) or `Uncomparable` (a value that cannot be compared) state. A
  malformed or absent version never panics and never compares as "newest".
- **The comparison is the specification's numeric rule.** `0.9.13` is at
  least `0.9.12`; `0.9.9` is *not* at least `0.9.10`; the `0.7.9 or lower`
  catch-all line is handled by the catch-all rule rather than by a
  lexicographic comparison.
- **The floors are derived, not hand-maintained.** The obligation rows are
  data in the comparator; the floors are derived from those rows and from
  the declared level. A literal copy of the floor in the policy module is a
  static-guarded violation.
- **The floors are specification constants, not operator knobs.** No
  configuration key can lower them. An operator cannot put the router below
  the level it declares.
- **Both enforcement points fail closed.** A peer whose version is unknown,
  uncomparable, or below the floor is never used as a tunnel-build peer and
  is never sent a DSM. When every candidate is refused, the build fails
  closed with the *reason* (`NoEligibleVersionPeers`), never a bare "no
  candidates" and never a loop.
- **Every refusal is counted and explainable.** A peer that was skipped is
  never a silent skip: the reason is one of `unknown-version`,
  `uncomparable-version`, or `below-floor`. The receipts carry counts and
  stable reason labels only — no peer hash, no address, no key, no version
  string, no payload.
- **The declared version is never edited to dodge a floor.** A peer that
  refuses the declaration is a finding, not a licence to edit it.

The two reference implementations are the behavioral oracles: Java I2P
2.13.0 applies `0.9.62` in `net.i2p.router.tunnel.pool.TunnelPeerSelector`,
and i2pd 2.61.0 applies `0.9.62` in `libi2pd/NetDb.hpp`
(`NETDB_MIN_HIGHBANDWIDTH_VERSION`, `NETDB_MIN_FLOODFILL_VERSION`).

Evidence: `crates/i2pr-proto/src/common/version.rs`,
`crates/i2pr-netdb/src/version.rs`, `crates/i2pr-netdb/src/profile.rs`,
`crates/i2pr-netdb/src/profile_store.rs`, `crates/i2pr-netdb/src/selection.rs`,
`crates/i2pr-netdb/src/publication.rs`,
`crates/i2pr-daemon/src/destination_peers.rs`,
`crates/i2pr-daemon/src/netdb_tunnels.rs`,
`crates/i2pr-daemon/src/netdb_participation.rs`,
`crates/i2pr-daemon/tests/version_eligibility_mesh.rs`,
`crates/i2pr-daemon/tests/version_floor_boundary.rs`, and the sanitized lane
artifact `tests/integration/network-participation/evidence/plan491/result.json`.
The closure record is `plans/closure/network-participation/491-status.md`.

## Transit participation posture (Plan 492)

Plan 492 turned the existing transit machinery on for an **operator-opted
profile**. What is claimed, and what is not:

- **The posture is opt-in and default-off.** A new `[transit]` configuration
  section carries `participation` (the single switch), `share_limit`,
  `participating_tunnels`, and `peer_participation_cap`. The default
  configuration does not change: a router that has never enabled
  participation does not relay another router's tunnel traffic.
- **The transit role is still not advertised and still not claimed for the
  public network.** `router.version`, `caps`, and the published RouterInfo
  are unchanged, and nothing is advertised. The posture is internal; the
  public participation qualification is the two-family lane's work.
- **The limits are bounded and validated before anything is constructed.**
  `share_limit` is a percentage of the enforced share, `participating_tunnels`
  and `peer_participation_cap` have named ceilings, and the per-peer cap may
  not exceed the router-wide count.
- **An opt-in requires an enforced share.** Participation without an enabled
  bandwidth policy is a configuration error, because the share limit is
  enforced against the governor's permitted share and would otherwise be
  advisory.
- **The share limit is enforced, not advisory.** The participation owner
  holds the governor's permit and revalidates it at the point of use; the
  relay is charged against the permitted share *before* the cell leaves the
  router; and exceeding the share refuses the relay rather than growing it.
- **The counts are the data plane's own.** The participating-tunnel count and
  the per-peer count are the transit service's registration counts, so the
  owner never keeps a second, diverging ledger of live tunnels.
- **Revocation is prompt and observable.** An operator policy change, a
  withdrawn or replaced permit, and a health loss each stop participation,
  count the stop, and record it.
- **The receipts are bounded and carry counts only.** No peer identity, no
  address, no tunnel id, no key, and no payload.
- **The dispatch order is unchanged** — creator/service first, transit
  second, unknown fail-closed — and the posture is consulted after the
  creator correlation check and before any slot is allocated.
- **The router's own build hops now satisfy Plan 488's /16 diversity rule
  and per-peer participation cap**, and a selection is recorded, so the cap
  is not advisory.

Evidence: `crates/i2pr-daemon/src/transit_participation.rs`,
`crates/i2pr-daemon/src/transit_owner.rs`,
`crates/i2pr-daemon/src/transit_compose.rs`,
`crates/i2pr-daemon/src/destination_peers.rs`,
`crates/i2pr-daemon/src/destination_tunnels.rs`,
`crates/i2pr-daemon/src/config.rs`,
`crates/i2pr-daemon/tests/transit_participation_posture.rs`,
`crates/i2pr-daemon/tests/transit_participation_boundary.rs`,
`crates/i2pr-daemon/tests/transit_participation_lane.rs`, and the sanitized
lane artifact
`tests/integration/network-participation/evidence/plan492/result.json`. The
closure record is `plans/closure/network-participation/492-status.md`.

## NTCP2 private-LAN operator profile (Plan 496)

Plan 101 refused every NTCP2 activation that was not the explicit
controlled-loopback profile. Plan 496 keeps that refusal for every
configuration the old guard rejected and adds **one** narrowly-scoped
operator profile: a guarded, RFC1918-only, **non-public** private LAN. What is
claimed, and what is not:

- **The profile is opt-in and default-off.** The NTCP2 surface's
  `qualification_profile` accepts `disabled`, `controlled_loopback`, and
  `private_lan_qualification`. An **enabled** NTCP2 surface must name one
  explicitly; there is no default that activates anything, and
  `default_ntcp2_enabled()` remains `false`.
- **The profile binds an operator-supplied RFC1918 IPv4 endpoint and nothing
  else.** It requires `enabled = true`; exactly one RFC1918 IPv4
  `bind_address` literal (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`); an
  explicit nonzero `listen_port` of at least 1024; a network id representable
  by the handshake width; and **family coherence with SSU2** — the `[ssu2]`
  surface must itself be in `private_lan_qualification` mode and bind an
  RFC1918 IPv4 address, because one RouterInfo carries both transports'
  endpoints.
- **Every unauthorized shape is refused by name.** Loopback, the unspecified
  address `0.0.0.0`, public IPv4, TEST-NET `192.0.2.0/24`, and every IPv6
  literal are refused at parse time by `ConfigError::Semantic`, each naming
  the offending field and stating the requirement. A public IPv4 literal and
  TEST-NET share the "requires an RFC1918 IPv4 address (10.0.0.0/8,
  172.16.0.0/12, or 192.168.0.0/16)" reason, because both fail the same
  `is_private()` test; the loopback, unspecified, and IPv6 shapes each carry
  their own reason.
- **The activation gate is a pure function with a named refusal for each
  shape.** `ntcp2_activation_scope(&Ntcp2Config)` returns either the address
  scope the transport may dial or one of five typed refusals — `Disabled`,
  `NoExplicitProfile`, `NonLoopbackBind`, `NonPrivateLanBind`,
  `PrivilegedPort` — each with its own reason text. It is pure on purpose:
  the configuration layer already refuses every shape the gate refuses, so an
  in-closure gate branch could never be reached from a configuration file.
  Extracting it makes every branch reachable by a unit test.
- **The gate is called from two places, so there is one authority for the
  decision.** The daemon composition root fails startup with the refusal's
  reason, and the `ntcp2-router` service closure applies the same gate as
  defense in depth. That service is registered with
  `StartupRequirement::Optional`, so a refusal there degrades that one
  service and the rest of the router keeps running. The gate never widens a
  scope.
- **The runtime's dial admission is scoped, and the scope is the caller's.**
  `Ntcp2DialScope::Loopback` admits loopback addresses only — exactly the
  historical clamp — and `Ntcp2DialScope::PrivateLan` admits RFC1918 IPv4
  literals only; loopback, the unspecified address, public IPv4, and every
  IPv6 form including ULA and IPv4-mapped literals are refused.
  `dial_authenticated_peer_in_scope` applies the scope as
  `if !scope.admits(endpoint.ip())`, and `dial_authenticated_loopback_peer`
  delegates to it with `Ntcp2DialScope::Loopback`, so its behaviour is
  unchanged. `DialOutcome::AddressOutOfScope` distinguishes "every advertised
  address was out of scope" from a transport failure. The scope is never
  widened inside the runtime.
- **The signed dual-transport record is held locally and is never published.**
  `generate_scoped_dual_transport_router_info` copies both transports'
  endpoints from the live owners' validated configuration and identity-bound
  stores and requires both to fall inside the scope; the controlled-loopback
  and private-LAN forms delegate to it. The operator hands the record to a lab
  peer **out of band**.
- **It advertises nothing.** The profile adds no publication path, and the
  record carries **no capability letter** — no `caps=f`. The NetDB publication
  path still requires an `Active` floodfill role **and** an in-memory
  `FloodfillAdvertisementPermit`, and neither is reachable from this profile.
  `[ssu2] advertise = true` remains refused at parse time.
- **It promotes no capability.** `specs/support.toml` is unchanged — 94
  surfaces, 88 experimental, 5 deferred, 1 ready, and **0 advertised** —
  `router.version` stays `0.9.62`, and the published RouterInfo capability set
  is unchanged.
- **It is not a public transport activation.** Public transport activation
  remains unclaimed: it requires external two-family public evidence, which
  needs an authorized two-host topology that this profile does not supply.
  What this profile delivers is the source-side activation substrate, not an
  activation.
- **The two families do not share one link registry.** SSU2 owns a private
  `i2pr_transport::TransportManager`; NTCP2 owns its own admission
  structures. What they share is one delivery registry
  (`Ntcp2RouterDeliveryRegistry`), one bandwidth governor, one authenticated
  I2NP dispatcher, and one local-RouterInfo channel.

Evidence: `crates/i2pr-daemon/src/ntcp2_activation.rs`,
`crates/i2pr-daemon/src/config.rs`,
`crates/i2pr-daemon/src/router_i2np.rs`,
`crates/i2pr-daemon/src/lib.rs`,
`crates/i2pr-runtime/src/ntcp2_runtime.rs`,
`crates/i2pr-runtime/src/ssu2_runtime.rs`,
`crates/i2pr-daemon/tests/ntcp2_private_lan_profile.rs` (including the
`#[ignore]`-gated row
`private_lan_profile_binds_the_operator_endpoint_and_advertises_nothing`),
and the operator procedure
[`docs/operations/ntcp2-private-lan-qualification.md`](../docs/operations/ntcp2-private-lan-qualification.md).
The plan of record and its current status are in
[`plans/registry.md`](../plans/registry.md).

## NetDB participation consumer phase (Plan 497)

Plan 488 built the peer-profile store and Plan 489 built the floodfill-indexed
routing table; both recorded that nothing in the daemon called them. Plan 497 is
the **consumer phase**, and it is **closed** as
`passed-netdb-participation-consumer-phase`: the source-side work is landed and
its evidence matrix is recorded in
[`plans/closure/network-participation/497-status.md`](../plans/closure/network-participation/497-status.md).
What is claimed, and what is not:

- **The claim is a consumer seam and a daemon-owned service — not a participation
  capability.** Plan 497 adds no network role. The router is still not found by a
  peer, still enters the network only through a reseed, and still carries no
  other router's traffic.
- **It is default-off, and the one new key is additive.** `netdb.routing_table_enabled`
  is `false` by default, so the daemon's behaviour is identical to today: the same
  peers are asked, in the same order, from the flat store. The single new key,
  `netdb.participation_maintenance_period_ms`, defaults to `60_000` ms, is
  bounded to `[1 000, 3_600_000]` ms, and is refused at parse time with a
  `ConfigError::Semantic` naming the field outside that range. No existing key
  changes meaning.
- **The flat store stays the authority; the table is a cache of derived state.**
  `begin_tunnel_lookup` computes the flat store's selection first and only then
  asks the routing table how it would order it. The table's answer may **reorder**
  that selection and nothing more: the candidate set is never narrowed, the
  lookup target stays excluded, and a peer the flat store would not have served is
  ignored. A table that cannot answer does not fail the lookup — it falls back to
  the flat store's selection exactly as today.
- **Every fallback is counted by reason.** The consumer answers with either the
  table's candidate list or a named fallback reason — `Disabled`, `NoCandidate`
  or `Unavailable` (a poisoned table) — and `RoutingTableCounters` counts
  `answered`, `fell_back_disabled`, `fell_back_no_candidate` and
  `fell_back_unavailable` separately. A table miss is a counted outcome, not a
  silent skip. The counters are held outside the table's lock on purpose: a
  poisoned table cannot answer, and that fallback must still be counted.
- **The preference is a priority prefix, not a closed list.** The engine's
  preferred-order entry point computes the same selection the ordinary entry point
  computes and only permutes it. On every later advance it re-selects against the
  authoritative store and puts the preferred peers first, so **a peer ingested
  while the lookup runs is still queried**. Plan 489's `start_with_candidates`
  keeps its closed-list contract unchanged, and a test pins the difference.
- **The two ordering metrics are different quantities.** The routing table orders
  a peer by its **daily keyspace position** against the routing key; the flat
  store orders by the peer's **canonical `RouterHash`** XOR distance against the
  same key. The table's answer is therefore a genuine reordering, not a
  restatement of the flat store's own order.
- **The coordinator is not composed into the daemon.** `NetDbTunnelCoordinator`
  is not created by the daemon today — every construction of it in the tree is in
  test code. Plan 497 delivers the **seam** (`NetDbTunnelCoordinator::with_routing_table`)
  that Plan 433's composition will use; Plan 433 owns the composition itself and
  is registered **blocked** on Plans 431/432. Until Plan 433 lands, the shared
  routing-table consumer is reachable only from the daemon crate.
- **The startup projection is loaded when present and current, and discarded and
  counted otherwise.** The bounded, sanitized, digest-framed peer-profile
  projection is read from `<data_dir>/peer-profiles/` at startup; a corrupt,
  oversized or stale projection is discarded with a count, never a startup
  failure. "Current" is carried by the **file name**
  (`profile-projection-<yyyyMMdd>.txt`), not by a format change — the projection
  type carries no UTC-day field and the projection **bytes** are unchanged from
  Plan 488's format. A projection from another UTC day is discarded, counted, and
  removed.
- **The daemon service is optional in both senses.** It is registered as
  `netdb-participation` with `ServiceClassification::Optional` **and**
  `StartupRequirement::Optional`, so a degraded participation owner degrades that
  one feature and does not gate router readiness. With the routing table off the
  service is inert: no population, no tick, no persistence.
- **The crate is no longer unpoliced by the repository boundary script.**
  `scripts/check-runtime-boundaries.sh` gains an `i2pr-netdb` section with 12
  source rules, a manifest rule set, one positive control per rule category,
  grouped-import controls, and a negative control over the crate's legitimate
  surface. The plan **raised** that script's floors — the positive-control floor
  from 14 to 30 and the raw-scan floor from 17 to 23 — rather than relaxing
  them.
- **Nothing is published and nothing is advertised.** `specs/support.toml` is
  unchanged — 94 surfaces, 88 experimental, 5 deferred, 1 ready, and **0
  advertised** — `router.version` stays `0.9.62`, and the published RouterInfo
  capability set is unchanged. The routing table is **internal state**.
- **The evidence is a private controlled mesh, not a public-network
  measurement.** The stock-peer NetDb exploration lane and the `caps=f`
  advertisement remain unlanded and stay gated on Plan 493's authorized two-host
  topology. No public-network claim is made here.

Evidence: `crates/i2pr-daemon/src/netdb_participation.rs`,
`crates/i2pr-daemon/src/netdb_tunnels.rs`,
`crates/i2pr-daemon/src/netdb_seam.rs`,
`crates/i2pr-daemon/src/lib.rs`,
`crates/i2pr-daemon/src/config.rs`,
`crates/i2pr-netdb/src/lookup_engine.rs`,
`crates/i2pr-netdb/src/bucket_set.rs`,
`crates/i2pr-netdb/src/lookup_policy.rs`,
`scripts/check-runtime-boundaries.sh`,
`crates/i2pr-daemon/tests/netdb_routing_consumer.rs`, and the preferred-order
rows in `crates/i2pr-netdb/src/lookup_engine.rs`
(`a_preferred_peer_is_asked_first`,
`a_preferred_peer_the_flat_store_would_not_serve_is_ignored`,
`the_preferred_path_never_asks_the_target`,
`the_preferred_path_still_reaches_a_peer_ingested_mid_lookup`). The plan of
record is in
[`plans/implementation/network-participation/497-netdb-participation-consumer-phase.md`](../plans/implementation/network-participation/497-netdb-participation-consumer-phase.md),
and its closure record, requirement-to-evidence matrix and unblock audit are in
[`plans/closure/network-participation/497-status.md`](../plans/closure/network-participation/497-status.md).
Plan 497 **discharged** Plan 488 F1, F3 and F4, and **partially discharged** Plan
489 §11 known limitation 2 ("the routing table is not wired into the router's
tunnel building"): the coordinator-level consumer exists and the discriminator
proves it, while the production composition of the coordinator into the
daemon's service graph remains Plan 433's blocked scope and stays open and
recorded there.

## SSU2 introducer role owner in the live inbound loop (Plan 498)

Plan 490 built the SSU2 introducer (Bob) role owner and the runtime's relay
handoff, and recorded that the daemon's ordinary inbound pump never drained
that handoff: a Relay Request a peer really sent was decoded, authenticated,
and then dropped. Plan 498 drains the handoff **on the daemon's live inbound
loop**, and it corrects Plan 490's prescription on the way. It is **closed** as
`passed-ssu2-introducer-role-owner-in-the-live-inbound-loop`: the source-side
work is landed and its evidence matrix is recorded in
[`plans/closure/network-participation/498-status.md`](../plans/closure/network-participation/498-status.md).
What is claimed, and what is not:

- **The claim is a drained handoff and a counted decision — not a working
  end-to-end role.** The role now reaches its owner and the owner's decisions
  are counted, but the produced Relay Intro / Relay Response is **not** yet
  carried to the wire.
- **Plan 490's prescription named the wrong place, and the correction is part
  of the delivery.** Plan 490's Medium #1 finding said to add the drain to
  `ServiceProduct::poll_inbound`. The daemon's own source says `poll_inbound`
  is **not on the live path** (`crates/i2pr-daemon/src/service_product.rs:3664-3665`);
  the live inbound pump is the daemon loop in `crates/i2pr-daemon/src/lib.rs`.
  A fix confined to `poll_inbound` would have closed the finding on paper
  while a live router still dropped every relay block. The drain is in the
  daemon loop.
- **Plan 490 Medium #1 is discharged; Medium #2 stays open.** The drain
  reaches the role owner, so Medium #1 — the handoff was never drained — is
  discharged. Medium #2 — **no live socket carriage of a Relay Request** — is
  **not** closed: the runtime still has no outbound relay queueing API, so a
  produced block is counted as `awaiting_carriage` and is not carried onto the
  wire. **Plan 499 discharges Medium #2** — see "SSU2 introducer live tag
  request and relay-block carriage (Plan 499)" below.
- **There is exactly one owner.** `NormalSsu2Owner` is either
  `Router(Ssu2DaemonHandle)` or `Groups(Box<ServiceProduct>)`; the live loop
  drains through whichever holds the handle. The product exposes the **pull**
  only and does not own a role owner, so there are not two sets of counters.
- **The drain is bounded and never fails.** One pass pulls at most
  `MAX_RELAY_DRAIN_PER_PASS` (16) events, which is the runtime relay
  channel's own capacity (`max_inbound_relay_queue`, default 16, ceiling
  `MAX_SSU2_INBOUND_QUEUE_CEILING` = 1024). A refusal is a counted outcome,
  not an error. `RelayDrainOutcome` carries `drained`, `admitted`, `refused`,
  and `awaiting_carriage`, and carries counts only — no peer hash, no key, no
  tag, no payload.
- **The resolver answers only from the validated store.** The production
  `IntroducerSignerResolver` is `NetDbSignerResolver`, which answers through
  `Bootstrap::validated_signing_key` — a narrow read-only accessor over the
  NetDB's validated `RouterInfoStore`. An unknown peer yields `None`, and the
  role owner refuses the block with `UnknownRequester` rather than guessing.
- **The runtime gained one accessor and nothing else.**
  `Ssu2ServiceHandle::try_next_relay_event` pulls from the **existing**
  bounded relay channel. There is no new channel, no new capacity, and no
  change to the runtime's authentication path. The runtime still has **no
  outbound relay queueing API**. **Plan 499 adds that API** — see below.
- **Nothing is published and nothing is advertised.** `specs/support.toml` is
  unchanged — 94 surfaces, 88 experimental, 5 deferred, 1 ready, and **0
  advertised** — `router.version` stays `0.9.62`, `caps` and the published
  RouterInfo are unchanged, and `ssu2.advertise` stays refused. The role
  remains behind the existing default-off `ssu2.introducer_service` opt-in.
- **The evidence is behavioural and static — not a live-network measurement.**
  The drain's behaviour is proved in-process and socket-free: a
  **discriminator** (a relay block reaches the role owner and the receipt
  moves 0→1), a **control** (an empty handoff delivers nothing and counts
  nothing), a **refusal row** (an unresolvable requester is refused and
  counted by reason, with no tracked state left behind), and a **bound row**.
  The **live wiring** is proved by a **static guard** that asserts the
  daemon's live loop calls the drain, that the live owner is the drain's
  event source, and that the live role owner answers requester keys from the
  validated store. The static guard does **not** prove that a live daemon
  executed the drain: the runtime's relay channel admits no external injection
  and the role requires the non-strict operator profile. There is **no**
  end-to-end relay-block injection on this host and **no** stock-peer or
  public-network evidence.

Evidence: `crates/i2pr-daemon/src/ssu2_relay_drain.rs`,
`crates/i2pr-daemon/src/ssu2_introducer.rs`,
`crates/i2pr-daemon/src/bootstrap.rs`, `crates/i2pr-daemon/src/lib.rs`,
`crates/i2pr-daemon/src/router_i2np.rs`,
`crates/i2pr-daemon/src/service_product.rs`,
`crates/i2pr-runtime/src/ssu2_runtime.rs`, and
`crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs`. The plan of record is
[`plans/implementation/network-participation/498-ssu2-introducer-role-owner-in-the-live-inbound-loop.md`](../plans/implementation/network-participation/498-ssu2-introducer-role-owner-in-the-live-inbound-loop.md);
its closure record, requirement-to-evidence matrix and unblock audit are in
[`plans/closure/network-participation/498-status.md`](../plans/closure/network-participation/498-status.md);
the predecessor closure record is
[`plans/closure/network-participation/490-status.md`](../plans/closure/network-participation/490-status.md).
Plan 498 **discharged** Plan 490 **Medium #1** (the handoff was never drained)
and **corrected** its prescription — `ServiceProduct::poll_inbound` is not on
the live path, so the live drain in `lib.rs` is the mandatory half. Plan 490
**Medium #2** (no live socket carriage of a Relay Request) **stays open** and
is recorded in the Plan 498 closure record: the runtime still has no outbound
relay queueing API, so a produced Relay Intro / Relay Response is counted as
`awaiting_carriage` rather than carried to the wire. **Plan 499 discharges
Medium #2** — the runtime gained a bounded outbound relay queueing API, the
daemon's drain gained a carriage seam, and the live three-party UDP mesh shows
the role completing on the wire.

## SSU2 introducer live tag request and relay-block carriage (Plan 499)

Plan 490 built the SSU2 introducer (Bob) role owner and the transport's relay
machinery; Plan 498 made the role owner reachable in a live daemon. Plan 499
makes the role **work**: the introducer can now be asked for a relay tag, and
a relay block it produces can be carried onto the wire. The plan of record is
[`plans/implementation/network-participation/499-ssu2-introducer-live-tag-request-and-relay-block-carriage.md`](../plans/implementation/network-participation/499-ssu2-introducer-live-tag-request-and-relay-block-carriage.md);
its closure record is
[`plans/closure/network-participation/499-status.md`](../plans/closure/network-participation/499-status.md);
the predecessor closure record is
[`plans/closure/network-participation/498-status.md`](../plans/closure/network-participation/498-status.md).
What is claimed, and what is not:

- **The claim is that the role completes on the wire between live routers, not
  that it is supported.** Three live routers over real loopback UDP exchange a
  relay tag, a Relay Request, a Relay Intro, a Relay Response and the
  verified response. The role remains experimental, default-off and
  non-advertised.
- **The wire format did not change.** The block vocabulary for `RelayTagRequest`
  (block 15) and `RelayTag` (block 16) already existed; only the session's
  outbound control queue lacked them. Plan 499 adds
  `Ssu2Session::queue_relay_tag_request()` and
  `Ssu2Session::queue_relay_tag(block)` and nothing else on the wire.
- **The relay tag names the TARGET, not the requester.** The transport's
  parameter is *named* `alice_hash`, but in the live target-shaped path it
  carries the **tag owner's** hash — the target's. The transport's own
  `issue_tag` doc comment says the parameter name is historical
  (`crates/i2pr-transport-ssu2/src/relay.rs:789-793`); the comment above the
  binding check (`crates/i2pr-transport-ssu2/src/relay.rs:877`) describes the
  parameter's **name**, not its
  binding, and is **misleading in the live path**.
- **The requester's own router hash is NOT in the request's signature
  preimage.** `relay_request_preimage(bob_hash, charlie_hash, ...)`
  (`crates/i2pr-transport-ssu2/src/relay.rs:144-166`) binds **Bob's** and
  **Charlie's** hashes and nothing else. The requester's identity comes from
  the session and only selects **which key verifies** the signature; a request
  is **not** cryptographically bound to the requester's hash. (The *Intro*
  block Bob produces **does** carry the requester's hash — that is Bob's
  output, not a field Alice signed.)
- **The response's nonce must equal the request's nonce, and the response's
  sender must be the target.** The transport correlates by nonce against the
  live request table and consumes the tracked request, so a replayed response
  is refused as an unknown correlation; the daemon's role owner makes the same
  two checks (`crates/i2pr-daemon/src/ssu2_introducer.rs:437`,
  `:439-440` — `UnknownCorrelation`, `SenderMismatch`).
- **The live tag request binds the authenticated session peer.** The runtime
  mints a nonzero tag from the OS CSPRNG and binds it to the handshake-
  authenticated session peer's router hash — never to a caller-supplied
  identity — through the transport introducer, whose quota is the bound:
  `MAX_RELAY_TAGS_GLOBAL` 16, `MAX_RELAY_TAGS_PER_PEER` 4,
  `RELAY_TAG_LIFETIME_SECS` 120. The tag is answered **on the same session**.
- **The carriage is a bounded queue on an existing session, not a new send
  path.** The runtime's four peer-facing methods resolve the destination peer
  to **exactly one** live session — ambiguity is refused, not guessed — and
  queue the block on that session's bounded control queue. The daemon's drain
  gained a `RelayCarriage` seam beside the existing `RelayEventSource` pull
  seam, with a production `RuntimeRelayCarriage` over the runtime that owns
  the sessions. `awaiting_carriage` becomes `carried` plus
  `carriage_refusals` by reason.
- **The RouterInfo publication step is NOT closed.** The transport's
  documentation says the target "asks the introducer for a tag **and
  publishes it**". Publishing the tag in the target's RouterInfo is a
  **NetDB/RouterInfo mechanism**, not an SSU2 relay-block mechanism. Plan 499
  delivers the ask and the answer; it publishes **nothing**.
- **The mesh exercises the role owner against a TEST signer resolver, not the
  production `NetDbSignerResolver`.** `NetDbSignerResolver` needs a
  `Bootstrap` holding validated RouterInfos and has its own unit rows; the
  live mesh uses a test resolver (`MeshSigners`,
  `crates/i2pr-daemon/tests/ssu2_introducer_live_mesh.rs:355`).
- **There is no stock-peer lane.** A lane against the pinned i2pd needs the
  Plan 493 topology, which does not exist yet. The mesh is three instances of
  this router.
- **`Ssu2RelayQueueError::SessionTerminated` is not reachable end to end.**
  The runtime removes a terminated session from both `state.active` and
  `state.peer_links` in the same locked operation, so the *observable* refusal
  after a termination is `NoSession`. The `SessionError` →
  `Ssu2RelayQueueError` mapping is asserted by a focused in-crate unit row
  instead.
- **`Ssu2RelayQueueError::AmbiguousSession` is not reachable end to end
  either.** The transport manager's duplicate policy resolves a **same-peer**
  duplicate as `RejectNew`, `ReplaceExisting` or `RetainExistingDrainNew`
  (`AcceptNew` is reserved for a *different* peer), so `peer_links[peer]`
  never holds two links. The ambiguous branch is asserted through a focused
  in-crate unit row of the resolution helper.
- **The daemon's anti-amplification check never fires on the daemon path.**
  The role owner passes the constant `MAX_INTRODUCER_REQUEST_BYTES` (128)
  rather than the real request length, so the transport's
  `request_bytes * 3 < 128` test is always satisfied. This is **pre-existing**
  behaviour, not added or fixed by Plan 499, and it is recorded here as a
  **known limitation**: the transport's amplification budget is not enforced by
  the daemon path.
- **The runtime still exposes a public `issue_relay_tag(tag, target_hash,
  now_secs)`** that binds a tag to a **caller-supplied** hash (pre-existing,
  Plan 490). Plan 499's live path binds the authenticated session peer and
  adds no such API, but the surface remains.
- **Nothing is promoted.** `specs/support.toml` stays at **94 surfaces, 88
  experimental, 5 deferred, 1 ready, and 0 advertised**; `router.version` stays
  `0.9.62`; `caps` and the published RouterInfo are unchanged; `ssu2.advertise`
  stays refused. The role stays behind the existing **default-off**
  `ssu2.introducer_service` opt-in.
- **The mesh needed no external environment** and is not environment-gated.

Evidence: `crates/i2pr-transport-ssu2/src/session.rs`,
`crates/i2pr-transport-ssu2/src/relay.rs`,
`crates/i2pr-transport-ssu2/tests/peer_relay.rs`,
`crates/i2pr-runtime/src/ssu2_runtime.rs`,
`crates/i2pr-runtime/tests/ssu2_local.rs`,
`crates/i2pr-daemon/src/ssu2_relay_drain.rs`,
`crates/i2pr-daemon/src/ssu2_introducer.rs`,
`crates/i2pr-daemon/src/lib.rs`,
`crates/i2pr-daemon/tests/ssu2_introducer_handoff.rs`, and
`crates/i2pr-daemon/tests/ssu2_introducer_live_mesh.rs`.

## The lookup engine's per-lookup peer budget (Plan 500)

Plan 497 recorded that `LookupPolicy::max_peers_per_lookup` was declared and
validated but read by nothing. Plan 500 makes it load-bearing. The plan of
record is
[`plans/implementation/network-participation/500-lookup-peer-budget-enforcement.md`](../plans/implementation/network-participation/500-lookup-peer-budget-enforcement.md);
its closure record is
[`plans/closure/network-participation/500-status.md`](../plans/closure/network-participation/500-status.md);
the predecessor closure record is
[`plans/closure/network-participation/497-status.md`](../plans/closure/network-participation/497-status.md).
What is claimed, and what is not:

- **The claim is that the budget is a real bound on the number of peers a
  lookup queries.** It is enforced in the engine's single attempt site, so
  it binds on `start`, `start_with_candidates`, `start_with_preferred` and
  the response-driven advance alike — not on one entry point.
- **The budget narrows attempts, never candidates.** The check sits before
  the peer is marked queried and before the attempt counter moves, so a
  candidate the budget prevented querying stays unqueried and the candidate
  list itself is untouched. Plan 489's pinned list and Plan 497's
  preference prefix are unchanged.
- **The two exhaustions are typed and counted separately.**
  `LookupFinalState::PeerExhausted` means the **candidate list** ran out;
  `LookupFinalState::PeerBudgetExhausted` means the **policy** ceiling was
  reached while candidates remained. The daemon's routing receipt counts
  them in separate buckets keyed by the state's stable label, and the
  receipt stays counts-only — no peer hash, no key, no payload.
- **The behaviour change is on the failure path, and it is stated here.**
  A lookup that succeeds within the budget is unaffected. A lookup that
  fails now stops at the budget (default 8 peers) instead of querying as
  many peers as the candidate list holds (up to
  `max_candidates_considered`, default 32) — so it does less work, finishes
  sooner, and **may fail where it previously succeeded**. That is the
  declared policy's intent: two distinct ceilings, the tighter one on peers
  actually queried.
- **`max_candidates_considered` is unchanged.** It remains the separate
  ceiling on the candidate set the selector produces, enforced in
  `select_floodfill_candidates`.
- **The policy's existing validations stay load-bearing.** A zero peer
  budget is still `LookupPolicyError::ZeroPeers`, and a peer budget above
  the candidate ceiling is still `PeerBudgetExceedsCandidates`.
- **The routing table's selection semantics are unchanged.** The table may
  **reorder** the flat store's selection and never narrow or widen it —
  Plan 497's invariant, unchanged here.
- **The participation driver is still not composed into the router's tunnel
  building.** That composition is Plan 433's scope and remains registered
  **blocked**; this plan changes the driver's *reason vocabulary*, not its
  wiring.
- **There is no stock-peer evidence** for the new exhaustion, and this is a
  lookup-engine bound, not a network capability.
- **Nothing is promoted.** `specs/support.toml` stays at **94 surfaces, 88
  experimental, 5 deferred, 1 ready, and 0 advertised**; `router.version`
  stays `0.9.62`; `caps` and the published RouterInfo are unchanged.

Evidence: `crates/i2pr-netdb/src/lookup_action.rs`,
`crates/i2pr-netdb/src/lookup_engine.rs`,
`crates/i2pr-netdb/tests/lookup_peer_budget.rs`,
`crates/i2pr-daemon/src/netdb_participation.rs`, and
`crates/i2pr-daemon/tests/netdb_peer_budget_receipt.rs`.

## The transit data-plane owner in the product composition (Plan 502)

Plan 492 built the transit participation posture and closed with a finding:
*"a router that has never been given a transit owner does not relay. Owner:
the plan that installs the transit owner in the product composition."* Plan
502 is that plan. The plan of record is
[`plans/implementation/network-participation/502-transit-owner-in-the-product-composition.md`](../plans/implementation/network-participation/502-transit-owner-in-the-product-composition.md);
its closure record is
[`plans/closure/network-participation/502-status.md`](../plans/closure/network-participation/502-status.md).

What is claimed, and what is not:

- **The owner is installed in the production composition, behind the
  operator's existing default-off `[transit] participation` opt-in.** The
  default profile constructs **nothing at all** — no service, no counters, no
  owner, and no entropy drawn — and still publishes
  `TransitParticipation::Disabled`. The three transit selectors therefore
  still report `0`, `0`, and `0.0` for a default router, **because the
  disabled posture owns no counters**, not because a zero was substituted for
  a missing measurement.
- **One pump, one decoder, one socket.** The owner is installed inside the
  SSU2 service that already owns the transport, the inbound pump and the
  `RouterDeliveryService`. There is no second socket, no second event loop,
  and no second I2NP decode: the raw inbound is handed to the owner, which
  performs the single canonical decode.
- **The disabled probe is the no-owner fallback only.** The production
  inbound path dispatches through the composition's entry point, which
  consults the installed owner first and falls back to the controlled
  disabled probe **only** when no owner exists, so the two can never run
  beside each other.
- **The peer index is reconciled against the live session set** and expired
  registrations are swept, on the daemon's existing pump tick — bounded,
  never per message, and never on a new task.
- **The router's own coordinator traffic is not claimed as transit.** The
  coordinator publishes the `(peer, message_id)` pairs of its own pending
  attempts, keyed by the attempt's target peer, and the owner bypasses them.
  The documented dispatch order — creator/service first, transit second,
  unknown fail-closed — is unchanged.
- **The boundary rule was strengthened, not satisfied.** The M11 rule 10 was
  two text searches that a composition could pass by naming a module and
  calling a no-op probe. It is now a function over the two production files
  with eight rules and nine controls, every rule with a control that fires
  it, and the counts derived. The control set includes the vestigial shape
  the old rule accepted; the pre-Plan-502 text search is run against the
  same fixture and is required to still accept it.
- **The build's delivery to a peer is still NOT proven.** The evidence shows
  the composition-installed owner **admits** a peer's transit work and
  attempts the forward; the attempt reports
  `RouterDeliveryOutcome::NoActiveSession`, because the private mesh has no
  established session to the responder. The share is charged and the bytes
  are accounted; **the cell does not reach a peer**. Plan 492's F1 is
  unchanged, and the evidence asserts the concrete delivery outcome rather
  than a `Forwarded(_)` wildcard or a byte count — the share is charged
  *before* the forward, so either of those would pass on a relay that
  reached nobody.
- **There is no stock-peer evidence**, no LOCAL OBEP consumer (a LOCAL
  action refuses with the typed `NoLocalConsumer`), and no NTCP2 ingress.
- **The transit role is still not advertised and still not claimed for the
  public network.** `specs/support.toml` stays at **94 surfaces, 88
  experimental, 5 deferred, 1 ready, and 0 advertised**; `router.version`
  stays `0.9.62`; `caps` and the published RouterInfo are unchanged. Plan 436
  stays **blocked** on Plan 433; this plan advances its source-side WP3 only.

Evidence: `crates/i2pr-daemon/src/transit_composition.rs`,
`crates/i2pr-daemon/src/transit_owner.rs`,
`crates/i2pr-daemon/src/transit_participation.rs`,
`crates/i2pr-daemon/src/lib.rs`,
`crates/i2pr-daemon/src/exploratory_build.rs`,
`crates/i2pr-daemon/src/i2pcontrol_inspection.rs`,
`crates/i2pr-daemon/tests/transit_composition.rs`, and
`scripts/check-m11-transit-boundaries.sh`.

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

## I2P-routed outproxy provider (Plan 343; request paths Plan 342; live resilience Plan 376)

Plan 327 was blocked on a missing outproxy. The provider exists in both halves —
a runtime-neutral policy layer and a daemon route owner — Plan 342 wired the
request paths and landed loopback wire evidence, and Plan 376 owns the remaining
live failover and post-restart rows. See
[`specs/references/proposal-170-outproxy-provider.md`](references/proposal-170-outproxy-provider.md).

What is claimed, and what is not:

- **Superseded in part: the option surface, the request paths, and a loopback
  wire lane all landed at Plan 342.** The sentence this bullet replaced said no
  Proposal 170 option sets an outproxy and no handler consults the provider.
  That is false as of Plan 342 (`passed-loopback-wire-lane-landed-live-failover-rotation-unproven`),
  which wired all canonical fields and all four request paths. What Plan 342
  proved is a **loopback wire lane**, and **loopback evidence is not live
  failover/restart evidence**: the lane exercises one configured outproxy
  fixture, so within-request failover across two configured outproxies, and a
  routed request carried across a real daemon restart, remain unproven and are
  owned by Plan 376. Plan 327 therefore stays blocked, but on the narrower
  residual Plan 342 recorded rather than on its original missing-provider
  diagnosis. No outproxy capability is advertised in `specs/support.toml`.
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
| SAM 3.3 local profile | client-facing loopback server | Plan 368 closed: pinned Java 2.13.0 child/traffic matrix and full local SAM profile passed | non-gating diagnostic only; PRIMARY unsupported at pin | not counted | optional | `plans/closure/sam/368-status.md`; `tests/integration/sam/run-java-368.sh`; no remote-router claim |
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
