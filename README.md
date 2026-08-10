# i2pr

`i2pr` is an experimental, long-term effort to build a clean, maintainable I2P router in Rust.

The project is intended to provide a CLI-first router with a modular architecture, a defense-in-depth security posture, strict protocol handling, and clear internal boundaries between wire protocols, routing policy, client APIs, and application-facing tunnel services.

The initial compatibility target is the current I2P network as implemented by I2P/I2P+, i2pd, and other interoperable routers. The internal design does not need to mirror the Java router or Emissary, but protocol behavior must remain wire-compatible unless an explicitly isolated research mode states otherwise.

## Project status

The repository contains a buildable nine-crate Rust workspace with bounded
protocol codecs, reviewed cryptographic wrappers, versioned storage, runtime-
neutral service and transport contracts, a Tokio-owned runtime, a deterministic
testkit, and a non-networked daemon shell. NTCP2 remains experimental and
non-advertised. The non-production interoperability tooling includes the
runtime-owned NTCP2 wire driver, source-locked Java I2P and i2pd direct drivers,
staged evidence contracts, and the Plan 082 state-preparation boundary.
No retained real mixed-router TCP, NTCP2 handshake, authenticated-frame, or
DeliveryStatus result exists. Earlier milestone details are retained below as
an implementation history.
Plans 011–013 provide the structural and local cryptographic foundation for
common I2P identities, mappings, certificates, RouterInfo, RouterAddress,
Lease, classic LeaseSet, explicit identity generation, atomic reload, local
RouterInfo signing, and the initial bounded I2NP message model. Cryptographic
interoperability, LeaseSet2-family records, transport integration, networking,
router behavior, and I2NP body state-machine semantics remain unimplemented.
Normal development and CI use pinned Rust 1.95.0; the declared Rust 1.85 MSRV
is checked by a dedicated Ubuntu CI job. Plan 021 now provides a concrete,
non-networked Tokio runtime with deterministic service supervision, wakeable
cancellation, readiness/health snapshots, bounded restart policy, and
graceful/forced shutdown. Live router behavior and network interoperability
remain unimplemented.
Plan 022 now adds bounded command, request, and event channels, latest-state
snapshots, typed overload outcomes, and runtime-neutral resource leases with
atomic bundles and bounded diagnostics. These are infrastructure contracts
only; no live transport, NetDB, tunnel, client, or listener behavior has been
introduced. Plan 023 now adds a bounded deterministic testkit with manually
wakeable monotonic time, domain-separated seeds, in-memory stream/datagram
links, executable fault scripts, ephemeral peer/topology factories, and
privacy-safe replay records. The testkit is a manual simulation pump only: it
opens no sockets, performs no DNS, persists no private identities, and does not
provide transport interoperability evidence.
Plan 024 now adds fixed-name, privacy-aware runtime events; redacted aggregate
supervisor/channel/resource snapshots; latest-state health correctness when no
subscriber is attached; integrated clean, overload, restart, essential-failure,
and stream/datagram fault scenarios; and a fixed 32-seed deterministic replay
matrix. These are bounded local validation artifacts, not protocol, anonymity,
resilience, or public-network evidence.
Plan 025 corrects forced child cleanup ownership, cancellation-aware service
completion classification, physical protocol module ownership, CI guardrails,
and resource-release underflow visibility. Its closure remains limited to
bounded local evidence and does not add router behavior or interoperability.
Plan 031 established the first Milestone 3 boundary: the runtime-neutral
`i2pr-transport` contracts, the Tokio-free `i2pr-transport-ntcp2` skeleton,
bounded link/delivery/resource vocabulary, and deterministic synthetic
transport evidence. Plan 032 added the non-I/O transcript foundation and Plan
033 now adds the bounded, runtime-neutral NTCP2 handshake codecs and consuming
state machines. Plan 034 now adds bounded authenticated data frames, strict
payload blocks, direction-specific frame owners, and deterministic partial-I/O
evidence. None of these plans add sockets, live addresses, mixed-router
interoperability, NetDB mutation, or capability advertisement; all transport
support remains non-advertised experimental work.
Plan 032 now adds the non-I/O NTCP2 cryptographic foundation: reviewed
X25519/AES/ChaCha20-Poly1305/HMAC/SipHash wrappers, a consuming three-message
transcript model, an independently generated deterministic crypto corpus, and
a separate hardened static-key/IV store. Plan 033 adds bounded codecs for all
three handshake messages, consuming initiator/responder transitions, replay and
clock-skew policy seams, RouterInfo/static-key binding, and explicit runtime-
neutral I/O actions. Plan 034 adds SipHash-masked lengths, AEAD frames, strict
authenticated blocks, and terminal counter/error handling. These remain local
experimental evidence only; sockets, mixed-router interoperability, NetDB
mutation, and capability advertisement remain unimplemented.
Plan 035 now adds a bounded runtime-owned TCP integration seam: strict NTCP2
address interpretation, pre-crypto admission, replay/backoff owners, controlled
loopback listener/dial services, and joined link children. The runtime socket
surface is disabled outside explicit controlled tests; no public listener,
automatic address publication, NetDB mutation, mixed-router interoperability, or
capability advertisement is claimed.
Plan 037 was the corrective integration plan. Its boundary keeps inbound
admission attached to the accepted stream through handshake completion or a
typed terminal outcome, applies configured cancellation/deadline policy to the
actual link I/O, and gives each queued frame one bounded ownership path for item
and byte accounting. It also separates strict SessionConfirmed parsing from
general data-phase block parsing. Plan 042 now supplies the complete bounded
authenticated socket/data-phase composition through the non-production
launcher; daemon activation and mixed-router evidence remain disabled.

Plan 042 defines the bounded NTCP2 wire driver owned by `i2pr-runtime` and
driven by the runtime-neutral handshake/data state machines. The runtime driver
owns socket I/O, action deadlines, cancellation, replay and admission
decisions, authenticated frame state, bounded queues, and link/task cleanup.
The non-production `i2pr-interop` launcher now validates confined scenario
input, prepares disposable identity/RouterInfo state, drives listener or dial
handshakes, promotes authenticated links, and exchanges a bounded
DeliveryStatus message; it does not activate `i2pr-daemon`.

Plan 038/040 define an Ubuntu-only, opt-in reference-router harness for
acquiring the missing evidence under controlled conditions. Plan 041 adds the
dedicated Java I2P/i2pd reference-pair crosscheck. It is a harness contract,
not a production bootstrap path or an interoperability result. The supported
host contract is Ubuntu amd64 with `apt`, Bash 4+, Python 3, Linux network
namespaces, `iproute2`, and `sudo`. Preparation may install declared packages,
fetch only the pinned Java I2P 2.12.0 and i2pd 2.60.0 sources, build disposable
reference artifacts, and record hashes. Execution is a separate network-
isolated phase: it creates disposable namespaces joined only by a veth pair,
rejects default routes, DNS, and public egress, generates temporary state, and
cleans up before reporting a result. The execution phase must not download,
reseed, bootstrap, publish RouterInfo, mutate NetDB, or start the normal daemon.

The command surface is:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline
bash scripts/interop/run-scenario.sh --scenario <id> --reference java_i2p --build-cache <path> --run-root <path>
bash scripts/interop/run-scenario.sh --scenario <id> --reference i2pd --build-cache <path> --run-root <path>
bash scripts/interop/run-matrix.sh --profile environment-smoke
i2pr-interop ntcp2 prepare --state-dir <path> --local-address <synthetic-ip> --local-port <port> --network-id 99
i2pr-interop ntcp2 validate-scenario --scenario-config <path>
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

The launcher status boundary is explicit. A completed `listen` path emits
listener readiness separately from a later authenticated terminal result;
`dial` emits one terminal typed result; and `inspect` emits only bounded,
redacted state metadata. Readiness is not authentication. State, handshake,
data-phase, timeout, and cleanup failures remain typed rejections; no launcher
status is mixed-router evidence.

Environment smoke proves only that each reference can start, produce
disposable state, avoid public connections, and stop cleanly. The
`reference-crosscheck-ipv4` profile runs both directional reference-pair
scenarios with the separately owned topology, explicit private network ID 99,
strict RouterInfo validation/import, and dual authenticated-link observations.
It remains reference-control evidence, not an i2pr run. A missing host/cache,
strict parser, or authoritative observation is a typed blocker. Neither
profile is i2pr evidence. The
i2pr mixed-router profile requires bounded authenticated runs in both
directions against each reference; the full manifest and its adversarial
profiles remain gated on positive i2pr handshake/data smoke in both directions.
Retained evidence is written only under `target/interop/evidence/` and is
limited to typed outcomes, run metadata, and hashes of sanitized artifacts.
Secret-bearing run roots under `target/interop/runs/<run-id>/` are deleted.
Raw addresses, peer identities, RouterInfo, I2NP, keys, transcripts, logs, and
remote error text are disposable and must not be committed.

Plan 042 selects the existing fixed-size DeliveryStatus message (I2NP type 10)
as the initial smoke scope. Its body is 12 bytes; the NTCP2/SSU2 short I2NP
encoding is 21 bytes before the 3-byte NTCP2 block header, frame overhead, and
padding. The launcher’s local gate is one valid outbound and one valid inbound
DeliveryStatus, with bounded message IDs/timestamps and no
NetDB, tunnel, garlic, or public-routing behavior. Reference acceptance and
response behavior have not been verified here, so this selection is a Plan 042
scope decision, not interoperability evidence; padding or TCP readiness cannot
stand in for the message exchange.

No production-ready router functionality exists yet. Do not use `i2pr` for anonymity, privacy, censorship resistance, or security-sensitive workloads until the project has completed protocol interoperability, adversarial testing, and an independent security review.

### Plan 043 build-system status

The Ubuntu build-system lane has an explicit ordered promotion contract:

```text
contract -> reference-build -> reference-offline-reuse -> environment-smoke
-> reference-crosscheck-ipv4 -> i2pr-handshake-smoke-ipv4 -> full-matrix
-> evidence-validation -> cleanup-verification
```

Preparation is the only network-enabled trust domain. Execution is offline and
uses only verified reference caches plus disposable namespace-local veth links.
The exact host is Ubuntu 24.04 amd64/x86_64 with the lock-listed package set,
namespace/nftables capability, UTF-8 locale, non-interactive sudo when needed,
and at least 4 GiB free under `target/`. Cache reuse binds the canonical
reference, full source revision, lock digest, host contract, build-command
version, and relevant tool/ABI metadata; a miss never permits a fetch.

Environment smoke and the Java-I2P/i2pd `reference-crosscheck-ipv4` profile are
harness controls only. The reference control must pass before the four
independent i2pr/reference IPv4 directions are eligible. A positive i2pr gate
requires authenticated handshake, strict binding, bounded DeliveryStatus
exchange in each direction, sanitized evidence, and clean state. The full
matrix adds bounded adversarial and resource cases; it does not run unbounded
fuzzing.

The evidence gate accepts only an aggregate manifest and sanitized typed JSON
with approved hashes. Cleanup runs unconditionally, and an independent
clean-host verifier must reject residual namespaces, veths, processes,
secret-bearing run roots, forbidden retained files, and attributable host
firewall or route changes. A cleanup failure overrides protocol success.

Promotion is manual first, scheduled only after repeated clean-checkout and
cache-reuse success, then a current successful run at Milestone 3 closure. The
workflow and helper apparatus now expose the ordered manual Plan 043 lane,
including clean-host verification and aggregate validation. No completed
successful aggregate run or mixed-router i2pr evidence is present in this
checkout; these are blockers, not skipped successes. NTCP2 remains experimental
and non-advertised.

### Plan 045 mixed-router integration status

Plan 045 supersedes Plan 044 as the plan of record. Plan 044's
"implementation-complete locally" status is amended: ten Plan 045
defects (D1–D10) invalidated the prior claim. Plan 045 closes those
defects as a structured corrective pass.

- D1: the ``-gen`` and live reference adapters share one disposable
  ``reference-data`` directory so the live reference restarts from the
  identity that produced the exported RouterInfo; the i2pr side shares
  the same ``state`` directory across the ``-gen`` and live phases.
- D2: the Rust launcher persists RouterInfo inside the scenario's
  ``state_dir``; the mixed-runner exports it from there to the
  ``exchange`` directory and records a real SHA-256 digest in the
  evidence record. The reference RouterInfo digest is recorded too.
- D3, D6: the strict launcher scenario schema now allows an explicit
  allowlist of optional fields (``data_phase_mode``,
  ``data_phase_required_peer_action``, ``data_phase_timeout_ms``,
  ``expected_observation``) and supports the
  ``fixed-12-byte-payload`` smoke profile alongside
  ``delivery-status``. The Rust launcher parses the same schema.
- D4: the reference trigger performs the per-direction SAM v3 (Java)
  or HTTP JSON-RPC (i2pd) dial inside the disposable namespace.
- D5: the data-phase oracle records per-side observation code keyed
  by the i2pr launcher's authenticated-frame counters; no echo
  assumption is made.
- D6 (Rust): the launcher dispatches ``DataPhaseMode::HandshakeOnly``,
  ``InitiatorDataOnly``, ``ResponderDataOnly``, and the prior
  ``RoundTripDeliveryStatus`` mode with distinct typed terminal
  reasons. Initiator and responder scenarios can complete without
  requiring the peer to echo a ``DeliveryStatus``.
- D7: the mixed-runner requires the i2pr terminal result to be
  ``passed``, the reference observation to be ``authenticated``, and
  the data-phase oracle's per-side observation to be ``observed``
  before marking a direction ``passed``. The prior pass-after-handshake
  predicate is removed.
- D8: the sanitized evidence record now carries
  ``i2pr_router_info_sha256``, ``reference_router_info_sha256``,
  ``data_phase_mode``, and ``expected_observation`` typed fields
  populated by the runner.
- D9: ``run-matrix.sh`` continues to route the four directional mixed
  scenario IDs through ``mixed_runner.py``. The Plan 045 typed
  blocker for "i2pr-mixed-router-profile-not-wired" remains reserved
  for scenario IDs that are not allowlisted for the active gate.
- D10: an unknown reference kind now fails closed with a typed
  ``unknown-reference-kind`` rejection; it does not silently fall
  through to the i2pd adapter.

The Plan 044 closure document (`plans/044-closure.md`) is amended to
record the Plan 045 supersession. No completed mixed-router i2pr
record is present in this checkout; these remain typed blockers. NTCP2
remains experimental and non-advertised.

### Plan 046 rootless sealed-namespace evidence lane

Plan 046 replaces the host-global namespace requirement for the primary
NTCP2 interoperability evidence path with a **rootless, process-scoped
sandbox**. The primary evidence topology is now:

```text
rootless-sealed-single-netns
```

with privilege model `unprivileged-userns`. It is runnable by an ordinary
user without `sudo`, passwordless elevation, host capabilities, setuid
helpers, host-visible named network namespaces, host-visible veth
devices, or host nftables mutation. The legacy
`privileged-dual-netns-veth` topology is preserved as an explicit
optional qualification lane; it is never the default and is never a
silent fallback.

The rootless lane proves protocol compatibility. It does not claim
separate-stack network behavior, asymmetric firewall semantics, packet
loss, route mutation, or interface-failure semantics. The retained claim
is intentionally narrow:

> The pinned i2pr and reference-router processes completed the declared
> NTCP2 direction inside a process-scoped, rootless user/network
> namespace whose canonical isolation checks passed and whose creation
> and teardown did not alter the parent host's canonical network state.

A passing rootless run requires a sanitized sandbox attestation and an
unchanged parent-host network digest. The mixed-router evidence schema
now carries `topology_kind`, `privilege_model`,
`sandbox_attestation_sha256`, and `parent_network_state_unchanged`. A
passed record that fails any of these checks is rejected. NTCP2 remains
experimental and non-advertised; Milestone 3 is still open.

The new rootless entrypoint, probe, supervisor, and topology modules
live under:

- `scripts/interop/rootless-enter.sh` — outer entrypoint; the only path
  that creates the sandbox.
- `scripts/interop/probe-rootless-sandbox.sh` — bounded create /
  configure / connect / teardown probe with strict typed outcomes.
- `tests/integration/ntcp2/harness/rootless_supervisor.py` — inner
  namespace verification.
- `tests/integration/ntcp2/harness/rootless_topology.py` — sealed
  in-sandbox topology backend.
- `tests/integration/ntcp2/harness/rootless_inner_runner.py` — inner
  scenario dispatch.
- `tests/integration/ntcp2/harness/interop_topology.py` — backend
  contract (`ProcessPlacement`, `InteropTopology`, topology registry).
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md` —
  architectural decision record.
- `scripts/check-rootless-interop-boundary.sh` — static boundary
  checker (no `sudo`, no host network mutation, no fallback).
- `.github/workflows/ntcp2-interop-rootless.yml` — manual,
  no-escalation workflow.

The Plan 046 status file (`plans/046-status.md`) records the
implementation-completion stage; the closure record is
`plans/046-closure.md`. Plan 046 closed with a typed host-level blocker
on this checkout (`blocked_unprivileged_user_namespace`); the on-host
evidence is at
`target/interop/evidence/handshake-smoke-rootless--host-blocked/`.
Cross-host recovery is recorded in
`plans/047-cross-host-rootless-lane-expansion.md`.

### Plan 050 cloud-init recovery and guest-probe pass

Plan 050 minimizes the Multipass cloud-init unit (no `rustup` or host
toolchain inside the guest), adds a sanitized cloud-init failure
taxonomy in `scripts/interop/multipass/cloud_init_status.py`, exposes
`--guest-probe-only` in `run-evidence-lane.sh`, and adds a
`selective-purge.sh` remediation that only invokes
`multipass purge <instance>` when an ownership contract is proven. The
implementation is local-only; the Plan 048/049 external evidence
remains the negative baseline on this host. Implementation status is
recorded in `plans/050-status.md`.

### Plan 052 Milestone 3 evidence closure follow-up

Plan 052 is the corrective execution and evidence-closure plan for
Milestone 3. Milestone 3 remains open: NTCP2 stays experimental and
non-advertised. Plan 052 introduces:

- A single-source `run-identity.json` (`i2pr-interop-run-identity-v1`)
  bound to every direction, attestation, trigger, observation,
  cleanup, and aggregate record via a new `RUN_IDENTITY_BIND_FIELDS`
  suffix on the existing evidence record schema.
- A tri-state `I2PR_INTEROP_DIAGNOSTICS=off|sanitized|raw-local` env
  var that replaces the prior `I2PR_INTEROP_DUMP_RUN_LOGS` switch.
  `raw-local` is forbidden under any export root.
- A typed per-side observation schema
  (`i2pr-ntcp2-direction-observation-v2`) with bounded levels and
  the new directional predicate requiring both-side
  `ntcp2_authenticated`, sender `frame_emitted`, and receiver
  `frame_authenticated_and_decrypted` AND `i2np_message_decoded`.
- Atomic evidence-bundle export under
  `target/interop/evidence/milestone-3/<run-id>/` with the
  environment block, per-direction records, sanitized manifest, and
  hash-verified export acknowledgement.
- A standalone Java startup probe
  (`tests/integration/ntcp2/harness/java_startup_probe.py`) that
  isolates Java startup from i2pr and NTCP2.
- Reference-trigger contracts
  (`tests/integration/ntcp2/reference-trigger-contracts.md`) and a
  source-derived observation catalog
  (`tests/integration/ntcp2/reference-observation-catalog.md`).

Plan 052 closes only when one exact clean source commit produces at
least two complete sanitized bundles in the rootless sealed Multipass
lane, each bundle contains exactly the four primary directions, every
direction proves both-side NTCP2 authentication plus sender emission
and receiver frame/I2NP acceptance, every record is bound to the same
run identity and pinned references, and every run finishes with
verified evidence export, unchanged parent network state, and no
surviving process. Anything less remains a typed diagnostic result.
The scaffolding status is in `plans/052-status.md`.

### Plan 053 evidence-pipeline integration corrective pass

Plan 053 wires the Plan 052 evidence primitives into the canonical rootless and
Multipass dispatch path. `plan052_pipeline.py` measures one clean source
identity before directions, freezes it, binds all four directions to that
identity, and writes complete attestation, direction, trigger, observation-v2,
and cleanup classes even for blocked or rejected outcomes. The hardened bundle
writer verifies exact JSON bytes, manifest checksums, semantic schemas, safe
paths, regular-file trees, and immutable finalization before atomic export.
Export acknowledgements are written beside, never inside, the immutable
`target/interop/evidence/milestone-3/<run-id>/` bundle.

A local blocked/rejected bundle is classified as
`diagnostic-complete-not-certificate`; it is not interoperability evidence and
does not close Milestone 3. Java/i2pd receiver markers remain source-lock and
external-run blockers, so NTCP2 stays experimental and non-advertised. Plan
053 status and validation results are recorded in `plans/053-status.md`.

### Plan 054 Java startup and reference-observation qualification pass

Plan 054 is the local qualification corrective pass for the two Plan 052
predicates that depend on a live reference and a per-side observation marker.
It introduces the Plan 054 Java startup matrix, the frozen Java template
lifecycle, and the machine-readable reference observation catalog.

- The matrix driver lives in
  `tests/integration/ntcp2/harness/java_matrix.py`. It composes
  `java_startup_probe.py` once per cell of the 16-cell matrix
  (namespace × data-state × launcher × sequence) with three
  independent attempts each. The data-state `seeded-clone` is the
  new frozen-template clone; the new
  `java-process-spawn-failed` through `java-state-lock-invalid`
  failure stages classify every blocked or rejected attempt.
- The Java template preparation driver
  (`scripts/interop/java-prepare-template.py`) is the only path that
  may download, install, or seed Java state. It freezes the resulting
  template into a deterministic tree SHA-256 and writes
  `template-manifest.json` plus `template-tree.sha256`. The execution
  phase is restricted to `seeded-clone` and never re-launches the
  frozen template directly.
- The machine-readable reference observation catalog
  (`tests/integration/ntcp2/reference-observation-catalog.toml`,
  schema `i2pr-reference-observation-catalog-v1`) is the source of
  truth. The Markdown catalog
  (`reference-observation-catalog.md`) is now generated, drift-checked
  documentation; the static boundary checker rejects any pending
  source-inspection entry.
- The Java and i2pd adapters expose
  `collect_observation(role, run_id, correlation, log_cursor, catalog)`
  and return finalized
  `i2pr-ntcp2-direction-observation-v2` records. The Plan 052
  directional predicate (`_evaluate_plan052_predicate`) now applies
  `receiver_passes_data_phase` against those records and is no longer
  hardcoded to reject every direction.
- The Plan 052 pipeline
  (`plan052_pipeline._build_observation`) accepts the live
  `i2pr_observation` and `reference_observation` records; the
  synthetic builder remains as the typed fallback for blocked and
  rejected directions.

A complete 48-start matrix, the ten-consecutive-start qualification,
and the source-locked control experiments all require the pinned
Java 2.12.0 and i2pd 2.60.0 references on an authorized Ubuntu 24.04
amd64 host or Multipass guest. The local checkout on the
`apparmor_restrict_on` Plan 046 negative baseline cannot exercise the
matrix yet; the Plan 048/049 Multipass recovery lane is the
canonical external path. Plan 054 status is in `plans/054-status.md`.

### Plan 055 reference-initiated trigger and topology qualification pass

Plan 055 is the qualification pass for the two reference-initiated
directions (`java-to-i2pr-ipv4` and `i2pd-to-i2pr-ipv4`).

- The locked machine-readable trigger record schema
  `i2pr-reference-trigger-v3` lives in
  `tests/integration/ntcp2/harness/trigger_record.py` with the
  bounded `TriggerHelperKind` and `TriggerOutcome` enumerations. The
  trigger record carries helper source/binary digests, the target
  RouterInfo hash, the public NTCP2 static-key digest, the synthetic
  endpoint, the correlation nonce, the bounded monotonic timestamps,
  and the typed outcome. The Plan 052/053 pipeline binds the trigger
  digest into the per-direction record and rejects mismatches.
- The source-inspection record at
  `tests/integration/ntcp2/reference-trigger-contracts.md` carries
  the Plan 055 B5 i2pd direct-helper decision
  (`i2pd-direct-helper-selected`) and the Plan 055 C5 Java
  decision (`java-direct-helper-rejected-global-context-not-isolatable`).
  The i2pd helper drives `Transports::SendMessage` →
  `ConnectToPeer` directly against the unmodified pinned libraries;
  the Java helper would have to initialize a full
  `RouterContext`, which the plan forbids for the qualification
  pass.
- The optional `java-minimal-support-topology` fallback is
  governed by ADR 0021
  (`docs/adr/0021-minimal-java-support-topology.md`); the ADR must
  be approved before any support-topology helper may be
  implemented.
- A successful trigger record alone never marks a direction
  passed. The Plan 052 receiver-side observation predicate
  (`ntcp2_authenticated` + `frame_authenticated_and_decrypted` +
  `i2np_message_decoded`) remains the source of truth. A rejected
  i2pr responder stage preserves the bounded reason code even
  when the trigger reports `authenticated`.

The Plan 055 helpers and the support topology live in
external recovery lanes (Plan 046 sealed-namespace lane or the
Plan 048/049 Multipass lane). On the Plan 046
`apparmor_restrict_on` negative baseline the helpers cannot be
exercised, so the two reference-initiated directions remain typed
blockers. Plan 056 (`plans/056-closure.md`) closed with the same
typed host-environment blocker, and Plan 058 record and candidate
integrity closure pass split the previous Plan 057 follow-up plan
into three new plans:

- `plans/058-plan056-record-and-candidate-integrity-closure-pass.md`
  — retired the Plan 056 candidate and superseded Plan 057. The
  local diagnostic bundles under `target/interop/evidence/plan056/`
  are locally generated and intentionally untracked; the only
  tracked footprint is the bounded local-diagnostic receipt at
  `tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`.
- `plans/059-reference-side-implementation-and-live-qualification-closure-pass.md`
  — implements the i2pd direct helper, the per-reference
  observation qualification receipts, and the canonical pipeline
  live-mode wiring. The helper source, build contract, and
  source-lock record are committed under
  `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`;
  the qualification receipts live under
  `tests/integration/ntcp2/reference-observation-qualification/`.
  Plan 058 rejected ADR 0021, so the `java-to-i2pr-ipv4` direction
  remains a typed blocker for Java I2P 2.12.0 and Plan 059 closes
  with the typed blocker `blocked_java_support_topology_rejected`.
- `plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md`
  — cuts a fresh candidate after Plan 059 closes and produces the
  two-run certificate. Until Plan 060 produces two passing
  bundles from a fresh implementation-floor candidate, NTCP2
  stays experimental and non-advertised and Milestone 3 stays
  open.

### Plan 058 record and candidate integrity closure pass

Plan 058 is a documentation, provenance, and execution-contract
closure pass that retires the Plan 056 candidate, supersedes the
Plan 057 follow-up plan, decides ADR 0021 (Rejected), and splits
the previous Plan 057 responsibilities into Plan 059 (reference-side
implementation) and Plan 060 (fresh candidate + two-run
certificate). Plan 058 does not implement the i2pd direct helper,
the Java support topology, or external mixed-router execution.

- The candidate record integrity validator
  (`tests/integration/ntcp2/harness/candidate_record.py`, schema
  `i2pr-interop-candidate-v1`) refuses records with multiple
  authoritative SHAs, retired candidates consumed by execution
  tooling, candidates frozen before the implementation floor, and
  `committed` evidence claims that name ignored diagnostics.
- The Plan 058 test matrix (`test_plan058.py`) covers the positive
  and 14 negative fixtures, the on-disk
  candidate/ADR/Plan 057 supersession markers, the locked field
  set, and the two-lane contract.
- The static boundary checker
  (`scripts/check-ntcp2-interoperability.sh`) enforces the
  candidate record integrity invariants, the supersession markers,
  and the ADR decision marker.
- The Plan 058 record and candidate integrity closure pass defines
  two alternative execution lanes for any future Milestone 3
  evidence run: Lane A (direct-host, requires
  `rootless_sandbox_available` on the execution host) and Lane B
  (guest, the outer host may continue to report
  `blocked_unprivileged_user_namespace` but the guest must report
  `rootless_sandbox_available`). Exactly one lane is selected per
  candidate; a certificate may not combine Run A from one lane
  with Run B from another.
- The Plan 056 candidate is marked
  `retired; never used for an authoritative external run`. The
  historical SHA `fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf` is
  preserved verbatim as an audit record. The Plan 056 closure
  record describes the locally generated diagnostic bundles under
  the ignored `target/interop/evidence/plan056/` directory and
  names the bounded local-diagnostic receipt at
  `tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`
  with `artifact_storage = local-untracked`. Plan 057 is
  superseded before execution.
- ADR 0021
  (`docs/adr/0021-minimal-java-support-topology.md`) is Rejected.
  The repository does not implement the Java support topology; the
  `java-to-i2pr-ipv4` direction remains a typed blocker for the
  pinned Java I2P 2.12.0 revision; Plan 059 must close with the
  typed blocker `blocked_java_support_topology_rejected`; Plan 060
  must not start under the current four-direction contract.

The Plan 058 status and validation commands are recorded in
`plans/058-status.md`.

### Plan 059 reference-side implementation and live qualification closure pass

> **Note (Plan 068).** Plan 059 is a historical closure pass. The
> `blocked_java_support_topology_rejected` blocker in the original
> Plan 059 closure text is superseded by ADR 0022 (Accepted direct
> Java driver) and ADR 0023 (Accepted staged-evidence tiers); the
> historical text below is preserved verbatim for audit.

Plan 059 implements the i2pd direct helper, the per-reference
observation qualification receipts, and the canonical pipeline
live-mode wiring that Plans 055-057 deferred. ADR 0021 was Rejected
by the Plan 058 record and candidate integrity closure pass, so
Plan 059 closes with the typed blocker
`blocked_java_support_topology_rejected` and the `java-to-i2pr-ipv4`
direction remains blocked for the pinned Java I2P 2.12.0 revision.
Plan 060 cannot start under the current four-direction contract.

- The i2pd direct helper source, build contract, and source-lock
  record are committed under
  `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`.
  The C++ helper (`i2pd_direct_connect.cpp`) links against the
  pinned i2pd 2.60.0 libraries and exercises the documented
  `i2pd::transports::Transports::SendMessage` call graph recorded
  in `tests/integration/ntcp2/reference-trigger-contracts.md`. The
  Python bounded driver
  (`i2pd_direct_connect.py`) provides the local qualification seam
  when the C++ helper cannot be built.
- The per-reference observation qualification receipts
  (`i2pd-2.60.0.json` and `java_i2p-2.12.0.json`) record the
  catalog metadata, the runtime-control blocker, and the typed
  absence per semantic level. The summary at
  `summary.json` tracks the overall qualification status.
- The Plan 052 pipeline now exposes a `live_mode` flag; in live
  mode a passed reference-initiated direction requires a real
  trigger record and live sender/receiver observation-v2 records.
  Helper, source, catalog, and qualification-receipt digests bind
  into the direction record so drift fails the bundle cross-check.
  Cleanup failure overrides pass.
- The Plan 059 test matrix
  (`tests/integration/ntcp2/harness/test_plan059.py`) covers 36
  cases grouped into the five Plan 059 surfaces: i2pd helper,
  Java support-topology gate, receiver observations, Java startup
  gate, and pipeline live mode.
- The Plan 059 closure contract recorded the typed blocker
  `blocked_java_support_topology_rejected` because Plan 058 had
  Rejected ADR 0021. The Java support topology remains forbidden
  per that ADR rejection, but the
  `blocked_java_support_topology_rejected` interpretation is
  superseded by ADR 0022 (Accepted direct Java driver) and ADR
  0023 (Accepted staged-evidence tiers); the historical plan text
  is preserved verbatim. The active Java path is the Plan 063
  direct stripped-router driver integrated by Plan 065.

The Plan 059 status and validation commands are recorded in
`plans/059-status.md`.

### Plan 060 fresh-candidate and two-run Milestone 3 certificate closure pass

**Plan 060 is retired by Plan 062** (Plan 062 evidence-contract and
architecture correction pass). Plan 060 is no longer active
execution authority. The Plan 060 candidate record is preserved
verbatim at `plans/060-candidate.md` for audit. The Plan 060 closure
record at `plans/060-closure.md` carries the explicit "Superseded
by Plan 062" marker. Future candidates must descend from the Plan 065
implementation floor or later and must use the Plan 062 v4 trigger
schema, the Plan 062 reference-event v1 schema, the Plan 062 v3
observation schema, and the 64-hex SHA-256 Router Hash contract.

Plan 060 inherited the rejected Java-support-topology premise (ADR
0021 Rejected by Plan 058); Plan 062 ADR 0022 (Accepted) replaces
that premise with two-process direct transport drivers. The
Plan 060 candidate was frozen before the Plan 062 schema
corrections and the 64-hex SHA-256 Router Hash contract, so it is
not the authoritative source for the four-direction Milestone 3
closure.

On this host the historical Plan 060 typed blocker is
`blocked_execution_lane_unavailable` and the candidate is
`declared-not-executable`. The Plan 046 rootless sealed-namespace
probe returns `blocked_unprivileged_user_namespace` (the host's
kernel activates
`kernel.apparmor_restrict_unprivileged_userns=1`, which confines
every unprivileged user namespace to a restrictive AppArmor
policy). The Plan 048/049 Multipass recovery lane is the canonical
external path but cannot complete on this constrained host (per
Plan 051: 15 GiB physical RAM, three reserved qemu guests,
multipassd unresponsive). The Plan 060 implementation surface is
preserved as an audit record.

The Plan 060 implementation surface remains mandatory:

- `tests/integration/ntcp2/harness/plan060.py` — the Plan 060
  helper module. Exports the typed blocker
  (`blocked_execution_lane_unavailable`), the close-status
  classifier (`declared-not-executable`), the lane-lock helper
  for the Plan 058 two-lane contract, the candidate-record digest
  table, the freeze-readiness checklist, the
  `assert_plan060_freeze_invariants` enforcer, and the
  cross-bundle independence checker
  (`plan060_two_bundle_independence`).
- `tests/integration/ntcp2/harness/test_plan060.py` — the Plan 060
  test matrix (35 cases).
- `scripts/check-ntcp2-interoperability.sh` enforces the Plan 060
  artifacts and the Plan 060 test matrix coverage as historical
  invariants.
- `plans/060-candidate.md` — the Plan 060 candidate record
  (status `retired`).
- `plans/060-closure.md` — the Plan 060 closure record with the
  Plan 062 supersession marker.

A future pinned Java revision that exposes a transport-only
direct seam may trigger an ADR re-issue that supersedes the ADR
0021 rejection and unblocks the `java-to-i2pr-ipv4` direction.

### Plan 062 NTCP2 evidence-contract and architecture correction

Plan 062 is the evidence-contract and architecture correction pass
that supersedes the Plan 060 execution authority. The plan does
not implement the Java or i2pd drivers and does not perform an
authoritative external interoperability run; those belong to
Plans 063 and 064.

Plan 062 lands:

- `docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md`
  (Accepted) replacing the rejected Java-support-topology
  premise with two-process direct transport drivers for Java I2P
  and i2pd. ADR 0022 explicitly supersedes the conclusion of
  ADR 0021 without rewriting ADR 0021.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  — the source-locked API inspection record for the pinned Java
  I2P 2.12.0 and i2pd 2.60.0 revisions.
- `tests/integration/ntcp2/harness/reference_trigger_v4.py` — the
  Plan 062 v4 trigger schema (`i2pr-reference-trigger-v4`) with
  64-lowercase-hex Router Hash, per-run DeliveryStatus
  `message_id` (`1..=0xffffffff`), and full provenance digests.
- `tests/integration/ntcp2/harness/reference_event.py` — the
  Plan 062 reference-event v1 schema (`i2pr-reference-event-v1`)
  recording per-driver structured events with exact DeliveryStatus
  message ID correlation for data-phase events.
- `tests/integration/ntcp2/harness/observation_v3.py` — the
  Plan 062 v3 observation schema (`i2pr-ntcp2-direction-observation-v3`)
  with the mandatory correlation fields
  `delivery_status_message_id`, `peer_router_hash_sha256`,
  `local_router_hash_sha256`, and `source_event_sha256`. The v3
  receiver pass predicate requires nonzero decrypt and decode
  counts and rejects generic-phrase-only sources.
- The historical `trigger_record.py` (v3) and `observation.py`
  (v2) modules remain readable for historical inspection but
  cannot contribute to a new passing bundle.
- Retirement of the Plan 060 candidate from all future candidate
  validators and the static boundary checker. The future
  candidate implementation floor is Plan 065 closure or later.

Plan 062 documentation updates:

- `README.md` records the Plan 060 retirement and the Plan 062
  supersession.
- `docs/architecture/interop-apparatus.md` records the Plan 062
  evidence-contract correction.
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` records the
  Plan 062 workstream summary.
- `plans/030-milestone-3-closure.md` records the Plan 062
  supersession in the aggregate Milestone 3 status.

NTCP2 stays experimental and non-advertised; Milestone 3 stays
open until Plan 065 closes with one complete four-direction live
diagnostic bundle and Plan 066 produces a verified Milestone 3
certificate.
Until then, NTCP2 stays experimental and non-advertised and
Milestone 3 stays open.

### Plan 063 Java I2P stripped-router direct NTCP2 driver

Plan 063 implements the source-locked Java I2P 2.12.0 stripped-router
direct NTCP2 driver. The driver is **test-only** and never becomes a
production dependency of `i2pr-daemon`. It uses the upstream
embedded `net.i2p.router.Router` and `RouterContext` with the pinned
dummy facades, the real NTCP/NTCP2 transport, the real outbound
message pool, and the real inbound message pool. Plan 063 does not
patch NTCP2 cryptography, the Noise handshake, framing, or RouterInfo
signature verification.

The Plan 063 deliverables are:

- `tests/integration/ntcp2/reference-drivers/java/src/JavaNtcp2InteropDriver.java`
  — the source-locked Java driver with strict config validation,
  bounded `inspect`/`listen`/`dial` modes, real `OutNetMessage`
  DeliveryStatus submission, and structured `i2pr-reference-event-v1`
  emission.
- `tests/integration/ntcp2/reference-drivers/java/source-lock.json` —
  the source-lock record
  (`i2pr-java-helper-source-lock-v1`) binding the pinned Java
  revision, the helper source path, and the locked constraints.
- `tests/integration/ntcp2/reference-drivers/java/classpath-manifest.json`
  — the runtime classpath binding every pinned jar in
  `target/interop/cache/java_i2p/<tree>/lib/` to its purpose.
- `tests/integration/ntcp2/reference-drivers/java/build-manifest.schema.json`
  — the build-manifest schema
  (`i2pr-java-helper-build-manifest-v1`).
- `tests/integration/ntcp2/reference-drivers/java/build-driver.sh`
  and `run-driver.sh` — the offline build and runtime seams.
- `tests/integration/ntcp2/harness/java_direct_driver.py` — the
  Python harness adapter that binds every helper invocation into a
  Plan 062 v4 trigger record (`i2pr-reference-trigger-v4`) and
  validates the Plan 063 strict driver config contract.
- `tests/integration/ntcp2/harness/test_java_direct_driver.py` and
  `test_java_direct_control.py` — the Plan 063 test matrix
  covering the source-verification contract, strict config
  contract, Python harness adapter, structured event contract, and
  the local inspect-mode round-trip where the pinned Java cache is
  available.
- `tests/integration/ntcp2/qualification/java-direct-driver.json` —
  the Plan 063 qualification receipt
  (`i2pr-java-direct-driver-qualification-v1`). On this host the
  receipt records the typed host-environment blocker
  (`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
  qualification remains to be produced in the Plan 046 rootless
  sealed-namespace lane or the Plan 048/049 Multipass recovery lane.

The repository remains NTCP2-experimental and non-advertised;
Milestone 3 stays open until Plan 065 closes with one complete
four-direction live diagnostic bundle and Plan 066 produces a
verified Milestone 3 certificate. Plan 063 does not wire the Java
driver into the canonical primary `mixed_runner.py`; that wiring
belongs to Plan 065. The Plan 063 closure record is in
`plans/063-status.md`.

### Plan 064 i2pd direct NTCP2 driver and observer correction

Plan 064 replaces the partial Plan 059 i2pd direct connect helper
with a correctly initialized, dual-mode, source-locked i2pd 2.60.0
NTCP2 interoperability driver. The driver is **test-only** and
never becomes a production dependency of `i2pr-daemon`. It uses
the real pinned NTCP2 transport implementation, performs the
source-verified pinned initialization sequence, imports one exact
peer RouterInfo directly, sends one real `CreateDeliveryStatusMsg`
in dial mode, and acts as a real NTCP2 listener in listen mode.
Plan 064 also includes a compile-time-gated passive observer
after successful AEAD decryption and I2NP conversion plus an
uninstrumented control build that proves the observer does not
alter transport success.

Plan 064 explicitly eliminates the eight documented defects of the
Plan 059 helper:

- `D1` — Router Hash is now a 32-byte SHA-256 `IdentHash` encoded
  as 64 lowercase hex characters; the 40-hex SHA-1 contract is
  rejected.
- `D2` — The transport static key is selected from the NTCP2
  `RouterAddress` used for the target endpoint and hashed from its
  `s` field; the SSU2 accessor cannot satisfy the validation.
- `D3` — Initialization is the source-verified pinned sequence
  (`config::Init` → `context::ParseConfig` → `fs::SetAppDir` →
  `crypto::Init` → `context::Init` → transport singleton → `netdb.Start`
  → `transports.Start(true, false)` → `context.Start`); shutdown is
  the strict reverse order.
- `D4` — Dial mode constructs a real `CreateDeliveryStatusMsg`
  and submits through `Transports::SendMessage` exactly once; the
  null-message trigger is removed.
- `D5` — Initial null session is no longer classified as final
  failure; the driver waits boundedly for the established
  `TransportSession` state and exact sender observer completion.
- `D6` — Reserved-range rejection is disabled through the rendered
  i2pd configuration for the sealed synthetic topology; every
  other target validation remains in force.
- `D7` — The passive observer reports the exact decoded
  DeliveryStatus message ID and peer Router Hash after AEAD
  verification and FromNTCP2 conversion; generic log phrases
  cannot satisfy the receive path.
- `D8` — Every source, patch, compiler, library, and binary input
  is bound by measured SHA-256 digests in `source-lock.json` and
  the build manifest; all-zero or placeholder digests fail closed.

The Plan 064 deliverables are committed under
`tests/integration/ntcp2/reference-drivers/i2pd/`:

- `src/i2pd_ntcp2_interop_driver.cpp` — the source-locked C++
  driver with strict config validation, bounded `inspect` /
  `listen` / `dial` modes, real `CreateDeliveryStatusMsg`
  submission, and structured `i2pr-reference-event-v1` emission.
- `src/interop_observer.h` and `src/interop_observer.cpp` — the
  compile-time-gated passive observer API and sink.
- `patches/i2pd-2.60.0-interop-observer.patch` — the minimal
  observer patch that activates the post-AEAD receive seam and
  the successful frame-write send seam.
- `CMakeLists.txt`, `build-driver.sh`, `run-driver.sh`,
  `build-manifest.schema.json`, and `source-lock.json` — the
  build contract, the offline build seam, the runtime seam, the
  build-manifest schema, and the source-lock record binding every
  artifact SHA-256.
- `README.md` — the driver README documenting the call graph, the
  strict config contract, the observer design, the behaviour-
  neutrality contract, and the Plan 064 controls.

The harness adapter and test matrices live under
`tests/integration/ntcp2/harness/`:

- `i2pd_direct_driver.py` — the Python harness adapter that binds
  every helper invocation into a Plan 062 v4 trigger record
  (`i2pr-reference-trigger-v4`) and validates the Plan 064 strict
  driver config contract. The adapter never reaches inside the C++
  helper state and never synthesises a passing record.
- `test_i2pd_direct_driver.py` and `test_i2pd_direct_control.py` —
  the Plan 064 test matrices covering the source-verification
  contract, strict config contract, Python harness adapter,
  structured event contract, observer compile-time gating, the
  Plan 059 supersedure, and the typed host blocker.

The qualification receipt lives at
`tests/integration/ntcp2/qualification/i2pd-direct-driver.json`
(schema `i2pr-i2pd-direct-driver-qualification-v1`). On this host
the receipt records the typed host-environment blocker
(`blocked_unprivileged_user_namespace`); the 10/10 fresh-state
qualification remains to be produced in the Plan 046 rootless
sealed-namespace lane or the Plan 048/049 Multipass recovery
lane.

The Plan 064 source-verification record addition lives in
`tests/integration/ntcp2/reference-drivers/source-verification.md`
under the Plan 064 i2pd topology contract section.

The Plan 064 helper does not wire the i2pd driver into the
canonical primary `mixed_runner.py`; that wiring belongs to Plan
065. The legacy Plan 059 helper at
`tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`
is replaced by a fail-closed compatibility stub with the explicit
Plan 064 supersedure marker; the original source-lock record is
preserved verbatim as the bounded historical-reader path. The
repository remains NTCP2-experimental and non-advertised;
Milestone 3 stays open until Plan 065 closes with one complete
four-direction live diagnostic bundle and Plan 066 produces a
verified Milestone 3 certificate.

### Plan 076 real pinned i2pd library and direct driver construction

Plan 076 replaces the Plan 064 terminal-stub helper with a real
source-locked i2pd 2.60.0 test executable that links against the
unmodified pinned i2pd 2.60.0 libraries built from the pinned
CMake project. The Plan 076 implementation surface is mandatory
for the canonical mixed-router lane in Plan 065.

Plan 076 explicitly eliminates the six documented defects
(`P1`-`P6`) from the Plan 064 implementation surface:

- `P1` — `CMakeLists.txt` linked headers but did not compile or
  link the actual pinned i2pd library targets. Plan 076 builds
  `libi2pd`, `libi2pdclient`, `libi2pdlang` from the pinned i2pd
  CMake project with `WITH_LIBRARY=ON` and `WITH_BINARY=OFF`,
  then links the driver against the produced archives.
- `P2` — `I2PD_PLAN076_LINKED` was not defined by the build. Plan
  076 defines the marker through the driver CMake project; the
  runtime `pinned_libraries_linked()` gate fails closed with
  exit 66 when the marker is absent.
- `P3` — `run_listen()` and `run_dial()` were terminal rejection
  stubs. Plan 076 implements both with the real i2pd NTCP2
  transport.
- `P4` — Inspect mode did not prove real i2pd initialization or
  RouterInfo production. Plan 076 inspect mode initialises the
  full i2pd context, captures the local Router Hash from
  `i2p::context.GetIdentity()->GetIdentHash()`, and emits a
  `router_info_exported` event carrying the measured hash.
- `P5` — Build manifests described linked i2pd behaviour the
  current binary did not contain. Plan 076 records the measured
  SHA-256 of every linked i2pd archive under
  `i2pd_libraries_sha256`; the build script refuses to write a
  manifest that omits these digests.
- `P6` — A control binary that omits observer calls was not
  sufficient unless both binaries execute the same genuine
  transport path. Plan 076 builds both binaries from the same
  pinned tree via the `I2PD_PATCHED_TREE` / `I2PD_PRISTINE_TREE`
  / `I2PD_LIB_DIR` CMake cache variables; `nm` confirms the
  instrumented binary has the observer call sites and the control
  binary has zero reachable observer call sites.

The Plan 076 closure boundary does **not** require a mixed-router
pass; the closure is a real binary with verifiable source linkage
and locally testable inspect / listen / dial behaviour. On this
host (the Plan 046 `apparmor_restrict_on` negative baseline) the
qualification receipt records the typed host blocker and an
all-zero attempt count. NTCP2 stays experimental and
non-advertised.

### Plan 065 NTCP2 canonical integration and live qualification

Plan 065 wires the corrected Java and i2pd direct drivers into the
canonical four-direction mixed-router lane, enforces the exact
DeliveryStatus correlation on the i2pr side, and produces one
complete four-direction live diagnostic bundle from a clean
implementation commit. Plan 065 establishes the implementation
floor from which Plan 066 may cut a candidate.

The Plan 065 implementation delivers:

- `tools/i2pr-interop/src/scenario.rs` — the strict scenario schema
  bumped to `i2pr-launcher-scenario-v2` with the per-run
  DeliveryStatus `message_id`, the 64-lowercase-hex expected sender
  and receiver Router Hashes, the `reference_driver_mode` field, and
  the `run_identity_sha256` field. Legacy schema 1 records are
  rejected by the strict parser.
- `tools/i2pr-interop/src/main.rs` — the i2pr sender uses the
  scenario-owned message ID and verifies the round-trip envelope
  message ID and the DeliveryStatus payload message ID before frame
  emission. The i2pr receiver requires the exact envelope and payload
  message ID, rejects duplicates, and emits the bounded Plan 065
  typed failure categories (`SenderDeliveryStatusMessageIdZero`,
  `SenderRouterIdentityMismatch`,
  `SenderDeliveryStatusConstructionFailed`,
  `SenderFrameQueueAmbiguous`, `SenderFrameWriteFailed`,
  `SenderMultiplePrimaryDeliveryStatusEmitted`,
  `SenderCancellationObserved`, `ReceiverFrameReadFailed`,
  `ReceiverFrameAuthenticationFailed`, `ReceiverI2npDecodeFailed`,
  `ReceiverDeliveryStatusMissing`,
  `ReceiverDeliveryStatusIdMismatch`,
  `ReceiverDeliveryStatusDuplicate`,
  `ReceiverPeerIdentityMismatch`,
  `ReceiverDeliveryStatusTimestampInvalid`). The hard-coded
  `0x0420_0001` DeliveryStatus authority is removed.
- `tools/i2pr-interop/src/status.rs` — the status counter carries
  the per-run DeliveryStatus `message_id` and the expected peer
  Router Hash. The typed failure categories are added to the bounded
  `StatusReason` allowlist.
- `tests/integration/ntcp2/harness/launcher_protocol.py` and
  `tests/integration/ntcp2/harness/launcher_renderer.py` — the Python
  strict scenario schema and renderer mirror the Rust schema with the
  same `i2pr-launcher-scenario-v2` marker, the same required primary
  fields, the same 64-hex Router Hash contract, and the same
  `reference_driver_mode` allowlist. The strict parser rejects SAM,
  HTTP, I2PControl, support-topology, and synthetic-fallback helpers
  for any primary direction.
- `tests/integration/ntcp2/harness/mixed_runner.py` — the canonical
  mixed-runner wires the new scenario primary fields through
  `render_and_validate` for both the i2pr initiator and responder
  paths. The `_plan065_primary_fields` helper derives the
  DeliveryStatus `message_id` from the run identity and the
  correlation nonce; the `_reference_driver_mode_for` helper returns
  the source-locked driver mode for a reference kind. The runner
  refuses to fall back to SAM, HTTP, support-topology, or synthetic
  helpers for a primary direction.
- `tests/integration/ntcp2/harness/test_plan065.py` — the Plan 065
  test matrix covering scenario v2 acceptance and rejection (zero
  message ID, 40-hex Router Hash, unknown reference driver mode,
  direction-helper mismatch), DeliveryStatus message ID derivation
  uniqueness, status counter contract (correlation counters,
  invalid message ID, invalid peer Router Hash), reference trigger
  v4 correlation, observation v3 correlation, pass predicate exact
  message ID and Router Hash correlation, support-router rejection,
  Plan 060 candidate retirement, and the Plan 066 implementation
  floor marker.
- `scripts/check-ntcp2-interoperability.sh` — the static boundary
  checker enforces the Plan 065 schema marker, the required primary
  fields, the bounded typed failure categories, the absence of the
  hard-coded `0x0420_0001` DeliveryStatus authority, and the
  Plan 065 test matrix existence.

The repository remains NTCP2-experimental and non-advertised;
Milestone 3 stays open until Plan 065 closes with one complete
four-direction live diagnostic bundle and Plan 066 produces a
verified Milestone 3 certificate. NTCP2 remains experimental and
non-advertised.

### Plan 066 fresh-candidate and authoritative NTCP2 two-run closure pass

> **Supersession notice (Plan 068, ADR 0023 Accepted).** Plan 066 is
> the historical record of the failed release-qualification
> environment on the constrained host. Plan 067 is the active
> Milestone 3 roadmap. The Plan 066 implementation surface remains
> mandatory as an audit record, but the Plan 066 two-run certificate
> is no longer the active gate for the first external protocol run;
> that role belongs to Plan 069 under ADR 0023. The historical text
> below is preserved verbatim.

Plan 066 is the execution-only pass that cuts one fresh candidate
descended from the Plan 065 implementation floor, selects exactly
one execution lane (direct-host or guest), runs the four primary
IPv4 mixed-router directions twice on independent mutable state,
and produces a verified Milestone 3 certificate over the two
sanitized bundles. The Plan 066 plan-of-record is
`plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md`.

The plan cannot start under the current four-direction contract
until either a future pinned Java revision is adopted or the
closure contract is revised through a new ADR (because ADR 0021 is
Rejected by Plan 058). The Plan 046 rootless sealed-namespace
lane returns `blocked_unprivileged_user_namespace` on this host;
the Plan 048/049 Multipass recovery lane is the canonical external
path but cannot complete on this constrained host (per Plan 051).
Plan 066 therefore closes on this host with the typed environment
blocker `blocked_execution_lane_unavailable`; the candidate is
`declared-not-executable`.

The Plan 066 implementation surface is mandatory:

- `tests/integration/ntcp2/harness/plan066.py` — the Plan 066
  helper module. Exports the typed blocker
  (`blocked_execution_lane_unavailable`), the close-status
  classifier (`declared-not-executable`), the
  `plan066_execution_lane_lock(...)` lane-lock helper for the
  Plan 058/060 two-lane contract, the 23-row candidate-record
  digest table, the freeze-readiness checklist, the
  `assert_plan066_freeze_invariants` enforcer, the
  `plan066_directional_record` per-direction skeleton, the
  cross-bundle independence checker
  (`plan066_two_bundle_independence`), and the bundle mutation
  guard (`plan066_finalized_bundle_marker`).
- `tests/integration/ntcp2/harness/test_plan066.py` — the Plan 066
  test matrix (41 cases covering the 30 enumerated Plan 066
  Phase 12 cases plus the typed-blocker, freeze-readiness,
  helper-contract, and Plan 065 plan-of-record helpers).
- `scripts/check-ntcp2-interoperability.sh` enforces the Plan 066
  artifacts, the Plan 066 test matrix coverage, and the
  candidate/closure marker invariants.
- `plans/066-candidate.md` — the Plan 066 candidate record
  (status `declared-not-executable`).
- `plans/066-closure.md` — the Plan 066 closure record with the
  typed blocker and the close-status.

A future pinned Java revision that exposes a transport-only
direct seam may trigger an ADR re-issue that supersedes the ADR
0021 rejection and unblocks the `java-to-i2pr-ipv4` direction.
Until then, NTCP2 stays experimental and non-advertised and
Milestone 3 stays open.

### Plan 067 staged interoperability corrective roadmap

Plan 067 is the **active** Milestone 3 corrective roadmap. Plan 067
supersedes Plan 066 as the active execution authority. Plan 066
remains an immutable historical record of the unavailable
release-qualification lane on the constrained host.

Plan 067 separates NTCP2 interoperability evidence into four bounded
tiers: local-conformance (Level 0), external-loopback-smoke
(Level 1), repeated-development-interop (Level 2),
conditional-differential (Level 2D), and release-qualification
(Level 3). Level 1 and Level 2 are host-compatible manual
integration lanes; they require the pinned i2pd driver (and
optionally the pinned Java direct driver when available) but they
do not require a rootless namespace, a Multipass guest, a frozen
candidate, a two-bundle certificate, or a reviewer record.
Emissary is the conditional secondary implementation. Java and i2pd
remain required for Level 3 release qualification.

### Plan 068 staged evidence and authority correction

Plan 068 implements the staged-evidence and authority correction.
Plan 068 lands:

- `docs/adr/0023-staged-ntcp2-interoperability-evidence.md`
  (Accepted). ADR 0023 separates evidence into four bounded tiers
  and forbids lower-tier promotion into release bundles.
- `tests/integration/ntcp2/harness/evidence_tier.py` — the
  evidence-tier constants and tier-separation rules.
- `tests/integration/ntcp2/harness/loopback_smoke_record.py` —
  the Level 1 smoke record schema
  (`i2pr-ntcp2-loopback-smoke-v1`).
- `tests/integration/ntcp2/harness/development_validation.py` —
  the Level 2 development-validation summary schema
  (`i2pr-ntcp2-development-validation-v1`).
- The Plan 068 test matrices (`test_evidence_tier.py`,
  `test_loopback_smoke_record.py`,
  `test_development_validation.py`).
- The static boundary checker
  (`scripts/check-ntcp2-interoperability.sh`) now enforces the new
  schemas and rejects lower-tier records in release bundles while
  leaving the historical plan surfaces and freeze-readiness
  invariants intact.

Plan 068 also removes the stale `blocked_java_support_topology_rejected`
interpretation from the active Java path: ADR 0021 remains Rejected
and the Java support topology remains forbidden, but the ADR 0022
direct Java driver is the active Java architecture. Java may still be
unavailable because of host/runtime/build defects, but not because
ADR 0021 forbids the already accepted replacement architecture.

The focused closure baseline for Plans 069-073 is the touched-code
test suite plus `cargo fmt --all --check`, `cargo check --workspace
--all-targets`, `cargo test --workspace`,
`scripts/check-dependency-direction.sh`, and
`scripts/check-runtime-boundaries.sh`. Full historical harness
matrices, rootless checks, and Multipass checks remain available for
explicit integration checkpoints but are not required for Level 1
or Level 2 closures.

### Plan 069 host-compatible NTCP2 loopback smoke lane

> **Reclassification (Plan 074, supersession note).** Plan 069
> implements the Plan 067 Level 1 host-loopback smoke runner and its
> static boundary check, but at the time Plan 074 was registered the
> runner was scaffolding/fake-process coverage only. The Plan 069
> runner selected the i2pr launcher for both process handles, did not
> invoke the supplied i2pd binary as the reference process, and could
> promote protocol milestones without consuming real structured
> reference events. The Plan 064 i2pd helper's listen/dial paths were
> terminal stubs when real pinned i2pd libraries were not linked. The
> Plan 069 closure record (`plans/069-status.md`) is preserved as a
> snapshot of that scaffolding state. Plan 075 is the runner integrity
> and evidence correction pass; the Plan 069 lane is not valid
> mixed-router evidence until Plan 075 closes.

Plan 069 implements the Plan 067 Level 1 host-loopback smoke lane.
The lane is a non-production composition that exercises a single
two-process NTCP2 direction (one i2pr launcher process, one Plan 064
i2pd direct driver process) on the host loopback, without sudo,
namespaces, Multipass, or any public-network access. The runner is
structurally incapable of producing a Level 3 release bundle or
certificate. A passed Level 1 record satisfies the Plan 068 smoke
schema (`i2pr-ntcp2-loopback-smoke-v1`, evidence tier
`external-loopback-smoke`); it never satisfies a release-qualification
predicate.

Plan 069 lands:

- `tests/integration/ntcp2/harness/loopback_smoke.py` — the runner
  module. Owns the strict CLI/config parser, the run-root lifecycle,
  the loopback port allocator, the Plan 065 strict scenario
  renderer, the Plan 064 strict driver config builder, the
  listener/dialer process ownership and cleanup, the network-audit
  probe (strace-allowlist or configuration-only), the failure-stage
  classifier, and the Plan 068 smoke record writer. The runner
  must not import or call Plan 056/066 candidate, bundle,
  certificate, rootless-topology, or Multipass authority.
- `scripts/interop/run-ntcp2-loopback-smoke.sh` — the thin shell
  entry point. The wrapper must never invoke sudo, namespaces,
  containers, VMs, or public-network access.
- `tests/integration/ntcp2/harness/test_loopback_smoke.py` — the
  Plan 069 test matrix (42 cases) covering the strict config
  parser, the failure staging, the cleanup contract, the
  network-audit degradation, the listener-before-dialer ordering,
  the exact DeliveryStatus correlation, the typed-blocker rules,
  the runner ownership invariants, and the static shell wrapper
  contract.
- `scripts/check-ntcp2-loopback-smoke-boundary.sh` — the static
  Plan 069 boundary check. Verifies the runner/shell/test artifacts
  are present, the allowlist markers are committed, and the runner
  is free of release/rootless/Multipass authority.
- `plans/069-status.md` — the closure record with exact commands,
  results, and no fabricated live pass.

Plan 069 does not claim mixed-router interoperability by itself; the
implementation surface is **scaffolding and fake-process test coverage
only** under Plan 074 until Plan 075 restores direction-aware process
roles, structured reference events, measured provenance, and
fail-closed guards. Plan 069 also does not modify production NTCP2
code or the i2pd direct driver.

### Plan 090 i2pd RouterInfo and Plan 087 evidence corrective pass

Plan 090 closes the Plan 087 zero-address `router.info` defect.
The Plan 087 instrumented attempt reached `listener_ready` and
then the i2pr dialer rejected the i2pd `router.info` with
`peer_router_info_invalid` because the i2pd direct driver's
emitted buffer contained zero `RouterAddress` entries. The root
cause was a type-and-storage defect in the driver's
`initialise_i2pd_runtime` rather than a transport defect.

Plan 090 applies four behavior-neutral corrections in the i2pd
direct driver
(`tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`):

- `set_bool_option("ntcp2.published", true)` — store the option
  as `bool` (was stored as `int` by the Plan 064 helper, which
  silently failed to update the i2pd `boost::program_options` map
  because `value<bool>()->default_value(true)` rejects an `int`
  payload and silently no-ops). With `ntcp2.published = false`,
  `RouterContext::NewRouterInfo()` (libi2pd/RouterContext.cpp
  lines 152-157) takes the non-published branch and the NTCP2
  address is serialized without `host`, `port`, or `i`.
- `i2p::config::ParseCmdline(1, fake_argv, ignoreUnknown=true)` +
  `Finalize()` — populate the i2pd option store with declared
  defaults before the driver mutates individual options. Without
  this, `SetOption` calls silently no-op because `m_Options` is
  empty until `store()` runs.
- `set_uint16_option` helper — store `port` and `ntcp2.port` as
  `uint16_t` (was stored as `int`, which throws
  `boost::bad_any_cast` on extraction because both options are
  registered as `value<uint16_t>()` in i2pd Config.cpp lines 63
  and 331).
- `i2p::transport::transports.SetCheckReserved(false)` — disable
  reserved-range filtering so loopback addresses survive
  `RouterInfo::ReadFromBuffer` deserialization
  (RouterInfo.cpp lines 256-262 strip the `host` field for any
  IP in the reserved range).

The driver also fails closed with
`router-info-endpoint-mismatch` if the authoritative in-memory
RouterInfo does not carry the exact configured NTCP2 endpoint.

Plan 090 also corrects the Plan 083 pre-TCP classification: the
canonical runner now forbids generic `protocol_rejected` /
`reference-events-missing` before `tcp_connected` and serializes
pre-TCP failures as `pre_protocol_rejected` with the bounded
pre-protocol reason allowlist. Plan 083 host-loopback
`validate-scenario` is routed through
`HostLoopbackDevelopmentPlacement.run` so the runner never
composes a shell or namespace wrapper.

Plan 090 lands:

- `tests/integration/ntcp2/harness/plan083_runner.py` —
  pre-TCP classifier, placement-owned scenario validation,
  typed pre-protocol reason allowlist, and the bounded
  pre-protocol reject path.
- `tests/integration/ntcp2/harness/test_plan090.py` — the Plan
  090 test matrix (14 cases) covering the source verification,
  driver binary, control parity, pre-TCP classification,
  placement validation, and record validation.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  — the "Plan 090 verified RouterInfo lifecycle" section
  documents the pinned-source lifecycle and config/export
  ownership.
- `scripts/check-ntcp2-interoperability.sh` — enforces the Plan
  090 driver corrections, lifecycle documentation, and test
  matrix presence.

The Plan 090 corrections landed on this host. The first clean
committed-head forward attempt authenticated the i2pd listener
and reached TCP, then the NTCP2 Noise handshake closed the
socket with `Io(ExactIoError { kind: Closed })` before the i2pr
initiator reached `ntcp2_authenticated`. The Plan 090 closure
remains open: the forward direction did not pass. Per the Plan
090 "Forward attempt reaches TCP and fails protocol" branch, the
failed record is preserved and Plan 088 is not allowed to run
until the forward direction passes. See
[plans/087-status.md](plans/087-status.md) and
[plans/088-status.md](plans/088-status.md) for the closure
records. NTCP2 stays experimental and non-advertised.

### Plan 091 forward NTCP2 Noise-handshake corrective pass

Plan 091 closes the bounded Plan 087 forward-direction Noise
handshake work without broadening the Milestone 3 scope. It
lands four i2pd direct driver preconditions and an i2pr
launcher `tcp_connected` emission, retains an evidence-only
reproduction of the open failure, and classifies the
forward direction for the Plan 088 development decision
without claiming a pass.

Plan 091 lands four more i2pd direct driver corrections
(`tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp`):

- `i2p::context.SetNetID(cfg.network_id)` between
  `i2p::crypto::InitCrypto(false)` and `i2p::context.Init()` —
  the i2pd standalone daemon performs the same call; without
  it `RouterContext::GetNetID()` returns the default
  `I2PD_NET_ID` (=2) and the NTCP2 listener rejects the
  SessionRequest with `networkID 99 mismatch. Expected 2`.
- `i2p::log::Logger().SendTo(<data_dir>/i2pd.log)` plus
  `Logger().Start()` after `InitCrypto` and `Logger().Stop()`
  in `main()` before each `run_*` return. Without the
  explicit `Start` the global `Log::Log` thread is not
  running and `LogPrint` calls in the i2pd transport are
  no-ops; the `Stop` in `main()` joins the background
  thread and prevents the `terminate called without an
  active exception` abort on shutdown.
- `run_listen` waits boundedly for the i2pd transport to
  record a real TCP accept through the Plan 064 observer
  (`WaitForTcpAccepted`) and emits a `tcp_accepted` event
  before continuing; without it the i2pd listener reported
  `ntcp2_authenticated` only on stale observer slots from a
  previous run.
- `run_listen` composes a `DeliveryStatus` with the exact
  correlation `message_id` and submits it through the real
  i2pd transport (`transports.SendMessage(peer_ident_hash,
  reply)`); without it the i2pr's `receive_delivery_status`
  block reports `receiver_delivery_status_missing` and the
  Plan 065 directional predicate cannot pass.

The Plan 091 corrections landed on this host. The first
clean committed-head forward attempt authenticated the i2pd
listener, the i2pr dialer started, the i2pd NTCP2 transport
accepted the TCP connection, the i2pd log shows
`NTCP2: SessionRequest read error: End of file` (the i2pd
transport read zero bytes on the first body read), and the
i2pr status log shows `tcp_connected` immediately followed by
`terminal, result: rejected, reason_code:
receiver_delivery_status_missing`. The wire trace, the i2pd
log, and the i2pr status do not yet agree on which side
terminates the Noise handshake. The retained record is
preserved at
`/tmp/opencode/plan091-evidence/forward/forward-record.json`;
see [plans/091-status.md](plans/091-status.md) for the
closure record, exact live command, recorded digests, and the
ownership analysis. Plan 088 remains blocked on a
follow-up ownership pass under a successor plan. NTCP2 stays
experimental and non-advertised.

### Plan 092 forward-handshake evidence integrity and ownership closure (superseded)

Plan 092 delivered the privacy-safe handshake stage observation
schema, the i2pr runtime observed handshake driver entry points,
the terminal-counter preservation in the i2pr launcher, the
Plan 083 event-ingestion repair with current-run dedup, the
dedicated Plan 092 regression matrix, and the static boundary
check extensions that enforce the privacy-safe observation
schema. The retained record
(`/tmp/opencode/plan092-test-1/forward-record.json`,
`record_sha256 = 696aa1339d3d950f9fec2a2e0b1f5bede2035761a71e167af6ab28b249cc998d`)
preserves the first clean committed-head reproduction. Plan 092
is **superseded by Plan 093** — its Branch A (i2pr runtime /
state-machine defect) ownership analysis is corrected by the
Plan 093 source reclassification that names the i2pd log
diagnostic as data-phase length reader traffic rather than a
handshake-state defect. Plan 092 remains the immutable
diagnostic history; Plan 093 is the active implementation
authority.

### Plan 093 Plan 087 forward data-phase and reference-observer closure (superseded)

Plan 093 implementation landed on this host but the closure is
**incomplete**. Plan 094 lands the runner/provenance authority
corrections but the live closure environment is blocked on this
host. Plan 095 is the **active single next executable plan**: the
GitHub Actions `ubuntu-24.04` host-loopback live-wire closure lane
that supersedes the local host environment-blocked path that Plan
094 expected to run. Plan 093 corrected the Plan 092 "Branch A i2pr
state-machine defect" misclassification with a privacy-safe
source reclassification of the i2pd NTCP2 log diagnostic,
implemented the i2pd observer reset/generation/ring contract,
shipped the i2pr bounded multi-frame receive oracle, bound the
i2pr binary provenance into the live wrapper, and introduced
the runner event authority gate.

Plan 094 closes Plan 093 without reopening its already-landed
NTCP2 data-phase design. Plan 094 proves or corrects the
canonical Plan 083 event identity and ingestion contract, makes
the forward pass classification require exact target metadata,
binds the i2pr build manifest into the probe record, prunes the
stale `plan_093b` token from the active vocabulary, and runs the
required static surface before any live attempt. Plan 095 adds
the dedicated CI workflow, the focused test matrix, the bounded
CI environment blocker vocabulary, and the sanitized CI gate
record. Plan 088 remains blocked until Plan 095 closes with a
passing instrumented and a passing control forward record from
the same CI evidence pair. NTCP2 stays experimental and
non-advertised.

### Plan 095 CI host-loopback live-wire evidence lane

Plan 095 is the **active single next executable plan** and the
authoritative forward-direction closure pass. The plan implements
the GitHub Actions `ubuntu-24.04` host-loopback live-wire
evidence lane that runs the Plan 086 `host-loopback-development`
topology on a fresh VM, with the contract/build/forward-instrumented/
forward-control/validate-gate job sequence, provenance-bound
binaries, and a sanitized CI gate record. The lane is
**development-only**; it never satisfies a release or isolation
qualification and cannot become a Milestone 3 certificate.

Plan 095 implements:

- `.github/workflows/ntcp2-interop-host-loopback-development.yml`
  — the dedicated manual CI workflow. The `workflow_dispatch`
  trigger is the only initial trigger; no `pull_request` automatic
  execution. Permissions are `contents: read`. The live jobs
  (`forward-instrumented`, `forward-control`, `validate-gate`) run
  on `ubuntu-24.04` and never invoke sudo, ip netns, nft, iptables,
  unshare, `--privileged`, `--network host`, multipass, or docker.
  The build job may install declared packages via `apt-get` only.
- `tests/integration/ntcp2/harness/test_plan095.py` — the Plan 095
  test matrix that statically enforces the workflow contract, the
  build/manual gates, the live-path prohibition list, the
  controlled CI environment blocker vocabulary, and the bounded
  artifact upload allowlist.
- `target/interop/evidence/plan095-ci-gate.json` — the sanitized
  Plan 095 CI gate record schema
  (`i2pr-ntcp2-plan095-ci-gate-v1`) that binds the workflow run
  metadata to the two passing evidence record digests and the
  implementation commit.

Plan 095 supersedes the Plan 094 assumption that the Plan 046
rootless sealed-namespace lane or the Plan 048/049 Multipass
guest must become runnable before development-only forward evidence
can close. Those lanes remain valid historical/qualification lanes
but are **not prerequisites** for the `host-loopback-development`
evidence needed to advance the current roadmap. The static
contract test (`scripts/check-ntcp2-interoperability.sh`) is
extended by the workflow contract test to enforce the Plan 095
artifacts, the schema tokens, the plan-of-record reference, and
the post-Plan-094 status authority.

The Plan 095 result feeds Plan 088 only after a passing CI
evidence pair proves the forward direction. Plan 088 remains
blocked pending the actual two-way Plan 088 decision. Plan 079
remains blocked pending the Plan 088 two-way pass. Plan 072
remains inactive pending the Plan 088 ambiguity decision. NTCP2
remains experimental and non-advertised. The plan-of-record is
`plans/095-ci-host-loopback-live-wire-evidence-lane.md`.

### Plan 096 Plan 095 CI workflow correctness and pre-dispatch closure

Plan 096 is the active workflow correctness and pre-dispatch
closure pass. The plan is a narrow corrective change to the Plan
095 GitHub Actions workflow (`Plan 095 manual`) so the first
authoritative live run is execution-correct and statically
verifiable before any manual dispatch.

The plan delivers:

- `tests/integration/ntcp2/harness/test_plan096.py` — the Plan 096
  regression matrix (36 cases) that rejects the pre-correction
  workflow on i2pr build path ambiguity, sanitized evidence nested
  inside the disposable run root, the embedded Python `os` import
  defect, and the filesystem-wide i2pd source digest that
  includes `.git` administrative files. The matrix also enforces
  the dependency graph, the fail-closed live-attempt semantics,
  the disjoint build/evidence artifact trust boundaries, and the
  plan 095/088/079/072 gate preservation.
- `scripts/check-plan095-workflow.sh` — the pre-dispatch audit
  script. It is invoked by `scripts/check-ntcp2-interoperability.sh`
  before the rest of the static surface. The audit returns
  nonzero on any of the four demonstrated defects.

The four demonstrated workflow corrections:

1. The i2pr Cargo invocation now uses an explicit
   `--manifest-path "${GITHUB_WORKSPACE}/Cargo.toml"` and an
   explicit `--target-dir` variable. The downstream binary is
   copied from the explicit target directory and is asserted
   regular, executable, and non-symlink before hashing.
2. The instrumented and control sanitized evidence trees are
   disjoint from the disposable run roots. The plan moves them
   to `target/interop/plan095-evidence/instrumented` and
   `target/interop/plan095-evidence/control`. The cleanup step
   deletes only the disposable run root and asserts the
   sanitized evidence still exists before artifact upload.
3. Every embedded Python heredoc is audited for missing imports;
   the known control validator now imports `os` so the
   `os.environ` reference resolves. The audit uses a structural
   Python check that mirrors the static test matrix.
4. The i2pd source digest uses `git -C i2pd ls-files -z` over
   the pinned tracked tree. The pinned revision equality and the
   worktree-dirty check are asserted before the digest is
   computed.

After Plan 096 lands, the current status of the active sequence
is:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_096 = passed-pre-dispatch-workflow-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
```

Plan 095 is the single next executable plan. Exactly one manual
Plan 095 GitHub Actions dispatch follows the Plan 096 correction.
The plan-of-record is
`plans/096-plan095-ci-workflow-correctness-and-pre-dispatch-closure.md`.

### Plan 097 Plan 095 artifact-path and cleanup corrective pass

Plan 097 is the active narrow corrective pass over the Plan 095
GitHub Actions workflow that closed two workflow defects that
remained after Plan 096. Plan 097 does **not** dispatch a Plan 095
live run; it only restores execution correctness so the next
manual dispatch can produce a usable evidence pair.

The two demonstrated workflow defects closed by Plan 097:

1. **Artifact-path ownership (Defect A).** The
   `build-i2pr-interop` step wrote the i2pr binary to a
   CWD-relative `output/i2pr-interop` while the
   `hash-i2pr-build-manifest` and `verify-build-artifacts`
   steps consumed from `${BUILD_DIR}/output/i2pr-interop` after
   a step-local `cd "$BUILD_DIR"`. Producer and consumer
   identities did not match; the manifest would have hashed a
   file that did not yet exist at the consumer's path. Plan 097
   defines one canonical absolute `BUILD_OUTPUT` path used by
   every producer, verifier, manifest generator, artifact
   uploader, and live consumer. No step relies on inherited
   step working directory to establish artifact identity.
2. **Disposable run-root cleanup (Defect B).** The cleanup
   used `find $RUN_ROOT -mindepth 1 -delete` (descendant-only)
   plus `test ! -e "$RUN_ROOT" || true` (suppressed absence
   assertion). The root directory could survive cleanup while
   the job claimed the cleanup is clean. Plan 097 replaces the
   descendant-only deletion with strict `rm -rf -- "$RUN_ROOT"`
   after an exact `case` path guard, and removes every
   suppression from the post-cleanup absence assertion.

Plan 097 lands:

- `tests/integration/ntcp2/harness/test_plan097.py` — the Plan
  097 regression matrix (45 cases) that rejects the pre-Plan-097
  workflow on the two defects and exercises the canonical
  absolute `$BUILD_OUTPUT` path identity, the exact path guard
  before `rm -rf`, the unsuppressed absence assertion, and the
  synthetic mutation tests that prove the regression surface
  catches the prior defective semantics on synthetic fixtures.
- `scripts/check-plan095-workflow.sh` — extended to reject both
  Plan 097 defects. The audit now fails closed when the workflow
  writes to a relative `output/i2pr-interop` destination,
  relies on a relative output directory, omits the canonical
  `$BUILD_OUTPUT` variable, omits the cleanup path guard, or
  retains the suppressed absence assertion.
- Status authority corrections in `plans/087-status.md` and
  `plans/088-status.md`. The new `plan_097` token is
  `passed-artifact-path-and-cleanup-correction`. Plan 087
  remains open pending Plan 095 CI forward evidence pair. Plan
  088 remains blocked pending Plan 095 CI closure.
- Documentation propagation in `README.md`, `AGENTS.md`, the
  `i2pr-ntcp2-interop` skill, and
  `docs/architecture/interop-apparatus.md`.

After Plan 097 lands, the current status of the active sequence
is:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
```

Plan 095 remains the single next executable plan. Exactly one
manual Plan 095 GitHub Actions dispatch follows the Plan 097
correction commit. The plan-of-record is
`plans/097-plan095-artifact-path-and-cleanup-corrective-pass.md`.

### Plan 074 real-driver and constrained-host corrective roadmap (historical)

Plan 074 is historical execution authority. Plan 081 supersedes its active
sequence with **Plan 082 → Plan 083 → Plan 084 → Plan 079**. Plans 075, 076,
077, and 080 are closed prerequisites or historical lane records.

The corrected repository state is:

```text
plan_068_staged_evidence = implemented
plan_069_runner_scaffolding = historical
real_i2pd_driver = implemented
real_i2pd_library_linkage = present
real_reference_process_in_plan069_runner = corrected_by_plan075
real_mixed_router_attempts = 0
current_rootless_namespace_lane = unavailable
multipass_lane = qualified
support = experimental
advertised = false
normal_daemon_activation = disabled
```

The constrained-host lane decision and Plan 077 capability probe remain
historical records. Do not treat capability probing or the pre-protocol
Plan 078 stop as protocol evidence.

### Plan 075 Plan 069 runner integrity and evidence correction

Plan 075 corrects the Plan 069 runner so it is structurally
incapable of producing a mixed-router pass unless it launches one
real i2pr process and one configured real reference process and
consumes authentic structured events from both.

The corrected runner must:

- launch the reference role through the configured reference driver
  via `tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh`,
  not a second `i2pr-interop` process;
- bind every accepted event to a measured reference process binary
  digest, implementation name, run ID, direction, Router Hash pair,
  and exact DeliveryStatus message ID;
- derive milestones only from validated structured events
  (`ntcp2_authenticated`, `frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`), never
  from a TCP loopback probe alone;
- refuse synthetic provenance fallback hashes that fabricate a
  schema-valid digest from a run string;
- fail closed with one of the typed blockers
  `runner-reference-process-not-executed`,
  `runner-reference-events-missing`,
  `runner-synthetic-provenance-rejected`, or
  `runner-protocol-event-unproven` whenever any of the above
  contracts is violated.

Plan 075 closes the runner-integrity work only; it does not build i2pd, run a real mixed-router direction, add Docker/QEMU/namespaces/CI, change NTCP2 protocol code, or produce a Level 2 or Level 3 record. The next active plan is Plan 076, followed by Plans 077, 078, and 079. The current repository therefore has no real mixed-router attempt and remains experimental and non-advertised.

## MVP direction

The feature MVP is expected to include:

- A foreground, CLI-operated router daemon.
- Persistent router identity and validated configuration.
- I2NP message handling and core router dispatch.
- NTCP2 and SSU2 transport support.
- NetDB client behavior and floodfill participation.
- Inbound, outbound, exploratory, and transit network tunnels.
- Destination and LeaseSet management.
- I2P streaming.
- SAM and I2CP client interfaces.
- HTTP and SOCKS5 client proxies.
- Generic TCP client and server tunnels.
- IRC client and IRC server tunnel profiles.
- Bounded resource accounting, graceful shutdown, health reporting, and operational metrics.

This is a substantial scope. Development will first target a smaller interoperable-router milestone before closing the complete feature MVP.

## Architectural principles

### Wire compatibility and policy separation

Protocol codecs, cryptographic state machines, and negotiated capabilities must remain separate from router policy. Peer selection, transit participation, resource allocation, tunnel quantities, and floodfill eligibility may vary by profile without changing wire behavior.

### Modular monolith

The initial router will be one process composed from focused Rust crates. Crate boundaries should follow security boundaries, protocol churn, and ownership—not arbitrary source-file size. The project will not begin as a distributed collection of services or as a runtime plugin platform.

### Explicit trust boundaries

All network, persisted network, client API, configuration, and local service inputs are untrusted until validated. Subsystems receive only the capabilities they require. A global mutable router context or unrestricted service locator is not an acceptable default.

### Bounded execution

Queues, buffers, handshakes, sessions, tunnel builds, NetDB operations, destinations, streams, and API clients must have explicit limits. Peer-controlled work must have deadlines, cancellation paths, and cleanup semantics.

### Defensive Rust

Safe Rust is the default. Protocol and routing crates should forbid unsafe code. Cryptographic primitives should come from reviewed implementations rather than being created locally. Secret-bearing types must avoid accidental logging, cloning, serialization, or long-lived retention.

### Small dependency surface

Prefer the standard library and focused pure-Rust crates where that produces a maintainable and auditable implementation. Dependency minimization must not justify implementing cryptography, parsers, compression, or other high-risk primitives without adequate expertise and review.

### Testability by design

Protocol state machines should support deterministic clocks, seeded randomness, in-memory transports, fault injection, and reproducible simulation. The test harness is a core project component, not a late-stage accessory.

## Intended workspace shape

The exact workspace will evolve, but the initial direction is:

```text
crates/
  i2pr-proto/               Wire types, codecs, constants, validation
  i2pr-crypto/              Protocol-specific cryptographic wrappers
  i2pr-core/                Shared contracts, lifecycle, budgets, health
  i2pr-transport/           Transport-neutral link management and selection
  i2pr-transport-ntcp2/     NTCP2 implementation
  i2pr-transport-ssu2/      SSU2 implementation
  i2pr-netdb/               RouterInfo/LeaseSet storage, lookup, publication
  i2pr-tunnel/              Network tunnel construction and participation
  i2pr-client/              Destinations, LeaseSets, garlic, streaming
  i2pr-api/                 SAM and I2CP adapters
  i2pr-service-tunnels/     HTTP, SOCKS5, IRC, generic TCP forwarding
  i2pr-storage/             Atomic persistence and migration support
  i2pr-runtime/             Tokio-backed supervision and cancellation
  i2pr-daemon/              CLI, configuration, composition, supervision
  i2pr-testkit/             Deterministic simulation and adversarial fixtures
```

The current workspace contains `i2pr-proto`, `i2pr-crypto`, `i2pr-storage`,
`i2pr-core`, `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-runtime`,
`i2pr-daemon`, and `i2pr-testkit`. The runtime crate is the only production
crate that owns Tokio tasks, timers, channels, sockets, or wakeable cancellation;
transport crates expose pure contracts and protocol seams only. Plan 035's
listener, dialer, replay owner, and per-link reader/writer children all remain
inside that runtime boundary. Later plans
will add protocol and service crates when their contracts are understood;
empty placeholder crates are not created in advance.

Plan 036 adds the controlled interoperability and adversarial-validation
evidence boundary under `tests/integration/ntcp2/`. Its preflight is manual and
fail-closed: it requires disposable identities, a synthetic private network,
disabled reseed/bootstrap, pinned Java I2P/i2pd artifacts, and sanitized
typed-result records. The current checkout keeps live activation disabled and
does not claim mixed-router interoperability until a complete wire-level
runtime adapter and authorized runs in both directions are available.

Plan 037 corrects the local integration defects found during that review:
inbound admission now travels with the accepted stream, link queue entries
release their accounting through RAII, and supervised reader/writer I/O uses
configured cancellation and deadline bounds. General data-phase block parsing
also separates its deployed-wire ordering rules from strict SessionConfirmed
payload parsing. Plan 042 now supplies the bounded socket-to-state-machine/data-
phase composition through the non-production launcher. Plan 044 composes the
mixed-router execution model with the four directional i2pr/reference scenarios,
the strict launcher renderer, the non-echo data-phase oracle, and the mixed
evidence schema. Java I2P/i2pd mixed-router evidence remains pending
execution, so Milestone 3 and all NTCP2 support rows remain blocked,
experimental, and non-advertised.

The current `i2pr-proto` API uses borrowed cursors and caller-visible maximums,
strict exact-consumption decoding, canonical immutable mappings, typed
algorithm/length validation, preserved signed-byte regions, and a bounded I2NP
registry with standard and short header codecs. I2NP bodies that need later
cryptography or state machines are named `Deferred`/`Opaque` values rather
than support claims. The separate
`i2pr-crypto` crate implements only type-7 Ed25519 signing/verification,
type-4 X25519 public-key derivation, SHA-256 wrappers, constant-time equality,
and zeroizing private wrappers. `i2pr-storage` implements the version-1
permission-hardened private identity file. None of these crates introduce
transport behavior, runtime integration, network publication, or capability
advertisement.

## External integration direction

Future integration with `synvoid` should occur at the service boundary, normally by forwarding an I2P server destination to a local Unix socket or loopback service. `synvoid` should not become part of the routing core.

Future integration with `eggsec` should use stable testkit, fault-injection, and private-testnet interfaces. Adversarial tests must be constrained to systems and networks where authorization is explicit.

## Documentation

- [Project guardrails](GUARDRAILS.md)
- [MVP roadmap](plans/000-mvp-roadmap.md)
- [Workspace and skeleton pre-plan](plans/001-preplan-workspace-skeleton.md)
- [Milestone 0 closure record](plans/001-closure.md)
- [Milestone 1 common-structures closure record](plans/012-closure.md)
- [Milestone 1 identity/crypto/storage closure record](plans/013-closure.md)
- [Milestone 1 I2NP/evidence/fuzzing closure record](plans/014-closure.md)
- [Aggregate Milestone 1 corrective closure record](plans/010-milestone-1-closure.md)
- [Plan 021 supervision and cancellation closure record](plans/021-closure.md)
- [Plan 022 bounded channels and resource governor closure record](plans/022-closure.md)
- [Plan 023 deterministic network testkit closure record](plans/023-closure.md)
- [Aggregate Milestone 2 closure record](plans/020-milestone-2-closure.md)
- [Plan 024 observability and validation plan](plans/024-m2-observability-validation-closure.md)
- [Plan 025 targeted corrective closure](plans/025-closure.md)
- [Plan 031 transport contracts and crate boundaries](plans/031-m3-transport-contracts-and-crate-boundaries.md)
- [Plan 031 closure record](plans/031-closure.md)
- [Plan 032 NTCP2 crypto/transcript plan](plans/032-m3-ntcp2-crypto-transcript-and-vectors.md)
- [Plan 032 closure record](plans/032-closure.md)
- [Plan 033 NTCP2 handshake state machines](plans/033-m3-ntcp2-handshake-state-machines.md)
- [Plan 033 closure record](plans/033-closure.md)
- [Plan 034 NTCP2 data phase and blocks](plans/034-m3-ntcp2-data-phase-and-blocks.md)
- [Plan 034 closure record](plans/034-closure.md)
- [Plan 035 runtime link manager and addresses](plans/035-m3-runtime-link-manager-and-addresses.md)
- [Plan 035 closure record](plans/035-closure.md)
- [Plan 036 interoperability and adversarial validation](plans/036-m3-interoperability-adversarial-validation-closure.md)
- [Plan 036 closure record](plans/036-closure.md)
- [Plan 037 corrective integration and closure](plans/037-m3-corrective-integration-closure.md)
- [Plan 037 closure record](plans/037-closure.md)
- [Plan 038 Ubuntu reference-router interoperability harness](plans/038-ubuntu-reference-router-interoperability-harness.md)
- [Plan 042 runtime-owned NTCP2 wire driver](plans/042-runtime-owned-ntcp2-wire-driver.md)
- [Plan 042 current status](plans/042-status.md)
- [Plan 053 evidence pipeline corrective pass](plans/053-plan052-evidence-pipeline-integration-corrective-pass.md)
- [Plan 053 status](plans/053-status.md)
- [Plan 054 Java startup and reference-observation qualification pass](plans/054-java-startup-and-reference-observation-qualification-pass.md)
- [Plan 054 status](plans/054-status.md)
- [Plan 058 record and candidate integrity closure pass](plans/058-plan056-record-and-candidate-integrity-closure-pass.md)
- [Plan 058 status](plans/058-status.md)
- [Plan 059 reference-side implementation and live qualification closure pass](plans/059-reference-side-implementation-and-live-qualification-closure-pass.md)
- [Plan 059 status](plans/059-status.md)
- [Plan 064 i2pd direct NTCP2 driver and observer correction](plans/064-i2pd-direct-ntcp2-driver-and-observer-correction.md)
- [Plan 064 status](plans/064-status.md)
- [Plan 076 real pinned i2pd library and direct driver construction](plans/076-real-pinned-i2pd-library-and-direct-driver-construction.md)
- [Plan 076 status](plans/076-status.md)
- [Plan 065 NTCP2 canonical integration and live qualification](plans/065-ntcp2-canonical-integration-and-live-qualification.md)
- [Plan 065 status](plans/065-status.md)
- [Plan 066 fresh candidate and authoritative NTCP2 two-run closure](plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md)
- [Plan 066 candidate record](plans/066-candidate.md)
- [Plan 066 closure record](plans/066-closure.md)
- [Plan 060 fresh candidate and two-run Milestone 3 certificate closure pass](plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md)
- [Plan 060 candidate record](plans/060-candidate.md)
- [Plan 060 closure record](plans/060-closure.md)
- [Plan 067 Milestone 3 staged interoperability corrective roadmap](plans/067-milestone-3-staged-interoperability-corrective-roadmap.md)
- [Plan 068 staged evidence and Milestone 3 authority correction](plans/068-staged-interop-evidence-and-milestone-3-authority-correction.md)
- [ADR 0023 staged NTCP2 interoperability evidence](docs/adr/0023-staged-ntcp2-interoperability-evidence.md)
- [Plan 072 conditional Emissary NTCP2 differential validation](plans/072-conditional-emissary-ntcp2-differential-validation.md)
- [Plan 072 activation amendment](plans/072-activation-amendment-plan-084.md)
- [Plan 083 minimal i2pr-to-i2pd NTCP2 wire probe](plans/083-minimal-i2pr-to-i2pd-ntcp2-wire-probe.md)
- [Plan 084 i2pd-to-i2pr reverse probe and development decision](plans/084-i2pd-to-i2pr-reverse-probe-and-development-decision.md)
- [Plan 084 status](plans/084-status.md)
- [Plan 085 Milestone 3 host-loopback development execution roadmap](plans/085-milestone-3-host-loopback-development-execution-roadmap.md)
- [Plan 086 status authority and host-loopback development lane](plans/086-status-authority-and-host-loopback-development-lane.md)
- [Plan 087 first real i2pr-to-i2pd host-loopback probe](plans/087-first-real-i2pr-to-i2pd-host-loopback-probe.md)
- [Plan 088 reverse host-loopback probe and development decision](plans/088-reverse-host-loopback-probe-and-development-decision.md)
- [Plan 088 status](plans/088-status.md)
- [Plan 072/079 gate amendment (Plan 088)](plans/072-079-gate-amendment-plan-088.md)
- [Aggregate Milestone 3 closure record](plans/030-milestone-3-closure.md)
- [Controlled NTCP2 interoperability lane](tests/integration/ntcp2/README.md)
- [Machine-readable protocol support ledger](specs/support.toml)
- [Architecture](docs/architecture.md)
- [Protocol support matrix](docs/protocol-support.md)
- [Security model](docs/security-model.md)
- [Controlled private-testnet boundary](docs/private-testnet.md)
- [Architecture decision records](docs/adr/0000-adr-process.md)
- [Runtime and supervision ADR](docs/adr/0008-runtime-supervision-and-cancellation.md)
- [Runtime observability and validation ADR](docs/adr/0009-runtime-observability-and-validation.md)
- [Transport contracts and crate boundaries ADR](docs/adr/0010-transport-contracts-and-crate-boundaries.md)
- [NTCP2 crypto and static-key storage ADR](docs/adr/0011-ntcp2-crypto-and-static-key-storage.md)
- [NTCP2 handshake state-machines ADR](docs/adr/0012-ntcp2-handshake-state-machines.md)
- [NTCP2 data-phase and blocks ADR](docs/adr/0013-ntcp2-data-phase-and-blocks.md)
- [NTCP2 runtime link manager and address policy ADR](docs/adr/0014-ntcp2-runtime-link-manager-and-address-policy.md)
- [Ubuntu reference-router harness ADR](docs/adr/0015-ubuntu-reference-router-harness.md)
- [Ubuntu build-system interoperability gates ADR](docs/adr/0016-ubuntu-build-system-interop-gates.md)
- [Rootless sealed-namespace interoperability evidence ADR](docs/adr/0017-rootless-sealed-namespace-interop-evidence.md)
- [Multipass rootless interoperability environment ADR](docs/adr/0018-multipass-rootless-interop-environment.md)
- [Plan 053 evidence-pipeline integrity ADR](docs/adr/0020-plan053-evidence-pipeline-integrity.md)
- [Contribution guide](CONTRIBUTING.md)
- [Protocol specification index and source ledger](specs/README.md)

## Development expectations

Before implementation work begins, read `GUARDRAILS.md`, the relevant plan in
`plans/`, the applicable ADRs, and the applicable protocol dossier and
conformance policy under `specs/`. Each implementation phase should define
acceptance criteria, tests, non-goals, dependency changes, security
implications, source revisions, and documentation updates.

The local quality baseline is:

```text
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
```

The optional nightly-only fuzz lane is maintained separately from the
production workspace. See `fuzz/README.md` and run
`bash scripts/fuzz-smoke.sh` for bounded local smoke tests.

Runtime changes must use deterministic Tokio test time (`start_paused` or
explicit `time::advance`) rather than wall-clock sleeps. Every spawned task
must be owned by the supervisor or a service child scope and must be joined or
explicitly aborted before the runtime returns.

Transport changes must keep `i2pr-transport` runtime-neutral and keep
`i2pr-transport-ntcp2` free of Tokio, filesystem, sockets, and live protocol
side effects. Plans 035 and 037 keep every TCP listener/stream, async deadline,
replay-cache owner, admission counter, queued-frame owner, and reader/writer
child inside `i2pr-runtime`; controlled sockets remain disabled-by-default test
infrastructure. Plan 037 requires the pending admission owner to survive the
handshake handoff, cancellation to win I/O races, and queue accounting to drop
exactly once on success, failure, cancellation, or teardown.
Plans 032–033 additionally keep cryptographic and handshake
state consuming and secret-safe, persist transport static key/IV material only
through the versioned storage boundary, and require the hashed fixture
validator. Drive
state through bounded explicit actions and outcomes; use
owned encoded-I2NP handoffs and redacted snapshots rather than raw payloads,
addresses, keys, or runtime channels. Plan 031's focused local checks are:

```text
cargo test -p i2pr-transport --all-targets
cargo test -p i2pr-transport-ntcp2 --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-vectors.sh
```

Plan 033 also requires the NTCP2 handshake codec/state tests and the separate
nightly fuzz workspace. Plans 036–037 add the fixed-seed 0..255 integrated
testkit matrix, parser-boundary regressions, and the sanitized interoperability
preflight. These tests are deterministic and local; they are not mixed-router
or public-network evidence.

The Plan 024 integrated validation lane is `cargo test -p i2pr-testkit
--all-targets`; it runs the five named scenarios and the fixed 32-seed replay
matrix. Run `rtk bash scripts/check-runtime-boundaries.sh` for the mechanical
runtime/testkit guardrails. Runtime snapshots and tracing events may contain
only validated service/channel identifiers, typed categories, counters,
bounded monotonic timing, and synthetic simulation metadata; health detail
text is redacted from default `Debug` output and aggregate snapshots.

The CLI exposes `--help`, `--version`,
`check-config --config <path>`, `identity generate --config <path>`,
`identity inspect --config <path>`, and `run --config <path> --dry-run`. Identity
generation is explicit, create-only, and permission-hardened. Inspection
loads and validates the file without displaying private material. Config
validation and dry-run do not create directories or identity files. A live
`run` deliberately remains non-networked and exits with code 20 until a later
daemon-composition plan wires the runtime into a live router. No command opens
a socket, publishes RouterInfo, or writes network state.

The project should favor incremental, reviewable changes. A protocol feature
is not complete merely because it compiles or communicates with one peer.
Completion requires negative tests, malformed-input handling, lifecycle
cleanup, bounded resource behavior, fuzz coverage, fixture provenance, and
interoperability evidence.

### Plan 048/049/050 Multipass lifecycle-owned permissive rootless evidence environment

The current host remains the Plan 046 negative baseline because
`kernel.apparmor_restrict_unprivileged_userns=1`. Plan 048 provides a
disposable Ubuntu 24.04 amd64 Multipass recovery guest without changing that
host policy. The guest uses fixed resources from
[`environment.toml`](scripts/interop/multipass/environment.toml), applies the
permissive sysctls only inside the VM, and runs the evidence lane as the
non-sudo `i2ptest` user.

Preparation transfers an exact clean source archive and the pinned reference
cache from the canonical `target/interop/cache` path. The reviewed environment
has a stable environment ID, separate from each generated run ID and concrete
Multipass instance generation. The default path reserves host lifecycle state
atomically before launch and allocates a collision-resistant instance name; the
legacy `i2pr-interop-rootless` name is not authoritative.

The host baseline probe is recorded separately from the guest rootless probe.
The host's `blocked_unprivileged_user_namespace` result remains a negative
baseline and does not gate guest launch. After ownership and guest policy
verification, and again immediately before any router process, `probe.sh` must
return `rootless_sandbox_available` before the four Plan 045 directions run.
Use the lifecycle-owned entrypoint:

```text
bash scripts/interop/multipass/run-evidence-lane.sh --all
bash scripts/interop/multipass/run-evidence-lane.sh --all \
  --run-id plan049-example --destroy-after-export
bash scripts/interop/multipass/run-evidence-lane.sh --inspect --run-id <run-id>
bash scripts/interop/multipass/run-evidence-lane.sh --all --resume-owned \
  --run-id <run-id>
```

Adoption, recreation, and destruction require explicit flags and a complete
host/guest ownership proof. No operation silently adopts, mutates, deletes, or
purges an existing instance; global `multipass purge` is forbidden in the
normal lifecycle. An interruption may be resumed only through a validated
state transition, and a retained blocker is marked `blocked` without exporting
raw diagnostics.

Exported evidence is independently hashed and atomically placed under
`target/interop/evidence/multipass/<run-id>/`; destroying an owned VM preserves
it. Directional records identify the environment contract, run ID, instance
generation, ownership and contract digests, and the environment evidence hash.
Mixed generations or run IDs cannot form one passing manifest. Multipass
blockers, reference-only control results, and partial matrices are not NTCP2
support evidence and do not advance the support ledger.

Plan 050 minimizes the cloud-init unit (no `rustup` or host toolchain inside
the guest), adds a sanitized cloud-init failure taxonomy, a
`--guest-probe-only` flow, and a selective-purge remediation that requires
a verified ownership contract. Cloud-init failures are classified via
`scripts/interop/multipass/cloud_init_status.py` and the
`cloud-init-status.sh` shell wrapper; the base environment is
post-verified via `verify-base.sh`; and `selective-purge.sh` only invokes
`multipass purge <instance>` (per-instance) when the ownership contract is
proven. The static boundary check
`scripts/check-multipass-interop-boundary.sh` enforces these additions.

### Plan 077 constrained-host execution lane

Plan 077 adds a read-only capability probe and strict execution/qualification
contracts for the constrained host. Selection is ordered as existing
rootful Docker with `--network none`, QEMU TCG with `-nic none`, the explicitly
reduced inherited-descriptor/seccomp diagnostic, a manual remote Linux lane,
then a typed no-full-runtime-lane result. The probe never installs Docker or
QEMU, changes host policy, invokes privilege escalation, retries rootless or
Multipass lanes, or starts a router.

Run:

```text
bash scripts/interop/probe-constrained-host-lanes.sh
bash scripts/check-constrained-host-lane-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
```

The original Plan 077 probe selected
`inherited-descriptors-seccomp` as a reduced-scope capability. Plan 080 later
qualified the owned Multipass guest as the full-runtime lane used for the
single Plan 078 attempt; the historical Plan 077 probe remains preserved in
[Plan 077 status](plans/077-status.md). The architecture decision is
[ADR 0024](docs/adr/0024-constrained-host-ntcp2-execution-lanes.md).
NTCP2 remains experimental and non-advertised.

### Plan 078 first real i2pd two-way execution

Plan 078 used the later-qualified Plan 080 guest, but its first direction
stopped before TCP at the i2pr pre-protocol RouterInfo stage. No NTCP2
handshake, authenticated frame, or I2NP DeliveryStatus result exists, and the
stop is not protocol rejection evidence. See [the Plan 078 status](plans/078-status.md)
and [Plan 080 status](plans/080-status.md).

### Current active sequence: Plan 095 → Plan 088

The earlier Plan 085 → Plan 086 → Plan 087 → Plan 088 summary is preserved
above as a historical section. As of 2026-08-08 the active Milestone 3
forward-direction closure lane is governed by
[Plan 095](plans/095-ci-host-loopback-live-wire-evidence-lane.md) (the
single next executable plan), with [Plan 096](plans/096-plan095-ci-workflow-correctness-and-pre-dispatch-closure.md)
and [Plan 097](plans/097-plan095-artifact-path-and-cleanup-corrective-pass.md)
as closed corrective passes that restored execution correctness before
the first authoritative dispatch. See the [Plan 095](#plan-095-ci-host-loopback-live-wire-evidence-lane),
[Plan 096](#plan-096-plan-095-ci-workflow-correctness-and-pre-dispatch-closure),
and [Plan 097](#plan-097-plan-095-artifact-path-and-cleanup-corrective-pass)
sections above for the full implementation surface, the bounded CI
environment blocker vocabulary, and the artifact-path and cleanup
corrections.

The current status of the active sequence is:

```text
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
```

Plan 095 is the single next executable plan; exactly one manual
GitHub Actions dispatch of
`.github/workflows/ntcp2-interop-host-loopback-development.yml` follows
the Plan 097 correction commit. Plan 088 remains blocked until Plan 095
closes with a passing instrumented and a passing control forward record
from the same CI evidence pair. Plan 079 remains blocked pending the
Plan 088 two-way pass. Plan 072 remains inactive pending the Plan 088
ambiguity decision. NTCP2 remains experimental and non-advertised.

### Plan 088 reverse probe and development decision

Plan 088 owns the reverse host-loopback probe and the active development
decision. Plan 088 inherits the Plan 086
`host-loopback-development` lane and reuses the Plan 084 reverse probe
record schema (`i2pr-minimal-i2pd-reverse-probe-v1`) and runner
orchestration module (`plan084_runner.py`) unchanged.

Plan 088 lands:

- The `host-loopback-development` topology kind in
  `tests/integration/ntcp2/harness/minimal_i2pd_probe.py`'s shared
  `ALLOWED_TOPOLOGY_KINDS` set and the new bounded
  `DEVELOPMENT_ONLY_TOPOLOGY_KINDS` marker.
- Acceptance of the development topology in the
  `plan083_runner.py` and `plan084_runner.py` lane validators and
  top-level topology override paths.
- `tests/integration/ntcp2/harness/test_plan088.py` — the Plan 088 test
  matrix (35 cases) covering the bounded development decision vocabulary,
  the Plan 079 entry gate, the Plan 072 activation gate, the handoff
  fields, the development-only topology contract, the reverse probe schema
  contract, the cross-direction rejection, and the module boundary.
- Extension of `scripts/check-ntcp2-interoperability.sh` to enforce the
  Plan 088 test matrix presence, the locked decision vocabulary, the
  `host-loopback-development` topology coverage, the plan-of-record
  reference, the `plans/088-status.md` decision token, and the
  prohibition of the legacy `lane-invalidated` and
  `same-stage-two-way-i2pr-defect` tokens.

The Plan 088 status record is `plans/088-status.md`. On this host the
recorded development decision is `insufficient-evidence`: the Plan 086
host-loopback lane closed as `host-loopback-development-ready`, the
Plan 087 forward direction reached TCP authentication before the
NTCP2 Noise handshake closed (Plan 091), and the Plan 094
implementation landed but its live closure environment is blocked on
this host. Plan 095 is the active single next executable plan; the
Plan 088 implementation surface is preserved for any future host where
the Plan 095 CI evidence pair records `two-way-development-probe-passed`
or `ambiguous-reference-divergence`. Plan 079 remains blocked; Plan 072
remains inactive. NTCP2 remains experimental and non-advertised.

## License

A project license has not yet been selected. Do not copy implementation code from I2P+, i2pd, Emissary, or another router into this repository until license compatibility and provenance have been reviewed. Specifications and observed interoperability behavior may be used for clean-room implementation, subject to their applicable terms.
