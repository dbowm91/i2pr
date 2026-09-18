# Plan 041: Reference-router private crosscheck

## Objective

Prove that the pinned Java I2P and i2pd builds, configuration templates, RouterInfo exchange paths, namespace isolation, and readiness/cleanup observations are capable of producing a real authenticated NTCP2 connection without involving i2pr.

This plan separates harness defects from i2pr protocol defects. A reference-only crosscheck must pass before the i2pr wire driver is used as the variable under test.

The crosscheck remains a private, synthetic, authorized test environment. It must not reseed, bootstrap, publish to the public NetDB, contact DNS, enable SSU2, enable UPnP/NAT traversal, accept transit tunnels, or expose management/client services.

## Prerequisites

- Plan 040 is complete on Ubuntu 24.04 amd64.
- Both pinned reference runtime caches have validated metadata and tree hashes.
- Offline cache reuse has passed.
- Environment smoke has produced and then removed disposable RouterInfo state for each reference.
- No known namespace, nftables, path, readiness, evidence, or cleanup defect remains.

## Required result

The harness must start one Java I2P router and one i2pd router in separate namespaces connected only by a private veth pair. Both routers must be configured for the same explicitly non-public test network, must exchange validated RouterInfo through implementation-specific import mechanisms, and must establish an authenticated NTCP2 connection.

The test must distinguish an authenticated NTCP2 session from:

- process readiness;
- listener readiness;
- TCP connection establishment;
- repeated failed dial attempts;
- RouterInfo file production;
- log text that merely contains the string `NTCP2`.

## Deliverable 1: Reference-pair scenario model

Add a dedicated scenario schema for reference-only crosschecks rather than aliasing i2pr scenarios.

A reference-pair scenario must include:

- scenario ID;
- schema version;
- Java and i2pd canonical reference IDs;
- full source revisions;
- private network-ID policy;
- address family;
- Java and i2pd local addresses;
- Java and i2pd NTCP2 ports;
- deterministic startup order;
- RouterInfo exchange order;
- selected dial initiator or deterministic simultaneous-connect policy;
- handshake deadline;
- observation method;
- expected authenticated-link count;
- cleanup policy.

Create at least:

```text
reference-java-i2pd-ipv4
reference-i2pd-java-ipv4
```

If the reference implementations deterministically choose their own connection direction after RouterInfo import, the two scenarios may share one physical policy only if evidence can prove both implementations can accept and initiate across separate runs. Do not fabricate directionality based solely on which process was started first.

## Deliverable 2: Dedicated reference-pair topology

Add a topology owner separate from the i2pr/reference topology. Suggested ownership:

```text
tests/integration/ntcp2/harness/reference_topology.py
```

The topology must create:

```text
java-<short-run-id>       i2pd-<short-run-id>
  lo                        lo
  peer0                     peer0
  192.0.2.1/30   <------>   192.0.2.2/30
```

Use distinct scenario subnets if concurrent crosschecks are ever allowed. Initially, serialize privileged interop runs and enforce workflow concurrency to reduce cleanup ambiguity.

Apply namespace-local nftables rules that permit only:

- loopback;
- established/related traffic;
- Java-to-i2pd TCP on the exact i2pd destination port;
- i2pd-to-Java TCP on the exact Java destination port;
- optional exact-peer ICMP probes used only by isolation validation.

Verify no host-side endpoint, default route, forwarding path, or public route exists. Hash the canonical topology and firewall description into evidence.

## Deliverable 3: Establish the private network-ID contract

Inspect the pinned Java I2P and i2pd source revisions to determine the exact supported configuration mechanism for selecting a non-public network ID.

Requirements:

- both routers must use the same explicit test network ID;
- the value must be distinct from the public I2P network;
- the exact configuration keys and source locations must be documented adjacent to the templates;
- adapters must assert the rendered value before launch;
- the runner must reject a crosscheck if either implementation cannot prove the configured network ID;
- no implementation may silently fall back to the public network ID.

Do not guess an i2pd configuration key. Verify it from the pinned revision. If the pinned i2pd revision cannot be configured for the required private network ID, stop and record the blocker. Do not compensate by weakening Java validation or using public-network semantics.

## Deliverable 4: Harden reference runtime configuration

### Java I2P

Render a disposable configuration that:

- binds NTCP2 only to the Java namespace address;
- publishes only the synthetic Java address and selected port;
- uses the private network ID;
- enables local/synthetic addresses only as needed for the test;
- disables reseed, updates, SSU2/UDP, UPnP, floodfill, transit tunnels, client applications, console, and public discovery;
- places all mutable state under the run root;
- runs in foreground or under a process model whose child tree can be fully supervised.

### i2pd

Render a disposable configuration that:

- binds NTCP2 only to the i2pd namespace address;
- publishes only the synthetic i2pd address and selected port;
- uses the same private network ID;
- disables reseed, SSU2, UPnP, floodfill, transit tunnels, proxies, SAM, I2CP, HTTP, control, tunnels, and public discovery;
- places all mutable state under the run root;
- remains in foreground.

Both adapters must reject unexpected enabled services by parsing the final rendered configuration rather than relying only on substring checks.

## Deliverable 5: RouterInfo generation, validation, and exchange

Implement an explicit staged exchange:

1. start Java I2P in isolation with no peer RouterInfo;
2. wait for a bounded readiness condition and RouterInfo production;
3. stop or pause as required by the pinned implementation for safe import;
4. start i2pd in isolation with no peer RouterInfo;
5. wait for readiness and RouterInfo production;
6. validate both RouterInfo files using i2pr's strict RouterInfo parser without accepting them into production state;
7. verify signatures, network ID, NTCP2 address material, exact synthetic endpoint, and size bounds;
8. copy each RouterInfo into the other implementation using the exact pinned filename and directory conventions;
9. restart/reload using the documented implementation behavior;
10. permit the selected dial policy;
11. observe an authenticated link.

If a reference can safely import RouterInfo while running, document and test that behavior. Otherwise, use bounded restart sequencing. Do not mutate cache directories or persistent user locations.

Raw RouterInfo files remain inside the secret-bearing run root and must be deleted after evidence derivation.

## Deliverable 6: Authenticated-link observation

Create at least two independent observation signals, one from each implementation where feasible.

Acceptable signals include:

- a structured local status output exposed by the pinned implementation and confined to the namespace;
- a process-local state file whose format and semantics are documented in the pinned source;
- a specific authenticated-session state transition in bounded logs, parsed into a typed event and then discarded;
- transport counters that increment only after NTCP2 authentication;
- a verified protocol-level exchange observed by a local test adapter.

Unacceptable as the sole signal:

- process remains running;
- port is listening;
- TCP connect succeeds;
- generic log line mentions NTCP2;
- peer RouterInfo appears in a directory;
- bytes were transferred without authentication proof.

The crosscheck should preferably require corroboration from both routers. If only one implementation exposes an authoritative signal, pair it with a protocol-level proof and document the limitation.

## Deliverable 7: Direction control and connection-race handling

The test must avoid ambiguous simultaneous dial behavior during the initial proof.

Implement one of these reviewed approaches:

- configure one router as reachable/listening and delay peer import or dial activation on the other;
- use namespace firewall rules temporarily to permit only one initiation direction, then reverse in a separate run;
- use implementation-specific controls verified from pinned source.

Do not infer direction from source port numbers after the fact.

After directional smoke passes, add a separate simultaneous-connect/duplicate-link reference scenario only if it provides useful harness validation. It must not be conflated with i2pr's duplicate-link policy test.

## Deliverable 8: Reference-crosscheck runner

Add a dedicated runner path, for example:

```bash
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
```

The runner must:

- require valid offline caches;
- perform the host contract check;
- allocate a disposable run root;
- create and verify the reference-pair topology;
- render and hash both configurations;
- start, supervise, and stop both routers;
- generate, validate, and exchange both RouterInfos;
- enforce direction and deadlines;
- collect typed authenticated-link outcomes;
- collect process and cleanup counters;
- write sanitized evidence outside the run root;
- remove all raw state;
- verify zero residual namespaces, processes, interfaces, identities, RouterInfos, and logs.

`reference-crosscheck-ipv4` must no longer map to the Java and i2pd i2pr handshake scenarios.

## Deliverable 9: Evidence schema extensions

A reference-pair record must contain:

- scenario ID and schema version;
- both full reference revisions;
- both artifact and installed-tree hashes;
- both configuration hashes;
- topology hash;
- private network-ID classification without exposing secret material;
- direction policy;
- RouterInfo validation results;
- authenticated-link observations from each side;
- connection-attempt and authenticated-link counters;
- process started/exited/forced counters for both routers;
- cleanup result;
- evidence digest;
- reproduction command.

Do not place raw endpoints, RouterInfo bytes, identities, static keys, log excerpts, home paths, or packet captures in committed or uploaded evidence.

## Deliverable 10: Tests

Add unprivileged tests for:

- scenario schema validation;
- identical private network-ID rendering;
- local/peer endpoint assignment;
- RouterInfo exchange path confinement;
- reference-pair evidence validation;
- direction-policy state transitions;
- rejection of TCP-connect-only success;
- cleanup overriding authenticated success.

Add opt-in Ubuntu privileged tests for:

- reference-pair namespace creation;
- directional firewall enforcement;
- RouterInfo production and import;
- complete reference crosscheck;
- emergency cleanup after injected child failure.

## Required execution sequence

On the authorized Ubuntu host:

```bash
bash scripts/interop/build-references.sh --offline
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
python3 scripts/interop/validate-evidence.py
sudo -E bash scripts/interop/cleanup.sh
```

Repeat the reference crosscheck from a fresh disposable run root without rebuilding. Then repeat after a host reboot or namespace cleanup cycle to prove no hidden persistent state is required.

## Stop conditions

Stop and record a typed blocker if:

- the references cannot be configured for the same non-public network ID;
- either implementation attempts reseed, DNS, update, public bootstrap, or public endpoint access;
- RouterInfo import requires modifying the immutable cache;
- RouterInfo signature, network ID, or endpoint validation fails;
- an authenticated session cannot be distinguished from TCP connection state;
- one router spawns an unsupervised process tree that cleanup cannot prove terminated;
- the crosscheck passes only when global firewall or host routing is modified;
- retained evidence would require raw logs, RouterInfo, keys, or packet captures.

## Exit criteria

Plan 041 is complete when:

- the pinned Java I2P and i2pd runtime trees are reused offline;
- both routers use the same proven private network ID;
- both RouterInfos are generated, strictly validated, and imported using pinned implementation conventions;
- authenticated NTCP2 reference-to-reference connectivity is proven under a controlled direction policy;
- a second run succeeds without rebuild or persistent router state;
- all evidence contains real hashes and typed observations;
- cleanup verifies zero residual state;
- the reference-only lane can be used as a control when implementing Plan 042.

This plan proves the test environment. It does not satisfy the Milestone 3 requirement for i2pr interoperability by itself.
