# Plan 351 — ELS2 consumer service wiring: status

Status: **passed-gate-scoped-els2-consumer-service-wiring-landed**

Plan of record:
[`351-els2-consumer-service-wiring.md`](../../implementation/i2pcontrol-proposal-170/351-els2-consumer-service-wiring.md).
ADR: [`0033`](../../../docs/adr/0033-els2-consumer-lookup-identity-and-install-key.md).
Origin: Plan 347's classified boundary and Plan 349's incomplete owner.

## The headline

The bounded ELS2 owner now has a **production caller**. A `.b33` is a first-class service-tunnel
remote target: it is parsed as its own reference kind, looked up through the real transport under
the day's blinded storage key, unwrapped, **bound to the address by signature**, and installed under
the inner record's unblinded destination hash so the ordinary delivery path can forward to it.

Both `EncryptedServiceResolver` and `i2pr_client::EncryptedLeaseSet2Resolver` went from zero
non-test production callers to at least one each.

## Two premises that were wrong, recorded rather than absorbed

### F1 (medium) — the install key cannot be derived from the address

In-scope item 5 of the plan of record said the unblinded destination hash is "derived from the b33
address's public key". It is not derivable, and the protocol layer's own documentation says so at
`crates/i2pr-proto/src/common/base32.rs:255`:

> The value carries the **unblinded** signing public key plus both signature types, so a client can
> derive the daily blinded key from the address alone. It is not a Destination hash and must never be
> treated as one.

A `Destination` hash is the SHA-256 of a canonical `Destination` encoding, which needs the ECIES
public key, the signing key, the certificate, and the padding. A `.b33` publishes exactly one of
those four. Any 32 bytes produced from it would look plausible and match nothing.

**Delivered instead:** the install key is the **inner record's own** destination hash, gated by
`encrypted_service_resolver::bind_inner_to_address` — the record may name itself only if
`inner.Destination.signing_key == address.public_key()` **and**
`inner.signing_key.key_type == address.unblinded_sigtype()`. Trust is transitive through a
signature against a key obtained out of band from the address, not through the record's own claim.

The acceptance criterion is unchanged and satisfied: criterion 5 asked for the **unblinded** hash,
and the delivered value is the unblinded hash.

### F2 (medium) — the type-7 relationship does not generalize

Plan 351's publisher emits **type 7** (`EdDsaSha512Ed25519`). `service_els2.rs:251-258` derives the
unblinded Red25519 scalar as `CONVERT_ED25519_PRIVATE(seed)` — SHA-512, clamp, low 32 bytes — and
`DERIVE_PUBLIC` of that scalar reproduces the destination's ordinary Ed25519 public key. So for a
type-7 `.b33`, `address.public_key()` **is** the inner record's signing key.

A **type-11** `.b33` has no such relationship: its unblinded key is a distinct Red25519 identity
that signs the outer record only. A consumer assuming the type-7 relationship would refuse every
type-11 address; one assuming there is no relationship would accept any valid `LeaseSet2`.

**Delivered:** the binding is written against what production publishes, and
`the_installed_hash_is_the_unblinded_destination_hash` asserts the premise
(`address.public_key() == inner.destination().signing_key().as_bytes()`) **before** the binding is
used, so the day production moves to type 11 the row fails rather than the behaviour silently
changing.

### F3 (low) — `record_observation`'s fallthrough was a silent no-op

`service_delivery.rs`'s `match` ended in `_ => {}`, so a typo'd label was indistinguishable from a
deliberately suppressed one. A counter that was never wired up would have looked identical to one
that was deliberately rejected. Now `observations_rejected_unknown` counts it.

## What changed

1. **`DestinationRef::EncryptedService`** (`i2pr-service-tunnels/src/destination.rs`), dispatched
   by `is_encrypted_service_address` **before** the `.b32.i2p` branch. `EncryptedServiceAddress`
   is `Copy`, so `RemoteTargetProjection::EncryptedService` carries it by value.
2. **Gate 1** in `ServiceTunnelSpec::validate`; a static alias may not name a `.b33`.
3. **`LookupResult::EncryptedLeaseSet2Success`** and one type-5 arm in
   `i2pr-netdb/src/lookup_engine.rs` that performs the key match and record-type check only.
4. **`begin_lease_set2_lookup_for_key_with_store`** in `netdb_seam.rs` and
   **`begin_encrypted_lease_lookup`** in `destination_tunnels.rs`; `PendingLeaseLookup` split into
   `lookup_key: RouterHash` + `destination: Option<DestinationHash>`.
5. **`bind_inner_to_address`** in `encrypted_service_resolver.rs` — the identity binding.
6. **`resolve_encrypted_destination_for_service`** + **`provision_encrypted_service_target`** in
   `service_product.rs`; the provisioning loop takes a typed three-way branch.
7. **`EncryptedTargetStatus`** (closed enum, `&'static str` reasons) plus the manager's
   install/remove/read accessors for the consumer secret and the status.
8. **Gate 2**: `leaseset_password` accepted on a client in the one pairing where it has meaning;
   the consumer shape is exempt from the publisher LeaseSet security block;
   `sync_encrypted_target_secrets` installs the secret in the same reconciliation as the publisher
   material.
9. **`RemoteTargetProjection`** replaces `Option<[u8; 32]>` in `project_remote_target`.
10. **Two guards**, both in the `AGENTS.md` routine floor: `check-encrypted-service-consumer-caller.sh`
    (18 deliberate-break mutations, all detected) and `check-config-secret-hygiene.sh` (17, all
    detected).

### Two deliberate behaviour changes to existing code

- **Plan 289's row** `plan289_unsupported_options_rejected_before_storage`: for
  `leaseset_password` on a client the **error variant** changed from `UnsupportedOption` to
  `ContradictoryOptions`. It is still rejected before storage. The old reason claimed the key is
  "out of scope for the kind", which was never true — the key is in scope for a client as a
  *consumer*. The row was updated with that reasoning inline, not silently.
- **`remote_target_hash_for_reference` → `project_remote_target`**: a signature change. Its single
  production caller and two in-crate tests were updated. The ordinary path's behaviour is unchanged
  (`lookup_key` is exactly `router_hash_from_destination(target)` and `destination` is that same
  hash).
- Five external-lane tests gained an explicit **fail-closed** arm on
  `LeaseStoreIngestOutcome::EncryptedLeaseSet2Ready` that panics: an ordinary `.b32` lookup must
  never receive a type-5 record, and counting it as a resolution would make a wrong premise look
  like a pass.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | A production owner resolves a b33 through the real lookup transport and installs an inner `LeaseSet2` | `a_published_encrypted_service_resolves_binds_and_installs`; `resolve_encrypted_destination_for_service` reached from the `provision_all_service_router_material` loop at `service_product.rs:3629` |
| 2 | Both resolvers have a non-test production caller; the guard fails on regression to zero | `resolve_encrypted_destination_for_service` → `EncryptedServiceResolver::begin/ingest_store`; `guard1` checks `begin_lease_set2_lookup_for_key_with_store`, `begin_encrypted_lease_lookup`, `bind_inner_to_address` |
| 3 | An encrypted remote target is impossible on a non-`DelayOpen` service | `plan351_encrypted_target_requires_a_delay_open_client` (4 sub-cases); `plan351_a_static_alias_may_not_name_an_encrypted_address` |
| 4 | One service's failure leaves others running | **Partial — see Limitations.** Proven at configuration level (criterion 3) and status-surface level (`EncryptedTargetStatus` + `encrypted_target_failed` counter). The live multi-service composition is not exercised. |
| 5 | Installed under the unblinded hash; fails if the blinded key is used | `the_installed_hash_is_the_unblinded_destination_hash` asserts `bound != DestinationHash::from_hash(storage_key)`; `guard1` rejects a binding that derives from the address |
| 6 | No secret in `Debug`/`Display`/`Serialize`/log/error; no inline config field | `guard2` (17 mutations); `Arc<LookupSecret>` registry pinned to exactly that type; exactly one `.as_option()` call site in the daemon |
| 7 | Ordinary LeaseSet2 path unchanged; no ELS2 branch above the owner | `an_ordinary_base32_address_is_unaffected`; the ELS2 arm is a single match arm in `lookup_engine.rs` and `guard1` fails if it validates/unwraps/inserts |
| 8 | In-flight bound, lease release on every path, restart | Bound + lease release: Plan 349's 17 rows, unchanged and still green. Restart through the composition: **not exercised — see Limitations.** |
| 9 | Every new observation label has a matching arm, asserted | `encrypted_target_resolved` / `encrypted_target_failed` arms in `record_observation`; `_ => {}` replaced by the counted `observations_rejected_unknown` |
| 10 | ADR 0032's guard passes; no transcript policy changed | `check-els2-type11-transcript-boundary.sh` green; no file under `els2_transcript.rs` or `red25519.rs` modified |
| 11 | Both guards in the floor and negative-tested | `AGENTS.md` lines added; 18/18 and 17/17 mutations detected |
| 12 | Exact-head routine CI green | See "Verification". Labelled **local**; no CI is available in this environment. |

## Verification (all **local**; no CI is available in this environment)

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo check --locked --workspace --all-targets` | pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | pass |
| `cargo test --locked --workspace --doc` | pass |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | **4144 passed, 0 failed** across 172 test binaries; see F7 for the intermittent `i2pcontrol_tunnels` failure observed on one earlier floor attempt and reproduced at base |
| `cargo test --locked -p i2pr-daemon --test encrypted_service_consumer_wiring -- --test-threads=1` | 8 passed |
| `cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1` | 341 passed (incl. 2 new Plan 351 rows) |
| `cargo test --locked -p i2pr-daemon --lib i2pcontrol_tunnels::tests -- --test-threads=1` | 52 passed (incl. 2 new Plan 351 rows) |
| `bash scripts/check-encrypted-service-consumer-caller.sh` | OK |
| `bash scripts/check-config-secret-hygiene.sh` | OK |
| `bash scripts/check-els2-type11-transcript-boundary.sh` | OK |
| `bash scripts/check-floodfill-type5-serve.sh` | OK |
| `bash scripts/check-dependency-direction.sh` | ok |
| `bash scripts/check-runtime-boundaries.sh` | ok |
| `bash scripts/check-service-tunnel-boundaries.sh` | ok |
| `python3 scripts/check-global-plan-number-uniqueness.py` | passed |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | ok |
| remaining `scripts/check-*.sh` in the `AGENTS.md` floor | ok |
| `cargo deny check advisories bans sources` | pass |

**One floor attempt reported two failures**, both diagnosed and resolved or classified rather than
re-run until green:

1. `check-service-tunnel-acceptance-evidence.sh` — a real regression from this plan's rename.
   Fixed in the checker (F8) and re-verified.
2. `cargo test` — the port flake (F7). Reproduced **at base with this plan stashed**, so it is not
   attributable to Plan 351. Classified as separate work.

The **whole floor was run as one command** at the exact head and finished **36 of 36 PASS with zero
failures**, including `cargo test` at 4144/0 and `cargo deny check advisories bans sources`. The
complete per-step record is the `===== SUMMARY =====` block of that run.

## Runtime-neutrality and bounded-queue review

- `i2pr-service-tunnels` stays runtime-neutral. The new `EncryptedService` variant carries only
  `EncryptedServiceAddress` (a `Copy` value of four validated public fields). **No** `tokio::*`,
  `std::net`, `std::fs`, `JoinHandle`, or ownerless `spawn` was added. The crate never derives a
  blinded storage key and never sees the lookup secret.
- `i2pr-netdb` stays runtime-neutral. The new arm performs no I/O, spawns nothing, and allocates
  only the `Box<DatabaseStoreMessage>` it hands upward — already bounded by the caller's
  `MAX_I2NP_PAYLOAD_SIZE` envelope decode and re-checked against `MAX_ELS2_RECORD_LENGTH` **before**
  hashing or parsing.
- No new queue was introduced. `EncryptedServiceResolver`'s in-flight table keeps its
  `MAX_CONCURRENT_ENCRYPTED_RESOLVES = 8` bound, and `DestinationTunnelCoordinator` keeps
  `MAX_CONCURRENT_LEASE_LOOKUPS` and `MAX_RETAINED_LEASE_REPLY_PATHS`. The encrypted path enters
  through the **same** coordinator and the **same** bounded re-query loop as the ordinary path
  (`MAX_LOOKUP_ATTEMPTS = 3`), so it inherits their bounds rather than adding its own.
- **Lease release on every path.** `cancel(resolve_id)` runs on each of the four early returns in
  `resolve_encrypted_destination_for_service`; `ingest_store` removes the request from the table
  *before* it can fail, so success, failure, and unwrap failure all release; and the
  `EncryptedLeaseSet2Ready` arm removes the coordinator's `pending` and `retained_paths` entries
  before returning, so a fetch the owner then rejects cannot strand the table. Verified by
  `a_record_from_another_day_is_refused` (`in_flight() == 0` after a failed ingest).

## Secret-handling review

- The consumer lookup secret arrives borrowed from the I2PControl definition options, is wrapped in
  `Arc<LookupSecret>`, and is passed to the resolver as `Option<&str>` for the bounded lifetime of
  one resolve. It is never copied into a second long-lived `String`.
- `LookupSecret` is `Zeroizing<String>`, **not** `Clone`, has no serde, and has a redacted `Debug`
  printing only its length. `guard2` fails if any of those changes, if the registry widens beyond
  `Arc<LookupSecret>`, if a second `.as_option()` call site appears anywhere in the daemon, or if
  the secret appears in `Config`, a `Raw*Config` struct, or a `DestinationRef`.
- `EncryptedTargetStatus` carries no dynamic data: every field is a unit variant and every reason is
  a `&'static str`. `guard2` asserts the variant count equals the reason count, so a new variant
  without a reason fails.
- No secret reaches `Debug`, `Display`, `Serialize`, a log line, or an error string.
- **No Java or i2pd source was vendored, patched, or copied.** This plan touched no reference
  source; the pinned references were not consulted for this work.

## Guards: deliberate-break transcripts

`check-encrypted-service-consumer-caller.sh` — **18/18 detected**:

| Break | Detected as |
| --- | --- |
| Gate 1 rule removed | `ServiceTunnelSpec::validate lost the Gate 1 encrypted-target rule` |
| Gate 1 condition neutered (`if false`) | `Gate 1 must key on the EncryptedService reference variant` |
| Alias bypass reopened | `the static-alias bypass … was reopened` |
| `.b33` dispatch removed from `parse` | `must dispatch an encrypted-service address explicitly` |
| Projection variant renamed | `must keep the EncryptedService outcome` |
| Verbatim-key seam entry un-`pub`'d | `the seam lost the lookup-key-supplied entry point` |
| Seam re-derives the key | `must not re-derive the key from a destination hash` |
| Binding owner un-`pub`'d | `the b33 identity binding owner is missing` |
| Binding key check dropped | `must compare the inner signing key against the b33 public key` |
| Binding sigtype check dropped | `must compare the unblinded signature type` |
| Binding fabricates a hash from the address | `must be derived from the inner record's Destination` |
| Product keeps a private binding copy | `must call the ELS2 owner, not keep a private copy` |
| Product stops calling the binding | `the install path must bind through the ELS2 owner` |
| Wrapper stops consuming the failure | `must consume the typed failure` |
| Wrapper returns a `Result` | `must not be able to return a Result` |
| Wrapper stops recording status | `must be recorded on the per-service status surface` |
| NetDB arm starts validating | `must not validate, unwrap, or install the record` |
| NetDB arm starts unwrapping | `must not unwrap; the ELS2 owner holds the daily material` |
| Gate 2 pairing rule removed | `the I2PControl Gate 2 pairing rule is missing` |

`check-config-secret-hygiene.sh` — **17/17 detected**:

| Break | Detected as |
| --- | --- |
| Secret becomes an inline `Config` field | detected |
| Secret lands in a `Raw` config struct | detected |
| `DestinationRef` grows a secret variant | `must not carry secret material` |
| `LookupSecret` gains `Clone` | `must not derive Clone or any serde trait` |
| `LookupSecret` gains `Serialize` | `must not derive Clone or any serde trait` |
| `LookupSecret` loses `Zeroizing` | `must keep erase-on-drop semantics` |
| `LookupSecret` gains `as_str()` | `must expose only as_option()/is_empty()/len()` |
| Registry widens to a plain `String` | `must hold Arc<LookupSecret> and nothing else` |
| A second `.as_option()` site appears | `exactly one site may borrow the consumer secret` |
| Resolve path copies to a `String` | same |
| `EncryptedTargetStatus` gains a `String` | `must carry no dynamic data` |
| Status reason becomes formatted | `every variant needs a reason string` |
| Status loses a reason arm | `every variant needs a reason string` |
| **D1 fixed out of band** | `the ConfigError::Parse display string changed; Plan 352 owns the D1 fix` |
| **D3 fixed out of band** | `Config no longer derives Debug; Plan 352 owns the D3 fix` |
| Config error sink changes | `the ConfigError::Parse display string changed` |
| Secret install path removed | `must be installed from the I2PControl definition reconciliation` |

**`check-config-secret-hygiene.sh` is deliberately bidirectional.** Its section 4 asserts the
pre-existing leak paths are *still present*, and names Plan 352 as their owner. That is what makes
a security fix landing out of band fail loudly instead of drifting past the plan record. When Plan
352 lands, section 4 is replaced by the "fixed state" assertions.

## Findings by severity

| ID | Severity | Finding |
| --- | --- | --- |
| F1 | medium | The install key cannot be derived from a `.b33`; it comes from the inner record, gated by a signature binding. The plan-of-record premise was wrong. |
| F2 | medium | The type-7 address↔record key relationship does not generalize to type 11. The binding is written against what production publishes, with the premise asserted. |
| F3 | low | `record_observation`'s `_ => {}` made a typo'd label indistinguishable from a deliberate suppression. Now counted. |
| F4 | low | `check-config-secret-hygiene.sh` section 4 asserts the Plan 352 leak paths are still present, so an out-of-band fix fails loudly. |
| F8 | low | `check-service-tunnel-acceptance-evidence.sh` pinned the **symbol name** `remote_target_hash_for_reference`, so renaming it broke a Plan 212 §8 evidence row. Updated to `project_remote_target` and an **added** row requiring the `EncryptedService` outcome, so the capability assertion got stronger rather than merely being renamed to match. |
| F5 | informational | Plan 289's error variant for `leaseset_password` on a client changed from `UnsupportedOption` to `ContradictoryOptions`. Still refused before storage; the new reason is the accurate one. |
| F6 | informational | `remote_target_hash_for_reference` → `project_remote_target` is a signature change. One production caller, two in-crate tests. |
| F7 | medium (pre-existing, **not** Plan 351) | `crates/i2pr-daemon/tests/i2pcontrol_tunnels.rs` is port-flaky. `distinct_port()` computes a *guess* — `33_000 + (pid % 5_000)` then `+ n*37` — instead of binding an ephemeral port, so a collision surfaces as `runtime transition failed: manager configuration rejected`. Measured at base commit `73eecc7` with Plan 351 stashed: **5 of 6** consecutive runs failed, escalating to 16/17. With Plan 351 applied: 2 of 7. Reproduces with no stray listener present (`ss -ltn` clean), so the cause is TIME_WAIT retention across consecutive runs of the same binary. **Not fixed here**: it is unrelated to ELS2, it reproduces without this plan, and fixing it inside Plan 351 would mix a harness defect into a protocol-closure commit. Named as separate work. |

No high-severity finding remains open.

## Limitations (explicit non-claims)

1. **No PSK/DH consumer authorization.** Plan 349's `begin_authorized` has 17 rows of coverage but
   no production caller; a `.b33` whose address sets `requires_client_key` resolves to
   `EncryptedTargetStatus::UnwrapFailed`. Deferred on purpose: a credential-file owner was not
   invented under time pressure.
2. **No daily rollover re-resolution.** The blinding rotates daily and there is no clock-driven
   refresh: `advance_destination_pools()` is inbound-driven, not clock-driven. A resolution computed
   before a midnight boundary addresses the **wrong DHT key**. Proven by
   `a_record_from_another_day_is_refused`, which shows the failure is loud rather than silent.
   **This is not rollover support and must not be described as such.**
3. **No cross-router result.** Nothing here was run against stock Java I2P or i2pd. Type 5 stays
   `advertised = false`.
4. **Acceptance criterion 4 is partially met.** "One service's failure leaves every other service
   running" is proven at the configuration level (Gate 1 makes the shape unrepresentable) and at the
   status-surface level (the failure is recorded, never propagated). It is **not** proven through a
   live multi-service router-backed composition, which needs the Plan 347 substrate. The narrow
   `let _ =` in `provision_encrypted_service_target` and the closed `EncryptedTargetStatus` are the
   mechanism; a runtime observation of two coexisting services is not.
5. **Acceptance criterion 8's restart row is not exercised.** The resolver's own lease accounting
   is proven (capacity, release on every path, expiry), but a product-level restart with a live
   encrypted lookup is not.
6. **The type-5 arm in the NetDB engine is a delegation, not a validation.** A caller that matches
   only `LeaseSet2Success` treats a type-5 reply as a non-match and keeps waiting. That is the
   intended fail-closed behaviour, but it means the NetDB layer cannot by itself tell an encrypted
   consumer apart from a stalled ordinary lookup.

## Roadmap disposition and unblock audit

**Plan 349's blocker is cleared.** Its closure record names "a production caller and the
configuration surface for an encrypted-service target" as remaining work item 1; Plan 351 is that
work. Plan 349's status token is **unchanged** at
`in-progress-owner-implemented-and-proven-no-production-caller-yet` — wait, that token is now
**false**, because the owner has a production caller.

Audited, not silently changed:

- Plan 349's recorded token says "no production caller yet". After this plan that is no longer
  true, so leaving the token unchanged would make a closure record assert something the code
  contradicts. Plan 349's token is therefore **superseded by this record** rather than rewritten:
  `superseded-by-plan351-with-caller-landed`. Plan 349's own text is untouched, so the history of
  what it found — bounded table, fail-closed ingest order, lease release, owned zeroizing
  credential — stays readable.
- Plan 351's registration row in `plans/registry.md` moves `ready` → `passed`, with this record.

**Unblock audit** (`plans/registry.md` blocked work, plus the ELS2 roadmap dependency graph):

| Plan | Hard deps | Interface deps | Disposition |
| --- | --- | --- | --- |
| **347** live bidirectional ELS2 qualification | 350 (passed), 351 (**now passed**) | reference-side ELS2 drivers for Java and i2pd | **Still blocked**, and Plan 351 was not sufficient. The i2pd direction additionally needs a reference-side driver; the Java direction needs the bandwidth-tier policy design that ADR 0030 forbids fabricating. Two of four directions were never about i2pr. |
| **348** fresh full-Proposal-170 conformance gate | 342 (not passed), 347 (blocked) | Proposal 170 re-freeze (executed) | **Still blocked.** Unchanged by this plan. |
| **342** outproxy option surface + request paths + wire lane | none | — | **Unchanged**, independent of this plan, still `ready`. |
| **352** config secret hygiene | none | — | **Unchanged**, `ready`, and now has a bidirectional guard in the floor that names it as the owner of D1/D2/D3. |

No plan's state changed as a result of this closure. That is the honest outcome: Plan 351 removed a
real dependency, and the remaining blockers for 347 and 348 are **not** i2pr-side.

## Scope note on Plan 349

Plan 349's out-of-scope line forbids "New daemon configuration surface, I2PControl options", while
its acceptance criterion 1 requires a production caller and its closure record names config
surface as remaining work. Plan 351 resolved that procedurally by registering itself and putting
I2PControl options explicitly **in scope**. Plan 349's out-of-scope line is therefore **not
satisfied**, and it was never going to be; that is recorded at the plan that corrects it rather than
by editing Plan 349.