# Plan 049 closure record: lifecycle ownership corrective pass

## Scope

This record closes the local orchestration-correctness work for
`plans/049-multipass-lifecycle-ownership-corrective-pass.md`. It does not
claim external interoperability closure or Milestone 3 support promotion.

Starting checkout: `e9a9993` (`main`). The final commit is recorded by the
handoff commit that contains this record.

## Corrective contract

The implementation provides:

- stable environment identity separate from run and instance identity;
- lowercase bounded run IDs and collision-resistant instance allocation;
- atomic pre-launch lifecycle reservation and validated transitions;
- per-run locking and generation-aware state;
- cryptographically linked host/guest ownership records;
- explicit adoption, resume, recreation, destruction, and inspection;
- structured Multipass state normalization, including deleted/unpurged state;
- no automatic mutation of unowned instances and no global purge;
- independent host-baseline and guest-rootless probe outcomes;
- early and final guest probing before router execution;
- sanitized environment blockers and shared evidence attribution;
- static, unit, fake-Multipass, and full harness coverage.

## Evidence classification

The host baseline was `blocked_unprivileged_user_namespace`. A fresh
Multipass guest was allocated under a generated instance name and reached
root-owned ownership files; no router process started. The first recovery
attempt returned `blocked_cloud_init_failed` before the guest probe, and
cleanup correctly stopped at the typed
`blocked_deleted_instance_requires_purge` boundary rather than invoking a
global destructive operation. The legacy colliding instance was not
mutated.

This is a typed environment outcome. It is not Java I2P/i2pd interoperability
evidence, does not satisfy the Plan 045 directional predicates, and does not
alter `specs/support.toml`.

## Commands and results

The exact local commands and results are maintained in `plans/049-status.md`.
The full repository Rust and documentation gates remain part of the final
handoff; any failure is corrected before commit. The target host's remaining
deleted/unpurged resource requires an operator-level selective purge outside
the no-global-purge lane.

NTCP2 remains experimental and non-advertised, and Milestone 3 remains open.
