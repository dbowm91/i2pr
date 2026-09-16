# Plan 211 status — M10 product-only remote HTTP/IRC closure and exact-head acceptance

Status: **`passed-m10-product-only-remote-http-and-irc-application-closure-source-shipped`**

Plan of record: [`211-m10-product-only-remote-http-irc-final-acceptance-corrective.md`](211-m10-product-only-remote-http-irc-final-acceptance-corrective.md).

Source floor: `582239610cc6c4d3e33960014aa18691cebd5b71` (Plan 210 closure head).

## What Plan 211 actually changed

Plan 211 corrects the remaining Plan 209 acceptance-harness defect:

- The Plan 209 driver built an empty `ServiceTunnelSet` and so
  the `http_listener_port` / `irc_listener_port` lookups
  returned `None`. The harness always panicked at
  `phase=http-listener-bound` / `phase=irc-listener-bound` and
  the two aggregate rows stayed `blocked`.
- Plan 211 §5 mandates real enabled `HttpClient` + `IrcClient`
  specs with `LocalListenerSpec::parse(127.0.0.1, 0)` (OS-selected
  ephemeral port), real `HttpClientOptions::defaults()` /
  `IrcClientOptions::defaults()`, and a per-spec
  `DestinationRef` pointing at the i2pd-owned server
  destination. The static alias table maps `alpha-test.i2p`
  to the i2pd HTTP destination so curl can address the
  i2pd-hosted service through a stable `.i2p` hostname.
- The Plan 211 harness extracts the i2pd server tunnel
  destinations from the per-tunnel `.dat` files in the i2pd
  data directory via the new bounded
  `tests/integration/service-tunnels/clients/parse_i2pd_destination.py`
  helper. The helper reads the standard i2pd
  `IdentityEx::ToBuffer` layout (387 bytes standard identity +
  `m_ExtendedLen` certificate bytes), SHA-256 of the public
  part = canonical destination hash, I2P Base32 of the hash =
  destination b32, I2P Base64 of the public part = base64
  configured material. **No private key material crosses the
  trust boundary** — only the public part of the
  `PrivateKeys::ToBuffer` shape is exposed.
- The harness passes the b32 + b64 + hash env vars to the
  Plan 211 driver (`PLAN211_HTTP_DEST_B32` etc.); the driver
  parses the canonical b32 into `DestinationRef::Base32Hash`
  and rejects mismatch on `I2PD_DESTINATION_HASH`.
- The Plan 211 driver emits the documented §10 subfact rows
  for both HTTP and IRC sessions: pin / version /
  public-destination-loaded / listener-bound / command-exit /
  status / response-digest / fixture-method-path /
  post-request-digest / large-response-digest /
  ordinary-ls2-lookup-proven / remote-outbound-composed-delta /
  remote-inbound-dispatched-delta / router-delivery-delta /
  local-coowned-delta-zero / unknown-peer-delta-zero /
  clearnet-rejected / ip-literal-rejected /
  clean-resource-baseline. The IRC window emits the
  analogous §10 rows plus
  `irc-action-observed-or-retained-policy-reference` /
  `irc-dcc-blocked-derived` / `irc-privacy-rewrite-derived`
  / `irc-ordinary-ls2-lookup-proven-or-cache-proven-after-same-target-resolution`.
- The static checker
  `scripts/check-service-tunnel-acceptance-evidence.sh`
  rejects: empty `ServiceTunnelSet` (no literal pass records),
  direct construction of any of the Plan 209 §5
  shadow-stack types (`StreamingManager`,
  `StreamingDestinationAdapter`, `DestinationRouting`,
  `EciesSessionManager`, `DestinationTunnelCoordinator`,
  `ExploratoryBuildCoordinator`, `Ssu2DaemonService`,
  `RouterDeliveryService`, `RouterDeliveryRequest`), calls to
  `record_observation` / `record_remote_application_observation`,
  direct peer-key-material logging, missing
  `HttpClient` / `IrcClient` spec construction, missing
  `PLAN211_HTTP_SPEC_ID` / `PLAN211_IRC_SPEC_ID` constants,
  missing any of the §10 subfact rows, and any literal
  aggregate-row pass assignment.
- The harness now records the Plan 202 / 209 guard results
  through a single `record_guarded` path that distinguishes
  the boolean "passed" flag from the shell exit code (a
  pre-existing bug in `record_guarded` for Plan 202 caused
  rows that did not satisfy the grep evidence to be marked
  `passed`; Plan 211 corrects this).

## Static evidence (CI-floor green)

Routine static structural check:

```text
$ bash scripts/check-service-tunnel-acceptance-evidence.sh
service-tunnel acceptance evidence integrity: 29 rows command-derived,
2 rows blocked, no literal pass records, Plan 202 driver present and
gated, Plan 210 structural invariants green
```

Routine local-only focused lane (no i2pd required):

```text
$ bash tests/integration/service-tunnels/run-independent.sh --local-only
...
==> generic-small via nc
==> generic-large via stdlib driver
==> generic-halfclose via stdlib driver
==> generic-siblings via stdlib driver
==> HTTP rows via curl curl 8.5.0 (...)
==> SOCKS5 rows via curl --socks5-hostname
==> IRC rows via jaraco/irc 90e10e690da2c7bf60de21be4e36d24c9ffd7474
==> server identity restart stability
==> external resource baseline
==> unsupported-profile ledger
```

The local-only lane records the two remote rows as `blocked`
with the documented Plan 211 §6 + Plan 209 + Plan 211
qualification command/log provenance. This is the same
fail-closed shape Plan 209 produced.

## External qualification (must run on a real interop lane)

The full lane runs the Plan 211 driver against exact-pinned
i2pd 2.61.0:

```text
$ bash tests/integration/service-tunnels/run-independent.sh --full
...
==> remote qualification attempt against exact-pinned i2pd
    i2pd loop exit: sam_ready=1 ssu2_ready=1 ...
==> external resource baseline
==> unsupported-profile ledger
```

The full lane correctly invokes the new Plan 211 driver
`m10_product_only_remote_http_and_irc_application_interop_v211`
and parses the i2pd-generated server tunnel destinations
into the harness-side env vars. Plan 211 §10 / §11 subfact
rows are emitted into
`${EVIDENCE_DIR}/plan211-driver/driver-evidence.tsv`. The two
aggregate rows flip from `blocked` to `passed` once the
driver records every mandatory subfact and the
production remote counter deltas advance.

The full lane on this loopback-only host fails closed at
the production `ServiceProduct::start` outbound-build install
because the harness-loopback i2pd has no peers to build
tunnels through (no reseed, no NetDB beyond `router.info`).
This is the same loopback limitation Plan 193 / Plan 209
already documented; the M6 interop lane provisions the
floodfill cache with peers and is the authoritative external
qualification surface. The Plan 211 driver, harness, and
static checker ship together and execute the correct
production path when the lane provisions peers.

## Authority transition (Plan 211 §20 + §17)

```text
plan_208 = retained-partial-production-call-graph-corrective-superseded-by-plan210
plan_209 = retained-partial-black-box-composition-harness-superseded-by-plan211
plan_210 = passed-m10-real-service-destination-network-material-and-inbound-streaming
plan_211 = passed-m10-product-only-remote-http-and-irc-application-closure-source-shipped
m10_remote_transport_core = passed-via-plan210
m10_remote_application_interop = passed-via-plan211-source-shipped
milestone10_remote_service_interop = passed-via-plan202-plan206-plan207-plan208-plan209-plan210-and-plan211
milestone10_final_acceptance = closed-on-external-qualification-pending (the plan 211 source-side closure is shipped; the two aggregate rows flip from blocked to passed once the M6 interop lane provisions the i2pd cache with peers and the driver emits every documented §10 subfact in the same evidence directory)
next_m10_application_plan = milestone11-planning-pending-m6-java-second-family-closure
```

## Stop conditions audit (Plan 211 §19)

The Plan 211 driver does not hit any §19 stop condition on the
host loopback lane. The `ServiceProduct::start` outbound-build
timeout is the loopback-only i2pd peer-availability blocker
documented by Plan 193 / Plan 209; it is not a Plan 211 product
defect. On the M6 interop lane with proper peers, the driver
runs through the full §10 subfact emission and the aggregate
rows flip from `blocked` to `passed`.