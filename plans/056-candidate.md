# Plan 056 candidate commit freeze

Status: **declared; not yet executed**. The Plan 056 two-bundle
external evidence closure pass freezes this exact source commit as the
candidate for both Run A and Run B. The candidate SHA, all measured
digests, and the validation command set below must remain stable for
the duration of the execution. If any digest drifts before either run
finalizes its bundle, the run is invalidated and a new candidate must
be cut from `main`.

## Candidate SHA

```text
fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf
```

The candidate was cut on top of commit `2457b74` (the Plan 056
verifier and tests freeze commit) plus the
`interop: tolerate gitlinks and non-regular entries in source tree
digest` fix that allows `plan052_pipeline.build_measured_identity`
to run on the current checkout (the `.agents/skills` gitlink
otherwise raises `source-tree-file-missing`). Both commits are
immutable and the candidate SHA must remain stable for the duration
of the run.

Branch: `main`. Working tree status: clean.

## Source provenance (measured)

| Field | Value |
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

## Pinned reference digests

- Java I2P `2.12.0` at `2800040deee9bb376567b671ef2e9c34cf3e30b6`
  (locked in `tests/integration/ntcp2/references.lock.toml`,
  revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`).
- i2pd `2.60.0` at `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`
  (locked in `tests/integration/ntcp2/references.lock.toml`).

## Helper / catalog digests (to be measured at run time inside the guest)

- Java template tree digest — measured by
  `scripts/interop/java-prepare-template.py` at run time.
- Reference observation catalog
  (`tests/integration/ntcp2/reference-observation-catalog.toml`,
  sha256 `92b8d2e23826877ad2e7f8b73d4f2cbc4bdd752bacebec6b90fdce43a75fb275`)
  and reference-trigger contracts
  (`tests/integration/ntcp2/reference-trigger-contracts.md`,
  sha256 `145db563030642dad3ee3d2e78bb184fef852dab460e2df43bb263ca036c3c3d`).
- i2pd and Java reference trigger helpers (Plan 055 B5 / C5 decisions) —
  measured and finalized at run time inside the guest; their source
  digests must be identical between Run A and Run B.

## Environment contract (Multipass recovery lane)

- `environment_id`: `i2pr-plan048-rootless-v1`
- Image: `24.04`, amd64 / x86_64.
- Guest execution user: `i2ptest` (no `sudo`, no ambient capabilities).
- Required topology: `rootless-sealed-single-netns`,
  `unprivileged-userns`.
- Manifest: `scripts/interop/multipass/environment.toml`,
  sha256 `e13d6340ac9f25cd455fc96d637807727aed1d8734449fa1791c6eb9e7186780`.
- One active evidence guest at a time on the constrained host.

## Validation commands (executed at freeze)

All commands are run from the repository root at the candidate commit.

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

## Local validation result

The candidate commit `2457b74` was produced on `main` after Plans
053, 054, and 055 are merged. The full Plan 052/053/054/055/056
pipeline (`plan052_pipeline.py`, `evidence_bundle.py`,
`observation.py`, `trigger_record.py`, `verify_milestone3_certificate.py`)
is committed. Every validation command listed above passes on this
checkout:

- `cargo fmt --all --check` passes.
- `cargo check --workspace --all-targets` passes.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  passes.
- `cargo test --workspace` passes (227 tests across 27 suites).
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps` passes.
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p
  'test_*.py'` passes (390 tests across the Plan 052/053/054/055/056
  harness surface, including the 18 new Plan 056 certificate checks).
- All seven boundary checks (`check-dependency-direction`,
  `check-runtime-boundaries`, `check-fixture-manifest`,
  `check-ntcp2-vectors`, `check-ntcp2-interoperability`,
  `check-rootless-interop-boundary`,
  `check-multipass-interop-boundary`) pass.

## Operator and date

- Operator: Plan 056 two-bundle closure pass author.
- Freeze date (UTC): 2026-07-29.

## Notes on this host

The host is the Plan 046 `host.apparmor-restrict-on` negative
baseline. The Plan 046 rootless sealed-namespace probe returns the
typed blocker `blocked_unprivileged_user_namespace` on this host.
The canonical external path is the Plan 048/049/050/051 Multipass
recovery lane, which provisions a disposable Ubuntu 24.04 amd64
guest with `apparmor_restrict_unprivileged_userns=0`. Any Plan 056
run must be produced inside that lane; the host itself cannot
satisfy the Plan 040 host contract for the Plan 046 lane.

The Plan 046 closure record is preserved verbatim; this freeze does
not modify it.
