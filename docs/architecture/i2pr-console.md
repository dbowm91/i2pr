# `i2pr-console` — Deep Dive

Crate: `crates/i2pr-console` · module root `i2pr_console` ·
`#![forbid(unsafe_code)]` · **no workspace dependencies**

The browser-facing router console. It owns the HTTP application — routes,
embedded assets, theme translation, security policy, and the read-only
control boundary — and nothing else. It does not own a socket.

---

## 1. Ownership boundary

| Concern | Owner |
| --- | --- |
| TCP listener, bind, accept, shutdown handshake | `i2pr-daemon` (`console.rs`) |
| HTTP/1 substrate (parsing, connection limits, request bounds) | EggServe |
| Routing, extraction, responses | `i2pr-console` (`routes.rs`) |
| Assets, themes, view models | `i2pr-console` |
| Canonical Proposal-170 dispatch | `i2pr-daemon` (`i2pcontrol_dispatch.rs`) |

The console crate has **zero** workspace dependencies, which is what keeps
it from becoming a back door into the router-owner crates. `scripts/check-dependency-direction.sh`
records that as `"i2pr-console": set()`.

The daemon never names an `axum` type. It receives `i2pr_console::AppRouter`
behind the EggServe tower adapter:

```text
i2pr-daemon::console
  -> RuntimeConfig (bounds, cache, shutdown)
  -> Server::builder().from_listener(listener).build()
  -> start_with_service(TowerToEggserve::new(console_router))
  -> ServerHandle::shutdown / wait on cancellation
```

`axum::serve` is forbidden workspace-wide and is checked. Axum is built with
`default-features = false, features = ["form", "json", "query"]` — notably
**without** `tokio`, so the crate stays outside the runtime-boundary rule
and never acquires an async runtime.

## 2. Modules

| Module | Responsibility |
| --- | --- |
| `assets.rs` | Compile-time `(path, body, content-type)` table. No filesystem, no path join. |
| `color.rs` | `#rrggbb` parsing, WCAG contrast, deterministic repair. |
| `theme.rs` | Halloy-compatible TOML → semantic palette → CSS custom properties. |
| `html.rs` | `Text` newtype; escaping is the only way to interpolate. |
| `routes.rs` | Route table, guard middleware, centralized response policy. |
| `secret.rs` | Redacted, zeroizing, non-`Clone` credential holder. |
| `security/` | Authority, headers, password, session, and the guard stack. |
| `control.rs` | `ControlClient`, `Availability`, `Overview`. |

## 3. Themes

The console speaks the Halloy theme vocabulary as **data**. A bounded TOML
document maps `[general]`, `[text]`, `[buttons.*]`, `[buffer]`, and
`[formatting]` onto web roles, which are emitted as CSS custom properties.

Three properties make this safe to serve under a strict CSP:

1. **Property names are compile-time constants.** `CSS_VARIABLE_ORDER` is
   the only list; a theme cannot add, rename, or reorder one.
2. **Values are `#rrggbb` and nothing else.** Everything passes through
   `Rgb::to_css_hex`, which cannot emit a delimiter or quote.
3. **Every pairing clears WCAG AA.** `ThemePalette::finalize` repairs any
   pair below 4.5:1 deterministically, so a low-contrast theme is corrected
   rather than rendered.

Bounds: 64 KiB per document, nesting depth 8, 256 declared keys, 9-byte
color literals. Depth and key count are checked by a lexical pre-scan so
parser work stays proportional to the input ceiling.

Bundled palettes are **original i2pr work**; no Halloy file is vendored.
See [`crates/i2pr-console/assets/themes/PROVENANCE.md`](../../crates/i2pr-console/assets/themes/PROVENANCE.md).

## 4. Browser security

The guard stack runs as middleware, in a fixed order, before any handler:

1. syntactic bounds on authority/origin headers;
2. `Host`/authority validation — DNS-rebinding defence;
3. origin validation for unsafe methods — cross-site request defence;
4. session authentication when the console is authenticated;
5. CSRF validation for unsafe authenticated actions;
6. the handler.

`RequestGuard::admit` is a pure function of headers + method + policy, so
the whole decision is testable without a router and cannot drift between
call sites.

### Authority policy

The accepted set is derived from the **bound** address and port: the literal
`ip:port` plus `localhost:port` for an IPv4 loopback listener. Matching is
exact apart from ASCII case. No proxy header is consulted, and no
trusted-proxy mode exists. `Referer` is a fallback only when `Origin` is
absent, and matches the origin **plus the `/` that terminates the
authority**, which is what stops `localhost:7070.evil.test`.

Because the policy depends on the real port, the router is built *after*
the bind, not in the constructor. With `port = 0` the port does not exist
until then.

### Credentials

- Argon2id, **16 MiB / 2 passes / 1 lane** — deliberately not the desktop
  default; a small SBC must not allocate 256 MiB to log in.
- At most **2 concurrent verifications**; saturation is refused, not
  queued, because queuing is an unbounded memory amplifier.
- A supplied PHC hash is range-checked against the console's own ceilings.
- The plaintext is converted during configuration parsing and does not
  survive into `Config`; `ConsolePasswordHash` and `RawConsoleConfig` both
  redact.
- `auth = true` without credential material is a configuration error, so
  there is no dormant default password.

### Sessions

Opaque 32-byte hex identifiers in an `HttpOnly; SameSite=Strict; Path=/`
cookie with no `Domain`. Idle (15 min) and absolute (8 h) expiry, a
32-session ceiling, deterministic eviction, explicit revocation, and a
`clear()` on shutdown. `Secure` is deliberately absent and the localhost
exception is documented rather than assumed.

Login throttling is **console-wide**. The substrate does not surface a
per-connection peer identity, and no proxy header is trusted, so there is
no unforgeable per-client key; inventing one from `Host` would be a guess
an attacker controls. The failure mode is a bounded delay that expires with
the window, not a lockout.

### Response policy

Every response — including every rejection and every redirect — goes
through one function, so no route can ship without the headers and none can
introduce CORS. Redirects are constructed through it rather than via
`axum::response::Redirect`, which would bypass the policy.

CSP is `default-src 'none'` with `script-src 'self'`, `style-src 'self'`,
`frame-ancestors 'none'`, `form-action 'self'`, `base-uri 'none'` and **no
`unsafe-inline`** — the shell ships no inline code, so the escape hatch is
never needed.

## 5. Read-only control boundary

The console never speaks the control wire and never names a method. It
depends on a two-method trait:

```rust
pub trait ControlClient: Send + Sync + Debug {
    fn router_info(&self) -> ControlReply;
    fn client_services(&self) -> ControlReply;
}
```

Because the surface is closed and has no method-name parameter, "a browser
cannot select arbitrary daemon methods" is a property of the type rather
than a runtime check. The daemon's implementation holds a
`LocalConsolePrincipal`, a unit struct with a private field that only the
daemon can issue, and whose allow-set is `RouterInfo` and
`ClientServicesInfo` — closed and read-only.

The principal bypasses **only** the external bearer-token transport step.
Method, parameter, selector, source-availability, bounds, and redaction
checks all still run, in `i2prcontrol_dispatch.rs`, which is now the single
implementation used by both the external listener and the console.

### Honest unavailability

`Availability` distinguishes `returned`, `unavailable`, `unsupported`, and
`failed`. A missing field is `unavailable` with a null value — never zero,
never empty. `requestable_overview_selectors` withholds selectors the
canonical source matrix does not mark `Available`, because requesting a
publish-gated row fails the *entire* request and would blank a truthful
page. That decision is derived from the contract, not hand-maintained.

## 6. Testing

| Suite | Scope |
| --- | --- |
| `src/**` unit tests | color/contrast, theme bounds, authority, sessions, throttle, view model |
| `tests/console_routes.rs` | router behaviour through the tower service |
| `tests/console_browser_security.rs` | guard stack end to end, cookies, CSRF, headers |
| `i2pr-daemon tests/console_loopback.rs` | real socket, real HTTP, real control data |
| `i2pr-daemon src/i2pcontrol_dispatch.rs` | local-principal vs external-wire parity |

## 7. Boundaries enforced by script

- `scripts/check-console-boundaries.sh`
- `scripts/check-console-browser-security.sh`

Both are in `AGENTS.md`'s routine floor and in routine CI.

## 8. Dependencies

**No workspace dependencies.** External production deps are `argon2`, `axum`,
`rand_core`, `serde`, `serde_json`, `toml`, and `zeroize` — the Argon2id
verifier, the router/extractor stack, CSRF/session token entropy, and the
bounded TOML theme reader. There is no HTTP client crate and no TLS crate: the
console makes no outbound request and terminates no TLS.

`axum` is declared workspace-wide as
`default-features = false, features = ["form", "json", "query"]`. Dropping
`tokio` is the load-bearing part — it is what lets the crate satisfy the
runtime-neutral posture without special-casing the checker.

## 9. Claims

The console is **experimental, loopback-only, disabled by default, and
non-advertised**. It presents no router capability beyond what the
canonical Proposal-170 dispatch already answers, and it promotes no
support or conformance claim.

It is also currently **unreachable from the product path**, because `i2pr run`
does not open any listener (see `README.md` → "Known limitation"). The
behaviour above is proven by `console_loopback.rs` and the crate's own suites,
not by a running router.

## 10. Cross-references

- [Overview](overview.md) — crate index and data flow.
- [i2pr-daemon.md](i2pr-daemon.md) — the listener, `ControlDispatcher`, and `[console]` config.
- [i2pr-i2pcontrol.md](i2pr-i2pcontrol.md) — the Proposal 170 contract the overview reads through.
- [dependency-graph.md](dependency-graph.md) — the `set()` allowlist entry.
- [security-model.md](../../docs/security-model.md) — browser-facing threat surface.
- ADR [`0034`](../../docs/adr/0034-eggserve-axum-router-console-http-substrate.md) — the substrate decision.
- Plans [`356`](../../plans/implementation/router-console/356-eggserve-axum-self-contained-console-foundation.md),
  [`357`](../../plans/implementation/router-console/357-loopback-browser-security-and-optional-authentication.md),
  [`358`](../../plans/implementation/router-console/358-prop170-control-client-and-read-only-overview.md);
  closure in [`plans/closure/router-console/`](../../plans/closure/router-console/).