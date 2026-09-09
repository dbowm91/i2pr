# Plan 173 status — Milestone 10 service tunnels, HTTP, SOCKS5, and IRC roadmap

Status: **`registered-m10-service-tunnels-http-socks5-irc-roadmap`**.

Registered: **2026-09-09**.

Plan of record:
[`plans/173-m10-service-tunnels-http-socks5-irc-roadmap.md`](173-m10-service-tunnels-http-socks5-irc-roadmap.md).

## Current authority

Milestone 9 remains closed via Plan 172. Plan 173 is the planning authority for Milestone 10; Plans 174–181 are implementation/acceptance handoff records and must execute in order unless a concrete blocker triggers a narrower corrective.

```text
plan_170_external_wire_data_plane = retained-passed
plan_172 = passed-m9-i2cp-independent-leaseset2-lifecycle-corrective
milestone9_final_acceptance = closed-via-plan172

plan_173 = registered-m10-service-tunnels-http-socks5-irc-roadmap
milestone10_planning_authority = plan173
milestone10_foundation = not-yet-passed
milestone10_generic_tunnels = not-yet-passed
milestone10_http_proxy = not-yet-passed
milestone10_socks5 = not-yet-passed
milestone10_irc_client = not-yet-passed
milestone10_irc_server = not-yet-passed
milestone10_local_product = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 174
next_product_layer = milestone10-service-tunnels
```

## Registered execution sequence

```text
173 roadmap / architecture authority
  -> 174 service-tunnel foundation + shared Streaming runtime
  -> 175 generic client/server tunnels + persistent server identities
  -> 176 HTTP/1.1 .i2p proxy + CONNECT
  -> 177 SOCKS5 no-auth .i2p CONNECT
  -> 178 IRC client privacy/filter profile
  -> 179 IRC server authenticated peer-hostname profile
  -> 180 full service composition / reconcile / hardening
  -> 181 independent application + independent I2P service acceptance / final closure
```

Plan records:

- [`174-m10-service-tunnel-foundation-and-shared-stream-runtime.md`](174-m10-service-tunnel-foundation-and-shared-stream-runtime.md)
- [`175-m10-generic-client-server-service-tunnels.md`](175-m10-generic-client-server-service-tunnels.md)
- [`176-m10-http-i2p-proxy-and-connect.md`](176-m10-http-i2p-proxy-and-connect.md)
- [`177-m10-socks5-i2p-connect-proxy.md`](177-m10-socks5-i2p-connect-proxy.md)
- [`178-m10-irc-client-profile-and-privacy-filtering.md`](178-m10-irc-client-profile-and-privacy-filtering.md)
- [`179-m10-irc-server-profile-and-authenticated-peer-hostname.md`](179-m10-irc-server-profile-and-authenticated-peer-hostname.md)
- [`180-m10-service-tunnel-composition-reconcile-and-hardening.md`](180-m10-service-tunnel-composition-reconcile-and-hardening.md)
- [`181-m10-independent-application-and-service-interop-final-closure.md`](181-m10-independent-application-and-service-interop-final-closure.md)

## Architecture locks

Retain these throughout M10:

1. `i2pr-service-tunnels` is runtime-neutral protocol/config/policy code only.
2. `i2pr-daemon` remains the sole new M10 socket/Tokio/task/lifecycle owner.
3. M10 reuses `i2pr-client` destination, LeaseSet2, ECIES, routing, and Streaming; there is no service-specific router stack.
4. Extract/reuse the proven bounded SAM raw socket <-> Streaming pump rather than duplicating it.
5. Generic server services use versioned, atomic, secret-safe persistent router-owned Destination identities.
6. Client-facing listeners are loopback-only; generic server TCP targets are loopback-only (Unix stream targets may be supported on Unix).
7. HTTP/SOCKS target resolution never falls back to clearnet DNS/IP routing.
8. HTTP is HTTP/1.1 `.i2p` proxy + CONNECT only; no clearnet outproxy/TLS interception/HTTP2/3 termination.
9. SOCKS is RFC 1928 v5 no-auth DOMAINNAME `.i2p` CONNECT only; no BIND/UDP/SOCKS4/auth/Tor extensions.
10. IRC client applies bounded privacy filtering/rewriting; DCC/WEBIRC are deferred.
11. IRC server hostname projection comes only from authenticated Streaming peer Destination metadata.
12. Service generation reconcile is transactional: stage complete candidate -> atomic commit -> bounded old-generation drain.
13. Self-composed application tests are local-product evidence, not mixed-router interoperability.
14. Final Plan 181 must independently classify local product, independent ordinary application clients, and independent I2P service/router evidence.

## Final interoperability disposition

The MVP roadmap requires ordinary HTTP traffic to an eepsite and IRC use against selected I2P IRC infrastructure. Therefore Plan 181 must attempt a controlled exact-pinned independent I2P service/router lane rather than substituting an i2pr-to-i2pr product.

The retained Milestone 6 mixed-router destination/Streaming debt may block this final gate. If it does:

```text
retain passed M10 local/application-client evidence
classify blocker = m6-mixed-router-streaming-blocker
stop Plan 181
register one narrow M6/M10 interop corrective
resume Plan 181 only after that corrective passes
```

Do not rebuild the historical rootless/VM harness stack merely to satisfy M10, and do not weaken the remote-service exit criterion.

## Research basis

The roadmap/plans are based on:

- the repository MVP roadmap and existing M6/M7/M9 destination/Streaming products;
- HTTP/1.1 RFC 9110/9112 proxy/CONNECT semantics;
- SOCKS5 RFC 1928;
- current IRCv3 message-tag framing/limits;
- clean-room behavior observation from exact-pinned Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`) I2PTunnel generic/HTTP/SOCKS/IRC implementations;
- exact-pinned independent `jaraco/irc` commit `90e10e690da2c7bf60de21be4e36d24c9ffd7474` as the planned counted IRC application-client reference in Plan 181.

Reference implementations are observational only; do not copy their code.

## Handoff

Execute Plan **174** next. Do not begin Plan 175 until Plan 174 has an explicit passing status record, and do not implement later protocol profiles early merely because their plan files are registered.
