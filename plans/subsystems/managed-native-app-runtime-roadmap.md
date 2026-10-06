# Managed Native Application Runtime Roadmap

Status: parallel — Plans 345, 349, 352, 353, 354, 355, 368, and 370 are closed. Corrective Plan 370 repaired the managed-app v1 `hello` instance-id codec, which encoded and then failed to decode for every value; its gate on Plan 369 is lifted. Plan 369 is active with WP1 landed and adds the separately supervised i2pr-appd/i2pr-apphost lifecycle foundation; **every work package is now unblocked**. Package trust/persistence, qualified OS sandboxing, brokered clearnet, UI hosting, and scoped Proposal 170 remain downstream. This workstream is parallel to router protocol milestones and does not gate M12, anonymity, transport, or current router interoperability work.

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
- The Plan-345 language-neutral wire/manifest contract is recorded in [Managed native application contract v1](../../specs/references/managed-native-app-runtime-v1.md). Plan 349 corrected its direction/reply semantics, broker reservation, and network-policy classification; Plan 352 closed the mapped-IPv6 canonicalization gap. Plan 354 owns the private protocol-transport substrate and Plan 355 owns the router-side principal/capability gateway. Plan 368 now owns the distinct private daemon↔AppManager protocol/bridge; Plan 369 consumes it for process/runtime lifecycle without claiming sandbox containment.

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

Plans 345, 349, 352, 353, 354, 355, 368, and 370 are closed. Plan 369 is active with WP1 landed and no remaining gate. Later capability milestones receive global numbers only when their implementation plans are written.

```text
345 architecture + runtime-neutral app contract foundation (closed)
  -> 349 v1 direction/reply + broker reservation + network-policy corrective (closed)
       -> 352 mapped-IPv6 policy canonicalization corrective (closed)
            -> 354 listener-independent SAM/I2CP private connection seams (closed)
                 -> 355 router app-principal gateway (closed)
                      -> 368 trusted AppManager protocol + daemon bridge (closed)
                           -> 370 hello instance-id codec corrective (closed; lifted 369's WP4 gate)
                                -> 369 i2pr-appd + i2pr-apphost lifecycle foundation (active, WP1 landed, no gate)
                                     -> package trust + restart-safe local lifecycle + AppManager admin owner (future plan)
                                     -> OS sandbox + process-tree/resource containment
                      -> scoped Proposal 170 adapter after its stable contract is ready
                                -> brokered clearnet policy/DNS/TCP
                                     -> SDK + embedded-UI host contract implementation
                                          -> adversarial cross-platform qualification

353 branch integration/planning-authority reconciliation (closed; integration hygiene, not a runtime capability)
```

Plan 352 closed the mapped-address policy gap and Plan 353 completed the integration-hygiene gate. Plan 354 passed the listener-independent SAM/I2CP connection seams with managed-profile host-target denial. Plan 355 passed the principal/capability boundary over those seams without choosing package/process IPC. Plan 368 is the next executable boundary: it freezes a private manager protocol and maps that protocol into the existing gateway without process launch. Plan 369 then adds the trusted manager/apphost process roles and fixture lifecycle; Secured launch must still fail closed until a later qualified OS backend exists.

The scoped Proposal 170 adapter remains separately blocked on canonical Proposal 170 completion; the current registry still has Plan 348 blocked on Plans 342 and 347. OS sandbox, broker, SDK, and UI-host plans remain sequenced behind their runtime/gateway owners as shown above.

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
| 369 | active | `in-progress-managed-app-runtime-manager-foundation-wp1-landed` | infrastructure + process lifecycle + capability plumbing | `plans/implementation/managed-native-app-runtime/369-trusted-application-runtime-manager-and-apphost-lifecycle-foundation.md` | future `plans/closure/managed-native-app-runtime/369-status.md` |
| 370 | closed | `passed-managed-app-v1-hello-instance-id-codec-corrective` | corrective invariant + infrastructure | `plans/implementation/managed-native-app-runtime/370-managed-app-v1-hello-instance-id-codec-corrective.md` | `plans/closure/managed-native-app-runtime/370-status.md` |

Plans 354 and 355 are closed, so the router gateway boundary is concrete. Plan 368 closed the daemon↔manager protocol/bridge with **no production caller**, which is what Plan 369 supplies: the supervised i2pr-appd/i2pr-apphost lifecycle consumer over the inherited anonymous transport. One Plan 368 limitation is carried into Plan 369's scope: the protocol's 128-streams-per-session ceiling exceeds what the runtime child-task ceiling admits, so admission fails closed rather than reaching the protocol number.

**Plan 370 was a corrective on the closed Plan 345 contract, registered because Plan 369 became the first runtime consumer of the managed-app v1 codec and exposed a defect no earlier verification could observe — and it has now passed.** `AppToHostMessage::Hello` carried `AppInstanceId` — a `u128` — inside a `#[serde(tag = "type")]` internally tagged enum. Serde deserialises such an enum by buffering the payload into `serde::__private::de::Content`, whose deserializer has no `visit_u128`, so a well-formed hello encoded and then failed to decode for **every** value including `instance_id = 1`, surfacing as a typed `InvalidControl` indistinguishable from corrupt JSON. Measured blast radius was exactly one message: every other variant of all four tagged enums decoded, as did the `u128`-bearing *structs* `AppPrincipal` and `PrincipalOwnedResource`, because structs are not buffered. Plan 345 could not catch it: it closed with no consumer, and its round-trip test exercises `Close`, not `Hello`. The corrective invented no representation — it propagated the **already-ratified Plan 368 D1 decision** (bounded canonical decimal-digit instance ids, `ManagerInstanceId`) to the older contract that decision was never applied to, and made `ManagerInstanceId` delegate to the same parser so one grammar serves both protocols. `hello` now decodes at every valid instance id and fails closed for every non-canonical spelling. Recurrence prevention is a **compile-exhaustive** `match` round-trip over all four enums, not a `scripts/` checker: an approximation that reads like enforcement is the exact defect class Plans 360–366 exist to close. The guard was negative-tested by injecting a variant and observing the build fail. **Plan 370's gate on Plan 369's WP4 is lifted; every work package of Plan 369 may proceed.**
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

Future app implementation must preserve the existing router ownership boundaries. Plans 368 and 370 are closed, so Plan 369 may proceed through every work package. Plan 368 may not launch processes; Plan 369 may not claim Secured containment or package trust. Package/persistence, OS sandbox, broker, SDK, UI, and scoped control work remain downstream and separately qualified.
