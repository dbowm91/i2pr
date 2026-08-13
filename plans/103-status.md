# Plan 103: closure record

- Status: **implemented and closed on the local host**.
- Date: 2026-08-13.
- Parent authority: Plan 102.
- Baseline: Milestone 3 closure (Plan 099/100/101) with the Plan 101
  NTCP2 activation guard in force.
- Implementation source: `crates/i2pr-netdb/` (`src/lib.rs`,
  `src/router_info.rs`, `src/store.rs`, `src/routing.rs`,
  `src/local.rs`).
- Next executable implementation: **Plan 104** (persistent cache +
  SU3 reseed trust path) under Plan 102 authority.

## Closure summary

Plan 103 created the first stateful router-information subsystem in
`i2pr`. The new runtime-neutral `i2pr-netdb` workspace crate:

1. Derives the canonical `RouterHash` from the encoded
   `RouterIdentity` and refuses to construct a `ValidatedRouterInfo`
   without cryptographic, freshness, and key-binding checks. The
   public entrypoint is `ValidatedRouterInfo::from_router_info`; no
   bypass constructor is exposed.
2. Verifies the signature against the contained signing public key
   and the exact retained signed bytes (delegated to
   `i2pr-crypto::verify_router_info`). Unsupported algorithms,
   invalid signatures, stale publications, excessive future skew,
   key mismatches, and oversize records produce distinct typed
   outcomes (`RouterInfoValidationError`).
3. Provides a bounded `RouterInfoStore` with deterministic
   replacement/conflict/expiry semantics and exact record-count and
   byte-quota accounting. All arithmetic uses checked operations; no
   saturating fallback hides accounting bugs.
4. Exposes pure routing primitives (`xor_distance`, `nearest`,
   `nearest_floodfill`) used by the Plan 105 query state machines.
5. Signs the local `RouterInfo` through the persistent
   `RouterIdentityBundle` via `LocalRouterInfoBuilder`, self-validates
   the result through the same validator, and rejects any attempt
   to advertise a `RouterAddress` — under Plan 101 authority the local
   record carries zero transport addresses.

The crate depends only on `i2pr-proto` and `i2pr-crypto`; it opens
no sockets, performs no filesystem I/O, and does not list `tokio`
as a dependency. The `scripts/check-dependency-direction.sh` and
`scripts/check-runtime-boundaries.sh` scripts enforce the boundary.

## Implementation surface

```text
crates/i2pr-netdb/Cargo.toml
crates/i2pr-netdb/src/lib.rs
crates/i2pr-netdb/src/router_info.rs   (RouterHash, ValidatedRouterInfo, Validator)
crates/i2pr-netdb/src/store.rs         (RouterInfoStore, InsertOutcome, accounting)
crates/i2pr-netdb/src/routing.rs       (xor_distance, nearest, nearest_floodfill)
crates/i2pr-netdb/src/local.rs         (LocalRouterInfoBuilder, LocalRouterInfo)
```

Wiring changes:

- `Cargo.toml` — workspace registration (`crates/i2pr-netdb`).
- `scripts/check-dependency-direction.sh` — allow
  `i2pr-netdb: { i2pr-crypto, i2pr-proto }`.

Documentation and support ledger updates:

- `README.md` — Plan 103 status section, ten-crate workspace text,
  current support claim.
- `AGENTS.md` — ten-crate workspace layout, dependency direction
  allowlist mention.
- `docs/architecture/overview.md` — crate index entry for
  `i2pr-netdb`; roadmap pointer for Plan 103.
- `docs/architecture/dependency-graph.md` — allowlist row and reverse
  edge for `i2pr-netdb`; updated ASCII graph.
- `docs/architecture/interop-apparatus.md` — Plan 103 landing
  paragraph in the active roadmap block.
- `docs/protocol-support.md` — "Reseed and RouterInfo publication"
  row now reflects Plan 103 implementation with explicit
  "persistence/SU3/publication remain Plan 104/105/106 work" wording.
- `specs/support.toml` — `milestone = 4`,
  `plan_103_implementation_floor = "plans/103-status.md"`, four
  new `netdb.*` surfaces for validation, store, peer-selection, and
  local RouterInfo; three new `deferred` entries for the
  Plan 104/105 surfaces.

## Validation commands and results

Each command ran from the repository root on the local host.

```text
$ cargo fmt --all --check
(constant: no output)

$ cargo check --locked --workspace --all-targets
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s

$ cargo test --locked --workspace
300 passed (29 suites)

$ cargo test --locked -p i2pr-netdb
34 passed (2 suites)

$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
No issues found

$ RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
Generated /home/sugarwookie/projects/i2pr/target/doc/i2pr_core/index.html and 11 other files

$ bash scripts/check-dependency-direction.sh
dependency direction: ok

$ bash scripts/check-runtime-boundaries.sh
runtime boundary checks passed

$ bash scripts/check-ntcp2-interoperability.sh
Plan 099 NTCP2 interoperability static check: OK

$ git diff --check
(no output)
```

## Coverage against Plan 103 closure criteria

| Plan 103 criterion | Result |
| --- | --- |
| 1. `i2pr-netdb` exists as a workspace crate with enforced runtime-neutral dependency direction. | Met. `Cargo.toml` registers the crate; `scripts/check-dependency-direction.sh` enforces the allowance; `scripts/check-runtime-boundaries.sh` confirms no Tokio/socket/transport dependency. |
| 2. RouterHash is derived canonically from encoded RouterIdentity and expected-key mismatch is rejected. | Met. `router_hash` derives `SHA256(encoded RouterIdentity)`; `ValidatedRouterInfo::from_router_info` compares `expected_key` and returns `RouterInfoValidationError::KeyMismatch` on mismatch. |
| 3. A caller cannot construct `ValidatedRouterInfo` without cryptographic/time/policy validation through the intended API. | Met. The only constructor is `ValidatedRouterInfo::from_router_info`; all fields are private. |
| 4. Signature verification uses the contained signing public key and exact retained signed bytes. | Met. Validator delegates to `i2pr_crypto::verify_router_info`, which uses `RouterInfo::router_identity().signing_key()`, `RouterInfo::signed_bytes()`, and `RouterInfo::signature()`. |
| 5. Unsupported algorithms, invalid signatures, stale/future records, key mismatch, and resource rejection have distinct typed outcomes. | Met. `RouterInfoValidationError` enumerates `UnsupportedAlgorithm`, `InvalidSignature`, `KeyMismatch`, `Stale`, `ExcessiveFuture`, `EncodedTooLarge`, `ArithmeticOverflow`, and the `Crypto` transparent variant. |
| 6. The in-memory store enforces record and byte quotas with exact accounting. | Met. `RouterInfoStoreConfig` exposes `max_records` and `max_total_encoded_bytes`; all arithmetic uses `checked_add`/`checked_sub`; `is_none_or(|total| total > limit)` enforces the bound. |
| 7. Replacement/conflict/idempotence semantics are deterministic and tested. | Met. `store.rs` covers every cell of the Plan 103 §3.3 matrix (insert, idempotent, replace, stale-replace, conflict, count quota, byte quota, growth replacement, prune, remove). |
| 8. Expired/stale records can be pruned without corrupting accounting. | Met. `RouterInfoStore::prune` removes stale records and the underlying `remove` releases byte accounting through `checked_sub`. |
| 9. Floodfill capability is exposed only as self-advertised metadata. | Met. `ValidatedRouterInfo::advertises_floodfill` parses the signed `caps` mapping without elevating trust; named accordingly. |
| 10. The local RouterInfo is signed by the persistent identity, self-validates through the same path, and advertises no NTCP2 address under current authority. | Met. `LocalRouterInfoBuilder` borrows the `RouterIdentityBundle`, signs through `sign_router_info`, and self-validates; the `RouterAddress` list is empty by construction. |
| 11. No socket, DNS, filesystem, HTTP, Tokio, or transport-implementation dependency entered `i2pr-netdb`. | Met. `Cargo.toml` lists only `i2pr-crypto`, `i2pr-proto`, and `thiserror`; the runtime boundary checker greps no Tokio/socket/transport symbols. |
| 12. Workspace tests/lints/docs and boundary checks pass. | Met. See the validation command results above. |
| 13. Documentation states that NetDB is local/in-memory only and that live lookup/reseed/publication remain unimplemented. | Met. The README, AGENTS.md, protocol-support.md, and the new `specs/support.toml` rows (`netdb.persistent-cache`, `netdb.su3-reseed`, `netdb.query-state-machines`) all record the deferred status. |
| 14. Plan 104 can consume one narrow public validation/store API without an unchecked insertion path. | Met. The narrow public surface is `ValidatedRouterInfo::from_router_info`, `RouterInfoStore::{insert, get, contains, remove, len, encoded_bytes, stats, iter, prune, floodfill_advertisers}`, and `LocalRouterInfoBuilder::{build, build_default, local_router_hash}`. |

## Handoff to Plan 104

The Plan 104 implementation consumes the following stable APIs:

```text
encoded RouterInfo -> structural decode -> validate -> ValidatedRouterInfo
   ValidatedRouterInfo::from_router_info(router_info, expected_key?, context)

ValidatedRouterInfo -> insert/replace outcome
   RouterInfoStore::insert(validated) -> InsertOutcome

store -> canonical record bytes for persistence
   ValidatedRouterInfo::encoded(maximum) -> Vec<u8>

current local RouterInfo -> validated signed snapshot
   LocalRouterInfoBuilder::build(published, options) -> LocalRouterInfo
```

Plan 104 must continue to route every persisted and reseed record
through `ValidatedRouterInfo::from_router_info` before insertion. No
new "trusted cache" or "trusted reseed" insertion path is required —
the same `RouterInfoStore::insert` is the only entry point.

## Status

Plan 103 is closed. NTCP2 remains experimental and non-advertised.
The next executable implementation is **Plan 104** (persistent
NetDB cache + SU3 reseed trust path).
