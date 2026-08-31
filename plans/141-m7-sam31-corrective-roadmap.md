# Plan 141 — Milestone 7 SAM 3.1 corrective roadmap

Status: **active corrective planning authority**.

Depends on: Plan 140 blocked closure audit; Plan 134 remains the Milestone 6 local-product authority.

## 1. Purpose

Recover Milestone 7 from the concrete defects and acceptance gaps exposed by Plan 140 without reopening Milestone 6 generally and without rebuilding the SAM architecture from scratch.

The existing Phase 7 implementation contains useful foundations that should be retained:

- `i2pr-api` as the runtime-neutral SAM protocol layer;
- `i2pr-daemon` as the Tokio/socket composition layer;
- loopback-only, disabled-by-default SAM listener;
- transactional session/destination ownership;
- bounded client/session/stream/accept/resource registries;
- explicit unsupported-feature replies;
- Plan 139 naming and FORWARD ownership/security policy where it does not depend on the missing raw STREAM product path.

The corrective work is limited to three sequential implementation passes:

```text
Plan 142 — SAM encoding/private-destination compatibility correction
Plan 143 — live CONNECT/ACCEPT raw STREAM product bridge
Plan 144 — independent-client validation + FORWARD revalidation + M7 closure
```

Do not create another generalized Milestone 6 audit or another historical harness lane unless one of these passes demonstrates a specific lower-layer defect.

## 2. Why correction is required

### 2.1 SAM Base64 protocol defect

The current implementation in `crates/i2pr-api/src/sam/base64.rs` encodes standard RFC 4648 `+/` Base64 and the current provenance document claims that SAM uses that representation.

The current official SAM v3 specification explicitly says:

```text
Base 64 encoding must use the I2P standard Base 64 alphabet
A-Z, a-z, 0-9, -, ~
```

Reference:

- https://www.i2p.net/en/docs/api/samv3/
- `BASE 64 Notes`

This is a protocol conformance defect, not merely an `i2plib` compatibility quirk. Plan 136's original plan-of-record also required I2P Base64, so the implementation diverged from its own acceptance criteria.

### 2.2 Private-destination evidence is insufficient

The current type-7/type-4 private-destination implementation uses a 455-byte `Destination || X25519 secret || Ed25519 seed` representation and proves it primarily by round-tripping i2pr-generated material through i2pr itself.

Do not assume this representation is wrong merely because the SAM prose retains legacy 663+-byte examples; current key-certificate-aware reference code supports type-specific key lengths. However, the representation is not independently proven. Plan 142 must reconcile it against actual reference behavior and at least one independent artifact before it may be called interoperable.

### 2.3 Plan 138 was closed below its stated acceptance bar

Plan 138 required a real path:

```text
SAM TCP socket
 -> StreamingManager
 -> StreamingDestinationAdapter
 -> destination routing / Garlic / tunnels product seam
 -> peer local destination
 -> reverse path
 -> same SAM TCP socket in raw mode
```

The shipped status explicitly defers or substitutes:

- permanent same-socket raw-mode handoff;
- TCP -> `send_data()` and `drain_delivered()` -> TCP byte bridging;
- per-destination retransmit / delayed-ACK driver;
- slow-reader/slow-writer backpressure acceptance;
- injected loss/reorder/duplicate acceptance through the SAM trajectory;
- live product delivery in place of the capture/established-material test seam.

Therefore Plan 138's historical `passed` label is not sufficient authority for M7 closure. Plan 143 owns the missing product implementation.

### 2.4 Plan 139 remains useful but conditionally closed

The Plan 139 FORWARD registry, loopback-only target policy, naming behavior, resource accounting, and unsupported matrix should be retained. However, the original Plan 139 acceptance trajectory depended on a real Plan 138 stream path. Plan 144 must re-run the FORWARD byte trajectory after Plan 143 lands before M7 closure.

## 3. Authority model

Until Plan 144 passes:

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_136 = protocol-foundation-landed-but-sam-encoding-evidence-superseded-by-plan142
plan_137 = passed-m7-sam31-loopback-server-session-lifecycle
plan_138 = implementation-landed-but-product-acceptance-superseded-by-plan143
plan_139 = local-forward-naming-implementation-landed; final-byte-path-acceptance-deferred-to-plan144
plan_140 = blocked-audit-superseded-by-plan141-corrective-roadmap
plan_141 = active-m7-corrective-roadmap
plan_142 = next-executable
plan_143 = blocked-on-plan142
plan_144 = blocked-on-plan143

milestone7_local_product = not-yet-closed
sam31_stream = not-yet-product-validated
sam_independent_clients = 0-passed
next_product_layer = remain-on-milestone7
```

Closure/status records remain audit history. Do not delete the earlier records; supersede their claims explicitly.

## 4. Cross-plan invariants

All corrective passes must preserve these invariants.

### 4.1 Architecture

- SAM protocol parsing/encoding/typed state remains in `i2pr-api`.
- Tokio sockets/tasks remain in `i2pr-daemon`.
- `i2pr-client` remains unaware of SAM types.
- `i2pr-api` must not depend on `i2pr-daemon`.
- Do not create a second streaming protocol or bypass `StreamingManager`.
- Do not introduce a direct manager-to-manager shortcut into acceptance tests.

### 4.2 Security

- SAM listener remains loopback-only.
- `[sam] enabled = false` remains the default until M7 closes and any later operator policy intentionally changes it.
- private destinations and application payloads are not logged.
- secret-owning types remain non-`Clone` unless a specific audited reason exists.
- secret buffers remain zeroized where practical.
- FORWARD remains loopback-target-only in this MVP phase.

### 4.3 Resource bounds

No unbounded queue/channel/vector may be introduced in the live raw bridge.

The following must remain named and bounded:

- accepted SAM clients;
- sessions;
- stream attachments;
- pending ACCEPTs;
- FORWARD registrations;
- TCP read chunks;
- pending local->I2P bytes;
- pending I2P->local bytes;
- retransmit/ACK work;
- destination-driver tasks.

Router-wide `max_tasks` and `max_buffered_bytes` remain aggregate ceilings above SAM-specific limits.

### 4.4 Environment constraints

The closure path must work on ordinary localhost TCP and the existing Rust test environment.

Do not require:

- root/sudo;
- user/network namespaces;
- Docker;
- Multipass/VMs;
- systemd;
- public I2P participation;
- mixed-router NTCP2/SSU2 operation.

Independent SAM client libraries/tools may be installed or pinned in an optional interoperability lane, but the repository's canonical Rust product tests must remain runnable without them.

## 5. Execution order

### Plan 142 — protocol compatibility correction

Correct I2P Base64 and independently prove the private-destination representation. No raw socket product bridge belongs in this pass.

Exit condition: an independent SAM client/reference fixture can consume i2pr `PUB`/`PRIV`, and i2pr can consume the corresponding independent material.

### Plan 143 — live STREAM product bridge

Replace the capture-only acceptance path with permanent same-socket raw CONNECT/ACCEPT behavior, bounded bidirectional byte flow, destination driver, ACK/retransmit progression, and close/reset handling.

Exit condition: two in-repo SAM protocol clients using only real localhost sockets can establish A<->B and exchange exact arbitrary bytes through the M6 product architecture under loss/reorder/backpressure tests.

### Plan 144 — independent-client closure

Use at least two independent SAM clients against the real listener, rerun FORWARD/naming over the corrected bridge, execute final boundedness/privacy/regression gates, and update all roadmap/support documents.

Exit condition: Milestone 7 closes truthfully and Milestone 8 becomes the next product frontier.

## 6. Rules for smaller-model execution

Each implementation model must:

1. read this roadmap and only the currently executable child plan;
2. inspect the existing source before changing APIs;
3. make the smallest changes needed for that child plan;
4. not pull later-plan work forward merely because nearby code is visible;
5. create the child plan's status file only after every acceptance criterion is proven;
6. record exact commands, test counts, CI run, and known deviations;
7. stop and write a narrow blocker status if an acceptance criterion cannot be met.

Do not mark a plan `passed` while moving mandatory acceptance criteria to a later plan. That was the principal process failure in the original Plan 138 closure.

## 7. Global validation floor

Every child plan must at minimum run:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Retain existing fixture/vector/constrained-host checks when files they cover are touched.

## 8. Final roadmap disposition

Do not begin Milestone 8 / SSU2 implementation planning until Plan 144 either:

- passes Milestone 7; or
- records a new concrete blocker that cannot be resolved within the ordinary localhost environment.

A failure of an obsolete independent client toolchain is not itself a blocker; choose a second viable client. A demonstrated protocol incompatibility in i2pr is a blocker and must be corrected.