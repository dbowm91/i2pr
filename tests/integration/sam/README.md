# SAM 3.1 localhost reference / independent-client evidence lane

This directory is the lightweight external/reference evidence surface for Milestone 7.

Current authority:

- `plans/closure/sam/146-status.md` — closed private-destination reference compatibility;
- `plans/closure/sam/147-status.md` — retained raw-socket owner/byte-pump implementation evidence;
- `plans/closure/sam/149-status.md` — passed self-composing local SAM product;
- `plans/closure/sam/150-status.md` — external-client core evidence retained; broad final closure superseded;
- `plans/closure/sam/151-status.md` — **passed** final Milestone 7 acceptance/evidence correction;
- `plans/closure/sam/152-status.md` — **passed** narrow M6 session/streaming robustness corrective;
- `plans/closure/sam/153-status.md` — **passed** post-M7 authority/CI hygiene.
- `plans/closure/sam/153-m7-closure-authority-and-ci-hygiene.md` — closed hygiene plan (docs/CI only, no `crates/` or `Cargo.lock` changes).

This lane is localhost-only. It must not require root, namespaces, Docker, a VM,
systemd, public I2P participation, or live NTCP2/SSU2.

## Plan 368 staged profile checks

`run-independent.sh` also runs the new staged SAM 3.3 loopback tests and records
them through the same guarded evidence path. They cover PRIMARY child add/remove,
private app-instance isolation, ordinary/private STREAM routing on ports 110
and 65535 including an omitted-`TO_PORT` negative case, and the combined
shared-PRIMARY data-plane fixture. Production negotiates the qualified SAM
3.1–3.3 range. These local rows are not Java I2P interoperability evidence.

`run-java-368.sh` is the separate Java 2.13.0 lane. It verifies the exact
source pin, compiles the unmodified pinned `SAMStreamSink`, and runs it against
the staged SAM 3.3 loopback profile. The current rows cover Java HELLO 3.3,
PRIMARY creation, STREAM/DATAGRAM1/DATAGRAM2/DATAGRAM3/RAW child operations,
`NAME=ME` correlation, Datagram1 receive framing, DATAGRAM1/2/3 receive traffic
on one Java-created PRIMARY, a STREAM byte round trip through that PRIMARY's
Java-created child, and primary-owner teardown. The ordinary profile fixture
also combines STREAM with DATAGRAM1/2/3, RAW 18, and RAW 42 under one PRIMARY.
The Java probe uses the unmodified pinned SAMStreamSink for its client-facing
row and a small Java wire peer for the child/traffic matrix, with the exact
Java 2.13.0 source pin checked before execution. Sanitized results are checked
with `scripts/check-sam368-java-evidence.sh`. These rows prove local SAM
interoperability and data-plane composition; they do not prove a live remote
I2P tunnel round trip.

`run-i2pd-368-diagnostic.sh` starts exact-pinned i2pd 2.61.0 with its SAM
listener on loopback, NTCP2 bound to loopback, SSU2 disabled, and reseed and
addressbook subscriptions disabled. The runtime row records its 3.3 HELLO and
PRIMARY rejection; source-locked diagnostic rows record its MASTER spelling,
STREAM-only child add, master-owned remove, and standalone DATAGRAM1/2/3 and RAW
style handling. These findings are non-gating and do not change the normative
Java/specification profile. `scripts/check-sam368-i2pd-evidence.sh` verifies the
sanitized pin and result record.

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

## Why Plan 151 was required (retained)

The pre-Plan-151 `run-independent.sh` contained acceptance bookkeeping that could mark a
required row passed without executing the corresponding case. The known example
was the `multiple-stream-lifecycle` row, which referred to a Plan 149 sibling suite
that was not present.

Plan 151 removed synthetic pass rows and added executable evidence for:

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

Plan 151 added the narrow static checker
`scripts/check-sam-acceptance-evidence.sh` that rejects unconditional `passed`
bookkeeping for required final acceptance labels.

The checker is CI-enforced: it runs in the routine Linux quality job
(`.github/workflows/ci.yml`) and in the manual SAM external workflow
(`.github/workflows/sam-external.yml`) before the external matrix. Do not
weaken the checker or duplicate its logic inside YAML.

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
remote reproduction. It ran on the exact Plan 151 closing head and must run
again on the exact Plan 153 closing head to prove the newly enforced checker
composes with the existing lane.

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

Plan 151 added the focused final-acceptance suite
`crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs` that keeps sibling,
slow-peer, fault, and lifecycle evidence auditable. Plan 152 (narrow M6
corrective, no wire change) is retained underneath it; see
`plans/closure/sam/152-status.md`.

## Privacy

Never commit/upload raw `PRIV` values, signing/static secrets, application
secret payloads, or unsanitized external environments. Evidence may contain
exact revisions, hashes, byte counts, result categories, command names, and
non-secret resource counters.

## Closure rule (retained)

Milestone 7 final localhost acceptance closed when:

1. Plan 146 remained green;
2. Plan 149 self-composed product remained green;
3. retained Plan 150 independent-client core evidence remained green;
4. every Plan 151 sibling/backpressure/fault/lifecycle/FORWARD/M6 row was executable and passed;
5. routine CI and the manual external-client workflow passed on the exact closing head;
6. `plans/closure/sam/151-status.md` explicitly closed the milestone.

Plan 152 is the retained narrow M6 corrective underneath that closure.
Plan 153 (docs/CI hygiene) has passed; Milestone 8 implementation
(Plan 155+) is unblocked under the Plan 154 roadmap.

Current handoff: **execute Plans 155 → 161 in order under Plan 154. SAM stays experimental, loopback-only, disabled by default, and non-advertised.**
