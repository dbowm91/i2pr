# Plan 343 — I2P-routed outproxy provider policy and route owner

Status: **passed-outproxy-provider-policy-and-route-owner-with-no-reachable-request-path**

Implementation commits: `f9d404a` (runtime-neutral policy), `4dc69d0` (daemon
route owner), plus the static-guard and record commit.

Plan document:
[`plans/implementation/i2pcontrol-proposal-170/343-i2p-routed-outproxy-provider-policy-and-route-owner.md`](../../implementation/i2pcontrol-proposal-170/343-i2p-routed-outproxy-provider-policy-and-route-owner.md)

Reference:
[`specs/references/proposal-170-outproxy-provider.md`](../../../specs/references/proposal-170-outproxy-provider.md)

## Scope actually delivered, and what is deliberately not

Plan 342 registered this whole line: provider, canonical fields, request paths,
and wire evidence. Implementing it showed the work is two separable pieces, and
this record closes only the first.

**Delivered and closed here:** the provider itself, in both halves — the
runtime-neutral policy and the daemon route owner, plus the static guards that
keep the central invariant true.

**Not delivered, still registered as Plan 342:** the seven Proposal 170 option
fields, the HTTP and SOCKS request-path integration, and the loopback outproxy
wire lane.

**No option can set an outproxy yet, and no request path consults the
provider.** The code is reachable only from its own tests. This is provider
infrastructure and is **not** a capability claim, and `specs/support.toml` and
every advertisement surface are untouched. Per the planning rules, a plan whose
user-visible capability is not wired is not presented as one.

Plan 327's status token is deliberately unchanged. It is still blocked; the
blocker list shrinks again (below), but the provider is not reachable by a
client, so nothing observable changed for one.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| A typed, closed outproxy dialect vocabulary that is not a provider name or an executable. | `i2pr_service_tunnels::outproxy::OutproxyType::parse`; `outproxy_type_is_a_closed_vocabulary` | Passed. 17 spellings accepted across three dialects; `""`, `"none"`, `"ssh"`, `"curl"`, `"/bin/sh"`, `"http://x"`, `"tor"` all refused. |
| Every outproxy endpoint is an I2P destination, never a clearnet host. | `OutproxyEndpoint::parse`; `a_clearnet_or_ip_outproxy_entry_is_refused` | Passed. `example.com`, `127.0.0.1`, `example.com:9050`, `http://proxy.i2p/`, `user@proxy.i2p`, `proxy.i2p:8888`, `localhost`, `.i2p`, and blank all refused. This is what makes the no-direct-clearnet invariant structural rather than procedural. |
| Bounded, operator-ordered outproxy list with rotation. | `OutproxyList::parse` / `select`; `an_i2p_outproxy_list_parses_in_operator_order`; `duplicate_and_oversize_lists_are_refused` | Passed. Order preserved, rotation wraps, duplicates refused rather than silently collapsed, entry count and total length capped. |
| An `.i2p` target is routed directly and never selected for an outproxy. | `OutproxyConfig::route` / `route_attempt`; `an_i2p_target_bypasses_the_outproxy_entirely`; `a_configured_provider_selects_and_never_prints_a_secret` | Passed. Verified at attempt 0, 1, and 99. The bypass is re-checked inside `open_via_outproxy` too, not trusted from the caller. |
| The direct path's `.i2p`-only grammar is not weakened. | `http::target::absolute_form_rejects_clearnet` and `authority_form_rejects_clearnet` (pre-existing, re-run green); `OutproxyTarget` is a separate grammar | Passed. `OutproxyTarget` shares no parse result with the HTTP target parser. |
| A clearnet target is an opaque label; i2pr performs no resolution. | `validate_outproxy_host`; `an_outproxy_target_rejects_ip_literals_and_odd_hosts` | Passed. 16 bad forms refused, including IP literals, bracketed IPv6, a trailing root dot, empty labels, leading/trailing hyphens, non-DNS bytes, and mixed `.i2p`/clearnet spellings. No resolver type exists in either file, which rules 9 and 10 check statically. |
| Bounded selection, retry, backoff, and connect timeout. | `OutproxyPolicy::new` / `backoff_ms`; `the_policy_clamps_every_operator_input`; `backoff_is_bounded_and_saturating` | Passed. 1000 attempts clamp to 4, 10^6 ms clamps to 120 000 ms, zero floors to 1. `backoff_ms(usize::MAX)` saturates at the ceiling rather than wrapping, and the ramp is monotone. |
| A configured credential is required to be present, and is recovered only through Plan 341's owner. | `RouterOutproxyProvider::auth_header`; `a_credential_that_cannot_be_recovered_fails_closed`; `an_armed_credential_without_a_stored_form_is_refused` | Passed. An armed credential that cannot be opened is an error, never an unauthenticated request. |
| Secrets are never printed, copied, or heap-allocated. | `OutboundSecret` / `OutproxyAuthHeader` have no `Debug`/`Display`/`Clone`; `the_auth_header_is_built_and_never_printable`; `the_auth_header_refuses_usernames_that_could_split_the_pair` | Passed. The header round-trips through `decode_basic_credentials` and contains neither the username nor the password in the clear. A username with `:`, whitespace, or a control byte is refused at build time. |
| The route owner opens only an I2P Streaming connection, with bounded handshake. | `open_via_outproxy` / `open_one`; `an_incomplete_reply_is_not_yet_malformed`; rules 9-11 of `scripts/check-service-tunnel-boundaries.sh` | Passed. The only socket-shaped call is a Streaming connect to a `DestinationRef` `OutproxyEndpoint::parse` already validated. Connect timeout, handshake deadline, poll interval, and staging buffer are all bounded. |
| A proxy that pipelines payload with its `CONNECT` response does not lose the first request. | `an_http_attempt_carries_the_credential_and_keeps_tunnelled_bytes`; `a_socks5_domain_reply_is_length_framed` | Passed. Bytes after the response head become the session's `tunnel_prefix`; a SOCKS5 DOMAINNAME reply is framed by its own length byte. |
| Every codec error maps to a typed failure, total over the enum. | `classify`; `classification_is_total_over_every_codec_error`; `every_outproxy_error_maps_to_a_typed_failure_reason` | Passed. All twelve `OutproxyError` variants have a `reason()` and a `classify` arm, so a new codec error cannot be silently reported as a dead outproxy. |
| No plugin loading and no command execution. | Rule 11 of `scripts/check-service-tunnel-boundaries.sh`; two inversions below | Passed. |
| Status/counter surfaces carry no secret and no operator value. | `OutproxyCounters`; `counters_are_sanitized_and_additive`; `outproxy_errors_carry_a_stable_reason_and_no_secret` | Passed. Counts only; a `Debug` of the struct contains no hostname and no username. |

## Teeth — every guard was shown to fail when its fix is disabled

| Inversion | Edit | Result |
|---|---|---|
| Direct-clearnet socket | Added a `&std::net::TcpStream` parameter to `note` in `outproxy_route.rs` | **Failed closed.** `outproxy source ... names a direct-clearnet capability`, exit 1. |
| Plugin load, process spelling | Added `fn _unused_plugin_load(c: &std::process::Command)` to `outproxy.rs` | **Failed closed.** `outproxy source must not load a plugin or spawn a process`, exit 1. |
| Plugin load, `dlopen` spelling | Added `fn _unused_dlopen()` to `outproxy_route.rs` | **Failed closed.** Same rule, exit 1. |
| Positive control for rule 9 | Ran the rule-9 pattern over `service_tunnels_http.rs` | **Matched**, so rule 9 is not vacuous. |

Rule 9's pattern deliberately does **not** match `std::net::IpAddr`: parsing an
address is how the target grammar *refuses* IP literals, which is the opposite
of opening a socket. The first draft of the rule matched `std::net::` and
correctly failed on the real source; narrowing it was the right fix, and the
distinction is recorded here so a later reader does not "helpfully" widen it back.

**A vacuous inversion, recorded rather than hidden.** The first attempt at the
`Command::new` inversion targeted `is_complete` in `outproxy.rs`; that function
lives in `outproxy_route.rs`, so the edit silently no-opped and the checker
correctly reported "passed". Had the edit been believed applied, this table would
have claimed evidence for a guard that was never exercised. The inversion was
re-run with an asserted anchor in the correct file and both plugin-load
spellings now fail closed. The lesson is recorded rather than the mistake: an
inversion harness must verify its edit applied.

## Defects found in this plan's own code, by this plan's own rows

Four, all fixed at source. They are listed because each is a case where the first
draft of the design was wrong and the row is what caught it.

1. **The `.i2p` bypass was unreachable.** The first grammar refused `.i2p` hosts
   outright, so `OutproxyTarget::is_i2p()` could never return true through the
   public constructor and the bypass became a property of call sites rather than
   of the type. Fixed by accepting both grammars and moving the refusal into
   `OutproxyConfig::route`, where it is enforced.

2. **A `.b32.i2p` endpoint was identified by its bare label.**
   `DestinationRef::as_str` returns the Base32 *label* without the suffix, so
   `OutproxyEndpoint::as_str` returned a string that is not a routable spelling.
   Fixed by using `canonical_string`.

3. **The response parser accepted `HTTP/1.0` to a `CONNECT`.** RFC 9110 §9.3.6
   requires HTTP/1.1, and accepting the prefix would let a 1.0 response pass as a
   tunnel grant. Now exactly `HTTP/1.1`.

4. **SOCKS5 reply framing was two bytes short for IPv4 and IPv6.**
   `socks5_reply_len` omitted the mandatory 2-byte port on those two arms, so
   the length-driven framing would have treated a truncated reply as complete and
   sliced tunnelled payload into the handshake parser. Found by
   `an_incomplete_reply_is_not_yet_malformed`, whose own fixture was one byte
   short and had to be corrected at the same time.

Two design mistakes were also caught before they became defects: an error
variant that carried a truncated operator value via `Box::leak` (a leak per
error, and unbounded wire input in an error that reaches logs) was removed in
favour of carrying no value at all; and a first backoff formula,
`ceiling * 2^n / 2^n`, was algebraically just `ceiling` and degenerated to 1
under saturation, which is what forced the linear ramp.

## Verification run

| Command | Outcome |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo check --locked --workspace --all-targets` | pass |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | pass |
| `cargo test --locked -p i2pr-service-tunnels` | 339 passed, 0 failed (was 313; +26) |
| `cargo test --locked -p i2pr-daemon --lib` | 549 passed, 0 failed (was 538; +11) |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | see the floor section below |
| `bash scripts/check-dependency-direction.sh` | pass |
| `bash scripts/check-runtime-boundaries.sh` | pass |
| `bash scripts/check-service-tunnel-boundaries.sh` | pass, including new rules 9-11 |
| `python3 scripts/check-global-plan-number-uniqueness.py` | pass |

All commands above were run locally on Linux. No hosted CI run backs this record.

**Evidence-ordering caveat, stated rather than glossed.** The full serial floor
ran at the implementation state (`4dc69d0` plus the guard script); the record
files in this commit touch no `crates/*/src`, so they cannot affect the result.
The floor was not re-run at the exact closure commit.

## Invariant, failure, migration, and security review

- **Dependency direction unchanged.** No new crate edge and no manifest change.
  The capability trait lives in `i2pr-service-tunnels` and the AEAD use in the
  daemon, which is the split `check-dependency-direction.sh` forces; both files
  compile under the existing allowlist.
- **No unbounded primitive.** No channel, queue, buffer growth, or retry is
  unbounded. `OutproxyWireBuffer` and `OutproxyAuthHeader` are fixed-size arrays
  with compile-time ceilings, and the handshake read is capped by both
  `MAX_OUTPROXY_HANDSHAKE_BYTES` and a deadline.
- **Cancellation and close are lifecycle events.** `open_via_outproxy` checks
  cancellation between attempts and inside the handshake loop, and every failure
  path after a connection exists calls `terminate`, which reads the port tuple
  from the connection so a close cannot be redirected.
- **No advertisement change.** `specs/support.toml` untouched; no RouterInfo,
  SAM, I2CP, or capability surface changed. Outproxy participation is not
  advertised, not claimed, and not reachable.
- **Migration.** None. No persisted format changed; the new types are new and
  nothing writes them yet.
- **Secrets.** No `Debug`/`Display`/`Clone` on a secret-bearing type, no heap
  secret, no plaintext in a log, error, or `Debug`, and the username is refused
  a `:` or control byte so it cannot alter the Basic pair framing. No secret
  bytes appear in any test assertion, only in round-trip equality checks.
- **No clearnet capability.** The central invariant, enforced structurally by
  `OutproxyEndpoint::parse`, behaviourally by the typed refusal of a
  providerless clearnet target, and statically by rules 9-11.

## Findings by severity

Critical 0. High 0. Medium 0. Low 1.

- **Low — the outproxy is not reachable from any request path.** This is a
  scope boundary, not a defect: the provider is correct and tested but
  unreachable, so it provides no egress. Recorded so no reader infers a
  capability from the code's presence.

Out of scope and deliberately unchanged: the `rand_chacha` duplicate pin in
`i2pr-daemon`, the `parse_configured_destination` `priv` substring rejection, and
the two observations noted in Plan 341.

## Roadmap disposition and unblock audit

Plan 342 remains **registered** and is now narrower in one respect and unchanged
in others: the provider it was waiting for exists, but the option surface, the
request-path integration, and the wire lane are all still open. No other
registered plan lists Plan 343 as a hard dependency, so nothing else moves to
ready from this closure.

Plan 327 stays **blocked**. Its blocker list is now a single item: no request
path performs outproxy semantics and no Proposal option configures one. Plan 328
remains blocked on 326 and 327.
