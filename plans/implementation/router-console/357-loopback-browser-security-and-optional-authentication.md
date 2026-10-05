# Plan 357 — loopback browser security and optional console authentication

Status: **blocked-router-console-browser-security-on-plan356**.

Classification: **invariant + infrastructure**.

Roadmap:
- `plans/subsystems/router-console-roadmap.md`

Hard dependency:
- Plan 356 must close with the EggServe/Axum console substrate, loopback-only listener, and embedded-resource boundary proven.

Successor:
- Plan 358 consumes this hardened browser boundary for the first Proposal-170-backed read-only overview.

## Objective

Harden the router-console browser boundary before any router state or mutating administrative action is exposed.

The result must provide:

- strict loopback/Host/Origin enforcement resistant to DNS rebinding and cross-site requests;
- a restrictive CSP and standard browser hardening headers;
- optional console-specific password authentication;
- finite, bounded, revocable browser sessions;
- login throttling;
- CSRF protection for all state-changing browser actions;
- explicit separation between browser credentials/sessions and I2PControl credentials/tokens;
- security semantics that remain valid whether the console is running in authenticated or intentionally unauthenticated localhost-only mode.

This plan does not add router control/data pages beyond security test fixtures.

## Why this plan is blocked

Plan 357 depends on Plan 356's actual request/response/runtime composition. Browser-security middleware must be tested on the real EggServe -> Tower -> Axum path, not planned against an abstract framework.

The security policy itself is sufficiently determined to register now.

## Current implementation evidence

At registration there is no console implementation, so no browser auth/session/CSRF owner exists.

Existing i2pr patterns relevant to the design include:

- I2PControl password/token redaction and constant-time secret comparisons;
- bounded token tables and finite token lifetimes;
- loopback-default/disabled-by-default listener policy;
- explicit fail-closed configuration normalization;
- secret types that do not expose raw values through `Debug`/`Display`.

These patterns should inform the console but must not cause browser sessions to reuse the I2PControl token.

## Architecture decisions frozen by this plan

### 1. Loopback remains mandatory even with authentication

Authentication is defense in depth and local multi-user protection; it is not authorization to expose the console remotely.

Non-loopback configuration remains invalid.

### 2. Validate authority before application routing

Reject requests whose effective authority/Host is not one of the console's loopback origins.

Accepted forms are bounded and derived from the configured local listener, including the exact port and loopback literals/names explicitly allowed by policy.

Do not trust `X-Forwarded-Host`, `Forwarded`, or other proxy headers.

Do not add trusted-proxy mode.

### 3. Origin checks are default-deny for unsafe methods

For POST/PUT/PATCH/DELETE or any route that can change server-side/browser session state:

- require an acceptable same-origin `Origin` when browsers send one;
- reject cross-origin origins;
- validate `Referer` only as a conservative fallback where required by browser behavior;
- never use permissive CORS.

Safe GET/HEAD routes must remain side-effect free.

### 4. Browser authentication is separate from I2PControl authentication

The console gets its own authentication configuration and secret type.

A successful browser login yields only a console session. It must not reveal, serialize, or derive from the I2PControl token.

The console backend separately owns whatever I2PControl credential mechanism Plan 358 introduces.

### 5. Use opaque bounded sessions, not browser-stored control tokens

Authenticated mode uses a cryptographically random opaque session identifier in an HttpOnly cookie.

Server-side session state is bounded by:

- maximum live sessions;
- idle and absolute expiry;
- deterministic expiry/eviction;
- finite per-session CSRF state;
- explicit logout/revocation.

Do not place router control tokens, passwords, capabilities, or serialized authority in the cookie.

Cookie policy must use at least:

- `HttpOnly`;
- `SameSite=Strict`;
- exact path suitable for the console;
- no broad Domain attribute.

If `Secure` cannot be relied on for plain-HTTP loopback across supported browsers, document the exact localhost exception rather than pretending transport confidentiality exists. Remote HTTP is forbidden regardless.

### 6. Password verification must resist offline/plain comparison mistakes

Prefer a standard password KDF such as Argon2id for a stored password hash/derived verifier, with bounded memory/time parameters suitable for the router's supported constrained hosts.

The plan must benchmark/select parameters rather than importing desktop defaults that can stall a small SBC.

If operator config initially accepts a plaintext password for compatibility/ergonomics, convert it to the in-memory verifier during startup and keep the raw secret in a zeroizing/non-debug handle for the shortest practical lifetime. Do not log it.

If a pre-hashed configuration representation is practical and migration-safe, prefer it.

No custom password hash/KDF.

### 7. CSRF is explicit even under SameSite

Each authenticated session receives a random CSRF secret/token.

Every state-changing form/API action must verify it in addition to same-origin policy.

Plan 357 may use fixture mutation routes to prove the mechanism; real router mutations remain future work.

### 8. Security headers are centralized

Every HTML/application response uses a centralized policy, with at least:

- `Content-Security-Policy` denying external origins and framing;
- `X-Content-Type-Options: nosniff`;
- `Referrer-Policy` restrictive enough for a local admin app;
- `Cross-Origin-Opener-Policy` where supported/compatible;
- no permissive CORS headers;
- cache policy appropriate to authenticated administrative content.

Prefer no inline JavaScript or style so the CSP does not need `unsafe-inline`.

If inline data is unavoidable, use non-executable JSON/data blocks rather than relaxing script policy.

## Invariants that must not regress

1. Authentication never permits non-loopback binding.
2. Host/authority validation happens for authenticated and unauthenticated modes.
3. Cross-origin unsafe requests fail before handler side effects.
4. GET/HEAD routes are side-effect free.
5. Browser session tokens are distinct from I2PControl tokens.
6. No password/session/CSRF/control token is logged or reflected.
7. Session/login/CSRF tables are bounded.
8. Expired/revoked sessions cannot be reused.
9. Authentication-disabled mode contains no dormant default password/backdoor session.
10. CORS is absent/default-deny.
11. CSP does not authorize remote scripts/styles/fonts.
12. Future eepsite listeners cannot present console cookies or routes.
13. Shutdown drops in-memory session/auth state.
14. No router capability/support claim changes.

## Scope

### In scope

- `[console]` authentication configuration;
- password verifier and secret handling;
- login/logout routes;
- opaque server-side sessions;
- session cookie middleware/extractor;
- login throttling;
- Host/authority validation;
- Origin/Referer policy;
- CSRF tokens and fixture mutation test route;
- centralized security headers/CSP;
- cache/header rules for authenticated pages;
- deterministic bounded cleanup;
- threat-model/architecture docs;
- focused fuzz/property tests for cookie/header/form parsing where useful.

### Out of scope

- router/I2PControl authentication itself;
- SSO/OAuth/OIDC/WebAuthn/passkeys;
- remote/LAN access;
- TLS termination for the console;
- reverse-proxy trust;
- multi-user RBAC;
- persistent browser sessions across router restart;
- recovery email/remote password reset;
- actual router mutation pages;
- managed-app authentication.

## Required production changes

### A. Console auth config

Extend `[console]` with an explicit authentication mode.

A simple initial shape may be:

~~~toml
[console]
enabled = true
auth = false
~~~

Authenticated mode must require configured/generated credential material; no universal/default password.

Avoid a confusing state where `auth = true` silently accepts an empty password.

### B. Password verification owner

Add a console-only secret/verifier type with redacted `Debug`.

Review the KDF dependency for:

- license;
- maintenance;
- unsafe/transitive crypto provider use;
- MSRV;
- memory behavior on constrained hosts.

Use a bounded semaphore or equivalent so concurrent login attempts cannot cause unbounded KDF memory/CPU amplification.

### C. Session store

Use an in-memory bounded map keyed by random opaque tokens.

Freeze ceilings and expiry durations in typed configuration/constants with conservative maxima.

Session creation must fail predictably under saturation rather than growing unboundedly.

### D. Browser request guard stack

Apply request policy in a deterministic order before protected handlers:

1. request/authority syntactic bounds;
2. Host/authority policy;
3. origin policy for unsafe methods;
4. authentication/session lookup where enabled;
5. CSRF for unsafe authenticated actions;
6. route handler.

Do not let route-specific code reinvent these checks.

### E. Security response policy

Add one reusable response/header layer for HTML/API errors and protected pages.

Static immutable assets may use a narrower cache policy but must retain CSP-compatible content types and `nosniff`.

## Work packages

### WP1 — Threat model and policy freeze

Document DNS rebinding, CSRF, XSS/content injection, clickjacking, session fixation/theft, brute-force login, and local multi-user threats.

### WP2 — Host/origin/security-header middleware

Implement and prove default-deny authority/origin semantics on the actual EggServe/Axum path.

### WP3 — Optional password verification + bounded login throttle

Implement the KDF/verifier and concurrency/resource limits.

### WP4 — Session/cookie lifecycle

Implement login, session issuance, expiry, logout, eviction, and shutdown cleanup.

### WP5 — CSRF and mutation fixture

Add CSRF enforcement with deterministic negative/positive tests before real router mutations exist.

### WP6 — Security/adversarial regression suite

Exercise malformed headers/cookies/forms, cross-origin attempts, DNS-rebinding Host forms, saturation, expiry, and secret redaction.

## Failure, cancellation, restart, and contention semantics

- invalid Host/Origin is rejected before auth/KDF/control work;
- malformed or oversized cookies/forms fail with bounded 4xx responses;
- login throttle saturation/limit produces deterministic rejection;
- KDF work is concurrency-bounded and cancellable at the request/task boundary as practical;
- session-table saturation rejects/evicts according to a documented deterministic policy;
- logout revokes immediately;
- absolute expiry wins over idle refresh;
- daemon restart invalidates all sessions by design;
- shutdown does not persist browser session secrets;
- authentication subsystem failure may make the console unavailable but cannot fail open into authenticated routes.

## Compatibility and migration

Existing Plan-356 unauthenticated loopback deployments remain valid with `auth = false`.

Enabling auth is explicit.

If the password config representation changes from plaintext to encoded/hash form, define migration/validation rules in this plan rather than silently accepting both indefinitely.

No I2PControl config/password behavior changes.

## Required tests

At minimum:

### Host/origin

- configured IPv4 loopback Host accepted;
- configured IPv6 loopback authority accepted;
- localhost form accepted only if explicitly in policy;
- wrong port rejected;
- arbitrary hostname rejected;
- public/private IP Host rejected;
- `X-Forwarded-Host` cannot override authority;
- cross-origin unsafe request rejected;
- same-origin unsafe request proceeds to auth/CSRF;
- missing Origin follows documented fallback policy;
- CORS preflight does not open administrative methods cross-origin.

### Authentication/session

- auth disabled reaches shell with no session;
- auth enabled requires valid password;
- empty/default password invalid;
- wrong password indistinguishable except generic failure;
- password/verifier redacted from debug/errors;
- successful login issues opaque bounded cookie;
- session token is not I2PControl token-shaped/reused;
- idle expiry;
- absolute expiry;
- logout revocation;
- max-session saturation;
- deterministic eviction if used;
- restart invalidation;
- KDF concurrency bound;
- throttle success/failure/reset behavior.

### CSRF

- authenticated mutation without token rejected;
- wrong token rejected;
- correct token accepted;
- token from another session rejected;
- logged-out/expired session token rejected;
- safe GET has no mutation side effect.

### Headers/XSS posture

- CSP exact expected policy;
- frame ancestors denied;
- nosniff;
- restrictive referrer policy;
- no permissive CORS;
- no unsafe-inline requirement introduced by normal assets;
- user-controlled fixture strings are escaped, never executable HTML/JS.

## Exact verification commands

Focused:

~~~text
cargo fmt --all --check
cargo check --locked -p i2pr-console --all-targets
cargo test --locked -p i2pr-console --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon console -- --test-threads=1
cargo clippy --locked -p i2pr-console -p i2pr-daemon --all-targets --all-features -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
~~~

Run the Plan-356 console resource/boundary checker plus new browser-security checker/tests.

Closure must also run the full routine floor from `AGENTS.md`.

## Documentation updates

Required:

- `docs/security-model.md`;
- `docs/architecture/i2pr-console.md`;
- console configuration reference;
- router-console roadmap;
- registry;
- `plans/closure/router-console/357-status.md` at closure.

Document exact local threat assumptions: loopback reduces network exposure but does not eliminate malicious websites/browser-origin attacks or hostile local users.

## Acceptance criteria

Plan 357 passes only when:

1. loopback-only remains invariant regardless of auth setting;
2. Host/authority and unsafe-method Origin validation are centralized and fail closed;
3. optional auth has no empty/default credential;
4. password verification uses a reviewed standard KDF with bounded concurrency/resources;
5. sessions are opaque, server-side, bounded, finite, and revocable;
6. browser session tokens are not I2PControl tokens;
7. CSRF protects every state-changing fixture route;
8. CSP/security headers are strict and compatible with the self-contained Plan-356 UI;
9. no permissive CORS exists;
10. secrets are redacted and never reflected;
11. restart invalidates sessions cleanly;
12. auth failure cannot fail open;
13. adversarial tests pass on real EggServe/Axum composition;
14. full routine floor passes.

## Stop conditions

Stop and register a narrower corrective/design plan if:

- secure authentication requires remote TLS/reverse-proxy semantics;
- the selected KDF cannot be bounded safely on supported SBCs;
- Axum middleware ordering makes a route reachable before Host/Origin/auth policy;
- existing inline assets require weakening CSP with broad `unsafe-inline`;
- cookies must be shared with another HTTP role;
- a security requirement would require merging console and I2PControl credentials.

## Closure evidence required

The closure must include:

- browser threat model;
- exact security-header policy;
- Host/Origin acceptance matrix;
- auth-disabled/auth-enabled behavior;
- KDF dependency/parameter/resource evidence;
- session ceilings/expiry/eviction evidence;
- CSRF negative/positive evidence;
- secret-redaction evidence;
- restart/shutdown evidence;
- full routine-floor results;
- unblock audit for Plan 358.

## Handoff notes

Keep the implementation independent of router-control data. Use fixture state to prove mutation protection.

Plan 358 should inherit one already-hardened browser boundary rather than attempting to solve web security while also introducing Proposal-170 data.
