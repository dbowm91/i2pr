# Plan 113 closure: inbound short-build specification/reference reconciliation

- Status: **passed-inbound-reference-reconciliation; local result retained**
- Date: 2026-08-17 post-closure handoff amendment
- Plan-of-record: [`plans/113-inbound-short-build-spec-reference-reconciliation.md`](113-inbound-short-build-spec-reference-reconciliation.md)
- Evidence: [`specs/references/short-build-inbound-creator-key.md`](../specs/references/short-build-inbound-creator-key.md)
- Active successor before external delivery: [`plans/114-short-build-terminal-routing-chain-correction.md`](114-short-build-terminal-routing-chain-correction.md)

## Post-closure handoff amendment

Plan 113's inbound standards/reference decision remains valid and closed. Its `reference-compatible-spec-text-discrepancy` policy is not reopened.

A later audit found a separate high-level routing/composition defect in `ShortBuildPath -> build_hop_specs()`:

- the terminal real hop's `next_router_hash` falls back to the terminal hop's own router hash;
- outbound `ShortBuildPath` cannot explicitly represent the OBEP reply-router identity;
- intermediate `next_tunnel` values are not required to equal the following hop's `receive_tunnel` value;
- the current high-level E2E success test is permissive enough to accept `InvalidReply`.

Those defects are owned by Plan 114. Therefore the previous Plan 113 statement that inbound/outbound external delivery is immediately eligible is superseded until Plan 114 closes.

Current authoritative state:

```text
plan_113                    = passed-inbound-reference-reconciliation
inbound_short_build_policy  = locally-reference-compatible
creator_key_semantics       = deployed-reference-policy
spec_text_discrepancy       = documented
originator_fake             = implemented-and-integrity-checked

plan_114                    = ready-for-implementation
terminal_routing_high_level = correction-required
qualified_external_delivery = blocked-on-plan114

normal_daemon_ntcp2         = disabled-and-unenableable
ntcp2                       = experimental-non-advertised
```

## Retained Plan 113 decision

Policy B remains selected. The final ECIES-X25519 specification mentions an inbound creator ephemeral public key in plaintext but does not define a serializable location. The pinned Java I2P and i2pd implementations agree on the visible deployed construction: normal fixed short-request fields plus Mapping/padding, and one separate originator fake with

```text
hash16 || fresh X25519 pub32 || random remainder
```

i2pr follows that policy without claiming strict final-spec text conformance for the unresolved prose.

The high-level path requires an explicit inbound creator identity hash; the first remote hop must be `InboundGateway`, later remote hops must be `Participant`, exactly one originator fake is randomized into the record set, and creator-side integrity is checked after reply postprocessing. Outbound paths remain unaffected by this semantic decision.

Plan 114 must preserve all of these behaviors while correcting only routing metadata and forwarding-chain invariants.

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

No live router, public network, NTCP2 activation, or external delivery claim is part of Plan 113 closure. The next executable action is Plan 114. Only after Plan 114 passes may a small independent-router delivery checkpoint begin.
