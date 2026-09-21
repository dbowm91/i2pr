# Plan 230 status — M6 Java reachability-capability/profile-bootstrap corrective and continuation

Status: **registered-ready-m6-java-reachability-capability-profile-bootstrap-corrective**.

Plan of record:
[`230-m6-java-profile-population-path-attribution.md`](../../implementation/mixed-router-interop/230-m6-java-profile-population-path-attribution.md).

The filename is retained to preserve the registered global Plan-230 path; the
plan body has been replaced by the narrower reachability/capability corrective.

## Registration basis

Plan 229 closed with:

```text
P229-C-NOT-EXPLORATORY-ELIGIBLE
profile_present=false
profile_count=0
not_failing_count=0
```

on all four counted attempts (SHAs `00dc368` / `10d1015`).

Exact-pinned Java I2P 2.13.0 source review after that closure changed the
interpretation of the stop:

1. `P229Probe` used `ProfileOrganizer.isFailing(C)` as its
   `unreachable` signal, but the exact-pinned method is deprecated and
   unconditionally returns `false`. Historical `unreachable=false` is therefore
   not authoritative reachability evidence.
2. `ProfileManagerImpl.heardAbout(peer, caps)` is the main profile-creation
   vector and creates a new profile only when its private
   `shouldCreate(caps)` predicate accepts the peer's RouterInfo.
3. That predicate requires `R` and, for a non-floodfill peer observed by the
   floodfill service router, rejects the normal low-bandwidth `L` class as well
   as `E`/`G`.
4. Plan 229 observed `f` but did not record `R/L/E/G/U` or reconstruct the
   creation predicate.
5. The controlled router uses Java's default 60 KiB/s outbound bandwidth unless
   explicitly configured; with the default 80% share the pinned
   `Router.getBandwidthClass()` normally places it at the `L` boundary.
6. The isolated loopback fixture also cannot depend on public-network peer
   testing to derive reachability. Pinned `UDPTransport` supports the stock
   `i2np.udp.status=ok` override, and an OK communication-system status causes
   `Router.getCapabilities()` to advertise `R`.

The open question is therefore not "how many minutes until a profile appears?"
but whether the exact RouterInfo is eligible for ordinary stock profile
creation at all.

## Current authority

```text
plan_229 = passed-m6-java-nonzero-exploratory-bootstrap-corrective-with-not-exploratory-eligible-boundary
plan_230 = registered-ready-m6-java-reachability-capability-profile-bootstrap-corrective

plan_201 = blocked-pending-plan230-reachability-capability-profile-bootstrap-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan230
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 230-m6-java-reachability-capability-profile-bootstrap-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## Authorized execution

Plan 230 first adds read-only capability/predicate evidence and performs one
baseline run with the Plan-229 topology unchanged.

Only baseline evidence may authorize these stock controlled-fixture changes:

- `i2np.udp.status=ok` for A/B/C when missing/incorrect reachability capability
  is proven;
- `i2np.bandwidth.outboundKBytesPerSecond=128` and
  `i2np.bandwidth.outboundBurstKBytesPerSecond=128` for transit Router C only
  when C's `L` class is the exact creation-predicate blocker.

`router.forceBandwidthClass` remains forbidden.

After a newly published corrected C RouterInfo traverses the ordinary
authenticated DatabaseStore bootstrap and C enters A's organizer population
naturally, this same plan continues directly through the existing non-zero
exploratory, client-tunnel, and frozen Plan-201 destination qualification gates.
Do not register another Java-bootstrap successor simply to repeat those gates.

## Prohibited shortcuts

Plan 230 does not authorize:

- production Rust changes merely to satisfy the Java fixture;
- Java source patches;
- direct profile creation, tier promotion, score mutation, or profile files;
- probe-side `heardAbout()` calls;
- direct Java NetDB store/publish;
- direct tunnel installation;
- VMComm, public I2P, or reseed;
- `netDb.alwaysQuery`;
- Plan-226 distinct-/24 topology;
- SAM pivot;
- timeout inflation;
- client-NetDB RI injection;
- using historical P229 `isFailing()` evidence as a reachability gate.

Closure must replace this registration token with one exact Plan-230 terminal
or a Java second-family pass and update the Plan-201/204 unblock audit.
