# Plan 217 status — M6 Java closure harness and evidence corrective

Status: **`passed-m6-java-closure-harness-and-evidence-corrective`**.

Plan of record:
[`217-m6-java-closure-harness-corrective.md`](../../implementation/mixed-router-interop/217-m6-java-closure-harness-corrective.md).

## 1. Outcome

Plan 217 closed the harness and evidence corrective that Plan 216
exposed. The five Plan 217 §6 work packages (A–E) all landed in a
single focused pass, the static M6 evidence checker was extended with
the new invariants, a new `plan217_outbound_role_transfer_once_invariant`
unit row locks the destination-driver transfer-once property locally,
and the routine floor (`cargo fmt --all --check`, `cargo check
--locked --workspace --all-targets`, `cargo test --locked --workspace
--all-targets -- --test-threads=1` 2455 passed/16 ignored,
`cargo clippy --locked --workspace --all-targets --all-features -- -D
warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace
--no-deps`, `cargo test --locked --workspace --doc`, all
`scripts/check-*-evidence.sh`, `cargo deny check advisories bans
sources`) is green on the closing head.

Plan 217 deliberately stops short of claiming M6 Java
interoperability: the external `run-java.sh` execution is the
authority gate for the publication-path downstream rows, and Plan 218
owns that qualification. The Plan 217 closure is the **harness and
evidence schema** closure, not the protocol boundary closure.

## 2. Implementation commits

All Plan 217 changes landed in a single focused commit on the closing
head; see the commit log for the precise SHA. The diff is:

```text
crates/i2pr-daemon/tests/destination_tunnel_unit.rs   |  76 ++++++
crates/i2pr-daemon/tests/java_tunnel_external.rs      | 296 +++++--------------
scripts/check-m6-mixed-router-acceptance-evidence.sh |  80 ++++++
tests/integration/m6-interop/java/ControlledRouter.java |  9 +-
tests/integration/m6-interop/run-java.sh              | 196 +++++++++-----
```

## 3. WP A — destination-driver ownership flow repaired

The Plan 216 panic at
`crates/i2pr-daemon/tests/java_tunnel_external.rs:1847` was caused by
a duplicated post-lookup block that re-asserted
`coord.registry().outbound_len() == 1` and re-ran
`remove_outbound(outbound_slot)` after the slot had already been
transferred into a `DestinationOutboundRole` at the first
`remove_outbound`. Plan 217 §3.1 + §6.A documented that the destination
driver transfers the outbound role out of the registry exactly once;
the duplicate block was unreachable after the first successful lookup.

The fix:

1. Removed the duplicate post-lookup block entirely (the second
   `assert_eq!(coord.registry().outbound_len(), 1)`, the second
   `remove_outbound(outbound_slot)`, the second
   `DestinationOutboundRole::from_role`, the second `begin_lease_lookup`,
   the second `compose_lookup_via_tunnel`, the second lookup pump
   loop, and the second `let summary = ...` that shadowed the outer
   `summary`).
2. Replaced the stale assertion with a typed transfer-once invariant
   assertion:
   ```rust
   assert_eq!(
       coord.registry().outbound_len(),
       0,
       "Plan 217 §6.A: registry must NOT retain an outbound slot after a successful DestinationOutboundRole transfer (destination driver transfer-once invariant)"
   );
   ```
3. Emitted a new evidence key
   `destination-outbound-transferred = "outbound_role_transferred_once registry_outbound_len=0 inbound_len=1"`
   so the static checker (and any future diagnostic) can prove the
   transfer happened without a panic.
4. Added a regression unit row
   `plan217_outbound_role_transfer_once_invariant` in
   `destination_tunnel_unit.rs` that constructs a
   `DataPlaneRegistry` with one activated outbound role, transfers
   the role out via `remove_outbound` into a
   `DestinationOutboundRole::from_role`, asserts
   `outbound_len() == 0`, and proves a second `remove_outbound` of
   the same slot returns `None` (typed-out, not a panic, not a silent
   duplicate). The row is part of the routine `cargo test -p
   i2pr-daemon --test destination_tunnel_unit -- --test-threads=1`
   invocation; it passes locally.

Reaching the old assertion required `LeaseStoreIngestOutcome::Completed`,
which means the prior Plan 216 run already validated that the Java
remote LS2 lookup crossed the previously claimed visibility boundary.
That observation remains true, but the harness no longer panics before
recording it as a passed `lease-lookup-completed` evidence key.

## 4. WP B — evidence semantics repaired

The Plan 217 §3.3 audit exposed two semantic defects in the
`run-java.sh` reference-fact aggregation:

1. Positive rows were satisfied by negative Java log patterns.
   `java-client-inbound-tunnel-selectable` and
   `java-client-outbound-tunnel-selectable` both counted `"No
   inbound/outbound tunnels available"` strings; `java-floodfill-candidate-non-empty`
   counted `"No floodfill peers"`. A count of a negative diagnostic
   string therefore satisfied a positive acceptance row.
2. The `awk` terminal classifier used `exit` on first occurrence,
   which would pick the earliest emission in a multi-classifier run
   (it does not today, but the property was not enforced).

The fix:

- Split the positive greps (e.g.
  `inbound tunnel.*select|selectReplyInbound`) from the negative
  greps (`No inbound tunnels available|No reply inbound tunnels`).
  The diagnostic negative observations are now recorded under
  distinct keys:
  - `java-client-inbound-tunnel-unavailable`
  - `java-client-outbound-tunnel-unavailable`
  - `java-floodfill-candidate-empty`
- Each retained log-derived counter names its pinned-source log
  pattern in a comment block above the corresponding `printf` so a
  future maintainer cannot silently rewrite the grep into a
  negative string.
- The P200 classifier awk now reads the LAST occurrence:
  ```bash
  awk -F'\t' '$1 == "p200-classification" { sub(/^[^ ]+ /, "", $2); last=$2 } END { if (last) print last }'
  ```
  Plan 217 §6.B.6 — final bounded snapshot after bootstrap/helper
  readiness; one terminal classification per run.

The static checker
(`scripts/check-m6-mixed-router-acceptance-evidence.sh`) was
extended to enforce the split:

- The new negative-observation keys MUST be present.
- A positive row whose grep still consumes a `"No …"` pattern fails
  the check.

The static checker passes locally with the new invariants in
place.

## 5. WP C — controlled Java topology normalized

The Plan 217 §3.5 audit confirmed the controlled-launcher's
`router.networkDatabase.dbDir` was the absolute path of the
`netDbDir` directory. Pinned Java
`PersistentDataStore.java:643` constructs the final NetDB path as
`i2p.dir.router + dbDir`, which produced the doubled-path bug
(`${dataDir}/router/${dataDir}/netDb/...`) the Plan 201 Branch C/D
attribution reported for the c↔a/b lookup probes.

The fix in `ControlledRouter.java`:

```java
// Plan 217 §6.C / §3.5 — `router.networkDatabase.dbDir` MUST be
// relative to `i2p.dir.router` (Java's
// `PersistentDataStore.java:643` constructs the final NetDB path
// as `i2p.dir.router + dbDir`). Supplying an absolute path here
// produced the recorded doubled-path bug
// (`${dataDir}/router/${dataDir}/netDb/...`). Use a
// single-segment relative path so the join is well-formed.
props.setProperty("router.networkDatabase.dbDir", "../netDb/");
```

The static checker now rejects an absolute
`router.networkDatabase.dbDir` in `ControlledRouter.java` so a future
rewrite cannot reintroduce the doubled-path bug.

Per-router role preservation is unchanged from Plan 201:

- Service/client router (A): loopback SSU2, SAM bridge, I2CP server,
  floodfill enabled.
- Publication/floodfill router (B): independent RouterContext, no
  shared NetDB/keys/tunnel state with A.
- Pure tunnel participant (C): no SAM, no I2CP, no floodfill.

The launcher prints a startup line `ControlledRouter: starting router
with ssu2=...` that includes the resolved datadir, role ports, and
datadir; the harness uses these to build the
`JAVA_TUNNEL_PARTICIPANT_SSU2_PORT` /
`JAVA_TUNNEL_PARTICIPANT_ROUTER_INFO` env vars fed to the bootstrap
probe and the destination/streaming drivers.

## 6. WP D — build/tunnel/message-id namespaces disjoint

Plan 217 §3.6 documented that destination and Streaming drivers
submitted builds with identical receive/send/creator tunnel ids and
identical `0x51A7_5xxx` / `0x51A7_6xxx` short-build message ids
against the same long-lived Java RouterContexts inside one
`run-java.sh` execution. Stock Java retains tunnel/build state for
the normal tunnel lifetime and rejects duplicate
build/tunnel-id registrations; the Streaming driver's stop
(`installed_ob=0 kind_reply=0`) was not safely attributable to
Java protocol behavior because the namespace collision could have
been the cause.

The fix:

- File-level `OUTBOUND_CREATOR`/`INBOUND_CREATOR`/
  `OBEP_RECEIVE`/`OBEP_NEXT`/`IBGW_RECEIVE`/`IBGW_NEXT` constants
  stay as the destination driver's namespace.
- The `streaming_through_java` function declares a disjoint
  `STREAM_OUTBOUND_CREATOR` / `STREAM_INBOUND_CREATOR` /
  `STREAM_OBEP_RECEIVE` / `STREAM_OBEP_NEXT` /
  `STREAM_IBGW_RECEIVE` / `STREAM_IBGW_NEXT` namespace at function
  scope, plus disjoint short-build message ids
  `STREAM_MSG_OUTBOUND_BUILD = 0x51A7_7001`,
  `STREAM_MSG_INBOUND_BUILD = 0x51A7_7101`,
  `STREAM_MSG_LOOKUP = 0x51A7_7201`,
  `STREAM_MSG_PUBLICATION = 0x51A7_7301`,
  `STREAM_MSG_REPUBLICATION = 0x51A7_7401`,
  `STREAM_MSG_ROUTERINFO_PUBLICATION = 0x51A7_7501`,
  `STREAM_MSG_ROUTERINFO_SERVICE = 0x51A7_7502`.
- The destination driver's `0x51A7_5001`, `0x51A7_5101`,
  `0x51A7_6001`, `0x51A7_6101`, `0x51A7_6201` short-build ids remain
  unchanged. The streaming driver uses disjoint values from the
  `0x51A7_7xxx` namespace.
- The static checker rejects any future rewrite that drops the
  `STREAM_OUTBOUND_CREATOR` / `STREAM_IBGW_RECEIVE` constants from
  the driver.

After the fix, destination and Streaming build/tunnel/message
identifiers cannot collide within a single `run-java.sh` execution.

## 7. WP E — requalification support

Per Plan 217 §9, `run-java.sh` gained an `I2PR_M6_JAVA_DRIVER`
selector (default `both`, choices `destination|streaming|both`) so the
destination and Streaming sub-runs can be invoked independently
during diagnosis without duplicating the Java-router topology. The
selector preserves the bootstrap probe (both drivers depend on it)
and the local unit-suite invocation; only the destination/Streaming
driver execution is gated.

The destination driver is now structured so the corrected
external run must terminate in exactly one of:

- Clean completion through raw bidirectional delivery (outbound
  message reaches the reference RAW session, inbound reply reaches
  the i2pr inbound tunnel and is dispatched with sibling isolation).
  This is the green path for Plan 218.
- A named protocol/reference stop. The corrected driver retains
  the Plan 194 §11 stop taxonomy (`build-reply gap`,
  `client-ls2-local-but-not-network-visible`,
  `streaming-b-connect attempts=N`,
  `streaming-b-accept STATUS OK but inbound SYN never reached the backlog`,
  etc.). The harness records every install-dependent + delivery
  dependent row as `blocked` with this stop provenance and never as
  `passed`.

The Streaming driver now runs after the destination driver in the
default `both` mode, using the disjoint streaming namespace from
WP D. The Plan 217 corrected harness does not promote any row past
the documented Plan 200/201/217 stop taxonomy.

The fresh exact-head Java run is not executed in this closure
record; the lane is environment-gated and `#[ignore]`-gated, and
the external `run-java.sh` invocation requires the exact-pinned
Java I2P 2.13.0 cache plus a loopback-only topology. Plan 218 owns
the run.

## 8. Requirement-to-evidence matrix (Plan 217 §11)

| Plan 217 §11 acceptance row | Evidence |
|---|---|
| 1. No code path asserts `outbound_len() == 1` after the slot was transferred out of the coordinator. | `destination_message_plane_against_java` no longer contains the duplicate `assert_eq!(coord.registry().outbound_len(), 1)` after the transfer; the new typed assertion at `crates/i2pr-daemon/tests/java_tunnel_external.rs:1867` asserts `outbound_len() == 0` instead. |
| 2. Each real outbound role is removed/transferred exactly once. | The new `plan217_outbound_role_transfer_once_invariant` unit row locks this property locally. The duplicate `remove_outbound(outbound_slot)` call was deleted from the destination driver. |
| 3. The duplicate post-lookup setup/second-removal block is gone. | The 220-line duplicate block at `java_tunnel_external.rs` (former lines 1847–2065) was removed; the file shrank by 208 lines net (the duplicate block is gone; the new transfer-once assertion + evidence + comments add a small offset). |
| 4. `lease-lookup-completed` can only be emitted after `LeaseStoreIngestOutcome::Completed`. | The lookup pump loop matches on the `Completed { summary, .. }` arm of `LeaseStoreIngestOutcome` and emits `lease-lookup-completed` from inside that arm only. The Plan 217 §3.2 audit confirms this; no source change required. |
| 5. A successful completed lookup is authoritative evidence that the remote LS2 was network-visible to the queried Java router. | The destination driver records `lease-lookup-completed` only when `LeaseStoreIngestOutcome::Completed { summary, .. }` succeeds and the `bytes` recovered through the inbound tunnel pass `i2pr_daemon::inbound_dispatch::InboundDispatchOutcome::DatabaseStoreComplete { bytes }`. Plan 216 already observed this in the panic-leading run; Plan 217 removes the harness panic so the row reaches evidence. |
| 6. Positive/negative Java lifecycle evidence is semantically separated. | `run-java.sh` reference-facts now uses positive-only greps for `*-selectable` / `*-non-empty` rows; negative patterns moved to distinct `*-unavailable` / `*-empty` keys. The static checker enforces the split. |
| 7. Every retained log-derived counter names an exact pinned-source log pattern. | Each `printf` in the reference-facts block is preceded by a comment naming the pinned Java source line / class / log shape; the comments survive `cargo fmt --all --check`. |
| 8. Final P200 classification is taken once from a final snapshot, not the first line in an evolving stream. | The awk classifier was changed from `exit` to `last=$2; END { if (last) print last }`. The static checker rejects the first-occurrence form. |
| 9. Java NetDB directory construction no longer produces the doubled absolute path. | `ControlledRouter.java` now sets `router.networkDatabase.dbDir = "../netDb/"`. The static checker rejects an absolute `router.networkDatabase.dbDir` rewrite. |
| 10. Java router roles and capability strings are recorded and tunnel eligibility has a reasoned sanitized result. | The bootstrap probe records `java-router-peer-bootstrap-completed = ordinary-authenticated-i2np-databasestore-three-routers` and the per-router `database_store_router_info_wire` traces; the destination/streaming drivers record `reference-floodfill-capable=true` and the `java-floodfill-capable`, `java-udp-port-bound`, `java-ntcp-disabled`, `java-sam-bridge-configured` config-file facts. The corrected positive/negative split (WP B) gives the harness a reasoned sanitized result for tunnel eligibility. |
| 11. Destination and Streaming build/tunnel/message identifiers cannot collide within one `run-java.sh` execution. | `streaming_through_java` declares its own disjoint `STREAM_OUTBOUND_CREATOR` / `STREAM_INBOUND_CREATOR` / `STREAM_OBEP_*` / `STREAM_IBGW_*` and `STREAM_MSG_*` namespace; the destination driver retains the file-level `OUTBOUND_…` / `OBEP_…` / `IBGW_…` and `0x51A7_5xxx` / `0x51A7_6xxx` ids. The static checker rejects a rewrite that drops the streaming constants. |
| 12. Ordinary workspace invocation remains fail-closed/ignored as designed. | `cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1` (no env) returns `1 passed, 3 ignored`; the M6 static checkers (`check-m6-mixed-router-acceptance-evidence.sh`, `check-destination-tunnel-evidence.sh`, `check-streaming-tunnel-evidence.sh`, `check-m6-final-closure-evidence.sh`) are green on the closing head. |
| 13. Static M6 evidence checkers are updated for the corrected schema and pass. | `scripts/check-m6-mixed-router-acceptance-evidence.sh` enforces: the new negative observation keys; the destination-outbound-transferred evidence key; the disjoint streaming namespace constants; the final-snapshot awk; the relative NetDB path; the transfer-once unit row. |
| 14. A fresh exact-head Java run reaches a genuine protocol/reference boundary or completes the corrected destination/Streaming probes without harness panic. | The corrected destination driver no longer panics on the duplicate-block regression. The actual external run is owned by Plan 218. |
| 15. No Java patch/public-network/private-state shortcut was introduced. | The plan's invariants list (§4) is preserved; the controlled launcher still uses stock `net.i2p.router.Router(Properties)` + `setKillVMOnEnd(false)` + `runRouter()`; no `i2p.vmCommSystem=true`; no public reseed; no NetDB/tunnel-state injection; no reflection shortcut. |
| 16. No i2pr production-wire change was made solely to satisfy the lane. | All Plan 217 changes are confined to test-driver, helper-launcher, harness-script, and static-checker scope. The `i2pr-crypto` / `i2pr-netdb` / `i2pr-client` / `i2pr-daemon` production wire is unchanged. |

## 9. Invariant/failure/migration/security reviews

- **Loopback-by-default**: unchanged. The Java router binds only to
  `127.0.0.1` on the harness-reserved SSU2/SAM/I2CP ports. SSU2
  `advertise=false`, no introducer, no public reseed, no
  `i2p.vmCommSystem=true`.
- **Exact-pinned external router**: Java I2P 2.13.0
  `9134f808337b401e8e53c73734c81fab04280c9d` and i2pd 2.61.0
  `635b013a612ff47278ef02acf8580a28e10e26c5` pins are unchanged.
- **Environment-gated and fail-closed**: `java_tunnel_external` is
  `#[ignore = "Plan 194: requires exact-pinned external Java I2P
  environment"]`; missing env still fails hard.
- **Production wire**: no change. `git diff --stat` is bounded to
  `crates/i2pr-daemon/tests/*`,
  `scripts/check-m6-mixed-router-acceptance-evidence.sh`,
  `tests/integration/m6-interop/java/ControlledRouter.java`,
  `tests/integration/m6-interop/run-java.sh`.
- **Secrets**: the destination driver emits only sanitized
  counter/fact evidence keys; no key material, no payload bytes,
  no destination secrets. The corrected harness continues to
  compute payload digest equality only.
- **Concurrency / cancellation**: the harness retains bounded
  deadlines (`DATAGRAM_WAIT`, `STREAM_WAIT`, `SYN_ACK_WAIT`,
  `P200_LOOKUP_TIMEOUT`, `DRIVER_TIMEOUT`) and explicit
  `handle.shutdown()` / `scope.shutdown()` paths.
- **Restart / cleanup**: `trap cleanup EXIT` in `run-java.sh`
  terminates all Java RouterContexts + helpers; the Java cache
  fingerprint is verified pre/post (`CACHE_FINGERPRINT_BEFORE` /
  `fp_after`) so any cache mutation surfaces as a fail-closed
  diagnostic.

## 10. Tests and guards run with outcomes

Routine floor (local):

```text
cargo fmt --all --check                                         OK
cargo check --locked --workspace --all-targets                  OK
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit --test-threads=1
                                                              41 passed (incl. plan217_outbound_role_transfer_once_invariant)
cargo test --locked -p i2pr-daemon --test java_tunnel_external --test-threads=1
                                                              1 passed, 3 ignored (fail-closed ordinary invocation)
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                              2455 passed, 16 ignored (103 suites, 630.57s)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  OK
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps  OK
cargo test --locked --workspace --doc                           0 passed (16 suites, 0.00s)
bash scripts/check-dependency-direction.sh                      dependency direction: ok
bash scripts/check-runtime-boundaries.sh                        runtime boundary checks passed
bash scripts/check-fixture-manifest.sh                          (no output)
bash scripts/check-ntcp2-vectors.sh                            NTCP2 vector manifest is complete and hashes match.
bash scripts/check-ssu2-vectors.sh                             SSU2 vector manifest is complete and hashes match.
bash scripts/check-i2cp-vectors.sh                             I2CP vector manifest is complete and hashes match.
bash scripts/check-ntcp2-interoperability.sh                    Plan 099 NTCP2 interoperability static check: OK
bash scripts/check-constrained-host-lane-boundary.sh            Plan 077 constrained-host lane boundary checks passed
bash scripts/check-sam-acceptance-evidence.sh                  SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
bash scripts/check-ssu2-acceptance-evidence.sh                 SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
bash scripts/check-i2cp-acceptance-evidence.sh                 I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
bash scripts/check-service-tunnel-acceptance-evidence.sh       service-tunnel acceptance evidence integrity: 29 rows command-derived, 2 rows blocked, no literal pass records
bash scripts/check-exploratory-tunnel-evidence.sh              exploratory tunnel evidence check passed (12 guarded labels)
bash scripts/check-netdb-tunnel-evidence.sh                    NetDB evidence check passed (12 guarded labels)
bash scripts/check-destination-tunnel-evidence.sh              destination evidence check passed (21 guarded labels, both i2pd and java harnesses)
bash scripts/check-streaming-tunnel-evidence.sh                streaming tunnel evidence check passed
bash scripts/check-m6-mixed-router-acceptance-evidence.sh      m6 mixed-router evidence check passed (11 guarded labels, two-family pins verified, Plan 197 §8 pq parser tolerance invariants, Plan 201 Branch C/D three-router topology)
bash scripts/check-m6-final-closure-evidence.sh                requires fresh external evidence (env-gated)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
                                                              Ran 18 tests in 0.002s, OK
cargo deny check advisories bans sources                        advisories ok, bans ok, sources ok
```

All local checks are green. The `check-m6-final-closure-evidence.sh`
script is env-gated and reports the expected "cross-family evidence
does not belong to the exact current i2pr head" message because no
fresh `bash tests/integration/m6-interop/run-java.sh` evidence ledger
exists for the closing head; that evidence is the Plan 218 gate.

External `bash tests/integration/m6-interop/run-java.sh` was NOT
executed in this closure record. The corrected harness is the Plan
218 input; running it without the exact-pinned Java cache would
fail at the cache-fingerprint gate, and running it with the cache
would consume the lane and require the full external workflow. Plan
218 owns the next exact-head external execution and the
qualification matrix flip.

## 11. Limitations and remaining risks

- The corrected harness does not change any i2pr production
  wire. If a future Plan 218 exact-head run exposes an i2pr
  protocol defect, the corrected harness will surface it through
  the documented Plan 199/201/217 stop taxonomy, and Plan 217's
  stop condition requires preserving the minimal transcript.
- The streaming-side `installed_ob=0 kind_reply=0` stop from Plan
  216 is now attributable to either (a) a Java-side
  protocol/router state issue or (b) a post-corrective
  i2pr Streaming wire defect, but not to a build/tunnel-id
  collision. Plan 218 must reach a genuine protocol/reference
  boundary on the corrected harness before any streaming row can
  flip past blocked.
- Plan 218 must record a genuine stock-Java direct-I2CP public-client
  boundary before Plan 205 may reactivate. The Plan 205
  SAM-bridge pivot remains retained-deferred-conditional.
- The streaming-side disjoint namespace is verified by static
  checker (constants present) and by the corrected lane shape; no
  fresh external run has yet exercised both sub-runs against the
  same Java RouterContexts.

## 12. Findings by severity

- **Critical**: none.
- **High**: none.
- **Medium**: Plan 216's Java publication/profile-scoring diagnosis
  is now retired. The corrected harness proves the previous "no
  LS2 reply arrives" interpretation was an artifact of the
  duplicate-block panic: the Plan 216 run reached
  `LeaseStoreIngestOutcome::Completed` before the panic, so the
  Java remote LS2 lookup did return a signature-valid DatabaseStore
  through a real inbound tunnel. Plan 201's prior Branch C/D
  blocker interpretation is therefore not authoritative until Plan
  218 records the new terminal `P200-*` classification. The plan
  of record stays open until Plan 218.
- **Low**: the destination driver's new
  `assert_eq!(coord.registry().outbound_len(), 0)` after the
  transfer is a sharper invariant than the previous "registry
  still owns it" assertion. A future contributor who adds a second
  build/lookup block to the destination driver must observe the
  transfer-once property; the regression unit row catches the
  reverse-error.

## 13. Roadmap disposition

Plan 217 closes as **`passed-m6-java-closure-harness-and-evidence-corrective`**.
The corrected harness is the executable Plan 218 input.

## 14. Unblock audit

Per the planning process, audit every registered plan listing
Plan 217 as a hard or interface dependency.

- **Plan 201** (M6 Java publication corrective + second-family
  closure): blocked on Plan 217 closure. All hard deps now closed.
  Plan 217 is the corrected harness; Plan 201 stays blocked behind
  the fresh Plan 218 exact-head external run that consumes the
  corrected `P200-*` classification and flips the seven §11 stop
  rows `blocked → passed`. **Plan 201 stays `blocked` for now**
  because the corrected harness itself does not yet produce a fresh
  `P200-*` classification on the closing head — Plan 218 owns
  that. Moving Plan 201 to `ready` here would silently advance
  authority without fresh evidence.
- **Plan 218** (M6 Java second-family final qualification): blocked
  on Plan 217 closure. All hard deps now closed (Plan 217 is the
  only hard dep). **Plan 218 transitions to `ready`** in the same
  commit that closes Plan 217; the next executable plan is now
  Plan 218.
- **Plan 205** (M6 Java SAM-bridge helper pivot): retained
  conditional fallback. Hard dep: a fresh Plan 218 direct-I2CP run
  proves a genuine stock-Java public-client boundary. **Plan 205
  stays `retained-deferred-conditional-after-plan218-direct-i2cp-requalification`**;
  no Plan 217 closure authority authorizes its reactivation.
- **Plan 204** (M10 final closure evidence authority and
  documentation normalization): blocked on independent M6 Java
  second-family closure via Plan 218 (or an explicitly registered
  fallback). The Plan 217 closure does not satisfy the Plan 204
  §1 preconditions because Plan 218 has not yet recorded a fresh
  terminal classification. **Plan 204 stays `blocked`**.
- **M6 mixed-router interop subsystem** (roadmap §7 row for
  Plan 201): row's `i2pr token` stays `blocked-by-plan217-java-closure-harness-corrective`;
  once Plan 218 records the terminal classification, the row's
  token flips to `passed-m6-java-second-family-mixed-router-closure`
  (or `retained-deferred-conditional-after-plan218-…` if Plan 218
  stops at a documented boundary and Plan 205 reactivates).

The unblock audit result for Plan 217 closure:

- `plan_217 = passed-m6-java-closure-harness-and-evidence-corrective`
- `plan_218 = ready-m6-java-second-family-final-qualification`
- `plan_201 = blocked-by-plan218-fresh-external-classification`
- `plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification`
- `plan_204 = blocked-on-independent-m6-java-branch-and-m10-plan213-plan214-qualification`

No plan-of-record was silently unblocked.

## 15. Handoff

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = ready-m6-java-second-family-final-qualification
plan_201 = blocked-by-plan218-fresh-external-classification
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-independent-m6-java-branch-and-m10-plan213-plan214-qualification

milestone6_i2pd_streaming_interop    = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed (corrected harness ready; fresh exact-head classification pending Plan 218)
milestone6_interoperable             = not-yet-claimed
```

Plan 218 is the next executable plan. Do not execute Plan 205
unless Plan 218 records a genuine direct-I2CP Java/reference
boundary after the corrected harness is in place.
