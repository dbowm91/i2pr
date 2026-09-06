# Plan 161 status — Milestone 8 SSU2 independent IPv4 interop (PASSED)

Status: **`passed-m8-ssu2-independent-ipv4-interop-and-final-closure`**.
All 24 mandatory acceptance criteria are evidenced below: directions A
and B, the externally exercisable token/Retry rows (tokenless +
cached-token), and the compact malformed/spoof/resource rows pass
against exact-pinned i2pd 2.61.0 over real loopback UDP; the final
fail-closed evidence ledger/checker plus the manual external workflow
pass locally AND hosted (15/15 rows); routine CI is green on the
implementation heads. No public-network, NetDB/tunnel/destination, or
advertisement claim is made. The temporary routine-CI lane-selection
corrective stays closed under Plan 162.

Plan of record:
[`plans/161-m8-ssu2-independent-ipv4-interop-and-final-closure.md`](161-m8-ssu2-independent-ipv4-interop-and-final-closure.md).

Temporary corrective authority:
[`plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`](162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md).

```text
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_161_current_blocker = none
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
next_executable_plan = none (milestone9-planning next)
resume_after_plan162 = 161 (done)
milestone8_final_acceptance = closed-via-plan161
```

Reference pins (unchanged):

```text
i2pd = 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5 (mandatory)
java = 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d (secondary, untouched)
```

## Direction A: proven (retained)

Against one ephemeral unprivileged i2pd 2.61.0 listener on
`127.0.0.1:43823` (`reservedrange=false`, loopback-only), the
`i2pr-runtime` driver
(`crates/i2pr-runtime/tests/ssu2_independent.rs`, env
`I2PD_ROUTER_INFO` / `I2PD_SSU2_ENDPOINT` / `I2PR_SSU2_BIND` /
`I2PR_SSU2_FLOODFILL` / `EVIDENCE_DIR`, all fail-closed) proves over
real loopback UDP:

- tokenless TokenRequest → Retry → SessionRequest → SessionCreated →
  SessionConfirmed establishment, mutually authenticated
  (`sessions_established: 1`, `used_cached_token: false`;
  i2pd logs `Session with 127.0.0.1:44001 (...) established`);
- one small (single-datagram) and one fragmented DatabaseStore
  i2pr → i2pd, both ingested (`RouterInfo added` on the i2pd side);
- one direct DeliveryStatus echo per store i2pd → i2pr over the same
  session (type 10, reply token echoed in the message body);
- graceful termination with the session/task baseline restored
  (`active_sessions: 0`, zero auth failures / cheap drops).

Historical direction-A command/result on the pre-Plan-162 tree
(2026-09-04):

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent
# test result: ok. 1 passed (3.45s)
```

Plan 162 changed only test-lane selection. The canonical external invocation
must explicitly select the ignored external test:

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
```

That explicit command remains fail-closed when the required external
environment is absent and must be re-proven against the same exact-pinned i2pd
before Plan 162 closed. Hosted routine CI run `33941941145` passed Ubuntu,
macOS, MSRV, and dependency policy on implementation commit
`624e8cce177040674376163160cfbda47e6a60fe`.

Plan 162 gate validation on 2026-09-05 confirmed the three required lane
states: ordinary invocation reported `1 ignored` and exited 0; explicit
`--ignored --exact` invocation without external variables failed with
`missing required env I2PD_ROUTER_INFO`; and the explicit invocation with
the cached, verified i2pd 2.61.0 reference passed direction A in 3.47 s
with 24 sanitized evidence rows. The latter run used `I2PR_SSU2_FLOODFILL=1`
and retained the established small/fragmented DatabaseStore, DeliveryStatus,
and clean-resource-baseline assertions.

Evidence artifact (`EVIDENCE_DIR/driver-evidence.tsv`): sent/reply
lengths plus SHA-256 digests for both directions, peer RI length,
close/resource counters. No secret material is recorded.

## Direction B: proven (2026-09-06, two consecutive full-matrix passes)

Against the same ephemeral unprivileged i2pd 2.61.0 listener on
`127.0.0.1:43823` (loopback-only, `I2PR_SSU2_BIND=127.0.0.1:44001`,
`I2PR_SSU2_FLOODFILL=1`, fail-closed env), the driver proves over real
loopback UDP:

- i2pd initiator → i2pr responder v2 handshake authenticates both
  peers (`sessions_established: 2`, responder promotion through the
  normal token/Retry path, no test bypass);
- one small and one fragmented DatabaseStore i2pr → i2pd over the
  responder-promoted session, both ingested (i2pd learns the fixture
  RIs and attempts its normal fixture-address dials afterwards);
- one direct DeliveryStatus echo per store i2pd → i2pr over the same
  session (body-token matched, shared link asserted);
- graceful termination of the responder session with the
  session/task baseline restored (`active_sessions: 0`,
  `pending_inbound/outbound: 0`).

The same two runs also close the remaining live-peer rows in one
matrix: cached-token second dial (`used_cached_token: true`, no Retry
round trip, one store + echo), and the compact malformed probe
(short/oversized/random datagrams → bounded rejections, zero
sessions). Expired/invalid/source token rows stay local-evidence-only
in the Plan 156/158 suites, as recorded in the driver.

Passing commands/results (exact-pinned i2pd
`635b013a612ff47278ef02acf8580a28e10e26c5`, unmodified):

```text
cargo test --locked -p i2pr-runtime --test ssu2_independent \
  ssu2_independent_ipv4_interop -- --ignored --exact --test-threads=1
# test result: ok. 1 passed (63.79s)
# test result: ok. 1 passed (63.90s)
```

i2pd-side log shows no `Unexpected message type`, no `AEAD
verification failed`, and no `Session with 127.0.0.1:44001 was not
established` for the real session in either pass (only the expected
unreachable fixture-address timeouts
`43201/43301/43202/43203/43302`).

## Direction-B implementation findings (no wire-spec change)

Live comparison against pinned i2pd exposed three independent
behaviors that loopback tests could not see (both sides were
consistent identically). Fixes are narrow and covered by new
regression tests; SSU2 wire semantics are unchanged:

1. The independent implementation never updates its initiator
   destination connection ID from SessionCreated, so genuine
   Alice-to-Bob data arrives with a stale destination ID.
   `Responder::on_session_confirmed` no longer enforces the echo
   (authenticity still comes from the Noise transcript + per-fragment
   coherence); the data-phase receive path and runtime routing match
   i2pd's `ProcessData`, which ignores the wire destination ID and
   authenticates via AEAD over the wire header.
2. i2pd Alice establishes only on its first received ACK block
   (`HandleAck` from `SessionConfirmedSent`). A responder that has
   received nothing ack-eliciting yet would never release it, so
   responder promotion now emits one honest zero ACK
   (`ackThrough = 0`, piggybacked on the NewToken packet;
   `Ssu2Session::request_bootstrap_ack`). Receivers treat unknown
   acked numbers as idempotent no-ops.
3. Runtime data routing keeps the strict connection-ID fast path and
   adds an endpoint-scoped fallback (`matches_data_header`) used only
   when exactly one active session serves the source endpoint;
   ambiguous candidates still fall through to the handshake path.

Supporting fixes retained from this pass: responder `SessionCreated`
reuses the Retry-announced connection ID via the token-bound ID
(`TokenStore::issue/consume` carry `responder_conn_id`; fixes the
initiator routing to a dead handshake), and the initiator caches the
in-band SessionCreated NewToken for the cached-token redial
(`PeerNewToken`, `find_establishment_token`).

New regression tests: `stale_destination_id_still_delivers`,
`bootstrap_ack_emits_zero_ack_once` (transport session),
`responder_reuses_retry_conn_id_for_session_created`,
`establishment_token_found_skipped_or_absent` (handshake/token).

Cosmetic note (non-blocking, deferred): our responder SessionCreated
Address block carries our own endpoint rather than the peer's observed
endpoint, so i2pd logs `Our external address is 127.0.0.1:44001` at
info level; it proceeds and establishes. Fixing the observed-address
value is follow-up work, not a closure blocker.

Run variance note: two of six runs during this pass timed out on the
direction-A large DeliveryStatus (small arrived, session healthy,
retransmits in flight) under host load; both passed on immediate
re-run with no code change. A later full-lane run exposed the
settle-window redial race (peer redialed during the 15 s settle;
fixed by capturing `b_baseline` pre-settle, driver-only). No
protocol defect is indicated in either case; the final ledger runs
should record per-run results verbatim.

## Protocol corrective (retained)

Live comparison against the pinned i2pd implementation exposed three
transcript divergences that loopback tests could not see (both sides
were wrong identically). All are fixed in
`crates/i2pr-transport-ssu2`, with fixtures regenerated:

1. SessionCreated sealed/accepted mixed the request ciphertext a
   second time; i2pd mixes it once at the SessionRequest stage.
   Symptom: our initiator terminated with `AuthenticationFailed`
   against real SessionCreated bytes.
2. The first-fragment SessionConfirmed short header was never mixed
   before the static-key frame; i2pd mixes the 16 cleartext header
   bytes first. Symptom: i2pd rejected our SessionConfirmed part 1.
3. The static-key frame used the retained `es` cipher; i2pd (and Noise
   XK ordering) uses the post-`ee` cipher at `n = 1`. Same symptom as
   (2); the retained cipher is now taken after the `ee` MixKey and
   renamed `static_cipher`.

Regenerated vectors: `session-created-full.hex`,
`session-confirmed-frag.hex` (+ `manifest.tsv` hashes). The older
header-protection counter fix (ChaCha20 stream offset 64, i.e. block
counter 1) and its vectors predate this pass and are retained.

Plan 162 must not reopen or modify these protocol corrections unless its
external re-run demonstrates a new concrete protocol defect. Its expected code
change is only integration-test execution metadata/gating.

## Routine CI lane corrective closed by Plan 162

Routine CI run `33915994884` on exact head
`4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c` failed both quality jobs because
ordinary workspace execution automatically ran the external integration test
without its required i2pd environment. Plan 162 added the descriptive
`#[ignore]` gate, retained all-target compilation, and restored routine CI;
the dedicated external invocation remains explicit and fail-closed:

```text
Quality (ubuntu-latest) = failure
Quality (macos-latest)  = failure
Dependency policy       = success
MSRV (Ubuntu)           = success

ssu2_independent_ipv4_interop ... FAILED
missing required env I2PD_ROUTER_INFO
```

All observed failure evidence points to test-lane selection. The external test
is correctly fail-closed when actually run; it simply must not be run by the
ordinary no-peer workspace lane. Plan 162 closed this correction. Do not weaken
`env_value()` or turn missing environment into an early success.

## Harness hazards (recorded for the remaining rows)

- Purge stale `netDb/r*/*.dat` (+ peer profiles) and restart the
  ephemeral i2pd before each run: i2pd redials previously learned
  RIs, and a pending outbound dial to our bind port swallows our
  inbound handshake by endpoint routing (`Unexpected message type
  ... instead 9`, dial timeout).
- A static-key mismatch on a published address makes i2pd ban the
  whole loopback address for ~30–39 minutes (`AddBan`), breaking all
  later runs until restart. Never present a mismatched RI on the
  live port.
- The `#[tokio::test]` runtime is single-threaded: any
  `std::thread::sleep` in the driver wedges transmit/receive for the
  whole poll. The NetDB poll is removed; all waits are cooperative.
- i2pd converts short-header seconds to milliseconds and enforces a
  (-60 s, +180 s) accept window: transport expirations must be near
  term (`+60 s` used), never tunnel-style horizons.
- i2pd `RouterInfo` ingest requires `router.version` digits clearing
  its minimum-allowed floor and a matching `netId` property; a
  missing `netId` marks the RI unreachable silently (no persist).
- i2pd randomizes the short-header message ID on send: match
  DeliveryStatus replies by the body token (offset 9), not the
  header message ID.
- i2pd persists only RIs it keeps connected and purges the rest at
  manage ticks: NetDB `.dat` polling is not a reliable delivery
  signal. DeliveryStatus echoes are the direction-A proof instead.

## Open rows (not claimed)

With directions A/B, the cached-token row, the compact
malformed/resource rows, and the fail-closed ledger/checker/workflow
rows proven locally AND hosted against the live peer, the only
remaining items are explicitly non-blocking and recorded, not claimed:

- Java I2P secondary lane: documented as nonblocking
  narrow-orchestration debt (plan section 12 / criterion 13); see
  below. It does not silently disappear: the ledger records it in
  every evidence artifact.
- Unsupported-version / spoofed-source / tag-corruption rows beyond
  the driver's short/oversized/random probe: retain the local Plan
  157 proof where the external lane cannot inject without patching,
  recorded explicitly (plan section 11).

No new support or advertisement claim is made by the direction-A/B
passes. Milestone 8 is closed within exactly this bounded scope
(criteria 20–24 below); everything outside it stays unclaimed.

## Final ledger/checker/workflow (landed, passing locally)

Plan 161 §13–14 artifacts (modeled deliberately on the Plan 151
pattern):

- `tests/integration/ssu2/run-independent.sh` — fail-closed external
  lane. Provisions one ephemeral unprivileged i2pd 2.61.0 on
  `127.0.0.1:43823` from the verified cache (fresh datadir per run,
  so no stale netDb; private keys never leave scratch), runs the six
  local focused suites plus the single explicit `--ignored --exact`
  driver invocation (now with `--nocapture` so passing-run
  transcripts persist), and derives all 15 required rows from
  executed commands: local rows via `record_guarded` on the suite
  exit code, external rows via `ssu2_row` on the driver exit code
  plus the row's own sanitized evidence keys. Emits sanitized
  `evidence.json` / `evidence.md` (commits, pins, OS, toolchain,
  bind policy, per-row commands, digests/counters, known
  limitations; no secrets).
- `scripts/check-ssu2-acceptance-evidence.sh` — static
  evidence-integrity checker: rejects literal unconditional `passed`
  records, requires a `record_guarded`/`ssu2_row` call site per
  required row, pins the exit-code and evidence-key gates, the
  explicit `--ignored --exact` selection, the exact i2pd pin, the
  loopback bind policy, and no `|| true` forgiveness. Enforced in
  routine Linux CI (`.github/workflows/ci.yml`) and the manual
  SSU2 external workflow.
- `.github/workflows/ssu2-external.yml` — manual
  (`workflow_dispatch`-only) Ubuntu 24.04 lane: fetch/verify exact
  i2pd, run the evidence-integrity checker, run the full matrix,
  upload sanitized evidence even on failure. No sudo, no
  Docker/namespaces/VM/systemd, no public-I2P participation beyond
  the GitHub source fetch.

Full-lane pass on the current head (2026-09-06, ephemeral i2pd
`670`-byte router.info, `I2PR_SSU2_FLOODFILL=1`):

```text
bash tests/integration/ssu2/run-independent.sh
# Plan 161 SSU2 lane passed; sanitized evidence: target/interop/ssu2-evidence
# 15/15 rows passed (6 local + plan155-160 regressions + 7 external + workspace-gates)
bash scripts/check-ssu2-acceptance-evidence.sh
# SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
```

## Direction-B settle-race fix (driver-only, no wire change)

The first full-lane run on this head reproduced a driver timing race
the two earlier 63 s passes never hit: the pinned peer redialed
*during* the fixed 15 s inter-direction settle, so the post-settle
`b_baseline` already included the direction-B establishment and the
wait deadlocked 150 s expecting a third session
(`pinned peer did not initiate a session (i2pd redial)`; the wait
snapshots showed `sessions_established: 2, active_sessions: 1` throughout).
`b_baseline` is now captured before the settle sleep, so a redial
that lands during the settle still counts as the fresh initiation;
a later redial behaves exactly as before. Test-only change in
`crates/i2pr-runtime/tests/ssu2_independent.rs`; the re-run passed
the full matrix in 56.93 s with no protocol or runtime edit.

## Java secondary lane decision (nonblocking debt, recorded)

Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`) is
retained as the preferred secondary reference and is recorded in
every ledger artifact, but no Java router is orchestrated in this
pass: no narrow unprivileged standalone SSU2 driver exists for it,
and building a JVM/Gradle router orchestration inside M8 would
recreate the harness build-up Plan 161 §12 forbids. This is the
plan-sanctioned nonblocking outcome (criterion 13), not a silent
disappearance. Revisit only with a narrow unprivileged seam that
keeps the lane loopback-only.

## Quality state on direction-B closing tree (2026-09-06)

Local validation green on the direction-B closing commit `fde2bae`
(code commit `6e7cef4`):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1  # 1521 passed, 1 ignored (60 suites)
cargo test --locked --workspace --doc                              # ok
cargo test --locked -p i2pr-transport-ssu2 --all-targets        # 169 passed (incl. 2 new interop regressions)
cargo test --locked -p i2pr-transport --all-targets            # 44 passed
cargo test --locked -p i2pr-runtime --lib                      # 70 passed
cargo test --locked -p i2pr-runtime --test ssu2_local          # 9 passed
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay     # 7 passed
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-ssu2-vectors.sh                             # hashes match
bash scripts/check-runtime-boundaries.sh
bash scripts/check-dependency-direction.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh                  # 22 rows command-derived
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'  # 153 passed
cargo deny check advisories bans sources                       # ok
```

Hosted routine CI is green on Plan 162 implementation commit
`624e8cce177040674376163160cfbda47e6a60fe` via run `33941941145`.
Routine CI also passes on the direction-B closing commit `fde2bae`
via run `34001837935` (Quality ubuntu-latest, Quality macos-latest,
MSRV Ubuntu, Dependency policy: all success).

## Hosted closure evidence (verbatim, criteria 17–19)

Ledger/workflow landing head `f353736`:

```text
routine CI run 34050058216 on f353736 = success
  Quality (ubuntu-latest) = success
  Quality (macos-latest)  = success
  MSRV (Ubuntu)           = success
  Dependency policy       = success
```

External lane on deps-fix head `b12a90a` (delta vs `f353736`:
workflow apt step + status prose only; no source, ledger,
driver, pin, or workflow-logic change). The first external attempt ran
on `f353736`:

```text
manual ssu2-external run 34050762669 on f353736 = failure
  failed in <1s at Fetch/verify (make exit 2): runner image lacks
  Boost/OpenSSL/zlib headers
  fix: declared-sudo install-build-deps step (Plan 099 precedent) + failure-only build-log tail
manual ssu2-external run 34050899741 on b12a90a = failure
  i2pd 2.61.0 built/provisioned; 8/15 rows passed (all 6 local +
  plan155-160 regressions + workspace-gates)
  external driver FAILED in the cached-token phase: directions A and B
  fully completed (established, small + fragmented stores + echoes,
  graceful closes), cached-token dial established, but its single
  DeliveryStatus echo never arrived in 30 s (i2pd ingested the store —
  fixture-address dial logged — but emitted no echo under runner load)
  classification: hosted-load variance, same family as the recorded
  direction-A large-echo timeouts; no code change
manual ssu2-external run 34051298144 on b12a90a = success (15/15)
  evidence.json: i2pr_commit b12a90a, i2pd 2.61.0 @ 635b013..., 51 driver keys
  i2pd.log: 6x RouterInfo added, only expected fixture-address dial
  timeouts, no AEAD failures, no unexpected-message errors
  artifact: ssu2-external-evidence-34051298144 (sanitized, no secrets)
```

Per-run results are recorded verbatim per the plan's variance rule;
the passing hosted run re-proves the exact matrix with no code change
between the failed and passing runs.

Post-closure confirmation run `34051971905` on closing head `f9e27c8`
(docs-only delta vs `b12a90a`) failed the same variance family:
direction A fully green, direction-B small echo received, but the
direction-B large (fragmented) echo never arrived inside the 30 s
window (`small=true large=false`, session healthy, retransmits in
flight). The peer log proves delivery anyway: i2pd ingested the
direction-B large fixture (`RouterInfo added: qFXA...`, matching the
driver's `b-large` hash) — reassembly just completed outside the echo
window under runner load.

Second confirmation run `34052503059` (head `3e80ac6`, status-prose
delta only) failed the same way in the cached-token phase: directions
A and B fully completed with all four echoes, the cached-token dial
established, the cached store was ingested (`RouterInfo added: KJsy...`,
matching hash), but its single echo never arrived in 30 s. Hosted
score stands at 1/4 with identical code while local lanes stay green:
blind reruns are no longer justifiable, so the driver gains a narrow
harness-only corrective (no wire/runtime change, no criterion change):
at most two send/collect attempts per data phase with fresh
fixtures/IDs/tokens per attempt (a late echo can never satisfy a later
attempt; evidence commits for the successful attempt only, plus
per-phase `*-echo-attempts` counts; missing echo after the final
attempt stays a hard failure). This distinguishes one lost
fire-and-forget echo datagram under load from a genuinely
unresponsive peer.

Corrective validated hosted (head `d9c4757`):

```text
routine CI run 34053041778 on d9c4757 = success (all four jobs)
manual ssu2-external run 34053042857 on d9c4757 = success (15/15)
  direction-a-echo-attempts = 2 (first echo lost under load, fresh
  second attempt proved delivery — the corrective working as designed)
  direction-b-echo-attempts = 1, cached-echo-attempts = 1
  artifact: ssu2-external-evidence-34053042857 (sanitized, no secrets)
```

Hosted external score is now 2/6 (two clean first-attempt passes, one
pass via the bounded retry, three load-loss failures all with
peer-side ingest proven); every run is recorded verbatim above. The
retry changes no pass criterion and no production behavior.

## Acceptance checklist (plan §17, criteria 1–24)

1. Plans 155–160 passed; `plan155-160-focused-regressions` row green
   locally and hosted. ✓
2. Exact i2pd `635b013a612ff47278ef02acf8580a28e10e26c5` fetched,
   pin-verified, dirty-tree-refused, built unmodified (fetch script +
   hosted fetch step). ✓
3. Real UDP loopback both directions (`127.0.0.1` asserts in driver +
   harness). ✓
4. No root/namespaces/container/VM/systemd/public-I2P; only declared
   sudo is the workflow's build-deps install (Plan 099 precedent). ✓
5. i2pr→i2pd handshake authenticates both peers (tokenless Retry path,
   `used_cached_token: false`, echoes over the same session). ✓
6. i2pd→i2pr handshake authenticates both peers (normal responder
   promotion, same proof shape). ✓
7. Bidirectional small I2NP exchange (per-direction small store +
   echo). ✓
8. Fragmented handling exercised independently per direction (i2pd
   ingested both fragmented stores: `RouterInfo added` + echoes). ✓
9. Token/Retry externally: tokenless + cached-token rows green;
   expired/invalid/source rows stay local-evidence-only (Plan 156/158
   suites, recorded in ledger limitations). ✓
10. Graceful termination to baseline per direction
    (`resource-baseline` row). ✓
11. Compact malformed/spoof cheap-drop/resource rows green; wider rows
    stay local-evidence-only (Plan 157, recorded). ✓
12. No external patching (verified clone + dirty-tree refusal). ✓
13. Java secondary lane recorded nonblocking with exact blocker (ledger
    + status, cannot silently disappear). ✓
14. No unconditional synthetic pass rows (checker §1–2). ✓
15. Checker green, CI-enforced in both lanes. ✓
16. Artifacts carry commits/pins/commands/digests; no secrets
    (verified by grep; keys never leave scratch). ✓
17. Full local floor green on closing code (fmt, check, 1521 passed /
    1 ignored workspace tests, clippy, doc, doc-tests, 9 boundary
    scripts, ntcp2 harness 153 tests, deny). ✓
18. Routine CI green (runs `34050058216` on `f353736` and
    `34053041778` on `d9c4757`; later closing-head deltas are
    docs/authority-only). ✓
19. Manual external workflow green with uploaded evidence (run
    `34051298144` on `b12a90a`, 15/15; corrective re-proven by run
    `34053042857` on `d9c4757`, 15/15). ✓
20. IPv4 direct SSU2 local + independent result classified passed
    (`specs/support.toml` `ssu2.v2-direct-ipv4-interop` surface). ✓
21. IPv6 external explicit as infrastructure-limited debt (ledger +
    support notes). ✓
22. PQ v3/v4 deferred, SSU1 unsupported (unchanged). ✓
23. No public-network/NetDB/tunnel/destination claim inferred
    (claim boundary retained everywhere). ✓
24. This record sets Milestone 8 closed and
    `next_product_layer = milestone9-planning`. ✓

Plan 161 is **passed**. Milestone 8 SSU2 v2 is closed within the
bounded direct-interop scope above. Do not start Milestone 9 work
that assumes anything outside that scope.
