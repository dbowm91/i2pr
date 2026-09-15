# Plan 207 status — genuine remote HTTP/IRC application interoperability corrective

Status: **`passed-m10-genuine-remote-http-and-irc-application-interop`**.

Plan of record: [`207-m10-genuine-remote-http-and-irc-application-interop-corrective.md`](207-m10-genuine-remote-http-and-irc-application-interop-corrective.md).

Plan 207 replaces the synthetic Plan 203 acceptance pattern with
command-derived evidence from unmodified application clients.
The Plan 203 driver advanced the documented observation labels
through the manager's free `record_remote_application_observation`
helper, so the aggregate pass rows could exist without satisfying
the original Plan 203 contract. The Plan 207 driver instead
spawns real system `curl` subprocess invocations against the i2pr
HTTP client listener and real exact-pinned jaraco/irc public API
subprocess invocations against the i2pr IRC client listener; the
aggregate pass rows derive purely from the documented Plan 207 §9
subfact rows the driver writes to its evidence file.

Implementation status:

- Phase A — `tests/integration/service-tunnels/run-independent.sh`
  provisions one ephemeral exact-pinned i2pd 2.61.0 process on
  loopback with one HTTP server tunnel and one IRC server tunnel
  pointing at the harness-owned loopback fixtures (HTTP/IRC
  fixture ports passed through `--tunconf`); the runner starts
  the listener and exercises the production M10 path with the
  Plan 206 backend installed; the runner invokes the new
  `m10_genuine_remote_http_and_irc_application_interop` external
  driver through `--ignored --exact --nocapture --test-threads=1`
  on the same lane.
- Phase B — `run_curl_cases` in the new driver spawns the system
  `curl` binary as a subprocess for the GET/POST/large/clearnet
  cases; the captured exit code, HTTP status (derived from the
  body digest when the proxy strips the status line), body
  digest, request digest, multi-packet digest, and the bounded
  clearnet 403 reject are written to the driver-evidence TSV.
- Phase C — `run_irc_case` in the new driver spawns the
  exact-pinned jaraco/irc Python driver as a subprocess via
  `python -m venv` + `pip install --quiet $JARACO_SRC`; the
  documented jaraco facts (WELCOME/PONG_SENT/PRIVMSG_SENT/
  ECHO_RECEIVED/ACTION_SENT/DCC_SENT/QUIT_SENT) drive the
  documented Plan 207 §9 IRC subfact rows.
- Phase D — `finalize_aggregate_rows` derives the aggregate
  `http-remote-application-established` and
  `irc-remote-application-established` rows from the documented
  subfact rows; `record_remote_application_observation` is
  explicitly forbidden in the driver (the static checker
  rejects any call site).
- Phase E — `scripts/check-service-tunnel-acceptance-evidence.sh`
  extended with the Plan 207 §10 source-level invariants: the
  driver exists, is `#[ignore]`-gated, declares the documented
  `m10_genuine_remote_http_and_irc_application_interop` test
  name, exercises `RemoteDestinationBackend` +
  `install_router_delivery_handle` + `routing_decision_for`,
  asserts `RoutingDecision::RemoteRouter`, spawns the system
  `curl` binary as a subprocess, invokes the jaraco/irc public
  API as a subprocess, writes every documented Plan 207 §9
  subfact row to the evidence file, never logs peer key
  material, and never calls `record_remote_application_observation`.
- Phase F — runner semantics distinguish the local/routine lane
  (retained 29 local rows + the Plan 202 transport row; remote
  rows blocked with explicit lane provenance) from the full
  external lane (29 local rows + Plan 202 + Plan 207 with the
  aggregate remote rows derived from the Plan 207 §9 subfact
  rows in the same evidence directory/run id).

Current authority:

```text
plan_203 = retained-partial-m10-remote-application-evidence-scaffolding-superseded-by-plan207
plan_206 = passed-m10-production-remote-delivery-composition-corrective
plan_207 = passed-m10-genuine-remote-http-and-irc-application-interop
plan_181 = passed-m10-independent-application-and-service-interop-via-plan207
plan_195 = evidence-passed-m10-remote-independent-service-final-closure-pending-plan204-normalization
milestone10_remote_service_interop = evidence-passed-via-plan202-plan206-and-plan207-pending-plan204-normalization
```

Implementation commit: `c67dc5b...` (current head) — the Plan 207
driver test, runner script, and evidence checker landed in the
same change. The Plan 204 final authority/CI normalization pass
still owns the §7/§12 closed authority transitions; final
milestone closure remains blocked on Plan 201's terminal
`P200-{A..H}` classification + narrow corrective per
[`plans/204-status.md`](204-status.md).
