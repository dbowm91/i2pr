# Plan 150 SAM external-client closure evidence

This checked-in summary records the successful localhost-only Plan 150
qualification. The run-specific JSON and Markdown transcript are generated
under `target/interop/sam-evidence` and are intentionally not committed.

## Scope and provenance

- i2pr listener: `127.0.0.1:0` ephemeral loopback; SAM remains disabled by
  default and non-advertised.
- i2pr revision: the commit containing this record and the Plan 150 closure.
- Rust toolchain: pinned `1.95.0`.
- libsam3: `https://github.com/i2p/libsam3` at
  `7d6e658798baec31394c5685f9583343cc00900b`; built and probed, but not
  counted because its public `sam3CreateSession` API requires an 884-character
  minimum private-key value while i2pr's canonical Ed25519 SAM `PRIV` is 608
  characters.
- i2psam: `https://github.com/i2p/i2psam` at
  `b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`; unmodified normal public API.
- i2plib substitute: `https://github.com/l-n-s/i2plib` at
  `6edf51cd5d21cc745aa7e23cb98c582144884fa8`; unmodified `i2plib.sam`
  message/Base64 surface with a thin socket harness, qualified under the
  plan's substitute clause.

The two counted clients are independently implemented i2psam and the pinned
i2plib SAM surface. Neither external source tree is vendored or patched for
i2pr. The i2psam snapshot seeds session IDs from wall-clock seconds, so the
harness serializes launches into distinct one-second slots to make its exact
revision reproducible.

## Results

| Surface | Result |
| --- | --- |
| i2plib substitute ACCEPT ↔ i2psam CONNECT | passed; exact 2 MiB binary payloads in both directions |
| i2psam ACCEPT ↔ i2plib substitute CONNECT | passed; exact 2 MiB binary payloads in both directions |
| Binary matrix | passed; LF/CRLF/NUL/invalid UTF-8/all-byte/SAM-looking payloads |
| `SILENT=true` raw transition | passed; supporting transcript verifies raw bytes before status lines |
| Private destination generate/import | passed through both counted client APIs |
| NAMING | passed for `ME`, full Destination, malformed, and unknown-name cases |
| Negative SAM matrix | passed for unsupported version/style/options, unknown, malformed, and duplicate inputs |
| STREAM FORWARD | passed with authenticated peer metadata and a real loopback echo target |
| Multiple streams/lifecycle | passed by the Plan 149 self-composed black-box suite |
| Plan 149 product/resource/privacy gates | passed |

## Reproduction

From the repository root, with network access only for fetching the pinned
sources:

```text
bash scripts/interop/fetch-sam-clients.sh --rebuild
bash tests/integration/sam/clients/build.sh
bash tests/integration/sam/run-independent.sh
```

The run fails closed on any required result and writes sanitized evidence to
`target/interop/sam-evidence`. This is localhost SAM-client interoperability
evidence only; it does not claim router-to-router NTCP2/SSU2, public I2P, or
mixed-router tunnel interoperability.
