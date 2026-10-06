# Plan 342 — I2P-routed outproxy provider and canonical field completion: status

Status: **passed-loopback-wire-lane-landed-live-failover-rotation-unproven**

Plan of record:
[`342-i2p-routed-outproxy-provider-and-canonical-fields.md`](../../implementation/i2pcontrol-proposal-170/342-i2p-routed-outproxy-provider-and-canonical-fields.md).
Origin: Plan 327's blocked remainder, on top of Plan 343 (provider), Plan 341
(outbound secret owner), Plan 292 (ProxyAuth precedent), Plan 176 / Plan 290
(HTTP and SOCKS request-target grammars).

## The headline

A clearnet request through an I2P-routed outproxy now **executes end to end in
tree**, and — more importantly — executing it is what exposed the fact that the
step-3 request-path integration was structurally correct and *completely
unreachable*. All four request paths now carry outproxy requests, the option
surface is admitted as one all-or-none block, and the credential moves through
Plan 341's owner and nowhere else.

Plan 327 stays **blocked**. The live failover rotation between two configured
outproxies is unproven, and nothing here has been run against Java I2P or i2pd.
This is loopback evidence, not interoperability.

## The defect the wire lane found

This is the reason the lane exists and it deserves to be stated first.

Every request-target grammar in the tree hard-required a `.i2p` suffix:

| grammar | rule | refused clearnet at |
|---|---|---|
| `parse_absolute_form` (Plan 176) | `validate_host` requires `.i2p` | parse, `403` |
| `parse_authority_form` (Plan 176) | `validate_host` requires `.i2p` | parse, `403` |
| `validate_domain_policy` (Plan 290 SOCKS5) | requires `.i2p` | inside the negotiator, `HostUnreachable` |
| `validate_domain_policy` (Plan 290 SOCKS4a) | requires `.i2p` | inside the negotiator |

So a `CONNECT example.com:443` was answered `403` **by the parser**, and a SOCKS5
`CONNECT example.com:80` was answered `HostUnreachable` **from inside the
negotiator**. Both returned before `classify_client_target` was reached.
`ClientTargetClass::ViaOutproxy` and `Refused` were dead in production while
their unit rows — which call the classifier directly — passed.

That is the inert-acceptance failure Plan 342 was written to prevent, one level
deeper than the plan anticipated. The plan worried about an option surface with
no route behind it; what actually shipped was a route with no reachable input.

### The fix: target policy is a policy value, not a grammar property

New `i2pr_service_tunnels::target_policy::TargetPolicy`:

- `I2pOnly` — the Plan 176 / Plan 290 default. Every pre-Plan-342 entry point is
  a thin wrapper over it, so nothing widened by accident and a caller must
  *name* the relaxed policy to get it.
- `AllowsClearnet` — a well-formed clearnet name parses and then reaches
  `classify_client_target`, which decides Direct / ViaOutproxy / Refused.

Only the **suffix requirement** became policy-dependent. Still refused under the
relaxed policy, deliberately:

- **IP literals** — an outproxy carrying a numeric authority has no name to
  apply a `Host`-based policy to, so allowing it would make the tunnel an open
  relay by address.
- **`localhost` / `.localhost`** — a foreign resolver must not be asked for
  loopback.
- **userinfo**, control bytes, length ceilings, zero port, port-only authority,
  scheme and form grammar — all unchanged.

Mixed-suffix confusion (`example.i2p.com`) is the classifier's decision, not the
parser's: the parser decides syntax, the classifier decides routes, and neither
guesses at the other's job.

`ServiceTunnelManager::target_policy(spec_id)` is the single decision point, and
it reads the **provider registry**, not the spec's options value. Both the
parser policy and the classifier's provider come from `outproxy_provider`, so a
provider removed between reconciliation and a request cannot leave the two
disagreeing — the parse and the classification cannot diverge.

## Scoping question, settled

Plan 342's step 3 says "the HTTP and CONNECT client paths". `handle_proxy_request`
is the other half of the HTTP client path — `run_http_connection` dispatches
CONNECT and everything else to it — and the seven-field block is admitted on the
`httpclient` kind.

**A forward path that ignored the block would be inert acceptance at sub-path
granularity**: accepted on the kind, honoured on only one of its two request
forms. So `handle_proxy_request` classifies too, and `forward_via_outproxy`
carries the request.

It is not a copy of the direct path, because after `build_attempt` an
`OutproxySession` is a **byte pipe to `example.com:80`**, not a conversation with
a forward proxy. The request written onto it must be origin-form with the real
name in `Host:` — and above all *not* the `b32.i2p` substitution the direct path
makes, which would send every forwarded request to the outproxy's default vhost.
`forward_via_outproxy` therefore receives the client's own `RequestTarget` and
uses it for both the request line and `rewrite_headers`. `scripts/check-outproxy-request-path.sh`
forbids `target_for_remote_destination` appearing in that function at all.

## Two more real defects, both found by running the lane

1. **The outproxy opener never kicked the delivery driver.**
   `service_tunnels_http::open_streaming` calls `notify_outbound_signal` right
   after the SYN so the queued SYN is routed immediately rather than waiting for
   the fallback tick. `open_outproxy_streaming`, written later, did not. Without
   the kick the SYN stayed queued, `wait_for_established` timed out, and every
   route failed with `TargetUnreachable` while the counters looked like a network
   problem. Fixed by adding the kick; the direct path's comment is the precedent.

2. **A control commit drops manager-only specs.** A control reconcile publishes
   `startup ∪ control-owned definitions`, so a tunnel that is only in the
   *manager's* spec set is dropped by the first `create`. In the lane the
   outproxy endpoint kept vanishing. The endpoint is therefore **startup-owned**,
   which is also how it would be deployed — the endpoint and the proxy client are
   separate tunnels with separate lifecycles.

A third was in the test rather than the product, recorded because it nearly
produced a false finding: the lane's read deadline (5s) was shorter than
`OUTPROXY_HANDSHAKE_DEADLINE_MS`, so it gave up while the route was legitimately
in flight and reported an empty read as a refusal.

## Requirement-to-evidence matrix

| Plan 342 requirement | status | evidence |
|---|---|---|
| Seven canonical fields with real semantics | **met** | `plan342_complete_block_reaches_the_spec`; `TUNNEL_OPTIONS` 46→52, `SECRET_OPTIONS` 4→5, `canonical_option` seven rows, census 281→309 apply / 336→362 cells |
| All seven or none (no inert acceptance) | **met** | `plan342_no_proper_prefix_of_the_block_is_accepted`; `OUTPROXY_BLOCK_KEYS` + guard row |
| `OutproxyType` a closed vocabulary, not an executable | **met** | `plan342_outproxy_type_is_a_closed_vocabulary_at_the_boundary`; row rejects `"/usr/bin/curl"` |
| `ProxyList` entries are I2P destinations only | **met** | `plan342_clearnet_outproxy_entry_is_refused_at_the_boundary` |
| `OutproxyPassword` only through Plan 341's sealed form | **met** | `plan342_outproxy_password_is_sealed_before_storage` (round-trips open), `plan342_get_redacts_the_outproxy_credential` |
| Credential never echoed into control output | **met** | same row; `SECRET_OPTIONS` redaction |
| `.i2p` direct routing bypasses the outproxy | **met at runtime** | `plan342_i2p_authority_bypasses_the_outproxy` — asserts the outproxy recorded **zero** authorities |
| Clearnet target succeeds through a controlled loopback outproxy | **met at runtime** | `plan342_clearnet_target_succeeds_through_the_loopback_outproxy` — outproxy records `CONNECT example.com:443`, replies 200 **plus** a pipelined prefix, prefix reaches the local client |
| Clearnet target without a provider fails, opening no direct socket | **met at runtime** | `plan342_clearnet_target_without_a_provider_is_refused_and_opens_nothing` |
| HTTP CONNECT coverage | **met at runtime** | the headline row drives `ConnectClient` over loopback TCP |
| Applicable SOCKS coverage | **met at runtime** | `plan342_socks5_request_is_carried_by_the_outproxy` — real SOCKS5 greeting + CONNECT, success reply |
| HTTP forward coverage | **met at runtime** | `plan342_http_forward_request_is_carried_by_the_outproxy` — asserts the outproxy is asked for `example.com:80`, the scheme default, proving the forward path carries the request's own port |
| No DNS leak / no direct-clearnet fallback | **met** | guard: no `ToSocketAddrs` / `lookup_host` / literal-address connect in any request path; no `DirectClearnet` variant anywhere; evidence checker: the lane itself holds no clearnet capability |
| Proxy auth redaction | **met at runtime** | `plan342_outproxy_auth_is_presented_and_never_echoed` — the fixture *requires* the exact Basic value, so the row proves recovery **and** encoding; the client never sees the password |
| Bounded selection, retry, backoff ceiling | **partly met** | selection order + ceiling unit-pinned in `i2pr_service_tunnels::outproxy`; the **live** ceiling is proven by `plan342_unreachable_outproxy_fails_typed_and_bounded`; the **rotation** is unproven — see Limitations |
| Transactional reconfiguration with drain/rebuild | **met** | `sync_outproxy_providers` runs in the same reconciliation as `sync_els2_materials` / `sync_encrypted_target_secrets`, with the same rollback; `plan342_provider_registry_drives_the_request_target_policy` pins install *and* removal |
| `UseOutproxyPlugin` with no provider fails before allocation | **met** | block rule refuses the block on kinds without a request target; the no-provider row proves no listener carries traffic |
| No plugin loading, no command execution | **met** | `scripts/check-outproxy-wire-lane-evidence.py` scans the lane **and** the five production modules for `libloading` / `dlopen` / `Command::new` / `std::env::var` |
| Restart and persistence | **partly met** | credential + block survive a generation round trip (`plan342_sealed_block_survives_a_generation_round_trip`); a live restart carrying a request is unproven |
| Self-composed loopback fixture, nothing vendored | **met** | the lane binds only `127.0.0.1:0`, is `#![forbid(unsafe_code)]`, and resolves no name |

## What the loopback lane is composed of

```text
local test client
  -> the client tunnel's own loopback listener            (127.0.0.1:0)
  -> bounded HTTP/1.1 or SOCKS5 head read
  -> the tunnel's TargetPolicy + classify_client_target
  -> Streaming to the outproxy destination
  -> `outproxy-svc`, a service *server* tunnel whose ServerTarget is
     LoopbackTcp(the outproxy fixture)                    (127.0.0.1:0)
  -> the outproxy fixture, speaking the server side of an I2P outproxy
  -> a loopback origin fixture                            (127.0.0.1:0)
```

The outproxy is reached through **Streaming**, not a loopback shortcut:
`resolve_reference` finds it through `lookup_local_service_destination` exactly
as it would find any I2P outproxy destination. That is the property under test.

The fixture maps the authority it was asked for onto a **loopback** origin it was
given at construction. It never resolves a name and never connects to anything
but `127.0.0.1`, so the lane can assert "a clearnet request succeeded through an
outproxy" without the test process itself ever holding a clearnet capability —
which would invert invariant 1 at the layer meant to enforce it. The evidence
checker enforces that (`TcpStream::connect("` forbidden; every `bind` must be
loopback).

## Verification (all **local**; no CI is available in this environment)

- `cargo test --locked -p i2pr-service-tunnels --all-targets` — **357 passed, 0 failed**.
  This includes the four new `target_policy` rows and, importantly, every
  pre-existing Plan 176 / Plan 290 `.i2p`-only row, unchanged.
- `cargo test --locked -p i2pr-daemon --lib -- plan342` — **10 passed**.
- `cargo test --locked -p i2pr-daemon --test outproxy_loopback_wire -- --test-threads=1`
  — **8 passed, 0 failed**, 33.5s. `--test-threads=1` is required: the lane binds
  loopback listeners and spawns per-destination delivery drivers.
- Full routine floor at the F1-fix commit — **39/39 PASS, 0 failures**,
  `/tmp/floor342b.log`. `cargo test --locked --workspace --all-targets` =
  **4190 passed, 0 failed** across 173 binaries.
- Full routine floor at the closing commit — **40/40 PASS, 0 failures**,
  `/tmp/floor342final.log`. This run includes both outproxy guards
  (`check-outproxy-request-path.sh` and the new
  `check-outproxy-wire-lane-evidence.sh`), which are steps 39 and 40 of the
  floor. `cargo test --locked --workspace --all-targets` =
  **4198 passed, 0 failed, 35 ignored** across 174 binaries.
- An intermediate closing-commit floor run recorded **39 PASS / 1 FAIL**, the
  FAIL being F6 above. It is recorded rather than dropped because a result is
  never quietly replaced by a better one.
- A first attempt to extend the floor runner **silently failed**: the edit
  matched a differently-spaced line, so the new evidence-checker step was never
  inserted, and the runner reported "39/39" while `AGENTS.md` listed 40 steps.
  Recorded because it is the exact shape of a false-green the repo forbids --
  a check that is in the documented floor but was not executed is not a check.
  Caught by comparing the runner's step count against `AGENTS.md`, fixed, and
  re-run to the 40/40 above.
- `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets
  --all-features -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, doctests,
  `cargo deny` — clean.

## Guards: deliberate-break transcripts

### `scripts/check-outproxy-request-path.sh` / `.py` — 39/39

Extended from 24. New sections 8 and 9 cover the forward path and the target
policy. All 39 mutations detected. Reproduce with
`python3 scripts/check-outproxy-request-path.py --mutation-table`.

Two of the new mutations initially escaped and are recorded because the reasons
are instructive:

- *"the forward path never reaches its outproxy arm"* was anchored on
  `forward_via_outproxy(`, which matches the **definition** as well as the call
  site, so the mutation renamed the definition and left the call intact. Re-anchored
  on the indented call site.
- *"the target policy default becomes fail-open"* used an anchor that does not
  exist in `service_tunnels.rs` (the manager spells the variants through the
  crate path), so the mutation skipped rather than firing. Re-anchored.

### `scripts/check-outproxy-wire-lane-evidence.sh` / `.py` — 7/7, controls 2/2

New. Checks the *integrity* of the lane as evidence: the eight required rows
exist by name; no row is `#[ignore]`d, `should_panic`, or `cfg`-gated; the lane
holds no clearnet capability, no `unsafe`, no dynamic library loading, no command
execution; the five production modules it drives hold none either; and the static
request-path guard still passes.

It carries two deliberate-break *controls* — mutations that must **not** be
flagged — because comment-stripping is a real design decision with a real failure
mode in the other direction, and a checker that is merely too weak is only half
tested.

Its own `attribute_window` helper was initially wrong in a way that made the
`#[ignore]` row vacuous: `re.search(r"\bfn\s+name")` matches the `fn` of an
`async fn`, so `source[:fn_start]` ends with `async ` and the upward walk broke on
the first line, returning an empty window for *every* row. The first mutation
table caught it. Worth recording as a general hazard for function-scoped guards.

Reproduce with `python3 scripts/check-outproxy-wire-lane-evidence.py --mutation-table`.

## Runtime-neutrality and bounded-queue review

- `i2pr-service-tunnels` gained `target_policy.rs`, a two-variant enum with no
  I/O. `http/target.rs`, `socks5/request.rs`, `socks5/socks4a.rs` gained only
  policy parameters and `*_with_policy` entry points. No `tokio`, no `std::net`,
  no `std::fs`, no `JoinHandle`, no `spawn` — verified by
  `scripts/check-runtime-boundaries.sh`.
- `outproxy_counters()` returns `&Mutex<OutproxyCounters>` rather than a guard.
  A guard held across an `.await` would serialise every request's counters for
  the length of an outproxy handshake.
- `target_policy` takes and releases the registry lock inside `outproxy_provider`,
  which returns a cloned `Arc` — the lock is never held across an await.
- The lane's fixtures are test-local `tokio::spawn`s in a `#[tokio::test]`, which
  is permitted (the boundary is on library crates, not test binaries).

## Secret-handling review

- `outproxy_username` is an identifier, not a credential: `OptionSensitivity::Public`,
  kept in the clear, never interpolated into an error.
- `outproxy_password` is `OptionSensitivity::Secret`. It is `Zeroizing<String>`
  with a hand-written redacting `Debug`, and it exists only inside
  `parse_outproxy_block`'s return value, consumed immediately by the seal step.
- The block parser reads the password's **presence**, never its content, which is
  why it is safe to run twice: `normalize_definition` sees plaintext and seals it;
  `build_control_spec` sees the sealed stored form.
- The plaintext's only reader is `OutboundSecret::new` → `store.seal`. The only
  reader of the sealed form is `provider.auth_header()` → `store.open`, in one
  place. Both guard rows pin this.
- `OutboundSecret` / `OutboundSecretAuthHeader` deliberately have no `Debug`,
  `Display`, or `Clone`.
- `RouterIdentityBundle::with_signing_seed` is a **closure**, not a getter. One
  `Arc<dyn OutboundSecretStore>` is derived once at the composition root and
  shared between control plane (seal) and spawned runtime task (open), because
  re-deriving per task would create the second key the plan forbids.

## Findings by severity

**F1 (high, found and fixed) — the request-path integration was unreachable.**
Every request-target grammar refused a clearnet authority before the classifier
ran. Fixed by `TargetPolicy`; pinned by four runtime-neutral rows, one
reconciliation-chain row, and guard sections 8–9.

**F2 (medium, found and fixed) — the outproxy opener never kicked the delivery
driver.** Every route would have failed with `TargetUnreachable` in production.
Found only because the lane opens a real Streaming route.

**F3 (medium, found and fixed) — `OutproxyRoute` could not say "no".**
`route()`/`route_attempt()` returned `DirectI2p` for a clearnet target with an
empty proxy list. `Refused(OutproxyFailure)` was added. The daemon happened to
refuse that arm, so nothing was exploitable — but the guarantee lived in a caller
rather than in the type.

**F4 (low, found and fixed) — `proposal_tunnel_option_name` would have silently
dropped `SSLProxies`.** The generic snake_case→Pascal derivation produces
`SslProxies`, which is not in the frozen field inventory, so the value would have
vanished from `rawConfig` with no error.

**F5 (low) — the evidence checker's `attribute_window` returned an empty string
for every row.** Found by its own mutation table; the `#[ignore]` row was
vacuous until fixed.

**F6 (low, pre-existing, NOT introduced by this plan, NOT fixed) — a load-sensitive
timing flake in `plan292_idle_sweep_closes_quiet_tunnels`.** It failed once in
a full workspace floor run (`no early fire: [IdleSweepApplied { spec_id:
"idleclose", action: "close" }]`) and passed in the next. Cause: the row captures
`base = service_streaming_now_ms()` *after* `create`, then asserts nothing fires
at `base + 500` against a `1000ms` idle timeout — so the effective margin is only
500ms, and `create` exceeding it under workspace-wide build-and-test load makes
the quiet tunnel fire early. **Evidence it is not a regression from this plan:**
`git diff` shows this plan touches neither that row nor `distinct_ports`; the row
passed in this plan's own earlier floor run; and it passed 6/6 in isolation and
6/6 across full `i2pr-daemon --lib` runs (577 tests each).

This is **distinct from Plan 351's F7**, which is a *port-collision* flake caused
by `distinct_ports` computing a guess (`25_000 + (pid % 5_000)`, then `+ n*37`)
instead of binding an ephemeral port. Both are in the same test module and both
are pre-existing. Neither is fixed here: fixing them means changing a test that
this plan has no reason to touch, and recording a fix as part of an outproxy
closure would obscure both the outproxy evidence and the pre-existing defect.
Both are candidates for their own plan.

## Limitations (explicit non-claims)

- **The live failover rotation is unproven.** A dead first endpoint and a live
  second one did not come up green within this pass. What *is* proven is the half
  that makes rotation safe — an unreachable outproxy fails typed and bounded —
  plus unit-pinned selection order and retry ceiling. Two rows were written and
  then **removed** rather than left in the file ungreen, and their names are
  recorded in `DOCUMENTED_ABSENCES` in the evidence checker so the absence is
  machine-checked rather than a comment. The cause was not diagnosed.
- **A live restart carrying a request is unproven.** The control-plane half
  (credential and block survive a generation round trip) is proven.
- **No interoperability evidence.** Nothing here ran against Java I2P
  `2.13.0` / `9134f808…` or i2pd `2.61.0` / `635b013a…`. The reference pins are
  untouched.
- **SOCKS4a with an outproxy is not enabled.** SOCKS4a has no authentication
  stage, so a provider needing a credential cannot be used there; the classifier's
  refusal is the correct answer and the capability is not exposed.
- **The outproxy fixture speaks the HTTP CONNECT dialect only.** The SOCKS5 and
  SOCKS4a *dialects* are decoded by `interpret_reply` and unit-pinned, but no lane
  row exercises a SOCKS-dialect outproxy end to end.
- **No `OutproxyType` beyond `http` is proven at runtime**; the other two are
  refused at the control boundary as outside the shipped subset.
- **Type 5 (ELS2) stays `advertised = false`** and is untouched by this plan.

## Roadmap disposition and unblock audit

- **Plan 327** remains **blocked**, and this record does not unblock it. Its
  remaining scope needed an outproxy that actually answers. A loopback fixture
  is not that. The two unproven rows above (live rotation, live restart) are
  exactly the kind of thing a mixed-router lane would exercise, so they belong in
  a future interop lane rather than being asserted from a loopback fixture.
- **Plan 342** closes with the status token at the top of this record. The token
  names the gap (`live-failover-rotation-unproven`) rather than claiming more than
  the evidence supports, because a bare `passed` here would misreport the plan's
  own "Evidence" section.
- **Plan 326 / 335** (Encrypted LeaseSet2, Red25519) and the type-11 ecosystem
  split are untouched. ADR 0032's ownership of the ELS2 type-11 profile is
  unaffected; this plan adds no type-11 verifier.
- **No advertisement change.** No capability, version, RouterInfo, SAM, or I2CP
  behaviour beyond the tested subset is claimed.

## Supersession

Nothing is superseded by this record. The one status token that *was* wrong is
Plan 342's own: the plan document recorded `in-progress-option-surface-and-request-paths-landed-awaits-self-composed-wire-lane`,
which described the integration as landed when it was unreachable. That token was
corrected forward to the one at the top of this record; the pre-fix token is
retained in the plan document's history rather than deleted, per the repo's rule
that historical records are superseded and not rewritten.