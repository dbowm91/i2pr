# i2pr plans

Plan-of-record and closure/status records for `i2pr`. Closure/status records
(`NNN-status.md`) are authoritative; per-plan narratives are historical context
unless the newest status points to them as executable work.

The current **Milestone 6 local closure authority** is
[**Plan 134**](134-status.md) (`passed-milestone6-recv-window-ack-ceiling-closure`).
Independent-router interoperability is not claimed and remains separate
external acceptance debt.

The current **Milestone 7 SAM 3.1 final-acceptance authority** is
[**Plan 151**](151-status.md)
(`passed-m7-sam31-final-acceptance-evidence-correction`).

Current sequence:

**Plan 146 closed → Plan 147 implementation retained → Plan 148 blocked audit → Plan 149 closed product composition → Plan 150 external core evidence retained → Plan 151 passed final acceptance correction (+ Plan 152 passed narrow M6 corrective).**

- Plan 146 closed private-destination compatibility with bidirectional Java I2P/i2pd reference evidence.
- Plan 147 landed the owned same-socket raw TCP↔Streaming path and supervised ACK/retransmit driver. Its broad original acceptance label was superseded because several criteria were deferred.
- Plan 148 correctly rejected invalid independent-client evidence but misdiagnosed the remaining blocker as only client acquisition/build availability.
- [Plan 149](149-m7-sam31-self-composing-local-product-corrective.md) closed the self-composed localhost STREAM product. `SESSION CREATE` now builds the local destination/LeaseSet2/bridge/inbound-delivery/driver product before success and the canonical black-box test drives it only through SAM TCP after startup.
- [Plan 150](150-m7-sam31-external-client-reproducible-final-closure.md) produced valid external-client core evidence with exact pinned `i2psam` and qualified `i2plib.sam` client surfaces, plus SILENT/private-destination/NAMING/negative/basic-FORWARD results. Its broad final closure is superseded because some required lifecycle/backpressure/fault/FORWARD/M6 evidence was recorded without being executed.
- [Plan 151](151-m7-sam31-final-acceptance-evidence-correction.md) is the only next executable plan. It makes every final evidence row executable, closes sibling-stream/slow-peer/fault/CLOSE-RESET/FORWARD lifecycle gaps, explicitly reruns the Plan 127–134 regression floor, and requires the final hosted external lane to pass on the exact closing head.
- [Plan 152](152-m6-session-streaming-robustness-corrective.md) is the narrow Milestone 6 corrective the Plan 151 §17 stop required: per-connection delivered-bytes cap with ACK gating, coalesced duplicate ACKs, and sender ECIES ratchet-key trimming, with no wire change. Fixes landed with manager/ECIES unit tests.

Current classification:

```text
plan_134 = passed
plan_146 = passed
plan_147_raw_driver = retained
plan_149 = passed-self-composing-local-product
plan_150_external_core_evidence = retained-passed
plan_150_final_acceptance = superseded-by-plan151
plan_151 = passed
plan_152 = passed-narrow-m6-corrective

milestone7_local_product = passed-via-plan149
milestone7_sam_localhost = passed-via-plan151
milestone7_sam_localhost_final_acceptance = closed
sam_independent_clients = at-least-two-passed-via-plan150
next_executable_plan = none-milestone7-closed
next_product_layer = milestone8-planning
```

Milestone 7 SAM localhost acceptance is closed via Plan 151. Milestone 8
needs a plan-of-record before any implementation begins.

## MVP roadmap

- [`000-mvp-roadmap.md`](000-mvp-roadmap.md) — milestone sequence from empty repository to the first feature-complete MVP.
- [`145-m7-sam31-remaining-gap-corrective-roadmap.md`](145-m7-sam31-remaining-gap-corrective-roadmap.md) — historical Milestone 7 corrective umbrella.
- [`149-m7-sam31-self-composing-local-product-corrective.md`](149-m7-sam31-self-composing-local-product-corrective.md) — passed production-composition corrective.
- [`150-m7-sam31-external-client-reproducible-final-closure.md`](150-m7-sam31-external-client-reproducible-final-closure.md) — external-client core evidence and historical attempted final closure.
- [`151-m7-sam31-final-acceptance-evidence-correction.md`](151-m7-sam31-final-acceptance-evidence-correction.md) — current final acceptance/evidence corrective.
- [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md) — destination / garlic / LeaseSet2 / Streaming router construction.
- [`126-129-milestone6-final-corrective-roadmap.md`](126-129-milestone6-final-corrective-roadmap.md) and [`126-130-milestone6-final-corrective-roadmap.md`](126-130-milestone6-final-corrective-roadmap.md) — Milestone 6 final corrective roadmaps.

## Milestone 7 plan hierarchy

| Plan | Current authority status | Record |
| --- | --- | --- |
| 135 | superseded by later audits/corrective roadmaps | [`135-status.md`](135-status.md) |
| 136 | foundation landed; broader claims superseded | [`136-status.md`](136-status.md) |
| 137 | passed loopback server/session lifecycle | [`137-status.md`](137-status.md) |
| 138 | implementation landed; acceptance superseded | [`138-status.md`](138-status.md) |
| 139 | FORWARD/naming implementation retained; final matrix owned by Plan 151 | [`139-status.md`](139-status.md) |
| 140 | blocked closure audit; historical | [`140-status.md`](140-status.md) |
| 141 | historical corrective roadmap | [`141-status.md`](141-status.md) |
| 142 | Base64 correction retained | [`142-status.md`](142-status.md) |
| 143 | local delivery seam retained | [`143-status.md`](143-status.md) |
| 144 | partial local in-process handshake evidence | [`144-status.md`](144-status.md) |
| 145 | historical corrective umbrella | [`145-status.md`](145-status.md) |
| 146 | **passed** private-destination reference requalification | [`146-status.md`](146-status.md) |
| 147 | raw-driver implementation/local byte pump retained; broad original acceptance superseded | [`147-status.md`](147-status.md) |
| 148 | blocked historical audit | [`148-status.md`](148-status.md) |
| 149 | **passed** self-composing SAM local product | [`149-status.md`](149-status.md) |
| 150 | external-client core evidence **retained passed**; final acceptance superseded | [`150-status.md`](150-status.md) |
| 151 | **active** final acceptance evidence correction; only next executable plan | [`151-status.md`](151-status.md) |

Do not restore a historical broad `passed` label as final authority without
satisfying the newest superseding acceptance contract.

## Milestone 6 plan hierarchy

Closure records remain authoritative for Milestone 6.

| Plan | Status | Closure |
| --- | --- | --- |
| 119 | `passed-leaseset2-protocol-foundation` | [`119-status.md`](119-status.md) |
| 120 | `passed-destination-lifecycle-and-pools` | [`120-status.md`](120-status.md) |
| 121 | `superseded-by-126` | [`121-status.md`](121-status.md) |
| 122 | `passed-corrected-local-destination-routing` | [`122-status.md`](122-status.md) |
| 123 | `passed-corrected-streaming-wire-local` | [`123-status.md`](123-status.md) |
| 124 | `passed-plan122-corrective-closure` | [`124-status.md`](124-status.md) |
| 125 | `superseded-by-final-corrective-closure` | [`125-status.md`](125-status.md) |
| 126 | `passed-ecies-destination-ratchet-corrective-foundation` | [`126-status.md`](126-status.md) |
| 127 | `passed-destination-session-routing-final-closure` | [`127-status.md`](127-status.md) |
| 128 | `passed-streaming-wire-protocol-corrective-closure` | [`128-status.md`](128-status.md) |
| 129 | superseded by later final gates | [`129-status.md`](129-status.md) |
| 130 | superseded by later final gates | [`130-status.md`](130-status.md) |
| 131 | superseded by later final gates | [`131-status.md`](131-status.md) |
| 132 | implementation evidence superseded by Plan 133 | [`132-status.md`](132-status.md) |
| 133 | evidence authority superseded by Plan 134 | [`133-status.md`](133-status.md) |
| 134 | **current Milestone 6 local authority** | [`134-status.md`](134-status.md) |

## What's implemented

- Bounded protocol codecs and cryptographic wrappers.
- Persistent identity/configuration/runtime foundations.
- Local NetDB and exploratory tunnel substrate.
- Local destination lifecycle, signed LeaseSet2, ECIES destination session layer, routing, and Streaming core.
- SAM 3.1 parser/session/STREAM/FORWARD/NAMING foundations.
- Plan 142 canonical I2P Base64 behavior.
- Plan 146 reference-compatible private destinations.
- Plan 147 dedicated raw STREAM socket owner and byte pump.
- Plan 149 self-composed SAM `SESSION CREATE` product path and black-box exact-byte evidence.
- Plan 150 external-client core interoperability evidence on localhost.

## What's not yet accepted

- Live NTCP2/SSU2 transport activation or mixed-router interoperability.
- Public I2P participation.
- Network-transport-bound NetDB/public router behavior.
- Final Milestone 7 localhost SAM acceptance: Plan 151 must close the remaining sibling-stream, slow-peer, deterministic-fault, CLOSE/RESET, full FORWARD lifecycle, explicit Plan 127–134 regression, and evidence-integrity gates.
- Client proxies and service tunnels.

The NTCP2 development interoperability result remains historical and no passed
mixed-router result is claimed.

## Working with plans

Before editing or claiming conformance, load
[`.opencode/skills/i2pr-local-dev/SKILL.md`](../.opencode/skills/i2pr-local-dev/SKILL.md)
for the routine local product/SAM seam or the architecture skill for broader
navigation.

When records disagree, the newest explicit superseding status wins. Current
handoff: **Plan 151 passed; Milestone 8 needs a plan-of-record**.