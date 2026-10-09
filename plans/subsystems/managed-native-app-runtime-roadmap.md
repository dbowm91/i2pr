# Managed Native Application Runtime Roadmap

Status: parallel — Plans 345, 349, 352–355, 368–371 and 382–383 are closed. Plan 407 is active for the first qualified Linux `Secured` sandbox backend plus private persistent app data. The former managed-app Plans 385–388 drafts are archived after global-number reconciliation; local-service ingress and the external SDK remain future work. macOS/Windows sandboxing, live AppManager administration, brokered clearnet, UI hosting, remote update/TUF, and scoped Proposal 170 remain downstream. This workstream is parallel to router protocol milestones and does not gate M12, anonymity, transport, or current router interoperability work.

Long-term references:
- `GUARDRAILS.md`
- `specs/CONFORMANCE.md`
- `specs/support.toml`
- `docs/security-model.md`
- `docs/architecture/overview.md`
- `docs/architecture/dependency-graph.md`
- `docs/architecture/i2pr-api.md`
- `docs/architecture/i2pr-i2pcontrol.md`
- `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`

Related ADRs:
- [ADR 0032](../../docs/adr/0032-managed-native-app-process-and-capability-boundary.md) freezes the managed-app trust/process boundary and default-deny network-capability model.
- The Plan-345 language-neutral wire/manifest contract is recorded in [Managed native application contract v1](../../specs/references/managed-native-app-runtime-v1.md). Plan 349 corrected its direction/reply semantics, broker reservation, and network-policy classification; Plan 352 closed the mapped-IPv6 canonicalization gap. Plan 354 owns the private protocol-transport substrate and Plan 355 owns the router-side principal/capability gateway. Plan 368 owns the distinct private daemon↔AppManager protocol/bridge; Plan 369 consumed it for process/runtime lifecycle and is now closed, without claiming sandbox containment.

## 1. Purpose and ownership boundary

Build the foundation for native first- and third-party applications that are launched and supervised as separate processes, use a stable application API to reach I2P networking/control, and can later expose an embedded frontend inside the router console without being linked into the router process.

The router remains the authority for I2P protocol state and app-facing router capabilities. Future user-space application-runtime components own package/lifecycle state, OS sandboxing, resource containment, and user-approved clearnet brokering. Applications never receive router internals, an unrestricted router control credential, or authority to mutate their own security policy.

The intended long-term split is:

```text
future i2pr-console
   |                 \
   | Proposal 170     \ AppManager/admin API
   v                   v
i2pr-daemon <------ trusted application runtime/manager
   ^                         |
   | app-principal gateway   | OS sandbox + process supervision
   |                         v
   +------------------ native application
                       inherited capability channel only
```

The current `i2pr` repository owns the router-facing contract and adapters. OS-specific native application hosting is expected to remain outside router-core crates and may ultimately live in a separate product/runtime repository. Plan 345 freezes that boundary before downstream implementation.

## 2. Work classification

- **Invariant:** process isolation is a trust boundary; applications cannot promote their own permissions; router identities, destination identities, publisher identities, app identities, and launch-instance identities remain separate; direct networking is denied by default; loopback/LAN/public-network access is not implicitly trusted.
- **Infrastructure:** the versioned runtime-neutral application protocol, principals/capabilities, manifest schema, policy vocabulary, UI-bridge contract, sandbox-attestation vocabulary, and resource ceilings.
- **Capability:** later milestones will provide a supervised native process, brokered SAM/I2CP/Proposal-170 access, user-approved clearnet egress, package lifecycle, SDKs, and UI hosting. Plan 345 itself is not a user-visible application capability.
- **Polish:** console UX, app catalog/store behavior, richer diagnostics, updater ergonomics, and developer tooling follow only after security/correctness closure.

## 3. Non-goals

This workstream does not implement mail, IRC, torrent, or any other end-user application.

Plan 345 does not:
- launch an application process;
- add OS sandbox backends;
- add a clearnet proxy/relay path;
- expose SAM/I2CP/Proposal 170 through the new channel yet;
- install or update packages;
- build a router console;
- embed a webview;
- change public router advertisement or protocol claims;
- add an application store;
- make a production anonymity/privacy claim.

The managed-app architecture must not become another destination runtime, tunnel manager, NetDB, SAM implementation, I2CP implementation, or Proposal-170 implementation.

## 4. Current state

The current repository already has the protocol-side prerequisites needed for a future gateway:

- `i2pr-api` owns runtime-neutral SAM 3.1 and I2CP wire/state contracts while `i2pr-daemon` owns the listeners and composition.
- `i2pr-i2pcontrol` owns the runtime-neutral Proposal 170 / I2PControl contract while `i2pr-daemon` adapts it to live router owners.
- `i2pr-runtime` is the sole router production owner of Tokio/sockets/timers/channels.
- `i2pr-daemon` is the composition root and does not expose a global `RouterContext`.
- Current client/admin listeners are loopback-oriented network services, which is not sufficient for a hostile-app sandbox because loopback itself may provide a clearnet escape through local proxies/helpers.

Plan 345 now provides the runtime-neutral `i2pr-app-proto` foundation: application/publisher/instance principals, requested/granted/effective capability types, bounded frame/control vocabulary, manifest/UI descriptors, pure network-policy decisions, resource ceilings, and sandbox-attestation vocabulary. It deliberately provides no transport owner, process supervisor, network broker, package lifecycle owner, or console UI host.

Post-closure review found four contract defects before any runtime consumer existed: hostname rules required a second public-IP allow; the IP classifier treated some non-global/special-purpose ranges as public by fall-through; role-separated messages were not direction/reply complete; and `brokered_tcp` was openable without a defined connect transaction. Plan 349 corrected those defects; see `plans/closure/managed-native-app-runtime/349-status.md`. A later review found a representation-confusion defect: IPv4-mapped IPv6 targets inherited IPv4 scope classification but were not canonicalized before exact IPv4/CIDR rule matching. Plan 352 closed that gap by canonicalizing targets and rejecting mapped policy selectors; see `plans/closure/managed-native-app-runtime/352-status.md`. Plan 353 then reconciled the combined branch, planning authority, generated artifacts, and ADR numbering. The Plan-345/349/352/353 closures remain authoritative for the evidence they ran. Repository review of the next runtime boundary found that SAM and I2CP daemon connection drivers are still concrete-`TcpStream` paths; Plan 354 owns the listener-independent refactor, and Plan 355 consumes it for the app-principal gateway.

For planning purposes, this roadmap assumes Proposal 170 will reach the required complete/stable interface before the future managed-app Proposal-170 adapter is claimed complete. Plans 345, 349, and 352 do not depend on current Proposal-170 closure and may not falsify its current support state.

## 5. Target architecture

The default secured profile is capability-oriented rather than "ordinary process plus firewall":

```text
native app
   |
   | one inherited/duplicated IPC capability
   v
trusted app runtime
   |            \
   |             \ user-approved brokered clearnet stream
   |
   +---- router app gateway ----> same SAM/I2CP/Proposal-170 owners
```

The application has no ordinary host network path in the secured profile. I2P access is supplied through the capability channel. Clearnet access, when approved by the user, is opened by a trusted broker and returned as a logical byte stream; the application does not receive authority to edit the policy.

The API has two distinct principals/surfaces:

1. **Application channel:** lifecycle handshake, app identity, effective capability inspection, SAM/I2CP access, scoped control access, UI/backend messages, brokered network requests, health/shutdown, and permission *requests*.
2. **Administrative AppManager channel:** install/update/uninstall, start/stop, grant/revoke, network-policy mutation, direct-network launch profile, resource policy, and inspection. Managed apps and their embedded UI cannot obtain or upgrade to this principal.

Future console UI assets are package-relative static resources under a distinct app origin/bridge. Apps are not required to run a localhost web server.

## 6. Dependency graph

Plans 345, 349, 352, 353, 354, 355, 368, 369, 370, 371, 382, and 383 are closed. Plan 407 is registered and ready. The managed-app drafts formerly numbered 385–388 are archived because those global numbers belong to Proposal 170 on main; later milestones receive unique global numbers when their implementation plans are written.

```text
345 architecture + runtime-neutral app contract foundation (closed)
  -> 349 v1 direction/reply + broker reservation + network-policy corrective (closed)
       -> 352 mapped-IPv6 policy canonicalization corrective (closed)
            -> 354 listener-independent SAM/I2CP private connection seams (closed)
                 -> 355 router app-principal gateway (closed)
                      -> 368 trusted AppManager protocol + daemon bridge (closed)
                           -> 370 hello instance-id codec corrective (closed; lifted 369's WP4 gate)
                                -> 369 i2pr-appd + i2pr-apphost lifecycle foundation (passed)
                                     -> 371 optional non-blocking startup substrate corrective (closed; lifted 369's invariant-1/§5 blocker)
                                     -> 382 signed immutable package + local store foundation (closed)
                                          -> 383 persistent trust/grants + production catalog + offline admin (closed)
                                               -> 407 Linux Secured apphost sandbox + private app data (ready)
                                               -> future host-owned local-service ingress
                                               -> future external Rust app SDK + package builder
                                               -> live AppManager administrator API (future plan)
                      -> scoped Proposal 170 adapter after its stable contract is ready
                                -> brokered clearnet policy/DNS/TCP
                                     -> SDK + embedded-UI host contract implementation
                                          -> adversarial cross-platform qualification

353 branch integration/planning-authority reconciliation (closed; integration hygiene, not a runtime capability)
```

Plan 352 closed the mapped-address policy gap and Plan 353 completed the integration-hygiene gate. Plan 354 passed the listener-independent SAM/I2CP connection seams with managed-profile host-target denial. Plan 355 passed the principal/capability boundary over those seams without choosing package/process IPC. Plan 368 froze a private manager protocol and mapped that protocol into the existing gateway without process launch. **Plan 369 then added the trusted manager/apphost process roles and the fixture lifecycle, and has passed**; Plans 382–383 added signed packages and persistent explicit policy, while Secured launch still fails closed until a later qualified OS backend exists.

The scoped Proposal 170 adapter remains separately sequenced behind its canonical control contract. Plan 407 owns Linux sandboxing and private app data. Host-owned local ingress and the external Rust SDK remain future work and require newly numbered plans when registered. Brokered clearnet, live admin, UI hosting, and remote update remain future owners.

## 7. Milestones

| Plan | State | i2pr token | Classification | Implementation | Closure |
|---|---|---|---|---|---|
| 345 | closed | `passed-managed-native-app-runtime-contract-foundation` | invariant + infrastructure | `plans/implementation/managed-native-app-runtime/345-native-app-runtime-foundation-and-capability-contract.md` | `plans/closure/managed-native-app-runtime/345-status.md` |
| 349 | closed | `passed-managed-app-v1-direction-broker-network-policy-corrective` | corrective invariant + infrastructure | `plans/implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md` | `plans/closure/managed-native-app-runtime/349-status.md` |
| 352 | closed | `passed-managed-app-mapped-ipv6-policy-canonicalization` | corrective invariant + infrastructure | `plans/implementation/managed-native-app-runtime/352-managed-app-mapped-ipv6-policy-canonicalization-corrective.md` | `plans/closure/managed-native-app-runtime/352-status.md` |
| 353 | closed | `passed-plan349-branch-integration-and-planning-authority-reconciliation` | corrective invariant + polish/integration | `plans/implementation/managed-native-app-runtime/353-plan349-branch-integration-and-planning-authority-reconciliation.md` | `plans/closure/managed-native-app-runtime/353-status.md` |
| 354 | closed | `passed-managed-app-private-client-transport-seams` | infrastructure + invariant | `plans/implementation/managed-native-app-runtime/354-listener-independent-sam-i2cp-private-connection-seams.md` | `plans/closure/managed-native-app-runtime/354-status.md` |
| 355 | closed | `passed-managed-app-principal-gateway-private-client-seams` | infrastructure + invariant + bounded capability plumbing | `plans/implementation/managed-native-app-runtime/355-router-app-principal-gateway-over-private-client-seams.md` | `plans/closure/managed-native-app-runtime/355-status.md` |
| 368 | closed | `passed-trusted-appmanager-bridge-and-manager-protocol-foundation` | invariant + infrastructure + capability boundary | `plans/implementation/managed-native-app-runtime/368-trusted-appmanager-bridge-and-manager-protocol-foundation.md` | `plans/closure/managed-native-app-runtime/368-status.md` |
| 369 | closed | `passed-trusted-application-runtime-manager-and-apphost-lifecycle-foundation` | infrastructure + process lifecycle + capability plumbing | `plans/implementation/managed-native-app-runtime/369-trusted-application-runtime-manager-and-apphost-lifecycle-foundation.md` | `plans/closure/managed-native-app-runtime/369-status.md` |
| 371 | closed | `passed-optional-non-blocking-service-startup-corrective` | runtime supervision substrate corrective | `plans/implementation/managed-native-app-runtime/371-optional-non-blocking-service-startup-corrective.md` | `plans/closure/managed-native-app-runtime/371-status.md` |
| 370 | closed | `passed-managed-app-v1-hello-instance-id-codec-corrective` | corrective invariant + infrastructure | `plans/implementation/managed-native-app-runtime/370-managed-app-v1-hello-instance-id-codec-corrective.md` | `plans/closure/managed-native-app-runtime/370-status.md` |
| 382 | closed | `passed-managed-app-signed-package-store-foundation` | invariant + infrastructure | `plans/implementation/managed-native-app-runtime/382-signed-immutable-managed-app-package-and-local-store-foundation.md` | `plans/closure/managed-native-app-runtime/382-status.md` |
| 383 | closed | `passed-managed-app-persistent-policy-production-catalog-and-offline-administration` | invariant + capability + persistence/lifecycle | `plans/implementation/managed-native-app-runtime/383-persistent-managed-app-policy-production-catalog-and-offline-administration.md` | `plans/closure/managed-native-app-runtime/383-status.md` |
| 407 | active | `in-progress-linux-secured-apphost-sandbox` | invariant + capability + platform security | `plans/implementation/managed-native-app-runtime/407-linux-secured-apphost-sandbox.md` | — |





Plans 354 and 355 are closed, so the router gateway boundary is concrete. Plan 368 closed the daemon↔manager protocol/bridge with **no production caller**, which is what Plan 369 supplied and is now closed: the supervised i2pr-appd/i2pr-apphost lifecycle consumer over the inherited anonymous transport. One Plan 368 limitation was carried into Plan 369's scope and resolved: the protocol's 128-streams-per-session ceiling exceeds what the runtime child-task ceiling admits, so admission fails closed rather than reaching the protocol number.

**Plans 382 and 383 now own the package/lifecycle successor.** Plan 382 deliberately stops at cryptographic package identity plus an immutable local store: a valid signature proves only possession of one publisher key and creates no trust, grant, selection, or launch authority. Plan 383 owns restart-safe local administrator policy, exact package selection, the offline `i2pr-appctl` boundary, and the production `LaunchCatalog`. Live administrator IPC remains separately deferred so persistence and authentication/race semantics do not land together. The later WP4 narrative below is historical: its statement that production used `EmptyCatalog` describes the Plan-369 closure state; Plan 383 supersedes that catalog.

**Plan 369 WP2 landed the first two processes, and proved the supervisor cannot yet degrade an optional one.** WP2 added the `i2pr-appd` workspace crate and `crates/i2pr-daemon/src/app_runtime.rs`. The manager executable is resolved as a **sibling of the daemon's own binary** — never from configuration, never through `PATH`, never through a shell — and the `[app_runtime]` block carries `enabled` only, with `deny_unknown_fields` so a `manager_path` key is a hard error rather than a silently ignored setting. The daemon creates two anonymous pipes, `env_clear`s the child, and execs it directly; possession of the inherited pipe ends is the entire authentication fact. Readiness is signalled **only after** the manager handshake completes, which required extending `AppManagerBridge::run` to publish the handshake outcome rather than letting a service infer it from a timer. WP2's manager **refuses every request** with a typed `UnsupportedOperation`, because Plan 369 has no package or grant owner and therefore no way to hold launch authority; that refusal is the load-bearing security property, not a stub.

Building it hit the Stop condition Plan 369 itself predicted. `Supervisor::run` awaits initial readiness for every service in `startup_order()` and returns `StartupFailed` for any that never signals, **regardless of classification**; the `RestartExhaustion::Degrade` branch is reachable only after startup succeeds. So a manager that is spawned and then rejected takes the whole router down, contradicting invariant 1 and §5. WP2 mitigated rather than hid this: the composition root now **preflights** the sibling and returns an actionable configuration error when it cannot be resolved, and `a_manager_that_sends_the_wrong_magic_never_becomes_ready` asserted the current behaviour with a doc comment naming the gap and requiring replacement once Plan 371 lands. **Plan 371 has now passed, so that assertion has been replaced** — see §"Plan 371" below — and the preflight's stated justification is no longer true; it is retained on the stronger ground that an operator who enabled a feature without installing its manager should get an actionable error rather than a silently degraded router. Two production defects were also found and fixed in WP2 itself: a readiness deadlock where the pinned bridge future was never polled while awaiting the handshake channel, and an unbounded stderr drain whose lifetime depended on EOF even though Plan 369 §12 explicitly disclaims grandchild containment.

**Plan 371 is the generic substrate corrective WP2 proved was missing, and it has passed.** `ServiceClassification` describes how a service behaves *once the router is running*; Plan 371 adds a deliberately orthogonal `StartupRequirement::{Required, Optional}` for whether the router **needs it ready before startup completes**. `Required` is the `#[default]`, so no existing service changed behaviour; `Optional` is the only supported way for a broken subsystem to degrade its own feature instead of aborting startup, and graph validation refuses the contradictory `Optional` + `Essential` combination rather than resolving it silently. Two policies now make a startup failure non-fatal and both are decided at registration time: the explicit `Optional` opt-in, and `RestartExhaustion::Degrade` on a `Restartable` service, which is honoured **during startup as well as after** — before, exhaustion meant one thing during startup and another afterwards, so a service that broke in its first seconds killed a router that would have survived the identical failure a minute later. Two semantics turned out to be load-bearing and are now pinned rather than assumed. First, **a dependency edge constrains start order, not availability**: without this, `Optional` would only work for leaf services and every real optional subsystem — which always has dependants — would still take the router down. Second, **degrading a service and releasing router readiness are separate decisions**: a `Restartable` service that degraded still gates `SupervisorSnapshot::ready` unless it *also* opts into `Optional`, which is exactly why Plan 369's `app-runtime` service carries both. **Six further defects were found while implementing it and fixed**, and two of them were introduced *by* the change rather than merely inherited — which is the point of running the mutation suite. The inherited ones: dependents of a degraded service still failed with `DependencyUnavailable`, so `Optional` would only ever have worked for leaf services; the degradation was overwritten by a transient `Failed`; and `restart_count` was reset to `0`. The self-inflicted ones: collapsing the peer-observation arm into a return value let an exiting peer clear **another** service's readiness gate without that service ever signalling one — the Plan 360 defect class, reintroduced through the Plan 371 path and caught by `clippy::never_loop` and a regression test; and a startup degradation arriving late in the `JoinSet` was re-processed by the steady-state handler, whose `mark_dependents_degraded` then cancelled dependents that startup had deliberately allowed to run. That second one surfaced only as a 1-in-6 flake until a test was written that *forces* the ordering rather than racing it. Nine mutations were negative-tested, two against **real production composition** — flipping the real `lifecycle` service to `Optional` is refused by the graph, and dropping the real `app-runtime` opt-in makes the wrong-magic test fail because the router aborts startup again. Three coverage gaps the negative testing exposed are recorded in the closure record rather than papered over.

**WP3 landed the apphost half of the process boundary.** `crates/i2pr-apphost` is a new trust-zone crate that depends only on the managed-app contracts and on `tokio`/`tokio-util`. It accepts exactly one bounded bootstrap request over the inherited anonymous transport, refuses `Secured` before any filesystem or process work, execs the application **directly** with no shell and no `PATH` lookup, and then becomes a byte-transparent relay until one side closes — at which point the direct child is closed, given a grace, killed if it does not exit, and always reaped. Containment is checked **twice**: `LaunchRequest::validate` rejects `..`, `.`, absolute, and backslash forms structurally, and `resolve_command` then canonicalises both paths and re-checks containment on the resolved result, because a symlink inside the root can otherwise point anywhere. The bootstrap framing constants were **moved into the contract crate** rather than defined in both binaries — two independently chosen constants is exactly how a handshake ends up working until it does not. On the manager side `i2pr-appd` gained `apphost_launch`, which resolves the sibling `i2pr-apphost` (never configuration, never `PATH`, never a shell), spawns it over two anonymous pipes, writes the framed bootstrap, and returns a transport only on `Ready`. It drops the transport halves *before* waiting for exit, because EOF on those pipes is the apphost's only shutdown signal and holding them open until after the wait would make every cooperative teardown burn the whole grace before being killed. `i2pr-apphost` was added to the dependency-direction allowlist **and** to the manager-boundary checker's exact protocol-consumer set; the latter is what caught it. Note that `apphost_launch` has no production caller yet, because WP2 made `i2pr-appd` refuse every request for want of a package or grant owner. That is a recorded intermediate state, not a silent layer: the function is public, documented, and exercised against real child processes. WP5 closes the loop. Three gates were negative-tested — the host's independent `Secured` re-check, the canonical containment re-check, and the forced kill — and each was shown to fail a named test when removed.

**WP4 landed the app v1 consumer, and it found a defect in WP2 that nothing else could have found.** The manager protocol has two disjoint control vocabularies, one per direction, and **WP2 read the wire backwards**: its manager loop decoded inbound frames with `decode_manager_to_daemon_control`, and its test peer also sent manager-direction frames. The two mistakes cancelled, so the suite asserted the inverted contract and passed. Because the vocabularies share no tags, the failure is not a mis-parse but a hard `InvalidControl` — so against the real Plan-368 bridge the first reply would have ended the transport, and no amount of unit testing could have surfaced it. This is the general lesson worth carrying: **a test that shares the implementation's direction assumption cannot detect a direction bug.** WP4 decodes inbound frames as daemon-to-manager, drives the peer in the daemon's direction, and additionally fails closed on any *correlated* reply for a request the manager never sent, which is what a confused or hostile daemon would send.

WP4 then built the pieces WP2 deliberately left absent. `authority` holds `LaunchAuthority` and `AuthorityRequest` behind private fields with **no decoder**, and derives effective capabilities only through the administrator grant path — so `BrokeredTcp` is ungrantable, `EffectiveCapabilities` enforces the ceiling, and there is no `&mut` path to the capability set for a permission request to change. That seal is asserted by **method resolution**, not by scanning for `#[derive(Deserialize)]`, so a future derive makes the crate fail to *compile*; writing it took two attempts and **both earlier attempts were vacuous** — an associated const resolves to the trait fallback rather than the inherent impl, and behind a generic helper the receiver is unresolved so rustc must pick the candidate valid for every `T`. Both versions compiled, passed, and proved nothing, and only the positive control caught them. `catalog` is the trusted source of authority and the shipped `i2pr-appd` owns `EmptyCatalog`, so it launches nothing; there is no manager-receivable launch request in the protocol, so the daemon cannot ask either. `manager_link` is the concurrent manager client — one reader, one writer, a single-writer queue, a bounded in-flight ledger — and the reader performs route registration *before* delivering a reply, closing the window in which a notification could be dropped between "reply delivered" and "caller registered its route". `runtime` is the bounded instance registry and launch pipeline, using `MAX_MANAGER_SESSIONS` as the global ceiling rather than a third independently chosen number. `session` enforces greeting, hello-first/exactly-once with an **exact** app-id/instance-id/major/minor match, presents immutable capabilities, denies permission requests deterministically, applies `SessionLimits`, maps SAM/I2CP onto daemon handles, forwards exact octets both ways, maps backend close/reset, and tears every backend stream down on EOF. **15 mutations were negative-tested**; M15 is the one that breaks the build.

Two harness-level findings are recorded rather than buried. The negative-evidence script originally restored mutated files with `shutil.copy2`, which **preserves mtime**; cargo fingerprints on mtime, so the run after a mutation tested the mutated build. It surfaced as two session tests failing with exactly one mutation's signature; re-applying that mutation reproduced them exactly and a fresh-mtime restore turned them green, which is what makes the diagnosis evidence rather than a guess. Separately, the apphost-launcher tests hit a real `ETXTBSY` about one spawn in fifty when several tests spawn concurrently; it does not occur serially, does not occur spawning a pre-existing binary, does not occur from the same pattern in another language, survives `fsync` and rename-into-place, and no process holds the file open when it fires — so the fixture now tolerates that one errno with a bounded retry, deliberately **not** in `launch_apphost`, because in production the apphost is installed rather than written-then-exec'd.

**Plan 370 was a corrective on the closed Plan 345 contract, registered because Plan 369 became the first runtime consumer of the managed-app v1 codec and exposed a defect no earlier verification could observe — and it has now passed.** `AppToHostMessage::Hello` carried `AppInstanceId` — a `u128` — inside a `#[serde(tag = "type")]` internally tagged enum. Serde deserialises such an enum by buffering the payload into `serde::__private::de::Content`, whose deserializer has no `visit_u128`, so a well-formed hello encoded and then failed to decode for **every** value including `instance_id = 1`, surfacing as a typed `InvalidControl` indistinguishable from corrupt JSON. Measured blast radius was exactly one message: every other variant of all four tagged enums decoded, as did the `u128`-bearing *structs* `AppPrincipal` and `PrincipalOwnedResource`, because structs are not buffered. Plan 345 could not catch it: it closed with no consumer, and its round-trip test exercises `Close`, not `Hello`. The corrective invented no representation — it propagated the **already-ratified Plan 368 D1 decision** (bounded canonical decimal-digit instance ids, `ManagerInstanceId`) to the older contract that decision was never applied to, and made `ManagerInstanceId` delegate to the same parser so one grammar serves both protocols. `hello` now decodes at every valid instance id and fails closed for every non-canonical spelling. Recurrence prevention is a **compile-exhaustive** `match` round-trip over all four enums, not a `scripts/` checker: an approximation that reads like enforcement is the exact defect class Plans 360–366 exist to close. The guard was negative-tested by injecting a variant and observing the build fail. **Plan 370's gate on Plan 369's WP4 is lifted; every work package of Plan 369 may proceed.**

**WP5 closed the loop, and the first run of the whole chain found a defect that four work packages of unit testing structurally could not.** `crates/i2pr-app-fixture` supplies the native fixture application and a fixture **manager**; the manager is a separate binary because the shipped `i2pr-appd` refuses all arguments, so a flag would have reopened the very user-configurable-program hole the refusal protects. It runs the **real** `Appd::with_catalog` over the **real** `inherited()` transport and gates every authority through the real `LaunchAuthority::new`, so what is qualified is the product manager rather than a stand-in shaped to the tests.

The defect: `manager_link::writer_task` wrote each frame with `write_all` and **never flushed**. Every WP2–WP4 test drives a `tokio::io::duplex` transport, which is unbuffered and therefore always "flushed"; the real transport's daemon end is `tokio::io::Stdout`, which is *line-buffered*, and a length-prefixed control frame contains no newline. Every manager→daemon frame therefore sat in the buffer until process exit, **every `CreateSession` timed out, and no launch could ever have succeeded**. The handshake escaped only because `Appd::write_handshake` flushes on its own. This is the same lesson as WP4's direction bug in a different costume: **a test that shares the implementation's transport assumption cannot detect a transport bug.** The regression asserts the flush directly with a counter rather than inferring it from a timeout, so it fails the moment the flush is removed.

`crates/i2pr-daemon/src/app_runtime_qualification.rs` now drives **18 black-box tests** over the whole chain. The Plan-369 cases cover SAM and I2CP happy paths, denied capability/service, wrong-identity hello, frame-before-hello, oversized/malformed frames, early close, shutdown hang, stderr flood, data-before-open, foreign/duplicate stream ids, two-instance isolation, duplicate SAM session ids, and the production manager's fixture-source boundary. Plan 383 adds a production-catalog case: two signed installed apps receive explicit autostart policy and reach private SAM/I2CP twice across an appd restart with fresh instance ids, while a tampered selected sibling is refused without blocking them. The harness records two hazards: stale sibling binaries are rejected by `assert_fresh`, and teardown kills/reaps the direct child after the bridge closes because EOF is the manager's only shutdown signal.

The stale-sibling finding also exposed a **real gap in the routine floor**: `cargo check`, `cargo test --all-targets` and `cargo clippy` all emit only test harnesses under `target/debug/deps` and never the plain `target/debug/<name>` binaries the qualification execs. The floor and CI now build them explicitly. WP5 also added `scripts/check-managed-app-process-boundary.py`, which polices **which process execs what** — something the crate-edge scripts cannot see — and its own controls found two guards that were wrong in the permissive direction: rule 3 asserted the token `args_os` rather than the behaviour, so a manager that read argv and carried on passed; and the negative controls replaced whole files, silently deleting a production `current_exe()` lookup so an unrelated rule fired. A third finding is a coverage hole outside WP5's own code: removing the `i2pr-app-fixture` entry from `check-dependency-direction.sh` left it printing `dependency direction: ok`, because its loop iterates the **map** rather than the workspace. `check-console-boundaries.sh` rule 7 caught the same set, but the check belongs in the script that owns the map, and it now fails closed there. **12 mutations, all caught.**
 Package trust/persistence and the administrator API stay unnumbered until this execution owner is concrete. Later capability classes remain sequenced behind their owners:
- scoped Proposal-170 adapter after the canonical control contract is ready;
- package trust, publisher verification, restart-safe local lifecycle, and administrator API;
- Linux/macOS/Windows sandbox backends plus launch attestation;
- brokered DNS/TCP clearnet egress and user-owned firewall policy;
- Rust SDK plus language-neutral protocol documentation and embedded UI bridge;
- adversarial multi-OS qualification and final security/operational closure.

## 8. Cross-cutting requirements

- Secured application execution is fail-closed. Missing required sandbox properties may not silently degrade to an ordinary child process.
- Default direct networking is **none**, including ordinary loopback. I2P connectivity is an app capability, not native socket permission.
- App-requested permissions are not grants. Only the administrative owner may grant/revoke permissions or switch launch profiles.
- Direct-host-network mode, if later supported, is a distinct operator-selected launch profile requiring process restart; it is not dynamically granted by the application.
- Brokered network rules are default-deny and distinguish public, loopback, private/LAN, link-local, multicast, and unspecified destinations. Broker-owned DNS must re-check resolved address scope so hostname rules cannot bypass scope policy by resolving to local/private addresses.
- No app receives the console/router administrator Proposal-170 token. Future control access is scoped to the app principal and app-owned resources.
- App/package publisher identity is independent from router identity and I2P destination identity.
- Every frame, stream, manifest field, requested capability set, UI message, policy rule, resource request, queue, retry, and diagnostic collection is bounded.
- UI integration must not create a second egress path. Package UI descriptors name package-relative resources, not remote URLs or localhost HTTP servers.
- First-party applications use the same sandbox/capability model as third-party applications unless a separately named operator override is selected.
- An app-runtime failure cannot become a router-core availability dependency when the feature is disabled.
- No future milestone may weaken the current dependency/runtime boundaries merely to make application hosting convenient.

## 9. Verification strategy

Plan 345 is contract/invariant evidence only:
- exact protocol/manifest golden fixtures;
- malformed/truncated/oversized/unknown-field/max+1 decoding;
- capability and principal separation tests;
- exhaustive default-deny network-policy decisions, including IPv4/IPv6 loopback/private/link-local/multicast/unspecified classes;
- hostname-rule post-resolution scope checks at the pure policy layer;
- proof that app-channel message vocabulary contains no grant/revoke/firewall-disable/direct-network mutation operation;
- UI descriptor tests rejecting URLs, absolute paths, traversal, and non-package resources;
- sandbox-attestation requirement tests;
- dependency/runtime static guards showing the new contract crate owns no sockets, process launching, filesystem I/O, timers, or Tokio;
- routine workspace floor.

Later runtime milestones must add hostile fixture applications that attempt direct IPv4/IPv6, DNS, loopback, LAN, local IPC, process escape, filesystem escape, child-process escape, and resource exhaustion while proving permitted I2P/broker operations still work.

## 10. Risks and decision points

Primary risks:
- putting OS sandbox/process logic into `i2pr-runtime` and eroding the router/user-space boundary;
- confusing requested permissions with effective grants;
- handing applications a general I2PControl credential;
- treating loopback as safe;
- introducing a localhost UI/backend server that becomes an escape path;
- allowing package signatures to imply privilege elevation;
- making first-party apps privileged special cases and leaving the third-party path untested;
- designing an API around a Rust SDK rather than a documented language-neutral protocol;
- overclaiming "malicious-app anonymity": network containment blocks direct unauthorized egress but cannot stop an app from intentionally encoding identifying data into traffic it is legitimately allowed to send.

OS mechanisms differ materially, so the common contract must specify security properties rather than force one mechanism. Linux user namespaces cannot be assumed available on every host; macOS sandbox/code-signing constraints require dedicated qualification; Windows AppContainer/job-object behavior requires its own qualification. These belong to later backend plans.

## 11. Completion definition

The managed-native-app-runtime workstream is complete only when:
- a versioned, language-neutral app protocol and SDK exist;
- package identity/manifests and publisher verification are bounded and restart-safe;
- applications run as separate supervised processes under qualified OS security boundaries;
- the secured profile proves no direct public/LAN/loopback network access;
- SAM/I2CP use the existing router protocol owners through an app principal;
- Proposal 170 access uses the completed canonical owner and is authorization-scoped;
- user-approved clearnet traffic is brokered under default-deny rules;
- application policy cannot be raised from the app or embedded UI principal;
- console UI resources run under an isolated app origin/bridge with no independent egress;
- resource/process-tree limits and cleanup are proven;
- restart/update/rollback preserve package and permission state correctly;
- adversarial fixtures pass on every supported OS backend;
- documentation states exact security guarantees and explicit non-guarantees.

This completion boundary is independent of whether any mail/IRC/torrent application has been implemented.

## 12. Milestone status summary

Plan 345 remains closed as `passed-managed-native-app-runtime-contract-foundation`: ADR 0032 and the initial v1 contract landed with `i2pr-app-proto`, identity/capability separation, framing/control vocabulary, manifest/UI descriptors, pure network policy, resource limits, sandbox-attestation vocabulary, fuzzing, and static boundary enforcement. It does not launch processes or establish sandbox/network containment.

Plan 349 corrected the four pre-runtime API defects without invalidating Plan 345's executed infrastructure evidence: hostname grants now handle globally routable results with explicit-deny protection, non-global address classification is frozen and fail-closed, control messages have directional correlated outcomes, and `brokered_tcp` is reserved until a connect transaction is designed. Its closure remains authoritative for those corrections.

Plan 352 passed as `passed-managed-app-mapped-ipv6-policy-canonicalization`: mapped targets canonicalize to IPv4 before exact/CIDR matching, and mapped selectors fail validation. Plan 353 passed as `passed-plan349-branch-integration-and-planning-authority-reconciliation`: tracked build output was removed, the exact Plan-349 collision authorities were reconciled, the portable ADR was renumbered to 0033, and the branch was verified on current main.

Plan 354 passed: both loopback listeners and trusted private connections use one listener-independent protocol driver, and managed-app SAM denies `STREAM FORWARD`/host-target behavior. Plan 355 passed: trusted composition binds one `AppPrincipal` and immutable `EffectiveCapabilities` to isolated private SAM/I2CP contexts. Neither plan launches applications, implements package lifecycle, establishes sandbox containment, or promotes SAM/I2CP support. Proposal 170 integration remains gated on canonical Proposal 170 completion.

Future app implementation must preserve the existing router ownership boundaries. Plans 368–371 and 382–383 are closed. Plan 382 created no trust/grants/launch authority; Plan 383 added persistent local authority but did not claim Secured containment or add a live administrator endpoint. Plan 407 is registered ready for Linux Secured containment and private app data. Former managed-app drafts 385–388 are archived because those global numbers belong to Proposal 170; local ingress and the SDK need newly numbered plans before implementation. Brokered clearnet, UI, remote repository/update, live admin, and scoped control remain downstream and separately qualified.
