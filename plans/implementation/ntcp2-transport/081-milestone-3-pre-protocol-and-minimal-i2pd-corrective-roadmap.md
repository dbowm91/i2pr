# Plan 081: Milestone 3 pre-protocol and minimal i2pd corrective roadmap

## Status and authority

- Status: planned, active corrective roadmap.
- Date: 2026-08-01.
- Parent authority: Plan 067, ADR 0023, and Plan 074.
- Predecessor state: Plans 075, 076, 077, and 080 are implemented; Plan 078 closed without a protocol pass.
- Active children: Plans 082, 083, and 084.
- Plan 079 remains blocked until Plan 084 closes with the required development decision.
- NTCP2 remains experimental, non-advertised, and disabled in normal daemon operation.

## Corrected problem statement

Plan 080 established two useful facts:

1. the owned Multipass guest can provide a qualified full-runtime, loopback-only execution lane; and
2. the source-locked i2pd 2.60.0 driver can produce a real signed RouterInfo through the real linked i2pd implementation.

Plan 080 did **not** establish an NTCP2 protocol incompatibility. The attempted direction stopped before TCP connection, Noise authentication, SessionConfirmed processing, authenticated frame transfer, or I2NP decoding.

The active pre-protocol defects are:

1. the strict Plan 065 launcher scenario requires real local/reference Router Hashes and a real run-identity digest before the live launcher starts;
2. the mixed runner supplies empty values for those required fields;
3. the reverse-direction generation path attempts to use an unallowlisted `-gen` live scenario instead of a dedicated state-preparation operation;
4. broad exception translation collapses scenario-render, state-preparation, launcher-start, and export failures into `typed-harness-operation-failed`;
5. process counters may report a process as started through fallback accounting even when launch did not occur;
6. the current evidence pipeline is too broad to serve as the first diagnostic tool for this defect.

The resulting failure is a harness/launcher composition defect. It is not yet evidence that i2pr NTCP2 is conformant or non-conformant.

## Decision: i2pd is sufficient for the current gate

The current development gate requires one independent implementation, not Java+i2pd+Emissary simultaneously.

Use the existing real Plan 076 i2pd driver as the primary reference because it already provides:

- source-locked i2pd 2.60.0 linkage;
- real NTCP2 listen and dial paths;
- real RouterInfo import/export;
- real DeliveryStatus construction and submission;
- passive post-AEAD and post-I2NP-decode observations;
- an uninstrumented control binary;
- a prepared cache in the qualified Plan 080 guest.

Emissary remains conditional. It is activated only after a real i2pd wire attempt reaches an ambiguous protocol stage that cannot be owned through the I2P specification and pinned i2pd source review. It is not a substitute for correcting i2pr state preparation because every external peer still requires a valid i2pr RouterInfo.

## Objectives

This roadmap must:

- expose a dedicated test-only i2pr state-preparation operation;
- produce and validate i2pr RouterInfo before rendering a live scenario;
- derive real i2pr and i2pd Router Hashes from the prepared state;
- freeze a small canonical run-identity record before live process launch;
- render the strict Plan 065 live scenario with real nonzero correlation fields;
- remove the fake `-gen` live-scenario path;
- classify failures by the actual pre-protocol or protocol stage;
- run one stripped-down real `i2pr -> i2pd` probe before invoking the broad historical evidence pipeline;
- run the reverse `i2pd -> i2pr` probe after the first direction reaches a meaningful wire result;
- decide whether Plan 079 may begin, whether a narrow i2pr correction is required, or whether Plan 072 should be conditionally activated;
- reuse the qualified Plan 080 guest and Plan 076 driver rather than reopening environment provisioning.

## Plan decomposition

### Plan 082: i2pr state preparation and mixed-runner contract correction

Owns:

- `i2pr-interop ntcp2 prepare` or an equivalently narrow preparation command;
- reuse of the existing local-state construction and RouterInfo signing code;
- a bounded preparation status record;
- `I2prAdapter.prepare_state()`;
- real Router Hash derivation and strict RouterInfo validation;
- canonical run-identity freezing;
- valid Plan 065 scenario rendering;
- removal of the `-gen` live-scenario path;
- precise pre-protocol failure categories;
- truthful process accounting.

Plan 082 contains no interoperability-pass requirement. It closes when a live scenario can be rendered from authentic prepared state and a controlled no-peer run reaches the expected listener/dial boundary without schema or state-preparation rejection.

### Plan 083: minimal i2pr-to-i2pd wire probe

Owns one real direction only:

```text
i2pr initiator -> i2pd responder
```

The probe must bypass the broad Plan 045/052 release-style finalization path and report the highest authentic stage reached:

```text
state_prepared
peer_router_info_imported
tcp_connected
noise_authenticated
session_confirmed_accepted
authenticated_frame_written
authenticated_frame_decrypted
i2np_delivery_status_decoded
```

Plan 083 closes with either:

- a real authenticated DeliveryStatus decode by i2pd; or
- a precise reproducible failure at the first real protocol stage.

A pre-protocol failure returns to Plan 082 and does not close Plan 083.

### Plan 084: reverse i2pd-to-i2pr probe and development decision

Owns:

```text
i2pd initiator -> i2pr responder
```

It also owns the development decision:

- `two-way-development-probe-passed` -> unblock Plan 079;
- `one-way-passed-reverse-defect` -> create a narrow owning correction, then repeat the affected direction;
- `same-stage-two-way-i2pr-defect` -> correct the i2pr protocol/runtime owner before Plan 079;
- `ambiguous-reference-divergence` -> conditionally activate Plan 072;
- `lane-invalidated` -> return to Plan 077/080 lane ownership only when the environment proof actually changed.

## Dependency graph

```text
Plan 081 roadmap
      |
      v
Plan 082 state preparation + runner correction
      |
      v
Plan 083 minimal i2pr -> i2pd probe
      |
      v
Plan 084 i2pd -> i2pr probe + decision
      |
      +--> Plan 079 repeated i2pd validation, when two-way probe is adequate
      +--> narrow protocol correction, when a real owned defect is observed
      +--> Plan 072, only for unresolved reference divergence
      +--> Plan 073, later release qualification only
```

## Active execution environment

Reuse the Plan 080 owned Multipass lane when it remains available and its ownership/environment contract validates.

Required reuse checks:

```text
instance ownership contract valid
environment manifest digest matches
source transfer or source commit is explicit
reference cache manifest matches
rootless sandbox probe still passes
nftables/no-public-network marker still holds
artifact digests are re-measured for the new source commit
```

Do not require the instance name to remain unchanged. A fresh owned guest is acceptable when the preserved instance is unavailable, but the same Plan 080 lane implementation should be used.

Do not reopen:

- rootful Docker discovery;
- QEMU installation;
- rootless-host enablement;
- bubblewrap or rootless container research;
- a new remote CI matrix;
- repeated Multipass architecture redesign.

A lane refresh is allowed only when the existing guest is unavailable or its ownership/attestation contract is invalid.

## Evidence boundary

Plans 082-084 use a development diagnostic record, not a release bundle.

Minimum record fields:

```text
schema
run_id
source_commit
direction
reference = i2pd
reference_revision
lane_qualification_sha256
i2pr_binary_sha256
i2pd_binary_sha256
i2pr_router_info_sha256
i2pd_router_info_sha256
i2pr_router_hash_sha256
i2pd_router_hash_sha256
delivery_status_message_id
highest_stage_reached
terminal_result
reason_code
process_counters
cleanup_result
record_sha256
```

No passed record may infer a stage from process survival, port availability, or a later stage on the other process.

The existing broad evidence pipeline may be reintegrated after Plan 084. It must not block the first real wire diagnosis.

## Stage authority

Use these minimum authorities:

| Stage | Required authority |
| --- | --- |
| `state_prepared` | preparation command plus signed RouterInfo validation |
| `peer_router_info_imported` | owning reference/i2pr import result |
| `tcp_connected` | owning socket/transport event, not port probe |
| `noise_authenticated` | i2pr terminal handshake state and/or real reference authenticated event |
| `session_confirmed_accepted` | responder-side authenticated handshake completion |
| `authenticated_frame_written` | successful real transport write completion |
| `authenticated_frame_decrypted` | receiver post-AEAD observation |
| `i2np_delivery_status_decoded` | receiver post-I2NP conversion with exact nonzero message ID |

Later stages may imply earlier protocol stages only within the same owning process when the source-verified event location makes the implication unavoidable. The diagnostic record should still preserve every directly observed event.

## Global implementation rules

1. Do not weaken the strict Plan 065 live-scenario schema to accept empty hashes or empty run identity.
2. Do not add preparation-only IDs to the primary live-scenario allowlist.
3. Do not create a second protocol implementation or synthetic peer to prepare i2pr state.
4. Reuse the existing `prepare_local_state` logic or extract it without changing RouterInfo/crypto semantics.
5. The preparation command must not open a listener, dial a peer, or claim authentication.
6. Private keys, RouterInfo bytes, I2NP bytes, transcripts, addresses outside the synthetic test contract, and raw peer errors remain unexported.
7. Local raw diagnostics may contain stack traces only inside the disposable guest/run root and must be deleted or retained under the existing explicit raw-local policy.
8. Process counters increment only after successful process creation.
9. A protocol failure must not be relabeled as environment blocked.
10. An evidence-finalization defect must not overwrite the actual protocol result in the minimal probe.
11. Do not add recurring CI or a large matrix.
12. Do not touch NetDB, tunnels, SAM/I2CP, SSU2, daemon activation, or public-network behavior.

## Roadmap acceptance criteria

Plan 081 is complete as a planning artifact when:

- Plans 082-084 exist;
- the active sequence is registered in a Plan 067 amendment;
- Plan 080's protocol-defect interpretation is corrected without deleting its historical environment evidence;
- Plan 079 explicitly depends on Plan 084 rather than the failed Plan 078 attempt;
- the existing Plan 080 lane and Plan 076 i2pd driver remain the selected route;
- Emissary is explicitly conditional rather than mandatory;
- each child plan has ownership, work packages, acceptance criteria, stop rules, validation, non-goals, and small-model guidance;
- no child plan expands into another evidence architecture or environment research cycle.

## Handoff order

Execute exactly one plan at a time:

1. Plan 082;
2. Plan 083;
3. Plan 084;
4. Plan 079 only after the Plan 084 decision permits it.

The next implementation model must not execute a protocol run while Plan 082 still supplies empty correlation fields or uses a fake generation scenario.