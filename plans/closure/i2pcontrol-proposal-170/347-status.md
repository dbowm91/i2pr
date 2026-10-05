# Plan 347 — Live bidirectional Encrypted LeaseSet2 cross-router qualification: status

Status: **stopped-p347-classified-boundary-java-caps-tier-and-i2pr-missing-type5-consumer-path; i2pd-direction-unblocked-for-plan349**

Plan of record:
[`347-live-bidirectional-els2-cross-router-qualification.md`](../../implementation/i2pcontrol-proposal-170/347-live-bidirectional-els2-cross-router-qualification.md).

**Plan 347 did not pass.** It is closed as stopped with an exact, stage-classified boundary. This
record preserves the boundary rather than narrowing the acceptance criteria to whatever was
reachable. One positive result was obtained and is recorded below; it does not substitute for
the four-direction matrix the plan requires.

## What was executed

Plan 347's own "On failure" clause requires the exact boundary to be preserved and classified
by stage. This record does that, and it adds the one piece of executable evidence the
environment allowed: a **reference freeze plus pinned-source verification of the reference-side
capability Plan 347's rows assume**.

### Reference freeze (executed)

| Item | Value |
|---|---|
| i2pr SHA under test | this branch, `8386d22` implementation head (see "Commit" below) |
| i2pd repository | `PurpleI2P/i2pd` |
| i2pd pin | `635b013a612ff47278ef02acf8580a28e10e26c5` |
| i2pd version (self-reported) | `i2pd version 2.61.0 (0.9.70)`, Boost 1.83.0 |
| i2pd build | built from a clean clone at the pin, **unmodified**, `make i2pd`; binary at `/tmp/p347-i2pd/i2pd` |
| i2pd build deps | OpenSSL 3.0.13, Boost 1.83.0, zmq 4.3.5, g++ 13.3.0 (all system packages; no reference source patched) |
| Java I2P pin | `9134f808337b401e8e53c73734c81fab04280c9d` (2.13.0) — **not built; see the boundary** |
| Proposal 146 / Red25519 spec | as frozen by Plan 329; unchanged by Plan 346 |
| Encrypted LeaseSet spec | as frozen by Plan 329 |
| Plan-346 ADR / profile | ADR 0032, `Els2Type11Profile`, commit `8386d22` |

No reference was patched, vendored into i2pr, or configured to change protocol behaviour. The
`Cargo.lock` hash is unchanged by Plan 346 (no dependency was added).

### Pinned-source verification of the reference-side capability (executed, positive)

Plan 347's matrix assumes each reference can store, look up, and decrypt a type-5 record. That
assumption had never been established from pinned source. It now is, for i2pd, and it is
**positive** — i2pd 2.61.0 at the pin implements the full type-5 consumer surface:

| Required reference capability | Pinned i2pd evidence |
|---|---|
| Type-5 store type constant | `libi2pd/LeaseSet.h:146` — `NETDB_STORE_TYPE_ENCRYPTED_LEASESET2 = 5` |
| Publish an encrypted LeaseSet2 | `libi2pd/LeaseSet.h:296-302` — `LocalEncryptedLeaseSet2`; `LeaseSet.cpp:291-308` |
| **Store/parse an inbound type-5 record** | `libi2pd/Destination.cpp:491` — `case i2p::data::NETDB_STORE_TYPE_ENCRYPTED_LEASESET2: // 5` |
| Blinded-key `DatabaseLookup` | `libi2pd/Destination.h:155-157` — `RequestDestinationWithEncryptedLeaseSet (BlindedPublicKey)`; `Destination.h:202` — `RequestLeaseSet (..., std::shared_ptr<BlindedPublicKey> requestedBlindedKey = nullptr)`; `Destination.h:129` — `requestedBlindedKey` |
| Blinding / alpha | `libi2pd/Ed25519.cpp:470` — `BlindPublicKey`; `:485` — `BlindPrivateKey` |
| The deployed type-11 transcript | `libi2pd/Signature.h:577-609` — `RedDSA25519Verifier`, `RedDSA25519Signer`, `CreateRedDSA25519RandomKeys`; `libi2pd/Ed25519.cpp:169` — `SignRedDSA` |
| Streaming to an encrypted service | `libi2pd/Destination.h:269-274` — `CreateStream (..., BlindedPublicKey, port)`, `SendPing` |

**Transcript cross-verification against the pin, independent of the earlier fixture.** Reading
`libi2pd/Ed25519.cpp:169-200` at pin `635b013a`, `SignRedDSA` computes:

```text
T = 80 random bytes
r = SHA-512(T || publicKeyEncoded(32) || data) mod L ;  R = [r]B
c = SHA-512(R(32) || publicKey(32) || data) mod L     ;  S = (r + c·a) mod L
```

This is the deployed transcript Plan 346 implemented, statement for statement, and it is the
transcript `i2pr_crypto::red25519_deployed` produces. Plan 346's evidence previously rested on
a signed-output fixture generated at the older `2c694149`; it is now additionally confirmed
against the **2.61.0 pin itself**. No Plan 346 claim changed — this strengthens it.

## The classified boundary

Plan 347 requires four mandatory directions, each a closure gate. Classified by stage:

| Direction | Required | Boundary | Class |
|---|---|---|---|
| i2pr → i2pd | pass | **Not executed.** The i2pr publisher path is a live network send and the pinned i2pd consumer surface is proven to exist, so the row is reachable — but there is no ELS2 lane, no i2pd-side ELS2 consumer driver, and no executed matrix. | harness |
| i2pd → i2pr | pass | **Not executed, and structurally unreachable today.** i2pr has **no type-5 consumer path**: `EncryptedLeaseSet2Resolver` has zero production callers (only `crates/i2pr-client/tests/els2_publish_resolve.rs` and `els2_authorized_publish_resolve.rs`), and no runtime owner composes a `DatabaseLookup` for a blinded storage key and feeds the result to the resolver. The generic lookup composer is key-agnostic and would accept a blinded key, but nothing wires `current_storage_key` into it. | **missing production capability** |
| i2pr → Java I2P | pass | **Not executable.** Stock Java I2P will not initiate transport to i2pr. This is Plan 306's exhausted, recorded boundary: caps `f` → zero initiation; caps `fR` → listed and `has_floodfill_capability=true` but still zero initiation, bandwith tier still `Unknown`. Java's `TunnelPeerSelector` requires a tier letter, and advertising one "would assert a measured capacity class with no measurement behind it — a fabrication", forbidden by Plan 306 §3/§8 and ADR 0030. | **policy design, out of 347's scope** |
| Java I2P → i2pr | pass | **Not executable**, for the same Java tier boundary, *and* independently blocked by the missing i2pr type-5 consumer path above. | both |

### Stage-by-stage classification

| Stage | State |
|---|---|
| **Signature / transcript** | **CLOSED.** Plan 346 corrected the policy; the deployed profile cross-verifies against executed Java I2P and i2pd output in both directions, and the transcript is now additionally confirmed against the pinned i2pd `2.61.0` `SignRedDSA` source. No signature-stage failure remains, in either direction. |
| **Store (DatabaseStore type 5)** | **Publisher side exists in production**, unexecuted externally. `crates/i2pr-daemon/src/service_product.rs:3654-3685` builds the `DatabaseStoreMessage` (type-5 branch at `:3679-3683`) and `publish_service_ls2_for_service` at `:3707-3811` sends it over a real tunnel, requiring `RouterDeliveryOutcome::Accepted` per cell (`:3796-3809`), with the floodfill selected from the type-5 blinded key (`:3739-3742`) and failing closed on `"no floodfill for publication"`. Production callers exist (`:1895`, `:3596`). The type-5 branch has only ever run in unit tests, never against an external peer. Inbound type-5 **store** is served by the controlled floodfill (`crates/i2pr-netdb/src/floodfill_service.rs:739-752`). |
| **Lookup (DatabaseLookup at a blinded storage key)** | **Missing on the i2pr consumer side.** The transport exists and is key-agnostic (`crates/i2pr-daemon/src/netdb_tunnels.rs:447` `compose_lookup_via_tunnel`, target taken as a plain `Hash` at `:460-462`; `begin_tunnel_lookup` at `:386`), but its only callers are tests. |
| **Decrypt (layer 1 / layer 2)** | **Local only.** Layer cryptography is proven against an independent pure-Python re-derivation across five cases (`crates/i2pr-netdb/tests/els2_foundation.rs`). Never exercised with bytes received from an external router. |
| **Inner LeaseSet2 validation** | **Local only**, as above. |
| **Streaming / application payload** | **Not reached.** No ELS2/type-5 artifact of any kind exists under `tests/integration/`; the nearest two-process NetDB harness is the floodfill lane, which runs the type 1/3/7 floor and deliberately excludes type 5. |
| **Harness** | **Does not exist.** No `run-*.sh`, driver, or fixture for ELS2 cross-router. The reusable substrate does exist and is proven: the controlled loopback, unprivileged, reseed-disabled topology with RouterInfos file-seeded into stock reference netDb layouts (`tests/integration/floodfill/run-i2pd.sh:90-132`, `:159-170`; `run-java-floodfill.sh:32-40`), and a passing i2pd NetDB store/ack/lookup matrix on it (Plan 303 `publisher-store-accepted=1, store_ack_delivered=1`; Plan 306 WP3 requal green). No public I2P participation is required or used. |

**The single largest blocker is not ELS2.** The signature stage is done. What blocks three of
the four directions is (a) i2pr having no type-5 *consumer* path at all, and (b) stock Java
I2P refusing to dial i2pr without a bandwidth-tier design that ADR 0030 forbids inventing. (b)
is explicitly a separate policy design with no bounded in-posture step (Plan 306 §8), and is
out of scope for an ELS2 plan.

## Requirements matrix

| Plan 347 requirement | Result |
|---|---|
| Reference freeze recorded | **Met** (table above). |
| Stock, unmodified reference routers | **Met** for i2pd (built clean at the pin). Java not built; the Java rows are blocked before a build matters. |
| Controlled topology with real store + lookup | **Substrate met** (Plan 303/306), **type-5 matrix not built**. |
| Harness proves it did not inject a decoded LeaseSet | **Not applicable** — no harness ran. |
| Four mandatory directions | **0 of 4 passed.** See the classified table. |
| Capability matrix (no-auth / lookup secret / PSK / DH) | **Not executed.** |
| Rollover and persistence | **Not executed.** |
| Negative rows | **Not executed.** |
| Bounded machine-readable evidence artifact + fail-closed checker | **Not produced.** Not produced rather than produced empty: a checker that passes on a missing matrix would be a fail-open guard, which this repository forbids. |
| On success consequences | **Not applied.** `specs/support.toml` and `specs/CONFORMANCE.md` keep type 5 non-advertised, exactly as Plan 346 left them. |
| On failure consequences | **Applied** — this record, plus corrective Plan 349. |

## What was deliberately not done

- **No acceptance criteria were narrowed.** A "crypto-boundary-only" pass was explicitly
  rejected: Plan 346 already closed that, so claiming it again would add nothing and would
  misrepresent Plan 347 as satisfied.
- **No production wire change was made to go green**, and no guard, budget, boundary script, or
  test-selection rule was weakened.
- **No reference was patched**, and no reference source was copied into i2pr.
- **No bandwidth tier letter was invented.** This is the single easiest way to make the two
  Java rows go green and it is a fabrication; Plan 306 §8 and ADR 0030 forbid it, so the Java
  rows stay stopped.
- **Plan 346's transcript policy was not changed ad hoc**, as Plan 347's failure clause
  requires. The transcript is correct; what is missing is a consumer path and a bandwidth
  design.

## Security review

- No secret, private key, PSK, lookup secret, DH private key, destination signing key, or proxy
  credential was recorded. The frozen artifacts are repository SHAs, version strings, and
  library build-dependency versions.
- No i2pr production code was changed for this plan, so no attack surface moved.
- `sudo` was used only to install `libzmq3-dev`, a build dependency for compiling the pinned
  i2pd reference. No reference source, no i2pr code, and no network configuration was modified
  with it, and no container, namespace, VM, or systemd unit was used.
- The reference build is stock: a clean clone at the pin, `make i2pd`, no local patches. The
  working tree was not modified after checkout (`git rev-parse HEAD` = the pin).

## Findings by severity

- **Critical:** none.
- **High:** none. No i2pr defect was found; the signature stage is correct.
- **Medium (architecture gap, not a defect):** i2pr has no type-5 **consumer** path. A router
  that can publish an encrypted LeaseSet but cannot resolve one is asymmetric, and the
  asymmetry is invisible from the publisher side. This is the actionable finding and Plan 349
  owns it.
- **Medium (policy gap, by design):** stock Java I2P cannot be made to dial i2pr inside the
  current advertisement posture, because its peer selector requires a bandwidth tier that
  cannot be truthfully produced. Already recorded by Plan 306; re-observed here, not re-litigated.
- **Low (positive):** the pinned i2pd 2.61.0 implements the full type-5 consumer surface, and
  its `SignRedDSA` matches the Plan 346 deployed transcript exactly. This is the first positive
  reference-side capability evidence for type 5 in the repository.
- **Low (documentation):** `plans/subsystems/i2pr-netdb.md` previously stated no reference can
  verify the type-5 transcript. Plan 346 corrected that; Plan 347 confirms it against the
  2.61.0 pin.

## Roadmap disposition and unblock audit

`plans/subsystems/red25519-encrypted-leaseset-roadmap.md` §8 records Plan 347 as stopped at
this boundary and keeps it as the branch's closure gate.

**Unblock audit** over registered plans depending on Plan 347:

| Plan | Dependency on 347 | Disposition |
|---|---|---|
| 348 | hard | **stays blocked.** 347 did not pass, and 348's other hard dependency (Plan 342) is also unmet. |
| 326 | via 347 (external evidence) | **stays blocked, correctly.** 326 requires end-to-end external evidence, which 347 did not produce. Its historical token is unchanged. |
| 335 | via 347 | **stays blocked historical.** Plan 346 already superseded its forward interpretation; nothing here reopens it. |
| **349 (registered by this closure)** | corrective for the i2pr type-5 consumer path | **`ready`.** Its only hard dependency is Plan 346, which passed, and Plan 347's boundary is a stable written contract for what is missing. |

**No other registered plan lists 347 as a dependency.** Nothing was silently unblocked, and no
historical closure (326, 335, 336, 306, 279) was rewritten.

## Corrective registered

**Plan 349 — i2pr encrypted-LeaseSet2 consumer lookup path.** It owns the one actionable gap
this plan found: giving i2pr a real type-5 *consumer* path (address → blinded storage key →
`DatabaseLookup` → `ValidatedEncryptedLeaseSet2` → layer decrypt → inner `LeaseSet2` → service
use), so `EncryptedLeaseSet2Resolver` stops being test-only and the i2pd→i2pr direction becomes
reachable. It is deliberately scoped to i2pr's own consumer path and does **not** attempt the
Java bandwidth-tier design, which is a separate policy question with its own plan of record.

With Plan 349 passing, the `i2pr → i2pd` and `i2pd → i2pr` rows become executable and Plan 347
can be re-attempted for those two directions. **The two Java rows will still require the
bandwidth-tier design before any ELS2 work can reach them**, and that is a fact about Java's
peer selector, not about encrypted LeaseSet2.
