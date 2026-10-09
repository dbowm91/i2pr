# i2pr-app-sdk

Application-side Rust SDK for the experimental managed native app protocol.
The default feature set is runtime-neutral and exposes the canonical protocol
types. The optional `tokio-adapter` feature provides a generic session over
caller-owned `AsyncRead` and `AsyncWrite` channels.

The SDK never opens sockets, chooses grants, starts processes, or provides
direct host networking. Applications request only protocol capabilities
declared by their signed package and separately granted by the administrator.
The protocol is experimental; compatibility is governed by the protocol major
and minor values in `i2pr-app-proto`.
