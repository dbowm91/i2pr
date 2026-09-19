# Plan 222 status — M6 Java client-NetDB/OCMOSJ narrowing corrective

Status: **`passed-m6-java-client-netdb-ocmosj-narrowing-corrective`**.

Plan of record:
[`222-m6-java-client-netdb-ocmosj-narrowing-corrective.md`](../../implementation/mixed-router-interop/222-m6-java-client-netdb-ocmosj-narrowing-corrective.md).

## 0. Registration basis (retained from registration — historical context)

Exact-pinned Java I2P 2.13.0 source review verifies that Plan 221 should not be
executed unchanged.

### Selector-equivalence correction

The real `IterativeSearchJob`:

- computes the daily routing key with
  `ctx.routingKeyGenerator().getRoutingKey(key)`;
- calls `FloodfillPeerSelector` with the routing key, not the original
  Destination hash;
- uses effective `netdb.searchLimit + EXTRA_PEERS`, not a fixed N=3; and
- performs the lookup through `ctx.clientNetDb(fromDestinationHash)`.

Client sub-databases share the main k-buckets and peer selector, but maintain
their own transient LeaseSet storage; their `getAllRouters()` fallback is
empty.

### Stronger public per-message observation

The pinned public `I2PSession` API supports listener-enabled sends returning a
nonce:

```java
long sendMessage(..., SendMessageOptions options,
                 SendMessageStatusListener listener)
```

The same nonce is used for router admission and OCMOSJ terminal status
notifications. This allows exact reverse-send correlation without patching
Java, reflection, or private-state inspection.

Plan 222 therefore repairs selector equivalence first, then uses
`SendMessageStatusListener` as the primary OCMOSJ discriminator. Exact
router stats/log correlation is conditional fallback only.

### Listener limitation

The listener path is strongest for nonce-correlated admission and specific
terminal status codes. Its absence is not a terminal fact: the pinned client
`MessageState` default listener lifetime and OCMOSJ default overall timeout
are both 60 seconds, so a timeout notification may race listener expiration.
Plan 222 keeps the message expiration unchanged and uses bounded exact
stats/log correlation only for the accepted/no-terminal-callback branch.

## 1. Outcome

One exact-clean-head destination-only run emits exactly one `P222-*`
terminal derived from the real client lookup selector inputs and one
nonce-correlated helper send:

```text
p222-classification = P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION
```

The exact client-lookup preflight passes as production-equivalent
(`selector_equivalent=Known(true)`, nonempty selector with Router B as one
candidate), the tracked helper send is admitted (`TRACKED_SENT nonce=1`),
and the pinned OCMOSJ path returns the specific nonce-correlated failure
`STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION (17)` for that exact send while
i2pr observes no TunnelData and no payload inside the frozen 45-second
acceptance window. Per §12 F1 this proves OCMOSJ obtained or evaluated
destination/LeaseSet material far enough to return that specific failure;
the boundary is the OCMOSJ send-preparation encryption evaluation, not the
client-NetDB lookup (which demonstrably had candidates) and not a bootstrap
defect. No protocol/topology fix is implemented by this plan.

```text
i2pr_commit = cd334f838d6c80acbeedbcc2ebf6b1ae612a68fa
java_i2p    = 2.13.0 (9134f808337b401e8e53c73734c81fab04280c9d)
i2pd        = 2.61.0 (635b013a612ff47278ef02acf8580a28e10e26c5, pin frozen, not exercised by this lane)
os_image    = Linux-6.8.0-139-generic-x86_64-with-glibc2.39
rust        = 1.95.0 (59807616e 2026-04-14)
p222_terminal = P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION
```

## 2. Implementation commits

All implementation landed before the authoritative run (Plan 222 §17 WP K).
The tree was clean (`git status --porcelain=v1` empty) at the implementation
head before each lane attempt.

```text
cd334f8  plan222: exact client-NetDB preflight + nonce-tracked OCMOSJ narrowing
```

`cd334f8` carries the full WP A–J implementation plus WP I static guards.
No code, config, topology, or timeout changed between lane attempts.

Files changed (`cd334f8`):

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs                  | 1232 ++++-
scripts/check-m6-mixed-router-acceptance-evidence.sh              |  178 +-
tests/integration/m6-interop/java/ControlledRouter.java           |   77 +
tests/integration/m6-interop/java/ReferenceRawDestination.java    |  150 ++
tests/integration/m6-interop/run-java.sh                          |   26 +-
tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P222SelectorProbe.java (new)
```

No production i2pr wire changed. No Java source patched. No reflection. No
NetDB/tunnel mutation. No public I2P participation. No reference vendoring.
No VMComm. No SAM pivot. No topology expansion. No change to the 45-second
reverse-delivery acceptance window. No Java timeout property changed.

## 3. Exact source facts used for selector-width/routing-key logic

Verified by decompiling the staged exact-pinned jars
(`target/interop/cache/m6-java/9134f808337b401e8e53c73734c81fab04280c9d/lib/{i2p,router}.jar`),
not from memory:

- `IterativeSearchJob` constructor: `_rkey =
  ctx.routingKeyGenerator().getRoutingKey(key)`; `default_total = 3` iff
  `facade.floodfillEnabled()` and `ctx.router().getUptime() > 1800000 ms`
  (30 min), else `5`; `_totalSearchLimit =
  ctx.getProperty("netdb.searchLimit", default_total)`.
- `IterativeSearchJob.runJob()`: selector call is
  `selectFloodfillParticipants(_rkey, _totalSearchLimit + 1, kbs)` — the
  3-argument overload (`iconst_1; iadd` = `EXTRA_PEERS = 1`).
- `FloodfillPeerSelector` 3-argument overload delegates to the 4-argument
  form with `Collections.singleton(ctx.routerHash())`; the 4-argument form
  adds the router hash when absent. The production path therefore always
  self-ignores; the probe calls the same 3-argument overload, which is
  production-equivalent by construction. Passing an explicit empty set
  would NOT be equivalent (static guard rejects it on the P222 path).
- `RouterContext.clientNetDb(Hash)` exists and is the OCMOSJ client-DB
  path; `KademliaNetworkDatabaseFacade.isClientDb()` distinguishes the
  client facade from main fallback.
- `MessageStatusMessage` status codes match Plan 222 §2.5
  (`STATUS_SEND_ACCEPTED=1`, `BEST_EFFORT_FAILURE=3`,
  `GUARANTEED_SUCCESS=4`, `NO_TUNNELS=16`, `UNSUPPORTED_ENCRYPTION=17`,
  `BAD_LEASESET=19`, `EXPIRED_LEASESET=20`, `NO_LEASESET=21`,
  `META_LEASESET=22`, …). The long `I2PSession.sendMessage(Destination,
  byte[], int, int, int, int, int, SendMessageOptions,
  SendMessageStatusListener)` overload returning the nonce exists.

## 4. Authoritative-run evidence (attempt 2 of 2, same SHA)

Retry accounting (§17 K1, budget 3, no tuning between attempts):

- Attempt 1: harness exited pre-bootstrap — ephemeral Java service router
  SAM never bound (`ephemeral Java I2P SAM did not listen`). No driver ran;
  no `p222-classification` emitted. Fresh scratch context, no code/config
  change. Infrastructure flake, not a diagnostic outcome.
- Attempt 2 (AUTHORITATIVE): full destination-only run on `cd334f8`,
  clean tree, fresh scratch RouterContexts. Exactly one
  `p222-classification` emitted (destination
  `driver-evidence.tsv:45`); `evidence.json` `i2pr_commit` equals the
  implementation SHA.

### 4.1 Frozen Plan 220 facts (retained authority)

The P220 classifier still emits `P220-OBSERVABILITY-GAP-CLIENT-NETDB`
with every earlier stage `Known(pass)`; the J219-B refutation reproduces:

```text
rust_b_hash_hex = 84b16ab67292a2d8ae70d409c87dfff81cd66ca415c284cfbccec814f037315d
java_b_self_hex = Known("84b16ab6…f037315d") hash_match = Known(true)
A-stored-B: present=true identity_match=true f=true
  stored_sha256 = live_sha256 = 2581de1ce036f13785bb3ca9f1c246a6e6a64bb3747a45aff1ed969538f37225
  stored_published = live_published = 1789795946
PeerManager B indexed=true; P220 selector contains B (historical row only)
forward_i2pr_to_java_received = Known(true) (27 B digest ec61e08d… match)
lease-lookup-completed leases=1
reverse_java_send_admitted = Known(true) (tracked admission)
reverse TunnelData/payload in 45 s = Known(false)/Known(false)
```

The historical P220 selector row is NOT consumed as P222 authority (WP A).

### 4.2 Exact client-lookup preflight (WP B)

`P222-CLIENT-LOOKUP-PREFLIGHT <helper-DBID> <target> <router-B>` against
Router A at the post-driver-bootstrap/pre-reverse-send epoch:

```text
observable=true client_db_resolved=true client_db_is_client=true
target_hash_hex   = 665b740f4058a084f96eb30ded102aabce764bd303d9e5006d8e4bbaaca68429
routing_key_hex   = a02c81a31449c56ca1d274d40205e581e9df2ff73bec6fc6188bf295edd5b150
routing_key_differs = true
target_ls_present_before_send = false
facade_floodfill_enabled = false router_uptime_ms = 179398
netdb_search_limit_effective = 5 selector_extra_peers = 1 selector_width = 6
selector_input_kbucket_size = 4 selector_count = 2
selector_contains_b = true selector_empty = false
```

Interpretation per §8 B6: the helper client DBID resolved to an actual
client DB (not main fallback); Java derived a distinct routing key; the
width 6 = effective 5 + 1 matches pinned `IterativeSearchJob` logic
(uptime 179 s < 30 min ⇒ default 5 — consistent); the selector is nonempty
so the lookup has candidates; B present proves B is one candidate, not
that B was queried; B absence would not have been a root cause. The empty
selector branch did not fire.

Limitations (low severity, evidence-shape only): the helper client DBID hex
(the sha256 of the helper destination behind
`JAVA_RAW_REFERENCE_DESTINATION_B64`) is passed to the probe but the
aggregated `p222-client-lookup-preflight` row echoes resolved/is-client
rather than the DBID hex itself; the bounded selected peer hashes (≤8) are
returned by the Java probe response but the aggregated row persists
count/membership rather than each hash. Count, membership, width, and both
keys are fully recorded; the terminal depends on none of the omitted
hashes.

### 4.3 Tracked send + ordered listener statuses (WP C–E)

```text
TRACKED_SENT nonce=1 payload_len=27
  digest = 8a9e8146bb7d8c0b32b19b2483913d9f38aab1c7bfc925b1a73e7cd5aebb2271
  reverse_sha256 = 8a9e8146…ebb2271 (equal — intended reverse payload)
ordered_statuses = [17]
  17 = STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION
status_accepted = Unknown("accepted-not-observed")
```

`SEND_TRACKED` used the public listener-enabled long `sendMessage` with a
default `SendMessageOptions` (no expiration/reliability/tunnel change; the
only behavior difference from legacy `SEND` is the status listener).
`reverse_java_send_admitted=Known(true)` was set only after the tracked
call returned the valid nonce. No ACCEPTED callback was observed for this
nonce; per §2.7 an early/specific terminal failure remains strong
nonce-correlated evidence while callback absence stays Unknown (it is
recorded Unknown, not false).

### 4.4 Frozen 45-second window + status-only extension (WP E)

```text
frozen_tunneldata_45s = false frozen_payload_45s = false
p222-status-only-observation: nonce=1 events=[17]
```

The i2pr inbound pump ran exactly `DATAGRAM_WAIT` (45 s) with periodic
non-delaying `SEND_STATUS` polls against a wall-clock-fixed deadline; the
payload result froze at 45 s. The decisive status (17) was already present
at the freeze boundary, so the status-only extension to 70 s broke
immediately with no new events and did not resume payload acceptance. The
later deadline was never reused for the payload row (unit J16 locks this).

### 4.5 P222 terminal

```text
P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION
selector_equivalent=Known(true) selector_nonempty=Known(true)
target_leaseset_present_pre_send=Known(false) tracked_nonce=Known(1)
status_unsupported_encryption=Known(true), all other status facts Known(false)
  except status_accepted/status_other_terminal per §4.3
reverse_i2pr_tunneldata_observed_45s=Known(false)
reverse_i2pr_payload_recovered_45s=Known(false)
```

Package G fallback was not reached (a decisive terminal status exists), so
no router stat/log correlation was implemented, per §13.

## 5. Requirement-to-evidence matrix (Plan 222 §19)

### Selector equivalence

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | helper source Destination hash used as client DBID | PASS | `helper_client_dbid_hex` from `reference_hash`; `client_db_resolved=true` |
| 2 | `clientNetDb(clientDbid)` resolves to client DB | PASS | `client_db_is_client=true` (not main fallback) |
| 3 | Java derives routing key with own generator | PASS | `routingKeyGenerator().getRoutingKey` in probe; `routing_key_hex` recorded |
| 4 | raw target hash and routing key both recorded | PASS | both hexes in preflight row; `routing_key_differs=true` |
| 5 | selector width matches pinned logic | PASS | 5 + 1 = 6; uptime 179398 ms ⇒ default 5 consistent |
| 6 | no hard-coded N=3 | PASS | width 6; checker rejects `PROBE_FANOUT` on P222 path |
| 7 | production-equivalent self-ignore overload | PASS | 3-arg overload; decompile proves internal singleton self-ignore |
| 8 | selected peers bounded and recorded | PASS with limitation | count=2, contains_b=true, kbuckets=4; per-hash persistence gap noted in §4.2 |
| 9 | empty vs nonempty semantics distinct | PASS | classifier + `p222_selector_empty_maps_to_no_lookup_peer` |
| 10 | B absence from nonempty selector is not a root cause | PASS | classifier falls to gap; `p222_selector_nonempty_b_absent_is_not_root_cause` |

### Per-message send correlation

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 11 | public listener-enabled `sendMessage` | PASS | helper `SEND_TRACKED` via long overload + `SendMessageStatusListener` |
| 12 | unique nonce returned and recorded | PASS | `nonce=1` in tracked row |
| 13 | length/digest match intended payload | PASS | 27 B; digest == reverse_sha256 |
| 14 | all listener events retained in order | PASS | `ordered_statuses=[17]`; helper keeps ≤16 ordered events; J8 unit row |
| 15 | ACCEPTED treated only as admission | PASS | classifier F1; here ACCEPTED unobserved ⇒ Unknown, never a pass (J9) |
| 16 | exact failure statuses map only to documented boundaries | PASS | 17 ⇒ `OCMOSJ-UNSUPPORTED-ENCRYPTION` (F1) |
| 17 | no listener status synthesized from logs | PASS | events only from helper callbacks; G fallback not implemented |
| 18 | no missing callback becomes false | PASS | `status_accepted=Unknown`; J14 |
| 19 | legacy `SEND` remains compatible | PASS | `case "SEND":` retained; checker I2 |

### Timing/evidence

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 20 | 45-second payload window unchanged | PASS | `DATAGRAM_WAIT` 45 s; frozen vars; J16 |
| 21 | status-only observation independently bounded ≤75 s | PASS | 70 s deadline constant |
| 22 | later status cannot turn payload fail into pass | PASS | frozen vars feed P220/P222 payload facts; J16 |
| 23 | exact implementation SHA precedes run | PASS | `cd334f8` == `evidence.json` `i2pr_commit` |
| 24 | authoritative run starts clean | PASS | porcelain empty at head before attempts |
| 25 | no more than 3 exact-head attempts | PASS | 2 attempts (1 pre-bootstrap infra flake + 1 authoritative) |
| 26 | no tuning between attempts | PASS | same SHA; fresh scratch contexts; no config change |
| 27 | exactly one P222 terminal per attempt | PASS | attempt 1: no driver ran (harness exited at SAM wait — documented deviation, same class as Plan 220 §12); attempt 2: exactly one `p222-classification` |
| 28 | one authoritative terminal selected | PASS | attempt 2 `OCMOSJ-UNSUPPORTED-ENCRYPTION` |
| 29 | Plan 193 and Plan 215 untouched | PASS | diff touches diagnostics/evidence only; their checkers green |
| 30 | no protocol/topology corrective leaks | PASS | Java helper + probe + driver + harness/checker only |

## 6. Tests and guards run with outcomes

Routine floor (local, on `cd334f8` unless noted; lane evidence in
`target/interop/m6-java-evidence`, untracked scratch):

```text
cargo fmt --all --check                                                  OK
cargo check --locked --workspace --all-targets                           OK
cargo test --locked --workspace --all-targets -- --test-threads=1        2484 passed, 16 ignored (103 suites, ~637 s)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings   No issues found
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps      OK
cargo test --locked --workspace --doc                                   0 passed (16 suites)
bash scripts/check-dependency-direction.sh                              dependency direction: ok
bash scripts/check-runtime-boundaries.sh                                runtime boundary checks passed
bash scripts/check-service-tunnel-boundaries.sh                         service-tunnel boundary checks passed
bash scripts/check-fixture-manifest.sh                                  (no output)
bash scripts/check-ntcp2-vectors.sh                                     NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh                                      SSU2 vector manifest is complete and hashes match.
bash scripts/check-i2cp-vectors.sh                                      I2CP vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh                            Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh                    Plan 077 constrained-host lane boundary checks passed
bash scripts/check-sam-acceptance-evidence.sh                           SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh                          SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh                          I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
bash scripts/check-service-tunnel-acceptance-evidence.sh                service-tunnel acceptance evidence integrity: 29 rows command-derived, 2 rows blocked, no literal pass records (+ Plans 202/210/212–215 invariants green)
bash scripts/check-exploratory-tunnel-evidence.sh                       exploratory tunnel evidence check passed (12 guarded labels)
bash scripts/check-netdb-tunnel-evidence.sh                             NetDB evidence check passed (12 guarded labels)
bash scripts/check-destination-tunnel-evidence.sh                       destination evidence check passed (21 guarded labels, both i2pd and java harnesses)
bash scripts/check-streaming-tunnel-evidence.sh                         Plan 193 streaming evidence check passed (33 guarded labels, helpers wired)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh               m6 mixed-router evidence check passed (11 guarded labels, …, Plan 222 §15 exact-selector/tracked-send invariants)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'   18 tests OK
cargo deny check advisories bans sources                                 advisories ok, bans ok, sources ok
```

Focused suites on `cd334f8`:

```text
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1    41 passed
cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1       30 passed (11 p220_* + 18 p222_* + java_pq_capabilities_surfaced), 3 ignored (fail-closed ordinary invocation)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p222 -- --test-threads=1  18 passed
cargo test --locked -p i2pr-daemon --test java_tunnel_external p220 -- --test-threads=1  11 passed (+ pq row under broader filter)
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run                  OK (exit 0)
```

External lane (exact-clean-head, `I2PR_M6_JAVA_DRIVER=destination`):

```text
attempt 1 (cd334f8, clean): harness exited pre-bootstrap, ephemeral Java SAM never bound; no driver ran; no classification emitted.
attempt 2 (cd334f8, clean, AUTHORITATIVE): lease-lookup-completed (leases=1), forward digest match (27 B, ec61e08d…),
  P220-OBSERVABILITY-GAP-CLIENT-NETDB (frozen) + P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION (§4);
  harness row external-p222-classification=passed (diagnostic observation);
  Plan 199 aggregator still exits non-zero on destination-inbound/streaming rows (expected: destination-only run + Plan 218 boundary reproduces);
  workspace-gates row passed inside the lane.
```

Deviation from §17 (documented, no concealment): attempt 1 stopped before
any driver epoch because Java SAM never bound — the same pre-epoch
infrastructure class Plan 220 §12 documented. Both attempts ran the
unmodified committed harness on the same SHA with fresh scratch contexts;
no topology, window, or bootstrap tuning was applied between attempts. The
M6 final-closure checker remains correctly unmet (`m6_java: failed` —
reverse delivery still absent; streaming rows fail on a destination-only
run by construction).

## 7. Findings by severity

- **Critical**: none.
- **High**:
  - **H-1 (new, closed by this status) — the Java → i2pr reverse failure
    is narrowed to OCMOSJ send-preparation encryption evaluation.**
    The exact client lookup has candidates (width-6 selector over the
    helper client DB, B included), the tracked send is admitted
    (nonce=1), and OCMOSJ answers that exact nonce with
    `STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION (17)` while the frozen
    45-second i2pr window sees no TunnelData/payload. Per F1 the
    destination/LeaseSet material was evaluated far enough to fail
    specifically at encryption support — not at lookup-peer selection
    and not at bootstrap (J219-B stays refuted; P220 RI/PeerManager facts
    reproduce).
  - **H-2 (retained) — the Plan 218 reverse-delivery boundary
    reproduces.** `destination-inbound` still fails; `m6_java: failed`.
    The boundary now carries an exact OCMOSJ attribution instead of an
    observability gap.
- **Medium**:
  - **M-1 — no ACCEPTED callback observed for nonce=1.** The send was
    admitted at the control protocol (`TRACKED_SENT`), but the listener
    sequence is `[17]` without a preceding `1`. Treated per §2.7 as
    Unknown admission-callback, not as non-admission; the specific
    terminal (17) stands on its own. A follow-up may check whether the
    helper should also surface the admission callback for full
    ACCEPTED→terminal sequencing, without changing message lifetime.
  - **M-2 — helper-LS2 publication race persists as the pre-epoch risk.**
    Attempt 1 never reached bootstrap (SAM bind). The 3-attempt budget
    with fresh contexts remains the correct process control; no tuning.
- **Low**:
  - **L-1 — aggregated evidence omits DBID hex + per-peer hashes**
    (§4.2). Count/membership/width/keys are recorded; the terminal does
    not depend on the omitted values. A follow-up may persist
    `client_dbid_hex` and `peer_*_hex` into `driver-evidence.tsv` if a
    future boundary needs them.
  - **L-2 — streaming rows fail on a destination-only run by
    construction.** `I2PR_M6_JAVA_DRIVER=destination` never starts the
    streaming helper; those `failed` rows are lane-scoping artifacts,
    not regressions (unit/live Streaming rows pass locally).

## 8. Roadmap disposition and next plan

Plan 222 closes as
**`passed-m6-java-client-netdb-ocmosj-narrowing-corrective`**.
The P222 diagnostic path (exact preflight, tracked send, frozen 45 s +
70 s status-only extension, §15 guards) is reusable by the corrective
pass.

The smallest next plan implied by the evidence is a NEW plan-of-record
(not authorized by Plan 222) owning the
`OCMOSJ-UNSUPPORTED-ENCRYPTION` corrective: inspect what the i2pr
destination's published LeaseSet2 carries for encryption (lease
construction, encType, LS2 signing/encryption material) against what
Java OCMOSJ send-preparation accepts for a `PROTO_DATAGRAM_RAW`
client send, then target the exact mismatch. No bootstrap, floodfill,
tunnel-length, or publication-topology change is implicated by this
evidence and MUST NOT be smuggled into that plan.

## 9. Unblock audit

Per the planning process, every registered plan listing Plan 222 as a
hard or interface dependency was audited:

- **Plan 201** (M6 Java publication corrective + second-family closure):
  was `blocked-pending-plan222-client-netdb-ocmosj-narrowing`. Plan 222
  delivered the narrowing, but the boundary it names
  (`OCMOSJ-UNSUPPORTED-ENCRYPTION`) needs a dedicated corrective before
  any 201 closure work can target it. **Plan 201 moves to
  `blocked-pending-ocmosj-unsupported-encryption-corrective-after-plan222`**
  (amended in `201-status.md` in the same commit). Not unblocked.
- **Plan 204** (M10 final closure convergence): stays blocked — the M6
  Java lane is narrowed, not closed. Token updated to
  `blocked-on-m6-java-second-family-closure-pending-ocmosj-corrective-after-plan222`
  (amended in `204-status.md` in the same commit). Not unblocked.
- **Plan 205** (SAM-bridge pivot, retained-conditional): stays
  `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`.
  The narrowed boundary (OCMOSJ encryption evaluation below the client
  send API) is not a layer a SAM bridge would replace; no reactivation
  authorized. Registry note updated; status-file token unchanged.
- **Plan 218** (stopped reverse-delivery boundary): stays
  `stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary`.
  Its behavioral stop reproduces under corrected observation (attempt 2);
  its attribution is now exact (§7 H-1). A short follow-up note is
  appended to `218-status.md` in the same commit; the stop token is
  unchanged.
- **Plan 220**: stays
  `passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required`
  (token unchanged — history is not rewritten). The selector-equivalence
  follow-up it required is now complete via this closure; a short
  completion note is prepended to `220-status.md` in the same commit.
- **Plan 221**: stays
  `superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective`.
- **M6 roadmap**: §7 row for 222 flips to
  `passed-m6-java-client-netdb-ocmosj-narrowing-corrective`; §7 row for
  201 flips to the new blocked token; §4/§11/§12 authority text updated.

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = passed-m6-java-client-netdb-ocmosj-narrowing-corrective
plan_201 = blocked-pending-ocmosj-unsupported-encryption-corrective-after-plan222
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-ocmosj-corrective-after-plan222

milestone6_i2pd_streaming_interop    = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION; dedicated corrective owns the fix)
milestone6_interoperable             = not-yet-claimed
```

No plan-of-record was silently unblocked.

## 10. Compatibility and migration

No user-facing compatibility or support change. Evidence authority changes
only: the Plan 220 selector row stays historical; Plan 222 governs the
corrected client-NetDB/OCMOSJ attribution; downstream work consumes Plan
222, never the P220 selector pass or J219-B. `milestone6_interoperable =
not-yet-claimed` is unchanged. No `specs/support.toml` / `docs/adr/`
change: no qualification is claimed.

## 11. Handoff

```text
plan_222 = passed-m6-java-client-netdb-ocmosj-narrowing-corrective

plan_201 = blocked-pending-ocmosj-unsupported-encryption-corrective-after-plan222
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-ocmosj-corrective-after-plan222

next_executable_plan = none-pending-new-plan-of-record-for-ocmosj-unsupported-encryption-corrective
```

No bootstrap/topology/protocol corrective is registered or authorized by
this closure. The open M6 Java second-family boundary is the OCMOSJ
unsupported-encryption evaluation for the tracked reverse send; its fix
belongs to a new plan.
