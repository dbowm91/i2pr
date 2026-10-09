# `i2pr-app-sdk` — application-side managed-app SDK

`i2pr-app-sdk` is a public Rust client for the managed-app protocol. It depends
only on `i2pr-app-proto`; the optional `tokio-adapter` feature provides a
generic framed session over caller-owned async readers and writers. The default
feature set does not own a runtime.

`Session` sends the application greeting and correlated `hello`, records the
host-origin effective capability set, and exposes bounded logical stream and
local-service publish operations. It never opens a socket, creates a grant,
chooses a service destination, starts a process, or handles signing keys.
Frames are length-checked against the protocol ceiling. Outgoing writes await
the caller's writer, so the SDK adds no hidden unbounded queue.

The API and protocol are experimental. Crate versions are semver-managed
independently from the router binary; wire compatibility follows the protocol
major/minor fields and normative managed-app reference.
