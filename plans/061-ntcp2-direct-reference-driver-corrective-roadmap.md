# Plan 061: NTCP2 direct reference-driver corrective roadmap

## Status and authority

- Status: planned.
- Plan type: corrective roadmap and execution-order authority for the remaining Milestone 3 mixed-router interoperability work.
- Supersedes the technical premise in Plan 059 and Plan 060 that pinned Java I2P 2.12.0 requires a support-router/floodfill topology or a future Java revision before `java-to-i2pr-ipv4` can run.
- Does not erase or rewrite prior evidence. Plan 058, Plan 059, and Plan 060 remain historical records of what was implemented and what was blocked at those commits.
- Becomes the active plan-of-record for restoring the four-direction NTCP2 contract.
- Milestone 3 remains open and every NTCP2 support row remains experimental and `advertised = false` until Plan 066 closes with two verified complete bundles.

## Problem statement

The current repository contains a substantial mixed-router harness, evidence schemas, an i2pr NTCP2 launcher, a partial i2pd direct helper, and a final-certificate pipeline. It is not ready for authoritative execution because several foundational assumptions and evidence contracts are wrong:

1. Java I2P was treated as requiring SAM, tunnels, floodfill support, or a future upstream transport seam. The pinned Java source contains the upstream stripped-router test pattern needed to run real NTCP/NTCP2 transport code with dummy client, NetDB, peer-manager, and tunnel facades while directly importing a RouterInfo and submitting an `OutNetMessage`.
2. The reference trigger schema treats an I2P Router Hash as 40 hexadecimal characters. Router Hash is SHA-256 and must be represented as 64 lowercase hexadecimal characters when encoded as hex.
3. The i2pr interoperability launcher sends a hard-coded DeliveryStatus identifier and currently accepts any decoded DeliveryStatus message instead of requiring exact per-run correlation.
4. The current i2pd helper validates the wrong hash width, hashes the SSU2 key instead of the selected NTCP2 `s` field, starts an incomplete subset of the i2pd runtime, sends `nullptr` instead of a real I2NP message, and interprets the `Transports::SendMessage` future as if it directly proves a new asynchronous connection and message transfer.
5. Java and i2pd observations are not yet symmetric, structured, exact-message evidence. Generic log phrases must not satisfy frame-decrypt or I2NP-decode requirements.
6. Plan 060 attempted candidate/certificate closure before the reference drivers and exact receiver-side oracles were live-qualified.

## Correct target architecture

Each direction runs exactly two router processes in one fresh sealed rootless network namespace or one equivalently isolated guest-contained rootless namespace:

```text
reference router (192.0.2.1) <---- NTCP2 ----> i2pr-interop (192.0.2.2)
```

The test architecture uses:

- one fixed synthetic IPv4 address per peer;
- one fixed per-scenario port allocation;
- private network ID 99;
- no default route;
- no DNS;
- no reseed;
- no public floodfill or support router;
- no SAM or I2CP trigger;
- no tunnel construction;
- direct signed RouterInfo exchange through the owned run root;
- one real correlated DeliveryStatus I2NP message per direction;
- fresh mutable identity and runtime state per direction and per run.

The four required directions are:

1. `i2pr-to-java-ipv4`;
2. `java-to-i2pr-ipv4`;
3. `i2pr-to-i2pd-ipv4`;
4. `i2pd-to-i2pr-ipv4`.

## Required proof for one direction

A direction may report `passed` only when all of the following are independently bound to one run identity:

```text
exact_source_provenance = true
exact_reference_provenance = true
router_info_continuity = true
sender.ntcp2_authenticated = observed
receiver.ntcp2_authenticated = observed
sender.frame_emitted = observed
receiver.frame_authenticated_and_decrypted = observed
receiver.i2np_message_decoded = observed
receiver.delivery_status_message_id = expected_delivery_status_message_id
receiver.peer_router_hash_sha256 = expected_sender_router_hash_sha256
sandbox_attestation = valid
parent_network_state_unchanged = true
cleanup_result = clean
synthetic_fallback_used = false
```

A sender callback, socket write, connection-established state, or generic type-10 log line is not by itself receiver proof.

## Plan decomposition

### Plan 062: evidence-contract and architecture correction

Owns:

- new ADR superseding the rejected Java-support-topology conclusion;
- Router Hash correction from 40-hex to 64-hex;
- trigger schema v4;
- mandatory per-run DeliveryStatus correlation;
- structured reference-event and observation requirements;
- candidate retirement/supersession rules for Plan 060;
- fail-closed schema and negative fixtures.

Plan 062 must land first. Plans 063 and 064 must not invent incompatible local schemas.

### Plan 063: Java I2P stripped-router direct NTCP2 driver

Owns:

- source verification against the exact pinned Java I2P 2.12.0 tree;
- a test-only Java driver derived from the upstream `SSUDemo` architecture;
- `listen` and `dial` modes;
- dummy NetDB direct RouterInfo import;
- real `OutNetMessage` DeliveryStatus submission;
- exact receive handler and peer correlation;
- bounded startup/shutdown and 10/10 qualification;
- Java-specific negative controls and provenance.

### Plan 064: i2pd direct driver and observational hook correction

Owns:

- replacement of the current helper;
- canonical pinned i2pd initialization order;
- real DeliveryStatus submission;
- correct asynchronous connection/message semantics;
- correct NTCP2 address/static-key validation;
- dual `listen` and `dial` modes;
- compile-time-gated post-decrypt/post-`FromNTCP2` receive observer;
- successful-frame-write sender observer;
- uninstrumented control build;
- i2pd-specific negative controls and provenance.

Plans 063 and 064 may execute in parallel after Plan 062 closes.

### Plan 065: canonical integration and live qualification

Owns:

- i2pr scenario and launcher changes;
- exact DeliveryStatus ID verification;
- Java/i2pd adapter replacement;
- canonical mixed-runner integration;
- removal of SAM/HTTP trigger authority;
- strict two-process topology;
- qualification receipts;
- full four-direction live diagnostic bundle;
- fail-closed aggregate semantics;
- evidence durability and cleanup verification.

Plan 065 starts only after Plans 063 and 064 close locally and their exact drivers are buildable from pinned source.

### Plan 066: fresh candidate and authoritative two-run closure

Owns:

- retirement of the Plan 060 declared-not-executable candidate;
- final pre-freeze validation;
- exact candidate provenance;
- one execution-lane lock;
- Run A and independent Run B;
- final bundle verification;
- independent review;
- Milestone 3 closure decision.

Plan 066 is execution-only after freeze. Any code, configuration, driver, schema, observer, reference, or verifier change retires the candidate and returns work to the owning earlier plan.

## Dependency graph

```text
Plan 061 roadmap
        |
        v
Plan 062 contract + ADR + schema
        |
        +------------------+
        |                  |
        v                  v
Plan 063 Java driver   Plan 064 i2pd driver
        |                  |
        +---------+--------+
                  |
                  v
Plan 065 integration + live qualification
                  |
                  v
Plan 066 fresh candidate + two-run certificate
```

## Global implementation rules

1. Preserve pinned references unless a separate explicit re-pin decision is recorded. Do not silently substitute current upstream `master` or `openssl` branch state for the repository lock.
2. Source-verify every referenced API against the pinned tree before implementation. Current-upstream examples are research guidance, not a substitute for pinned-source inspection.
3. Reference drivers and observers are test/integration code only. They must never become production dependencies of `i2pr-daemon` or lower production crates.
4. No reference driver may implement, bypass, or patch NTCP2 cryptography, Noise transcript state, framing, RouterInfo signature verification, or transport acceptance policy.
5. The i2pd observational patch may observe only after successful protocol operations. It must not alter control flow, return values, buffering, cryptographic state, framing, timing decisions, routing, or retry policy.
6. No generic log phrase may satisfy exact message correlation.
7. No synthetic fixture, mocked reference, self-handshake, reference-only control, or handshake-only run may produce a passing primary direction.
8. No public network access is allowed during execution. Preparation/build and sealed execution remain distinct trust domains.
9. No candidate may be frozen before one complete live diagnostic four-direction bundle passes from the same implementation surface.
10. Do not weaken the four-direction contract to accommodate a driver defect. Fix the driver or record a typed blocker.

## Global non-goals

This roadmap does not authorize:

- production daemon NTCP2 activation;
- RouterInfo publication to the public network;
- NetDB participation, reseed, tunnel building, or floodfill behavior;
- SAM or I2CP interoperability work;
- SSU2 work;
- IPv6 qualification;
- throughput or load benchmarking;
- support advertisement;
- public-network anonymity or security claims;
- modifications to reference-router cryptographic behavior.

## Shared execution environment

The authoritative lane should use a dedicated Ubuntu 24.04 amd64 host or guest with:

```text
recommended: 8 vCPU, 24-32 GiB RAM, 80+ GiB free disk
minimum qualification target: 4 vCPU, 16 GiB RAM, 50 GiB free disk
```

Required tools include:

- JDK 17 and Ant 1.10.2 or later;
- pinned Rust toolchain and Cargo;
- CMake and Ninja;
- compatible GCC/G++;
- pinned-required Boost and OpenSSL development libraries;
- Python 3;
- nftables and iproute2;
- working unprivileged user/network/mount/PID namespaces.

Build/reference acquisition may use the network only in the preparation domain. The execution domain must have no public route and no DNS.

## Cross-plan validation baseline

Every implementation plan must retain these local checks unless it explicitly adds a stricter superset:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh

git diff --check
```

A plan may use focused checks during development, but closure requires the full applicable baseline.

## Roadmap-level stop rules

Stop the active plan and record a typed blocker when:

- the pinned Java source does not contain an equivalent real NTCP transport path after direct inspection;
- the pinned i2pd source cannot expose a non-behavioral observer without modifying protocol semantics;
- a required reference API is private and no source-locked test-only adapter can call it without production behavior changes;
- exact DeliveryStatus correlation cannot be demonstrated at the receiver;
- the selected execution lane cannot enforce no-public-egress isolation;
- cleanup cannot prove zero residual processes/namespaces/sockets/state;
- a live failure requires changing a frozen candidate;
- provenance contains placeholders, zero digests, ambiguous source revisions, or uncommitted guest edits.

Do not convert these conditions into `skipped`, `not-required`, or a reduced pass predicate.

## Roadmap acceptance criteria

Plan 061 is complete as a planning artifact when:

- Plans 062 through 066 exist and reference this roadmap;
- each plan has explicit dependencies, file ownership, work packages, tests, acceptance criteria, non-goals, and stop rules;
- the chain preserves the four primary IPv4 directions;
- the chain removes the support-router requirement from the target design;
- the chain requires exact SHA-256 Router Hash and DeliveryStatus correlation;
- the chain separates local driver qualification, live diagnostic qualification, and final candidate execution;
- the chain explicitly keeps NTCP2 experimental and non-advertised until final closure.

## Handoff order

A small or medium implementation model should execute exactly one numbered plan at a time:

1. Plan 062;
2. Plan 063 and Plan 064, independently, with neither marking the other complete;
3. Plan 065;
4. Plan 066.

For each plan:

- read the full plan before modifying files;
- inspect every named existing file before editing it;
- create a checklist from the plan work packages;
- commit cohesive work packages rather than broad unrelated changes;
- run the named focused tests after each package;
- run the full closure checks before writing a status/closure record;
- report exact commit SHA and any typed blockers;
- never write a closure record that claims an external run that was not actually executed.
