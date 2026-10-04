# Proposal 170 provenance manifest (Plan 286)

Frozen references for the Proposal 170 / I2PControl workstream. Only
manifest-listed Proposal 170 files receive the ADR 0028 reuse exception;
all other Emissary/upstream code remains under the normal
`GUARDRAILS.md` §14 provenance rules.

## Frozen pins

| Artifact | Pin | Recorded |
|---|---|---|
| Proposal 170 (I2PControl Expansion, Open) | revision 2026-05-20 | roadmap §long-term references |
| Base I2PControl API version 1 documentation | site update 2026-07-10 | roadmap §long-term references |
| eggstack/emissary fork master | `6885a945d25a5ae61bc68191d27c5816bc3df4c9` | ADR 0028 |
| eepnet/emissary upstream master | `9b43484a21d5a1291c4881cdae62a36c527f8c0f` | ADR 0028 |
| Java I2PControl Proposal 170 PR 6 head | `45bb593000408071dd376b78848fdc246dccd964` | ADR 0028 |
| PurpleI2P/i2pd (adopted/base behavior) | openssl head `2d57d3f6783efbfebde6c5b03f29e6c231a84d6b` | ADR 0028 / roadmap |

If a pin moved before execution, the rows above stay as research
provenance and the newer pin is added explicitly; history is never
silently rewritten.

## Upstream absence verification

At Plan 286 execution time the researched eepnet/emissary upstream tree
at `9b43484a21d5a1291c4881cdae62a36c527f8c0f` contains no `i2pcontrol`
subtree (per ADR 0028 research statement and the roadmap long-term
references). Ownership is therefore never inferred from adjacency: only
the fork paths listed below as `reusable` or `behavioral-reference`
receive the narrow ADR 0028 exception.

Mechanical re-verification is owned by the first plan that performs a
literal reuse (Plan 287 reuses bounded auth/batch behavior only, no
literal file import; the first literal file import must record exact
blob hashes here before landing).

## Fork path classification

`R` = reusable protocol/domain/security/test code (with notice
preserved, reconciled to i2pr architecture, never wholesale import).
`B` = behavioral reference only (read for wire/semantic parity, never
imported). `X` = Emissary/Yosemite-specific or unrelated upstream code,
prohibited from direct import.

| Fork path (under eggstack/emissary @ `6885a94`) | Class | Notes |
|---|---|---|
| `i2pcontrol/auth/*` (token mint/verify, expiry, eviction) | R | Bounded behavior donor for Plan 287 (32-byte tokens, 1-day lifetime, 1024-table, 256-byte presented cap, source-IP throttle). Reuse as behavior + tests, not as runtime transplant. |
| `i2pcontrol/jsonrpc/*` (envelope, batch, error mapping) | R | Bounded behavior donor for Plan 287 batch/admission semantics. |
| `i2pcontrol/routerinfo/*` (selector serialization) | B | Wire-shape reference; i2pr sources stay with existing router owners (Plan 288). |
| `i2pcontrol/addressbook/*` (book/config/subscription domain) | R | Domain-model donor for the Plan 294 canonical owner; Yosemite backends excluded. |
| `i2pcontrol/tunnelmanager/*` (actions, lifecycle, option matrix) | R | Protocol/domain donor for Plans 289–293; M10 manager stays authoritative. |
| `i2pcontrol/clientservices/*` (service selectors) | B | Selector-shape reference; sources stay with SAM/I2CP/service owners. |
| `i2pcontrol/conformance/*` (fixtures, vectors, matrices) | R | Test-behavior donor where byte-compatible with i2pr structures. |
| `i2pcontrol/http/*` + `i2pcontrol/filters/*` (proxy/filter policy) | R | Policy donor for Plan 290/292 shared primitives; Emissary supervisors excluded. |
| `i2pcontrol/streamr/*` + datagram substrate | R | Substrate-behavior donor for Plan 291 under i2pr destination ownership. |
| `i2pcontrol/*yosemite*`, session/supervisor backends | X | Yosemite-specific runtime; prohibited. |
| Any Emissary Red25519 / Encrypted LeaseSet2 cryptographic implementation (blinding, Red25519 sign/verify, ELS2 layer framing/derivation, b33 codec) | X | **Excluded by Plan 329 from the ADR 0028 §7 reuse exception.** Readable-reference and reuse classification for cryptographic code is `reference/test-only`: behavioral oracle use after an implementation freeze, never source adoption. See [`specs/references/red25519-clean-room-freeze.md`](../../specs/references/red25519-clean-room-freeze.md) §5. |
| Any non-`i2pcontrol` upstream/fork path | X | Unrelated code; prohibited under this exception. |

Applicable source notices from reused files must be preserved alongside
the importing module, and each reuse records its exact commit + blob
origin in the importing plan's closure record.

## Plan 286 reuse statement

Plan 286 performs **no literal reuse**: the `i2pr-i2pcontrol` crate is a
clean-room contract written from the pinned Proposal/base wire
semantics and the plan's explicit inventory requirements. This manifest
is the authorization basis for bounded behavioral reuse starting with
Plan 287.
