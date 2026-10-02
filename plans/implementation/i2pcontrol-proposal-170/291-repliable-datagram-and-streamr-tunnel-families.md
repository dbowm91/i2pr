# Plan 291 — Repliable-datagram substrate and Streamr tunnel families

Status: registered-prop170-streamr-blocked-on-plan289

Classification: infrastructure + capability.

Hard dependency: Plan 289 closed. May execute in parallel with Plan 290.

## Objective

Implement the bounded authenticated/repliable-datagram application substrate required by Proposal 170 streamrclient and streamrserver, then expose both as real TunnelManager backends.

i2pr currently has no datagram application substrate; treating Streamr as a streaming alias would be incorrect.

## Protocol/source freeze

Before code, pin the exact I2P repliable-datagram framing/identity semantics used by current Java I2PTunnel Streamr and the project-owned Emissary Prop 170 implementation. Record:
- sender authentication/identity representation;
- destination/port metadata;
- maximum protocol payload and fragment behavior;
- reply semantics;
- malformed/unverifiable input behavior;
- whether any required behavior is SAM-specific versus router-internal datagram behavior.

Do not copy GPL Java implementation code. Java is a behavioral/specification reference only. Manifest-authorized Emissary Prop 170 code may be reused where it is independent of Yosemite.

## i2pr-client substrate

Add a runtime-neutral repliable-datagram surface under the existing destination ownership model:
- typed send request including destination and I2P port metadata required by the protocol;
- authenticated sender identity encoding/verification;
- bounded receive event carrying only public authenticated sender information and payload;
- hard payload/count/queue ceilings;
- no unbounded fragmentation/reassembly;
- destination-runtime registration/delivery path;
- explicit malformed/signature/unknown-destination/backpressure outcomes.

Secrets remain in DestinationIdentity/owned capabilities. The datagram layer does not gain a second destination identity store.

If raw datagrams are not required for Streamr, do not broaden this plan to raw-datagram support.

## Daemon/runtime composition

Daemon owns any UDP socket used on the local side of Streamr and all tasks/timers.

streamrserver:
- persistent router-owned I2P destination;
- loopback-only local UDP source/listener;
- bounded subscriber table keyed by authenticated remote destination plus protocol-required port metadata;
- explicit refresh/expiry/unsubscribe behavior;
- bounded fanout;
- hard payload ceiling;
- no peer-controlled task spawning.

streamrclient:
- configured remote producer destination;
- loopback-only local UDP target;
- bounded subscribe/refresh schedule with deterministic shutdown/unsubscribe;
- received repliable datagrams forwarded only to the configured loopback target;
- no DNS or non-loopback target fallback.

Use the Emissary fork's mature policy as a starting point only after reconciling it to the pinned Java behavior and i2pr's resource ceilings. Values such as 15-second refresh, 60-second expiry, ten subscribers, and 1200-byte application payload are candidates because they are already proven in the fork, but the plan closure must state the exact selected bounds and why.

## TunnelManager integration

Add streamrclient and streamrserver to:
- ServiceTunnelKind/backend registry;
- Proposal type mapping;
- exact option applicability;
- generation diff/reconcile;
- snapshots/get output;
- StartOnLoad lifecycle;
- persistent server identity ownership.

Unsupported streamr options fail before UDP bind/destination allocation.

## Evidence

Required:
- fixed repliable-datagram positive vectors and independently generated/decoded vectors;
- malformed/truncated/oversized/invalid-auth vectors;
- destination/port identity binding tests;
- queue/backpressure/expiry deterministic-clock tests;
- local UDP loopback restrictions including non-loopback rejection;
- subscriber cap and expiry;
- repeated subscribe/unsubscribe;
- client refresh/shutdown;
- multi-subscriber fanout with payload digest equality;
- sibling service isolation;
- TunnelManager lifecycle/restart/persistent identity;
- existing streaming/service tests unchanged.

## Acceptance criteria

Plan 291 closes only when repliable datagrams have real authenticated router-internal semantics and both Streamr families move data end-to-end through the actual destination runtime. A UDP forwarding loop over ordinary I2P Streaming is not acceptance.

After Plans 290 and 291 both close, all twelve Proposal tunnel families have real backends and Plan 292 becomes ready.
