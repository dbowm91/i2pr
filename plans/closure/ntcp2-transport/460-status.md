# Plan 460 status: blocked — normal stock send has no eligible target role

Closure token: `blocked-stock-send-trigger-requires-floodfill-or-transit-role`

Plan: [`460-stock-to-i2pr-ntcp2-without-sam-destination.md`](../../implementation/ntcp2-transport/460-stock-to-i2pr-ntcp2-without-sam-destination.md)

## Outcome

The Plan 459 live-ingress interface was available and exercised before this
investigation. The exact pinned i2pd 2.61.0 binary and source checkout were
verified at commit `635b013a612ff47278ef02acf8580a28e10e26c5`. The retained
Plan 455 i2pr-initiated lower-tier attempt still passed: one authenticated
session, one I2NP DatabaseStore sent, and one stock own-RouterInfo receipt
marker. That evidence is the opposite direction and does not satisfy Plan 460.

The pinned source call graph identifies the missing precondition. Stock
`Transports::PostMessages` can connect only after a normal caller selects a
router identity and `NetDb::FindRouter` yields an eligible record; unreachable,
unreachable-from-local, or too-old non-high-bandwidth records are rejected
before `ConnectToPeer` (`libi2pd/Transports.cpp:498–548`). The normal direct
router-I2NP callers found in this pinned tree select floodfill identities:

- `NetDb::ReseedFromFloodfill` sends RouterInfo DatabaseLookup messages to the
  selected floodfill identity (`libi2pd/NetDb.cpp:503–527`).
- `RouterContext` publishes its own RouterInfo to a selected floodfill
  (`libi2pd/RouterContext.cpp:1505–1521`).
- `NetDbRequests` routes LeaseSet lookup requests to a selected floodfill
  (`libi2pd/NetDbRequests.cpp:485–500`).

The normal stock tunnel path can send a tunnel-build message to a selected
next-hop router (`libi2pd/Tunnel.cpp:120–130`) and router-delivery messages to
their selected peer (`libi2pd/Tunnel.cpp:408–422`). Those paths need i2pr to
operate as the selected transit/tunnel hop for a useful response. The current
ordinary controlled i2pr profile has no transit owner: it publishes
`TransitParticipation::Disabled` and calls only the disabled probe from the
shared owner. The other direct-router option would require an eligible
floodfill role/address, which the loopback controlled profile does not have
and must not fabricate.

**Exact missing selection precondition:** the stock operation must select the
i2pr RouterIdentity as either (a) an eligible floodfill for direct NetDB
router-control traffic, or (b) an enabled transit hop with a live i2pr
TunnelBuild consumer and response. Neither condition exists in the current
normal controlled profile. A custom helper calling the stock transport API,
false floodfill/reachability claims, or enabling a fake product response would
not be valid evidence. Per the plan's stop condition, no stock-to-i2pr or fresh
stock-initiation result is claimed.

This is an evidence blocker, not an NTCP2 protocol failure. Plan 460 remains
blocked until a source-supported stock-owned trigger can target the existing
normal i2pr role without widening the profile or manufacturing a response.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Frozen reference source and binary | Passed: source checkout is clean at the exact 2.61.0 commit; binary reports i2pd 2.61.0. No source changes were made. |
| Plan 455 lower-tier regression | Passed: i2pr initiator authenticated to stock and stock decoded the self-RouterInfo DatabaseStore. This is not stock-to-i2pr evidence. |
| Normal stock operation targeting i2pr's current RouterIdentity | Blocked at the source-owned target-selection precondition above. |
| Stock I2NP consumed by normal daemon | Not run; no eligible source-supported trigger was found for the existing controlled i2pr role. |
| Fresh stock-initiated NTCP2 session | Not run; no normal stock send operation reached peer selection for i2pr. |
| Reply-required stock request and correlated response | Not run; no request reached a live i2pr consumer. |
| Negative matrix | Not run because the positive stock send trigger is absent. Local network-ID/restart and bounded-admission controls are recorded under Plan 459. |

## Commands and outcomes

| Command / inspection | Outcome |
| --- | --- |
| `rtk bash tests/integration/ntcp2/plan455_direct_stock_attempt.sh` | Passed retained lower tier: `authenticated_sessions=1`, `i2np_messages_sent=1`, `stock_database_store_self_drop_markers=1`, `stock_delivery_status_log_markers=0`. No reply was owed for that self-RouterInfo store. |
| `rtk git -C target/interop/cache/ssu2/i2pd-source rev-parse HEAD` | Exact frozen pin `635b013a612ff47278ef02acf8580a28e10e26c5`. |
| `rtk target/interop/cache/ssu2/i2pd/635b013a612ff47278ef02acf8580a28e10e26c5/bin/i2pd --version` | `i2pd version 2.61.0 (0.9.70)`. |
| `rtk rg -n "transports\.Send(Message|Messages)|\.SendI2NPMessages\(" target/interop/cache/ssu2/i2pd-source/libi2pd target/interop/cache/ssu2/i2pd-source/libi2pd_client` | Enumerated pinned call sites; direct router NetDB callers target selected floodfills, while tunnel paths target selected hops/routes. |
| Plan 459 full workspace tests and floor | Passed on the same code tree: 4,828 passed, 39 ignored across 187 suites; workspace clippy, rustdoc, doc tests, boundary/evidence scripts, and `cargo deny` passed. |

No stock source, binary, or repository evidence file was modified. The raw
reference logs and test datadirs were temporary and removed by the existing
runner. The clean exact-pinned source checkout remains under ignored
`target/interop/cache` for source inspection only.

## Security, compatibility, and findings

- Critical/high: none.
- Medium: no source-supported normal stock trigger currently targets the
  default-off, non-floodfill, non-transit controlled i2pr RouterIdentity.
- No public I2P, false `R`/`f`/reachability claim, patched reference, custom
  transport-linked sender, or support-inventory change was used.
- Plan 459 remains blocked at its separate live initiator-role and correlated
  reply requirements. Plan 461 remains blocked on Plans 459/460 actual peer
  receipt and dual-family continuity. Plan 462 is independent and remains
  ready/deferred.

Disposition: **blocked at stock-owned router-send target selection**. Resume
only after a normal stock operation can select the existing i2pr profile with
a real product consumer; otherwise a new plan must first authorize the needed
i2pr transit/floodfill capability and its evidence gates.
