# Plan 038 operations reference

Run commands from the repository root. The authoritative harness instructions
are in `tests/integration/ntcp2/README.md`; this reference is a compact routing
guide for an agent.

## Files to inspect

- `tests/integration/ntcp2/references.lock.toml`: Ubuntu contract, source pins,
  build commands, and the exact IzPack SHA-256.
- `tests/integration/ntcp2/scenarios/*.toml`: the eight bounded i2pr/reference
  scenario definitions. Keep their IDs synchronized with
  `tests/integration/ntcp2/manifest.toml`.
- `tests/integration/ntcp2/reference-scenarios/`: the separate Plan 041 pair
  schema and the two directional Java I2P/i2pd control scenarios.
- `tests/integration/ntcp2/harness/`: Python topology, adapters, process
  bounds, runner, and evidence code.
- `scripts/interop/`: host setup, builders, isolation, matrix, and cleanup.
- `tools/i2pr-interop/`: non-production launcher seam; it currently reports
  `blocked_missing_driver` for listen/dial.
- `target/interop/evidence/`: sanitized records only; `target/interop/runs/`
  is secret-bearing and is deleted after every run.

## Host and build gates

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
sudo bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
```

Use `build-references.sh --offline` only with a complete prepared cache. The
builders reject dirty or mismatched source trees and record per-build hashes.
Do not substitute packaged routers or floating revisions.

The only reference identifiers are `java_i2p` and `i2pd`. Cache resolution uses
`target/interop/cache/current-cache.json` and a strict metadata schema; it does
not recursively search for a matching text substring.

## Profiles

```text
sudo -E bash scripts/interop/run-matrix.sh --profile environment-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile reference-crosscheck-ipv4
sudo -E bash scripts/interop/run-matrix.sh --profile handshake-smoke
sudo -E bash scripts/interop/run-matrix.sh --profile full
```

`environment-smoke` checks reference startup, disposable RouterInfo production,
and cleanup. `reference-crosscheck-ipv4` runs both Plan 041 reference-pair
scenarios, validates the explicit private network ID and RouterInfo exchange,
and requires authoritative authenticated observations from both references; it
does not make an i2pr claim. `handshake-smoke` and `full` require the complete
runtime-owned i2pr NTCP2 wire adapter; until it exists, the correct result is
`blocked_missing_driver`, not a substituted self-handshake.

The Plan 041 runner serializes reference-pair executions with a host-local
lock. Its emergency cleanup owns the dedicated `java-*`/`i2pd-*` namespaces and
their short `jv…`/`iv…` veth names.

For one bounded run, use:

```text
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-java-ipv4 --reference java_i2p
sudo -E bash scripts/interop/run-scenario.sh --scenario smoke-i2pd-ipv4 --reference i2pd
```

## Result interpretation and cleanup

`blocked_host_contract` means no router process or protocol claim was made.
`blocked_missing_driver` means the requested i2pr wire path is not complete.
Typed failures, cleanup failures, and evidence validation failures must remain
visible. Never convert them to pass or omit them from the closure record.

```text
bash scripts/interop/validate-evidence.py
bash scripts/check-ntcp2-interoperability.sh
sudo -E bash scripts/interop/cleanup.sh
```

Retain only sanitized typed JSON records and approved hashes. If cleanup is
uncertain, stop and investigate the disposable host before any later run.
