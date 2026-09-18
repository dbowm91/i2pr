# Plan 074: Milestone 3 real-driver and constrained-host corrective roadmap

## Status and authority

- Status: planned.
- Plan type: corrective roadmap and active execution-order authority for the next Milestone 3 work.
- Parent authority: Plan 067 and ADR 0023.
- Supersedes Plan 070 as the next executable plan. Plan 070 remains a historical planning artifact describing the intended first i2pd run, but its preconditions are false on the current repository and host.
- Reclassifies the implemented Plan 069 lane as orchestration scaffolding and fake-process control coverage only. It is not valid mixed-router evidence until Plan 075 closes.
- Does not erase Plan 064, Plan 069, their status records, or prior qualification receipts.
- NTCP2 remains experimental, non-advertised, and disabled in normal daemon operation.

## Corrected problem statement

The immediate Milestone 3 blocker is a conjunction of three independent defects:

1. **The Plan 064 i2pd helper is not a real linked i2pd transport driver.** Its current CMake project does not build or link the pinned i2pd library targets. The listen and dial functions terminate through the `pinned-libraries-not-linked` stub path rather than initializing i2pd, importing RouterInfo, opening NTCP2, or sending a DeliveryStatus message.
2. **The Plan 069 runner is not presently a mixed-router runner.** It launches i2pr for both process roles, does not invoke the supplied i2pd binary as the reference process, and marks several protocol milestones without consuming real structured reference events.
3. **The current Ubuntu host cannot provide the previously assumed rootless network namespace or reliable Multipass lane.** A valid execution lane must therefore use an already available privileged container daemon, a no-NIC QEMU guest, a narrowly scoped inherited-socket/seccomp diagnostic, or a manual remote runner.

A rootless-capable host alone would not fix items 1 or 2. The repository must first restore a truthful real-driver and real-runner surface.

## Roadmap objectives

This roadmap must:

- remove all stub or synthetic authority from the active i2pd path;
- build a real source-locked i2pd test executable against pinned i2pd code;
- make the runner launch exactly one i2pr process and one real reference process;
- consume structured events rather than inferring protocol success from a listening socket;
- provide a deterministic constrained-host lane selection procedure;
- prefer existing rootful Docker with `--network none`, then QEMU TCG with `-nic none`;
- retain inherited connected sockets plus seccomp as a lower-scope protocol diagnostic, not full transport-manager qualification;
- retain manual GitHub Actions or another dedicated remote Linux runner as the final practical fallback;
- obtain the first real two-direction i2pr/i2pd result only after driver and lane prequalification;
- preserve exact DeliveryStatus ID and Router Hash continuity;
- avoid recreating the broad Plan 046-066 certificate apparatus for development testing.

## Plan decomposition

### Plan 075: Plan 069 runner integrity and evidence correction

Owns:

- preventing the current runner from producing a pass without real structured events;
- launching the configured reference driver rather than a second i2pr process;
- removing synthetic provenance and automatic protocol milestone promotion;
- validating process-role and event-source identity;
- explicitly classifying all prior Plan 069 execution capability as unqualified scaffolding.

Plan 075 must close before a real reference execution is attempted.

### Plan 076: real pinned i2pd library and direct driver construction

Owns:

- building pinned i2pd through its actual CMake/library targets;
- linking the test-only direct driver to real i2pd code;
- implementing inspect, listen, dial, RouterInfo import/export, DeliveryStatus submission, structured observer events, and bounded shutdown;
- retaining a behavior-neutral uninstrumented control build;
- removing `pinned_libraries_linked()` and terminal stub paths.

Plan 076 has no external interoperability pass requirement. It closes on a real, inspectable, source-locked executable and focused local controls.

### Plan 077: constrained-host execution lane provisioning

Owns:

- a capability probe and deterministic lane-selection record;
- rootful Docker `--network none` lane when an existing daemon is accessible;
- QEMU TCG `-nic none` lane when Docker is unavailable;
- optional inherited-descriptor/seccomp protocol lane with explicit reduced scope;
- manual remote runner fallback when no local full-runtime lane exists;
- a common guest/container entry contract and sanitized result extraction.

Plan 077 must not attempt protocol qualification with the stub driver.

### Plan 078: first real two-way i2pd execution and bounded correction

Owns:

- one real `i2pr-to-i2pd-ipv4` execution;
- one real `i2pd-to-i2pr-ipv4` execution;
- earliest-stage failure classification;
- bounded protocol corrections driven by observed failures;
- behavior-neutral control comparison;
- Level 1 receipts only.

### Plan 079: repeated i2pd development validation and Milestone 3 continuation decision

Owns:

- three fresh-state passes per direction;
- bounded negative controls;
- development validation summary;
- decision whether later implementation work may continue while Java/release qualification remains deferred;
- handoff to Plan 072 only when Emissary differential evidence is useful;
- handoff to Plan 073 only when a suitable Level 3 lane exists.

Plan 079 supersedes Plan 071 as the active repeated-validation plan because Plan 071 assumes Plan 070 closes successfully.

## Dependency graph

```text
Plan 074 roadmap
      |
      v
Plan 075 runner integrity
      |
      v
Plan 076 real i2pd driver
      |
      v
Plan 077 constrained-host lane
      |
      v
Plan 078 first real two-way execution
      |
      v
Plan 079 repeated development validation
      |
      +--> Plan 072 conditional Emissary differential work
      +--> Plan 073 deferred Java/release qualification
```

## Global implementation rules

1. Never represent a port probe, process survival, sender callback, or schema-valid synthetic fixture as NTCP2 authentication or I2NP receipt.
2. Every passing mixed-router record must bind the event source to the real reference binary digest, implementation name, run ID, direction, Router Hash pair, and exact DeliveryStatus message ID.
3. No active config or record may use fabricated source-tree, driver-source, observer-patch, or binary provenance.
4. Build and execution are separate phases. Network access is permitted only in an explicitly documented preparation phase when dependencies are not already cached.
5. Do not patch i2pd cryptography, Noise state, frame encoding, RouterInfo verification, or transport acceptance semantics.
6. Passive observer hooks may emit data only after the corresponding real operation succeeds. They must not alter control flow or return values.
7. Do not require rootless namespaces, Multipass, bubblewrap, rootless Podman, rootless Docker, or user-level `PrivateNetwork` on the known constrained host.
8. Do not install or configure a privileged daemon automatically. Existing rootful Docker access may be used; otherwise select QEMU or remote execution.
9. Keep CI optional and manual. Do not add a per-push or per-PR heavy interoperability matrix.
10. Preserve historical receipts as typed absence. Never edit zero-attempt records into passes.

## Lane priority

Plan 077 must use this order:

```text
1. existing accessible rootful Docker daemon, one container, --network none
2. QEMU system emulation using TCG, one guest, -nic none
3. inherited connected TCP descriptors plus no_new_privs/seccomp for reduced-scope protocol validation
4. manually triggered dedicated remote Linux runner or GitHub Actions job
5. typed no-full-runtime-lane blocker
```

The first available full-runtime lane wins. The inherited-descriptor lane must not be promoted to full transport-manager qualification.

## Roadmap closure criteria

Plan 074 is complete as a planning artifact when:

- Plans 075 through 079 exist;
- Plan 070 and Plan 071 are explicitly superseded for active execution;
- Plan 069 is explicitly reclassified as unqualified scaffolding until corrected;
- the active sequence is registered in a Milestone 3 planning amendment;
- each child plan contains explicit ownership, acceptance criteria, stop rules, non-goals, validation commands, and small-model guidance;
- no child plan assumes rootless namespaces or Multipass;
- no child plan accepts synthetic provenance or inferred protocol milestones;
- Plan 073 remains the final Java/i2pd release-qualification gate.

## Handoff order

Execute one plan at a time:

1. Plan 075;
2. Plan 076;
3. Plan 077;
4. Plan 078;
5. Plan 079.

Do not begin Plan 078 merely because a container or VM lane exists. The real driver and corrected runner must both be closed first.