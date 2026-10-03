# Anonymity qualification lanes

Plan 298 defines the normalized, identity-free trace format in
`i2pr-testkit::streaming_fingerprint` and the fixed stimulus matrix in
`streaming-scenarios.toml`. Exact source authorities are in
`references.lock.toml`.

Plan 304 adds an Ubuntu-only preflight (`bash
scripts/interop/anonymity/preflight-ubuntu.sh`) and the topology/corpus
contracts in `topology.toml` and `http-corpus.toml`. Reference sources and
artifacts are built only in ignored `target/interop`; record their hashes with
`scripts/interop/anonymity/record-reference-manifest.py`.

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

The controlled hostile-Destination topology remains unqualified until all
three family runners produce HTTP and Streaming smoke rows. Stock Java and
i2pd Streaming APIs expose application streams, not the server packet response
boundary required to control ACK/loss stimuli; their existing application
lanes must not be counted as Plan 304 evidence.


## ADR 0030 continuation

The Plan 298/304 three-family hostile-Destination contract above is retained historical evidence and is not the future Streaming qualification gate. ADR 0030 selects exact-pinned i2pd 2.61.0 as the Streaming observable-profile authority.

Plans 312–313 use directional black-box measurement: i2pr owns the opposite endpoint while unmodified i2pd runs as the client or server under test. Java I2P may be recorded for context but is not a pass/fail Streaming dependency.

Plan 312's bounded handshake baseline runs with
`bash tests/integration/anonymity/run-plan312-streaming.sh`. It wraps the
exact-pinned Plan 193 i2pd lane and retains only one SYN/SYN-ACK's registered
metadata for each i2pr/i2pd client/server role. The registered dimensions are
flags, FROM inclusion, maximum packet payload, and initial payload length.
Window/choke, ACK/NACK, RTO/retransmission, loss/reorder, close/reset, terminal,
and timing behavior are not observed by this scenario and cannot justify Plan
313 tuning. The runner clears stale traces, fails closed on a missing role or
pin/schema drift, and writes sanitized traces plus a comparison matrix under
ignored `target/interop/anonymity/plan312-streaming`. Raw packet bytes,
Destination identifiers, and stream identifiers are not retained. See
`plans/closure/anonymity/312-status.md` for the measured differential and
verification record.

HTTP qualification is separated from hostile Streaming. Plan 308 may use ordinary controlled server Destinations and the retained Ubuntu/reference artifacts without waiting for a raw packet-control seam inside Java or i2pd.

Plan 308's bounded HTTP capture normalizer is `canonicalize_http_capture.py`; its
fail-closed evidence contract is checked by `bash scripts/check-http-anonymity-evidence.sh`.
The checker requires all three family captures, exact pins, the current source head,
the fixed corpus, and completed owned-process cleanup. It intentionally fails while
the controlled peer topology or any family capture is missing.
