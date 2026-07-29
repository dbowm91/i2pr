# Plan 057: cross-host Milestone 3 external evidence run

## Status: superseded before execution by Plans 058, 059, and 060.

Plan 057 is no longer active execution authority. It is preserved
as an audit record of the original ordering that Plan 058
corrected. The original defects that this plan inherited are:

- the candidate was frozen before the reference-side implementation
  surface required by the four-direction predicate was implemented;
- the implementation floor for the candidate was undefined, so the
  candidate could be consumed by tooling that depends on helpers
  that do not exist;
- the direct-host and guest execution lanes were conflated: the
  plan required the physical host to pass the rootless probe and
  also required a Multipass guest;
- the Java minimal support topology (ADR 0021) was required but
  the ADR was Proposed, not accepted.

The Plan 058 record and candidate integrity closure pass splits
this plan into three new plans:

| Plan 058 responsibility | New owner |
| --- | --- |
| record/candidate correction | Plan 058 |
| i2pd direct helper | Plan 059 |
| Java topology and ADR decision | Plan 059 |
| receiver marker qualification | Plan 059 |
| new candidate freeze | Plan 060 |
| two external runs and certificate | Plan 060 |

Plan 058 supersedes this plan. Plan 059 is the active
implementation and qualification pass. Plan 060 is the active
candidate freeze and two-run certificate pass.

## Original plan (preserved verbatim)

The remainder of this document is preserved verbatim as an audit
record of the original execution contract. Do not execute any
command from this document; the commands reference the retired
candidate SHA, the deprecated host-only-and-guest gate, and the
unspecified implementation floor.

### Status and dependencies (original)

- Plan type: follow-up external execution pass.
- Starting branch: `main` after Plan 056 is closed.
- Hard dependencies:
  - Plan 056: certificate verifier, candidate freeze, diagnostic
    bundles, and reviewer record are committed.
  - Plan 046: rootless sealed-namespace lane is the canonical
    authorization gate; this plan reuses it.
  - Plan 048/049/050: Multipass recovery lane is the canonical
    external environment.
- This plan owns the two-run external evidence pass that Plan 056
  could not complete on the Plan 046 negative baseline. It does not
  change the verifier, the candidate freeze, or the Plan 052/053
  bundle pipeline. It is the only path that can close Milestone 3.
- Milestone 3 may close only through this plan (or a successor that
  inherits every Plan 056 invariant).

### Objective (original)

Produce two independently executed, complete, sanitized Plan 052
evidence bundles from one exact clean source commit. Each bundle
must contain four accepted IPv4 mixed-router directions:

1. `i2pr-to-java-ipv4`;
2. `java-to-i2pr-ipv4`;
3. `i2pr-to-i2pd-ipv4`;
4. `i2pd-to-i2pr-ipv4`.

Every direction must prove the same invariants Plan 056 enumerates
(both-side NTCP2 authentication, sender frame emission, receiver
authenticated frame decryption, receiver correlated I2NP decode,
rootless isolation, unchanged parent network state, clean teardown,
immutable bundle verification before and after export). The two
bundles must be independently reproducible and must not share
mutable run state.

The Plan 056 candidate commit and the Plan 056 certificate verifier
are inherited verbatim. No source edits are permitted during the
authoritative runs.

## Host contract (original)

The execution host must satisfy the Plan 040 host contract:

- Ubuntu 24.04 amd64, Bash 4+, UTF-8 locale.
- Non-interactive `sudo` available to the operator.
- Linux namespace and nftables support.
- At least 16 GiB of physical RAM (Plan 051 closed with a typed
  blocker on a 15 GiB host that could not run two qemu guests plus
  the reference build concurrently).
- At least 4 GiB free under `target/`.
- The Plan 046 rootless sealed-namespace probe returns
  `rootless_sandbox_available` — i.e. the host is in the
  `host.no-apparmor-and-userns-allowed` or
  `host.apparmor-restrict-off` category.
- Multipass `1.16.x` or later is installed.
- `multipass launch` can provision a fresh guest with the
  `i2pr-plan048-rootless-v1` environment contract.

The execution host must NOT be the Plan 046 negative baseline
(`host.apparmor-restrict-on`). If the host probe returns
`blocked_unprivileged_user_namespace`, this plan cannot proceed
and the typed blocker remains the canonical answer.

## Phase 1: preflight (original)

1. Verify the host contract above on the candidate execution host.
2. Verify the Plan 046 rootless probe returns
   `rootless_sandbox_available`.
3. Record the Plan 056 candidate SHA `fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf`
   (or the latest Plan 056 freeze successor) and confirm the
   source tree is clean on the execution host.
4. Build the `i2pr-interop` launcher at
   `target/debug/i2pr-interop` and record the launcher binary
   digest.
5. Run every validation command listed in
   `plans/056-candidate.md`. Any failure blocks the run.
6. Refresh the Plan 057 candidate file at
   `plans/057-candidate.md` with the measured run-identity fields.

## Phase 2: environment provisioning (original)

1. Provision one fresh Multipass guest under the
   `i2pr-plan048-rootless-v1` environment contract.
2. Transfer the source archive and the Plan 043 reference cache.
3. Build the pinned Java I2P `2.12.0` and i2pd `2.60.0` references
   inside the guest. Verify the artifact and installed-tree digests.
4. Verify the in-guest rootless probe returns
   `rootless_sandbox_available`.
5. Snapshot the `source-and-cache-ready` state.

## Phase 3: authoritative Run A (original)

1. Generate Run A identity from measured values (recommended ID:
   `plan057-a-YYYYMMDDhhmmss-<8hex>`).
2. Run the four Plan 045 directions through `run-matrix.sh --profile
   handshake-smoke-rootless`. Use direction order:

   ```text
   i2pr-to-java-ipv4
   java-to-i2pr-ipv4
   i2pr-to-i2pd-ipv4
   i2pd-to-i2pr-ipv4
   ```

3. Use the Plan 055 trigger helpers
   (`i2pd-direct-helper`, `java-minimal-support-topology` per
   ADR 0021 after approval) for the two reference-initiated
   directions.
4. For each direction, verify both sides reach
   `ntcp2_authenticated`, the sender emits a frame, the receiver
   decrypts and decodes the I2NP message, the parent network state
   is unchanged, and the topology tears down cleanly.
5. Stop all processes, destroy the topology, verify no residual
   state.
6. Finalize the Run A bundle, export it via `export-evidence.sh`,
   and verify the exported bundle.

## Phase 4: reset (original)

1. Inspect the guest for residual state. Stop or destroy any
   owned instance as required by Plan 049.
2. Provision a new guest generation (or restore the
   `source-and-cache-ready` snapshot on a fresh generation).
3. Re-run the preflight, environment provisioning, and rootless
   probe. If any source, reference, helper, or environment
   manifest digest drifts, stop; the candidate is invalidated.

## Phase 5: authoritative Run B (original)

1. Generate Run B identity from measured values (recommended ID:
   `plan057-b-YYYYMMDDhhmmss-<8hex>`).
2. Use the reversed direction order:

   ```text
   i2pd-to-i2pr-ipv4
   i2pr-to-i2pd-ipv4
   java-to-i2pr-ipv4
   i2pr-to-java-ipv4
   ```

3. Reuse the Plan 055 helpers for the reference-initiated
   directions.
4. Repeat Phase 3, finalize, export, verify.

## Phase 6: cross-run review (original)

1. Run `python3 tests/integration/ntcp2/harness/verify_milestone3_certificate.py
   --run-a target/interop/evidence/milestone-3/<run-a>
   --run-b target/interop/evidence/milestone-3/<run-b>
   --output target/interop/evidence/milestone-3/certificate.json`.
2. The verifier must report `verified: true`. Any failure is a
   typed blocker; the run is invalidated and Phase 3 must restart
   from a new candidate commit.
3. Sign the sanitized reviewer record containing only name/role,
   date, bundle manifest digests, verifier commit, and outcome.

## Phase 7: closure (original)

1. Commit the two sanitized bundles, the certificate, and the
   reviewer record under `target/interop/evidence/milestone-3/`.
2. Update `plans/057-closure.md` with the two-run outcome table,
   the candidate SHA, the verifier commit, the bundle manifest
   digests, the reference revisions, the helper/catalog digests,
   the environment contract summary, and the reviewer record
   digest.
3. Reconcile `specs/support.toml`,
   `docs/protocol-support.md`, `plans/030-milestone-3-closure.md`,
   `plans/056-closure.md`, and the plan-051 status file to reflect
   the bounded Milestone 3 evidence. Do not advertise NTCP2 support
   beyond the bounded IPv4 NTCP2 handshake + DeliveryStatus smoke
   direction. The support-ledger row stays
   `advertised = false` unless a separate advertisement decision
   explicitly changes it.
4. Open a follow-up plan for Milestone 4 (the next plan after the
   bounded IPv4 NTCP2 evidence scope).

## Failure and invalidation rules (original)

This plan inherits every Plan 056 failure rule:

- A direction fails → finish cleanup; write a typed failed/diagnostic
  bundle; do not replace only that direction; if
  code/config/catalog changes are needed, cut a new candidate commit
  and restart Phase 3.
- Run A passes, Run B fails → Run A remains diagnostic evidence;
  restart both runs from a new candidate commit.
- Environment becomes unstable → classify with one of
  `blocked_environment_resource_contract`,
  `blocked_guest_unreachable`,
  `blocked_rootless_sandbox_regression`,
  `blocked_reference_cache_drift`,
  `blocked_parent_network_state_changed`.
- Bundle verification fails → treat the entire run as invalid;
  never repair finalized bundle contents in place.

## Tests required before external execution (original)

The Plan 056 certificate verifier and its 18-test matrix must
remain green at the Plan 057 candidate commit. Add the following
Plan 057-specific tests under
`tests/integration/ntcp2/harness/test_plan057.py`:

- positive fixture with two independent passing bundles;
- cross-run divergence check on real runner-observed
  `i2pr_router_info_sha256` and `reference_router_info_sha256`;
- support topology (`java-minimal-support-topology`) cross-run
  divergence;
- per-direction `ntcp2_authenticated`, `frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`
  positive observations;
- plan056 cross-run divergent field regression.

These tests run from the canonical Plan 052 bundle fixture used by
`test_plan056.py`; they extend rather than duplicate.

## Required final handoff artifacts (original)

- `plans/057-candidate.md` — frozen candidate SHA and measured
  digests for the Plan 057 execution host.
- `target/interop/evidence/milestone-3/<run-a>/` — Run A bundle.
- `target/interop/evidence/milestone-3/<run-b>/` — Run B bundle.
- `target/interop/evidence/milestone-3/certificate.json` —
  Plan 056 verifier certificate.
- `target/interop/evidence/milestone-3/reviewer-record.json` —
  sanitized reviewer record.
- `plans/057-closure.md` — closure document with outcome table,
  digests, environment contract, reviewer record digest, and
  remaining limitations.
- Updated `plans/030-milestone-3-closure.md`,
  `specs/support.toml`, `docs/protocol-support.md`,
  `plans/056-closure.md`, `plans/051-status`.

## Explicit non-claims (original)

- Plan 057 does not modify the Plan 056 verifier, the candidate
  freeze, the Plan 052/053/054/055 bundle pipeline, or the Plan 046
  static boundary checker.
- Plan 057 does not advertise NTCP2 support beyond the bounded
  IPv4 NTCP2 handshake + DeliveryStatus smoke direction.
- Plan 057 does not claim Internet-scale interoperability, SSU2
  support, daemon integration beyond what is wired, anonymity or
  security readiness, broad I2NP coverage, or performance
  qualification.
- Plan 057 does not weaken the typed-blocker taxonomy or any
  static boundary checker.

## Acceptance summary (original)

Plan 057 closes when the Plan 056 certificate verifier reports
`verified: true` on two independent bundles produced from the same
source commit, the reviewer record is signed, every Plan 056
invariant holds, and the closure document is committed. Until then,
Milestone 3 stays open and NTCP2 stays experimental and
non-advertised.
