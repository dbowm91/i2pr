# Plan 145 — Milestone 7 SAM 3.1 remaining-gap corrective roadmap

Status: **active corrective roadmap; execute Plan 146 next**.

Depends on: Plan 134 local Milestone 6 closure; Plan 141 corrective roadmap; useful implementation work from Plans 142–144.

Supersedes for next-action authority: the `Plan 145 candidate` described in `plans/141-status.md` and `plans/144-status.md`.

## 1. Purpose

The Plan 142–144 implementation sequence produced useful SAM protocol and local-product work, but the acceptance claims are not yet strong enough to close Milestone 7.

This roadmap narrows the remaining work into three independent gates so that one kind of evidence cannot substitute for another:

1. **Plan 146 — private-destination reference compatibility requalification**
   - preserve the correct I2P Base64 fix from Plan 142;
   - determine the actual SAM `PRIV` / PrivateKeyFile representation accepted by current reference implementations;
   - obtain real external generation/import evidence rather than source-derived alphabet vectors;
   - correct the current 455-byte/608-character claim if reference behavior requires the documented 663+/884+ SAM form or another representation.
2. **Plan 147 — dedicated raw STREAM socket driver and product completion**
   - implement the missing permanent command-to-raw socket handoff;
   - connect raw TCP bytes to the existing `StreamingManager` and Plan 129 destination path;
   - drive ACK/retransmit/timeouts under bounded runtime ownership;
   - correct premature `Established` state and deterministic production RNG;
   - prove binary data, backpressure, loss/reorder/duplicate, close/reset, sibling-stream, and SILENT semantics.
3. **Plan 148 — independent-client interoperability and final Milestone 7 closure**
   - run two independent SAM implementations against the real loopback listener;
   - prove cross-client CONNECT/ACCEPT raw bytes through the Plan 147 product path;
   - re-run FORWARD/naming byte-path acceptance;
   - close resource/privacy and M6 regressions;
   - only then advance the roadmap to Milestone 8.

Do not combine these gates during execution. A reference-format failure must stop Plan 146. An internal Rust byte-path test cannot satisfy Plan 148. An independent client handshake cannot compensate for an incomplete Plan 147 raw driver.

## 2. Corrected authority classification

Treat the repository as follows until the successor status records prove otherwise:

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure

plan_142_base64 = passed
plan_142_private_destination_external_compatibility = not-yet-proven

plan_143_local_delivery_seam = landed-and-retained
plan_143_full_raw_stream_acceptance = not-passed

plan_144_in_process_streaming_handshake = passed-local-evidence
plan_144_independent_client_closure = not-passed

milestone7_local_product = not-closed
sam_independent_clients = 0-passed
next_executable_plan = 146
next_product_layer = remain-on-milestone7
```

The historical status files remain audit records. This Plan 145 roadmap and its status record are the current next-action authority.

## 3. Concrete defects / evidence gaps that must be preserved in handoff

### 3.1 Plan 142: protocol fix is good; closure evidence is incomplete

Retain:

- I2P Base64 alphabet `A-Z a-z 0-9 - ~`;
- `=` padding behavior currently implemented for SAM fields;
- rejection of RFC 4648 `+` / `/`;
- strict bounds, padding validation, no whitespace folding;
- secret redaction / zeroizing ownership improvements.

Do not treat source inspection of Java I2P / i2pd / i2plib as proof that the current `SamPrivateDestination` binary shape is interoperable.

The current official SAM v3 documentation says `DEST GENERATE` returns a private key consisting of Destination + Private Key + Signing Private Key, documented as **663 or more binary bytes / 884 or more Base64 characters**, and notes that the 256-byte encryption-private-key field is unused and may contain random or zero data. Current common-structures documentation separately permits type-specific private-key sizes in contexts where the key type is known. The repository must resolve this apparent distinction against actual SAM / PrivateKeyFile behavior, not choose one interpretation by prose alone.

### 3.2 Plan 143: local delivery extraction is useful; raw SAM product is incomplete

Retain:

- removal of `CapturedOutbound` from product acceptance;
- `i2pr_client::deliver` runtime-neutral local delivery seam;
- Plan 129 destination stack traversal;
- `bridge_to_peer` concept;
- Plan 144 canonical-vs-receiver StreamingManager routing correction;
- in-process bidirectional SYN/SYN-response test.

Correct:

- production `STREAM CONNECT` currently sets the SAM attachment to `Established` immediately after producing a SYN instead of waiting for the actual Streaming state;
- production CONNECT currently seeds `ChaCha8Rng` deterministically; production cryptography must use the router CSPRNG policy;
- the `StreamRawMode` state does not own and run the TCP socket as a raw application stream;
- `STREAM ACCEPT` does not yet wait for/accept the real inbound Streaming connection and transition the socket into raw mode;
- no production raw TCP -> `StreamingManager::send_data` loop exists;
- no `drain_delivered` -> raw TCP loop exists;
- no per-destination retransmit / delayed-ACK driver exists;
- SILENT and post-command-byte preservation are not proven;
- slow-reader/slow-writer resource bounds are not proven.

### 3.3 Plan 144: internal handshake passed; independent lane is still zero

Retain the in-process handshake as a lower-level regression. It is not independent-client evidence.

Current external candidates are useful:

- `i2plib` — Python SAM 3.1 client;
- `libsam3` — C SAM client;
- `txi2p` — optional historical candidate, currently blocked by legacy `ometa` in the development environment.

Plan 148 should prefer `i2plib` + `libsam3` unless a concrete incompatibility makes another maintained SAM client more practical. Do not make `txi2p` a hard prerequisite.

## 4. Environment constraints

All remaining Milestone 7 work must run under the current constrained development environment:

```text
root/sudo                 = not required
Linux namespaces          = not required
Docker                    = not required
Multipass/VM              = not required
systemd                   = not required
public I2P network        = not required
live NTCP2 / SSU2         = not required
mixed-router transport    = not required
localhost TCP             = allowed/required
reference client/library  = allowed
reference key tooling     = allowed
```

Use the existing Plan 129 authenticated-router-link-bypassed local seam below the full destination stack. Do not reopen the historical NTCP2 harness architecture.

## 5. Architecture invariants

The corrective work must preserve:

```text
i2pr-api    = runtime-neutral SAM parsing/state/replies

i2pr-client = authoritative destination + Streaming implementation
               StreamingManager
               StreamingDestinationAdapter
               Plan 129 local delivery seam

i2pr-daemon = Tokio sockets, task supervision, deadlines, cancellation,
              raw SAM socket driver, composition
```

Forbidden shortcuts for closure:

- a second SAM-specific byte-stream protocol;
- direct application-byte copying between two `StreamingManager`s;
- fabricated `Established` state;
- test-only captured outbound queues as product evidence;
- direct test calls that move application bytes around the Plan 129 path;
- unbounded channels/queues or payload accumulation;
- deterministic cryptographic RNG in production paths.

## 6. Execution sequence and stop conditions

### Step 1 — Plan 146

Execute `plans/146-m7-sam31-private-destination-reference-requalification.md`.

Stop if the reference representation cannot be imported/exported without changing the supported destination identity profile. Record the exact blocker and do not continue into Plan 147 with an uncertain SAM private-key format.

### Step 2 — Plan 147

Execute `plans/147-m7-sam31-dedicated-raw-stream-driver-corrective.md` only after Plan 146 passes.

Stop if any canonical localhost STREAM acceptance trajectory:

- returns `OK` before actual Streaming establishment;
- keeps parsing SAM commands after raw transition;
- bypasses Plan 129 destination routing;
- relies on direct manager-to-manager application-byte transfer;
- has an unbounded buffer or unowned task.

### Step 3 — Plan 148

Execute `plans/148-m7-sam31-independent-client-final-closure.md` only after Plan 147 passes.

If one external client is stale or environment-blocked for reasons unrelated to i2pr, record that evidence and select another maintained client. Do not weaken i2pr protocol semantics to match a demonstrable client defect.

## 7. Documentation authority updates during execution

Each successor must update, as applicable:

```text
plans/145-status.md
plans/README.md
README.md
AGENTS.md
specs/support.toml
docs/protocol-support.md
docs/protocols/08-sam.md
specs/references/sam31-private-destination.md
tests/integration/sam/README.md
```

Do not silently rewrite historical Plan 142–144 status records. Add explicit superseding notes or rely on the newer Plan 145+ authority records.

## 8. Global validation floor

Every implementation closure must keep the normal repository gates green:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
```

Run additional boundary scripts when their governed files change.

Plan 147 and Plan 148 must also explicitly re-run the retained Plan 129–134 focused regressions rather than relying only on the aggregate workspace test count.

## 9. Milestone 7 closure contract

Milestone 7 closes only after all three successor plans pass and the authority can truthfully state:

```text
sam31_base64 = reference-compatible
sam31_private_destination = reference-generated-and-reference-consumed
sam31_stream_connect_accept = dedicated-raw-socket-product-passed
sam31_forward = real-byte-path-passed-with-loopback-target-policy
sam31_naming = local-supported-surface-passed
sam31_independent_clients = at-least-two-passed
milestone7_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
next_product_layer = Milestone 8 / SSU2
```

Router-to-router / public-network interoperability remains separate debt and is not part of this SAM localhost closure.

## 10. Handoff

The next implementation model must execute **Plan 146 only**.

Do not begin raw STREAM implementation until the private-destination compatibility ambiguity has a real reference artifact and bidirectional evidence.