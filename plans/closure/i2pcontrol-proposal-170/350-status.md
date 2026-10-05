# Plan 350 — Floodfill type-5 lookup serve path: status

Status: **passed-floodfill-now-stores-and-serves-encrypted-leaseset2-type5; reference-consumers-can-resolve-an-i2pr-published-service**

Implementation commit: see "Commits" below. Plan of record:
[`350-floodfill-type5-serve-path.md`](../../implementation/i2pcontrol-proposal-170/350-floodfill-type5-serve-path.md).
Origin: the corrected boundary in
[`347-status.md`](347-status.md).

## The headline

The controlled floodfill now **stores** an encrypted LeaseSet2 and then **serves** it to a
blinded-key LeaseSet lookup. That completes the store→lookup half of the chain every Plan 347
`i2pr → reference` direction needs, and it is the only thing that was missing on the i2pr side
of those rows.

## Scope correction made before implementing

This plan was registered from Plan 347's boundary, which at that point claimed the type-5
**store** half already worked and that only the serve list was missing. **That was wrong**, and
reading the code before editing it is what caught it:

```rust
// FloodfillStoreService::handle, before this plan
if record_type == 5 {
    return FloodfillStoreEffect::Unsupported;
}
```

The floodfill refused type 5 *outright*, before `validate` was reached, so the type-5 arm in
`validate` (`floodfill_service.rs`, the `DatabaseStoreData::EncryptedLeaseSet` branch) and the
complete `5 =>` arm in `ServerNetDb::database_store_for_answer` (`server_store.rs`) were both
unreachable. The plan's scope was widened from one change to two, and Plan 347's record was
corrected in the same commit.

The guard was present in the commit that created the service (`53a404b netdb: add bounded
floodfill DatabaseStore service`) — a deliberate from-the-start hold-back, not an oversight. Its
rationale was the type-11 transcript disagreement, which Plan 346 / ADR 0032 closed: before
that, a controlled floodfill could not have usefully admitted a *reference-published* type-5
record because the verification in `ValidatedEncryptedLeaseSet2::validate` would have rejected
the deployed RedDSA transcript. **Plan 346 is a hard prerequisite of this plan**, not a
formality.

## What changed

1. **`FloodfillStoreService::handle` no longer refuses `record_type == 5`.** The hold-back is
   removed and the existing type-5 arm in `validate` becomes reachable for the first time.
2. **Record type 5 was added to the floodfill's lookup candidate lists** —
   `SERVABLE_NORMAL_LOOKUP` (`0 => [0,1,3,5,7]`) and `SERVABLE_LEASE_LOOKUP`
   (`1 => [1,3,5,7]`). Type 5 belongs in the LeaseSet list specifically, because an encrypted
   LeaseSet2 is filed under its **blinded** storage key rather than the destination hash, and
   `lookup_type == 1` is what a reference client issues when resolving an encrypted service.
3. **The candidate lists are now named constants** (`SERVABLE_RECORD_TYPES`,
   `SERVABLE_NORMAL_LOOKUP`, `SERVABLE_LEASE_LOOKUP`) instead of inline literals, exported from
   `i2pr-netdb`, so the store/lookup correspondence is checkable.
4. **`scripts/check-floodfill-type5-serve.sh`** (new, added to the `AGENTS.md` routine floor).

`lookup_type` 2 (RouterInfo) and 3 (exploration) are **unchanged**.

## Resource and security review

Admitting a new record type into a store is the part of this plan that could have been unsafe,
so it was checked rather than assumed.

**Nothing escapes the existing budgets.** The removed guard sat *before* the crypto budget
check, so the question was whether removing it let type-5 stores run unbounded. They do not:
`FloodfillStoreService::admit_request` already applied, for every record type, a **global
request cap** (`self.global.admit(now, window_ms, max_global_requests)`), a **global byte cap**
(`global_bytes.admit(..., max_global_bytes)`), a **per-source cap**, and a **per-key cap** — and
for type 5 the per-key cap is keyed on `RecordId::new(5, blinded_key)`, so distinct daily
blinded keys are distinct budget buckets inside a still-global ceiling. The `crypto` budget,
which now bounds the extra Red25519 verification type 5 performs, is applied immediately after
the removed guard and now covers it.

**Storage is separately bounded.** Type-5 records land in `Els2Store`, which has its own
capacity policy and already enforced `CapacityExceeded` on overflow (covered by the existing
`store_capacity_is_enforced_without_mutating_existing_state` row).

**The floodfill still cannot derive a subcredential.** The type-5 validate arm keeps its
existing property: a floodfill stores the record opaquely and never learns the unblinded public
key, so it has no material it could misuse. The new guard script asserts that arm is still
present.

**A publisher still cannot park a record under a key it does not own.** The storage-key gate is
unchanged and is covered by
`a_record_is_not_served_under_a_key_it_does_not_own`, which requires the store to reject the
insert and requires nothing to be served afterwards.

**Client-tunnel ingress is still refused** for type 5, as for every record type
(`a_client_tunnel_type5_store_is_still_refused`), and a non-serving role is still disabled
(`a_non_serving_role_cannot_store_a_type5_record`). No new dependency, no wire-format change, no
configuration surface, no advertisement change, and `i2pr-netdb` remains runtime-neutral.

## Requirement-to-evidence matrix

All rows are in `crates/i2pr-netdb/tests/floodfill_type5_serve.rs` (13 rows) unless noted. Every
row drives the public production surface — `FloodfillStoreService::handle` / `handle_lookup`
over a real `ServerNetDb` — not private helpers.

| Plan 350 requirement | Evidence | Result |
|---|---|---|
| Add type 5 to the floodfill's lookup candidate list for `lookup_type == 1`. | `a_type5_record_is_stored_and_served_to_a_blinded_key_lease_lookup` | **Met.** |
| Decide and pin the `lookup_type == 0` case. | `a_normal_lookup_at_the_blinded_key_also_serves_the_type5_record`. **Decision:** type 5 is included in the normal list. A normal lookup at a plain destination hash simply misses the type-5 slot and is answered from the 1/3/7 slots, so the extra probe is a normal store miss, not an error; excluding a servable type from the catch-all list is precisely the omission that caused this bug. The decision is pinned by both this row and `the_servable_type_tables_agree_with_each_other`. | **Met.** |
| Regression row: stored type-5 record returned byte-identically. | `a_type5_record_is_stored_and_served_to_a_blinded_key_lease_lookup` asserts `encode_to_vec` equality of served vs stored bytes, explicitly because a re-encode that shifted the signature preimage would break the consumer's outer type-11 verification. | **Met.** |
| Same for the offline-key form. | `a_type5_record_with_offline_keys_is_stored_and_served` | **Met.** |
| Store-serve coverage guard. | `scripts/check-floodfill-type5-serve.sh`, in the `AGENTS.md` floor. Asserts: `database_store_for_answer`'s implemented arms equal `SERVABLE_RECORD_TYPES`; every declared type is probed by at least one list; the LeaseSet list contains 1, 3, 5, 7; `handle` does not refuse type 5; each constant is actually used; `lookup_body` routes through the named constants and has no inline list. | **Met.** |
| Positive: lookup-secret form. | `a_lookup_secret_type5_record_is_stored_and_served` | **Met.** |
| Positive: the served record still validates and keeps its transcript profile. | `the_served_record_revalidates_and_keeps_its_transcript_profile` — re-validates through `ValidatedEncryptedLeaseSet2::validate`, asserts `deployed`, asserts the storage key, and **decrypts the served record back to exactly the inner LeaseSet2 that was published**. | **Met.** |
| Negative: tampered record refused, and never served. | `a_tampered_type5_record_is_refused_at_the_store` | **Met.** |
| Negative: expired record not served. | `an_expired_type5_record_is_refused_and_never_served` | **Met.** |
| Negative: wrong storage key refused and not served. | `a_record_is_not_served_under_a_key_it_does_not_own` | **Met.** |
| Negative: clean miss at an unstored blinded key. | `a_lookup_at_an_unstored_blinded_key_is_a_clean_miss` | **Met.** |
| Negative: client-tunnel ingress still refused. | `a_client_tunnel_type5_store_is_still_refused` | **Met.** |
| Negative: non-serving role still disabled. | `a_non_serving_role_cannot_store_a_type5_record` | **Met.** |
| `lookup_type` 0/2/3 answers unchanged. | `routerinfo_and_exploration_answers_unchanged` (exploration still returns `DatabaseSearchReply`; RouterInfo form still never returns a store body), plus `the_servable_type_tables_agree_with_each_other` asserting the LeaseSet list still excludes 0. The whole pre-existing floodfill test module in `floodfill_service.rs` is unchanged and green. | **Met.** |
| Malformed lookup still refused. | Pre-existing `floodfill_service.rs` rows plus the `excluded_peers` / `lookup_reply_peer_count` guards in `handle_lookup`, which run before `lookup_body`. | **Met.** |

**Both changes are independently load-bearing**, verified by reverting each in isolation:
reverting only the store refusal fails 3 rows; reverting only the serve list fails 3 rows
(including the headline row). The guard script was negative-tested the same way — removing `5`
from the LeaseSet list, reintroducing the store refusal, and declaring a servable type the store
does not implement each make it exit 1.

## Commits

| Commit | Content |
|---|---|
| `c7a1e4b` | Implementation: store refusal removed, type 5 added to both lookup lists, named constants, new guard script, new integration test suite, `AGENTS.md` floor entry |
| `9e5b2c1` | Closure record, registry and roadmap update |

## Commands run

`cargo fmt --all --check`; `cargo check --locked --workspace --all-targets`;
`cargo test --locked --workspace --all-targets -- --test-threads=1`;
`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`;
`cargo test --locked --workspace --doc`;
`bash scripts/check-floodfill-type5-serve.sh`;
`bash scripts/check-dependency-direction.sh`; `check-runtime-boundaries.sh`;
`check-service-tunnel-boundaries.sh`; `check-fixture-manifest.sh`;
`check-els2-type11-transcript-boundary.sh`; `check-ntcp2-vectors.sh`; `check-ssu2-vectors.sh`;
`check-i2cp-vectors.sh`; `check-ntcp2-interoperability.sh`;
`check-constrained-host-lane-boundary.sh`; `check-m11-transit-boundaries.sh`;
`check-m11-transit-qualification-evidence.sh`; `check-sam-acceptance-evidence.sh`;
`check-ssu2-acceptance-evidence.sh`; `check-i2cp-acceptance-evidence.sh`;
`check-i2pcontrol-acceptance-evidence.sh`; `check-service-tunnel-acceptance-evidence.sh`;
`check-exploratory-tunnel-evidence.sh`; `check-netdb-tunnel-evidence.sh`;
`check-destination-tunnel-evidence.sh`; `check-m6-mixed-router-acceptance-evidence.sh`;
`check-m12-floodfill-qualification-evidence.sh --self-test`;
`python3 scripts/check-global-plan-number-uniqueness.py`;
`python3 -m unittest discover -s tests/planning -p 'test_*.py'`;
`python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'`;
`cargo deny check advisories bans sources`.

**All results are local. No CI run was available in this environment**, so the plan's
"exact-head routine CI is green" criterion is satisfied in substance (the whole floor passes at
the exact head) but is **not** claimed as a CI run.

## Migration and compatibility evidence

- **No wire format changed.** The record is stored and served byte-identically; the row asserts
  it.
- **No existing record type changed behaviour.** Types 0/1/3/7 store and serve exactly as before;
  the only additions are that type 5 is no longer refused and that two lookup lists probe one
  more store slot. The full pre-existing floodfill suite is unchanged and green.
- **Nothing new is advertised or configured.** No caps change, no configuration surface, no
  `R`/tier question, and ADR 0030 is untouched.
- **No new dependency**; `cargo deny` unchanged; `Cargo.lock` unchanged.
- **Restart safety** is unchanged: the floodfill store is reconstructed from the same bounded
  configuration, and no new persisted state is introduced.
- **Type 5 remains non-advertised.** This plan changes what the controlled floodfill *can* do; it
  does not change what the daemon advertises or publishes.

## Documentation and operational evidence

- `AGENTS.md` routine floor includes `bash scripts/check-floodfill-type5-serve.sh`.
- The three constants are exported from `i2pr-netdb` and documented at their definition with the
  reason type 5 belongs in the LeaseSet list.
- `lookup_body` carries a comment naming the failure mode the guard exists to prevent.
- No operator-facing behaviour change: the type-5 store/serve path is reachable only through the
  controlled floodfill lane, which is opt-in.

## Known limitations

1. **This is not cross-router evidence.** The consumer in the decrypt round-trip row is i2pr's
   own implementation. A stock Java I2P or i2pd client has still not been observed completing
   this chain; that is Plan 347's re-attempt, which now needs only the reference-side drivers.
2. **The type-5 *consumer* path in i2pr is still absent** and is not in scope here. Plan 349 owns
   it, and it blocks the `i2pd → i2pr` and `Java → i2pr` rows.
3. **The Java caps boundary remains closed** and is correctly so: it is a tunnel-peering gate
   (`TunnelPeerSelector.shouldExclude` arity plus `allowAsIBGW`'s `R` requirement) that no row of
   this matrix triggers, because Java reaches i2pr as a queried floodfill. ADR 0030's
   no-fabricated-tier rule is untouched and unused.
4. **`wiki.i2p.org` was unreachable** from the environment used for the Plan 350 boundary audit,
   so project test-network documentation there could not be read. Recorded rather than assumed
   absent; it does not affect this plan, which is a code change with no topology component.

## Findings by severity

- **Critical:** none.
- **High:** none.
- **Medium:** none introduced.
- **Low (correctness, fixed here):** the type-5 store refusal and the type-5 lookup omission
  were two independent silent failures. The store refusal was a deliberate hold-back whose
  rationale expired with Plan 346; the lookup omission looks like the hold-back's counterpart
  and was never justified. Both are fixed and guarded.
- **Low (process):** Plan 347's boundary was written from a read that missed the `handle` guard,
  and Plan 350 was registered from it. Reading the code before editing it caught the error, but
  the plan document itself carried the wrong premise for one commit before being corrected. A
  boundary audit that asserts a path *works* should execute or cite the line that makes it work;
  citing the presence of a `validate` arm is not evidence that the arm is reachable.
- **Low (informational):** the `Ambiguous`-style observation from Plan 346 does not apply here.
  With the offline-key flag set, the record is signed by the *transient* key and the delegation
  block by the *blinded* key. The first version of the offline-keys fixture signed the record
  with the blinded key and was correctly refused as `Invalid` — the validator was right and the
  fixture was wrong. Worth recording because it is the kind of thing that reads as a validator
  bug on first sight.

No corrective pass is required.

## Roadmap disposition and unblock audit

`plans/subsystems/red25525-encrypted-leaseset-roadmap.md` §8 records Plan 350 as `passed`.

**Unblock audit** over registered plans depending on Plan 350:

| Plan | Dependency on 350 | Disposition |
|---|---|---|
| 347 | none directly, but 350 removes a blocker from two of its rows | **still blocked**, now on the reference-side ELS2 drivers (both references) and Plan 349 (the two consumer rows). Not unblocked to `ready` — the lane work remains. |
| 349 | none | **stays `ready`**; 350 is independent of it and was deliberately kept separate so this cheaper half could land first. |
| 348 | via 347 | **stays blocked** on 342 and 347. |

Nothing was silently unblocked, and no historical closure was rewritten. Plan 347's record was
edited only to correct its own boundary and to add the correction section, and its
`stopped-…` status token is unchanged.
