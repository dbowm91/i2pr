# SAM Roadmap — SAM 3.1 Closed, SAM 3.3 Extension Active

Status: SAM 3.1 closed; Plan 368 SAM 3.3 extension ready

Long-term references:

- `GUARDRAILS.md` (non-negotiable security/architecture constraints)
- `specs/CONFORMANCE.md` (what counts as protocol-support evidence)
- `specs/support.toml` (machine-readable support inventory)

Research specs (dossier map: `specs/README.md`):

- `specs/protocols/08-sam.md` (SAM endpoint dossier)
- `specs/references/sam31-private-destination.md` (Plan 136 PUB/PRIV provenance)

Related ADRs:

- See `docs/adr/` index for SAM decisions.

## 1. Purpose and ownership boundary

SAM 3.1 protocol/private-destination foundation, loopback server + session
lifecycle, STREAM connect/accept bridge, FORWARD/naming hardening,
independent-client closure, self-composing local product, final acceptance +
CI hygiene are closed through the historic 135–153 line.

Plan 368 is a parallel extension. It adds SAM 3.3 PRIMARY/subsession
shared-Destination semantics so one application identity can expose STREAM plus
protocol-17 repliable DATAGRAM and protocol-18 RAW children without moving
I2P Streaming into the application.

Historic plans: 135–153. Active extension: 368.

## 2. Work classification

- **Capability** — the milestone-scoped user/operator-visible behavior each plan closes.
- **Infrastructure** — runtime-neutral crates, state machines, and harnesses underneath the capabilities.
- **Invariant** — bounded decodes, typed errors, loopback-by-default disclosure, fail-closed lanes.
- **Polish** — CI hygiene, docs normalization, and lane isolation passes.

## 3. Non-goals

- No non-loopback SAM exposure; ordinary SAM stays disabled by default.
- No M6 interoperability claim (see Plan 152 note in `destination-streaming`).
- Plan 368 does not replace I2CP, implement BitTorrent DHT, or add
  torrent-specific router APIs.
- Managed-app DATAGRAM/RAW semantics must not grant host UDP authority.
- Datagram2/3 are not claimed unless Plan 368 independently completes their
  full substrate and interoperability evidence.

## 4. Current state

Plan 151 (`passed-m7-sam31-final-acceptance-evidence-correction`) remains
Milestone 7 SAM 3.1 final-acceptance authority.

Plan 368 is **ready** as a parallel extension:
`plans/implementation/sam/368-sam33-primary-subsession-shared-destination-profile.md`.

No SAM 3.3 support claim exists until Plan 368 closes with the required
Java I2P, i2pd, self-product, and private managed-app evidence.

## 5. Target architecture

Plan 368 is the new plan-of-record for this subsystem's extension:

```text
one SAM 3.3 primary/control owner
        -> one existing i2pr Destination + tunnel set
             +-> STREAM child -> existing StreamingManager
             +-> DATAGRAM child -> existing protocol 17 DatagramManager
             +-> RAW child -> existing protocol 18 raw datagram path
```

The primary owns lifetime. Children share its Destination and may be removed
without destroying siblings. Primary loss tears down all children. The adapter
must reuse existing destination/streaming/datagram owners rather than create a
second data plane.

## 6. Dependency graph

```text
135 -> 136..140 -> 141..145 correctives -> 146..150 requalification
    -> 151 acceptance -> 152/153 hygiene     [SAM 3.1 closed]

151 + existing Destination/Streaming/Datagram substrate
    -> 368 SAM 3.3 PRIMARY/subsessions       [ready]
         -> downstream i2pr-tc C003
```

## 7. Milestones

One row per historic plan. `i2pr token` is the authoritative classification (newest status wins on
conflict); `state` is the codegg-registry projection. Filenames keep global i2pr numbers.

| Plan | State | i2pr token | Implementation | Closure |
|---|---|---|---|---|
| 135 | superseded | superseded-by-plan140-audit. | `plans/implementation/sam/135-m7-sam31-implementation-roadmap.md` | `plans/closure/sam/135-status.md` |
| 136 | closed | passed-m7-sam31-protocol-private-destination-foundation. | `plans/implementation/sam/136-m7-sam31-protocol-private-destination-foundation.md` | `plans/closure/sam/136-status.md` |
| 137 | closed | passed-m7-sam31-loopback-server-session-lifecycle. | `plans/implementation/sam/137-m7-sam31-loopback-server-session-lifecycle.md` | `plans/closure/sam/137-status.md` |
| 138 | closed | passed-m7-sam31-stream-connect-accept-bridge. | `plans/implementation/sam/138-m7-sam31-stream-connect-accept-bridge.md` | `plans/closure/sam/138-status.md` |
| 139 | closed | passed-m7-sam31-forward-naming-hardening | `plans/implementation/sam/139-m7-sam31-forward-naming-hardening.md` | `plans/closure/sam/139-status.md` |
| 140 | superseded | blocked-audit-superseded-by-plan141-corrective-roadmap. | — | `plans/closure/sam/140-m7-sam31-interoperability-closure.md`; `plans/closure/sam/140-status.md` |
| 141 | see token | active-m7-sam31-corrective-roadmap. | `plans/implementation/sam/141-m7-sam31-corrective-roadmap.md` | `plans/closure/sam/141-status.md` |
| 142 | closed | passed-m7-sam31-encoding-private-destination-corrective. | `plans/implementation/sam/142-m7-sam31-encoding-private-destination-corrective.md` | `plans/closure/sam/142-status.md` |
| 143 | closed | passed-m7-sam31-live-stream-product-bridge-corrective. | `plans/implementation/sam/143-m7-sam31-live-stream-product-bridge-corrective.md` | `plans/closure/sam/143-status.md` |
| 144 | see token | partial-passed-m7-sam31-independent-client-handshake-corrective. | — | `plans/closure/sam/144-m7-sam31-independent-client-final-closure-corrective.md`; `plans/closure/sam/144-status.md` |
| 145 | see token | m7-corrective-umbrella-final-acceptance-open-via-plan151. | `plans/implementation/sam/145-m7-sam31-remaining-gap-corrective-roadmap.md` | `plans/closure/sam/145-status.md` |
| 146 | closed | passed | `plans/implementation/sam/146-m7-sam31-private-destination-reference-requalification.md` | `plans/closure/sam/146-status.md` |
| 147 | see token | [plan_147_raw_driver] retained | `plans/implementation/sam/147-m7-sam31-dedicated-raw-stream-driver-corrective.md` | `plans/closure/sam/147-status.md` |
| 148 | superseded | blocked-audit-superseded-by-plan149-150-corrective-sequence. | — | `plans/closure/sam/148-m7-sam31-independent-client-final-closure.md`; `plans/closure/sam/148-status.md` |
| 149 | closed | passed-self-composing-local-product | `plans/implementation/sam/149-m7-sam31-self-composing-local-product-corrective.md` | `plans/closure/sam/149-status.md` |
| 150 | see token | [plan_150_external_core_evidence] retained-passed | — | `plans/closure/sam/150-m7-sam31-external-client-reproducible-final-closure.md`; `plans/closure/sam/150-status.md` |
| 151 | closed | passed-m7-sam31-final-acceptance-evidence-correction | `plans/implementation/sam/151-m7-sam31-final-acceptance-evidence-correction.md` | `plans/closure/sam/151-status.md` |
| 152 | closed | passed-m6-session-streaming-robustness-corrective | `plans/implementation/sam/152-m6-session-streaming-robustness-corrective.md` | `plans/closure/sam/152-status.md` |
| 153 | closed | passed-post-m7-authority-and-ci-hygiene | — | `plans/closure/sam/153-m7-closure-authority-and-ci-hygiene.md`; `plans/closure/sam/153-status.md` |
| 368 | ready | registered-sam33-primary-subsession-shared-destination-profile | `plans/implementation/sam/368-sam33-primary-subsession-shared-destination-profile.md` | closure pending: `plans/closure/sam/368-status.md` |

## 8. Cross-cutting requirements

Storage, protocol, security, concurrency, observability, and docs constraints per `GUARDRAILS.md`;
evidence model per `specs/CONFORMANCE.md`. Service/transport/API crates stay runtime-neutral;
`i2pr-runtime` owns Tokio/sockets/timers; listeners stay loopback-only and disabled by default.

## 9. Verification strategy

Routine floor plus `scripts/check-sam-acceptance-evidence.sh`.

Environment-gated lanes are `#[ignore]`-gated: ordinary runs skip them, explicit runs require
`--ignored --exact`, and missing env must fail, never silently pass.

## 10. Risks and decision points

- PRIMARY versus MASTER behavior differs across deployed implementations and
  must be dispositioned by evidence, not assumption.
- SAM 3.3 support is not advertised until shared STREAM/DATAGRAM/RAW identity
  and lifecycle pass against Java I2P and i2pd.
- If canonical protocol-17/18 delivery cannot be reused without a parallel
  router data plane, Plan 368 must stop.
- Independent external rows stay loopback/private-client only; no public SAM
  bind is introduced.

## 11. Completion definition

SAM 3.1 remains closed via Plan 151; Plan 152 (M6 robustness corrective
discovered by 151, normalized by 153) is retained.

The 3.3 extension closes only through Plan 368's own closure record and support
inventory update.

## 12. Milestone status summary

Full row history is §7. Current SAM 3.1 authority: Plan 151
(`passed-m7-sam31-final-acceptance-evidence-correction`). Current extension
authority: Plan 368, ready.
