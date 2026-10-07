# Plan 369 closure — trusted application runtime manager and apphost lifecycle foundation

Status: **passed-trusted-application-runtime-manager-and-apphost-lifecycle-foundation**.

Classification: **process/runtime lifecycle foundation**. This closure records a
supervised application-manager process, a direct-exec application host, and the
black-box evidence that the chain works end to end. It ships **no sandbox**, **no
package trust**, **no capability advertisement**, and **no user-visible protocol
support**.

Plan: `plans/implementation/managed-native-app-runtime/369-trusted-application-runtime-manager-and-apphost-lifecycle-foundation.md`.

## Implementation commits

| WP | Commit | Substance |
| --- | --- | --- |
| WP1 | `686c3306` | Freeze the AppManager/apphost trust zone and bootstrap contract |
| WP2 | `7b351292` | Supervise `i2pr-appd` over an inherited anonymous transport |
| WP3 | `72368c4e` | Bound the direct application exec; refuse `Secured` |
| WP4 | `2ca61aed` | Bind managed-app v1 sessions; correct the manager protocol direction |
| WP5 | `12e13c34` | Black-box fixture qualification; the manager-link flush fix |
| WP6 | see `git log` | Checkers, documentation, floor, and this record |

Two corrective plans were registered *by* this plan and closed inside it:
Plan 370 (`1f6625f9`, managed-app v1 `hello` instance-id codec) and Plan 371
(`dc8a4504`, optional/non-blocking service startup). Both exist because WP4 and
WP2 respectively exposed defects that made Plan 369's own invariants
unimplementable. Neither is rewritten here.

## Process and authority diagram

```text
                     i2pr-daemon  (composition root, the only production owner
                     |            of Tokio, sockets, timers, and children)
                     |
   (1) supervisor spawns the manager as a direct child over two INHERITED
       anonymous pipes on fd 0 (read) and fd 1 (write)
                     |
                     v
                  i2pr-appd            <- separate runtime trust zone
                     |                    only deps: the two wire contracts
   (2) manager spawns i2pr-apphost, also a current_exe() SIBLING, also
       over two inherited anonymous pipes
                     |
                     v
                i2pr-apphost          <- separate runtime trust zone
                     |                    the ONLY component that execs an app
   (3) direct exec, no shell, no PATH; then a byte-transparent relay
                     |
                     v
              <application>           managed-app v1 on stdin/stdout
                                       (diagnostics on stderr, bounded)
```

There is no edge from (1) to (3): the daemon never names an application, and
`i2pr-appd` never reaches a router crate. The only arrows are process edges, and
`scripts/check-managed-app-process-boundary.py` rule 1 pins exactly these three.

## Exact executable-resolution rules

| Edge | Resolution | Configurable? | Shell? | `PATH`? |
| --- | --- | --- | --- | --- |
| daemon → manager | `std::env::current_exe().parent()` + `MANAGER_FILE_NAME`, platform suffix applied (`i2pr.exe` on Windows) | **no** — there is no config counterpart, and `[app_runtime]` is `deny_unknown_fields`, so a `manager_path` key is a hard parse error | no | no |
| appd → apphost | `std::env::current_exe().parent()` + `APPHOST_FILE_NAME` (`i2pr-apphost`) | **no** | no | no |
| apphost → application | the root/entrypoint of a launch request the manager already validated; double-checked (structural strings, then canonicalised paths) | **no** | no | no |

Rule 1b of the process-boundary checker asserts the two `current_exe()` lookups
and forbids any shell launcher and any `PATH` lookup. Rule 3 asserts the shipped
manager **refuses** argv rather than merely reading it, and never names
`with_catalog`.

The apphost's target is deliberately *not* a sibling: it is governed by the
containment rules instead, which is why two different mechanisms appear in the
table rather than one being repeated.

## Manager inherited-transport evidence

- `i2pr_appd::transport::inherited()` binds fd 0 to the manager's read half and
  fd 1 to its write half. There is no listener, no port, no discovery endpoint,
  and no standalone mode: without an inherited transport the process has nothing
  to talk to and holds no authority.
- `the_app_runtime_is_disabled_by_default_and_selects_no_executable` and
  `the_block_rejects_any_manager_executable_key`
  (`crates/i2pr-daemon/tests/app_runtime_supervision.rs`) assert the switch and
  the `deny_unknown_fields` posture.
- `a_manager_that_completes_the_handshake_reaches_ready_and_is_reaped_on_shutdown`
  proves readiness is signalled **after** the handshake, not on spawn.
- The Plan-368 reference now fixes the binding normatively as §3.1.

## Restart, degrade, and bounded cleanup

- `a_manager_that_sends_the_wrong_magic_degrades_the_feature_not_the_router`
  asserts `lifecycle == Ready` **and** `snapshot.ready == true` while
  `app-runtime == Degraded(Degraded(LocalPolicy))`. This is Plan 371's
  `StartupRequirement::Optional` doing its job; the app runtime registers with
  both `Optional` and `RestartExhaustion::Degrade`, because degrading a service
  and releasing router readiness are separate decisions.
- `a_missing_manager_binary_is_refused_at_composition_with_an_actionable_message`
  keeps the WP2 preflight, now justified on the stronger ground that an operator
  who enabled a feature without installing its manager should get an actionable
  error rather than a silently degraded router.
- `terminate_manager` drops the pipe ends (EOF, the manager's only shutdown
  signal), waits `MANAGER_EXIT_GRACE` (5 s), then `start_kill` and reaps after
  `MANAGER_REAP_GRACE` (500 ms). A manager that already violated the protocol
  takes the short reap path, because waiting the full graceful grace there buys
  nothing and multiplies the restart budget by an order of magnitude.
- `an_application_that_hangs_is_stopped_by_a_bounded_shutdown` proves the apphost
  kills and reaps a direct child that ignores the relay closing.

**Non-goal, stated because it is a plausible misreading:** grandchild
containment. Only the direct child at each hop is owned and reaped.

## `Secured` fails closed, before exec

No qualified OS sandbox backend exists in Plan 369, so reporting a successful
`Secured` launch would be a forged containment claim.

- `secured_is_refused_before_an_authority_exists` (`crates/i2pr-appd/src/authority.rs`)
  — the refusal happens in the sealed authority, before any filesystem access,
  so **no authority value can exist** for a launch apphost would refuse.
- `secured_launch_is_refused_before_any_exec` (`crates/i2pr-apphost/src/lib.rs`)
  — the apphost independently re-checks before exec, so no future refactor can
  quietly skip the gate.
- Both gates were negative-tested: removing either fails a named test.

## Hello and identity-mismatch negatives

| Case | Test | Transcript |
| --- | --- | --- |
| app id differs from authority | `a_hello_whose_identity_differs_from_the_authority_kills_the_launch` | `start` → `verdict-requested {"step":"verdict-wrong-identity"}` → `fixture-error "fixture transport failed: failed to fill whole buffer"` — the host closed the transport rather than answering |
| first request is not `hello` | `a_frame_before_hello_kills_the_launch` | `start` only; no further record, because the launch died before the fixture could be answered |
| data before open | `data_before_open_is_refused` | — |
| oversized frame | `an_oversized_frame_is_refused_rather_than_truncated` | — |
| malformed frame | `a_malformed_frame_is_refused_rather_than_skipped` | — |
| denied capability | `a_permission_request_is_denied_and_mutates_nothing` | `capabilities ["Sam","I2cp"]` → `permission-denied {"capability":"lifecycle","status":"denied"}` → `complete` |
| denied service | `opening_a_service_the_launch_was_not_granted_is_refused` | `capabilities ["Sam"]` → `denied-service {"refusal":"PermissionDenied","service":"I2cp"}` → `complete` |

The first two are the interesting ones: the fixture asked for a verdict and got
**no reply at all**, which is the correct shape — a host that answers a refused
launch has already acknowledged the attempt.

## Fixture SAM black-box transcript

From `i2pr-app-fixture-sam-*/transcript.jsonl` (real run, retained):

```json
{"detail":{"app_id":"i2pr.fixture.app","instance":"101","pid":831724,"scenario":"sam-happy-path"},"step":"start"}
{"detail":["Sam","I2cp"],"step":"capabilities"}
{"detail":{"service":"Sam","stream_id":1},"step":"sam"}
{"detail":{"bytes":31,"stream_id":1},"step":"sam-hello"}
{"detail":{"line":"...\"HELLO REPLY RESULT=OK VERSION=3.1","stream_id":1},"step":"sam-hello-reply"}
{"detail":{"reply":"...\"HELLO REPLY RESULT=OK VERSION=3.1"},"step":"sam-version"}
{"detail":{"bytes":91,"stream_id":1},"step":"sam-session-create"}
{"detail":{"line":"...SESSION STATUS RESULT=OK DESTINATION=IzhR-hK7DD1YjOWP3E-…","stream_id":1},"step":"sam-session-result"}
{"detail":{"id":"3f2a9c1d-0000-4000-8000-0000000000ff","result":"…RESULT=OK…"},"step":"sam-session"}
{"detail":{},"step":"closing-sam"}
{"detail":{},"step":"closed-sam"}
{"detail":{"scenario":"sam-happy-path"},"step":"complete"}
```

The full destination line is a real `DESTINATION=TRANSIENT` allocation and is
present verbatim in the retained transcript; it is abbreviated here because the
closure record does not need a 700-character b64 key. Note this router's SAM
dialect replies `HELLO REPLY RESULT=OK VERSION=3.1` rather than prefixing
`SAM 3.1`, and that `DESTINATION=` is **required** on `SESSION CREATE` here —
the assertions test `RESULT=OK` + `VERSION=3.1` accordingly.

## Fixture I2CP black-box transcript

From `i2pr-app-fixture-i2cp-*/transcript.jsonl` (real run):

```json
{"detail":{"app_id":"i2pr.fixture.app","instance":"101","pid":831707,"scenario":"i2cp-happy-path"},"step":"start"}
{"detail":["Sam","I2cp"],"step":"capabilities"}
{"detail":{"service":"I2cp","stream_id":1},"step":"i2cp"}
{"detail":{"bytes":13,"stream_id":1},"step":"i2cp-get-date"}
{"detail":{"bytes":20,"stream_id":1},"step":"i2cp-set-date"}
{"detail":{"body_bytes":20},"step":"i2cp-set-date"}
{"detail":{"scenario":"i2cp-happy-path"},"step":"complete"}
```

Protocol byte `0x2a`, then a 4-byte big-endian length + 1-byte type header, with
`GetDate` (32) and `SetDate` (33). The round trip completed, so the I2CP stream
carried exact octets in both directions through the private gateway.

**Neither path required a loopback listener.** `AppGatewaySession::new` forces
`sam_config.enabled = false` and `i2cp_config.enabled = false`; the gateway
allocates backends through the injected composition, not a bound socket.

Transcripts are retained on demand: set `I2PR_APP_FIXTURE_EVIDENCE_DIR` before a
run to keep the per-run scratch directories. The read is confined to the
`#[cfg(test)]` harness and changes only *retention*, so it cannot change what a
green run proves.

## Sibling isolation and cleanup

- `two_instances_are_isolated_and_one_ending_does_not_touch_the_sibling` — two
  fixture applications under one manager; ending one leaves the other alive.
- `two_instances_sharing_a_sam_session_id_stay_isolated` — identical SAM session
  ids across instances do not cross streams.
- `a_foreign_stream_id_cannot_reach_this_instances_stream` — asserts the *held*
  stream survives, because `AppSession::on_reset` treats an unknown stream id as
  a no-op **by design**; the correct claim is isolation, not refusal.
- `an_early_close_ends_only_that_application` — one application's teardown does
  not reach its sibling.

## stderr and resource ceilings

- `an_application_that_floods_stderr_still_completes` writes
  `STDERR_FLOOD_BYTES = 524288` (512 KiB, recorded in the transcript as
  `{"step":"stderr-flood","detail":{"bytes":524288}}`), well past the apphost's
  and the daemon's 8 KiB retained snapshot, and the run still completes. stderr is
  drained continuously with a **bounded retained prefix and an uncapped byte
  total**: a flooding application cannot drive unbounded allocation, and the
  operator can still see why something failed.
- Ceilings: `MAX_MANAGER_SESSIONS` 32 (also `MAX_APP_INSTANCES` — one shared
  number, not a third independently chosen one), 128 streams per session, 64
  in-flight requests, 64 queued outbound frames, 8 KiB `RELAY_CHUNK_BYTES`.

## The product defect WP5 found: the manager link never flushed

`manager_link::writer_task` wrote each frame with `write_all` and **never
flushed**. The manager's write half is an anonymous pipe whose daemon end is
`tokio::io::Stdout`, which is *line-buffered*; a length-prefixed control frame
contains no newline, so every manager→daemon frame sat in the buffer until
process exit. **Every `CreateSession` timed out and no launch could ever have
succeeded.**

The handshake escaped only because `Appd::write_handshake` flushes on its own.
This is the second time in this plan that a test shared the implementation's
assumption and cancelled against a real defect — WP4's protocol direction, here
the transport's buffering. Every WP2–WP4 test drives a `tokio::io::duplex`
transport, which is unbuffered and therefore always "flushed"; no in-memory
transport could have exposed it.

Fix: flush after every `write_all`.
Regression: `every_frame_is_flushed_without_waiting_for_a_newline`, a
`FlushCounter` writer that fails the moment the flush is removed (mutation N1
below), rather than inferring the property from a timeout.

## Guard mutation evidence — 12 mutations, all caught

| ID | Mutation | Caught by |
| --- | --- | --- |
| N1 | manager link stops flushing each frame | `manager_link::tests::every_frame_is_flushed_without_waiting_for_a_newline` |
| N2 | a non-apphost crate spawns a child | process boundary, rule 1 |
| N3 | the daemon resolves its manager from configuration | process boundary, rule 1b |
| N4 | apphost launches through a shell | process boundary, rule 1b |
| N5 | apphost resolves its executable by searching `PATH` | process boundary, rule 1b |
| N6 | production code names the fixture manager | process boundary, rule 2 |
| N7 | a production crate depends on the fixture | process boundary, rule 2 |
| N8 | the shipped manager reads argv and carries on | process boundary, rule 3 |
| N9 | the shipped manager selects its own catalog | process boundary, rule 3 |
| N10 | production code calls the manager test seam | process boundary, rule 4 |
| N11 | the dependency map forgets the fixture crate | `check-dependency-direction.sh` |
| N12 | the manager-protocol allow-set forgets the fixture | `check-managed-app-manager-boundary.py` |

Earlier mutation suites: WP4's **15 mutations** (M15 breaks the *build*, because
the authority seal is asserted by method resolution rather than by scanning for
a derive) and Plan 371's **nine**, two against real production composition.

### Three guards this plan found weak

These are recorded because a green guard is only worth its evidence, and three
of them were weaker than they read until something forced the question:

1. **N8 missed, and it was the checker's fault.** Rule 3 asserted the *token*
   `args_os`, which passes a manager that reads argv, prints it, and carries on —
   exactly the "silently tolerates arguments" failure the rule exists to prevent.
   It now asserts the *shape*: an argv guard whose body returns. **Stated limit:**
   this is structural, not semantic. It cannot prove the guard's condition is
   always true (a deliberate `if false && ..` would satisfy it). The limit is
   recorded in the script rather than papered over.
2. **The negative controls were replacing whole files.** `expect_accepted`
   overwrote a real source, which deleted the production `current_exe()` lookup so
   rule 1b fired; the control then passed by filtering on its own label while the
   tree it scanned was nonsense. Controls now **append**, so each tests exactly
   one thing.
3. **`check-dependency-direction.sh` iterated its map, not the workspace.** N11
   removed the `i2pr-app-fixture` entry and the script still printed
   `dependency direction: ok` — deleting an entry made that crate's forbidden
   edges *unreported* rather than reported. `check-console-boundaries.sh` rule 7
   asserted the same set, but the check belongs in the script that owns the map,
   and a reader running only the dependency check was getting a false all-clear.
   It now fails closed on any unmapped `i2pr-*` member; the console rule is kept
   as a redundant check.

### A gap in the routine floor, surfaced by a guard

`cargo check`, `cargo test --all-targets` and `cargo clippy` all emit only test
harnesses under `target/debug/deps`; none produces the plain
`target/debug/<name>` binaries the qualification execs. Verified directly on
this host: `cargo test --workspace --all-targets --no-run` left
`target/debug/i2pr-app-fixture-manager` at its pre-existing mtime. The floor and
CI now build them explicitly, and the harness asserts binary freshness itself and
fails closed rather than silently testing last week's code.

## Support and configuration diff

**`specs/support.toml`: no diff.** Managed-app v1 remains unreleased and
unadvertised, and Plan 369 introduces no version, RouterInfo, SAM, or I2CP
behaviour change. `specs/CONFORMANCE.md` is likewise unchanged.

**Configuration:** exactly one new block, `[app_runtime]`, carrying `enabled`
only, defaulting to `false`. It has no per-connection ceiling because it owns no
listener, no port, and no connection budget. Because the struct is
`deny_unknown_fields`, a `manager_path` key is a **hard parse error** rather than
a silently ignored setting — which is what keeps the executable
distribution-owned in the face of an operator who believes otherwise.

No existing router configuration key changes meaning, no migration is required,
and no existing deployment changes behaviour.

## Documentation evidence

Written or updated in WP6:

- New deep dives: `i2pr-appd.md`, `i2pr-apphost.md`, `i2pr-app-fixture.md`
  (the last two crates had no per-crate document at all, and
  `AGENTS.md` promises one per workspace member).
- `overview.md`: workspace map, ASCII graph, per-crate allowlist, module index,
  three new §4 overviews, and the guardrail-script list.
- `dependency-graph.md`: allowlist rows, reverse edges, an explicit "separate
  island" graph, the fixture's non-production entry, and the corrected daemon
  dependency count (the table said 17 while the list and `cargo metadata` both
  said 18).
- `security-model.md`: a new "Plan 369 managed-application process threats and
  controls" section, including an explicit non-claims list.
- `i2pr-daemon.md`: corrected the module list (it predated Plan 368/369
  entirely), replaced "as of Plan 368 this module has no production caller",
  and added the `app_runtime` section.
- `tooling.md`: the new checker, the new floor line.
- `specs/references/managed-app-manager-protocol-v1.md`: **§3.1**, the normative
  concrete inherited binding, plus supervision semantics in §9.
- `specs/references/managed-native-app-runtime-v1.md`: the normative
  `hello` identity-binding clarification (stdin/stdout transport, exact
  app-id/instance-id match).
- `AGENTS.md`: the new floor line with the reason it exists, plus the
  `macOS/bash-3.2` and routine-floor notes.
- Registry, this roadmap, and the subsystem roadmap.

## Requirement-to-evidence matrix

| # | Acceptance criterion | Evidence |
| --- | --- | --- |
| 1 | `i2pr-appd` is a separately supervised trusted process | `app_runtime.rs`, `app_runtime_supervision.rs` |
| 2 | daemon↔appd uses only anonymous inherited transport, no discoverable endpoint | `inherited()`, §3.1 of the manager reference |
| 3 | manager failure degrades only app runtime; bounded restart proven | `a_manager_that_sends_the_wrong_magic_degrades_the_feature_not_the_router` |
| 4 | `i2pr-apphost` is the only component that execs an application | process boundary rule 1 (mutations N2, N4) |
| 5 | `Secured` fails before exec | `secured_is_refused_before_an_authority_exists`, `secured_launch_is_refused_before_any_exec` |
| 6 | app stdin/stdout carry v1; stderr is bounded diagnostics | `an_application_that_floods_stderr_still_completes` |
| 7 | `hello` identity exactly matches the launch identity | `a_hello_whose_identity_differs_from_the_authority_kills_the_launch` |
| 8 | permission requests cannot change capabilities | `a_permission_request_is_denied_and_mutates_nothing` |
| 9 | fixture SAM/I2CP traverse Plan 368→355 without listeners | SAM and I2CP transcripts above |
| 10 | sibling isolation and crash behaviour proven | the four isolation tests above |
| 11 | direct child cleanup and manager restart cleanup are bounded | `..._is_reaped_on_shutdown`, `an_application_that_hangs_...` |
| 12 | no package/grant/admin/broker/UI/Proposal-170 work | `EmptyCatalog`, sealed authority, no decoder |
| 13 | guards enforce the new trust zones | `check-managed-app-process-boundary.py` (+`--self-test`), 12 mutations |
| 14 | no support/advertisement promotion | `specs/support.toml` and `specs/CONFORMANCE.md` unchanged |
| 15 | focused and full routine floors pass | see below |

## Verification commands and outcomes

Focused:

```text
cargo fmt --all --check                                                        -> ok
cargo check --locked --workspace --all-targets                                 -> ok
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost                        -> ok
cargo test --locked -p i2pr-appd --all-targets                                 -> ok
cargo test --locked -p i2pr-apphost --all-targets                              -> ok
cargo test --locked -p i2pr-app-fixture --all-targets                          -> 17 passed
cargo test --locked -p i2pr-daemon --lib app_runtime_qualification \
    -- --test-threads=1                                                        -> 17 passed, 114 s
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  -> ok
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps            -> ok
bash scripts/check-dependency-direction.sh                                     -> ok
bash scripts/check-runtime-boundaries.sh                                       -> ok
bash scripts/check-console-boundaries.sh                                       -> ok
python3 scripts/check-managed-app-private-client-seams.py                      -> ok
python3 scripts/check-managed-app-gateway-boundary.py                          -> ok
python3 scripts/check-managed-app-manager-boundary.py                          -> ok
python3 scripts/check-managed-app-process-boundary.py                          -> ok
python3 scripts/check-managed-app-process-boundary.py --self-test               -> ok
python3 -m unittest discover -s tests/planning -p 'test_*.py'                  -> ok
```

Routine floor: see **Full routine-floor results** below.

Local vs CI, stated honestly: **every result in this record was executed on this
host, locally.** No claim is made about an exact-head CI run; this plan does not
record one, and the closure does not assert one.

## Full routine-floor results

Complete `AGENTS.md` routine floor, executed on this host at the closing
tree. **All 52 entries PASS.** Recorded verbatim from the run's own
per-step output; the two `macOS/bash-3.2` trap scripts (`check-fixture-manifest.sh`,
`check-java-source-lock-gating.sh`) are included for completeness and pass here
because this host runs bash 5.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | **PASS** |
| `cargo check --locked --workspace --all-targets` | **PASS** |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost` | **PASS** |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | **PASS** |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **PASS** |
| `env RUSTDOCFLAGS=-D warnings cargo doc --locked --workspace --no-deps` | **PASS** |
| `cargo test --locked --workspace --doc` | **PASS** |
| `bash scripts/check-dependency-direction.sh` | **PASS** |
| `python3 scripts/check-global-plan-number-uniqueness.py` | **PASS** |
| `python3 scripts/check-adr-number-uniqueness.py` | **PASS** |
| `python3 scripts/check-portable-service-tunnel-api.py` | **PASS** |
| `bash scripts/check-portable-service-tunnel-consumer.sh` | **PASS** |
| `python3 -m unittest discover -s tests/planning -p test_*.py` | **PASS** |
| `bash scripts/check-runtime-boundaries.sh` | **PASS** |
| `bash scripts/check-console-boundaries.sh` | **PASS** |
| `bash scripts/check-console-browser-security.sh` | **PASS** |
| `python3 scripts/check-managed-app-private-client-seams.py` | **PASS** |
| `bash scripts/check-service-tunnel-boundaries.sh` | **PASS** |
| `python3 scripts/check-managed-app-gateway-boundary.py` | **PASS** |
| `python3 scripts/check-managed-app-manager-boundary.py` | **PASS** |
| `bash scripts/check-m11-per-epoch-composition.sh` | **PASS** |
| `bash scripts/check-service-anonymity-boundaries.sh` | **PASS** |
| `bash scripts/check-ntcp2-vectors.sh` | **PASS** |
| `bash scripts/check-ssu2-vectors.sh` | **PASS** |
| `bash scripts/check-i2cp-vectors.sh` | **PASS** |
| `bash scripts/check-ntcp2-interoperability.sh` | **PASS** |
| `bash scripts/check-constrained-host-lane-boundary.sh` | **PASS** |
| `bash scripts/check-m11-transit-boundaries.sh` | **PASS** |
| `bash scripts/check-m11-transit-qualification-evidence.sh` | **PASS** |
| `bash scripts/check-sam-acceptance-evidence.sh` | **PASS** |
| `bash scripts/check-ssu2-acceptance-evidence.sh` | **PASS** |
| `bash scripts/check-i2cp-acceptance-evidence.sh` | **PASS** |
| `bash scripts/check-i2pcontrol-acceptance-evidence.sh` | **PASS** |
| `bash scripts/check-els2-type11-transcript-boundary.sh` | **PASS** |
| `bash scripts/check-encrypted-service-consumer-caller.sh` | **PASS** |
| `bash scripts/check-outproxy-request-path.sh` | **PASS** |
| `bash scripts/check-outproxy-wire-lane-evidence.sh` | **PASS** |
| `bash scripts/check-config-secret-hygiene.sh` | **PASS** |
| `python3 scripts/check-workflow-validity.py` | **PASS** |
| `bash scripts/check-floodfill-type5-serve.sh` | **PASS** |
| `bash scripts/check-service-tunnel-acceptance-evidence.sh` | **PASS** |
| `bash scripts/check-exploratory-tunnel-evidence.sh` | **PASS** |
| `bash scripts/check-netdb-tunnel-evidence.sh` | **PASS** |
| `bash scripts/check-destination-tunnel-evidence.sh` | **PASS** |
| `bash scripts/check-streaming-tunnel-evidence.sh` | **PASS** |
| `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | **PASS** |
| `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | **PASS** |
| `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | **PASS** |
| `python3 -m unittest discover -s tests/integration/ntcp2/harness -p test_execution_lane.py` | **PASS** |
| `cargo deny check advisories bans sources` | **PASS** |
| `bash scripts/check-fixture-manifest.sh` | **PASS** |
| `bash scripts/check-java-source-lock-gating.sh` | **PASS** |

Re-run after the negative-mutation suite to confirm the tree was restored:

```text
cargo fmt --all --check                                                -> ok
cargo check --locked --workspace --all-targets                         -> ok
python3 scripts/check-managed-app-process-boundary.py                  -> ok
python3 scripts/check-managed-app-process-boundary.py --self-test       -> ok
```

And the WP5 negative suite re-run at the closing tree: **12/12 mutations
caught**.


## Findings by severity

**Critical:** none.

**High:** none outstanding. Three were found and fixed inside this plan — the WP2
protocol direction inversion (WP4), the WP5 manager-link flush (WP5), and the
WP2 supervisor-semantics blocker (Plan 371).

**Medium — open, recorded rather than closed:**

1. `check-managed-app-process-boundary.py` rule 3 is structural and cannot prove
   the shipped manager's argv condition is always true. Stated in the script.
2. The same script's rule 1 is a token/regex scan, so a sufficiently obfuscated
   process creation could evade it. `check-runtime-boundaries.sh` and
   `cargo deny` cover the surrounding ground, and the rule's own self-test
   includes negative controls, but it is a scanner and not a proof.
3. The fixture's `assert_fresh` staleness guard is mtime-based, so a
   `cargo fmt`-only change makes a correct run fail closed until the siblings are
   rebuilt. This is the intended direction of failure.
4. **`check-runtime-boundaries.sh`'s Tokio manifest rule is inert against this
   repo's manifest style.** It greps `^(tokio|tokio-util)[[:space:]]*=`, which
   does **not** match `tokio.workspace = true` — the style every crate here
   uses. Verified directly: `printf 'tokio.workspace = true\n' | grep -En
   '^(tokio|tokio-util)[[:space:]]*='` does not match. So the "Tokio
   dependencies are confined to approved runtime/testkit manifests" rule
   cannot currently fire.

   This gap **predates Plan 369** — it was already inert for `i2pr-daemon`,
   which has declared `tokio.workspace = true` since well before this plan. What
   Plan 369 changed is the *consequence*: the rule's approved-owner allowlist
   (`i2pr-runtime`, `i2pr-testkit`) is now also stale, because `i2pr-appd`,
   `i2pr-apphost`, and `i2pr-app-fixture` are legitimate process-trust-zone
   Tokio owners under this plan's design. Repairing the regex today would fail
   all six crates at once and force a decision about whether `i2pr-daemon` is an
   approved Tokio owner — which contradicts `AGENTS.md`'s own workspace-boundary
   statement and is far larger than Plan 369.

   The script was therefore **not modified**, because the rule is still to be
   fixed rather than relaxed, and choosing which crates are approved runtime
   owners is a separate plan-of-record. `AGENTS.md` is corrected to state the
   real situation and to name this finding rather than leave a claim that is no
   longer true.

**Low — open:**

1. `AppSession::on_reset` treats an unknown stream id as a no-op. That is by
   design, but it means "foreign stream id" is qualified as *isolation* rather
   than as *refusal*, and a reviewer should not read the test as the latter.
2. `i2pr-app-fixture` is a workspace member, so `cargo build --workspace` builds
   evidence tooling. It never reaches a shipped router binary, and the process
   boundary checker asserts that; excluding it from the workspace was judged
   worse than the explicit guard.

## Limitations and non-claims

Stated plainly because each is a plausible misreading:

- **No sandbox.** `Secured` is refused, not approximated. No qualified OS
  backend exists.
- **No grandchild containment.** Only the direct child at each hop is owned.
- **No package trust** — no store, signature, publisher-key identity, signed
  manifest, grant persistence, or transactional install.
- **No administrator or general control credential** is exposed to a manager or an
  application.
- **No restart recovery.** Restart begins empty.
- **No user-visible capability.** Enabling `[app_runtime]` currently buys a
  supervised, bounded process and its health signal. It launches nothing, and
  that is structural: the shipped manager owns `EmptyCatalog` and the Plan-368
  protocol has no manager-receivable launch request.
- **Managed-app v1 remains unreleased** and is not a stable external SDK
  guarantee.
- **No protocol support or advertisement change.** Both SAM and I2CP evidence
  here exercises *existing* router paths through the private gateway.

## Unblock audit

Performed at closure over `plans/registry.md` blocked work and the affected
roadmap dependency graph.

- **Plans 370 and 371** were registered as correctives *by* this plan and are
  both closed. Neither remains a blocker.
- The subsystem roadmap records no other plan gated on Plan 369.
- **No registered plan lists Plan 369 as a hard or interface dependency**, so no
  work is moved to `ready` by this closure and nothing is silently unblocked.
- The next milestone — package trust and restart-safe local lifecycle
  (immutable versions, publisher-key identity, signed manifest/file inventory,
  transactional install/update/uninstall, grant persistence, and the production
  constructor for prevalidated launch authority) — is **not registered here**.
  It is named as the successor owner, and it needs its own plan-of-record before
  any package or grant work begins.

## Roadmap disposition

Plan 369 is **closed**. `plans/registry.md` and
`plans/subsystems/managed-native-app-runtime-roadmap.md` are updated in the same
commit. The managed-native-app-runtime workstream continues in parallel to the
router protocol milestones and gates none of them.