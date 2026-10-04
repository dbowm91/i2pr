# Plan 324 — Destination signing/encryption policy closure

Status: **passed-prop170-destination-signing-and-encryption-policy**

Implementation commit: `dd99798` (`Implement Proposal 170 destination crypto policy`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Canonical SigType and EncType names decode with exact string values and apply to every Proposal TunnelManager type. | `crates/i2pr-i2pcontrol/src/tunnel_request.rs`; `crates/i2pr-i2pcontrol/src/proposal_tunnel_matrix.rs`; `cargo test --locked -p i2pr-i2pcontrol --test contract plan289_tunnel_request_envelope_rules -- --test-threads=1`; `cargo test --locked -p i2pr-daemon --lib plan293_deep_primitives_rejected_with_named_limitation -- --test-threads=1` | Passed. SigType 7 and active LeaseSet2 EncType 4 are accepted; unsupported algorithms and values reject before allocation. |
| Selected crypto policy belongs to the existing Destination identity owner and cannot conflict within a shared group. | `crates/i2pr-service-tunnels/src/config.rs`; `cargo test --locked -p i2pr-service-tunnels --all-targets` | Passed. The typed policy preserves existing group/storage ownership and rejects mixed policies. |
| Generated/restored signing type and actual published LeaseSet2 encryption key match the configured policy. | `crates/i2pr-daemon/src/service_tunnels.rs`; `cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels tunnelmanager_emits_canonical_proposal_result_and_redacts_secrets -- --test-threads=1` | Passed. Runtime validation checks SigType 7 and requires the single active type-4 key bytes to match the identity's X25519 public key. |
| Get returns canonical persisted options and a same-policy edit does not replace identity material. | `crates/i2pr-daemon/tests/i2pcontrol_tunnels.rs`; same focused product test above | Passed. Canonical rawConfig contains selected values; serialized Destination identity is unchanged after a no-op policy edit. |
| Differential disposition remains deterministic and truthful. | `crates/i2pr-daemon/tests/i2pcontrol_differential.rs`; `cargo test --locked -p i2pr-daemon --test i2pcontrol_differential differential_corpus_against_production_composition -- --test-threads=1` | Passed. The stale Plan 293 SigType rejection probe was replaced by a valid client request that checks exact rejection of unsupported DSA-SHA1. |
| Support inventory and option determinations match implemented scope. | `specs/support.toml`; `specs/protocols/13-tunnel-option-matrix.md`; `specs/protocols/14-tunnel-deep-option-determinations.md`; I2PControl acceptance-evidence guard | Passed. The Plan 293 record is preserved as historical authority and explicitly overridden for the current generated types; the new surface is experimental and non-advertised. |

## Verification

Local commands and outcomes:

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed: 3,775 passed, 35 ignored, 133 suites. The first run found a stale differential expectation for the newly supported SigType 7; the probe was corrected to check unsupported DSA-SHA1, its focused test passed, and the complete workspace rerun passed.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed (19 suites).
- `bash scripts/check-dependency-direction.sh` — passed.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- `bash scripts/check-service-tunnel-boundaries.sh` — passed.
- `bash scripts/check-i2pcontrol-acceptance-evidence.sh` — passed.
- `bash scripts/check-exploratory-tunnel-evidence.sh` — passed.
- `bash scripts/check-i2cp-acceptance-evidence.sh` — passed.
- `git diff --check` — passed.

These are local results, not CI claims. No dependency was added or changed. `cargo deny` and unrelated protocol-vector/external-lane checks were not rerun for this plan.

## Compatibility, migration, and security

No key format or persisted identity migration is introduced. Existing stored identity bytes and the separate legacy Destination public-key slot remain unchanged. For both persistent and ephemeral owners, accepted policy is checked against the identity and actual LeaseSet2 key before product publication. Invalid choices fail before listener, destination, or persistent-key allocation. An explicit type-7/type-4 no-op edit retains the existing identity. No alternate supported algorithm exists, so public-identity replacement between algorithms cannot currently occur; Red25519 remains gated by Plan 325.

No private key bytes or proxy secrets are added to logs or control responses. The runtime uses existing identity-storage and manager ownership paths; it introduces no new lock, queue, background task, or concurrent rotation mechanism. Shared groups reject members with different crypto policies.

## Limitations and findings

- Supported generation is limited to SigType 7 EdDSA-SHA512/Ed25519 and active Standard LeaseSet2 EncType 4 X25519. All other values reject explicitly.
- Red25519 type 11 remains unavailable pending Plan 325's qualified provider. Plan 326 therefore remains blocked on Plan 325.
- Encrypted/blinded LeaseSet2 and client authorization remain Plan 326 scope. I2P-routed outproxy remains Plan 327 scope.
- Findings by severity: critical 0; high 0; medium 0; low 0.

## Roadmap disposition

Plan 324 is closed. Plans 323 and 324 are now satisfied prerequisites for Plan 326; its status is narrowed from blocked on 323/324/325 to blocked only on Plan 325. Plan 327 is already active after Plan 323 and remains eligible. Plan 322 remains active. Plan 328 remains blocked on Plans 322, 326, and 327. No other dependency is newly unblocked, and this parallel workstream does not change M12/mainline readiness or any protocol advertisement.
