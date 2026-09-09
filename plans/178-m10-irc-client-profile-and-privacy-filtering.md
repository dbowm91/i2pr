# Plan 178 — Milestone 10 IRC client profile and privacy filtering

Status: **blocked until Plan 177 passes**.

## 1. Goal

Add the M10 IRC client tunnel profile on top of the generic fixed-target client-tunnel runtime:

```text
local IRC client
  -> loopback i2pr IRC client listener
  -> bounded IRC/IRCv3 line parser + privacy filter
  -> fixed configured I2P IRC destination
  -> existing Streaming path
```

The profile exists to remove common local-address/host leakage while preserving ordinary IRC registration, messaging, CAP/SASL negotiation, and channel use.

DCC transport is deliberately not implemented in M10.

## 2. Reference basis

Use clean-room behavior from:

- RFC 1459 / RFC 2812 message framing and registration behavior as historical IRC baseline;
- current IRCv3 Capability Negotiation and Message Tags specifications;
- exact-pinned Java I2P 2.13.0 commit `9134f808337b401e8e53c73734c81fab04280c9d` behavior references:
  - `apps/i2ptunnel/java/src/net/i2p/i2ptunnel/I2PTunnelIRCClient.java`;
  - `apps/i2ptunnel/java/src/net/i2p/i2ptunnel/irc/IRCFilter.java`;
  - inbound/outbound filter wrappers under the same package.

Do not copy Java source. Its DCC machinery and broad operator-command compatibility are outside the initial i2pr profile.

## 3. Runtime-neutral IRC parser

Add an IRC module under `i2pr-service-tunnels`, e.g.:

```text
src/irc/
  mod.rs
  line.rs
  tags.rs
  client_filter.rs
  policy.rs
```

No Tokio/socket ownership.

The parser must operate on bytes/validated text conservatively and preserve exact pass-through bytes for accepted messages except where a named rewrite applies.

### 3.1 Length bounds

Enforce modern IRC/IRCv3 framing explicitly:

- non-tag IRC message portion: maximum 512 bytes including terminator semantics;
- message-tag envelope: maximum 8191 bytes including leading `@` and separating space;
- client-originated tag data: maximum 4094 bytes;
- line-retention ceiling must be the combined documented maximum plus terminator allowance, not unbounded `read_line` growth.

Never truncate an overlong line into a syntactically different valid command. Reject/drop and account it.

Tests must cover exact maximum and `+1` failure.

## 4. Command classification

Implement a typed command classifier and explicit client-profile allowlist.

The initial allowlist must cover ordinary clients, at least:

```text
PASS
CAP
AUTHENTICATE
NICK
USER
PING
PONG
JOIN
PART
QUIT
PRIVMSG
NOTICE
MODE
TOPIC
AWAY
NAMES
LIST
WHO
WHOIS
WHOWAS
ISON
INVITE
KICK
USERHOST
```

Numeric server replies are accepted inbound.

Common server-originated commands required for registration/channel operation should be explicitly accepted inbound (e.g. `PING`, `MODE`, `JOIN`, `NICK`, `QUIT`, `PART`, `KICK`, `TOPIC`, `CAP`, `AUTHENTICATE`, `ACCOUNT`, `CHGHOST`, `ERROR`).

Unknown/unclassified commands do not automatically pass. The disposition must be a typed `Allow`, `Rewrite`, or `Drop(reason)` decision and be covered by tests.

Do not inherit Java's entire IRC-operator command list just for parity.

## 5. IRCv3 tags

Support structural message-tag framing so modern clients can negotiate/use IRCv3 without the privacy filter accidentally treating `@tag ...` as the command.

Rules:

- tag names/values are treated as opaque per IRCv3 framing;
- enforce client tag-size and combined tag-size bounds;
- never substitute replacement bytes into invalid UTF-8 tag values in a way that changes identity/collision semantics;
- accepted tags remain attached to accepted/rewritten IRC commands unless the specific rewrite documents otherwise;
- tag presence must not bypass command filtering;
- line-size enforcement treats tag and core-message limits separately.

No attempt to implement every IRCv3 extension belongs here.

## 6. Client-to-network privacy rewrites

### 6.1 USER

For legacy form:

```text
USER <username> <hostname> <servername> :<realname>
```

rewrite hostname/servername fields to stable non-identifying placeholders, following the reference intent:

```text
hostname   -> hostname
servername -> localhost
```

For RFC 2812 form (`USER <user> <mode> <unused> :realname`), preserve numeric mode and `*`/unused semantics while ensuring no local hostname/address is introduced.

Preserve username and realname bytes within normal IRC bounds; do not log them in routine diagnostics.

### 6.2 PING/PONG

Ordinary `PING <nonce>` may pass unchanged.

If a client PING includes an additional server/location argument that could expose the local proxy address, strip/rewrite the location while retaining enough per-connection bounded state to restore the corresponding expected PONG shape for the client.

Bound retained expected-PONG state to one outstanding rewrite token. A new rewrite replaces/clears the old one deterministically; no unbounded nonce map.

### 6.3 QUIT/PART text

If a configured privacy profile suppresses client-generated version/application strings in QUIT/PART reasons, use one documented stable replacement. Do not silently mutate them without a named policy option.

The minimum M10 profile may pass ordinary reasons unchanged if they contain no proxy-generated metadata; document the choice.

## 7. CTCP/DCC policy

For `PRIVMSG`/`NOTICE` containing CTCP delimiters (`0x01`):

- allow `ACTION`;
- reject malformed/multiple-delimiter ambiguity rather than partially parsing it;
- block address-bearing `DCC` by default;
- block other CTCP requests by default unless a later explicitly bounded allowlist is added;
- do not start DCC helper tunnels in M10.

This is intentionally stricter than the full Java I2PTunnel IRC feature set.

Inbound network-to-client CTCP follows the same conservative policy: ACTION may pass; DCC and other CTCP are dropped unless explicitly supported.

Every drop should increment bounded aggregate counters only; do not retain message text.

## 8. Daemon composition

Activate `irc-client` in the service manager.

It is a fixed-target client tunnel:

1. bind loopback listener under shared limits;
2. accept local IRC client;
3. establish Streaming to configured I2P IRC destination/port;
4. run line-aware outbound and inbound filtering rather than the raw byte pump;
5. each side maintains one bounded partial-line buffer;
6. after EOF/cancel/remote close, release filter buffers and Streaming/local sockets.

Do not select target based on IRC command contents.

The daemon-specific line pump should consume runtime-neutral filter decisions from `i2pr-service-tunnels`; it must not duplicate command policy inline.

## 9. Partial reads and backpressure

The filter must handle:

- multiple IRC lines in one read;
- one line fragmented across many reads;
- IRCv3 tagged line fragmentation;
- accepted line followed by partial next line;
- client half-close after a complete line;
- stalled reader/writer under bounded buffers.

Do not wait for an entire unbounded TCP stream before filtering.

## 10. Product tests

Add a black-box suite such as:

```text
crates/i2pr-daemon/tests/service_tunnel_irc_client_product.rs
```

Canonical topology:

```text
local scripted IRC client
 -> i2pr IRC client tunnel
 -> I2P Streaming
 -> i2pr generic server tunnel
 -> loopback scripted IRC server fixture
```

After service startup, application data moves only through OS sockets.

Required cases:

1. PASS/CAP/NICK/USER registration reaches fixture;
2. USER local-host values are rewritten and originals never reach fixture;
3. CAP/SASL AUTHENTICATE sequence survives;
4. JOIN/PRIVMSG/NOTICE round trip;
5. tagged IRCv3 PRIVMSG survives within bounds;
6. `PING nonce local-address` is rewritten and paired PONG presented safely;
7. CTCP ACTION survives;
8. CTCP VERSION/DCC is dropped;
9. overlong core line dropped without truncation;
10. overlong client tag data dropped;
11. fragmented and coalesced lines are exact;
12. sibling clients keep independent expected-PONG/filter state;
13. stalled local/network reader remains bounded;
14. disconnect/shutdown returns buffers/tasks/connection counts baseline.

## 11. Negative/privacy tests

Include explicit probes for:

- local IPv4/IPv6 text in USER fields;
- hostname containing control bytes;
- command preceded by IRCv3 tags to ensure no allowlist bypass;
- duplicate/invalid tag framing;
- CTCP with unbalanced delimiters;
- DCC SEND/CHAT address forms;
- giant realname/QUIT/PART payload;
- unknown command;
- malformed prefix-only/empty command lines;
- slowloris byte-at-a-time line under deadline.

## 12. Non-goals

No:

- DCC tunnel support;
- WEBIRC;
- TLS termination;
- SASL credential management (the tunnel transparently permits normal client/server SASL IRC messages);
- IRC bouncer functionality;
- channel/user state database;
- arbitrary protocol-aware server-side filter in this plan.

## 13. Documentation/support

Update at passing closure:

- `specs/protocols/11-service-tunnels.md` IRC client profile and command matrix;
- architecture docs with line-filter boundary;
- example disabled IRC-client service config;
- `specs/support.toml` experimental IRC-client row;
- `plans/178-status.md`.

Record IRCv3 tag-size semantics explicitly in the protocol dossier.

## 14. Validation floor

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --test service_tunnel_irc_client_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_socks5_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
cargo deny check advisories bans sources
```

## 15. Acceptance criteria

Plan 178 passes only when:

1. IRC/IRCv3 framing/filter logic is runtime-neutral and explicitly bounded.
2. Tag/core-message size limits match the documented profile; overlong lines are never truncated into valid commands.
3. Command allowlists are typed/tested and tags cannot bypass them.
4. USER rewrite removes client local hostname/servername leakage.
5. location-bearing PING/PONG handling is bounded per connection.
6. ACTION works while DCC/unsupported CTCP are blocked.
7. ordinary registration/CAP/SASL/channel/messaging behavior passes through a real local product path.
8. fragmented/coalesced/sibling/backpressure paths stay bounded and isolated.
9. no DCC/WEBIRC/TLS/bouncer claim is introduced.
10. retained M10 generic/HTTP/SOCKS and SAM/M9 regressions remain green.
11. full workspace floor and exact-head routine CI pass.
12. `plans/178-status.md` advances `next_executable_plan = 179`.

## 16. Stop conditions

Write a narrow corrective if:

- a normal selected IRC client requires an omitted command that cannot safely be classified;
- correct filtering requires unbounded per-client protocol history;
- message tags expose a parser ambiguity/bypass not covered by the current line model;
- an underlying Streaming correctness issue appears;
- DCC becomes necessary to satisfy the M10 exit criteria (it is not currently required).

## 17. Handoff

Expected transition:

```text
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering
milestone10_irc_client = passed-via-plan178
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 179
```
