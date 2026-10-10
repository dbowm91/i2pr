# Red25519 / Encrypted LeaseSet2 Clean-Room Continuation

Status: Plans 329–334, 336–338, and 346 passed. Plan 335 remains an authoritative measured-negative record, but its forward interpretation is superseded: Proposal 146 / standalone Red25519 uses `I2P_Red25519H(x)` plus length framing, while the Encrypted LeaseSet specification and deployed Java I2P / i2pd use the randomized RedDSA transcript without those additions. **Plan 346 corrected exactly that ELS2 network profile** (ADR 0032) without touching the strict primitive: the deployed profile now cross-verifies against executed Java I2P and i2pd output in both directions, outbound records carry it, and inbound records accept it plus the strict transcript inside the bounded type-5 verifier. What is still missing is the **live** end-to-end cross-router evidence, so **Plan 347 is the sole remaining gate for this branch — and it is `stopped` at a classified boundary, not ready** (see its closure record). The branch is not complete.

Parent roadmap:
- `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`

Related authority:
- `docs/adr/0005-crypto-dependency-selection.md`
- `docs/adr/0028-i2pcontrol-proposal-170-control-plane.md`
- `plans/closure/i2pcontrol-proposal-170/325-status.md`
- `plans/closure/i2pcontrol-proposal-170/326-status.md`
- `plans/implementation/floodfill/281-m12-encrypted-leaseset-floor-correction.md`

## 1. Purpose

Continue the blocked Proposal 170 encrypted-LeaseSet branch without copying or adapting the
Red25519/ELS2 implementation from the eggstack/emissary fork.

The implementation authority is:

1. normative I2P specifications;
2. independently written i2pr Rust code over maintained cryptographic primitives;
3. Java I2P and i2pd as readable implementation references for ambiguity/interoperability;
4. Emissary only as a post-implementation behavioral/differential oracle.

This line does not weaken the repository rule against locally implementing low-level elliptic-curve
arithmetic. i2pr may implement the I2P-specific Red25519 protocol construction by composing a
reviewed curve library, but scalar/field/point arithmetic, decompression, subgroup checks, and
constant-time curve operations remain dependency-owned.

## 2. Reference pins at registration

- I2P Red25519 specification: current public specification, re-frozen by Plan 329.
- I2P Encrypted LeaseSet specification: current public specification, re-frozen by Plan 329.
- Proposal 170: Open, revision 2026-05-20 at registration.
- Java I2P reference: `i2p/i2p.i2p@93eef5db87fae48025de00c0eb9b669e97b92149`.
- i2pd reference: `PurpleI2P/i2pd@2c694149fa6996eaeb23e378d5f83c9d3232c22f`.
- Emissary: behavioral oracle only; no Red25519/ELS2 production source may be copied, translated,
  adapted, or used as implementation text for Plans 330–334.

Plan 329 must re-freeze all of these before implementation.

## 3. Dependency graph

```text
329 provenance/reference boundary + normative algorithm freeze
  -> 330 independent Red25519 implementation over reviewed curve primitives
       -> 331 independent Red25519 qualification and external differential
            -> 332 type-5 Encrypted LeaseSet2 foundation + lookup-secret/B33
                 -> 333 PSK and DH client authorization
                      -> 334 passed  canonical Proposal 170 encrypted-LeaseSet mode mapping
                           -> 335 blocked historical measurement: Java+i2pd share the
                                deployed ELS2 type-11 transcript and reject i2pr's
                                Proposal-146 strict transcript
                                -> 346 passed  ELS2-only transcript/deployment corrective
                                     -> 347 stopped  live bidirectional Java+i2pd ELS2 qualification
                                          -> 350 passed  floodfill stores and serves type 5
                                               -> 351 ready  ELS2 consumer as a service-tunnel
                                                          remote target (three gates)
                                                       -> 349 re-close  consumer owner gains a
                                                                       production caller
351 -> 352 ready  config secret hygiene (parse echo + mode check);
                     independent of 351, live today for I2pControlPassword

337 passed  one shared ServiceTunnelManager (corrective pass on Plan 289)
  -> 338 passed  one owner for the service identity store + transaction rollback
```

Plan 347 is now the closure gate for the encrypted-LeaseSet/Red25519 branch, and Plan 346 passing
makes it `ready`. Historical Plan 322's
source gaps are closed by Plans 339/340. The outproxy branch still requires ready Plan 342 after
Plans 341/343 supplied its secret owner and provider. Plan 348 is the fresh final Proposal-170 gate
and waits on both Plan 342 and Plan 347 before any `full-proposal-conformant` claim.

## 4. Clean-room/reference boundary

### Normative inputs

Implementation may directly use:
- Red25519 specification equations and official vectors;
- Encrypted LeaseSet specification;
- common structures;
- Proposal 123/149 where normative;
- I2CP/NetDB specifications.

### Readable implementation references

Java I2P and i2pd may be inspected to:
- resolve ambiguous byte ordering/framing;
- compare edge behavior;
- understand interoperable lifecycle expectations;
- produce independent fixtures.

No line-by-line translation is permitted. The implementation plan must contain a prose/typed
algorithm derivation from the specification before production code begins.

### Emissary

The Emissary Red25519/ELS2 implementation is explicitly outside the reusable-source exception for
this branch. It may be invoked only after an i2pr implementation commit is frozen, and only for:
- deterministic output comparison;
- signature cross-verification;
- blinded address/storage-key comparison;
- encrypted LeaseSet publication/lookup interop.

Do not inspect Emissary Red25519/ELS2 source during Plans 330–334.

## 5. Cryptographic dependency policy

The current lockfile already contains `curve25519-dalek 4.1.3` transitively. Plan 330 may make it a
direct `i2pr-crypto` dependency after the ADR 0005 review is updated.

The curve dependency owns:
- scalar reduction/canonical decoding;
- Edwards point decoding/encoding;
- basepoint multiplication;
- point addition;
- subgroup/torsion checks.

i2pr owns only:
- I2P domain separation;
- transcript construction;
- alpha derivation;
- key re-randomization composition;
- Red25519 sign/verify composition;
- typed bounds/zeroization/error semantics.

No custom field/bignum/curve formulas are permitted.

## 6. Milestones

| Plan | State | Purpose |
|---|---|---|
| 329 | passed | provenance/reference correction and exact algorithm/vector freeze (closure: `plans/closure/i2pcontrol-proposal-170/329-status.md`) |
| 330 | passed | independent Rust Red25519 construction (closure: `plans/closure/i2pcontrol-proposal-170/330-status.md`) |
| 331 | passed | independent vectors + Java/i2pd + post-freeze Emissary differential (closure: `plans/closure/i2pcontrol-proposal-170/331-status.md`) |
| 332 | passed | type-5 ELS2 foundation: first-class DatabaseStore type 5, no-auth layer crypto, daily blinding, lookup secret, B33, NetDB store/serve, client publish/resolve (closure: `plans/closure/i2pcontrol-proposal-170/332-status.md`) |
| 333 | passed | PSK and DH/X25519 client authorization: both derivations, the bounded authorization block, constant-time recovery, and the four-role secret owner; byte-identical to Emissary in both directions after the `f525578` freeze (closure: `plans/closure/i2pcontrol-proposal-170/333-status.md`) |
| 334 | passed | exact Proposal 170 mode/field mapping; control plane complete, a control-created tunnel is on the publication path, a type-5 record is filed at the record's own blinded storage key, and a control-created encrypted server exposes a resolving `.b32.i2p` **on the JSON-RPC wire**. Black-box evidence (JSON-RPC round trip, rejected edit, restart, refused modes) landed in `db63bc0`; reclosed 2026-10-05 |
| 335 | blocked historical | Measured Java+i2pd mutual type-11 compatibility and i2pr strict-only incompatibility. The measurement remains valid; forward interpretation is superseded by Plan 346 because Proposal 146 and the Encrypted-LS2 specification define different transcripts. Closure: `plans/closure/i2pcontrol-proposal-170/335-status.md` |
| 337 | passed | corrective pass on Plan 289: the composition root now builds the one `ServiceTunnelManager` and injects the same `Arc` into the control state and the product, so a control-created tunnel is the same runtime and publishes through the existing sweep (ADR 0031, closure: `plans/closure/i2pcontrol-proposal-170/337-status.md`) |
| 338 | passed | corrective pass on Plans 289 and 334, found while implementing 337, and it **corrects Plan 337's own diagnosis**: every server group is already persistent, so a control-created server always had a persisted identity record — the ELS2 loader read `for_service` while the runtime wrote `for_group`. `ServiceTunnelManager` is now the single owner of that resolution, and `rollback_state` reconciles the shared manager as well as the mirror, so a failed transaction leaves no ghost runtime. Closure: `plans/closure/i2pcontrol-proposal-170/338-status.md` |
| 336 | passed historical | Spec-first remains authoritative for standalone Proposal-146 Red25519; Plan 346 supersedes only the ELS2 network transcript decision. Closure: `plans/closure/i2pcontrol-proposal-170/336-closure.md` |
| 346 | passed | ELS2-only transcript/deployment corrective (ADR 0032). The strict Proposal-146 primitive is byte-exact; `i2pr_crypto::red25519_deployed` is a separate composition over the same `curve25519-dalek` arithmetic; `i2pr_netdb::els2_transcript` owns a typed four-state profile with a fail-closed `Ambiguous`; `i2pr_proto::Els2SignedRegion` is the structural compensating control; no generic dual-transcript type-11 verifier exists. Deployed profile cross-verifies against executed Java I2P (`93eef5db`) and i2pd (`2c694149`) output in **both** directions. Closure: `plans/closure/i2pcontrol-proposal-170/346-status.md` |

## 7. Completion boundary

This branch is complete only when:
- the Red25519 primitive passes all official vectors and independent negative tests;
- Java and i2pd accept/produce compatible blinded keys and deployed ELS2 type-11 signatures; standalone Proposal-146 strict Red25519 remains independently qualified; the two transcript definitions are explicitly separated rather than conflated;
- Emissary black-box differential agrees after the implementation freeze (achieved in Plan 331: byte-identical alpha, blinded keys, storage keys, and signatures);
- DatabaseStore type 5 is a first-class validated/publishable/lookup record;
- B33/blinded address behavior is interoperable;
- lookup-secret, PSK and DH client authorization work end-to-end (achieved in Plans 332 and 333:
  lookup secret in the foundation, both authorization schemes byte-identical to Emissary in both
  directions after the `f525578` freeze);
- every Proposal 170 encrypted-LeaseSet control mode has a real owner or a spec-grounded explicit
  disposition;
- external publication/lookup succeeds against at least one independent router implementation;
- no Emissary production source is incorporated.



## 8. Specification/deployment correction (Plans 346–347)

### Historical finding preserved

Plan 335's executable result is not discarded:

- Java and i2pd verify each other's type-11 signatures.
- both reject i2pr's former ELS2 signature;
- i2pr rejects theirs;
- blinded public keys agree.

What changes is the conclusion drawn from it.

The original Proposal-146 text already used the domain-separated, length-framed HStar construction.
The Encrypted LeaseSet specification separately describes the deployed randomized RedDSA
construction, and Java/i2pd implement that ELS2 form. This has existed since the initial 2019
deployment era.

### Forward rule

Plan 346 keeps the generic Proposal-146 Red25519 primitive strict and adds any compatibility
semantics only at the typed ELS2 type-5 boundary. **Delivered** (ADR 0032): the deployed
transcript is a separate composition, reachable only from the ELS2 type-5 verifier, with a
typed signed region and no generic dual-transcript verifier.

Plan 347 must then prove the real cross-router lifecycle, not only signature
cross-verification. **It stopped at a classified boundary rather than passing**: the signature
stage is closed, but the matrix needs an i2pr-side consumer path (closed by Plan 351) and the
reference-side drivers, which did not exist. The Java blocker 347 recorded as needing a
bandwidth-tier design was **not** that — it is a tunnel-peering gate, and neither Java row needs
Java to peer with i2pr; ADR 0030 stands untouched. **Plan 351 has since passed, and it was necessary but
not sufficient.** The branch's remaining work is external evidence: a reference-side ELS2 driver
per reference family, which Plans 374 (i2pd) and 375 (Java) own with convergence in Plan 377. Plan 406 has completed Plan 374's i2pd remainder through the frozen NONE/PSK/DH matrix; Plan 375 remains independently blocked;
the Java directions were never about i2pr. The
consumer work was split in two, and the split is measured rather than cosmetic: **Plan 350**
delivered the server half (the floodfill both stores and serves type 5), **Plan 351** delivered
the client half (a b33 address as a service-tunnel remote target, resolving through the real
lookup transport), and **Plan 352 has passed**, closing a pre-existing config-secret leak
that Plan 351's own design was forced to route around.

Plan 351 closed as `passed-gate-scoped-els2-consumer-service-wiring-landed` (ADR 0033) with two
plan premises corrected at the plan that corrected them rather than absorbed: the unblinded
destination hash is **not** derivable from a `.b33` — the address carries the unblinded *signing*
key and lacks the ECIES key, certificate, and padding — so the install key comes from the inner
record under a signature binding; and the address↔record key relationship holds for the **type-7**
the publisher emits and does not generalize to type 11. Its acceptance criterion 4 (failure
isolation through the real composition) is **partially met only** — proven at the configuration and
status-surface level, not through a live multi-service composition — and is recorded as a
limitation rather than counted. A signature
harness — even one that cross-verifies against executed Java I2P and i2pd output in both
directions, which Plan 346's is — is not sufficient: Plan 347 must show a real
`DatabaseStore` publication, an independent-router NetDB store and lookup, layer-1/layer-2
decrypt, inner LeaseSet2 validation, and an application payload round-trip, in all four
i2pr↔reference directions.

| Plan | State | Purpose |
|---|---|---|
| 346 | **passed** | ELS2 type-11 transcript authority + deployed Java/i2pd compatibility corrective. Delivered as ADR 0032. The cryptographic boundary is closed in both directions; the strict primitive is unchanged and still byte-exact. |
| 347 | **stopped** (0 of 4 directions) | Bidirectional Java/i2pd type-5 publication, lookup, decrypt, inner-LS2 and streaming/application qualification. Closed with a stage-classified boundary: the **signature stage is closed** by Plan 346, and the pinned i2pd 2.61.0 `SignRedDSA` source confirms the deployed transcript. Blocked on (a) i2pr having **no type-5 consumer path** — `EncryptedLeaseSet2Resolver` has zero production callers, so `i2pd → i2pr` is structurally unreachable; and (b) the reference-side ELS2 drivers, which do not exist yet. **The Java caps boundary recorded here has since been corrected**: it is a *tunnel-peering* gate (`TunnelPeerSelector.shouldExclude` caps arity plus `allowAsIBGW`'s `R` requirement), **not** a bandwidth-tier gate, and neither Java row of the matrix needs Java to peer with i2pr — the existing `run-java-floodfill.sh` topology has Java reach i2pr purely as a queried floodfill, already proven by Plans 303/306. ADR 0030 is untouched and no tier letter is needed. Closure: `plans/closure/i2pcontrol-proposal-170/347-status.md` |
| 350 | **passed** | Corrective from 347's boundary, and the cheapest one — but **two** failures, not one. The floodfill **refused** type 5 outright in `handle` (a deliberate hold-back from `53a404b` whose rationale expired with Plan 346), **and** omitted it from every `DatabaseLookup` candidate list, so a reference consumer's blinded-key lookup always missed. Both fixed, with named `SERVABLE_*` constants and `scripts/check-floodfill-type5-serve.sh` to prevent recurrence. Closure: `plans/closure/i2pcontrol-proposal-170/350-status.md` |
| 349 | **superseded by 351** | Corrective from 347's boundary: the i2pr ELS2 **consumer** lookup path — address → secret → daily blinded key → storage key → `DatabaseLookup` over the existing key-agnostic composer → `ValidatedEncryptedLeaseSet2` → layer decrypt → inner `LeaseSet2` → service. Makes `i2pd → i2pr` reachable and `i2pr → i2pd` executable. Does **not** attempt the Java bandwidth-tier design. **Superseded, not in progress.** Plan 349 landed the bounded `EncryptedServiceResolver` owner with all 17 rows green but no production caller, because a service-tunnel destination is a raw 32-byte `DestinationId`, so pointing one at a b33 address needs new config surface, secret storage, and tunnel-lifecycle integration. **Plan 351 supplied exactly that missing caller and passed**, which is what supersedes this plan. `EncryptedLeaseSet2Resolver` now has a production caller. Closure: `plans/closure/i2pcontrol-proposal-170/349-status.md` (`superseded-by-plan351-with-caller-landed`) |
| 351 | **passed** | Corrective on **349**, not on 347. Plan 349 cannot close as written: its Out-of-scope line forbids "new daemon configuration surface, I2PControl options" while its acceptance criterion 1 requires a production owner reaching an inner `LeaseSet2` a service can use, and its closure record then names configuration surface as remaining work item 1. Plan 351 re-scopes that explicitly and supplies the missing caller for both `EncryptedServiceResolver` and `i2pr_client::EncryptedLeaseSet2Resolver`. Three measured gates: **(1)** an encrypted remote target is permitted **only** on a `DelayOpen` client, because a failed remote lookup is fatal to the whole service-tunnel product today (two of three callers of `provision_all_service_router_material` run `manager.shutdown()` + `token.cancel` + `ssu2_handle.shutdown()`) while per-destination isolation already exists *and is tested* for deferred groups; **(2)** the surface is I2PControl, not TOML, because `delay_open` is unsettable from TOML — a TOML `encrypted_destination` field would be dead on arrival under Gate 1 — and because the publisher-side lookup secret already lives in the control definition's options map (`leaseset_password`), making this symmetric rather than a second secret channel; **(3)** no inline config secret and no daily-rollover claim. The measured blocker is deeper than wiring: `netdb_seam.rs:328-332` re-derives the lookup identity from a `DestinationHash` and `lookup_engine.rs:606-613` refuses anything but type 3, so a blinded key cannot be requested at all. **No new `LookupKind` is needed** — `LookupKind::LeaseSet2.wire_code() == 1` is exactly the lookup type a reference client issues for a blinded resolve (`floodfill_service.rs:372-375`) — and `RouterHash::from_hash` is `pub const` and does not re-hash, so `BlindedStorageKey` → `RouterHash` is lossless. The scoped netdb delta is one `LookupResult` variant plus one type-5 arm that delegates all ELS2 policy upward, so ADR 0032 stays in the owner. Two new guards delivered and **negative-tested**: 18/18 and 17/17 deliberate breaks detected. **Two premises were wrong and are recorded as findings, not absorbed:** (a) the unblinded hash is **not** derivable from the address, so the install key is the **inner record's own** hash gated on it signing with the unblinded public key the `.b33` names; (b) that address↔record key relationship holds for **type-7** only and does not generalize to type 11. Criterion 5 is still satisfied — the delivered key is the unblinded hash — and a row asserts the premise before relying on it. **Criterion 4 is partially met only** and recorded as a limitation. Also fixed: the silent `_ => {}` fallthrough in `record_observation` (now counted), and a pre-existing port flake in `tests/i2pcontrol_tunnels.rs` classified, not fixed, because it reproduces at base. Closure: `plans/closure/i2pcontrol-proposal-170/351-status.md` |
| 352 | **passed** | Config secret hygiene. Closure: `plans/closure/i2pcontrol-proposal-170/352-status.md` (`passed-structural-toml-error-redaction-and-conditional-at-rest-mode-gate`) — `RedactedTomlError` resolves the span to a line/column pair and retains no content, and a conditional `& 0o077` gate in `Config::load` refuses a password-bearing group/world-readable config (a deliberate Windows fail-closed change, factored into a platform-independent verdict function so the non-POSIX refusal is testable on a POSIX host). Plan: `plans/implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md`. | **Pre-existing and live today**, not caused by any ELS2 work, and filed separately so Plan 351's scope stays honest. `toml-1.1.6/src/de/error.rs:138` prints the **entire offending source line** on a syntax error and `serde-1.0.228/src/core/de/mod.rs:410` prints the **value** on a type mismatch; both reach `eprintln!("error: {error}")` at `main.rs:51` through `ConfigError::Parse` (`config.rs:2853`) and the transparent `DaemonError::Config` (`error.rs:70-71`), so a secret on a malformed line is printed to the terminal — and the likely operator response to a typo is to paste that output into a bug report. `I2pControlPassword` is exposed now. Separately `Config::load` (`config.rs:1485-1491`) is a bare `fs::read_to_string` with **no mode check**, making the config the only secret-bearing file in the daemon without the `& 0o077` gate that `i2pr-storage:1083`, `i2pcontrol_tunnels.rs:1022-1023`, and `addressbook.rs:808,879` all enforce. `ConfigError::Semantic { field: &'static str, reason: &'static str }` is already leak-proof and is the shape new rejections must use. Residual risk recorded rather than hidden: `Config` is `Clone + Debug` (`config.rs:1399`) and `CommandOutcome` derives `Debug + PartialEq` while embedding it (`lib.rs:76-84`), so any future `assert_eq!` would dump the whole config; removing those derives is out of scope and named as a follow-on. No dependency in either direction with Plan 351. Plan: `plans/implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md` |
| 380 | **passed** | Closes the Plan 351 deferral: the i2pr ELS2 **consumer** can now present the PSK or DH key a `.b33` publisher authorised it to use, so `EncryptedServiceResolver::begin_authorized` has a production caller and Plan 374's authorization rows are writable. Adds `EncryptedTargetCredential` (owned, zeroizing, non-`Clone`, non-`Debug`, non-`Display`, non-`serde`; the DH form derives its public key from the private one) and `SealedEncryptedTargetCredential` (ciphertext + the owner that opens it), under a **second, domain-separated** sealed store — a distinct HKDF label and a distinct marker used as AEAD associated data — so a stored outproxy credential cannot be copied into the credential slot. The manager's install signature takes the sealed type, so "the manager never holds the key" is structural. **The ingress was a real boundary and was escalated rather than decided in code:** Proposal 170 defines no consumer-credential field, `proposal_tunnel_value_type` gates every top-level field on the frozen 75-name inventory, and adding a row there would have made the audited conformance matrix a false statement. The chosen answer is a **typed i2pr extension seam** on Proposal 170's `CustomOptions` field — one namespace key, a closed allowlist, string-only values, the untyped blob form still refused for the reason it always was, an unknown extension name refused rather than ignored. Three new `EncryptedTargetStatus` arms replace a mapping that reported a missing credential as `StorageKeyUnavailable`. **Two defects found and fixed:** `edit` merges options, so both seal steps re-ran over the value they already held — for the ELS2 credential that failed every edit outright, and for the **pre-existing** Plan 342 outproxy credential it silently sealed twice, so opening it once returned the previous stored form as text and the router presented that as the proxy password. No Plan 342 or Plan 376 row had ever edited an outproxy tunnel; Plan 342's record is not rewritten, this is the forward correction. No advertisement change, no support surface added, `PROPOSAL_TUNNEL_MANAGER_FIELDS` still 75. Closure: `plans/closure/i2pcontrol-proposal-170/380-status.md` (`passed-authorized-consumer-production-path-closed-with-two-defects-found-and-fixed`) |

The Emissary source quarantine remains unchanged. Emissary may continue to serve as a post-freeze
black-box strict-profile oracle, but Java+i2pd deployment interoperability is the external network
closure criterion, now owned by Plans 374/375 with convergence in Plan 377.


## 9. Reference-specific live qualification successors (Plans 373–377)

Proposal 170/347 remains the stopped historical four-direction attempt. Its local i2pr blockers
were subsequently closed by Proposal 170/350 (type-5 floodfill store/serve) and Proposal 170/351
(production B33 consumer). The remaining ELS2 work is external evidence.

After the authority/support reconciliation in Plan 373:

```text
374  stock i2pd:  i2pr -> i2pd, i2pd -> i2pr
375  stock Java:  i2pr -> Java, Java -> i2pr
  \               /
   \             /
      377 external ELS2 convergence
```

Each reference plan must prove a real DatabaseStore/DatabaseLookup/decrypt/inner-LS2/application
trajectory. Neither crypto fixtures nor local decoded-record injection count.

Plan 375 explicitly uses Java as a requester/publisher through a controlled i2pr floodfill and its
own Java relay peers. It must not invent a bandwidth tier and does not require Java to select i2pr
as a tunnel peer.

Plan 377 closes the branch only when all four mandatory directions pass and records the exact auth
mode overlap per reference. It may promote the exact ELS2 subset in support metadata, but it cannot
claim whole-Proposal conformance; parent-roadmap Plan 378 owns that.

| Plan | State | Purpose |
|---|---|---|
| 373 | **passed** | reconcile support/planning truth before external qualification. Closure: `plans/closure/i2pcontrol-proposal-170/373-status.md`. No capability promotion, zero production change; its unblock audit moved 374/375/376 to `ready`. |
| 374 | **superseded by Plan 406** | Original blocked record retained at `plans/closure/i2pcontrol-proposal-170/374-status.md`; its i2pd scope is delivered by Plan 406. |
| 411 | **passed** | Fresh one-attempt Java SAM session-create diagnosis. Source trace and live response attribute `Address already in use` to the occupied default `127.0.0.1:7655` DATAGRAM listener bind, before I2CP session construction. Closure: `plans/closure/i2pcontrol-proposal-170/411-status.md`. |
| 412 | **active** | Stock Java no-auth requester direction: real type-5 lookup/decrypt and application payload from an i2pr ELS2 publisher through controlled F. Handoff: `plans/implementation/i2pcontrol-proposal-170/412-java-els2-noauth-requester-direction.md`. |
| 381 | **passed** (`passed-i2pd-consumer-direction-live-reverse-and-authority-parked-for-successor`; merged `f791263c` + `c223001f`, CI `37781167269` green) | Live ELS2 external driver lane, i2pd direction — Plan 374's remaining scope, with 380 as its closed hard dependency. Plan: `plans/implementation/i2pcontrol-proposal-170/381-els2-live-external-driver-lane.md`. WP1 cheap gate (no i2pd): `tunnels.conf` writer/validator, b33 extractor with independent recompute, R-side `.b33` service creation over loopback I2PControl. Stop conditions 1 and 3 resolved by WP1 (both PSK spellings accepted — prefix-match reader — with the real trap being the mandatory `:`; pinned source tree present at the pin), stop condition 2 retired by WP2 (stock i2pd client carried a payload through the mesh). WP2 landed the lane runner (8 rows green, 11 self-test rows) and seven reference facts plus a self-found validator defect fixed with 21 new rows. **WP3–WP5 executed 2026-10-08:** payload row passes (NONE, i2pd→i2pr) after an 8-defect chain fixed forward; WP4 consumer matrix green live (PSK + DH rows, two live negatives, different-day unit, mesh authority control); WP5 landed the evidence checker plus packaged `evidence.json`/`evidence.md`. Reverse direction and i2pr-side authority parked for a named successor. Closure: `plans/closure/i2pcontrol-proposal-170/381-status.md`. |
| 384 | **superseded by Plan 406; scope delivered** | Original blocked snapshot remains at `plans/closure/i2pcontrol-proposal-170/384-status.md`. Plan 406 completed the reverse NONE/PSK/DH and post-start authority rows; 375 remains independently blocked, so 377/378 remain blocked. |
| 388 | **blocked** (`blocked-post-start-inbound-builds-stall-before-response-plan-389`) | Fixed Plan 387's outbound-first replenishment starvation and exposed bounded destination profile/build counters. A clean post-fix NONE run submitted two inbound builds but still reached no registrations or LS2; product-wide outcomes could not attribute replies to the new server. Closure: `plans/closure/i2pcontrol-proposal-170/388-status.md`. |
| 389 | **blocked** (`blocked-destination-role-registry-capacity-plan-390`) | Destination-scoped counters localized 61 inbound role activation failures: the coordinator registry is sized from exploratory pool maxima rather than aggregate Destination roles. Closure: `plans/closure/i2pcontrol-proposal-170/389-status.md`. |
| 390 | **blocked** (`blocked-post-registration-inbound-owner-installation-plan-391`) | Finite aggregate role capacity fixed activation failures (33 inbound, 3 outbound activations), but post-start pool snapshot still has zero retained inbound registrations/LS2. Closure: `plans/closure/i2pcontrol-proposal-170/390-status.md`. |
| 391 | **blocked** (`blocked-bridge-inbound-projection-rejection-plan-392`) | Exact-pinned counters show all 40 inbound owner registrations reached bridge append but were rejected. Closure: `plans/closure/i2pcontrol-proposal-170/391-status.md`. |
| 392 | **blocked** (`blocked-publication-coordinator-retry-leak-plan-393`) | Staged router state fixed bridge projection; exact-pinned snapshot now has usable leases and LS2 but publication coordinator retries fail at begin. Closure: `plans/closure/i2pcontrol-proposal-170/392-status.md`. |
| 393 | **blocked** (`blocked-publication-begin-rejection-cause-plan-394`) | Added post-begin cancellation and max+1/capacity-release coverage, but exact-pinned publication still fails at begin. Closure: `plans/closure/i2pcontrol-proposal-170/393-status.md`. |
| 394 | **blocked** (`blocked-encrypted-publication-record-rejection-plan-395`) | Bounded counts localized the rejection to invalid record; encrypted services submit type 5 but coordinator admits only Standard LS2. Closure: `plans/closure/i2pcontrol-proposal-170/394-status.md`. |
| 395 | **blocked** (`blocked-post-admission-reverse-els2-delivery-plan-396`) | Type-5 admission now passes, but the consumer receives no reverse payload after local DatabaseStore admission. Closure: `plans/closure/i2pcontrol-proposal-170/395-status.md`. |
| 396 | **blocked** (`blocked-inbound-receive-owner-transition-plan-397`) | Type-5 admission and client connect pass, but no fixture payload returns; cumulative orphan count needs transaction-scoped attribution. Closure: `plans/closure/i2pcontrol-proposal-170/396-status.md`. |
| 397 | **blocked** (`blocked-no-inbound-streaming-arrives-after-owner-fix-plan-398`) | The owner mismatch is fixed and orphan delta is zero, but no server SYN or target dial arrives. Closure: `plans/closure/i2pcontrol-proposal-170/397-status.md`. |
| 398 | **blocked** (`blocked-reverse-garlic-binding-rejection-plan-399`) | Healthy pinned ingress reaches the correct service owner, but all seven complete Garlic envelopes are rejected in the typed binding family before queueing. Closure: `plans/closure/i2pcontrol-proposal-170/398-status.md`. |
| 399 | **blocked** (`blocked-default-i2pd-signature-type-outside-selected-profile-plan-400`) | Reverse i2pd sender LeaseSet2 uses unsupported signature type 0; ADR 0004 selects type 7. Closure: `plans/closure/i2pcontrol-proposal-170/399-status.md`. |
| 400 | **blocked** (`blocked-reverse-authorized-els2-lookup-fails-after-type7-plan-401`) | Reverse NONE passes under explicit SAM signature type 7, but PSK is admitted locally and the stock requester reports `LeaseSet not found` before Garlic ingress. Closure: `plans/closure/i2pcontrol-proposal-170/400-status.md`. |
| 401 | **active** (`in-progress-reverse-authorized-els2-lookup-attribution`) | Attribute the post-admission PSK lookup failure using exact-pin i2pd source and bounded runtime evidence; run DH only if controls are healthy and the attempt is diagnostic. Plan: `plans/implementation/i2pcontrol-proposal-170/401-reverse-authorized-els2-lookup-attribution.md`. |
| 401 | **blocked** (`blocked-reverse-authorized-client-keys-not-loaded-plan-402`) | Pinned i2pd loads requester PSK/DH keys only when `i2cp.leaseSetType=5`; Plan 400 omitted the property. Closure: `plans/closure/i2pcontrol-proposal-170/401-status.md`. |
| 402 | **active** (`in-progress-reverse-authorized-requester-key-loading-corrective`) | Add and guard the mode-coupled LS2 type option, then qualify reverse PSK and DH with healthy controls. Plan: `plans/implementation/i2pcontrol-proposal-170/402-reverse-authorized-requester-key-loading-corrective.md`. |
| 402 | **blocked** (`blocked-reverse-requester-uses-publication-auth-options-plan-403`) | Plan 402 added local publisher auth options; the reverse consumer requires `i2cp.leaseSetPrivKey`. PSK still fails with `LeaseSet not found`. Closure: `plans/closure/i2pcontrol-proposal-170/402-status.md`. |
| 403 | **active** (`in-progress-reverse-els2-consumer-secret-option-corrective`) | Supply the actual PSK/DH consumer secret via the pinned i2pd option, remove publisher-only options, and rerun both modes. Plan: `plans/implementation/i2pcontrol-proposal-170/403-reverse-els2-consumer-secret-option-corrective.md`. |
| 403 | **blocked** (`blocked-authorized-dh-consumer-stops-before-lookup-plan-404`) | Correct consumer secret option; PSK reverse payload passes. DH's i2pr-to-reference consumer path fails before target resolution/lookup. Closure: `plans/closure/i2pcontrol-proposal-170/403-status.md`. |
| 404 | **active** (`in-progress-authorized-dh-consumer-activation-attribution`) | Attribute the earliest failing i2pr DH consumer transition with bounded evidence before behavior changes. Plan: `plans/implementation/i2pcontrol-proposal-170/404-authorized-dh-consumer-activation-attribution.md`. |
| 404 | **retained** (`retained-authorized-dh-failure-not-reproduced-on-following-pinned-run`) | A first DH attempt failed without resolver status and coincided with a reference process crash; a subsequent exact-pin run passed all rows. Retained anomaly and evidence: `plans/closure/i2pcontrol-proposal-170/404-status.md`. |
| 405 | **active** (`in-progress-final-i2pd-els2-reverse-and-authority-qualification`) | Final NONE/PSK/DH exact-pinned runs and full routine floor for Plan 384 completion. Plan: `plans/implementation/i2pcontrol-proposal-170/405-final-i2pd-els2-reverse-and-authority-qualification.md`. |
| 405 | **blocked** (`blocked-standard-authority-lookup-succeeds-but-payload-does-not-plan-406`) | Final healthy NONE lookup succeeded but the authority payload did not; no stream-connect start or failed-connect count was observed. Closure: `plans/closure/i2pcontrol-proposal-170/405-status.md`. |
| 406 | **passed** (`passed-healthy-i2pd-authority-and-none-psk-dh-matrix-plan384-scope-delivered`) | Healthy-mesh authority and reverse payload matrix passed for NONE/PSK/DH; peer health and active-connection evidence are recorded; complete AGENTS.md routine floor passed. Closure: `plans/closure/i2pcontrol-proposal-170/406-status.md`. |
| 380 | **passed** | i2pr ELS2 **authorized** consumer production path (PSK/DH). Closure: `plans/closure/i2pcontrol-proposal-170/380-status.md`. Closed the deferral Plan 351 made on purpose — the only production-code gap behind 374/375's block. Local-only by design, and it closed without a reference router. Escalated its ingress decision rather than routing around the frozen Proposal inventory: the credential has no Proposal 170 field, so it now arrives through a typed i2pr extension seam on `CustomOptions` with the frozen inventory untouched at 75. Also fixed a **pre-existing** defect in which `edit` re-sealed the Plan 342 outproxy credential. Does **not** unblock 374, whose blocker is the external driver (Plan 381). |
| 375 | **blocked** | stock Java I2P bidirectional live ELS2 qualification. Closure: `plans/closure/i2pcontrol-proposal-170/375-status.md`. Build verified at the pin (JDK 21 required); source proof incomplete; same blocker. |
| 377 | blocked on 374 + 375 | converge all four directions and close historical 326/347 forward blocker |


Plan 406 completed the i2pd live qualification remainder. The final exact-pinned NONE, PSK, and DH rows all pass, including the ordinary post-start authority payload, with reference processes healthy before and after each driver run. Plan 404's first DH failure and Plan 405's failed authority attempts remain preserved as unresolved, unreproduced anomalies; they are not erased or treated as fresh failures. Plan 411 attributed the former Java setup error: stock SAM's DATAGRAM branch binds `127.0.0.1:7655` before constructing an I2CP session, and that port was occupied on the diagnostic host. Plan 412 now owns the Java NONE requester direction with an explicit loopback UDP port. Plan 375 remains blocked until both Java directions and its required auth/negative/restart matrix pass. Plans 377 and 378 remain blocked. No type-5, ELS2, or full-Proposal-170 support or advertisement promotion follows from the scoped i2pd rows.
