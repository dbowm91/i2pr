# Plan 442 status: blocked — independent SSU2 LAN qualification unavailable

Closure token: `blocked-independent-lan-evidence-unavailable-local-owner-passed`

Plan: `plans/implementation/core-router-recovery/442-ssu2-normal-owner-preparation-and-independent-topology.md`

## Local owner implementation

The local implementation portion passed. The daemon now loads or atomically
creates an SSU2 transport identity in its private router data directory. The
versioned record binds the static X25519 key and introduction key to the
configured RouterIdentity hash, uses exact-length/checksum/public-key
validation, rejects symlinks and unsafe file metadata through the existing
storage policy, and is installed with atomic no-replace semantics. A changed
RouterIdentity fails closed rather than silently rotating or reusing the
transport keys.

The normal daemon SSU2 owner supplies those persisted keys when signing its
controlled loopback RouterInfo before socket startup. The prior ephemeral
identity helper remains available to focused tests. The existing profile still
rejects non-loopback binds, advertisement and introducer service; no address,
capability or public support claim was promoted.

## Evidence

- `Ssu2TransportIdentityStore` round-trip test verifies generated keys survive
  reload, a different router hash is rejected, and truncation, trailing bytes
  and corruption fail closed.
- Unix permission test verifies the persisted identity file is mode `0600`.
- Daemon test verifies the signed RouterInfo SSU2 `s` and `i` values match the
  persisted public/static and introduction keys, and rejects a key record
  bound to another router identity.
- Existing daemon preflight and runtime local SSU2 suites pass, preserving the
  loopback-only profile and real local session lifecycle.

Commands and results:

```text
rtk cargo fmt --all --check                                      passed
rtk cargo check --locked -p i2pr-storage -p i2pr-daemon          passed
rtk cargo test --locked -p i2pr-storage                          29 passed
rtk cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd
                                                                  passed
rtk cargo test --locked -p i2pr-daemon --lib controlled_identity_uses_router_bound_persistent_transport_keys -- --test-threads=1
                                                                  1 passed
rtk cargo test --locked -p i2pr-daemon --test ssu2_daemon_preflight -- --test-threads=1
                                                                  6 passed, 1 ignored
rtk cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
                                                                  10 passed
rtk git diff --check                                            passed
```

## Independent LAN qualification blocker

No separately addressed, authorized two-host or equivalent independently
addressed lab pair was supplied. Consequently the planned source/destination
address checks, independent stock-router sessions, public-address withdrawal,
and restart/failover behavior were not run. Loopback results are local
component evidence only and cannot close Plan 431 or establish external
reachability. The permitted LAN evidence lane remains available when that
infrastructure is authorized; no external attempt was made here.

Plan 442 is recorded as blocked at that external infrastructure boundary, with
its local durable-identity owner completed. Plan 431 remains stopped. Plans
443 and 444 remain independently eligible under their own controlled evidence
gates. `specs/support.toml`, protocol conformance, defaults and advertisement
policy are unchanged.
