# Plan 056 candidate commit freeze

Status: **retired; never used for an authoritative external run** (Plan 058).

The historical candidate SHA was `fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf`.
It is not eligible for future Milestone 3 evidence because the supported
execution path (Plan 059) and the new candidate freeze (Plan 060) are
not yet closed. A successor candidate may be cut only under Plan 060
after Plan 059 closes.

This document is preserved as an audit record of the Plan 056
implementation surface. The measured historical fields below were
captured at the original freeze date and are not part of the current
M3 evidence chain. The repository does not track the
`target/interop/evidence/plan056/` diagnostic bundles; they were
generated locally under the ignored working tree and are described by
the bounded local-diagnostic receipt

```text
tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json
```

with `artifact_storage = local-untracked`. The on-disk evidence
directory is not a tracked committed artifact.

No external Run A or Run B was produced from this candidate. The
candidate must not be used by Plan 060 tooling; the plan 058
candidate record validator returns `active_candidate_record == False`
for any record carrying `status: retired`.

## Historical snapshot (preserved verbatim)

The following fields were measured at the original freeze date
(2026-07-29) and are preserved for auditability. They are not a
candidate declaration.

- Retired historical SHA: `fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf`
- Verifier commit referenced by the original freeze document:
  `1eb6cd640ce3c3e5141b62910fcae8d42f72c54a`
- Historical floor commit (Plan 056 verifier and tests):
  `2457b74a0a129e8ef2aedd3abcd4883925f5b376`

The narrative in the Plan 056 closure record originally described
the local diagnostic bundles as "committed" under
`target/interop/evidence/plan056/`. The repository does not track
this directory. The accurate description is that the bundles were
generated locally under the ignored working tree, and the canonical
wording is corrected in `plans/056-closure.md` as part of Plan 058.

## Why this candidate is retired

Plan 058 identified the following defects that invalidate this
candidate for any future external Milestone 3 evidence:

1. The candidate was frozen before the reference-side implementation
   surface required by the four-direction predicate (Plan 059) was
   implemented. A candidate frozen before the implementation floor
   cannot be the authoritative source for a direction that the
   implementation does not yet support.
2. The Plan 057 execution contract inherited this candidate and
   required missing helper and topology artifacts. It is now
   superseded by Plan 058 and Plan 060.
3. The Plan 056 closure narrative described locally generated
   diagnostic bundles as committed evidence. The Plan 058 record
   integrity invariant forbids this claim, and the corrected
   closure record replaces the wording with the explicit
   `local-untracked` storage classification.

The Plan 058 candidate record (`tests/integration/ntcp2/harness/candidate_record.py`)
rejects consumption of this candidate by Plan 060 tooling: the
`active_candidate_record` helper returns `False` for any record whose
`status` is `retired`, and the static boundary check
`scripts/check-ntcp2-interoperability.sh` enforces the same invariant
for the entire repository.

## Historical freeze commands (preserved verbatim)

The original freeze executed every validation command listed below
on a clean checkout at the retired candidate SHA. Plan 058 does not
re-run those commands and does not re-affirm them as current
validation. The current validation commands are documented in
`AGENTS.md` and `scripts/check-ntcp2-interoperability.sh`.

```bash
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan055.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan052.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'

bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

## Historical measurement table (preserved verbatim)

The historical measurement table from the original freeze is
preserved below for auditability. Every value is a historical
measurement that was correct on 2026-07-29; it is not a current
candidate declaration. The candidate record validator
(`tests/integration/ntcp2/harness/candidate_record.py`) does not
consume this table.

| Field | Historical value |
| --- | --- |
| `source_commit` | `fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf` |
| `source_commit_object_sha256` | `71609e118cce7fe632ca2ed9bcd0af6bfd807c3fe0be1a79b084117769f57266` |
| `source_tree_sha256` | `da9f8db742ad24dd325cbd3d4168365b2c792246982959819fe5fd4db1002d72` |
| `source_archive_sha256` | `244c102caac9816f7e131ca2483d0bd7ace471ac550f9dfb062db1d982b80c2f` |
| `source_archive_format` | `git-tar` |
| `source_dirty` | `clean` |
| `host_source_manifest_sha256` | `0350ac40178836ff24e972fae8240f83d8ee45c5115495a43da060e2e955b388` |
| `reference_lock_sha256` | `943af1f7af3ba5f3df52c499cfd386be4b76cb2f650218c174981b114f4121ef` |
| `environment_manifest_sha256` | `0cfbff8615e48f7ee17bd038a0fa852c9c0095b8fc74055214486a77197c07b4` |
| `topology_kind` | `rootless-sealed-single-netns` |
| `privilege_model` | `unprivileged-userns` |
| `rustc_version` | `rustc 1.95.0 (59807616e 2026-04-14)` |
| `cargo_version` | `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` |
| `target_triple` | `x86_64-unknown-linux-gnu` |

## Notes on this host

The host is the Plan 046 `host.apparmor-restrict-on` negative
baseline. The Plan 046 rootless sealed-namespace probe returns the
typed blocker `blocked_unprivileged_user_namespace` on this host.
The Plan 058 execution-lane contract documents two alternative
execution lanes that resolve this host-side blocker:

- Lane A (direct-host): the execution host itself must report
  `rootless_sandbox_available`. This host does not.
- Lane B (guest): the outer host records its baseline (this one)
  as `blocked_unprivileged_user_namespace` and the Multipass
  recovery guest must report `rootless_sandbox_available`. The
  outer-host baseline does not reject a valid guest lane.

The retired candidate is independent of the chosen lane: it is
ineligible for any future external evidence regardless of lane.
