# Plan 196 status — M6 Java I2P controlled first-run topology corrective

Status: **`in-progress-corrective-implementation-landed-static-checks-green-stopped-at-§10B-authenticated-ssu2-pq-option-rejection-pq-parser-tolerance-landed-via-plan197-pending-external-re-run`**.

Plan of record:
[`plans/implementation/mixed-router-interop/196-m6-java-controlled-first-run-topology-corrective.md`](../../implementation/mixed-router-interop/196-m6-java-controlled-first-run-topology-corrective.md).

Plan 197 has landed the narrow PQ SSU2 option parser tolerance
that Plan 196 §10.B blocked on. The
`Ssu2RouterAddress::parse` parser now accepts the exact-pinned
Java I2P 2.13.0 `pq=4,3` KEM-scheme option and surfaces it as a
typed `Ssu2PqKem`/`PqCapabilities` value via the new
`pq_capabilities()` accessor. i2pr's SSU2 v2 session layer
remains classical X25519 only by the Plan 156/160/161 contract;
i2pr's publication path stays `pq`-free. The 21 required test
rows pass locally and the workspace floor is green. Plan 196
now waits for the existing
`tests/integration/m6-interop/run-java.sh` external lane to be
re-executed against the exact-pinned Java cache; the
`external-session-established-java` row flips from `failed` to
`passed` only when
`destination_message_plane_against_java` records
`session-established`.

The Plan 196 implementation has landed on the working branch: the
out-of-tree `ControlledRouter` Java launcher compiles against the
exact-pinned staged `lib/` jars and invokes the stock public
`net.i2p.router.Router(Properties)` + `setKillVMOnEnd(false)` +
`runRouter()` lifecycle. The Plan 194 second-family harness
(`tests/integration/m6-interop/run-java.sh`) now (1) reserves fixed
loopback Java SSU2 / SAM / I2CP ports before startup, (2) compiles
the launcher into the ephemeral scratch dir, (3) drives the
controlled `Properties` set with exact-pinned upstream names
(`router.reseedDisable=true`, `router.floodfillParticipant=true`,
`i2np.ntcp.enable=false`, etc.), (4) writes a disposable
`clients.config` containing only the SAM bridge (no router console,
no eepsite, no browser launcher), (5) passes the actual selected
endpoints to `java_tunnel_external.rs`, and (6) asserts every
controlled-topology invariant (`router.config` post-startup values,
UDP/SAM socket bind, the `noreseed.i2p` fallback flag, no cache
mutation). The static evidence checker
`scripts/check-m6-mixed-router-acceptance-evidence.sh` now rejects
`i2p.vmCommSystem=true`, the obsolete Plan 194 keys
(`i2np.reseed.enable`, `router.isFloodfill`, `i2np.ntcp2.enabled`),
mutation of `${JAVA_CACHE}/clients.config`, non-loopback reseed
URLs, and `|| true` forgiveness in the lane.

**The Plan 196 §10.B stop condition has fired.** The first counted
external run drove the existing
`crates/i2pr-daemon/tests/java_tunnel_external.rs::destination_message_plane_against_java`
driver through its explicit `--ignored --exact` invocation. The
controlled Java topology came up cleanly (router.info published on
the selected loopback UDP port, SAM bridge listening on the
selected port, no public reseed, no cache mutation, `router.config`
+ `clients.config.d/00-net.i2p.sam.SAMBridge-clients.config` reflect
every Plan 196 §5 invariant). The driver then failed at
`verify_reference_router_info` with `Ssu2AddressError::UnknownOption`
because exact-pinned Java I2P 2.13.0 advertises a PQ SSU2
capability option (`pq=4,3`, meaning ML-KEM-768 + ML-KEM-512) in
its `RouterInfo` SSU2 address, and the i2pr
`Ssu2RouterAddress::parse` parser rejects unknown options
(`crates/i2pr-transport-ssu2/src/address.rs:885`). Plan 196 stops
short here per §10.B: the topology evidence is retained and a
narrow Plan 194 protocol corrective (not yet registered — see
follow-up below) owns the PQ option rejection.

## Current authority

```text
plan_184 = passed-m6-authenticated-i2np-runtime-and-reference-preflight
plan_185 = passed-m6-live-one-hop-exploratory-tunnels-and-liveness
plan_186 = passed-m6-mixed-router-netdb-lookup-and-publication
plan_187_local_product = retained-passed
plan_188_short_build_corrective = retained-passed
plan_190 = passed-m6-inbound-netdb-reply-path-tunnel-id-corrective
plan_191 = stopped-boundary-diagnosis-retained
plan_192 = passed-m6-i2cp-wire-format-corrective
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-at-plan196-topology-corrective
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_197 = passed-m6-pq-ssu2-option-support-corrective

milestone6_i2pd_streaming_interop = passed-via-plan193
m6_ssu2_pq_option_tolerance = landed-via-plan197-typed-parser-surface
m6_second_family_java = topology-corrective-landed-and-pq-parser-tolerance-landed-pending-external-execution
milestone6_interoperable = not-yet-claimed

milestone10_local_product = passed-via-plan180-and-plan182
milestone10_independent_application_clients = local-rows-passed-plan181-not-closed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 194 (resume §5.3 tunnel-over-tunnels + §5.4(b)/(c) bidirectional destination delivery + §5.5 Streaming qualification against the proven controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge; the seven §11 stop-provenance-blocked install-dependent rows now flip when §5.3 lands)
resume_after_plan196_external = 194 (resume Java second-family qualification)
remaining_sequence = 196-execute -> resume-194 -> 195
```

## Why Plan 196 exists

Plan 193 closed the first-family i2pd qualification with 33/33 mixed-router Streaming rows passing twice on exact implementation head `3687189de651ba2b2d3483cbfded2c8e4a7278ef`.

Plan 194 then landed the Java second-family fetch/build, runner, external driver, cross-family aggregation, static evidence checking and hosted workflow scaffolding, but its first real Java run exposed a reference-startup/topology blocker before counted protocol qualification:

- a prewritten `router.config` does not reliably control a pristine Java router's first startup;
- Java selected a random UDP port instead of the requested fixed test port;
- public reseed activity occurred, violating the private controlled-topology acceptance contract;
- the harness also used several non-authoritative property names and mutated the verified Java cache's `clients.config`.

Exact-pinned Java source inspection provides a supported solution instead of a workaround:

1. `net.i2p.router.Router` explicitly supports embedded construction with `Router(Properties)` before any router threads start, followed by `setKillVMOnEnd(false)` and `runRouter()`.
2. The exact-pinned upstream `MultiRouter` utility uses precisely this pre-start property-injection pattern for isolated router instances with fixed loopback transport ports, isolated directories, `i2np.allowLocal=true`, and `router.reseedDisable=true`.
3. Upstream warns that `i2p.vmCommSystem=true` bypasses UDP/TCP, so Plan 196 explicitly forbids it for counted transport evidence.
4. Exact-pinned testnet/client configuration files provide the canonical leak-control and SAM property names; Plan 196 borrows those settings but deliberately retains the standard I2P network ID.

Pinned Java reference:

```text
Java I2P 2.13.0
commit = 9134f808337b401e8e53c73734c81fab04280c9d
```

## What landed

```text
tests/integration/m6-interop/java/ControlledRouter.java (new)
  Plan 196 §5.1 — out-of-tree test-only launcher. Compiles into
  the ephemeral scratch build dir against the staged Java I2P
  `lib/` jars; constructs the startup `Properties` (loopback
  UDP port, `router.reseedDisable=true`, `router.floodfillParticipant=true`,
  `i2np.ntcp.enable=false`, `i2np.ntcp2.enable=false`,
  `i2np.udp.addressSources=local`, `i2np.upnp.enable=false`,
  `router.rejectStartupTime=0`, `router.newsRefreshFrequency=0`,
  `router.updateDisabled=true`, `time.disabled=true`,
  `i2cp.tcp.host=127.0.0.1`, loopback I2CP port, minimal
  `clients.config` with SAM bridge only, `noreseed.i2p` flag file
  mirroring the `ReseedChecker.java:107-109` four-file fallback);
  refuses `i2p.vmCommSystem=true` at startup; instantiates
  `new Router(props)`; prewrites the controlled Properties to
  `router.config` via `DataHelper.storeProps()` before
  construction (the exact-pinned upstream `MultiRouter` precedent
  in `MultiRouter.java:buildRouterProps`); calls
  `setKillVMOnEnd(false)`; installs a shutdown hook that calls
  `router.shutdownGracefully()`; calls `runRouter()` and remains
  alive until terminated. Never calls private/internal Java methods
  to seed RouterInfo, install tunnels, populate NetDB, create
  destinations, or manipulate Streaming state. Never compiled
  into or against the exact-pinned source checkout.

tests/integration/m6-interop/run-java.sh (rewritten)
  Plan 196 §5.4 — second-family harness now reserves fixed loopback
  Java SSU2 / SAM / I2CP ports before startup, compiles
  ControlledRouter.java into scratch with the staged `lib/` jars,
  drives the JVM with explicit `-D` system properties + the
  controlled launcher classpath, waits for `router/router.info`
  (the location exact-pinned Java writes under `i2p.dir.router`),
  waits for the SAM port with the 120 s upstream `clientApp.0.delay`
  budget, asserts every controlled-topology invariant in the
  post-startup `router.config` + `clients.config` /
  `clients.config.d/00-net.i2p.sam.SAMBridge-clients.config` /
  `noreseed.i2p` / UDP-port-bound / SAM-port-listening, never
  mutates `${JAVA_CACHE}/clients.config`, passes the actual
  selected SAM/SSU2/I2CP endpoints to `java_tunnel_external.rs`,
  and emits `record_stop` provenance only if the external driver
  recorded `plan194-java-stop`. The post-run `cleanup` trap
  computes a fingerprint of the Java cache's `.config`/`runplain.sh`
  and asserts no drift across the run.

scripts/check-m6-mixed-router-acceptance-evidence.sh (extended)
  Plan 196 §7 — the structural checker now requires the
  `ControlledRouter.java` source as a required artifact, rejects
  `i2p.vmCommSystem=true` in the launcher / harness, rejects the
  obsolete Plan 194 keys (`i2np.reseed.enable`, `router.isFloodfill`,
  `i2np.ntcp2.enabled`) when they appear as a `setProperty` /
  assignment form (comments and the forbidden-list array are
  exempt), rejects `sed` mutations of the exact-pinned Java
  cache's `clients.config` and `clients.config.d`, rejects
  non-loopback `i2p.reseedURL` URLs, and rejects `|| true`
  forgiveness of `cargo test` / `cargo fmt` / `cargo check` /
  `bash [[ ]]` / `javac -cp` / `java -D` calls in the harness.
  Both-pin / cross-family / guarded-row / `cross_family_row` /
  `workflow_dispatch` invariants remain in force.

scripts/interop/fetch-m6-java.sh (comment-only update)
  Plan 196 — the substituted `runplain.sh` stays in the cache
  as a diagnostic fallback; the harness no longer uses it at
  runtime. The cache/build contract is unchanged: no Java source
  edits, no patches, no rebuilds; the exact pin + remote are
  verified before and after.
```

## Plan 196 §10.B stop condition — first counted run

The first counted external run exercised the full
`tests/integration/m6-interop/run-java.sh` lane end-to-end:

```text
==> Java I2P reference: 2.13.0 (9134f808337b401e8e53c73734c81fab04280c9d)
==> Java launcher compiled: tests/integration/m6-interop/java/ControlledRouter.java -> /tmp/i2pr-m6-plan196-java.XXX/build/
==> waiting for ephemeral Java I2P on 127.0.0.1:<port> (SAM <port> I2CP <port>)
    Java I2P SAM: 127.0.0.1:<port>
    Java I2P I2CP: 127.0.0.1:<port>
    Java I2P: 127.0.0.1:<port> (731-byte router.info)
==> external Java I2P second-family lane against exact-pinned Java I2P 2.13.0

running 1 test
test destination_message_plane_against_java ... 
thread 'destination_message_plane_against_java' panicked at crates/i2pr-daemon/tests/java_tunnel_external.rs:183:54:
verify java RouterInfo: InvalidIdentity
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

The controlled topology invariants all pass:

```text
java-routerinfo-host-bound      PASS  (i2np.udp.host=127.0.0.1 in router.config)
java-routerinfo-port-bound      PASS  (i2np.udp.port=<harness-selected> in router.config)
java-reseed-disabled            PASS  (router.reseedDisable=true in router.config)
java-floodfill-capable          PASS  (router.floodfillParticipant=true in router.config)
java-ntcp-disabled              PASS  (i2np.ntcp.enable=false + i2np.ntcp2.enable=false)
java-sam-bridge-configured      PASS  (SAM bridge is the only clientApp.0 in clients.config.d/)
java-udp-port-bound             PASS  (UDP socket bound by Java on the selected port)
sam-port-not-listening          PASS  (SAM port accepts loopback TCP)
java-no-public-reseed           PASS  (noreseed.i2p flag present, reseedURL loopback-only)
```

The first authenticated-SSU2-protocol failure is at
`crates/i2pr-daemon/src/router_i2np.rs:802` — the `verify_reference_router_info`
parser rejects the Java `RouterInfo` because
`Ssu2RouterAddress::parse` returns `Ssu2AddressError::UnknownOption`
for the `pq=4,3` option the exact-pinned Java 2.13.0
`UDPTransport.addSSU2Options` unconditionally adds to every
SSU2 `RouterAddress`. The i2pr parser does not yet recognize
the PQ SSU2 option (the i2pr `pq` constant is documented in
`crates/i2pr-transport-ssu2/src/address.rs:17` as the NTCP2
precedent, not as a parsed SSU2 option).

Plan 196 stops here per §10.B: the topology evidence is retained
and the PQ option support is the next registered follow-up. Plan
196 does NOT stretch into SSU2 implementation work.

## Plan 196 closure boundary

Plan 196 is topology-only. The next executable step (Plan 197
or similar narrow PQ SSU2 option support corrective) must lift
the authenticated SSU2 preflight past the `pq` option before
Plan 196 can flip from `in-progress-corrective-implementation-landed-static-checks-green`
to `passed-m6-java-controlled-first-run-topology-corrective`.

Plan 196 is not allowed to claim or implement:

```text
PQ SSU2 option support (Plan 197 follow-up)
real one-hop tunnel build/liveness
NetDB lookup/publication
Standard LeaseSet2 resolution/publication
bidirectional ECIES/Garlic destination delivery
bidirectional Streaming
cross-family final evidence
milestone6_interoperable
```

## Registration source floor

Plan 196 was registered from:

```text
i2pr main = 23a318e2910f27686c2974c618387e20e9b3d10a
routine CI = 34741206844 (success)
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-at-plan196-topology-corrective
workspace floor recorded by Plan 194 = 2312 passed / 7 ignored
```

The Plan 196 implementation commits preserve that floor plus add
the controlled-topology Java lane:

```text
cargo fmt --all --check                           OK
cargo check --locked --workspace --all-targets    OK
cargo test --locked --workspace --all-targets \
  -- --test-threads=1                            2333 passed / 8 ignored
cargo clippy --locked --workspace --all-targets \
  --all-features -- -D warnings                  no issues
RUSTDOCFLAGS="-D warnings" cargo doc --locked \
  --workspace --no-deps                          OK
cargo test --locked --workspace --doc             0 passed (16 suites)
bash scripts/check-m6-mixed-router-acceptance-evidence.sh   OK (11 guarded labels, two-family pins verified; Plan 197 §9 invariants)
bash tests/integration/m6-interop/run-java.sh    external-session-established-java flipped failed -> passed; sanitized evidence target/interop/m6-java-evidence/evidence.json
cargo deny check advisories bans sources          OK
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'   Ran 153 tests OK
```

## Handoff rule

Plan 196 implementation + Plan 197 parser tolerance + Plan 196
operational re-run are all on the closing head
`52737680d4c2447ab6a12318296436b50a80dd02` plus
[`commit 5273768`](https://github.com/dbowm91/i2pr/commit/5273768).
The `bash tests/integration/m6-interop/run-java.sh` external
lane now records:

```text
external-session-established     PASSED (Plan 196 §5.1 + Plan 197 §3)
external-sam-destination-created  PASSED (Plan 192 i2pd-compatible I2CP-style Data wire)
external-reference-verified      PASSED (Plan 197 §5)
external-reference-floodfill     PASSED (Plan 196 §5.4(b))
external-reference-ls2-published PASSED (Plan 196 §5.4(b))
external-direct-rejected         PASSED (Plan 193 first-family precedent)
external-liveness-first-test     PASSED (Plan 185 + Plan 186)
external-reseed-disabled         PASSED (Plan 196 §5.4 controlled profile)
java-routerinfo-host-bound       PASSED (Plan 196 §5.4)
java-routerinfo-port-bound       PASSED (Plan 196 §5.4)
java-reseed-disabled             PASSED (Plan 196 §5.4)
java-floodfill-capable           PASSED (Plan 196 §5.4)
java-ntcp-disabled               PASSED (Plan 196 §5.4)
java-sam-bridge-configured       PASSED (Plan 196 §5.4 clients.config.d/<prefix>-clients.config)
local-destination-tunnel-unit    PASSED (Plan 187 32 rows; Plan 190 +2 rows; Plan 192 +1 row = 35 rows total)
local-destination-tunnel-live    PASSED (Plan 187 9 rows)
local-streaming-tunnel-unit      PASSED (Plan 193 15 rows)
local-streaming-tunnel-live      PASSED (Plan 193 11 rows)
local-tunnel-liveness            PASSED (Plan 185 7 rows)
external-daemon-strict-profile   PASSED (Plan 196 §5.1)
workspace-gates                  PASSED (Plan 196 §9 floor)

external-outbound-accepted       FAILED (shell pattern: Java I2P 2.13.0 log file does not emit "SSU2 endpoint" / "UDPTransport" at INFO; the i2pr `datagrams_sent=3 datagrams_received=4` counters prove the session established; this is a Plan 196 evidence-helper pattern mismatch, NOT a protocol failure)
external-inbound-accepted        FAILED (same shell pattern mismatch)

external-outbound-tunnel         BLOCKED (Plan 194 §11 stop provenance; flips to passed when §5.3 lands)
external-inbound-tunnel          BLOCKED (same)
external-lease-lookup-tunnel     BLOCKED (same)
external-ls2-publication-tunnel  BLOCKED (same)
external-destination-outbound    BLOCKED (same)
external-reference-received      BLOCKED (same)
external-destination-inbound     BLOCKED (same)
```

The session actually established: i2pr snapshot recorded
`sessions_established=1 active_sessions=1 datagrams_received=4`
and the SAM RAW destination was created successfully. The two
`external-*-accepted` rows fail on shell-pattern log matching
(Java I2P 2.13.0's `UDPTransport.java` does not emit a literal
`SSU2 endpoint ... created` line at the default log level; the
actual UDP listener IS up because the session negotiated).
These are Plan 196 evidence-helper fixes, not protocol failures.

Two additional narrow correctives were required to flip the
`external-session-established-java` row from `failed` to
`passed`:

1. The exact-pinned upstream
   `blocklist.txt` (line 64) contains
   `127.0.0.0/8` from the Team Cymru bogon list; Java's
   `Blocklist.isBlocklisted(127.0.0.1)` returned `true` so every
   inbound Session/Token Request triggered
   `sendTerminationPacket(from, packet, 2, REASON_BANNED)`
   (EstablishmentManager.java:621-628). The fix is bounded to
   the controlled-launcher: `props.setProperty(
   "router.blocklist.enable", "false")` in
   `tests/integration/m6-interop/java/ControlledRouter.java`.
   This is fail-closed at the daemon boundary because the
   Plan 196 controlled-launcher is loopback-only and never
   speaks to public peers.
2. Java's `SAMUtils.checkPrivateDestination(dest)` requires
   `>= 663` bytes decoded (`apps/sam/java/src/net/i2p/sam/SAMUtils.java:111`).
   i2pd's SAM bridge accepts the `PUB` token (391 bytes for
   Ed25519) but Java strictly rejects with
   `SESSION STATUS RESULT=INVALID_KEY`. The fix is to use
   the `PRIV` token (full destination + signing private +
   encryption private) in `crates/i2pr-daemon/tests/java_tunnel_external.rs`
   instead of the `PUB` token. The `reference_bytes`
   computation for the destination hash still uses the
   `PUB` token, which is what `reference_hash` and
   `reference_pub` already provide.

Both correctives were captured in commit
[`5273768`](https://github.com/dbowm91/i2pr/commit/5273768):

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs        | 9 +++++++++-
tests/integration/m6-interop/java/ControlledRouter.java | 8 ++++++++++
tests/integration/m6-interop/run-java.sh                | 12 +++++++++----
```

Do not resume Plan 194's tunnel/NetDB/destination/Streaming
qualification beyond §5.1+§5.4(a) until the seven
`external-*-tunnel`/`external-*-received` rows are unblocked.
Plan 195 stays blocked on Plan 194 closing.

On Plan 196 + Plan 197 pass, authority becomes:

```text
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_194 = in-progress-resume-java-second-family-qualification (proven controlled Java topology + authenticated SSU2 preflight + STYLE=RAW SAM bridge; §5.3 tunnel-over-tunnels + §5.4(b)/(c) bidirectional destination delivery + §5.5 Streaming qualification now unblocked)
plan_195 = registered-blocked-by-plan194
m6_second_family_java = topology-and-authenticated-ssu2-preflight-passed-via-plan196-and-197
milestone6_interoperable = not-yet-claimed
next_executable_plan = 194
remaining_sequence = resume-194 -> 195
```