# External portable service-tunnel consumer — current revision

This is a standalone Cargo package outside the root workspace. It imports only the
published Rust crate name from the Plan 379 Git revision
`f0fb74a8582d6077a7c5db49d613699688115bad`. It does not use source paths, daemon,
runtime, testkit, or private modules.

It **supplements** [the Plan 351 fixture](../portable-service-tunnel-consumer/README.md)
rather than replacing it. That fixture stays pinned to the Plan 350 revision
`fa082497…` on purpose: it is the evidence that the portable boundary held *before*
the `i2pr-proto` edge existed. Repointing it would destroy the comparison. This one
exercises the same policy/filter matrix against today's package, plus the one
workspace edge Plan 359 added.

Run from the repository root:

```sh
bash scripts/check-portable-service-tunnel-consumer.sh
```

The script runs both fixtures and asserts each resolves its own exact pinned
revision.

## What the current pin adds

The Plan 359 amendment permits exactly one workspace dependency — `i2pr-proto` —
so a `.b33` encrypted-service address can be a service-tunnel remote target. Until
this fixture existed, **no external consumer had ever compiled against that edge.**
Three tests here do:

- a valid narrow (56-character) and wide (60-character) b33 address parses, exposes a
  structured `EncryptedServiceAddress`, and round-trips through `canonical_string()`;
- a b33-shaped value that fails validation is reported as an *encrypted-service*
  problem and never as a Base32 length error — the exact misdiagnosis the edge exists
  to prevent;
- a `StaticAliasTable` refuses an encrypted-service target, so a service without
  `delay_open` cannot reach the encrypted path by naming a global alias instead.

The addresses are embedded as **literals**, not constructed through `i2pr-proto`.
The fixture depends on `i2pr-service-tunnels` alone, so it proves the whole
parse/validate/expose path runs on data it never had to build.

The eight Plan 351 tests are carried forward byte-for-byte. They are duplicated
rather than shared deliberately: a shared `include!` would couple the two fixtures'
sources, and the historical fixture's value is that it is a snapshot of what a
consumer could do at a fixed revision.

The crate is still `publish = false`; the Git dependency verifies repository
consumption without making a release claim. See
`docs/architecture/i2pr-service-tunnels.md` ("Distribution posture") in the i2pr
repository for why publication is blocked, and
`specs/references/portable-service-tunnel-sam-adapter-handoff.md` for the downstream
transport ownership contract.