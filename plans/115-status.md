# Plan 115 closure: independent short-build Q0

- Status: **passed-emissary-q0-construction-and-obep-reply-only**
- Q0 completion date: **2026-08-18**
- Plan-of-record:
  [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md)
- Corrective/completion plan:
  [`plans/115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md)
- Predecessor: [`plans/114-status.md`](114-status.md)
- Immediate successor:
  [`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)

## Closure result

Plan 115's canonical i2pr production bridge and its independent native-consumer
Q0 both passed at the level required to resume local Milestone 5 construction.

Production source path exercised:

```text
ShortBuildStateMachine::prepare
 -> deliver_action
 -> ShortBuildI2npBridge
 -> complete standard-header I2NP type-25 message
 -> independent Emissary standard-I2NP parser
 -> independent Emissary native short-build processor
 -> accepted OBEP reply path
```

Pinned reference:

```text
reference_kind       = emissary
reference_repository = https://github.com/eepnet/emissary.git
reference_revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
reference_package    = emissary-core 0.4.0
i2pr_source_commit   = 1e15239e1849ed24c294252ad16a5fb7bc7e4318
reference_test_name  = i2pr_production_stbm_is_consumed_by_emissary_obep
```

Focused command used in the temporary pinned Emissary checkout:

```text
cargo test -p emissary-core --lib i2pr_production_stbm_is_consumed_by_emissary_obep -- --nocapture
```

The test used i2pr's `i2pr-proto` and `i2pr-tunnel` as test-only path
dependencies. Emissary production source was not changed.

## Independent stages reached

The observed pass demonstrates that the independent implementation reached its
native short-build handling path, including enough processing to:

```text
parse complete standard I2NP type 25
target the i2pr-produced encrypted record
perform native short-build request processing
accept the OBEP tunnel role
construct the OBEP reply route
return TunnelGateway with Garlic inner message
return native feedback channel for accepted transit state
```

This is not a parser-only result.

## Sanitized observed evidence

Stable structural fields:

| Field | Value |
| --- | --- |
| `record_count` | `4` |
| `stbm_body_length` | `873` (`1 + 4 * 218`) |
| `i2np_encoded_length` | `889` (`16 + 873`) |
| `reference_decision` | `passed` |
| `returned_message_type` | `TunnelGateway` |
| `returned_reply_tunnel_matches` | `true` |
| `raw_secret_material_retained` | `false` |

One observed run produced:

| Field | Value |
| --- | --- |
| `stbm_body_sha256` | `f219f426771a886ed24ee52cb95bfb325081e2697b8c52bf82449041cd4ae3c2` |
| `i2np_encoded_sha256` | `3c7546644778fb7e87fd9798bc006c54121b95b518820f3231b9d82bf70e1abf` |

These message digests are intentionally not stable across runs because the
reference test generates a fresh router/X25519 key and i2pr uses fresh
cryptographic randomness. Record count and framing lengths are the stable
structural values.

## Evidence-bookkeeping limitation

The temporary Emissary checkout and test patch were deleted after execution as
planned. The corrective plan requested recording a SHA-256 of that temporary
test-only patch, but that patch digest was not retained.

Record:

```text
reference_patch_sha256 = not-retained
classification         = evidence-bookkeeping-limitation
q0_rerun_required      = false
```

The pinned source revision, test name, production i2pr source commit, structural
lengths, observed message digests, and native reference decision remain
recorded. Absence of the temporary patch digest is **not** affirmative evidence
of a protocol defect and must not trigger another validation cycle.

## Q0 scope boundary

Plan 115 Q0 proves only:

```text
production short-build construction
canonical type-25 I2NP bridge
independent native OBEP consumption/reply construction
```

It does not prove:

```text
Q1 authenticated transport delivery
Q2 returned live reply -> i2pr Established
live mixed-router TunnelData
Milestone 3 NTCP2 exit
Milestone 5 mixed-router exit
```

Those claims remain deferred.

## Current authoritative state

```text
plan_111                          = retained-core-crypto-corrected
plan_112                          = passed-outbound-pre-delivery-closure
plan_113                          = passed-inbound-reference-reconciliation
plan_114                          = passed-terminal-routing-chain-correction
plan_115                          = passed-emissary-q0-construction-and-obep-reply-only
plan_115_q0                       = passed
short_build_local_outbound        = strict-established
short_build_local_inbound         = strict-established
canonical_i2np_bridge             = locally-conformant-no-double-prefix
independent_short_build           = passed-emissary-q0-native-consumer
Q1_authenticated_transport        = deferred
Q2_external_return_established    = deferred
qualified_live_delivery           = deferred
plan_116_local_data_plane         = unblocked-and-next
plan_117_live_integration         = blocked-until-plan116-passes
milestone3_two_reference_transport = still-requires-qualified-lane
milestone5_mixed_router_exit      = still-requires-data-plane-and-live-evidence
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                              = experimental-non-advertised
```

## Historical Branch E

The earlier `blocked-no-bounded-independent-consumer-seam` conclusion is
historical and superseded. It remains available in Git history, including the
Plan 115 Branch E commit and the later Emissary-Q0 corrective planning commits.
Do not reproduce the large historical Branch E block in current handoff/status
documents.

## Successor rule

Plan 116 is executable now.

Do not go directly to Plan 117 and do not wait for rootless namespaces,
Multipass, or an NTCP2 Q1/Q2 pass before implementing the local TunnelData data
plane.

See:

- [`plans/116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md)
- [`plans/116-handoff.md`](116-handoff.md)
