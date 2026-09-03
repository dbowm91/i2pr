# SAM 3.1 localhost reference / independent-client evidence lane

This directory is the lightweight external/reference evidence surface for Milestone 7.

Current authority:

- `plans/146-status.md` — closed private-destination reference compatibility;
- `plans/147-status.md` — retained raw-socket owner/byte-pump implementation evidence;
- `plans/149-status.md` — passed self-composing local SAM product;
- `plans/150-status.md` — external-client core evidence retained; broad final closure superseded;
- `plans/151-status.md` — **active final Milestone 7 acceptance/evidence authority**;
- `plans/151-m7-sam31-final-acceptance-evidence-correction.md` — only next executable plan.

This lane is localhost-only. It must not require root, namespaces, Docker, a VM,
systemd, public I2P participation, or live NTCP2/SSU2.

## Retained reference/product evidence

### SAM Base64

Plan 142's Base64 correction is retained:

```text
alphabet = A-Z a-z 0-9 - ~
padding  = =
```

### Private destination — Plan 146

Pinned references:

- Java I2P `2800040deee9bb376567b671ef2e9c34cf3e30b6` (2.12.0);
- i2pd `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` (2.60.0).

Reference evidence confirms the compact representation:

```text
PRIV binary = 455 bytes
PRIV Base64 = 608 chars
PUB binary  = 391 bytes
PUB Base64  = 524 chars
```

### Raw socket/product composition

Plan 147's owned raw TCP↔Streaming implementation is retained. Plan 149 then
made `SESSION CREATE` self-compose the actual localhost product before success:
destination runtime, signed LeaseSet2, local bridge/delivery capability,
Streaming/session registries, and one supervised destination driver.

The canonical Plan 149 black-box test starts the listener and then drives only
SAM TCP commands/raw bytes. It covers:

- exact bidirectional 2 MiB transfer;
- SILENT raw transition;
- same-read command+raw-byte preservation;
- session teardown/resource baselines.

Do not describe that four-test suite as containing sibling-stream or complete
fault/backpressure acceptance; those are Plan 151 work.

## Retained Plan 150 external-client core evidence

Pinned external provenance:

```text
i2psam
  repository = https://github.com/i2p/i2psam
  revision = b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
  role = counted external client

i2plib SAM surface
  repository = https://github.com/l-n-s/i2plib
  revision = 6edf51cd5d21cc745aa7e23cb98c582144884fa8
  role = counted qualified independent substitute

libsam3
  repository = https://github.com/i2p/libsam3
  revision = 7d6e658798baec31394c5685f9583343cc00900b
  role = built/probed, not counted
```

libsam3's public API requires an 884-character minimum private key, while
i2pr's canonical Ed25519 SAM `PRIV` is 608 characters. Do not change i2pr's
reference-qualified private-destination shape to satisfy that client API.

The Plan 150 lane genuinely proved:

- i2plib-surface ACCEPT ↔ i2psam CONNECT exact 2 MiB binary traffic;
- i2psam ACCEPT ↔ i2plib-surface CONNECT exact 2 MiB binary traffic;
- binary payload matrix;
- SILENT transcript behavior;
- private destination import/generation;
- NAMING supported surface;
- negative version/style/option/malformed cases;
- positive STREAM FORWARD through a real loopback echo target.

The committed [`evidence.md`](evidence.md) is therefore retained as Plan 150
external-core evidence, not as current final Milestone 7 authority.

## Why Plan 151 is required

The current `run-independent.sh` contains acceptance bookkeeping that can mark a
required row passed without executing the corresponding case. The known example
is the `multiple-stream-lifecycle` row, which refers to a Plan 149 sibling suite
that is not present.

Plan 151 must remove synthetic pass rows and add executable evidence for:

- two simultaneous sibling streams and close-one/keep-one isolation;
- slow reader;
- slow writer/reverse pressure;
- DATA drop;
- ACK drop;
- duplicate DATA;
- reordered DATA;
- authenticated/ciphertext corruption;
- retransmission ceiling;
- CLOSE/RESET/control-session lifecycle;
- full FORWARD lifecycle/negative matrix;
- explicit focused Plan 127–134 regressions.

Each final result must derive from an actual command/test exit result.

## Evidence-integrity rule

Plan 151 should add a narrow static checker (for example
`scripts/check-sam-acceptance-evidence.sh`) that rejects unconditional `passed`
bookkeeping for required final acceptance labels.

The generated evidence must associate each result with its command/test,
closing commit, execution lane, and external revision where applicable.

## Reproducible external lane

Retain the existing acquisition/build/run shape:

```text
bash scripts/interop/fetch-sam-clients.sh --rebuild
bash tests/integration/sam/clients/build.sh
bash tests/integration/sam/run-independent.sh
```

The manual GitHub-hosted Ubuntu `workflow_dispatch` lane remains the preferred
remote reproduction. It must run again on the exact Plan 151 closing head.

Do not vendor or patch third-party sources. Do not require privileged runners.

## Routine local commands

```text
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-daemon --test sam_loopback
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
```

Plan 151 should add a focused final-acceptance suite if that keeps sibling,
slow-peer, fault, and lifecycle evidence auditable.

## Privacy

Never commit/upload raw `PRIV` values, signing/static secrets, application
secret payloads, or unsanitized external environments. Evidence may contain
exact revisions, hashes, byte counts, result categories, command names, and
non-secret resource counters.

## Closure rule

Milestone 7 final localhost acceptance closes only when:

1. Plan 146 remains green;
2. Plan 149 self-composed product remains green;
3. retained Plan 150 independent-client core evidence remains green;
4. every Plan 151 sibling/backpressure/fault/lifecycle/FORWARD/M6 row is executable and passes;
5. routine CI and the manual external-client workflow pass on the exact closing head;
6. `plans/151-status.md` explicitly closes the milestone.

Current handoff: **execute Plan 151 only; do not begin Milestone 8 implementation yet**.