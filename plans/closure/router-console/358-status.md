# Plan 358 closure — Proposal 170 control client and read-only overview

Status: **passed-prop170-control-client-and-read-only-overview**.

Classification: **capability + infrastructure**. This closure proves the
architectural contract: a useful browser view can be built entirely on the
router's canonical control plane, without enabling the external I2PControl
listener and without growing an implementation-private management API. It adds
**read-only** presentation. It establishes no mutating capability, no Java/I2P+
feature parity, and no anonymity or privacy property.

## Commits

- `73506b38` — implementation (`crates/i2pr-console/src/control.rs`,
  `crates/i2pr-daemon/src/console.rs`, the new
  `crates/i2pr-daemon/src/i2pcontrol_dispatch.rs`, and the delegation change in
  `src/i2pcontrol.rs`).

The closure/registry/roadmap status transition is in the commit that adds this
record.

## Dispatch ownership, before and after

**Before.** `I2pControlServiceState` owned the RouterInfo and ClientServices
handlers directly. A console wanting router state had exactly two honest options:
speak the external JSON-RPC wire (requiring an external listener, a control
password, and a bearer token), or reach into router-owner handles (forbidden).

**After.**

```text
                    ┌─────────────────────────────────────────┐
  external          │  I2pControlServiceState                  │
  I2PControl  ─────▶│    TLS + framing + token table            │
  (TLS, token auth) │    │  dispatch_body                        │
                    │    ▼                                     │
                    │  ControlDispatcher  ◀── LocalConsolePrincipal
                    │    process_router_info                   │
                    │    process_client_services               │
                    └─────────────────────────────────────────┘
                                      │  (the ONLY implementation)
                                      ▼
                        ConsoleControlClient (i2pr_console::ControlClient)
                                      │
                                      ▼
                              i2pr-console  ──▶  browser
```

`ControlDispatcher` was extracted from `I2pControlServiceState` into
`src/i2pcontrol_dispatch.rs`; the listener state now delegates to it. There is
one implementation of each handler, reached by both surfaces.

The principal bypasses **only** the external bearer-token transport step. Method,
parameter, selector, source-availability, bounds, and redaction checks all still
run in the dispatcher.

## Local-console authority type and allow-set

`LocalConsolePrincipal` is a unit struct with a private field, so it can be
issued only by the daemon composition root. Its allow-set is closed:

| Method | Reachable from the console |
|---|---|
| `RouterInfo` | yes — read-only |
| `ClientServicesInfo` | yes — read-only |
| `TunnelManager` | **no** |
| `AddressBook` | **no** |
| any mutating or future method | **no**, until explicitly added |

Because the allow-set is an allow-list, a **new** control method is invisible to
the console until someone adds it to the set. That is the correct direction for a
read-only posture to fail in.

`local_principal_admits_only_the_read_only_methods` proves the refusal.

## No external I2PControl listener, password, or token required

| Property | Evidence |
|---|---|
| Console operation does not enable or bind the external listener | `console_does_not_enable_the_external_listener_by_its_own` |
| A console-only configuration needs no control password and no control listener | `console_only_configuration_needs_no_control_password_or_listener` |
| The console holds no I2PControl credential | the daemon-side client holds only an `Arc<ControlDispatcher>` and a principal |
| External wire/auth behaviour unchanged | `I2pControlServiceState` gained a dispatcher field and delegating methods; no wire, framing, token, or TLS change. The existing I2PControl suites pass unchanged. |

## Local-vs-wire differential corpus

`local_principal_result_matches_the_external_wire_result` drives the **same**
request through both surfaces and requires byte-identical `result` values. It
also asserts the difference that *is* real: the external path refuses an
unauthenticated read with no token, which is precisely what the principal
bypasses — and only that.

Because the dispatcher is shared rather than duplicated, divergence is impossible
by construction rather than by review. The parity test is a backstop against a
future refactor that reintroduces a second implementation.

## `i2pr-console` dependency graph

Zero `i2pr-*` production dependencies, unchanged from Plan 356. The console
reaches router state **only** through the two-method `ControlClient` trait:

```rust
pub trait ControlClient: Send + Sync + Debug {
    fn router_info(&self) -> ControlReply;
    fn client_services(&self) -> ControlReply;
}
```

The surface is closed and has **no method-name parameter**, so "a browser cannot
select arbitrary daemon methods" is a property of the type rather than a runtime
check. The console crate names no `axum::serve`, no socket, and no router owner.

## Overview field / source / support matrix

Overview selectors are compile-time constants in
`i2pr_console::control::{OVERVIEW_SELECTORS, CLIENT_SERVICE_SELECTORS}` and are
canonical contract fields (`every_overview_selector_is_a_canonical_contract_field`).

| Selector | Requested? | Note |
|---|---|---|
| `i2p.router.version` | yes | verified base row; round-trip proved by `verified_base_overview_selectors_answer` |
| `i2pr.router.uptime` | yes | verified base row |
| `i2pr.router.status` | yes | verified base row |
| `i2pr.router.net.status` | only if the canonical source matrix marks it `Available` | otherwise withheld |
| `i2p.router.netdb.knownpeers` | only if `Available` | otherwise withheld |
| `i2p.router.netdb.activepeers` | only if `Available` | otherwise withheld |

Client-service selectors: `I2PTunnel`, `HTTPProxy`, `SOCKS`, `SAM`, `BOB`, `I2CP`
— all canonical, all read-only.

**Selector withholding is derived, not hand-maintained.** Requesting a
publish-gated row fails the *entire* canonical request, which would blank an
otherwise truthful page, so `requestable_overview_selectors` withholds anything
the source matrix does not mark `Available`
(`the_overview_requests_a_strict_subset_so_publish_gated_rows_are_withheld`). The
three base rows use an explicit `VERIFIED_BASE_OVERVIEW_SELECTORS` set because
base rows have no canonical source matrix; `verified_base_overview_selectors_answer`
proves each one actually answers, so a selector that stops answering fails a
test instead of silently blanking the page.

## Explicit unavailable / unsupported rendering

`Availability` distinguishes `returned`, `unavailable`, `unsupported`, and
`failed`. A missing field renders as `unavailable` with a **null** value — never
zero, never empty.

| Situation | Rendered |
|---|---|
| field present in a returned reply | value + `returned` |
| reply returned, field absent | `null` + `unavailable` |
| reply itself unavailable | `null` + `unavailable` |
| method unsupported | `null` + `unsupported` |
| request failed | `null` + `failed` |
| service object with no scalar leaf | `null` + `unavailable` |

`an_unpublished_selector_fails_honestly_rather_than_reporting_zero` covers the
upstream source; `console_serves_real_control_data_to_the_browser` proves it end
to end over a real socket against **real** control data, asserting that every
unavailable metric is `"value":null` and every unavailable service row is
`"state":null`.

**Defect found and fixed during this work:** the service-row availability match
had no `(None, Returned)` arm, so a service object exposing no scalar leaf was
reported `returned` — claiming the control plane answered when it rendered no
value at all. The metric path already had the correct arm; the service path did
not. Fixed in `Overview::build`, and the loopback test was scoped so it asserts
`"state":null` for service rows rather than accidentally conflating them with
metrics. That test now proves the fix end to end.

## Browser no-secret, escaping, and refresh evidence

| Property | Evidence |
|---|---|
| No I2PControl or browser-auth secret in any response | the console holds no such credential; `check-console-browser-security.sh` greps the rendering path |
| Interpolated text is escaped | `Text` is a newtype with no unescaped constructor; `rendered_text_is_escaped`, `escape_html` unit coverage including multibyte |
| Initial HTML works without JavaScript | the server renders the full overview; `console.js` only refreshes it |
| Refresh is same-origin | `connect-src 'self'`; the poll target is a relative path |
| Bounded | 15 s interval, 5 s floor, 120 s backoff ceiling |
| Single in-flight | one request at a time; a new poll aborts nothing because it waits |
| Cancellable | `AbortController`; `pagehide` aborts |
| No fan-out from a hidden tab | polling pauses while hidden |

## Requirement-to-evidence matrix

| # | Acceptance criterion | Evidence |
|---|---|---|
| 1 | Console consumes a narrow control-client abstraction and no router owners | `ControlClient`; zero `i2pr-*` deps in `i2pr-console` |
| 2 | Daemon provides an unforgeable local-console authority | `LocalConsolePrincipal`, private field, daemon-issuable |
| 3 | Local authority enters the same canonical protected dispatcher | `ControlDispatcher`, single implementation, parity test |
| 4 | Read-only and closed to RouterInfo/ClientServicesInfo | `local_principal_admits_only_the_read_only_methods` |
| 5 | Does not enable/bind the external listener or require its password | two dedicated tests |
| 6 | Local-vs-wire differential cases agree | `local_principal_result_matches_the_external_wire_result` |
| 7 | External wire/auth behaviour unchanged | no wire/framing/token/TLS change; existing suites pass |
| 8 | Overview derives all router data from canonical control results | `Overview::build` takes only two `ControlReply`s |
| 9 | Unavailable/unsupported data is explicit, never fabricated | availability matrix above |
| 10 | Initial HTML works without JavaScript | server-rendered shell |
| 11 | Refresh is same-origin, bounded, single-in-flight, cancellable | `console.js` properties above |
| 12 | No I2PControl/browser-auth secrets in browser responses | no such credential is reachable from the console |
| 13 | Missing standard admin capability documented as control-plane work | see gap matrix below |
| 14 | No router support/conformance claim changes | `specs/` untouched |
| 15 | Full routine floor passes | recorded in [`356-status.md`](356-status.md) |

## Standard control-gap matrix

The console exposes only what the standard control plane truthfully supports.
Missing capability is recorded as control-plane work, **not** bypassed with a
console-private router accessor.

| Desired capability | Standard method | Status | Recorded as |
|---|---|---|---|
| Router version / uptime / status | `RouterInfo` | available | — |
| Network status, peer counts | `RouterInfo` selectors | publish-gated; withheld when unavailable | I2PControl/Proposal-170 source-matrix work |
| Service status | `ClientServicesInfo` | available | — |
| Log ring | `LogRing` source exists (Plan 295) | not exposed here | later console page |
| Ban ledger, SSU2 snapshot | inspection sources exist (Plan 295) | not exposed here | later console page |
| Tunnel management | `TunnelManager` | implemented on the wire | **out of scope** — the console principal cannot reach it |
| Address book | `AddressBook` | implemented on the wire | **out of scope** — the console principal cannot reach it |
| Mutating any setting | none exposed | not available | requires both standard-method work and a separate authenticated/authorizing plan |

## Verification

All evidence below is from the local worktree on implementation head
`73506b38`. **No hosted CI result is claimed by this record.** The workspace
floor is recorded in [`356-status.md`](356-status.md) and was green at this head
(4 251 passed, 0 failed, 35 ignored, 152 suites).

| Command | Result |
|---|---|
| `cargo test -p i2pr-console --all-targets` | PASS — 118 unit (incl. `Overview::build` availability cases) + 23 + 9 |
| `cargo test --locked -p i2pr-daemon --lib` | PASS — 574, incl. 8 `i2pcontrol_dispatch.rs` tests |
| `cargo test --locked -p i2pr-daemon --test console_loopback -- --test-threads=1` | PASS — 6, incl. `console_serves_real_control_data_to_the_browser` |
| `bash scripts/check-console-boundaries.sh` | PASS |
| `bash scripts/check-console-browser-security.sh` | PASS |
| `bash scripts/check-dependency-direction.sh` | PASS |
| `bash scripts/check-runtime-boundaries.sh` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo deny check advisories bans sources` | PASS |

The seven bash-4+ floor checkers listed in `356-status.md` were not run on this
bash-3.2 host and are not claimed as passing.

## Security-relevant decisions

1. **`ControlDispatcher` extraction over a second implementation.** The console
   and the external listener share one implementation of every handler, so the
   console cannot drift into a private dialect of the control plane.
2. **Allow-set, not deny-list**, for the principal's method vocabulary.
3. **Derived selector withholding.** The honest presentation is computed from
   the canonical source matrix rather than maintained by hand.
4. **Null, never zero.** Availability is a first-class field on every row.
5. **The console holds no I2PControl credential at all**, which is stronger than
   holding one and not using it.

## Known limitations

- **Read-only.** No mutating console page exists, and none can be added without
  a separate plan that changes the principal's authority.
- **No Java/I2P+ feature parity.** Deliberately out of scope for this milestone.
- The overview's deeper selectors are withheld whenever the source matrix marks
  them unavailable, so the first slice presents fewer panels than a
  Java/I2P+-class console.
- The console is experimental, loopback-only, disabled by default, non-advertised,
  and has **no product-reachable path** while `i2pr run` remains broken.

## Unresolved findings

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low:** the overview is intentionally narrow; richer panels are follow-on work
  recorded in the control-gap matrix rather than gaps in this one.

## Unblock audit for later console administration planning

No registered plan was blocked on 358. The roadmap's "later functional pages"
remain unnumbered until the first vertical slice identifies exact missing
standard control methods; this closure supplies the evidence to do that
(control-gap matrix above). Any mutating page requires, at minimum: a new plan,
a change to the principal's authority, and an authorising (not merely
authenticating) control method — none of which exists today.

## Roadmap disposition

Plan 358 is **closed** as capability plus infrastructure. The initial
console-console foundation milestone described in
`plans/subsystems/router-console-roadmap.md` §11 is met except for the honest
caveat that `i2pr run` does not open any listener, so the console is not
reachable through the product binary. That is a pre-existing daemon defect
tracked separately, not a Plan 358 gap.