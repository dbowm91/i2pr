# Plan 281 — M12 defer EncryptedLeaseSet type-5 support pending Red25519 provider

Status: **passed-m12-record-floor-corrected-type5-deferred**

Classification: architecture correction following the Plan 280 provider qualification stop.

Hard dependency: Plan 280 completed provider review and found no acceptable maintained Rust
provider compatible with the exact I2P Red25519 specification.

## Objective

Correct ADR 0027 and the downstream Plan 272 record-validation floor so implementation can
continue safely without I2P signature type 11. Keep EncryptedLeaseSet payloads unsupported and
unanswerable until a new plan qualifies a vetted provider. This plan makes no crypto or wire
implementation change.

## Frozen support correction

- Plan 272's bounded validated record floor is RouterInfo plus DatabaseStore types 1, 3, and 7.
- DatabaseStore type 5 remains structurally recognized as deferred/unsupported, is not retained
  in server-authority storage, and can never be served, persisted, or replicated.
- A later provider qualification must include exact I2P Red25519 official vectors and supply-chain
  review, and must register its own plan before adding type-5 validation.
- No broad record-floor or floodfill-serving claim follows from this correction.

## Verification and unblock audit

`git diff --check` and TOML parsing pass. Plan 272 is the only newly ready plan after Plans 271
and 281 pass. Plans 273–279 remain blocked in dependency order. Plan 280 remains closed with a
provider-unavailable disposition; no other plan was unblocked.
