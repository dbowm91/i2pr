# Portable Service-Tunnel SAM Adapter Handoff

Status: downstream design contract; no SAM implementation is included in i2pr.

This document assigns the work needed for a separate SAM library and tunnel manager. `i2pr-service-tunnels` remains a runtime-neutral policy and filtering core. The adapter validates policy before starting resources, supplies authenticated transport metadata, invokes core filters, and owns all I/O and lifecycle.

## Responsibilities outside i2pr

A future separate repository owns SAM HELLO/version/capability negotiation; command/reply codecs and state machines; STREAM connect, accept, and forwarding; DATAGRAM/RAW and DATAGRAM2/3 where implemented; PRIMARY/MASTER and shared-session compatibility; naming lookup; Destination generation and persistence integration; router capability/quirk profiles; reconnect, backoff, and session teardown; async runtime and blocking facade; C ABI/Python bindings; and tunnel daemon, configuration syntax, WebUI, and application-sidecar packaging. It must not copy the policy/filter implementation from this crate.

## Identity and linkability mapping

- Each core `DestinationGroupKey::Dedicated` is a separate Destination/session ownership domain.
- Each equal `DestinationGroupKey::Explicit` denotes one configured shared identity domain. The adapter preserves it as one SAM Destination identity; any router-compatible subsession spelling stays inside the adapter.
- The adapter must not merge groups based on tunnel kind, target, port, or resource limits, and must not split an explicit shared group in a way that changes configured linkability.
- Key references and persistence intent are metadata/policy. The core does not own key material, generate Destinations, or access storage.

## Inbound peer authentication

For peer access, rate, or IRC hostname policy, the adapter obtains the authenticated remote I2P Destination from the accepted stream/session API and maps it to the canonical hash used by the core. A local TCP address, SAM session ID, nickname, or unverified claimed hostname is not an authenticated peer identity. If the selected router/profile cannot provide authenticated identity, the adapter rejects profiles that need it or fails the peer-dependent policy closed.

## Runtime and lifecycle ownership

The adapter owns listeners and target sockets, task spawning/cancellation, bounded pumps and backpressure, monotonic time, timeout enforcement, SAM connection/session lifecycle, retries, persistence I/O, and transport-specific logs/metrics. It validates a complete proposed policy generation before starting changes, uses deterministic core diffs to choose add/remove/replace/mutate actions, and reports each start/stop failure explicitly. A failed start must not silently mutate the accepted policy generation. Restart reconstructs state from validated specs and adapter-owned persistent identity/key references.

The core owns policy decisions and bounded protocol parsing only. It does not own tasks or connections.

## Filtering placement and byte flow

Client direction:

```text
local application
    -> adapter reads bounded stream bytes
    -> i2pr-service-tunnels protocol/privacy filter
    -> adapter writes filtered bytes to SAM STREAM
    -> I2P network
```

Client response direction (where the selected profile defines response filtering):

```text
I2P network -> SAM STREAM -> adapter reads bounded bytes
    -> core response privacy/filter policy
    -> adapter writes filtered bytes -> local application
```

Server direction:

```text
I2P network -> SAM accepted STREAM
    -> adapter extracts authenticated peer Destination hash
    -> core access/rate policy (adapter supplies monotonic time)
    -> core application protocol filter (HTTP/IRC/etc.)
    -> adapter writes permitted bytes -> local target
```

Server response direction:

```text
local target -> adapter reads bounded bytes
    -> core response policy when defined by the profile
    -> adapter writes permitted bytes -> SAM STREAM -> I2P network
```

Raw SAM streams must never be wired directly to local applications when the selected profile requires a core filter. The adapter owns local target/Host projection values and passes them to the applicable filter. No router/SAM implementation identifier is injected into application traffic.

## Conformance evidence

The external fixture and its matrix are in `tests/portable-service-tunnel-consumer/`. The fixture pins the Plan 350 supported Git revision and imports only the public crate. It is deterministic and does not exercise live SAM interoperability. Revisit this handoff if the external transport cannot provide authenticated peer identity or cannot preserve explicit group ownership; do not add SAM-specific types to the core as a workaround.
