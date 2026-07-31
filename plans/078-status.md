# Plan 078 status — preflight blocked

## Status

Plan 078 stops at preflight with the typed blocker
`full_runtime_lane_unavailable`. No protocol process was started, no Level 1
direction record was emitted, and no NTCP2 interoperability result is claimed.
The plan remains open for a future host or manually qualified remote lane;
Plan 079 must not start from this result.

The selected capability was:

```text
selected_lane = inherited-descriptors-seccomp
scope = reduced-scope-diagnostic
full_runtime_lane = unavailable
qualified = false
reason_code = full_runtime_lane_unavailable
```

## Preflight evidence

The exact source commit was:

```text
b706a396a72b7b36ee50db0fc7c11c24139fa2ce
```

The pinned i2pd source revision remains:

```text
f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e
```

The Plan 077 probe selected the reduced-scope capability because Docker's
daemon was inaccessible and QEMU TCG was absent. The remote workflow is
manual-only and therefore was not treated as a qualified lane. The resulting
sanitized records are ignored working data:

```text
probe.json sha256         = 84412c024c1f6c9c23129e3743d515b245df5dc82a72f69bbf7c019408c71e7a
qualification.json sha256 = 654f3f3c078342592f633e9046548c04a183dcee10fdf700521fde83e95370d8
qualification record_sha256 = b6ae500f18ddaa3aff739b029380c6816dfdac27910a241899d86b1aa317cb9c
```

The qualification record has empty artifact digests because no lane executed.
Consequently, no i2pr or i2pd binary digest exists for a Plan 078 protocol
run. The tracked i2pd driver inputs were measured for provenance review:

```text
driver source sha256       = cb1c0a0d8681179928252265b8cf83ce3abd26dfa469afb888d470e7538133e1
observer header sha256     = 77ca510439ac430362f15a0dd0006318963d22810d91f077ee20e7a13066d12b
observer source sha256     = 1be6e5db85aed063ba5d88aa252801b1e26275a0120c5e21628def0c2cbf6b28
observer patch sha256      = e1feef9cca60d3c184db3b79ee56f073e5916938d38be48ba7f462b0875b9595
run-driver.sh sha256       = e274ab9052ae9b1afaad1225e9c79651ac6f000692b43221a2ee599139ffe5df
source-lock.json sha256    = fd321bc7750209554ecf2e836d2f23112be291db99fcda12e42c611ec3053a68
manifest schema sha256    = e4be635d00c6aa074d58d61ad9f5e3fe0762741f3202fc5bfc5acc7bea7037bc
```

The pre-existing Multipass instance was inspected read-only and rejected: its
ownership contract is not verified and its guest source manifest points at a
different historical commit. It was not adopted, resumed, modified, or
destroyed.

## Exact commands and results

```text
bash scripts/interop/probe-constrained-host-lanes.sh
  passed; selected inherited-descriptors-seccomp

bash scripts/check-constrained-host-lane-boundary.sh
  passed

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
  passed; 18 tests

bash scripts/interop/multipass/run-evidence-lane.sh --inspect --run-id plan049-20260721105135-3d7e7a68
  inspected read-only; ownership/contract verification failed; no mutation

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
  passed; 51 tests
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
  passed; 35 tests
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
  passed; 13 tests
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
  passed
```

The loopback tests are contract tests only. They did not launch a real
reference process and are not Plan 078 protocol evidence.

## Corrections and closure boundary

No protocol correction was made because the required full-runtime lane was
unavailable and the stop rule prohibits starting a run without it. No schema,
orchestration framework, isolation path, or synthetic result was added. The
existing Plan 077 fail-closed preflight is the owning correction for this
host state. While running the CI-equivalent static checks, the build-contract
validator exposed that `offline-reuse.sh` built the launcher without the
pinned `+1.95.0` toolchain marker. The script now uses
`cargo +1.95.0 build --locked --package i2pr-interop`; this is a preparation
contract correction, not protocol evidence.

Level 2 repeated validation, negative controls, Java, Emissary, release
certification, support advertisement, and normal NTCP2 enablement remain open
and out of scope.
