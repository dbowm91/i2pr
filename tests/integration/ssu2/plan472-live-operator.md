# Plan 472 two-host SSU2 qualification operator steps

This is an owner-run private-LAN qualification lane. It does not use SSH,
provision another host, or start a router during `prepare` or `verify`. The
Plan 476 offline receipt simulator checks pair authorization, independent role
receipts, run-epoch agreement, phase order, and replay rejection without
opening sockets or starting processes:

```sh
python3 tests/integration/ssu2/test_plan476_executor.py
```

This simulator is infrastructure evidence only. The local router-process
launcher and attributable live receipt collector are not available yet;
`run --live` continues to fail closed. An offline pass is not a live result.

## Per-host preparation

Use the same temporary Plan 449 inventory on both hosts. Keep it and all
RouterInfo artifacts outside the repository, with owner-only permissions. The
inventory must describe the actual assigned private IPv4 interfaces, distinct
ports, owner consent, attempt budget 1, and the exact i2pd source revision.
Never use public, shared, or unapproved hosts.

On the i2pr host:

```sh
python3 tests/integration/ssu2/plan472_live.py prepare \
  --manifest /private/path/inventory.json --role i2pr \
  --reference-source /private/path/i2pd-source \
  --i2pd-binary /private/path/i2pd
```

On the stock i2pd host, use `--role i2pd` and its local reference paths. A
successful result means only that this host's interface/address, UDP bind,
route source, private data directory, owner consent, and pinned stock build
passed offline checks.

## RouterInfo exchange and verification

Create each host's real signed RouterInfo from its normal private-LAN
configuration and durable transport identity. Exchange the two artifacts by
an owner-controlled authenticated transfer. Do not synthesize or edit signed
RouterInfo bytes. Then verify both artifacts from either host:

```sh
python3 tests/integration/ssu2/plan472_live.py verify \
  --manifest /private/path/inventory.json \
  --i2pr-router-info /private/path/i2pr-router.info \
  --i2pd-router-info /private/path/i2pd-router.info
```

The signed RouterInfo inspector verifies signatures and emits only a bounded
summary. Verification requires network ID 2, the exact declared SSU2 endpoint,
static and introduction keys, and the pinned stock router version. It does not
print endpoint values, keys, identities, or artifact paths.

## Live gate

Do not run a live command until the per-host process/session executor is
implemented and reviewed, both operators have explicitly authorized the
bounded attempt, and the exact signed peer RouterInfo has been exchanged.
Until then, the following command must fail closed after artifact
verification with `live-executor-unavailable` and `packet_attempt: none`:

```sh
python3 tests/integration/ssu2/plan472_live.py run --live \
  --manifest /private/path/inventory.json \
  --i2pr-router-info /private/path/i2pr-router.info \
  --i2pd-router-info /private/path/i2pd-router.info
```

Do not interpret this gate or the Plan 449 preflight as a handshake, I2NP,
restart, withdrawal, or reachability result. Keep routine CI offline.
