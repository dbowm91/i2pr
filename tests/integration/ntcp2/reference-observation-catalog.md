# Plan 052 reference observation marker catalog

This document binds the Plan 052 typed observation levels to source-derived
markers emitted by the pinned reference routers. It is consumed by
`tests/integration/ntcp2/harness/observation.py` and the per-side
observation records.

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
log_marker_paths = ["router/net/transport/ntcp2/NTCP2Transport.java"]
event_marker = "NTCP2 connection established"
semantic_level = "ntcp2_authenticated"
sanitization_rule = "strip-ipv4-endpoint-prefix"

[i2pd]
version = "2.60.0"
revision = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
log_marker_paths = ["libi2pd/Transports.cpp"]
event_marker = "NTCP2: SessionConfirmed sent"
semantic_level = "ntcp2_authenticated"
sanitization_rule = "strip-ipv4-endpoint-prefix"
```

## Pinned observations

### Java I2P 2.12.0 (revision `2800040deee9bb376567b671ef2e9c34cf3e30b6`)

The pinned Java revision exposes its transport events through the bounded
`eventlog.txt` writer inside the router runtime. The current harness
adapter (`tests/integration/ntcp2/harness/java_i2p.py`) treats the phrase
`NTCP2 connection established` as the post-authentication marker. This is
strictly a handshake observation; it does not prove receiver-side I2NP
acceptance.

For Plan 052 the Java observation levels are:

| Level | Source | Marker / Event | Status |
| --- | --- | --- | --- |
| `process_started` | typed-status | JavaI2pAdapter `wait_ready()` | Stable. |
| `listener_ready` | structured-log | eventlog.txt: `Starting I2P` | Stable. |
| `tcp_connected` | structured-log | eventlog.txt: `NTCP2 listener bound on` | Stable. |
| `ntcp2_authenticated` | structured-log | eventlog.txt: `NTCP2 connection established` | Stable. |
| `frame_emitted` | not-applicable | n/a (Java is the receiver in this lane) | Not applicable. |
| `frame_authenticated_and_decrypted` | pending source inspection | pending | **Pending F2**. |
| `i2np_message_decoded` | pending source inspection | pending | **Pending F2**. |
| `terminal_clean` | typed-status | adapter stop process counter | Stable. |

The current Plan 045-051 lane relies on `NTCP2 connection established` and
the i2pr-side `authenticated` counter. Plan 052 requires the receiver-side
`frame_authenticated_and_decrypted` and `i2np_message_decoded` markers
above. They must be source-locked to revision
`2800040deee9bb376567b671ef2e9c34cf3e30b6` before Milestone 3 closure.

### i2pd 2.60.0 (revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`)

The pinned i2pd revision emits structured Boost.Log events. The current
harness adapter (`tests/integration/ntcp2/harness/i2pd.py`) treats the
phrases `NTCP2: SessionConfirmed sent` and `NTCP2: SessionConfirmed from`
as authentication markers. Plan 052 still requires per-direction receiver
decrypt and I2NP decode markers; the current Boost.Log surface must be
source-locked to revision `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`.

| Level | Source | Marker / Event | Status |
| --- | --- | --- | --- |
| `process_started` | typed-status | I2pdAdapter `wait_ready()` | Stable. |
| `listener_ready` | structured-log | `NTCP2: Listen` | Stable. |
| `tcp_connected` | structured-log | `NTCP2: Connected` | Stable. |
| `ntcp2_authenticated` | structured-log | `NTCP2: SessionConfirmed sent` (initiator), `NTCP2: SessionConfirmed from` (responder) | Stable. |
| `frame_emitted` | not-applicable | n/a (i2pd is the receiver in this lane) | Not applicable. |
| `frame_authenticated_and_decrypted` | pending source inspection | pending | **Pending F2**. |
| `i2np_message_decoded` | pending source inspection | pending | **Pending F2**. |
| `terminal_clean` | typed-status | adapter stop process counter | Stable. |

## Update rules

- Adding a new marker requires updating both the Java and i2pd sections
  here, in the matching `reference_observation_catalog.toml` file (when
  it exists), and in the relevant adapter.
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

A marker that is not present in this document, or that disagrees with the
pinned source revision, is a typed blocker in `validate_observation()`.

## Open source-inspection work

1. Java I2P `frame_authenticated_and_decrypted` event name and source path
   (probably `router/java/src/net/i2p/router/transport/ntcp/NTCP2Connection.java`
   in the pinned revision).
2. Java I2P `i2np_message_decoded` event name and source path (probably in
   `router/java/src/net/i2p/router/transport/ntcp/NTCP2Reader.java`).
3. i2pd `frame_authenticated_and_decrypted` event name and source path
   (probably in `libi2pd/Transports.cpp` near the NTCP2 session handler).
4. i2pd `i2np_message_decoded` event name and source path (probably in
   `libi2pd/NTCP2Session.cpp` near the receive handler).

Each entry above must include the exact symbol, file path, revision, and
sanitization rule. Source inspection must be performed against the locked
revision only; any drift is a typed blocker.