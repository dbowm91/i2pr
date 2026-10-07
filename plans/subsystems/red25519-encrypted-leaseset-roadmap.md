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
per reference family, which Plans 374 (i2pd) and 375 (Java) now own with convergence in Plan 377;
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
| 352 | **passed** | Config secret hygiene. Closure: `plans/closure/i2pcontrol-proposal-170/352-status.md` (`passed-structural-toml-error-redaction-and-conditional-at-rest-mode-gate`) — `RedactedTomlError` resolves the span to a line/column pair and retains no content, and a conditional `& 0o077` gate in `Config::load` refuses a password-bearing group/world-readable config (a deliberate Windows fail-closed change, factored into a platform-independent verdict function so the non-POSIX refusal is testable on a POSIX host). Plan: `plans/implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md`. |
| 380 | **passed** | Closes the Plan 351 deferral: the i2pr ELS2 **consumer** can now present the PSK or DH key a `.b33` publisher authorised it to use, so `EncryptedServiceResolver::begin_authorized` has a production caller and Plan 374's authorization rows are writable. Adds `EncryptedTargetCredential` (owned, zeroizing, non-`Clone`, non-`Debug`, non-`Display`, non-`serde`; the DH form derives its public key from the private one) and `SealedEncryptedTargetCredential` (ciphertext + the owner that opens it), under a **second, domain-separated** sealed store — a distinct HKDF label and a distinct marker used as AEAD associated data — so a stored outproxy credential cannot be copied into the credential slot. The manager's install signature takes the sealed type, so "the manager never holds the key" is structural. **The ingress was a real boundary and was escalated rather than decided in code:** Proposal 170 defines no consumer-credential field, `proposal_tunnel_value_type` gates every top-level field on the frozen 75-name inventory, and adding a row there would have made the audited conformance matrix a false statement. The chosen answer is a **typed i2pr extension seam** on Proposal 170's `CustomOptions` field — one namespace key, a closed allowlist, string-only values, the untyped blob form still refused for the reason it always was, an unknown extension name refused rather than ignored. Three new `EncryptedTargetStatus` arms replace a mapping that reported a missing credential as `StorageKeyUnavailable`. **Two defects found and fixed:** `edit` merges options, so both seal steps re-ran over the value they already held — for the ELS2 credential that failed every edit outright, and for the **pre-existing** Plan 342 outproxy credential it silently sealed twice, so opening it once returned the previous stored form as text and the router presented that as the proxy password. No Plan 342 or Plan 376 row had ever edited an outproxy tunnel; Plan 342's record is not rewritten, this is the forward correction. No advertisement change, no support surface added, `PROPOSAL_TUNNEL_MANAGER_FIELDS` still 75. Closure: `plans/closure/i2pcontrol-proposal-170/380-status.md` (`passed-authorized-consumer-production-path-closed-with-two-defects-found-and-fixed`) | **Pre-existing and live today**, not caused by any ELS2 work, and filed separately so Plan 351's scope stays honest. `toml-1.1.6/src/de/error.rs:138` prints the **entire offending source line** on a syntax error and `serde-1.0.228/src/core/de/mod.rs:410` prints the **value** on a type mismatch; both reach `eprintln!("error: {error}")` at `main.rs:51` through `ConfigError::Parse` (`config.rs:2853`) and the transparent `DaemonError::Config` (`error.rs:70-71`), so a secret on a malformed line is printed to the terminal — and the likely operator response to a typo is to paste that output into a bug report. `I2pControlPassword` is exposed now. Separately `Config::load` (`config.rs:1485-1491`) is a bare `fs::read_to_string` with **no mode check**, making the config the only secret-bearing file in the daemon without the `& 0o077` gate that `i2pr-storage:1083`, `i2pcontrol_tunnels.rs:1022-1023`, and `addressbook.rs:808,879` all enforce. `ConfigError::Semantic { field: &'static str, reason: &'static str }` is already leak-proof and is the shape new rejections must use. Residual risk recorded rather than hidden: `Config` is `Clone + Debug` (`config.rs:1399`) and `CommandOutcome` derives `Debug + PartialEq` while embedding it (`lib.rs:76-84`), so any future `assert_eq!` would dump the whole config; removing those derives is out of scope and named as a follow-on. No dependency in either direction with Plan 351. Plan: `plans/implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md` |

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
| 374 | **blocked** | stock i2pd bidirectional live ELS2 qualification. Closure: `plans/closure/i2pcontrol-proposal-170/374-status.md`. Freeze executed; controlled mesh verified running; both directions reference-feasible; all three auth modes implemented. Blocker: no ELS2 live driver exists. |
| 381 | **in progress** (WP1 done) | Live ELS2 external driver lane, i2pd direction — Plan 374's remaining scope, with 380 as its closed hard dependency. Plan: `plans/implementation/i2pcontrol-proposal-170/381-els2-live-external-driver-lane.md`. Work packages ordered so the cheap gate (a `tunnels.conf` writer/validator, a b33 extractor with an independent recompute, the static checker with its self-test and mutation table, and R-side `.b33` service creation over loopback I2PControl) is provable with **no i2pd process at all**, before the lane runner and driver are attempted. **WP1 has executed** and this registration's PSK-key claim was wrong in the direction of over-warning: both the bare and the indexed spelling are accepted (prefix-match reader), and the actual trap is that the value must carry a `:` or i2pd drops it silently. Stop conditions 1 and 3 are resolved; stop condition 2 (client tunnels in the controlled mesh) is untested and is the one remaining unknown that can block. **Not passed** — no live-mesh row exists, and the auth-mode matrix actually executed is **none of the three**. Closure: `plans/closure/i2pcontrol-proposal-170/381-status.md`. |
| 380 | **passed** | i2pr ELS2 **authorized** consumer production path (PSK/DH). Closure: `plans/closure/i2pcontrol-proposal-170/380-status.md`. Closed the deferral Plan 351 made on purpose — the only production-code gap behind 374/375's block. Local-only by design, and it closed without a reference router. Escalated its ingress decision rather than routing around the frozen Proposal inventory: the credential has no Proposal 170 field, so it now arrives through a typed i2pr extension seam on `CustomOptions` with the frozen inventory untouched at 75. Also fixed a **pre-existing** defect in which `edit` re-sealed the Plan 342 outproxy credential. Does **not** unblock 374, whose blocker is the external driver (Plan 381). |
| 375 | **blocked** | stock Java I2P bidirectional live ELS2 qualification. Closure: `plans/closure/i2pcontrol-proposal-170/375-status.md`. Build verified at the pin (JDK 21 required); source proof incomplete; same blocker. |
| 377 | blocked on 374 + 375 | converge all four directions and close historical 326/347 forward blocker |
