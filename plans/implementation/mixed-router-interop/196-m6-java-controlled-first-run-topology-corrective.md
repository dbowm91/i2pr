# Plan 196 — M6 Java I2P controlled first-run topology corrective

Status: **registered executable corrective**. This plan is the only executable work on the retained M6/M10 blocker line until it passes or records a narrower protocol stop. Plan 194 remains the actual Java second-family qualification/closure gate; Plan 195 remains blocked by Plan 194.

## 1. Goal

Remove the Plan 194 §11 first-run topology blocker without patching Java I2P and without weakening the controlled/private interop contract.

The current Plan 194 harness prewrites `router.config` and launches the staged Java I2P distribution normally. On a fresh first run, stock Java I2P 2.13.0 regenerates/overrides critical startup configuration, selects a random UDP port, and attempts public reseed. That prevents the exact-pinned reference from reaching the deterministic loopback SSU2/SAM topology required by Plan 194.

Plan 196 must replace that fragile first-run file race with the exact-pinned Java router's supported embedded startup path: construct the stock `net.i2p.router.Router` with a complete `Properties` object **before any router thread starts**, then call `setKillVMOnEnd(false)` and `runRouter()`. This is an out-of-tree test launcher only; the exact-pinned Java source/package remains unmodified.

Plan 196 closes only when the controlled Java topology is proven usable and the existing Plan 194 external driver reaches an authenticated SSU2 session against it. It does **not** claim tunnel, NetDB, destination, Streaming, cross-family, or Milestone 6 closure. Those remain Plan 194 work.

## 2. Starting authority

Starting repository authority:

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_194 = in-progress-scaffolding-landed-blocked-at-java-first-run-topology
plan_195 = registered-blocked-by-plan194
plan_197 = implementation-landed-parser-tolerance-static-floor-green-pending-plan196-external-re-run (parser-only tolerance of the SSU2 `pq` option Java I2P 2.13.0 unconditionally publishes; typed Ssu2PqKem/PqCapabilities surface with bounded MAX_SSU2_PQ_SCHEMES = 8; i2pr session layer remains classical X25519 only; i2pr publication path stays pq-free; ML-KEM not implemented, claimed, or silently enabled; 21 required test rows green locally)
milestone6_i2pd_streaming_interop = passed-via-plan193
m6_second_family_java = topology-corrective-landed-and-pq-parser-tolerance-landed-pending-external-execution
m6_ssu2_pq_option_tolerance = landed-via-plan197-typed-parser-surface
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
next_executable_plan = 196 (re-run external lane; session-established-java row must flip on the exact-pinned Java 2.13.0 cache; the Plan 197 parser tolerance is already in place)
```

Reference remains exactly:

```text
Java I2P 2.13.0
repository = https://github.com/i2p/i2p.i2p.git
commit     = 9134f808337b401e8e53c73734c81fab04280c9d
```

Retain Plan 193's exact-pinned i2pd evidence unchanged.

## 3. Research findings that define the corrective

### 3.1 Stock Java I2P already supports pre-start embedded configuration

At the exact Plan 194 pin, `router/java/src/net/i2p/router/Router.java` documents embedded use directly:

```text
Router r = new Router(props);
r.setKillVMOnEnd(false);
r.runRouter();
```

The `Router(Properties)` constructor starts no threads and accepts the base/config directory and `router.configLocation` properties before initialization. This means Plan 196 does not need a first-run/restart workaround and must not patch Java source.

Pinned source:

```text
https://github.com/i2p/i2p.i2p/blob/9134f808337b401e8e53c73734c81fab04280c9d/router/java/src/net/i2p/router/Router.java
```

### 3.2 Exact-pinned `MultiRouter` is the upstream topology precedent

The same exact-pinned tree's `router/java/src/net/i2p/router/MultiRouter.java` creates stock routers from `Properties` before startup and sets isolated directories, `router.configLocation`, `router.reseedDisable=true`, loopback NTCP/UDP hosts, fixed UDP/internal ports, fixed I2CP ports, `i2np.allowLocal=true`, and `router.rejectStartupTime=0` before `runRouter()`.

It also explicitly warns that `i2p.vmCommSystem=true` bypasses UDP/TCP and therefore is not suitable for testing real transports. Plan 196 must keep the normal transport implementation and reject VMCommSystem use.

Pinned source:

```text
https://github.com/i2p/i2p.i2p/blob/9134f808337b401e8e53c73734c81fab04280c9d/router/java/src/net/i2p/router/MultiRouter.java
```

### 3.3 Use upstream property names, not the current harness aliases

The current `run-java.sh` prewritten config includes keys that are not the authoritative Java I2P properties for this purpose (`i2np.reseed.enable`, `router.isFloodfill`, `i2np.ntcp2.enabled`). Replace them rather than preserving compatibility aliases in the test lane.

Use exact-pinned upstream names/patterns, including:

```text
router.reseedDisable=true
router.floodfillParticipant=true
i2np.ntcp.enable=false
i2np.allowLocal=true
i2np.udp.host=127.0.0.1
i2np.udp.port=<fixed-selected-port>
i2np.udp.internalPort=<same-fixed-selected-port>
i2np.udp.addressSources=local
i2np.upnp.enable=false
router.rebuildKeys=false
router.rejectStartupTime=0
router.newsRefreshFrequency=0
router.updateDisabled=true
time.disabled=true
```

The exact-pinned `installer/resources/router.testnet.config` supplies the leak-control precedent for reseed, UPnP, news, NTP and local-address settings. **Do not copy its `router.networkID=99` setting.** Plan 194 is testing ordinary I2P protocol interoperability, so Java and i2pr must remain on the standard network ID.

### 3.4 SAM should be a disposable client config, not a mutation of the reference cache

Exact-pinned `installer/resources/clients.config` launches SAM with:

```text
main = net.i2p.sam.SAMBridge
args = sam.keys 127.0.0.1 <sam-port> i2cp.tcp.host=127.0.0.1 i2cp.tcp.port=<i2cp-port>
```

The current harness mutates `${JAVA_CACHE}/clients.config` with `sed`. Remove that. The exact-pinned cache/build output is reference material and should remain immutable after verification.

Generate a minimal `${JAVA_DATA}/clients.config` containing only the SAM bridge with `startOnLoad=true`. No router console, browser launcher, eepsite, or i2ptunnel application is required for this qualification topology.

## 4. Architecture lock

The controlled topology remains:

```text
one normal i2pr daemon
    <---- real loopback UDP / SSU2 ---->
one unmodified exact-pinned Java I2P Router instance

Java:
  ordinary UDP/SSU2 transport implementation
  fixed loopback transport port
  stock RouterInfo generation/signing
  stock floodfill/NetDB implementation
  stock SAM bridge on loopback
  no public reseed/network dependency

i2pr:
  existing Plan 184-193 production path
  no test-only peer/tunnel injection
```

Forbidden shortcuts:

- Java source edits or patched jars;
- `i2p.vmCommSystem=true`;
- private Java NetDB/tunnel/Streaming state injection;
- `LocalZeroHop` substitution for counted i2pr router paths;
- direct destination-over-SSU2 substitution;
- public reseed/public I2P participation for green evidence;
- Docker, namespaces, VM/Multipass, root/sudo, or systemd requirements;
- a prebuilt secret-bearing Java datadir checked into the repository.

An out-of-tree launcher compiled under `target/` or the ephemeral evidence scratch directory is allowed because it only invokes the public stock Router lifecycle and supplies startup properties.

## 5. Required implementation

### 5.1 Add the controlled stock-router launcher

Add a small test-only Java source such as:

```text
tests/integration/m6-interop/java/ControlledRouter.java
```

Responsibilities only:

1. parse explicit paths/ports supplied by the shell harness;
2. create the disposable directories;
3. construct the startup `Properties`;
4. instantiate exact-pinned `net.i2p.router.Router`;
5. call `setKillVMOnEnd(false)`;
6. install a shutdown hook that calls the public shutdown lifecycle;
7. call `runRouter()` and remain alive until terminated.

It must not call private/internal Java methods to seed RouterInfo, install tunnels, populate NetDB, create destinations, or manipulate Streaming state.

Compile it against the exact staged Java I2P `lib/` jars into the ephemeral scratch/build directory. Never copy it into or compile it over the exact-pinned source checkout.

### 5.2 Build the property set before `new Router(...)`

At minimum set the following from explicit harness-selected paths/ports:

```text
i2p.dir.base=<verified staged Java distribution>
i2p.dir.config=<JAVA_DATA>
i2p.dir.log=<JAVA_DATA>/logs
i2p.dir.pid=<JAVA_DATA>
i2p.dir.router=<JAVA_DATA>
i2p.dir.app=<JAVA_DATA>
router.configLocation=<JAVA_DATA>/router.config
router.clientConfigFile=<JAVA_DATA>/clients.config

router.reseedDisable=true
i2p.reseedURL=https://127.0.0.1:1/disabled
router.newsRefreshFrequency=0
router.updateDisabled=true
time.disabled=true
time.sntpServerList=localhost

router.floodfillParticipant=true
router.rebuildKeys=false
router.rejectStartupTime=0

i2np.allowLocal=true
i2np.udp.enable=true
i2np.udp.host=127.0.0.1
i2np.udp.port=<JAVA_SSU2_PORT>
i2np.udp.internalPort=<JAVA_SSU2_PORT>
i2np.udp.addressSources=local
i2np.upnp.enable=false
i2np.ntcp.enable=false

i2cp.tcp.bindAllInterfaces=false
i2cp.tcp.host=127.0.0.1
i2cp.port=<JAVA_I2CP_PORT>
```

Do not set `router.networkID`; retain Java I2P's standard network ID/default.

If exact-pinned source inspection proves one listed key is ignored or renamed at this revision, use the exact property consumed by that source and document the evidence in Plan 196 status. Do not silently add multiple aliases.

### 5.3 Generate a minimal disposable clients config

Before router construction, write `${JAVA_DATA}/clients.config` with one SAM application only:

```text
clientApp.0.main=net.i2p.sam.SAMBridge
clientApp.0.name=SAM application bridge
clientApp.0.args=sam.keys 127.0.0.1 <JAVA_SAM_PORT> i2cp.tcp.host=127.0.0.1 i2cp.tcp.port=<JAVA_I2CP_PORT>
clientApp.0.startOnLoad=true
```

Do not modify `${JAVA_CACHE}/clients.config` or `${JAVA_CACHE}/clients.config.d`.

### 5.4 Rework `run-java.sh` only at the topology/startup boundary

Update:

```text
tests/integration/m6-interop/run-java.sh
```

Required changes:

- keep exact pin/cache verification from Plan 194;
- reserve/select fixed loopback Java SSU2, SAM and I2CP ports;
- compile the controlled launcher into scratch;
- start Java through the controlled launcher rather than `runplain.sh` first-run behavior;
- delete the prewritten `router.config` + mutation/race logic that Plan 194 proved ineffective;
- delete all `sed` mutations of the exact-pinned cache;
- retain Plan 194 result helpers/evidence format;
- retain fail-closed startup, timeout and cleanup behavior;
- pass the actual selected SAM/SSU2 endpoints to `java_tunnel_external.rs` rather than assuming 7656 when a dynamic loopback port is selected.

Do not lift Plan 194's later tunnel/NetDB/destination/Streaming rows into Plan 196. The existing Plan 194 driver remains the owner of those rows.

### 5.5 Topology evidence must be externally observable

Record sanitized command-derived facts, not merely launcher claims:

- exact Java source revision and clean/unmodified reference material;
- Java process alive under the disposable data dir;
- `router.info` generated by the stock router;
- RouterInfo advertises the selected loopback SSU2 endpoint/port expected by the driver;
- fixed SSU2 UDP port is actually bound by the Java process;
- SAM accepts a normal loopback TCP connection at the selected port;
- floodfill capability is present when required by the Plan 194 driver;
- no reference cache file changed during the run (pre/post digest or git/source cleanliness check as appropriate);
- no non-loopback transport address is advertised;
- configured reseed URL is loopback-only and `router.reseedDisable=true` remains effective in the runtime config;
- selected listeners/process disappear after cleanup.

Raw private router keys, Destination secrets, application bodies, or full logs containing sensitive material are never copied into evidence.

## 6. Plan 194 preflight gate

After topology readiness, run the existing ignored Plan 194 Java external driver through its ordinary invocation.

Plan 196's counted interop gate is narrow:

```text
external-reference-verified-java = passed
external-session-established-java = passed
```

The authenticated SSU2 session must be established by the existing i2pr daemon-owned runtime against the stock Java RouterInfo/transport endpoint. This proves the topology is usable rather than merely proving that a Java process opened sockets.

Tunnel-build, NetDB, LeaseSet2, raw destination and Streaming rows remain Plan 194 acceptance work and may stay blocked during Plan 196.

## 7. Static evidence/integrity checks

Extend the existing M6 evidence checker or add one narrow Plan 196 checker. Routine CI must reject at least:

- missing exact Java pin;
- any edit/copy-over of files under the verified Java source/cache after fetch/build;
- `i2p.vmCommSystem=true`;
- the obsolete Plan 194 topology keys `i2np.reseed.enable`, `router.isFloodfill`, or `i2np.ntcp2.enabled` in the controlled launcher/harness;
- a non-loopback reseed URL;
- public/network-dependent success semantics;
- hard-coded `passed` topology or Java-session rows;
- an external Java row that becomes passed without the external driver's command exit/evidence keys;
- mutation of `${JAVA_CACHE}/clients.config` or `clients.config.d`;
- a topology lane that forgives launcher/port/SAM/session failure with `|| true` or equivalent.

Keep the expensive Java execution manual/external; routine CI statically validates the topology/evidence boundary.

## 8. Focused validation

At minimum execute:

```text
bash scripts/interop/fetch-m6-java.sh --rebuild
bash <Plan-196 topology/evidence checker>
bash tests/integration/m6-interop/run-java.sh
```

Plus the ordinary repository floor on the final implementation revision:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
cargo deny check advisories bans sources
```

Retain Plan 193's i2pd evidence as passed. Plan 196 does not need to rerun the full 33-row i2pd Streaming matrix unless code outside the Java harness/topology boundary changes.

## 9. Acceptance criteria

Plan 196 passes only when all are true:

1. Exact-pinned Java I2P 2.13.0 / `9134f808337b401e8e53c73734c81fab04280c9d` remains unmodified.
2. The controlled launcher is out-of-tree/test-only and uses stock `Router(Properties)` + public lifecycle only.
3. The Java base/cache remains immutable across the counted run.
4. Every mutable/config/data/log/pid/router/app path is inside the disposable Java data directory.
5. The router uses normal UDP/SSU2 transport; VMCommSystem is absent/false.
6. The selected Java SSU2 port is fixed before startup, actually bound, and represented consistently in the RouterInfo consumed by the Plan 194 driver.
7. Relevant Java transport and SAM/I2CP listeners remain loopback-only.
8. Public reseed/network dependency is disabled; no public reseed is required for readiness or session establishment.
9. Floodfill mode required by Plan 194 is enabled through the exact upstream property.
10. SAM starts from the disposable minimal client config on the exact selected loopback port.
11. The Plan 194 external driver verifies the Java reference and establishes an authenticated SSU2 session through the normal i2pr runtime.
12. No tunnel/NetDB/Destination/Streaming state is synthetically installed or injected.
13. Shutdown returns process/listener/resource baselines to zero and removes the scratch data cleanly.
14. Static topology/evidence integrity checks pass.
15. Full workspace quality floor and exact-head routine CI pass.
16. `plans/closure/mixed-router-interop/196-status.md` records the exact-head evidence and advances `next_executable_plan = 194` without claiming `milestone6_interoperable`.

## 10. Stop conditions

Stop and preserve a minimized reproducer if any of the following occurs:

### A. Stock property is ignored before topology readiness

If the exact-pinned `Router(Properties)` path ignores/overwrites a critical loopback/reseed/port property, identify the first ignored property from exact source/runtime evidence. Do not patch Java and do not fall back to public networking. Register a narrower Java-startup corrective only if no standards/reference-supported property path exists.

### B. Topology becomes correct but authenticated SSU2 fails

If RouterInfo, fixed port, loopback listeners, floodfill and no-reseed constraints are all proven correct but the Plan 194 external driver reaches a genuine SSU2 protocol failure, do **not** stretch Plan 196 into SSU2 implementation work. Preserve the first protocol failure and register one narrow Plan 194 protocol corrective. Retain the topology evidence.

### C. Authenticated SSU2 passes

This is the intended Plan 196 closure. Do not continue implementing tunnels/NetDB/Destination/Streaming under Plan 196. Return authority to Plan 194.

## 11. Handoff after pass

On success (combined with the registered Plan 197 PQ SSU2 option
support corrective; see
[`plans/implementation/mixed-router-interop/197-m6-pq-ssu2-option-support-corrective.md`](197-m6-pq-ssu2-option-support-corrective.md)):

```text
plan_193 = passed-m6-i2pd-mixed-router-streaming
plan_197 = passed-m6-pq-ssu2-option-support-corrective
plan_196 = passed-m6-java-controlled-first-run-topology-corrective
plan_194 = in-progress-resume-java-second-family-qualification
plan_195 = registered-blocked-by-plan194

m6_second_family_java = topology-and-authenticated-ssu2-preflight-passed-via-plan196-and-197
milestone6_interoperable = not-yet-claimed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed

next_executable_plan = 194
remaining_sequence = resume-194 -> 195
```

Plan 194 then owns the remaining Java one-hop tunnel, liveness, NetDB, LeaseSet2, bidirectional destination message, Streaming, two-family evidence, exact-head external workflow, and Milestone 6 closure criteria. Plan 195 becomes executable only after Plan 194 sets `milestone6_interoperable = passed-via-plan194`.
