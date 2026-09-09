# Plan 176 — Milestone 10 HTTP `.i2p` proxy and CONNECT

Status: **blocked until Plan 175 passes**.

## 1. Goal

Add an experimental loopback HTTP/1.1 proxy for I2P destinations on top of the generic client/service Streaming runtime.

Mandatory product:

```text
browser/curl HTTP proxy request
 -> bounded HTTP proxy parser
 -> .i2p-only destination resolution
 -> privacy/hop-by-hop rewrite
 -> existing I2P Streaming client path
 -> generic server tunnel / remote I2P HTTP service
```

and:

```text
CONNECT host.i2p:port
 -> validate .i2p target + allowed port
 -> establish I2P Streaming
 -> return successful 2xx CONNECT response
 -> blind bounded byte pump
```

No clearnet outproxy is implemented.

## 2. Reference profile

Use clean-room behavior from:

- RFC 9110 HTTP Semantics, especially proxy routing, `Connection`, and CONNECT;
- RFC 9112 HTTP/1.1 message framing;
- current official I2P I2PTunnel HTTP documentation;
- exact-pinned Java I2P 2.13.0 commit `9134f808337b401e8e53c73734c81fab04280c9d`, behavior reference path `apps/i2ptunnel/java/src/net/i2p/i2ptunnel/I2PTunnelHTTPClient.java`.

Do not copy Java implementation code.

## 3. Runtime-neutral HTTP module

Add under `i2pr-service-tunnels`, e.g.:

```text
src/http/
  mod.rs
  parser.rs
  rewrite.rs
  response.rs
```

No Tokio/socket ownership.

### 3.1 Hard limits

Central constants must bound:

- request-line bytes;
- total header bytes;
- header count;
- individual field-name/value bytes;
- CONNECT authority bytes;
- parser retained bytes before a complete header section;
- generated error-response bytes.

Choose conservative values compatible with ordinary browsers/curl. Tests must hit exact boundaries and `+1` rejection.

The parser must reject:

- NUL/control characters forbidden by HTTP field grammar;
- obs-fold unless explicitly normalized per current HTTP rules (prefer reject);
- malformed CRLF framing;
- duplicate/contradictory Host authority;
- request smuggling ambiguities such as conflicting `Content-Length` values and `Transfer-Encoding` + `Content-Length` where forwarding would be ambiguous;
- absolute URI schemes other than `http` for ordinary proxy requests;
- malformed/empty CONNECT port;
- userinfo in proxy target URI;
- overlong/incomplete headers under deadline.

Do not build an HTTP body buffer. After validated headers are forwarded, request/response body bytes stream through bounded socket/Streaming backpressure.

## 4. Request-target forms

Support:

### 4.1 Ordinary HTTP proxy request

Canonical expected form:

```text
METHOD http://host.i2p[:port]/path?query HTTP/1.1
Host: host.i2p[:port]
```

Resolve the URI authority through M10 destination resolution.

Forward to the remote service as origin-form:

```text
METHOD /path?query HTTP/1.1
Host: normalized-host[:port]
```

The proxy may also accept origin-form requests only when Host is present and resolves to `.i2p`; absolute-form is the mandatory independent-client path.

### 4.2 CONNECT

Accept authority-form only:

```text
CONNECT host.i2p:port HTTP/1.1
```

- explicit nonzero port required;
- host must resolve through the `.i2p` resolver;
- restrict ports through a bounded policy. Initial default should allow ordinary HTTPS port 443 plus configurable explicit I2P destination ports; do not permit a blanket arbitrary-port relay without policy.
- no TLS termination, certificate inspection, SNI parsing, or HTTP parsing after tunnel mode starts.

A successful reply must not include Content-Length or Transfer-Encoding. After the header terminator, all bytes are opaque to the proxy.

## 5. No outproxy / local-address rejection

Hard fail before any network action for:

- clearnet DNS names;
- IPv4/IPv6 literals;
- `localhost`, `.localhost`, router console addresses;
- malformed suffix confusion (`foo.i2p.example`);
- unsupported schemes (`ftp`, `ws`, etc.) in the M10 profile.

Use deterministic bounded HTTP errors, e.g.:

- 400 malformed request;
- 403 policy/non-I2P/local target rejection;
- 502 destination/LeaseSet/Streaming connection failure;
- 504 bounded connect timeout where distinguishable.

Do not echo untrusted full URLs/headers into logs or generated diagnostic bodies.

## 6. Hop-by-hop and privacy rewrite

Before forwarding an ordinary HTTP request:

1. parse every `Connection` field value as a bounded list of case-insensitive field names;
2. remove the fields named by `Connection`;
3. remove/replace `Connection` itself;
4. remove known proxy/hop-by-hop fields including `Proxy-Connection`, `Keep-Alive`, `TE`, `Trailer`, `Upgrade` as applicable to the selected forwarding profile;
5. handle Transfer-Encoding/message framing according to RFC 9112; do not dechunk/rechunk unless explicitly implemented and tested;
6. normalize Host from the validated destination authority;
7. force `Connection: close` for the first M10 profile to avoid browser/proxy request multiplexing complexity.

Privacy policy must be explicit and test-vector driven. At minimum remove or normalize fields that directly reveal local proxy/network metadata. The initial profile should strip `Via`, `Forwarded`, `X-Forwarded-*`, `Proxy-Authorization`, and other local-proxy credentials before I2P forwarding. `Referer`, `From`, and User-Agent handling must be an explicit documented policy, not accidental pass-through. Prefer a conservative stable I2P User-Agent or removal where browser compatibility permits.

Do not claim perfect browser fingerprint protection; document application-layer leakage as a known limitation.

## 7. Host/destination resolution

Mandatory:

- `.b32.i2p` direct hash;
- bounded configured alias -> destination reference.

Unknown human-readable `.i2p` names return explicit lookup failure. Do not query clearnet DNS.

If existing NetDB destination lookup can resolve a known destination hash, use it through the shared client seam. HTTP code never imports NetDB internals.

## 8. Daemon composition

Add the `http-client` service kind to the Plan 175 service manager.

Listener:

- loopback only;
- disabled by default;
- conventional port 4444 may be a documented example/default only if no collision exists; tests use ephemeral ports;
- per-listener and aggregate connection ceilings;
- header read deadline / idle deadline;
- one local TCP connection owns one parser/tunnel lifecycle under the initial close-after-request profile.

Ordinary HTTP requests forward validated/rewritten initial header bytes, then pump the remaining request body and response bytes through the established Streaming connection.

CONNECT hands the socket to the generic raw pump after successful Streaming establishment and 2xx response.

## 9. Standard server compatibility

The HTTP client proxy must work with Plan 175's generic server tunnel terminating at an ordinary loopback HTTP server. Do not require an HTTP-aware server tunnel or M10-specific metadata on the remote side.

Canonical product fixture:

```text
curl/browser-like client
 -> i2pr HTTP proxy
 -> I2P Streaming
 -> i2pr generic server tunnel
 -> loopback HTTP fixture
```

## 10. Tests

### Runtime-neutral parser/rewrite

Cover at least:

- GET absolute-form -> correct origin-form rewrite;
- POST with streamed body;
- HEAD;
- CONNECT;
- Host mismatch rejected;
- HTTP vs unsupported schemes;
- malformed URI/authority/port;
- clearnet/local/IP rejected;
- header/count/line exact ceilings;
- CRLF injection/control bytes rejected;
- conflicting Content-Length;
- Transfer-Encoding ambiguity;
- Connection-nominated arbitrary field removal;
- Proxy-Connection/Keep-Alive/TE/Trailer/Upgrade removal policy;
- privacy-header vectors;
- no untrusted value copied into error/log diagnostics.

### Black-box product

Add real TCP tests using a tiny local HTTP fixture after service startup:

1. curl-like GET returns byte-exact body/digest;
2. POST body digest matches target;
3. larger response exercises Streaming segmentation/backpressure;
4. CONNECT then opaque bidirectional bytes;
5. CONNECT same-read bytes after header are preserved as first tunnel bytes;
6. unknown `.i2p` returns bounded 502 without target connection;
7. clearnet target returns 403 and performs no I2P connect;
8. slow/incomplete headers time out and cleanup;
9. stalled HTTP target does not exceed buffers;
10. sibling HTTP connections isolated;
11. shutdown returns counters/tasks baseline.

If curl is available in routine CI, it may be used as a smoke test; Plan 181 owns counted independent-client evidence.

## 11. Explicit unsupported profile

Document and test rejection/non-support for:

- clearnet HTTP outproxy;
- proxy auth;
- address helpers/jump-service UI;
- x-i2p-gzip;
- HTTP/2 proxy protocol;
- HTTP/3/QUIC;
- WebSocket-specific upgrade semantics beyond ordinary rejected Upgrade;
- TLS interception;
- persistent browser-to-proxy pipelining/keepalive optimization.

## 12. Security/resource negative tests

Include slowloris headers, repeated oversized fields, many tiny headers, connection saturation, invalid UTF-8/opaque field bytes policy, response-target disconnect, client disconnect while target stalls, and repeated malformed CONNECT attempts. No test may require public-network traffic.

## 13. Documentation/support

Update:

- `specs/protocols/11-service-tunnels.md` HTTP compatibility table;
- architecture doc with parser/runtime boundary;
- example config;
- support ledger row as `experimental`, loopback-only, `.i2p`-only;
- `plans/176-status.md` with exact focused/full evidence.

## 14. Validation floor

At minimum:

```text
cargo test --locked -p i2pr-service-tunnels --all-targets
cargo test --locked -p i2pr-daemon --test service_tunnel_http_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnel_generic_product -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
cargo deny check advisories bans sources
```

## 15. Acceptance criteria

Plan 176 passes only when:

1. parser/rewrite code remains runtime-neutral;
2. request/header/body handling is explicitly bounded and smuggling ambiguities fail closed;
3. `.i2p`/Base32/static-alias targets work and clearnet/local/IP targets are blocked;
4. hop-by-hop Connection semantics are implemented, not a fixed incomplete header list alone;
5. privacy rewrite policy is explicit and vector-tested;
6. ordinary HTTP proxying works through a generic server tunnel;
7. CONNECT establishes Streaming first, returns correct 2xx, then preserves opaque bytes bidirectionally;
8. same-read CONNECT payload bytes are not lost;
9. slow/oversize/stalled/sibling/shutdown resource tests pass;
10. no clearnet outproxy/TLS interception/HTTP2/3 claim is introduced;
11. retained generic/SAM/M9 regressions and full workspace floor pass;
12. exact-head routine CI is green;
13. `plans/176-status.md` advances `next_executable_plan = 177`.

## 16. Stop conditions

Write a narrow corrective if:

- correct HTTP framing would require buffering unbounded request bodies;
- the proxy can be tricked into connecting to clearnet/local IP targets;
- a CONNECT test only passes by parsing tunnel bytes after 2xx;
- a shared Streaming defect appears;
- an HTTP framework dependency is proposed whose unsafe/parser footprint violates the dependency policy without review.

## 17. Handoff

Expected transition:

```text
plan_176 = passed-m10-http-i2p-proxy-and-connect
milestone10_http_proxy = passed-via-plan176
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 177
```