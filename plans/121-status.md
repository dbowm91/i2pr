# Plan 121 status — corrective closure required

## Current authority

- Status: **`corrective-reopened-plan126`**.
- Reopened: **2026-08-25** after post-Plan-125 protocol/source audit.
- Original plan: `121-m6-ecies-garlic-session-layer.md`.
- Corrective plan: `126-m6-ecies-destination-ratchet-corrective-foundation.md`.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.

## Retained work

The existing Plan 121 work remains useful for:

- external Elligator2 dependency selection;
- X25519/ChaChaPoly/HKDF building blocks;
- typed ECIES payload-block structures;
- destination-scoped bounded session configuration;
- deterministic cryptographic test infrastructure.

## Why final closure is reopened

The source-floor destination ratchet is not current I2P wire compatible. In particular:

```text
current Noise initializer = Noise_NK_25519_ChaChaPoly_SHA256
current NS representation = i2pr-only 0xE0 + clear static + representative + ciphertext
current NSR/ES classifier = i2pr-only 0xE2 marker
```

The current I2P destination ECIES protocol uses the IKelg2 initializer, encrypted bound static-key section, tag-prefixed NSR/ES formats, and destination-bound paired sessions.

The production `EciesSessionManager` also does not yet retain/bind the complete NS -> NSR -> Existing Session state required for repliable Streaming traffic.

## Current classification

```text
plan_121 = corrective-reopened-plan126
plan_122 = provisional-blocked-on-plan126-plan127
plan_124 = primary-composition-fix-retained-full-closure-reopened-plan127
milestone6_local_product = not-closed
next = plans/126-m6-ecies-destination-ratchet-corrective-foundation.md
```

No mixed-router ECIES interoperability is claimed.