# Plan 150 status — reproducible external-client final Milestone 7 closure

Status: **`passed-m7-sam31-external-client-final-closure`**.

Registered: **2026-09-02** (UTC). Closed: **2026-09-03** (UTC).

Plan of record:
[`plans/150-m7-sam31-external-client-reproducible-final-closure.md`](150-m7-sam31-external-client-reproducible-final-closure.md).

## Current authority

Plan 149's self-composed localhost product is closed and remains the local
composition authority. Plan 150 closes the remaining SAM 3.1 localhost
application layer with independently implemented external clients through the
real TCP listener. This is not router-to-router NTCP2/SSU2 or mixed-router
tunnel interoperability evidence, and no SAM feature is advertised.

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_146 = passed-m7-sam31-private-destination-reference-requalification
plan_147_raw_driver = landed-and-retained
plan_148 = blocked-audit-historical-superseded
plan_149 = passed-m7-sam31-self-composing-local-product-corrective
plan_150 = passed-m7-sam31-external-client-final-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
milestone7_local_product = closed-via-plan149
milestone7_sam_localhost = closed-via-plan150
sam_independent_clients = at-least-two-passed
router_to_router_interoperability = not-claimed
next_product_layer = Milestone 8 planning
```

## Exact external provenance

```text
libsam3:
  repository = https://github.com/i2p/libsam3
  revision = 7d6e658798baec31394c5685f9583343cc00900b
  result = built-and-probed; not counted
  reason = public sam3CreateSession requires PRIV length >= 884, while i2pr's canonical Ed25519 PRIV is 608 characters

i2psam:
  repository = https://github.com/i2p/i2psam
  revision = b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac
  result = passed; unmodified normal public API

i2plib substitute:
  repository = https://github.com/l-n-s/i2plib
  revision = 6edf51cd5d21cc745aa7e23cb98c582144884fa8
  result = passed; unmodified i2plib.sam surface plus thin socket harness
```

The substitute is independently implemented and is qualified under Plan 150
§6 because libsam3's public key-shape contract cannot consume the canonical
i2pr SAM private destination. No external source is vendored or patched.
The harness accounts for the pinned i2psam snapshot's second-seeded session ID
generator by placing separately launched i2psam clients in distinct one-second
slots.

## Acceptance evidence

The successful run was:

```text
bash scripts/interop/fetch-sam-clients.sh --rebuild
bash tests/integration/sam/clients/build.sh
bash tests/integration/sam/run-independent.sh
```

The run uses only the listener's `127.0.0.1:0` TCP surface after startup and
writes sanitized JSON/Markdown evidence to `target/interop/sam-evidence`.
The required results were:

| Surface | Result |
| --- | --- |
| i2plib substitute ACCEPT ↔ i2psam CONNECT | passed; exact bidirectional 2 MiB payloads |
| i2psam ACCEPT ↔ i2plib substitute CONNECT | passed; exact bidirectional 2 MiB payloads |
| Binary matrix | passed; LF/CRLF/NUL/invalid UTF-8/all-byte/SAM-looking/2 MiB payloads |
| `SILENT=true` raw transition | passed; raw bytes precede any status line |
| Private destination | passed through both counted client APIs |
| NAMING | passed for `ME`, full Destination, malformed, and unknown names |
| Negative matrix | passed for unsupported version/style/options, unknown, malformed, and duplicate inputs |
| STREAM FORWARD | passed with real loopback target and authenticated peer metadata |
| Multiple streams/lifecycle | passed by the Plan 149 self-composed black-box suite |
| Plan 149 local resource/privacy/fault gates | passed |

The committed sanitized summary is
[`tests/integration/sam/evidence.md`](../tests/integration/sam/evidence.md).

## Local CI-equivalent validation

The pre-commit workspace gate passed on the pinned Rust 1.95.0 toolchain:

```text
cargo fmt --all --check                                  = passed
cargo check --locked --workspace --all-targets            = passed
cargo test --locked --workspace --all-targets -- --test-threads=1 = 1,260 passed (51 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings = passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps = passed
cargo test --locked -p i2pr-api --all-targets                  = 120 passed
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1 = 4 passed
bash scripts/check-dependency-direction.sh                    = passed
bash scripts/check-runtime-boundaries.sh                      = passed
bash scripts/check-fixture-manifest.sh                        = passed
bash scripts/check-ntcp2-vectors.sh                           = passed
bash scripts/check-ntcp2-interoperability.sh                  = passed
bash scripts/check-constrained-host-lane-boundary.sh          = passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' = 153 passed
cargo deny check advisories bans sources                     = advisories/bans/sources passed
```

## Product correction included

External FORWARD exposed that a successful raw handoff must outlive the
control-command task and that the listener needs a supervised per-session
forward worker. The daemon now registers a bounded forwarding attachment,
accepts authenticated inbound streams, connects each to a loopback target,
and reuses the existing raw byte driver/bridge with cancellation and terminal
cleanup. `SamStreamRegistry::register_forward_attachment` keeps FORWARD mode
separate from the pending ACCEPT queue and preserves the session ceiling.

## Documentation and CI

README, repository guidance, the local-development skill, SAM provenance
guidance, protocol support, daemon/client/API architecture deep-dives, and
the architecture audit record all point at this closure. A manual,
unprivileged GitHub-hosted Ubuntu workflow reproduces the exact external
client lane and uploads only the sanitized evidence directory. The routine
workspace CI remains the required quality gate; Milestone 8 planning may begin
only after the pushed current head is green.

## Handoff

Plan 150 is closed. Retain SAM as experimental, loopback-only, and
non-advertised. Future work may plan Milestone 8, but must not convert this
localhost client result into router-to-router interoperability.
