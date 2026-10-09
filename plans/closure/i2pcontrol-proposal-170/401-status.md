# Plan 401 closure — blocked pending requester profile correction

Status: **blocked-reverse-authorized-client-keys-not-loaded-plan-402**.

Plan: `plans/implementation/i2pcontrol-proposal-170/401-reverse-authorized-els2-lookup-attribution.md`.

## Requirement-to-evidence

| Requirement | Evidence | Result |
|---|---|---|
| Inspect exact pinned request path | Fetched upstream `PurpleI2P/i2pd` and checked out detached SHA `635b013a612ff47278ef02acf8580a28e10e26c5` in `/tmp/i2pd-plan401`; `libi2pd/Destination.cpp:778-795` accepts the blinded key and queues the lookup; `libi2pd_client/SAM.cpp:847` calls it without authorization arguments. | Pass |
| Determine client auth-key loading | `libi2pd/Destination.cpp:1084-1102` loads `i2cp.leaseSetClient.{psk,dh}` only inside `GetLeaseSetType() == NETDB_STORE_TYPE_ENCRYPTED_LEASESET2`; `GetAuthType()` selects the key group. The Plan 400 test requester supplies auth type and key but omits `i2cp.leaseSetType=5`, so this code leaves `m_AuthKeys` unset. | Pass; i2pr-owned test-driver configuration defect identified |
| Preserve failed live evidence | Plan 400 PSK artifact `target/interop/els2-evidence-plan400-psk-type7-20261009`; `LeaseSet not found` occurred before Garlic ingress. Evidence hash `92a991d1cc22e769e6a591ad80c492d1cb65c77ed7e7e3366699825215484e5`; results hash `80bc12a4d37e4683efadecd0905c373ca9c3e4c7ef0745d8cbce6bbe399cbffb`; driver hash `b16bce0775f461d0aeb9c5bdd136ff4c9f0fb5d632fbb026c93128b5a4a99538`. | Retained; diagnosis narrows it |
| Avoid uninformative duplicate attempts | Both PSK and DH requester setup share the omission; do not spend a DH attempt before correcting the common setup. | Pass |

## Commands and outcomes

- `git clone --filter=blob:none --no-checkout https://github.com/PurpleI2P/i2pd.git /tmp/i2pd-plan401` — pass; detached checkout at the frozen SHA.
- `rg`/`sed` inspection of pinned `Destination.cpp` and `SAM.cpp` — pass; exact symbols and conditional above.
- No production or driver behavior changed in Plan 401. No additional lane run was made because the source proof explained the PSK boundary and the same missing requester property applies to DH.

## Security, compatibility, limitations

No secret or raw runtime log was retained. The fetched source is external read-only material and no external router was modified. The correction is confined to the test requester profile; it does not add crypto support or alter the reference pin. Plan 400 NONE remains passed under explicit SAM signature type 7. Plan 400 PSK/DH remain unqualified until a corrected requester run.

## Disposition and unblock audit

Plan 401 is blocked on Plan 402, which will add `i2cp.leaseSetType=5` to the reverse requester when configuring PSK/DH credentials, guard that condition, and rerun PSK/DH under healthy controls. No other registered plan becomes dependency-ready from this source attribution. Plans 374/375 remain blocked, as do 377/378. No capability, support, conformance, or advertisement status changes.
