# Plan 270 status — passed M12 floodfill architecture and source authority freeze

- Plan: `plans/implementation/floodfill/270-m12-architecture-authority.md`
- Parent: `66055c73c9d4895336a05163a901d6075619fb81` (Plan 269 passed)
- Disposition: **passed with a newly registered crypto prerequisite**. The architecture and
  current source authority are frozen. Type-5 EncryptedLeaseSet validation remains gated until
  Plan 280 establishes a reviewed Red25519 (type 11) verification provider.
- Scope: documentation/planning only; no product behavior, support claim, or advertisement changed.

## Requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| Refresh official sources and compare current upstream | `specs/SOURCES.md` records the official website head `8baa1d680db263941daf2fb4462fbd75ba01c47f`, the prior pin, relevant unchanged protocol pages, and exact Java 2.13.0 / i2pd 2.61.0 behavioral pins. |
| Freeze the architecture and rejected alternatives | `docs/adr/0027-floodfill-role-provenance-and-advertisement.md` specifies role boundaries, provenance, validation/disclosure, record key/signature/freshness rules, lookup and reply policy, replication, rollover, persistence, advertisement, evidence gates, and rejected alternatives. |
| Resolve existing LS2 flag drift | ADR 0027 and Plan 272 specify bit 0 offline, bit 1 unpublished, bit 2 blinded-on-publication, and bits 15–3 reserved; bit 3 is rejected. |
| Reconcile protocol dossier, roadmap, conformance and support vocabulary | `specs/protocols/04-reseed-netdb.md`, `plans/subsystems/floodfill-roadmap.md`, `specs/CONFORMANCE.md`, `specs/support.toml`, and `plans/registry.md` now agree on the staged implementation and evidence gates. |
| Preserve the required record floor without guessing about missing cryptography | Current reviewed workspace crypto has Ed25519 type 7 verification but no I2P Red25519 type 11 verifier. Plan 280 is registered as a prerequisite for Plan 272; no local primitive or unverified substitute is authorized. |
| Avoid implementation/support claims | The production/test/workflow/dependency diff from the parent is empty; support inventory continues to claim no floodfill implementation or `caps=f`. |

## Unblock audit

Plan 271 is the only newly ready plan. Plan 280 is blocked on Plan 271 and must identify a
vetted compatible verifier or stop and register an architecture correction. Plan 272 remains
blocked on both Plans 271 and 280. Plans 273–279 remain blocked by their declared predecessors.
No other registered plan became eligible.

## Verification

- `rtk git diff --check` — passed.
- Python `tomllib` parse of `specs/support.toml` — passed.
- `rtk git diff --name-only 66055c73c9d4895336a05163a901d6075619fb81 -- 'crates/**' 'tests/**' 'tools/**' '.github/**' 'Cargo.toml' 'Cargo.lock'` — empty.
- `rtk bash scripts/check-dependency-direction.sh` — passed.
- `rtk bash scripts/check-runtime-boundaries.sh` — passed.
- Official I2P website pinned-to-current comparison for the relevant protocol pages — no page-content changes.
- External qualification was not run; this plan is documentation-only and makes no interoperability claim.
