# Plan 224 status — M6 Java NO_LEASESET lookup-path attribution

Status: **`passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap`**.

Plan of record:
[`224-m6-java-no-leaseset-lookup-path-attribution.md`](../../implementation/mixed-router-interop/224-m6-java-no-leaseset-lookup-path-attribution.md).

## 1. Closure result

Plan 224 is closed as an attribution pass with the single authoritative
terminal:

```text
P224-OBSERVABILITY-GAP-LOOKUP-PATH
```

The earliest supported conclusion is deliberately bounded:

```text
Router B main NetDB: current, receivedAsPublished, query-answerable LS2
Router A helper client DB: target absent before and after the send
Java send: ACCEPTED (1) -> NO_LEASESET (21)
Exact A->B/B->A lookup trace: not observable from the permitted diagnostics
```

Therefore Plan 224 does not attribute the failure to publication, query
dispatch, Router-B handling, reply delivery, or client-subDB installation.
Those stages remain unknown. Plan 225 owns the narrowly scoped observability
corrective; no production correction was implemented here.

The overall legacy `run-java.sh` process exited nonzero because the enclosing
Plan-199/M6 Java lane still has its pre-existing qualification failures. The
Plan-224 destination evidence was nevertheless emitted and passed its own
fail-closed classifier on three authoritative attempts across two
implementation heads after one pre-epoch startup/publication stop. The final
implementation head reproduced the same bounded result.

## 2. Implementation and pinned inputs

The implementation was committed before counted external execution:

```text
implementation_sha = 96824f8e5e2cd56c90bb94ebb12aedad68435e66
implementation_commits = 895132cabb9a220c64e288bad288c1bf77bda81f, 96824f8e5e2cd56c90bb94ebb12aedad68435e66
java_i2p          = 2.13.0
java_commit       = 9134f808337b401e8e53c73734c81fab04280c9d
i2pd_pin          = 2.61.0
i2pd_commit       = 635b013a612ff47278ef02acf8580a28e10e26c5
rust              = 1.95.0
```

Immediately before counted execution:

```text
counted external HEAD   = 44419250e9db770a7fd136ffc9c705b99e5ec3c7
git status --porcelain   = empty
```

The i2pd pin was retained for repository consistency and was not exercised by
this Java destination-only lane. No reference source was patched or vendored.

## 3. Authoritative sanitized evidence

The primary reproducible result is the final-head attempt. The sanitized artifacts were
written to:

```text
target/interop/m6-java-evidence/evidence.md
target/interop/m6-java-evidence/evidence.json
target/interop/m6-java-evidence/driver/destination/driver-evidence.tsv
```

These are generated, ignored artifacts; the durable facts below are the
closure evidence. Raw Java logs were not copied into any evidence artifact.

### 3.1 Exact identity correlation

```text
target_hash_hex = 9ea4ece81411ab18bc771ed53566da595044e638d393d132373d1d53652de6e8
target_hash_b64 = nqTs6BQRqxi8dx7VNWbaWVBE5jjTk9EyNz0dU2Ut5ug=
router_b_hash_hex = 184e0ce1eae5bac1de305803ac8a55fcc7de071ca2419383140cf8b4f53f88f3
helper_dbid_hex = f636fc5a8b58ea9c2dacab6de06593e731003c7b57bcbf68601899979158d429
```

The target hash is identical in every Plan-224 snapshot. The helper DBID is
resolved and reported as a client DB; the probe rejects a main-DB fallback.

### 3.2 Router B pre-send main-NetDB snapshot

```text
raw_present=true
validated_present=true
entry_type=3
received_as_published=true
received_as_reply=false
received_by_hex=none
ls2_unpublished=false
lease_count=1
key_count=1
key_types=4
latest_lease_ms=1789849789000
current=true
```

Under the exact-pinned Java `HandleDatabaseLookupMessageJob` rule, this is a
current, published, query-answerable Standard LS2. It is not evidence that a
query was actually sent or answered; those are separate stages.

### 3.3 Router A pre-send client-subDB snapshot

```text
client_db_resolved=true
client_db_is_client=true
raw_present=false
validated_present=false
entry_type=-1
received_as_published=unknown
received_as_reply=unknown
received_by_hex=none
lease_count=-1
key_count=-1
key_types=unknown
current=unknown
```

The exact target was absent from the helper client sub-DB before the tracked
send. No diagnostic lookup was issued before `SEND_TRACKED`.

### 3.4 Tracked send and frozen outcome

```text
nonce=1
payload_len=27
reverse_sha256=8a9e8146bb7d8c0b32b19b2483913d9f38aab1c7bfc925b1a73e7cd5aebb2271
ordered_statuses=[1, 21]
frozen_tunneldata_45s=false
frozen_payload_45s=false
reply_encryption_error_seen=false
```

This is the existing single-send causal path. The 45-second result was
collected before lookup-trace collection and was not modified by it.

### 3.5 Lookup trace and post-send snapshots

The sanitized trace row was emitted with:

```text
trace_observable=false
```

The targeted `logger.config` was verified in the disposable Router-A and
Router-B datadirs, but no exact target-correlated facts proved query start,
query-to-B dispatch, B receipt, B answer, or A client-tunnel DSM receipt.
The serializer's false-valued fields in the unobservable row are not treated
as negative protocol facts; they are unknown because the trace was not
observable.

Post-send Router A remained:

```text
client_db_resolved=true
client_db_is_client=true
raw_present=false
validated_present=false
```

Post-send Router B remained the same exact answerable state as the pre-send
snapshot, including `validated_present=true`, `received_as_published=true`,
`key_types=4`, and `current=true`.

## 4. Attempt history

Attempts 1–3 used implementation SHA `895132c`; the final counted attempt used
implementation SHA `96824f8`. Every attempt used fresh Java RouterContexts,
the exact Java pin, the same topology, the same logger configuration, the
existing destination driver, and the frozen timing. The final attempt used the
default sanitized output directory and did not retain raw scratch logs.

1. Attempt 1 used the default sanitized evidence directory and stopped before
   the authoritative epoch at `plan199-java-stop:
   client-ls2-local-but-not-network-visible`. It emitted only a pre-epoch
   observability-gap record and is not authoritative for stage attribution.
2. Attempt 2 reached the authoritative epoch and emitted the same terminal,
   `P224-OBSERVABILITY-GAP-LOOKUP-PATH`, with an answerable Router-B snapshot,
   an empty helper client DB, `[1, 21]`, and no frozen-window payload.
3. Attempt 3 on `895132c` reached the authoritative epoch and reproduced the
   same terminal and stage facts on a fresh scratch context.
4. The final-head attempt on `96824f8` ran from clean counted external HEAD
   `4441925`, reached the authoritative epoch, and reproduced the same terminal
   and stage facts. Its sanitized evidence is the primary result recorded above.

No attempt used a standalone Java lookup, publication retry, topology change,
tunnel tuning, timeout change, or production behavior change.

## 5. Java source provenance

The attribution boundary follows the exact-pinned Java 2.13.0 source review in
Plan 224:

- `OutboundClientMessageOneShotJob` maps an unsuccessful bounded client-NetDB
  LeaseSet lookup to `STATUS_SEND_FAILURE_NO_LEASESET (21)`;
- `IterativeSearchJob` separates client lookup dispatch, reply-tunnel
  capability, and search completion;
- `HandleDatabaseLookupMessageJob` answers a LeaseSet lookup only when the
  main-NetDB entry is a current LeaseSet marked `receivedAsPublished`;
- `InboundMessageDistributor` tags a client-tunnel LeaseSet DSM with the
  receiving client;
- `FloodfillDatabaseStoreMessageHandler` routes a received-by-client DSM to
  that client sub-DB;
- `InNetMessagePool` runs the matching DSM store inline before queuing the
  lookup-success reply job.

The last ordering rules out treating a hypothetical store-vs-success race as
the explanation. Since the client-tunnel DSM was not observably proven, no
contradiction was asserted.

## 6. Requirement-to-evidence matrix

| Plan-224 requirement | Evidence / result |
|---|---|
| Preserve Plan-222 selector and Plan-223 identity/LS2 invariants | Focused tests, static guards, exact destination lane; status 17 absent and `[1,21]` retained |
| Exact target identity in all snapshots | `target_hash_hex` and helper/router context above; one target in every P224 row |
| Router-B raw, validated, current, received-as-published state | Pre/post main-LS snapshots above; answerability derived explicitly |
| Router-A helper client DB pre/post state | Client-scoped snapshots above; no main-DB fallback |
| One causal tracked send | `nonce=1`, one digest, ordered statuses `[1,21]` |
| Frozen 45-second result | `frozen_tunneldata_45s=false`, `frozen_payload_45s=false` |
| Targeted logging before startup | Verified scratch `logger.config`, default ERROR and three class overrides |
| No raw or secret log evidence | Whitelist-only typed rows; raw logs remained disposable scratch-only |
| Distinguish selector membership from actual lookup stages | Trace fields remain unobservable; no stage was inferred from selector membership |
| Exact DSM/store ordering respected | Pinned Java source provenance above; no store-race claim |
| One terminal and earliest supported conclusion | Exactly one `p224-classification`: `P224-OBSERVABILITY-GAP-LOOKUP-PATH` |
| No production correction or Java mutation | Read-only same-package probe; static checker rejects mutation/reflection/standalone lookup |
| Clean committed head before external evidence | SHA and clean-tree proof above |
| Retry budget | Three attempts on predecessor SHA plus one fresh final-head attempt after the lint-only implementation commit; one pre-epoch stop and three authoritative reproductions |
| Registry/roadmap/dependency audit | Sections 8–9 below; Plan 225 registered, blocked rows retained |

## 7. Verification

Completed before closure:

```text
cargo fmt --all --check                                      PASS
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run PASS
cargo test --locked -p i2pr-daemon --test java_tunnel_external p224 -- --test-threads=1 PASS (29)
cargo test --locked -p i2pr-daemon --test java_tunnel_external p222 -- --test-threads=1 PASS (18)
cargo test --locked --workspace --all-targets -- --test-threads=1 PASS (2519 passed, 16 ignored; 103 suites)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings PASS
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps PASS
cargo test --locked --workspace --doc PASS (0 doc tests, 16 suites)
cargo deny check advisories bans sources PASS (duplicate-dependency warnings only)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh     PASS
bash scripts/check-dependency-direction.sh                    PASS
bash scripts/check-runtime-boundaries.sh                      PASS
bash scripts/check-fixture-manifest.sh                        PASS
bash scripts/check-ntcp2-vectors.sh                           PASS
bash scripts/check-ssu2-vectors.sh                            PASS
bash scripts/check-i2cp-vectors.sh                            PASS
bash scripts/check-ntcp2-interoperability.sh                  PASS
bash scripts/check-constrained-host-lane-boundary.sh           PASS
bash scripts/check-sam-acceptance-evidence.sh                  PASS
bash scripts/check-ssu2-acceptance-evidence.sh                 PASS
bash scripts/check-i2cp-acceptance-evidence.sh                 PASS
bash scripts/check-service-tunnel-acceptance-evidence.sh      PASS
bash scripts/check-service-tunnel-boundaries.sh                PASS
bash scripts/check-exploratory-tunnel-evidence.sh              PASS
bash scripts/check-netdb-tunnel-evidence.sh                    PASS
bash scripts/check-destination-tunnel-evidence.sh              PASS
bash scripts/check-streaming-tunnel-evidence.sh                PASS (warnings retained by checker)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py' PASS (18)
bash -n tests/integration/m6-interop/run-java.sh              PASS
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh  PASS
javac exact-pinned ControlledRouter + P220/P222/P223/P224 probes PASS
```

The streaming evidence checker emitted its repository-defined warnings about
labels not yet bound by `run-m6-mixed-router.sh`, but exited successfully.
The final external Java destination run itself exited nonzero at the enclosing
Plan-199 workspace-gates slice; its sanitized Plan-224 classifier passed with
the terminal recorded above. No failed external result was converted into
success.

## 8. Security and operational review

- The Java probe performs only read-only local main/client NetDB snapshots.
- No `store`, `registerKeys`, lookup initiation, reflection, or private-key
  extraction was added.
- The logger defaults to `ERROR`; only the three named source classes receive
  temporary diagnostic levels.
- Sanitization requires the exact target and helper correlation for the client
  receipt fact and emits only bounded booleans, counts, type codes, hashes, and
  timestamps already required by the evidence contract.
- Session keys, reply tags, raw payloads, private identity material, and raw
  Java log lines are absent from committed evidence and closure facts.
- Listeners and routers remained loopback-only and disposable.

Findings by severity:

```text
critical: none
high:     none
medium:   lookup-path trace remains unobservable; handed to Plan 225
low:      enclosing legacy Java qualification still exits nonzero; does not
          invalidate the independently emitted P224 attribution terminal
```

## 9. Dependency and unblock audit

No existing future plan can be unblocked by this result:

- Plan 201 remains `blocked-pending-plan225-no-leaseset-lookup-path-observability-corrective`.
  The exact failing lookup stage is still unknown and M6 Java closure cannot
  be claimed.
- Plan 204 remains `blocked-on-m6-java-second-family-closure-pending-plan225-observability`.
  Its cross-milestone convergence gate is not satisfied.
- Plan 205 remains `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`.
  The direct Java client-NetDB path is still the active lane; no SAM pivot is
  authorized.
- Plan 218 remains the stopped behavioral boundary; Plans 222 and 223 remain
  closed and their evidence is retained.

Plan 225 is registered as the next dependency-ready work item with the exact
boundary proved here. It owns observability, not a guessed protocol fix:

```text
plan_224 = passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap
plan_225 = registered-ready-m6-java-no-leaseset-lookup-path-observability-corrective
next_executable_plan = 225-m6-java-no-leaseset-lookup-path-observability-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

The live registry, mixed-router roadmap, Plan-201 status, and Plan-204 status
are updated in the same closure change.
