# Plans 038–042 operations reference

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
- `tools/i2pr-interop/`: non-production launcher seam; the current checkout
  composes bounded state preparation, listener/dial, handshake, authenticated
  link, and DeliveryStatus smoke. Its success is local driver validation only.
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
does not make an i2pr claim. `handshake-smoke` and `full` now invoke the
bounded runtime-owned i2pr launcher path. A successful launcher result is
local driver validation only; the reference profile still requires
authenticated data exchange and cleanup, not TCP or listener readiness alone.
The current runner returns `i2pr-mixed-router-profile-not-wired` for those
profiles until it connects the launcher to the reference adapter.

Plan 042's selected smoke scope is DeliveryStatus (I2NP type 10): a 12-byte
body, 21-byte NTCP2/SSU2 short transport message, and 24-byte NTCP2 block
before frame overhead and padding. Require one valid outbound and one valid
inbound message per direction. No reference echo behavior is currently proven,
so this remains a bounded plan scope rather than interoperability evidence.

The launcher status meanings are fixed: schema-1 `i2pr-interop-status` records
use fixed phase, result, reason-code, and aggregate counters; `listen` readiness
is separate from a later authenticated terminal result, `dial` has one
terminal result, and `inspect` returns only redacted metadata. Typed state,
authentication, data-phase, timeout, and cleanup failures are terminal results,
never readiness or evidence.

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
`i2pr-mixed-router-profile-not-wired` means the reference runner has not
connected the launcher to a reference adapter. Rejected
configuration/state, authentication, timeout, cleanup, and evidence-validation failures remain
typed and visible. Never convert them to pass or omit them from the closure
record. An empty evidence directory is not success; Plan 041 reference-pair
records are harness controls, not i2pr mixed-router evidence.

```text
bash scripts/interop/validate-evidence.py
bash scripts/check-ntcp2-interoperability.sh
sudo -E bash scripts/interop/cleanup.sh
```

Retain only sanitized typed JSON records and approved hashes. If cleanup is
uncertain, stop and investigate the disposable host before any later run.
