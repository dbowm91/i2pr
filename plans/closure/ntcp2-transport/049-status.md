# Plan 049 status: Multipass lifecycle ownership corrective pass

## Local implementation status

The Plan 049 lifecycle correction is implemented locally. The authoritative
path now separates the stable environment ID
`i2pr-plan048-rootless-v1`, a generated safe run ID, and a bounded concrete
instance name with generation tracking. It reserves an atomic host lifecycle
record before launch, uses an OS-backed per-run lock, injects a root-owned
guest ownership contract during provisioning, and requires the linked token
hash and contract before adoption, snapshot, restore, or destruction.

Normal operations do not use the legacy fixed instance name, silently adopt,
delete unowned resources, or invoke global `multipass purge`. The command
surface includes explicit `--inspect`, `--resume-owned`, `--adopt-owned`,
`--recreate-owned`, `--destroy-owned`, `--destroy-after-export`, and
`--keep-on-blocker` operations. Environment blockers are sanitized and kept
separate from protocol evidence. Direction records and aggregate validation
bind the environment contract, run, generation, ownership-record digest, and
guest probe outcome.

## Host execution outcome

The target host has Multipass 1.16.3 and the direct host baseline remains the
expected negative result:

```json
{"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}
```

The pre-existing legacy instance was inspected read-only and remains
untouched because its Plan 048 ownership contract could not be proven. A
fresh generated run reached a guest with root-owned ownership files and
`ownership_verified`; the first interrupted recovery path returned the typed
`blocked_cloud_init_failed` outcome before the guest probe and did not start a
router. Subsequent inspection showed the guest ownership files were present,
but no protocol phase was entered.

An owned deleted generation was intentionally not globally purged. The
cleanup operation returned `blocked_deleted_instance_requires_purge`, leaving
the selective-purge decision to an operator. No raw RouterInfo, identity,
key, endpoint, transcript, log, payload, or secret-bearing run root was
exported.

The fresh guest recovery result is therefore an environment blocker, not a
protocol result. No four-direction i2pr/reference evidence was produced and
Plan 049 external closure remains unclaimed.

## Local validation

- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'` — 159 tests, 2 skips.
- `bash scripts/check-multipass-interop-boundary.sh` — passed.
- `bash scripts/check-rootless-interop-boundary.sh` — passed.
- `bash -n scripts/interop/multipass/*.sh` — passed.
- `cargo fmt --all --check` — passed.
- `cargo check --workspace --all-targets` — passed.
- `cargo test --workspace` — 219 tests passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps` — passed.
- dependency-direction, runtime-boundary, and NTCP2 evidence-boundary checks — passed.

NTCP2 remains experimental and non-advertised; Plan 049 does not close
Milestone 3.
