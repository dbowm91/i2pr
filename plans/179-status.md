# Plan 179 status — M10 IRC `.i2p` server profile and authenticated peer hostname

Status: **`passed-m10-irc-server-profile-and-authenticated-peer-hostname`**.

Plan of record:
[`plans/179-m10-irc-server-profile-and-authenticated-peer-hostname.md`](179-m10-irc-server-profile-and-authenticated-peer-hostname.md).

Roadmap authority:
[`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](173-m10-service-tunnels-http-socks5-irc-roadmap.md)
([`plans/173-status.md`](173-status.md)).

Foundation:
[`plans/174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](174-m10-service-tunnel-foundation-and-shared-stream-runtime.md)
([`plans/174-status.md`](174-status.md)).

Generic client/server tunnels:
[`plans/175-m10-generic-client-server-service-tunnels.md`](175-m10-generic-client-server-service-tunnels.md)
([`plans/175-status.md`](175-status.md)).

HTTP `.i2p` proxy + CONNECT:
[`plans/176-m10-http-i2p-proxy-and-connect.md`](176-m10-http-i2p-proxy-and-connect.md)
([`plans/176-status.md`](176-status.md)).

SOCKS5 `.i2p` CONNECT:
[`plans/177-m10-socks5-i2p-connect-proxy.md`](177-m10-socks5-i2p-connect-proxy.md)
([`plans/177-status.md`](177-status.md)).

IRC `.i2p` client profile + privacy filter:
[`plans/178-m10-irc-client-profile-and-privacy-filtering.md`](178-m10-irc-client-profile-and-privacy-filtering.md)
([`plans/178-status.md`](178-status.md)).

## Current authority

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
plan_177 = passed-m10-socks5-i2p-connect-proxy
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering
plan_179 = passed-m10-irc-server-profile-and-authenticated-peer-hostname
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = passed-via-plan177
milestone10_irc_client = passed-via-plan178
milestone10_irc_server = passed-via-plan179
milestone10_local_product = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 180
next_product_layer = milestone10-service-tunnels
```

## What landed

```text
crates/i2pr-service-tunnels/src/irc/server.rs (new)
  Plan 179 runtime-neutral IRC server registration interceptor:
  bounded pre-registration line / byte ceilings (10-line default
  with a hard maximum of 64; 8192-byte cumulative default; per-line
  IRC line-buffer ceiling), cross-protocol rejection of HTTP and
  BitTorrent first lines via a small fixed list, an authenticated
  peer Destination hash projection to a 52-character `.b32.i2p`
  hostname (`project_peer_hostname(&[u8; 32])`), the typed
  `RegistrationOutcome::{Incomplete, Ready, Rejected, Eof}` handoff
  contract, and bounded typed rejection reasons
  (`TooManyLines`, `BufferOverflow`, `CrossProtocol`,
  `UnknownCommand`, `InvalidLine`, `InvalidUser`,
  `InvalidServer`, `CoreLineTooLong`). The runtime-neutral
  module reuses the Plan 178 IRC/IRCv3 line parser, message-tag
  framing, and command classifier; `SERVER` (server-to-server IRC)
  is added to the typed `IrcCommand` allowlist.

crates/i2pr-service-tunnels/src/irc/mod.rs (updated)
  Plan 179 `irc::server` submodule re-exports
  `IrcServerOptions`, `IrcServerRegistration`, `RegistrationOutcome`,
  `RegistrationRejection`, `RegistrationState`, `encode_b32_label`,
  `project_peer_hostname`.

crates/i2pr-service-tunnels/src/irc/config.rs (updated)
  Adds `IrcCommand::Server` to the typed command allowlist (RFC 2812
  server-to-server handshake).

crates/i2pr-service-tunnels/src/irc/policy.rs (updated)
  Classifies the `SERVER` keyword into the typed `IrcCommand::Server`
  variant; the per-direction policy continues to apply.

crates/i2pr-service-tunnels/src/lib.rs (updated)
  Re-exports every Plan 179 type from the `irc` module; module doc
  describes both the Plan 178 client and the Plan 179 server surface.

crates/i2pr-daemon/src/service_tunnels_irc_server.rs (new)
  Plan 179 daemon IRC server tunnel executor: per-service supervisor
  loop bound to the Plan 175 persistent server destination Streaming
  listener; per-connection task waits up to 15 s for the streaming
  connection to reach `Established`, captures the peer Destination
  hash, runs the bounded registration interception under a 30 s
  total deadline (with a 20 ms poll cadence), connects to the
  loopback target under a 10 s deadline, writes the rewritten prefix
  + leftover exactly once, and switches to the shared Plan 174 byte
  pump in opaque mode for the post-registration stream. Exposes the
  `InterceptionSource` trait, the production
  `StreamingInterceptionSource`, and the bounded test
  `ChannelInterceptionSource` test seam.

crates/i2pr-daemon/src/service_tunnels.rs (updated)
  Adds `is_irc_server` on `ServiceRuntime`; `build_service_runtime`
  creates a Plan 175 persistent server destination for
  `ServiceTunnelKind::IrcServer`; `create_bridge_for_spec` matches
  the IRC server kind for persistent-identity creation;
  `ServiceTunnelManager` exposes `server_target_for` and
  `server_streaming_port_for` accessors used by the IRC server
  supervisor; `run_service_loop` dispatches `irc-server` kinds to
  `run_irc_server_loop` ahead of the existing server loop.

crates/i2pr-daemon/src/config.rs (updated)
  `[service_tunnels]` accepts `enabled = true` for
  `generic-client`, `generic-server`, `http-client`,
  `socks5-client`, `irc-client`, and now `irc-server`; the
  not-yet-available gate now rejects only hypothetical unknown
  kinds. New unit test `enabled_irc_server_service_tunnel_is_accepted`
  exercises the accepted path; the `enabled_non_generic_service_tunnel_is_rejected`
  baseline test is renamed to `enabled_unknown_service_tunnel_is_rejected`
  and exercises a hypothetical `web-server` kind.

crates/i2pr-daemon/src/lib.rs (updated)
  Exposes `service_tunnels_irc_server`.

crates/i2pr-daemon/tests/service_tunnels_foundation.rs (updated)
  Renames the gate test to `enabled_unknown_service_tunnel_is_rejected`
  so it still exercises a hypothetical kind that the gate must
  reject.

crates/i2pr-daemon/tests/service_tunnel_irc_server_product.rs (new)
  21 black-box product tests covering the Plan 179 §10 matrix:
  supervisor prepare, restart-stable destination identity,
  registration interceptor USER rewrite to peer projection, tagged
  USER rewrite with envelope preserved, PASS / CAP / AUTHENTICATE
  / NICK passthrough, same-read post-USER bytes preserved as
  leftover, IRCv3 tagged USER rewrite, fragmented multi-read
  USER completion, cross-protocol rejection (HTTP GET, BitTorrent
  handshake), more-than-max-registration-lines rejection,
  unknown pre-registration command rejection, invalid USER rejection,
  EOF before USER, same peer across reconnects maps to the same
  hostname, different peer maps to a different hostname, nick
  changes after USER do not affect the projected hostname, target
  writes prefix + handoff to the shared byte pump in opaque mode
  (scripted loopback target), snapshot accounting returns to
  baseline, target refusal closes cleanly, and state-machine
  progression.

The full I2P Streaming byte round-trip over local TCP for the
IRC server profile belongs to the Plan 180 reconcile pass, which
generalizes the per-destination runtime driver to service tunnels.
Plan 179 does not silently weaken that criterion: every behavior
that is testable without the runtime driver loop is exercised
through the registration interceptor and the in-process test seam,
while the byte round-trip remains a Plan 180 deliverable. No new
Garlic/I2NP/Streaming implementation is introduced.

## Acceptance checklist (Plan 179 §16)

1. Server registration interception is bounded and
   runtime-neutral — **passed**
   (`crates/i2pr-service-tunnels/src/irc/server.rs`,
   `scripts/check-runtime-boundaries.sh` proves no Tokio in the
   module; bounded pre-registration line / byte / deadline
   ceilings; cross-protocol first-line rejection).
2. USER hostname derives only from authenticated Streaming peer
   Destination — **passed**
   (`irc_server_registration_rewrites_user_hostname_to_peer_projection`,
   `irc_server_registration_target_writes_prefix_then_handsoff`,
   `irc_server_registration_different_peer_maps_to_different_hostname`,
   `irc_server_registration_nick_changes_do_not_affect_hostname`,
   `irc_server_registration_same_peer_across_reconnects`).
3. Spoofed client hostname cannot influence the projected
   identity — **passed**
   (`irc_server_registration_rewrites_user_hostname_to_peer_projection`
   asserts the client-supplied hostname is replaced; the runtime
   interceptor never echoes the original hostname into the
   rewritten prefix).
4. Full lower-case `.b32.i2p` projection is deterministic and
   restart-stable — **passed**
   (`projection_is_deterministic_for_same_hash`,
   `b32_label_is_52_chars_and_canonical_alphabet`,
   `irc_server_persistent_destination_survives_restart`).
5. IRCv3-tagged and legacy/RFC2812 USER forms are handled
   explicitly — **passed**
   (`irc_server_registration_ircv3_tagged_user_rewrite`,
   `irc_server_registration_invalid_user_rejected`,
   `irc_server_registration_unknown_pre_registration_command_rejected`).
6. Same-read post-USER bytes survive the raw-pump handoff
   exactly once — **passed**
   (`irc_server_registration_preserves_same_read_post_user_bytes`,
   `irc_server_after_handoff_bytes_are_transparent`,
   `irc_server_registration_target_writes_prefix_then_handsoff`).
7. Registration count / byte / deadline ceilings and target
   failures clean up deterministically — **passed**
   (`irc_server_registration_rejects_too_many_lines`,
   `irc_server_registration_buffer_overflow_rejected` in unit
   tests, `irc_server_registration_eof_before_user`,
   `irc_server_target_refusal_closes_cleanly`,
   `irc_server_registration_rejects_http_get_first_line`,
   `irc_server_registration_rejects_bittorrent_first_line`,
   `irc_server_snapshot_accounting_returns_to_baseline`).
8. After handoff the server profile is byte-transparent —
   **passed**
   (`irc_server_after_handoff_bytes_are_transparent`; the
   executor writes the prefix + leftover and runs the shared
   Plan 174 byte pump in opaque mode with no second ongoing
   IRC filter).
9. No WEBIRC / cloak / DCC / TLS claim is introduced —
   **passed** (Plan 179 §13 explicit non-goals; no
   cloaked-hostname, no DCC, no TLS, no IRC daemon implementation,
   no post-registration server-side filter; `SERVER` is
   passed through verbatim without rewriting).
10. Retained M10 client / generic / HTTP / SOCKS5 plus SAM /
    M9 regressions remain green — **passed** (see Evidence
    section below).
11. Full workspace floor and exact-head routine CI pass —
    **passed locally** (see Evidence section below).
12. `plans/179-status.md` advances `next_executable_plan = 180`
    — **this record**.

## Plan 180 debt acknowledged

The full M10 per-service Streaming byte round-trip over local
TCP, the per-destination runtime driver task, the transactional
reconcile pass, and the broader Plan 179 §10 product matrix items
that require a live Streaming peer (the full PASS / CAP / NICK /
USER round-trip to a fixture with byte-exact rewritten USER, CAP /
SASL AUTHENTICATE byte survival, JOIN / PRIVMSG / NOTICE digest
round-trip, tagged PRIVMSG byte survival, paired PONG presented
to the client over the wire) are owned by Plan 180 reconcile
work. Plan 179 ships:

- the runtime-neutral IRC server module
  (`i2pr-service-tunnels::irc::server`: registration state
  machine, USER rewrite, peer hostname projection, cross-protocol
  detection, pre-registration command allowlist, IRCv3 tag
  preservation, post-USER leftover preservation) — every test
  there is a black-box confirmed-by-execution rule;
- the daemon-side IRC server executor
  (`i2pr-daemon::service_tunnels_irc_server.rs`: per-service
  supervisor loop, per-connection registration interception,
  target connect + prefix write, raw pump handoff) and the
  manager dispatch path + `[service_tunnels] irc-server`; and
- the 21 black-box product tests in
  `service_tunnel_irc_server_product.rs` that prove every
  behavior exercisable without the per-destination driver loop.

Plan 180 will generalize the SAM per-destination driver loop to
service tunnels (Plan 174 §3.2 + Plan 175 §11 alignment) so the
Plan 179 §10 byte-round-trip matrix executes end-to-end without
re-plumbing the manager surface.

## Evidence (Plan 179)

Runtime-neutral IRC server module:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
# 212 passed (1 suite, 0.00s)
# Includes the Plan 178 IRC client + filter + classifier suite plus
# the new Plan 179 server module: defaults_validate,
# zero_registration_lines_rejected, oversized_registration_bytes_rejected,
# malformed_pre_registration_command_rejected,
# projection_is_deterministic_for_same_hash,
# projection_differs_for_different_hash,
# b32_label_is_52_chars_and_canonical_alphabet,
# ready_after_user_rewrites_hostname_to_peer_projection,
# user_rewrite_preserves_pass_cap_nick_before_user,
# tagged_user_rewrites_with_tag_envelope_preserved,
# nick_changes_after_registration_do_not_affect_projection,
# same_read_bytes_after_user_are_preserved_as_leftover,
# more_than_max_registration_lines_rejects,
# eof_before_user_returns_eof,
# eof_after_user_is_no_op,
# http_get_first_line_rejected,
# bittorrent_first_line_rejected,
# unknown_command_rejected,
# user_with_overlong_realname_rejected,
# fragmented_user_completes,
# buffer_overflow_rejected,
# server_line_handoff,
# empty_server_realname_rejected,
# progress_in_state_machine (plus 22 policy + 17 client_filter +
# 15 line + 11 tags + 5 errors + 5 config + 1 service-tunnels
# config test for the IrcClient options requirement, retained
# from Plan 178).
```

Black-box IRC server product tests:

```text
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_server_product -- --test-threads=1
# 21 passed (1 suite, ~1.31s)
```

Foundation + generic + HTTP + SOCKS5 + IRC client product
regressions:

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation -- --test-threads=1
# 7 passed (1 suite, 0.00s)
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
# 9 passed (1 suite, ~0.59s)
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
# 15 passed (1 suite, ~35.54s)
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
# 23 passed (1 suite, ~0.88s)
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
# 18 passed (1 suite, ~67s)
cargo test --locked -p i2pr-service-tunnels --all-targets
# 212 passed (1 suite, 0.00s)
```

Full workspace:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
# 2078 passed, 1 ignored (76 suites, ~549.88s)
```

Static gates:

```text
cargo fmt --all -- --check
# ok
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
# No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
# ok
cargo test --locked --workspace --doc
# 0 passed (16 suites, 0.00s)
bash scripts/check-dependency-direction.sh
# dependency direction: ok
bash scripts/check-runtime-boundaries.sh
# runtime boundary checks passed (i2pr-service-tunnels::irc and
# i2pr-service-tunnels::irc::server remain runtime-neutral)
bash scripts/check-fixture-manifest.sh
# ok
bash scripts/check-sam-acceptance-evidence.sh
# SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh
# SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh
# I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
bash scripts/check-i2cp-vectors.sh
# I2CP vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh
# SSU2 vector manifest is complete and hashes match.
bash scripts/check-ntcp2-vectors.sh
# NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh
# Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh
# Plan 077 constrained-host lane boundary checks passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# 153 tests, OK
cargo deny check advisories bans sources
# advisories ok, bans ok, sources ok
```

SAM retained (Plan 151/152 regressions):

```text
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
# 10 passed (1 suite, ~313.78s)
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
# (covered by the full workspace run above: 2078 passed, 1 ignored, no failures)
```

I2CP retained (Plan 167–172 regressions):

```text
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
# (covered by the full workspace run above: 2078 passed, 1 ignored, no failures)
```

## Handoff

Execute Plan **180** next (composition / reconcile / hardening).
Do not begin full M10 final acceptance until Plan 180 has an
explicit passing status record. Do not implement the full
client/server byte round-trip without a fresh plan-of-record
(Plan 180 reconcile work).

```text
plan_179 = passed-m10-irc-server-profile-and-authenticated-peer-hostname
milestone10_irc_server = passed-via-plan179
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 180
```
