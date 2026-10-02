# Plan 289 — TunnelManager control state and existing service-runtime adapter

Status: registered-prop170-tunnelmanager-foundation-blocked-on-plan287

Classification: capability + persistence/lifecycle infrastructure.

Hard dependencies: Plan 287 closed; M10 service-tunnel authority remains closed and is reused rather than replaced.

## Objective

Implement the Proposal 170 TunnelManager lifecycle and durable administrative ownership over the existing i2pr ServiceTunnelManager, initially for the service families i2pr already has real runtime primitives for.

This plan is about control ownership, transactionality, exact wire semantics, and reuse of M10. It must not create a second service/destination/tunnel runtime.

## Ownership model

Every tunnel definition has explicit provenance:
- StartupOwned: originated from daemon TOML/static configuration. Inspectable, but I2PControl mutation is rejected unless a future explicit import operation is standardized.
- ControlOwned: originated from TunnelManager and is persisted beneath a dedicated i2pcontrol administrative state root.

Name collision across ownership classes fails closed.

Control state must not rewrite router.toml or ordinary service-tunnel config files.

## Durable state

Use i2pr-storage primitives or a new narrowly scoped generation store with:
- schema/version marker;
- complete-generation serialization;
- deterministic ordering;
- hard file/definition/option/byte ceilings;
- restrictive owner-only permissions where supported;
- temp write + sync + atomic rename + directory sync where supported;
- prior-generation recovery;
- corruption/restart tests;
- symlink/special-file/path escape rejection;
- bounded retention.

Secrets are classified and never returned through response rawConfig or logs.

## Transaction coordinator

Proposal mutations must keep persistent definition and runtime state coherent.

For create/edit/rename/delete:
1. parse and validate the complete candidate definition before any side effect;
2. build a candidate aggregate ServiceTunnelSet;
3. stage any persistence generation without publishing it;
4. ask the existing ServiceTunnelManager to reconcile/stage the candidate under bounded drain policy;
5. publish durable control generation only at the defined commit point;
6. if persistence publication fails after runtime transition, reconcile back to the prior committed generation under a hard deadline;
7. return success only when durable intent and the authoritative runtime generation agree.

Crash points and rollback limitations must be documented and tested. A successful response may not mean merely "accepted into memory".

Start/stop/restart are runtime actions over the exact control-owned definition; StartOnLoad remains persisted intent and is handled separately from current running state.

## Exact actions

Implement the pinned Proposal action vocabulary exactly:
- get;
- create;
- edit;
- delete;
- start;
- stop;
- restart.

If rename is represented as an edit/name transition in the donor implementation, preserve the pinned public wire contract rather than inventing a new action.

Get is the only read action and must report actual runtime state separately from persisted intent.

## Initial family mappings

Map Proposal types onto existing i2pr runtime families where semantics are already real:
- client -> GenericClient;
- server -> GenericServer;
- httpclient -> HttpClient;
- socks -> Socks5Client as an initial backend, while Plan 290 owns any required SOCKS4a/profile parity before full support;
- ircclient -> IrcClient;
- ircserver -> IrcServer.

The mapping layer is typed and exhaustive. A Proposal type without a runtime backend returns explicit unsupported and allocates no listener/destination/task.

Do not claim the remaining six types yet.

## Runtime state

Expose a per-name supervisor snapshot with at least:
- unsupported/stopped/starting/running/stopping/failed classification;
- current committed generation id;
- public server destination where applicable;
- current listener/bind metadata safe for control output;
- active connection counters as allowed by the wire contract.

Persisted enabled/StartOnLoad does not substitute for current runtime state.

## Startup and restart

At daemon startup:
- load and validate the newest durable control generation;
- recover prior valid generation if latest is corrupt;
- detect collisions against StartupOwned names before starting anything;
- start only ControlOwned definitions with StartOnLoad and supported backends;
- isolate failures per definition;
- never rotate a persistent server identity due merely to control-server restart.

I2PControl disabled mode preserves control state on disk but must not read, reconcile, start, or affect it unless the architecture explicitly chooses load-without-activation; the chosen isolation rule must be deterministic and tested.

## Evidence

Required tests:
- every action happy/failure path;
- create/edit/delete one-publication semantics;
- rollback on persistence/runtime staging failure;
- crash/restart at each publication boundary;
- control/startup collision and attempted mutation rejection;
- StartOnLoad vs current state truthfulness;
- persistent server identity across stop/start/restart/daemon restart;
- unsupported type is resource-free;
- secret option redaction in Debug/errors/responses/generation diagnostics;
- cardinality/name/path/option +1 bounds;
- concurrent mutations on the same name serialize deterministically;
- mutations on different names remain bounded and do not bypass aggregate generation consistency.

## Acceptance criteria

Plan 289 closes when the seven lifecycle actions are real over one M10 ServiceTunnelManager for the six existing families, durable state is recoverable, ownership provenance is enforced, and unsupported Proposal families fail before resource allocation.

Plans 290 and 291 become ready in parallel after closure.
