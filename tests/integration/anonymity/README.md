# Anonymity qualification lanes

Plan 298 defines the normalized, identity-free trace format in
`i2pr-testkit::streaming_fingerprint` and the fixed stimulus matrix in
`streaming-scenarios.toml`. Exact source authorities are in
`references.lock.toml`.

The schema retains direction, 10 ms relative-time buckets, flags, payload
length only, sequence/ACK deltas, retransmission ordinal, advertised packet
size/window/choke state, and a bounded terminal category. It excludes packet
bytes, payload digests, peer identifiers, Destination hashes, and addresses.
Timing-sensitive scenarios require five repetitions and retain observed
distributions; the 250 ms tolerance is a classification aid, not an exact
timing claim.

The fixed matrix is a scenario contract. A row is not executed evidence until
an exact-pinned router run emits a validated trace. Existing M6 Streaming
interoperability evidence is not a substitute for these hostile-Destination
fingerprint scenarios.
