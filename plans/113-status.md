# Plan 113 closure: inbound short-build specification/reference reconciliation

- Status: **passed-inbound-reference-reconciliation**
- Date: 2026-08-15
- Plan-of-record: [`plans/113-inbound-short-build-spec-reference-reconciliation.md`](113-inbound-short-build-spec-reference-reconciliation.md)
- Evidence: [`specs/references/short-build-inbound-creator-key.md`](../specs/references/short-build-inbound-creator-key.md)

## Authoritative state

```text
plan_113                    = passed-inbound-reference-reconciliation
inbound_short_build         = locally-reference-compatible
creator_key_semantics       = deployed-reference-policy
spec_text_discrepancy       = documented
originator_fake             = implemented-and-integrity-checked
inbound_external_delivery   = eligible-for-independent-check
outbound_short_build        = unaffected-and-locally-conformant
ntcp2                       = experimental-non-advertised
normal_daemon_ntcp2         = disabled-and-unenableable
```

## Decision

Policy B is selected. The final ECIES-X25519 specification mentions an inbound
creator ephemeral public key in plaintext but does not define a serializable
location. The pinned Java I2P and i2pd implementations agree on the visible
deployed construction: normal fixed short-request fields plus Mapping/padding,
and one separate originator fake with
`hash16 || fresh X25519 pub32 || random remainder`. i2pr follows that policy
without claiming strict final-spec text conformance for the unresolved prose.

The high-level path now requires an explicit inbound creator identity hash;
the first remote hop must be `InboundGateway`, later remote hops must be
`Participant`, exactly one originator fake is randomized into the record set,
and creator-side integrity is checked after reply postprocessing. Outbound
paths remain unchanged and do not require originator identity material.

## Local closure checks

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

No live router, public network, NTCP2 activation, or external delivery claim
is part of this closure. The next inbound-specific checkpoint is a small
independent-router delivery test using the existing message semantics.
