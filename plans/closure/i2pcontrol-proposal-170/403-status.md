# Plan 403 closure — blocked on DH consumer activation attribution

Status: **blocked-authorized-dh-consumer-stops-before-lookup-plan-404**.

Plan: `plans/implementation/i2pcontrol-proposal-170/403-reverse-els2-consumer-secret-option-corrective.md`.

## Requirement-to-evidence

| Requirement | Evidence | Result |
|---|---|---|
| Use pinned i2pd remote consumer parameter | Reverse SAM session now passes the 32-byte mode secret through `i2cp.leaseSetPrivKey`; Plan 401/402 source attributions and Plan 403 closure plan cite `Destination.cpp:88-97`, `498-503`, and `LeaseSet.cpp:661-706`. Publisher-only requester options removed. | Pass |
| Guard exact requester profile | `scripts/check-els2-live-lane-evidence.py` requires `leaseSetPrivKey`, rejects publisher-only auth options, and has negative mutation rows. | Pass |
| Reverse PSK | Run `target/interop/els2-evidence-plan403-psk-privkey-20261009`: all controls and i2pr authority row passed; reverse fixture payload passed; runner exited successfully. Hashes: evidence `6bcf85f96947db7f0b5534fd542a14411aaf4492847694b18ba46a1df1e3d11a`; results `7594bf7bd3a26a574b7776f463487dac7d9449b7842cac2386f1e7f7dda66c6b`; driver `95dd9e4f87c62813756b5890b5d7da22ff100bc1b1924c2a4f096de98986688e`. | Pass |
| Reverse DH and shared i2pr consumer controls | Run `target/interop/els2-evidence-plan403-dh-privkey-20261009`: reference mesh and standard authority control passed, but the i2pr authorized consumer returned no banner; `encrypted-target-status=None`, counters remained zero, failed-connects advanced to one. The control invocation also reported one i2pd process aborted during teardown. Key-material scrubs passed. Hashes: evidence `413eeeca008fe43c1c208792767d07ffae78bbde0a7541912be4ab526309986a`; results `b8764d24e55570649017030f253d6668c63cb156b0ab80a3d8c8be98a1642bb4`; driver `e3f06627ff5e7df755de22b33f01bf2e6ef2e8a4470955893062a69022448cd0`. | Blocked; no DH success claim |

## Commands and outcomes

- `cargo fmt --all --check` — pass.
- `python3 scripts/check-els2-live-lane-evidence.py` — pass.
- `bash tests/integration/els2/run-i2pd-els2.sh --self-test` — pass.
- `bash scripts/check-els2-live-lane-evidence.sh` — pass.
- `bash scripts/check-encrypted-service-consumer-caller.sh` — pass.
- `python3 scripts/check-global-plan-number-uniqueness.py` — pass.
- `cargo check --locked -p i2pr-daemon --test els2_i2pd_external` — pass.
- Exact-pinned Plan 403 PSK and DH runs each used `MAX_ATTEMPTS=1`; DH attempt retained as failed.
- Full routine floor not run because the required reverse DH matrix row remains incomplete.

## Security, compatibility, disposition

Only the external requester SAM parameters changed; no production crypto or wire
behavior changed. Evidence scrub passed for both PSK and DH. PSK is qualified only
for the explicit type-7 requester. DH is not qualified. Plan 404 owns attribution of
the DH consumer path before a product change or additional mode attempt. Plans 374/375
remain blocked; 377/378 remain blocked. No support/conformance/advertisement change.

## Unblock audit

Plan 404 is registered to diagnose the named DH activation boundary. No other
registered plan becomes dependency-ready from Plan 403. Plan 400 remains blocked
until its authorized reverse rows and required full acceptance floor are complete.
