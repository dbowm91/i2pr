# Plan 272 status — passed M12 record validation/storage; type 5 deferred

Status: **passed-m12-record-validation-storage-types-1-3-7-type5-deferred**

Implementation commit: recorded in Git history for this closure.

## Requirement-to-evidence

| Requirement | Evidence |
|---|---|
| Correct LS2 flag assignments, offline delegation parsing and signature verification | `crates/i2pr-proto/src/common/lease2.rs`; `crates/i2pr-crypto/src/lib.rs`; `crates/i2pr-netdb/src/lease_set2.rs` |
| Canonical classic LeaseSet and MetaLeaseSet validation/storage | `crates/i2pr-netdb/src/lease_set.rs` |
| Typed I2NP DatabaseStore payloads for supported types 1/3/7; type 5 stays deferred | `crates/i2pr-proto/src/i2np/netdb.rs`; `crates/i2pr-proto/src/i2np/message.rs`; `crates/i2pr-netdb/src/server_store.rs` |
| Provenance-aware server authority, answer/replication policy, count and byte caps | `crates/i2pr-netdb/src/server_store.rs`; `crates/i2pr-netdb/src/provenance.rs` |
| Fuzz smoke coverage for LS2 and MetaLeaseSet | `fuzz/fuzz_targets/leaseset2.rs`; `fuzz/fuzz_targets/metaleaseset.rs`; `scripts/fuzz-smoke.sh` |
| Support statements distinguish storage infrastructure from serving | `specs/support.toml`; `specs/protocols/04-reseed-netdb.md`; `docs/architecture/i2pr-netdb.md` |
| Runtime/dependency boundary guard for M12 record floor | `scripts/check-m12-floodfill-boundaries.sh` |

## Verification

All commands were run locally on the implementation checkout:

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-proto --all-targets` — 154 passed.
- `rtk cargo test --locked -p i2pr-crypto --all-targets` — 52 passed.
- `rtk cargo test --locked -p i2pr-netdb --all-targets` — 148 passed.
- `rtk cargo test --locked -p i2pr-daemon --test netdb_integration` — 38 passed.
- `rtk cargo clippy --locked -p i2pr-proto -p i2pr-crypto -p i2pr-netdb --all-targets -- -D warnings` — passed.
- `rtk bash scripts/fuzz-smoke.sh` — passed, including both added targets.
- `rtk bash scripts/check-dependency-direction.sh` — passed.
- `rtk bash scripts/check-runtime-boundaries.sh` — passed.
- `rtk bash scripts/check-m12-floodfill-boundaries.sh` — passed.

## Compatibility, security, and limits

Type 5 EncryptedLeaseSet remains opaque/deferred and cannot enter validated server-authority
storage. Type 7 support is limited to the implemented MetaLeaseSet structure and supported
Ed25519 offline-signature path; unknown key/signature families fail closed. Unpublished,
blinded-on-publication, and expired records are not answerable. This is validation and bounded
storage infrastructure only: no DatabaseStore service, reply, replication action, persistence,
daemon role, or floodfill advertisement is implemented here.

No dependency was added. The fuzz lockfile was refreshed by the fuzz smoke workflow; fuzz build
reported existing kebab-case target-name and deprecated `Atomic::fetch_update` warnings, with a
successful exit. No unresolved critical or high findings were identified. Medium/low findings:
none recorded.

## Unblock audit and roadmap disposition

Plan 273 lists Plan 272 as its sole hard predecessor; its message/record interface is now
implemented and documented by `ValidatedNetDbRecord` and `ServerNetDb`. Move Plan 273 to ready.
Plan 274 remains blocked on Plan 273; Plans 275–279 remain blocked in declared sequence. Plan
280 remains stopped pending a vetted I2P-compatible Red25519 provider; Plan 281's type-5
deferral remains authoritative. No other blocked plan is unblocked by this closure.
