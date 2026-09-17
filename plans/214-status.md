# Plan 214 status — M10 HTTP/IRC product-only external requalification and final closure

Status: **`local-pass-proven-hosted-double-pass-pending`**.

Plan of record: [`214-m10-http-irc-product-only-external-requalification-evidence-hardening-and-final-closure.md`](214-m10-http-irc-product-only-external-requalification-evidence-hardening-and-final-closure.md).

## Why this plan exists

Plan 211 retains useful real HTTP/IRC service specs and external clients, but its current evidence path is not yet terminal authority. The current driver still directly constructs a placeholder `ServiceTunnelManager`, stamps pin/privacy facts, infers some target observations from client-side results, uses an insufficient DCC-block inference, and runs blocking subprocesses without guaranteed concurrent `ServiceProduct::poll_inbound()` progress.

Plan 214 owns the final application-profile qualification hardening and exact-head closure after Plan 213 proves the generic router-backed product.

## Execution graph

```text
Plan 212 source closure
  -> Plan 213 generic Direction A+B harness completion + external proof (passed)
  -> Plan 214 HTTP + IRC product-only requalification (source landed; hosted lane pending)
  -> M10 final closure
```

## Source side landed (Commits A–D, Plan 214 §25)

- **Commit A (black-box driver + concurrent pump + runner pins).**
  The counted driver
  (`crates/i2pr-daemon/tests/service_tunnels_application_product_only_remote_qualification.rs::m10_product_only_remote_http_and_irc_application_interop_v214`)
  no longer constructs `ServiceTunnelManager` (the runtime-neutral
  `ServiceTunnelSet::validate` is the only preflight) and consumes
  only `ServiceProduct::start` / `poll_inbound` /
  `remote_counters` / `inbound_orphan_receives` /
  `service_router_network_summary` plus the bounded config
  surface. Every external subprocess runs through the bounded
  `run_pumped` helper (non-blocking `try_wait` + concurrent
  `poll_inbound`, deadline, child kill on timeout, post-exit
  drain). Pin facts are runner-verified preconditions
  (`PLAN214_I2PD_PIN_OK` / `PLAN214_JARACO_PIN_OK`); the driver
  stamps no literal pin row.
- **Commit B (HTTP target-side facts).** Actual curl
  `%{http_code}` status via `parse_curl_status_code` (never body
  length); fresh per-request fixture records gated by monotonic
  `seq` baselines (`http-get-fixture-method/path`); POST
  sent-digest == target-observed-digest plus the
  `posted=<len>` response contract; large response gated by
  equality with the fixture-published contract the runner
  captures at fixture startup; clearnet/IP-literal rejections
  prove zero fresh target records.
- **Commit C (IRC fixture-derived facts).** Target-observed
  NICK/USER registration, `user-privacy` booleans (b32 present,
  loopback/nodename absent — no PII uploaded), target-issued
  PING + target-observed matching PONG, token PRIVMSG observed
  on both sides with `irc-privmsg-token-match`, target-observed
  ACTION, and an explicit DCC attempt with
  `irc-dcc-policy-result` + target non-observation (absence-only
  inference rejected). The `irc_driver.py` client carries the
  deterministic `--token` and emits `ECHO_TOKEN_MATCH` +
  `DCC_ATTEMPTED`; the `irc_fixture.py` streams facts mid-run,
  follows the client's JOIN channel for the echo, and preserves
  the fixed `echo-hello` marker the Plan 181 local rows gate on.
- **Commit D (runner + checker + workflow).** New standalone
  `tests/integration/service-tunnels/run-plan214-applications.sh`
  (fresh evidence dir, command-derived pin verification, fixture
  contract capture, i2pd HTTP+IRC server tunnels, public-only
  destination parsing with hash/b32 consistency, counted driver
  invocation, §16/§17 row validation with duplicate-key
  rejection, `plan214-remote-http-eepsite` /
  `plan214-remote-irc-service` aggregates, exactly one `P214-*`
  terminal classification, resource baseline, no-secret + .dat
  audit, evidence.json). `run-independent.sh` keeps the retained
  local matrix and delegates remote qualification to this runner
  (mapping its aggregates onto `remote-independent-*`); the
  historical §6.3/Plan 202/207/208/211/212 inline blocks are
  removed. `scripts/check-service-tunnel-acceptance-evidence.sh`
  extended with the Plan 214 §28 invariants (driver black-box +
  literal-row + pump + window + subfact rules, runner pin/fixture/
  aggregate/classification rules, workflow ordering, synthetic-key
  extraction-boundary self-test). The hosted workflow orders
  Plan 213 → M10 matrix (delegating Plan 214) → final checker and
  uploads the Plan 214 evidence.

Focused unit floor: 21 `plan214_*` rows in the driver
(`plan214_evidence_unit_tests`) lock the §19 predicates plus the
§13 evidence-bound rule; the static checker requires at least 20.
The retained 29-row local M10 matrix is untouched (fixture changes
are additive: HTTP `seq` field, IRC streamed facts with preserved
`line`/`user` event shapes and the `echo-hello` echo marker;
fixture accept windows raised to a bounded 600 s — see Commit E).

## Commit E — product + harness correctives, first local pass

Worked through the exact closing head
`0238c08654f4e12de382efe1127471cb601260b3` (Plan 213 closure).
The pre-corrective lane stopped at `P214-D-service-product-start`
(HTTP 400: non-local client references had no remote resolution
path) and then at the IRC leg. Three correctives landed; the lane
then produced the first local `P214-N-passed` (both aggregates
`passed`, zero failed rows, zero `remote-stop`) on HEAD plus the
working-tree correctives below (final closure SHA = the Commit E
commit; hosted double-pass runs there).

- **E1 — product: typed remote-mirror client resolution (Plan 214
  §C).** `ServiceTunnelManager::resolve_remote_client_target`
  resolves a non-local destination hash through the requesting
  service runtime's installed router-backed remote LeaseSet2
  mirror (keys exclusively from the validated record: signing key
  from the record Destination, static key from its usable X25519
  key; `None` on mirror miss / unknown service — no local
  fallback). The HTTP, SOCKS5, and IRC client executors fall
  through to it on `DestinationFailure::LookupRequired`. Three
  new manager-level unit rows
  (`plan214_resolve_remote_client_target_{miss,hit,unknown}`)
  lock miss/hit/unknown-service; the static checker pins the seam
  + all three executor fallbacks + the 3-row minimum. The
  driver's startup rows now prove exact-target addressing through
  the typed `RoutingDecision::RemoteRouter` (the summary's own
  hash is the local service destination, so hash comparison was
  the wrong probe).
- **E2 — harness: fixture lifetimes for the remote lane.** The
  IRC fixture's single-accept window (60 s) and the HTTP
  fixture's per-accept window (90 s) expired before the remote
  lane's first connection (i2pd boot + provisioning + HTTP
  phases take minutes); i2pd logged `Connect error ...
  Connection refused` and closed the stream. Both windows are
  now a bounded 600 s (happy-path behavior unchanged; no
  checker/fixture-shape coupling exists).
- **E3 — harness: transparent i2pd server tunnel for the IRC leg
  (upstream line-mangling isolation, proven by packet tap).**
  With `type = irc`, the fixture observed stray bare `\n` lines,
  a duplicated bare `QUIT`, and missed the token PRIVMSG even
  though jaraco's bytes (captured directly), i2pr's filter/pump
  path (byte-exact in the local burst probe), and the
  HTTP-leg digests were all clean. A loopback `tcpdump` tap on
  the i2pd→fixture TCP stream proved the extras present on the
  wire (`...\r\n\n` after nearly every line). i2pd's
  `I2PTunnelConnectionIRC::Write re-emits every received chunk
  line-by-line (`getline` + bare `\n`, partial tails emitted
  early), so any stream chunking corrupts the application
  stream; i2pr's transport legitimately chunks as a byte stream.
  The A/B is decisive: identical i2pr bytes arrive byte-exact
  (zero strays, single `QUIT :done`) through `type = server`,
  which forwards transparently. i2pd is the
  encrypted-transport endpoint here, not the IRC application
  counterpart (that is the harness fixture); every IRC
  application semantic under test (registration, neutral USER
  privacy rewrite, PING/PONG, token PRIVMSG echo, ACTION pass,
  DCC block) is enforced by the i2pr IRC client profile and
  observed at the fixture either way. The runner provisions
  `type = server` with this justification inline. Deliberately
  NOT done: making the fixture `\n`-tolerant would silently
  absorb a genuine ours-side byte defect — rejected as
  test-weakening.
- **E4 — driver proof + hygiene.** The IRC privacy row now
  proves OUR documented contract from target facts (neutral
  shape `i2p localhost` in the observed USER line — jaraco sends
  `0 *`, so its presence proves our rewrite — plus
  loopback/nodename absence) instead of the b32 hostname the old
  row attested, which was i2pd's server-tunnel rewrite masking
  ours. `append_evidence` truncates values past 350 chars
  (`...[truncated]`, char-boundary safe) so failure excerpts
  can never trip the runner's 400-char overlong-token audit;
  one new unit row locks it (21 total).

## Current authority

```text
plan_211 = retained-source-harness-superseded-for-final-evidence-by-plan214
plan_212 = passed-source-and-generic-external-qualification-via-plan213
plan_213 = passed-m10-router-backed-generic-external-qualification
plan_214 = local-pass-proven-hosted-double-pass-pending

m10_local_rows = passed (29/29 retained)
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
m10_remote_application_interop = locally-passed-once (P214-N, Commit E tree)
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

## What remains (Commit F, Plan 214 §25)

1. Local exact-head validation on the Commit E SHA (§20 source
   side: fmt, check, workspace tests, clippy, doc, boundary +
   evidence checkers, `run-independent.sh --local-only`,
   `run-plan213-generic.sh`, full `run-independent.sh` incl. the
   Plan 214 delegation as the stability second pass).
2. Hosted `full` service-tunnels workflow twice consecutively on
   the same exact source SHA, each producing `P214-N-passed`
   with both aggregate rows command-, target-, and
   counter-derived (§21, §24 items 38–40).

Only then do the §26 authority transitions land
(`plan_214 = passed-...`, `m10_remote_application_interop`,
`milestone10_remote_service_interop`, `milestone10_final_acceptance`).
No file claims Java M6 second-family closure as a consequence of
M10 progress.
