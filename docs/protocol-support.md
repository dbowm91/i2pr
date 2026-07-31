# Protocol support matrix

This matrix is intentionally explicit: every row describes the exact evidence
available, not just code presence. “Experimental structural subset” means
bounded codecs exist and are tested locally, but no mixed-router interoperability
or capability claim exists.

The fine-grained, machine-readable inventory through the current Milestone 3
corrective integration is
[`specs/support.toml`](../specs/support.toml). Structural entries may be marked
`experimental` with repository evidence, but remain `advertised = false`; the
ledger does not itself publish protocol capabilities.

Plan 031 adds transport-neutral link, delivery, lifecycle, and resource
contracts. Plan 032 adds a Tokio-free NTCP2 cryptographic/transcript foundation
plus static-key persistence, and Plan 033 adds bounded handshake codecs and
consuming action-driven state machines. These are experimental local evidence,
not complete NTCP2 protocol support; no transport capability is advertised or
published in RouterInfo.

Plan 037 records local corrections to admission ownership, deadline-enforced
link I/O, queue RAII, and general data-phase block ordering. It does not add a
complete socket-to-state-machine adapter or mixed-router evidence; NTCP2 rows
therefore remain experimental and non-advertised.
Plan 034 adds runtime-neutral authenticated data frames, strict payload
blocks, and deterministic partial-I/O evidence. The current specification has
no in-session rekey threshold; counter exhaustion remains terminal and requires
a fresh handshake. This is still local evidence only; no sockets, NetDB
mutation, mixed-router interoperability, or transport capability is claimed.
Plan 035 adds controlled runtime-owned TCP lifecycle, strict NTCP2 address
interpretation, admission, replay/backoff, and joined link-child ownership.
Loopback/private socket tests are local lifecycle evidence only; public
listeners, automatic address publication, NetDB mutation, mixed-router
interoperability, and capability advertisement remain excluded.
Plan 036 adds the pinned, manual interoperability manifest, sanitized-evidence
format, preflight check, and fixed-seed 0..255 local validation campaign. The
runtime-owned NTCP2 wire adapter is implemented and locally validated; mixed-
router harness composition and authorized evidence are pending; NTCP2 remains
experimental and non-advertised.

Plans 038/040/041 document the Ubuntu-only, amd64-only harness for resolving
that blocker; Plan 041 adds a reference-only Java I2P/i2pd control crosscheck
but does not change any row in this matrix. Preparation may use
declared package/source network access to build and hash pinned references.
Execution is a separate fail-closed phase using disposable namespaces joined
only by a veth pair, with no default route, DNS, or public egress. Environment
smoke and Java I2P/i2pd reference crosscheck are harness validation only. An
i2pr mixed-router claim still requires sanitized bounded authenticated runs
against each reference in both directions, plus the evidence and
advertisement requirements in `specs/CONFORMANCE.md`.

| Protocol area | Status | Planned milestone | Specification/source starting point | Test-vector status | Interoperability status |
| --- | --- | --- | --- | --- | --- |
| Common identity, keys, and certificates | Experimental structural subset plus local type-4/type-7 execution | 1 | `specs/protocols/01-common-identity-crypto.md`, pinned source in `specs/SOURCES.md` | Locally authored structural bytes, Ed25519 mutation tests, and X25519 derivation tests; no independent router vectors | None |
| Router identity generation and local RouterInfo signing | Experimental local lifecycle | 1 | `plans/013-m1-identity-crypto-storage.md`, ADRs 0004 and 0007 | Deterministic injected-RNG generation, exact signed-region verification, save/reload and mutation tests | None |
| Private router identity storage | Experimental local persistence | 1 | `plans/013-m1-identity-crypto-storage.md`, ADR 0006 | Version/length/truncation/integrity/permission/concurrency tests; no external storage interoperability claim | None |
| I2NP envelope and header variants | Experimental structural subset; not advertised | 1, 3–6 | `specs/protocols/02-i2np.md`, pinned 0.9.69 source in `specs/SOURCES.md` | Locally authored standard/short vectors, truncation, size, checksum, and trailing-byte tests; hashed fixture manifest | None |
| I2NP type registry and selected body codecs | Experimental structural subset; NetDB body semantics deferred | 1, 4 | `specs/protocols/02-i2np.md`, `crates/i2pr-proto/src/i2np/mod.rs` | Fixed and malformed local vectors for DatabaseLookup, DatabaseSearchReply, DeliveryStatus, DatabaseStore framing, and fixed tunnel framing | None |
| I2NP tunnel, garlic, data, and later record semantics | Deferred or framing-only | 1, 5–6 | `specs/protocols/02-i2np.md`, `specs/protocols/05-tunnels.md`, `specs/protocols/06-garlic-ecies-leasesets.md` | Bounded `Deferred`/`Opaque` retention and shape checks only; no crypto or state-machine vectors | None |
| NTCP2 crypto/transcript foundation | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0011, `plans/036-closure.md`, `plans/037-closure.md` | Independent deterministic primitive/transcript vectors and corrective review; no router interoperability run | `tests/integration/ntcp2/manifest.toml` pinned but execution blocked; the Plan 046 rootless variant reports `blocked_unprivileged_user_namespace` on the host recorded in `plans/046-closure.md`; the Plan 066 fresh-candidate pass is `declared-not-executable` on this host under the historical Plan 058/060 two-lane contract, see `plans/066-closure.md`; the Plan 067 active roadmap records Level 1 smoke and Level 2 development validation lanes for the host loopback |
| NTCP2 handshake codecs and state machines | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0012, `plans/036-closure.md`, `plans/037-closure.md` | Fixed/malformed/bounded state and policy tests plus local corrective campaign; no mixed-router interoperability | Required Java I2P/i2pd lanes blocked; Plan 046 rootless lane is closed with `blocked_unprivileged_user_namespace`; Plan 066 fresh-candidate pass is `declared-not-executable` on this host with the typed blocker `blocked_execution_lane_unavailable`; see `plans/066-closure.md` and `specs/CONFORMANCE.md`; the Plan 067/068 active roadmap separates evidence into local-conformance, external-loopback-smoke, repeated-development-interop, conditional-differential, and release-qualification tiers (ADR 0023) |
| NTCP2 authenticated data frames and payload blocks | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0013, `plans/036-closure.md`, `plans/037-closure.md` | Deterministic frame/block vectors, corrected repeated-block/termination ordering tests, partial-I/O cleanup, and local campaign; no mixed-router interoperability | Required Java I2P/i2pd lanes blocked; Plan 046 rootless lane is closed with a typed blocker (`plans/046-closure.md`); the Plan 066 fresh-candidate pass is `declared-not-executable` on this host, see `plans/066-closure.md`; the Plan 067/068/074 active roadmap defines Plan 069 (host-loopback smoke), Plan 075 (runner integrity), Plan 076 (real i2pd driver), and Plan 079 (repeated i2pd validation) for development interoperability; no real mixed-router attempt has yet occurred |
| NTCP2 runtime link manager, addresses, and controlled TCP lifecycle | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0014, `plans/036-closure.md`, `plans/037-closure.md` | Bounded address/admission/replay/backoff/duplicate/RAII cleanup tests plus loopback lifecycle and preflight; runtime-owned wire adapter implemented and locally validated, mixed-router evidence pending | Required Java I2P/i2pd lanes blocked; Plan 046 rootless lane is closed with a typed blocker (`plans/046-closure.md`); the Plan 066 fresh-candidate pass is `declared-not-executable` on this host, see `plans/066-closure.md`; the Plan 067/068 active roadmap keeps NTCP2 experimental and non-advertised |
| Reseed and RouterInfo publication | Not implemented | 4 | `specs/protocols/04-reseed-netdb.md` | None imported | None |
| Network tunnels and transit participation | Not implemented | 5 | `specs/protocols/05-tunnels.md` | None imported | None |
| Classic LeaseSet structural codec | Experimental structural subset; LeaseSet2-family deferred | 6 | `specs/protocols/06-garlic-ecies-leasesets.md` | Local Lease/LeaseSet vectors and negative tests; no independent router vectors | None |
| LeaseSet2, EncryptedLeaseSet, and MetaLeaseSet | Deferred | 6 | `specs/protocols/06-garlic-ecies-leasesets.md` | Explicit rejection/deferred framing only | None |
| I2P streaming | Not implemented | 6 | `specs/protocols/07-streaming.md` | None imported | None |
| SAM | Not implemented | 7 | `specs/protocols/08-sam.md` | None imported | None |
| SSU2 | Not implemented | 8 | `specs/protocols/09-ssu2.md` | None imported | None |
| I2CP | Not implemented | 9 | `specs/protocols/10-i2cp-service-tunnels.md` | None imported | None |
| Service tunnels | Not implemented | 10 | `specs/protocols/10-i2cp-service-tunnels.md` | None imported | None |

The workspace may name the `common` and `i2np` namespaces and now includes the
non-networked `i2pr-runtime` supervision crate, but runtime infrastructure is
not protocol support evidence. Plan
013 adds local type-4/type-7 execution plus a private identity file. These
local operations do not establish mixed-router protocol support, complete
signature/encryption coverage, transport support, network compatibility, or
capability advertisement. Legacy NTCP and SSU1 are outside the MVP target
unless a later plan explicitly changes scope.

The I2NP implementation recognizes the pinned message identifiers and strictly
decodes standard, obsolete-SSU, and NTCP2/SSU2 short headers. It fully models
the structural fields of DatabaseLookup, DatabaseSearchReply, DeliveryStatus,
and DatabaseStore; only classic LeaseSet payloads reuse an existing structural
codec. Compressed RouterInfo, LeaseSet2-family records, tunnel-build record
cryptography, garlic/data semantics, duplicate/expiry policy, routing,
transport authentication, and capability advertisement remain deferred. No
I2NP row is `advertised = true`, and no row claims mixed-router support.

DatabaseLookup legacy and ECIES reply-key/tag wrappers are non-cloneable and
zeroizing structural containers. They provide memory hygiene only; they do not
implement encrypted reply semantics, key derivation, decryption, or NetDB
behavior.

Each future protocol row must be updated with exact targeted proposal/spec
revisions, limits, malformed-input behavior, vectors, and mixed-router evidence
before its status changes.

### Plan 048 evidence-environment notice

Plan 048 adds only a disposable Multipass recovery environment for the Plan
046 rootless lane. The current host remains the AppArmor-restricted negative
baseline, the guest applies permissive policy only inside its VM, and the
canonical cache is `target/interop/cache`. A guest probe, matrix, or exported
reference control result does not advance any support row. NTCP2 remains
experimental and non-advertised until sanitized mixed-router conformance
evidence satisfies `specs/CONFORMANCE.md`.

### Plan 060 fresh-candidate and two-run Milestone 3 certificate closure pass

Plan 060 was the execution-only fresh-candidate and two-run
Milestone 3 certificate closure pass. Plan 060 is now **retired
by Plan 062** (Plan 062 evidence-contract and architecture
correction pass). The Plan 060 candidate record
(`plans/060-candidate.md`) is preserved verbatim for audit; the
Plan 060 closure record (`plans/060-closure.md`) carries the
explicit "Superseded by Plan 062" marker. Future candidates must
descend from the Plan 065 implementation floor or later and must
use the Plan 062 v4 trigger schema, the Plan 062 reference-event
v1 schema, the Plan 062 v3 observation schema, and the 64-hex
SHA-256 Router Hash contract.

Plan 060 inherited the rejected Java-support-topology premise
(ADR 0021 Rejected by Plan 058). Plan 062 ADR 0022 (Accepted)
replaces that premise with two-process direct transport drivers.
The historical Plan 060 typed blocker on this host is
`blocked_execution_lane_unavailable` and the historical candidate
status is `declared-not-executable`. The Plan 046 rootless
sealed-namespace probe reports
`blocked_unprivileged_user_namespace` on this host; the Plan
048/049 Multipass recovery lane is the canonical external path
but cannot complete on this constrained host (per Plan 051).

The Plan 060 implementation surface
(`tests/integration/ntcp2/harness/plan060.py`, the Plan 060 test
matrix, the static boundary checker extension, the candidate
record `plans/060-candidate.md`, and the closure record
`plans/060-closure.md`) is preserved as an audit record. NTCP2
remains experimental and non-advertised until a future pinned
Java revision exposes a transport-only direct seam (or ADR 0021
is re-issued) and either the Plan 046 rootless sealed-namespace
lane or the Plan 048/049 Multipass guest lane becomes runnable
on a host with the resources Plan 051 required.

### Plan 062 NTCP2 evidence-contract and architecture correction

Plan 062 is the evidence-contract and architecture correction
pass. Plan 062 does not implement the Java or i2pd drivers and
does not perform an authoritative external interoperability run;
those belong to Plans 063 and 064.

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
  Plan 062 reference-event v1 schema
  (`i2pr-reference-event-v1`) recording per-driver structured
  events with exact DeliveryStatus message ID correlation for
  data-phase events.
- `tests/integration/ntcp2/harness/observation_v3.py` — the Plan
  062 v3 observation schema
  (`i2pr-ntcp2-direction-observation-v3`) with the mandatory
  correlation fields `delivery_status_message_id`,
  `peer_router_hash_sha256`, `local_router_hash_sha256`, and
  `source_event_sha256`. The v3 receiver pass predicate requires
  nonzero decrypt and decode counts and rejects
  generic-phrase-only sources.
- The historical `trigger_record.py` (v3) and `observation.py`
  (v2) modules remain readable for historical inspection but
  cannot contribute to a new passing bundle.

Plan 062 does not close any interoperability claim. NTCP2 stays
experimental and non-advertised; Milestone 3 stays open until a
verified Milestone 3 certificate is produced under ADR 0023 Level 3
release qualification.

### Plan 065 NTCP2 canonical integration and live qualification

Plan 065 establishes the implementation floor from which Plan 066
may cut a candidate. The plan does not perform an authoritative
external live qualification run; the four primary IPv4 directions
remain typed blockers until the Plan 046 rootless sealed-namespace
lane or the Plan 048/049 Multipass recovery lane can produce a
fresh 10/10 qualification on the pinned Java 2.12.0 and i2pd
2.60.0 references.

Plan 065 lands:

- The strict launcher scenario schema is bumped to
  `i2pr-launcher-scenario-v2` (`schema_version` 2). The strict
  parser requires the per-run DeliveryStatus `message_id` in
  `1..=0xffffffff`, the 64-lowercase-hex expected sender and
  receiver Router Hashes, the `reference_driver_mode` field
  allowlisted to `java-direct-driver` or `i2pd-direct-driver`, and
  the `run_identity_sha256` 64-lowercase-hex digest. The historical
  schema 1 path is rejected.
- The i2pr sender and receiver bind the typed counters
  `delivery_status_message_id` and `expected_peer_router_hash_sha256`
  to the status record. The hard-coded `0x0420_0001` DeliveryStatus
  authority is removed.
- The i2pr sender and receiver emit the bounded Plan 065 typed
  failure categories (`SenderDeliveryStatusMessageIdZero`,
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
  `ReceiverDeliveryStatusTimestampInvalid`). The broad
  `DataPhaseFailed` reason is no longer emitted on the receiver
  side.
- The canonical mixed-runner wires the new scenario primary fields
  through `render_and_validate` for both the i2pr initiator and
  responder paths. The `_plan065_primary_fields` helper derives
  the DeliveryStatus `message_id` from the run identity and the
  correlation nonce; the `_reference_driver_mode_for` helper
  returns the source-locked driver mode for a reference kind. The
  runner rejects SAM, HTTP, I2PControl, support-topology, and
  synthetic-fallback helpers for any primary direction.
- The Plan 065 test matrix (`test_plan065.py`) covers scenario
  v2 acceptance and rejection, DeliveryStatus message ID derivation
  uniqueness, status counter contract, reference trigger v4
  correlation, observation v3 correlation, pass predicate exact
  message ID and Router Hash correlation, support-router rejection,
  Plan 060 candidate retirement, and the Plan 066 implementation
  floor marker.
- The static boundary checker
  (`scripts/check-ntcp2-interoperability.sh`) enforces the Plan
  065 schema marker, the required primary fields, the bounded
  typed failure categories, the absence of the hard-coded
  `0x0420_0001` DeliveryStatus authority, and the Plan 065 test
  matrix existence.

Plan 065 does not close any interoperability claim. NTCP2 stays
experimental and non-advertised; Milestone 3 stays open until a
verified Milestone 3 certificate is produced under ADR 0023 Level 3
release qualification.

### Plan 067 staged interoperability corrective roadmap

Plan 067 is the **active** Milestone 3 corrective roadmap. Plan 067
supersedes Plan 066 as the active execution authority. Plan 066
remains an immutable historical record of the unavailable
release-qualification lane on the constrained host.

Plan 067 separates NTCP2 interoperability evidence into four bounded
tiers:

- **Level 0 — local conformance.** Deterministic local protocol and
  runtime ownership.
- **Level 1 — external loopback smoke.** Two real processes on the
  host loopback. i2pd is the primary initial validator. Emissary is
  conditional. No rootless namespace, no Multipass, no candidate
  freeze, no two-bundle certificate, no reviewer record.
- **Level 2 — repeated development interoperability.** Both
  directions against the primary independent validator (pinned i2pd
  2.60.0), three fresh-state repetitions per direction, exact
  message and identity correlation, bounded negative controls.
- **Level 3 — release qualification.** Java I2P 2.12.0 and i2pd
  2.60.0, isolated no-public-egress lane, reproducible
  source/reference provenance, exact authenticated data-phase
  message correlation, independent fresh state, sanitized durable
  evidence. The Plan 066 certificate verifier may be reused at Level
  3.

Java and i2pd remain required for release qualification. NTCP2 stays
experimental and non-advertised; Milestone 3 stays open until a Level
3 run produces a verified certificate.

### Plan 068 staged evidence and authority correction

Plan 068 implements the staged-evidence and authority correction
that Plan 067 proposes. Plan 068 lands:

- `docs/adr/0023-staged-ntcp2-interoperability-evidence.md`
  (Accepted). ADR 0023 separates evidence into four bounded tiers
  and forbids lower-tier promotion into release bundles. ADR 0023
  does not supersede ADR 0022's direct-driver decision.
- `tests/integration/ntcp2/harness/evidence_tier.py` — the
  evidence-tier constants (`local-conformance`,
  `external-loopback-smoke`, `repeated-development-interop`,
  `conditional-differential`, `release-qualification`) and
  tier-separation rules. The release bundle validators refuse every
  record whose tier is missing or lower than `release-qualification`.
- `tests/integration/ntcp2/harness/loopback_smoke_record.py` — the
  Level 1 smoke record schema
  (`i2pr-ntcp2-loopback-smoke-v1`). A passed record requires every
  positive boolean, `cleanup_clean = true`, and `network_audit` not
  equal to `not-run`. Raw payload, private key, Noise state, and
  full RouterInfo bytes are forbidden.
- `tests/integration/ntcp2/harness/development_validation.py` —
  the Level 2 development-validation summary schema
  (`i2pr-ntcp2-development-validation-v1`). A passed summary
  requires three fresh-state passes per direction, four named
  negative controls reporting `rejected`, `cleanup_passed = true`,
  and an explicit network audit per direction.
- The Plan 068 test matrices (`test_evidence_tier.py`,
  `test_loopback_smoke_record.py`,
  `test_development_validation.py`).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the new schema modules, the new test matrices, the ADR 0023
  acceptance marker, and the release-bundle smoke/development
  rejection. The historical plan surfaces (Plan 055/056/058/059/060
  /062/063/064/065/066 freeze-readiness invariants) remain intact.

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

NTCP2 stays experimental and non-advertised. No external pass has
yet occurred.

### Plan 074 real-driver and constrained-host corrective roadmap

Plan 074 is the active corrective roadmap for Milestone 3 NTCP2
interoperability. Plan 074 supersedes Plan 070 as the next
executable plan and reclassifies the implemented Plan 069 lane as
orchestration scaffolding and fake-process test coverage only; it
is not valid mixed-router evidence until Plan 075 closes. Plan 074
is the parent authority for the active sequence **Plan 075 → Plan
076 → Plan 077 → Plan 078 → Plan 079**. Plan 070 and Plan 071 are no
longer active execution authority.

The corrected repository state is:

```text
plan_068_staged_evidence = implemented
plan_069_runner_scaffolding = implemented_but_not_valid_mixed_router_lane
real_i2pd_driver = not_implemented
real_i2pd_library_linkage = absent
real_reference_process_in_plan069_runner = absent
real_mixed_router_attempts = 0
current_rootless_namespace_lane = unavailable
multipass_lane = unreliable_or_unavailable
support = experimental
advertised = false
normal_daemon_activation = disabled
```

The constrained-host lane decision is ordered: existing accessible
rootful Docker daemon (`--network none`), QEMU TCG guest (`-nic
none`), inherited connected TCP descriptors plus
`no_new_privs`/seccomp for reduced-scope protocol diagnostics,
manually triggered dedicated remote Linux runner, and a typed
no-full-runtime-lane blocker. Rootless namespaces, bubblewrap,
rootless Podman/Docker, user-level systemd `PrivateNetwork`, and
repeated Multipass recovery are not active work items on the known
host.

### Plan 075 Plan 069 runner integrity and evidence correction

Plan 075 corrects the Plan 069 runner so it is structurally
incapable of producing a mixed-router pass unless it launches one
real i2pr process and one configured real reference process and
consumes authentic structured events from both. The corrected
runner must launch the reference role through the configured
reference driver via
`tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh`,
bind every accepted event to a measured reference process binary
digest, implementation name, run ID, direction, Router Hash pair,
and exact DeliveryStatus message ID, derive milestones only from
validated structured events, refuse synthetic provenance fallback
hashes, and fail closed with one of the typed blockers
`runner-reference-process-not-executed`,
`runner-reference-events-missing`,
`runner-synthetic-provenance-rejected`, or
`runner-protocol-event-unproven`.

Plan 075 does not build i2pd, run a real mixed-router direction,
add Docker/QEMU/namespaces/CI, change NTCP2 protocol code, or
produce a Level 2 or Level 3 record. Zero real mixed-router
attempts remain until Plan 078 executes.
