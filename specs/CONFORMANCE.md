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
- **One half of this was not enforced in production until Plan 503.** The
  tunnel-build half ran on the live path from the start. The DSM half did
  not: the router's two live `DatabaseStore` sends selected floodfills by
  XOR distance alone and never consulted the gate, and the typed DSM
  refusal this section describes had no production caller because the
  coordinator that enforces it is never composed. **Plan 503** puts the
  floor on both live DSM paths and makes this section's receipt shape a
  live product — see "The live DSM version gate and the live peer-
  eligibility receipt (Plan 503)" below.

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
- **It promotes no capability.** `specs/support.toml` is unchanged — 97
  surfaces, 92 experimental, 5 deferred, 1 ready, and **0 advertised** —
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
  unchanged — 98 surfaces, 92 experimental, 5 deferred, 1 ready, and **0
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
  unchanged — 98 surfaces, 92 experimental, 5 deferred, 1 ready, and **0
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
- **Nothing is promoted.** `specs/support.toml` stays at **98 surfaces, 92
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
- **Nothing is promoted.** `specs/support.toml` stays at **98 surfaces, 92
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
  public network.** `specs/support.toml` stays at **98 surfaces, 92
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

## The live DSM version gate and the live peer-eligibility receipt (Plan 503)

Plan 491 gave the router two API-version obligations. Its **second** one —
*minimum floodfill peers will send DSM to* — was **enforced nowhere in
production**. Plan 503 puts it on the live paths and makes the decision
observable in the running router.

The gap, precisely:

- the router's **two live `DatabaseStore` sends** selected floodfills by XOR
  distance alone and never consulted `dsm_target_eligibility` — the creator
  RouterInfo publication and the service LeaseSet2 publication;
- the typed, counted DSM refusal Plan 491 built
  (`crates/i2pr-daemon/src/netdb_tunnels.rs:790` `begin_publication`) had
  **zero production callers**, because `NetDbTunnelCoordinator` is never
  composed — Plan 433 owns that composition and is registered `blocked`;
- so the router published its RouterInfo and its LeaseSet2 to floodfills that
  publish no API version, an uncomparable API version, or an API version
  below `0.9.62` — the exact under-compliance Plan 491 exists to prevent;
- and Plan 491's receipt shape was produced only by the participation driver's
  own selection methods, which have only test callers, so **no running router
  ever produced a receipt**.

What is claimed, and what is not:

- **The floor is now enforced on both live DSM paths**, through one shared,
  runtime-neutral, version-gated selection in `i2pr-netdb`
  (`lookup_policy::select_version_gated_floodfills`). The closed enforcement
  point (`PublicationCoordinator::nearest_eligible_floodfills`) and the live
  paths are now **one** implementation; its public behaviour is unchanged.
- **A refused floodfill is never a DSM target, and never a silent skip.** A
  floodfill that publishes no version, an uncomparable version, or a version
  below the floor is excluded from the target set and counted by reason
  (`unknown-version`, `uncomparable-version`, `below-floor`).
- **An empty eligible set fails closed** through the paths that already
  existed: the creator publication is skipped with a debug transition, and the
  LeaseSet2 publication releases its reserved intent and returns the existing
  typed request-construction failure. A refused floodfill is a **refusal**,
  not a delivery failure.
- **The candidates are considered within the policy's ceiling and the
  eligible ones are kept**, so a below-floor floodfill can no longer consume
  one of the three publication slots.
- **The receipt is a live product.** The live tunnel-build gate and the live
  DSM gate each record a receipt into a bounded live-owned log published
  through the daemon's inspection seam. The receipts carry **counts and
  stable reason labels only** — no peer hash, no address, no key, no version
  string, no payload. The log is bounded and evicts its oldest entry at its
  cap.
- **The identity `examined == accepted + refused` is exact for the DSM gate**,
  whose only two outcomes are eligible and refused-by-version-reason. It does
  **not** hold for the build gate, whose `examined` is the whole scanned
  population: the build projection's `unqualified` and `capped_participation`
  reasons are not version refusals, so they stay in
  `CandidateProjectionSummary` and are deliberately absent from the receipt.
- **The lookup-target selection is deliberately NOT gated.** A
  `DatabaseLookup` is not a DSM and is not one of Plan 491's two obligations.
  That gap is recorded as an open finding, not silently taken.
- **No advertisement and no capability change.** `router.version` stays
  `0.9.62`; `caps` and the published RouterInfo are unchanged; the support
  inventory's advertised count stays **zero**; `Cargo.toml` and `Cargo.lock`
  are byte-identical.
- **This is not a stock-peer qualification.** Plan 503 claims **no interop
  evidence**. Plan 491's F2 (no stock-peer evidence for the floors) needs the
  authorized two-host topology Plan 493 owns, and Plan 491's F4 (an
  intermittent `i2pcontrol_tunnels` staging failure) belongs to the
  I2PControl line. Neither is closed here.

Evidence: `crates/i2pr-netdb/src/lookup_policy.rs`,
`crates/i2pr-netdb/src/publication.rs`,
`crates/i2pr-netdb/src/version.rs`,
`crates/i2pr-daemon/src/version_eligibility.rs`,
`crates/i2pr-daemon/src/destination_tunnels.rs`,
`crates/i2pr-daemon/src/destination_peers.rs`,
`crates/i2pr-daemon/src/service_product.rs`,
`crates/i2pr-daemon/src/i2pcontrol_inspection.rs`,
`crates/i2pr-daemon/tests/version_eligibility_live.rs`, and
`crates/i2pr-daemon/tests/version_floor_boundary.rs`. The plan of record is
`plans/implementation/network-participation/503-live-dsm-version-gate-and-peer-eligibility-receipt.md`;
the closure record is `plans/closure/network-participation/503-status.md`.

## The tunnel liveness scheduler in the product composition (Plan 504)

Plan 185 delivered a tunnel-liveness scheduler for the creator side: pair
each active **outbound** exploratory tunnel with an active **inbound**
exploratory tunnel, probe through the outbound path, and correlate the
answer that comes back. It is complete, bounded and unit-tested — and it had
**never been constructed by the production daemon**. It was the only public
module in the daemon crate with zero production consumers, and the live
inbound path called the coordinator's dispatcher directly, so every inbound
`DeliveryStatus` echo was discarded.

Plan 185's closure record says the daemon pump already called that seam.
**That statement was false of the tree it described.** Plan 185's record is
not rewritten; the correction lives in Plan 504's closure record and in the
registry's row.

What is claimed, and what is not:

- **The scheduler is now constructed by the live product**, owned beside the
  coordinator it watches, driven on the **existing** 250 ms product tick,
  with a bounded number of drives per tick.
- **The correlation is authenticated.** The dispatcher now carries the
  acknowledged-message id, and the scheduler matches on it: a response
  credits exactly the probe that carried the id, and an id that matches
  nothing is counted as an **unauthenticated response** and credited to
  nothing. The previous "any `DeliveryStatus`, when exactly one test is
  pending" fallback is unchanged and still exported for the lanes that use
  it.
- **A pair becomes proven only when it has produced an authenticated
  response, and only a proven pair may ever be torn down.** Until the router
  has seen that peer answer, a missing response is indistinguishable from
  *this peer does not answer*, and the router must not act on a signal it
  cannot distinguish from its negation. The withheld teardown is counted
  once per pair and published, so the condition is visible rather than
  silent.
- **The probe reuses the canonical outbound path** — the same
  `compose_outbound_build_deliveries`, the same `RouterDeliveryService`
  delivery as the build reply. No private parallel pipeline.
- **The liveness probe ids live in the high half of the `u32` range**, so
  they cannot collide with the NetDB LS2 publication reply token that
  `DeliveryStatusMessage::message_id` already carries.
- **The exploratory/destination distinction is structural**: the pair
  listing reads only the exploratory pool's own registration lists, so a
  destination-role slot cannot appear in it.
- **The measured limitation.** The production daemon installs **no
  exploratory tunnels** — `ExploratoryBuildCoordinator::submit` is called
  only from the out-of-crate lanes, while the daemon submits only
  *destination* builds. The scheduler therefore tracks **zero pairs in
  production today**: it is **installed and inert**, not broken, and the
  proven-pair gate is what makes that safe. Wiring an exploratory build
  driver into the product was a separate plan, and **Plan 505 is that
  plan**: the daemon now names a peer, builds and installs a one-hop
  exploratory tunnel toward it, and records the outcome — so the
  scheduler has a population to watch.
- **No stock-peer evidence of the liveness response.** The response half
  had never been observed over a wire — with any peer — before Plan 504, and
  Plan 504's feasibility artefact is a **local** one. Whether an ordinary
  router relays a probe back to the named inbound gateway is **unproven**,
  and the proven-pair gate exists precisely because that question is open.
- **No advertisement and no capability change.** `router.version` stays
  `0.9.62`; `caps` and the published RouterInfo are unchanged; the support
  inventory's advertised count stays **zero**; `Cargo.toml` and
  `Cargo.lock` are byte-identical. No new task, no new timer, no new
  listener, no new socket.

Evidence: `crates/i2pr-daemon/src/tunnel_liveness.rs`,
`crates/i2pr-daemon/src/exploratory_build.rs`,
`crates/i2pr-daemon/src/router_i2np.rs`,
`crates/i2pr-daemon/src/service_product.rs`,
`crates/i2pr-daemon/src/i2pcontrol_inspection.rs`,
`crates/i2pr-daemon/tests/tunnel_liveness_boundary.rs`, and
`crates/i2pr-daemon/tests/tunnel_liveness_live.rs`. The plan of record is
`plans/implementation/network-participation/504-live-tunnel-liveness-in-the-product-composition.md`;
the closure record is `plans/closure/network-participation/504-status.md`.

## The exploratory build driver in the product composition (Plan 505)

Plan 504 installed the tunnel-liveness scheduler and left one thing open:
the **population** the scheduler watches did not exist.
`ExploratoryBuildCoordinator::submit` had **zero production callers** — every
call site was an out-of-crate integration lane — so the daemon installed no
exploratory tunnels, `exploratory_liveness_pairs()` returned empty, and the
scheduler tracked **zero pairs**. The production rebuild path was complete but
unreachable, because no pair existed to be removed.

The peer-profile store was in the same state: constructed in production and
**never fed**. `learn` had one caller and it was inside `#[cfg(test)]`, and
`select_tunnel_build_peers` — Plan 491's version-gated selection — had no
production caller at all, so the store was empty in a live router and nothing
wrote the four observation signals Plan 497 recorded as open.

The mechanism was complete; only its inputs were missing. `rebuild_exploratory`
is already a complete first-build driver with the peer-selection step removed,
and `install_established` already installs an exploratory build into the pool
and the registry. **One new thing was needed: a peer.**

What is claimed, and what is not:

- **The driver names a peer through Plan 491's version-gated selection**
  (`require_floodfill: false` — an exploratory tunnel is built toward an
  ordinary peer, not necessarily a floodfill). A below-floor, unknown-version
  or uncomparable-version peer is **refused and counted by reason**, never
  silently skipped.
- **The build goes through the existing `rebuild_exploratory`** — the same
  path the liveness rebuild uses — so there is exactly **one** code path that
  builds an exploratory tunnel, and the first build and the rebuild cannot
  drift apart.
- **The peer's static encryption key is the RouterInfo identity public key**,
  exactly as the production rebuild path uses it, and **not** the SSU2 address
  `s` option that Plan 185's external lane found to be the wrong key. The peer
  selection and the peer-material resolver read the **same** store, so they
  can never disagree about which peer exists.
- **The peer-profile store is now fed in production.** The daemon learns its
  real validated peers from the bootstrap snapshot — the same population the
  routing table is populated from — so the store is no longer empty, and the
  router has a live **writer** of the four observation signals. A learned peer
  does not reset its restored observation state.
- **The outcome map is total over all fourteen `BuildCoordinatorOutcome`
  variants, and it distinguishes a peer fault from a local condition.**
  `Installed` answers, `TimedOut` is unanswered, `HopRejected` is refused,
  `DeliveryFailed` is unreachable and `InvalidReply` is misbehaving — but
  `Cancelled`, `PoolRejected`, `InvalidRegistration`, `InvalidPath` and
  `CoordinatorRejected` are **this router's** condition and produce **no**
  peer-profile write. The router must never blame a peer for a fault of its
  own. The four destination-scoped variants are never attributed to the
  driver, which owns no destination attempt; an outcome with no in-flight
  attempt, a wrong direction or a mismatched attempt id is an **orphan**:
  counted and credited to nothing.
- **The posture is off by default.** A new `[exploratory]` configuration
  section carries one switch, `builds`, following the `[transit] participation`
  precedent, and it is `deny_unknown_fields`, so a key that looks like an
  advertisement is refused rather than ignored. There is deliberately **no**
  count knob: the number of exploratory pairs is bounded by the coordinator's
  own pool and by Plan 504's `MAX_LIVENESS_PAIRS`, and a configuration key that
  could disagree with those ceilings would be a second source of truth.
- **Off by default for a concrete reason, not out of caution.** The daemon's
  transport can only dial a peer it may legally reach — the SSU2 dial target is
  loopback-only and the private-LAN variant accepts only RFC1918 IPv4 — so a
  public deployment's peer population is **not dialable at all**. Enabling the
  posture is an explicit operator decision that this router should build
  exploratory tunnels inside the mesh it can actually reach.
- **No new task, no new timer, no new listener, no new socket.** The driver
  runs on the **existing** 250 ms group tick beside Plan 504's liveness
  driver, and the terminal outcomes reach it on both the expiry sweep and the
  inbound build-reply path.
- **The feasibility artefact is not vacuous, and that is measured.** It drives
  a controlled mesh of two real SSU2 daemons against the real coordinator,
  profile store and delivery seam. Swapping the responder's identity
  encryption key for the SSU2 static key — the shortcut the pre-existing
  `exploratory_build_live.rs` harness uses on both sides — makes the build
  fail to install. The artefact therefore proves the production path, not a
  test-only shortcut.
- **No stock-peer evidence is claimed.** Whether an ordinary public router
  accepts a driver-initiated one-hop exploratory build, and whether it relays
  a liveness probe back, is **unproven**. The external lanes stay open until
  Plan 493's authorized topology is available.
- **No advertisement and no capability change.** `router.version` stays
  `0.9.62`; `caps` and the published RouterInfo are unchanged; the support
  inventory's advertised count stays **zero**; `Cargo.toml` and `Cargo.lock` are
  byte-identical.

**What Plan 505 does not close.** The validated store is still not refreshed
from live NetDB lookups, so the driver's population is the bootstrap snapshot.
Plan 497 limitation 5's **read** half stays open: the driver becomes a live
reader of the profile store, but the routing table's only reader remains Plan
433's never-constructed `NetDbTunnelCoordinator`. Plan 487 F3's
destination-inbound half still needs tunnel rekeying, which the tree does not
implement at all.

Evidence: `crates/i2pr-daemon/src/exploratory_driver.rs`,
`crates/i2pr-daemon/src/exploratory_build.rs`,
`crates/i2pr-daemon/src/netdb_participation.rs`,
`crates/i2pr-daemon/src/service_product.rs`,
`crates/i2pr-daemon/src/lib.rs`, `crates/i2pr-daemon/src/config.rs`,
`crates/i2pr-daemon/src/i2pcontrol_inspection.rs`,
`crates/i2pr-daemon/tests/exploratory_driver_boundary.rs`, and
`crates/i2pr-daemon/tests/exploratory_driver_live.rs`. The plan of record is
`plans/implementation/network-participation/505-exploratory-build-driver-in-the-product-composition.md`;
the closure record is `plans/closure/network-participation/505-status.md`.

## The peer-knowledge reconcile (Plan 506)

Plan 505 closed the exploratory build driver and left one thing open: the
**population** the driver draws from. Plan 506 closes it.

The gap, precisely:

- the router accepts a peer's RouterInfo on a **live** session — three
  production sites ingest an authenticated unsolicited `DatabaseStore` into the
  bootstrap NetDB;
- but the two stores the router actually **builds with** — the destination
  coordinator's authoritative store and the peer-profile store — were seeded
  **once at startup** and never again. `insert_validated_router_info` had
  exactly one production caller, the startup seed, and `learn_bootstrap_peers`
  had exactly one production caller, also at startup;
- so a peer learned after startup was invisible to both, and the population
  the driver selects from never grew.

**The framing is narrower than it looks, and Plan 506 corrected it.** Plan
505 recorded that "the validated store is still not refreshed from live NetDB
lookups". That phrasing was wrong in a way that mattered: the router does
**not** learn peers from a NetDB lookup at all. A `DatabaseStore` carries
exactly one record, a `DatabaseSearchReply` carries peer **hashes only**, and
the production router runs **no** RouterInfo-kind lookup — the only
`begin_lookup` caller is Plan 433's never-constructed `NetDbTunnelCoordinator`.
The live-learned RouterInfos the router holds are exactly the unsolicited
`DatabaseStore`s. Plan 506's subject is therefore the **reconcile**, not a
lookup, and adding a RouterInfo-kind lookup would have been a
protocol-composition change with its own plan-of-record.

What is claimed, and what is not:

- **One bounded reconcile, owned by the product.** It snapshots the
  bootstrap's validated RouterInfos under the bootstrap's lock and **releases**
  that lock, inserts each record into the destination store through the
  canonical `insert_validated_router_info`, and learns the same records into
  the peer-profile store through the canonical `learn_all`. One snapshot, one
  cadence, no skew between the two stores.
- **The three locks are taken in one order and are never nested.** The
  bootstrap's lock is released before the destination coordinator's lock is
  taken, and the destination coordinator's lock is released before the
  peer-profile store's lock is taken. The peer-profile closure never awaits,
  so no lock is held across an await. The static guard asserts this
  **structurally**: each step's body may not name another step's lock.
- **Every outcome is counted by reason, and a refusal is never fatal.** A
  capacity refusal, a conflict and a stale replacement are counted and the
  batch continues — deliberately unlike the startup seed, which treats a
  capacity refusal as fatal. One bad record never aborts the reconcile.
- **The reconcile writes peer records, never a selection.** Plan 491's version
  gate stays at **selection**, so a refreshed peer below the floor is still
  refused there and counted. The reconcile does not weaken the gate; it feeds
  it.
- **The cadence is the existing NetDB participation maintenance period.** One
  source of truth, and **no second configuration knob**: `config.rs` is
  untouched and no new key is added. The reconcile's own bound is the same
  range the configuration parser already enforces, so the reconcile's refusal
  cannot fire on a configured router.
- **No new task, no new timer, no new listener, no new socket.** The reconcile
  is a step on the product's **existing** tick, beside the exploratory driver
  whose population it grows.
- **The counters are published through the inspection seam.** They carry
  counts and stable reason labels only — no peer, address, key or payload — and
  an unpublished or poisoned slot is a **gap**, never a fabricated zero.
- **A real defect was fixed, not just recorded.** The unsolicited store was
  ingested **twice** for every message the router did not serve as a
  floodfill, because the second site's condition was a strict **subset** of
  the first's. The two remaining ingest sites sit in **mutually exclusive**
  branches, so exactly one can run for any message. The static guard asserts
  that structurally: one ingest inside the `!ssu2_config.enabled` branch, one
  outside it, and no more.
- **The feasibility artefact is not vacuous, and that is measured.** It drives
  the **real** `Bootstrap`, the **real** `DestinationTunnelCoordinator` and the
  **real** `PeerParticipationDriver`, and the peer arrives through the
  production unsolicited-store path. Removing the `learn_all` call fails four
  tests, the first with "the peer must be selectable after the reconcile";
  removing the `insert_validated_router_info` call fails five, the first with
  "the peer must be insertable into the destination store by the reconcile";
  raising the below-floor peer's published version to the floor fails the
  refusal assertion.
- **The selection set is recorded as a finding, not left to be discovered.**
  The destination projection is a hash-ordered **256-prefix**, so adding peers
  does not merely add candidates — it can change **which** peers the build path
  sees. Changing the ordering is out of scope; recording the behaviour is not.
- **No stock-peer evidence is claimed.** Whether an ordinary public router
  sends unsolicited stores this router will accept is **unproven**. The
  external lanes stay open until Plan 493's authorized topology exists.
- **No advertisement and no capability change.** `router.version` stays
  `0.9.62`; `caps` and the published RouterInfo are unchanged; the support
  inventory's advertised count stays **zero**; `Cargo.toml` and `Cargo.lock` are
  byte-identical.

**What Plan 506 does not close.** A **RouterInfo-kind NetDB lookup** — the
router performs none in production, and adding one is a protocol-composition
change with its own plan-of-record. Plan 497 limitation 5's **read** half: the
routing table's only reader remains Plan 433's never-constructed
`NetDbTunnelCoordinator`. The destination store's **capacity policy** — it
refuses at capacity, and this plan adds no eviction. Plan 487 F3's
destination-inbound half still needs tunnel rekeying, which the tree does not
implement at all. Plan 491 F2 and F4, Plan 490 Medium #3, Plan 502 M1 and
Plan 503's lookup-target API-version gate all stay open.

Evidence: `crates/i2pr-daemon/src/peer_reconcile.rs`,
`crates/i2pr-daemon/src/service_product.rs`,
`crates/i2pr-daemon/src/lib.rs`, `crates/i2pr-daemon/src/bootstrap.rs`,
`crates/i2pr-daemon/src/destination_tunnels.rs`,
`crates/i2pr-daemon/src/netdb_participation.rs`,
`crates/i2pr-daemon/src/i2pcontrol_inspection.rs`,
`crates/i2pr-daemon/tests/peer_reconcile_boundary.rs`, and
`crates/i2pr-daemon/tests/peer_reconcile_live.rs`. The plan of record is
`plans/implementation/network-participation/506-peer-knowledge-reconcile.md`;
the closure record is `plans/closure/network-participation/506-status.md`.

## The transit creator-correlation retention window (Plan 508)

Plan 502 gave the transit owner the creator-correlation facts it needs to
keep the router's own coordinator traffic out of the transit path. It
recorded, as Finding M1, that those facts **never expire**, and it
deferred the fix as *a protocol decision*.

**The decision, made here on the record.** A creator-correlated fact is
installed while the coordinator's build is **pending**; the coordinator's
own memory of that build's identifiers starts when the build is
**retired**. The window is therefore the build deadline plus the
coordinator's own longest identifier retention —
`BUILD_DEADLINE_MS + BUILD_TUNNEL_ID_RETENTION_MS`, 46 minutes — a
function of constants the tree already enforces. There is **one** source of
truth and **no new configuration key**.

What is claimed, and what is not:

- **The facts are bounded over the router's lifetime.** The three sets are
  `BTreeMap`s from a fact to its expiry, not insert-only `BTreeSet`s, and
  the sweep drops the expired entries.
- **The expiry is checked at use time.** That is the **safe** direction: an
  expired fact is declined, and a live one never is. The router's own
  traffic is therefore never claimed as transit work, which is the
  invariant the facts exist for.
- **The sweep rides the pump tick that already installs the facts.** The
  owner gains no second timer and no second task.
- **Every comparison rounds in the safe direction.** Two of the four use
  sites carry seconds and two carry milliseconds; the window is stored in
  seconds with a ceiling at install time and a floor at comparison time,
  so a fact is never declined before it has expired.
- **The counts are the observability seam**, and they carry counts only.
- **The coordinator's own fact set is untouched.** It was already bounded
  and already expired; only the owner's **copy** was unbounded.
- **The controls are not vacuous, and that is measured.** Four mutations of
  the real tree, each applied, run against the real lanes, and reverted: a
  use site that stops consulting the expiry fails **six** tests, among them
  `an_expired_fact_no_longer_bypasses_transit`; a sweep that compares
  against nothing fails **eleven**, among them
  `an_expired_fact_is_declined`; a window that collapses to zero fails
  **eleven**, among them `the_window_must_be_the_trees_own_bound`; and an
  install that drops the expiry fails **eight**, among them
  `a_fact_inside_the_window_still_bypasses_transit`. The tree was restored
  byte-for-byte after each control.
- **No advertisement and no capability change.** `router.version` stays
  `0.9.62`; `caps` and the published RouterInfo are unchanged; the support
  inventory's advertised count stays **zero** (100 surfaces, 94
  experimental, 5 deferred, 1 ready); `Cargo.toml` and `Cargo.lock` are
  byte-identical.

**What Plan 508 does not close.** The **transit participation posture**,
the relay accounting and the OBEP path stay with Plan 492. Plan 487 F3's
**destination-inbound half** (which needs tunnel rekeying), Plan 497
limitation 5's **read** half, Plan 503's **lookup-target API-version
gate**, Plan 491 F2 and F4, Plan 490 Medium #3 and every externally-blocked
plan stay open. **No stock-peer evidence is claimed**: whether an ordinary
public router's own build replies still arrive inside a 46-minute window
is **unproven**, and the external lanes stay open until Plan 493's
authorized topology exists.

Evidence:
`crates/i2pr-daemon/src/transit_owner.rs`,
`crates/i2pr-daemon/src/exploratory_build.rs`,
`crates/i2pr-daemon/src/lib.rs`,
`crates/i2pr-daemon/tests/transit_creator_retention_feasibility.rs`,
`crates/i2pr-daemon/tests/transit_creator_retention_boundary.rs`, and
`crates/i2pr-daemon/tests/transit_creator_retention_live.rs`. The plan of
record is
`plans/implementation/network-participation/508-transit-creator-correlation-retention.md`;
the closure record is `plans/closure/network-participation/508-status.md`.

## The lookup-target API-version posture (Plan 509)

Plan 503 recorded as a Medium finding that *the router still asks a
below-floor floodfill for a lookup*, and left it open: a `DatabaseLookup` is
neither a tunnel build nor a DSM, so the API-version gate was never extended
to the lookup path, and extending the floor to the lookup path would change
which floodfills the router can ask at all — which is a product decision, and
therefore a plan-of-record's work.

**Plan 509 makes that decision: the lookup target is not version-gated.**

What is claimed, and what is not:

- **No floor is derivable, and a floor that cannot be derived may not be
  invented.** The specification's API-version table attaches exactly two
  duties to the `0.9.62` row — *minimum peers will build tunnels through, as
  of 0.9.68* and *minimum floodfill peers will send DSM to, as of 0.9.68* —
  and a `DatabaseLookup` is neither. `i2pr_proto::MinimumPeerObligation`
  carries only `constrains_tunnel_peers` and `constrains_dsm_targets`,
  `i2pr_proto::VersionFloors` carries only `min_peer_for_tunnel_build` and
  `min_floodfill_for_dsm`, and `i2pr_netdb::VersionGate` has exactly two
  variants. A lookup floor would have to be invented, and this repository's
  rule is that a floor is **derived**, never hand-maintained.
- **The two existing floors encode duties *on the peer*, and they do not
  transfer.** Each floor says that a peer **will not** do something: a
  below-floor peer will not build tunnels through the router, and will not
  send the router's DSM onward. The router's refusal is therefore exactly
  *do not ask a peer for something it will not do*. The specification imposes
  no such duty for a lookup, so there is no peer-side behaviour to refuse
  against.
- **The responder's version buys no safety.** The lookup's answer is a
  `DatabaseStore` the router **validates itself** before it trusts it. A
  below-floor peer that answers correctly is answered to; one that answers
  incorrectly is refused by the validation. The refusal is the router's own
  answer check, not a verdict on the peer.
- **The rotation absorbs an unhelpful candidate.** The state machine excludes
  already-queried peers and re-selects from the store, so a silent candidate
  costs one round trip and the lookup moves on. The rotation is bounded by the
  policy's candidate ceiling and the peer budget, and it terminates.
- **The two reference implementations disagree, so the router does not pick a
  side.** i2pd refuses to *file* a floodfill below `0.9.62`; Java's `0.9.62`
  is a **peer-exclusion** floor and its floodfill-profile predicate imposes no
  version floor at all. Neither gates a lookup by version.
- **The claim is proved, not asserted.** Five rows drive the real lease-set2
  lookup path through its public surface: a below-floor floodfill's correct
  answer completes the lookup; an incorrect answer is refused by the
  validation rather than by the version; a silent below-floor floodfill is
  skipped rather than re-asked, and the lookup completes; a peer that
  publishes no version at all is asked too. The inverted probe shows the
  claim is not vacuous — a version-gated selection **would have refused** the
  very peer the lookup asked.
- **The decision is structural, so it cannot drift.** A static guard asserts
  that the lookup path selects through the nearest-floodfill selection and
  takes no version decision at all; that the specification's obligation table
  still carries no lookup duty, the derived floors still carry no lookup
  floor, and the gate vocabulary still has exactly two variants; that the
  answer path still validates; and that the rotation still skips what it has
  already asked. Every rule carries its own inverted probe, and the guard is
  fail-closed.
- **No production behaviour changed.** The router asks the same floodfills,
  in the same order, with the same messages. Nothing here changes
  `router.version`, `caps`, the published RouterInfo, or any advertisement;
  the support inventory's advertised count stays **zero** (101 surfaces, 95
  experimental, 5 deferred, 1 ready); `Cargo.toml` and `Cargo.lock` are
  byte-identical.
- **No stock-peer evidence is claimed.** The decision is proved over the
  tree's own deterministic surfaces. Whether a real network's floodfills
  publish versions the router can compare is **unproven**, and the external
  lanes stay open until Plan 493's authorized topology exists.

**What Plan 509 does not close.** Plan 491 **F2** and **F4**, Plan 487 F3's
**destination-inbound half** (which needs tunnel rekeying), Plan 497
limitation 5's **read** half, Plan 490 Medium #3 and every externally-blocked
plan stay open.

Evidence:
`crates/i2pr-proto/src/common/version.rs`,
`crates/i2pr-netdb/src/selection.rs`,
`crates/i2pr-netdb/src/lookup_policy.rs`,
`crates/i2pr-netdb/src/lookup_engine.rs`,
`crates/i2pr-daemon/src/destination_tunnels.rs`,
`crates/i2pr-daemon/src/netdb_seam.rs`,
`crates/i2pr-netdb/tests/lookup_target_version_decision.rs` and
`crates/i2pr-daemon/tests/lookup_target_version_boundary.rs`. The plan of
record is
`plans/implementation/network-participation/509-lookup-target-api-version-decision.md`;
the closure record is `plans/closure/network-participation/509-status.md`.

## The managed-tunnel staging path's reason classification (Plan 510)

Plan 491 recorded F4: an intermittent `i2pcontrol_tunnels` managed-tunnel
staging failure — *"runtime transition failed: manager configuration
rejected"* — that did not reproduce in three isolated runs, in the
`--no-fail-fast` workspace sweep, or in two later floor runs. The residual
risk was named as *"a load-sensitive race in the managed-service staging
path"*, and the failure was undiagnosable: every manager rejection
collapsed into one static label, so an occurrence could not be attributed.

Plan 491's own words named two things, and the plan delivers both.

What is claimed, and what is not:

- **The rejection now names its class.** The daemon's `InvalidConfig`
  detail is classified into a **value-free class** through a pinned list of
  forty (marker, class) pairs. The marker is a stable phrase *inside* the
  detail — the service id is embedded at the front of most messages, so a
  prefix match would have to know the id, and a class that depends on the id
  is not a class — and the class is a short label of letters and spaces.
- **The change can never lose information.** The mapping is **total**: every
  detail yields a label. A detail that matches no marker keeps today's
  single generic label, so a rejection can never become *less* informative
  than it was before the change.
- **No value can reach the wire.** Both the classifier and the reason mapper
  return `&'static str`, so the **type system** — not a review — forbids a
  detail from reaching the operator. The service id, the socket address and
  the underlying error stay inside the daemon. The reason template is fixed:
  a class plus the prefix `runtime transition failed: `, nothing else.
- **The staging path now has a load-sensitive lane.** The path had none.
  `crates/i2pr-daemon/tests/i2pcontrol_tunnels_load.rs` drives **eight
  concurrent** managed-tunnel creates against one real daemon, each with its
  own name, its own loopback port and its own target destination, and each
  generating a destination identity, building a lease set and binding a
  listener. The load is **real work**, never a sleep; every wait is an
  explicit bounded deadline, and the concurrency is bounded.
- **The reject path is exercised, not assumed.** The same lane creates a
  server kind with `UseSSL` and no daemon TLS policy — a configuration the
  request envelope and the control spec build both admit, and which is
  refused later, while the shared manager stages the runtime. That is the
  exact path F4 failed on. The lane asserts that the reason names a class
  and carries no value, and that the shared manager still stages a runtime
  afterwards, so a rejection cannot quietly poison the path.
- **The decision is structural, so it cannot drift.** A static guard pins the
  table (non-empty, no duplicate marker or class, no shadowed marker), the
  classification's totality, the agreement between the pinned table and the
  production function, the value-freeness of every class, the **liveness** of
  every marker in the daemon's own error construction, the `&'static str`
  signatures, the fixed reason template, and the lane's boundedness,
  concurrency and freedom from sleeps. Every rule carries an inverted probe,
  and the guard is fail-closed.
- **No production behaviour beyond the reason string changed.** The staging
  path is not made faster, lazier or more tolerant; it is observed and
  reported. Nothing here changes `router.version`, `caps`, the published
  RouterInfo, or any advertisement; the support inventory's advertised
  count stays **zero** (102 surfaces, 96 experimental); `Cargo.toml` and
  `Cargo.lock` are byte-identical.
- **The original failure did NOT reproduce.** The lane is the plan's
  contribution whether or not it reproduces: if it reproduces, the plan has
  found the defect; if it does not, the path is under a load-sensitive lane
  that did not exist before, and the reason is diagnosable if it ever does.
  Plan 491's F4 is therefore closed **conditionally**, and the closure record
  names the condition and the exact future evidence that would close it
  unconditionally.
- **No stock-peer evidence is claimed.** The lane is over the tree's own
  deterministic surfaces.

**What Plan 510 does not close.** Plan 491 **F2** (no stock-peer evidence)
and **F5** (the comparator's four-component ceiling), Plan 487 **F3** (the
transit-hop failure detection), Plan 497 limitation 5's **read** half, Plan
490 Medium #3 and every externally-blocked plan stay open.

Evidence:
`crates/i2pr-daemon/src/i2pcontrol_tunnels.rs`,
`crates/i2pr-daemon/src/service_tunnels.rs`,
`crates/i2pr-daemon/tests/i2pcontrol_tunnels_load.rs` and
`crates/i2pr-daemon/tests/managed_tunnel_reason_boundary.rs`. The plan of
record is
`plans/implementation/core-router-recovery/510-managed-tunnel-staging-under-load.md`;
the closure record is `plans/closure/core-router-recovery/510-status.md`.

## The API-version comparator's component ceiling (Plan 511)

Plan 491 recorded **F5**: *"The comparator's four-component ceiling is an
unstated assumption in the specification's terms."* The specification's
API-version table contains a four-component row (`0.6.1.10`), but this tree
held only the table's **minimum-peer obligation** rows as data, so the row
the ceiling depended on lived in a prose comment. Plan 511 makes that row a
**held record** and the ceiling a **derivation** from it.

What is claimed, and what is not:

- **The ceiling has a source.** `i2pr_proto::SPECIFICATION_WIDEST_VERSION_ROW`
  holds the specification's four-component row as data, and
  `i2pr_proto::MAX_API_VERSION_COMPONENTS` is that row's **width** — through
  `version_row_width` — rather than a literal beside it. The stored array's
  width and the parser's bound are that same constant, so a version the
  parser accepts is stored in full and the ordering stays **exact**.
- **A wider row widens the comparator.** This is the point. If the
  specification's table gains a five-component row, the held record becomes
  that row, the ceiling follows it, and such a version becomes
  **comparable** — which is exactly what F5 says the hard-coded ceiling
  denies. The refusal is no longer a property of a literal.
- **No production behaviour changes.** The record is four components wide, so
  the constant is four, so the comparator accepts and refuses **exactly** the
  versions it accepts and refuses today. `ApiVersion` stays 10 bytes and
  `PublishedVersion` stays 12; the sizes were measured, not estimated. A
  widening to an input-derived 128-component bound would have cost 26x on
  both and about 1 MiB of profile-store memory, which is why the ceiling is
  derived rather than raised.
- **The references are recorded, for the first time here.** `i2pd 2.61.0`
  (pin `635b013a612ff47278ef02acf8580a28e10e26c5`) reads `router.version` by
  collapsing every digit into an `int` with **no component count**, so
  `"0.9.62.1.2"` becomes `96212` and is **above** the floor `962`.
  **Java I2P**'s `net.i2p.util.VersionComparator` walks both strings to
  exhaustion with **no cap**, parsing components as `long`s. So `i2pr` is the
  only one of the three that refuses such a version — a divergence that is
  now recorded with its evidence rather than left implicit. The **Java
  reading is from 2.13.1**, not the pinned **2.13.0**, and is labelled as
  such; the primary derivation is i2pd's, at the exact pin.
- **The divergence is safe.** The refusal is **typed**
  (`PublishedVersion::Uncomparable` → `VersionEligibility::UncomparableVersion`),
  **counted** (`VersionRefusalCounts::uncomparable_version`, published under
  the stable label `uncomparable-version`), **fail-closed**, and **monotone**:
  it can only exclude a peer both references would admit, never admit one
  below a floor.
- **The per-component width is recorded, not settled.** The router's `u16`
  bound against Java's `long` is a second divergence, deferred to a successor.
- **The decision is structural.** A guard pins the derivation, the held row,
  the stored width, the parser's bound, the ordering's exactness, the
  comparator's `const`, every floor's three-component width, the absence of a
  peer-to-peer ordering path, and the refusal's counting in the live path.
  Every rule carries an inverted probe.
- **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
  published RouterInfo are unchanged; the support inventory's advertised
  count stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical.
- **Plan 491's closure record is not rewritten.** The discharge lives here and
  in the registry row.

**What Plan 511 does not close.** Plan 491's **F2** (no stock-peer evidence)
is blocked externally on Plan 493's authorized two-host topology. The
per-component width divergence is recorded for a successor.

Evidence:
`crates/i2pr-proto/src/common/version.rs`,
`crates/i2pr-proto/tests/version_ceiling_derivation.rs` and
`crates/i2pr-daemon/tests/version_ceiling_boundary.rs`. The references'
evidence is recorded in `specs/QUALIFICATION-GATES.md` §10. The plan of record
is
`plans/implementation/core-router-recovery/511-version-component-ceiling.md`;
the closure record is `plans/closure/core-router-recovery/511-status.md`.


## The destination inbound pool's failure detection (Plan 512)

Plan 487 recorded, and left open, the finding that **the destination inbound
pool has no transit-hop failure detection**: a dead transit hop is invisible
to the router until the tunnel expires, so a destination whose tunnels die
with a hop is not replenished until the lease expires.

Plan 512 lands the mechanism.

**What was already there.** The router had Plan 504's **pair-level** liveness
scheduler: a probe leaves through one tunnel and must come back through the
other, the answer is correlated on the **acknowledged** message id, and a pair
that misses two consecutive responses is reported unhealthy. It was installed
and driven — but its **population** was the exploratory role's alone. The
consequences of a failure were already live too: failing a tunnel removes it
from the destination's pool, the daemon's tick notices that the installed
lease set no longer matches the pool's usable set and marks the destination
for republication, the replenishment deficit grows, and the existing submit
path builds the replacement. Only the **detection** and the **routing of the
failure** were missing.

**What Plan 512 adds.** Three things, and nothing else.

- **The population.** A destination role's own **outbound** role is paired with
  its own **inbound** role. The pairing is deliberately **within** one
  destination: pairing across destinations would let one destination's dead
  outbound fail another destination's inbound, which is a false failure the
  plan must not produce.
- **The composition.** The destination probe is addressed to the **inbound
  tunnel's gateway**, the way **i2pd 2.61.0** addresses it at the exact pin
  (`libi2pd/TunnelPool.cpp:452`), so the answer returns through the inbound
  tunnel the destination advertises and **no remote cooperation** is
  required. The destination role's routing comes from the shared data-plane
  registry's typed gateway route, because that role does not live in the
  exploratory pool. The exploratory composition keeps its same-peer
  precondition unchanged.
- **The failure arm.** An unhealthy destination pair fails the destination's
  inbound tunnel through the runtime's own `mark_tunnel_failed` seam, so the
  existing republication and replenishment paths are the ones that fire. i2pd's
  **"don't fail last tunnel"** rule is adopted verbatim, and the refusal is
  **counted** rather than silent.

**What is not claimed.** The failure is a **pair-level** fact: the probe did
not come back. The router does **not** attribute it to a hop, and **no peer
profile is written** — the decision type names a pool slot and carries no
peer identity. i2pd fails **both** sides of the pair; `i2pr` fails the
**inbound** only, and the divergence and its cost are recorded. No stock-peer
evidence is claimed, and no per-hop attribution is claimed.

**The safety property is inherited, not relaxed.** Only an **authenticated**
response proves a pair, and only a proven pair may be failed. A destination
probe path that is systematically broken therefore proves nothing and fails
nothing — which is what bounds the blast radius of the gateway-addressed
probe.

**The decision is structural.** A guard pins that the live tick registers the
destination population, that the population is grouped by destination, that
the tick routes the failure to the destination arm, that the arm reaches the
pool's failure seam, that the last-tunnel rule is enforced, that the probe is
composed through the canonical seam and addressed to the gateway route, that
the destination probe does not read the exploratory pool, that the
exploratory composition and population are unchanged, that the proven-pair
gate appears exactly once in the scheduler's production code, that the
binding table is bounded, that the failure decision carries no peer identity,
and that **Plan 504's guard and lane stay byte-identical** — their SHA-256
digests are pinned, so the claim is checkable rather than asserted. Every
rule carries an inverted probe.

**No advertisement.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical. **Plan 487's
closure record is not rewritten.**

**What Plan 512 does not close.** Plan 491's **F2** (no stock-peer evidence)
is blocked externally on Plan 493's authorized two-host topology, and the
same applies to Plan 487's F3: the mechanism is landed and proven in-tree,
but the stock-peer qualification that would exercise a real dead transit hop
is external. The **per-component width** divergence (the parser's `u16` bound
against Java's `long`) and the **routing table's live reader** (Plan 497
limitation 5's read half) are recorded for their own successors.

Evidence:
`crates/i2pr-daemon/src/service_product.rs`,
`crates/i2pr-daemon/src/exploratory_build.rs`,
`crates/i2pr-daemon/tests/destination_liveness_live.rs` and
`crates/i2pr-daemon/tests/destination_liveness_boundary.rs`. The references'
evidence is recorded in `specs/QUALIFICATION-GATES.md` §11. The plan of record
is
`plans/implementation/core-router-recovery/512-destination-inbound-liveness.md`;
the closure record is `plans/closure/core-router-recovery/512-status.md`.
## The live NetDB discovery spine (Plan 513)

Plan 497 recorded, and left open, the finding that **the shared routing
table has no production reader**: the daemon populates and persists the
table, and the coordinator asks it, but the coordinator was never in the
daemon's service graph. A table nothing reads is a table whose answers
cannot change the router's behaviour.

Plan 513 lands the spine.

**The order of the two halves is not arbitrary.** An exploration's answer
is peer **hashes**, and a hash the router cannot resolve is a name, not a
peer. So the **RouterInfo-kind resolution** is the enabling half, and the
exploration that produces names to resolve comes first in the cycle. A
router that could only explore would accumulate names it could never ask.

**The cycle is bounded and one step at a time.** A RouterInfo-kind lookup
that is in flight is awaited; an exploration that is in flight is awaited; a
named peer an earlier exploration found is resolved; otherwise a new
exploration starts. A request the wire never answered is **abandoned** when
its deadline passes and **counted** — it is never retried in a loop.

**Two deliveries, and neither substitutes for the other.**

- An **exploratory** request (wire lookup type `3`) is delivered
  **directly** to the floodfill being asked. This is the specification's
  addressing for an exploratory lookup, and it is the one place `i2pr`
  diverges from the reference: i2pd 2.61.0 sends the exploratory request
  through the exploratory tunnels and falls back to a **direct RouterInfo**
  lookup (wire type `2`) when it cannot. The divergence, its reason, and the
  reference's exact behaviour at the pin are recorded in
  `specs/QUALIFICATION-GATES.md` §12.
- A **RouterInfo-kind** request (wire lookup type `2`) rides the
  **outbound** exploratory tunnel and takes its reply through the
  **inbound** exploratory tunnel. Plan 102's closed authority amendment
  forbids substituting a direct peer link for the exploratory-tunnel path,
  so a router that holds no usable outbound exploratory role **refuses** the
  request rather than sending it directly. The refusal is counted.

**The routing table's answer is a preference, never a replacement.** It
reorders the flat store's own selection: a peer the flat store could serve
is never dropped, and a peer the flat store would not have served is
ignored. Every fallback — the table off, the table empty, the table
unreachable — is counted **by reason** on both the table's own counters and
the spine's.

**A suggestion is never evidence.** The peers an exploration names enter
the routing table as **suggestions**, and the table accepts a suggestion
only from a peer it already holds with signed evidence. A named peer is
then queued for resolution, and the RouterInfo-kind reply's signed record is
validated by the lookup engine's own rules and inserted into the router's
NetDB — so the named peer becomes a peer the router can ask. A reply from a
peer the exploration never asked introduces **nothing**: the reply's sender
is unauthenticated.

**The spine owns no task, no timer, no listener and no socket.** The
daemon's existing tick drives it: the tick advances the RouterInfo-kind
validation clock, refreshes the inbound exploratory tunnel's reply path,
and hands the composed request to the transport. The delivery outcome is
counted as delivered or failed — never silently.

**The version gate is inherited, and the reason is stated.** Plan 491's
DSM version gate is scoped by its own definition — `dsm_target_eligibility`
— to the peers the router sends a **DSM** to: the two **publication**
paths. Plan 513 adds no DSM and changes neither publication path, and
neither publication path consults the routing table, so no gated candidate
can be bypassed by the table's preference. The lookup paths — the live
LeaseSet2 lookup and this plan's new RouterInfo-kind lookup — were never
version-gated, and this plan does not change that. The gate's scope is
recorded rather than assumed.

**No advertisement.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical. The
`netdb.live-routerinfo-lookup` surface moves from `deferred` to
`experimental` because the lane has proved it; it stays
`advertised = false`. **Plan 497's closure record is not rewritten.**

**What Plan 513 does not close.** The **stock-peer** half — a run against a
pinned stock router that answers a real exploration and a real RouterInfo
lookup — is blocked externally on Plan 493's authorized two-host topology.
Plan 497's **limitation 5**'s read half is discharged here; its stock-peer
half is not. The reference's **direct** fallback (a type-2 lookup when the
exploratory tunnels are unavailable) is **not** implemented, and the
difference is recorded.

Evidence:
`crates/i2pr-daemon/src/netdb_participation.rs`,
`crates/i2pr-daemon/src/netdb_seam.rs`,
`crates/i2pr-daemon/src/netdb_tunnels.rs`,
`crates/i2pr-daemon/src/service_product.rs`,
`crates/i2pr-daemon/tests/netdb_discovery_live.rs` and
`crates/i2pr-daemon/tests/netdb_discovery_boundary.rs`. The references'
evidence is recorded in `specs/QUALIFICATION-GATES.md` §12. The plan of
record is
`plans/implementation/network-participation/513-live-netdb-discovery-spine.md`;
the closure record is `plans/closure/network-participation/513-status.md`.

## The exploratory request's target (Plan 514)

Plan 513 landed the spine, but the request it sent was **aimed wrongly**, and
the operator could not switch it off.

**The aim was wrong.** The live path picked the **lowest-hash** floodfill
advertiser in the flat store and addressed the request to it. The store is a
`BTreeMap` keyed by the peer's hash, so its first advertiser is the
lowest-hash one, and the choice did not depend on the search key at all:
**every** exploration asked the **same** peer, until a smaller-hash floodfill
entered the store. A peer that did not answer was asked again and again. The
routing table's closest-first selection — the mechanism Plan 489 built and
Plan 513 gave a reader — was **not consulted** on that path at all, and the
table's own exploration entry point had no production caller.

The reference asks the same question of its own floodfill bucket set: i2pd
2.61.0 derives the random search key's routing key and runs the bucket set's
closest-first search for it (`libi2pd/NetDbRequests.cpp:479`, at the exact
pin; `libi2pd/NetDb.cpp:1312-1316`). The reference's exact behaviour, and the
divergence, are recorded in `specs/QUALIFICATION-GATES.md` §13.

**The switch was inert.** `netdb.exploration_enabled` was parsed, defaulted to
`false`, and cross-validated against `netdb.routing_table_enabled` — and **no
production code read it**. The exploration ran whenever the routing table was
on. The key's own documentation had already stated the contract: *"whether the
router runs exploratory lookups"*, and *"Exploration is **off by default**, and
it cannot be enabled while the routing table is off"*. The wiring was missing,
not the design.

**What changed.** The peer the exploratory request is addressed to is the
routing table's **closest** verified floodfill for the search key's daily
routing key. The router already computed that routing key for the request, so
nothing new had to be derived — the fix was to *use* it.

**A request the table cannot target is refused, not redirected.** A table that
names no floodfill — off, empty, or unreachable — ends the request, counted
under its own reason, and the request is never redirected to an arbitrary
substitute. The reference ends its request too, rather than asking one
(`libi2pd/NetDbRequests.cpp:502-503`).

**The switch is honoured, and its default does not move.** The switch gates the
**exploratory request** only. The RouterInfo-kind resolution stays above it,
so the routing table keeps its production reader and Plan 497's **M2** is not
reopened. The posture is:

| `routing_table_enabled` | `exploration_enabled` | What runs |
| --- | --- | --- |
| `false` (the default) | anything | no table, no participation owner, no spine, no exploration |
| `true` | `false` (the default) | the table is maintained and persisted; **no** exploration |
| `true` | `true` | the full discovery spine |

The second row is the one behaviour that changed, and it **is** the defect:
the exploration ran there anyway, against the key's documented contract.

**The lookup target stays unversion-gated.** Plan 509 decided that the lookup
target is not version-gated, and made the decision structural by naming the
selection sites. The two live lookup paths Plan 513 added — the RouterInfo-kind
resolution and the exploratory request — were **not** named, so the decision
was unenforced on exactly the paths added since it was recorded. All of them
are named now, and this plan's own selection is named too.

**No advertisement.** `router.version` stays `0.9.62`; `caps` and the published
RouterInfo are unchanged; the support inventory's advertised count stays
**zero**; `Cargo.toml` and `Cargo.lock` are byte-identical. The inventory holds
**106 surfaces, 101 experimental, 4 deferred, 1 ready, 0 advertised**. **Plan
497's closure record is not rewritten**, and Plan 513's closure record is not
rewritten either: the correction to Plan 513's claim lives here and in Plan
513's registry row.

**Plan 515 adds one surface** — `daemon.router-info-publication` — so the
inventory holds **107 surfaces, 102 experimental, 4 deferred, 1 ready, 0
advertised**. The advertised count stays **zero**: Plan 515 refreshes the
router's own record, and advertises nothing new.

**Plan 516 adds one surface** — `daemon.router-info-publication-confirmation`
— so the inventory holds **108 surfaces, 103 experimental, 4 deferred, 1
ready, 0 advertised**. The advertised count stays **zero**: Plan 516 makes
the publication *confirmable*, and advertises nothing new.

**Plan 517 adds one surface** — `daemon.router-info-publication-retry` —
so the inventory holds **109 surfaces, 104 experimental, 4 deferred, 1 ready,
0 advertised**. The advertised count stays **zero**: Plan 517 makes the
publication *retry*, and advertises nothing new.

**Plan 518 adds one surface** — `daemon.router-info-publication-freshness` —
so the inventory holds **110 surfaces, 105 experimental, 4 deferred, 1 ready,
0 advertised**. The advertised count stays **zero**: Plan 518 makes the
published record *current*, and advertises nothing new.

**Plan 519 adds one surface** — `daemon.router-info-publication-off-loop` —
so the inventory holds **111 surfaces, 106 experimental, 4 deferred, 1 ready,
0 advertised**. The advertised count stays **zero**: Plan 519 moves the
publication onto its own task, and advertises nothing new.

**Plan 520 adds one surface** — `daemon.own-ssu2-address-publication` —
so the inventory holds **112 surfaces, 107 experimental, 4 deferred, 1 ready,
0 advertised**. The advertised count stays **zero**: Plan 520 qualifies the
address the router already published, and advertises nothing new.

**Plan 521 adds one surface** — `daemon.own-peer-test` — so the inventory
holds **113 surfaces, 108 experimental, 4 deferred, 1 ready, 0 advertised**.
The advertised count stays **zero**: Plan 521 gives the router a way to earn
the reachability evidence its published address needs, and advertises
nothing new.

**Plan 522 adds two surfaces** — `ssu2.outbound-session-keepalive` and
`daemon.outbound-peer-session-pool` — so the inventory holds **115 surfaces,
110 experimental, 4 deferred, 1 ready, 0 advertised**. The advertised count
stays **zero**: Plan 522 holds sessions the router already wanted to hold,
and advertises nothing new.

**Plan 523 adds one surface** — `ssu2.introducer-address-form` — so the
inventory holds **116 surfaces, 111 experimental, 4 deferred, 1 ready, 0
advertised**. The advertised count stays **zero**: Plan 523 corrects the
*form* of the SSU2 introducer set and publishes nothing.

**Plan 524 adds one surface** — `daemon.introducer-request` — so the
inventory holds **117 surfaces, 112 experimental, 4 deferred, 1 ready, 0
advertised**. The advertised count stays **zero**: Plan 524 makes the router
*acquire* an introducer, and publishes nothing.

**Plan 525 adds one surface** — `daemon.introducer-publication` — so the
inventory holds **118 surfaces, 113 experimental, 4 deferred, 1 ready, 0
advertised**. The advertised count stays **zero**: Plan 525 makes the router
*publish* the introducer set it learned, in the specification's
introducer-only address form, and it still advertises **no** introducer
capability of its own.

**Plan 526 adds one surface** — `daemon.introducer-advertisement` — so the
inventory holds **119 surfaces, 114 experimental, 4 deferred, 1 ready, 0
advertised**. The advertised count stays **zero**: Plan 526 makes the router
*advertise its own introducer role*, in the `caps` option of its own address
and **only** while the role is live, and the surface is internal and
non-advertised.

**Plan 527 adds one surface** — `daemon.reachability-contradiction` — so the
inventory holds **120 surfaces, 115 experimental, 4 deferred, 1 ready, 0
advertised**. The advertised count stays **zero**: Plan 527 gives the
router's reachability **one owner and one reader**, and the surface is
internal and non-advertised.

**Plan 528 adds one surface** — `daemon.peer-test-verdict` — so the
inventory holds **121 surfaces, 116 experimental, 4 deferred, 1 ready, 0
advertised**. The advertised count stays **zero**: Plan 528 stops the router
from throwing its peer-test verdict away, and the surface is internal and
non-advertised.

**Plan 529 adds one surface** — `daemon.observed-address` — so the
inventory holds **122 surfaces, 117 experimental, 4 deferred, 1 ready, 0
advertised**. The advertised count stays **zero**: Plan 529 makes the router
learn the address at which a peer sees it, and the surface is internal and
non-advertised.

**Plan 530 adds one surface** — `daemon.explicit-bind-corroboration` — so the
inventory holds **123 surfaces, 118 experimental, 4 deferred, 1 ready, 0
advertised**. The advertised count stays **zero**: Plan 530 makes the router
record its own bound address as reachability evidence, and the surface is
internal and non-advertised.

**Plan 531 adds one surface** — `daemon.relay-requester` — so the inventory
holds **124 surfaces, 119 experimental, 4 deferred, 1 ready, 0 advertised**. The
advertised count stays **zero**: Plan 531 makes the router able to reach a peer
through that peer's introducer, and the surface is internal and non-advertised.

**Plan 532 adds one surface** — `daemon.relay-target` — so the inventory holds
**125 surfaces, 120 experimental, 4 deferred, 1 ready, 0 advertised**. The
advertised count stays **zero**: Plan 532 makes the router able to answer a
relay introduction when it is the peer an introducer named, and the surface is
internal and non-advertised.

**Plan 533 adds one surface** — `daemon.relay-establishment` — so the inventory
holds **126 surfaces, 121 experimental, 4 deferred, 1 ready, 0 advertised**. The
advertised count stays **zero**: Plan 533 makes the router able to establish a
session with the peer it reached through a relay, and the surface is internal
and non-advertised.

**Plan 534 adds one surface** — `daemon.relay-session-peer` — so the inventory
holds **127 surfaces, 122 experimental, 4 deferred, 1 ready, 0 advertised**. The
advertised count stays **zero**: Plan 534 makes the router build a tunnel toward
the peer it reached through a relay, and the surface is internal and
non-advertised.

**What Plan 514 does not close.** The **stock-peer** half remains blocked
externally on Plan 493's authorized two-host topology. The **rate** is
still below the reference's: the reference asks `1..9` exploratory requests
per `27..55` seconds, and `i2pr` asks **one** per participation tick — `60`
seconds by default. The structure matches — one exploratory request, one
random key, one closest floodfill, and the reference's exploratory request
does not advance to the next closest either
(`libi2pd/NetDbRequests.cpp:261`) — but the rate does not, and it is
recorded as a finding for a successor.

Evidence: `crates/i2pr-daemon/src/netdb_participation.rs`,
`crates/i2pr-daemon/src/lib.rs`, `crates/i2pr-daemon/src/config.rs`,
`crates/i2pr-daemon/tests/netdb_exploration_target.rs` and
`crates/i2pr-daemon/tests/netdb_exploration_target_boundary.rs`. The
references' evidence is recorded in `specs/QUALIFICATION-GATES.md` §13. The
plan of record is
`plans/implementation/network-participation/514-live-exploratory-request.md`;
the closure record is `plans/closure/network-participation/514-status.md`.

## The router learns that the network holds its record (Plan 516)

Plan 515 made the router republish its own RouterInfo on a period. That
made the publication **repeatable**; it did not make it **observable**. The
router learned that the transport accepted the store, and nothing more.

The correlator already minted a reply token for every attempt and
documented that the peer would correlate the acknowledgement with it, but
the store message carried `reply_token: 0` — a token that asks for no
acknowledgement. So the correlator's correlation half was unreachable, and
the `acknowledged` counter was structurally always zero.

Plan 516 closes that join. The store message now carries the correlator's
minted token in the reference's **direct-reply** form, the inbound owner
routes the floodfill's answer to the correlator, and the attempt moves to
`Acknowledged`. The **delivery** admission and the **network
acknowledgement** are counted separately, because they are different facts.

**Proved here:** the token reaches the wire and is the correlator's own; the
reply form is the reference's direct reply; the store's key, gateway and
payload are one record; the daemon composes no store message of its own;
the one-shot and the refresh share the correlator; the inbound owner joins
the publication; an unmatched acknowledgement is refused and counted.

**Not proved here, and not claimed:** that a **stock** floodfill answers.
That half needs Plan 493's authorized two-host topology, and the
stock-peer evidence remains blocked on it. Nothing is advertised: the
inventory's advertised count stays zero, `router.version` stays `0.9.62`,
and the published RouterInfo is unchanged.

## The router's own record carries a fresh date (Plan 518)

Plan 515 made the publication periodic. Plan 516 made it confirmable. Plan 517
made it retry. All three re-send the **same bytes** — and those bytes carry the
publication date the router signed at **startup**.

So the record ages. The netdb's acceptance window is `DEFAULT_MAX_AGE`, 24 hours
(`crates/i2pr-netdb/src/router_info.rs:20`), and the reference's own netdb
expires routers between one and a half and twenty-seven hours
(`libi2pd/NetDb.hpp:49-51`). After a day of uptime the router keeps
republishing a record the network will not accept, every 39 minutes, with no
counter and no diagnostic. The publication is not slow: it is **dead**.

The reference avoids this by re-dating and re-signing its record before each
publish (`libi2pd/RouterInfo.cpp:1218`, `libi2pd/RouterContext.cpp:1474`,
`libi2pd/RouterContext.h:33`).

**The record is now re-dated and re-signed, once per cycle.** The composition
root re-dates the record at the cycle's start, installs the fresh record — so
the RouterInfo the router hands out in `SessionConfirmed` carries the fresh date
too — and publishes it. Only the date and the signature change: the signed
region is re-encoded from the record's own decoded parts, so the addresses, the
peers and the options are preserved, and the router's **hash** does not change,
because a RouterInfo's key is derived from the router identity rather than from
the signed bytes.

**Proved here:** a record is publishable when it is fresh and refused when it is
stale, and a record dated too far in the future is refused too; the netdb's own
validator refuses the stale record and accepts the re-dated one at the same
reading; the re-dated record keeps the content and the router hash, and keeps a
peer; the re-dated record's signature verifies and the original's does not cover
the new date; a record that cannot be re-dated is refused; the staleness refusal
is counted under its own reason; the re-dating is deterministic; the
re-dating happens once per cycle, at the cycle's start, and not per rung; and
every rung of a cycle publishes the cycle's **one** record, because the
record lives in the owner rather than being read back out of the runtime.

**Not proved here, and not claimed:** that a **stock** floodfill accepts the
re-dated record in the live network. That half needs Plan 493's authorized
two-host topology, and the stock-peer evidence remains blocked on it. Nothing
is advertised: the inventory's advertised count stays zero, `router.version`
stays `0.9.62`, and the published RouterInfo's content is unchanged.

## The publication leaves the transport's loop (Plan 519)

Plan 515 made the publication periodic. Plan 516 made it confirmable. Plan 517
made it retry. Plan 518 made the record current. All four ran the publication
**inside** the SSU2 service's `select!` loop.

That loop's publication arm awaited a session establishment. While that await
was pending the loop could not run its other arms: the relay drain and the
inbound acknowledgement owner both waited. The transport's **socket** I/O was
not the victim — `i2pr-runtime` owns the sockets, the timers and the handshake
machinery, so a handshake completed while the daemon's service loop was
stalled. What was lost was the **service layer's** other work. This was the
**Medium** finding `M1` that Plan 517's and Plan 518's closure records both
carry open.

Neither pinned reference blocks its transport's loop on a handshake. i2pd
2.61.0's `Transports::SendMessage` posts the send and the publisher **ignores**
the returned future (`libi2pd/Transports.cpp:483-496`). Java I2P 2.13.0 runs
the publication as a **job on its own pool** (`PublishLocalRouterInfoJob`,
`JobQueue.java:163`), hands the store to a `FloodfillStoreJob` with its own
`PUBLISH_TIMEOUT = 90 s` (`FloodfillNetworkDatabaseFacade.java:174`,
`:246-257`), and delivers it through `commSystem().processMessage(m)`, so the
transport owns the dial (`StoreJob.java:415-441`). The I2P documentation states
the same rule in prose: a router publishes its RouterInfo by directly
connecting to a floodfill router, and only in some circumstances out an
exploratory tunnel.

**The publication now runs on its own task.** The owner — the schedule, the
cycle, the correlator and the cycle's record — the transport capability, the
identity and the acknowledgement inbox all move into it. The composition root
spawns it under the service's own child scope with an explicit cancellation;
there is no ownerless task and no detached one. The transport's service loop
holds **none** of the publication's state and never awaits it.

The inbound `DeliveryStatus` still crosses the boundary, and it crosses it
over a **bounded** channel: the loop's hand-over never waits for room, and a
full channel counts the acknowledgement as **unmatched** — the refusal Plan
516 already defines — rather than dropping it silently.

The publication owns a **bounded per-attempt delivery timeout** of 90 seconds,
in the shape of the reference's `PUBLISH_TIMEOUT`. A delivery that did not
finish inside it is refused under **its own reason** and counted, so a hung
handshake holds neither the transport's loop nor the publication's task.

The selection **prefers a peer the router is already connected to**, because
that is the peer whose send needs no handshake — i2pd's `IsConnected` and
Java's `isEstablished` both do. The preference is applied **on top of** the
version-gated closeness selection, and the targets it skips for want of one
are counted under their own reason. Exploratory tunnels stay out of scope:
i2pr installs none in production, so the direct route is the only one.

**Proved here:** the transport's service loop never awaits the publication and
never names its work; the publication's state is not owned by the loop, and the
owner is constructed once and handed to the task; the task is spawned with
explicit ownership and cancellation and stops when its cancellation fires; the
acknowledgement handoff is bounded, never waits for room, and counts a refused
acknowledgement as unmatched; the selection prefers an established session and
counts the targets it skipped; the delivery timeout is bounded and owned by the
publication, and a delivery that timed out is refused under its own reason; the
transport's loop carries no publication timer; and the ladder's timing, the
correlator's matching and Plan 518's staleness rule are unchanged.

**Not proved here, and not claimed:** that a **stock** floodfill accepts the
router's record in the live network — that half needs Plan 493's authorized
two-host topology and remains blocked on it. The **live-dial** half is also not
re-measured here: the lane drives the publication's public behaviour without a
socket, so what stands between the publication and the transport's loop is the
structural gate, which reads the production files. Nothing is advertised: the
inventory's advertised count stays zero, `router.version` stays `0.9.62`, and
the published RouterInfo's content is unchanged.

## The router retries a publication the network did not acknowledge (Plan 517)

Plan 515 made the publication repeatable. Plan 516 made it confirmable. Neither
made it **retry**.

A publication the network never acknowledged was simply unconfirmed: the next
attempt was `39` minutes away, the reference's publish interval
(`libi2pd/RouterContext.h:34`). The reference does not wait. It publishes, and
while the answer has not arrived it asks the **next closest** floodfill
(`libi2pd/RouterContext.cpp:1492`, `:1502`), excluding the ones it has already
asked (`:1540`) and repeating every `ROUTER_INFO_CONFIRMATION_TIMEOUT`
milliseconds (`:37`, `:1560`) until one acknowledges (`:1321-1328`) or the
exclusion set reaches its bound of fifteen (`:38`).

**A publication is now a cycle.** One cycle is a bounded ladder of **batches**;
each batch is the closest eligible targets the cycle has not already asked. The
cycle carries an **exclusion set**, so a target is never asked twice, a
**confirmation deadline** — the reference's `1600` milliseconds, measured from
the moment the batch went out — and a **bound** of fifteen asked targets. A
confirmation ends the cycle; a delivery refusal advances at once; a cycle that
reaches the bound ends and the next period starts a fresh one.

**Proved here:** the cycle asks the closest eligible targets and the next batch
excludes the ones already asked; a cycle is not retried before the reference's
timeout and is retried once it passes; a confirmation ends the cycle; a delivery
refusal advances at once; the cycle is bounded by the reference's fifteen and
the fan-out by the tree's three; the retry rides the loop's **existing** tick,
and no new timer is introduced; the cycle never reads a clock; and the cycle
**cannot get stuck** -- an outcome that leaves nothing to await halts it, so it
never holds the next period's cycle out.

**Two divergences are recorded rather than imitated.** The tree's batch is
Plan 515's fan-out of **three** rather than the reference's one target at a
time, and a cycle that reaches the bound **ends** where the reference clears
its exclusion set and publishes again inside the same period.

**Not proved here, and not claimed:** that a **stock** floodfill answers and
that the retry therefore reaches one. That half needs Plan 493's authorized
two-host topology, and the stock-peer evidence remains blocked on it. Nothing
is advertised: the inventory's advertised count stays zero, `router.version`
stays `0.9.62`, and the published RouterInfo is unchanged.

## The router republishes its own RouterInfo (Plan 515)

Until Plan 515 the router published its own RouterInfo to the network
**once**, from the service-tunnel provisioning path
(`crates/i2pr-daemon/src/service_product.rs:7932`, called at `:8122`), and
never again. The local RouterInfo was read in exactly two places in the
whole tree: that publication, and the SSU2 service installing the record for
handshakes.

**Two consequences, both core-router defects.** A record that is never
refreshed ages out of the network's view, so a peer that wanted to reach
`i2pr` could not find it however long the router had been running. And
because the publication was a side effect of building service tunnels, a
router with **no** service tunnels never published at all.

The reference republishes on a timer — every `ROUTER_INFO_PUBLISH_INTERVAL`
seconds (`39 * 60`, `libi2pd/RouterContext.h:34`), plus up to `105` seconds
of variance (`:36`) — to the **closest** floodfill it has not already asked
that cycle (`libi2pd/RouterContext.cpp:1502`), with a `1600` millisecond
resend burst (`:37`) up to fifteen floodfills (`:38`).

**What this tree now does.** A runtime-neutral schedule owns the period, the
due test and the counted outcomes; it takes `now_ms` from its caller and
never reads a clock. A router-level refresh — driven by the router's own
tick, not by the service product — republishes the record on that period.
The refresh and the service product's one-shot publication select through
the **same version-gated function** — the coordinator over its own store,
where it leaves a DSM receipt, and the refresh over the router-wide
bootstrap's store — so Plan 503's API-version floor holds for every
publication. They share **one** store construction and **one** delivery
path, so they cannot drift apart. The fan-out stays bounded at the closest three
floodfills, and every outcome — published, no local record, no eligible
target, delivery refused, cancelled — is counted by reason.

**What is not claimed.** The reference's resend burst is **not** adopted: one
pass reaches several peers immediately where the reference reaches one and
then waits, and the divergence — three targets per period against the
reference's fifteen — is recorded as a finding, not as parity. The period is
a constant, not a configuration key. Nothing here is advertised:
`router.version` stays `0.9.62`, `caps` and the published RouterInfo are
unchanged, and the inventory's advertised count stays zero.

## The NetDB retention policy (Plan 507)

Plan 506 gave the router a **population** it could grow. Plan 507 gives that
population a **shelf life**.

The gap, precisely:

- `RouterInfoStore::prune(now, max_age_ms)` — the tree's own explicit prune,
  the one Plan 103 §3.4 preferred over silent eviction — had **exactly one
  caller**: its own unit test at `crates/i2pr-netdb/src/store.rs`. No
  production path pruned.
- `RouterInfoStore::insert` **refuses** at the quota and **never evicts**, so
  a NetDB that filled up refused every new peer, permanently, and kept records
  the router's own validator would refuse to accept today.
- the router refused a RouterInfo older than `DEFAULT_MAX_AGE` (24 h) at
  validation and then **retained** one indefinitely. The acceptance rule and
  the retention rule disagreed.

What is claimed, and what is not:

- **The retention window is the router's own acceptance window.** The
  daemon's `RETENTION_WINDOW_MS` is derived from
  `i2pr_netdb::ValidationPolicy::default_const().max_age` through a `const
  fn`, so the router retains exactly what it accepts and the constant cannot
  drift from the policy without a compile error. A second value would be a
  second source of truth, which is the defect this plan removes.
- **The prune is the tree's own explicit prune.** The daemon calls
  `RouterInfoStore::prune` through a bounded method on the bootstrap and on
  the destination coordinator. `i2pr-netdb` is **not** changed: the plan adds
  a **caller**, not a mechanism.
- **The prune rides the reconcile's existing cadence.** It is folded into
  Plan 506's `PeerReconcile::run`, so the prune and the reconcile can never
  drift apart in time, and the router gains no second knob and no second
  task. **No new configuration key**: `config.rs` is untouched, and the static
  guard refuses a `retention` or `max_age` key there.
- **The ordering is prune, prune, snapshot, insert, learn.** The prune runs
  **before** the reconcile's snapshot, so a record the prune removed is never
  carried into the destination store.
- **The locks are taken one at a time and are never nested.** The bootstrap's
  lock is released before the destination coordinator's lock is taken, and
  the step takes **no await at all**, so no lock is ever held across an await.
- **The destination store's lock is tried, never waited for.** A store another
  task is using is left alone for that pass; the condition is **counted**
  under the stable reason label `prune-unavailable`, and the reconcile still
  runs. The store turns over on the next pass.
- **Every prune is counted.** `pruned-bootstrap`, `pruned-destination` and
  `prune-unavailable` carry counts and stable reason labels only — no peer,
  address, key or payload — and they are published through the **same**
  inspection seam Plan 506 established, so an unpublished or poisoned slot
  stays a **gap**, never a fabricated zero.
- **The peer-profile store is not pruned.** It has its own eviction policy and
  self-corrects through the observation tiers; an age prune there would be a
  second policy for the same store.
- **The router's own RouterInfo is never a prune victim.** The local record
  lives beside the store, not inside it, so the prune cannot reach it.
- **The prune never selects.** It removes peer **records**; Plan 491's
  version gate stays at **selection**.
- **The controls are not vacuous, and that is measured.** Four mutations of
  the real tree, each applied, run against the real lanes, and reverted:
  removing the prune call from `fn run` fails **nine** tests, among them
  `the_product_prunes_the_bootstrap_and_the_destination_store` and
  `the_prune_runs_before_the_snapshot`; moving the prune call after the
  reconcile's snapshot fails **seven**, among them
  `the_prune_runs_before_the_snapshot`; collapsing the retention window to
  zero fails **seven**, among them
  `a_record_inside_the_window_survives_the_prune`; and dropping the
  destination-store prune count fails **five**, among them
  `every_prune_must_be_counted`. The tree was restored byte-for-byte after
  each control.
- **No advertisement and no capability change.** `router.version` stays
  `0.9.62`; `caps` and the published RouterInfo are unchanged; the support
  inventory's advertised count stays **zero** (99 surfaces, 93 experimental,
  5 deferred, 1 ready); `Cargo.toml` and `Cargo.lock` are byte-identical.

**What Plan 507 does not close.** The **floodfill-exhaustion consequence**:
pruning a stale floodfill can leave the router with no lookup target.
Protecting floodfills would mean retaining records the router would refuse to
accept — the exact inconsistency this plan removes — so the consequence is
recorded, not designed away. The **destination store's quota values** are
unchanged (4 096 records and 4 MiB by default); the plan changes what
happens *at* the quota, not the quota. The **peer-profile store's age
policy** is out of scope: it already evicts to make room and self-corrects.
Plan 503's **lookup-target API-version gate** stays open. Plan 491 F2 and F4,
Plan 490 Medium #3, Plan 502 M1 and Plan 487 F3's destination-inbound half
stay open. **No stock-peer evidence is claimed**: whether an ordinary public
router's peer set survives a 24-hour retention window is **unproven**, and
the external lanes stay open until Plan 493's authorized topology exists.

Evidence: `crates/i2pr-daemon/src/peer_reconcile.rs`,
`crates/i2pr-daemon/src/bootstrap.rs`,
`crates/i2pr-daemon/src/destination_tunnels.rs`,
`crates/i2pr-daemon/src/service_product.rs`,
`crates/i2pr-netdb/src/store.rs`,
`crates/i2pr-daemon/tests/netdb_retention_feasibility.rs`,
`crates/i2pr-daemon/tests/netdb_retention_boundary.rs`, and
`crates/i2pr-daemon/tests/netdb_retention_live.rs`. The plan of record is
`plans/implementation/network-participation/507-netdb-retention-policy.md`;
the closure record is `plans/closure/network-participation/507-status.md`.

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

## The router publishes its own qualified SSU2 address (Plan 520)

The router publishes its own RouterInfo to the network. Since the tree
began that record has carried **one** SSU2 address — `generate_identity_with_keys`
builds it from the **bound** endpoint with the options `host`, `port`, `v`,
`s` and `i` — but the address was **unqualified**: it omitted the `caps`
and `mtu` options the specification's SSU2 form carries, and the netdb's own
`is_qualified_ssu2_address` refuses an address without them. So the record
named an endpoint no peer could qualify, and the reference publishes both
(`libi2pd/RouterInfo.cpp:1434-1449`, `:281`).

**The switch is `ssu2.advertise`, and it is off by default.** It is refused
unless the SSU2 service itself runs, and it is refused outside the
controlled loopback profile, because the runtime's publication material is
refused for any non-loopback bind. With the switch off the installed record
stands byte-identical: the default router publishes exactly what it
published before.

**The decision has exactly one owner.** The transport's
`build_publication_snapshot` renders the direct form — host, port, the
family capability and the transport's MTU — only when the reachability
state is `Reachable` and the policy allows it; otherwise it renders the
endpoint-less form. The runtime applies that builder to the router's own
evidence, and the daemon's owner **asks the runtime** for the material: it
never builds a snapshot, never writes an address's options, and never
records a reachability signal of its own. The daemon owns the opt-in, the
hand-off to the netdb's own qualified builder, the install, the withdrawal
and the count.

**The withdrawal is the expiry.** When the evidence lapses the published
record returns to the netdb's ordinary **zero-address** form, so the router
does not keep publishing an endpoint it can no longer justify. The runtime's
install gate was changed to allow that: it still binds every SSU2 address the
record carries to the router's own bound endpoint with the router's own
keys, and still refuses two conflicting ones, but it no longer *requires* an
address to be present.

**`caps=f` stays permit-gated.** The netdb's addressed builder takes **no**
floodfill permit, so it cannot mint the floodfill capability, and the
floodfill path's own record is left alone by the owner's stand-down rule.

**Proved here:** the opt-in is off by default and changes nothing when off;
the direct form is published when the transport's material admits it, and the
address is the router's own bound endpoint; the netdb's own qualification
accepts the published form; the record carries no `f` capability and the
floodfill builders still require the permit; the lapsed evidence withdraws
the address; the re-dated record still carries the address; the floodfill
path's record is left alone; and every outcome is counted under its own
reason.

**Not proved here, and not claimed:** that a **stock** peer qualifies the
published record in the live network — that half needs Plan 493's authorized
two-host topology and remains blocked on it. The direct form also needs three
distinct corroboration classes, and Plan 521 closes the gap that Plan 520
recorded here: there is now a production entry point that starts a peer test
outside the controlled floodfill activation. Nothing is
advertised: the inventory's advertised count stays zero, `router.version`
stays `0.9.62`, the published record's `caps` string is unchanged, and
`Cargo.toml` and `Cargo.lock` are byte-identical.

## The router runs its own peer test (Plan 521)

The router publishes its own RouterInfo, and Plan 520 made the SSU2 address
in that record **qualified** — gated on the router's own reachability
evidence. Plan 520 closed with the finding that the ordinary record could
**not** be earned in production: the direct form needs **three** distinct
corroboration classes, and the third — a confirmed peer test — was recorded
in production only by the controlled floodfill activation, which installs
the *floodfill* record Plan 520's stand-down rule leaves alone.

**The mechanism is the I2NP peer test, message 231.** The daemon already
modelled it (`crates/i2pr-daemon/src/peer_test.rs`) and the dispatcher
already classified it (`RouterI2npOutcome::PeerTest`), but neither half had
a production caller. The transport's separate **out-of-session** three-party
peer test is a different mechanism, stays controlled-only, and is untouched.

**The switch is `ssu2.peer_test`, and it is off by default.** It is refused
unless the SSU2 service itself runs, because a peer test is an exchange with
a peer the router already holds a session with. With the switch off the
router sends no probe and records nothing.

**One bounded owner holds both roles.** The outstanding-probe table is keyed
by the probed identifier **and** the probed timestamp, so an echo matches the
probe that produced it; a probe completes only when the answer came from the
peer the probe was sent to, and a mismatch neither completes the probe nor
releases its entry. One probe at a time per peer: a second attempt is
refused, not queued. The table is a ceiling that **refuses** rather than
evicts, so an answer can never be matched against a different probe.

**The responder sits on the live inbound path and decodes once.** It reads
the dispatcher's **classified** outcome — the same outcome the liveness
scheduler correlates — so the inbound message is decoded exactly once and
the responder runs no decoder of its own. An echo of the router's own probe
completes the probe and is owed **no** answer; anything else is owed an
answer that repeats the probed identifier and timestamp unchanged, and the
answer travels to the **authenticated** peer the test arrived from. The
responder records **nothing** about itself.

**The reply traffic is bounded by the router, not by the peer.** A bounded
table of answered keys means one key is answered at most once per age
window, and the table refuses rather than evicting. Without it a peer could
make the router answer the same test again and again.

**Only the transport's tracker decides.** The daemon hands the runtime a
**sealed** proof — `i2pr_transport::ConfirmedPeerTest`, which has no
constructor that takes a boolean or a claim, so the unmatched and timed-out
paths cannot mint one — and the runtime derives the address family from the
session the exchange travelled on. The daemon can neither assert a family
nor record an unmatched exchange, and it never writes a reachability signal
of its own. The transport's corroboration policy is untouched: the
threshold, the classes and the expiry remain the transport's.

**Failure is neutral.** A timeout, an unmatched echo, a peer mismatch and a
refused probe all record **nothing**, so a failed test can never poison
later evidence.

**Proved here:** the opt-in is off by default and refused without the
service; the responder answers with the probed fields unchanged, back to the
peer that sent the test; a matched echo completes with the locally measured
elapsed time; a confirmed exchange records the router's **own** reachability
through the runtime's own recorder; the responder records nothing about
itself; a probe that is not answered times out and records no signal; an
unmatched echo completes nothing; the table's ceiling refuses rather than
evicts; a repeated key is answered at most once; and the transport's tracker
reaches `Reachable` once the confirmed peer test supplies the missing third
class.

**Not proved here, and not claimed:** that a **stock** peer answers the
probe. No stock router implements an I2NP 231 responder at the router level:
i2pd uses message 231 only for a garlic-carried **tunnel** test
(`libi2pd/TunnelPool.cpp:436`, `:493-501` at the pin), and Java I2P has no
`TunnelTestMessage` class at all — its peer test is the out-of-session
`PeerTestManager` (`router/java/src/net/i2p/router/transport/udp/`). The
stock-peer half therefore stays blocked on Plan 493's authorized two-host
topology, and the router-level 231 responder is proven against this router's
own peer. Nothing is advertised: the inventory's advertised count stays
zero, `router.version` stays `0.9.62`, the published record's `caps` string
is unchanged, and `Cargo.toml` and `Cargo.lock` are byte-identical.

## The SSU2 introducer set is the specification's form (Plan 523)

The SSU2 **introducer set** — the peers that will introduce this router to
others — is the only way a firewalled router is reachable at all, and it is
the second half of the network-participation roadmap's completion
definition: *"publishes the validated introducer set in its RouterInfo
address"*. Plan 490 answered the first half — the router answers a peer's
Relay Request as the introducer — and Plan 523 corrects the **form** the
second half depends on.

**What was wrong.** The tree implemented the **SSU 1** introducer form. The
specification's *Published Router Info* section defines the SSU2 introducer
group as `ih[N]` (the introducer's router hash), `iexp[N]` (its expiry) and
`itag[N]` (its relay tag), and says of `ihost[N]`/`iport[N]`/`ikey[N]`:
*"The following options are for SSU only and are not used for SSU2. In SSU2,
Alice gets this information from Charlie's RI instead."* The pinned i2pd
2.61.0 publishes and parses exactly the specification's three options
(`libi2pd/RouterInfo.cpp:1402`, `:1413`, `:1424`; `:353`, `:359`, `:361`),
and the pinned Java I2P 2.13.0 uses `itag0` as the introducer marker
(`router/java/src/net/i2p/router/transport/udp/UDPTransport.java:2335`,
`:2576`).

**Two defects followed, and the first was production-reachable.**

1. **The router could not read a real peer's introducer set.** The parser
   refused `ih`/`iexp` as unknown options, and the refusal is reached by
   the daemon's dial path, its identity path, the floodfill service's
   identity and three runtime sites. A peer that publishes introducers —
   which a firewalled router must, and which the pinned i2pd does — was
   refused outright.
2. **The router's own introducer-only address, were it published, would
   have been unreadable.** The publication builder emitted the SSU 1
   options, and the pinned i2pd reads none of them, so it would have seen
   **no introducers at all** in a record this router signed.

**What changed.** The transport's introducer value is the specification's
tuple `(router hash, expiry, tag)` and exposes no endpoint: an SSU2
introducer is resolved by hash through the network database. The parser
reads `ih`/`iexp`/`itag`, **refuses** the SSU 1 options in an SSU2 address
rather than ignoring them, and refuses an address that carries both a
direct endpoint and introducers — a combination the specification forbids.
The publication builder renders the specification's form. The handshake's
"this address claims introducers" marker names the specification's options,
so the check can still fire. The netdb gains a **second** qualification rule
and a **second** builder for the introducer-only form, beside the untouched
direct rule, and the new builder carries no floodfill permit.

**What this does not close.** The **composition** is explicitly out of
scope: the router still never asks a peer for a relay tag, never records an
introducer, and never publishes an introducer set, and the `C` capability
that would tell a peer this router can introduce is not advertised. That is
a successor's subject, and the form must be right before it is used. The
**stock-peer** half needs Plan 493's authorized two-host topology.

**The advertisement.** Nothing is advertised. `router.version` stays
`0.9.62`; the published RouterInfo's `caps` string is unchanged; the
inventory's advertised count stays **zero**; `Cargo.toml` and `Cargo.lock`
are byte-identical; and the proto crate gains no wire vocabulary.

**Evidence.** The lane `crates/i2pr-daemon/tests/ssu2_introducer_address.rs`
(11 rows) drives the real parser, the real netdb rule and the real netdb
builder over the specification's form and over the pinned i2pd's own
published bytes. The guard
`crates/i2pr-daemon/tests/ssu2_introducer_address_boundary.rs` (13 rules,
13 inverted probes) pins the form structurally. The transport's own unit
tests cover the publication builder's snapshot and its round-trip through
the strict parser.

## The router holds a bounded pool of outbound peer sessions (Plan 522)

The router used to dial per need and hold nothing between needs, so the
publication's ordinary path was the handshake path: the publication's period
is `39 * 60` seconds (`crates/i2pr-daemon/src/router_info_publication.rs:72`)
and a session's idle timeout is `300_000` milliseconds
(`crates/i2pr-daemon/src/config.rs:1486`), so a session established by one
publication was always gone before the next. Plan 522 gives the router a
**bounded pool** of outbound peer sessions, and the transport an
**outbound-only** keepalive that holds a pooled session open.

**The population is the router's own selection.** The pool reads the same
version-gated floodfill selection the publication reads, under the
**default** policy the publication's owner already holds. There is no second
policy and no second source of truth: a peer Plan 491's version floor would
refuse is never a pool candidate.

**The bounds are ceilings that refuse.** One publication cycle's worth of
peers (`15`, `crates/i2pr-daemon/src/router_info_publication.rs:116`) and
two dials per period. A candidate beyond either bound is **refused and
counted** under its own reason — never evicted, never skipped silently — so
a router under flood cannot grow the pool past its ceiling, and a reader can
always tell a refused candidate from a peer the pool never tried.

**There is one dial path.** The pool dials through the **one** existing
narrow seam, so the router's own publication and the pool share a single dial
path. The publication's delivery path is **unchanged**: its existing session
check already reuses a session, so the pool's only job is to make the
session exist. With the pool on and a session present, the publication
performs **no** handshake; with the pool on and no session, it dials exactly
as it did before.

**The owner owns no infrastructure.** It holds no session, no socket, no task
and no timer. The dial is awaited on the pool's **own** task, for the same
reason the publication has one — a session establishment is an await, and the
daemon's `select!` loop must never block on a handshake — and the bootstrap's
lock is released before any dial, so a slow handshake can never hold the
router's NetDB.

**The keepalive is the reference's shape, and it is outbound only.** A
padding-only SSU2 data packet with the immediate-ACK flag set, twenty bytes,
no I2NP message, no new block type and no new message type
(`libi2pd/SSU2Session.cpp:269`, at the pin). The runtime passes the interval
to an **outbound** session and to nothing else, so an inbound session's
lifetime is exactly what it was. The unanswered deadline is the keepalive's
**own** — three intervals — because the keepalive refreshes the session's
send timestamp while the idle timeout reads the **maximum** of the send and
receive timestamps, so the idle timeout could never detect a dead peer. One
keepalive is outstanding at a time, so an unanswered keepalive reaches its
deadline instead of being refreshed by a second one, and the keepalive
respects the transport's congestion gate and the payload budget.

**The pool writes no peer profile.** A dial outcome is this router's
condition as often as the peer's, so the pool counts its outcomes in its own
counters and nowhere else.

**The switch is real.** `ssu2.peer_pool` is **off** by default, refused
without the SSU2 service, and refused when the session's idle timeout cannot
hold the keepalive's unanswered deadline. With the switch off the router
sends nothing it did not already send.

**Proved here:** the opt-in is off by default and refused without the
service; the opt-in is refused when the idle timeout is too short; the pool
plans a dial to a peer it knows and reuses a session it already holds; a peer
whose session lapsed is planned again and the lapse is counted; a candidate
beyond the ceiling is refused and counted; a candidate the version gate
refuses is never planned; every dial outcome is counted under its own reason
and a cancelled dial releases the peer; the pool **establishes a real
session** to a peer it knows over a real SSU2 handshake between two
daemon-owned services on loopback; a peer with no dialable address is
refused under its own reason; a router with the opt-in off establishes no
session; the keepalive's interval reaches the runtime only when the pool is
on; and the counters reach the inspection plane.

**Not proved here, and not claimed:** the **stock-peer** half, which needs
Plan 493's authorized two-host topology. The **dial scope** limit is not
discharged either: the transport's own dial target accepts a loopback
endpoint or an RFC1918 IPv4 address, so a public deployment cannot use the
pool today, and that limit is a transport boundary with its own
plan-of-record. Nothing is advertised: the inventory's advertised count stays
zero, `router.version` stays `0.9.62`, the published record's `caps` string
is unchanged, and `Cargo.toml` and `Cargo.lock` are byte-identical.

## The router publishes the introducer set it learned (Plan 525)

The SSU2 **introducer set** is the only way a firewalled router is reachable
at all. Plan 490 answered the first half (the router answers a peer's Relay
Request as the introducer), Plan 523 corrected the **form** of the second
half's data, Plan 524 gave the router the **acquisition**, and Plan 525
gives the router the **publication**: it publishes the validated introducer
set it learned, in the specification's introducer-only address form.

**The reference publishes one address whose shape depends on reachability.**
The pinned i2pd 2.61.0 writes one SSU2 address
(`libi2pd/RouterInfo.cpp:1333`):

```cpp
bool isPublished = address.published && !address.host.is_unspecified () && address.port;
```

— and when the router is **not** published directly, the `caps` option
carries only the **family** letter, `host` and `port` are **omitted**, `i`
is written, and the `iexp[N]`/`ih[N]`/`itag[N]` groups are written whenever
the address carries introducers (`:1350`-`:1466`). The tree now renders
exactly that form, and the netdb's own second qualification rule accepts it.

**The runtime owns the decision, and the daemon matches on the form.**
`Ssu2RuntimeService::self_publication_material` asks for the **direct** form
first and falls back to the introducer-only form **only** when the router's
reachability evidence refuses the direct one. A refusal that is not about
reachability — a closed runtime, an unbound runtime, a poisoned state — is
propagated unchanged: such a router is **broken**, not firewalled, and
publishing an introducer-only address would hide the fault behind a
plausible record. The daemon never infers a form from a refusal; the
transport's material carries a typed `Ssu2PublicationForm`, and the daemon
builds the record with the netdb builder that matches it.

**Two latent defects were fixed on the way.** Both were invisible because
the path had no production caller. The runtime's introducer-only builder
could **never succeed**: it passed the build time as the snapshot's
evidence expiry, so the transport's own tested gate withdrew every snapshot
and the function always refused with a malformed-address reason. And the
runtime's install gate could not admit the form at all: it required every
SSU2 address the record carries to name the router's own bound **endpoint**,
and the introducer-only form has none. The gate now binds the router's
**own** identity and contact material *by the address's form*.

**The form's evidence is the introducer set.** The snapshot's evidence
expiry is the **earliest** expiry among the introducers it publishes, so the
form is withdrawn the moment its first listed introducer dies and the next
publication cycle re-selects. The family capability is derived from the
router's **bound** socket rather than hardcoded, so the router never claims
a family it has no socket for.

**What is deliberately not claimed.** No configuration key is added:
`ssu2.advertise` already means "the router publishes its own SSU2 address",
and the introducer-only form is that address in its other shape. The **`C`
advertisement** — this router advertising its own introducer role — is a
**third**, separate matter and is not claimed. The **withdrawal** is
unchanged: a firewalled router with no introducer keeps publishing the
netdb's ordinary zero-address record, where the reference publishes an
unpublished `s`+`i`+`v` form. Nothing is advertised: `router.version` stays
`0.9.62`, the published RouterInfo's `caps` string is unchanged, the
inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` are byte-identical. The **stock-peer** half needs Plan 493's
authorized two-host topology.

Evidence: `crates/i2pr-runtime/src/ssu2_runtime.rs`,
`crates/i2pr-daemon/src/self_publication.rs`,
`crates/i2pr-daemon/src/i2pcontrol_inspection.rs`,
`crates/i2pr-netdb/src/local.rs`,
`crates/i2pr-daemon/tests/ssu2_introducer_publication.rs` and
`crates/i2pr-daemon/tests/ssu2_introducer_publication_boundary.rs`. The
references' evidence is recorded in `specs/QUALIFICATION-GATES.md` §24. The
plan of record is
`plans/implementation/network-participation/525-the-router-publishes-the-introducer-set-it-learned.md`;
the closure record is `plans/closure/network-participation/525-status.md`.

## The router's reachability has one owner and one reader (Plan 527)

The router's reachability had **two** trackers, and the product read the one
that could only ever be *confirmed*.

The runtime owned one, inside its service state, and the peer-relay service
owned another, with the **default** policy rather than the runtime's. Every
**contradicting** signal was written only into the second: `RelayFirewalledSignal`
at exactly one place, `AddressMismatch` and `FirewalledLikely` at exactly two.
The tracker the product read received only `LocalConfiguredBind`,
`PeerTestResult{Confirmed}`, `ValidatedPath` and
`AuthenticatedPeerObservedExternalAddress` — none of which contradicts. The
state machine's `Firewalled` and `Unreachable` branches were therefore **dead
code in the running router**, and the peer-relay service's snapshot had **no
reader in the daemon at all**.

**The consequence reached the whole introducer composition.** The daemon's
firewalled gate reads the runtime's tracker, so it was permanently `false`;
Plan 524's owner refuses every request while the router is not firewalled;
the router therefore never asked a peer for a relay tag, never obtained an
authenticated introducer, and the introducer set it published stayed empty —
which left Plan 525's publication with nothing to publish and Plan 526's `C`
riding a role whose acquisition half could never run in the field. Plan 524's
lane drove the owner with the flag **injected**
(`owner.decide(&eligible, true, ...)`), so it proved the consumer honours the
flag and never exercised the producer.

**The transport decoded the observed address and discarded it.** The
data-phase Address block carries a validated endpoint and the session already
decoded it, but the event was a unit variant: the router was told that a peer
observed an external address and never **which** address, so it could not
compare it with its own bound address and could not discover that it was
behind a NAT.

Plan 527 gives the router's reachability **one owner and one reader**. The
peer-relay service keeps no tracker at all: it returns the typed facts it
computes and the **runtime** records them. Every typed peer-test outcome is
recorded, not only the confirming one, so a contradicting outcome reaches the
product. The transport keeps the endpoint it already decoded — **no codec,
block or option changes** — and the runtime compares the observation with the
router's **own bound socket**: an agreement corroborates and a disagreement
contradicts. The transport's reachability **model** — the five signal kinds,
the corroboration classes, the policy floor and the state machine — is
untouched.

**What Plan 527 does not close.** The **requester** side of the SSU2 relay
protocol, which produces the relay-success indication in the field, stays
unwired and out of scope, with its owner named. The **stock-peer** half
remains blocked externally on Plan 493's authorized two-host topology.

## The router's peer-test verdict is a fact, not an error (Plan 528)

The tree ran a **complete** three-party SSU2 peer test whose verdict carried
exactly what the third party observed — and then threw every verdict away
except "the observation matched the router's own bound address and two peers
corroborated". A **disagreement** became `ExchangeNotConfirmed`, an **error**,
not a **fact**.

**The specification says the verdict has four outcomes.** The SSU2
specification's peer-test result table is a table over the three messages
Alice received, and the row "message 4 arrived, message 5 did not" is
**FIREWALLED**. The pinned i2pd 2.61.0 reaches the same conclusion from a
**single** peer test (`libi2pd/SSU2Session.cpp:2703`-`:2705`); the
specification's "two or more tests" is *recommended*, not required. So the
tree's model had to be able to express that verdict at all, and the verdict
had to reach the one tracker the product reads.

**The transport applied the corroboration floor to contradictions.** Every
peer-test verdict was filed under one corroboration class, so two peer-test
verdicts contradicted **once** — the transport's own test says so — and the
floor of two distinct contradicting classes could never be reached. One
contradiction therefore left the router on `ObservedUnconfirmed` for ever,
and `Firewalled` and `Unreachable` were unreachable states: the daemon's
firewalled gate was **permanently false**, and with it the introducer
composition's acquisition half.

Plan 528 separates the two floors. The **corroboration** floor is untouched
at 2, so `Reachable` still needs three distinct supporting classes and the
**positive** claim is not weakened at all. The new **contradiction** floor is
1 — the specification's own single-test rule. The **firewalled verdict** is
first class: a peer test that concludes the router is not directly reachable,
and the relay-firewalled indication, put the router in `Firewalled`; a bare
disagreement between two observations puts it in `Unreachable`.

**The disagreement is the router's own firewalled fact.** The runtime's
comparison against the router's **own bound socket** records the disagreement
as the firewalled verdict, not as a bare mismatch: a peer seeing the router
at an address it does not own **is** the router behind a NAT, and it is the
same fact the specification's FIREWALLED row reports, so both routes reach the
same state. This **corrects** Plan 527's deep-dive, which described the
disagreement as `PeerTestResult{AddressMismatch}`; Plan 527's closure record
is not rewritten, and the correction lives here and in this plan's closure
record.

**The verdict is a fact, not an error.** The controlled driver's outcome enum
grew the verdicts it used to throw away, and its error arm keeps only the
genuinely **failed** cases — a refused call, a missed deadline, a
cancellation, a protocol deviation. A caller can therefore tell "the test
failed" from "the test succeeded and the router is behind NAT". The daemon's
controlled activation still refuses anything that is not a confirmation, so
the reachability proof is still minted only from a real confirmation.

**The specification's table is transcribed and pinned.** It is written into
the tree as **data** and pinned by a test, so the tree's rule is checkable
against the specification rather than only against prose. The specification's
"FIREWALLED or SYMNAT" row is read **conservatively** as ambiguous — neither
a confirmation nor a firewalled verdict.

**What Plan 528 does not close.** The **production** SSU2 peer-test driver
stays unbuilt — and its choreography question is named: the tree's Alice role
needs the helper's identity up front, while the specification has Bob select
the helper. The **requester** side, the `B` and `caps=f` advertisements and
the **stock-peer** half stay out of scope, with their owners named. The
delivery is confined to the **controlled loopback profile**: the comparison
is between two loopback addresses, so the contradiction is a **qualification**
claim. The **stock-peer** half remains blocked externally on Plan 493's
authorized two-host topology.

## The router must learn the address at which a peer sees it (Plan 529)

The SSU2 specification's Address block carries "Alice's address, sent to Alice
by Bob, or Bob's address, sent to Bob by Alice" — the address at which the
**recipient** sees the **sender**. The tree sent that block with the **wrong
value** and then **threw the answer away**.

**The value was wrong.** The responder put its own bound address in the
Session Created, which told the initiator that the initiator's address was the
responder's. The pinned i2pd 2.61.0 does the opposite: it puts the
initiator's remote endpoint in the Session Created
(`libi2pd/SSU2Session.cpp:999`).

**The answer was discarded.** The initiator decrypted the Session Created's
payload and read only the NewToken announcement from it; the Address block
was decoded and dropped. The reference reads the whole decrypted payload in
the very same message (`libi2pd/SSU2Session.cpp:1110`), and its
`HandleAddress` logs "SSU2: Our external address is "
(`libi2pd/SSU2Session.cpp:2072`) and hands the address to the router
(`libi2pd/SSU2Session.cpp:2075`).

**The consequence was a missing corroboration class.** The class-1
corroboration — an authenticated peer reporting the address at which it sees
this router — had **no field producer** outside the controlled profile. The
corroboration floor needs three distinct supporting classes before
`Reachable`, so no lane could ever earn it: the router could never claim
that it is directly reachable, because it never learned its own address from
a peer.

Plan 529 corrects the value and stops the discard. The responder's Session
Created and its Retry carry the **observed source address** of the request
they answer — built from the datagram's own header, never from a configured
or bound value — and `ResponderConfig::local_address` is gone, because the
value it held was never the right one. The initiator's Session Request now
carries Alice's view of Bob and the responder reads it: the specification's
block is symmetric, and the reference reads it in **both** directions
(`libi2pd/SSU2Session.cpp:934`).

**The observation reaches the tracker.** The runtime routes it into Plan
527's seam, which compares it with the router's **own bound socket**. An
agreement corroborates; a disagreement is Plan 528's firewalled verdict,
unchanged. The comparison is recorded only once a session was really
established, so a corroboration class is never counted for a session that
does not exist.

**A missing block is evidence absent.** A session with no Address block is
established all the same and records nothing: the observation is evidence,
not a handshake requirement.

**What Plan 529 does not close.** The delivery is confined to the
**controlled loopback profile**: the comparison is between loopback
addresses, so the observation is a **qualification** claim. The **production**
SSU2 peer-test driver, the **requester** side, the `B` advertisement and
`caps=f` stay out of scope, with their owners named. The **stock-peer** half
remains blocked externally on Plan 493's authorized two-host topology.

## The router advertises its own introducer role (Plan 526)

The SSU2 **introducer** role is worth nothing to a peer that cannot see it.
Plan 490 built the role, Plan 498 made the role owner reachable in a live
daemon, Plan 499 made it work, Plan 523 corrected the **form** of the
introducer set, Plan 524 gave the router the **acquisition** and Plan 525 the
**publication**. Plan 526 gives the router the **advertisement**: the
specification's Relay capability, the `"C"` letter, in the `caps` option of the
router's **own** address.

**The letter is the specification's.** "Relay — As advertised with a `"C"` in
the `"caps"` parameter", and the option's alphabet is `[B,C,4,6]`. The pinned
i2pd 2.61.0 publishes the role letters on the **published** form only
(`libi2pd/RouterInfo.cpp:1350`-`:1373`), and its peer-side check demands an
endpoint for an introducer (`:1059`):

```cpp
return addr && addr->IsIntroducer () && !addr->host.is_unspecified () && addr->port;
```

— so a router that publishes no host and no port is **not** an introducer to a
reference peer, however its `caps` read. The tree's `C` therefore rides the
**direct** form and never the introducer-only one, and the netdb's own
introducer rule — which refuses any role letter — enforces the exclusion
without being weakened.

**The tree publishes the combination, not the reference's role-only form.** The
reference's published form carries `"C"`; the tree's carries `"4C"`, because
the tree's stricter rule requires the capability string to name the address's
own family. Both are inside the specification's alphabet, and both references
parse a combination: i2pd's `ExtractAddressCaps` is a per-character switch
(`libi2pd/RouterInfo.cpp:562`-`:581`), and the pinned Java I2P 2.13.0 reads the
capability with a containment test
(`router/java/src/net/i2p/router/transport/udp/UDPTransport.java:2588`,
`:4185`).

**The letter is a claim, so it is published only while the role is live.** The
runtime owns that fact (`introducer_service_enabled`) and owns the wire, so it
composes the capability in one place: the family letter, plus the role letter
**only** while the role is live. The runtime's install gate **refuses** a
role-bearing record while the role is not live, so a stale letter cannot reach
the wire and the installed record cannot stand with a claim the router has
stopped making.

**The netdb gains a third rule and a third builder**, beside the untouched
direct and introducer rules. The third rule is **exact** — the address's own
family letter followed by the role letter — so an unearned role cannot reach
the wire through a permissive rule, and the third builder carries no floodfill
permit, which matches the reference, which clears the role for a floodfill
router (`libi2pd/RouterContext.cpp:655`).

**The registration recorded a latent defect.** The composition refused the
SSU2 service outright whenever `ssu2.introducer_service` was set, so the role
owner the composition constructs was **unreachable in every configuration**,
and Plan 499's closed record — "the daemon's live loop then constructs the
role owner and the carriage" — was false of the tree. The blanket refusal is
replaced by the role's own explicit, default-off opt-in. Plan 499's closed
record is **not** rewritten; the correction lives in Plan 526's closure record
and in the registry row.

**What is deliberately not claimed.** No configuration key is added:
`ssu2.introducer_service` is the role's switch and `ssu2.advertise` is the
publication's switch, and the `C` rides both. The **`B`** (peer-test)
advertisement is out of scope. The advertisement is confined to the
**controlled loopback profile**, because `ssu2.advertise` is refused outside
it, so the `C` is a local qualification claim and not yet a network-reachable
one. Nothing is advertised in the inventory sense: `router.version` stays
`0.9.62`, the inventory's advertised count stays **zero**, and `Cargo.toml` and
`Cargo.lock` are byte-identical. The **stock-peer** half needs Plan 493's
authorized two-host topology.

Evidence: `crates/i2pr-runtime/src/ssu2_runtime.rs`,
`crates/i2pr-netdb/src/floodfill_role.rs`, `crates/i2pr-netdb/src/local.rs`,
`crates/i2pr-daemon/src/self_publication.rs`, `crates/i2pr-daemon/src/lib.rs`,
`crates/i2pr-daemon/tests/ssu2_introducer_advertisement.rs` and
`crates/i2pr-daemon/tests/ssu2_introducer_advertisement_boundary.rs`. The
references' evidence is recorded in `specs/QUALIFICATION-GATES.md` §25. The
plan of record is
`plans/implementation/network-participation/526-the-router-advertises-its-own-introducer-role.md`;
the closure record is `plans/closure/network-participation/526-status.md`.

## The router asks a peer for a relay tag (Plan 524)

The SSU2 **introducer set** — the peers that will introduce this router to
others — is the only way a firewalled router is reachable at all, and it is
the second half of the network-participation roadmap's completion
definition. Plan 490 answered the first half (the router answers a peer's
Relay Request as the introducer), Plan 523 corrected the **form** the second
half depends on, and Plan 524 gives the router the **acquisition**: it
learns that a peer is an introducer, asks such a peer for a relay tag, and
records the answer as an authenticated introducer.

**The condition is the reference's own.** The pinned i2pd 2.61.0 asks for a
relay tag under exactly one condition
(`libi2pd/SSU2Session.cpp:753`):

```cpp
if (GetRouterStatus () == eRouterStatusFirewalled && m_Address->IsIntroducer ())
```

— the router is **firewalled** and the peer **advertises the introducer
capability** (`libi2pd/RouterInfo.h:184`, `caps & eSSUIntroducer`). A
reachable router needs no introducer, so it asks nobody; and a peer that
*publishes* introducers is a peer that *has* introducers, not a peer willing
to *be* one, so the published set is never the test.

**The capability read is authenticated.** The daemon reads the peer's
**validated** RouterInfo from the authoritative store and parses its SSU2
address with the transport's own strict parser, so the bytes are the ones
the netdb validated on ingest. A peer with no stored record, with no SSU2
address, with an unparsable address, or with an address that advertises no
introducer letter is **not** an introducer, and each is a distinct counted
refusal — never a silent absence. (Plan 524's §17.2 corrected the plan's
registered design, which put this read in the transport's validated peer
record: the transport's `validate_router_info` runs on exactly one path, the
**responder's** SessionConfirmed, and the initiator's peer record carries an
empty RouterInfo — so the read could not have answered the question for the
outbound sessions the request travels on.)

**The request rides an existing session.** The peer is drawn from the peers
the runtime already holds a session with; no new connection is made, and at
most one request leaves per cadence window. The window is **bounded** and
**rotates**, so a busy router sees every peer it holds a session with over
time instead of re-examining the same head of the list for ever, and a peer
the router already asked is not asked again while its session lives.

**A tag is recorded only in answer to this router's own request.** The
runtime keeps a bounded set of outstanding requests. A tag for a peer outside
that set is **refused** and counted: a peer may send the block unsolicited,
and recording an introducer nobody asked for would put an unauthenticated
peer into the router's introducer set. The recorded tuple is the
specification's — the introducer's router hash, an expiry and the nonzero
relay tag — and the record's endpoint and introduction key are **this
router's own**, because the SSU 1 shape published the *target's* contact
material and the SSU2 form carries neither.

**The switch is real.** `ssu2.introducer_request` is **off** by default and
refused without the SSU2 service, so a router that has not asked to learn an
introducer never sends a relay-tag request.

**The advertisement.** Nothing is advertised. `router.version` stays
`0.9.62`; the published RouterInfo's `caps` string is unchanged; the
inventory's advertised count stays **zero**; `Cargo.toml` and `Cargo.lock`
are byte-identical; and the router still advertises **no** introducer
capability of its own.

**What this does not close.** The **publication** half — building the
introducer-only address, choosing it over the direct form, and signing it —
is a successor's subject, and so is the `C` capability this router publishes
about **itself**. The **stock-peer** half needs Plan 493's authorized
two-host topology.

**Evidence.** The lane
`crates/i2pr-daemon/tests/ssu2_introducer_request.rs` (13 rows) drives the
daemon's owner and the runtime's live service over a **real** SSU2 session
between two daemon-owned services on loopback: the opt-in is off by default
and refused without the service; the capability read answers from the
validated record and counts each refusal under its own reason; a firewalled
router asks exactly one peer per window and a reachable router asks nobody;
the peer is never named without a session; the request rides the established
session; the answer is recorded as an authenticated introducer bound to the
peer that issued it; and a tag for a peer the router did not ask yields no
record. The guard
`crates/i2pr-daemon/tests/ssu2_introducer_request_boundary.rs` (15 rules,
15 inverted probes) pins the production surface structurally.

## The router must record its own bound address (Plan 530)

The transport's reachability model says `Reachable` needs **three distinct
supporting classes**. The tree had **two** ordinary producers — Plan 529's
handshake observation (class 1) and Plan 521's peer test (class 3) — and one
that had none.

**The class was counted but never earned.** The router's own configured bind
(class 0) was written by exactly one recorder,
`note_explicit_bind_for_controlled_qualification`, whose only caller was the
controlled floodfill activation
(`crates/i2pr-daemon/src/floodfill.rs:648`). The opt-in and the counting were
already in place: the daemon mapped `explicit_bind_corroboration: true`
(`crates/i2pr-daemon/src/router_i2np.rs:1539`) and the runtime turned it into
the transport's `configured_direct_allowed`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:1830`), so the transport skipped the
class only when the policy forbade it. **Only the ordinary recorder was
missing.**

**The consequence.** A router in ordinary operation held at most two classes,
its state stalled at `CandidateReachable`, and it published the firewalled
form for ever. The direct form's gate
(`crates/i2pr-transport-ssu2/src/publication.rs:244`) needs
`ReachabilityState::Reachable`, and `Reachable` needs *more* than the
corroboration floor of two
(`crates/i2pr-transport/src/reachability.rs:66`).

**What the tree now does.** The runtime records the class at **bind time**,
because that is when the fact becomes true. The recorder is **private** and
lives in the runtime, which owns the tracker and the sockets; it is gated on
the **existing** opt-in; it records a family only when the runtime can
**name** its own bound endpoint for it, reusing the predicate `bound_endpoint`
already applies, so a wildcard or port-zero bind names no address and records
nothing; and the signal carries only the **family**, never a literal endpoint.

**What is unchanged.** The transport's reachability **model** — its signal
kinds, its corroboration classes, the corroboration floor of two and the
contradiction floor of one — is untouched, and `Reachable` still needs three
distinct classes. No configuration key is added and no default moves. The
controlled activation's narrow recorder and its call site are byte-identical,
so Plan 283's foundation is not rewritten, and the daemon records no
reachability signal of its own.

**The triple is now earnable.** The bind, the peer's observed external
address and a confirmed peer test are three distinct classes, so the
corroboration floor is above them and the direct form becomes publishable.
The lane proves it over a real session between two real routers on loopback.

**What is not claimed.** The runtime's publication path stays **loopback-only**
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3111`), so the direct form the plan
makes reachable is a **controlled-loopback** claim. The peer test stays behind
the default-off `ssu2.peer_test` opt-in, so the third class is earned only by
a router whose operator asked for it. The class expires with the observation
TTL and the plan adds no timer, so a router that binds once and never observes
a peer loses its evidence. The **production** SSU2 peer-test driver stays
unbuilt, the **requester** side stays unwired, and the **stock-peer** half
needs Plan 493's authorized two-host topology.

## The router must reach a peer through its introducer (Plan 531)

The SSU2 relay protocol has two halves. The **introducer** half — the router
that answers a relay request and introduces a target — was already in the tree
(Plans 499, 524 and 526). The **requester** half was not.

**The requester half existed as machinery with no producer.** The transport's
`RelayRequester` state machine had no production caller, the runtime's
`start_relay_request` seam had no caller, and the transport's
`RelayFirewalledSignal` — class 4, contradicting
(`crates/i2pr-transport/src/reachability.rs:197`) — had **no producer at all**.
So a firewalled peer was unreachable, and the tree had no way to record that
fact either: the one signal that names it had no writer.

**The division of ownership.** The daemon owns the *policy*; the runtime owns
the *mechanics*. `crates/i2pr-daemon/src/relay_requester.rs` selects a target
from the **validated** RouterInfo store, resolves the introducer by hash, draws
a fresh nonzero nonce from the router's own randomness, signs the request with
the router's own identity, and starts it through the runtime's existing
`start_relay_request` seam
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2809`). The runtime owns the
sockets, the session, the bounded tables and the transport's verification. The
transport's relay state machines are **used, not edited**: the only transport
change is a `pub use RequesterState` re-export
(`crates/i2pr-transport-ssu2/src/lib.rs:100`).

**The hole punch needed its own route.** A hole punch travels **out of
session**: it is sealed under the *receiver's* introduction key and it names
neither a session nor a peer hash, so it cannot ride the session-shaped relay
handoff. The runtime gives it a route of its own
(`crates/i2pr-runtime/src/ssu2_runtime.rs:7635`) and a bounded handoff of its
own (`crates/i2pr-runtime/src/ssu2_runtime.rs:1709`), with the same ceiling as
the relay handoff.

**A response is routed by correlation.** The session loop sends a Relay
Response to the requester's handoff **iff** it correlates with one of the
router's own live request nonces
(`crates/i2pr-runtime/src/ssu2_runtime.rs:7310`). Every other response keeps
travelling the introducer's handoff exactly as before, and the existing
`drain_relay_events` is byte-identical.

**The signal is a contradiction, and the daemon never writes it.** The daemon
calls the **runtime's** `note_relay_firewalled`
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3579`), so the transport's class-4
contradiction is recorded through its own owner. No corroborating class is
ever claimed for a relayed peer: a router that is reachable only through a
relay is reachable *through the relay*, not directly, and the tree records it
that way.

**One opt-in only.** `ssu2.relay_requester`, default **off**, refused at
parse time without `ssu2.enabled`
(`crates/i2pr-daemon/src/config.rs:3536`). Every refusal is a typed, counted
reason (`crates/i2pr-daemon/src/relay_requester.rs:304`), and **no refusal
writes into a peer's record** — a local condition never becomes a peer's
profile.

**The exchange completes.** The lane drives three real routers over real
loopback UDP: Alice asks Bob for Charlie, Bob introduces Charlie, Charlie
accepts, Bob relays the response, and Charlie's hole punch arrives
out-of-session. The response is correlated, the hole punch is routed, the
relay-firewalled signal is recorded, and the router's corroboration count is
**unchanged**.

**What is not claimed.** The **target** side's response emission has no
product wiring in this tree: the transport says the caller emits the accept and
the hole punch, and no daemon role does that yet, so the lane completes that
half over the transport's own preimage. The delivery is confined to the
**controlled loopback profile** — the router's SSU2 dial target is loopback-only
and the private-LAN variant accepts only RFC1918 IPv4 — which is Plan 431's
bound. The **production** SSU2 peer-test driver stays unbuilt; the `B`
advertisement and the `caps=f` advertisement stay with their owners; and the
**stock-peer** half needs Plan 493's authorized two-host topology.

## The router must answer a relay introduction (Plan 532)

The SSU2 relay protocol has three roles. The **introducer** answers a relay
request and forwards an introduction; the **requester** asks an introducer to
reach a peer it cannot reach directly; the **target** is the peer that is
reached, and it **answers**. The introducer half was in the tree from Plans
490, 499 and 524, and the requester half from Plan 531. The **target** half
was the last one with no producer.

**The target half existed as machinery with no caller.** The transport's
`RelayTarget` state machine was complete
(`crates/i2pr-transport-ssu2/src/relay.rs:1025`) — it verifies the introducer
context against the specification's preimage, enforces the global quota,
suppresses a replayed introduction and expires a pending one — and the runtime
held the table and exposed the seam that drives it
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2881`). Nothing in production called
the seam. The introduction was routed to the **introducer's** handoff
unconditionally, so the introducer owner refused it as not its role and the
target's table was never fed.

**The division of ownership.** The daemon owns the *policy*; the runtime owns
the *mechanics*. `crates/i2pr-daemon/src/relay_target.rs` reads the
requester's **validated** RouterInfo out of the authoritative store, takes the
requester's signing key and introduction key from that validated record
through the transport's strict `Ssu2RouterAddress::parse`, drives the
runtime's existing target seam so the transport's own state machine is used
and not re-implemented, signs the accept with the router's **own** identity
key over the introducer-bound preimage, and hands the hole punch's pieces to
the runtime's outbound seam. The transport's relay state machines are **used,
not edited**: the plan changes no transport file at all.

**The introduction has its own bounded handoff.** The runtime adds a **fourth**
channel for the target's events (`crates/i2pr-runtime/src/ssu2_runtime.rs:2231`)
with the same ceiling as the relay handoff, and the session loop routes an
inbound Relay Intro to it **iff** the operator's opt-in is on
(`crates/i2pr-runtime/src/ssu2_runtime.rs:7484`).

**The hole punch is staged, never sessioned.** The outbound seam
(`crates/i2pr-runtime/src/ssu2_runtime.rs:2923`) builds the datagram with the
transport's own codec, seals it under the **requester's** introduction key,
and stages it through the runtime's existing bounded queue as control traffic.
Its destination is checked against the **union** of the two dial profiles the
runtime offers (`crates/i2pr-runtime/src/ssu2_runtime.rs:5427`), so the target
role cannot widen the router's reach beyond what the router already dials.

**The answer gains no corroborating class.** A router that answers an
introduction is reachable **through the relay**, not directly, so the daemon
writes **no** reachability signal of its own and the target's owner touches no
tracker. The transport's relay state machines and its reachability model are
untouched, and both floors are unchanged.

**One opt-in only.** `ssu2.relay_target`, default **off**, refused at parse
time without `ssu2.enabled` (`crates/i2pr-daemon/src/config.rs:3566`). Every
refusal is a typed, counted reason
(`crates/i2pr-daemon/src/relay_target.rs:62`), and **no refusal writes into a
peer's record** — a requester this router does not hold is a **local**
condition, never a peer's profile.

**The exchange completes.** The lane drives three real routers over real
loopback UDP: Alice asks Bob for Charlie, Bob introduces Charlie, **Charlie
verifies the introduction through the transport's own target state machine and
answers**, Bob verifies the accept and forwards it, Alice correlates it, and
Charlie's hole punch arrives out-of-session. The relay-firewalled fact is
recorded and the router's corroboration count is **unchanged**.

**What is not claimed.** Two limits are recorded, not hidden. The
introduction is routed to the target's handoff **only while the opt-in is
on**: with the opt-in off — the default — the introduction keeps travelling the
introducer's handoff exactly as before, and the lane proves both halves. And
the transport's own requester table requires the accept to be correlated
**before** a hole punch is processed, while the punch travels one hop and the
accept two, so on loopback the punch reaches the requester's handoff first;
the requester owner has no deferred-punch buffer, and the lane presents the
punch after the accept. The delivery is confined to the **controlled loopback
profile** — the router's SSU2 dial target is loopback-only and the private-LAN
variant accepts only RFC1918 IPv4 — which is Plan 431's bound. The
requester's **validated** RouterInfo must be in the authoritative store. The
**production** SSU2 peer-test driver stays unbuilt; the `B` advertisement and
the `caps=f` advertisement stay with their owners; and the **stock-peer** half
needs Plan 493's authorized two-host topology.

## The router must establish a session with the peer it reached through the relay (Plan 533)

The SSU2 relay exchange was complete in **shape** and not in **effect**. Plan
531 gave the requester its role and Plan 532 gave the target its role, and
Plan 532's lane drove all three routers over real loopback UDP — but the
exchange ended at a hole punch, and the requester never opened a session with
the peer it had been relayed to. This is the clause the roadmap's completion
definition names: *"so that a firewalled peer is reachable through it"*.

**Three gaps stood in the way.**

- The accept's token was **inert**. The target drew it from the daemon's own
  randomness, while the only possible consumer is the target's **responder**,
  whose authority is the runtime's `TokenStore`
  (`crates/i2pr-runtime/src/ssu2_runtime.rs`). A token that was never issued
  into that table is refused by the responder as unknown, so the value the
  accept carried had no consumer at all.
- An early hole punch was **refused and lost**. The transport's requester
  table requires the accept before a hole punch
  (`crates/i2pr-transport-ssu2/src/relay.rs:582`), and the punch travels
  **one** hop while the accept travels **two** — the introducer must receive,
  verify and forward it — so on loopback the punch arrives first. The daemon
  collapsed the refusal into a generic transport refusal after the event had
  already been consumed.
- There was **no establishment**. Nothing read the accept's endpoint and
  token, nothing read the target's validated static and introduction keys,
  and nothing dialled the target — while the transport's own documentation
  already named the design: *"the caller dials via the standard establishment
  path with the HolePunch token — never a relay-specific fake session"*.

**The token is now issued by the responder that will consume it.** The runtime
issues the establishment token from its **own** responder table, bound to the
requester's **exact** source address, with the runtime's randomness. The
daemon's own draw is removed, so a token a responder could never accept is no
longer produced. The binding is the whole security property of the
establishment, and it is enforced by the transport's own table: exact address,
one use, and a 30-second lifetime.

**The early punch is held, not refused.** The deferral lives in the daemon's
requester owner, which sees the punch and the accept on the same handoff. The
transport's wrong-role rule is untouched and still enforced; it is the
**deferral signal** rather than a failure, and the held punch is presented
again through the transport's own seam once the accept is correlated. The hold
is bounded, and its overflow is counted under its own reason.

**The establishment is the ordinary establishment path.** The accept's
endpoint is the target's own bound endpoint and the target's static and
introduction keys come from its **validated** RouterInfo through the
transport's strict parser; the whole target is checked against the
**qualification profile**, so the relay establishment cannot widen the
router's reach. The dial presents the accept's token explicitly, and the
runtime's cached-token path is skipped for it — an older cached entry must
never shadow the accept's token, and the cache is left for the dial it was
issued for.

**The establishment runs on a bounded, owned task.** A session establishment
is an await, and the daemon's `select!` loop must never block on a handshake:
the relay tick is a 100 ms cadence and a handshake is measured in seconds. The
establishment therefore runs on a task of its own under the service's child
scope, so it is owned, cancelled and joined with the service. The handoff
between the requester owner and the task is bounded, and a full handoff
returns the material to the owner under a counted reason — it is never
dropped silently.

**The exchange completes and the peer is reached.** The lane drives three real
routers over real loopback UDP: the early punch is held and re-driven, the
target's responder consumes the accept's token, and the requester holds an
**authenticated session** with the peer it was relayed to. The token is
**one-use** and **source-bound**, and both refusals are counted by the
runtime's own responder. The corroboration count is **unchanged** and the
recorded state is `Firewalled`: a peer reached through a relay is reachable
*through the relay*, not directly.

**What is not claimed.** The transport's relay state machines are **used, not
edited**: the plan changes no transport file at all. **No configuration key is
added** — the establishment rides `ssu2.relay_requester` and the token
issuance rides `ssu2.relay_target`, and both stay default **off** and refused
at parse time without `ssu2.enabled`. The daemon writes **no** reachability
signal of its own. The delivery is confined to the **controlled loopback
profile** — the router's SSU2 dial target is loopback-only and the private-LAN
variant accepts only RFC1918 IPv4 — which is Plan 431's bound. The target's
**validated** RouterInfo must be in the authoritative store. The
**production** SSU2 peer-test driver stays unbuilt; the `B` advertisement and
the `caps=f` advertisement stay with their owners; and the **stock-peer** half
needs Plan 493's authorized two-host topology.

## The router must build a tunnel toward the peer it reached through the relay (Plan 534)

The SSU2 relay exchange was **complete** after Plan 533: the requester holds an
authenticated session with the peer it was relayed to. What the router still
could not do was **use** that peer.

**The deliberate reach was dropped on the floor.** The product's tunnel-build
route draws its peer from the peer-profile store's policy selection
(`crates/i2pr-daemon/src/exploratory_driver.rs`), under the store's own
constraints — the rank floor, the participation cap, the per-prefix diversity
rule and the version gate. The peer the router had deliberately reached through
a relay was in that store, but it was **one candidate among every peer the
router knew**, and nothing distinguished it. An expensive, explicit act — the
whole point of the relay — was diluted into a random draw.

**The named target is the route's subject, not one more candidate.** The
selection's rank floor, participation cap and per-prefix diversity rule exist
to **spread load**, and letting them dilute a deliberate reach would defeat
the point. A bounded, expiring **local observation table**
(`crates/i2pr-daemon/src/relay_session_peer.rs`) records that this router
reached a peer through a relay; the establishment's own task records into it
when the handshake succeeds; and the driver **takes** the observation and names
that peer.

**The named target passes the same floor.** A peer below the API-version floor
cannot complete the build, and a build that cannot complete burns a tunnel slot
and a coordinator lease. The named target is therefore gated by the **same**
function every drawn candidate passes (`tunnel_peer_eligibility`), and the
gate's verdict is carried faithfully: a below-floor peer is refused and
counted, never admitted. A named target whose **validated** record the router
does not hold is a **local** condition and is counted as one
(`peer_material_absent`) — never as the peer's missing version.

**The observation is a local fact.** It says that **this router's** session
with the peer is established. It writes nothing into the peer's record, claims
nothing about the peer's reachability, and grants the peer no corroborating
class: a peer reached through a relay is reachable *through the relay*, not
directly. The corroboration count is unchanged and the recorded state stays
`Firewalled`.

**The observation is consumed, and it expires.** Taking is consuming: the
router builds toward a peer it reached through a relay **once**, and a router
that wants to reach the peer again reaches it again through the relay. The
observation expires after a bounded lifetime, because a session that died
while the router was not looking must not be built toward. The table is
**bounded**, and a full table is refused under its own counted reason.

**One build path.** The named target's build goes through the **existing**
`ExploratoryBuildCoordinator::rebuild_exploratory`, so Plan 504's liveness
scheduler and Plan 505's pairing apply unchanged. The route falls back to the
store's own selection when a named target is refused, so one ineligible peer
never blocks the exploratory posture.

**What is not claimed.** **No configuration key is added**: the table rides
`ssu2.relay_requester` and is created only when the operator asked for the
requester role, so an ordinary router records nothing and names nothing. The
transport's relay state machines and the reachability model are **untouched**.
The delivery is confined to the **controlled loopback profile** — the
router's SSU2 dial target is loopback-only and the private-LAN variant accepts
only RFC1918 IPv4 — which is Plan 431's bound. The **production** SSU2
peer-test driver stays unbuilt; the `B` advertisement and the `caps=f`
advertisement stay with their owners; and the **stock-peer** half needs Plan
493's authorized two-host topology.
