# Plan 385 status — blocked; Plan 386 owns reverse publication

Status: **blocked-reverse-els2-publication-not-visible-to-stock-i2pd-plan-386**.

Plan: `plans/implementation/i2pcontrol-proposal-170/385-ordinary-lookup-and-els2-reverse-publication-corrective.md`.

Implementation/evidence commit: `23c0e81` (`Plan 385: fix ordinary ELS2
authority lookup lane`). This commit contains the ordinary lookup fix, live
authority and reverse driver rows, sanitized evidence checker/runner updates,
and focused regression coverage. The closure and successor registration are
recorded separately after this implementation commit.

Plan 385 corrects the post-start ordinary lookup path and proves its authority
payload, but does not meet the required reverse ELS2 matrix. The exact-pinned
NONE, PSK, and DH attempts all reached the reverse control-created server and
stock i2pd returned `CANT_REACH_PEER` / `LeaseSet not found`. Plan 386 owns the
unresolved publication/DHT visibility boundary and the replacement matrix.

## Requirement-to-evidence matrix

| Requirement | Evidence | Outcome |
|---|---|---|
| Ordinary post-start `DelayOpen` listener requests product activation instead of leaving its supervisor parked | Production branch in `service_tunnels.rs`; checker mutation `ordinary delay-open clients stop reaching product activation`; exact live authority row | Passed; the reference standard-LS2 lookup and payload now complete after startup |
| Ordinary client resolution consumes only its requesting runtime's validated mirror | `plan214_resolve_remote_client_target_hit_returns_validated_keys`; checker mutation `ordinary resolution stops using the validated cached target`; live authority row | Passed |
| Post-start authority payload with one-attempt lookup | `authority-b32-payload-returned` in all three mode runs | Passed for NONE, PSK, and DH runs |
| Existing i2pd-publishes → i2pr-consumes consumer control | `application-payload-returned` | Passed in NONE, PSK, and DH runs |
| Control-created i2pr type-5 server commits | `reverse-server-create` | Passed in PSK and DH runs; no-auth creation also reaches the reverse lookup in the NONE run |
| i2pd consumes i2pr-published ELS2 and returns fixture payload | `reverse-payload-returned` | **Failed** in NONE, PSK, and DH: stock i2pd reports `CANT_REACH_PEER` / `LeaseSet not found` |
| Complete reverse authentication matrix | Three exact-pinned runs, attempt budget 1 | **Not passed; no reverse payload row passed** |
| Mesh controls and evidence credential scrubbing | each mode's results rows | Passed in all three runs |

The driver preserves the server authorization public key separately from the
consumer credential: PSK uses the shared key, while DH publishes the derived
X25519 public key and supplies the private key through the pinned i2pd SAM
client-auth group. A failed row remains failed; local address projection and
control create do not count as publication proof.

## Commands and evidence

Build prerequisite and focused checks:

- `rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed; no crates needed rebuilding.
- `rtk cargo fmt --all --check` — passed before the final documentation-only edits.
- `rtk cargo test --locked -p i2pr-daemon --test i2pcontrol_els2_black_box -- --test-threads=1` — passed, 5 tests.
- `rtk cargo test --locked -p i2pr-daemon --lib plan214_resolve_remote_client_target_hit_returns_validated_keys -- --test-threads=1` — passed, 1 test.
- `rtk bash tests/integration/els2/run-i2pd-els2.sh --self-test` — passed before the final driver diagnostic edits.
- `rtk python3 scripts/check-els2-live-lane-evidence.py --self-test` — passed.
- `rtk python3 scripts/check-els2-live-lane-evidence.py --mutation-table` — 12 mutations detected, 0 missed; 2 controls accepted.
- `rtk bash scripts/check-els2-live-lane-evidence.sh` — passed static driver/runner/property checks.
- `rtk bash scripts/check-encrypted-service-consumer-caller.sh` — passed.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed before registering Plan 386; rerun at handoff.
- `rtk git diff --check` — passed before the planning closure edits.
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, 51 tests. The workflow-validity guard emitted its expected fixture notice that a temporary repository lacked `.github/workflows`; the suite result was still `OK`.

Live commands, each with a separate ignored evidence directory:

- `I2PR_ELS2_EVIDENCE_DIR=/home/sugarwookie/projects/i2pr/target/interop/els2-evidence-plan385-none rtk bash tests/integration/els2/run-i2pd-els2.sh` — driver failed closed after the authority payload passed; reverse returned `LeaseSet not found`. Packaged `results_sha256`: `0376951c01f9653fa558049139e55b04dee0257921cfd30104f77ecae76553c9`.
- `I2PR_ELS2_AUTH_MODE=psk I2PR_ELS2_EVIDENCE_DIR=/home/sugarwookie/projects/i2pr/target/interop/els2-evidence-plan385-psk rtk bash tests/integration/els2/run-i2pd-els2.sh` — driver failed closed after the consumer and authority payloads passed; reverse returned `LeaseSet not found`. Packaged `results_sha256`: `435f0b38f9baeb7700d9d85d340697613a36726d8d03aa38e6c1c0d098129ae2`.
- `I2PR_ELS2_AUTH_MODE=dh I2PR_ELS2_EVIDENCE_DIR=/home/sugarwookie/projects/i2pr/target/interop/els2-evidence-plan385-dh rtk bash tests/integration/els2/run-i2pd-els2.sh` — driver failed closed after the consumer and authority payloads passed; reverse returned `LeaseSet not found`. Packaged `results_sha256`: `2b0bdb07aed72e6d47b5527045b6ce88602337c600c318724c60136f4503edb6`.

The `results_sha256` values are the runner's packaged hashes over the evidence
rows before the final `evidence-packaged` self-attestation row. All three
evidence directories are under ignored `target/interop/` and contain sanitized
rows only. Raw i2pd logs and credentials are not committed. The routine floor
was not run because every required reverse payload row failed; no CI result is
claimed.

## Invariant, failure, lifecycle, compatibility, and secret review

- All live attempts used unmodified i2pd 2.61.0 at
  `635b013a612ff47278ef02acf8580a28e10e26c5`; the runner's frozen
  `MAX_ATTEMPTS=1` was unchanged.
- The standard authority result follows the existing bounded product activation
  and one-attempt ordinary lookup. No retries were added. Cancellation remains
  wired to both service and admission cancellation tokens in the delayed
  ordinary client loop.
- No persistent format, public control schema, Proposal 170 inventory,
  transcript profile, or Plan 380 credential seam changed. Type 5 remains
  non-advertised; `support.toml` and full-conformance flags remain unchanged.
- The reverse SAM session remains open while the separate SAM `STREAM CONNECT`
  socket uses its ID. This avoids invalidating the transient session when the
  session socket closes.
- Authorization secrets are not written into evidence. PSK and DH values are
  passed to the consumer only through the mode-specific SAM option; the server
  receives only its mode-specific authorization entry. Runner key-scrub rows
  passed for all three modes.
- Failure is fail-closed: reverse lookup does not report success based on a
  local encrypted address or a committed tunnel definition. No queue, timer,
  retry budget, or public reachability guarantee was widened.

## Findings

- **Medium — i2pr-published type-5 LeaseSet is not found by stock i2pd.** The
  failure reproduces across NONE, PSK, and DH after a committed control create.
  The SAM result does not yet distinguish missing publication, wrong DHT key,
  store rejection, or retrieval failure. Plan 386 owns that diagnosis and the
  repair. No conformance or advertisement claim follows from the local rows.
- **Low — routine floor and CI not run.** The plan's mandatory live reverse
  payload gate failed, so downstream full-floor acceptance remains open.

## Unblock audit and disposition

Plan 385 is **blocked**, not passed. Its ordinary lookup defect is fixed and
the post-start authority payload is proven, but the reverse matrix remains
unqualified. Plan 386 is registered as the concrete corrective successor.

| Future plan | Remaining hard dependencies | Can unblock now? | Disposition |
|---|---|---|---|
| 374 | reverse i2pd-consumes-i2pr payload matrix | No | remains blocked; Plan 386 owns the missing rows |
| 375 | Java pinned-source proof and Java live driver/matrix | No | remains blocked; this i2pd result does not discharge Java evidence |
| 377 | Plans 374 and 375 passed | No | remains blocked on both inputs |
| 378 | Plan 377 passed | No | remains blocked on Plan 377; `full-proposal-conformant` stays unset |

No dependent plan is unblocked by partial infrastructure or by the passing
authority control. Plan 384 remains a blocked historical record; Plan 385 does
not rewrite it.
