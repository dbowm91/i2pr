# Plan 115 completion: bounded Emissary native short-build Q0

## Status

- **Ready for execution**.
- Date: 2026-08-17.
- Corrective amendment to
  [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md).
- Supersedes only the Plan 115 Branch E conclusion that this environment has no
  bounded independent short-build consumer.
- Does **not** reopen Plans 109-114 or the historical NTCP2 interop harness.

## 1. Objective

Answer one narrow question with one bounded independent test:

> Can pinned upstream Emissary execute its native short-build processor against
> the exact I2NP type-25 message produced by i2pr's production short-build
> state machine and bridge?

If yes, move directly to Plan 116 local tunnel-data-plane implementation.

If Emissary reaches native processing and exposes a reproducible i2pr protocol
error, localize that one defect and correct it once.

If the reference test itself cannot execute after this plan's fixed build/test
budget, record external Q0 as deferred and **still move to Plan 116**. An
environment/reference-build limitation is not a protocol defect.

Q1 authenticated transport delivery and Q2 live return-to-`Established` are
explicitly deferred.

## 2. Research result and pinned reference

Use upstream Emissary, not the i2pr project's fork:

```text
repository = https://github.com/eepnet/emissary.git
revision   = 9b43484a21d5a1291c4881cdae62a36c527f8c0f
package    = emissary-core 0.4.0
```

At this revision:

- `emissary-core/src/i2np/tunnel/build/short.rs` owns the native 154-byte short
  request parser.
- `emissary-core/src/tunnel/transit/mod.rs` exposes
  `TransitTunnelManager::handle_short_tunnel_build()`.
- That handler performs target-record selection, Noise-N request decryption,
  request parsing, admission, tunnel-key derivation, reply-record processing,
  and forwarding/OBEP reply composition.
- `emissary-core/src/tunnel/tests/mod.rs` already provides
  `TestTransitTunnelManager<MockRuntime>`, exposes the generated router hash and
  X25519 public key, and delegates directly to the production native handler.
- Emissary's own tests call this path directly for participant, IBGW, and OBEP
  cases without starting a daemon or network transport.

This is the bounded native consumer Plan 115 was intended to find.

The current official I2P specification defines ShortTunnelBuild as I2NP type
25 with body `count || count*218-byte records`, `count` in `1..=8`. Each short
encrypted request record is `hash-prefix[16] || ephemeral-X25519[32] ||
ciphertext[154] || Poly1305[16]`.

Primary references:

- <https://geti2p.net/spec/tunnel-creation-ecies>
- <https://geti2p.net/spec/i2np>
- <https://github.com/eepnet/emissary/blob/9b43484a21d5a1291c4881cdae62a36c527f8c0f/emissary-core/src/i2np/tunnel/build/short.rs>
- <https://github.com/eepnet/emissary/blob/9b43484a21d5a1291c4881cdae62a36c527f8c0f/emissary-core/src/tunnel/transit/mod.rs>
- <https://github.com/eepnet/emissary/blob/9b43484a21d5a1291c4881cdae62a36c527f8c0f/emissary-core/src/tunnel/tests/mod.rs>

## 3. Corrected gate semantics

```text
Q0 independent native short-build consumption = bounded attempt now
Q1 authenticated transport delivery            = deferred
Q2 live reply -> i2pr Established               = deferred
```

Plan 116 local data-plane work is blocked only by **affirmative native evidence
of a short-build protocol defect**.

It is not blocked by:

- absence of rootless namespaces;
- absence of Multipass/VM support;
- NTCP2 remaining disabled;
- failure to start a reference daemon;
- inability to perform live wire delivery;
- a reference build/tooling failure before native short-build processing.

This is the anti-loop rule for this line of development.

## 4. Scope

Mandatory:

1. pin upstream Emissary at the revision above;
2. use its existing `TestTransitTunnelManager<MockRuntime>` seam;
3. let Emissary own/generate the reference router identity and private key;
4. give i2pr only the Emissary router hash and X25519 public key;
5. generate one production i2pr outbound STBM through
   `ShortBuildStateMachine::prepare -> deliver_action`;
6. wrap it through `ShortBuildI2npBridge` as a complete standard-header I2NP
   type-25 message;
7. parse that message using Emissary's own `Message::parse_standard`;
8. pass the parsed message to Emissary's production
   `handle_short_tunnel_build()`;
9. require native acceptance and OBEP reply composition;
10. record only sanitized digests/stages.

Forbidden:

- NTCP2 or SSU2 changes;
- Java I2P;
- another i2pd adapter;
- daemon startup;
- rootless/privileged namespaces;
- Docker, Multipass, or other VMs;
- public I2P participation;
- Python orchestration;
- a permanent cross-project harness;
- generic I2NP dispatch infrastructure;
- inbound garlic integration;
- Q1/Q2;
- speculative edits to Plans 109-114 crypto.

## 5. Minimum test topology

Use a **one-real-hop outbound build** whose only real hop is the Emissary router
with role `OutboundEndpoint`.

This is enough for Q0 and avoids fake complexity. It causes Emissary to execute:

```text
I2NP type-25 parse
 -> local hash-prefix record match
 -> X25519/Noise-N request decryption
 -> short request parser
 -> role/tunnel/next-router field consumption
 -> transit admission
 -> tunnel/garlic key derivation
 -> reply-record transformation
 -> OBEP TunnelGateway/Garlic reply construction
```

Do not require an independently routed inbound build in this pass. That would
pull garlic-delivery/data-plane work forward and defeat the purpose of the
bounded probe.

## 6. Execution method

### 6.1 Temporary checkout only

Recommended preparation:

```bash
set -euo pipefail
I2PR_ROOT="$(git rev-parse --show-toplevel)"
WORK="$(mktemp -d)"
git clone https://github.com/eepnet/emissary.git "$WORK/emissary"
git -C "$WORK/emissary" checkout --detach 9b43484a21d5a1291c4881cdae62a36c527f8c0f
ln -s "$I2PR_ROOT" "$WORK/emissary/i2pr-under-test"
```

Delete the temporary checkout at the end.

Do not vendor Emissary into i2pr.

### 6.2 Test-only reference patch

Make only two conceptual changes in the temporary checkout:

1. add these `emissary-core` dev dependencies:

   ```toml
   i2pr-proto = { path = "../i2pr-under-test/crates/i2pr-proto" }
   i2pr-tunnel = { path = "../i2pr-under-test/crates/i2pr-tunnel" }
   ```

2. add one `#[tokio::test]` inside Emissary's existing tunnel test module.

Do not change Emissary production code or expose a new public API.

Record the SHA-256 of this temporary patch in the final Plan 115 status. Do not
commit the patch or temporary checkout to i2pr.

## 7. Required test construction

The exact import/type spelling may be adjusted for the pinned APIs. The
following data flow and assertions are mandatory.

### A. Build the reference router

```rust
let mut reference = TestTransitTunnelManager::new(true);
let router_hash = reference.router_hash();
let static_public = reference.public_key();
```

Convert the 32-byte hash/public-key values into the corresponding i2pr public
value types. Do not export or serialize the Emissary private key.

### B. Build a one-hop i2pr outbound path

Use:

```text
direction              = Outbound
originator_hash         = None
outbound_reply_router   = explicit fixed non-secret 32-byte test hash
hops                    = one Emissary hop
hop role                = OutboundEndpoint
receive_tunnel          = explicit nonzero value
hop next_tunnel         = explicit nonzero reply tunnel
creator_tunnel_id       = distinct explicit nonzero value
next_message_id         = explicit nonzero value
request_time            = valid nonzero current/mock time
options                 = empty/default
```

Construct the hop with the Emissary router hash and Emissary X25519 static
public key.

### C. Use the production i2pr source path

Mandatory sequence:

```rust
let mut state = i2pr_tunnel::short::ShortBuildStateMachine::new(path, deadline);
let stbm = state.prepare(&mut rng)?;
let action = state.deliver_action(stbm)?;
let (i2np, record) = ShortBuildI2npBridge::new().wrap_deliver_action(
    &action,
    BridgeHeader::Standard { message_id, expiration_ms },
)?;
let encoded = i2np.encode_standard_to_vec(i2pr_proto::MAX_I2NP_PAYLOAD_SIZE)?;
```

Do not hand-build the STBM body or I2NP header.

### D. Require independent I2NP agreement

With Emissary:

```rust
let parsed = Message::parse_standard(&encoded)?;
```

Require:

```text
parsed.message_type       = ShortTunnelBuild
parsed.payload            = exact ShortBuildAction::Deliver.message bytes
parsed.serialize_standard = exact original encoded bytes
```

The reserialization equality gives an independent check of the complete
standard I2NP envelope rather than relying only on i2pr's own decoder.

### E. Require native short-build acceptance

Call:

```rust
let (next_router, response, feedback) =
    reference.handle_short_tunnel_build(parsed)?;
```

A Q0 pass requires all of:

```text
feedback is Some(...)               # native transit admission accepted
next_router == i2pr reply router
response.message_type == TunnelGateway
TunnelGateway.tunnel_id == i2pr hop next_tunnel
TunnelGateway inner standard message type == Garlic
```

Do not require decrypting the garlic-wrapped OTBRM. That is Q2/integration
scope and is deliberately deferred.

## 8. Stage and failure taxonomy

Highest-stage labels:

```text
q0_reference_checkout_pinned
q0_i2pr_stbm_generated
q0_i2pr_i2np_encoded
q0_emissary_i2np_parsed
q0_emissary_target_record_found
q0_emissary_request_decrypted
q0_emissary_request_parsed
q0_emissary_policy_accepted
q0_emissary_obep_reply_constructed
```

Failure labels:

```text
failed-reference-build
failed-i2pr-path-construction
failed-i2pr-i2np-envelope
failed-emissary-i2np-parse
failed-emissary-target-record-not-found
failed-emissary-build-record-decrypt
failed-emissary-build-request-fields
rejected-by-emissary-policy
failed-emissary-obep-reply-composition
```

Never collapse these into generic `failed`.

## 9. Strict attempt budget

Reference test budget:

```text
1 baseline compile/test attempt
1 direct correction for trivial path/import/dev-dependency/API spelling
1 confirmation attempt
STOP
```

Do not switch references inside this completion pass.

If native Emissary processing localizes a protocol disagreement, allow exactly
one narrow i2pr protocol correction followed by the same focused probe once.
Do not create a broader harness.

Zero transport corrections are authorized.

## 10. Local i2pr preflight

Before the reference probe:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Do not make unrelated production changes for the reference test.

## 11. Focused reference command

Run only the new unit test first, using its actual path, e.g.:

```bash
cargo test -p emissary-core \
  tunnel::tests::i2pr_production_stbm_is_consumed_by_emissary_obep \
  -- --exact
```

If necessary, use `cargo test -p emissary-core -- --list` only to resolve the
exact test selector.

A broad Emissary suite is not required because production Emissary code is
unchanged.

## 12. Durable evidence

Record only:

```text
i2pr_source_commit
reference_repository
reference_revision
reference_version
reference_patch_sha256
reference_test_name
reference_command
stbm_record_count
stbm_body_length
stbm_body_sha256
i2np_encoded_length
i2np_encoded_sha256
reference_highest_stage
reference_decision
returned_message_type
returned_reply_tunnel_matches
raw_secret_material_retained = false
```

Do not commit private keys, ephemeral keys, raw STBM records, decrypted request
plaintext, raw OTBRM/garlic bytes, full logs, or temporary paths.

## 13. Closure branches

### Branch A — native Q0 passes

```text
plan_115                     = passed-independent-native-consumer-emissary
Q0                           = passed
Q1                           = deferred
Q2                           = deferred
independent_short_build      = passed-independent-native-consumer
qualified_live_delivery      = deferred-not-required-for-plan116
plan_116_local_data_plane    = unblocked
normal_daemon_ntcp2          = disabled-and-unenableable
ntcp2                         = experimental-non-advertised
```

Proceed directly to Plan 116. No additional short-build validation plan.

### Branch B — native protocol defect localized

This branch requires Emissary to have reached native short-build processing.

```text
plan_115                     = protocol-defect-localized-emissary
reference_highest_stage      = <exact stage>
plan_116_local_data_plane    = temporarily-blocked-on-one-narrow-correction
```

Correct exactly that defect, rerun this same focused probe once, then move on.

### Branch C — reference/build limitation before protocol processing

After the fixed attempt budget:

```text
plan_115                     = external-q0-deferred-environment-or-reference-build
Q0                           = not-observed
short_build_protocol_defect  = not-demonstrated
qualified_live_delivery      = deferred
plan_116_local_data_plane    = unblocked-with-external-evidence-deferred
mixed_router_milestone5_exit = still-blocked
```

Move to Plan 116. Do not write another harness plan.

## 14. Acceptance criteria

This pass is complete when:

1. Emissary is pinned exactly to the specified upstream revision.
2. Reference instrumentation is test-only.
3. Emissary owns the reference identity/private key; i2pr receives only public
   targeting material.
4. STBM bytes come from production `ShortBuildStateMachine` output.
5. Complete type-25 bytes come from production `ShortBuildI2npBridge` output.
6. Emissary independently parses and reserializes the complete I2NP message.
7. The exact i2pr STBM body reaches Emissary's native handler.
8. A pass requires native acceptance, not parser-only success.
9. OBEP output uses the reply router/tunnel declared by i2pr and produces the
   native TunnelGateway/Garlic envelope.
10. No socket, daemon, namespace, VM, container, or transport handshake is used.
11. No new Python or permanent interop framework is added.
12. The attempt budget is obeyed.
13. Durable evidence is sanitized.
14. Temporary reference files are removed.
15. Plan 115 authority is updated to Branch A, B, or C.
16. Plan 116 is unblocked for Branch A or C; only Branch B may temporarily block
    it.

## 15. Handoff summary

```text
verify local i2pr
 -> temporary pinned upstream Emissary checkout
 -> tiny test-only dev-dependency patch
 -> TestTransitTunnelManager generates reference identity/keypair
 -> one-hop i2pr outbound OBEP path targets that reference
 -> production i2pr STBM + production I2NP type-25 bridge
 -> Emissary parse_standard
 -> Emissary handle_short_tunnel_build
 -> require native acceptance + OBEP reply envelope
 -> record sanitized result
 -> delete temporary checkout
 -> move to Plan 116 unless a native protocol defect was actually demonstrated
```

Independent validation remains important, but this environment must no longer
use unavailable live transport infrastructure as a prerequisite for building
transport-neutral router functionality.
