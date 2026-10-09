# i2pr-app-proto

Runtime-neutral, bounded types and codecs for the experimental i2pr managed
native application protocol. This crate owns no runtime, socket, filesystem,
process, grant, or router authority.

Use the protocol version fields and normative reference when implementing
another language client. Unknown fields, invalid values, oversized frames, and
unsupported protocol majors fail closed. The crate does not imply release or
support status for managed applications.
