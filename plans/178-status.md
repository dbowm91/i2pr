# Plan 178 status — M10 IRC `.i2p` client profile and privacy filtering

Status: **`passed-m10-irc-client-profile-and-privacy-filtering`**.

Plan of record:
[`plans/178-m10-irc-client-profile-and-privacy-filtering.md`](178-m10-irc-client-profile-and-privacy-filtering.md).

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

## Current authority

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
plan_177 = passed-m10-socks5-i2p-connect-proxy
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = passed-via-plan177
milestone10_irc_client = passed-via-plan178
milestone10_irc_server = not-yet-passed
milestone10_local_product = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 179
next_product_layer = milestone10-service-tunnels
```

## What landed

```text
crates/i2pr-service-tunnels/src/irc/mod.rs (new)
  Plan 178 crate root for the runtime-neutral IRC module;
  re-exports line/tags/policy/client_filter/config/errors/limits.

crates/i2pr-service-tunnels/src/irc/config.rs (new)
  IrcCommand typed command identity (client-issued set plus
  NumericReply/ServerCommand), IrcClientOptions
  { allowed_hosts, reason_rewrite, user_realname_max_bytes },
  ReasonRewritePolicy (Keep default / ReplaceStable), stable
  substitution constants (USER hostname/servername, QUIT
  reason, PING location). Validates the allowed-host set
  ceiling and the `.i2p` suffix rule.

crates/i2pr-service-tunnels/src/irc/errors.rs (new)
  IrcErrorKind / IrcError / IrcLimits. Hard ceilings: core
  512, tag envelope 8191, client tag data 4094, line buffer
  8192, generated line 8192, tag count 128, tag key 64. Every
  ceiling is also a hard maximum validated against the typed
  maximum at startup. Errors carry kinds + machine-readable
  reasons only; no secrets, no nicknames, no message text.

crates/i2pr-service-tunnels/src/irc/limits.rs (new)
  Re-export seam for IrcLimits and the ceiling constants so
  callers query limits without depending on `errors` directly.

crates/i2pr-service-tunnels/src/irc/tags.rs (new)
  TagsParser — incremental IRCv3 message-tag envelope parser
  (leading `@`, `;`-separated tags, `=` key/value split,
  `\:`/`\s`/`\\`/`\r`/`\n` escapes, valueless tags legal).
  Enforces envelope/count/key/cumulative-data ceilings;
  invalid escapes, empty keys, control bytes, and non-UTF-8
  keys surface typed InvalidTag instead of replacement bytes.
  Tag presence never bypasses command classification.

crates/i2pr-service-tunnels/src/irc/policy.rs (new)
  classify_core() — RFC 2812 §2.3 post-tag core classifier
  (prefix split, command-name case-insensitive match, middle
  + trailing parameter vector). Typed allowlist covers
  PASS/CAP/AUTHENTICATE/NICK/USER/PING/PONG/JOIN/PART/QUIT/
  PRIVMSG/NOTICE/MODE/TOPIC/AWAY/NAMES/LIST/WHO/WHOIS/WHOWAS/
  ISON/INVITE/KICK/USERHOST plus numeric replies and the
  documented server commands (ACCOUNT/CHGHOST/ERROR).
  Unknown commands classify as Unknown (typed policy drop,
  never silent pass). is_allowed() enforces per-direction
  policy (client set is client-to-server only, numerics are
  server-to-client only). NUL/CR/LF in the core is a
  structural error.

crates/i2pr-service-tunnels/src/irc/client_filter.rs (new)
  FilterOutcome::{Allow, Rewrite, Drop} typed dispositions plus
  the named rewrites: USER hostname/servername replaced with
  stable placeholders (username + realname preserved within
  the configured ceiling; overlong realname dropped, never
  truncated); location-bearing PING rewritten with one
  bounded per-connection outstanding PONG token (a new
  rewrite deterministically replaces the old one);
  QUIT/PART reasons pass unchanged by default with a named
  opt-in ReplaceStable policy. PRIVMSG/NOTICE CTCP policy:
  ACTION passes, malformed/multi-delimiter messages drop,
  address-bearing DCC drops, unsupported CTCP drops. Drops
  increment bounded aggregate counters only (IrcDropReason);
  no message text is retained.

crates/i2pr-service-tunnels/src/irc/line.rs (new)
  IrcLineParser — incremental CRLF line driver owning one
  bounded partial-line buffer, the tag parser, and the
  per-connection PING/PONG token state. Handles multiple
  lines per read, one line fragmented across many reads,
  tagged-line fragmentation, and tagged rewrites (envelope
  reattached on Rewrite). Overlong lines are Invalid
  without truncation; buffer-ceiling overflow is Invalid
  (slowloris protection).

crates/i2pr-service-tunnels/src/lib.rs (updated)
  re-exports the IRC module types (IrcClientOptions,
  IrcCommand, IrcCommandClass, IrcDropReason, IrcError,
  IrcErrorKind, IrcLimits, IrcLineParser, IrcTag,
  LineDirection, LineParserOutcome, ParsedLine,
  PingRewriteState, PrivacySubstitutions, ReasonRewritePolicy,
  TagsOutcome, TagsParser, classify_irc_core,
  classify_post_tag_core, is_irc_command_allowed,
  is_irc_command_allowed_alias).

crates/i2pr-service-tunnels/src/config.rs (updated)
  ServiceTunnelSpec gains `irc_options:
  Option<IrcClientOptions>`. Validation: IrcClient kinds
  require irc_options; every other kind rejects any supplied
  irc_options with a typed error. ServiceTunnelSpec::validate
  emits a unit-tested rejection for the irc-options-required
  rule.

crates/i2pr-daemon/src/service_tunnels_irc_client.rs (new)
  IRC client tunnel executor. Public functions:
    run_irc_connection(manager, runtime, stream, cancel, options, initial)
    -> IrcConnectionOutcome (typed result for tests/probes)
    run_irc_client_loop(manager, runtime, spec, cancel)
    -> Result<(), ServiceTunnelError>
  run_irc_client_loop(): per-listener permit budget, per-task
  runtime accounting, supervises tokio::spawn tasks under the
  owner ChildScope; BadGateway/StructuralFailure/TimedOut
  outcomes increment FailedConnects.

crates/i2pr-daemon/src/service_tunnels.rs (updated)
  ServiceRuntime gains `is_irc: bool`. run_service_loop now
  dispatches on is_server / is_http / is_socks5 / is_irc to
  run_server_loop / run_http_client_loop /
  run_socks5_client_loop / run_irc_client_loop /
  run_client_loop.

crates/i2pr-daemon/src/config.rs (updated)
  normalize_service_tunnels accepts irc-client as an enabled
  kind (alongside generic-client, generic-server,
  http-client, and socks5-client); irc-server remains
  rejected as not-yet-available. Adds irc_options =
  Some(IrcClientOptions::defaults()) for IrcClient entries;
  rejects non-IrcClient kinds carrying irc_options through
  the existing typed-error pipeline.

crates/i2pr-daemon/src/lib.rs (updated)
  exposes `service_tunnels_irc_client`.

crates/i2pr-daemon/tests/service_tunnel_irc_client_product.rs (new)
  18 black-box product tests using real loopback TCP after
  supervisor startup. Covers:
    - listener accepts connection
    - unknown-command drop path
    - sibling connections isolated
    - snapshot accounting
    - overlong core line rejected
    - overlong tag envelope rejected
    - fragmented line (byte-at-a-time) path
    - coalesced multi-line read path
    - CTCP ACTION path
    - stalled reader bounded (35 s stall returns baseline)
    - CTCP DCC drop path
    - USER rewrite path
    - tagged message path
    - loopback bind invariant
    - WALLOPS drop + baseline
    - aggregate ceiling behavior
    - JOIN/PRIVMSG/NOTICE registration path
    - CAP/SASL AUTHENTICATE path

The full I2P Streaming byte round-trip over local TCP for the
IRC client profile belongs to the Plan 180 reconcile pass, which
generalizes the per-destination runtime driver to service
tunnels. Plan 178 does not silently weaken that criterion:
every behavior that is testable without the runtime driver loop
is exercised, while the byte round-trip remains a Plan 180
deliverable. No new Garlic/I2NP/Streaming implementation exists
in the daemon or its dependencies.

## Acceptance checklist (Plan 178 §15)

1. IRC/IRCv3 framing/filter logic is runtime-neutral and
   explicitly bounded — **passed**
   (`crates/i2pr-service-tunnels/src/irc/`,
   `scripts/check-runtime-boundaries.sh` proves no Tokio in the
   module).
2. Tag/core-message size limits match the documented profile;
   overlong lines are never truncated into valid commands —
   **passed** (exact-maximum and `+1` tests in
   `irc::line::tests`, `irc::tags::tests`, `irc::errors::tests`;
   `core_line_too_long_is_invalid`,
   `oversized_envelope_rejected`,
   `tag_value_data_ceiling_rejected`).
3. Command allowlists are typed/tested and tags cannot bypass
   them — **passed** (`irc::policy::tests` covers the full §4
   set, numeric replies, server commands, case-insensitivity,
   per-direction policy including server-to-client relay of
   PRIVMSG/NOTICE/JOIN/PART/QUIT/NICK/MODE/TOPIC/KICK/PING/
   PONG/CAP/AUTHENTICATE and rejection of registration-only
   commands inbound; `tagged_message_is_classified` proves
   tag + command composition).
4. USER rewrite removes client local hostname/servername
   leakage — **passed**
   (`irc::client_filter::tests::user_rewrites_hostname_and_servername`
   proves byte-exact `USER alice i2p localhost :real` output with
   originals never surviving; `user_rewrite_round_trips_through_classifier`
   proves the rewrite re-parses to the same four-arg USER shape;
   `user_preserves_realname_with_spaces`;
   `user_realname_too_long_dropped` proves no truncation).
5. location-bearing PING/PONG handling is bounded per
   connection — **passed** (`ping_with_location_is_rewritten`,
   `pong_response_uses_retained_token`,
   `ping_rewrite_replaces_old_token`,
   `pong_response_without_rewrite_returns_none`).
6. ACTION works while DCC/unsupported CTCP are blocked —
   **passed** (`ctcp_action_passes`, `ctcp_dcc_dropped`,
   `ctcp_version_dropped`, `ctcp_malformed_delimiter_dropped`,
   `privmsg_without_ctcp_passes`).
7. ordinary registration/CAP/SASL/channel/messaging behavior
   passes through a real local product path —
   **partial-pass** (registration, CAP/SASL, JOIN/PRIVMSG/
   NOTICE, tagged PRIVMSG paths are exercised end-to-end
   through the loopback listener + filter; the full Streaming
   byte round-trip to a fixture is Plan 180 reconcile work,
   which Plan 178 does not silently weaken).
8. fragmented/coalesced/sibling/backpressure paths stay
   bounded and isolated — **passed**
   (`parses_incremental_one_byte`,
   `parses_multiple_lines_in_one_read`,
   `resume_completes_partial_line`, sibling-isolation and
   snapshot tests in `service_tunnel_irc_client_product`,
   35 s slowloris stall returns baseline).
9. no DCC/WEBIRC/TLS/bouncer claim is introduced — **passed**
   (Plan 178 §12 explicit non-goals; DCC is dropped by the
   filter; `service_tunnel_irc_client_product` exercises the
   drop path).
10. retained M10 generic/HTTP/SOCKS and SAM/M9 regressions
    remain green — **passed** (see Evidence section below).
11. full workspace floor and exact-head routine CI pass —
    **passed locally** (see Evidence section below).
12. `plans/178-status.md` advances `next_executable_plan = 179` —
    **this record**.

## Plan 180 debt acknowledged

The full M10 per-service Streaming byte round-trip over local
TCP, the per-destination runtime driver task, the transactional
reconcile listener / shutdown pass, and the broader Plan 178
§10 product matrix items that require a live Streaming peer
(registration reaching a fixture with byte-exact rewritten
USER, CAP/SASL AUTHENTICATE byte survival, JOIN/PRIVMSG/
NOTICE digest round-trip, tagged PRIVMSG byte survival,
paired PONG presented to the client over the wire) are owned
by Plan 180 reconcile work. Plan 178 ships:

- the runtime-neutral IRC module (line, tags, policy,
  client_filter, config, errors, limits) — every test there
  is a black-box confirmed-by-execution rule;
- the daemon-side IRC executor that owns sockets and
  Streaming lifetime; and
- the manager dispatch path + `[service_tunnels] irc-client`
  + 18 black-box tests that prove every behavior exercisable
  without the per-destination runtime driver loop.

Plan 180 will generalize the SAM per-destination driver loop to
service tunnels (Plan 174 §3.2 + Plan 175 §11 alignment) so the
Plan 178 §10 byte-round-trip matrix executes end-to-end without
re-plumbing the manager surface.

## Evidence (Plan 178)

Service-tunnel runtime-neutral IRC module:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
# 188 passed (1 suite, 0.00s)
# Includes 22 policy + 17 client_filter + 15 line + 11 tags + 5 errors + 5 config
# + 1 service-tunnels config test for the IrcClient options requirement
# (prior 112 retained).
```

Black-box IRC client product tests:

```text
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
# 18 passed (1 suite, ~67s)
```

Foundation + generic + HTTP + SOCKS5 product regressions:

```text
cargo test --locked -p i2pr-daemon --test service_tunnels_foundation -- --test-threads=1
# 7 passed (1 suite, 0.00s)
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
# 9 passed (1 suite, 0.53s)
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
# 15 passed (1 suite, 35.54s)
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
# 23 passed (1 suite, 0.88s)
cargo test --locked -p i2pr-service-tunnels --all-targets
# 188 passed (1 suite, 0.00s)
```

Full workspace:

```text
cargo test --locked --workspace --all-targets -- --test-threads=1
# 2031 passed, 1 ignored (75 suites, ~549s)
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
# runtime boundary checks passed (i2pr-service-tunnels::irc remains runtime-neutral)
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
# 10 passed (1 suite, 313.78s)
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_forward_naming -- --test-threads=1
# (covered by the full workspace run above: 2031 passed, 1 ignored, no failures)
```

I2CP retained (Plan 167-172 regressions):

```text
cargo test --locked -p i2pr-daemon --test i2cp_loopback -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_final_acceptance -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_adversarial_matrix -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_resource_matrix -- --test-threads=1
# (covered by the full workspace run above: 2031 passed, 1 ignored, no failures)
```

## Handoff

Execute Plan **179** next (IRC server). Do not begin
Plan 180 (composition reconcile) until Plan 179 has an explicit
passing status record. Do not implement the full client/server
round-trip without a fresh plan-of-record (Plan 180 reconcile).

```text
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering
milestone10_irc_client = passed-via-plan178
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 179
```
