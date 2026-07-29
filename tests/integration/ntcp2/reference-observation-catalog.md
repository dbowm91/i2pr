# Plan 052 reference observation marker catalog

The catalog binds the Plan 052 typed observation levels to source-derived
markers emitted by the pinned reference routers. It is consumed by
`tests/integration/ntcp2/harness/observation.py` and the per-side
observation records.

The machine-readable source of truth is
`tests/integration/ntcp2/reference-observation-catalog.toml`. The
document you are reading is generated from or checked against that file;
it must not be the executable source of truth. Drift between this
document and the TOML is detected by `tests/integration/ntcp2/harness/test_plan054.py`.

## Schema

The catalog uses a single TOML table with one section per reference. Each
section declares the events the reference exposes after authentication,
AEAD frame decrypt, frame-block parsing, and I2NP message dispatch.

```text
schema = "i2pr-reference-observation-catalog-v1"
revision = 1

[java_i2p]
version = "2.12.0"
revision = "2800040deee9bb376567b671ef2e9c34cf3e30b6"

[[java_i2p.observations]]
semantic_level = "ntcp2_authenticated"
source_path = "router/java/src/net/i2p/router/transport/ntcp/NTCP2Transport.java"
symbol = "NTCP2Transport.connectionEstablished"
marker_kind = "structured-log"
marker = "NTCP2 connection established"
sanitization_rule = "strip-ipv4-endpoint-prefix"
minimum_count = 1

[i2pd]
version = "2.60.0"
revision = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"

[[i2pd.observations]]
semantic_level = "ntcp2_authenticated"
source_path = "libi2pd/Transports.cpp"
symbol = "Transports::ConnectToPeer"
marker_kind = "structured-log"
marker = "NTCP2: SessionConfirmed sent"
sanitization_rule = "strip-ipv4-endpoint-prefix"
minimum_count = 1
```

## Pinned observations

### Java I2P 2.12.0 (revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`)

| Level | Source path | Symbol | Marker | Status |
| --- | --- | --- | --- | --- |
| `ntcp2_authenticated` | `router/java/src/net/i2p/router/transport/ntcp/NTCP2Transport.java` | `NTCP2Transport.connectionEstablished` | `NTCP2 connection established` | Stable. |
| `frame_authenticated_and_decrypted` | `router/java/src/net/i2p/router/transport/ntcp/NTCP2Connection.java` | `NTCP2Connection.receive` | `NTCP2 data frame authenticated and decrypted` | Source-locked via catalog. |
| `i2np_message_decoded` | `router/java/src/net/i2p/router/transport/ntcp/NTCP2Reader.java` | `NTCP2Reader.messageReceived` | `NTCP2 I2NP message decoded` | Source-locked via catalog. |

The handshake-only `NTCP2 connection established` marker may never satisfy
`frame_authenticated_and_decrypted` or `i2np_message_decoded`.

### i2pd 2.60.0 (revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`)

| Level | Source path | Symbol | Marker | Status |
| --- | --- | --- | --- | --- |
| `ntcp2_authenticated` | `libi2pd/Transports.cpp` | `Transports::ConnectToPeer` | `NTCP2: SessionConfirmed sent` | Stable. |
| `frame_authenticated_and_decrypted` | `libi2pd/NTCP2Session.cpp` | `NTCP2Session::HandleData` | `NTCP2: data frame authenticated and decrypted` | Source-locked via catalog. |
| `i2np_message_decoded` | `libi2pd/NTCP2Session.cpp` | `NTCP2Session::HandleI2NP` | `NTCP2: I2NP message decoded` | Source-locked via catalog. |

The handshake-only `SessionConfirmed sent` and `SessionConfirmed from`
markers may never satisfy `frame_authenticated_and_decrypted` or
`i2np_message_decoded`.

## Update rules

- Adding a new marker requires updating both the Java and i2pd sections
  in the TOML file, this explanatory document, and the matching adapter.
- Removing a marker requires bumping the catalog schema revision and
  updating all dependent evidence bundles.
- Marker text must be exact-string matched; normalized-whitespace matching
  is forbidden because it inflates duplicate counts.
- Handshake-only markers (`SessionConfirmed sent`, `SessionConfirmed from`,
  `NTCP2 connection established`) MUST NEVER satisfy the data phase.

## Validation

The observation catalog is consumed by the static `check-*.sh` boundary
checkers (run before any author changes touch the catalog):

```text
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

A marker that is not present in the TOML file, or that disagrees with the
pinned source revision, is a typed blocker in `validate_observation()`.
