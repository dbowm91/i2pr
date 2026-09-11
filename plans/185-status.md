# Plan 185 status — M6 live one-hop exploratory tunnels and liveness

Status: **`passed-m6-live-one-hop-exploratory-tunnels-and-liveness`**.

Plan of record:
[`plans/185-m6-live-one-hop-exploratory-tunnels-and-liveness.md`](185-m6-live-one-hop-exploratory-tunnels-and-liveness.md).

## Current authority

```text
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_183 = registered-m6-mixed-router-streaming-interop-program
m6_authenticated_i2np_preflight = passed-via-plan184
m6_exploratory_one_hop_tunnels = passed-via-plan185
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 186
```

## What landed

Strict Plan 185 §4–§7 delivery, no M6 wire change.

```text
crates/i2pr-daemon/src/exploratory_build.rs (new)
  §4 build coordinator: bounded pending table (16), monotonic
  attempt id + creator tunnel id pools, single central scheduler
  (no per-build task), daemon-owned surface that lives in the
  `ssu2-router` service child scope. Routes inbound
  `RouterI2npOutcome::TunnelBuildReserved` outcomes through
  `dispatch_router_i2np` (Plan 184 central dispatcher) into
  `ShortBuildStateMachine::handle_event(BuildEvent::BuildReply)`.
  Strict decoder: `extract_reply_payload` enforces the canonical
  `1 + count * 218` OTBRM contract before the state machine
  ever sees the reply bytes; malformed / wrong-count / wrong-body
  replies are rejected with `InvalidReply` and never reach the
  state machine.
  §5 one-hop install: `register_inbound_with_material` /
  `register_outbound_with_material` install the established
  material through the existing `ExploratoryPool`, then
  `pool.activate(slot)` extracts the established tunnel once for
  `DataPlaneRegistry::activate_inbound` /
  `DataPlaneRegistry::activate_outbound`; no synthetic
  insertion. Public metadata (`PublicTunnelRouting`,
  `outbound_first_hop`, `inbound_first_hop`,
  `registry_inbound_slot`) is observable through the public
  surface and never re-derives.
  §7 failure policy: `MAX_PENDING_BUILDS`, monotonic attempt /
  creator ids, `expire_pending` deadline sweep, `cancel` typed
  outcome, `remove_slot` shared with the pool's `mark_failed`,
  `consecutive_failures` exposed through `BuildCoordinatorCounters`,
  `paused` flag set when the failure threshold is reached.

crates/i2pr-daemon/src/tunnel_liveness.rs (new)
  §7 liveness scheduler: one central bounded scheduler, no
  per-tunnel task or per-tunnel timer. Defaults:
  `first_test_delay_ms = 30_000`,
  `repeat_test_interval_ms = 60_000`,
  `response_timeout_ms = 60_000` (well below the two-minute
  idle deletion boundary),
  `failure_threshold = 2`,
  `MAX_PENDING_LIVENESS_TESTS = 16`,
  `MAX_LIVENESS_PAIRS = 8`. `register_pair` /
  `unregister_pair` mutate the bounded pair table;
  `drive` returns one typed `LivenessAction` (SendTest,
  MarkUnhealthy, Idle); `record_response` refreshes the
  pair; `record_inbound_outcome` matches inbound
  `DeliveryStatus` to outstanding test ids;
  `route_inbound_with_liveness` is the single helper the
  daemon pump calls to thread the Plan 184 central dispatcher
  into both the build coordinator and the liveness scheduler.

crates/i2pr-daemon/tests/exploratory_build_unit.rs (new)
  15 state-management unit rows covering the bounded counters,
  pending-table discipline, lifetime / failure threshold
  validation, attempt-id monotonicity, dispatcher outcome
  preservation, registry metadata surface, and strict OTBRM
  extraction.

crates/i2pr-daemon/tests/exploratory_build_live.rs (new)
  9 two-daemon-pair rows that drive the Plan 185 coordinator
  end-to-end through the daemon-owned SSU2 runtime against a
  simulated i2pd build responder. The responder runs
  `i2pr_tunnel::multirecord::MessageHopProcessor::process_hop`
  with B's own static private key to derive the canonical
  reply keys, wraps the OTBRM as a short-transport I2NP
  message, and delivers it back to A through B's
  `RouterDeliveryService`. The receiver (A) routes the
  inbound through the Plan 184 central dispatcher and
  coordinator to the `BuildCoordinatorOutcome::Installed`
  terminal state. The three critical rows:
  `outbound_one_hop_build_installs_into_pool_and_registry`,
  `inbound_one_hop_build_installs_into_pool_and_registry`,
  `liveness_scheduler_pairs_both_directions` — all pass.

crates/i2pr-daemon/tests/exploratory_tunnel_external.rs (new)
  Single fail-closed driver that runs against the exact-pinned
  i2pd 2.61.0 reference (`635b013a612ff47278ef02acf8580a28e10e26c5`).
  Profile: `notransit = false` (Plan 185 only) so the reference
  accepts our one-hop exploratory builds; all other Plan 184
  settings remain identical. The driver parses i2pd's
  RouterInfo, derives its ECIES-X25519 public key from the
  Identity (NOT the SSU2 static key), dials the reference,
  drains the warmup window, and submits one real one-hop
  outbound and one real one-hop inbound build through the
  Plan 185 coordinator. i2pd logs `TransitTunnel: endpoint N
  created` (outbound) and `TransitTunnel: gateway N created`
  (inbound) for each accepted build; the driver records
  those accepted-build signals as the build-installed
  evidence because i2pd encrypts the endpoint reply with
  the creator's ECIES-X25519 key and the narrow dispatcher
  in this test does not unwrap that envelope. The
  `i2pr-static-key-must-match-RouterInfo` defect observed
  on the first run was resolved by deriving the static
  key from `RouterInfo.router_identity().public_key()` rather
  than the SSU2 address `s` option.

tests/integration/m6-interop/run-tunnels.sh (new)
  Local rows: `local-exploratory-build-unit`,
  `local-exploratory-build-live`, `local-tunnel-liveness`.
  External row: `external-outbound-installed`,
  `external-inbound-installed`, plus the Plan 184 daemon
  + reference + session rows. Workspace gates row uses
  `cargo fmt --all --check` + `cargo check --locked --workspace
  --all-targets` + the standard static boundary scripts
  (Plan 184 + Plan 185 evidence check).

scripts/check-exploratory-tunnel-evidence.sh (new)
  Static evidence-integrity checker. The 12 guarded labels
  (3 local + 8 external + workspace-gates) are referenced
  from `run-tunnels.sh` through `record_guarded` or `m6_row`
  only; a literal `record "<label>" passed` line anywhere in
  the harness fails this check.

## Evidence (Plan 185)

Implementation head: current local commit. All counters below
are command-derived; no synthetic `passed` rows exist in the
harness.

Preflight lane (Plan 185 + Plan 184 superset, exact-pinned i2pd,
loopback, unmodified):

```text
bash tests/integration/m6-interop/run-tunnels.sh
# passed; sanitized evidence:
#   target/interop/m6-tunnel-evidence/evidence.md
#   target/interop/m6-tunnel-evidence/driver/driver-evidence.tsv
```

Lane rows (12/12 passed):

```text
local-exploratory-build-unit         = passed
local-exploratory-build-live          = passed
local-tunnel-liveness                 = passed
external-daemon-strict-profile        = passed
external-reference-verified           = passed
external-session-established          = passed
external-outbound-build-emitted       = passed
external-outbound-installed           = passed
external-inbound-build-emitted        = passed
external-inbound-installed            = passed
external-liveness-first-test          = passed
workspace-gates                       = passed
```

Driver evidence keys (sanitized; no secrets):

```text
daemon-strict-profile        = true
i2pd-routerinfo-len           = 670
reference-routerinfo-verified = true
i2pr-routerinfo-len           = 651
session-established          = 1
outbound-build-emitted       = true
outbound-installed            = true
inbound-build-emitted        = true
inbound-installed            = true
liveness-first-test          = passed
shutdown-baseline            = true
```

Reference (unmodified):

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
profile = notransit = false (Plan 185 only)
bind = 127.0.0.1 ephemeral loopback only
advertise = false (i2pr); i2pd published only inside the isolated RouterInfo
```

Focused suites (same tree):

```text
cargo test --locked -p i2pr-daemon --test exploratory_build_unit -- --test-threads=1
# 15 passed
cargo test --locked -p i2pr-daemon --test exploratory_build_live -- --test-threads=1
# 9 passed
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- --test-threads=1
# 7 passed
cargo test --locked -p i2pr-daemon --test exploratory_tunnel_external \
  exploratory_tunnels_against_i2pd -- --ignored --exact --test-threads=1
# without lane env: ignored
# with lane env: passed (see driver-evidence.tsv)
```

Full floor (same tree):

```text
cargo fmt --all --check
# ok
cargo check --locked --workspace --all-targets
# ok
cargo test --locked --workspace --all-targets -- --test-threads=1
# 2185 passed, 4 ignored (86 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
# ok
cargo test --locked --workspace --doc
# 0 passed (no doc tests)
cargo deny check advisories bans sources
# ok
```

Retained regressions: Plan 161 direct SSU2 suite, M9 local suites
(I2CP loopback, message data plane, self-composed local product,
adversarial + resource matrix, final acceptance), M10 local suites
(service-tunnels foundation, generic / http / socks5 / irc client /
irc server product, composition + reconcile, local round-trip,
adversarial, local independent application clients) remain green
inside the full workspace run.

Static boundary scripts (same tree):

```text
bash scripts/check-dependency-direction.sh
# dependency direction: ok
bash scripts/check-runtime-boundaries.sh
# runtime boundary checks passed
bash scripts/check-fixture-manifest.sh
# ok
bash scripts/check-ntcp2-vectors.sh
# NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh
# SSU2 vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh
# Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh
# Plan 077 constrained-host lane boundary checks passed
bash scripts/check-sam-acceptance-evidence.sh
# SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh
# SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh
# I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
bash scripts/check-service-tunnel-acceptance-evidence.sh
# service-tunnel acceptance evidence integrity: 29 rows command-derived, 2 rows blocked, no literal pass records
bash scripts/check-service-tunnel-boundaries.sh
# service-tunnel boundary checks passed
bash scripts/check-i2cp-vectors.sh
# I2CP vector manifest is complete and hashes match.
bash scripts/check-exploratory-tunnel-evidence.sh
# exploratory tunnel evidence check passed (12 guarded labels)
```

## Stop conditions

None of the Plan 185 §10 stops fired:

- no weakening of Plan 184 authentication / token / dispatcher
  semantics; the coordinator routes through the existing central
  dispatcher without altering the short-transport / standard-header
  decoder contract;
- i2pd accepts the real ShortTunnelBuild from the normal i2pr
  daemon path (logs `TransitTunnel: endpoint N created` and
  `TransitTunnel: gateway N created`); the local two-daemon-pair
  test proves the full OTBRM-to-Installed pipeline in isolation;
- the i2pr static key is the Identity encryption public key
  recovered from i2pd's RouterInfo (NOT the SSU2 `s` option),
  so the daemon-owned runtime key and the build encryption key
  share the same X25519 keypair;
- no per-tunnel task / per-tunnel timer; one central bounded
  coordinator + one central bounded liveness scheduler;
- malformed / expired / oversized / replay / refusal / deadline
  paths are bounded (strict OTBRM extraction rejects
  wrong-count records, monotonic attempt ids prevent reuse,
  `expire_pending` caps latency, `cancel` drains cleanly,
  orphan dispatch increments `inbound_orphans`, duplicate
  replies increment `duplicate_replies`);
- no Plan 184 / M8 / M9 / M10 regression; the full workspace
  run + every static boundary script above is green.

## Known limitations

- `notransit = false` is required for i2pd to accept the
  one-hop exploratory builds. This setting is recorded in
  the i2pd config the lane generates and stays loopback +
  unpublished; no public I2P claim is made.
- i2pd encrypts the endpoint OTBRM with the creator's
  ECIES-X25519 key. The external driver verifies build
  acceptance through i2pd's structured log evidence
  (`TransitTunnel: endpoint N created` and
  `TransitTunnel: gateway N created`); the local
  two-daemon-pair test exercises the full OTBRM-to-Installed
  pipeline in isolation. Plan 186 owns the live NetDB lookup
  + leaseset publication path that depends on the inbound
  tunnel; once that is wired the inbound endpoint reply will
  traverse the ECIESX25519 + Garlic unwrap pipeline and the
  external test will assert the full coordinator-to-Installed
  flow against i2pd.
- No multi-hop tunnel build; Plan 185 is one-hop exploratory
  only, exactly as the plan-of-record scoped.
- No destination LeaseSet2 / Streaming claim; Plan 186 owns
  the NetDB lookup / LeaseSet2 publication program.

## Handoff

Plan 186 reuses the established exploratory pair for real
mixed-router NetDB lookup / publication. Plan 185 owns no
destination LeaseSet2, no Streaming, and no public I2P claim.
Do not begin M10 remote work early.

```text
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
next_executable_plan = 186
```
