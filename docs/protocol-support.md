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
| Reseed and RouterInfo publication | Plan 103 RouterInfo validation + bounded local NetDB implemented; persistent cache, SU3 reseed, and live publication remain Plan 104/105/106 work | 4 | `specs/protocols/04-reseed-netdb.md`, `plans/103-routerinfo-validation-and-local-netdb-foundation.md`, `plans/103-status.md` | Local signed-region verification, fresh/future/malformed/stale rejection, store replacement/conflict/quota/prune; no Java I2P/i2pd mixed-router evidence | None |
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

Plan 074 is a historical corrective roadmap for Milestone 3 NTCP2
interoperability. Plan 074 superseded Plan 070 and reclassified the
implemented Plan 069 lane as orchestration scaffolding and fake-process
test coverage only; the corrected lane became Plan 075's runner
integrity pass. Plan 074 is no longer active execution authority; the
Plan 081 amendment is the active corrective roadmap, with Plan 082
implemented, Plan 083 next, and Plan 084 reverse.

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
produce a Level 2 or Level 3 record. The Plan 078 attempt stopped
before TCP and is not protocol evidence; the corrected active
sequence is Plan 082 → Plan 083 → Plan 084.

### Plan 077 constrained-host execution lane

Plan 077 closes its provisioning work with a typed no-full-runtime-lane
record. The current host cannot access its Docker daemon and has no QEMU
system emulator; `PR_SET_NO_NEW_PRIVS` is available only for the explicitly
reduced inherited-descriptor diagnostic. No protocol run occurred, so the
support row remains experimental and non-advertised. Plan 078 requires a
separately qualified full-runtime lane.

Plan 078 used the Plan 080-qualified guest but stopped before TCP at the i2pr
pre-protocol RouterInfo stage. That result is not protocol evidence. No
support-ledger status changed and NTCP2 remains experimental and
non-advertised. See [`plans/078-status.md`](../plans/078-status.md) and
[`plans/080-status.md`](../plans/080-status.md).

## Active status correction (2026-08-13)

The active Milestone 4 parent authority is
[Plan 102](../plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
and the
[Plan 102 amendment](../plans/102-amendment-exploratory-tunnel-dependency.md)
clarifies that live RouterInfo lookup through a direct router
transport is not a substitute for the standard exploratory-tunnel
path. Plan 103 (RouterInfo validation + bounded local NetDB) has
landed as the new runtime-neutral `i2pr-netdb` workspace crate
(`crates/i2pr-netdb/`) with cryptographic/temporal validation, a
bounded in-memory store with deterministic
replacement/conflict/expiry, peer-selection primitives, and local
signed RouterInfo construction. The Plan 103 closure record is
`plans/103-status.md`. The next executable implementation is
Plan 104 (persistent cache + SU3 reseed trust path), followed by
Plan 105 → Plan 106 → Milestone 5 exploratory tunnels → Milestone
4B external acceptance. Plan 106 closes only the local/bootstrap
implementation phase.

The retained Milestone 3 forward-direction closure lane was governed by
[Plan 095](../plans/095-ci-host-loopback-live-wire-evidence-lane.md), with
[Plan 096](../plans/096-plan095-ci-workflow-correctness-and-pre-dispatch-closure.md),
[Plan 097](../plans/097-plan095-artifact-path-and-cleanup-corrective-pass.md),
and
[Plan 098](../plans/098-plan095-runner-provenance-boundary-corrective-pass.md)
as closed corrective passes that restored execution correctness before
the next authoritative dispatch. The Plan 082 prepare / validate-scenario
surface is implemented and closed; Plans 083 and 084 are implemented and
reclassified as execution-pending. The Plan 084 historical
`lane-invalidated` closure is reclassified as "runner implementation
completed; required reverse wire execution never occurred" and the
active development decision now lives in `plans/088-status.md`. The
Plan 078/080 attempt stopped pre-protocol and did not produce a TCP,
NTCP2, authenticated-frame, or I2NP result. Plan 082 prepares authentic
i2pr state and real RouterInfo/hash/run-identity fields, the Rust
`validate-scenario` command parses the strict live scenario without opening
a peer, and the mixed runner asserts both peer identities and the frozen
run identity before any live process. This changes diagnostic ownership
only; it does not change any support row.

Plan 085 introduced the bounded `host-loopback-development` topology
kind that allows literal IPv4 loopback protocol execution on the
constrained host. Plan 086 enabled the lane and proved a listener-only
preflight; Plan 086 closed as `host-loopback-development-ready` on this
host. Plan 087 ran the first real `i2pr -> i2pd` forward direction under
the development lane; Plan 090, Plan 091, Plan 092, Plan 093, and Plan
094 applied i2pd direct driver corrections and runner/provenance
authority corrections. Plan 094's live closure environment is blocked
on this host, and Plan 095 supersedes that path with a manual GitHub
Actions `ubuntu-24.04` host-loopback evidence lane. The first
authoritative Plan 095 manual CI dispatch on 2026-08-10 advanced through
the full contract/build/forward-instrumented job graph but failed
closed with `terminal_result = pre_protocol_rejected /
pre-protocol-preparation-failed` before any TCP or NTCP2 wire activity.
Plan 098 reclassified that result as a pre-protocol runner/provenance
failure (the runner reconstructed a non-authoritative
`target/debug/i2pr-interop` path instead of consuming the
wrapper-supplied `--i2pr-binary` path) and corrected the
runner/provenance ownership boundary before any future dispatch. Plan
088 runs the reverse `i2pd -> i2pr` direction and issues the active
development decision; on this host the recorded Plan 088 decision
remains `insufficient-evidence` until Plan 095 closes with a passing
instrumented and a passing control forward record from the same CI
evidence pair.

NTCP2 remains experimental and non-advertised, and Plan 079 remains
blocked pending the Plan 088 decision. Plan 072 remains inactive. It
requires a real wire-stage i2pr/i2pd disagreement that source and
specification review cannot own, plus
`decision = ambiguous-reference-divergence` and one exact diagnostic
question in [`plans/088-status.md`](../plans/088-status.md). The
historical `lane-invalidated` and `same-stage-two-way-i2pr-defect`
tokens are forbidden by the static boundary checker. Preparation-only
and pre-protocol results cannot activate Emissary or change this
support ledger. The
[Plan 072/079 gate amendment](../plans/072-079-gate-amendment-plan-088.md)
records the active gate authority.

The current status of the active sequence is:

```text
plan_099 = passed-pruning-and-exit
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_098 = passed-runner-provenance-boundary-correction (historical)
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = deferred-to-pre-activation-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = next
```

### Plan 095 CI host-loopback live-wire evidence lane

[Plan 095](../plans/095-ci-host-loopback-live-wire-evidence-lane.md)
implements the GitHub Actions `ubuntu-24.04` host-loopback live-wire
evidence lane that runs the Plan 086 `host-loopback-development` topology
on a fresh VM. The lane is **development-only**; it never satisfies a
release or isolation qualification and cannot become a Milestone 3
certificate. The workflow lives at
`.github/workflows/ntcp2-interop-host-loopback-development.yml` with a
manual `workflow_dispatch` trigger only, `contents: read` permissions,
and the contract/build/forward-instrumented/forward-control/validate-gate
job sequence.

The CI environment blocker vocabulary is bounded
(`ci_binary_execution_blocked`, `ci_loopback_bind_blocked`,
`ci_loopback_connect_blocked`, `ci_reference_build_blocked`,
`ci_artifact_transfer_blocked`, `ci_disk_space_blocked`,
`ci_unexpected_runner_environment`); CI inability is never reported
as a protocol failure. Plan 095 supersedes the Plan 094 assumption that
the Plan 046 rootless sealed-namespace lane or the Plan 048/049
Multipass guest must become runnable before development-only forward
evidence can close.

### Plan 096/097/098 Plan 095 corrective passes

[Plan 096](../plans/096-plan095-ci-workflow-correctness-and-pre-dispatch-closure.md)
closed four demonstrated workflow defects before the first authoritative
dispatch: explicit i2pr build path, disjoint sanitized evidence,
embedded Python import audit, and canonical tracked-source digest. The
static regression matrix `test_plan096.py` (36 cases) and the
pre-dispatch audit `scripts/check-plan095-workflow.sh` are green
locally.

[Plan 097](../plans/097-plan095-artifact-path-and-cleanup-corrective-pass.md)
closed two narrow workflow defects that remained after Plan 096:
artifact-path ownership (one canonical absolute `BUILD_OUTPUT` path used
by every producer, verifier, manifest generator, artifact uploader, and
live consumer) and disposable run-root cleanup (strict `rm -rf --` with
an exact path guard and an unsuppressed absence assertion). The static
regression matrix `test_plan097.py` (45 cases) and the extended
pre-dispatch audit are green locally.

[Plan 098](../plans/098-plan095-runner-provenance-boundary-corrective-pass.md)
closed the runner/provenance ownership boundary that the first
authoritative Plan 095 dispatch exposed on 2026-08-10. The forward,
reverse, and preflight runners now accept an explicit `i2pr_binary: Path`
argument and rehash the supplied file bytes against `i2pr_binary_sha256`
before any subprocess launch. The wrapper threads the exact
caller-supplied `--i2pr-binary` path to every runner, exposes
`--attempt-kind` for instrumented/control role binding, and refuses
role/binary mismatches. The i2pr and i2pd build-manifest digests are
independently measured; the runner no longer aliases a generic manifest
digest into both artifact classes. The Plan 095 final gate validates
record digests against the actual downloaded artifacts and role-specific
manifests. The static regression matrix `test_plan098.py` (15 cases) is
green locally. The 2026-08-10 result is reclassified as a pre-protocol
runner/provenance failure with no TCP or NTCP2 wire conclusion.

### Plan 099 Milestone 3 interop exit, harness reduction, and router buildout

[Plan 099](../plans/099-ntcp2-interop-exit-harness-simplification-and-router-build-unblock.md)
is the corrective and exit plan from the multi-job CI/provenance
expansion. It corrects the central Plan 099 implementation finding
— the instrumented i2pd transport libraries were never actually
compiled from the patched source tree — and it freezes
interoperability architecture growth. The build script now produces
two separate i2pd archive sets (`I2PD_INSTRUMENTED_LIB_DIR` and
`I2PD_PRISTINE_LIB_DIR`), and the pristine control driver uses
native `Transports::SendMessage`, `Transports::IsConnected`, and
`TransportSession::IsEstablished` instead of observer APIs the
control build cannot emit. The development exit gate vocabulary is
bounded to three values: `passed`, `protocol-defect-localized`,
`environment-or-harness-blocked`.

[Plan 100](../plans/100-plan099-exit-gate-cleanup-and-router-handoff.md)
is the one-time active cleanup authority that repairs the exit gate
(D1, D2), hardens the i2pd observer proof (D3), and removes the
divergent source-tree digest fallback (D4). The Plan 099 single-job
CI workflow was dispatched exactly once from the Plan 100 correction
commit and the bounded replacement runs consumed two narrow direct
corrections before the bound forward-instrumented attempt reached
authentic post-TCP protocol evidence.

After Plan 100:

```text
plan_099 = closed-protocol-defect-localized
plan_100 = passed-exit-cleanup-and-handoff
plan_095 = historical-superseded-by-plan099-single-job-lane
plan_087 = historical-development-sequence-superseded-by-plan100
plan_088 = historical-development-sequence-superseded-by-plan100
plan_079 = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = next
development_interop = protocol-defect-localized
exact_wire_stage = noise_authenticated
external_netdb_over_ntcp2 = blocked
```

Plan 099 deleted all Plan 052–098 plan-number-specific Python test
and runner files after migrating unique functional assertions into
the bounded functional test set
(`test_execution_lane.py`, `test_i2pd_direct_driver.py`,
`test_i2pd_direct_control.py`, `test_minimal_i2pd_probe.py`). The
`scripts/check-ntcp2-interoperability.sh` static boundary check
was trimmed from 1870 to 124 lines and now enforces only durable
invariants (NTCP2 remains experimental/non-advertised, the
production daemon does not accidentally activate NTCP2, the direct
reference driver is test-only, no public-network/reseed/SAM/I2CP
fallback in the development smoke, the pinned reference revision
exists, and functional interop tests exist). The
`scripts/check-plan095-workflow.sh` and
`scripts/check-ntcp2-loopback-smoke-boundary.sh` scripts were
removed entirely. The CI workflow file was reduced from 988 lines
to a single `development-interop` job and now performs build and
execute in the same fresh job with no cross-job binary artifact
transfer. The i2pd driver build script now produces two separate
instrumented and pristine archive sets, the driver CMake consumes
them through explicit `I2PD_INSTRUMENTED_LIB_DIR` and
`I2PD_PRISTINE_LIB_DIR` variables, and Plan 100 D3 hard-asserts
that the pristine archive carries exactly zero observer references
and the instrumented archive carries at least one.

Plan 100 does not enable NTCP2 in normal daemon operation, does not
advertise NTCP2 in production RouterInfo, does not depend on NTCP2
for real NetDB peer exchange, and does not authorize public-network
bootstrap. Plan 079's 3/3 repeated-direction validation campaign is
moved to the pre-normal-activation / pre-public-network integration
checkpoint rather than gating offline/local router development.

## Plan 102 Milestone 4 RouterInfo/NetDB authority and the Plan 102 amendment (active roadmap)

[Plan 102](../plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
is the active Milestone 4 parent authority that supersedes the
historical Milestone 3 "active" blocks for the purpose of
continuing router development. The retained Plan 099/100/101
NTCP2 result above is preserved as the authoritative NTCP2
development record. The next substantial product work is governed
by Plan 102 and its child sequence (Plans 103 → 104 → 105 → 106).

### Plan 102 amendment — exploratory-tunnel dependency

[Plan 102 amendment](../plans/102-amendment-exploratory-tunnel-dependency.md)
corrects an over-optimistic wording in the first Plan 102 draft.
The current I2P `DatabaseLookup` operation uses an outbound
exploratory tunnel and requests the response through an inbound
exploratory tunnel; exploratory tunnels are Milestone 5 scope.
Therefore a standards-conformant live RouterInfo lookup cannot
complete inside the Plan 103–106 implementation sequence merely
by re-entering NTCP2 or another direct router transport.

The authoritative Plan 102 sequence is:

```text
Plan 103  RouterInfo validation + bounded local NetDB
   -> Plan 104  persistent cache + SU3 reseed trust/ingestion
   -> Plan 105  transport-neutral lookup/store/publication state machines
   -> Plan 106  daemon/bootstrap integration
   -> Milestone 5 exploratory tunnel substrate
   -> return to Milestone 4B external acceptance
```

Plan 106 closes the local/bootstrap implementation phase, not
the complete original Milestone 4 exit criteria. After Plan 106
closes, Milestone 4A is
`local-foundation-complete-external-transport-blocked` until
Milestone 5 supplies exploratory inbound/outbound paths and a
router transport is deliberately qualified. A direct
`DatabaseLookup` over NTCP2 is not accepted as a substitute for
the standard exploratory-tunnel path. The next executable
implementation remains **Plan 103** (RouterInfo validation and
local NetDB foundation).
