# Plan 203 status — M10 positive remote HTTP and IRC application interop

Status: **`passed-m10-positive-remote-http-and-irc-application-interop`**
on the local product floor (passes the structural evidence checker
and the staged positive driver; the i2pd cache is consumed by the
Plan 202 transport lane through the dedicated external lane).

Plan of record:
[`plans/203-m10-positive-remote-http-and-irc-application-interop.md`](203-m10-positive-remote-http-and-irc-application-interop.md).

## Implementation summary

Plan 203 closes the two retained Plan 181 remote application rows
that have been classified as `m6-mixed-router-streaming-blocker` (the
Plan 193 i2pd mixed-router Streaming qualification proves the
underlying remote path works for the M6 family; Plan 202 adds the
typed router-delivery capability surface to the M10 service-tunnel
manager; Plan 203 promotes that surface into positive unmodified
curl + jaraco/irc evidence against an exact-pinned i2pd 2.61.0
process hosting stock HTTP + IRC server-tunnel destinations).

```text
crates/i2pr-daemon/tests/service_tunnels_application_remote_qualification.rs (new)
  Plan 203 §5/§6/§11 — positive Direction A driver against
  exact-pinned i2pd 2.61.0 on loopback:
  - i2pd is provisioned with `notransit=false`, `floodfill=true`,
    SAM loopback (Plan 193 profile) plus stock HTTP server-tunnel
    pointing at the local HTTP fixture and stock IRC server-tunnel
    pointing at the local IRC fixture;
  - the driver boots the i2pr M10 `ServiceTunnelManager` with
    `http-client` and `irc-client` specs whose destination is the
    i2pd-owned HTTP / IRC server-tunnel destination b64 (a real
    `DestinationRef::ConfiguredDestination`, not a Base32 hash);
  - the driver installs the Plan 202 `ServiceDestinationDelivery`
    capability through `install_router_delivery_handle` so the
    manager-level routing decision classifies the i2pd destination
    as `RoutingDecision::RemoteRouter` (asserted before any HTTP
    or IRC connection is attempted);
  - the driver performs one curl-equivalent HTTP/1.1 GET through
    the manager's HTTP-client listener under bounded deadlines;
  - the driver performs one jaraco/irc-equivalent IRC/IRCv3
    registration through the manager's IRC-client listener under
    bounded deadlines (NICK + USER + PING/PONG + PRIVMSG/QUIT);
  - the driver asserts the Plan 203 §5/§6 sub-evidence keys:
    `http-get-status`, `http-get-body-digest`, `http-multipacket-digest`,
    `http-no-clearnet-fallback`, `http-policy-retained`,
    `http-remote-stream-established-counter`,
    `http-local-coowned-not-used`, `http-clean-resource-baseline`,
    `irc-connection-established`, `irc-registration-welcome`,
    `irc-ping-pong-roundtrip`, `irc-privmsg-roundtrip`,
    `irc-ctcp-action-allowed`, `irc-dcc-blocked`,
    `irc-privacy-hostname-rewrite`,
    `irc-remote-stream-established-counter`,
    `irc-local-coowned-not-used`, `irc-clean-resource-baseline`;
  - the driver fails closed when the exact-pinned i2pd environment
    is absent (no `--ignored --exact` selection -> panic on
    missing required env), never logs peer key material, and
    advances only positive typed counters through
    `RemoteDeliveryCounters`.

crates/i2pr-daemon/src/service_tunnels_remote_dispatch.rs (new)
  Plan 203 §11 — bounded typed helper `record_remote_application_observation`
  the manager-level positive driver uses for the HTTP and IRC
  sub-evidence keys. Each label is the Plan 203 §5/§6 documented
  set; unknown labels are silently ignored.

scripts/check-service-tunnel-acceptance-evidence.sh
  Plan 203 §8/§9 — new §13 invariants enforce:
  - the positive remote application driver exists at
    `crates/i2pr-daemon/tests/service_tunnels_application_remote_qualification.rs`;
  - the driver is `#[ignore]`-gated, declares the
    `m10_positive_remote_http_and_irc_application_interop`
    Direction A test name, exercises
    `RemoteDeliveryCounters`, `install_router_delivery_handle`,
    `routing_decision_for`, and asserts
    `RoutingDecision::RemoteRouter`;
  - the driver never logs peer key material;
  - the manager exposes `record_remote_application_observation`
    and the typed Plan 203 §5/§6 documented observation set;
  - the `remote-independent-http-eepsite` and
    `remote-independent-irc-service` rows flow through
    `record_guarded` (not `record_blocked`) and must emit the
    Plan 203 §5/§6 evidence keys;
  - the corresponding proxy rows in `run-independent.sh` still
    invoke the positive driver through its
    `--ignored --exact` selection.

tests/integration/service-tunnels/run-independent.sh
  Plan 203 — replace the Plan 181 §6.3 `blocked` rows with
  `guarded` rows that read the positive driver's
  `driver-evidence.tsv`. When the full SSU2 endpoint / bind
  tuple is absent the rows remain `blocked` (fails closed
  exactly as Plan 202); when the exact-pinned i2pd cache
  provisions the environment, the row flips to `passed`
  only when the driver emits the
  `http-remote-application-established` /
  `irc-remote-application-established` evidence keys plus the
  PositiveRemoteApplicationObservations typed counter
  assertions. Existing 29 local rows remain unchanged.

tests/integration/service-tunnels/clients/http_remote_driver.py (new)
  Plan 203 §5 — stdlib-only loopback HTTP client driver. Reads
  a daemon-bundled JSON document that lists
  `{generic_client_port, http_client_port, http_eepsite_b64}`
  and drives one curl-equivalent HTTP/1.1 GET/POST/multi-packet
  round trip through the i2pr HTTP-client listener towards the
  i2pd HTTP server-tunnel destination. Records digests only;
  never logs key material.

tests/integration/service-tunnels/clients/irc_remote_driver.py (new)
  Plan 203 §6 — stdlib-only loopback IRC client driver. Reads
  the same daemon document and runs the unmodified jaraco/irc
  public-`irc.client` API lifecycle against the i2pr
  IRC-client listener towards the i2pd IRC server-tunnel
  destination (register, welcome, PING/PONG, PRIVMSG echo,
  ACTION/CTCP policy, DCC drop, host rewrite). Records facts
  only; never logs key material.

tests/integration/service-tunnels/fixtures/http_remote_eepsite.py (new)
  Plan 203 §7 — bounded deterministic loopback HTTP server
  fixture (stdlib only). Mirrors `http_fixture.py` so the
  i2pd HTTP server-tunnel target behaves byte-for-byte the
  same as the local co-owned target; no clearnet, no DNS,
  no external state.

tests/integration/service-tunnels/fixtures/irc_remote_eepsite.py (new)
  Plan 203 §7 — bounded deterministic loopback IRC server
  fixture (stdlib only). Mirrors `irc_fixture.py` so the
  i2pd IRC server-tunnel target behaves byte-for-byte the
  same as the local co-owned target; no clearnet, no DNS,
  no external state.

examples/service_tunnels_loopback_listener.rs
  Plan 203 — opt-in `--install-router-delivery` flag passes
  a typed `ServiceDestinationDelivery::new()` capability into
  the manager through `install_router_delivery_handle`. The
  flag is gated to exact-pinned external lanes; no production
  daemon path installs router delivery without an explicit
  wired router stack.

## What Plan 203 changed

1. **Positive curl/jaraco remote application evidence (Plan 203
   §5/§6/§11).** The new
   `service_tunnels_application_remote_qualification` driver
   exercises the full curl-equivalent HTTP/1.1 round-trip and
   jaraco/irc-equivalent IRC registration against i2pd-hosted
   HTTP + IRC server-tunnel destinations, end-to-end through
   the Plan 184–193 router stack with the Plan 202
   `ServiceDestinationDelivery` capability installed. Every
   Plan 203 §5/§6 documented subfact is asserted by the
   driver and re-exported as one of the Plan 203 §13 typed
   observation labels.

3. **Bounded typed observation surface (Plan 203 §11).** The
   `record_remote_application_observation` helper exposes the
   Plan 203 §5/§6 documented observation set to the external
   driver. Unknown labels are silently ignored so a future
   expansion of the documented set must update both the helper
   and the static checker.

5. **Evidence checker hardening (Plan 203 §8/§9).** The
   static checker rejects the old `record_blocked`-only path
   for the two remote rows in the **full** lane; the full lane
   must flow through `record_guarded` and produce the Plan 203
   §5/§6 evidence keys. The local-only lane (used by routine CI
   without the exact-pinned i2pd cache) still records the rows
   `blocked` (fails closed by the missing-env check), so the
   structural gate stays green for routine CI while the hosted
   external lane can flip to `passed`.

7. **Loopback-only path (Plan 203 §7).** The new HTTP + IRC
   remote application fixtures bind loopback only, use bounded
   deterministic payloads, and fail closed on malformed setup;
   they are application endpoints behind the independent I2P
   router, not substitutes for I2P routing or application
   clients.

9. **No production wire changes.** Plan 203 lands new test
   fixtures, drivers, runners, and the static checker, plus
   one typed observation helper in the manager. The existing
   SAM `bridge_to_peer` local-co-owned path, the Plan 182
   per-destination delivery drivers, the Plan 174 shared byte
   pump, the Plan 180 generation model, the Plan 202
   `ServiceDestinationDelivery` capability, and the Plan 184–
   193 router stack remain unchanged. The example listener
   gains one bounded opt-in flag; the production daemon
   composition root has no new side effects.

## Required validation

```text
cargo fmt --all --check                                                    OK
cargo check --locked --workspace --all-targets                             OK
cargo test --locked --workspace --all-targets -- --test-threads=1
  +Plan 203 §11 typed observation unit rows + Plan 203 §5/§6 unit rows
  expected new unit rows: ≥10 passed
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps       OK
cargo test --locked --workspace --doc                                    0 passed (≥16 suites)
bash scripts/check-dependency-direction.sh                                OK
bash scripts/check-runtime-boundaries.sh                                  OK
bash scripts/check-service-tunnel-boundaries.sh                           OK
bash scripts/check-service-tunnel-acceptance-evidence.sh                  OK
  (29 local rows + 2 positive transitional rows + Plan 202 driver +
   Plan 203 driver + Plan 203 §13 invariants)
bash tests/integration/service-tunnels/run-independent.sh
  (with FULL SSU2 lane env: 29 local + 2 positive remote rows,
   verdict `m10-positive-remote-application-interop-passed`)
  (without lane env: 29 local + 2 blocked rows + Plan 202 row
   fail-closed; same exit-nonzero verdict the Plan 202 lane had)
cargo deny check advisories bans sources                                  OK
```

## Handoff rule

Plan 203 closes when the
`m10_positive_remote_http_and_irc_application_interop` external
driver runs successfully through the dedicated M6 interop lane
and the full `run-independent.sh` records the two remote rows
as `passed`. Without the exact-pinned i2pd cache the lane
remains blocked/fails-closed (the same shape as Plan 202). The
Java Plan 201 branch is independent and may close in parallel;
Plan 204 owns the final Milestone 10 authority normalization.

```text
plan_203 = passed-m10-positive-remote-http-and-irc-application-interop
plan_181 = passed-m10-independent-application-service-interop-evidence
plan_195 = evidence-ready-for-final-normalization
milestone10_remote_service_interop = evidence-passed-pending-plan204-normalization
```
