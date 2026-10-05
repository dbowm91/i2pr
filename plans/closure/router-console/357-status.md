# Plan 357 closure — loopback browser security and optional authentication

Status: **passed-loopback-browser-security-and-optional-authentication**.

Classification: **invariant + infrastructure**. This closure hardens the browser
boundary of an experimental, loopback-only, disabled-by-default console. It
establishes no router capability and no anonymity or privacy property. A
loopback-only listener is not by itself a browser boundary, and none of the
controls below should be read as making the console safe against a hostile
network, a hostile local process, or browser-executed code.

## Commits

- `73506b38` — implementation (the `i2pr-console/src/security/` tree, the
  `[console]` config surface, and `scripts/check-console-browser-security.sh`).

The closure/registry/roadmap status transition is in the commit that adds this
record.

## Browser threat model

The console's client is a **browser executing attacker-influenced code on the
operator's machine**. That is a threat class the other local adapters do not
have, and it is why "loopback only" is not the end of the analysis.

| Threat | Control |
|---|---|
| DNS rebinding (a hostile page resolving to `127.0.0.1`) | Exact `Host` allow-list derived from the bound address and resolved port |
| Cross-site state change (CSRF from a hostile page) | `Origin` validation for unsafe methods; `Referer` fallback only when `Origin` is absent |
| Cross-site authenticated request | Per-session CSRF token |
| XSS / stolen session | `HttpOnly` cookie, strict CSP with no `unsafe-inline`, escaped interpolation |
| Clickjacking | `frame-ancestors 'none'` |
| Session fixation / replay | Opaque server-side sessions, revocation, `clear()` on shutdown |
| Credential guessing | Argon2id with bounded concurrency plus a console-wide login throttle |
| Credential disclosure in logs/config | Plaintext converted at parse time; redacting `Debug` on every secret-bearing config type |
| Proxy-header spoofing | No forwarded header is consulted; no trusted-proxy mode exists |
| Resource exhaustion | Bounded connections, session ceiling, bounded concurrency on verification |

## Exact security-header policy

Applied centrally in `security/headers.rs` through one `respond()` function, so
no route — including every rejection and every redirect — can ship without them.

```
Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self';
  img-src 'self'; font-src 'self'; connect-src 'self'; form-action 'self';
  frame-ancestors 'none'; base-uri 'none'; object-src 'none'
Referrer-Policy: no-referrer
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Resource-Policy: same-origin
Permissions-Policy: accelerometer=(), autoplay=(), camera=(), …
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Cache-Control: no-store
```

There is **no** `Server` header, deliberately. EggServe's default was empty
rather than absent and it rejects an empty value, so the header is removed
entirely — consistent with advertising nothing beyond the tested subset.

There is **no permissive CORS header on any response** —
`no_response_ever_carries_a_cors_header` asserts this across every status the
guard can produce.

**Defect found and fixed:** `axum::response::Redirect` bypassed the centralized
policy. Every redirect is now constructed through the module-local `redirect()`
helper so the same headers apply.

## Host/Origin acceptance matrix

| Request | Result |
|---|---|
| `Host: localhost:<real-port>` | accepted |
| `Host: 127.0.0.1:<real-port>` | accepted |
| `Host: [::1]:<real-port>` | accepted |
| `Host: localhost` (no port) | **403** — the allow-set includes the resolved port |
| `Host: evil.test` | **403** |
| no `Host` | **403** — `requests_without_an_authority_are_refused` |
| ASCII case variants of an accepted authority | accepted (case-insensitive only) |
| unsafe method, matching `Origin` | accepted |
| unsafe method, foreign `Origin` | **403 before the handler** |
| unsafe method, no `Origin` and no `Referer` | **403 by default** |
| unsafe method, `Referer` fallback only | matched **including the terminating `/`**, refusing `localhost:7070.evil.test` |

Because the policy depends on the real port, the `AxumRouter` is constructed
**after** `bind`, not before. With `port = 0` the port does not exist until then.
Constructing first would either forbid the real port or allow whatever was
configured rather than whatever was bound.

## Auth-disabled / auth-enabled behaviour

| Mode | Behaviour |
|---|---|
| `auth = false` (default) | No login form is served, no session cookie is issued, no login throttle is engaged. `an_unauthenticated_console_offers_no_login_form`; the API route is served directly. |
| `auth = true` | Unauthenticated request → redirect to `/login`. `authenticated_console_redirects_to_login_without_a_session`. |
| `auth = true` without credential material | **configuration error** — there is no dormant default password |
| credential material with `auth = false` | **configuration error** — a password that is never used is a misconfiguration, not a harmless extra |
| wrong password | refused; the failure is counted |
| correct password | both cookies issued with the required attributes |

Authority validation applies **in both modes** —
`authority_validation_applies_in_authenticated_mode_too`. Authentication is not a
substitute for the `Host` check.

## KDF dependency / parameter / resource evidence

Argon2id at **16 MiB / 2 passes / 1 parallelism**, chosen deliberately **not** to
be the desktop default: a small SBC must not allocate hundreds of megabytes to
log in.

- At most **2 concurrent verifications**; saturation is **refused**, not queued,
  because a queue is an unbounded memory amplifier.
- A supplied PHC hash is range-checked against the console's own ceilings at
  **config parse time**, so an over-large pre-hash cannot be loaded later.
- `argon2` is a dependency of `i2pr-console` only. The daemon does not depend on
  it.

## Session ceilings / expiry / eviction

| Property | Value |
|---|---|
| Identifier | opaque 32-byte hex, server-side |
| Cookie | `HttpOnly; SameSite=Strict; Path=/`, no `Domain` |
| Idle timeout | 15 min (ceiling 60 min) |
| Absolute timeout | 8 h (ceiling 24 h) |
| Session ceiling | 32 (absolute 256) |
| Eviction | deterministic |
| Revocation | explicit |
| Shutdown | `clear()` |
| Restart | sessions are not persisted, so a restart invalidates them cleanly |

`Secure` is **deliberately omitted** and the localhost exception is documented
rather than assumed: the listener is plain HTTP on loopback, and a `Secure`
cookie over `http://localhost` would silently never be sent. This console does
not terminate TLS.

## CSRF negative and positive evidence

| Case | Result |
|---|---|
| valid session + valid CSRF token | mutation permitted |
| valid session, no CSRF token | refused |
| valid session, wrong CSRF token | refused |
| forged session cookie | refused |
| logout without a CSRF token | refused |
| logout with a wrong CSRF token | refused **and the session survives** |

**One documented exemption:** `/logout` is exempt from the middleware CSRF check
because its token travels as a form field, which middleware cannot read. The
handler verifies it with the same `verify_csrf` before performing the state
change, so the exemption removes a duplication, not a control.

## Secret-redaction evidence

- `ConsoleSecret` is **not `Clone`**, has a redacting `Debug`/`Display`, and
  zeroizes on drop.
- `RawConsoleConfig` and `ConsolePasswordHash` both have hand-written redacting
  `Debug`; the plaintext password is converted to a PHC hash during
  normalization and does not survive into `Config`.
- Browser session tokens are **not** I2PControl tokens. The console holds no
  I2PControl credential of any kind.
- No response reflects a password, token, private key, or raw config.

## Restart and shutdown evidence

`logout_revokes_the_session_so_it_cannot_be_replayed`;
`listener_shuts_down_and_stops_accepting`; sessions are in-memory and are
`clear()`ed on shutdown, so a restart leaves no valid session behind.

**Defect found and fixed:** `LoginThrottle` silently dropped its `max_failures`
bound, so the throttle could never engage. It now refuses and is covered by
`login_throttle_blocks_repeated_failures`.

## Verification

All evidence below is from the local worktree on implementation head
`73506b38`. **No hosted CI result is claimed by this record.** The workspace
floor is recorded in [`356-status.md`](356-status.md) and was green at this head
(4 251 passed, 0 failed, 35 ignored, 152 suites).

| Command | Result |
|---|---|
| `cargo test -p i2pr-console --all-targets` | PASS — 118 unit + **23 browser-security** + 9 route |
| `cargo test --locked -p i2pr-daemon --lib` | PASS — 574, including 11 console-config tests |
| `cargo test --locked -p i2pr-daemon --test console_loopback -- --test-threads=1` | PASS — 6, including `authenticated_console_serves_a_live_authority_policy` over a real socket |
| `bash scripts/check-console-browser-security.sh` | PASS |
| `bash scripts/check-console-boundaries.sh` | PASS |
| `bash scripts/check-dependency-direction.sh` | PASS |
| `bash scripts/check-runtime-boundaries.sh` | PASS |

The seven bash-4+ floor checkers listed in `356-status.md` were not run on this
bash-3.2 host and are not claimed as passing.

## Security-relevant decisions

1. **`Secure` omitted, exception documented.** See above.
2. **Console-wide login throttle, deliberately.** The substrate exposes no
   unforgeable per-client identity and no proxy header is trusted, so there is no
   sound key to throttle on; deriving one from `Host` would trust the attacker.
   The failure mode is therefore a bounded delay that expires with the window,
   **not** a lockout. Recorded rather than hidden.
3. **Low Argon2id parameters, deliberately.** Bounded cost is a resource-safety
   requirement for the supported SBC class.
4. **Runtime ceilings bind only an enabled console**, so a disabled console can
   never shadow another subsystem's budget error.
5. **No `Server` header.**
6. **No trusted-proxy mode.** Adding one would require forwarding-header trust
   that a loopback console has no honest basis for.

## Known limitations

- The console terminates **no TLS**. A loopback-only plain-HTTP listener is the
  posture; any future remote exposure needs a separate design.
- The login throttle is global rather than per-client, for the reason above.
- The console is experimental, disabled by default, and non-advertised, and has
  no product-reachable path while `i2pr run` remains broken.

## Unresolved findings

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low:** the global throttle is a known, deliberate trade; it bounds damage
  rather than per-client-attributing it.

## Unblock audit for Plan 358

Plan 358 was registered `blocked on 356 and 357` and required "the server and
browser-security boundaries". Both exist and are exercised over a real loopback
socket. The block is released; Plan 358 is closed in [`358-status.md`](358-status.md).

## Roadmap disposition

Plan 357 is **closed** as invariants plus infrastructure.