# Plan 229 registration follow-up

Plan 228 remains closed at:

```text
P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both
```

Pinned-source review after closure shows the zero-hop exploratory tunnels are
stock startup fallbacks, while Java's client builder requires non-zero paired
infrastructure. Java's bundled small-router profile supplies a supported
one-hop exploratory configuration, and the repo's current launcher incorrectly
makes Router C floodfill despite Plan-201 defining it as a non-floodfill
transit participant.

Plan 229 is therefore registered as the bounded corrective: restore C's
transit role, apply the stock one-hop exploratory profile to A only, prove C
entered the ordinary profiled/selectable population through the existing
authenticated RouterInfo DatabaseStore bootstrap, require real non-zero
exploratory tunnels, and rerun the unchanged Plan-227/228 client build path.

```text
plan_228 = passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary
plan_229 = registered-ready-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective
next_executable_plan = 229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective
```

No direct profile/NetDB/tunnel mutation, Java patch, VMComm,
`netDb.alwaysQuery`, public I2P, timeout increase, exploratory
`explicitPeers`, or Plan-227 client-profile change is authorized.

# Plan 228 status — M6 Java client-tunnel build-path attribution

Status: **`passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary`**.

Plan of record:
[`228-m6-java-client-tunnel-build-path-attribution.md`](../../implementation/mixed-router-interop/228-m6-java-client-tunnel-build-path-attribution.md).

## Closure result

Plan 228 is closed as a completed, fail-closed attribution-only
investigation. It closes the exact Plan-227 build boundary
(`P227-EXPLICIT-ONE-HOP-NOT-BUILT`) at the earliest proven missing stock-Java
stage: the raw helper's client tunnel configs are created through Router C,
but neither direction can obtain the paired tunnel Java requires before
Router C receives a build request. All four counted attempts on the final
implementation line proved the same authoritative terminal:

```text
P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both
```

No workaround was implemented, no profile/tier was mutated, no NetDB entry
was stored, no tunnel was installed directly, no exploratory/client tunnel
policy was changed, no paired-tunnel override was set, no VMComm was enabled,
no `netDb.alwaysQuery` was set, no timeout was changed, no topology was
changed, and no M6 Java interoperability is claimed. The Streaming helper was
neither executed nor modified (destination-only lane).

Authoritative per-attempt facts:

```text
attempt=1 sha=8f2167d router_c_hex=639cc481063ec0fac761a90a8ecab2ad45deb5d0ee7f86f80747c90d5bd19ae2
  main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false
  b64_len=44 renderer=P224-HASH-B64 helper_connect_elapsed_ms=301613 helper_ready=false helper_connect_timeout_seen=true
  infra_pre: free=1 inbound=1 outbound=1 inbound_expl_nonzero=0 outbound_expl_nonzero=0
  infra_post: free=2 inbound=2 outbound=2 inbound_expl_nonzero=0 outbound_expl_nonzero=0
  selector_both=true config_contains_c=true configuring_seen=true paired_missing=true
  dispatched=false c_read_slot=false
  terminal=P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both

attempt=2 sha=8f2167d router_c_hex=52aa977781c2df1745ca3cece2a58e017ff87469b0c227a9eb35b3931ae4f6ca
  main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false
  b64_len=44 renderer=P224-HASH-B64 helper_connect_elapsed_ms=301751 helper_ready=false helper_connect_timeout_seen=true
  infra_pre: free=1 inbound=1 outbound=1 inbound_expl_nonzero=0 outbound_expl_nonzero=0
  infra_post: free=2 inbound=2 outbound=2 inbound_expl_nonzero=0 outbound_expl_nonzero=0
  selector_both=true config_contains_c=true configuring_seen=true paired_missing=true
  dispatched=false c_read_slot=false
  terminal=P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both

attempt=3 sha=8f2167d router_c_hex=1c8f0fa40806eb07067662e95963bcd5f67c82e324d838c775ef4ff778a911f8
  main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false
  b64_len=44 renderer=P224-HASH-B64 helper_connect_elapsed_ms=301842 helper_ready=false helper_connect_timeout_seen=true
  infra_pre: free=1 inbound=1 outbound=1 inbound_expl_nonzero=0 outbound_expl_nonzero=0
  infra_post: free=2 inbound=2 outbound=2 inbound_expl_nonzero=0 outbound_expl_nonzero=0
  selector_both=true config_contains_c=true configuring_seen=true paired_missing=true
  dispatched=false c_read_slot=false
  terminal=P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both

attempt=4 sha=8990849 router_c_hex=519dae8e43519995c554eed3b36104647655d39a760995dc4f91e4f47d3f416d
  main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false
  b64_len=44 renderer=P224-HASH-B64 helper_connect_elapsed_ms=330666 helper_ready=false helper_connect_timeout_seen=true
  infra_pre: free=1 inbound=1 outbound=1 inbound_expl_nonzero=0 outbound_expl_nonzero=0
  infra_post: free=2 inbound=2 outbound=2 inbound_expl_nonzero=0 outbound_expl_nonzero=0
  selector_both=true config_contains_c=true configuring_seen=true paired_missing=true
  dispatched=false c_read_slot=false
  terminal=P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both
```

Attempt 4 is the confirmatory run on the final SHA after a lint-only
fixup (clippy `derivable_impls` / `collapsible_if` /
`field-reassign-with-default` / `useless_format`; no classifier, sanitizer,
harness, or probe semantic change). Attempts 1–3 saturated the
three-per-SHA budget on `8f2167d`; attempt 4 is the single counted run on
`8990849`, within budget.

Because no build request was ever dispatched, no Router-C request handling,
no Router-A reply handling, and no local join/install stage was reached or
inferred. The exact Plan-226 target search was never re-observable and the
frozen 45-second reverse-payload window was never entered. This is the
prescribed earliest-stage stop, not a production protocol fix.

## Implementation commits and pinned inputs

Implementation was committed before the counted attempts (Plan 228 §16):

```text
8f2167d interop: implement Plan 228 client-tunnel build-path attribution
8990849 interop: fix Plan 228 clippy lints (no behavior change)
```

Counted attempts 1–3 ran only on `8f2167d`:

```text
8f2167dbd68334f235b55f0fd1bf6f6f02df6b41
```

The confirmatory attempt 4 ran only on `8990849`:

```text
8990849ea2d2fc6bd0b1d77ea7418cf65fa4e7bf
```

Reference inputs remained frozen: Java I2P `2.13.0` at
`9134f808337b401e8e53c73734c81fab04280c9d`; the i2pd reference pin remained
`2.61.0` at `635b013a612ff47278ef02acf8580a28e10e26c5`. No dependency,
fixture, production protocol, or Java reference source changed. No Java
source patch, reflection, profile/tier mutation, client-NetDB RI store,
direct tunnel install, exploratory/client tunnel setting change,
paired-tunnel policy override, VMComm, `netDb.alwaysQuery`, public I2P,
distinct topology, build/timeout change, streaming-helper change, or
reverse-window change.

Test-only deltas (no production Rust code changed):

```text
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P228Probe.java (new)
tests/integration/m6-interop/java/ControlledRouter.java (P228-TUNNEL-INFRA / P228-CLIENT-POOLS / P228-LOGGER-CONFIG)
tests/integration/m6-interop/run-java.sh (P228 logger scopes, infra snapshots, attribution driver, aggregation)
crates/i2pr-daemon/tests/java_tunnel_external.rs (P228 classifier, trace sanitizer, attribution driver, 21 unit rows)
scripts/check-m6-mixed-router-acceptance-evidence.sh (Plan 228 §21 invariants)
```

## Requirement-to-evidence matrix

| Plan-228 requirement | Evidence / result |
|---|---|
| Reference pins unchanged (§18.1) | Java `9134f80…` + i2pd `635b013a…` on all 4 attempts; static guard pins both. |
| Plan-227 raw-helper profile unchanged (§18.2) | `inbound.length=1 outbound.length=1 allowZeroHop=false` + scoped `inbound/outbound.explicitPeers=<C-B64>` retained; streaming helper frozen zero-hop without explicitPeers; static guard rejects profile drift. |
| No production Rust changes (§18.3) | `git diff --stat` touches only the five test-only files above; dependency/runtime boundary scripts pass. |
| Router-C selectability proven or contradiction recorded (§18.4) | All 4 attempts: `main_raw_present=true main_valid_present=true selectable=true` via read-only `P227-PEER-ELIGIBILITY`; no contradiction. |
| Router/exploratory tunnel infrastructure observable (§18.5) | `P228-TUNNEL-INFRA` pre-build (`free=1 outbound=1`) and at-timeout (`free=2 outbound=2`) on all attempts; prerequisite met, so `BuildExecutor` was not infra-blocked. |
| Client pool selection/configuration separately observable (§18.6) | `peers_for_inbound_seen=true peers_for_outbound_seen=true config_contains_c=true configuring_new_tunnel_seen=true` on all attempts; selector presence alone never sufficed (unit row locks this). |
| Paired-tunnel availability separately observable (§18.7) | `paired_missing_seen=true` (`Tunnel build failed, as we couldn't find a paired tunnel for …`) on all attempts; exploratory nonzero counts `0/0` corroborate: only zero-hop exploratory tunnels existed, which stock Java refuses as paired tunnels. Terminal is `NO-PAIRED-TUNNEL direction=both`. |
| Build-message creation separately observable (§18.8) | No `couldn't create the tunnel build message` line on any attempt; correctly not attributed (earlier paired stage wins). |
| Router-A dispatch separately observable (§18.9) | No inbound/outbound dispatch line on any attempt; correctly not attributed past the paired stage. |
| Router-C receipt/decrypt/response observable when reached (§18.10) | Not reached: `c_read_slot_seen=false c_response_code=none`; C logger proven effective (`c_log_observable=true`), so absence is evidence of no dispatch, not of logging being off. |
| Router-A reply/decrypt/status/join observable when reached (§18.11) | Not reached: no reply handling, status, decrypt-fail, dup-ID, or timeout line; install snapshot negative (`inbound_installed=false outbound_installed=false`). |
| Earliest missing stage without inferring from timeout (§18.12) | Classifier precedence is unit-locked (21 rows): infra → config → paired → create → dispatch → first-hop → C → reply → join. The paired terminal wins over all later signals deterministically. |
| No workaround implemented (§18.13) | Static guard rejects `usePairedTunnels`, `netDb.alwaysQuery`, VMComm, policy/timeout/topology changes; none present. |
| Raw logs scratch-only (§18.14) | Sanitizer counts only exact-pinned English scaffolding; secret-bearing lines never satisfy a fact; peer lists/keys/records never enter evidence (unit row locks rejection). |
| Exactly one terminal emitted (§18.15) | Each attempt emits exactly one `p228-classification` row (last-occurrence aggregation); unit row locks single emission. |
| Focused/routine verification (§18.16) | See Verification below. |
| Closure/registry/roadmap/unblock audit (§18.17) | This record plus registry/roadmap/201/204 amendments in the same commit. |

Timeout/result counters (WP G): the helper never connected, so no client
pools existed to snapshot (`p228-client-pools observable=false
reason=client-dbid-unresolvable-no-helper-tunnels`); no build reply timeout
line was observed because no dispatch ever occurred — the run correctly stops
at the paired stage rather than inventing a timeout terminal.

## External attempt history

Each attempt used fresh disposable Java RouterContexts, the same baseline
loopback topology, the same raw-helper Plan-227 SessionConfig, the same
Router-C explicit peer derivation (`P220-SNAPSHOT` hex validated as 64
lowercase hex, `P224-HASH-B64` render, 44-char I2P Base64), the same
Java/i2pd pins, the same five-minute helper ceiling, destination-only lane
(`I2PR_M6_JAVA_DRIVER=destination`), and no tuning between retries, per §16.

| Attempt | Implementation SHA | P228 result |
|---|---|---|
| 1 | `8f2167d` | `P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both`, C selectable, 301613 ms |
| 2 | `8f2167d` | `P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both`, C selectable, 301751 ms |
| 3 | `8f2167d` | `P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both`, C selectable, 301842 ms |
| 4 | `8990849` | `P228-ATTRIBUTION-NO-PAIRED-TUNNEL direction=both`, C selectable, 330666 ms (confirmatory on final SHA) |

The enclosing legacy Plan-199 Java wrapper exited nonzero on all attempts
because install-dependent destination/streaming rows remain unqualified.
The Plan-228 diagnostic rows (`external-p228-classification`,
`external-p228-tunnel-infra`, `external-p228-logger-config-a/c`,
`external-p228-client-pools`, `external-p228-trace`, retained P227 rows,
`workspace-gates`) passed; raw Java logs remained scratch-only.

## Verification

Successful verification on final SHA `8990849` (local truth, not CI),
except the full workspace floor which ran on `8f2167d` (2565 passed) with
only lint-only test-file delta afterwards, re-verified by the focused rows
below plus clippy:

```text
cargo fmt --all --check                                      PASS
cargo check --locked --workspace --all-targets                PASS
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                                 PASS on 8f2167d (2565 passed, 17 ignored)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                                 PASS (final SHA)
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                                 PASS (final SHA)
cargo test --locked --workspace --doc                         PASS (0 doc tests, final SHA)
cargo deny check advisories bans sources                     PASS (final SHA)
bash scripts/check-dependency-direction.sh                   PASS (final SHA)
bash scripts/check-runtime-boundaries.sh                     PASS (final SHA)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh    PASS (§21 invariants, final SHA)
bash scripts/check-destination-tunnel-evidence.sh            PASS (final SHA)
bash scripts/check-streaming-tunnel-evidence.sh              PASS (final SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p228_ -- --test-threads=1
                                                                 PASS (21 passed, final SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
                                                                 PASS (14 passed, final SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p226_ -- --test-threads=1
                                                                 PASS (7 passed, final SHA)
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
                                                                 PASS (final SHA)
bash -n tests/integration/m6-interop/run-java.sh             PASS
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh PASS
```

Focused Plan-228 unit rows (21, all green locally):

```text
p228_no_router_tunnel_infra_maps_to_infra_terminal
p228_selector_without_config_maps_to_no_client_config
p228_config_through_c_without_paired_tunnel_maps_correctly
p228_message_created_but_not_dispatched_maps_correctly
p228_outbound_first_hop_failure_maps_correctly
p228_dispatched_but_c_receives_nothing_maps_correctly
p228_c_receive_decrypt_failure_maps_correctly
p228_c_explicit_reject_code_maps_correctly
p228_c_accept_reply_absent_at_a_maps_correctly
p228_a_reply_decrypt_failure_maps_correctly
p228_a_remote_rejection_maps_correctly
p228_local_join_failure_maps_correctly
p228_build_reply_timeout_maps_correctly
p228_inbound_only_install_is_insufficient
p228_outbound_only_install_is_insufficient
p228_both_installed_maps_only_to_next_boundary
p228_unrelated_exploratory_logs_cannot_classify_client_build
p228_unrelated_destinations_cannot_classify_raw_helper
p228_secret_raw_log_lines_rejected_from_evidence
p228_classification_emits_exactly_one_terminal
p228_earliest_stage_precedence_is_deterministic
```

## Security, compatibility, and operational decisions

- No production crate or protocol behavior changed; all deltas are in the
  external Java driver (`P228Probe.java`, `ControlledRouter.java` read-only
  commands), the scratch `logger.config` scope list, the `run-java.sh`
  harness gates, the `java_tunnel_external.rs` classifier/unit rows, and the
  static evidence checker.
- No Java source patching, reflection, private-state mutation, NetDB/key/
  tunnel injection, publication retry, paired-tunnel policy override
  (`router.usePairedTunnels` untouched), timeout change (helper five-minute
  ceiling and 45-second payload window frozen), or `netDb.alwaysQuery`
  override was used.
- All SSU2 hosts remain `127.0.0.1` baseline; the distinct topology path
  stays rejected by construction for counted runs.
- Durable evidence contains only bounded booleans, counts, direction,
  hashes, response/status codes, and elapsed milliseconds. Raw Java logs,
  peer path lists, keys, tags, SessionConfig contents, request records, and
  payloads remain scratch-only or are redacted; the sanitizer rejects
  secret-bearing lines before they can satisfy any fact, and the C response
  parser extracts only the bounded acceptance integer, never the record.
- Java and i2pd pins, SAM/I2CP/diagnostic loopback policy, and frozen
  45-second reverse-payload authority remain unchanged.
- A client tunnel config is never considered proven from selector log
  presence alone; only Router-C-correlated selector activity plus a
  build-config event qualifies (unit-locked). Installed tunnels are proven
  only from installed-pool snapshots, never from logs.

## Findings and limitations

No security finding was introduced. The plan proved its narrow attribution:
stock Java creates the explicit-peer client configs through Router C but
cannot obtain a paired tunnel in either direction because the only available
router tunnels are zero-hop exploratory tunnels, which pinned
`BuildRequestor` explicitly refuses as paired tunnels for non-zero-hop
client builds (length check), and no opposite-direction client tunnel exists
yet to pair with. Four consecutive fresh-context attempts each exhausted the
full five-minute ceiling (~301–330 s) with `No tunnels built` and the exact
`couldn't find a paired tunnel` warn line in both pool directions.

This is diagnostic evidence only; it is not a product fix and does not prove
Java second-family destination or streaming interoperability. Router-B
answerability for the exact target LS2 is retained from Plan 224, not
re-proven here (the destination lane was correctly never entered). The
paired-tunnel requirement is stock-Java policy (`BuildRequestor` +
`BuildExecutor` exploratory kick-start runs unmodified); any future
corrective that changes tunnel length/quantity, exploratory settings,
paired-tunnel policy, or the helper profile needs its own plan-of-record —
none is authorized here.

Severity: no critical/high findings. Medium: the controlled one-hop client
build path stalls at paired-tunnel selection despite selectability and
adequate router tunnel counts (owned by a future narrow corrective, not by
profile scoring). Low: none new; the full five-minute ceiling was consumed
on every attempt because no exact fail-fast branch was implemented — the
terminal was conclusive at timeout epoch regardless.

## Roadmap and unblock audit

Plan 228 is formally closed as the bounded build-path attribution. The
unblock audit found no future plan eligible to resume without a new
corrective plan-of-record:

```text
plan_227 = passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary
plan_228 = passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary
plan_201 = blocked-pending-paired-tunnel-corrective-after-plan228-no-paired-tunnel
plan_204 = blocked-on-m6-java-second-family-closure-pending-paired-tunnel-corrective
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
active_plan = none
next_executable_plan = none (narrow paired-tunnel successor may be registered separately; not authorized here)
```

Plan 201 remains blocked because Plan 228 produced attribution, not
installed one-hop tunnels or Java second-family closure; its blocker is
narrowed from generic Plan-228 pendency to the specific paired-tunnel
corrective the terminal prescribes. Plan 204 remains blocked on that same
independent M6 closure; its M10 authority is unchanged. Plan 205 remains
retained/deferred because the observed boundary is below the SAM bridge
(specifically at paired-tunnel selection, below client-tunnel
establishment). No successor plan is registered here: the final terminal is
an attribution stop, not a `NEXT-BOUNDARY` terminal requiring a new
corrective plan in this commit. A future narrow plan may own the
paired-tunnel availability path (exploratory nonzero-hop presence, client
pool bootstrap order, or helper profile — each requiring its own
plan-of-record with justification that it is not a profile-manufacturing
bypass).

## Registration basis (retained)

Plan 227 closed with:

```text
P227-EXPLICIT-ONE-HOP-NOT-BUILT
```

on all three counted attempts at final implementation SHA
`b59bf5b77b33fab7784e4c8bfba24b7f5e46273b`.

Retained facts:

- Router C is present and valid in Router A's main NetDB;
- `ProfileOrganizer.isSelectable(C)=true`;
- Router C is not banlisted;
- the raw helper receives ordinary I2CP
  `inbound.explicitPeers` / `outbound.explicitPeers`;
- the helper requests one-hop inbound/outbound client tunnels with zero-hop
  disabled;
- stock Java reaches its approximately five-minute
  `I2PSession.connect()` ceiling without installing either client tunnel;
- Plan 227 did not proceed to the NetDB lookup or reverse-delivery stage.

Exact-pinned Java source shows several distinct pre-install stages that Plan
227 did not discriminate:

```text
client pool wants build
  -> selector/config creation
  -> BuildExecutor scheduling
  -> paired tunnel selection
  -> build-message creation
  -> A dispatch
  -> C receive/decrypt/respond
  -> A receive/decrypt statuses
  -> local join/install
```

For client tunnels, pinned `BuildRequestor` requires a paired tunnel. If no
opposite-direction client tunnel exists yet, Java falls back to router/
exploratory tunnel infrastructure. Absence of a suitable paired tunnel ends
the attempt as `OTHER_FAILURE` before Router C receives a build request.

Plan 228 therefore owned attribution only. It located the earliest missing
stage (`NO-PAIRED-TUNNEL direction=both`) and stopped without implementing a
workaround.

## Registration constraints (retained)

Plan 228 did not authorize:

- i2pr production changes;
- Java source patches;
- profile/tier manipulation;
- NetDB mutation;
- direct tunnel installation;
- exploratory/client tunnel setting changes;
- paired-tunnel policy overrides;
- VMComm;
- `netDb.alwaysQuery`;
- timeout increases;
- topology changes;
- Streaming-helper execution or changes.

Closure replaced the registration token with the single Plan-228
attribution terminal above and updated the dependency/unblock audit.
