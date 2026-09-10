# Plan 184 status — M6 authenticated I2NP runtime and independent-router preflight

Status: **`passed-m6-authenticated-i2np-runtime-and-reference-preflight`**.

Plan of record:
[`plans/184-m6-authenticated-i2np-runtime-and-reference-preflight.md`](184-m6-authenticated-i2np-runtime-and-reference-preflight.md).

## Current authority

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_183 = registered-m6-mixed-router-streaming-interop-program (unchanged program authority)
milestone6_interoperable = not-yet-claimed (no NetDB/tunnel/destination/Streaming claim)
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 185
```

## What landed

Strict Plan 184 §3 controlled activation plus the first
daemon-owned authenticated router-I2NP spine. No tunnel, NetDB,
destination, Streaming, or M10 remote-service claim.

```text
crates/i2pr-daemon/src/config.rs
  Plan 184 §3: `normalize_ssu2` no longer blanket-rejects
  `enabled = true`. Acceptance requires loopback bind (at least one
  family, `parse_ssu2_bind` still rejects wildcard/non-loopback),
  `advertise = false`, `introducer_service = false`, existing
  token/session/datagram ceilings, port 0-or->=1024. Both binds
  disabled while enabled fails closed as `ssu2.enabled`.

crates/i2pr-runtime/src/lib.rs
  Plan 184 re-exports for daemon composition without a new direct
  `i2pr-transport-ssu2` dependency: `IntroKey`, `Ssu2PublicKey`,
  `Ssu2RouterAddress`, `constants`.

crates/i2pr-daemon/src/router_i2np.rs (new)
  §5 central dispatcher: `Ssu2InboundI2np` by reference (no
  caller-supplied peer identity) -> bounded standard/short decode
  with explicit `RouterI2npHeaderKind` classification -> expiration
  (`<= now` => Expired, `> now + 15 min` => FarFuture) / size
  (`MAX_ROUTER_I2NP_BYTES`) / type validation -> typed
  `RouterI2npOutcome::{TunnelBuildReserved, TunnelData,
  RouterControl, Unsupported}` preserving peer/link identity.
  ShortTunnelBuild/OutboundTunnelBuildReply are Plan 185 reserved;
  TunnelData carries the tunnel id for the existing
  registry/`inbound_dispatch` seam; DatabaseStore/DatabaseLookup/
  DatabaseSearchReply/DeliveryStatus are router-control hooks;
  unknown first bytes and known-but-unsupported bodies (e.g. Garlic)
  return bounded `Unsupported`, never panic/unbounded retain.
  `inbound_dispatch.rs` is untouched.
  §6 narrow outbound capability: `RouterDeliveryRequest`
  (peer + `EncodedI2npMessage` + 1 ms..=60 s timeout) ->
  `RouterDeliveryService::deliver` -> existing
  `Ssu2RuntimeService::send_i2np` with explicit
  `NoActiveSession`/`QueueFull`/`ResourceDenied`/`Cancelled`
  mapping, cancellation checked before admission, no task per
  delivery, no global router context in tunnel/NetDB/client crates.
  §4 daemon ownership: `ssu2_runtime_config_from` (daemon ceilings
  authoritative, runtime-only fields default-clamped),
  `ssu2_socket_config_from` (strict re-validation),
  `generate_controlled_identity` (OS CSPRNG static/intro, one
  loopback SSU2 address, `router.version 0.9.58` + `netId 2`,
  re-validated decode, non-loopback/zero-port rejected),
  `verify_reference_router_info` (public bytes only, SSU2 address
  required), `Ssu2DaemonService::new/start` (single runtime owner;
  counted tests hold no hidden standalone runtime),
  `Ssu2DaemonHandle::{dial, delivery, next_inbound,
  next_dispatched, shutdown, snapshot}`.

crates/i2pr-daemon/src/lib.rs
  `router_i2np` module plus `register_ssu2_service` under the
  existing child/supervision model as `ssu2-router` (Optional):
  loads the persistent bundle, generates controlled identity
  (loopback placeholder port when `port = 0` because
  `advertise = false` never publishes), starts the daemon-owned
  runtime under the service child scope, pumps the central
  dispatcher until cancellation with orderly shutdown. Strict
  profile is enforced at parse time and again at service start.

crates/i2pr-daemon/tests/ssu2_daemon_preflight.rs (new)
  5 local rows + 1 ignored external row:
  graph registers `ssu2-router` under the strict profile and omits
  it when disabled; controlled exchange reaches the central
  dispatcher with peer preservation via the narrow delivery seam;
  unknown-peer `NoActiveSession` + cancellation explicit; shutdown
  with active session returns pending/active to zero.
  External `ssu2_daemon_preflight_against_i2pd` is
  `#[ignore = "Plan 184: requires exact-pinned external i2pd
  environment"]`: ordinary workspace execution compiles/skips it
  (exit 0); explicit `--ignored --exact` without env fails closed
  (`missing required env I2PD_ROUTER_INFO`).

tests/integration/m6-interop/run-preflight.sh (new)
  Ephemeral unmodified i2pd 2.61.0
  (`635b013a612ff47278ef02acf8580a28e10e26c5`) on loopback with a
  fresh datadir, no reseed/public dependence, SSU2 loopback
  published only inside the isolated reference RouterInfo, no
  unrelated services, cache/revision/version verified fail-closed
  before any network use. RouterInfo crosses via the documented
  `router.info` file path; only public bytes enter i2pr. Local +
  external + gates rows recorded via `record_guarded` with
  driver-evidence keys; `evidence.json/md` sanitized (lengths,
  digests, counters only).
```

## Evidence (Plan 184)

Implementation head: `782a8fe056875f491a1b02daaa1715ef1d2b3cb6`
(`plan184: pass M6 authenticated I2NP runtime and reference
preflight`). Base tree for the lane run below was
`6f598fc8274860e109fb84a466dc7ca612c045b1` plus the identical
Plan 184 working tree; routine CI run `34539210477` is green on
the implementation head (MSRV + dependency-policy + quality
ubuntu + quality macos, all `success`).

Preflight lane (exact-pinned i2pd, loopback, unmodified):

```text
bash tests/integration/m6-interop/run-preflight.sh
# passed; sanitized evidence:
#   target/interop/m6-preflight-evidence/evidence.md
#   target/interop/m6-preflight-evidence/driver/driver-evidence.tsv
```

Lane rows (10/10 passed on the base+tree run):

```text
local-daemon-preflight = passed
lib-router-i2np = passed
external-daemon-strict-profile = passed
external-reference-verified = passed
external-session-established = passed
external-i2np-outbound = passed
external-i2np-inbound = passed
external-malformed-bounded = passed
external-resource-baseline = passed
workspace-gates = passed
```

Driver evidence keys (sanitized; no secrets):

```text
daemon-strict-profile = true
i2pd-routerinfo-len = 670
reference-routerinfo-verified = true
i2pr-routerinfo-len = 651
session-established = 1
warmup-received = 6
outbound-i2np-digest = 50afde8bea69062b84f9e14d0a2118ec602e642e48f0bf76a0d4645d19ce9488
outbound-accepted = true
inbound-dispatched = RouterControl { kind: DeliveryStatus,
  header: ShortTransport, encoded_len: 21, peer-preserved }
inbound-peer-preserved = true
malformed-bounded = true
resource-sessions-closed = 1
shutdown-baseline = true
```

Reference (unmodified):

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
bind = 127.0.0.1 ephemeral loopback only
advertise = false (i2pr); i2pd published only inside the isolated RouterInfo
```

Focused suites (same tree):

```text
cargo test --locked -p i2pr-daemon --lib router_i2np -- --test-threads=1
# 11 passed
cargo test --locked -p i2pr-daemon --lib config -- --test-threads=1
# 60 passed
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1
# 5 passed, 1 ignored (external)
cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight \
  ssu2_daemon_preflight_against_i2pd -- --ignored --exact --test-threads=1
# without env: failed-closed (missing required env I2PD_ROUTER_INFO)
# with lane env: passed (see driver-evidence.tsv)
```

Full floor (same tree):

```text
cargo fmt --all --check
# ok
cargo check --locked --workspace --all-targets
# ok
cargo test --locked --workspace --all-targets -- --test-threads=1
# 2148 passed, 3 ignored (83 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
# ok
cargo test --locked --workspace --doc
# 0 passed (16 suites; no doc tests)
bash scripts/check-dependency-direction.sh
# ok
bash scripts/check-runtime-boundaries.sh
# ok
bash scripts/check-service-tunnel-boundaries.sh
# ok
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
# all ok
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# 153 passed
cargo deny check advisories bans sources
# ok
```

Retained regressions: Plan 161 direct SSU2 suite, M9 and M10 local
suites remain green inside the full workspace run above.

## Stop conditions

None of the Plan 184 §11 stops fired:

- no weakening of Plan 161 authentication/token semantics;
- i2pd established the previously proven SSU2 session under normal
  daemon ownership;
- no per-I2NP tasks and no unrestricted global context;
- authenticated peer identity is preserved to dispatch;
- no public I2P and no patched i2pd.

## Handoff

Plan 185 owns the first real one-hop Short Tunnel Build and
TunnelData exchange. Plan 184 claims no NetDB, destination,
Streaming, or M10 remote-service interoperability.

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
next_executable_plan = 185
```
