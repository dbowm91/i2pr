# External portable service-tunnel consumer

This is a standalone Cargo package outside the root workspace. It imports only the
published Rust crate name from the exact Plan 350 Git revision
`fa0824970b67b5ee90d5907765c408ccd0a19941`. It does not use source paths, daemon,
runtime, testkit, or private modules. The crate is still `publish = false`; the Git
dependency verifies repository consumption without making a license or release claim.

Run from the repository root:

```sh
bash scripts/check-portable-service-tunnel-consumer.sh
```

The test suite covers generic client/server validation, explicit dedicated/shared
Destination groups, authenticated peer access/rate inputs, HTTP privacy and server
filtering, SOCKS CONNECT, IRC privacy filtering, bounded rejection, and a fake
adapter's generation lifecycle. Policy decisions are always made by the imported core;
the fake adapter models only identity provenance and resource lifecycle.

See [CONFORMANCE.md](CONFORMANCE.md) for the requirement-to-test matrix and
`specs/references/portable-service-tunnel-sam-adapter-handoff.md` for the
downstream transport ownership contract.
