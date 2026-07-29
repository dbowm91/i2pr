# Plan 056 closure: NTCP2 Milestone 3 two-run external evidence closure pass

## Status

**Closed with a typed host-environment blocker.** Plan 056 is
implementation-complete and gate-clean; the verifier, the test matrix,
the candidate freeze, the local-evidence driver, and the local
diagnostic bundles it produced are committed. The local diagnostic
bundles and certificate were constructed from typed synthetic
blocked-direction inputs, finalized, exported locally, and audited by
the certificate verifier. They are not committed evidence; they were
generated under the ignored `target/interop/evidence/plan056/` working
directory and the only repository-tracked footprint is the bounded
local-diagnostic receipt at
`tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`
with `artifact_storage = local-untracked`. The plan does not advertise
NTCP2 support, does not produce a passing Milestone 3 certificate, and
does not close Milestone 3.

The Plan 046 closure record is preserved verbatim. The Plan 056
closure adds a follow-up plan to perform the cross-host external
evidence run; no follow-up plan is opened here because Plan 047
already enumerates the cross-host recovery categories and a new
follow-up plan is opened below.

## Implementation surface

Plan 056 added the following files in the closure scope (the commit
identifiers are listed at the bottom of this section):

- `tests/integration/ntcp2/harness/verify_milestone3_certificate.py`
  — the canonical two-bundle Milestone 3 certificate verifier. Schema
  `i2pr-milestone3-certificate-v1`. Accepts `--run-a PATH --run-b
  PATH --output PATH`, re-verifies every bundle via the existing
  `evidence_bundle.verify_bundle` helper, then enforces the
  cross-bundle provenance, direction-predicate, and independence
  rules required by the plan. The CLI exits with status `0` only when
  `verified == True`, `3` when the certificate is denied, and `2`
  when a structural failure (missing bundle, manifest corruption)
  raises an exception.
- `tests/integration/ntcp2/harness/test_plan056.py` — the
  certificate verification test matrix (18 tests): one positive
  fixture that produces two independent valid bundles, plus 16
  negative fixtures enumerated in Plan 056 Workstream 6.3
  (same run-id rejected, source commit mismatch, launcher digest
  mismatch, reference digest mismatch, copied observation files,
  missing direction, rejected direction, handshake-only receiver,
  cleanup failure, parent-network change, raw diagnostics, undeclared
  bundle file, support-topology mismatch, unauthorized divergent
  field, missing-bundle failure, allowlisted-divergent acceptance).
- `scripts/interop/plan056_drive_bundles.py` — a local evidence
  driver that constructs two independent Plan 052 diagnostic bundles
  via the synthetic-fallback `write_direction_artifacts` path and
  runs the certificate verifier against them. It is the only path
  that can exercise the verifier end-to-end on this host. It is not
  a substitute for external execution. The driver never opened a
  mixed-router NTCP2 connection.
- `plans/056-candidate.md` — the frozen candidate SHA
  (`fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf`) with measured source
  provenance fields, every validation command, and the local
  validation result. Plan 058 marks this candidate as **retired**;
  the historical fields are preserved verbatim as an audit record.
- `scripts/check-ntcp2-interoperability.sh` — the static NTCP2
  interoperability boundary check now refuses a commit that omits
  `verify_milestone3_certificate.py` or `test_plan056.py`, or that
  drops the locked certificate schema marker. Plan 058 extends this
  checker with the candidate-record integrity invariants.
- Three follow-up pipeline fixes in
  `tests/integration/ntcp2/harness/plan052_pipeline.py`:
  - `_source_tree_digest` skips gitlink (`120000`) entries so the
    pipeline runs against the current checkout (`.agents/skills` is
    a symlink to `.opencode/skills` recorded as a gitlink).
  - `create_context` mkdirs the staging root before writing the
    identity file (otherwise `write_run_identity` creates the
    staging root as a side effect and the subsequent
    `mkdir(parents=True)` raises `FileExistsError`).
  - `create_context` typo fix (`privile_model` → `privilege_model`).

Plan 056 commit identifiers (chronological):

- `2457b74` — `interop: add Plan 056 two-bundle certificate verifier and tests`
- `7a57ec4` — `docs: freeze Plan 056 candidate commit`
- `fbf2cdb` — `interop: tolerate gitlinks and non-regular entries in source tree digest`
- `1b756be` — `docs: refresh Plan 056 candidate SHA after gitlink fix`
- `e178ae0` — `interop: add Plan 056 driver script and fix tempfile import`
- `fc2c8a5` — `interop: pre-clean export target before staging rebuild in driver`
- `56ded55` — `interop: clean staging tree before each Plan 056 bundle build`
- `27dc5b4` — `interop: separate staging and export paths in Plan 056 driver`
- `5b70357` — `interop: mkdir staging_root before writing the identity file`
- `1eb6cd6` — `interop: fix typo in create_context privilege_model`

## Two-run external evidence status

Plan 056 cannot produce two passing IPv4 mixed-router evidence
bundles on this host. The Plan 046 rootless sealed-namespace
evidence lane is closed with the typed blocker
`blocked_unprivileged_user_namespace` on this host because
`kernel.apparmor_restrict_unprivileged_userns=1` confines every
unprivileged user namespace to a restrictive AppArmor policy. The
canonical external path is the Plan 048/049/050/051 Multipass
recovery lane, which provisions a disposable Ubuntu 24.04 amd64
guest with `apparmor_restrict_unprivileged_userns=0`. The
host-side blocker therefore remains the canonical answer for this
host, exactly as Plan 046 documented.

Plan 051 (the prior external validation attempt) closed with a
typed host-environment blocker after the Multipass dispatch lane
repeatedly lost its SSH endpoint under host memory contention. The
host's 15 GiB RAM budget, three Plan 049-owned guests still
consuming reserved qemu memory, and the absence of non-interactive
`sudo` together rule out reproducing the Plan 051 dispatch run on
this host. The Plan 046 and Plan 051 closure records are preserved
verbatim; Plan 056 inherits both blockers.

Plan 056 therefore closes with the same typed blocker pattern as
Plan 046: the implementation is complete, the canonical external
path is enumerated, and the host-level blocker is recorded as
sanitized local diagnostics rather than masquerading as a protocol
pass.

## Local diagnostic evidence

The Plan 056 local-evidence driver was used to construct two
independent Plan 052 diagnostic bundles from synthetic blocked
direction inputs. The bundles were finalized, exported under the
ignored local working directory
`target/interop/evidence/plan056/`, and audited by the certificate
verifier:

```text
target/interop/evidence/plan056/   (locally generated, not tracked)
  run-a/plan056-a-20260729000000-testbundle/
    run-identity.json
    environment/{environment,source-transfer,cache-transfer,
                  offline-transition,parent-network-{before,after}}.sha256
    attestations/{i2pr-to-java-ipv4,java-to-i2pr-ipv4,
                  i2pr-to-i2pd-ipv4,i2pd-to-i2pr-ipv4}.json
    directions/{i2pr-to-java-ipv4,java-to-i2pr-ipv4,
                i2pr-to-i2pd-ipv4,i2pd-to-i2pr-ipv4}.json
    triggers/{i2pr-to-java-ipv4,java-to-i2pr-ipv4,
              i2pr-to-i2pd-ipv4,i2pd-to-i2pr-ipv4}.json
    observations/{i2pr-to-java-ipv4,java-to-i2pr-ipv4,
                  i2pr-to-i2pd-ipv4,i2pd-to-i2pr-ipv4}.json
    cleanup/{i2pr-to-java-ipv4,java-to-i2pr-ipv4,
             i2pr-to-i2pd-ipv4,i2pd-to-i2pr-ipv4}.json
    diagnostics/sanitized-summary.json
    manifest.json
    manifest.sha256
  run-b/plan056-b-20260729000000-testbundle/
    (identical layout, independent run_identity and trigger records)
  certificate/milestone3-certificate.json
  staging/{run-a,run-b}/     (retained only as pipeline working area)
```

The directory above is **not** a tracked repository artifact. The
only committed repository footprint of the local diagnostic effort
is the bounded receipt at
`tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`,
which carries `artifact_storage = local-untracked` and identifies
the driver, the run IDs, the verifier schema, and the
`verified: false` outcome. The Plan 058 candidate record integrity
validator enforces this invariant: any documentation claim that
treats target-tree output as committed evidence fails closed.

The two synthetic bundles pass the existing `evidence_bundle.verify_bundle`
sanity check, carry four primary direction records in each of the
five required artifact classes, share the same frozen source commit
and the same pinned launcher binary digest, differ only in their
`run_id` and `run_identity_sha256` (the allowlisted divergent
fields), and emit the canonical
`diagnostic-complete-not-certificate` result code on every direction.

The certificate at
`target/interop/evidence/plan056/certificate/milestone3-certificate.json`
records the verifier verdict: `verified: false`, with the typed
failure list enumerated per direction (each direction is missing
`ntcp2_authenticated`, `frame_emitted`,
`frame_authenticated_and_decrypted`, `i2np_message_decoded`, and
the attestation is missing `parent_network_state_unchanged: true`).
This is the expected outcome for a diagnostic bundle constructed
from the local harness seam: the verifier cannot issue a passing
certificate without an actual NTCP2 protocol exchange, and the
local seam has no external Java I2P or i2pd to exchange with.

The local-evidence driver is `scripts/interop/plan056_drive_bundles.py`
with the explicit, sanitized command surface:

```bash
python3 scripts/interop/plan056_drive_bundles.py \
    --repo-root . \
    --run-a-id plan056-a-20260729000000-testbundle \
    --run-b-id plan056-b-20260729000000-testbundle \
    --evidence-root target/interop/evidence/plan056
```

It is the canonical reproducible path to exercise the verifier
end-to-end on this host and produces the same diagnostic outcome
on every invocation. It does not exercise the mixed-router handshake;
it exercises the synthetic blocked-direction producer and the
verifier.

## Eight-direction outcome table

| Direction | Run A | Run B | Receiver data phase (A) | Receiver data phase (B) |
| --- | --- | --- | --- | --- |
| `i2pr-to-java-ipv4` | blocked | blocked | not-observed | not-observed |
| `java-to-i2pr-ipv4` | blocked | blocked | not-observed | not-observed |
| `i2pr-to-i2pd-ipv4` | blocked | blocked | not-observed | not-observed |
| `i2pd-to-i2pr-ipv4` | blocked | blocked | not-observed | not-observed |

Every direction in both bundles reports `result: blocked`,
`cleanup: clean`, `receiver_i2np_decoded: not-observed`,
`receiver_frame_decrypted: not-observed`, and
`parent_network_state_unchanged: false`. The verifier reports 40
typed failures (eight per bundle: one `actual_typed_result !=
passed`, one `parent_network_state_unchanged is False`, one
receiver data-phase predicate, one sender frame emission, one
`both_authenticated` predicate). The verifier confirms the
provenance cross-check (identical source commit, source tree
SHA-256, launcher binary SHA-256, reference lock SHA-256,
topology kind, privilege model), the allowlisted divergent fields
(only `run_id` and `run_identity_sha256`), and the four-direction
catalog.

## Reference revisions, helper/catalog digests

- Java I2P `2.12.0` at `2800040deee9bb376567b671ef2e9c34cf3e30b6`.
- i2pd `2.60.0` at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.
- Reference lock `tests/integration/ntcp2/references.lock.toml`,
  sha256 `943af1f7af3ba5f3df52c499cfd386be4b76cb2f650218c174981b114f4121ef`.
- Observation catalog
  `tests/integration/ntcp2/reference-observation-catalog.toml`,
  sha256 `92b8d2e23826877ad2e7f8b73d4f2cbc4bdd752bacebec6b90fdce43a75fb275`.
- Trigger contracts
  `tests/integration/ntcp2/reference-trigger-contracts.md`,
  sha256 `145db563030642dad3ee3d2e78bb184fef852dab460e2df43bb263ca036c3c3d`.
- Environment manifest
  `scripts/interop/multipass/environment.toml`,
  sha256 `e13d6340ac9f25cd455fc96d637807727aed1d8734449fa1791c6eb9e7186780`.
- Source commit: `1eb6cd640ce3c3e5141b62910fcae8d42f72c54a` (the
  Plan 056 verifier-fix commit, which the original candidate
  document fingerprints as `fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf`; the
  pipeline fix and tooling commits followed without changing the
  source tree SHA-256).

## Environment contract summary

- Topology: `rootless-sealed-single-netns`.
- Privilege model: `unprivileged-userns`.
- Required environment: Ubuntu 24.04 amd64 with permissive
  AppArmor (`apparmor_restrict_unprivileged_userns=0`,
  `unprivileged_userns_clone=1`) — the Multipass recovery lane.
- Current host is the Plan 046 `host.apparmor-restrict-on`
  negative baseline; the rootless sealed-namespace probe returns
  `blocked_unprivileged_user_namespace`. Plan 058 documents two
  alternative execution lanes (direct-host and guest); the
  outer-host baseline on this host does not reject a valid guest
  lane.

## Validation commands and results

Every command listed below was executed at the Plan 056 closure
candidate commit and passes locally:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace                  # 227 cargo tests pass
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# 390 python tests pass (1 skipped), including the 18 new Plan 056 tests.
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

Plan 058 extends `scripts/check-ntcp2-interoperability.sh` with the
candidate record integrity invariants defined in
`tests/integration/ntcp2/harness/candidate_record.py`. The post-Plan
058 validator refuses to consume any candidate whose `status` is
`retired` or whose `history_commits` contains a descendant of the
authoritative `candidate_commit`.

## Reviewer record

| Field | Value |
| --- | --- |
| Reviewer | Plan 056 two-bundle closure pass author (acting as operator) |
| Date (UTC) | 2026-07-29 |
| Run A bundle path | `target/interop/evidence/plan056/run-a/plan056-a-20260729000000-testbundle` (locally generated, not tracked) |
| Run B bundle path | `target/interop/evidence/plan056/run-b/plan056-b-20260729000000-testbundle` (locally generated, not tracked) |
| Certificate path | `target/interop/evidence/plan056/certificate/milestone3-certificate.json` (locally generated, not tracked) |
| Tracked receipt | `tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json` (`artifact_storage = local-untracked`) |
| Verifier schema | `i2pr-milestone3-certificate-v1` |
| Verifier commit | `1eb6cd6` |
| Outcome | `verified: false` (diagnostic-complete-not-certificate) |
| Manifest digest (Run A) | sha256 of `manifest.json` inside the run-A export (recomputed by the verifier) |
| Manifest digest (Run B) | sha256 of `manifest.json` inside the run-B export (recomputed by the verifier) |
| Bound scope | Bounded IPv4 NTCP2 handshake + DeliveryStatus smoke direction with synthetic local-only observation |

## Remaining limitations

- No external NTCP2 protocol pass against Java I2P `2.12.0` or
  i2pd `2.60.0` was performed on this host. The Plan 046 rootless
  sealed-namespace lane is closed with a typed host-level blocker
  on this host; the Plan 048/049/050/051 Multipass recovery lane
  is the canonical external path but cannot complete on this
  constrained host (per Plan 051 closure).
- The local diagnostic bundles carry `diagnostic-complete-not-certificate`
  on every direction. They prove the verifier works end-to-end and
  prove the pipeline integration is correct, but they are not
  interoperability evidence.
- Milestone 3 closure requires two reproducible passing bundles
  from the same source commit on a host where the rootless probe
  returns `rootless_sandbox_available`; that host is not this one.

## Statement of bounded evidence scope

The Plan 056 implementation demonstrates that the bounded IPv4 NTCP2
handshake + DeliveryStatus smoke direction can be constructed from
typed synthetic blocked-direction inputs, finalized, exported
locally, and audited by the Plan 052/053/054/055 pipeline on this
host, and that the Plan 056 verifier independently re-checks the
cross-bundle provenance, direction predicates, and independence rules
required by the plan. The two local diagnostic bundles at
`target/interop/evidence/plan056/run-{a,b}/` are reproducible from
the Plan 056 driver and from the same frozen source commit. They
were generated locally and are not committed evidence. They are not
mixed-router interoperability evidence.

## Reconciliation of status documentation

`specs/support.toml` is unchanged. `docs/protocol-support.md` is
unchanged. The NTCP2 evidence status remains
`experimental` and `advertised = false`. The Milestone 3 closure
record (`plans/030-milestone-3-closure.md`) is updated as part of
Plan 058 to reflect the Plan 056 candidate retirement and the
Plan 057 supersession. The Plan 056 candidate document
(`plans/056-candidate.md`) is marked retired; the historical
fields are preserved verbatim as an audit record under the
"Historical snapshot" section. The Plan 055 status record is
updated to note that the Plan 056 verifier and candidate freeze are
committed but the external evidence run is blocked on this host,
and the candidate is no longer eligible for future external
evidence.

## Follow-up plan (cross-host external evidence)

Plan 056 originally opened
`plans/057-cross-host-milestone-3-external-evidence-run.md` to own
the canonical two-run external evidence pass. Plan 058 supersedes
that follow-up plan: Plan 057 is **no longer active execution
authority**. The Plan 058-to-Plan 060 split reassigns:

| Plan 057 responsibility | New owner |
| --- | --- |
| record/candidate correction | Plan 058 |
| i2pd direct helper | Plan 059 |
| Java topology and ADR decision | Plan 059 |
| receiver marker qualification | Plan 059 |
| new candidate freeze | Plan 060 |
| two external runs and certificate | Plan 060 |

Until Plan 060 produces two passing bundles from a fresh
implementation-floor candidate, NTCP2 stays experimental and
non-advertised and Milestone 3 stays open.
