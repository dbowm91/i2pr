# Plan 048 status: Multipass rootless evidence environment

## Status

The Plan 048 implementation surface is present, but the external evidence
ladder is not closed on this host. The required canonical Multipass instance
name is already occupied by a running instance that was not provisioned by
the Plan 048 cloud-init contract. The lane therefore fails closed with the
typed blocker `blocked_instance_name_collision`; it does not replace or
destroy that instance implicitly.

The direct host probe independently remains the Plan 046 negative baseline:

```json
{"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}
```

No host AppArmor or user-namespace policy was changed. No i2pr mixed-router
evidence was produced, and no Plan 048 closure record is claimed.

## Implementation surface

- `scripts/interop/multipass/` provides the strict manifest, cloud-init,
  source/cache transfer, offline enforcement, probe, scenario/matrix runner,
  sanitized export, snapshot/restore, cleanup, and host-state verification
  workflow.
- `docs/adr/0018-multipass-rootless-interop-environment.md` records the
  guest-only recovery architecture and evidence boundary.
- The rootless supervisor, topology, probe, and source-identity paths now
  preserve typed attestations and verified transferred-source identity when
  running without a guest `.git` directory.
- The harness includes deterministic tests for manifest parsing, source/cache
  integrity, instance collision handling, export rejection, shell strictness,
  and cloud-init policy.

## Validation on this checkout

The deterministic harness lane passes:

- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p
  'test_*.py'` — 154 tests, 2 expected skips.
- `bash scripts/check-rootless-interop-boundary.sh` — passes.
- `bash -n scripts/interop/multipass/*.sh` — passes.

The Multipass client is installed as version 1.16.3. The live recovery lane
was not started because `i2pr-interop-rootless` already exists; this is a
reproducible external-state blocker, not a protocol result. A future run may
use the documented explicit replacement/destroy workflow only after the
owner of the existing instance authorizes that state change.

Plan 048 does not advertise NTCP2 support and does not close Milestone 3.
