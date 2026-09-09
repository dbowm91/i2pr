# Plan 179 — Milestone 10 IRC server profile and authenticated peer hostname

Status: **blocked until Plan 178 passes**.

## 1. Goal

Add the M10 IRC server tunnel profile on top of Plan 175's persistent generic server destination:

```text
remote I2P IRC client
  -> i2pr persistent IRC server Destination / Streaming accept
  -> bounded registration interceptor
  -> authenticated peer Destination -> safe IRC hostname projection
  -> loopback IRC server target
  -> raw bounded byte pump after registration
```

The server profile does not implement an IRC daemon. It makes an ordinary loopback IRC server usable behind an I2P service while ensuring the hostname injected into registration is derived from authenticated I2P peer identity, not attacker-supplied IRC text.

## 2. Reference basis

Use clean-room behavior from:

- RFC 1459 / RFC 2812 registration framing;
- current IRCv3 message-tag framing where tags appear before registration commands;
- exact-pinned Java I2P 2.13.0 commit `9134f808337b401e8e53c73734c81fab04280c9d`, especially `apps/i2ptunnel/java/src/net/i2p/i2ptunnel/I2PTunnelIRCServer.java` as a behavior reference only.

The Java reference offers `USER` and WEBIRC methods plus configurable cloaking. The M10 minimum profile is deliberately narrower: authenticated Destination-derived hostname projected into `USER`. WEBIRC and configurable secret cloaking are deferred.

## 3. Authenticated peer identity invariant

The only acceptable source for the projected remote hostname is the authenticated peer Destination attached to the accepted Streaming connection.

Never derive it from:

- IRC `USER` hostname/servername fields;
- remote IP/socket metadata;
- unverified destination text in application bytes;
- a NetDB lookup result not bound to the accepted Streaming peer;
- a caller-provided arbitrary hostname.

Required default projection:

```text
<52-char lower-case base32 destination hash>.b32.i2p
```

The projection is public metadata, deterministic across nick changes for the same Destination, and contains no private keys.

A future keyed cloak may be added only by a separate plan with explicit persistent-key semantics. Do not add an ephemeral random cloak that silently changes on restart.

## 4. Runtime-neutral registration interceptor

Add server-profile logic under `i2pr-service-tunnels::irc`, separate from daemon socket code.

Required typed states:

```text
AwaitRegistration
SawPassOrCap
SawNick
SawUser
ReadyForRawPump
Rejected
```

The implementation need not enforce a complete IRC registration state machine; it must safely scan a bounded prefix until `USER` (or explicitly supported `SERVER`) is found, rewrite that one registration field, then hand the remaining byte stream to the shared pump.

## 5. Registration bounds

Central limits must cover:

- maximum registration lines before USER/SERVER: initial M10 ceiling <= 10;
- per-line IRC/IRCv3 bounds from Plan 178;
- total retained registration bytes;
- per-line read deadline;
- total registration deadline;
- bytes received in the same read after the terminating USER line.

If USER/SERVER is not reached before a count/byte/deadline ceiling, reject and close/reset. Do not keep reading indefinitely.

Do not truncate overlong lines.

## 6. USER rewriting

For an accepted USER message, preserve the user/mode/unused/realname semantics needed by ordinary IRC servers, but replace the hostname field with the authenticated Destination projection.

Legacy example:

```text
USER alice attacker-host attacker-server :Alice
```

becomes conceptually:

```text
USER alice <peer-hash>.b32.i2p attacker-server :Alice
```

For RFC 2812 numeric-mode form, do not reinterpret the mode parameter as a hostname. Define and test the exact rewrite for both forms.

Rules:

- the output must remain within IRC core-line limit;
- if projection would make an already-near-limit line too large, reject rather than truncate;
- IRCv3 tags preceding USER remain attached if structurally valid;
- untrusted original hostname text must not appear in logs/errors.

## 7. Pre-registration command handling

Allow a bounded ordinary pre-registration sequence such as:

```text
PASS
CAP
AUTHENTICATE
NICK
USER
```

Other structurally valid lines may be retained/passed only through an explicit pre-registration disposition table. Unknown or suspicious traffic is rejected once it exceeds the bounded registration policy.

Add first-line cross-protocol rejection for obvious accidental misuse such as HTTP request lines and BitTorrent protocol markers. This is defense-in-depth only; do not build protocol detection beyond a small fixed list.

## 8. Local target connection

Reuse Plan 175 server-target policy:

- loopback TCP mandatory;
- Unix stream target where supported by generic server runtime;
- bounded connect deadline;
- no DNS/LAN/WAN target.

Order:

1. accept authenticated Streaming connection;
2. run bounded registration interception on I2P-side bytes;
3. obtain rewritten prefix;
4. connect local target under deadline (or stage target connection before reads if the shared manager's ownership model requires it, while preserving bounds);
5. write rewritten registration prefix to target exactly once;
6. preserve any same-read bytes after USER as first raw-pump bytes;
7. switch permanently to generic raw pump;
8. do not continue parsing IRC after handoff.

If local target connect fails, send at most one small bounded IRC-style failure to the remote peer where safe, then close/reset. Do not retain peer message contents.

## 9. No post-registration server filter

After registration rewrite, this profile is byte-transparent.

Do not add a second ongoing IRC command filter between the remote peer and local IRC daemon. The client-profile privacy filter belongs on the client side; the server profile's mandatory concern is authenticated hostname projection and bounded registration.

This matches the narrow M10 architecture and avoids maintaining two divergent IRC interpreters.

## 10. Product tests

Add a black-box suite such as:

```text
crates/i2pr-daemon/tests/service_tunnel_irc_server_product.rs
```

Canonical topology:

```text
i2pr IRC client tunnel A
 -> Destination/Streaming A
 -> i2pr IRC server tunnel B
 -> loopback scripted IRC daemon
```

The server side must observe authenticated peer Destination A from Streaming metadata.

Required cases:

1. PASS/CAP/NICK/USER registration reaches target;
2. target sees `<A-hash>.b32.i2p`, never the client-supplied hostname;
3. same remote Destination across reconnects maps to the same hostname;
4. different Destination maps to a different hostname;
5. nick changes do not change projected host identity;
6. IRCv3-tagged USER rewrites correctly;
7. USER near line ceiling either rewrites validly or rejects without truncation;
8. more than max registration lines rejects;
9. registration slowloris hits total deadline;
10. EOF before USER rejects cleanly;
11. obvious HTTP/BitTorrent first-line misuse rejects;
12. same-read bytes after USER are delivered exactly once after target handoff;
13. target refusal/timeout closes/reset without leaking;
14. after handoff arbitrary valid IRC bytes are transparent;
15. repeated connections/shutdown return all resource counters baseline.

## 11. Restart-stable server identity interaction

Because Plan 175 persists the server Destination, prove that restarting the IRC server service with the same identity path preserves:

- server public Destination;
- inbound service address;
- hostname projection algorithm for the same authenticated remote Destination.

Do not create a separate IRC-specific private identity storage format.

## 12. Privacy/security tests

Explicitly prove:

- spoofed USER hostname cannot override projected hostname;
- application bytes cannot select another authenticated peer Destination;
- raw peer Destination private material is never requested/exposed;
- snapshot/log fields contain only sanitized public service IDs/counts;
- registration line text is not logged;
- an attacker cannot force unlimited pre-registration line retention;
- post-handoff bytes are not accidentally reparsed or rewritten.

## 13. Non-goals

No:

- WEBIRC;
- secret cloaking keys;
- DCC;
- TLS termination;
- IRC daemon implementation;
- nick/account tracking database;
- post-registration IRC command filtering;
- remote/LAN local targets.

## 14. Documentation/support

Update at passing closure:

- `specs/protocols/11-service-tunnels.md` IRC server profile;
- architecture docs with authenticated peer metadata path;
- sample disabled IRC-server config;
- `specs/support.toml` experimental IRC-server row;
- `plans/179-status.md`.

Document precisely that the projected hostname is authenticated Destination public identity, not anonymity against the local IRC operator.

## 15. Validation floor

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_server_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
cargo deny check advisories bans sources
```

## 16. Acceptance criteria

Plan 179 passes only when:

1. server registration interception is bounded and runtime-neutral.
2. USER hostname derives only from authenticated Streaming peer Destination.
3. spoofed client hostname cannot influence the projected identity.
4. full lower-case `.b32.i2p` projection is deterministic and restart-stable.
5. IRCv3-tagged and legacy/RFC2812 USER forms are handled explicitly.
6. same-read post-USER bytes survive the raw-pump handoff exactly once.
7. registration count/byte/deadline ceilings and target failures clean up deterministically.
8. after handoff the server profile is byte-transparent.
9. no WEBIRC/cloak/DCC/TLS claim is introduced.
10. retained M10 client/generic/HTTP/SOCKS plus SAM/M9 regressions remain green.
11. full workspace floor and exact-head routine CI pass.
12. `plans/179-status.md` advances `next_executable_plan = 180`.

## 17. Stop conditions

Write a narrow corrective if:

- authenticated peer Destination is not available at the server Streaming accept seam;
- ordinary selected IRC infrastructure requires WEBIRC rather than USER hostname projection;
- preserving same-read post-registration bytes needs unbounded buffering;
- stable server identity semantics regress;
- a real Streaming/session defect appears.

## 18. Handoff

Expected transition:

```text
plan_179 = passed-m10-irc-server-profile-and-authenticated-peer-hostname
milestone10_irc_server = passed-via-plan179
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 180
```
