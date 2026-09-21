# Plan 229 registration follow-up

Plan 227 remains closed at `P227-EXPLICIT-ONE-HOP-NOT-BUILT`. Plan 228
proved why that helper could not build: client configs through C were valid,
but no non-zero paired tunnel was available in either direction. Plan 229 is
the registered corrective for that exact prerequisite and does not reopen
Plan 227 evidence.

```text
plan_227 = passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary
plan_228 = passed-m6-java-client-tunnel-build-path-attribution-with-no-paired-tunnel-boundary
plan_229 = registered-ready-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective
next_executable_plan = 229-m6-java-nonzero-exploratory-paired-tunnel-bootstrap-corrective
```

# Plan 228 registration follow-up

Plan 227 remains closed at
`P227-EXPLICIT-ONE-HOP-NOT-BUILT`. Pinned Java source review shows the next
unresolved path is not peer selectability but stock tunnel construction:
client config creation, BuildExecutor scheduling, paired-tunnel availability,
build-message creation/dispatch, Router-C request handling, reply processing,
and local join.

Plan 228 is registered as attribution-only and must stop at the earliest
proven missing stage.

```text
plan_227 = passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary
plan_228 = registered-ready-m6-java-client-tunnel-build-path-attribution
next_executable_plan = 228-m6-java-client-tunnel-build-path-attribution
```

No workaround, exploratory/client tunnel policy change, profile mutation,
VMComm, `netDb.alwaysQuery`, topology change, or timeout increase is
authorized.

# Plan 227 status — M6 Java explicit one-hop client-tunnel corrective

Status: **`passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary`**.

Plan of record:
[`227-m6-java-explicit-one-hop-client-tunnel-corrective.md`](../../implementation/mixed-router-interop/227-m6-java-explicit-one-hop-client-tunnel-corrective.md).

## Closure result

Plan 227 is closed as a completed, fail-closed reference-harness corrective.
It replaced only the raw helper's zero-hop client profile with a genuine
stock-Java one-hop profile through controlled Router C via ordinary I2CP
SessionConfig `inbound.explicitPeers` / `outbound.explicitPeers`, proved
Router C selectable in A's main NetDB before helper start, derived the exact
Router-C I2P Base64 identity via the authoritative `P224-HASH-B64` renderer,
and reran the frozen destination lane. All three counted attempts on the
final implementation SHA proved the same build-stop terminal:

```text
P227-EXPLICIT-ONE-HOP-NOT-BUILT
```

with Router C selectable. No helper zero-hop tunnel was accepted as
qualification, no lookup behavior was interpreted past the build gate, and no
M6 Java interoperability claim follows. This is the prescribed §12
preflight/build-stop behavior, not a production protocol fix.

Authoritative per-attempt facts (final SHA `b59bf5b`):

```text
attempt=1 router_c_hex=d4aa002955d5108e34fc240c7be2ddef110cdfe9d233d2cf8089254efd94f44a
  main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false
  b64_len=44 renderer=P224-HASH-B64 helper_connect_elapsed_ms=301787 helper_ready=false helper_connect_timeout_seen=true
  terminal=P227-EXPLICIT-ONE-HOP-NOT-BUILT

attempt=2 router_c_hex=bdccf4ad588f9fc1a012daa56cf8335910101b8962bd817b173b6efd108a6a4d
  main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false
  b64_len=44 renderer=P224-HASH-B64 helper_connect_elapsed_ms=301408 helper_ready=false helper_connect_timeout_seen=true
  terminal=P227-EXPLICIT-ONE-HOP-NOT-BUILT

attempt=3 router_c_hex=9695740c2682caf2ba5796db232f31b7e3eb8493267c89ca13c2726738b4a8ad
  main_raw_present=true main_valid_present=true selectable=true established=false banlisted=false
  b64_len=44 renderer=P224-HASH-B64 helper_connect_elapsed_ms=301356 helper_ready=false helper_connect_timeout_seen=true
  terminal=P227-EXPLICIT-ONE-HOP-NOT-BUILT
```

Because the helper never connected, no installed one-hop pools existed to
snapshot, the exact Plan-226 target search was never re-observable, and the
frozen 45-second reverse-payload window was never entered. The zero-hop guard
(`b_zero_hop_unknown_rejected`) is therefore moot for these runs: there were
no client tunnels at all, not a zero-hop tunnel that triggered the guard.
Per §17, while C is selectable the next investigation is the real stock-Java
tunnel build request/reply path — not peer profile scoring.

## Implementation commits and pinned inputs

Implementation was committed before the counted attempts (Plan 227 §14):

```text
fed29b7 interop: implement Plan 227 explicit one-hop client-tunnel corrective
b415d23 interop: fix Plan 227 I2P Base64 grep class
70a5a40 interop: fix Plan 227 Base64 length and cut handling
f4fbe83 interop: handle Plan 227 five-minute helper timeout as counted terminal
b59bf5b interop: fix Plan 227 helper timeout flag on PID death
```

Counted attempts ran only on final SHA `b59bf5b`:

```text
b59bf5b77b33fab7784e4c8bfba24b7f5e46273b
```

Reference inputs remained frozen: Java I2P `2.13.0` at
`9134f808337b401e8e53c73734c81fab04280c9d`; the i2pd reference pin remained
`2.61.0` at `635b013a612ff47278ef02acf8580a28e10e26c5`. No dependency,
fixture, production protocol, or Java reference source changed. No Java
source patch, reflection, profile/tier mutation, client-NetDB RI store,
direct tunnel install, VMComm, `netDb.alwaysQuery`, public I2P, distinct
topology, streaming-helper change, or reverse-window change.

## Requirement-to-evidence matrix

| Plan-227 requirement | Evidence / result |
|---|---|
| Router C proven present/valid/selectable before helper start (§6, §16.2) | All 3 attempts: `main_raw_present=true main_valid_present=true selectable=true` via read-only `P227-PEER-ELIGIBILITY` on Router A; `established=false banlisted=false` diagnostic only. |
| Raw helper receives Router C through ordinary SessionConfig explicitPeers (§8, §16.3) | Helper `options()` sets `inbound.length=1 outbound.length=1 allowZeroHop=false inbound/outbound.explicitPeers=<C-B64>` when the 5th arg or `I2PR_M6_JAVA_EXPLICIT_PEER_B64` is present; counted harness always passes the 5th arg derived via `P224-HASH-B64`; no router-global `explicitPeers`. |
| No profile/tier manufactured (§16.4) | Probe uses only `isSelectable`, `isEstablished`, `isBanlisted`, NetDB local reads, tunnel-manager pool reads; static guard rejects `registerKeys`, `.store(`, reflection, tier promotion. |
| Real inbound one-hop through C installed (§16.5) | Not built: helper `connect()` threw `No tunnels built after waiting 5 minutes` on all 3 attempts; no `P227-CLIENT-TUNNELS` pools to snapshot; terminal is `NOT-BUILT`, not a pass. Recorded honestly. |
| Real outbound one-hop through C installed (§16.6) | Same as inbound: not built on all 3 attempts. |
| No zero-hop accepted (§16.7) | Tunnel gate requires `inbound_zero_hop_present=false outbound_zero_hop_present=false` plus exact `local+C` length-2 path; zero-hop fallback maps to `NOT-BUILT`; unit rows lock inbound-only, outbound-only, zero-hop insufficiency. |
| Router B remains answerable (§16.8) | Retained from Plan 224 (`receivedAsPublished=true` target LS2); not re-proven in NOT-BUILT runs because the destination driver was correctly skipped before publication/lookup. No contrary evidence. |
| Exact Plan-226 target search observable (§16.9) | Not re-observable: no helper tunnels means no client lookup ran; `p226-target-job-trace` rows from prior plans retained, not reinterpreted. |
| Zero-hop rejection recorded (§16.10) | Moot: no tunnels existed, so the zero-hop branch could not fire; classifier would emit `EVIDENCE-CONTRADICTION-ONE-HOP-BUT-ZERO-HOP-UNKNOWN` only if one-hop pools were proven and the branch still fired (did not occur). |
| A sends lookup to B recorded (§16.11) | Not reached: build gate failed before reverse send; honestly recorded as NOT-BUILT. |
| B receive/answer and A client-tunnel/store stages (§16.12) | Not reached; same honest stop. |
| Tracked send statuses (§16.13) | No tracked send admitted (no helper); no statuses invented. |
| Unchanged 45-second digest result (§16.14) | Never entered; `DATAGRAM_WAIT` remains `Duration::from_secs(45)` (static guard); no payload claimed. |
| Exactly one P227 terminal (§16.15) | Each attempt emits exactly one `p227-classification` row: `P227-EXPLICIT-ONE-HOP-NOT-BUILT`; shell early-stop and driver authoritative paths are mutually exclusive; unit row locks single-emission plus frozen-payload-wins. |
| Focused and routine verification (§16.16) | See Verification below. |
| Closure/registry/roadmap/unblock (§16.17) | This record plus registry/roadmap updates in the same commit. |

## External attempt history

Each attempt used fresh disposable Java RouterContexts, the same A/B/C
loopback baseline topology, the same raw-helper one-hop profile, the same
Router-C derivation (`P220-SNAPSHOT` hex validated as 64 lowercase hex,
`P224-HASH-B64` render, 43–44 char I2P Base64 strictly validated in Java),
the same target/publication path (not entered), and no tuning between
retries, per §14. Maximum three counted attempts per SHA observed.

| Attempt | Implementation SHA | Evidence directory | P227 result |
|---|---|---|---|
| 1 | `b59bf5b` | `target/interop/m6-java-p227-attempt1` | `P227-EXPLICIT-ONE-HOP-NOT-BUILT`, C selectable, 301787 ms, timeout seen |
| 2 | `b59bf5b` | `target/interop/m6-java-p227-attempt2` | `P227-EXPLICIT-ONE-HOP-NOT-BUILT`, C selectable, 301408 ms, timeout seen |
| 3 | `b59bf5b` | `target/interop/m6-java-p227-attempt3` | `P227-EXPLICIT-ONE-HOP-NOT-BUILT`, C selectable, 301356 ms, timeout seen |

Pre-counted diagnostic runs on prior SHAs (`fed29b7` grep-class bug,
`b415d23` cut/`=` truncation, `70a5a40` length check, `f4fbe83` timeout-exit
handling) are not counted; they exposed and fixed harness bugs without
changing Java selection semantics. No helper retry occurred inside any single
counted run after Java's five-minute failure (§8).

The enclosing legacy Plan-199 Java wrapper exited nonzero on all attempts
because install-dependent destination/streaming rows remain unqualified.
The Plan-227 diagnostic rows (`external-p227-classification`,
`external-p227-peer-eligibility`, `external-p227-explicit-peer-derivation`,
`external-p227-helper-connect`, `workspace-gates`) passed; raw Java logs
remained scratch-only.

## Verification

Successful verification on final SHA `b59bf5b` (local truth, not CI):

```text
cargo fmt --all --check                                      PASS
cargo check --locked --workspace --all-targets                PASS
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                                PASS (2544 passed, 16 ignored)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                                PASS
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                                PASS
cargo test --locked --workspace --doc                         PASS (0 doc tests)
cargo deny check advisories bans sources                     PASS
bash scripts/check-dependency-direction.sh                   PASS
bash scripts/check-runtime-boundaries.sh                     PASS
bash scripts/check-m6-mixed-router-acceptance-evidence.sh    PASS (§20 invariants)
bash scripts/check-destination-tunnel-evidence.sh            PASS
bash scripts/check-streaming-tunnel-evidence.sh              PASS
cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
                                                                PASS (14 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p226_ -- --test-threads=1
                                                                PASS (7 passed)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p225_ -- --test-threads=1
                                                                PASS (per prior closure; re-verified via workspace floor)
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
                                                                PASS
bash -n tests/integration/m6-interop/run-java.sh             PASS
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh PASS
```

Focused Plan-227 unit rows (14, all green locally):

```text
p227_c_selectable_passes_eligibility_gate
p227_c_non_selectable_maps_to_not_selectable
p227_exact_c_identity_mismatch_rejected
p227_inbound_only_tunnel_is_insufficient
p227_outbound_only_tunnel_is_insufficient
p227_zero_hop_fallback_is_insufficient
p227_exact_one_hop_both_directions_passes_gate
p227_one_hop_plus_zero_hop_contradiction
p227_one_hop_no_b_query_maps_to_next_boundary
p227_b_query_reply_client_store_downstream
p227_changed_send_status_maps_to_next_boundary
p227_digest_matched_delivery_passes
p227_secret_raw_log_rejected
p227_classification_is_single_and_frozen_payload_wins
```

## Security, compatibility, and operational decisions

- No production crate or protocol behavior changed; all deltas are in the
  external Java driver (`P227Probe.java`, `ControlledRouter.java`
  read-only commands), the test-only raw helper SessionConfig scoping,
  the `run-java.sh` harness gates, the `java_tunnel_external.rs`
  classifier/unit rows, and the static evidence checker.
- No Java source patching, reflection, private-state mutation, NetDB/key/
  tunnel injection, publication retry, search-limit change (beyond the
  prescribed length-1 profile), timeout change (readiness budget covers
  Java's own five-minute ceiling; the 45-second payload window is frozen),
  or `netDb.alwaysQuery` override was used.
- All SSU2 hosts remain `127.0.0.1` baseline; the distinct `127.0.1.1/`
  `127.0.2.1`/`127.0.3.1` topology was rejected by construction for P227
  counted runs (Plan 226 IP-close filter remains disproved).
- Durable evidence contains only bounded booleans, counts, hex hashes,
  B64 lengths, elapsed milliseconds, and status facts. Raw Java logs,
  peer lists, keys, tags, SessionConfig contents, and payloads remain
  scratch-only or are redacted; parsers reject secret-bearing rows.
- Java and i2pd pins, SAM/I2CP/diagnostic loopback policy, and frozen
  45-second reverse-payload authority remain unchanged.
- An explicit-peer tunnel is never considered proven from configuration
  alone; only installed `TunnelPool.listTunnels()` state with the exact
  `local+C` length-2 path would qualify (never observed here).

## Findings and limitations

No security finding was introduced. The plan proved its narrow
selectability gate (Router C is present, valid, and selectable in A's main
NetDB on all attempts) but disproved the implicit liveness assumption that
a selectable peer yields a one-hop client tunnel within Java's five-minute
`I2PSession.connect()` ceiling in the controlled loopback topology. Three
consecutive fresh-context attempts each exhausted the full ceiling
(~301 s) with `No tunnels built`. The corroborative `TunnelPeerSelector` /
`ClientPeerSelector` log presence (1868 selector lines in attempt 1) shows
selection machinery ran; no build success/reject/timeout strings matched
the narrow whitelist, consistent with a silent build-path stall rather than
an explicit reject.

This is diagnostic evidence only; it is not a product fix and does not
prove Java second-family destination or streaming interoperability. The
zero-hop guard (`P226-BASELINE-B-ZERO-HOP-UNKNOWN`) was not re-tested
because no client tunnels existed to carry a lookup. Router-B
answerability for the exact target LS2 is retained from Plan 224, not
re-proven here.

Severity: no critical/high findings. Medium: the controlled one-hop build
path stalls despite selectability (owned by a future build-request/reply
investigation, not by profile scoring). Low: helper-timeout flag logic
required a PID-death elapsed check (fixed in `b59bf5b`); pre-fix attempts
are diagnostic only.

## Roadmap and unblock audit

Plan 227 is formally closed as the bounded explicit-one-hop corrective.
The unblock audit found no future plan eligible to resume:

```text
plan_226 = passed-m6-java-loopback-peer-diversity-corrective-with-exact-baseline-non-ip-pre-dispatch-boundary
plan_227 = passed-m6-java-explicit-one-hop-client-tunnel-corrective-with-selectable-c-but-not-built-boundary
plan_201 = blocked-pending-build-path-investigation-after-plan227-selectable-c-not-built
plan_204 = blocked-on-m6-java-second-family-closure-pending-build-path-investigation
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
active_plan = none
next_executable_plan = none (narrow build-request/reply successor may be registered separately; not authorized here)
```

Plan 201 remains blocked because Plan 227 did not produce installed
one-hop tunnels or Java second-family closure; its blocker is narrowed
from generic Plan-227 pendency to the specific build-path investigation
the terminal prescribes. Plan 204 remains blocked on that same independent
M6 closure; its M10 authority is unchanged. Plan 205 remains
retained/deferred because the observed boundary is below the SAM bridge
(specifically below client-tunnel establishment). No successor plan is
registered here: the final terminal is a build-stop, not a `NEXT-BOUNDARY`
terminal requiring a new corrective plan in this commit. A future narrow
plan may own the stock-Java tunnel build request/reply path.

## Registration basis (retained)

Plan 226 closed with the exact target-job terminal:

```text
P226-BASELINE-B-ZERO-HOP-UNKNOWN
```

Pinned/current Java I2P provides `explicitPeers` as a public I2CP
debug/testing option. Through ordinary SessionConfig processing it can
select a known/selectable Router C for a genuine one-hop client tunnel
without manufacturing fast/high-capacity profile state. Plan 227 owned the
bounded reference-harness corrective to prove C selectable, install/prove
one-hop raw-helper tunnels through C, and rerun the frozen destination
lookup. Profile mutation, VMComm, `netDb.alwaysQuery`, direct tunnel
install, and client-NetDB RI injection remained forbidden throughout.
