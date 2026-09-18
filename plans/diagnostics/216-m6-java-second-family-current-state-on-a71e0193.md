# Plan 216 — M6 Java second-family current-state diagnostic on `a71e0193`

Status: **diagnostic-only-snapshot, no-source-change**. Captures the current
Java second-family lane state on the Plan 215 re-closure SHA so the
`milestone6_java_mixed_router_interop = not-yet-passed` claim is anchored to
command-derived evidence on the latest immutable head.

## Provenance

- i2pr commit: `865badfd3cf227d32da1574c205127d7aae073a3`
- underlying tested SHA: `a71e0193c420c8d8464fd05ff67d5323515ed4dd` (Plan 215 re-closure, dependabot-merged head)
- Java I2P: `2.13.0` @ `9134f808337b401e8e53c73734c81fab04280c9d` (unmodified)
- OS/image: `Linux-6.8.0-139-generic-x86_64-with-glibc2.39`
- Rust: `rustc 1.95.0 (59807616e 2026-04-14)`
- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer
- Java profile: `i2p.dir.config=scratch`, `router.reseedDisable=true`, public client helpers

This is a **diagnostic only** pass — no production source change, no Plan 205
SAM-bridge helper rewrite, no Plan 201 Branch corrective.

## Lane summary

- `cargo test --locked -p i2pr-daemon --test java_tunnel_external -- --test-threads=1` (no env) → **1 passed, 3 ignored** (fail-closed ordinary invocation as expected).
- `bash tests/integration/m6-interop/run-java.sh` (full controlled-topology lane) → **workspace gates green**, bootstrap probe + streaming driver ran, destination driver **panicked** at `crates/i2pr-daemon/tests/java_tunnel_external.rs:1847` with `coord.registry().outbound_len() == 0`.

## Workspace gates

```
dependency direction: ok
runtime boundary checks passed
NTCP2 vector manifest is complete and hashes match.
SSU2 vector manifest is complete and hashes match.
Plan 099 NTCP2 interoperability static check: OK
Plan 077 constrained-host lane boundary checks passed
SAM acceptance evidence integrity: 22 rows command-derived, no literal pass records
SSU2 acceptance evidence integrity: 15 rows command-derived, no literal pass records
I2CP acceptance evidence integrity: 24 rows command-derived, no literal pass records
service-tunnel acceptance evidence integrity: 29 rows command-derived, 2 rows blocked, no literal pass records
NetDB evidence check passed (12 guarded labels)
destination evidence check passed (21 guarded labels, both i2pd and java harnesses)
Plan 193 streaming evidence check passed (33 guarded labels, helpers wired)
m6 mixed-router evidence check passed (11 guarded labels, two-family pins verified, Plan 197 §8 pq parser tolerance invariants, Plan 201 Branch C/D three-router topology)
```

All 11 static checkers green.

## Plan 200 §C/D sanitized lifecycle facts

`target/interop/m6-java-evidence/reference-facts.tsv` (counts only, never key material):

```
java-udp-listening                          0
java-sam-bridge-up                          2
java-reseed-disabled                        1
java-floodfill-capable                      1
java-udp-port-bound                         1
java-ntcp-disabled                          1
java-no-public-reseed                       1
java-sam-bridge-configured                  1
java-streaming-accepted                     0
java-client-subdb-created                   0
java-create-leaseset2-received              0
java-client-leaseset-stored-current         0
java-client-leaseset-publish-scheduled      0
java-client-leaseset-republish-job-ran      0
java-client-inbound-tunnel-selectable       0
java-client-outbound-tunnel-selectable      0
java-floodfill-candidate-non-empty          1
java-store-emitted                          0
java-store-ack-observed                     0
java-store-failure-reason                   0
```

The 10 Plan 200 §C/D lifecycle keys stay at **0** — exactly the documented
Java-side LeaseSet2 publication gap. The two floodfill-side rows
(`java-floodfill-capable`, `java-floodfill-candidate-non-empty`) and the
SAM-bridge-side rows (`java-sam-bridge-up`, `java-sam-bridge-configured`)
fire normally. The two router-config rows (`java-udp-port-bound`,
`java-ntcp-disabled`) are committed in the controlled data dir.

## Bootstrap probe (Plan 200 §B)

`bootstrap_java_router_peers` ran 20 probes (10 a↔b + 10 b↔a). Outcome:

```
p200-routerinfo-lookup-a-knows-b  response_observed=true  105 / 210 (~50%)
p200-routerinfo-lookup-b-knows-a  response_observed=true   21 / 210 (~10%)
```

The Branch A decode fix (`decode_inbound_i2np` helper, gzip decompression
before RouterInfo parse, commit `2dc926f`) IS in this head. Most of the
a-knows-b probes round-trip Java's standard-form gzipped DatabaseStore
correctly. The b-knows-a direction is asymmetric — the same probe
occasionally succeeds, but many still fail with
`Truncated { offset: 387, needed: 49858, remaining: 59 }` decode errors
when Java's reply body runs past the standard-form header.

Terminal P200 classification as captured by `awk` (first occurrence in the
210-probe evidence stream):

```
P200-A-router-a-missing-router-b
   java-main-netdb-a-knows-b           = false
   java-main-netdb-b-knows-a           = false
   java-client-ls2-not-created         = false
   java-client-tunnel-publication-path = false
   java-floodfill-candidate            = false
   java-store-emitted                  = false
   java-network-visible-leaseset       = false
```

Some later probes in the same evidence stream record
`P200-B-router-b-missing-router-a` (a-knows-b=true, b-knows-a=false)
after the a→b direction's reliable decode was confirmed. The shell's
`awk` extraction picks the first occurrence only.

The downstream row `external-p200-classification` is recorded as
**passed** because the classification itself is a diagnostic observation,
not a publication claim. The `failed (exit 1)` rows for the 14 Plan 200
§C/D lifecycle keys (`external-java-client-subdb-created` etc.) record
the zero count — they are *positive-result* rows that expect `≥1`, so a
zero count fails the row.

## Destination driver (panic — NEW regression)

`destination_message_plane_against_java` panicked at
`crates/i2pr-daemon/tests/java_tunnel_external.rs:1847`:

```
assertion `left == right` failed
  left: 0
 right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED
```

Line 1847 is `assert_eq!(coord.registry().outbound_len(), 1)` — the
post-LS2-lookup registry check. The pre-LS2-lookup check at line 1575
passed (registry showed `outbound_len() == 1`), but after
`compose_lookup_via_tunnel` + the LS2 reply pump loop, the registry
reports `outbound_len() == 0`. This is a NEW test-driver defect vs. the
prior recorded Plan 200/201 runs that cleanly entered the `else` branch
at line 1812 (no LS2 resolved) and emitted stop provenance. Either
`compose_lookup_via_tunnel` removes/expires the outbound tunnel as a
side effect, OR the lookup's reply pump loop processes a stale inbound
that triggers `remove_outbound` on the registry. This regression was
not in the pre-Plan-215 state and would need a focused Plan 216.x
diagnostic if a follow-up plan attempts to keep the destination path
green.

## Streaming driver

`streaming_through_java` ran cleanly: `test result: ok. 1 passed; 0
failed; 0 ignored; 0 measured; 3 filtered out; finished in 33.20s`.

Recorded facts in `target/interop/m6-java-evidence/driver/streaming/driver-evidence.tsv`:

```
daemon-strict-profile                       true
reference-routerinfo-verified               true
java-service-routerinfo-verified            true
reference-bootstrap-store                   1
reference-floodfill-capable                 true
session-established                         2
java-router-peer-bootstrap-submitted        service-to-publication-and-publication-to-service
public-streaming-session-established        control_pong=true dest_len=391
public-streaming-destination-created        dest_len=391 session=connected control_ready=1 publication_observed=external
public-streaming-leaseset-status            STATUS ... publications_observed=no
outbound-build-emitted                      true
inbound-build-emitted                       true
plan199-java-stop                           Plan 194 §11 stop: java outbound build never installed (installed_ob=0 kind_reply=0)
```

The streaming-side record_stop fires before any outbound tunnel
install because the `installed_ob=0 kind_reply=0` branch is the
*streaming-driver's* early-exit path — independent of the destination
driver's later panic.

## Plan 201 §11 stop rows — current state

The seven §11 stop rows stay `blocked` with documented Plan 198/199
stop provenance:

```
external-lease-lookup-tunnel                  blocked (m6-java-second-family-stop; Plan 199)
external-ls2-publication-tunnel               blocked (m6-java-second-family-stop; Plan 199)
external-destination-outbound                 blocked (m6-java-second-family-stop; Plan 199)
external-reference-received                   blocked (m6-java-second-family-stop; Plan 199)
external-destination-inbound                  blocked (m6-java-second-family-stop; Plan 199)
external-streaming-syn-sent                   blocked (m6-java-second-family-stop; Plan 199)
external-streaming-syn-accepted               blocked (m6-java-second-family-stop; Plan 199)
external-streaming-established                blocked (m6-java-second-family-stop; Plan 199)
external-streaming-data-digest                blocked (m6-java-second-family-stop; Plan 199)
external-streaming-multipacket-digest         blocked (m6-java-second-family-stop; Plan 199)
external-streaming-reverse-data-digest        blocked (m6-java-second-family-stop; Plan 199)
external-streaming-reverse-multipacket-digest blocked (m6-java-second-family-stop; Plan 199)
external-streaming-sibling-established        blocked (m6-java-second-family-stop; Plan 199)
external-streaming-sibling-data-digest        blocked (m6-java-second-family-stop; Plan 199)
external-streaming-close                      blocked (m6-java-second-family-stop; Plan 199)
external-streaming-sibling-isolated           blocked (m6-java-second-family-stop; Plan 199)
external-streaming-b-established              blocked (m6-java-second-family-stop; Plan 199)
external-streaming-b-data-digest              blocked (m6-java-second-family-stop; Plan 199)
external-streaming-b-reverse-data-digest      blocked (m6-java-second-family-stop; Plan 199)
external-streaming-b-close                    blocked (m6-java-second-family-stop; Plan 199)
external-manager-cleanup                      blocked (m6-java-second-family-stop; Plan 199)
external-streaming-reference-accepted         blocked (m6-java-second-family-stop; Plan 199)
external-p201-lookup-ls2-signature-rejected   blocked (m6-java-second-family-stop; Plan 199)
external-p201-inbound-garlic-completed        blocked (m6-java-second-family-stop; Plan 199)
```

## Conclusion

The M6 Java second-family row is **not closer to passing** on
`865badf` / `a71e0193` than on the documented `Plan 201 in-progress` state. The
boundary remains:

- **Java-side**: stock `i2p.jar 2.13.0` LeaseSetPublisher does not
  fire `sendStore` for the helper destination under the controlled
  loopback topology (ProfileOrganizer fast-peer scoring does not
  promote loopback peers, and zero-hop helpers have no eligible client
  tunnel path).
- **i2pr-side**: production wire is unchanged; the Branch A decode fix
  and Branch G observation framework are in place but no LS2 reply
  ever arrives to traverse them.

Plan 205 (SAM-bridge helper pivot) has not been implemented in this
diagnostic pass. Per Plan 205 §1, the SAM-bridge path is the next
documented executable attempt, but Plan 194 explicitly noted Java's
SAM bridge does NOT auto-publish the SAM-destination LS2 to the local
NetDB in a controlled private topology — exactly the gap Plan 205 is
attempting to work around. A successful closure requires either:
1. Plan 205 source changes + a fresh exact-head external run; or
2. A different Java-side entry point that bypasses LeaseSetPublisher
   entirely (e.g. directly invoking `JobQueue` store dispatch, which
   crosses into Java-side patching territory and violates Plan 198 §4
   / Plan 201 §4).

Neither path can be closed in this diagnostic pass; both require a
new plan-of-record commit.

## Authority claim retained

```text
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable            = not-yet-claimed
next_executable_plan                = 205-sam-bridge-helper-pivot
m6_java_publication_branch_a_corrective   = landed-this-head (commit 2dc926f; one-direction proof)
m6_java_publication_branch_g_framework     = landed-this-head (commit 9bce8a7)
m6_java_publication_branch_c_d_corrective  = attempted-blocked-on-java-loopback-peer-profile-scoring (commit d0fe596)
```

Plan 204 (M10 final closure evidence authority and documentation
normalization) stays deferred until Plan 201 records the terminal
`P200-{A..H}` classification and lands its narrow corrective.

## Evidence

Full evidence preserved under
`target/interop/m6-java-evidence/` (gitignored, not part of the
commit). The committed artifact is this diagnostic file plus the
documented upstream plan-of-record chain
(`plans/198-205`).
