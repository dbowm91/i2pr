# Plan 113 closure: inbound short-build specification/reference reconciliation

- Status: **passed-inbound-reference-reconciliation; local result retained**
- Date: 2026-08-17 post-Plan-114 authority reconciliation
- Plan-of-record: [`plans/113-inbound-short-build-spec-reference-reconciliation.md`](113-inbound-short-build-spec-reference-reconciliation.md)
- Evidence: [`specs/references/short-build-inbound-creator-key.md`](../specs/references/short-build-inbound-creator-key.md)
- Terminal-routing successor: [`plans/114-status.md`](114-status.md) — **passed**
- Active external-evidence successor: [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md)

## Current authoritative state

Plan 113's inbound standards/reference decision remains valid and closed. Its `reference-compatible-spec-text-discrepancy` policy is not reopened.

The later high-level routing/composition defects discovered after Plan 113 were corrected by Plan 114. The old `blocked-on-plan114` handoff is superseded.

```text
plan_113                    = passed-inbound-reference-reconciliation
inbound_short_build_policy  = locally-reference-compatible
creator_key_semantics       = deployed-reference-policy
spec_text_discrepancy       = documented
originator_fake             = implemented-and-integrity-checked

plan_114                    = passed-terminal-routing-chain-correction
terminal_routing_high_level = corrected
intermediate_tunnel_chain   = validated
high_level_inbound_e2e      = strict-established
high_level_outbound_e2e     = strict-established

plan_115                    = ready-qualified-independent-consumption-and-delivery
qualified_external_delivery = active-plan115

normal_daemon_ntcp2         = disabled-and-unenableable
ntcp2                       = experimental-non-advertised
```

## Retained Plan 113 decision

Policy B remains selected. The final ECIES-X25519 specification mentions an inbound creator ephemeral public key in plaintext but does not define a serializable location. The pinned Java I2P and i2pd implementation evidence recorded by Plan 113 agreed on the deployed construction used by i2pr: normal fixed short-request fields plus Mapping/padding, and one separate originator fake with:

```text
hash16 || fresh X25519 pub32 || random remainder
```

i2pr follows that policy without claiming strict final-spec text conformance for the unresolved prose.

The high-level path requires an explicit inbound creator identity hash; the first remote hop is `InboundGateway`, later remote hops are `Participant`, exactly one originator fake is randomized into the record set, and creator-side integrity is checked after reply postprocessing.

Plan 114 preserved this policy while correcting only terminal-routing metadata and forwarding-chain invariants.

## What Plan 115 may and may not conclude about inbound behavior

Plan 115's minimum Q0 criterion requires one independent implementation to natively consume a production-generated short build. Outbound is the first required case because it is the smallest useful independent protocol check.

If outbound Q0 passes, an equivalent inbound Q0 run is useful secondary evidence when the selected reference API exposes it cheaply. Inbound Q0 is not allowed to reinterpret the Plan 113 policy merely because the reference helper exposes a different testing surface.

If independent evidence reproducibly shows that the Plan 113 deployed-reference policy is wrong, Plan 115 must stop at a localized protocol defect and create a new narrow corrective plan. It must not patch the inbound crypto/layout inline.

## Local closure checks retained from Plan 113

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

No live router, public network, NTCP2 activation, or external delivery claim was part of Plan 113 closure. Those concerns are now owned by Plan 115 under its explicitly bounded evidence tiers.
