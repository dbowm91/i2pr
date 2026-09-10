# Plan 181 status — M10 independent application/service interoperability

Status: **`blocked-by-m6-mixed-router-streaming-blocker`**
(local matrix green; remote gate pending the Plan 183 program).

Plan of record:
[`plans/181-m10-independent-application-and-service-interop-final-closure.md`](181-m10-independent-application-and-service-interop-final-closure.md).

## Current authority

```text
plan_173 = registered-m10-service-tunnels-roadmap
plan_174 = passed-m10-service-tunnel-foundation-and-shared-stream-runtime
plan_175 = passed-m10-generic-client-server-service-tunnels
plan_176 = passed-m10-http-i2p-proxy-and-connect
plan_177 = passed-m10-socks5-i2p-connect-proxy
plan_178 = passed-m10-irc-client-profile-and-privacy-filtering
plan_179 = passed-m10-irc-server-profile-and-authenticated-peer-hostname
plan_180 = passed-m10-service-tunnel-composition-reconcile-and-hardening
plan_181 = blocked-by-m6-mixed-router-streaming-blocker
plan_182 = passed-m10-local-delivery-corrective
plan_183 = registered-m6-mixed-router-streaming-interop-program
milestone10_planning_authority = plan173
milestone10_foundation = passed-via-plan174
milestone10_generic_tunnels = passed-via-plan175
milestone10_http_proxy = passed-via-plan176
milestone10_socks5 = passed-via-plan177
milestone10_irc_client = passed-via-plan178
milestone10_irc_server = passed-via-plan179
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_local_roundtrip = passed-via-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 183
next_product_layer = m6-mixed-router-streaming-interop
```

## What landed (Plan 181 §§7/9/10 deliverables)

```text
tests/integration/service-tunnels/run-independent.sh (new)
  External lane runner: prerequisite-status gate, tool/pin
  verification, static boundary checker, focused Rust suites
  (final-acceptance/adversarial with per-test ok-line rows,
  Plan 182 round-trip, wire surface), two example-listener
  generations (echo-backed generic rows; HTTP/IRC-backed
  application rows), unmodified curl HTTP/SOCKS rows, nc +
  stdlib generic rows, exact-pinned jaraco/irc venv rows,
  restart-stability row, i2pd remote qualification section,
  resource-baseline row, unsupported-profile ledger row,
  classified evidence.json/evidence.md emission. record_guarded
  is the only pass path; remote rows use record_blocked only.

tests/integration/service-tunnels/fixtures/{http,echo,irc}_fixture.py (new)
  Loopback-only stdlib fixtures with per-request flushed JSON
  facts (digests/lengths/policy facts only, never bodies).

tests/integration/service-tunnels/clients/{irc,generic}_driver.py (new)
  jaraco/irc public-`irc.client`-API-only driver (register,
  CAP-where-exposed, join, privmsg, ACTION, DCC-probe,
  PING/PONG, quit); opaque stdlib generic driver (small,
  large, half-close, siblings; no profiled-tunnel framing).

scripts/interop/fetch-service-tunnel-clients.sh (new)
  Exact-pin jaraco/irc fetch + clean-checkout (no-patch)
  enforcement into the disposable target/interop cache.

scripts/check-service-tunnel-acceptance-evidence.sh (new)
  Routine-CI static checker: 29 command-derived rows, 2
  blocked-only remote rows, no literal passes, pin/head/
  cleanliness gates, curl/--socks5-hostname/jaraco-API gates,
  generic-driver opacity gate, ignored-exact remote selection,
  i2pd pin + SAM PUB provenance + unknown-peer gates,
  loopback/forgiveness/secrets/cleanup gates.

.github/workflows/service-tunnels-external.yml (new)
  Manual dispatch lane (full/local-only): toolchain 1.95.0,
  i2pd build deps + netcat exception, both fetch scripts,
  evidence checker, matrix run, sanitized artifact upload.

crates/i2pr-daemon/tests/service_tunnels_remote_qualification.rs (new)
  Ignored-by-default qualification driver: one M10 connect to
  the i2pd-generated destination, asserting the §6.3 stop
  condition (no establishment, unknown_peer > 0, delivered 0,
  counted failure); fail-closed without I2PD_PEER_PUB_B64;
  never logs key material.
```

## Local evidence (full lane, 29 passed rows)

Representative passing full-lane verdict (31 rows: 29 passed,
2 blocked; exit nonzero fail-closed per the stop condition):

- `m10-prerequisite-plans`, `m10-tool-pin-verification`
  (curl 8.5.0, OpenBSD netcat, python 3.12, jaraco/irc
  `90e10e690da2c7bf60de21be4e36d24c9ffd7474` verified clean),
  `m10-foundation-boundary-checks`,
  `m10-local-final-product`,
  `reconcile-rollback-local`,
  `cross-service-resource-bounds`,
  `m10-local-roundtrip-suite`,
  `m10-wire-surface-suite`;
- `generic-small-independent` (nc exact bytes),
  `generic-large-independent` (98 304 B digest),
  `generic-half-close-independent` (prompt EOF),
  `generic-siblings-independent` (isolated pair),
  `server-identity-restart-stable` (identical b64 across
  process restart);
- `curl-http-get` (200 + body digest),
  `curl-http-post` (posted-body digest equality),
  `curl-http-large` (65 536 B),
  `curl-http-connect` (:443 opaque 200+digest with :80
  contrast 403),
  `http-clearnet-rejected` (403),
  `http-unknown-i2p-bounded` (400/502);
- `curl-socks5-domainname` (`--socks5-hostname` DOMAINNAME
  CONNECT body digest; `.i2p` success proves no local DNS),
  `socks-clearnet-ip-rejected` (IPv4 ATYP fail-closed);
- `irc-venv-install` (unmodified pin installed from verified
  source), `irc-independent-register` (public-API
  connect/register/welcome),
  `irc-independent-message-roundtrip` (PRIVMSG echo +
  PING/PONG), `irc-user-hostname-authenticated-destination`
  (fixture USER hostname is the `<52-char
  b32>.b32.i2p` projection, client-supplied hostname absent),
  `irc-ctcp-policy-local` (ACTION passes, DCC absent);
- `external-clean-resource-baseline` (no listener/i2pd
  process, loopback ports refused),
  `unsupported-profile-ledger`
  (`specs/CONFORMANCE.md` M10 ledger).

Privacy-header facts are recorded as observed values
(User-Agent `i2pr/0.1`, no Referer/From), never raw headers.

## Remote qualification attempt (§6.1 execution, §6.3 verdict)

Independent reference (exact-pinned, unmodified, loopback-only):

```text
i2pd 2.61.0
commit = 635b013a612ff47278ef02acf8580a28e10e26c5
```

Ephemeral i2pd provisioned with a fresh datadir (no reseed, no
transit, SAM on 127.0.0.1). One SAM session:
`HELLO VERSION MIN=3.1 MAX=3.1` → `RESULT=OK`, then
`DEST GENERATE SIGNATURE_TYPE=7` → `DEST REPLY PUB=<524-char
Ed25519 destination> PRIV=<withheld, never logged>` (i2pd
deviation recorded: no `RESULT=OK` token on DEST replies;
success is `DEST REPLY PUB=...`).

Qualification driver
(`service_tunnels_remote_qualification`, explicit
`--ignored --exact` selection, `I2PD_PEER_PUB_B64` from the SAM
transcript):

```text
REMOTE_QUALIFY_ESTABLISHED=0
REMOTE_QUALIFY_UNKNOWN_PEER=1
REMOTE_QUALIFY_DELIVERED=0
REMOTE_QUALIFY_FAILED_CONNECTS=1
test result: ok (the blocker, asserted as specified)
```

Classification: **`m6-mixed-router-streaming-blocker`** —
the independent destination parses as structurally valid, zero
routes exist to non-local destinations on the local product
(`unknown_peer`), nothing is delivered, nothing establishes,
and the attempt fails closed inside a bounded timeout. Both
remote rows (`remote-independent-http-eepsite`,
`remote-independent-irc-service`) are recorded `blocked` with
this provenance; self-composed rows are never substituted.

## Stop conditions (Plan 181 §§15/16)

Milestone 10 does NOT close in this plan:

- remote independent HTTP/IRC service rows are absent as
  passes (present as `blocked` with §6.3 provenance);
- no patched reference router was used (exact pins verified,
  clean checkouts enforced);
- no mixed-router failure was relabeled as an M10 issue;
- no public-network workaround was introduced;
- routine + external exact-head CI gating continues below.

The single registered follow-up is Plan 183 (M6
mixed-router destination/Streaming interop program); Plan 181
resumes only after it produces passing remote rows.

## Handoff

```text
plan_181 = blocked-by-m6-mixed-router-streaming-blocker
milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 183
```

Closing implementation heads and exact-head CI run IDs are
recorded in the follow-up closure pointer commit once routine
CI is green (same practice as prior plans).
