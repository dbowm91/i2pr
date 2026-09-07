# Plan 163 status — Milestone 9 I2CP roadmap

Status: **`registered-m9-i2cp-roadmap`**.

Registered: **2026-09-07**.

Plan of record:
[`plans/163-m9-i2cp-roadmap.md`](163-m9-i2cp-roadmap.md).

## Current authority

```text
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_163 = registered-m9-i2cp-roadmap

milestone8_final_acceptance = closed-via-plan161
milestone9_protocol = i2cp
milestone9_final_acceptance = not-yet-closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 164
m9_sequence = 164 -> 165 -> 166 -> 167 -> 168 -> 169 -> 170
next_product_layer = milestone9-i2cp
```

## Locked M9 architecture

- I2CP protocol/state belongs in runtime-neutral `i2pr-api::i2cp`.
- `i2pr-daemon` remains the only I2CP TCP/Tokio listener owner.
- `i2pr-client` remains the single destination/routing product layer.
- M9 must add a client-owned destination mode: SessionConfig proves signing-key ownership; the client supplies signed Standard LeaseSet2 plus required decryption key material; i2pr does not require the client's destination signing private key.
- Existing router-owned SAM destination behavior must remain intact.
- I2CP is disabled by default and loopback-only for M9; non-loopback binding is rejected.
- The exact supported I2CP feature/API profile must be documented; blanket 0.9.67 compliance is not assumed.
- Service tunnels, HTTP, SOCKS5, and IRC remain Milestone 10.

## External reference targets

```text
Official I2CP docs snapshot target:
  i2p/i2p.website @ 26467e4b275e3a58280b9d4e6d4745d58bb8c499

Java I2P 2.13.0 client:
  9134f808337b401e8e53c73734c81fab04280c9d

go-i2cp:
  b529ee1c10a6011558b4d69fc9436a4afc489eac
```

Plan 164 must verify/freeze source provenance before code changes. Plan 170 must use unmodified exact-pinned independent clients and command-derived evidence.

## Handoff

Execute **Plan 164** next. Do not begin Plan 165+ until Plan 164 has an explicit passing closure record.