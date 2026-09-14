# Plan 181 status — M10 independent application/service interoperability

Status: **`passed-m10-independent-application-and-service-interop-final-closure-evidence`**.
The Plan 204 docs/CI/evidence-authority normalization pass
reclassifies Plan 181 from
`blocked-by-m6-mixed-router-streaming-blocker` to the current
status because Plan 203 supplies the positive remote application
interop that the two retained remote rows in the Plan 181 §6.3
lane were blocked on. The local matrix stays green as it was at
Plan 181 close; the remote application rows flip
`blocked → passed-on-env` through Plan 203's positive external
driver, and the static checker enforces the
`remote-independent-*` flip on the dedicated M6 interop lane.

Plan of record:
[`plans/181-m10-independent-application-and-service-interop-final-closure.md`](181-m10-independent-application-and-service-interop-final-closure.md).

## Current authority

```text
plan_182 = passed-m10-local-delivery-corrective
plan_181 = passed-m10-independent-application-service-interop-final-closure-evidence
        (29 local rows retained; the two remote application rows
         were blocked-at-plan181-close and flipped to
         passed-on-env through Plan 203's positive Direction A
         external driver)
plan_195 = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_204 = in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = passed-via-plan177
milestone10_irc_client = passed-via-plan178
milestone10_irc_server = passed-via-plan179
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_local_roundtrip = passed-via-plan182
milestone10_independent_application_clients = passed-via-plan181-and-plan203
milestone10_remote_service_interop = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
milestone10_final_acceptance = not-yet-closed (Plan 203 closes the positive remote application rows; Plan 204 owns the docs/CI normalization; Plan 201 must close before M6 interoperable + M10 final acceptance can both be claimed)

next_executable_plan = 201-branch-g-finalize (Plan 200 exact-head external run consumes the P200 classification)
next_product_layer = m10-unified-final-closure-blocked-at-plan201
```

## What landed in the Plan 181 + Plan 204 lane

```text
tests/integration/service-tunnels/run-independent.sh
  Plan 181 — external lane runner. Prerequisite-status gate, tool/pin
  verification, static boundary checker, focused Rust suites
  (final-acceptance/adversarial with per-test ok-line rows, Plan 182
  round-trip, wire surface), two example-listener generations
  (echo-backed generic rows; HTTP/IRC-backed application rows),
  unmodified curl HTTP/SOCKS rows, nc + stdlib generic rows,
  exact-pinned jaraco/irc venv rows, restart-stability row, i2pd
  remote qualification section, resource-baseline row, unsupported-
  profile ledger row, classified evidence.json/evidence.md emission.
  Plan 202 added the m10-remote-destination-streaming-composition
  blocked-on-env row. Plan 203 rewrote the two `remote-independent-*`
  rows to flow through record_guarded and emit the
  http-remote-application-established /
  irc-remote-application-established evidence keys.

tests/integration/service-tunnels/fixtures/{http,echo,irc}_fixture.py (Plan 181)
  Loopback-only stdlib fixtures with per-request flushed JSON facts
  (digests/lengths/policy facts only, never bodies).

tests/integration/service-tunnels/fixtures/{http,irc}_remote_eepsite.py (Plan 203)
  Bounded deterministic loopback HTTP + IRC server fixtures
  mirroring the local co-owned target byte-for-byte. No clearnet,
  no DNS, no external state.

tests/integration/service-tunnels/clients/{irc,generic}_driver.py (Plan 181)
  jaraco/irc public-`irc.client`-API-only driver; opaque stdlib
  generic driver.

tests/integration/service-tunnels/clients/{http,irc}_remote_driver.py (Plan 203)
  Stdlib-only loopback HTTP + IRC client drivers; reads a daemon-
  bundled JSON document and drives one curl-equivalent HTTP/1.1 round
  trip plus one jaraco/irc-equivalent IRC registration through the
  i2pr client listeners towards the i2pd-owned server-tunnel
  destination b64. Records digests only; never logs key material.

scripts/interop/fetch-service-tunnel-clients.sh (Plan 181)
  Exact-pin jaraco/irc fetch + clean-checkout (no-patch) enforcement.

scripts/check-service-tunnel-acceptance-evidence.sh
  Routine-CI static checker. The checker enforces:
    - Plan 181 §15 evidence: 29 command-derived rows, 2
      blocked-only remote rows.
    - Plan 182 §11 invariants: per-destination delivery drivers
      over the bridge_to_peer seam, wildcard Streaming port 0,
      SAM-parity accept paths, completed IRC client executor,
      orderly pump half-close, active-slot hygiene.
    - Plan 197 §8 pq option-tolerance invariants.
    - Plan 202 §12 invariants: the
      m10-remote-destination-streaming-composition external
      driver exists, is #[ignore]-gated, declares the Direction A
      test name, exercises RemoteDeliveryCounters +
      install_router_delivery_handle + routing_decision_for +
      RoutingDecision::RemoteRouter, and never logs peer key
      material.
    - Plan 203 §13 invariants: the
      m10_positive_remote_http_and_irc_application_interop
      external driver exists, is #[ignore]-gated, declares the
      Direction A test name, exercises
      record_remote_application_observation +
      install_router_delivery_handle +
      RoutingDecision::RemoteRouter + RemoteDeliveryCounters,
      rejects literal "record ... passed" lines, requires the
      positive rows to flow through record_guarded with the
      documented evidence keys, and never logs peer key
      material.
    - Pin/head/cleanliness/curl/--socks5-hostname/jaraco-API
      gates, generic-driver opacity gate, ignored-exact remote
      selection, i2pd pin + SAM PUB provenance + unknown-peer
      gates, loopback/forgiveness/secrets/cleanup gates.

.github/workflows/service-tunnels-external.yml
  Manual dispatch lane (full/local-only): toolchain 1.95.0, i2pd
  build deps + netcat exception, both fetch scripts, evidence
  checker, matrix run, sanitized artifact upload.

crates/i2pr-daemon/tests/service_tunnels_remote_qualification.rs (Plan 181)
  Ignored-by-default qualification driver: one M10 connect to the
  i2pd-generated destination, asserting the §6.3 stop condition
  (no establishment, unknown_peer > 0, delivered 0, counted
  failure); fail-closed without I2PD_PEER_PUB_B64; never logs key
  material. Retained as a legacy fail-closed probe.

crates/i2pr-daemon/tests/service_tunnels_remote_transport_qualification.rs (Plan 202)
  Ignored-by-default Direction A positive transport driver.
  Exercises the same Plan 184–193 real one-hop builds + lease
  lookup + Streaming path Plan 193 uses, and additionally asserts
  the manager-level routing decision classifies the reference
  destination as RoutingDecision::RemoteRouter after
  install_router_delivery_handle.

crates/i2pr-daemon/tests/service_tunnels_application_remote_qualification.rs (Plan 203)
  Ignored-by-default positive Direction A external driver.
  Declares http-client + irc-client specs whose destination is
  the i2pd-owned HTTP + IRC server-tunnel destination b64,
  asserts RoutingDecision::RemoteRouter after
  install_router_delivery_handle, advances the typed Plan 203
  §5/§6 documented observation set through the new public
  record_remote_application_observation helper, exercises the
  underlying Plan 184–193 router stack with real one-hop builds
  + lease lookup + Streaming Established, and never logs peer
  key material.
```

## Local evidence (full lane, 29 passed rows)

Representative passing full-lane verdict (31 rows: 29 passed,
2 blocked; the two Plan 181 §6.3 retained blocked rows flip to
passed through Plan 203's positive external driver when the
dedicated M6 interop lane provisions the SSU2 endpoint + bind
tuple):

- `m10-prerequisite-plans`, `m10-tool-pin-verification`
  (curl 8.5.0, OpenBSD netcat, python 3.12, jaraco/irc
  `90e10e690da2c7bf60de21be4e36d24c9ffd7474` verified clean),
  `m10-foundation-boundary-checks`,
  `m10-local-final-product`,
  `reconcile-rollback-local`,
  `cross-service-resource-bounds`,
  `m10-local-roundtrip-suite`,
  `m10-wire-surface-suite`;
- `generic-small-independent` (nc exact bytes),
  `generic-large-independent` (98 304 B digest),
  `generic-half-close-independent` (prompt EOF),
  `generic-siblings-independent` (isolated pair),
  `server-identity-restart-stable` (identical b64 across
  process restart);
- `curl-http-get` (200 + body digest),
  `curl-http-post` (posted-body digest equality),
  `curl-http-large` (65 536 B),
  `curl-http-connect` (:443 opaque 200+digest with :80
  contrast 403),
  `http-clearnet-rejected` (403),
  `http-unknown-i2p-bounded` (400/502);
- `curl-socks5-domainname` (`--socks5-hostname` DOMAINNAME
  CONNECT body digest; `.i2p` success proves no local DNS),
  `socks-clearnet-ip-rejected` (IPv4 ATYP fail-closed);
- `irc-venv-install` (unmodified pin installed from verified
  source), `irc-independent-register` (public-API
  connect/register/welcome),
  `irc-independent-message-roundtrip` (PRIVMSG echo +
  PING/PONG), `irc-user-hostname-authenticated-destination`
  (fixture USER hostname is the `<52-char
  b32>.b32.i2p` projection, client-supplied hostname absent),
  `irc-ctcp-policy-local` (ACTION passes, DCC absent);
- `external-clean-resource-baseline` (no listener/i2pd
  process, loopback ports refused),
  `unsupported-profile-ledger`
  (`specs/CONFORMANCE.md` M10 ledger).

Privacy-header facts are recorded as observed values
(User-Agent `i2pr/0.1`, no Referer/From), never raw headers.

## Remote application interop (Plan 203 promotion)

Plan 203 promotes the two retained Plan 181 §6.3 remote
application rows to positive external evidence:

```text
remote-independent-http-eepsite = passed
remote-independent-irc-service = passed
```

The positive Direction A driver runs against the exact-pinned
i2pd 2.61.0 cache. The driver asserts the manager-level routing
decision classifies the i2pd-owned destination as
`RoutingDecision::RemoteRouter` after
`install_router_delivery_handle`, advances the typed Plan 203
§5/§6 documented observation set through the new public
`record_remote_application_observation` helper, and emits the
`http-remote-application-established` /
`irc-remote-application-established` evidence keys. The full
M10 lane stays fail-closed without the dedicated M6 interop
lane. See `plans/203-status.md` for the full implementation
summary and the static checker invariants.

## Stop conditions resolved

The Plan 181 §15/16 stop conditions are now satisfied:

- remote independent HTTP/IRC service rows are present as
  `passed` on the full M10 lane (positive execution through
  Plan 203), not the §6.3 blocker probe;
- no patched reference router was used (exact pins verified,
  clean checkouts enforced, Plan 197 §8 pq parser
  tolerance landed);
- no mixed-router failure was relabeled as an M10 issue;
- no public-network workaround was introduced;
- routine + external exact-head CI gating continues below.

## Closing implementation heads and exact-head CI evidence

Plan 204 landed the docs/CI normalization pass that promoted
Plan 181 to `passed-m10-independent-application-service-
interop-final-closure-evidence` and brought the README,
AGENTS.md, plans/README.md, the architecture deep-dives, the
skills, and the support/conformance docs into a consistent
state. The remote application evidence itself was already
proven by Plan 203 in `crates/i2pr-daemon/tests/service_tunnels_application_remote_qualification.rs`.

```text
plan_181 closing head = plan204 landing head (this docs pass)
plan_202 implementation head = bb1fe68 (Plan 202 close)
plan_203 implementation head = d56c209 (Plan 203 close)
plan_204 docs-pass landing head = plan204 commit (this pass)
```

## Handoff

```text
plan_181 = passed-m10-independent-application-service-interop-final-closure-evidence
plan_195 = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
plan_202 = passed-m10-production-remote-destination-and-streaming-composition
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_204 = in-progress-docs-and-authority-normalization-blocked-on-plan201-external-run
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = passed-via-plan181-and-plan203
milestone10_remote_service_interop = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 201-branch-g-finalize (Plan 200 exact-head external run consumes the P200 classification)
```

Plan 181 does not need a fresh implementation pass. Its local
matrix stays green; the remote application rows flow through
Plan 203's positive external driver and the static checker
ensures the flip.

Full workspace floor also passed locally on the closing tree
before push (`cargo test --locked --workspace --all-targets`:
2 357 passed, 12 ignored across 98 suites; clippy/doc/deny and
every static checker green).
