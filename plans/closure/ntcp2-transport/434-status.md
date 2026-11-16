# Plan 434 status: blocked — current-pinned i2pd reverse initiator path unavailable

Closure token: `blocked-current-pinned-i2pd-reverse-initiator-runner-unavailable`

Plan: `plans/implementation/ntcp2-transport/434-ntcp2-authenticated-link-discrepancy-recovery.md`

## Source baseline and scope

This bounded diagnostic was activated after Plan 432 closed at `e518214`. The
source audit found an existing Plan 042 launcher implementation, but it is
strictly tied to the synthetic network ID 99. The stock i2pd 2.61.0 executable
is present in the local ignored cache at
`target/interop/cache/ssu2/i2pd/635b013a612ff47278ef02acf8580a28e10e26c5/bin/i2pd`;
its cached metadata identifies source revision
`635b013a612ff47278ef02acf8580a28e10e26c5`. Presence of a router executable
does not provide the missing reverse initiator driver or stage-correlation
contract.

The old NTCP2 reference lock is i2pd 2.60.0 / Java I2P 2.12.0, while the
current repo pins are i2pd 2.61.0 / Java I2P 2.13.0. The historical NTCP2
interop skill and Plan 434 both prohibit treating that retired lane as the new
current-pin runner. No old reference source, runner, scenario, or closure was
modified.

## Requirement-to-evidence matrix

| Requirement | Result |
| --- | --- |
| Preserve Plan 101 normal-daemon NTCP2 refusal and non-advertisement | Preserved. No production or configuration source was changed. |
| Use current pinned stock i2pd and a bounded local path | Current i2pd 2.61.0 binary metadata is present in the ignored target cache, but the existing launcher scenario parser rejects network IDs other than 99 (`tools/i2pr-interop/src/scenario.rs::PRIVATE_NETWORK_ID` / `Scenario::from_raw`), and the runner constructs both initiator and responder handshake states with literal network ID 99 (`tools/i2pr-interop/src/main.rs`). |
| Observe both directions and correlated I2NP | Not run. The current launcher does not expose an isolated network-ID-2 profile or reverse i2pd initiator operation. The old direct driver is tied to the historical reference lock and is not the authorized current-pin successor runner. No two-way handshake/I2NP evidence exists. |
| Localized cause and corrective | No wire attempt was made, so no protocol divergence was observed and no Rust protocol correction is justified. A separate, narrowly bounded Plan 434 runner/driver work item must first define the network-ID-2 loopback input boundary and provide a current-pin reverse initiator with stage and I2NP correlation. |
| Plan 435 readiness | Blocked. Plan 435 remains blocked on Plans 433 and 434. |

## Inspection commands and outcomes

No protocol or external-router command was run. The disposition follows from
the source and metadata inspection below:

- `rtk cat plans/implementation/ntcp2-transport/434-ntcp2-authenticated-link-discrepancy-recovery.md` — acceptance requires genuine forward and reverse authenticated NTCP2 plus bounded I2NP delivery.
- `rtk cat .opencode/skills/i2pr-ntcp2-interop/SKILL.md` — historical lane is archaeology-only; do not extend it without a new plan of record.
- `rtk rg -n 'PRIVATE_NETWORK_ID|network_id' tools/i2pr-interop/src/main.rs tools/i2pr-interop/src/scenario.rs` — parser only allows ID 99; both handshake roles and prepared RouterInfo use 99.
- `rtk cat tests/integration/ntcp2/references.lock.toml` — historical lock is i2pd 2.60.0 / Java 2.12.0, not current pins.
- `rtk cat target/interop/cache/ssu2/i2pd/635b013a612ff47278ef02acf8580a28e10e26c5/source-revision.txt` — cached i2pd revision is `635b013a612ff47278ef02acf8580a28e10e26c5`.
- `rtk cat target/interop/cache/ssu2/i2pd/635b013a612ff47278ef02acf8580a28e10e26c5/source-version.txt` — cached version is `2.61.0`.
- `rtk bash scripts/check-ntcp2-interoperability.sh` — passed as a static historical-boundary check only; it is not Plan 434 interop evidence.
- `rtk git diff --check` — passed.

No listener, reference router, or public I2P network was started. No raw logs,
identity material, or packet captures were produced. No Plan 099/100 closure
record or legacy runner was changed.

## Security, compatibility, findings, and disposition

- **Medium — required successor execution surface absent.** A successful
  synthetic network-ID-99 launcher run cannot establish interoperability
  against stock network-ID-2 i2pd. Reusing the old i2pd direct driver would
  violate the new plan's current pin and the historical-lane boundary. Plan
  434 is blocked until a reviewable, strictly loopback-bound successor runner
  can exercise network ID 2 and initiate both directions against the current
  pinned implementation while preserving bounded stage and I2NP evidence.
- Normal-daemon NTCP2 remains disabled and non-advertised. No address,
  capability, support inventory, or conformance claim changed.
- No source fix or migration is required from this inspection; the first
  required work is the missing diagnostic owner described above. Do not infer
  protocol failure from the absence of an event or protocol success from a
  stock binary's presence.

## Unblock audit and roadmap disposition

Plan 434 is blocked, not passed or stopped on a protocol result. Plan 435
remains blocked on 433 and 434. Plans 433, 436, 437, 438, and 439 remain blocked
on their listed dependencies. Plan 432 passed; Plan 431 remains stopped. No
other 430–439 plan was unblocked by this record. No capability promotion is
made.
