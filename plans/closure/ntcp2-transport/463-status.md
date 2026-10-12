# Plan 463 status: blocked — pinned Explore does not select the seeded record

Closure token: `blocked-plan463-stock-explore-candidate-not-selected`

Plan: [`463-controlled-floodfill-stock-trigger-and-live-reply.md`](../../implementation/ntcp2-transport/463-controlled-floodfill-stock-trigger-and-live-reply.md)

## Outcome

The default-off controlled-loopback profile is implemented. It requires fixed
loopback NTCP2 and SSU2 endpoints and an active floodfill owner, signs one
dual-address RouterInfo from the same identity and transport keys, publishes
only `caps=f`, and restores a no-`f` record on withdrawal. Ordinary floodfill
eligibility is unchanged. Focused configuration, builder, floodfill-state,
and signature tests passed.

The exact-pinned i2pd 2.61.0 lane started the ordinary daemon, observed the
controlled floodfill activation, exported the signed RouterInfo, seeded only
that record into a fresh stock NetDB, and started an unmodified pinned stock
process with `reservedrange=false`. The stock process stayed alive and entered
its ordinary `NetDbRequests::Explore` path four times in the finite attempt
window. It produced no accepted i2pr DatabaseLookup and no correlated
DatabaseSearchReply. The lane exited with its negative result; no stock-to-i2pr
capability is claimed.

Source inspection of commit `635b013a612ff47278ef02acf8580a28e10e26c5`
confirms `Explore` selects through `NetDb::GetClosestFloodfill`, then calls
the ordinary stock transport directly when no exploratory tunnels are
available. `GetClosestFloodfill` only searches the floodfill set and excludes
RouterInfos marked unreachable, profiles marked unreachable, and excluded
identities. New floodfill-set admission additionally requires the floodfill
role, the pinned version threshold, and a published IPv4 address (or the
documented reachable/published IPv6 case). The implementation record was
constructed to meet those published record fields, but the stock run did not
expose whether the entry was rejected at parse, floodfill-set admission, or
selection. Thus the first missing observable transition is **stock candidate
selection**, and its internal reason remains unlocalized. The earlier
source-only assumption that a signed `caps=f` loopback record was sufficient
is not validated by the live process.

No further retry is justified until a bounded stock-owned receipt identifies
that exclusion reason or a legitimate, source-supported trigger is established.
Do not work around this with a fake profile, fabricated reachability/version,
patched reference, direct helper send, or altered normal eligibility.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Explicit default-off dual-loopback profile | Implemented; parser rejects missing, mismatched, non-loopback, ephemeral, and unsupported transport endpoints. |
| One signed identity and RouterInfo with live NTCP2 and SSU2 keys | Focused signature/endpoint test passed. |
| `caps=f` only, no `R` or bandwidth tier | Implemented and unit tested. |
| Withdrawal removes `f` and preserves only still-live addresses | Implemented; NTCP2 owner shutdown restores the SSU2-only record. |
| Pinned stock source trigger | Source path confirmed, but live Explore did not select the seeded record. |
| Fresh stock NTCP2 initiation, authenticated i2pr attribution, live DatabaseLookup consumer and correlated DatabaseStore reply | Not observed. |
| Wrong identity/netId/key, stale record, disabled profile, no-trigger and teardown matrix | Not run because the positive source-owned selection gate failed. |
| Plan 459/460 closure | Remains blocked; this plan does not replace their status records. |

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rtk cargo fmt --all --check` | Passed before the final script diagnostic adjustment; rerun in handoff floor. |
| Focused config, floodfill, dual-RouterInfo, and local netdb builder tests | Passed individually (1, 1, 1, and 2 tests respectively). |
| `rtk python3 tests/integration/ntcp2/plan463_controlled_floodfill_stock_trigger.py` | Bounded run reached stock Explore four times; accepted lookup and correlated reply both 0; exit 3. |
| Exact-pinned source inspection of `NetDbRequests::Explore`, `HandleExploratoryTimer`, `NetDb::GetClosestFloodfill`, `RouterInfo::IsEligibleFloodfill`, and address parsing | Confirmed direct-send path and the candidate gates above. |

The raw stock logs, temporary identities, and temporary NetDB were removed by
the lane. Only sanitized counts are retained here. No reference source or
binary was modified.

## Security and compatibility

- No public address, reachability claim, bandwidth tier, support inventory, or
  conformance claim was added.
- NTCP2 and this qualification profile remain default-off, loopback-only, and
  non-advertised.
- No stock source, public network, fake peer result, or helper-owned transport
  send was used.

Disposition: **blocked at stock floodfill candidate selection**. Resume after
the exact exclusion reason is established without weakening the stated
protocol or architecture boundaries.
